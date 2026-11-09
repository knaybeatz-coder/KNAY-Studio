// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional playlist controls and stable automation reference values.
use crate::app::State;
use crate::playlist::{Cmd, Mark, PItem, PickKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control { Show(PickKind), Select(PickKind), Fades, ZeroCrossing, Step, Slide, NoteColor(u8) }

pub struct Controls {
    pub shown: Option<PickKind>,
    pub fades: bool,
    pub zero_crossing: bool,
    pub step: bool,
    pub slide: bool,
    pub note_color: u8,
}
impl Default for Controls {
    fn default() -> Self {
        use crate::settings_file as sf;
        Self {
            shown: match sf::get("playlist_clip_controls").as_deref() { Some("audio") => Some(PickKind::Audio), Some("automation") => Some(PickKind::Automation), Some("pattern") => Some(PickKind::Pattern), _ => None },
            fades: sf::flag("playlist_fade_controls", true), zero_crossing: sf::flag("playlist_zero_crossing", false),
            step: sf::flag("playlist_auto_step", false), slide: sf::flag("playlist_auto_slide", true),
            note_color: sf::get("playlist_note_color").and_then(|v| v.parse().ok()).unwrap_or(2).min(2),
        }
    }
}
pub fn run(s: &mut State, cmd: Control) {
    use crate::settings_file as sf;
    let c = &mut s.playlist.controls;
    match cmd {
        Control::Show(k) | Control::Select(k) => {
            // View-menu visibility can toggle; category tabs must never hide the panel.
            c.shown = if matches!(cmd,Control::Show(_)) && c.shown == Some(k) { None } else { Some(k) };
            sf::write("playlist_clip_controls", match c.shown { Some(PickKind::Audio) => "audio", Some(PickKind::Automation) => "automation", Some(PickKind::Pattern) => "pattern", _ => "hidden" });
        }
        Control::Fades => { c.fades = !c.fades; sf::set_flag("playlist_fade_controls", c.fades); }
        Control::ZeroCrossing => { c.zero_crossing = !c.zero_crossing; sf::set_flag("playlist_zero_crossing", c.zero_crossing); }
        Control::Step => { c.step = !c.step; sf::set_flag("playlist_auto_step", c.step); }
        Control::Slide => { c.slide = !c.slide; sf::set_flag("playlist_auto_slide", c.slide); }
        Control::NoteColor(n) => { c.note_color = n.min(2); sf::write("playlist_note_color", &c.note_color.to_string()); }
    }
}
pub fn items(s: &State, k: PickKind) -> Vec<PItem> {
    let c = &s.playlist.controls;
    let check = |label, cmd, on| PItem::new(label, Cmd::Control(cmd)).mark(Mark::Check(on));
    use Control::*;
    match k {
        PickKind::Audio => vec![check("Show fade editing controls", Fades, c.fades), check("Check for zero crossing when trimming", ZeroCrossing, c.zero_crossing), PItem::new("Stretch audio when resizing", Cmd::StretchMode).mark(Mark::Check(s.playlist.stretch))],
        PickKind::Automation => vec![check("Step editing", Step, c.step), check("Slide editing", Slide, c.slide)],
        PickKind::Pattern => ["Note group", "Parent channel", "Parent pattern"].into_iter().enumerate().map(|(i, label)| PItem::new(label, Cmd::Control(NoteColor(i as u8))).mark(Mark::Radio(c.note_color == i as u8))).collect(),
    }
}

/// Capture before editing or playback can move the target. Imported clips use their saved initial control,
/// then their first point as a fallback. New clips capture their creation value directly.
pub fn original(s: &State, ch: u16) -> Option<f64> {
    s.playlist.original_values.get(&ch).copied().or_else(|| {
        let a = s.project.automation(ch)?;
        a.targets.first().and_then(|t| crate::init::entry(s, *t)).and_then(|c| c.unit()).or_else(|| a.points.first().map(|p| p.value))
    }).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Reference { channel: u16, name: Option<String>, targets: Vec<u32>, value: f64 }
fn reference_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string(); name.push(".lemon.json"); name.into()
}
pub fn load_references(s: &mut State) {
    s.playlist.original_values = s.project.automations.iter().filter_map(|a| original(s, a.channel).map(|v| (a.channel, v))).collect();
    let Some(path) = s.path.as_deref() else { return };
    let Ok(bytes) = std::fs::read(reference_path(path)) else { return };
    let Ok(values) = serde_json::from_slice::<Vec<Reference>>(&bytes) else { return };
    for r in values {
        if r.value.is_finite() && (0.0..=1.0).contains(&r.value) && s.project.automation(r.channel).is_some_and(|a| a.targets == r.targets)
            && s.project.channel(r.channel).is_some_and(|c| c.name == r.name) {
            s.playlist.original_values.insert(r.channel, r.value);
        }
    }
}
pub fn save_references(s: &State, path: &std::path::Path) -> std::io::Result<()> {
    if s.project.automations.is_empty() { return Ok(()); }
    let values: Vec<_> = s.project.automations.iter().filter_map(|a| Some(Reference { channel: a.channel, name: s.project.channel(a.channel)?.name.clone(), targets: a.targets.clone(), value: original(s, a.channel)? })).collect();
    let path = reference_path(path);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&values)?)?;
    std::fs::rename(tmp, path)
}

/// Search only a few milliseconds around a trim, using both channels to avoid choosing a loud stereo edge.
pub fn zero_crossing(s: &State, source: os_model::ClipSource, ms: f64) -> f64 {
    if !s.playlist.controls.zero_crossing { return ms; }
    let os_model::ClipSource::Channel(ch) = source else { return ms };
    let Some(path) = s.project.channel(ch).filter(|c| c.kind == os_model::ChannelKind::AudioClip).and_then(|c| c.sample_path.as_deref()) else { return ms };
    let path = crate::plugins::resolve_sample(path);
    let Ok(sample) = os_engine::sample::load_shared(std::path::Path::new(&path)) else { return ms };
    let frames = sample.frames(); if frames < 2 { return ms; }
    if ms <= 0.0 || ms >= frames as f64 * 1000.0 / f64::from(sample.rate) { return ms; }
    let at = (ms * f64::from(sample.rate) / 1000.).round().clamp(1., (frames - 1) as f64) as usize;
    let radius = (sample.rate as usize / 200).max(1);
    let best = (at.saturating_sub(radius).max(1)..=(at+radius).min(frames-1))
        .filter(|&i| sample.left[i].is_sign_positive() != sample.left[i-1].is_sign_positive())
        .min_by(|&a,&b| {
            let score = |i: usize| f64::from(sample.left[i].abs()+sample.right[i].abs()) + i.abs_diff(at) as f64 / radius as f64 * 0.01;
            score(a).total_cmp(&score(b))
        });
    best.map_or(ms, |i| i as f64 * 1000. / f64::from(sample.rate))
}
