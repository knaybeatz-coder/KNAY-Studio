//! Stereo-linked, frequency-selective STFT dynamics with fixed, reported latency.
//! Four-times-overlapped sqrt-Hann windows reconstruct an unmodified signal at unity.
use super::{Settings, dynamic::Config, fft::Fft};
const HARMONIC_FLOOR: f64 = 1e-12;
pub struct Processor {
    n: usize,
    hop: usize,
    rate: f64,
    fft: Fft,
    window: Vec<f64>,
    window_sum: f64,
    input: [Vec<f64>; 2],
    output: [Vec<f64>; 2],
    re: [Vec<f64>; 2],
    im: [Vec<f64>; 2],
    magnitude: [Vec<f64>; 5],
    db: [Vec<f64>; 5],
    prefix: [Vec<f64>; 5],
    gain: [Vec<f64>; 4],
    target: [Vec<f64>; 4],
    weights: [Vec<f64>; 7],
    band_gain: [Vec<f64>; 7],
    global_gain: Vec<f64>,
    previous: Option<Settings>,
    time: usize,
    engaged: bool,
    pub response: [f32; 256],
    pub meters: [f32; 8],
    pub trigger: [f32; 7],
}
impl Processor {
    pub fn new(rate: f64) -> Self {
        let n = if rate >= 176400. {
            4096
        } else if rate >= 88200. {
            2048
        } else {
            1024
        };
        let bins = n / 2 + 1;
        let window: Vec<_> = (0..n)
            .map(|i| (std::f64::consts::PI * i as f64 / n as f64).sin())
            .collect();
        let window_sum = window.iter().sum();
        Self {
            n,
            hop: n / 4,
            rate,
            fft: Fft::new(n),
            window,
            window_sum,
            input: std::array::from_fn(|_| vec![0.; n]),
            output: std::array::from_fn(|_| vec![0.; n * 4]),
            re: std::array::from_fn(|_| vec![0.; n]),
            im: std::array::from_fn(|_| vec![0.; n]),
            magnitude: std::array::from_fn(|_| vec![0.; bins]),
            db: std::array::from_fn(|_| vec![-240.; bins]),
            prefix: std::array::from_fn(|_| vec![0.; bins + 1]),
            gain: std::array::from_fn(|_| vec![0.; bins]),
            target: std::array::from_fn(|_| vec![0.; bins]),
            weights: std::array::from_fn(|_| vec![0.; bins]),
            band_gain: std::array::from_fn(|_| vec![0.; bins]),
            global_gain: vec![0.; bins],
            previous: None,
            time: 0,
            engaged: false,
            response: [0.; 256],
            meters: [0.; 8],
            trigger: [-120.; 7],
        }
    }
    pub fn reset(&mut self, engaged: bool) {
        for x in &mut self.input {
            x.fill(0.);
        }
        for x in &mut self.output {
            x.fill(0.);
        }
        for x in &mut self.re {
            x.fill(0.);
        }
        for x in &mut self.im {
            x.fill(0.);
        }
        for x in &mut self.gain {
            x.fill(0.);
        }
        for x in &mut self.target {
            x.fill(0.);
        }
        for x in &mut self.band_gain {
            x.fill(0.);
        }
        self.global_gain.fill(0.);
        self.time = 0;
        self.engaged = engaged;
        self.response = [0.; 256];
        self.meters = [0.; 8];
        self.trigger = [-120.; 7];
    }
    pub fn engage(&mut self) {
        self.engaged = true;
    }
    pub fn latency(&self) -> usize {
        if self.engaged { self.n } else { 0 }
    }
    fn prepare(&mut self, s: &Settings) {
        for k in 0..7 {
            if self.previous.is_some_and(|p| p.bands[k] == s.bands[k]) {
                continue;
            }
            let b = s.bands[k];
            let octave = (0.1 + 4.9 * b.width.clamp(0., 1.)) / 2.354820045;
            let center = b.hz().max(10.);
            for (i, w) in self.weights[k].iter_mut().enumerate() {
                let hz = (i as f64 * self.rate / self.n as f64).max(10.);
                let distance = (hz / center).log2();
                *w = match b.kind {
                    5 => 1. / (1. + (distance * 4.).exp()),
                    7 => 1. / (1. + (-distance * 4.).exp()),
                    _ => (-0.5 * (distance / octave.max(0.03)).powi(2)).exp(),
                };
            }
        }
        self.previous = Some(*s);
    }
    pub fn process(
        &mut self,
        l: &mut [f32],
        r: &mut [f32],
        s: &Settings,
        routes: [u8; 7],
        c: Config,
    ) {
        if !self.engaged {
            return;
        }
        self.prepare(s);
        for (l, r) in l.iter_mut().zip(r.iter_mut()) {
            let input = [f64::from(*l), f64::from(*r)];
            let out_at = self.time % (self.n * 4);
            let in_at = self.time % self.n;
            for ch in 0..2 {
                self.input[ch][in_at] = input[ch];
            }
            *l = self.output[0][out_at] as f32;
            *r = self.output[1][out_at] as f32;
            self.output[0][out_at] = 0.;
            self.output[1][out_at] = 0.;
            self.time = self.time.wrapping_add(1);
            if self.time % self.hop == 0 {
                self.frame(s, routes, c);
            }
        }
    }
    fn frame(&mut self, s: &Settings, routes: [u8; 7], c: Config) {
        let n = self.n;
        let bins = n / 2 + 1;
        for ch in 0..2 {
            for j in 0..n {
                self.re[ch][j] = self.input[ch][(self.time + j) % n] * self.window[j];
                self.im[ch][j] = 0.;
            }
            self.fft.run(&mut self.re[ch], &mut self.im[ch], false);
        }
        let mut used = [true, false, false, false, false];
        for k in 0..7 {
            if c.spectral & (1 << k) != 0 {
                used[routes[k].min(4) as usize] = true;
            }
        }
        for route in 0..5 {
            if !used[route] {
                continue;
            }
            self.prefix[route][0] = 0.;
            for i in 0..bins {
                let l = (self.re[0][i], self.im[0][i]);
                let r = (self.re[1][i], self.im[1][i]);
                let amp = match route {
                    1 => ((l.0 + r.0) * 0.5).hypot((l.1 + r.1) * 0.5),
                    2 => ((l.0 - r.0) * 0.5).hypot((l.1 - r.1) * 0.5),
                    3 => l.0.hypot(l.1),
                    4 => r.0.hypot(r.1),
                    _ => l.0.hypot(l.1).max(r.0.hypot(r.1)),
                } * 2.
                    / self.window_sum;
                self.magnitude[route][i] = amp;
                self.db[route][i] = 20. * amp.max(HARMONIC_FLOOR).log10();
                self.prefix[route][i + 1] = self.prefix[route][i] + amp;
            }
        }
        for ch in 0..4 {
            self.target[ch].fill(0.);
        }
        self.meters = [0.; 8];
        self.trigger = [-120.; 7];
        let radius = (3. + (1. - c.density) * 14.) as usize;
        let ga = (-(self.hop as f64) / (self.rate * 0.006)).exp();
        let gr = (-(self.hop as f64) / (self.rate * 0.1)).exp();
        let attacks: [f64; 7] = std::array::from_fn(|k| {
            (-(self.hop as f64)
                / (self.rate
                    * if c.auto[k] {
                        0.006
                    } else {
                        f64::from(c.bands[k][2]) * 0.001
                    }))
            .exp()
        });
        let releases: [f64; 7] = std::array::from_fn(|k| {
            (-(self.hop as f64)
                / (self.rate
                    * if c.auto[k] {
                        0.1
                    } else {
                        f64::from(c.bands[k][3]) * 0.001
                    }))
            .exp()
        });
        for i in 1..bins - 1 {
            let hz = i as f64 * self.rate / n as f64;
            if hz < 20. || hz > self.rate * 0.47 {
                continue;
            }
            let a = i.saturating_sub(radius);
            let b = (i + radius + 1).min(bins);
            let local = (self.prefix[0][b] - self.prefix[0][a] - self.magnitude[0][i])
                / (b - a - 1).max(1) as f64;
            let prominence = (self.db[0][i] - 20. * local.max(HARMONIC_FLOOR).log10() - 1.).max(0.);
            let select = (prominence / (3. + 9. * f64::from(c.density))).clamp(0., 1.);
            let tilted = if c.tilt { 3. * (hz / 1000.).log2() } else { 0. };
            let mut global = 0.;
            if c.harsh[0] > 0. && (1500.0..=14000.0).contains(&hz) {
                let edge =
                    ((hz - 1500.) / 300.).clamp(0., 1.) * ((14000. - hz) / 2000.).clamp(0., 1.);
                let over = if c.harsh_auto {
                    select
                } else {
                    ((self.db[0][i] + tilted - f64::from(c.harsh[1])) / 12.).clamp(0., 1.) * select
                };
                global = -f64::from(c.harsh[0]) * over * edge;
            }
            let alpha = if global.abs() > self.global_gain[i].abs() {
                ga
            } else {
                gr
            };
            self.global_gain[i] = global + (self.global_gain[i] - global) * alpha;
            self.target[0][i] = self.global_gain[i];
            self.target[1][i] = self.global_gain[i];
            self.meters[7] = self.meters[7].min(self.global_gain[i] as f32);
            for k in 0..7 {
                let enabled =
                    c.spectral & (1 << k) != 0 && s.mask & (1 << k) != 0 && s.bands[k].kind != 0;
                if (!enabled || self.weights[k][i] < 0.001) && self.band_gain[k][i].abs() < 1e-7 {
                    continue;
                }
                let route = routes[k].min(4);
                let ri = route as usize;
                let db = self.db[ri][i];
                self.trigger[k] = self.trigger[k].max(db as f32);
                let local = (self.prefix[ri][b] - self.prefix[ri][a] - self.magnitude[ri][i])
                    / (b - a - 1).max(1) as f64;
                let select = ((db - 20. * local.max(HARMONIC_FLOOR).log10() - 1.).max(0.)
                    / (3. + 9. * f64::from(c.density)))
                .clamp(0., 1.);
                let amount = if c.auto[k] {
                    select
                } else {
                    ((db + tilted - f64::from(c.bands[k][1])) / 12.).clamp(0., 1.) * select
                };
                let target = if enabled {
                    f64::from(c.bands[k][0]) * amount * self.weights[k][i]
                } else {
                    0.
                };
                let a = if target.abs() > self.band_gain[k][i].abs() {
                    attacks[k]
                } else {
                    releases[k]
                };
                self.band_gain[k][i] = target + (self.band_gain[k][i] - target) * a;
                let gain = self.band_gain[k][i];
                self.meters[k] = if gain < 0. {
                    self.meters[k].min(gain as f32)
                } else if gain > 0. {
                    self.meters[k].max(gain as f32)
                } else {
                    self.meters[k]
                };
                match route {
                    1 => self.target[2][i] += gain,
                    2 => self.target[3][i] += gain,
                    3 => self.target[0][i] += gain,
                    4 => self.target[1][i] += gain,
                    _ => {
                        self.target[0][i] += gain;
                        self.target[1][i] += gain;
                    }
                }
            }
        }
        for i in 0..bins {
            for ch in 0..4 {
                self.gain[ch][i] = self.target[ch][i].clamp(-36., 18.);
            }
            let gl = 10f64.powf(self.gain[0][i] / 20.);
            let gr = 10f64.powf(self.gain[1][i] / 20.);
            let gm = 10f64.powf(self.gain[2][i] / 20.);
            let gs = 10f64.powf(self.gain[3][i] / 20.);
            for imaginary in [false, true] {
                let x = if imaginary {
                    &mut self.im
                } else {
                    &mut self.re
                };
                let l = x[0][i] * gl;
                let r = x[1][i] * gr;
                let m = (l + r) * 0.5 * gm;
                let side = (l - r) * 0.5 * gs;
                x[0][i] = m + side;
                x[1][i] = m - side;
                if i > 0 && i < n / 2 {
                    x[0][n - i] = if imaginary { -x[0][i] } else { x[0][i] };
                    x[1][n - i] = if imaginary { -x[1][i] } else { x[1][i] };
                }
            }
        }
        for j in 0..256 {
            let hz = 20. * 1000f64.powf(j as f64 / 255.);
            let bin = (hz * n as f64 / self.rate)
                .round()
                .clamp(0., (bins - 1) as f64) as usize;
            self.response[j] =
                ((self.gain[0][bin] + self.gain[1][bin] + self.gain[2][bin] + self.gain[3][bin])
                    * 0.5) as f32;
        }
        for ch in 0..2 {
            self.fft.run(&mut self.re[ch], &mut self.im[ch], true);
            for j in 0..n {
                let out = (self.time + j) % (n * 4);
                self.output[ch][out] += self.re[ch][j] * self.window[j] / (n as f64 * 2.);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_latency_and_partitioning() {
        let mut a = Processor::new(48000.);
        a.engage();
        let n = 8192;
        let input: Vec<_> = (0..n)
            .map(|i| (i as f64 * 0.137).sin() as f32 * 0.2)
            .collect();
        let mut l = input.clone();
        let mut r = input.iter().map(|v| -v).collect::<Vec<_>>();
        a.process(
            &mut l,
            &mut r,
            &Settings::default(),
            [0; 7],
            Config::default(),
        );
        assert_eq!(a.latency(), 1024);
        for i in 1024..n {
            assert!((l[i] - input[i - 1024]).abs() < 1e-6);
            assert!((l[i] + r[i]).abs() < 1e-6);
        }
        let mut b = Processor::new(48000.);
        b.engage();
        let mut x = input.clone();
        let mut y = input.iter().map(|v| -v).collect::<Vec<_>>();
        for (x, y) in x.chunks_mut(37).zip(y.chunks_mut(37)) {
            b.process(x, y, &Settings::default(), [0; 7], Config::default());
        }
        assert_eq!(l, x);
    }
    #[test]
    fn suppresses_a_moving_resonance_and_preserves_bass() {
        let mut p = Processor::new(48000.);
        p.engage();
        let mut c = Config::default();
        c.harsh = [12., -40.];
        for hz in [100., 3100., 4700.] {
            let x: Vec<f32> = (0..48000)
                .map(|i| 0.4 * (std::f64::consts::TAU * hz * i as f64 / 48000.).sin() as f32)
                .collect();
            let mut l = x.clone();
            let mut r = x.clone();
            p.process(&mut l, &mut r, &Settings::default(), [0; 7], c);
            let ratio = (l[24000..].iter().map(|v| v * v).sum::<f32>()
                / x[24000..].iter().map(|v| v * v).sum::<f32>())
            .sqrt();
            if hz < 1000. {
                assert!(ratio > 0.98);
            } else {
                assert!(ratio < 0.8, "{hz}: {ratio}");
            }
            assert!(l.iter().all(|v| v.is_finite()));
        }
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn spectral_routes_preserve_the_unselected_component() {
        let rate = 48000.;
        let input: Vec<f32> = (0..24000)
            .map(|i| 0.4 * (std::f64::consts::TAU * 4100. * i as f64 / rate).sin() as f32)
            .collect();
        let mut settings = Settings::default();
        settings.bands[3].freq = super::super::freq_pos(4100.);
        settings.bands[3].width = 0.8;
        let mut c = Config::default();
        c.spectral = 1 << 3;
        c.bands[3] = [-12., -60., 0.5, 10.];
        for route in 1..=4 {
            let mut p = Processor::new(rate);
            p.engage();
            let mut l = input.clone();
            let mut r = input.iter().map(|x| x * 0.3).collect::<Vec<_>>();
            let mut routes = [0; 7];
            routes[3] = route;
            p.process(&mut l, &mut r, &settings, routes, c);
            let mut change = 0f32;
            for i in 12000..input.len() {
                let x = input[i - p.latency()];
                let delta = match route {
                    1 => (l[i] - r[i]) - x * 0.7,
                    2 => (l[i] + r[i]) - x * 1.3,
                    3 => r[i] - x * 0.3,
                    _ => l[i] - x,
                };
                assert!(delta.abs() < 1e-6, "route{route}: {delta}");
                change = change.max((l[i] - x).abs() + (r[i] - x * 0.3).abs());
            }
            assert!(
                change > 0.02,
                "route {route} did not process the chosen component"
            );
        }
    }
    #[test]
    fn spectral_envelopes_rates_and_release_to_identity() {
        for rate in [44100., 96000., 192000.] {
            let input: Vec<f32> = (0..(rate as usize / 5))
                .map(|i| 0.4 * (std::f64::consts::TAU * 4100. * i as f64 / rate).sin() as f32)
                .collect();
            let mut s = Settings::default();
            s.bands[3].freq = super::super::freq_pos(4100.);
            s.bands[3].width = 0.8;
            let mut c = Config::default();
            c.spectral = 1 << 3;
            c.bands[3] = [-12., -60., 0.5, 10.];
            let mut fast = Processor::new(rate);
            fast.engage();
            let mut l = input.clone();
            let mut r = input.clone();
            fast.process(&mut l, &mut r, &s, [0; 7], c);
            assert!(
                fast.meters[3] < -3.,
                "rate {rate} reduction {}",
                fast.meters[3]
            );
            let mut partition = Processor::new(rate);
            partition.engage();
            let mut x = input.clone();
            let mut y = input.clone();
            for (x, y) in x.chunks_mut(127).zip(y.chunks_mut(127)) {
                partition.process(x, y, &s, [0; 7], c);
            }
            assert_eq!(l, x);
            assert_eq!(r, y);
            c.bands[3][2] = 200.;
            let mut slow = Processor::new(rate);
            slow.engage();
            let mut x = input.clone();
            let mut y = input.clone();
            slow.process(&mut x, &mut y, &s, [0; 7], c);
            assert!(slow.meters[3].abs() < fast.meters[3].abs() * 0.8);
            assert!(l.iter().all(|x| x.is_finite()));
            let mut x = input.clone();
            let mut y = input.clone();
            let mut off = c;
            off.spectral = 0;
            fast.process(&mut x, &mut y, &s, [0; 7], off);
            let tail = input.len() - fast.n * 2;
            for i in tail..input.len() {
                assert!((x[i] - input[i - fast.latency()]).abs() < 1e-5);
            }
            fast.reset(false);
            assert_eq!(fast.latency(), 0);
        }
    }
}
