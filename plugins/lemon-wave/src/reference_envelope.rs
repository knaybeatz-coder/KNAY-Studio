// SPDX-License-Identifier: GPL-3.0-or-later
//! Recovered curve recurrence; bounded AHDSR integration. See research notes.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub attack: f64,
    pub hold: f64,
    pub decay: f64,
    pub sustain: f32,
    pub release: f64,
    pub curves: [f64; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            attack: 0.001,
            hold: 0.,
            decay: 1.,
            sustain: 1.,
            release: 0.015,
            curves: [0.5; 3],
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Curve {
    q: f64,
    scale: f64,
    factor: f64,
    value: f64,
    increment: f64,
    length: u32,
    elapsed: u32,
}
impl Curve {
    fn new(shape: f64, length: u32) -> Self {
        let c = (1. - shape).clamp(1e-12, 0.999999999998);
        let n = length.max(1);
        if (c - 0.5).abs() < 2e-6 {
            return Self {
                scale: 1.,
                factor: 1.,
                value: 1.,
                increment: -1. / n as f64,
                length: n,
                ..Self::default()
            };
        }
        let c = if c <= 0.5 {
            4. * c * c * c
        } else {
            1. - 4. * (1. - c).powi(3)
        }
        .clamp(1e-12, 0.999999999998);
        let q = (c / (1. - c)).powi(2);
        Self {
            q,
            scale: 1. / (1. - q),
            factor: q.powf(1. / n as f64),
            value: 1.,
            length: n,
            ..Self::default()
        }
    }
    fn tick(&mut self) -> f32 {
        self.elapsed += 1;
        if self.elapsed >= self.length {
            return 0.;
        }
        self.value = self.value * self.factor + self.increment;
        ((self.value - self.q) * self.scale).clamp(0., 1.) as f32
    }
}
/// Curve display uses the same law; progress is elapsed / total.
pub fn falling(shape: f64, progress: f64) -> f32 {
    let c = (1. - shape).clamp(1e-12, 0.999999999998);
    let t = progress.clamp(0., 1.);
    if (c - 0.5).abs() < 2e-6 {
        return (1. - t) as f32;
    }
    let c = if c <= 0.5 {
        4. * c * c * c
    } else {
        1. - 4. * (1. - c).powi(3)
    }
    .clamp(1e-12, 0.999999999998);
    let q = (c / (1. - c)).powi(2);
    ((q.powf(t) - q) / (1. - q)).clamp(0., 1.) as f32
}
#[derive(Clone, Copy, Default)]
pub struct Envelope {
    pub value: f32,
    pub stage: u8,
    curve: Curve,
    remaining: u32,
    from: f32,
}
impl Envelope {
    fn count(seconds: f64, rate: f32) -> u32 {
        (seconds.max(0.) * rate as f64).floor().min(u32::MAX as f64) as u32
    }
    pub fn start(&mut self, p: &Settings, rate: f32, retrigger_from_current: bool) {
        self.from = if retrigger_from_current {
            self.value
        } else {
            0.
        };
        self.value = self.from;
        self.stage = 1;
        self.remaining = Self::count(p.attack, rate);
        self.curve = Curve::new(p.curves[0], self.remaining);
    }
    pub fn off(&mut self, p: &Settings, rate: f32) {
        if self.stage != 0 && self.stage != 4 {
            self.from = self.value;
            self.stage = 4;
            self.remaining = Self::count(p.release, rate).max(1);
            self.curve = Curve::new(p.curves[2], self.remaining);
        }
    }
    pub fn tick(&mut self, p: &Settings, rate: f32) -> f32 {
        loop {
            match self.stage {
                1 if self.remaining == 0 => {
                    self.value = 1.;
                    self.stage = 5;
                    self.remaining = Self::count(p.hold, rate).max(1);
                }
                5 if self.remaining == 0 => {
                    self.stage = 2;
                    self.remaining = Self::count(p.decay, rate);
                    self.curve = Curve::new(p.curves[1], self.remaining);
                }
                2 if self.remaining == 0 => {
                    self.value = p.sustain;
                    self.stage = 3;
                }
                _ => break,
            }
        }
        match self.stage {
            1 => {
                self.value = 1. - self.curve.tick() * (1. - self.from);
                self.remaining -= 1;
            }
            5 => {
                self.value = 1.;
                self.remaining -= 1;
            }
            2 => {
                self.value = p.sustain + (1. - p.sustain) * self.curve.tick();
                self.remaining -= 1;
            }
            3 => self.value = p.sustain,
            4 => {
                self.value = self.from * self.curve.tick();
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.stage = 0;
                    self.value = 0.;
                }
            }
            _ => self.value = 0.,
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_recorded_serum_attack_hold_decay_and_early_release() {
        use crate::reference_oscillator::{Decimator, Oscillator, Table, Warp};
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../research/envelope-fixtures.json")).unwrap();
        let table = std::sync::Arc::new(Table::init_saw());
        for c in cases.as_array().unwrap() {
            let rate = c["sample_rate"].as_u64().unwrap() as f32;
            let get = |id: u64| {
                c["parameters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p[0].as_u64() == Some(id))
                    .unwrap()[1]
                    .as_f64()
                    .unwrap()
            };
            let p = Settings {
                attack: 32. * get(3000000).powi(5),
                hold: 32. * get(3000001).powi(5),
                decay: 32. * get(3000002).powi(5),
                sustain: get(3000003).powi(2) as f32,
                release: 32. * get(3000004).powi(5),
                curves: [get(3000005), get(3000006), get(3000007)],
            };
            let mut env = Envelope::default();
            env.start(&p, rate * 2., false);
            let mut osc = Oscillator::new(table.clone(), rate);
            let mut decimator = Decimator::default();
            let samples = c["samples"].as_array().unwrap();
            let mut at = 0;
            let mut error = 0_f64;
            let mut energy = 0_f64;
            for i in 0..c["frames"].as_u64().unwrap() {
                if c["off"].as_u64() == Some(i) {
                    env.off(&p, rate * 2.);
                }
                let pair = osc
                    .tick_pair(440., Warp::Off, 0., [0.75 * 0.75; 2])
                    .map(|v| v * std::f32::consts::SQRT_2 * env.tick(&p, rate * 2.));
                let y = decimator.tick(pair) * 0.0625;
                if at < samples.len() && samples[at][0].as_u64() == Some(i) {
                    let expected = samples[at][1].as_f64().unwrap();
                    let delta = y as f64 - expected;
                    error += delta * delta;
                    energy += expected * expected;
                    assert!(delta.abs() < 0.00011, "{} {i} {delta}", c["label"]);
                    at += 1;
                }
            }
            assert!(
                (error / energy).sqrt() < 0.0003,
                "{} {}",
                c["label"],
                (error / energy).sqrt()
            );
        }
    }
    #[test]
    fn holds_releases_and_curves_follow_the_display() {
        for rate in [44100., 48000., 96000.] {
            for shape in [0., 0.25, 0.5, 0.75, 1.] {
                let p = Settings {
                    attack: 0.1,
                    hold: 0.02,
                    decay: 0.1,
                    sustain: 0.25,
                    release: 0.1,
                    curves: [shape; 3],
                };
                let mut e = Envelope::default();
                e.start(&p, rate, false);
                let n = (rate * 0.1) as usize;
                for i in 1..=n {
                    let v = e.tick(&p, rate);
                    assert!((v - (1. - falling(shape, i as f64 / n as f64))).abs() < 2e-5);
                }
                for _ in 0..(rate * 0.02) as usize {
                    assert_eq!(e.tick(&p, rate), 1.);
                }
                for _ in 0..n + 2 {
                    e.tick(&p, rate);
                }
                assert_eq!(e.value, 0.25);
                e.off(&p, rate);
                for _ in 0..n {
                    e.tick(&p, rate);
                }
                assert_eq!(e.stage, 0);
                assert_eq!(e.value, 0.);
                // Release from halfway through an attack uses the present value, not the peak.
                e.start(&p, rate, false);
                for _ in 0..n / 2 {
                    e.tick(&p, rate);
                }
                let v = e.value;
                e.off(&p, rate);
                assert!(e.tick(&p, rate) <= v);
                for _ in 0..n {
                    e.tick(&p, rate);
                }
                assert_eq!(e.stage, 0);
            }
        }
    }
}
