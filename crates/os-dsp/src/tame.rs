//! Original causal resonance suppressor. No FFT buffering, look-ahead or audio-thread allocation.
//! Detectors see the unprocessed stereo signal; linked, bounded dynamic notches run in series.
const BANDS: usize = 72;
const HOP: u32 = 32;

#[derive(Clone, Debug)]
pub(crate) struct Tame {
    bands: [Band; BANDS],
    amount: f32,
    fast: f32,
    slow: f32,
    tick: u32,
    active: bool,
    detector: f32,
    attack: f32,
    release: f32,
    fast_coeff: f32,
    slow_coeff: f32,
    knob_coeff: f32,
}

impl Tame {
    pub(crate) fn new(rate: u32) -> Self {
        let rate = rate.max(1) as f32;
        let coefficient = |seconds: f32| 1. - (-1. / (rate * seconds)).exp();
        Self {
            bands: std::array::from_fn(|i| Band::new(i, rate)),
            amount: 0.,
            fast: 0.,
            slow: 0.,
            tick: 0,
            active: false,
            detector: coefficient(0.012),
            attack: coefficient(0.008),
            release: coefficient(0.120),
            fast_coeff: coefficient(0.001),
            slow_coeff: coefficient(0.030),
            knob_coeff: coefficient(0.020),
        }
    }

    pub(crate) fn process(&mut self, buffer: &mut [f32], amount: f32) {
        // Reset once after the bypass fade: no stale filter tails on reactivation.
        if amount == 0. && self.amount < 1e-5 {
            if self.active {
                for b in &mut self.bands {
                    b.reset();
                }
                self.fast = 0.;
                self.slow = 0.;
                self.tick = 0;
                self.amount = 0.;
                self.active = false;
            }
            return;
        }
        self.active = true;
        for frame in buffer.chunks_exact_mut(2) {
            self.amount += (amount - self.amount) * self.knob_coeff;
            let input = [frame[0], frame[1]];
            let power = (input[0] * input[0] + input[1] * input[1]) * 0.5;
            self.fast += (power - self.fast) * self.fast_coeff;
            self.slow += (power - self.slow) * self.slow_coeff;
            for b in &mut self.bands {
                let y = b.filter(input, false);
                let energy = (y[0] * y[0] + y[1] * y[1]) * 0.5;
                b.power += (energy - b.power) * self.detector;
            }
            if self.tick == 0 {
                self.detect();
            }
            self.tick = (self.tick + 1) % HOP;
            let mut output = input;
            for b in &mut self.bands {
                b.depth += (b.goal - b.depth)
                    * if b.goal > b.depth {
                        self.attack
                    } else {
                        self.release
                    };
                let band = b.filter(output, true);
                // 1 - depth * bandpass is a bounded notch, unlike summing many
                // independently subtracted overlapping bands from the original input.
                for c in 0..2 {
                    output[c] -= band[c] * b.depth * self.amount;
                }
            }
            frame.copy_from_slice(&output);
        }
    }

