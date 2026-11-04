//! Per-clip audio chains, using dedicated sources and independent effect inserts.
use crate::{
    app::{Action, App, State, Ui},
    dialog::{Body, DHit, Modal},
    history::{self, Snap, Tail},
    mixer, playlist as pl,
    plugin_menu::Target,
};
use os_model::{ChannelKind, ClipSource, MixerTrack};
use os_ui::{Font, Rect};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Close,
    Add(u8),
    Open(u8),
    Remove(u8),
    Move(u8, i8),
    Toggle(u8),
    Bypass,
    Copy,
    Paste,
    Parameters(u8),
    Page(i8),
}
#[derive(Clone, Copy)]
struct Location {
    arr: usize,
    track: Option<usize>,
    clip: usize,
}
fn clip(p: &os_model::Project, l: Location) -> Option<&os_model::Clip> {
    let a = p.arrangements.get(l.arr)?;
    match l.track {
        None => a.clips.get(l.clip),
        Some(t) => a.tracks.get(t)?.frozen.as_ref()?.get(l.clip),
    }
}
fn set_source(p: &mut os_model::Project, l: Location, id: u16) {
    let a = &mut p.arrangements[l.arr];
    let c = match l.track {
        None => &mut a.clips[l.clip],
        Some(t) => &mut a.tracks[t].frozen.as_mut().unwrap()[l.clip],
    };
    c.source = ClipSource::Channel(id);
}
fn locations(p: &os_model::Project) -> Vec<Location> {
    let mut v = Vec::new();
    for (a, arr) in p.arrangements.iter().enumerate() {
        for i in 0..arr.clips.len() {
            v.push(Location {
                arr: a,
                track: None,
                clip: i,
            });
        }
        for (t, track) in arr.tracks.iter().enumerate() {
            if let Some(clips) = &track.frozen {
                for i in 0..clips.len() {
                    v.push(Location {
                        arr: a,
                        track: Some(t),
                        clip: i,
                    });
                }
            }
        }
    }
    v
}
fn row(s: &State, id: u16) -> Option<usize> {
    s.project
        .channels
        .iter()
        .position(|c| c.index == id && c.kind.is_sampler())
}
pub fn bus(s: &State, id: u16) -> Option<u16> {
    if !s.project.clip_fx_channels.contains(&id) {
        return None;
    }
    let t = s.project.channels.get(row(s, id)?)?.mixer_track;
    (t != 0 && mixer::track(s, t).is_some_and(|m| m.clip_fx)).then_some(t)
}
fn free_buses(s: &State) -> Vec<u16> {
    let references = |raw: u32, n: u16| {
        crate::peak_link::mixer_target(raw) == Some(n)
            || matches!(os_model::target::decode(raw),os_model::target::Target::MixerSend(_,dst) if dst==n)
    };
    s.project
        .mixer
        .iter()
        .rev()
        .filter(|m| {
            let mut default = MixerTrack::new(m.index);
            default.slots_enabled = m.slots_enabled;
            // Missing saved fader/pan fields have the same neutral meaning.
            default.volume = m.volume.filter(|v| *v == 0.8);
            default.pan = m.pan.filter(|v| *v == 0.);
            default.stereo_separation = m.stereo_separation.filter(|v| *v == 0.);
            m.index > 0
                && m.index <= mixer::max_insert(&s.project)
                && **m == default
                && !s.project.channels.iter().any(|c| c.mixer_track == m.index)
                && !s
                    .project
                    .patterns
                    .iter()
                    .any(|p| p.controls.iter().any(|c| references(c.kind, m.index)))
                && !s.mixer.recording.armed.contains(&m.index)
                && !s.mixer.recording.inputs.contains_key(&m.index)
                && !mixer::receives(s, m.index)
                && !s.mixer.slot_names.keys().any(|(t, _)| *t == m.index)
                && !s
                    .project
                    .automations
                    .iter()
                    .any(|a| a.targets.iter().any(|&t| references(t, m.index)))
                && !s
                    .project
                    .init_controls
                    .iter()
                    .any(|c| references(c.target, m.index))
                && !s
                    .project
                    .peak_links
                    .iter()
                    .any(|l| l.source == m.index || references(l.target, m.index))
        })
        .map(|m| m.index)
        .collect()
}
fn copies(p: &os_model::Project) -> Vec<Location> {
    let mut sources = std::collections::BTreeSet::new();
    let mut buses = std::collections::BTreeSet::new();
    locations(p)
        .into_iter()
        .filter(|&l| {
            let Some(os_model::Clip {
                source: ClipSource::Channel(id),
                ..
            }) = clip(p, l)
            else {
                return false;
            };
            if !p.clip_fx_channels.contains(id) {
                return false;
            }
            let repeated_source = !sources.insert(*id);
            let repeated_bus = p
                .channels
                .iter()
                .find(|c| c.index == *id)
                .is_some_and(|c| !buses.insert(c.mixer_track));
            repeated_source || repeated_bus
        })
        .collect()
}
pub fn validate(p: &os_model::Project) -> Result<(), String> {
    if copies(p).is_empty() {
        Ok(())
    } else {
        Err("Clip Effects: resolve independent clip inserts before playback or export".into())
    }
}

