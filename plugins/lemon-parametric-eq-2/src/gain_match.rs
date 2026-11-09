//! Temporary, bounded RMS loudness matching for listening comparisons. No allocations.
#[derive(Default)]
pub struct GainMatch {
    input: f64,
    output: f64,
    pub gain: f64,
    active: bool,
    age: usize,
}
impl GainMatch {
    pub fn reset(&mut self) {
        *self = Self {
            gain: 1.,
            ..Self::default()
        };
    }
    pub fn energy(l: &[f32], r: &[f32]) -> f64 {
        l.iter()
            .chain(r)
            .map(|&x| f64::from(x).powi(2))
            .sum::<f64>()
            / (l.len() + r.len()).max(1) as f64
    }
    pub fn process(&mut self, input: f64, l: &mut [f32], r: &mut [f32], rate: f64, enabled: bool) {
        if self.gain == 0. {
            self.gain = 1.;
        }
        if !enabled && !self.active {
            return;
        } // Default is bit-exact, including silence.
        let n = l.len().min(r.len());
        if n == 0 {
            return;
        }
        if enabled && !self.active {
            self.input = 0.;
            self.output = 0.;
            self.age = 0;
        }
        let output = Self::energy(l, r);
        if !input.is_finite() || !output.is_finite() {
            self.reset();
            return;
        }
        self.age = self.age.saturating_add(n);
        let a = 1. - (-(n as f64) / (rate * 0.4)).exp();
        self.input += (input - self.input) * a;
        self.output += (output - self.output) * a;
        // Never chase a silent input, spectral startup or a near-null wet signal.
        let target = if enabled
            && self.age > (rate * 0.4) as usize
            && self.input > 1e-6
            && self.output > 1e-6
        {
            (self.input / self.output)
                .sqrt()
                .clamp(0.2511886432, 3.981071706)
        } else {
            1.
        };
        let slew = 1. - (-1. / (rate * 0.25)).exp();
        for (l, r) in l.iter_mut().zip(r.iter_mut()) {
            self.gain += (target - self.gain) * slew;
            *l *= self.gain as f32;
            *r *= self.gain as f32;
        }
        self.active = enabled || (self.gain - 1.).abs() > 1e-6;
        if !self.active {
            self.gain = 1.;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_matches_boost_smoothly_and_leaves_disabled_audio_exact() {
        for rate in [44100., 48000., 96000.] {
            let mut m = GainMatch::default();
            let signal: Vec<_> = (0..256).map(|i| 0.1 * (i as f32 * 0.17).sin()).collect();
            let input = GainMatch::energy(&signal, &signal);
            let mut l = signal.clone();
            let mut r = l.clone();
            m.process(input, &mut l, &mut r, rate, false);
            assert_eq!(l, signal);
            for _ in 0..(rate as usize / 256 * 3) {
                let mut l: Vec<_> = signal.iter().map(|x| x * 2.).collect();
                let mut r = l.clone();
                m.process(input, &mut l, &mut r, rate, true);
                assert!(l.iter().all(|x| x.is_finite()));
            }
            assert!((m.gain - 0.5).abs() < 0.002, "{}", m.gain);
            let old = m.gain;
            let mut l = signal.clone();
            let mut r = l.clone();
            m.process(input, &mut l, &mut r, rate, false);
            assert!(m.gain > old && m.gain < old + 0.02);
            for _ in 0..(rate as usize / 256 * 5) {
                m.process(input, &mut l, &mut r, rate, false);
            }
            assert_eq!(m.gain, 1.);
        }
    }
    #[test]
    fn silence_startup_and_near_null_do_not_create_extreme_gain() {
        let mut m = GainMatch::default();
        let mut l = vec![0.; 512];
        let mut r = l.clone();
        for _ in 0..100 {
            m.process(0.1, &mut l, &mut r, 48000., true);
        }
        assert_eq!(m.gain, 1.);
        m.reset();
        m.process(f64::NAN, &mut l, &mut r, 48000., true);
        assert_eq!(m.gain, 1.);
        for _ in 0..100 {
            l.fill(1e-9);
            r.fill(1e-9);
            m.process(0., &mut l, &mut r, 48000., true);
        }
        assert_eq!(m.gain, 1.);
    }
}
