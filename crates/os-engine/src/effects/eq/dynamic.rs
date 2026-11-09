//! Routed dynamic bell/shelf EQ plus frequency-selective spectral dynamics.
use super::{BANDS, STATE_LEN, Settings};
/// Nominal bell quality for the existing stored octave-bandwidth control.
pub fn q_of_width(width: f64) -> f64 {
    1. / (2. * (std::f64::consts::LN_2 * 0.5 * (0.1 + 4.9 * width.clamp(0., 1.))).sinh())
}
const OFFSET: usize = STATE_LEN + 18;
const VALUES: usize = BANDS * 4 + 2;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub bands: [[f32; 4]; BANDS],
    pub harsh: [f32; 2],
    pub auto: [bool; 7],
    pub spectral: u8,
    pub density: f32,
    pub tilt: bool,
    pub harsh_auto: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bands: [[0., -30., 10., 120.]; BANDS],
            harsh: [0., -30.],
            auto: [false; 7],
            spectral: 0,
            density: 0.5,
            tilt: true,
            harsh_auto: false,
        }
    }
}
impl Config {
    pub fn load(b: &[u8], spare: bool) -> Self {
        let mut c = Self::default();
        if b.get(OFFSET..OFFSET + 4) != Some(b"LSD1") {
            return c;
        }
        let base = OFFSET + 4 + usize::from(spare) * VALUES * 4;
        for i in 0..VALUES {
            if let Some(v) = b.get(base + i * 4..base + i * 4 + 4) {
                c.set(i, f32::from_le_bytes(v.try_into().unwrap()));
            }
        }
        let ext = OFFSET + 4 + VALUES * 8;
        if b.get(ext..ext + 4) == Some(b"LSA1") {
            for i in 30..47 {
                let j = ext + 4 + (usize::from(spare) * 17 + i - 30) * 4;
                if let Some(v) = b.get(j..j + 4) {
                    c.set(i, f32::from_le_bytes(v.try_into().unwrap()));
                }
            }
        }
        c
    }
    pub fn value(&self, i: usize) -> f32 {
        match i {
            0..=27 => self.bands[i / 4][i % 4],
            28..=29 => self.harsh[i - 28],
            30..=36 => u8::from(self.auto[i - 30]) as f32,
            37 => self.density,
            38 => u8::from(self.tilt) as f32,
            39 => u8::from(self.harsh_auto) as f32,
            40..=46 => u8::from(self.spectral & (1 << (i - 40)) != 0) as f32,
            _ => 0.,
        }
    }
    pub fn set(&mut self, i: usize, v: f32) {
        if !v.is_finite() {
            return;
        }
        match i {
            0..=27 => {
                let (lo, hi) = match i % 4 {
                    0 => (-18., 18.),
                    1 => (-72., 0.),
                    2 => (0.5, 200.),
                    _ => (10., 1500.),
                };
                self.bands[i / 4][i % 4] = v.clamp(lo, hi);
            }
            28 => self.harsh[0] = v.clamp(0., 12.),
            29 => self.harsh[1] = v.clamp(-72., 0.),
            30..=36 => self.auto[i - 30] = v >= 0.5,
            37 => self.density = v.clamp(0., 1.),
            38 => self.tilt = v >= 0.5,
            39 => self.harsh_auto = v >= 0.5,
            40..=46 => {
                let bit = 1 << (i - 40);
                if v >= 0.5 {
                    self.spectral |= bit;
                } else {
                    self.spectral &= !bit;
                }
            }
            _ => {}
        }
    }
    pub fn save(&self, b: &mut Vec<u8>, spare: bool) {
        if b.len() < OFFSET + 4 + VALUES * 8 {
            b.resize(OFFSET + 4 + VALUES * 8, 0);
            let d = Self::default();
            for bank in 0..2 {
                for i in 0..VALUES {
                    let j = OFFSET + 4 + (bank * VALUES + i) * 4;
                    b[j..j + 4].copy_from_slice(&d.value(i).to_le_bytes());
                }
            }
        }
        b[OFFSET..OFFSET + 4].copy_from_slice(b"LSD1");
        for i in 0..VALUES {
            let j = OFFSET + 4 + (usize::from(spare) * VALUES + i) * 4;
            b[j..j + 4].copy_from_slice(&self.value(i).to_le_bytes());
        }
        let ext = OFFSET + 4 + VALUES * 8;
        let needed = ext + 4 + 17 * 8;
        if b.len() < needed {
            b.resize(needed, 0);
            let d = Self::default();
            for bank in 0..2 {
                for i in 30..47 {
                    let j = ext + 4 + (bank * 17 + i - 30) * 4;
                    b[j..j + 4].copy_from_slice(&d.value(i).to_le_bytes());
                }
            }
        }
        b[ext..ext + 4].copy_from_slice(b"LSA1");
        for i in 30..47 {
            let j = ext + 4 + (usize::from(spare) * 17 + i - 30) * 4;
            b[j..j + 4].copy_from_slice(&self.value(i).to_le_bytes());
        }
    }
}
#[derive(Clone, Copy)]
struct Filter {
    c: [f64; 5],
    z: [[f64; 2]; 2],
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            c: [1., 0., 0., 0., 0.],
            z: [[0.; 2]; 2],
        }
    }
}
impl Filter {
    fn run(&mut self, x: f64, ch: usize) -> f64 {
        let y = self.c[0] * x + self.z[ch][0];
        self.z[ch][0] = self.c[1] * x - self.c[3] * y + self.z[ch][1];
        self.z[ch][1] = self.c[2] * x - self.c[4] * y;
        y
    }
    fn design(&mut self, hz: f64, q: f64, db: Option<f64>, rate: f64, kind: i32) {
        let w = std::f64::consts::TAU * hz.clamp(10., rate * 0.45) / rate;
        let a = w.sin() / (2. * q);
        let co = w.cos();
        self.c = if let Some(db) = db {
            let g = 10f64.powf(db / 40.);
            let den = 1. + a / g;
            if kind == 5 || kind == 7 {
                let t = 2. * g.sqrt() * a;
                let u = g + 1.;
                let v = g - 1.;
                let raw = if kind == 5 {
                    [
                        g * (u - v * co + t),
                        2. * g * (v - u * co),
                        g * (u - v * co - t),
                        u + v * co + t,
                        -2. * (v + u * co),
                        u + v * co - t,
                    ]
                } else {
                    [
                        g * (u + v * co + t),
                        -2. * g * (v + u * co),
                        g * (u + v * co - t),
                        u - v * co + t,
                        2. * (v - u * co),
                        u - v * co - t,
                    ]
                };
                [
                    raw[0] / raw[3],
                    raw[1] / raw[3],
                    raw[2] / raw[3],
                    raw[4] / raw[3],
                    raw[5] / raw[3],
                ]
            } else {
                [
                    (1. + a * g) / den,
                    -2. * co / den,
                    (1. - a * g) / den,
                    -2. * co / den,
                    (1. - a / g) / den,
                ]
            }
        } else {
            let den = 1. + a;
            [a / den, 0., -a / den, -2. * co / den, (1. - a) / den]
        };
    }
}
/// A display response designed once, then sampled across the frequency grid.
pub struct Response { c: [f64; 5], rate: f64, flat: bool }
impl Response {
    pub fn new(rate: f64, band: super::Band, gain: f64) -> Self {
        let mut f = Filter::default();
        let flat = gain.abs() < 1e-7;
        if !flat { f.design(band.hz(), q_of_width(band.width), Some(gain), rate, band.kind); }
        Self { c: f.c, rate, flat }
    }
    pub fn db(&self, hz: f64) -> f64 {
        if self.flat { return 0.; }
        let w = std::f64::consts::TAU * hz / self.rate;
        let (s, c) = w.sin_cos();
        let (s2, c2) = (2. * w).sin_cos();
        let f = &self.c;
        let num = (f[0] + f[1] * c + f[2] * c2).powi(2) + (f[1] * s + f[2] * s2).powi(2);
        let den = (1. + f[3] * c + f[4] * c2).powi(2) + (f[3] * s + f[4] * s2).powi(2);
        10. * (num / den.max(1e-20)).max(1e-20).log10()
    }
}
pub fn response_db(hz: f64, rate: f64, band: super::Band, gain: f64) -> f64 {
    Response::new(rate, band, gain).db(hz)
}

