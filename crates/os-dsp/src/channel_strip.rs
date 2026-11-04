//! Native stereo-linked channel strip: saturation, low cut, gate, resonance reduction and compression.
#[derive(Clone, Debug)]
pub struct ChannelStrip {
    rate: f32,
    target: [f32; 8],
    cut: f32,
    wet: f32,
    state: [[f64; 4]; 2],
    envelope: f32,
    gain: f32,
    amount: f32,
    pre: f32,
    pre_last: [f32; 2],
    drive_mix: [f32; 4],
    dc_x: [f32; 2],
    dc_y: [f32; 2],
    gate_env: f32,
    gate_gain: f32,
    gate_hold: u32,
    gate_amount: f32,
    tame: crate::tame::Tame,
    deess: crate::debess::DeBess,
    vocal_deess: bool,
}
impl Default for ChannelStrip {
    fn default() -> Self {
        Self::new([0; 2], 48000)
    }
}
pub fn cut_hz(v: f32) -> f32 {
    20.0 * 17.5f32.powf(v.clamp(0.0, 1.0))
}
impl ChannelStrip {
    pub fn new<const N: usize>(settings: [u16; N], rate: u32) -> Self {
        let mut target = [0.; 8];
        for (i, v) in settings.iter().take(7).enumerate() {
            target[i] = f32::from(*v) / 65535.0;
        }
        Self {
            rate: rate.max(1) as f32,
            target,
            cut: cut_hz(target[0]),
            wet: if target[0] > 0.0 { 1.0 } else { 0.0 },
            state: [[0.0; 4]; 2],
            envelope: 0.0,
            gain: 1.0,
            amount: target[1],
            pre: target[2],
            pre_last: [0.; 2],
            drive_mix: std::array::from_fn(|i| if i == settings.get(6).copied().unwrap_or(0).min(3) as usize {1.} else {0.}),
            dc_x: [0.; 2],
            dc_y: [0.; 2],
            gate_env: 0.,
            gate_gain: 1.,
            gate_hold: 0,
            gate_amount: target[3],
            tame: crate::tame::Tame::new(rate),
            deess: crate::debess::DeBess::new(rate),
            vocal_deess: false,
        }
    }
    pub fn with_deess(mut self, amount: u16) -> Self { self.vocal_deess=true; self.target[7]=f32::from(amount)/65535.; self }
    pub fn deess(&mut self, buf: &mut [f32]) { if self.vocal_deess {self.deess.process(buf, self.target[7]);} }
    pub fn settings(&self) -> [f32; 8] {
        self.target
    }
    pub fn automate(&mut self, control: usize, value: f32) {
        if let Some(v) = self.target.get_mut(control) {
            *v = value.clamp(0.0, 1.0);
        }
    }
    pub fn inherit(&mut self, old: &Self) {
        self.cut = old.cut;
        self.wet = old.wet;
        self.state = old.state;
        self.pre = old.pre;
        self.pre_last = old.pre_last;
        self.drive_mix = old.drive_mix;
        self.dc_x = old.dc_x;
        self.dc_y = old.dc_y;
        self.gate_env = old.gate_env;
        self.gate_gain = old.gate_gain;
        self.gate_hold = old.gate_hold;
        self.gate_amount = old.gate_amount;
        if self.rate == old.rate {
            self.tame = old.tame.clone();
            self.deess = old.deess.clone();
        }
        self.envelope = old.envelope;
        self.gain = old.gain;
        self.amount = old.amount;
    }
    pub fn low_cut(&mut self, buf: &mut [f32]) {
        let goal = if self.target[0] > 0.0 { 1.0 } else { 0.0 };
        if goal == 0.0 && self.wet < 0.00001 {
            self.state = [[0.0; 4]; 2];
            self.wet = 0.0;
            return;
        }
        // Coefficients stay constant within each block; cutoff and bypass move gently between blocks.
        let step = 1.0 - (-(buf.len() as f32) / 2.0 / (self.rate * 0.02)).exp();
        self.cut += (cut_hz(self.target[0]) - self.cut) * step;
        let w = 2.0 * std::f64::consts::PI * f64::from(self.cut.min(self.rate * 0.45))
            / f64::from(self.rate);
        let (sn, cs) = w.sin_cos();
        let alpha = sn / std::f64::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cs) * 0.5 / a0;
        let b1 = -(1.0 + cs) / a0;
        let b2 = b0;
        let a1 = -2.0 * cs / a0;
        let a2 = (1.0 - alpha) / a0;
        let smooth = 1.0 - (-1.0 / (self.rate * 0.01)).exp();
        for frame in buf.chunks_exact_mut(2) {
            self.wet += (goal - self.wet) * smooth;
            for (ch, x) in frame.iter_mut().enumerate() {
                let v = f64::from(*x);
                let s = &mut self.state[ch];
                let y = b0 * v + b1 * s[0] + b2 * s[1] - a1 * s[2] - a2 * s[3];
                *s = [v, s[0], y, s[2]];
                *x += (y as f32 - *x) * self.wet;
            }
        }
    }
    pub fn pre(&mut self, buf: &mut [f32]) {
        if self.target[2] == 0. && self.pre < 0.00001 {
            self.pre = 0.;
            self.pre_last = [0.; 2];
            self.dc_x = [0.; 2];
            self.dc_y = [0.; 2];
            let kind=((self.target[6]*65535.).round() as usize).min(3);
            self.drive_mix=std::array::from_fn(|i|if i==kind{1.}else{0.});
            return;
        }
        let smooth = 1. - (-1. / (self.rate * 0.02)).exp();
        let dc = (-2. * std::f32::consts::PI * 8. / self.rate).exp();
        for f in buf.chunks_exact_mut(2) {
            self.pre += (self.target[2] - self.pre) * smooth;
            let drive = 1. + 7. * self.pre;
            let kind=((self.target[6]*65535.).round() as usize).min(3);
            for (i,w) in self.drive_mix.iter_mut().enumerate() {
                let goal=if i==kind {1.}else{0.};
                *w+=(goal-*w)*smooth;
                if (*w-goal).abs()<0.00001 {*w=goal;}
            }
            for (c, x) in f.iter_mut().enumerate() {
                let v=(*x*drive).clamp(-40.,40.);
                let last=self.pre_last[c];
                self.pre_last[c]=v;
                let mut shaped=0.;
                for (mode,weight) in self.drive_mix.iter().copied().enumerate().filter(|(_,w)|*w>0.) {
                    let bias=if mode==0 {0.12*self.pre}else{0.};
                    let a=v+bias;
                    let b=last+bias;
                    let sat=if (a-b).abs()>0.0001 {
                        ((drive_integral(mode,a)-drive_integral(mode,b))/f64::from(a-b)) as f32
                    }else{drive_curve(mode,(a+b)*0.5)};
                    // Normalize the small-signal slope, as the original PRE did. This is
                    // fixed compensation, not a loudness-following automatic gain rider.
                    let slope=if mode==0 {1.-bias.tanh().powi(2)}else{1.};
                    shaped+=weight*(sat-drive_curve(mode,bias))/(drive*slope);
                }
                let filtered = shaped - self.dc_x[c] + dc * self.dc_y[c];
                self.dc_x[c] = shaped;
                self.dc_y[c] = filtered;
                *x += (filtered - *x) * self.pre;
            }
        }
    }
    pub fn gate(&mut self, buf: &mut [f32]) {
        if self.target[3] == 0. && self.gate_amount < 0.00001 {
            self.gate_amount = 0.;
            self.gate_gain = 1.;
            self.gate_env = 0.;
            self.gate_hold = 0;
            return;
        }
        let smooth = 1. - (-1. / (self.rate * 0.02)).exp();
        let attack = 1. - (-1. / (self.rate * 0.001)).exp();
        let release = 1. - (-1. / (self.rate * 0.08)).exp();
        for f in buf.chunks_exact_mut(2) {
            self.gate_amount += (self.target[3] - self.gate_amount) * smooth;
            let level = f[0].abs().max(f[1].abs());
            self.gate_env += (level - self.gate_env)
                * if level > self.gate_env {
                    attack
                } else {
                    release
                };
            let threshold = 10f32.powf((-72. + 54. * self.gate_amount) / 20.);
            if self.gate_env >= threshold {
                self.gate_hold = (self.rate * 0.04) as u32;
            } else {
                self.gate_hold = self.gate_hold.saturating_sub(1);
            }
            let goal = if self.gate_hold > 0 {
                1.
            } else {
                (self.gate_env / (threshold * 0.5).max(1e-8))
                    .powi(3)
                    .clamp(0.001, 1.)
            };
            self.gate_gain += (goal - self.gate_gain)
                * if goal > self.gate_gain {
                    attack
                } else {
                    release
                };
            let gain = 1. + (self.gate_gain - 1.) * (self.gate_amount * 50.).min(1.);
            f[0] *= gain;
            f[1] *= gain;
        }
    }
    pub fn tame(&mut self, buf: &mut [f32]) {
        self.tame.process(buf, self.target[4]);
    }
    pub fn compress(&mut self, buf: &mut [f32]) {
        if self.target[1] == 0.0 && self.amount < 0.00001 {
            self.amount = 0.0;
            self.gain = 1.0;
            self.envelope = 0.0;
            return;
        }
        let profile = &PROFILES[((self.target[5] * 65535.).round() as usize).min(5)];
        let attack = (-1.0 / (self.rate * profile.attack)).exp();
        let release = (-1.0 / (self.rate * profile.release)).exp();
        let smooth = 1.0 - (-1.0 / (self.rate * 0.020)).exp();
        for frame in buf.chunks_exact_mut(2) {
            self.amount += (self.target[1] - self.amount) * smooth;
            let power = (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
            let coeff = attack;
            self.envelope = coeff * self.envelope + (1.0 - coeff) * power;
            let threshold = -6.0 - profile.range * self.amount;
            let ratio = profile.ratio + 2.0 * self.amount;
            let reduction = |db: f32| {
                let over = db - threshold;
                let slope = 1.0 - 1.0 / ratio;
                if over <= -3.0 {
                    0.0
                } else if over >= 3.0 {
                    over * slope
                } else {
                    (over + 3.0).powi(2) * slope / 12.0
                }
            };
            let db = 10.0 * self.envelope.max(1e-20).log10();
            // Fixed-reference automatic makeup avoids chasing silence or pumping gain between phrases.
            let makeup = reduction(-18.0).min(18.0);
            let wanted = 10.0f32.powf((makeup - reduction(db)) * self.amount / 20.0);
            let response = if wanted < self.gain { attack } else { release };
            self.gain = response * self.gain + (1.0 - response) * wanted;
            for x in frame {
                *x *= self.gain;
            }
        }
    }
}
/// Lightweight waveshaping characters, not circuit/tape-machine emulations.
pub const DRIVE_TYPES: [&str;4] = ["Tube", "Tape", "Soft", "Clip"];
fn drive_curve(mode:usize,x:f32)->f32 {
    match mode {
        1=>x/(1.+x.abs()),
        3=>x.clamp(-1.,1.),
        _=>x.tanh(),
    }
}
fn drive_integral(mode:usize,x:f32)->f64 {
    let a=f64::from(x).abs();
    match mode {
        1=>a-a.ln_1p(),
        3=>if a<=1. {0.5*a*a}else{a-0.5},
        _=>a+(-2.*a).exp().ln_1p()-std::f64::consts::LN_2,
    }
}

