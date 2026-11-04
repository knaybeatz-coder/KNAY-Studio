//! Evidence-backed oscillator core used by Lemon Wave Reference Saw.
//! See research/oscillator-reference.md for the measured scope and limitations.
use std::sync::Arc;

const COEFF: &[u8; 65536] = include_bytes!("reference_interpolation.bin");
fn coefficient(delta: bool, row: usize, tap: usize) -> f32 {
    let at = ((usize::from(delta) * 512 + row) * 16 + tap) * 4;
    f32::from_le_bytes(COEFF[at..at + 4].try_into().unwrap())
}

/// Spectrally truncated, successively halved periodic tables. Construct off the
/// audio thread; sampling does not allocate or lock.
pub struct Table {
    levels: Vec<Vec<f32>>,
    exponent: i32,
}
impl Table {
    pub fn new(samples: &[f32]) -> Result<Self, &'static str> {
        let n = samples.len();
        if !(16..=4096).contains(&n)
            || !n.is_power_of_two()
            || samples.iter().any(|x| !x.is_finite())
        {
            return Err("Expected 16..4096 finite samples in a power-of-two cycle");
        }
        let mut re: Vec<_> = samples.iter().map(|&x| x as f64).collect();
        let mut im = vec![0.; n];
        crate::wavetable::fft(&mut re, &mut im, false);
        re[n / 2] = 0.;
        im[n / 2] = 0.;
        let mut levels = vec![samples.to_vec()];
        for level in 1..=n.trailing_zeros() as usize {
            let cutoff = n >> (level + 1);
            let (mut r, mut i) = (re.clone(), im.clone());
            for k in cutoff + 1..n - cutoff {
                r[k] = 0.;
                i[k] = 0.;
            }
            crate::wavetable::fft(&mut r, &mut i, true);
            levels.push(
                r.iter()
                    .step_by(1 << level)
                    .map(|x| (x / n as f64) as f32)
                    .collect(),
            );
        }
        Ok(Self {
            levels,
            exponent: n.trailing_zeros() as i32,
        })
    }
    /// Analytic Init saw inferred from measured output; no vendor WAV asset.
    pub fn init_saw() -> Self {
        let samples: Vec<_> = (0..2048)
            .map(|i| ((i + 1024) % 2048) as f32 / 1024. - 1.)
            .collect();
        Self::new(&samples).unwrap()
    }
    fn level(&self, increment: f64) -> usize {
        let bits = (increment as f32).to_bits();
        let level =
            ((bits >> 23) & 255) as i32 - i32::from(bits & 0x7fffff == 0) + self.exponent - 126;
        level.clamp(0, (self.levels.len() - 1).min(11) as i32) as usize
    }
    fn sinc(&self, phase: f64, level: usize) -> f32 {
        let table = &self.levels[level];
        let u = phase * table.len() as f64 * 512.;
        let integer = u as usize;
        let fraction = (u - integer as f64) as f32;
        let row = integer & 511;
        let index = integer >> 9;
        let mut lanes = [0.; 4];
        for (lane, sum) in lanes.iter_mut().enumerate() {
            for group in 0..4 {
                let tap = lane + group * 4;
                let sample = table[index.wrapping_add(tap).wrapping_sub(7) & (table.len() - 1)];
                let product = sample
                    * (coefficient(false, row, tap) + fraction * coefficient(true, row, tap));
                if group == 0 {
                    *sum = product;
                } else {
                    *sum += product;
                }
            }
        }
        (lanes[0] + lanes[2]) + (lanes[1] + lanes[3])
    }
    fn linear(&self, phase: f64) -> f32 {
        let table = &self.levels[0];
        let u = phase * table.len() as f64;
        let i = u as usize;
        (table[i] as f64 + (table[(i + 1) & (table.len() - 1)] - table[i]) as f64 * (u - i as f64))
            as f32
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Warp {
    Off,
    BendPlus,
    BendMinus,
    BendBoth,
    Pwm,
    AsymPlus,
    AsymMinus,
    AsymBoth,
    Flip,
    Mirror,
    Quantize,
}
impl Warp {
    pub fn from_reference_id(id: u32) -> Option<Self> {
        Some(match id {
            0 => Self::Off,
            2 => Self::BendPlus,
            3 => Self::BendMinus,
            4 => Self::BendBoth,
            5 => Self::Pwm,
            6 => Self::AsymPlus,
            7 => Self::AsymMinus,
            8 => Self::AsymBoth,
            9 => Self::Flip,
            10 => Self::Mirror,
            15 => Self::Quantize,
            _ => return None,
        })
    }
    pub(crate) fn apply(self, mut x: f64, amount: f32) -> (f64, f32) {
        let a = amount.clamp(0., 1.);
        let p = match self {
            Self::BendPlus | Self::AsymPlus => 0.5 - 0.4 * a,
            Self::BendMinus | Self::AsymMinus => 0.5 + 0.4 * a,
            Self::BendBoth | Self::AsymBoth => 0.1 + 0.8 * a,
            Self::Pwm => 1. - a,
            Self::Flip => 2. * a,
            Self::Mirror => 0.05 + 0.9 * a,
            Self::Quantize => ((1. - a) as f64).powi(5) as f32,
            _ => a,
        };
        let d = p as f64;
        let mut sign = 1.;
        match self {
            Self::BendPlus | Self::BendMinus | Self::BendBoth => {
                if x < 0.5 {
                    let q = 2. * p - 1.;
                    x = x * d / ((q + q) as f64 * x - d + 1.);
                } else {
                    let t = 2. * x - 1.;
                    x = (((3. * p - 2.) as f64 * t - d) * 0.5) / ((2. * p - 1.) as f64 * t - d);
                }
            }
            Self::Pwm => x = if x < d { x / d } else { 0. },
            Self::AsymPlus | Self::AsymMinus | Self::AsymBoth => {
                x = if x < d {
                    x * 0.5 / d
                } else {
                    ((d - x) / (p - 1.) as f64 + 1.) * 0.5
                }
            }
            Self::Flip => {
                if (p < 1. && x < d) || (p >= 1. && x > d - 1.) {
                    sign = -1.;
                }
            }
            Self::Mirror => {
                x = x.min(1. - x);
                x = if x < d * 0.5 {
                    x / d
                } else {
                    ((d - 2. * x) / (p - 1.) as f64 + 1.) * 0.5
                };
            }
            Self::Quantize => {
                if p != 1. {
                    let steps = (p * 1022. + 2.) as f64;
                    x = (x * steps + 0.5).floor() / steps;
                }
            }
            Self::Off => {}
        }
        (x - x.floor(), sign)
    }
}

#[derive(Clone, Copy, Default)]
pub struct Decimator {
    state: [[f32; 4]; 4],
}
impl Decimator {
    pub fn tick(&mut self, pair: [f32; 2]) -> f32 {
        const C: [[u32; 4]; 3] = [
            [0x3dd83bec, 0x3ce41212, 0x3eb2321d, 0x3e5f9c71],
            [0x3f199d0b, 0x3ef51d96, 0x3f4cfe8d, 0x3f35161e],
            [0x3f7604c1, 0x3f622cce, 0, 0],
        ];
        let mut x = [pair[0], pair[1], self.state[3][0], self.state[3][1]];
        let mut prev = self.state[0];
        self.state[0] = x;
        for (j, coeff) in C.iter().enumerate() {
            let old = self.state[j + 1];
            for k in 0..4 {
                x[k] = (x[k] - old[k]) * f32::from_bits(coeff[k]) + prev[k];
            }
            self.state[j + 1] = x;
            prev = old;
        }
        (x[2] + x[3]) * 0.5
    }
}

/// The observed upstream control ramp and local oscillator level ramp.
pub struct LevelSmoother {
    value: f64,
    gain: f32,
    alpha: f64,
}
impl LevelSmoother {
    pub fn new(rate: f32, initial: f32) -> Self {
        assert!(rate.is_finite() && rate >= 8000.);
        let initial = initial.clamp(0., 1.);
        Self {
            value: initial as f64,
            gain: initial * initial,
            alpha: (1. / (rate as f64 / 44100. * 1500. + 1.)) as f32 as f64,
        }
    }
    pub fn tick(&mut self, target: f32) -> [f32; 2] {
        let target = target.clamp(0., 1.) as f64;
        self.value += (target - self.value) * self.alpha;
        if (target - self.value).abs() < 0.001 {
            self.value = target;
        }
        let t = self.value as f32;
        let t = t * t;
        std::array::from_fn(|_| {
            self.gain += (t - self.gain) * 0.05;
            self.gain
        })
    }
}

/// Mono oscillator at2x, supporting the recovered fixed-table warp subset.
/// Voice allocation, envelopes, modulation and stereo unison belong to caller.
pub struct Oscillator {
    table: Arc<Table>,
    phase: f64,
    reciprocal: f64,
    level: usize,
    old_level: usize,
    remaining: u32,
    decimator: Decimator,
}
impl Oscillator {
    pub fn new(table: Arc<Table>, rate: f32) -> Self {
        assert!(rate.is_finite() && (8000.0..=384000.0).contains(&rate));
        Self {
            table,
            phase: 0.,
            reciprocal: (1_f32 / (rate * 2.)) as f64,
            level: usize::MAX,
            old_level: 0,
            remaining: 0,
            decimator: Decimator::default(),
        }
    }
    pub fn reset(&mut self, phase: f64) {
        self.phase = phase.rem_euclid(1.);
        self.level = usize::MAX;
        self.remaining = 0;
        self.decimator = Decimator::default();
    }
    pub fn tick(&mut self, frequency: f64, warp: Warp, amount: f32, gains: [f32; 2]) -> f32 {
        let pair = self.tick_pair(frequency, warp, amount, gains);
        self.decimator.tick(pair)
    }
    /// Oversampled output for a voice-level filter/envelope chain before decimation.
    pub fn tick_pair(
        &mut self,
        frequency: f64,
        warp: Warp,
        amount: f32,
        gains: [f32; 2],
    ) -> [f32; 2] {
        let increment = (frequency * self.reciprocal).clamp(f64::MIN_POSITIVE, 1.);
        let new = self.table.level(increment);
        if self.level == usize::MAX {
            self.level = new;
            self.old_level = new;
        } else if self.remaining == 0 && new != self.level {
            self.old_level = self.level;
            self.level = new;
            self.remaining = 64;
        }
        let mut pair = [0.; 2];
        for i in 0..2 {
            let (phase, sign) = warp.apply(self.phase, amount);
            let mut value = if matches!(warp, Warp::Pwm) {
                self.table.linear(phase)
            } else {
                self.table.sinc(phase, self.level)
            };
            if self.remaining > 0 && !matches!(warp, Warp::Pwm) {
                let previous = self.table.sinc(phase, self.old_level);
                value += (previous - value) * self.remaining as f32 * (1. / 64.);
                self.remaining -= 1;
            }
            pair[i] = value * sign * gains[i];
            self.phase += increment;
            if self.phase >= 1. {
                self.phase -= 1.;
            }
        }
        pair
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_recorded_serum_warps_and_level_changes() {
        let fixtures = include_bytes!("../research/oscillator-fixtures.bin");
        let table = Arc::new(Table::init_saw());
        let cases = [
            (0, 0., 48000, 96000, false),
            (2, 0.75, 48000, 96000, false),
            (5, 0.5, 48000, 96000, false),
            (10, 0.25, 48000, 96000, false),
            (15, 0.75, 48000, 96000, false),
            (0, 0., 44100, 44356, true),
            (0, 0., 48000, 48256, true),
            (0, 0., 96000, 96256, true),
        ];
        for (case, (mode, amount, rate, start, step)) in cases.into_iter().enumerate() {
            let mut osc = Oscillator::new(table.clone(), rate as f32);
            let mut smoother = LevelSmoother::new(rate as f32, if step { 1. } else { 0.75 });
            for frame in 0..start + 256 {
                let level = if step {
                    if frame >= rate { 0.5 } else { 1. }
                } else {
                    0.75
                };
                let y = osc.tick(
                    440.,
                    Warp::from_reference_id(mode).unwrap(),
                    amount,
                    smoother.tick(level),
                ) * std::f32::consts::SQRT_2
                    * 0.25
                    * 0.25;
                if frame >= start {
                    let at = (case * 256 + (frame - start) as usize) * 4;
                    let reference = f32::from_le_bytes(fixtures[at..at + 4].try_into().unwrap());
                    assert!(
                        (y - reference).abs() < 1e-6,
                        "case{case} frame{frame}: {y} vs {reference}"
                    );
                }
            }
        }
    }
    #[test]
    fn rejects_invalid_tables() {
        assert!(Table::new(&[0.; 15]).is_err());
        assert!(Table::new(&[f32::NAN; 16]).is_err());
        assert!(Table::new(&[0.; 8192]).is_err());
    }
}
