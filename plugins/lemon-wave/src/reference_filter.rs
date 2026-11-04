// SPDX-License-Identifier: GPL-3.0-or-later
//! Recovered nonlinear state-variable filter paths (12/24 dB LP/BP/HP).
//! Coefficients and operation order follow 0x1802fcc90 / 0x1802f7510.
#[derive(Clone, Copy, Default)]
struct Section {
    low: f32,
    band: f32,
    high: f32,
}
fn saturate(x: f32) -> f32 {
    let x = x.clamp(-1.5, 1.5);
    x * (-0.14814815) * x * x + x
}
impl Section {
    fn tick(&mut self, x: f32, g: f32, k: f32, mode: usize, nonlinear: bool) -> f32 {
        let prior = match mode {
            1 => self.low,
            2 => self.band,
            _ => self.high,
        } * 0.25;
        self.low = g * self.band + self.low;
        let low_mid = self.low;
        let high_mid = (x - k * self.band) - self.low;
        let delta = high_mid * g;
        self.band += if nonlinear {
            delta.clamp(-0.8333333, 0.8333333)
        } else {
            delta
        };
        if nonlinear {
            self.band = self.band - self.band * (g * 0.5) * self.band * self.band;
        }
        let band_mid = self.band;
        let x = x - k * self.band;
        self.low = g * self.band + self.low;
        self.high = x - self.low;
        let delta = g * self.high;
        self.band += if nonlinear {
            delta.clamp(-0.8333333, 0.8333333)
        } else {
            delta
        };
        if nonlinear {
            self.band = self.band - (g * 0.5) * self.band * self.band * self.band;
        }
        match mode {
            1 => self.low * 0.25 + low_mid * 0.5 + prior,
            2 => self.band * 0.25 + band_mid * 0.5 + prior,
            _ => self.high * 0.25 + high_mid * 0.5 + prior,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct Filter {
    sections: [Section; 2],
}
#[derive(Clone, Copy)]
pub struct Coefficients {
    g: f32,
    k: f32,
    k2: f32,
    gain: f32,
    drive: f32,
    pub mode: usize,
    pub poles: usize,
}
impl Coefficients {
    pub fn new(
        rate: f32,
        cutoff: f64,
        resonance: f64,
        drive: f32,
        mode: usize,
        poles: usize,
    ) -> Self {
        let ratio = (cutoff / rate as f64).clamp(0., 0.5);
        let gd = 2. * (ratio * 0.5 * std::f64::consts::PI).sin();
        let maximum = (2. / gd.max(1e-12) - gd * 0.5).clamp(0., 2.);
        let k = (2. * (1. - resonance.clamp(0., 1.).sqrt().sqrt())).min(maximum) as f32;
        Self {
            g: gd as f32,
            k,
            k2: ((k * k) as f64 * 0.25 + 1.).min(maximum) as f32,
            gain: (1. / (resonance.clamp(0., 1.) * 4. + 1.)) as f32,
            drive: (1. + drive.clamp(0., 1.)).powi(4),
            mode,
            poles,
        }
    }
}
impl Filter {
    pub fn tick(&mut self, input: f32, c: &Coefficients) -> f32 {
        if c.mode == 0 {
            return input;
        }
        let x = saturate(input * c.drive);
        let first = self.sections[0].tick(x * c.gain, c.g, c.k, c.mode, true);
        if c.poles == 4 {
            self.sections[1].tick(saturate(first), c.g, c.k2, c.mode, false)
        } else {
            first
        }
    }
}

/// Lemon routing: linear 2x interpolation and recovered decimator around the core.
/// This is not a claim of complete Serum voice-bus equivalence.
#[derive(Clone, Copy, Default)]
pub struct Oversampled {
    previous: f32,
    core: Filter,
    decimator: crate::reference_oscillator::Decimator,
}
impl Oversampled {
    pub fn tick(&mut self, x: f32, c: &Coefficients) -> f32 {
        if c.mode == 0 {
            self.previous = x;
            return x;
        }
        let pair = [(self.previous + x) * 0.5, x];
        self.previous = x;
        self.decimator.tick(pair.map(|v| self.core.tick(v, c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extreme_settings_remain_finite_and_modes_are_distinct() {
        let mut energies = Vec::new();
        for rate in [8000., 44100., 48000., 96000., 192000.] {
            for cutoff in [20., 400., 4000., 20000.] {
                for res in [0., 0.5, 0.99, 1.] {
                    for drive in [0., 1.] {
                        for mode in 1..=3 {
                            for poles in [2, 4] {
                                let c = Coefficients::new(
                                    rate * 2.,
                                    (cutoff as f64).min(rate as f64 * 0.42),
                                    res,
                                    drive,
                                    mode,
                                    poles,
                                );
                                let mut f = Oversampled::default();
                                let mut energy = 0.;
                                for i in 0..4096 {
                                    let x = if i < 2048 {
                                        (i as f32 * 0.071).sin() * 2.
                                    } else {
                                        0.
                                    };
                                    let y = f.tick(x, &c);
                                    assert!(
                                        y.is_finite() && y.abs() < 100.,
                                        "{rate} {cutoff} {res} {drive} {mode} {poles} {y}"
                                    );
                                    energy += y * y;
                                }
                                energies.push(energy);
                            }
                        }
                    }
                }
            }
        }
        assert!(energies.windows(2).any(|v| (v[0] - v[1]).abs() > 0.01));
    }
}