pub struct Profile {
    pub name: &'static str,
    pub attack: f32,
    pub release: f32,
    pub ratio: f32,
    pub range: f32,
}
pub const PROFILES: [Profile; 6] = [
    Profile {
        name: "General",
        attack: 0.010,
        release: 0.150,
        ratio: 2.,
        range: 30.,
    },
    Profile {
        name: "Vocal",
        attack: 0.008,
        release: 0.100,
        ratio: 2.,
        range: 28.,
    },
    Profile {
        name: "Guitar",
        attack: 0.015,
        release: 0.120,
        ratio: 2.,
        range: 26.,
    },
    Profile {
        name: "Bass",
        attack: 0.025,
        release: 0.200,
        ratio: 2.5,
        range: 30.,
    },
    Profile {
        name: "Synthesizer",
        attack: 0.012,
        release: 0.180,
        ratio: 1.5,
        range: 26.,
    },
    Profile {
        name: "Drums",
        attack: 0.025,
        release: 0.080,
        ratio: 2.5,
        range: 24.,
    },
];
#[cfg(test)]
mod tests {
    use super::*;
    fn tone(hz: f32, amp: f32) -> Vec<f32> {
        (0..48000)
            .flat_map(|i| {
                let x = amp * (2.0 * std::f32::consts::PI * hz * i as f32 / 48000.0).sin();
                [x, x]
            })
            .collect()
    }
    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }
    #[test]
    fn channel_strip_audio_contract() {
        let dry = tone(1000.0, 0.5);
        let mut b = dry.clone();
        let mut off = ChannelStrip::default();
        off.low_cut(&mut b);
        off.compress(&mut b);
        assert_eq!(b, dry);
        let mut low = tone(30.0, 0.5);
        let mut high = tone(4000.0, 0.5);
        ChannelStrip::new([65535, 0], 48000).low_cut(&mut low);
        ChannelStrip::new([65535, 0], 48000).low_cut(&mut high);
        assert!(rms(&low[48000..]) < 0.005);
        assert!(rms(&high[48000..]) > 0.34);
        let mut c = ChannelStrip::new([0, 65535], 48000);
        let mut loud = tone(1000.0, 0.9);
        c.compress(&mut loud);
        assert!(rms(&loud[48000..]) < 0.35);
        assert!(loud.iter().all(|v| v.is_finite()));
        assert!(loud.chunks_exact(2).all(|f| f[0] == f[1]));
        let mut silence = vec![0.0; 96000];
        c.compress(&mut silence);
        assert!(silence.iter().all(|v| *v == 0.0));
        let mut ref_tone = tone(1000.0, 0.178);
        ChannelStrip::new([0, 65535], 48000).compress(&mut ref_tone);
        assert!((rms(&ref_tone[48000..]) / 0.126 - 1.0).abs() < 0.12);
    }
}

