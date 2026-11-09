// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reversible instrument freeze and independent Playlist track bounces.
use crate::{
    app::{App, State},
    pattern_render as pr, playlist as pl,
};
use os_model::{
    target::{self, Target},
    ChannelKind, Clip, ClipSource, MixerTrack, PlaylistTrack, Project,
};
use std::path::PathBuf;

pub const VISIBILITY: &str = "visible_playlist-bounce-audio";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Freeze,
    BounceNew,
    BounceReplace,
}

#[derive(Debug, Clone)]
pub struct Pending {
    row: u16,
    mode: Mode,
    track: PlaylistTrack,
    name: String,
    original: Vec<Clip>,
    arrangement: u16,
    master_stem: u16,
}
fn automation(p: &Project, c: &Clip) -> bool {
    matches!(c.source,ClipSource::Channel(ch) if p.channels.iter().any(|v|v.index==ch&&v.kind==ChannelKind::Automation))
}
fn mixer_target(t: u32) -> Option<u16> {
    match target::decode(t) {
        Target::MixerVolume(t)
        | Target::MixerPan(t)
        | Target::MixerSeparation(t)
        | Target::MixerEq(t, ..)
        | Target::MixerStrip(t, ..)
        | Target::MixerSend(t, ..)
        | Target::SlotOn(t, ..)
        | Target::SlotMix(t, ..)
        | Target::SlotParam(t, ..) => Some(t),
        _ => None,
    }
}
fn neutral(index: u16) -> MixerTrack {
    let mut m = MixerTrack::new(index);
    m.slot_on = [false; 64];
    m.strip_bypassed = true;
    m.deess = Some(0);
    m
}
fn destination(s: &State, row: u16) -> Result<u16, String> {
    let mut at = row + 1;
    // Never split a comp/group leader from its children.
    while at < pl::TRACKS && pl::track_info(s, at).is_some_and(|t| t.grouped) {
        at += 1;
    }
    let last = pl::TRACKS - 1;
    if at >= pl::TRACKS
        || pl::clips(s).iter().any(|c| c.track == last)
        || pl::track_info(s, last)
            .is_some_and(|t| t.name.is_some() || t.grouped || t.comp.is_some() || t.frozen.is_some() || t.locked)
        || s.project.instrument_lanes.iter().any(|v| v.2 == last)
    {
        return Err("Bounce needs one empty row at the bottom of the Playlist. No tracks were changed.".into());
    }
    Ok(at)
}
/// Prepare a render without modifying the user's project. Keep automation at its original song time.
fn prepare(s: &State, row: u16, mode: Mode) -> Result<(Project, crate::export::Job, Pending), String> {
    let t = pl::track_info(s, row).ok_or("Track no longer exists")?;
    let freeze = mode == Mode::Freeze;
    if mode != Mode::BounceNew {
        if t.frozen.is_some() {
            return Err("This track is frozen. Choose Unfreeze track first.".into());
        }
        if t.locked {
            return Err("Unlock the track before rendering in place.".into());
        }
    }
    if crate::comping::parent(s, row).is_some_and(|p| p != row) {
        return Err("Choose the main comp track to freeze or bounce the selected takes.".into());
    }
    if mode == Mode::BounceNew {
        destination(s, row)?;
    }
    let original: Vec<_> = pl::clips(s).iter().filter(|c| c.track == row).copied().collect();
    let audible: Vec<_> = original
        .iter()
        .filter(|c| !c.muted && !automation(&s.project, c))
        .copied()
        .collect();
    let end = audible
        .iter()
        .map(Clip::end)
        .max()
        .filter(|e| *e > 0)
        .ok_or("This track has no unmuted audio or pattern clips to render.")?;
    let mut p = crate::rack::play_project(s);
    let ids = p.clip_channels(&audible);
    let rows: Vec<_> = p
        .channels
        .iter()
        .enumerate()
        .filter(|(_, c)| ids.contains(&c.index))
        .map(|(i, _)| i)
        .collect();
    let auto: Vec<_> = pl::clips(s)
        .iter()
        .filter(|c| automation(&p, c) && pl::track_on(s, c.track))
        .copied()
        .collect();
    let a = p.arrangement_mut().ok_or("No arrangement")?;
    let arrangement = a.number;
    a.clips = audible;
    a.clips.extend(auto);
    for t in &mut a.tracks {
        t.enabled = true;
        t.frozen = None;
    }
    a.markers.clear();
    if p.mixer.is_empty() {
        p.mixer.push(neutral(0));
    }
    let master_stem = p.mixer.len() as u16;
    if freeze {
        // Individual pre-mixer stems: preserve the real mixer, buses, sends and sidechains during playback.
        for m in &mut p.mixer {
            *m = neutral(m.index);
        }
        p.mixer.push(neutral(master_stem));
        for c in &mut p.channels {
            if c.mixer_track == 0 {
                c.mixer_track = master_stem;
            }
        }
        p.peak_links.clear();
    } else {
        // Bake inserts and their outgoing buses, but never bake the master twice.
        if let Some(m) = p.mixer.first_mut() {
            *m = neutral(0);
        }
        let mut used: std::collections::BTreeSet<u16> = p
            .channels
            .iter()
            .filter(|c| ids.contains(&c.index))
            .map(|c| c.mixer_track)
            .collect();
        loop {
            let n = used.len();
            for m in &p.mixer {
                if used.contains(&m.index) {
                    used.extend(m.routes.iter().copied().filter(|dst| {
                        m.send_levels.iter().find(|(t,_)|t==dst).is_none_or(|(_,v)|*v>0.0)
                            || p.automations.iter().any(|a|a.targets.iter().any(|t|matches!(target::decode(*t),Target::MixerSend(src,d) if src==m.index && d==*dst)))
                    }));
                }
            }
            if used.len() == n {
                break;
            }
        }
        // Zero-level routes are only possible inputs, not proof that a plugin uses
        // them. The factory template has many such dormant send connections.
        for m in p.mixer.iter().filter(|m|used.contains(&m.index) && m.enabled && m.slots_enabled) {
            let mut inputs:Vec<_>=p.mixer.iter().filter(|src|src.index!=m.index && src.routes.contains(&m.index)).map(|src|src.index).collect();
            inputs.sort_unstable();
            for (slot,fx) in m.slots.iter().enumerate().filter_map(|(i,f)|f.as_ref().map(|f|(i,f))) {
                if !m.slot_on[slot] || m.slot_mix[slot]<=0.0 {continue;}
                let ports=if fx.name==os_flp::wrapper::WRAPPER {
                    os_flp::wrapper::input_ports(&fx.state).into_iter().filter(|p|*p>0).map(|p|p as usize).collect::<Vec<_>>()
                } else {
                    os_engine::effects::from_slot(&fx.name,&fx.state,44100).map(|f|vec![f.key_input()]).unwrap_or_default()
                };
                if ports.into_iter().filter_map(|port|port.checked_sub(1).and_then(|i|inputs.get(i))).any(|src|!used.contains(src)) {
                    return Err("This track uses an external sidechain. Freeze keeps that sidechain live; for a wet bounce, render the participating tracks together.".into());
                }
            }
        }
        for m in &mut p.mixer {
            if !used.contains(&m.index) && m.index != 0 {
                let routes=std::mem::take(&mut m.routes);
                let levels=std::mem::take(&mut m.send_levels);
                *m = neutral(m.index);
                // Keep input numbering stable for plugins on the rendered buses.
                m.routes=routes;m.send_levels=levels;m.enabled = false;
            }
        }
    }
    let keep = |t: u32| mixer_target(t).is_none_or(|m| !freeze && m != 0);
    for a in &mut p.automations {
        a.targets.retain(|t| keep(*t));
    }
    p.init_controls.retain(|v| keep(v.target));
    let name = pl::track_name(s, row);
    let dir = pr::folder(s);
    let stem = pr::file_stem(&dir, &format!("{name} - {}", if freeze { "Frozen" } else { "Bounce" }));
    let mut job = pr::job(s, &dir, &stem);
    job.span = Some((0, u64::from(end)));
    job.instrument_rows = Some(rows);
    job.capture_tracks = freeze.then(|| {
        p.channels
            .iter()
            .filter(|c| ids.contains(&c.index))
            .map(|c| c.mixer_track as usize)
            .collect()
    });
    job.options.split = freeze;
    job.options.split_master = false;
    job.options.insert_fx = !freeze;
    job.options.master_fx = false;
    job.track_names = (0..p.mixer.len()).map(|i| format!("Freeze {i}")).collect();
    Ok((
        p,
        job,
        Pending {
            row,
            mode,
            track: t.clone(),
            name,
            original,
            arrangement,
            master_stem,
        },
    ))
}
pub fn unfreeze(s: &mut State, row: u16) -> String {
    let Some(original) = pl::track_info(s, row).and_then(|t| t.frozen.clone()) else {
        return "This track is not frozen".into();
    };
    pl::snapshot_as(s, "Unfreeze track");
    let a = pl::arr_mut(s);
    a.clips.retain(|c| c.track != row);
    a.clips.extend(original.into_iter().map(|c| Clip { track: row, ..c }));
    a.tracks[row as usize].frozen = None;
    s.playlist.sel.clear();
    s.piano.dirty = true;
    "Track unfrozen; original clips restored".into()
}
fn place(s: &mut State, p: &Pending, files: &[PathBuf]) -> Result<String, String> {
    if s.project.arrangement().map(|a| a.number) != Some(p.arrangement)
        || pl::track_info(s, p.row) != Some(&p.track)
        || pl::clips(s)
            .iter()
            .filter(|c| c.track == p.row)
            .copied()
            .collect::<Vec<_>>()
            != p.original
    {
        return Err("The source track changed during rendering. The audio was saved in Rendered; the Playlist was left untouched.".into());
    }
    let freeze = p.mode == Mode::Freeze;
    let mut stems = Vec::new();
    for (i, path) in files.iter().enumerate() {
        let mixer = if freeze {
            if i == 0 {
                continue;
            }
            let m = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.split(' ').next())
                .and_then(|n| n.parse::<u16>().ok())
                .ok_or("Unrecognized rendered stem")?;
            if m == p.master_stem {
                0
            } else {
                m
            }
        } else {
            0
        };
        let text = path.to_string_lossy().into_owned();
        let w = s
            .playlist
            .wave(&text)
            .ok_or("The rendered audio could not be read. Original clips were preserved.")?;
        let length = (w.secs() * s.project.tick_tempo(s.tempo) * f64::from(s.project.ppq) / 60.0)
            .ceil()
            .max(1.0) as u32;
        stems.push((text, mixer, length));
        if !freeze {
            break;
        }
    }
    if stems.is_empty() {
        return Err("The render was silent; original clips were preserved.".into());
    }
    let row = if p.mode == Mode::BounceNew { destination(s, p.row)? } else { p.row };
    let before = crate::history::Snap::Rack(Box::new(crate::history::RackSide::now(s, s.project.channels.len())));
    let action = match p.mode {
        Mode::Freeze => "Freeze track",
        Mode::BounceNew => "Bounce audio in new track",
        Mode::BounceReplace => "Bounce and replace track",
    };
    pl::snapshot_with(s, action, before);
    if p.mode == Mode::BounceNew {
        pl::insert_track(s, row);
        // As when muting a track by hand, end solo without toggling the other track states.
        s.playlist.solo = None;
        let a = pl::arr_mut(s);
        a.tracks[p.row as usize].enabled = false;
        a.tracks[row as usize].name = Some(format!("{} - Bounce", p.name));
        a.tracks[row as usize].color = p.track.color;
    } else {
        let kept = pl::clips(s).iter().copied()
            .filter(|c| c.track != row || automation(&s.project, c)).collect();
        let a = pl::arr_mut(s);
        a.clips = kept;
        if freeze {
            a.tracks[row as usize].frozen = Some(p.original.clone());
        }
    }
    let mut selection = Vec::new();
    for (path, mixer, length) in stems {
        let ch = pr::add_channel(s, &path, s.pattern);
        if let Some(c) = s.project.channels.last_mut() {
            c.name = Some(format!("{} - {}", p.name, if freeze { "Frozen" } else { "Bounce" }));
            c.mixer_track = mixer;
            c.misc.main_pitch = false;
        }
        let a = pl::arr_mut(s);
        selection.push(a.clips.len());
        a.clips.push(Clip::new(0, length, row, ClipSource::Channel(ch)));
    }
    s.playlist.sel = selection;
    s.playlist.track = row;
    s.piano.dirty = true;
    Ok(match p.mode {
        Mode::Freeze => "Track frozen. Mixer processing stays live.",
        Mode::BounceNew => "Bounced audio added below the source. The source track is muted.",
        Mode::BounceReplace => "Track clips replaced with bounced audio. Undo restores the original clips.",
    }.into())
}
impl App {
    pub(crate) fn render_track(&mut self, row: u16, mode: Mode) {
        if let Err(e)=crate::clip_effects::normalize(self){self.state.message=Some(e);self.invalidate();return;}
        if self.export.is_some() {
            return;
        }
        match prepare(&self.state, row, mode) {
            Ok((p, job, pending)) => {
                if let Err(e) = std::fs::create_dir_all(&job.folder) {
                    self.state.message = Some(format!("Cannot create render folder: {e}"));
                    self.invalidate();
                    return;
                }
                self.track_render = Some(pending);
                self.start_project_export(p, job, true);
            }
            Err(e) => {
                self.state.message = Some(e);
                self.invalidate();
            }
        }
    }
    pub(crate) fn track_render_done(&mut self, p: Pending, r: Result<(f64, Vec<PathBuf>), String>) {
        self.state.message = Some(match r.and_then(|(_, files)| place(&mut self.state, &p, &files)) {
            Ok(m) => m,
            Err(e) => format!("Track render: {e}"),
        });
        self.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(tag: &str) -> (App, PathBuf) {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.tempo = 120.;
        let dir = std::env::temp_dir().join(format!("lemon-track-render-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Source.wav");
        let audio: Vec<_> = (0..22050)
            .flat_map(|i| {
                let v = (i as f32 * 0.073).sin() * 0.15;
                [v, v * 0.7]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), 44100, &audio).unwrap();
        let s = &mut app.state;
        s.project.data_folder = Some(os_model::DataFolder {
            choice: os_model::DataFolder::SPECIFIED,
            folder: crate::data_folder::folder_text(&dir),
        });
        let ch = &mut s.project.channels[0];
        ch.kind = ChannelKind::AudioClip;
        ch.sample_path = Some(path.to_string_lossy().into_owned());
        ch.mixer_track = 1;
        ch.sound.sampler_flags = Some(os_model::ChannelSound::AUDIO_CLIP_FLAGS);
        pl::arr_mut(s).clips.push(Clip::new(96, 96, 0, ClipSource::Channel(0)));
        (app, dir)
    }
    fn finish(app: &mut App) {
        let now = std::time::Instant::now();
        while app.export.is_some() {
            assert!(now.elapsed().as_secs() < 60);
            app.export_tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.track_render.is_none());
    }
    fn audio(p: &Project) -> Vec<f32> {
        let mut lib = os_engine::SampleLibrary::new(None);
        let mut e = os_engine::Engine::new(
            p,
            &mut lib,
            crate::transport::Transport::device_rate().unwrap_or(44100),
            os_engine::PlayMode::Song,
        );
        e.set_interpolation(512);
        e.set_span(0, 192);
        e.render_all(192)
    }
    #[test]
    fn assigned_bounce_template_routes() {
        let (mut app,dir)=fixture("assigned");
        let bytes=include_bytes!("../assets/templates/Lemon Starter.flp");
        app.state.project.mixer=os_flp::project::load(&os_flp::FlpFile::parse(bytes).unwrap()).unwrap().mixer;
        for route in [0,8,21,32] {
            app.state.project.channels[0].mixer_track=route;
            assert!(prepare(&app.state, 0, Mode::BounceNew).is_ok(), "route {route}");
        }
        app.state.project.channels[0].mixer_track=8;
        let original=pl::clips(&app.state).to_vec();
        app.render_track(0, Mode::BounceNew);finish(&mut app);
        assert_eq!(pl::clips(&app.state)[0], original[0]);
        assert!(!pl::track_on(&app.state, 0));
        assert!(pl::track_on(&app.state, 1));
        let bounced=pl::clips(&app.state).iter().find(|c|c.track==1).expect("bounce on next row");
        let ClipSource::Channel(ch)=bounced.source else {panic!("audio bounce")};
        let path=app.state.project.channels.iter().find(|c|c.index==ch).unwrap().sample_path.as_ref().unwrap();
        let sound=os_engine::sample::load(std::path::Path::new(path)).unwrap();
        assert!(sound.left.iter().any(|v|v.abs()>0.001),"assigned bounce must be audible");
        let file = os_flp::project::create(&app.state.project).unwrap();
        let reopened = os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap()).unwrap();
        assert!(!reopened.arrangement().unwrap().tracks[0].enabled);
        crate::history::undo(&mut app.state).unwrap();
        assert_eq!(pl::clips(&app.state), original);
        assert!(pl::track_on(&app.state, 0));
        crate::history::redo(&mut app.state).unwrap();
        assert!(!pl::track_on(&app.state, 0));
        assert!(pl::track_on(&app.state, 1));
        let _=std::fs::remove_dir_all(dir);
    }
    #[test]
    fn track_render_freeze_roundtrip_audio_and_undo() {
        let (mut app, dir) = fixture("freeze");
        app.state.project.mixer[1].volume = Some(0.6);
        app.state.project.mixer[1].pan = Some(0.3);
        app.state.project.mixer[0].volume = Some(0.5);
        let before = crate::rack::play_project(&app.state);
        let expected = audio(&before);
        let source = pl::clips(&app.state).to_vec();
        let count = app.state.project.channels.len();
        app.render_track(0, Mode::Freeze);
        finish(&mut app);
        assert!(
            pl::track_info(&app.state, 0).unwrap().frozen.is_some(),
            "{:?}",
            app.state.message
        );
        let got = audio(&crate::rack::play_project(&app.state));
        let err = expected
            .iter()
            .zip(&got)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(expected.iter().any(|v| v.abs() > 0.001));
        assert!(
            err < 0.0001,
            "audio changed by {err}; worst {:?}; rate {:?}",
            expected
                .iter()
                .zip(&got)
                .enumerate()
                .max_by(|(_, (a, b)), (_, (c, d))| (*a - *b).abs().total_cmp(&(*c - *d).abs())),
            crate::transport::Transport::device_rate()
        );
        assert_eq!(app.state.project.mixer, before.mixer);
        let file = os_flp::project::create(&app.state.project).unwrap();
        let loaded = os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap()).unwrap();
        assert!(loaded.arrangement().unwrap().tracks[0].frozen.is_some());
        assert!(crate::history::undo(&mut app.state).is_some());
        assert_eq!(pl::clips(&app.state), source);
        assert_eq!(app.state.project.channels.len(), count);
        crate::history::redo(&mut app.state);
        app.state.project = loaded;
        pl::reorder_track(&mut app.state, 0, 3);
        let row = app
            .state
            .project
            .arrangement()
            .unwrap()
            .tracks
            .iter()
            .position(|t| t.frozen.is_some())
            .unwrap() as u16;
        assert!(unfreeze(&mut app.state, row).starts_with("Track unfrozen"));
        let restored = pl::clips(&app.state).iter().find(|c| c.track == row).unwrap();
        assert_eq!(
            (restored.position, restored.length, restored.source, restored.window),
            (source[0].position, source[0].length, source[0].source, source[0].window)
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn track_render_bounce_comp_preserves_source_and_group() {
        let (mut app, dir) = fixture("bounce");
        let s = &mut app.state;
        pl::arr_mut(s).tracks[0].comp = Some(1);
        let mut take = pl::clips(s)[0];
        take.track = 1;
        take.muted = true;
        let a = pl::arr_mut(s);
        a.tracks[1].grouped = true;
        a.tracks[1].enabled = false;
        a.clips.push(take);
        a.clips.push(Clip::new(192, 96, 2, ClipSource::Channel(0)));
        let original = pl::clips(s)[0];
        let count = s.project.channels.len();
        app.render_track(0, Mode::BounceNew);
        finish(&mut app);
        assert_eq!(pl::clips(&app.state)[0], original, "original clip untouched");
        assert!(!pl::track_on(&app.state, 0), "source row muted");
        assert!(!pl::track_on(&app.state, 1), "take lane kept its mute state");
        assert_eq!(crate::comping::parent(&app.state, 1), Some(0));
        assert_eq!(pl::clips(&app.state).iter().find(|c| c.track == 1), Some(&take));
        let bounced = pl::clips(&app.state)
            .iter()
            .find(|c| c.track == 2)
            .expect("new row after comp family");
        assert!(!bounced.muted && pl::track_on(&app.state, 2));
        assert!(!pl::track_info(&app.state, 2).unwrap().grouped);
        assert!(pl::clips(&app.state).iter().any(|c| c.track == 3 && c.position == 192));
        assert_eq!(app.state.project.channels.len(), count + 1);
        crate::history::undo(&mut app.state);
        assert_eq!(app.state.project.channels.len(), count);
        assert!(pl::clips(&app.state).iter().any(|c| c.track == 2 && c.position == 192));
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn track_render_replace_keeps_automation_and_other_rows_and_undoes() {
        let (mut app, dir) = fixture("replace");
        app.state.project.channels[1].kind = ChannelKind::Automation;
        let automation = Clip::new(0, 192, 0, ClipSource::Channel(1));
        let other = Clip::new(192, 96, 1, ClipSource::Channel(0));
        let a = pl::arr_mut(&mut app.state);
        a.clips.extend([automation, other]);
        a.tracks[0].name = Some("Voice".into());
        a.tracks[1].name = Some("Other".into());
        let tracks = a.tracks.clone();
        let original = a.clips.clone();
        let count = app.state.project.channels.len();

        let (_, _, pending) = prepare(&app.state, 0, Mode::BounceReplace).unwrap();
        assert!(place(&mut app.state, &pending, &[dir.join("missing.wav")]).is_err());
        assert_eq!(pl::clips(&app.state), original);
        assert_eq!(app.state.project.channels.len(), count);

        let menu = pl::track_menu(&app.state, 0, 0., 0.);
        let bounce = menu.items.iter().find(|item| item.label == "Bounce audio").unwrap();
        assert_eq!(bounce.children.len(), 2);
        pl::run(&mut app, bounce.children[1].cmd.clone());
        finish(&mut app);
        let clips = pl::clips(&app.state);
        assert_eq!(&clips[..2], &[automation, other]);
        assert_eq!(clips.len(), 3);
        assert_eq!(clips[2].track, 0);
        assert_eq!(app.state.project.channels.len(), count + 1);
        assert_eq!(app.state.project.arrangement().unwrap().tracks, tracks);
        let rendered = clips.to_vec();
        crate::history::undo(&mut app.state).unwrap();
        assert_eq!(pl::clips(&app.state), original);
        assert_eq!(app.state.project.channels.len(), count);
        crate::history::redo(&mut app.state).unwrap();
        assert_eq!(pl::clips(&app.state), rendered);
        crate::workspace_visibility::toggle(VISIBILITY);
        assert!(!pl::track_menu(&app.state, 0, 0., 0.).items.iter().any(|item| item.label == "Bounce audio"));
        assert_eq!(pl::clips(&app.state), rendered);
        let _=std::fs::remove_dir_all(dir);
    }
    #[test]
    fn track_render_guards_and_master_stem() {
        let (mut app, dir) = fixture("master");
        app.state.project.channels[0].mixer_track = 0;
        let expected = audio(&crate::rack::play_project(&app.state));
        app.render_track(0, Mode::Freeze);
        finish(&mut app);
        assert!(
            pl::track_info(&app.state, 0).unwrap().frozen.is_some(),
            "{:?}",
            app.state.message
        );
        let got = audio(&crate::rack::play_project(&app.state));
        assert!(expected.iter().zip(got).all(|(a, b)| (a - b).abs() < 0.0001));
        assert_eq!(app.state.project.channels.last().unwrap().mixer_track, 0);
        unfreeze(&mut app.state, 0);
        pl::arr_mut(&mut app.state).tracks[(pl::TRACKS - 1) as usize].name = Some("Keep".into());
        assert!(prepare(&app.state, 0, Mode::BounceNew).is_err());
        assert!(prepare(&app.state, 0, Mode::BounceReplace).is_ok());
        assert!(prepare(&app.state, 12, Mode::Freeze).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn track_render_multiple_routes_and_hosted_plugin() {
        use std::sync::{Arc, Mutex};
        struct Tone(bool);
        impl os_engine::Instrument for Tone {
            fn note_on(&mut self, _: u32, _: u8, _: f32) {
                self.0 = true;
            }
            fn note_off(&mut self, _: u32, _: u8) {
                self.0 = false;
            }
            fn process(&mut self, _: u32, l: &mut [f32], r: &mut [f32]) {
                l.fill(if self.0 { 0.1 } else { 0. });
                r.copy_from_slice(l);
            }
            fn reset(&mut self) {
                self.0 = false;
            }
        }
        let (mut app, dir) = fixture("hosted");
        app.state.project.channels[1].kind = ChannelKind::Plugin;
        app.state.project.channels[1].mixer_track = 2;
        app.state.project.patterns[0].notes.push(os_model::Note::step(1, 0));
        pl::arr_mut(&mut app.state)
            .clips
            .push(Clip::new(96, 96, 0, ClipSource::Pattern(1)));
        app.transport.set_instruments(vec![(
            1,
            Arc::new(Mutex::new(Tone(false))) as os_engine::SharedInstrument,
        )]);
        let original = pl::clips(&app.state).to_vec();
        app.render_track(0, Mode::Freeze);
        finish(&mut app);
        let routes: Vec<_> = app
            .state
            .project
            .channels
            .iter()
            .skip(5)
            .map(|c| c.mixer_track)
            .collect();
        assert_eq!(routes, vec![1, 2], "{:?}", app.state.message);
        assert!(app.state.project.frozen_instrument_rows().contains(&1));
        unfreeze(&mut app.state, 0);
        assert_eq!(pl::clips(&app.state), original);
        assert!(app.state.project.frozen_instrument_rows().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
