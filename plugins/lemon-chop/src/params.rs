// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent cue layout and host-visible controls. Source files are never modified.
use nih_plug::prelude::*;
use os_plugkit::live::LiveEdits;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
pub const PADS: usize = 32;
pub const GLOBAL: usize = 16;
pub const STRIDE: usize = 8;
pub const LEVEL: usize = 0;
pub const FILTER: usize = 1;
pub const ATTACK: usize = 2;
pub const RELEASE: usize = 3;
pub const REVERSE: usize = 4;
pub const PITCH: usize = 5;
pub const SPEED: usize = 6;
pub const OUTPUT: usize = 7;
pub const SOURCE_BPM: usize = 0;
pub const TARGET_BPM: usize = 1;
pub const SYNC: usize = 2;
pub const KEY: usize = 3;
pub const DETUNE: usize = 4;
pub const HOLD: usize = 5;
pub const POLY: usize = 6;
pub const VELOCITY: usize = 7;
pub const KEYBOARD: usize = 8;
pub const LINK: usize = 9;
pub const MASTER: usize = 10;
pub const GLIDE: usize = 11;
pub fn pad(n: usize, k: usize) -> usize {
    GLOBAL + n * STRIDE + k
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub enabled: bool,
    pub favorite: bool,
    pub name: String,
    pub color: Option<u32>,
}
impl Default for Cue {
    fn default() -> Self {
        Self {
            start: 0.,
            end: 0.,
            enabled: false,
            favorite: false,
            name: String::new(),
            color: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Document {
    pub path: String,
    pub stems: [String; 4],
    pub cues: Vec<Cue>,
    pub selected: usize,
    pub grid_start: f64,
    pub markers: Vec<f64>,
    /// Musical beat positions of tempo anchors; absent in older projects (four beats per anchor).
    pub marker_beats: Vec<f64>,
    pub key: String,
    pub glide_trigger: bool,
    pub slab: bool,
    pub analysis_complete: bool,
    /// Older projects retain their end-of-cue release behavior.
    #[serde(default)]
    pub trigger_decay: bool,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            path: String::new(),
            stems: Default::default(),
            cues: vec![Cue::default(); 32],
            selected: 0,
            grid_start: 0.,
            markers: vec![],
            marker_beats: vec![],
            key: String::new(),
            glide_trigger: false,
            slab: false,
            analysis_complete: false,
            trigger_decay: true,
        }
    }
}
impl Document {
    pub fn marker_beat(&self, i: usize) -> f64 {
        if self.marker_beats.len() == self.markers.len() {
            self.marker_beats.get(i).copied().unwrap_or(i as f64 * 4.)
        } else {
            i as f64 * 4.
        }
    }
    pub fn set_anchor(&mut self, time: f64, beat: f64) {
        if !time.is_finite() || !beat.is_finite() || time < 0. {
            return;
        }
        if self.marker_beats.len() != self.markers.len() {
            self.marker_beats = (0..self.markers.len()).map(|i| i as f64 * 4.).collect();
        }
        if let Some(i) = self
            .marker_beats
            .iter()
            .position(|b| (*b - beat).abs() < 0.001)
        {
            let lo = if i == 0 {
                0.
            } else {
                self.markers[i - 1] + 0.01
            };
            let hi = self.markers.get(i + 1).map_or(f64::MAX, |t| t - 0.01);
            self.markers[i] = time.clamp(lo, hi.max(lo));
        } else if self.markers.len() < 128 {
            let i = self.marker_beats.partition_point(|b| *b < beat);
            if i > 0 && time <= self.markers[i - 1] + 0.01 {
                return;
            }
            if self.markers.get(i).is_some_and(|t| time >= *t - 0.01) {
                return;
            }
            self.markers.insert(i, time);
            self.marker_beats.insert(i, beat);
        }
    }
    pub fn slip_grid(&mut self, seconds: f64) {
        let earliest = self.markers.first().copied().unwrap_or(self.grid_start);
        let shift = seconds.max(-earliest);
        self.grid_start += shift;
        for t in &mut self.markers {
            *t += shift;
        }
    }
    pub fn scale_grid(&mut self, ratio: f64) {
        let origin = self.markers.first().copied().unwrap_or(self.grid_start);
        for t in &mut self.markers {
            *t = origin + (*t - origin) * ratio;
        }
    }
    /// Consecutive bar anchors define four beats each; continue the edge tempo outside the map.
    pub fn beat_at(&self, time: f64, bpm: f64) -> f64 {
        if self.markers.len() < 2 {
            return self.marker_beats.first().copied().unwrap_or(0.)
                + (time - self.markers.first().copied().unwrap_or(self.grid_start)) * bpm / 60.;
        }
        let i = self
            .markers
            .partition_point(|x| *x <= time)
            .saturating_sub(1)
            .min(self.markers.len() - 2);
        self.marker_beat(i)
            + (time - self.markers[i]) * (self.marker_beat(i + 1) - self.marker_beat(i))
                / (self.markers[i + 1] - self.markers[i])
    }
    pub fn time_at(&self, beat: f64, bpm: f64) -> f64 {
        if self.markers.len() < 2 {
            return self.markers.first().copied().unwrap_or(self.grid_start)
                + (beat - self.marker_beats.first().copied().unwrap_or(0.)) * 60. / bpm;
        }
        let i = if self.marker_beats.len() == self.markers.len() {
            self.marker_beats
                .partition_point(|b| *b <= beat)
                .saturating_sub(1)
        } else {
            (beat / 4.).floor().max(0.) as usize
        };
        let i = i.min(self.markers.len() - 2);
        self.markers[i]
            + (beat - self.marker_beat(i)) * (self.markers[i + 1] - self.markers[i])
                / (self.marker_beat(i + 1) - self.marker_beat(i))
    }
    pub fn bpm_at(&self, time: f64, fallback: f64) -> f64 {
        if self.markers.len() < 2 {
            return fallback;
        }
        let i = self
            .markers
            .partition_point(|x| *x <= time)
            .saturating_sub(1)
            .min(self.markers.len() - 2);
        60. * (self.marker_beat(i + 1) - self.marker_beat(i))
            / (self.markers[i + 1] - self.markers[i])
    }

