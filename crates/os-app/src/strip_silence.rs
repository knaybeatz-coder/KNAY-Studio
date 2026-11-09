// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-destructive Playlist silence removal using the same source playback as normalization.
use crate::{
    app::{Action, App, Ui},
    dialog::{Body, DHit, Modal},
    playlist as pl,
};
use os_model::{Channel, Clip, ClipSource};
use os_ui::{Color, Font, Rect};
use std::sync::mpsc;
use winit::keyboard::{Key, NamedKey};

const BLOCK_MS: f64 = 5.;
const LABELS: [&str; 6] = [
    "Open level",
    "Close level",
    "Attack",
    "Hold",
    "Release",
    "Look ahead",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Auto,
    Manual,
    Split,
    Field(usize),
    Apply,
    Cancel,
}
#[derive(Clone)]
struct Source {
    index: usize,
    clip: Clip,
    channel: Channel,
    path: String,
}
#[derive(Clone, Copy)]
struct Settings {
    values: [f64; 6],
    split: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            values: [-40., -46., 2., 80., 40., 10.],
            split: true,
        }
    }
}
struct Analysis {
    peaks: Vec<f32>,
}
pub struct Form {
    sources: Vec<Source>,
    rx: Option<mpsc::Receiver<Result<Vec<Analysis>, String>>>,
    analysis: Vec<Analysis>,
    settings: Settings,
    automatic: bool,
    input: Option<(usize, String)>,
    input_selected: bool,
    tempo: f64,
    ppq: u16,
    message: String,
}

fn ranges(peaks: &[f32], settings: Settings) -> Vec<(usize, usize)> {
    let [open, close, _, hold, release, ahead] = settings.values;
    let amplitude = |db: f64| 10_f32.powf(db as f32 / 20.);
    let (open, close) = (amplitude(open), amplitude(close));
    let hold = (hold / BLOCK_MS).ceil() as usize;
    let release = (release / BLOCK_MS).ceil() as usize;
    let ahead = (ahead / BLOCK_MS).ceil() as usize;
    let mut result: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    let mut last = 0;
    for (i, &peak) in peaks.iter().enumerate() {
        if start.is_none() && peak >= open {
            start = Some(i.saturating_sub(ahead));
            last = i;
        }
        if let Some(a) = start {
            if peak >= close {
                last = i;
            }
            if i > last + hold {
                let end = (last + 1 + hold + release).min(peaks.len());
                if let Some(previous) = result.last_mut().filter(|p| p.1 >= a) {
                    previous.1 = previous.1.max(end);
                } else {
                    result.push((a, end));
                }
                start = None;
            }
        }
    }
    if let Some(a) = start {
        if let Some(previous) = result.last_mut().filter(|p| p.1 >= a) {
            previous.1 = peaks.len();
        } else {
            result.push((a, peaks.len()));
        }
    }
    if !settings.split && result.len() > 1 {
        result = vec![(result[0].0, result.last().unwrap().1)];
    }
    result
}
fn automatic(peaks: &[f32], split: bool) -> Settings {
    let mut db: Vec<f64> = peaks
        .iter()
        .map(|&p| 20. * f64::from(p.max(1e-6)).log10())
        .collect();
    db.sort_by(f64::total_cmp);
    let peak = db.last().copied().unwrap_or(-120.);
    let floor = db.get(db.len() / 5).copied().unwrap_or(-120.);
    let open = (floor + 10.)
        .clamp(peak - 45., peak - 20.)
        .clamp(-80., -15.);
    Settings {
        values: [open, open - 6., 2., 80., 40., 10.],
        split,
    }
}
impl Form {
    fn settings_for(&self, a: &Analysis) -> Settings {
        if self.automatic {
            automatic(&a.peaks, self.settings.split)
        } else {
            self.settings
        }
    }
    fn summary(&self) -> String {
        let count: usize = self
            .analysis
            .iter()
            .map(|a| ranges(&a.peaks, self.settings_for(a)).len())
            .sum();
        format!(
            "{} audio clips selected · {count} resulting clips",
            self.sources.len()
        )
    }
}

