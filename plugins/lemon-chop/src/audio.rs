// SPDX-License-Identifier: GPL-3.0-or-later
//! Background source analysis. No decoding or inference on the audio callback.
use os_engine::Sample;
use std::{path::Path, sync::Arc};
pub struct Audio {
    pub path: String,
    pub source: Arc<Sample>,
    pub stems: [Option<Arc<Sample>>; 4],
    pub peaks: Vec<(f32, f32)>,
    pub onsets: Vec<f64>,
    pub bpm: f32,
    pub key: String,
}
impl Audio {
    pub fn load(path: &str, stems: &[String; 4]) -> Result<Self, String> {
        Self::load_cancellable(path, stems, &|| false)
    }
    pub fn load_cancellable(
        path: &str,
        stems: &[String; 4],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        let check = || {
            if cancelled() {
                Err("Analysis cancelled".to_string())
            } else {
                Ok(())
            }
        };
        check()?;
        let source = os_engine::sample::load_shared(Path::new(path)).map_err(|e| e.to_string())?;
        if source.frames() == 0 || source.rate == 0 {
            return Err("The audio file is empty".into());
        }
        if source
            .left
            .iter()
            .chain(&source.right)
            .any(|v| !v.is_finite())
        {
            return Err("The audio contains invalid samples".into());
        }
        let mut parts: [Option<Arc<Sample>>; 4] = Default::default();
        for (i, p) in stems.iter().enumerate() {
            check()?;
            if !p.is_empty() {
                let s = os_engine::sample::load_shared(Path::new(p)).map_err(|e| e.to_string())?;
                if s.frames() != source.frames()
                    || s.rate != source.rate
                    || s.left.iter().chain(&s.right).any(|x| !x.is_finite())
                {
                    return Err("Stem duration does not match the source".into());
                }
                parts[i] = Some(s);
            }
        }
        let hop = (source.rate as usize / 200).max(1);
        let mut envelope = Vec::new();
        let mut previous = 0f32;
        for (chunk, right) in source.left.chunks(hop).zip(source.right.chunks(hop)) {
            check()?;
            let e = (chunk
                .iter()
                .zip(right)
                .map(|(l, r)| (l * l + r * r) * 0.5)
                .sum::<f32>()
                / chunk.len() as f32)
                .sqrt();
            envelope.push((e - previous).max(0.));
            previous = e;
        }
        let peak = envelope.iter().copied().fold(0., f32::max);
        let mut onsets = vec![0.];
        for i in 1..envelope.len().saturating_sub(1) {
            let time = i as f64 * hop as f64 / source.rate as f64;
            if envelope[i] > peak * 0.12
                && envelope[i] > envelope[i - 1]
                && envelope[i] >= envelope[i + 1]
                && time - onsets.last().copied().unwrap_or(0.) > 0.075
            {
                onsets.push(time);
            }
        }
        let mut best = (0f64, 120f32);
        let env = &envelope[..envelope.len().min(24000)];
        for tenth in 700..1801 {
            check()?;
            let bpm = tenth as f32 / 10.;
            let lag = (60. * source.rate as f32 / hop as f32 / bpm).round() as usize;
            if lag >= env.len() {
                continue;
            }
            let score = env[lag..]
                .iter()
                .zip(env)
                .map(|(a, b)| (*a as f64) * (*b as f64))
                .sum::<f64>()
                / (env.len() - lag) as f64;
            let score = score * (1. - 0.03 * (bpm / 120.).log2().abs() as f64);
            if score > best.0 {
                best = (score, bpm);
            }
        }
        check()?;
        let key = estimate_key(&source);
        check()?;
        let n = source.frames();
        let mut peaks = Vec::with_capacity(4096);
        for j in 0..4096 {
            check()?;
            let a = j * n / 4096;
            let b = ((j + 1) * n / 4096).max(a + 1).min(n);
            let mut amp = 0f32;
            let mut diff = 0f32;
            let mut prev = 0.;
            for (&x, &right) in source.left[a..b].iter().zip(&source.right[a..b]) {
                amp = amp.max(x.abs()).max(right.abs());
                diff += (x - prev).abs();
                prev = x;
            }
            peaks.push((amp, (diff / (b - a) as f32 * 3.).clamp(0., 1.)));
        }
        Ok(Self {
            path: path.into(),
            source,
            stems: parts,
            peaks,
            onsets,
            bpm: best.1,
            key,
        })
    }
    pub fn mixed_peaks(
        &self,
        gains: [f32; 4],
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<(f32, f32)>> {
        if !self.stems.iter().all(Option::is_some) {
            return Some(self.peaks.clone());
        }
        let n = self.source.frames();
        let mut peaks = Vec::with_capacity(4096);
        for j in 0..4096 {
            if cancelled() {
                return None;
            }
            let a = j * n / 4096;
            let b = ((j + 1) * n / 4096).max(a + 1).min(n);
            let (mut amp, mut diff, mut prev) = (0f32, 0f32, 0f32);
            for i in a..b {
                let v = self.frame(i as f64, gains);
                amp = amp.max(v[0].abs()).max(v[1].abs());
                diff += (v[0] - prev).abs();
                prev = v[0];
            }
            peaks.push((amp, (diff / (b - a) as f32 * 3.).clamp(0., 1.)));
        }
        Some(peaks)
    }
    pub fn duration(&self) -> f64 {
        self.source.frames() as f64 / self.source.rate as f64
    }
    pub fn frame(&self, pos: f64, gains: [f32; 4]) -> [f32; 2] {
        if pos < 0. || pos >= self.source.frames() as f64 {
            return [0.; 2];
        }
        let i = pos as usize;
        let j = (i + 1).min(self.source.frames() - 1);
        let f = (pos - i as f64) as f32;
        let read = |s: &Sample| {
            [
                s.left[i] + (s.left[j] - s.left[i]) * f,
                s.right[i] + (s.right[j] - s.right[i]) * f,
            ]
        };
        if self.stems.iter().all(Option::is_some) {
            let mut out = [0.; 2];
            for (s, g) in self.stems.iter().zip(gains) {
                let v = read(s.as_ref().unwrap());
                out[0] += v[0] * g;
                out[1] += v[1] * g;
            }
            out
        } else {
            read(&self.source)
        }
    }
}
fn estimate_key(s: &Sample) -> String {
    // Chroma energy sampled over the track; approximate analysis, editable in the UI.
    let step = (s.rate as usize / 8000).max(1);
    let energy = |channel: &[f32]| {
        channel
            .iter()
            .step_by(step)
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
    };
    let channel = if energy(&s.right) > energy(&s.left) {
        &s.right
    } else {
        &s.left
    };
    let rate = s.rate as f64 / step as f64;
    let mut chroma = [0f64; 12];
    let window = 2048;
    for section in 0..8 {
        let start = section * s.frames().saturating_sub(window * step) / 8;
        for midi in 36..84 {
            let hz = 440. * 2f64.powf((midi as f64 - 69.) / 12.);
            let coeff = 2. * (std::f64::consts::TAU * hz / rate).cos();
            let (mut a, mut b) = (0., 0.);
            for i in 0..window {
                let index = start + i * step;
                if index >= s.frames() {
                    break;
                }
                let x = channel[index] as f64
                    * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / window as f64).cos());
                let v = x + coeff * a - b;
                b = a;
                a = v;
            }
            chroma[midi % 12] += (a * a + b * b - coeff * a * b).max(0.);
        }
    }
    let major = [
        6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
    ];
    let minor = [
        6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
    ];
    let mut best = (0., 0, false);
    for root in 0..12 {
        for (profile, min) in [(major, false), (minor, true)] {
            let score = (0..12)
                .map(|i| chroma[(root + i) % 12] * profile[i])
                .sum::<f64>();
            if score > best.0 {
                best = (score, root, min);
            }
        }
    }
    if best.0 < 1e-8 {
        return "Unknown".into();
    }
    format!(
        "{} {}",
        [
            "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"
        ][best.1],
        if best.2 { "min" } else { "maj" }
    )
}