    pub fn normalize(&mut self) {
        self.cues.resize(32, Cue::default());
        self.cues.truncate(32);
        self.selected = self.selected.min(31);
        for c in &mut self.cues {
            if !c.start.is_finite() || !c.end.is_finite() {
                *c = Cue::default();
            }
            c.start = c.start.max(0.);
            c.end = c.end.max(c.start);
            c.name = c.name.chars().take(64).collect();
        }
        let mut anchors: Vec<_> = self
            .markers
            .iter()
            .enumerate()
            .map(|(i, t)| (*t, self.marker_beat(i)))
            .filter(|(t, b)| t.is_finite() && *t >= 0. && b.is_finite())
            .collect();
        anchors.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.markers.clear();
        self.marker_beats.clear();
        for (t, b) in anchors {
            if self.markers.len() == 128 {
                break;
            }
            if self.markers.last().is_some_and(|last| t - last < 0.01)
                || self
                    .marker_beats
                    .last()
                    .is_some_and(|last| b - last < 0.001)
            {
                continue;
            }
            self.markers.push(t);
            self.marker_beats.push(b);
        }
        if !self.grid_start.is_finite() {
            self.grid_start = 0.;
        }
    }
    pub fn autoset(
        &mut self,
        mode: usize,
        length: f64,
        bpm: f64,
        cursor: f64,
        beats: f64,
        onsets: &[f64],
        seed: u64,
    ) {
        let mut rng = seed.max(1);
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / ((1u64 << 53) as f64)
        };
        let step = 60. / bpm.clamp(20., 400.) * beats.clamp(0.0625, 16.);
        let selected = self.cues[self.selected].clone();
        for i in 0..32 {
            if self.cues[i].favorite {
                continue;
            }
            let start = match mode {
                0 => onsets
                    .get(((i as f64 + next()) * onsets.len() as f64 / 32.) as usize)
                    .copied()
                    .unwrap_or(i as f64 * length / 32.),
                1 => self.time_at(
                    self.beat_at(cursor, bpm) + i as f64 * beats.clamp(0.0625, 16.),
                    bpm,
                ),
                2 => next() * length,
                _ => selected.start,
            };
            let end = if mode == 3 {
                selected.end
            } else if mode == 1 {
                self.time_at(self.beat_at(start, bpm) + beats.clamp(0.0625, 16.), bpm)
                    .min(length)
            } else {
                (start + step).min(length)
            };
            self.cues[i] = Cue {
                start: start.min(length),
                end,
                enabled: start < length && end > start,
                name: format!("Chop {:02}", i + 1),
                favorite: false,
                color: if mode == 3 { selected.color } else { None },
            };
        }
    }
}
#[cfg(test)]
mod grid_tests {
    use super::*;
    #[test]
    fn numbered_anchors_roundtrip_slip_and_legacy_migration() {
        let mut d = Document::default();
        d.set_anchor(1., 0.);
        d.set_anchor(5., 8.);
        d.set_anchor(6.5, 10.);
        d.normalize();
        for t in [0., 1., 3., 5., 6., 8.] {
            assert!((d.time_at(d.beat_at(t, 120.), 120.) - t).abs() < 1e-9);
        }
        assert_eq!(d.bpm_at(2., 120.), 120.);
        assert_eq!(d.bpm_at(6., 120.), 80.);
        d.slip_grid(-0.25);
        assert_eq!(d.time_at(8., 120.), 4.75);
        d.scale_grid(0.5);
        assert_eq!(d.time_at(8., 120.), 2.75);
        let encoded = serde_json::to_string(&d).unwrap();
        let mut restored: Document = serde_json::from_str(&encoded).unwrap();
        restored.normalize();
        assert_eq!(restored, d);
        let mut old: Document = serde_json::from_str(r#"{"markers":[0,2,5]}"#).unwrap();
        old.normalize();
        assert_eq!(old.marker_beats, vec![0., 4., 8.]);
        assert!(!old.trigger_decay);
        assert_eq!(old.time_at(6., 120.), 3.5);
        let mut invalid = Document {
            markers: vec![0., 1., 2.],
            marker_beats: vec![0., f64::NAN, -1.],
            ..Default::default()
        };
        invalid.normalize();
        assert_eq!(invalid.markers, vec![0.]);
    }
}
pub struct Def {
    pub id: String,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: f32,
}
pub fn defs() -> Vec<Def> {
    let globals = [
        ("source_bpm", "Source BPM", 20., 400., 120., 0.01),
        ("tempo", "Tempo", 20., 400., 120., 0.01),
        ("sync", "Host sync", 0., 1., 1., 1.),
        ("key", "Key shift", -24., 24., 0., 1.),
        ("detune", "Detune cents", -100., 100., 0., 1.),
        ("hold", "Hold mode", 0., 1., 1., 1.),
        ("poly", "Polyphonic", 0., 1., 0., 1.),
        ("velocity", "Velocity", 0., 1., 1., 1.),
        ("keyboard", "Keyboard mode", 0., 1., 0., 1.),
        ("link", "Link pitch and speed", 0., 1., 0., 1.),
        ("master", "Output dB", -60., 12., 0., 0.1),
        ("glide", "Glide seconds", 0., 5., 0., 0.01),
        ("vocals", "Vocals", 0., 1., 1., 0.01),
        ("drums", "Drums", 0., 1., 1., 0.01),
        ("bass", "Bass", 0., 1., 1., 0.01),
        ("other", "Instruments", 0., 1., 1., 0.01),
    ];
    let mut out: Vec<_> = globals
        .into_iter()
        .map(|(id, n, min, max, d, step)| Def {
            id: id.into(),
            name: n.into(),
            min,
            max,
            default: d,
            step,
        })
        .collect();
    for i in 0..32 {
        for (id, n, min, max, d, step) in [
            ("level", "Level dB", -12., 12., 0., 0.1),
            ("filter", "Filter LP/HP", -1., 1., 0., 0.01),
            ("attack", "Attack seconds", 0., 10., 0., 0.001),
            ("release", "Release seconds", 0., 10., 0., 0.001),
            ("reverse", "Reverse", 0., 1., 0., 1.),
            ("pitch", "Semitones", -24., 24., 0., 1.),
            ("speed", "Speed", 0.25, 4., 1., 0.01),
            ("output", "Output", 0., 15., 0., 1.),
        ] {
            out.push(Def {
                id: format!("pad{}_{}", i + 1, id),
                name: format!("Pad {} {n}", i + 1),
                min,
                max,
                default: d,
                step,
            });
        }
    }
    out
}
pub struct ChopParams {
    pub p: Vec<FloatParam>,
    pub defs: Vec<Def>,
    pub doc: RwLock<Arc<Document>>,
    pub live: Arc<LiveEdits>,
    pending_cues: [(AtomicU64, AtomicU64); 32],
}
impl Default for ChopParams {
    fn default() -> Self {
        let defs = defs();
        let p = defs
            .iter()
            .map(|d| {
                FloatParam::new(
                    &d.name,
                    d.default,
                    FloatRange::Linear {
                        min: d.min,
                        max: d.max,
                    },
                )
                .with_step_size(d.step)
            })
            .collect();
        Self {
            p,
            defs,
            doc: RwLock::new(Arc::new(Document::default())),
            live: Arc::default(),
            pending_cues: std::array::from_fn(|_| (AtomicU64::new(u64::MAX), AtomicU64::new(0))),
        }
    }
}
impl ChopParams {
    pub fn pending_capture(&self, n: usize) -> bool {
        f64::from_bits(self.pending_cues[n].0.load(Ordering::Acquire)).is_finite()
    }
    pub fn clear_capture(&self, n: usize) {
        self.pending_cues[n].0.store(u64::MAX, Ordering::Release);
    }
    pub fn capture_cue(&self, n: usize, start: f64, end: f64, source: &str) {
        if let Ok(d) = self.doc.try_read() {
            if d.path == source && !d.cues[n].enabled {
                self.pending_cues[n]
                    .1
                    .store(end.to_bits(), Ordering::Relaxed);
                self.pending_cues[n]
                    .0
                    .store(start.to_bits(), Ordering::Release);
            }
        }
    }
    pub fn clear_captures(&self) {
        for (start, _) in &self.pending_cues {
            start.store(u64::MAX, Ordering::Release);
        }
    }
    pub fn v(&self, i: usize) -> f32 {
        self.live.value(&self.p[i])
    }
    pub fn document(&self) -> Arc<Document> {
        self.doc.read().unwrap().clone()
    }
    pub fn set_doc(&self, mut d: Document) {
        d.normalize();
        let mut current = self.doc.write().unwrap();
        if current.path != d.path {
            self.clear_captures();
        }
        for (n, c) in d.cues.iter().enumerate() {
            if c.enabled || current.cues[n].enabled {
                self.pending_cues[n].0.store(u64::MAX, Ordering::Release);
            }
        }
        *current = Arc::new(d);
    }
}
unsafe impl Params for ChopParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.p
            .iter()
            .zip(&self.defs)
            .map(|(p, d)| (d.id.clone(), p.as_ptr(), String::new()))
            .collect()
    }
    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let mut doc = (*self.document()).clone();
        for (n, (start, end)) in self.pending_cues.iter().enumerate() {
            let start = f64::from_bits(start.load(Ordering::Acquire));
            let end = f64::from_bits(end.load(Ordering::Relaxed));
            if !doc.cues[n].enabled && start.is_finite() && end > start {
                doc.cues[n] = Cue {
                    start,
                    end,
                    enabled: true,
                    name: format!("Chop {:02}", n + 1),
                    ..Default::default()
                };
                doc.analysis_complete = true;
            }
        }
        BTreeMap::from([
            ("chop-document".into(), serde_json::to_string(&doc).unwrap()),
            (
                "sample-path".into(),
                serde_json::to_string(&self.document().path).unwrap(),
            ),
        ])
    }
    fn deserialize_fields(&self, s: &BTreeMap<String, String>) {
        self.clear_captures();
        if let Some(mut d) = s
            .get("chop-document")
            .and_then(|s| serde_json::from_str::<Document>(s).ok())
        {
            if let Some(path) = s
                .get("sample-path")
                .and_then(|s| serde_json::from_str::<String>(s).ok())
                .filter(|s| !s.is_empty())
            {
                if path != d.path {
                    d.path = path;
                    d.stems = Default::default();
                }
            }
            self.set_doc(d);
        }
    }
}
