// SPDX-License-Identifier: GPL-3.0-or-later
//! Recorded-note playback. Assets are embedded; no disk access in the audio callback.
use crate::wavetable::MAX_FILE;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub name: String,
    pub rate: u32,
    pub root: f32,
    pub samples: Vec<f32>,
}
impl Sample {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.len() > 256
            || !(8000..=192000).contains(&self.rate)
            || !self.root.is_finite()
            || !(0.0..=127.0).contains(&self.root)
            || self.samples.len() < 2
            || self.samples.len() > self.rate as usize * 12
            || self.samples.len() > 2_304_000
            || self
                .samples
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 1.001)
        {
            return Err("Invalid sample (maximum 12 seconds)".into());
        }
        Ok(())
    }
    pub fn import(path: &std::path::Path, root: f32) -> Result<Self, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take((MAX_FILE + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::decode(
            &bytes,
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(100)
                .collect(),
            root,
        )
    }
    pub fn decode(bytes: &[u8], name: String, root: f32) -> Result<Self, String> {
        let err = |s: &str| Err(s.to_string());
        if bytes.len() > MAX_FILE {
            return err("WAV exceeds 16 MB");
        }
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return err("Choose a RIFF WAV file (PCM or 32-bit float)");
        }
        let u16_at = |b: &[u8], i| u16::from_le_bytes(b[i..i + 2].try_into().unwrap());
        let u32_at = |b: &[u8], i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let end = u32_at(bytes, 4) as usize + 8;
        if end > bytes.len() || end < 12 {
            return err("Truncated WAV header");
        }
        let (mut fmt, mut data) = (None, None);
        let mut p = 12;
        while p + 8 <= end {
            let len = u32_at(bytes, p + 4) as usize;
            let stop = p
                .checked_add(8)
                .and_then(|p| p.checked_add(len))
                .ok_or("WAV chunk overflow")?;
            if stop > end {
                return err("Truncated WAV chunk");
            }
            match &bytes[p..p + 4] {
                b"fmt " => fmt = Some(&bytes[p + 8..stop]),
                b"data" => data = Some(&bytes[p + 8..stop]),
                _ => {}
            }
            p = stop + len % 2;
        }
        let fmt = fmt.ok_or("WAV has no format chunk")?;
        let data = data.ok_or("WAV has no audio chunk")?;
        if fmt.len() < 16 {
            return err("Truncated WAV format");
        }
        let (mut kind, channels, align, bits) = (
            u16_at(fmt, 0),
            u16_at(fmt, 2) as usize,
            u16_at(fmt, 12) as usize,
            u16_at(fmt, 14),
        );
        if kind == 0xfffe {
            if fmt.len() < 40
                || u16_at(fmt, 16) < 22
                || u16_at(fmt, 18) != bits
                || fmt[26..40] != [0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
            {
                return err("Unsupported extensible WAV format");
            }
            kind = u16_at(fmt, 24);
        }
        if !(channels == 1 || channels == 2)
            || !((kind == 1 && [8, 16, 24, 32].contains(&bits)) || (kind == 3 && bits == 32))
            || align != channels * (bits as usize / 8)
            || u32_at(fmt, 4) == 0
        {
            return err("Use mono/stereo PCM 8/16/24/32-bit or float32 WAV");
        }
        if data.len() % align != 0 {
            return err("Incomplete WAV sample");
        }

        let mut samples = Vec::with_capacity(data.len() / align);
        for sample in data.chunks_exact(align) {
            let mut mono = 0.;
            for b in sample.chunks_exact(bits as usize / 8) {
                let value = match (kind, bits) {
                    (3, 32) => f32::from_le_bytes(b.try_into().unwrap()) as f64,
                    (_, 8) => (b[0] as f64 - 128.) / 128.,
                    (_, 16) => i16::from_le_bytes(b.try_into().unwrap()) as f64 / 32768.,
                    (_, 24) => {
                        ((i32::from_le_bytes([b[0], b[1], b[2], 0]) << 8) >> 8) as f64 / 8388608.
                    }
                    (_, 32) => i32::from_le_bytes(b.try_into().unwrap()) as f64 / 2147483648.,
                    _ => unreachable!(),
                };
                if !value.is_finite() || value.abs() > 16. {
                    return err("WAV contains invalid or excessive sample values");
                }
                mono += value / channels as f64;
            }
            samples.push(mono as f32);
        }

        let peak = samples.iter().fold(1.0_f32, |a, x| a.max(x.abs()));
        for x in &mut samples {
            *x /= peak;
        }
        let source = Self {
            name,
            rate: u32_at(fmt, 4),
            root,
            samples,
        };
        source.validate()?;
        Ok(source)
    }
}
pub struct Playback {
    pub source: Sample,
    levels: Vec<Vec<f32>>,
}
impl Playback {
    pub fn compile(source: Sample) -> Result<Self, String> {
        source.validate()?;
        let mut levels = vec![source.samples.clone()];
        // Windowed-sinc half-band decimation prepares antialiased octave levels off the audio thread.
        let taps: Vec<f64> = (-15..=15)
            .map(|i| {
                let x = i as f64;
                let h = if i == 0 {
                    0.45
                } else {
                    (std::f64::consts::PI * 0.45 * x).sin() / (std::f64::consts::PI * x)
                };
                h * (0.5 + 0.5 * (std::f64::consts::PI * x / 16.).cos())
            })
            .collect();
        let sum: f64 = taps.iter().sum();
        for _ in 0..14 {
            let prev = levels.last().unwrap();
            let next = (0..prev.len().div_ceil(2))
                .map(|i| {
                    taps.iter()
                        .enumerate()
                        .map(|(j, h)| {
                            let n = i as isize * 2 + j as isize - 15;
                            prev.get(n as usize).copied().unwrap_or(0.) as f64 * h / sum
                        })
                        .sum::<f64>() as f32
                })
                .collect();
            levels.push(next);
        }
        Ok(Self { source, levels })
    }
    pub fn tick(&self, cursor: &mut f64, frequency: f32, rate: f32) -> f32 {
        if *cursor >= self.source.samples.len() as f64 {
            return 0.;
        }
        let root = 440. * 2.0_f64.powf((self.source.root as f64 - 69.) / 12.);
        let step = self.source.rate as f64 / rate as f64 * frequency as f64 / root;
        let octave = step.max(1.).log2().clamp(0., 14.);
        let level = octave.floor() as usize;
        let read = |level: usize| {
            let pos = *cursor / (1u32 << level) as f64;
            let i = pos as usize;
            let t = pos.fract() as f32;
            let data = &self.levels[level];
            let a = data.get(i).copied().unwrap_or(0.);
            let b = data.get(i + 1).copied().unwrap_or(0.);
            a + (b - a) * t
        };
        let a = read(level);
        let value = a + (read((level + 1).min(14)) - a) * octave.fract() as f32;
        // A short edge fade suppresses clicks in recordings without a natural zero crossing.
        let fade = (*cursor / 32.)
            .min((self.source.samples.len() as f64 - *cursor) / 32.)
            .clamp(0., 1.) as f32;
        *cursor += step;
        value * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_pitch_duration_antialias_and_validation() {
        let source = Sample {
            name: "A4".into(),
            rate: 48000,
            root: 69.,
            samples: (0..48000)
                .map(|i| (std::f32::consts::TAU * 440. * i as f32 / 48000.).sin() * 0.5)
                .collect(),
        };
        let p = Playback::compile(source.clone()).unwrap();
        let mut cursor = 0.;
        let output: Vec<_> = (0..24000)
            .map(|_| p.tick(&mut cursor, 880., 48000.))
            .collect();
        assert!((cursor - 48000.).abs() < 0.01);
        assert_eq!(p.tick(&mut cursor, 880., 48000.), 0.);
        let crossings = output
            .windows(2)
            .filter(|w| w[0] <= 0. && w[1] > 0.)
            .count();
        assert!((crossings as i32 - 440).abs() < 3);
        let mut bad = source.clone();
        bad.root = f32::NAN;
        assert!(bad.validate().is_err());
        bad = source.clone();
        bad.samples[0] = f32::INFINITY;
        assert!(bad.validate().is_err());
        let high = Sample {
            samples: (0..48000)
                .map(|i| (std::f32::consts::TAU * 18000. * i as f32 / 48000.).sin() * 0.5)
                .collect(),
            ..source
        };
        let high = Playback::compile(high).unwrap();
        let mut c = 0.;
        let energy = (0..20000)
            .map(|_| high.tick(&mut c, 880., 48000.).powi(2))
            .sum::<f32>()
            / 20000.;
        assert!(energy < 0.001, "antialias energy {energy}");
    }
}
