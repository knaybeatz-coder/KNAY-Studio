// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    app::State,
    history::{self, Snap},
    mixer, prefs,
};
use std::path::Path;

pub fn existing_recording(s: &State, path: &str) -> Option<usize> {
    if !prefs::check(&prefs::ROUTE_RACK_SAMPLES) {
        return None;
    }
    let target = crate::plugins::resolve_sample(path);
    let mut rows = s.project.channels.iter().enumerate().filter(|(_, ch)| {
        ch.kind == os_model::ChannelKind::AudioClip
            && !s.project.clip_fx_channels.contains(&ch.index)
            && ch.sample_path.as_deref().is_some_and(|p| {
                crate::plugin_db::same_path(
                    Path::new(&crate::plugins::resolve_sample(p)),
                    Path::new(&target),
                )
            })
            && s.project.arrangements.iter().any(|a| {
                a.clips
                    .iter()
                    .any(|c| c.source == os_model::ClipSource::Channel(ch.index))
            })
    });
    let first = rows.next()?.0;
    rows.next().is_none().then_some(first)
}

fn unused(s: &State, n: u16) -> bool {
    let Some(track) = mixer::track(s, n) else {
        return false;
    };
    let mut blank = os_model::MixerTrack::new(n);
    blank.slots_enabled = track.slots_enabled;
    let targets_insert = |id| {
        use os_model::target::Target::*;
        match os_model::target::decode(id) {
            MixerVolume(t)
            | MixerPan(t)
            | MixerSeparation(t)
            | MixerEq(t, ..)
            | MixerStrip(t, ..)
            | SlotOn(t, ..)
            | SlotMix(t, ..)
            | SlotParam(t, ..) => t == n,
            MixerSend(t, dst) => t == n || dst == n,
            _ => false,
        }
    };
    track == &blank
        && !s.project.channels.iter().any(|ch| ch.mixer_track == n)
        && !s.project.mixer.iter().any(|m| m.routes.contains(&n))
        && !s
            .project
            .arrangements
            .iter()
            .any(|a| a.tracks.iter().any(|t| t.comp == Some(n)))
        && !s
            .project
            .peak_links
            .iter()
            .any(|l| l.source == n || targets_insert(l.target))
        && !s
            .project
            .automations
            .iter()
            .flat_map(|a| &a.targets)
            .any(|&id| targets_insert(id))
        && !s
            .project
            .init_controls
            .iter()
            .any(|v| targets_insert(v.target))
        && !s
            .project
            .patterns
            .iter()
            .flat_map(|p| &p.controls)
            .any(|v| targets_insert(v.kind))
        && !s.mixer.recording.armed.contains(&n)
        && !s.mixer.recording.inputs.contains_key(&n)
}

// Capture the route and the sample as a single undo step. Existing inserts keep their names and effects.
pub fn begin(s: &mut State, existing: Option<usize>, path: &Path) -> usize {
    let row = existing.unwrap_or(s.project.channels.len());
    let route = existing.map_or(0, |r| s.project.channels[r].mixer_track);
    if !prefs::check(&prefs::ROUTE_RACK_SAMPLES) || route != 0 || s.project.mixer.len() < 2 {
        history::load_sample(s, row);
        if existing.is_none() {
            crate::app::add_channel(&mut s.project);
            s.project.channels[row].mixer_track = 0;
        }
        return row;
    }
    let free = (1..=mixer::inserts(s)).find(|&n| unused(s, n));
    let n = free.unwrap_or_else(|| mixer::inserts(s) + 1);
    let sample = existing.map_or(Snap::Channel { row, ch: None }, |_| {
        Snap::Several(vec![
            Snap::sample(s, row).unwrap(),
            Snap::ChannelRoute(row, route),
        ])
    });
    history::record(
        s,
        history::LOAD_SAMPLE,
        Snap::SampleRoute {
            sample: Box::new(sample),
            route: n,
            name: None,
            insert: free.is_none().then_some(true),
        },
    );
    if free.is_none() {
        let selection = (s.selected_mixer, s.mixer.sel.clone());
        mixer::add_track(s, None);
        (s.selected_mixer, s.mixer.sel) = selection;
    }
    if existing.is_none() {
        crate::app::add_channel(&mut s.project);
    }
    s.project.channels[row].mixer_track = n;
    if let Some(track) = mixer::track_mut(s, n) {
        track.name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
    }
    row
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rack_sample_routing_is_opt_in_and_protects_templates() {
        let mut a = crate::app::App::new(None);
        a.state.project = crate::app::default_project();
        a.state.project.mixer[2].name = Some("KICK".into());
        assert!(!prefs::check(&prefs::ROUTE_RACK_SAMPLES));
        a.sample_to_channel("snare.wav".into(), None);
        assert_eq!(a.state.project.channels[1].mixer_track, 0);
        prefs::set_check(&prefs::ROUTE_RACK_SAMPLES, true);
        a.sample_to_channel("snare2.wav".into(), Some(1));
        assert_eq!(a.state.project.channels[1].mixer_track, 3);
        assert_eq!(a.state.project.mixer[2].name.as_deref(), Some("KICK"));
        a.sample_to_channel("kick.wav".into(), Some(0));
        assert_eq!(a.state.project.channels[0].mixer_track, 1);
        for track in &mut a.state.project.mixer {
            if track.name.is_none() {
                track.name = Some("Reserved".into());
            }
        }
        let original = a.state.project.clone();
        a.sample_to_channel("hat.wav".into(), None);
        let after = a.state.project.clone();
        assert_eq!(after.mixer.len(), original.mixer.len() + 1);
        assert_eq!(after.channels.last().unwrap().mixer_track, 17);
        history::undo(&mut a.state).unwrap();
        assert_eq!(a.state.project.mixer, original.mixer);
        assert_eq!(a.state.project.channels, original.channels);
        history::redo(&mut a.state).unwrap();
        assert_eq!(a.state.project.mixer, after.mixer);
        assert_eq!(a.state.project.channels, after.channels);
    }
    #[test]
    fn existing_recording_reuses_the_playlist_lane_and_its_routing() {
        let mut a = crate::app::App::new(None);
        a.state.project = crate::app::default_project();
        let path = "recorded-vocal.wav";
        let ch = &mut a.state.project.channels[0];
        ch.kind = os_model::ChannelKind::AudioClip;
        ch.sample_path = Some(path.into());
        ch.name = Some("My vocal take".into());
        ch.mixer_track = 0;
        crate::playlist::arr_mut(&mut a.state)
            .clips
            .push(os_model::Clip::new(
                192,
                96,
                5,
                os_model::ClipSource::Channel(0),
            ));
        let lanes = a.state.project.arrangements.clone();
        prefs::set_check(&prefs::ROUTE_RACK_SAMPLES, true);
        a.sample_to_channel(path.into(), None);
        let insert = a.state.project.channels[0].mixer_track;
        assert_ne!(insert, 0);
        a.sample_to_channel(path.into(), None);
        assert_eq!(a.state.project.channels.len(), 1);
        assert_eq!(a.state.project.channels[0].mixer_track, insert);
        assert_eq!(
            a.state.project.channels[0].name.as_deref(),
            Some("My vocal take")
        );
        assert_eq!(a.state.project.arrangements, lanes);
    }
}