pub fn open(app: &mut App, anchor: usize) -> Result<(), String> {
    let s = &app.state;
    let indices = if s.playlist.sel.contains(&anchor) {
        s.playlist.sel.clone()
    } else {
        vec![anchor]
    };
    let mut sources = Vec::new();
    for index in indices {
        let Some(&clip) = pl::clips(s).get(index) else {
            continue;
        };
        let ClipSource::Channel(id) = clip.source else {
            continue;
        };
        let Some(channel) = s
            .project
            .channel(id)
            .filter(|c| c.kind == os_model::ChannelKind::AudioClip)
        else {
            continue;
        };
        if pl::track_info(s, clip.track).is_some_and(|t| t.locked || t.frozen.is_some()) {
            return Err(
                "Unlock or unfreeze the selected audio tracks before stripping silence".into(),
            );
        }
        if crate::ara::is(channel) {
            return Err("Render ARA edits to audio before stripping silence".into());
        }
        let path =
            crate::plugins::resolve_sample(channel.sample_path.as_deref().ok_or("Missing sample")?);
        sources.push(Source {
            index,
            clip,
            channel: channel.clone(),
            path,
        });
    }
    if sources.is_empty() {
        return Err("Select audio clips to strip silence".into());
    }
    let tempo = s.tempo;
    let ppq = s.project.ppq;
    let work = sources.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut lib = os_engine::SampleLibrary::new(None);
            let mut output = Vec::new();
            for source in work {
                let (mut engine, rate) = crate::clip_normalize::analysis_engine(
                    source.clip,
                    &source.channel,
                    &source.path,
                    tempo,
                    ppq,
                    &mut lib,
                )?;
                let block = (f64::from(rate) * BLOCK_MS / 1000.).round().max(1.) as usize;
                let mut peaks = Vec::new();
                let mut peak = 0_f32;
                let mut frames = 0;
                while let Some((audio, tick, ..)) = engine.next_tick() {
                    if tick >= f64::from(source.clip.length) {
                        break;
                    }
                    for pair in audio.chunks_exact(2) {
                        peak = peak.max(pair[0].abs().max(pair[1].abs()) * source.clip.gain());
                        frames += 1;
                        if frames == block {
                            peaks.push(peak);
                            frames = 0;
                            peak = 0.;
                        }
                    }
                }
                if frames > 0 {
                    peaks.push(peak);
                }
                output.push(Analysis { peaks });
            }
            Ok(output)
        })();
        let _ = tx.send(result);
        os_host::wake();
    });
    app.state.dialogs.push(Modal::new(Body::StripSilence(Form {
        sources,
        rx: Some(rx),
        analysis: vec![],
        settings: Settings::default(),
        automatic: true,
        input: None,
        input_selected: false,
        tempo,
        ppq,
        message: "Analyzing selected audio...".into(),
    })));
    app.invalidate();
    Ok(())
}
pub fn poll(app: &mut App) {
    let Some(Modal {
        body: Body::StripSilence(f),
        ..
    }) = app.state.dialogs.last_mut()
    else {
        return;
    };
    let Some(rx) = f.rx.as_ref() else { return };
    let result = match rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("Audio analysis stopped".into()),
    };
    f.rx = None;
    match result {
        Ok(data) => {
            f.analysis = data;
            f.message = f.summary();
        }
        Err(e) => f.message = e,
    }
    app.invalidate();
}
fn accept_input(f: &mut Form) -> bool {
    let Some((i, text)) = f.input.take() else {
        return true;
    };
    let bounds = if i < 2 { (-96., 0.) } else { (0., 2000.) };
    let Ok(value) = text.trim().parse::<f64>() else {
        f.message = "Enter a number".into();
        f.input = Some((i, text));
        return false;
    };
    if !value.is_finite() || value < bounds.0 || value > bounds.1 {
        f.message = if i < 2 {
            "Levels must be from -96 to 0 dBFS"
        } else {
            "Times must be from 0 to 2000 ms"
        }
        .into();
        f.input = Some((i, text));
        return false;
    }
    let mut values = f.settings.values;
    values[i] = value;
    if values[1] > values[0] {
        f.message = "Close level must be at or below Open level".into();
        f.input = Some((i, text));
        return false;
    }
    f.settings.values = values;
    f.message = f.summary();
    true
}
pub fn command(app: &mut App, c: Command) {
    if c == Command::Cancel {
        app.state.dialogs.pop();
        app.invalidate();
        return;
    }
    let Some(Modal {
        body: Body::StripSilence(f),
        ..
    }) = app.state.dialogs.last_mut()
    else {
        return;
    };
    if !accept_input(f) {
        app.invalidate();
        return;
    }
    match c {
        Command::Auto => {
            f.automatic = true;
            f.input = None;
        }
        Command::Manual => {
            if f.automatic
                && let Some(a) = f.analysis.first()
            {
                f.settings = automatic(&a.peaks, f.settings.split);
            }
            f.automatic = false;
        }
        Command::Split => f.settings.split = !f.settings.split,
        Command::Field(i) if !f.automatic && i < 6 => {
            f.input = Some((i, f.settings.values[i].to_string()));
            f.input_selected = true;
        }
        Command::Apply => {
            if f.rx.is_some() || f.analysis.len() != f.sources.len() {
                return;
            }
            apply(app);
            return;
        }
        _ => {}
    }
    if f.rx.is_none() && !f.analysis.is_empty() {
        f.message = f.summary();
    }
    app.invalidate();
}
pub fn key(app: &mut App, key: &Key, ctrl: bool, alt: bool) {
    if matches!(key, Key::Named(NamedKey::Escape)) {
        command(app, Command::Cancel);
        return;
    }
    let Some(Modal {
        body: Body::StripSilence(f),
        ..
    }) = app.state.dialogs.last_mut()
    else {
        return;
    };
    match key {
        Key::Named(NamedKey::Enter) => {
            if f.input.is_some() {
                accept_input(f);
            } else {
                command(app, Command::Apply);
            }
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some((_, text)) = &mut f.input {
                if f.input_selected {
                    text.clear();
                    f.input_selected = false;
                } else {
                    text.pop();
                }
            }
        }
        Key::Character(c) if !ctrl && !alt => {
            if let Some((_, text)) = &mut f.input {
                if f.input_selected {
                    text.clear();
                    f.input_selected = false;
                }
                if text.len() < 16 {
                    text.push_str(c);
                }
            }
        }
        _ => {}
    }
    app.invalidate();
}
fn apply(app: &mut App) {
    let Some(Modal {
        body: Body::StripSilence(f),
        ..
    }) = app.state.dialogs.last()
    else {
        return;
    };
    if app.state.tempo != f.tempo
        || app.state.project.ppq != f.ppq
        || f.sources.iter().any(|s| {
            pl::clips(&app.state).get(s.index) != Some(&s.clip)
                || app.state.project.channel(s.channel.index) != Some(&s.channel)
        })
    {
        app.state.message = Some("The clips changed. Reopen Strip silence.".into());
        return;
    }
    let mut replacements = Vec::new();
    for (source, a) in f.sources.iter().zip(&f.analysis) {
        let settings = f.settings_for(a);
        let ticks_per_block = BLOCK_MS / 1000. * f.tempo / 60. * f64::from(f.ppq);
        let (w0, _) = pl::window(&app.state, &source.clip);
        let mut pieces = Vec::new();
        for (start, end) in ranges(&a.peaks, settings) {
            let start = (start as f64 * ticks_per_block).floor().max(0.) as u32;
            let end = (end as f64 * ticks_per_block)
                .ceil()
                .min(f64::from(source.clip.length)) as u32;
            if end <= start {
                continue;
            }
            if start == 0 && end == source.clip.length {
                pieces.push(source.clip);
                continue;
            }
            let mut clip = source.clip;
            clip.position += start;
            clip.length = end - start;
            let ms = |ticks| pl::to_src(&app.state, source.clip.source, f64::from(ticks));
            clip.window = Some((w0 + ms(start), w0 + ms(end)));
            if !pieces.is_empty() {
                clip.set_id(0);
            }
            let duration = ms(end - start) as f32;
            let in_ms = if start == 0 {
                source.clip.stored_fade_in().map_or(0., |v| v.0)
            } else {
                (settings.values[2] * ms(1) / (60000. / f.tempo / f64::from(f.ppq))) as f32
            };
            let out_ms = if end == source.clip.length {
                source.clip.stored_fade_out().map_or(0., |v| v.0)
            } else {
                (settings.values[4] * ms(1) / (60000. / f.tempo / f64::from(f.ppq))) as f32
            };
            clip.set_fade_in(Some((
                in_ms.min(duration / 2.),
                source.clip.fade_in_tension(),
            )));
            clip.set_fade_out(Some((
                out_ms.min(duration / 2.),
                source.clip.fade_out_tension(),
            )));
            pieces.push(clip);
        }
        replacements.push((source.index, pieces));
    }
    app.state.dialogs.pop();
    pl::snapshot_as(&mut app.state, "Strip silence");
    replacements.sort_by_key(|p| p.0);
    let mut selected = Vec::new();
    // Replace from the end so indices of the other original clips remain valid.
    for (index, pieces) in replacements.into_iter().rev() {
        pl::arr_mut(&mut app.state).clips.remove(index);
        for selected_index in &mut selected {
            if *selected_index > index {
                *selected_index -= 1;
            }
        }
        for piece in pieces {
            let clips = &mut pl::arr_mut(&mut app.state).clips;
            selected.push(clips.len());
            clips.push(piece);
        }
    }
    app.state.playlist.sel = selected;
    app.state.piano.dirty = true;
    app.state.message = Some("Silence stripped. Undo restores the original clips.".into());
    app.invalidate();
}
pub fn paint(ui: &mut Ui, r: Rect, f: &Form) {
    let yellow = Color::rgb(0xe8cf62);
    let button = |ui: &mut Ui, rect: Rect, label: &str, c: Command, on: bool| {
        ui.c.fill_rrect(rect, 4., if on { yellow } else { ui.t.button });
        ui.c.text_center(
            Font::new(13),
            rect,
            if on { Color::rgb(0x171914) } else { ui.t.text },
            label,
        );
        ui.hit(rect, Action::Dlg(DHit::StripSilence(c)), label.to_string());
    };
    button(
        ui,
        Rect::new(r.x + 20., r.y + 42., 245., 32.),
        "Easy · Automatic",
        Command::Auto,
        f.automatic,
    );
    button(
        ui,
        Rect::new(r.x + 280., r.y + 42., 245., 32.),
        "Manual",
        Command::Manual,
        !f.automatic,
    );
    ui.c.text(
        Font::new(12),
        r.x + 20.,
        r.y + 93.,
        ui.t.text,
        if f.automatic {
            "Detects a suitable level for each clip."
        } else {
            "Click a value to edit. Enter accepts the value."
        },
    );
    for i in 0..6 {
        let y = r.y + 121. + i as f32 * 35.;
        ui.c.text(Font::new(13), r.x + 24., y + 7., ui.t.text, LABELS[i]);
        let value = Rect::new(r.x + 280., y, 240., 28.);
        ui.c.fill_rrect(value, 3., ui.t.menu_panel);
        let number = f
            .input
            .as_ref()
            .filter(|v| v.0 == i)
            .map(|v| v.1.clone())
            .unwrap_or_else(|| format!("{:.1}", f.settings.values[i]));
        let label = if f.automatic {
            "Auto".into()
        } else {
            format!("{number} {}", if i < 2 { "dBFS" } else { "ms" })
        };
        ui.c.text_center(
            Font::new(13),
            value,
            if f.automatic { ui.t.text_dim } else { yellow },
            &label,
        );
        if !f.automatic {
            ui.hit(
                value,
                Action::Dlg(DHit::StripSilence(Command::Field(i))),
                "Edit value",
            );
        }
    }
    button(
        ui,
        Rect::new(r.x + 20., r.y + 341., 505., 30.),
        if f.settings.split {
            "Split clips at silence: On"
        } else {
            "Split clips: Off · trim start and end only"
        },
        Command::Split,
        f.settings.split,
    );
    ui.c.text_clip(
        Font::new(12),
        r.x + 20.,
        r.y + 385.,
        r.w - 40.,
        ui.t.text,
        &f.message,
    );
    button(
        ui,
        Rect::new(r.x + 20., r.bottom() - 49., 140., 30.),
        "Cancel",
        Command::Cancel,
        false,
    );
    if f.rx.is_none() && f.analysis.len() == f.sources.len() {
        button(
            ui,
            Rect::new(r.right() - 175., r.bottom() - 49., 155., 30.),
            "Strip silence",
            Command::Apply,
            true,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_hysteresis_hold_release_and_lookahead() {
        let s = Settings {
            values: [-20., -30., 2., 10., 5., 5.],
            split: true,
        };
        let peaks = [
            0., 0., 0.2, 0.05, 0., 0.05, 0., 0., 0., 0., 0., 0.2, 0.2, 0., 0., 0., 0., 0.,
        ];
        let r = ranges(&peaks, s);
        assert_eq!(r, vec![(1, 9), (10, 16)]);
        let trim = Settings { split: false, ..s };
        assert_eq!(ranges(&peaks, trim), vec![(1, 16)]);
        assert!(ranges(&[0.; 20], s).is_empty());
    }
    #[test]
    fn selected_strip_silence_menu_split_trim_cancel_and_undo() {
        let file = std::env::temp_dir().join(format!("strip-silence-{}.wav", std::process::id()));
        let rate = 8000;
        let audio: Vec<f32> = (0..rate * 4)
            .flat_map(|i| {
                let time = i as f64 / rate as f64;
                let v = if (0.5..1.).contains(&time) || (2.0..2.5).contains(&time) {
                    (i as f32 * std::f32::consts::TAU * 400. / rate as f32).sin() * 0.3
                } else {
                    0.
                };
                [v, v]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&file).unwrap(), rate, &audio)
            .unwrap();
        let mut app = App::new(None);
        app.state.project = crate::app::default_project();
        let i = pl::add_audio_clip(&mut app.state, file.to_str().unwrap(), 2, 0).unwrap();
        let mut other = pl::clips(&app.state)[i];
        other.track = 3;
        pl::arr_mut(&mut app.state).clips.push(other);
        let wait = |app: &mut App| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                poll(app);
                if let Some(Modal {
                    body: Body::StripSilence(f),
                    ..
                }) = app.state.dialogs.last()
                {
                    if f.rx.is_none() {
                        assert_eq!(f.analysis.len(), f.sources.len(), "{}", f.message);
                        break;
                    }
                }
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        app.state.playlist.sel = vec![i, i + 1];
        let original = pl::clips(&app.state).to_vec();
        let menu = pl::clip_menu(&app.state, i, 0., 0.);
        assert!(menu.items.iter().any(|v| v.cmd == pl::Cmd::StripSilence(i)));
        pl::run(&mut app, pl::Cmd::StripSilence(i)).unwrap_or_default();
        wait(&mut app);
        let mut canvas = os_ui::Canvas::new(1280, 900, 1.).unwrap();
        app.paint(&mut canvas, None);
        if let Some(dir) = std::env::var_os("OS_SHOT_DIR") {
            canvas
                .pixmap
                .save_png(std::path::PathBuf::from(dir).join("strip-silence.png"))
                .unwrap();
        }
        command(&mut app, Command::Apply);
        let pieces = pl::clips(&app.state);
        assert_eq!(pieces.len(), 4);
        assert_eq!(app.state.playlist.sel.len(), 4);
        assert!(
            pieces
                .iter()
                .all(|c| c.window.is_some() && c.position > 0 && c.length < original[0].length)
        );
        assert!(
            pieces
                .iter()
                .all(|c| c.source == original[0].source && c.gain() == original[0].gain())
        );
        crate::history::undo(&mut app.state).unwrap();
        assert_eq!(pl::clips(&app.state), original);
        app.state.playlist.sel = vec![i];
        open(&mut app, i).unwrap();
        wait(&mut app);
        command(&mut app, Command::Manual);
        command(&mut app, Command::Field(2));
        key(&mut app, &Key::Character("8".into()), false, false);
        key(&mut app, &Key::Named(NamedKey::Enter), false, false);
        command(&mut app, Command::Split);
        command(&mut app, Command::Apply);
        assert_eq!(pl::clips(&app.state).len(), 2);
        assert_eq!(
            pl::clips(&app.state)[0],
            original[1],
            "Unselected clip keeps its original data"
        );
        assert_eq!(app.state.playlist.sel.len(), 1);
        crate::history::undo(&mut app.state).unwrap();
        let original = pl::clips(&app.state).to_vec();
        open(&mut app, i).unwrap();
        command(&mut app, Command::Cancel);
        assert_eq!(pl::clips(&app.state), original);
        std::fs::remove_file(file).unwrap();
    }
}
