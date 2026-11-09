// SPDX-License-Identifier: GPL-3.0-or-later
//! Recording analysis runs on the editor thread. Only finished audition clips cross into audio.
use crate::{
    sample::Sample,
    wavetable::{SIZE, Source},
};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

pub const MAX_POINTS: usize = 12;
pub const PREVIEW_RATE: f64 = 24_000.;
#[derive(Clone)]
pub struct Point {
    pub time: f32,
    pub hz: f32,
    pub cycle: Vec<f32>,
}

fn at(samples: &[f32], index: f64) -> f32 {
    let index = index.clamp(0., (samples.len() - 1) as f64);
    let lo = index as usize;
    samples[lo] + (samples[(lo + 1).min(samples.len() - 1)] - samples[lo]) * index.fract() as f32
}

pub fn analyze(sample: &Sample, time: f32) -> Result<Point, String> {
    let s = &sample.samples;
    let rate = sample.rate as f64;
    let center = (time.clamp(0., 1.) * (s.len() - 1) as f32) as usize;
    let stride = (sample.rate / 10000).max(1) as usize;
    let analysis_rate = rate / stride as f64;
    let max_lag = ((analysis_rate / 45.) as usize).min(s.len() / stride / 3);
    let min_lag = (analysis_rate / 1600.).ceil() as usize;
    if max_lag <= min_lag + 2 {
        return Err("This recording is too short for pitch detection.".into());
    }
    let span = 3 * max_lag;
    let start = center
        .saturating_sub(span * stride / 2)
        .min(s.len() - span * stride);
    let window: Vec<f64> = s[start..start + span * stride]
        .chunks_exact(stride)
        .map(|chunk| chunk.iter().map(|&v| v as f64).sum::<f64>() / stride as f64)
        .collect();
    let mean = window.iter().sum::<f64>() / span as f64;
    if window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (span as f64) < 0.000001 {
        return Err("That spot is quiet. Choose a sustained note or vowel.".into());
    }

    // Cumulative mean normalized difference: prefer the earliest convincing period.
    let mut difference = vec![1.; max_lag + 1];
    let mut running = 0.;
    for lag in 1..=max_lag {
        let error = (0..span - max_lag)
            .map(|i| (window[i] - window[i + lag]).powi(2))
            .sum::<f64>();
        running += error;
        if running > 1e-15 {
            difference[lag] = error * lag as f64 / running;
        }
    }
    let local = (min_lag..max_lag)
        .filter(|&i| difference[i] <= difference[i - 1] && difference[i] < difference[i + 1]);
    let lag = local
        .clone()
        .find(|&i| difference[i] < 0.13)
        .or_else(|| {
            local
                .min_by(|&a, &b| difference[a].total_cmp(&difference[b]))
                .filter(|&i| difference[i] < 0.29)
        })
        .ok_or("No stable pitch here. Move the marker onto a held note or vowel.")?;
    let curvature = difference[lag - 1] + difference[lag + 1] - 2. * difference[lag];
    let offset = if curvature > 1e-12 {
        (0.5 * (difference[lag - 1] - difference[lag + 1]) / curvature).clamp(-0.5, 0.5)
    } else {
        0.
    };
    let period = (lag as f64 + offset) * stride as f64;
    if 2. * period + 2. > s.len() as f64 {
        return Err("Choose a longer recording.".into());
    }
    let limit = s.len() as f64 - 2. * period - 1.;
    let mut best = 0.;
    let mut score = f64::INFINITY;
    for i in 0..17 {
        let begin = (center as f64 - period + (i as f64 / 16. - 0.5) * period).clamp(0., limit);
        let cost = (0..96)
            .map(|j| {
                let t = begin + period * j as f64 / 96.;
                (at(s, t) - at(s, t + period)).powi(2) as f64
            })
            .sum::<f64>();
        if cost < score {
            best = begin;
            score = cost;
        }
    }
    let mut cycle: Vec<_> = (0..SIZE)
        .map(|i| {
            let t = best + period * i as f64 / SIZE as f64;
            (at(s, t) + at(s, t + period)) * 0.5
        })
        .collect();
    // Taper the small residual boundary mismatch before looping.
    let seam = cycle[SIZE - 1] - cycle[0];
    for (i, x) in cycle[SIZE - 32..].iter_mut().enumerate() {
        let t = i as f32 / 31.;
        *x -= seam * t * t * (3. - 2. * t);
    }
    let dc = cycle.iter().sum::<f32>() / SIZE as f32;
    let peak = cycle.iter().map(|v| (v - dc).abs()).fold(0., f32::max);
    if peak < 0.001 {
        return Err("No usable cycle at that marker.".into());
    }
    for x in &mut cycle {
        *x = (*x - dc) * 0.9 / peak;
    }
    Ok(Point {
        time,
        hz: (rate / period) as f32,
        cycle,
    })
}

