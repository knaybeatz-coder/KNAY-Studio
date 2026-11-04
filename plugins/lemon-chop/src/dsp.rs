// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{audio::Audio, params::*};
use std::sync::Arc;
const BLOCK: usize = 64;
pub struct Voice {
    pub active: bool,
    pub note: u8,
    pub channel: u8,
    pub pad: usize,
    pub output: usize,
    pub preview: bool,
    audio: Option<Arc<Audio>>,
    start: f64,
    end: f64,
    pos: f64,
    heard: f64,
    speed: f64,
    pitch: f64,
    reverse: bool,
    rate: f64,
    velocity: f32,
    age: usize,
    released: Option<usize>,
    attack: f64,
    release: f64,
    hold: bool,
    trigger_decay: bool,
    release_enabled: bool,
    level: f32,
    filter: f32,
    low: [f32; 2],
    stream: os_dsp::stretch::Stream,
    input: Vec<f32>,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    read: usize,
    frac: f64,
    direct: bool,
    glide_target: f64,
    glide_left: usize,
    grid: Vec<f64>,
    grid_beats: Vec<f64>,
    tempo_ratio: f64,
    source_bpm: f64,
    linked: bool,
    link_pitch: f64,
}
impl Voice {
    pub fn new(rate: u32) -> Self {
        let mut stream = os_dsp::stretch::Stream::new(rate);
        let mut l = [0.; BLOCK];
        let mut r = [0.; BLOCK];
        stream.process(&[0.; BLOCK * 2], &mut l, &mut r);
        let cap = (stream.lead() * 2).max(BLOCK * 2 * 32);
        Self {
            active: false,
            note: 0,
            channel: 0,
            pad: 0,
            output: 0,
            preview: false,
            audio: None,
            start: 0.,
            end: 0.,
            pos: 0.,
            heard: 0.,
            speed: 1.,
            pitch: 1.,
            reverse: false,
            rate: rate as f64,
            velocity: 1.,
            age: 0,
            released: None,
            attack: 0.,
            release: 0.,
            hold: true,
            trigger_decay: false,
            release_enabled: false,
            level: 1.,
            filter: 0.,
            low: [0.; 2],
            stream,
            input: Vec::with_capacity(cap),
            left: l,
            right: r,
            read: BLOCK,
            frac: 0.,
            direct: true,
            glide_target: 1.,
            glide_left: 0,
            grid: Vec::with_capacity(128),
            grid_beats: Vec::with_capacity(128),
            tempo_ratio: 120.,
            source_bpm: 120.,
            linked: false,
            link_pitch: 1.,
        }
    }
    pub fn start(
        &mut self,
        audio: Arc<Audio>,
        doc: &Document,
        values: &[f32],
        n: usize,
        note: u8,
        channel: u8,
        vel: f32,
        tempo: f64,
        preview: Option<f64>,
    ) {
        let source_cue = Cue {
            start: preview.unwrap_or(0.),
            end: audio.duration(),
            enabled: true,
            ..Default::default()
        };
        let cue = if preview.is_some() {
            &source_cue
        } else {
            &doc.cues[n]
        };
        self.active = false;
        if !cue.enabled || cue.end <= cue.start {
            return;
        }
        let rate = audio.source.rate as f64;
        self.start = cue.start * rate;
        self.end = (cue.end * rate).min(audio.source.frames() as f64);
        if self.start >= self.end {
            return;
        }
        self.pad = n;
        self.preview = preview.is_some() && note == 127;
        self.note = note;
        self.channel = channel;
        self.audio = Some(audio);
        self.age = 0;
        self.released = None;
        self.low = [0.; 2];
        self.frac = 0.;
        self.read = BLOCK;
        self.reverse = values[pad(n, REVERSE)] > 0.5;
        self.pos = if self.reverse {
            self.end - 1.
        } else {
            self.start
        };
        self.heard = self.pos;
        let semis = values[KEY]
            + values[DETUNE] / 100.
            + values[pad(n, PITCH)]
            + if values[KEYBOARD] > 0.5 {
                note as f32 - 60.
            } else {
                0.
            };
        self.pitch = 2f64.powf(semis as f64 / 12.);
        self.grid.clear();
        self.grid.extend_from_slice(&doc.markers);
        self.grid_beats.clear();
        self.grid_beats
            .extend((0..doc.markers.len()).map(|i| doc.marker_beat(i)));
        self.source_bpm = values[SOURCE_BPM] as f64;
        let bpm = if values[SYNC] > 0.5 {
            tempo
        } else {
            values[TARGET_BPM] as f64
        };
        self.tempo_ratio = bpm * values[pad(n, SPEED)] as f64;
        self.speed = (self.tempo_ratio / doc.bpm_at(cue.start, self.source_bpm)).clamp(0.025, 32.);
        self.linked = values[LINK] > 0.5;
        self.link_pitch = self.pitch;
        if self.linked {
            self.speed = (self.speed * self.pitch).clamp(0.025, 32.);
            self.pitch = self.speed;
        }
        self.velocity = if values[VELOCITY] > 0.5 { vel } else { 1. };
        self.hold = values[HOLD] > 0.5;
        self.trigger_decay = doc.trigger_decay && !self.preview;
        self.release_enabled = values[pad(n, RELEASE)] > 0.;
        self.attack = values[pad(n, ATTACK)] as f64 * self.rate;
        self.release = (values[pad(n, RELEASE)] as f64 * self.rate).max(32.);
        self.level = 10f32.powf(values[pad(n, LEVEL)] / 20.);
        self.filter = values[pad(n, FILTER)];
        self.output = values[pad(n, OUTPUT)] as usize;
        self.glide_left = 0;
        self.glide_target = self.pitch;
        self.direct =
            self.grid.len() < 2 && (self.speed - 1.).abs() < 1e-7 && (self.pitch - 1.).abs() < 1e-7;
        self.active = true;
        if !self.direct {
            let gains = std::array::from_fn(|i| values[12 + i]);
            self.feed(self.stream.lead(), gains);
            self.stream.set_transpose(self.pitch);
            self.stream.start(&self.input, self.speed);
            let mut delay = self.stream.delay();
            while delay > 0 {
                let count = delay.min(BLOCK);
                self.chunk(count, gains);
                delay -= count;
            }
            self.read = BLOCK;
        }
    }
    fn feed(&mut self, n: usize, gains: [f32; 4]) {
        self.input.clear();
        let Some(audio) = &self.audio else { return };
        let step = audio.source.rate as f64 / self.rate * if self.reverse { -1. } else { 1. };
        for _ in 0..n {
            let v = if self.pos >= self.start && self.pos < self.end {
                audio.frame(self.pos, gains)
            } else {
                [0.; 2]
            };
            self.input.extend_from_slice(&v);
            self.pos += step;
        }
    }
    pub fn glide_to(&mut self, note: u8, seconds: f32, gains: [f32; 4]) {
        let previous_target = if self.glide_left > 0 {
            self.glide_target
        } else {
            self.pitch
        };
        self.glide_target = previous_target * 2f64.powf((note as f64 - self.note as f64) / 12.);
        self.note = note;
        self.released = None;
        self.glide_left = (seconds as f64 * self.rate) as usize;
        if self.direct {
            self.direct = false;
            self.feed(self.stream.lead(), gains);
            self.stream.set_transpose(self.pitch);
            self.stream.start(&self.input, self.speed);
            let mut left = self.stream.delay();
            while left > 0 {
                let n = left.min(BLOCK);
                self.chunk(n, gains);
                left -= n;
            }
            self.read = BLOCK;
        }
    }
    fn chunk(&mut self, count: usize, gains: [f32; 4]) {
        if self.glide_left > 0 {
            let n = count.min(self.glide_left);
            self.pitch += (self.glide_target - self.pitch) * n as f64 / self.glide_left as f64;
            self.glide_left -= n;
            self.stream.set_transpose(self.pitch);
        }
        if self.grid.len() > 1 {
            let seconds = self.heard / self.audio.as_ref().unwrap().source.rate as f64;
            let i = self
                .grid
                .partition_point(|t| *t <= seconds)
                .saturating_sub(1)
                .min(self.grid.len() - 2);
            let bpm = 60. * (self.grid_beats[i + 1] - self.grid_beats[i])
                / (self.grid[i + 1] - self.grid[i]);
            self.speed = (self.tempo_ratio / bpm).clamp(0.025, 32.);
            if self.linked {
                self.speed = (self.speed * self.link_pitch).clamp(0.025, 32.);
                self.pitch = self.speed;
                self.stream.set_transpose(self.pitch);
            }
        }
        let desired = count as f64 * self.speed + self.frac;
        let n = desired.floor() as usize;
        self.frac = desired - n as f64;
        self.feed(n, gains);
        self.stream.process(
            &self.input,
            &mut self.left[..count],
            &mut self.right[..count],
        );
        self.read = 0;
    }
    pub fn update(&mut self, values: &[f32], tempo: f64) {
        if !self.active || self.preview {
            return;
        }
        let n = self.pad;
        self.level = 10f32.powf(values[pad(n, LEVEL)] / 20.);
        self.filter = values[pad(n, FILTER)];
        self.attack = values[pad(n, ATTACK)] as f64 * self.rate;
        self.release = (values[pad(n, RELEASE)] as f64 * self.rate).max(32.);
        self.hold = values[HOLD] > 0.5;
        self.release_enabled = values[pad(n, RELEASE)] > 0.;
        self.output = values[pad(n, OUTPUT)] as usize;
        let semis = values[KEY]
            + values[DETUNE] / 100.
            + values[pad(n, PITCH)]
            + if values[KEYBOARD] > 0.5 {
                self.note as f32 - 60.
            } else {
                0.
            };
        let mut pitch = 2f64.powf(semis as f64 / 12.);
        let bpm = if values[SYNC] > 0.5 {
            tempo
        } else {
            values[TARGET_BPM] as f64
        };
        self.source_bpm = values[SOURCE_BPM].max(20.) as f64;
        self.tempo_ratio = bpm * values[pad(n, SPEED)] as f64;
        let mut speed = (self.tempo_ratio / self.source_bpm).clamp(0.025, 32.);
        self.linked = values[LINK] > 0.5;
        self.link_pitch = pitch;
        if self.linked {
            speed = (speed * pitch).clamp(0.025, 32.);
            pitch = speed;
        }
        if self.grid.len() < 2 {
            self.speed = speed;
        }
        if self.glide_left == 0 {
            self.pitch = pitch;
            self.stream.set_transpose(pitch);
        }
        let reverse = values[pad(n, REVERSE)] > 0.5;
        if reverse != self.reverse {
            self.reverse = reverse;
            self.pos = self.heard;
        }
        if self.direct && ((speed - 1.).abs() > 1e-7 || (pitch - 1.).abs() > 1e-7) {
            self.direct = false;
            let gains = std::array::from_fn(|i| values[12 + i]);
            self.feed(self.stream.lead(), gains);
            self.stream.start(&self.input, self.speed);
            let mut delay = self.stream.delay();
            while delay > 0 {
                let count = delay.min(BLOCK);
                self.chunk(count, gains);
                delay -= count;
            }
            self.read = BLOCK;
        }
    }
    pub fn off(&mut self, note: u8, channel: u8) {
        if self.active
            && self.note == note
            && self.channel == channel
            && self.hold
            && self.released.is_none()
        {
            self.released = Some(self.age);
        }
    }
    pub fn stop(&mut self) {
        self.active = false;
    }
    pub fn tick(&mut self, gains: [f32; 4]) -> [f32; 2] {
        if !self.active {
            return [0.; 2];
        }
        let source_rate = self.audio.as_ref().unwrap().source.rate as f64;
        let remaining = if self.reverse {
            self.heard - self.start
        } else {
            self.end - self.heard
        };
        if remaining <= 0. {
            self.active = false;
            return [0.; 2];
        }
        let mut v = if self.direct {
            let v = self.audio.as_ref().unwrap().frame(self.pos, gains);
            self.pos += source_rate / self.rate * if self.reverse { -1. } else { 1. };
            v
        } else {
            if self.read >= BLOCK {
                self.chunk(BLOCK, gains);
            }
            let v = [self.left[self.read], self.right[self.read]];
            self.read += 1;
            v
        };
        let mut envelope = if self.attack > 0. {
            (self.age as f64 / self.attack).min(1.)
        } else {
            1.
        };
        let release_at = self.released.or_else(|| {
            (!self.hold
                && self.trigger_decay
                && self.release_enabled
                && self.age as f64 >= self.attack)
                .then_some(self.attack as usize)
        });
        if let Some(at) = release_at {
            envelope *= 1. - (self.age - at) as f64 / self.release;
            if envelope <= 0. {
                self.active = false;
                return [0.; 2];
            }
        }
        envelope *= ((remaining * self.rate / source_rate / self.speed)
            / if self.hold || self.trigger_decay {
                32.
            } else {
                self.release
            })
        .min(1.);
        envelope *= (self.age as f64 / 16.).min(1.);
        if self.filter.abs() > 0.001 {
            let hz = if self.filter < 0. {
                17000. * (80f64 / 17000.).powf(-self.filter as f64)
            } else {
                80. * (17000f64 / 80.).powf(self.filter as f64)
            };
            let alpha =
                1. - (-std::f64::consts::TAU * hz.min(self.rate * 0.45) / self.rate).exp() as f32;
            for i in 0..2 {
                self.low[i] += alpha * (v[i] - self.low[i]);
                v[i] = if self.filter < 0. {
                    self.low[i]
                } else {
                    v[i] - self.low[i]
                };
            }
        }
        let gain = self.velocity * self.level * envelope.max(0.) as f32;
        self.age += 1;
        self.heard += source_rate / self.rate * self.speed * if self.reverse { -1. } else { 1. };
        [v[0] * gain, v[1] * gain]
    }
    pub fn position(&self) -> f64 {
        self.heard / self.audio.as_ref().map(|a| a.source.rate).unwrap_or(1) as f64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_grid_and_interrupted_glide_keep_correct_pitch() {
        let a = Arc::new(Audio {
            path: "synthetic".into(),
            source: Arc::new(os_engine::Sample {
                rate: 8000,
                left: vec![0.25; 32000],
                right: vec![0.25; 32000],
            }),
            stems: Default::default(),
            peaks: vec![],
            onsets: vec![],
            bpm: 120.,
            key: String::new(),
        });
        let mut d = Document::default();
        d.cues[0] = Cue {
            start: 0.,
            end: 4.,
            enabled: true,
            ..Default::default()
        };
        d.markers = vec![0., 2., 4.];
        let p = ChopParams::default();
        let mut values: Vec<_> = p.defs.iter().map(|d| d.default).collect();
        values[LINK] = 1.;
        values[pad(0, PITCH)] = 12.;
        let mut v = Voice::new(8000);
        v.start(a.clone(), &d, &values, 0, 60, 0, 1., 120., None);
        for _ in 0..800 {
            v.tick([1.; 4]);
        }
        assert!(
            (v.position() - 0.2).abs() < 0.001,
            "linked speed must survive grid chunk updates"
        );
        assert_eq!(v.pitch, 2.);
        values[LINK] = 0.;
        values[pad(0, PITCH)] = 0.;
        d.markers.clear();
        v.start(a, &d, &values, 0, 60, 0, 1., 120., None);
        v.glide_to(72, 1., [1.; 4]);
        for _ in 0..100 {
            v.tick([1.; 4]);
        }
        v.glide_to(67, 1., [1.; 4]);
        assert!((v.glide_target - 2f64.powf(7. / 12.)).abs() < 1e-9);
    }
    #[test]
    fn cue_bounds_hold_reverse_velocity_and_stretch() {
        let dir = std::env::temp_dir().join(format!("lemon-chop-audio-{}.wav", std::process::id()));
        let rate = 8000;
        let mut bytes = Vec::new();
        os_engine::wav::write_f32_stereo_header(&mut bytes, rate, 16000).unwrap();
        for i in 0..16000 {
            let v = if i < 8000 {
                (std::f32::consts::TAU * 220. * i as f32 / rate as f32).sin() * 0.5
            } else {
                0.
            };
            bytes.extend_from_slice(&v.to_le_bytes());
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(&dir, bytes).unwrap();
        let audio = Arc::new(Audio::load(dir.to_str().unwrap(), &Default::default()).unwrap());
        let mut d = Document::default();
        d.cues[0] = Cue {
            start: 0.,
            end: 1.,
            enabled: true,
            ..Default::default()
        };
        let p = ChopParams::default();
        let mut values: Vec<f32> = p.defs.iter().map(|d| d.default).collect();
        let mut v = Voice::new(rate);
        v.start(audio.clone(), &d, &values, 0, 36, 0, 1., 120., None);
        let sound: Vec<_> = (0..8100).map(|_| v.tick([1.; 4])[0]).collect();
        assert!(sound[100..7000].iter().any(|v| v.abs() > 0.4));
        assert!(sound[8000..].iter().all(|v| *v == 0.));
        v.start(audio.clone(), &d, &values, 0, 36, 0, 0.5, 120., None);
        for _ in 0..100 {
            v.tick([1.; 4]);
        }
        v.off(36, 0);
        for _ in 0..200 {
            v.tick([1.; 4]);
        }
        assert!(!v.active);
        values[pad(0, REVERSE)] = 1.;
        values[pad(0, SPEED)] = 0.5;
        values[pad(0, PITCH)] = 12.;
        v.start(audio, &d, &values, 0, 36, 0, 1., 120., None);
        let x: Vec<_> = (0..16100).map(|_| v.tick([1.; 4])[0]).collect();
        assert!(x.iter().all(|x| x.is_finite()));
        assert!(x[1000..14000].iter().any(|x| x.abs() > 0.1));
        assert!(!v.active);
        let crossings = x[4000..12000]
            .windows(2)
            .filter(|w| w[0] < 0. && w[1] >= 0.)
            .count();
        assert!(
            (crossings as i32 - 440).abs() < 12,
            "pitch preserved independently: {crossings}"
        );
        let _ = std::fs::remove_file(dir);
    }
}
