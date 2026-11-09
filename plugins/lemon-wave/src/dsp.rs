// SPDX-License-Identifier: GPL-3.0-or-later
//! Original wavetable engine; analytic spectra, not recovered vendor tables.
use crate::controls::*;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, OnceLock};
const SIZE: usize = 1024;
const FRAMES: usize = 5;
const MIPS: usize = 9;
pub const VOICES: usize = 24;
pub type Patch = [f32; COUNT];
pub fn defaults() -> Patch {
    std::array::from_fn(|i| SETTINGS[i].start as f32)
}

/// Relative center/outer amplitudes used by both the visualizer and classic unison.
/// At the existing 75% default, classic voice gains remain exactly unchanged.
pub fn blend_weight(count: usize, voice: usize, blend: f32) -> f32 {
    if count <= 1 {
        return 1.;
    }
    if count == 2 {
        return 0.707946;
    }
    let b = blend.clamp(0., 1.).powi(2);
    let center = 0.99785 - 0.55366 * b;
    let outer = (-0.73764 * b + 1.2841) * b + 0.044372;
    if count % 2 == 1 && voice == count / 2 {
        center * 0.95794576
    } else if count % 2 == 0 && (voice == count / 2 || voice + 1 == count / 2) {
        center
    } else {
        outer
    }
}
fn harmonic(bank: usize, frame: usize, n: usize) -> f32 {
    let t = frame as f32 / 4.;
    let h = n as f32;
    match bank {
        0 => {
            let sine = if n == 1 { 1. } else { 0. };
            let saw = 1. / h;
            let square = if n % 2 == 1 { 1. / h } else { 0. };
            if t <= 0.5 {
                sine + (saw - sine) * t * 2.
            } else {
                saw + (square - saw) * (t - 0.5) * 2.
            }
        }
        1 => {
            if n % 2 == 1 {
                ((h * (0.2 + t * 0.7)).cos() * 0.4 + 0.6) / h
            } else {
                0.
            }
        }
        2 => {
            let peak = 2. + t * 14.;
            (-(h - peak).powi(2) / 8.).exp() * 0.45 + if n == 1 { 0.5 } else { 0.02 / h }
        }
        _ => {
            let f = 2. + t * 9.;
            ((-(h - f).powi(2) / 3.).exp() + 0.5 * (-(h - f * 2.7).powi(2) / 8.).exp()) / h.sqrt()
                + if n == 1 { 0.3 } else { 0. }
        }
    }
}
pub fn preview(bank: usize, pos: f32, phase: f32) -> f32 {
    let f = pos.clamp(0., 1.) * 4.;
    let a = (f.floor() as usize).min(3);
    let w = f - a as f32;
    (1..=32)
        .map(|n| {
            (harmonic(bank, a, n) * (1. - w) + harmonic(bank, a + 1, n) * w)
                * (TAU * phase * n as f32).sin()
        })
        .sum::<f32>()
        * 0.5
}
struct Tables {
    data: Vec<f32>,
}
impl Tables {
    fn new() -> Self {
        let mut data = vec![0.; 4 * FRAMES * MIPS * SIZE];
        for b in 0..4 {
            for f in 0..FRAMES {
                let mut full = vec![0.; SIZE];
                for mip in 0..MIPS {
                    // Incrementally add the harmonic band for this level.
                    let first = if mip == 0 { 1 } else { (1 << (mip - 1)) + 1 };
                    for n in first..=(1 << mip) {
                        let amp = harmonic(b, f, n);
                        for (i, x) in full.iter_mut().enumerate() {
                            *x += amp * (TAU * n as f32 * i as f32 / SIZE as f32).sin();
                        }
                    }
                    let start = ((b * FRAMES + f) * MIPS + mip) * SIZE;
                    data[start..start + SIZE].copy_from_slice(&full);
                }
                // One gain for every mip, so pitch changes do not change normalization.
                let peak = full.iter().fold(1.0_f32, |a, x| a.max(x.abs()));
                let start = (b * FRAMES + f) * MIPS * SIZE;
                for x in &mut data[start..start + MIPS * SIZE] {
                    *x /= peak;
                }
            }
        }
        Self { data }
    }
    fn sample(&self, bank: usize, pos: f32, phase: f32, freq: f32, rate: f32) -> f32 {
        let safe = (rate * 0.45 / freq.max(1.)).max(1.);
        let mip = safe.log2().floor().clamp(0., (MIPS - 1) as f32) as usize;
        let blend = (safe / (1 << mip) as f32 - 1.).clamp(0., 1.);
        let frame = pos.clamp(0., 1.) * 4.;
        let f = (frame.floor() as usize).min(3);
        let w = frame - f as f32;
        let p = phase * SIZE as f32;
        let i = (p as usize) % SIZE;
        let t = p - p.floor();
        let read = |f, m| {
            let start = ((bank * FRAMES + f) * MIPS + m) * SIZE;
            self.data[start + i] * (1. - t) + self.data[start + (i + 1) % SIZE] * t
        };
        let read = |f| {
            if mip == 0 {
                read(f, 0)
            } else {
                read(f, mip - 1) * (1. - blend) + read(f, mip) * blend
            }
        };
        read(f) * (1. - w) + read(f + 1) * w
    }
}

pub fn editable_bank(bank: usize) -> crate::wavetable::Source {
    let t = Tables::new();
    let mut samples = Vec::with_capacity(FRAMES * SIZE);
    for f in 0..FRAMES {
        let start = ((bank.min(3) * FRAMES + f) * MIPS + MIPS - 1) * SIZE;
        samples.extend_from_slice(&t.data[start..start + SIZE]);
    }
    crate::wavetable::Source {
        name: ["Basic", "Hollow", "Glass", "Vowel"][bank.min(3)].into(),
        samples,
    }
}