#[cfg(test)]
mod expanded_tests {
    use super::*;
    fn signal(rate: u32, hz: f32, amp: f32) -> Vec<f32> {
        (0..rate)
            .flat_map(|i| {
                let x = amp * (2. * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
                [x, x]
            })
            .collect()
    }
    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|v| v * v).sum::<f32>() / b.len() as f32).sqrt()
    }
    #[test]
    fn drive_types_are_distinct_finite_and_preserve_state() {
        for rate in [44100,48000,96000] {
            let dry=signal(rate,997.,0.65);
            let mut rendered=Vec::new();
            for mode in 0..4 {
                let mut off=dry.clone();
                ChannelStrip::new([0,0,0,0,0,0,mode],rate).pre(&mut off);
                assert_eq!(off,dry,"zero drive is an exact bypass for every type");
                let mut c=ChannelStrip::new([0,0,50000,0,0,0,mode],rate);
                let mut b=dry.clone();
                for chunk in b.chunks_mut(254) {c.pre(chunk);}
                assert!(b.iter().all(|x|x.is_finite()&&x.abs()<1.));
                assert!(b.chunks_exact(2).all(|f|f[0]==f[1]));
                assert!(rms(&b)>0.01);
                let mut whole=dry.clone();
                ChannelStrip::new([0,0,50000,0,0,0,mode],rate).pre(&mut whole);
                assert_eq!(b,whole,"partition independent");
                let mut next=ChannelStrip::new([0,0,50000,0,0,0,(mode+1)%4],rate);
                next.inherit(&c);
                assert_eq!(next.drive_mix,c.drive_mix,"type switches inherit crossfade state");
                let mut tail=vec![0.25;4096];let mut baseline=tail.clone();
                c.pre(&mut baseline);next.pre(&mut tail);
                assert!(tail.iter().all(|x|x.is_finite()&&x.abs()<1.));
                let delta:Vec<_>=tail.iter().zip(&baseline).map(|(x,y)|x-y).collect();
                assert!(delta[0].abs()<0.001);
                assert!(delta.windows(2).all(|w|(w[1]-w[0]).abs()<0.001),"type change itself must be smoothed");
                rendered.push(b);
            }
            for a in 0..4 {for b in a+1..4 {
                let difference=rendered[a].iter().zip(&rendered[b]).map(|(x,y)|(x-y).abs()).sum::<f32>()/dry.len() as f32;
                assert!(difference>0.0001,"types {a} and {b} must differ: {difference}");
            }}
        }
    }

    #[test]
    fn expanded_strip_audio_contract() {
        for rate in [44100, 48000, 96000] {
            let dry = signal(rate, 1000., 0.8);
            let mut b = dry.clone();
            let mut c = ChannelStrip::new([0; 6], rate);
            c.low_cut(&mut b);
            c.gate(&mut b);
            c.pre(&mut b);
            c.tame(&mut b);
            c.compress(&mut b);
            assert_eq!(b, dry);
            let mut pre = dry.clone();
            ChannelStrip::new([0, 0, 50000, 0, 0, 0], rate).pre(&mut pre);
            assert!(rms(&pre) < rms(&dry) * 0.8);
            assert!(pre.iter().all(|x| x.is_finite()));
            let mut quiet = signal(rate, 1000., 0.0001);
            ChannelStrip::new([0, 0, 0, 40000, 0, 0], rate).gate(&mut quiet);
            assert!(rms(&quiet[rate as usize..]) < 0.000001);
            let mut loud = dry.clone();
            ChannelStrip::new([0, 0, 0, 40000, 0, 0], rate).gate(&mut loud);
            assert!((rms(&loud) / rms(&dry) - 1.).abs() < 0.01);
            let hz = 250. * 48f32.powf(7. / 15.);
            let mut tone = signal(rate, hz, 0.4);
            let original = rms(&tone);
            ChannelStrip::new([0, 0, 0, 0, 65535, 0], rate).tame(&mut tone);
            assert!(
                rms(&tone[rate as usize..]) < original * 0.85,
                "resonance must be reduced"
            );
            for profile in 0..6 {
                let mut c = ChannelStrip::new([30000, 40000, 30000, 20000, 40000, profile], rate);
                let mut b = dry.clone();
                c.low_cut(&mut b);
                c.gate(&mut b);
                c.pre(&mut b);
                c.tame(&mut b);
                c.compress(&mut b);
                assert!(b.iter().all(|v| v.is_finite() && v.abs() < 2.));
                assert!(b.chunks_exact(2).all(|f| f[0] == f[1]));
            }
        }
    }
}