pub fn align(points: &mut [Point]) {
    for i in 1..points.len() {
        let prev = &points[i - 1].cycle;
        let next = &points[i].cycle;
        let dot = |shift: usize, step: usize| {
            (0..SIZE)
                .step_by(step)
                .map(|j| prev[j] * next[(j + shift) % SIZE])
                .sum::<f32>()
        };
        let coarse = (0..SIZE)
            .step_by(8)
            .max_by(|&a, &b| dot(a, 8).total_cmp(&dot(b, 8)))
            .unwrap();
        let shift = (coarse + SIZE - 8..=coarse + SIZE + 8)
            .map(|n| n % SIZE)
            .max_by(|&a, &b| dot(a, 2).total_cmp(&dot(b, 2)))
            .unwrap();
        points[i].cycle.rotate_left(shift);
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Blend {
    Linear,
    #[default]
    Smooth,
    Hold,
}
impl Blend {
    fn weight(self, t: f32) -> f32 {
        let smooth = |v: f32| v * v * (3. - 2. * v);
        match self {
            Self::Linear => t,
            Self::Smooth => smooth(t),
            Self::Hold => smooth(((t - 0.7) / 0.3).clamp(0., 1.)),
        }
    }
}
pub fn build(points: &[Point], blend: Blend, name: &str) -> Result<Source, String> {
    if !(2..=MAX_POINTS).contains(&points.len()) {
        return Err("Place at least two markers to build a morph.".into());
    }
    let mut samples = Vec::with_capacity(((points.len() - 1) * 10 + 1) * SIZE);
    for pair in points.windows(2) {
        for frame in 0..10 {
            let mix = blend.weight(frame as f32 / 10.);
            samples.extend(
                pair[0]
                    .cycle
                    .iter()
                    .zip(&pair[1].cycle)
                    .map(|(a, b)| a + (b - a) * mix),
            );
        }
    }
    samples.extend_from_slice(&points.last().unwrap().cycle);
    let source = Source {
        name: format!("{} morph", name.chars().take(48).collect::<String>()),
        samples,
    };
    source.validate()?;
    Ok(source)
}

// Produce a short pitched audition from a single frame or a complete table sweep.
pub fn audition(samples: &[f32], sweep: bool) -> Vec<f32> {
    let frames = samples.len() / SIZE;
    if frames == 0 {
        return Vec::new();
    }
    // Band-limit the C4 audition before sampling the periodic frame at 24 kHz.
    let mut filtered = Vec::with_capacity(samples.len());
    for frame in samples.chunks_exact(SIZE) {
        let mut real: Vec<_> = frame.iter().map(|&v| v as f64).collect();
        let mut imag = vec![0.; SIZE];
        crate::wavetable::fft(&mut real, &mut imag, false);
        let limit = (PREVIEW_RATE * 0.48 / 261.625565) as usize;
        real[limit + 1..SIZE - limit].fill(0.);
        imag[limit + 1..SIZE - limit].fill(0.);
        crate::wavetable::fft(&mut real, &mut imag, true);
        filtered.extend(real.iter().map(|v| (v / SIZE as f64) as f32));
    }
    let samples = &filtered;
    let length = if sweep { 48000 } else { 10800 };
    (0..length)
        .map(|i| {
            let pos = if sweep {
                i as f64 / (length - 1) as f64 * (frames - 1) as f64
            } else {
                0.
            };
            let phase = (i as f64 * 261.625565 / PREVIEW_RATE).fract() * SIZE as f64;
            let read = |f: usize| {
                let k = phase as usize;
                let a = samples[f * SIZE + k];
                let b = samples[f * SIZE + (k + 1) % SIZE];
                a + (b - a) * phase.fract() as f32
            };
            let a = read(pos as usize);
            let b = read(((pos as usize) + 1).min(frames - 1));
            let fade = ((i.min(length - 1 - i) as f32) / 240.).min(1.);
            (a + (b - a) * pos.fract() as f32) * fade * 0.22
        })
        .collect()
}

#[derive(Default)]
pub struct PreviewBus {
    pending: Mutex<(u64, Vec<f32>)>,
    generation: AtomicU64,
}
impl PreviewBus {
    pub fn play(&self, samples: Vec<f32>) {
        let mut slot = self.pending.lock().unwrap();
        let g = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        *slot = (g, samples);
    }
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
#[derive(Default)]
pub struct PreviewVoice {
    clip: Vec<f32>,
    generation: u64,
    position: f64,
    last: f32,
    fade: f32,
}
impl PreviewVoice {
    pub fn reset(&mut self, bus: &PreviewBus) {
        bus.stop();
        self.generation = bus.generation.load(Ordering::Acquire);
        self.position = self.clip.len() as f64;
        self.last = 0.;
        self.fade = 0.;
    }
    pub fn mix(&mut self, bus: &PreviewBus, rate: f32, left: &mut [f32], right: &mut [f32]) {
        let g = bus.generation.load(Ordering::Acquire);
        if g != self.generation {
            if let Ok(mut slot) = bus.pending.try_lock() {
                if slot.0 == g {
                    std::mem::swap(&mut slot.1, &mut self.clip);
                    self.position = 0.;
                } else {
                    self.position = self.clip.len() as f64;
                }
                self.generation = g;
                self.fade = self.last;
            }
        }
        if self.position >= self.clip.len() as f64 && self.fade.abs() < 1e-5 {
            return;
        }
        let decay = (-1. / (rate * 0.002)).exp();
        for (l, r) in left.iter_mut().zip(right) {
            let mut v = self.fade;
            self.fade *= decay;
            if self.position < self.clip.len() as f64 {
                v += at(&self.clip, self.position);
                self.position += PREVIEW_RATE / rate as f64;
            }
            *l += v;
            *r += v;
            self.last = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn vowels() -> Sample {
        let rate = 16000u32;
        let samples = (0..24000)
            .map(|i| {
                let seg = i / 8000;
                let phase = (i % 8000) as f32 * [190., 220., 247.][seg] / rate as f32
                    * std::f32::consts::TAU
                    + seg as f32 * 1.1;
                0.55 * phase.sin() + 0.28 * (phase * (seg + 2) as f32).sin()
            })
            .collect();
        Sample {
            name: "Vowels".into(),
            root: 60.,
            rate,
            samples,
        }
    }
    #[test]
    fn different_pitches_align_and_build_smooth_playable_frames() {
        let s = vowels();
        let mut points: Vec<_> = [0.16, 0.50, 0.84]
            .into_iter()
            .map(|t| analyze(&s, t).unwrap())
            .collect();
        for (p, hz) in points.iter().zip([190., 220., 247.]) {
            assert!((p.hz - hz).abs() < 2., "{}", p.hz);
        }
        align(&mut points);
        for pair in points.windows(2) {
            let dot = pair[0]
                .cycle
                .iter()
                .zip(&pair[1].cycle)
                .map(|(a, b)| a * b)
                .sum::<f32>();
            assert!(dot > 120.);
        }
        let a = build(&points, Blend::Linear, "Vowels").unwrap();
        let b = build(&points, Blend::Hold, "Vowels").unwrap();
        assert_eq!(a.frames(), 21);
        assert_eq!(&a.samples[10 * SIZE..11 * SIZE], &points[1].cycle);
        assert_eq!(&b.samples[5 * SIZE..6 * SIZE], &points[0].cycle);
        assert_ne!(a.samples, b.samples);
        let silent = Sample {
            samples: vec![0.; 12000],
            ..s
        };
        assert!(analyze(&silent, 0.5).is_err());
    }
    #[test]
    fn audition_reaches_stereo_output_and_stops_cleanly() {
        let p = analyze(&vowels(), 0.5).unwrap();
        let bus = PreviewBus::default();
        let mut voice = PreviewVoice::default();
        bus.play(audition(&p.cycle, false));
        let mut l = [0.; 4096];
        let mut r = l;
        voice.mix(&bus, 48000., &mut l, &mut r);
        assert_eq!(l, r);
        assert!(l.iter().any(|v| v.abs() > 0.05));
        bus.stop();
        l.fill(0.);
        r.fill(0.);
        voice.mix(&bus, 48000., &mut l, &mut r);
        assert!(l[3500..].iter().all(|v| v.abs() < 1e-5));
    }
}