#[derive(Default)]
struct Band {
    detect: Filter,
    eq: Filter,
    env: f64,
    gain: f64,
    auto_threshold: f32,
    slow: f64,
    slow_alpha: f64,
    clock: u32,
    hz: f64,
    q: f64,
    p: [f32; 4],
    attack: f64,
    release: f64,
}
impl Band {
    fn run(
        &mut self,
        l: f64,
        r: f64,
        route: u8,
        hz: f64,
        q: f64,
        p: [f32; 4],
        rate: f64,
        kind: i32,
        auto: bool,
    ) -> (f64, f64) {
        if p[0].abs() < 0.0001 && self.gain.abs() < 0.00001 {
            self.env = 0.;
            self.gain = 0.;
            self.eq = Filter::default();
            return (l, r);
        }
        if self.hz != hz || self.q != q {
            self.detect.design(hz, q, None, rate, 6);
            self.hz = hz;
            self.q = q;
        }
        if self.p != p {
            self.attack = (-1. / (rate * f64::from(p[2]) * 0.001)).exp();
            self.release = (-1. / (rate * f64::from(p[3]) * 0.001)).exp();
            self.p = p;
            self.slow_alpha = (-1. / (rate * 0.5)).exp();
        }
        let (a, b) = match route {
            1 => ((l + r) * 0.5, 0.),
            2 => ((l - r) * 0.5, 0.),
            3 => (l, 0.),
            4 => (r, 0.),
            _ => (l, r),
        };
        let peak = self.detect.run(a, 0).abs().max(self.detect.run(b, 1).abs());
        let alpha = if peak > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = peak + (self.env - peak) * alpha;
        self.slow = peak + (self.slow - peak) * self.slow_alpha;
        if self.clock == 0 {
            if auto {
                self.auto_threshold =
                    (20. * self.slow.max(1e-5).log10() - 6.).clamp(-72., -6.) as f32;
            }
            let over = ((20. * self.env.max(1e-12).log10()
                - f64::from(if auto { self.auto_threshold } else { p[1] }))
                / 12.)
                .clamp(0., 1.);
            let target = f64::from(p[0]) * over * over * (3. - 2. * over);
            let smooth = if target.abs() > self.gain.abs() {
                self.attack.powi(32)
            } else {
                self.release.powi(32)
            };
            self.gain = target + (self.gain - target) * smooth;
            self.eq.design(hz, q, Some(self.gain), rate, kind);
        }
        self.clock = (self.clock + 1) % 32;
        let (x, y) = (self.eq.run(a, 0), self.eq.run(b, 1));
        match route {
            1 => {
                let side = (l - r) * 0.5;
                (x + side, x - side)
            }
            2 => {
                let mid = (l + r) * 0.5;
                (mid + x, mid - x)
            }
            3 => (x, r),
            4 => (l, x),
            _ => (x, y),
        }
    }
}
pub struct Audition {
    filter: Filter,
    rate: f64,
    hz: f64,
    width: f64,
}
impl Audition {
    pub fn new(rate: f64) -> Self {
        Self {
            filter: Filter::default(),
            rate,
            hz: 0.,
            width: 0.,
        }
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], band: super::Band) {
        if self.hz != band.hz() || self.width != band.width {
            self.hz = band.hz();
            self.width = band.width;
            let q = q_of_width(band.width);
            self.filter.design(self.hz, q, None, self.rate, 6);
        }
        for (l, r) in l.iter_mut().zip(r.iter_mut()) {
            *l = self.filter.run(f64::from(*l), 0) as f32;
            *r = self.filter.run(f64::from(*r), 1) as f32;
        }
    }
}
pub struct Processor {
    rate: f64,
    config: Config,
    bands: [Band; BANDS],
    spectral: super::spectral::Processor,
    pub trigger: [f32; 7],
    pub threshold: [f32; 7],
    pub meters: [f32; 8],
}
impl Processor {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            config: Config::default(),
            bands: std::array::from_fn(|_| Band::default()),
            spectral: super::spectral::Processor::new(rate),
            trigger: [-120.; 7],
            threshold: [-30.; 7],
            meters: [0.; 8],
        }
    }
    pub fn configure(&mut self, c: Config) {
        self.config = c;
        if c.harsh[0] > 0. || c.spectral != 0 {
            self.spectral.engage();
        }
    }
    pub fn latency(&self) -> usize {
        self.spectral.latency()
    }
    pub fn spectral_response(&self) -> [f32; 256] {
        self.spectral.response
    }
    pub fn reset(&mut self) {
        self.bands = std::array::from_fn(|_| Band::default());
        self.meters = [0.; 8];
        self.trigger = [-120.; 7];
        self.threshold = [-30.; 7];
        self.spectral
            .reset(self.config.harsh[0] > 0. || self.config.spectral != 0);
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], s: &Settings, routes: [u8; 7]) {
        self.meters = [0.; 8];
        for k in 0..BANDS {
            if s.mask & (1 << k) == 0 || s.bands[k].kind == 0 {
                continue;
            }
            if self.config.spectral & (1 << k) != 0 {
                continue;
            }
            let mut p = self.config.bands[k];
            if self.config.auto[k] {
                p[2] = (10. * (1000. / s.bands[k].hz()).powf(0.3)).clamp(1., 80.) as f32;
                p[3] = (120. * (1000. / s.bands[k].hz()).powf(0.15)).clamp(40., 400.) as f32;
            }
            if p[0] == 0. && self.bands[k].gain == 0. {
                continue;
            }
            let hz = s.bands[k].hz();
            let q = q_of_width(s.bands[k].width);
            for (l, r) in l.iter_mut().zip(r.iter_mut()) {
                let (a, b) = self.bands[k].run(
                    f64::from(*l),
                    f64::from(*r),
                    routes[k],
                    hz,
                    q,
                    p,
                    self.rate,
                    s.bands[k].kind,
                    self.config.auto[k],
                );
                *l = a as f32;
                *r = b as f32;
            }
            self.meters[k] = self.bands[k].gain as f32;
            self.trigger[k] = (20. * self.bands[k].env.max(1e-9).log10()) as f32;
            self.threshold[k] = if self.config.auto[k] {
                self.bands[k].auto_threshold
            } else {
                p[1]
            };
        }
        self.spectral.process(l, r, s, routes, self.config);
        for k in 0..7 {
            if self.config.spectral & (1 << k) != 0 {
                self.meters[k] = self.spectral.meters[k];
                self.trigger[k] = self.spectral.trigger[k];
                self.threshold[k] = self.config.bands[k][1];
            }
        }
        self.meters[7] = self.spectral.meters[7];
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_sizes_rates_and_shelves_remain_stable() {
        for rate in [8000., 44100., 48000., 96000., 192000.] {
            let mut s = Settings::default();
            let mut c = Config::default();
            for k in 0..BANDS {
                s.bands[k].freq = super::super::freq_pos(80. * (k + 1) as f64);
                c.bands[k] = [if k % 2 == 0 { -9. } else { 6. }, -36., 2., 70.];
            }
            c.harsh = [4., -30.];
            let x: Vec<f32> = (0..4096)
                .map(|i| 0.2 * (std::f64::consts::TAU * 300. * i as f64 / rate).sin() as f32)
                .collect();
            let mut a = Processor::new(rate);
            a.configure(c);
            let mut l = x.clone();
            let mut r = x.clone();
            a.process(&mut l, &mut r, &s, [0; 7]);
            let mut b = Processor::new(rate);
            b.configure(c);
            let mut m = x.clone();
            let mut n = x.clone();
            for (m, n) in m.chunks_mut(37).zip(n.chunks_mut(37)) {
                b.process(m, n, &s, [0; 7]);
            }
            assert!(l.iter().zip(&m).all(|(a, b)| (a - b).abs() < 1e-5));
            assert!(l.iter().all(|v| v.is_finite() && v.abs() < 10.));
        }
    }
    #[test]
    fn dynamic_recall_bypass_threshold_and_stereo() {
        let mut c = Config::default();
        c.set(12, -12.);
        c.set(13, -30.);
        let mut bytes = vec![0; STATE_LEN];
        c.save(&mut bytes, false);
        assert_eq!(Config::load(&bytes, false), c);
        assert_eq!(Config::load(&bytes, true), Config::default());
        assert_eq!(Config::load(&bytes[..STATE_LEN], false), Config::default());
        let mut s = Settings::default();
        s.bands[3].freq = super::super::freq_pos(1000.);
        let input = |amp: f32| -> Vec<f32> {
            (0..48000)
                .map(|i| amp * (std::f64::consts::TAU * 1000. * i as f64 / 48000.).sin() as f32)
                .collect()
        };
        for amp in [0.001, 0.5] {
            let mut p = Processor::new(48000.);
            let x = input(amp);
            let mut l = x.clone();
            let mut r = x.iter().map(|v| -v).collect::<Vec<_>>();
            p.process(&mut l, &mut r, &s, [0; 7]);
            assert_eq!(l, x);
            p.configure(c);
            p.process(&mut l, &mut r, &s, [0; 7]);
            assert!(l.iter().zip(&r).all(|(a, b)| (a + b).abs() < 1e-6));
            if amp < 0.01 {
                assert!(p.meters[3].abs() < 0.001);
            } else {
                assert!(p.meters[3] < -10.);
                assert!(l[24000..].iter().map(|v| v.abs()).fold(0f32, f32::max) < 0.22);
            }
        }
        let mut p = Processor::new(48000.);
        p.configure(c);
        let x = input(0.5);
        let mut l = x.clone();
        let mut r = x.clone();
        p.process(&mut l, &mut r, &s, [3; 7]);
        assert_eq!(r, x);
        assert!(p.meters[3] < -10.);
    }
    #[test]
    fn deharsh_targets_upper_resonance() {
        let mut c = Config::default();
        c.harsh = [9., -36.];
        let mut s = Settings::default();
        s.mask = 0;
        for hz in [100., 4100.] {
            let x: Vec<_> = (0..48000)
                .map(|i| 0.4 * (std::f64::consts::TAU * hz * i as f64 / 48000.).sin() as f32)
                .collect();
            let mut l = x.clone();
            let mut r = x.clone();
            let mut p = Processor::new(48000.);
            p.configure(c);
            p.process(&mut l, &mut r, &s, [0; 7]);
            let ratio = (l[24000..].iter().map(|v| v * v).sum::<f32>()
                / x[24000..].iter().map(|v| v * v).sum::<f32>())
            .sqrt();
            if hz < 1000. {
                assert!(ratio > 0.98);
            } else {
                assert!(ratio < 0.8);
            }
            assert!(l.iter().all(|v| v.is_finite()));
        }
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;
    #[test]
    fn new_spectral_options_roundtrip_both_banks_and_old_states() {
        let mut b = vec![0; STATE_LEN];
        let mut a = Config::default();
        a.set(12, -9.);
        a.set(33, 1.);
        a.set(37, 0.83);
        a.set(38, 0.);
        a.set(39, 1.);
        a.set(43, 1.);
        a.save(&mut b, false);
        let mut spare = a;
        spare.set(37, 0.25);
        spare.set(43, 0.);
        spare.set(40, 1.);
        spare.save(&mut b, true);
        assert_eq!(Config::load(&b, false), a);
        assert_eq!(Config::load(&b, true), spare);
        let old = &b[..OFFSET + 4 + VALUES * 8];
        let old_config = Config::load(old, false);
        assert_eq!(old_config.bands, a.bands);
        assert_eq!(old_config.spectral, 0);
        assert_eq!(old_config.density, 0.5);
        assert!(old_config.tilt);
    }
}