fn store(app: &mut App) -> Result<(),String> {
    app.mixer_tick();
    app.store_builtin_states();
    crate::project_plugins::store_hosted(&mut app.state.project, &app.plugins)
}
fn fork(s: &mut State, l: Location, dst: u16) -> Result<u16, String> {
    let ClipSource::Channel(id) = clip(&s.project, l).ok_or("Clip no longer exists")?.source else {
        return Err("Clip Effects require an audio clip".into());
    };
    let source = row(s, id).ok_or("Clip Effects require an audio clip")?;
    if crate::ara::is(&s.project.channels[source]) {
        return Err("Bounce this ARA clip before adding Clip Effects".into());
    }
    let private = bus(s, id).is_some();
    let mut ch = s.project.channels[source].clone();
    let original_bus = ch.mixer_track;
    let mut fx = if private {
        mixer::track(s, original_bus)
            .cloned()
            .ok_or("Clip effect insert is missing")?
    } else {
        let mut m = MixerTrack::new(dst);
        m.routes = vec![original_bus];
        m
    };
    let new = s
        .project
        .channels
        .iter()
        .map(|c| c.index)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Channel limit reached")?;
    ch.index = new;
    ch.kind = ChannelKind::AudioClip;
    ch.layer_children.clear();
    ch.mixer_track = dst;
    fx.index = dst;
    fx.origin = mixer::track(s, dst).and_then(|m| m.origin);
    fx.clip_fx = true;
    fx.name = Some(format!(
        "Clip FX: {}",
        ch.name.as_deref().unwrap_or("Audio")
    ));
    *mixer::track_mut(s, dst).ok_or("Effect insert is missing")? = fx;
    s.project.channels.push(ch);
    s.project.clip_fx_channels.push(new);
    set_source(&mut s.project, l, new);
    s.piano.dirty = true;
    Ok(new)
}
/// Split copied references before playback/save/export. A preflight avoids half-applied forks.
pub fn normalize(app: &mut App) -> Result<(), String> {
    if app.state.project.clip_fx_channels.is_empty() {
        return Ok(());
    }
    let copies = copies(&app.state.project);
    if copies.is_empty() {
        return Ok(());
    }
    let available = free_buses(&app.state);
    if available.len() < copies.len() {
        return Err(
            "Clip Effects: add empty mixer inserts before duplicating more processed clips".into(),
        );
    }
    let last = app
        .state
        .project
        .channels
        .iter()
        .map(|c| c.index)
        .max()
        .unwrap_or(0);
    if usize::from(last) + copies.len() > usize::from(u16::MAX) {
        return Err("Clip Effects: channel limit reached".into());
    }
    for &l in &copies {
        let Some(os_model::Clip {
            source: ClipSource::Channel(id),
            ..
        }) = clip(&app.state.project, l)
        else {
            unreachable!()
        };
        let r = row(&app.state, *id).ok_or("Clip Effects: audio source is missing")?;
        if crate::ara::is(&app.state.project.channels[r]) {
            return Err("Bounce this ARA clip before copying Clip Effects".into());
        }
    }
    store(app)?;
    for (l, dst) in copies.into_iter().zip(available) {
        fork(&mut app.state, l, dst)?;
    }
    sync(app);
    Ok(())
}
fn sync(app: &mut App) {
    app.mixer_tick();
    app.sync_samplers(None);
    app.sync_effects();
    app.sync_wrapped();
    app.sync_native();
    app.transport.set_effects(app.plugins.effects());
    app.params_tick();
}
fn show(app: &mut App, id: u16, page: usize) {
    app.state
        .dialogs
        .push(Modal::new(Body::ClipFx(id, page.min(8))));
    app.invalidate();
}
pub fn open(app: &mut App, index: usize) -> Result<(), String> {
    normalize(app)?;
    let arr = app
        .state
        .project
        .arrangements
        .iter()
        .position(|a| Some(a.number) == app.state.project.current_arrangement)
        .unwrap_or(0);
    let l = Location {
        arr,
        track: None,
        clip: index,
    };
    let c = *clip(&app.state.project, l).ok_or("Clip no longer exists")?;
    let ClipSource::Channel(id) = c.source else {
        return Err("Clip Effects require an audio clip".into());
    };
    if bus(&app.state, id).is_some() {
        show(app, id, 0);
        return Ok(());
    }
    let source = row(&app.state, id).ok_or("Clip Effects require an audio clip")?;
    if crate::ara::is(&app.state.project.channels[source]) {
        return Err("Bounce this ARA clip before adding Clip Effects".into());
    }
    let dst = free_buses(&app.state)
        .first()
        .copied()
        .ok_or("Clip Effects: add an empty mixer insert first")?;
    if app
        .state
        .project
        .channels
        .iter()
        .any(|c| c.index == u16::MAX)
    {
        return Err("Channel limit reached".into());
    }
    store(app)?;
    let s = &mut app.state;
    history::record(
        s,
        "Add clip effects",
        Snap::Several(vec![
            Snap::Playlist(Box::new(pl::capture(s))),
            Snap::Tail(Box::new(Tail::now(s, s.project.channels.len()))),
            Snap::ClipFxOwners(s.project.clip_fx_channels.clone()),
            Snap::MixerChannel(Box::new(mixer::channel_copy(s, dst).unwrap())),
        ]),
    );
    let id = fork(s, l, dst)?;
    sync(app);
    show(app, id, 0);
    Ok(())
}
pub fn add(app: &mut App, id: u16, slot: u8, index: usize) -> Option<String> {
    let t = bus(&app.state, id)?;
    if slot >= 64 || slot == 10 {
        return None;
    }
    if app.plugins.effects.get(index).is_none()
        || mixer::track(&app.state, t)?.slots[slot as usize].is_some()
    {
        return Some("Choose an empty clip effect slot".into());
    }
    if let Err(e)=store(app){return Some(e);}
    snapshot(app, t, "Add clip effect");
    let old = app.state.selected_mixer;
    app.state.selected_mixer = t;
    let result = mixer::run(app, mixer::MCmd::AddEffect(slot, index));
    app.state.selected_mixer = old;
    result
}
fn snapshot(app: &mut App, t: u16, name: &'static str) {
    if let Some(c) = mixer::channel_copy(&app.state, t) {
        let targets = Snap::ClipFxTargets(
            app.state.project.automations.clone(),
            app.state.project.init_controls.clone(),
            app.state.project.peak_links.clone(),
        );
        let patterns = Snap::Patterns {
            patterns: app.state.project.patterns.clone(),
            current: app.state.pattern,
        };
        history::record(
            &mut app.state,
            name,
            Snap::Several(vec![Snap::MixerChannel(Box::new(c)), targets, patterns]),
        );
    }
}
// Keep automation attached to the same effect when slots move. Removing/replacing
// an effect unlinks its targets without deleting the user's automation clips.
fn targets(s: &mut State, t: u16, map: impl Fn(u8) -> Option<u8>) {
    use os_model::target::{self, Target as T};
    let remap = |raw: u32| -> Option<u32> {
        match target::decode(raw) {
            T::SlotParam(track, k, p) if track == t => map(k).map(|n| target::slot_param(t, n, p)),
            T::SlotOn(track, k) if track == t => {
                map(k).map(|n| target::slot(t, n, target::SLOT_ON))
            }
            T::SlotMix(track, k) if track == t => {
                map(k).map(|n| target::slot(t, n, target::SLOT_MIX))
            }
            _ => Some(raw),
        }
    };
    for p in &mut s.project.patterns {
        p.controls
            .retain_mut(|c| remap(c.kind).map(|v| c.kind = v).is_some());
    }
    for a in &mut s.project.automations {
        a.targets = a.targets.iter().filter_map(|&v| remap(v)).collect();
    }
    s.project
        .init_controls
        .retain_mut(|c| remap(c.target).map(|v| c.target = v).is_some());
}
pub fn command(app: &mut App, c: Command) {
    let Some(Modal {
        body: Body::ClipFx(id, page),
        ..
    }) = app.state.dialogs.last()
    else {
        return;
    };
    let (id, page) = (*id, *page);
    let Some(t) = bus(&app.state, id) else {
        app.state.dialogs.pop();
        return;
    };
    if matches!(c,Command::Remove(_)|Command::Move(..)|Command::Toggle(_)|Command::Bypass|Command::Copy|Command::Paste) {if let Err(e)=store(app){app.state.message=Some(e);return;}}
    let old = app.state.selected_mixer;
    app.state.selected_mixer = t;
    match c {
        Command::Close => {
            app.state.dialogs.pop();
        }
        Command::Page(d) => {
            if let Some(Modal {
                body: Body::ClipFx(_, p),
                ..
            }) = app.state.dialogs.last_mut()
            {
                *p = (*p as i32 + i32::from(d)).clamp(0, 8) as usize;
            }
        }
        Command::Add(k) => {
            app.state.dialogs.pop();
            crate::plugin_picker::open(app, Target::ClipFx(id, k));
        }
        Command::Open(k) => {
            app.state.dialogs.pop();
            app.open_fx(t, k);
        }
        Command::Parameters(k) => {
            app.state.dialogs.pop();
            app.params_cmd(crate::params::PCmd::Browse(crate::plugins::fx_key(t, k)));
        }
        Command::Remove(k) => {
            snapshot(app, t, "Remove clip effect");
            targets(
                &mut app.state,
                t,
                |slot| if slot == k { None } else { Some(slot) },
            );
            app.state.message = mixer::run(app, mixer::MCmd::SlotDelete(k));
        }
        Command::Move(k, d) => {
            let slots: Vec<u8> = (0..64).filter(|&v| v != 10).collect();
            if let Some(at) = slots.iter().position(|&v| v == k) {
                let to = at as i32 + i32::from(d);
                if (0..63).contains(&to) {
                    let other = slots[to as usize];
                    snapshot(app, t, "Move clip effect");
                    targets(&mut app.state, t, |slot| {
                        Some(if slot == k {
                            other
                        } else if slot == other {
                            k
                        } else {
                            slot
                        })
                    });
                    app.state.message = mixer::run(app, mixer::MCmd::SlotMove(k, d));
                }
            }
        }
        Command::Toggle(k) => {
            snapshot(app, t, "Bypass clip effect");
            if let Some(m) = mixer::track_mut(&mut app.state, t) {
                m.slot_on[k as usize] = !m.slot_on[k as usize];
            }
            app.state.piano.dirty = true;
        }
        Command::Bypass => {
            snapshot(app, t, "Bypass clip effects");
            if let Some(m) = mixer::track_mut(&mut app.state, t) {
                m.slots_enabled = !m.slots_enabled;
            }
            app.state.piano.dirty = true;
        }
        Command::Copy => {

            app.state.clip_fx_clipboard = mixer::track(&app.state, t).cloned();
            app.state.message = Some("Copied clip effects".into());
        }
        Command::Paste => {
            if let Some(copy) = app.state.clip_fx_clipboard.clone() {
                snapshot(app, t, "Paste clip effects");
                targets(&mut app.state, t, |_| None);
                for k in 0..64 {
                    crate::peak_link::remove_slot(&mut app.state, t, k);
                }
                let mut dest = mixer::channel_copy(&app.state, t).unwrap();
                dest.names.clear();
                dest.track.slots = copy.slots;
                dest.track.slot_on = copy.slot_on;
                dest.track.slot_mix = copy.slot_mix;
                dest.track.slots_enabled = copy.slots_enabled;
                mixer::put_channel(&mut app.state, dest);
                sync(app);
            } else {
                app.state.message = Some("Copy a clip effects chain first".into());
            }
        }
    }
    app.state.selected_mixer = old;
    let _ = page;
    app.invalidate();
}
pub fn paint(ui: &mut Ui, s: &State, r: Rect, id: u16, page: usize) {
    let Some(t) = bus(s, id).and_then(|n| mixer::track(s, n)) else {
        return;
    };
    let th = ui.t;
    ui.c.text_clip(
        Font::new(12),
        r.x + 14.,
        r.y + 34.,
        r.w - 28.,
        th.text,
        t.name.as_deref().unwrap_or("Clip Effects"),
    );
    ui.c.text_clip(
        Font::new(11),
        r.x + 14.,
        r.y + 54.,
        r.w - 28.,
        th.text_dim,
        "This clip only. Output continues through its normal mixer insert.",
    );
    let mut button = |rect: Rect, label: &str, c: Command, on: bool| {
        let hot = ui.hit(rect, Action::Dlg(DHit::ClipFx(c)), label.to_string());
        ui.c.fill_rrect(
            rect,
            3.,
            if on {
                th.accent
            } else if hot {
                th.button_hi
            } else {
                th.sunken
            },
        );
        ui.c.text_clip(
            Font::new(11),
            rect.x + 5.,
            rect.y + 5.,
            rect.w - 10.,
            if on { th.sunken } else { th.text },
            label,
        );
    };
    button(
        Rect::new(r.x + 14., r.y + 77., 100., 24.),
        if t.slots_enabled {
            "Effects on"
        } else {
            "Bypassed"
        },
        Command::Bypass,
        t.slots_enabled,
    );
    button(
        Rect::new(r.x + 123., r.y + 77., 80., 24.),
        "Copy chain",
        Command::Copy,
        false,
    );
    button(
        Rect::new(r.x + 211., r.y + 77., 80., 24.),
        "Paste chain",
        Command::Paste,
        false,
    );
    let slots: Vec<u8> = (0..64).filter(|&k| k != 10).collect();
    for (i, &k) in slots.iter().skip(page * 7).take(7).enumerate() {
        let y = r.y + 112. + i as f32 * 31.;
        let occupied = t.slots[k as usize].is_some();
        button(
            Rect::new(r.x + 14., y, 28., 25.),
            if t.slot_on[k as usize] { "On" } else { "Off" },
            Command::Toggle(k),
            occupied && t.slot_on[k as usize],
        );
        let name = mixer::slot_name(s, t.index, k)
            .unwrap_or_else(|| format!("+ Add effect {}", usize::from(k) + 1));
        button(
            Rect::new(r.x + 48., y, 220., 25.),
            &name,
            if occupied {
                Command::Open(k)
            } else {
                Command::Add(k)
            },
            false,
        );
        if occupied {
            button(
                Rect::new(r.x + 275., y, 50., 25.),
                "Params",
                Command::Parameters(k),
                false,
            );
            button(
                Rect::new(r.x + 332., y, 35., 25.),
                "Up",
                Command::Move(k, -1),
                false,
            );
            button(
                Rect::new(r.x + 373., y, 35., 25.),
                "Down",
                Command::Move(k, 1),
                false,
            );
            button(
                Rect::new(r.x + 415., y, 80., 25.),
                "Remove",
                Command::Remove(k),
                false,
            );
        }
    }
    button(
        Rect::new(r.x + 14., r.bottom() - 38., 44., 24.),
        "<",
        Command::Page(-1),
        false,
    );
    button(
        Rect::new(r.x + 66., r.bottom() - 38., 95., 24.),
        &format!("{} / 9  >", page + 1),
        Command::Page(1),
        false,
    );
    button(
        Rect::new(r.right() - 94., r.bottom() - 38., 80., 24.),
        "Close",
        Command::Close,
        false,
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> (App, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("lemon-clip-fx-{name}-{}.wav", std::process::id()));
        let audio: Vec<f32> = (0..48000)
            .flat_map(|i| {
                let v = (i as f32 * 0.0575959).sin() * 0.2;
                [v, v]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), 48000, &audio)
            .unwrap();
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        a.state.project.tempo = Some(120.);
        a.state.tempo = 120.;
        a.plugins.available = crate::plugins::builtin();
        a.plugins.effects = crate::plugins::builtin_effects();
        let ch = &mut a.state.project.channels[0];
        ch.kind = ChannelKind::AudioClip;
        ch.sample_path = Some(path.to_string_lossy().into());
        ch.sound.sampler_flags = Some(os_model::ChannelSound::AUDIO_CLIP_FLAGS);
        pl::arr_mut(&mut a.state).clips = vec![
            os_model::Clip::new(0, 96, 0, ClipSource::Channel(0)),
            os_model::Clip::new(0, 96, 0, ClipSource::Channel(0)),
        ];
        (a, path)
    }
    fn render(p: &os_model::Project) -> Vec<f32> {
        let mut e = os_engine::Engine::new(
            p,
            &mut os_engine::SampleLibrary::new(None),
            48000,
            os_engine::PlayMode::Song,
        );
        e.render_all(512)
    }
    fn silent_slot() -> os_model::SlotPlugin {
        os_model::SlotPlugin {
            name: "Fruity Balance".into(),
            state: os_engine::effects::balance::Settings { v: [0, 0] }.to_state(),
        }
    }
    fn source(a: &App, i: usize) -> u16 {
        match pl::clips(&a.state)[i].source {
            ClipSource::Channel(id) => id,
            _ => panic!(),
        }
    }
    #[test]
    fn clip_fx_overlap_bypass_routing_recall_and_original_file() {
        let (mut a, path) = fixture("audio");
        let bytes = std::fs::read(&path).unwrap();
        let original = a.state.project.channels[0].clone();
        let mut one = a.state.project.clone();
        one.arrangements[0].clips.pop();
        let clean = render(&one);
        open(&mut a, 0).unwrap();
        let id = source(&a, 0);
        let t = bus(&a.state, id).unwrap();
        assert_eq!(source(&a, 1), 0);
        assert_eq!(a.state.project.channels[0], original);
        assert_eq!(
            mixer::track(&a.state, t).unwrap().routes,
            vec![original.mixer_track]
        );
        let m = mixer::track_mut(&mut a.state, t).unwrap();
        m.slots[0] = Some(silent_slot());
        m.slots_enabled = true;
        let processed = render(&a.state.project);
        let lo = 4000;
        let hi = 20000;
        let error = processed[lo..hi]
            .iter()
            .zip(&clean[lo..hi])
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(error < 1e-5, "other overlapping clip changed: {error}");
        assert!(clean[lo..hi].iter().any(|v| v.abs() > 0.01));
        mixer::track_mut(&mut a.state, t).unwrap().slots_enabled = false;
        let bypass = render(&a.state.project);
        let error = bypass[lo..hi]
            .iter()
            .zip(&clean[lo..hi])
            .map(|(a, b)| (a - 2. * b).abs())
            .fold(0f32, f32::max);
        assert!(error < 1e-5, "bypass: {error}");
        let file = os_flp::project::create(&a.state.project).unwrap();
        let reopened =
            os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap())
                .unwrap();
        assert_eq!(reopened.clip_fx_channels, a.state.project.clip_fx_channels);
        assert!(
            reopened
                .mixer
                .iter()
                .find(|m| m.index == t)
                .unwrap()
                .clip_fx
        );
        assert_eq!(render(&reopened)[lo..hi], bypass[lo..hi]);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn clip_fx_copies_have_independent_hosts_and_undo_preserves_automation() {
        let (mut a, path) = fixture("instances");
        open(&mut a, 0).unwrap();
        let id = source(&a, 0);
        let t = bus(&a.state, id).unwrap();
        let m = mixer::track_mut(&mut a.state, t).unwrap();
        m.slots[0] = Some(silent_slot());
        m.slots_enabled = true;
        sync(&mut a);
        let key = crate::plugins::fx_key(t, 0);
        let original = a.plugins.hosted[&key].plugin.clone();
        let mut copy = pl::clips(&a.state)[0];
        copy.position = 192;
        pl::arr_mut(&mut a.state).clips.push(copy);
        normalize(&mut a).unwrap();
        let second = source(&a, 2);
        let t2 = bus(&a.state, second).unwrap();
        assert_ne!(id, second);
        assert_ne!(t, t2);
        let copied = a.plugins.hosted[&crate::plugins::fx_key(t2, 0)]
            .plugin
            .clone();
        assert!(!std::sync::Arc::ptr_eq(&original, &copied));
        assert_eq!(
            a.plugins.state(key),
            a.plugins.state(crate::plugins::fx_key(t2, 0))
        );
        let target = os_model::target::slot_param(t, 0, 0);
        a.state.project.automations.push(os_model::Automation {
            channel: 4,
            targets: vec![target],
            points: vec![],
            header: vec![],
            tail: vec![],
        });
        command(&mut a, Command::Move(0, 1));
        assert_eq!(
            a.state.project.automations.last().unwrap().targets,
            vec![os_model::target::slot_param(t, 1, 0)]
        );
        assert!(mixer::track(&a.state, t).unwrap().slots[0].is_none());
        assert!(mixer::track(&a.state, t2).unwrap().slots[0].is_some());
        history::undo(&mut a.state);
        a.plugin_tick();
        assert_eq!(
            a.state.project.automations.last().unwrap().targets,
            vec![target]
        );
        assert!(mixer::track(&a.state, t).unwrap().slots[0].is_some());
        command(&mut a, Command::Copy);
        command(&mut a, Command::Remove(0));
        assert!(a
            .state
            .project
            .automations
            .last()
            .unwrap()
            .targets
            .is_empty());
        assert!(mixer::track(&a.state, t).unwrap().slots[0].is_none());
        command(&mut a, Command::Paste);
        assert!(mixer::track(&a.state, t).unwrap().slots[0].is_some());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn clip_fx_panel_bounds_undo_and_resource_failure_are_safe() {
        let (mut a, path) = fixture("panel");
        let before = a.state.project.clone();
        open(&mut a, 0).unwrap();
        let id = source(&a, 0);
        let t = bus(&a.state, id).unwrap();
        let mut canvas = os_ui::Canvas::new(1280, 900, 1.).unwrap();
        a.paint(&mut canvas, None);
        let panel = a.state.dialogs.last().unwrap().rect.get();
        let hits: Vec<_> = a
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::Dlg(DHit::ClipFx(_))))
            .collect();
        assert!(hits.len() > 10);
        for h in hits {
            assert!(
                h.rect.x >= panel.x
                    && h.rect.right() <= panel.right()
                    && h.rect.y >= panel.y
                    && h.rect.bottom() <= panel.bottom()
            );
        }
        a.state.dialogs.clear();
        history::undo(&mut a.state);
        a.plugin_tick();
        assert_eq!(a.state.project, before);
        history::redo(&mut a.state);
        a.plugin_tick();
        assert_eq!(source(&a, 0), id);
        assert!(mixer::track(&a.state, t).unwrap().clip_fx);
        for m in &mut a.state.project.mixer {
            if m.name.is_none() {
                m.name = Some("Reserved".into());
            }
        }
        let before = a.state.project.clone();
        assert!(open(&mut a, 1).is_err());
        assert_eq!(a.state.project, before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn clip_fx_respects_reserved_inserts_and_shared_bus_copies() {
        let (mut a, path) = fixture("reservations");
        let free = free_buses(&a.state);
        assert!(free.len() > 5);
        mixer::track_mut(&mut a.state, free[0]).unwrap().polarity = true;
        mixer::track_mut(&mut a.state, free[1]).unwrap().color =
            Some(os_model::Rgb::new(0x34, 0x56, 0x78));
        a.state.project.init_controls.push(os_model::InitControl {
            head: 0,
            target: os_model::target::mixer(free[2], os_model::target::MIXER_VOLUME),
            value: 12800,
        });
        let available = free_buses(&a.state);
        assert!(!available.contains(&free[0]));
        assert!(!available.contains(&free[1]));
        assert!(!available.contains(&free[2]));
        open(&mut a, 0).unwrap();
        let first = source(&a, 0);
        let t = bus(&a.state, first).unwrap();
        let mut ch = a
            .state
            .project
            .channels
            .iter()
            .find(|c| c.index == first)
            .unwrap()
            .clone();
        let id = ch.index + 1;
        ch.index = id;
        a.state.project.channels.push(ch);
        a.state.project.clip_fx_channels.push(id);
        pl::arr_mut(&mut a.state).clips[1].source = ClipSource::Channel(id);
        assert!(validate(&a.state.project).is_err());
        normalize(&mut a).unwrap();
        assert!(validate(&a.state.project).is_ok());
        assert_ne!(bus(&a.state, source(&a, 1)), Some(t));
        let mut duplicate = pl::clips(&a.state)[0];
        duplicate.position = 384;
        pl::arr_mut(&mut a.state).clips.push(duplicate);
        for m in &mut a.state.project.mixer {
            if m.name.is_none() {
                m.name = Some("Reserved".into());
            }
        }
        let before = a.state.project.clone();
        assert!(normalize(&mut a).is_err());
        assert_eq!(a.state.project, before);
        std::fs::remove_file(path).unwrap();
    }
}