#[derive(Clone, Copy, Default)]
struct Env {
    value: f32,
    stage: u8,
    release_step: f32,
    elapsed: f32,
    reference: bool,
    recovered: crate::reference_envelope::Envelope,
    settings: crate::reference_envelope::Settings,
}
impl Env {
    fn start(&mut self) {
        *self = Self {
            stage: 1,
            ..Self::default()
        };
    }
    fn off(&mut self, r: f32, rate: f32) {
        if self.reference {
            self.settings.release = r as f64;
            if self.stage != 4 {
                self.elapsed = 0.;
            }
            self.recovered.off(&self.settings, rate);
            self.stage = self.recovered.stage;
            return;
        }
        if self.stage != 0 && self.stage != 4 {
            self.stage = 4;
            self.elapsed = 0.;
            self.release_step = self.value / (r * rate).max(1.);
        }
    }
    fn tick(&mut self, a: f32, d: f32, s: f32, rate: f32) -> f32 {
        let before = self.stage;
        self.elapsed += 1. / rate;
        if self.reference {
            self.value = self.recovered.tick(&self.settings, rate);
            self.stage = self.recovered.stage;
            if before != self.stage {
                self.elapsed = 0.;
            }
            return self.value;
        }
        match self.stage {
            1 => {
                self.value += 1. / (a * rate).max(1.);
                if self.value >= 1. {
                    self.value = 1.;
                    self.stage = 2;
                }
            }
            2 => {
                self.value -= (1. - s) / (d * rate).max(1.);
                if self.value <= s {
                    self.value = s;
                    self.stage = 3;
                }
            }
            3 => self.value = s,
            4 => {
                self.value -= self.release_step;
                if self.value <= 0. {
                    self.value = 0.;
                    self.stage = 0;
                }
            }
            _ => {}
        }
        if before != self.stage {
            self.elapsed = 0.;
        }
        self.value
    }
}
#[derive(Clone, Copy, Default)]
struct Filter {
    ic1: f32,
    ic2: f32,
}
impl Filter {
    fn tick(&mut self, x: f32, g: f32, k: f32, mode: usize) -> f32 {
        let a = 1. / (1. + g * (g + k));
        let v1 = a * (self.ic1 + g * (x - self.ic2));
        let v2 = self.ic2 + g * v1;
        self.ic1 = 2. * v1 - self.ic1;
        self.ic2 = 2. * v2 - self.ic2;
        match mode {
            1 => v2,
            2 => v1,
            3 => x - k * v1 - v2,
            _ => x,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Voice {
    owner: u64,
    note: u8,
    channel: u8,
    id: Option<i32>,
    age: u64,
    held: bool,
    velocity: f32,
    tune: f32,
    phase: [[f32; 7]; 2],
    sample_cursor: [[f64; 7]; 2],
    sub: f32,
    noise_cursor:f64,
    dual_filter:[[[Filter;2];2];4],
    dual_recovered:[[[crate::reference_filter::Oversampled;2];2];4],
    dual_model:[usize;2],
    lfo: f32,
    lfo_extra: [f32; 3],
    amp: Env,
    env: Env,
    filter: [Filter; 2],
    recovered_filter: [crate::reference_filter::Oversampled; 2],
    filter_model: usize,
    lane_filter: [[Filter; 2]; 4],
    lane_recovered: [[crate::reference_filter::Oversampled; 2]; 4],
}
pub struct Engine {
    pub curves: crate::modulation::Curves,
    pub live: crate::modulation::Live,
    free_phase: [f32; 4],
    reference: Vec<crate::reference_voice::ReferenceVoice>,
    pub custom: Arc<crate::assets::Tables>,
    tables: Arc<Tables>,
    voices: [Voice; VOICES],
    pub rate: f32,
    current: Patch,
    target: Patch,
    age: u64,
    bend: [f32; 16],
    pedal: [bool; 16],
    noise: u32,
    delay: [Vec<f32>; 2],
    chorus: [Vec<f32>; 2],
    head: usize,
    ch_head: usize,
    ch_phase: f32,
    racks: [crate::effects::Rack; 2],
    modular: [crate::fxrack::Rack; 2],
    rack_blend: f32,
    pub tempo: f32,
    ended: Vec<(u8, u8, Option<i32>)>,
    tail: usize,
}
impl Engine {
    pub fn new(rate: f32) -> Self {
        static TABLES: OnceLock<Arc<Tables>> = OnceLock::new();
        let _=crate::sources::library();
        let reference_table = Arc::new(crate::reference_oscillator::Table::init_saw());
        Self {
            curves: Default::default(),
            live: Default::default(),
            free_phase: [0.; 4],
            custom: Arc::default(),
            reference: (0..VOICES)
                .map(|_| crate::reference_voice::ReferenceVoice::new(reference_table.clone(), rate))
                .collect(),
            tables: TABLES.get_or_init(|| Arc::new(Tables::new())).clone(),
            voices: [Voice::default(); VOICES],
            rate,
            current: defaults(),
            target: defaults(),
            age: 0,
            bend: [0.; 16],
            pedal: [false; 16],
            noise: 0x53e974ab,
            delay: std::array::from_fn(|_| vec![0.; (rate * 1.2) as usize + 2]),
            chorus: std::array::from_fn(|_| vec![0.; (rate * 0.06) as usize + 2]),
            head: 0,
            ch_head: 0,
            ch_phase: 0.,
            racks: std::array::from_fn(|_|crate::effects::Rack::new(rate)),
            modular: std::array::from_fn(|_|crate::fxrack::Rack::new(rate)),
            rack_blend: 0.,
            tempo: 120.,
            ended: Vec::with_capacity(256),
            tail: 0,
        }
    }
    pub fn reset(&mut self) {
        self.voices.fill(Voice::default());
        self.free_phase = [0.; 4];
        self.live = Default::default();
        for voice in &mut self.reference {
            voice.reset(
                [self.target[A_LEVEL] / 1000., self.target[B_LEVEL] / 1000.],
                [[0.; 7]; 2],
            );
        }
        self.bend.fill(0.);
        self.pedal.fill(false);
        self.delay.iter_mut().for_each(|x| x.fill(0.));
        self.chorus.iter_mut().for_each(|x| x.fill(0.));
        self.head = 0;
        self.ch_head = 0;
        self.ch_phase = 0.;
        for rack in &mut self.racks {rack.reset();}
        for rack in &mut self.modular {rack.reset();}
        self.rack_blend=if self.target[DUAL_FILTER]>0.5 || self.target[RACK_MODE]>0.5 || crate::effects::needs_lanes(&self.target) {1.} else {0.};
        self.current = self.target;
        self.tail = 0;
        self.ended.clear();
    }
    pub fn set(&mut self, p: Patch) {
        self.target = std::array::from_fn(|i| {
            if p[i].is_finite() {
                p[i].clamp(SETTINGS[i].range.0 as f32, SETTINGS[i].range.1 as f32)
            } else {
                SETTINGS[i].start as f32
            }
        });
    }
    pub fn on(&mut self, note: u8, velocity: f32, channel: u8, id: Option<i32>) {
        self.start_voice(note, velocity, channel, id, 0);
    }
    pub(crate) fn start_voice(&mut self, note:u8, velocity:f32, channel:u8, id:Option<i32>, owner:u64) {
        if channel >= 16 || note > 127 {
            return;
        }
        if velocity <= 0. {
            self.off(note, channel, id);
            return;
        }
        self.age += 1;
        let index = self
            .voices
            .iter()
            .position(|v| v.amp.stage == 0)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| (v.held, v.age))
                    .unwrap()
                    .0
            });
        let old = self.voices[index];
        if old.owner == 0 && old.amp.stage != 0 && self.ended.len() < 256 {
            self.ended.push((old.note, old.channel, old.id));
        }
        let mut v = Voice {
            owner,
            note,
            channel,
            id,
            age: self.age,
            held: true,
            velocity: velocity.clamp(0., 1.),
            ..Voice::default()
        };
        for o in 0..2 {
            for u in 0..7 {
                v.phase[o][u] = ((u * 37 + o * 13) as f32 / 101.).fract();
            }
        }
        let noise_length=if self.target[NOISE_TYPE]>7.5 {self.custom.noise.as_ref().map(|p|p.source.samples.len()).unwrap_or(44100)}else{44100};
        v.noise_cursor=if self.target[NOISE_RANDOM]>0.5 && self.target[NOISE_ONESHOT]<0.5 {(self.age.wrapping_mul(7919)%noise_length as u64) as f64}else{0.};
        v.amp.start();
        v.env.start();
        if self.target[ENV_ENGINE] > 0.5 {
            for (envelope, base, hold) in [
                (&mut v.amp, ATTACK, AMP_HOLD),
                (&mut v.env, ENV_ATTACK, ENV_HOLD),
            ] {
                envelope.reference = true;
                envelope.settings = envelope_settings(&self.target, base, hold);
                envelope
                    .recovered
                    .start(&envelope.settings, self.rate, false);
            }
        }
        let mut phases = [[0.; 7]; 2];
        // Separate deterministic per-note generator: squared amount law is measured,
        // but this sequence is our own and is not claimed to be Serum's RNG.
        let mut random = (self.age as u32)
            .wrapping_mul(747796405)
            .wrapping_add(2891336453);
        for o in 0..2 {
            for u in 0..7 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                let base = if o == 0 { A_PHASE } else { B_PHASE };
                phases[o][u] = (self.target[base] as f64 / 360.
                    + (self.target[base + 1] as f64 / 1000.).powi(2) * random as f64
                        / u32::MAX as f64)
                    .fract();
                // Keep legacy Classic phase behavior when both added settings are zero.
                if self.target[base] != 0. || self.target[base + 1] != 0. {
                    v.phase[o][u] = phases[o][u] as f32;
                }
            }
        }
        self.reference[index].reset(
            [self.target[A_LEVEL] / 1000., self.target[B_LEVEL] / 1000.],
            phases,
        );
        self.voices[index] = v;
    }
    pub fn off(&mut self, note: u8, channel: u8, id: Option<i32>) {
        for v in &mut self.voices {
            if v.owner == 0 && v.note == note && v.channel == channel && id.is_none_or(|id| v.id == Some(id)) {
                v.held = false;
                if !self.pedal[channel.min(15) as usize] {
                    v.amp.off(seconds(self.target[RELEASE]), self.rate);
                    v.env.off(seconds(self.target[ENV_RELEASE]), self.rate);
                }
            }
        }
    }
    pub(crate) fn release_owner(&mut self, owner:u64, hard:bool) {
        debug_assert!(owner != 0);
        for voice in &mut self.voices {
            if voice.owner != owner { continue; }
            if hard { *voice=Voice::default(); }
            else if voice.held {
                voice.held=false;
                voice.amp.off(seconds(self.target[RELEASE]),self.rate);
                voice.env.off(seconds(self.target[ENV_RELEASE]),self.rate);
            }
        }
    }
    pub(crate) fn tune_owner(&mut self, owner:u64, value:f32) {
        for voice in &mut self.voices {if voice.owner==owner {voice.tune=value.clamp(-96.,96.);}}
    }
    pub(crate) fn owner_alive(&self,owner:u64)->bool {self.voices.iter().any(|v|v.owner==owner && v.amp.stage!=0)}
    pub(crate) fn lit_keys(&self)->[u64;2] {
        let mut keys=[0;2];
        for v in &self.voices {if v.held && v.amp.stage!=0 {keys[v.note as usize/64]|=1u64<<(v.note%64);}}
        keys
    }
    pub fn sustain(&mut self, channel: u8, on: bool) {
        if channel >= 16 {
            return;
        }
        self.pedal[channel as usize] = on;
        if !on {
            for v in &mut self.voices {
                if v.channel == channel && !v.held {
                    v.amp.off(seconds(self.target[RELEASE]), self.rate);
                    v.env.off(seconds(self.target[ENV_RELEASE]), self.rate);
                }
            }
        }
    }
    pub fn bend(&mut self, channel: u8, value: f32) {
        if channel < 16 && value.is_finite() {
            self.bend[channel as usize] = (value.clamp(0., 1.) * 2. - 1.) * 2.;
        }
    }
    pub fn tune(&mut self, note: u8, channel: u8, id: Option<i32>, value: f32) {
        if value.is_finite() {
            for v in &mut self.voices {
                if v.owner == 0 && v.note == note && v.channel == channel && id.is_none_or(|i| v.id == Some(i)) {
                    v.tune = value.clamp(-96., 96.);
                }
            }
        }
    }
    pub fn choke(&mut self, note: u8, channel: u8, id: Option<i32>) {
        for v in &mut self.voices {
            if v.owner == 0 && v.note == note && v.channel == channel && id.is_none_or(|i| v.id == Some(i)) {
                if self.ended.len() < 256 {
                    self.ended.push((v.note, v.channel, v.id));
                }
                *v = Voice::default();
            }
        }
    }
    pub fn all_off(&mut self, channel: u8, hard: bool) {
        for v in &mut self.voices {
            if v.channel == channel {
                v.held = false;
                v.amp.off(
                    if hard {
                        0.002
                    } else {
                        seconds(self.target[RELEASE])
                    },
                    self.rate,
                );
                v.env.off(0.002, self.rate);
            }
        }
    }
    pub fn take_ended(&mut self) -> impl Iterator<Item = (u8, u8, Option<i32>)> + '_ {
        self.ended.drain(..)
    }
    pub fn active(&self) -> bool {
        self.voices.iter().any(|v| v.amp.stage != 0) || self.tail > 0
    }
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let modular=self.target[RACK_MODE]>0.5;
        if modular {for rack in &mut self.modular {rack.prepare(&self.target);}}
        let modular_tail=modular && (0..RACK_SLOTS).any(|slot|{let b=crate::fxrack::base(slot);self.target[b]>0.&&self.target[b+2]>0.&&self.target[b+11]>0.});
        let smooth = 1. - (-1. / (0.01 * self.rate)).exp();
        for (out_l, out_r) in left.iter_mut().zip(right) {
            for i in 0..COUNT {
                self.current[i] += (self.target[i] - self.current[i]) * smooth;
            }
            for i in [A_SOURCE, B_SOURCE] {
                if (self.target[i] - self.current[i]).abs() < 0.0001 {
                    self.current[i] = self.target[i];
                }
            }
            let p = &self.current;
            let discrete = &self.target;
            let attack = seconds(p[ATTACK]);
            let decay = seconds(p[DECAY]);
            let ea = seconds(p[ENV_ATTACK]);
            let ed = seconds(p[ENV_DECAY]);
            let rates: [f32; 4] = std::array::from_fn(|i| {
                let c = LFO_CONTROLS[i];
                let mut timing = *p;
                timing[c[1]] = discrete[c[1]];
                timing[c[3]] = discrete[c[3]];
                crate::modulation::rate(&timing, i, self.tempo)
            });
            let mut live = crate::modulation::Live::default();
            let mut newest = 0;
            let mut output = [0.; 2];
            let mut lanes = [[0.; 2]; 4];
            let mut sounding = false;
            for (voice_index, v) in self.voices.iter_mut().enumerate() {
                if v.amp.stage == 0 {
                    continue;
                }
                sounding = true;
                if v.amp.reference {
                    v.amp.settings = envelope_settings(p, ATTACK, AMP_HOLD);
                }
                if v.env.reference {
                    v.env.settings = envelope_settings(p, ENV_ATTACK, ENV_HOLD);
                }
                let amp = v.amp.tick(attack, decay, p[SUSTAIN] / 1000., self.rate) * v.velocity;
                let env = v.env.tick(ea, ed, p[ENV_SUSTAIN] / 1000., self.rate);
                if v.owner == 0 && v.amp.stage == 0 && self.ended.len() < 256 {
                    self.ended.push((v.note, v.channel, v.id));
                }
                let mut phases = [v.lfo, v.lfo_extra[0], v.lfo_extra[1], v.lfo_extra[2]];
                for i in 0..4 {
                    if discrete[LFO_CONTROLS[i][2]] as usize == 1 {
                        phases[i] = self.free_phase[i];
                    }
                }
                let raw_lfo = if discrete[LFO_DRAW] > 0.5 {
                    self.curves.sample(0, phases[0])
                } else {
                    match discrete[LFO_SHAPE] as usize {
                        1 => 1. - 4. * (phases[0] - 0.5).abs(),
                        2 => phases[0] * 2. - 1.,
                        3 => {
                            if phases[0] < 0.5 {
                                1.
                            } else {
                                -1.
                            }
                        }
                        _ => (phases[0] * TAU).sin(),
                    }
                };
                let lfo = raw_lfo * p[LFO_AMOUNT] / 1000.;
                let sources = [
                    0.,
                    raw_lfo,
                    env,
                    v.velocity,
                    (v.note as f32 - 60.) / 60.,
                    self.curves.sample(1, phases[1]),
                    self.curves.sample(2, phases[2]),
                    self.curves.sample(3, phases[3]),
                    p[MACRO1] / 1000.,
                    p[MACRO2] / 1000.,
                    p[MACRO3] / 1000.,
                    p[MACRO4] / 1000.,
                    v.amp.value,
                ];
                let mut modulation = [0_f32; 21];
                for slot in ROUTES {
                    modulation[(discrete[slot + 1] as usize).min(20)] +=
                        sources[(discrete[slot] as usize).min(12)] * p[slot + 2] / 1000.;
                }
                v.lfo = crate::modulation::advance(
                    v.lfo,
                    rates[0] / self.rate,
                    discrete[LFO1_MODE] as usize,
                );
                for i in 0..3 {
                    v.lfo_extra[i] = crate::modulation::advance(
                        v.lfo_extra[i],
                        rates[i + 1] / self.rate,
                        discrete[LFO_CONTROLS[i + 1][2]] as usize,
                    );
                }
                if v.age >= newest {
                    newest = v.age;
                    live = crate::modulation::Live {
                        phases,
                        sources,
                        amounts: modulation,
                        env_elapsed: [v.amp.elapsed, v.env.elapsed],
                        env_stage: [v.amp.stage, v.env.stage],
                        active: v.amp.stage != 0,
                    };
                }
                let mut voice_params = *p;
                for (i, k) in EXTRA_TARGETS.iter().enumerate() {
                    let s = &SETTINGS[*k];
                    voice_params[*k] = (p[*k] + modulation[8 + i] * (s.range.1 - s.range.0) as f32)
                        .clamp(s.range.0 as f32, s.range.1 as f32);
                }
                let p = &voice_params;
                let target = discrete[LFO_TARGET] as usize;
                let pitch = v.note as f32 - 69.
                    + v.tune
                    + modulation[0] * 12.
                    + self.bend[v.channel as usize]
                    + if target == 2 { lfo * 2. } else { 0. };
                let base = 440. * 2.0_f32.powf(pitch / 12.);
                let mut sig = [0.; 2];
                let mut oscillator = [[0.; 2]; 4];
                for o in 0..2 {
                    let b = o * 8;
                    let source = if o == 0 { A_SOURCE } else { B_SOURCE };
                    let mix = p[source].clamp(0., 1.);
                    let count = discrete[b + 4] as usize;
                    let bank = discrete[b] as usize;
                    let pos = (p[b + 1] / 1000.
                        + modulation[4 + o]
                        + if target == 0 { lfo * 0.5 } else { 0. })
                    .clamp(0., 1.);
                    for u in 0..if mix < 1. { count } else { 0 } {
                        let offset = if count == 1 {
                            0.
                        } else {
                            u as f32 / (count - 1) as f32 * 2. - 1.
                        };
                        let freq = base * 2.0_f32.powf((p[b + 3] + offset * p[b + 5] / 100.) / 12.);
                        let phase = v.phase[o][u];
                        v.phase[o][u] = (phase + freq / self.rate).fract();
                        // Phase bend is an original two-slope warp; conservative mip selection limits added harmonics.
                        let pivot = 0.5 + p[b + 7] / 200.;
                        let fm = p[A_PHASE_FM + o * 2] / 2000.;
                        let (shaped, fm_slope) = if fm > 0. && self.custom.samples[o].is_none() {
                            let ratio = discrete[A_FM_RATIO + o * 2].round().clamp(1., 8.);
                            let fm = fm * (1. - (freq * ratio / self.rate - 0.10) / 0.27).clamp(0., 1.);
                            if fm > 0. {
                                ((phase + (phase * ratio * std::f32::consts::TAU).sin() * fm).rem_euclid(1.),
                                    1. + std::f32::consts::TAU * ratio * fm)
                            } else {
                                (phase, 1.)
                            }
                        } else {
                            (phase, 1.)
                        };
                        let warped = if shaped < pivot {
                            shaped / pivot * 0.5
                        } else {
                            0.5 + (shaped - pivot) / (1. - pivot) * 0.5
                        };
                        let max_slope = 0.5 / pivot.min(1. - pivot) * fm_slope;
                        let s = if freq < self.rate * 0.45 {
                            if let Some(sample) = &self.custom.samples[o] {
                                sample.tick(&mut v.sample_cursor[o][u], freq, self.rate)
                            } else if let Some(table) = &self.custom.oscillators[o] {
                                table.sample(pos, warped, freq * max_slope, self.rate)
                            } else {
                                self.tables
                                    .sample(bank, pos, warped, freq * max_slope, self.rate)
                            }
                        } else {
                            0.
                        };
                        let pan = (p[b + 6] / 100. + modulation[6] + offset * p[SPREAD] / 1000.)
                            .clamp(-1., 1.);
                        let angle = (pan + 1.) * PI / 4.;
                        let blend = p[if o == 0 { A_BLEND } else { B_BLEND }] / 1000.;
                        let gain = (1. - mix) * p[b + 2] / 1000. / (count as f32).sqrt()
                            * (blend_weight(count, u, blend) / blend_weight(count, u, 0.75));
                        oscillator[o][0] += s * angle.cos() * gain;
                        oscillator[o][1] += s * angle.sin() * gain;
                    }
                    if mix > 0.00001 || discrete[source] > 0.5 {
                        let ref_pitch = v.note as f64 - 69.
                            + v.tune as f64
                            + modulation[0] as f64 * 12.
                            + self.bend[v.channel as usize] as f64
                            + if target == 2 { lfo as f64 * 2. } else { 0. };
                        let frequency = 440. * 2_f64.powf((ref_pitch + p[b + 3] as f64) / 12.);
                        let mode = WARP_IDS[discrete[source + 1] as usize];
                        // The existing bipolar Bend control remains useful as a fine offset.
                        let amount = (p[source + 2] / 1000.
                            + modulation[2 + o]
                            + p[b + 7] / 200.
                            + if target == 0 { lfo * 0.5 } else { 0. })
                        .clamp(0., 1.);
                        let pair = self.reference[voice_index].tick(
                            o,
                            frequency,
                            count,
                            p[b + 5],
                            p[SPREAD] / 1000.,
                            p[if o == 0 { A_BLEND } else { B_BLEND }] / 1000.,
                            p[b + 6] / 200. + 0.5 + modulation[6] * 0.5,
                            crate::reference_oscillator::Warp::from_reference_id(mode).unwrap(),
                            amount,
                            (discrete[b + 2] / 1000. + modulation[10 + o]).clamp(0., 1.),
                        );
                        oscillator[o][0] += pair[0] * mix;
                        oscillator[o][1] += pair[1] * mix;
                    }
                }
                for o in 0..2 {for ch in 0..2 {oscillator[o][ch]*=p[if o==0{A_ON}else{B_ON}];}}
                let ring = p[B_RING] / 1000.;
                if ring > 0. {
                    for ch in 0..2 {
                        let modulator = (oscillator[0][ch] * 2.).clamp(-1., 1.);
                        oscillator[1][ch] *= 1. - ring + ring * modulator;
                    }
                }
                for ch in 0..2 {sig[ch]=oscillator[0][ch]+oscillator[1][ch];}
                let sub_base=if discrete[SUB_TRACK]>0.5 {base}else{261.62558};
                let sub_step=(sub_base*2_f32.powf(p[SUB_OCT])/self.rate).min(0.49);
                v.sub=(v.sub+sub_step).fract();
                let sub=crate::sources::sub(v.sub,sub_step,discrete[SUB_SHAPE] as usize)*p[SUB]/1000.*p[SUB_ON];
                self.noise^=self.noise<<13;self.noise^=self.noise>>17;self.noise^=self.noise<<5;
                let noise_source=(discrete[NOISE_TYPE] as usize).min(8);
                let raw_noise=if noise_source==0 && discrete[NOISE_PITCH]==0. && discrete[NOISE_TRACK]<0.5 && discrete[NOISE_ONESHOT]<0.5 && discrete[NOISE_RANDOM]>0.5 {
                    self.noise as f32/u32::MAX as f32*2.-1.
                }else{
                    let playback=if noise_source==8 {self.custom.noise.as_ref()}else{Some(&crate::sources::library()[noise_source])};
                    playback.map(|sample|{let root=440.*2_f32.powf((sample.source.root-69.)/12.);let pitch=if discrete[NOISE_TRACK]>0.5 {base/261.62558}else{1.};crate::sources::noise_tick(sample,&mut v.noise_cursor,root*pitch*2_f32.powf(p[NOISE_PITCH]/12.),self.rate,discrete[NOISE_ONESHOT]>0.5)}).unwrap_or(0.)
                };
                let noise=raw_noise*p[NOISE]/1000.*0.3*p[NOISE_ON];
                let cv = (p[CUTOFF]
                    + modulation[1] * 1000.
                    + env * p[ENV_AMOUNT]
                    + if target == 1 { lfo * 500. } else { 0. })
                .clamp(0., 1000.);
                let g = (PI * cutoff(cv).min(self.rate * 0.42) / self.rate).tan();
                let k = 2. - 1.9 * p[RESONANCE] / 1000.;
                let drive = 1. + p[DRIVE] / 100.;
                let am = if target == 3 {
                    1. - p[LFO_AMOUNT] / 2000. + lfo * 0.5
                } else {
                    1.
                };
                let model = discrete[FILTER_MODEL] as usize;
                if v.filter_model != model {
                    v.filter_model = model;
                    v.recovered_filter = Default::default();
                    v.filter = Default::default();
                    v.lane_filter = Default::default();
                    v.lane_recovered = Default::default();
                }
                let coeff = if model > 0 {
                    Some(crate::reference_filter::Coefficients::new(
                        self.rate * 2.,
                        cutoff(cv).min(self.rate * 0.42) as f64,
                        (p[RESONANCE] / 1000.) as f64,
                        p[DRIVE] / 1000.,
                        discrete[FILTER] as usize,
                        if model == 2 { 4 } else { 2 },
                    ))
                } else {
                    None
                };
                if discrete[DUAL_FILTER]>0.5 {
                    for ch in 0..2 {let pan=|v:f32|if ch==0 {1.-v.max(0.)/100.}else{1.+v.min(0.)/100.};oscillator[2][ch]=sub*pan(p[SUB_PAN]);oscillator[3][ch]=noise*pan(p[NOISE_PAN]);}
                    let keys=[[FILTER,FILTER_MODEL,CUTOFF,RESONANCE,DRIVE,F1_MIX,F1_KEY,F1_PAN,F1_ON],[F2_FILTER,F2_MODEL,F2_CUTOFF,F2_RESONANCE,F2_DRIVE,F2_MIX,F2_KEY,F2_PAN,F2_ON]];
                    let coeffs=keys.map(|keys|{let cv=if keys[0]==FILTER{cv}else{p[keys[2]]};let hz=(cutoff(cv)*2_f32.powf((v.note as f32-60.)/12.*p[keys[6]]/1000.)).min(self.rate*0.42);
                        let mode=discrete[keys[0]] as usize;let model=discrete[keys[1]] as usize;
                        ((PI*hz/self.rate).tan(),2.-1.9*p[keys[3]]/1000.,crate::reference_filter::Coefficients::new(self.rate*2.,hz as f64,(p[keys[3]]/1000.) as f64,p[keys[4]]/1000.,mode,if model==2{4}else{2}))});
                    for f in 0..2 {let signature=discrete[keys[f][0]] as usize+8*discrete[keys[f][1]] as usize;if v.dual_model[f]!=signature {for lane in 0..4{v.dual_filter[lane][f]=Default::default();v.dual_recovered[lane][f]=Default::default();}v.dual_model[f]=signature;}}
                    for lane in 0..4 {for ch in 0..2 {let dry=oscillator[lane][ch]*0.3;
                        let direct=if lane==2{p[SUB_DIRECT]}else if lane==3{p[NOISE_DIRECT]}else{0.};
                        let mut apply=|f:usize,x:f32|{let ks=keys[f];let amount=p[FILTER_INPUTS[f][lane]]*p[ks[8]];if amount<0.00001{return x;}
                            let mode=discrete[ks[0]] as usize;let model=discrete[ks[1]] as usize;let drive=1.+p[ks[4]]/100.;
                            let input=if model==0&&p[ks[4]]>0.1{(x*drive).tanh()/drive.sqrt()}else{x};
                            let wet=if model>0{v.dual_recovered[lane][f][ch].tick(input,&coeffs[f].2)}else{v.dual_filter[lane][f][ch].tick(input,coeffs[f].0,coeffs[f].1,mode)};
                            let pan=if ch==0{1.-p[ks[7]].max(0.)/100.}else{1.+p[ks[7]].min(0.)/100.};
                            x+(wet*pan-x)*p[ks[5]]/1000.*amount
                        };
                        let filtered=match discrete[FILTER_TOPOLOGY] as usize {1=>{let x=apply(0,dry);apply(1,x)},2=>{let x=apply(1,dry);apply(0,x)},_=>{
                            let a=p[FILTER_INPUTS[0][lane]]*p[F1_ON];let b=p[FILTER_INPUTS[1][lane]]*p[F2_ON];
                            dry+((apply(0,dry)-dry)+(apply(1,dry)-dry))/(a+b).max(1.)
                        }};
                        let filtered=filtered+(dry-filtered)*direct;
                        lanes[lane][ch]+=filtered*amp*am*(1.+modulation[7]).clamp(0.,2.);
                    }}
                }else{
                    oscillator[2]=[sub+noise;2];
                    for lane in 0..3 {for ch in 0..2 {
                        let x=oscillator[lane][ch]*0.3;
                        let x=if model==0 && p[DRIVE]>0.1 {(x*drive).tanh()/drive.sqrt()} else {x};
                        let filtered=if let Some(c)=&coeff {v.lane_recovered[lane][ch].tick(x,c)} else {v.lane_filter[lane][ch].tick(x,g,k,discrete[FILTER] as usize)};
                        lanes[lane][ch]+=filtered*amp*am*(1.+modulation[7]).clamp(0.,2.);
                    }}
                }
                for ch in 0..2 {
                    let x = (sig[ch] + sub + noise) * 0.3;
                    let x = if model == 0 && p[DRIVE] > 0.1 {
                        (x * drive).tanh() / drive.sqrt()
                    } else {
                        x
                    };
                    let filtered = if let Some(c) = &coeff {
                        v.recovered_filter[ch].tick(x, c)
                    } else {
                        v.filter[ch].tick(x, g, k, discrete[FILTER] as usize)
                    };
                    output[ch] += filtered * amp * am * (1. + modulation[7]).clamp(0., 2.);
                }
            }
            self.live = live;
            for i in 0..4 {
                self.free_phase[i] = (self.free_phase[i] + rates[i] / self.rate).fract();
            }
            if sounding {
                self.tail = (self.rate
                    * if modular_tail || !modular && (p[DELAY_MIX] > 0.1 || p[REVERB_MIX] > 0.1) {
                        90.
                    } else if p[CHORUS_MIX] > 0.1 {
                        0.06
                    } else {
                        0.
                    }) as usize;
            } else {
                self.tail = self.tail.saturating_sub(1);
            }
            self.ch_phase = (self.ch_phase + (0.05 + p[CHORUS_RATE] / 200.) / self.rate).fract();
            let mut wet_delay = [0.; 2];
            for ch in 0..2 {
                let ms = 12.
                    + (self.ch_phase * TAU + ch as f32 * PI / 2.).sin() * p[CHORUS_DEPTH] / 100.;
                let wet = read_delay(&self.chorus[ch], self.ch_head, ms * 0.001 * self.rate);
                self.chorus[ch][self.ch_head] = output[ch];
                output[ch] =
                    output[ch] * (1. - p[CHORUS_MIX] / 2000.) + wet * p[CHORUS_MIX] / 2000.;
                wet_delay[ch] = read_delay(
                    &self.delay[ch],
                    self.head,
                    (50. + p[DELAY_TIME]) * 0.001 * self.rate,
                );
            }
            for ch in 0..2 {
                self.delay[ch][self.head] =
                    (output[ch] + wet_delay[1 - ch] * p[DELAY_FEEDBACK] / 1000. * 0.85).tanh();
                output[ch] += wet_delay[ch] * p[DELAY_MIX] / 1000. * 0.6;
            }
            self.head = (self.head + 1) % self.delay[0].len();
            self.ch_head = (self.ch_head + 1) % self.chorus[0].len();
            // Preserve the original summed filter/chorus/delay sound for existing patches.
            // Independent assignments or a new effect crossfade into isolated oscillator lanes.
            let independent=discrete[DUAL_FILTER]>0.5 || modular || crate::effects::needs_lanes(discrete);
            self.rack_blend+=((if independent {1.} else {0.})-self.rack_blend)*smooth;
            if self.rack_blend<0.00001 {self.rack_blend=0.;} else if self.rack_blend>0.99999 {self.rack_blend=1.;}
            let (a,b)=if modular {(self.modular[0].tick(lanes[0],p,discrete,0,self.tempo),self.modular[1].tick(lanes[1],p,discrete,1,self.tempo))}
                else {(self.racks[0].tick(lanes[0],p,0),self.racks[1].tick(lanes[1],p,1))};
            for ch in 0..2 {output[ch]+=(a[ch]+b[ch]+lanes[2][ch]+lanes[3][ch]-output[ch])*self.rack_blend;}
            let gain = p[GAIN] / 1000.;
            *out_l = (output[0] * gain).tanh();
            *out_r = (output[1] * gain).tanh();
        }
    }
}
fn envelope_settings(p: &Patch, base: usize, hold: usize) -> crate::reference_envelope::Settings {
    crate::reference_envelope::Settings {
        attack: seconds(p[base]) as f64,
        hold: hold_seconds(p[hold]) as f64,
        decay: seconds(p[base + 1]) as f64,
        sustain: (p[base + 2] / 1000.).powi(2),
        release: seconds(p[base + 3]) as f64,
        curves: std::array::from_fn(|i| p[hold + 1 + i] as f64 / 1000.),
    }
}
fn read_delay(data: &[f32], head: usize, delay: f32) -> f32 {
    let at = (head as f32 - delay.clamp(1., data.len() as f32 - 2.)).rem_euclid(data.len() as f32);
    // f32 rem_euclid can round a tiny negative position up to exactly len.
    let i = (at as usize) % data.len();
    let f = at - at.floor();
    data[i] * (1. - f) + data[(i + 1) % data.len()] * f
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_fm_ratio_and_ring_modulation_change_audio_with_a_dry_default() {
        let render = |patch, note| {
            let mut engine = Engine::new(48000.);
            engine.set(patch);
            engine.reset();
            engine.on(note, 0.8, 0, None);
            let mut l = vec![0.; 8192];
            let mut r = l.clone();
            engine.render(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 2.));
            l
        };
        let mut p = defaults();
        p[A_LEVEL] = 650.;
        p[B_LEVEL] = 550.;
        p[B_TUNE] = 7.;
        let dry = render(p, 60);
        p[A_FM_RATIO] = 4.;
        assert_eq!(render(p, 60), dry);
        p[A_PHASE_FM] = 660.;
        let fm = render(p, 60);
        assert_ne!(fm, dry);
        p[A_FM_RATIO] = 2.;
        assert_ne!(render(p, 60), fm);
        p[A_PHASE_FM] = 0.;
        p[B_PHASE_FM] = 660.;
        assert_ne!(render(p, 60), dry);
        p[B_PHASE_FM] = 0.;
        p[B_RING] = 1000.;
        assert_ne!(render(p, 60), dry);
        p[A_PHASE_FM] = 1000.;
        p[B_PHASE_FM] = 1000.;
        p[A_FM_RATIO] = 8.;
        p[B_FM_RATIO] = 8.;
        render(p, 108);
    }
    #[test]
    fn dedicated_sources_power_pitch_pan_and_dual_filter_routes() {
        let mut engine=Engine::new(48000.);
        let mut render=|p:Patch,note:u8| {engine.set(p);engine.reset();engine.on(note,0.8,0,None);let mut l=vec![0.;8192];let mut r=l.clone();engine.render(&mut l,&mut r);(l,r)};
        let energy=|x:&Vec<f32>|x.iter().map(|x|x*x).sum::<f32>();
        for o in 0..2 {let mut p=defaults();p[A_LEVEL]=if o==0{700.}else{0.};p[B_LEVEL]=if o==1{700.}else{0.};for reference in [0.,1.]{p[if o==0{A_SOURCE}else{B_SOURCE}]=reference;assert!(energy(&render(p,60).0)>0.01);p[if o==0{A_ON}else{B_ON}]=0.;assert_eq!(energy(&render(p,60).0),0.);p[if o==0{A_ON}else{B_ON}]=1.;}}
        let mut p=defaults();p[A_ON]=0.;p[B_ON]=0.;p[SUB]=700.;p[SUB_OCT]=-1.;p[DUAL_FILTER]=1.;
        let (l,_)=render(p,69);let crossings=l[1024..].windows(2).filter(|w|w[0]<=0.&&w[1]>0.).count();assert!((crossings as f32-220.*7168./48000.).abs()<2.);
        p[SUB_PAN]=-100.;let (l,r)=render(p,69);assert!(energy(&l)>0.01);assert_eq!(energy(&r),0.);p[SUB_ON]=0.;assert_eq!(energy(&render(p,69).0),0.);
        p[SUB]=0.;p[NOISE]=500.;p[NOISE_TYPE]=6.;p[NOISE_RANDOM]=0.;assert!(energy(&render(p,60).0)>0.01);p[NOISE_ON]=0.;assert_eq!(energy(&render(p,60).0),0.);
        let mut p=defaults();p[DUAL_FILTER]=1.;p[FILTER]=1.;p[CUTOFF]=100.;p[A_BANK]=0.;p[A_POSITION]=1000.;p[F1_A]=0.;p[F1_B]=1.;let dry=render(p,72).0;p[F1_A]=1.;let low=render(p,72).0;assert!(energy(&low)<energy(&dry)*0.6);
        p[F1_A]=0.;p[F2_ON]=1.;p[F2_A]=1.;p[F2_FILTER]=3.;p[F2_CUTOFF]=950.;let high=render(p,72).0;assert!(energy(&high)<energy(&dry)*0.7);
        p[F1_A]=1.;p[FILTER_TOPOLOGY]=1.;let serial=render(p,72).0;p[FILTER_TOPOLOGY]=0.;let parallel=render(p,72).0;assert!(serial.iter().zip(parallel).any(|(a,b)|(a-b).abs()>0.00001));
        p[A_ON]=0.;p[SUB_ON]=1.;p[SUB]=700.;p[SUB_DIRECT]=1.;let direct=render(p,69).0;p[SUB_DIRECT]=0.;assert!(energy(&render(p,69).0)<energy(&direct));
    }
    #[test]
    fn oscillator_effect_routing_isolates_the_other_audio_lane() {
        for (row,mix) in [DIST_MIX,OTT_MIX,CHORUS_MIX,DELAY_MIX,REVERB_MIX].into_iter().enumerate() {
            let render=|assigned:usize| {
                let mut p=defaults();p[B_LEVEL]=0.;p[SUB]=0.;p[NOISE]=0.;p[FILTER]=0.;p[mix]=800.;p[DELAY_TIME]=0.;
                for pair in FX_ROUTES {for k in pair {p[k]=0.;}}
                if assigned<2 {p[FX_ROUTES[row][assigned]]=1.;}
                let mut e=Engine::new(48000.);e.set(p);e.reset();e.on(60,0.8,0,None);
                let mut l=vec![0.;8192];let mut r=l.clone();e.render(&mut l,&mut r);l
            };
            let dry=render(2);let unassigned=render(1);let wet=render(0);
            assert_eq!(dry,unassigned,"effect {row} leaked from B into A");
            assert!(dry.iter().zip(wet).any(|(a,b)|(a-b).abs()>0.0001),"effect {row} missing from A");
        }
    }
    #[test]
    fn macros_drawn_lfos_oneshot_and_free_run_drive_real_voice_audio() {
        fn render(
            p: Patch,
            note: u8,
            curves: crate::modulation::Curves,
            block: usize,
        ) -> (Vec<f32>, crate::modulation::Live) {
            let mut e = Engine::new(48000.);
            e.curves = curves;
            e.set(p);
            e.reset();
            e.on(note, 0.8, 0, None);
            let mut l = vec![0.; 24000];
            let mut r = l.clone();
            for (l, r) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
                e.render(l, r);
            }
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.));
            (l, e.live)
        }
        let mut p = defaults();
        p[A_SOURCE] = 1.;
        p[FILTER] = 0.;
        p[SUSTAIN] = 1000.;
        let curves = crate::modulation::Curves::default();
        let base = render(p, 60, curves, 127).0;
        p[MOD3_SOURCE] = 8.;
        p[MOD3_TARGET] = 0.;
        p[MOD3_AMOUNT] = 1000.;
        p[MACRO1] = 1000.;
        let transposed = render(p, 60, curves, 127).0;
        p[MOD3_SOURCE] = 0.;
        assert_eq!(transposed, render(p, 72, curves, 127).0);
        for (lfo, source) in [1, 5, 6, 7].into_iter().enumerate() {
            p[MOD3_SOURCE] = source as f32;
            p[MOD3_TARGET] = 7.;
            p[MOD3_AMOUNT] = 800.;
            p[LFO_DRAW] = 1.;
            let c = LFO_CONTROLS[lfo];
            p[c[1]] = 6.;
            p[c[2]] = 2.;
            let mut curves = curves;
            curves.samples[lfo] = std::array::from_fn(|i| 1. - i as f32 / 32.);
            let a = render(p, 60, curves, 127);
            assert_ne!(base, a.0);
            assert_eq!(a.0, render(p, 60, curves, 1).0);
            assert_eq!(a.1.phases[lfo], 1.);
            assert_eq!(a.1.sources[source], -1.);
        }
        let mut e = Engine::new(48000.);
        p[LFO2_MODE] = 1.;
        p[LFO2_SYNC] = 5.;
        e.set(p);
        e.reset();
        e.render(&mut vec![0.; 6000], &mut vec![0.; 6000]);
        e.on(60, 1., 0, None);
        e.render(&mut [0.], &mut [0.]);
        assert!((e.live.phases[1] - 0.25).abs() < 0.001);
    }
    #[test]
    fn curved_voices_release_with_pedal_and_are_block_invariant() {
        fn run(block: usize) -> (Vec<f32>, Vec<(u8, u8, Option<i32>)>) {
            let mut e = Engine::new(48000.);
            let mut p = defaults();
            p[ENV_ENGINE] = 1.;
            p[FILTER_MODEL] = 2.;
            p[FILTER] = 1.;
            p[A_SOURCE] = 1.;
            p[AMP_HOLD] = 300.;
            p[AMP_ATK_CURVE] = 750.;
            p[ENV_DEC_CURVE] = 750.;
            p[AMP_REL_CURVE] = 250.;
            p[RELEASE] = 250.;
            p[ENV_AMOUNT] = 500.;
            e.set(p);
            e.reset();
            e.on(60, 0.8, 0, Some(12));
            e.sustain(0, true);
            e.off(60, 0, Some(12));
            let mut out = Vec::new();
            for phase in 0..2 {
                if phase == 1 {
                    e.sustain(0, false);
                }
                let mut l = vec![0.; 12000];
                let mut r = l.clone();
                for (l, r) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
                    e.render(l, r);
                }
                out.extend(l);
                if phase == 0 {
                    assert!(e.active());
                } else {
                    assert!(!e.active());
                }
            }
            let ended = e.take_ended().collect();
            (out, ended)
        }
        let a = run(1);
        assert_eq!(a, run(127));
        assert_eq!(a.1, vec![(60, 0, Some(12))]);
        assert!(a.0.iter().any(|x| x.abs() > 0.001));
        assert!(a.0.iter().all(|x| x.is_finite()));
        assert!(a.0[23900..].iter().all(|x| *x == 0.));
    }
    #[test]
    fn phase_blend_and_two_routes_change_real_audio_without_breaking_blocks() {
        fn render(p: Patch, note: u8, block: usize) -> (Vec<f32>, Vec<f32>) {
            let mut e = Engine::new(48000.);
            e.set(p);
            e.reset();
            e.on(note, 1., 0, Some(1));
            let (mut l, mut r) = (vec![0.; 8192], vec![0.; 8192]);
            for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
                e.render(a, b);
            }
            assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.));
            (l, r)
        }
        let mut p = defaults();
        p[A_SOURCE] = 1.;
        p[ATTACK] = 0.;
        p[SUSTAIN] = 1000.;
        let baseline = render(p, 60, 127);
        p[A_PHASE] = 90.;
        assert_ne!(baseline, render(p, 60, 127));
        p[A_PHASE] = 360.;
        assert_eq!(baseline, render(p, 60, 127));
        p[A_PHASE] = 0.;
        p[MOD1_SOURCE] = 3.;
        p[MOD1_TARGET] = 0.;
        p[MOD1_AMOUNT] = 1000.;
        let transposed = render(p, 60, 127);
        p[MOD1_AMOUNT] = 0.;
        assert_eq!(transposed, render(p, 72, 127));
        p[MOD1_SOURCE] = 1.;
        p[MOD1_TARGET] = 6.;
        p[MOD1_AMOUNT] = 700.;
        p[MOD2_SOURCE] = 2.;
        p[MOD2_TARGET] = 2.;
        p[MOD2_AMOUNT] = 650.;
        p[A_MODE] = 9.;
        let routed = render(p, 60, 127);
        assert_ne!(baseline, routed);
        assert_eq!(routed, render(p, 60, 2048));
        p[MOD1_AMOUNT] = 0.;
        p[MOD2_AMOUNT] = 0.;
        p[A_MODE] = 0.;
        p[A_UNISON] = 5.;
        let blend = render(p, 60, 127);
        p[A_BLEND] = 0.;
        assert_ne!(blend, render(p, 60, 127));
        p[A_RANDOM] = 800.;
        let random = render(p, 60, 127);
        assert_eq!(random, render(p, 60, 2048));
        // Successive notes get new phases, while fixed phase is repeatable.
        let mut e = Engine::new(48000.);
        e.set(p);
        e.reset();
        e.on(60, 1., 0, None);
        let mut l = vec![0.; 1024];
        let mut r = l.clone();
        e.render(&mut l, &mut r);
        let first = l.clone();
        e.reset();
        e.on(60, 1., 0, None);
        e.render(&mut l, &mut r);
        assert_ne!(first, l);
    }
    #[test]
    fn reference_sources_play_all_warps_and_release_across_blocks() {
        for rate in [44100., 48000., 96000.] {
            let mut a = Engine::new(rate);
            let mut b = Engine::new(rate);
            let mut p = defaults();
            p[A_SOURCE] = 1.;
            p[B_SOURCE] = 1.;
            p[A_UNISON] = 3.;
            p[B_UNISON] = 2.;
            p[A_LEVEL] = 650.;
            p[B_LEVEL] = 300.;
            p[A_AMOUNT] = 650.;
            p[B_AMOUNT] = 420.;
            p[RELEASE] = 0.;
            p[DELAY_MIX] = 0.;
            p[CHORUS_MIX] = 0.;
            for mode in 0..11 {
                p[A_MODE] = mode as f32;
                p[B_MODE] = (10 - mode) as f32;
                for engine in [&mut a, &mut b] {
                    engine.set(p);
                    engine.reset();
                    engine.on(60, 0.8, 0, Some(1));
                    engine.on(67, 0.7, 0, Some(2));
                }
                let (mut al, mut ar) = (vec![0.; 2048], vec![0.; 2048]);
                let (mut bl, mut br) = (al.clone(), ar.clone());
                a.render(&mut al, &mut ar);
                for (l, r) in bl.chunks_mut(73).zip(br.chunks_mut(73)) {
                    b.render(l, r);
                }
                assert_eq!(al, bl);
                assert_eq!(ar, br);
                assert!(al.iter().chain(&ar).all(|v| v.is_finite() && v.abs() <= 1.));
                assert!(al.iter().any(|v| v.abs() > 0.001));
                a.off(60, 0, Some(1));
                a.off(67, 0, Some(2));
                a.render(&mut al, &mut ar);
                assert!(al[1024..].iter().chain(&ar[1024..]).all(|v| v.abs() < 1e-6));
                a.reset();
                a.render(&mut al, &mut ar);
                assert!(al.iter().chain(&ar).all(|v| *v == 0.));
            }
        }
    }
    #[test]
    fn delay_wrap_interpolates_across_the_ring_boundary() {
        let mut data = vec![0.; 57602];
        data[0] = 1.;
        let v = read_delay(&data, 18000, 18000.002);
        assert!(v.is_finite() && (0.99..=1.).contains(&v));
        data[57601] = 0.5;
        assert!((read_delay(&data, 18000, 18000.5) - 0.75).abs() < 1e-5);
    }
    #[test]
    fn render_pitch_release_reset_and_extremes() {
        for rate in [44100., 48000., 96000.] {
            let mut e = Engine::new(rate);
            let mut p = defaults();
            p[A_POSITION] = 0.;
            p[ATTACK] = 0.;
            p[SUSTAIN] = 1000.;
            p[FILTER] = 0.;
            p[RELEASE] = 0.;
            e.set(p);
            e.reset();
            e.on(69, 1., 0, Some(1));
            let mut l = vec![0.; rate as usize];
            let mut r = l.clone();
            e.render(&mut l, &mut r);
            let crosses = l[2000..]
                .windows(2)
                .filter(|s| s[0] <= 0. && s[1] > 0.)
                .count();
            let hz = crosses as f32 * rate / (l.len() - 2000) as f32;
            assert!((hz - 440.).abs() < 2., "{hz}");
            assert!(l.iter().any(|x| x.abs() > 0.01));
            e.off(69, 0, Some(1));
            e.render(&mut l[..1024], &mut r[..1024]);
            assert!(l[512..1024].iter().all(|x| x.abs() < 1e-6));
            for i in 0..COUNT {
                p[i] = SETTINGS[i].range.1 as f32;
            }
            e.set(p);
            e.reset();
            for n in 24..56 {
                e.on(n, 1., 0, None);
            }
            e.render(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.));
            e.reset();
            e.render(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| *x == 0.));
        }
    }
    #[test]
    fn channel_note_identity_pedal_and_block_boundaries() {
        let mut a = Engine::new(48000.);
        let mut b = Engine::new(48000.);
        a.on(60, 0.8, 0, Some(1));
        b.on(60, 0.8, 0, Some(1));
        let mut l = [0.; 1024];
        let mut r = l;
        let mut l2 = l;
        let mut r2 = l;
        a.render(&mut l, &mut r);
        for (x, y) in l2.chunks_mut(73).zip(r2.chunks_mut(73)) {
            b.render(x, y);
        }
        assert_eq!(l, l2);
        assert_eq!(r, r2);
        a.on(60, 0.8, 1, Some(2));
        a.sustain(0, true);
        a.off(60, 0, Some(1));
        assert!(a.voices.iter().any(|v| v.id == Some(1) && v.amp.stage != 4));
        a.sustain(0, false);
        assert!(a.voices.iter().any(|v| v.id == Some(1) && v.amp.stage == 4));
        assert!(a.voices.iter().any(|v| v.id == Some(2) && v.held));
    }
}
