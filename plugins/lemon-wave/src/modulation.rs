// SPDX-License-Identifier: GPL-3.0-or-later
//! Original Lemon modulation routing and drawn LFO curves, not a full Serum model.
use crate::controls::*;
use std::sync::atomic::{AtomicU32, Ordering};
pub const FIELD: &str = "lemon-wave-lfo-curves-v1";
pub const POINTS: usize = 65;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curves {
    pub samples: [[f32; POINTS]; 4],
}
impl Default for Curves {
    fn default() -> Self {
        Self {
            samples: std::array::from_fn(|_| {
                std::array::from_fn(|i| {
                    (std::f32::consts::TAU * i as f32 / (POINTS - 1) as f32).sin()
                })
            }),
        }
    }
}
impl Curves {
    pub fn sample(&self, lfo: usize, phase: f32) -> f32 {
        let x = phase.clamp(0., 1.) * (POINTS - 1) as f32;
        let i = (x as usize).min(POINTS - 2);
        let t = x - i as f32;
        self.samples[lfo][i] * (1. - t) + self.samples[lfo][i + 1] * t
    }
    pub fn encode(&self) -> String {
        serde_json::to_string(&self.samples.map(|x| x.to_vec())).unwrap()
    }
    pub fn decode(s: &str) -> Result<Self, String> {
        if s.len() > 16384 {
            return Err("LFO curve state exceeds size limit".into());
        }
        let v: Vec<Vec<f32>> = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if v.len() != 4
            || v.iter()
                .any(|v| v.len() != POINTS || v.iter().any(|x| !x.is_finite() || x.abs() > 1.))
        {
            return Err("Invalid LFO points".into());
        }
        Ok(Self {
            samples: std::array::from_fn(|o| std::array::from_fn(|i| v[o][i])),
        })
    }
    pub fn draw(&mut self, lfo: usize, from: (f32, f32), to: (f32, f32)) {
        let a = (from.0.clamp(0., 1.) * (POINTS - 1) as f32).round() as usize;
        let b = (to.0.clamp(0., 1.) * (POINTS - 1) as f32).round() as usize;
        for i in a.min(b)..=a.max(b) {
            let t = if a == b {
                1.
            } else {
                (i as f32 - a as f32) / (b as f32 - a as f32)
            };
            self.samples[lfo][i] = (from.1 + (to.1 - from.1) * t).clamp(-1., 1.);
        }
    }
    pub fn preset(&mut self, lfo: usize, shape: usize) {
        self.samples[lfo] = std::array::from_fn(|i| {
            let p = i as f32 / (POINTS - 1) as f32;
            match shape {
                1 => 1. - 4. * (p - 0.5).abs(),
                2 => p * 2. - 1.,
                3 => {
                    if p < 0.5 {
                        1.
                    } else {
                        -1.
                    }
                }
                _ => (p * std::f32::consts::TAU).sin(),
            }
        });
    }
}
pub fn rate(p: &crate::dsp::Patch, lfo: usize, tempo: f32) -> f32 {
    let c = LFO_CONTROLS[lfo];
    let sync = p[c[1]] as usize;
    if sync == 0 {
        return lfo_hz(p[c[0]]);
    }
    let beats = [16., 8., 4., 2., 1., 0.5, 0.25, 0.125][sync.saturating_sub(1).min(7)];
    let multiplier = match p[c[3]] as usize {
        1 => 2. / 3.,
        2 => 1.5,
        _ => 1.,
    };
    tempo.clamp(20., 400.) / 60. / beats * multiplier
}
pub fn advance(phase: f32, increment: f32, mode: usize) -> f32 {
    if mode == 2 {
        (phase + increment).min(1.)
    } else {
        (phase + increment).fract()
    }
}
#[derive(Clone, Copy, Default)]
pub struct Live {
    pub phases: [f32; 4],
    pub sources: [f32; 13],
    pub amounts: [f32; 21],
    pub env_elapsed: [f32; 2],
    pub env_stage: [u8; 2],
    pub active: bool,
}
pub struct Telemetry {
    values: [AtomicU32; 43],
}
impl Default for Telemetry {
    fn default() -> Self {
        Self {
            values: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}
impl Telemetry {
    pub fn publish(&self, v: Live) {
        let mut a = [0.; 43];
        a[..4].copy_from_slice(&v.phases);
        a[4..17].copy_from_slice(&v.sources);
        a[17..38].copy_from_slice(&v.amounts);
        a[38..40].copy_from_slice(&v.env_elapsed);
        a[40] = v.env_stage[0] as f32;
        a[41] = v.env_stage[1] as f32;
        a[42] = if v.active { 1. } else { 0. };
        for (x, y) in self.values.iter().zip(a) {
            x.store(y.to_bits(), Ordering::Relaxed);
        }
    }
    pub fn read(&self) -> Live {
        let a = self
            .values
            .each_ref()
            .map(|x| f32::from_bits(x.load(Ordering::Relaxed)));
        Live {
            phases: a[..4].try_into().unwrap(),
            sources: a[4..17].try_into().unwrap(),
            amounts: a[17..38].try_into().unwrap(),
            env_elapsed: a[38..40].try_into().unwrap(),
            env_stage: [a[40] as u8, a[41] as u8],
            active: a[42] > 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curves_validate_roundtrip_interpolate_and_draw_both_directions() {
        let mut c = Curves::default();
        c.draw(3, (0., -1.), (1., 1.));
        assert!(c.sample(3, 0.5).abs() < 1e-7);
        assert_eq!(c.sample(3, 1.), 1.);
        c.draw(2, (1., -1.), (0., 1.));
        assert_eq!(c.samples[2][0], 1.);
        assert_eq!(c.samples[2][64], -1.);
        assert_eq!(c, Curves::decode(&c.encode()).unwrap());
        assert!(Curves::decode("[]").is_err());
        assert!(Curves::decode(&" ".repeat(16385)).is_err());
        let mut bad = c.samples.map(|x| x.to_vec());
        bad[0][4] = 2.;
        assert!(Curves::decode(&serde_json::to_string(&bad).unwrap()).is_err());
    }
    #[test]
    fn clocks_follow_tempo_dotted_triplet_and_one_shot() {
        let mut p = crate::dsp::defaults();
        for lfo in 0..4 {
            let c = LFO_CONTROLS[lfo];
            p[c[1]] = 5.;
            p[c[3]] = 0.;
            assert_eq!(rate(&p, lfo, 120.), 2.);
            assert_eq!(rate(&p, lfo, 90.), 1.5);
            p[c[3]] = 1.;
            assert!((rate(&p, lfo, 120.) - 4. / 3.).abs() < 1e-6);
            p[c[3]] = 2.;
            assert_eq!(rate(&p, lfo, 120.), 3.);
        }
        assert_eq!(advance(0.9, 0.2, 2), 1.);
        assert!((advance(0.9, 0.2, 0) - 0.1).abs() < 1e-6);
    }
}