    fn detect(&mut self) {
        // Normalize each detector by its noise bandwidth so broad spectral slopes
        // don't get mistaken for resonances. Work in dB to interpolate those slopes.
        let levels = self
            .bands
            .map(|b| 10. * (b.power * b.normalization + 1e-20).log10());
        let mut cuts = [0f32; BANDS];
        let protection = (4. * self.slow / (self.fast + 1e-20)).min(1.);
        for i in 4..BANDS - 4 {
            let left = (levels[i - 4] + levels[i - 3]) * 0.5;
            let right = (levels[i + 3] + levels[i + 4]) * 0.5;
            let contrast = levels[i] - (left + right) * 0.5;
            let over = (contrast - (5. - 3. * self.amount)).max(0.);
            // Soft onset, limited reduction; stronger knob settings detect subtler peaks.
            cuts[i] = (over * over / (over + 2.) * 1.8).min(18.) * protection;
        }
        for i in 0..BANDS {
            // Limit overlapping cuts to avoid hollowing out a cluster of harmonics.
            let total: f32 = cuts[i.saturating_sub(2)..(i + 3).min(BANDS)].iter().sum();
            let budget = 12. + 12. * self.amount;
            let db = cuts[i] * (budget / total.max(budget));
            self.bands[i].goal = 1. - 10f32.powf(-db / 20.);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Band {
    b0: f32,
    a1: f32,
    a2: f32,
    detector: [[f32; 2]; 2],
    audio: [[f32; 2]; 2],
    power: f32,
    normalization: f32,
    depth: f32,
    goal: f32,
}
impl Band {
    fn new(i: usize, rate: f32) -> Self {
        let high = 18000f32.min(rate * 0.40);
        let low = 100f32.min(high / 10.);
        let hz = low * (high / low).powf(i as f32 / (BANDS - 1) as f32);
        let w = 2. * std::f32::consts::PI * hz / rate;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / 20.; // Q = 10; narrower than the old Q = 4 bank.
        Self {
            b0: alpha / (1. + alpha),
            a1: -2. * cs / (1. + alpha),
            a2: (1. - alpha) / (1. + alpha),
            // Exact integrated digital bandpass noise gain, not an analog Hz approximation.
            normalization: (1. + alpha) / alpha.max(1e-10),
            detector: [[0.; 2]; 2],
            audio: [[0.; 2]; 2],
            power: 0.,
            depth: 0.,
            goal: 0.,
        }
    }
    fn reset(&mut self) {
        self.detector = [[0.; 2]; 2];
        self.audio = [[0.; 2]; 2];
        self.power = 0.;
        self.depth = 0.;
        self.goal = 0.;
    }
    fn filter(&mut self, input: [f32; 2], audio: bool) -> [f32; 2] {
        let states = if audio {
            &mut self.audio
        } else {
            &mut self.detector
        };
        std::array::from_fn(|c| {
            let z = &mut states[c];
            let y = self.b0 * input[c] + z[0];
            z[0] = -self.a1 * y + z[1];
            z[1] = -self.b0 * input[c] - self.a2 * y;
            // Suppress denormals when an active strip receives a long silent tail.
            for v in z {
                if v.abs() < 1e-20 {
                    *v = 0.;
                }
            }
            y
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signal(rate: u32, seconds: f32, resonance: f32) -> Vec<f32> {
        let mut seed = 1234567u32;
        (0..(rate as f32 * seconds) as usize)
            .flat_map(|n| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = (seed as f64 / u32::MAX as f64 * 2. - 1.) as f32 * 0.08;
                let x = noise
                    + 0.22 * (std::f32::consts::TAU * resonance * n as f32 / rate as f32).sin();
                [x, x]
            })
            .collect()
    }
    fn energy(x: &[f32]) -> f64 {
        x.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / x.len() as f64
    }
    fn amplitude(x: &[f32], hz: f32, rate: u32) -> f64 {
        let (mut re, mut im) = (0., 0.);
        for (i, f) in x.chunks_exact(2).enumerate() {
            let phase = std::f64::consts::TAU * hz as f64 * i as f64 / rate as f64;
            re += f[0] as f64 * phase.cos();
            im += f[0] as f64 * phase.sin();
        }
        (re * re + im * im).sqrt() / (x.len() / 2) as f64 * 2.
    }
    #[test]
    fn tame_selectivity_level_and_rates() {
        for rate in [44100, 48000, 96000] {
            for hz in [330., 777., 2300., 6100., 10500.] {
                let dry = signal(rate, 0.6, hz);
                let mut wet = dry.clone();
                Tame::new(rate).process(&mut wet, 1.);
                let start = rate as usize / 2;
                let ratio = amplitude(&wet[start..], hz, rate) / amplitude(&dry[start..], hz, rate);
                eprintln!(
                    "TAME {rate}Hz resonance {hz}Hz: {:.2} dB",
                    20. * ratio.log10()
                );
                assert!(ratio < 0.55, "missed resonance {hz} @ {rate}: {ratio}");
                assert!(ratio > 0.1, "excessive cut");
                assert!(wet.iter().all(|x| x.is_finite()));
                assert!(wet.chunks_exact(2).all(|f| f[0] == f[1]));
                let mut quiet: Vec<_> = dry.iter().map(|x| x * 0.01).collect();
                Tame::new(rate).process(&mut quiet, 1.);
                let error = wet
                    .iter()
                    .zip(&quiet)
                    .map(|(a, b)| (a * 0.01 - b).abs())
                    .fold(0f32, f32::max);
                assert!(error < 2e-6, "level dependence {error}");
            }
        }
    }
    #[test]
    fn tame_noise_transients_stereo_and_bypass() {
        let dry = signal(48000, 1., 0.);
        let mut wet = dry.clone();
        Tame::new(48000).process(&mut wet, 1.);
        let db = 10. * (energy(&wet[48000..]) / energy(&dry[48000..])).log10();
        eprintln!("Broadband noise change: {db:.3} dB");
        assert!(db > -0.7, "broadband signal dulled");
        let mut impulse = vec![0.; 9600];
        impulse[0] = 1.;
        impulse[1] = -1.;
        let dry_impulse = impulse.clone();
        Tame::new(48000).process(&mut impulse, 1.);
        assert_eq!(impulse[0], 1.);
        assert_eq!(impulse[1], -1.);
        assert!(
            impulse
                .iter()
                .zip(&dry_impulse)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max)
                < 0.002
        );
        let mono = signal(48000, 0.5, 3000.);
        let mut stereo = mono.clone();
        for f in stereo.chunks_exact_mut(2) {
            f[1] = -f[0] * 0.3;
        }
        let mut processor = Tame::new(48000);
        processor.process(&mut stereo, 1.);
        assert!(
            stereo
                .chunks_exact(2)
                .all(|f| (f[1] + f[0] * 0.3).abs() < 2e-5)
        );
        let mut fading = signal(48000, 3., 3000.);
        processor.process(&mut fading, 0.);
        let mut bypass = mono.clone();
        processor.process(&mut bypass, 0.);
        assert_eq!(bypass, mono);
        let mut fresh = mono.clone();
        let mut restarted = mono.clone();
        Tame::new(48000).process(&mut fresh, 1.);
        processor.process(&mut restarted, 1.);
        assert_eq!(fresh, restarted, "bypass must discard old detector history");
    }
    #[test]
    fn tame_blocks_inherit_and_automation() {
        use crate::channel_strip::ChannelStrip;
        let dry = signal(48000, 0.8, 2800.);
        let mut whole = dry.clone();
        Tame::new(48000).process(&mut whole, 0.7);
        let mut chunks = dry.clone();
        let mut c = Tame::new(48000);
        for block in chunks.chunks_mut(254) {
            c.process(block, 0.7);
        }
        assert_eq!(whole, chunks, "host block size must not change detection");
        let mut c = ChannelStrip::new([0, 0, 0, 0, 45874, 0], 48000);
        let mut first = dry[..10000].to_vec();
        c.tame(&mut first);
        let mut replacement = ChannelStrip::new([0, 0, 0, 0, 45874, 0], 48000);
        replacement.inherit(&c);
        let mut a = dry[10000..].to_vec();
        let mut b = a.clone();
        c.tame(&mut a);
        replacement.tame(&mut b);
        assert_eq!(a, b);
        for (i, block) in a.chunks_mut(128).enumerate() {
            replacement.automate(4, if i % 2 == 0 { 0. } else { 1. });
            replacement.tame(block);
        }
        assert!(a.iter().all(|x| x.is_finite() && x.abs() < 1.));
    }
    #[test]
    #[ignore = "manual bounded CPU measurement"]
    fn tame_cpu_measurement() {
        let input = signal(48000, 1., 2700.);
        let start = std::time::Instant::now();
        for _ in 0..32 {
            let mut b = input.clone();
            Tame::new(48000).process(&mut b, 0.7);
            std::hint::black_box(b);
        }
        eprintln!(
            "32 active TAME strips x 1 second, serial CPU: {:?}",
            start.elapsed()
        );
    }
}
