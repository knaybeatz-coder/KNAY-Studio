//! Playable bridge: verified oscillator/unison laws with Lemon Wave's existing
//! envelopes, MIDI handling, modulation and effects. No whole-Serum parity claim.
use crate::reference_oscillator::{LevelSmoother, Oscillator, Table, Warp};
use std::sync::Arc;

pub struct ReferenceVoice {
    oscillators: [[Oscillator; 7]; 2],
    level: [LevelSmoother; 2],
    rate: f32,
    counts: [usize; 2],
    weights: [[[f32; 2]; 7]; 2],
    width: [f32; 2],
    blend: [f32; 2],
}
impl ReferenceVoice {
    pub fn new(table: Arc<Table>, rate: f32) -> Self {
        Self {
            oscillators: std::array::from_fn(|_| {
                std::array::from_fn(|_| Oscillator::new(table.clone(), rate))
            }),
            level: std::array::from_fn(|_| LevelSmoother::new(rate, 0.)),
            rate,
            counts: [0; 2],
            weights: [[[0.; 2]; 7]; 2],
            width: [-1.; 2],
            blend: [-1.; 2],
        }
    }
    pub fn reset(&mut self, levels: [f32; 2], phases: [[f64; 7]; 2]) {
        for o in 0..2 {
            for (u, osc) in self.oscillators[o].iter_mut().enumerate() {
                osc.reset(phases[o][u]);
            }
            self.level[o] = LevelSmoother::new(self.rate, levels[o]);
        }
        self.counts = [0; 2];
    }
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        o: usize,
        frequency: f64,
        count: usize,
        detune_cents: f32,
        width: f32,
        blend: f32,
        pan: f32,
        warp: Warp,
        amount: f32,
        level: f32,
    ) -> [f32; 2] {
        let count = count.clamp(1, 7);
        if self.counts[o] != count || self.width[o] != width || self.blend[o] != blend {
            // New unison voices start deterministically. The shared Serum RNG
            // draw order has not been reconstructed and is not claimed here.
            if count > self.counts[o] && self.counts[o] > 0 {
                for u in self.counts[o]..count {
                    self.oscillators[o][u].reset(0.);
                }
            }
            self.counts[o] = count;
            self.width[o] = width;
            self.blend[o] = blend;
            let l = (1. - width + 1.4158915) * (1. - width);
            let r = (width + 1.4158915) * width;
            let b = blend.clamp(0., 1.).powi(2);
            let outer = (-0.73764 * b + 1.2841) * b + 0.044372;
            let center = 0.99785 - 0.55366 * b;
            let gain = if count == 1 {
                1.
            } else {
                0.5 * (-0.028782314 * count as f32).exp()
            };
            let mut side = 0;
            for u in 0..count {
                self.weights[o][u] = if count == 1 {
                    [1.; 2]
                } else if count % 2 == 1 && u == count / 2 {
                    [center * 0.95794576 * gain; 2]
                } else {
                    let g = if count == 2 {
                        0.707946
                    } else if count % 2 == 0 && (u == count / 2 || u + 1 == count / 2) {
                        center
                    } else {
                        outer
                    };
                    let pair = if side % 2 == 0 { [l, r] } else { [r, l] };
                    side += 1;
                    pair.map(|v| v * g * gain)
                };
            }
        }
        let gains = self.level[o].tick(level);
        let mut out = [0.; 2];
        for u in 0..count {
            let offset = if count == 1 {
                0.
            } else {
                2. * u as f64 / (count - 1) as f64 - 1.
            };
            let hz = frequency * 2_f64.powf(offset * detune_cents as f64 / 1200.);
            let sample = self.oscillators[o][u].tick(hz, warp, amount, gains);
            // Avoid folded fundamentals beyond the output Nyquist limit.
            let attenuation =
                ((self.rate as f64 * 0.5 - hz) / (self.rate as f64 * 0.05)).clamp(0., 1.) as f32;
            for ch in 0..2 {
                out[ch] += sample * self.weights[o][u][ch] * attenuation;
            }
        }
        let p = pan.clamp(0., 1.);
        let q = (1. - p) * p * 0.41589156;
        let center = 0.5 + 0.25 * 0.41589156;
        [out[0] * (1. - p + q) / center, out[1] * (p + q) / center]
    }
}
