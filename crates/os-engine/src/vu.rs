//! Rectified-average stereo VU detector. A -18 dBFS sine reads 0 VU.
//! Two cascaded poles reach 99% of a steady level in approximately 300 ms.

#[derive(Clone, Copy, Default)]
pub struct Vu {
    first: [f64; 2],
    second: [f64; 2],
}

impl Vu {
    pub fn process(&mut self, stereo: &[f32], rate: u32) {
        let a = -(-1.0 / (rate.max(1) as f64 * 0.0452)).exp_m1();
        for frame in stereo.chunks_exact(2) {
            for i in 0..2 {
                let x = if frame[i].is_finite() { frame[i].abs() as f64 } else { 0.0 };
                self.first[i] += a * (x - self.first[i]);
                self.second[i] += a * (self.first[i] - self.second[i]);
            }
        }
    }

    /// Sine-peak equivalent amplitude; calibration and display are applied by the UI.
    pub fn level(&self) -> [f32; 2] {
        self.second.map(|v| (v * std::f64::consts::FRAC_PI_2) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibrated_sine_ballistics_and_stereo() {
        for rate in [44100, 48000, 96000] {
            let amplitude = 10f32.powf(-18.0 / 20.0);
            let tone: Vec<f32> = (0..rate).flat_map(|i| {
                let x = amplitude * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin();
                [x, x * 0.5]
            }).collect();
            let mut vu = Vu::default();
            vu.process(&tone[..(rate as usize / 100) * 2], rate);
            assert!(vu.level()[0] < amplitude * 0.04, "short transient is not a peak reading");
            vu.process(&tone[(rate as usize / 100) * 2..(rate as usize * 3 / 10) * 2], rate);
            assert!((vu.level()[0] / amplitude - 0.99).abs() < 0.005);
            vu.process(&tone[(rate as usize * 3 / 10) * 2..], rate);
            let db = vu.level().map(|x| 20.0 * x.log10() + 18.0);
            assert!(db[0].abs() < 0.03, "0 VU alignment: {db:?}");
            assert!((db[1] + 6.0206).abs() < 0.03);
            vu.process(&vec![0.0; rate as usize * 2], rate);
            assert!(vu.level()[0] < 1e-6);
        }
    }
}
