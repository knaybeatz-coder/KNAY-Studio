use super::MixerInput;
use crate::{Engine, HostedEffect, Instrument, PlayMode, SampleLibrary};
use os_model::{Channel, ChannelKind, MixerTrack, Note, Pattern, Project, SlotPlugin};
use std::sync::{Arc, Mutex};

fn project() -> Project {
    Project {
        ppq: 96,
        tempo: Some(120.),
        mixer: (0..4).map(MixerTrack::new).collect(),
        channels: vec![Channel {
            index: 0,
            kind: ChannelKind::Plugin,
            name: None,
            plugin: None,
            plugin_state: None,
            color: None,
            enabled: true,
            volume: 0.78125,
            pan: 0.,
            pitch_cents: 0,
            mixer_track: 1,
            sample_path: None,
            layer_children: vec![],
            envelopes: Default::default(),
            rack: Default::default(),
            misc: Default::default(),
            sound: Default::default(),
        }],
        patterns: vec![Pattern {
            number: 1,
            notes: vec![Note {
                length: 192,
                ..Note::step(0, 0)
            }],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn engine(p: &Project) -> Engine {
    Engine::new(
        p,
        &mut SampleLibrary::default(),
        48000,
        PlayMode::Pattern(1),
    )
}
fn render(e: &mut Engine) -> Vec<f32> {
    let mut b = vec![0.; 8192];
    e.process(&mut b);
    b
}
fn peak(b: &[f32]) -> f32 {
    b.iter().fold(0., |m, x| m.max(x.abs()))
}

struct Multi;
impl Instrument for Multi {
    fn note_on(&mut self, _: u32, _: u8, _: f32) {}
    fn note_off(&mut self, _: u32, _: u8) {}
    fn process(&mut self, _: u32, l: &mut [f32], r: &mut [f32]) {
        l.fill(0.);
        r.fill(0.);
    }
    fn extra_outputs(&self) -> usize {
        3
    }
    fn extra_output(&mut self, p: usize, l: &mut [f32], r: &mut [f32]) -> bool {
        l.fill((p + 1) as f32 * 0.1);
        r.copy_from_slice(l);
        true
    }
}
#[test]
fn aux_boundary() {
    let mut p = project();
    p.channels[0].mixer_track = 3;
    let mut e = engine(&p);
    e.clear_schedule();
    e.set_awake(true);
    e.set_instrument(0, Arc::new(Mutex::new(Multi)));
    render(&mut e);
    let main = peak(e.track_output(3));
    println!(
        "AUX_BOUNDARY: main output is silent; all 3 aux destinations are beyond last track; last track actual peak={main:.6}"
    );
    assert_eq!(
        main, 0.,
        "unavailable aux outputs must not fold into the last insert"
    );
    p.channels[0].mixer_track = 0;
    let mut e = engine(&p);
    e.clear_schedule();
    e.set_awake(true);
    e.set_instrument(0, Arc::new(Mutex::new(Multi)));
    render(&mut e);
    let peaks: Vec<_> = (1..4).map(|t| peak(e.track_output(t))).collect();
    println!("AUX_BOUNDARY control: enough tracks available, separate aux peaks={peaks:?}");
    assert!((peaks[1] / peaks[0] - 2.).abs() < 0.001 && (peaks[2] / peaks[0] - 3.).abs() < 0.001);
    assert!(peaks.iter().sum::<f32>() > 0.2);
}
struct Input {
    at: usize,
    both: bool,
}
impl MixerInput for Input {
    fn render(&mut self, _: u32, tracks: &mut [Vec<f32>]) {
        let frames = tracks[0].len() / 2;
        for i in 0..frames {
            if self.at + i == 2000 {
                for t in if self.both { vec![1, 2] } else { vec![1] } {
                    tracks[t][2 * i] += 0.25;
                    tracks[t][2 * i + 1] += 0.25;
                }
            }
        }
        self.at += frames;
    }
}
struct Delay {
    q: Vec<(f32, f32)>,
    at: usize,
}
impl HostedEffect for Delay {
    fn reported_delay_fx(&self) -> u32 {
        self.q.len() as u32
    }
    fn process_fx(&mut self, _: u32, l: &mut [f32], r: &mut [f32]) {
        for (a, b) in l.iter_mut().zip(r) {
            std::mem::swap(a, &mut self.q[self.at].0);
            std::mem::swap(b, &mut self.q[self.at].1);
            self.at = (self.at + 1) % self.q.len();
        }
    }
}
#[test]
fn input_pdc() {
    let mut p = project();
    p.channels.clear();
    p.mixer[1].routes = vec![2];
    p.mixer[2].routes = vec![0];
    let mut e = engine(&p);
    e.clear_schedule();
    e.set_awake(true);
    e.set_mixer_input(Box::new(Input { at: 0, both: true }));
    e.set_effect(
        1,
        0,
        Arc::new(Mutex::new(Delay {
            q: vec![(0., 0.); 512],
            at: 0,
        })),
    );
    let b = render(&mut e);
    let hits: Vec<_> = b
        .chunks_exact(2)
        .enumerate()
        .filter(|(_, f)| f[0].abs() > 0.01)
        .map(|(i, f)| (i, f[0]))
        .collect();
    println!(
        "LIVE_INPUT_PDC: simultaneous inputs into insert 1 and bus 2; insert 1 has 512-frame latency and routes to bus 2; output impulses={hits:?}"
    );
    assert_eq!(
        hits,
        vec![(2000, 0.5)],
        "receiving bus input must align with the delayed route"
    );
    let mut e = engine(&p);
    e.clear_schedule();
    e.set_awake(true);
    e.set_mixer_input(Box::new(Input { at: 0, both: true }));
    let b = render(&mut e);
    let hits: Vec<_> = b
        .chunks_exact(2)
        .enumerate()
        .filter(|(_, f)| f[0].abs() > 0.01)
        .map(|(i, f)| (i, f[0]))
        .collect();
    println!("LIVE_INPUT_PDC control: without delayed plugin, impulses={hits:?}");
    assert_eq!(hits, vec![(2000, 0.5)]);
}
#[derive(Default)]
struct MidiLog {
    on: usize,
}
impl Instrument for MidiLog {
    fn note_on(&mut self, _: u32, _: u8, _: f32) {
        self.on += 1;
    }
    fn note_off(&mut self, _: u32, _: u8) {}
    fn midi(&mut self, _: u32, b: [u8; 3]) {
        if b[0] & 0xf0 == 0x90 && b[2] > 0 {
            self.on += 1;
        }
    }
    fn process(&mut self, _: u32, l: &mut [f32], r: &mut [f32]) {
        l.fill(0.);
        r.fill(0.);
    }
}
fn seek(mpe: bool, truncated: bool) -> usize {
    let mut p = project();
    p.mpe.set_config(
        0,
        os_model::mpe::Config {
            enabled: mpe,
            ..Default::default()
        },
    );
    let log = Arc::new(Mutex::new(MidiLog::default()));
    let mut e = engine(&p);
    e.set_instrument(0, log.clone());
    e.set_truncated_on_transport(truncated);
    e.seek_tick(96);
    render(&mut e);
    let n = log.lock().unwrap().on;
    n
}
#[test]
fn mpe_seek() {
    let normal = seek(false, false);
    let mpe = seek(true, false);
    let enabled = seek(true, true);
    let normal_on = seek(false, true);
    println!(
        "MPE_SEEK: seek into held note; play truncated notes OFF: ordinary={normal}, MPE={mpe}; ON: ordinary={normal_on}, MPE={enabled}"
    );
    assert_eq!(normal, 0);
    assert_eq!(mpe, 0);
    assert_eq!(enabled, 1);
    assert_eq!(normal_on, 1);
}
struct Constant;
impl MixerInput for Constant {
    fn render(&mut self, _: u32, tracks: &mut [Vec<f32>]) {
        tracks[1].fill(0.25);
    }
}
#[test]
fn own_state() {
    let mut p = project();
    p.channels.clear();
    p.mixer[1].slots[0] = Some(SlotPlugin {
        name: "Fruity Balance".into(),
        state: crate::effects::balance::Settings { v: [0, 256] }.to_state(),
    });
    let mut old = engine(&p);
    old.clear_schedule();
    old.set_awake(true);
    old.set_mixer_input(Box::new(Constant));
    let before = peak(&render(&mut old));
    p.mixer[1].slots[0].as_mut().unwrap().state =
        crate::effects::balance::Settings { v: [0, 0] }.to_state();
    let mut next = engine(&p);
    next.clear_schedule();
    next.continue_from(&mut old);
    for _ in 0..5 {
        render(&mut next);
    }
    let after = peak(&render(&mut next));
    let mut fresh = engine(&p);
    fresh.clear_schedule();
    fresh.set_awake(true);
    fresh.set_mixer_input(Box::new(Constant));
    let fresh = peak(&render(&mut fresh));
    println!(
        "OWN_EFFECT_STATE: same slot Balance state changed from unity to silence; before={before}, live rebuild={after}, fresh engine={fresh}"
    );
    assert!(before > 0.2);
    assert_eq!(after, fresh);
    assert_eq!(fresh, 0.);
}
