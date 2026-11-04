// Adapted from Airwindows DeBess, Copyright (c) 2016 Chris Johnson.
// SPDX-License-Identifier: MIT
// See ../../../licenses/Airwindows-MIT.txt.
// Source: https://github.com/airwindows/airwindows/blob/master/plugins/WinVST/DeBess/DeBessProc.cpp
// Port: fixed Sharp=.5, Depth=.5, Filter=.5; Intense is the DS knob.
// Omits plugin dither/noise injection, adds smooth amount/bypass transitions.
// No lookahead or delay line in the audio path.
#[derive(Clone, Debug)]
pub struct DeBess {
    rate: f64,
    samples: [[f64; 41]; 2],
    slopes: [[f64; 41]; 2],
    low: [[f64; 2]; 2],
    ratio: [[f64; 2]; 2],
    flip: usize,
    amount: f64,
    wet: f64,
}
impl DeBess {
    pub fn new(rate: u32) -> Self {
        Self {rate: rate.max(1) as f64, samples:[[0.;41];2], slopes:[[0.;41];2], low:[[0.;2];2], ratio:[[1.;2];2], flip:0, amount:0., wet:0.}
    }
    pub fn process(&mut self, buf: &mut [f32], amount: f32) {
        let goal = f64::from(amount.clamp(0.,1.));
        if goal == 0. && self.wet < 1e-6 {
            *self = Self::new(self.rate as u32);
            return;
        }
        let smoothing=1.-(-1./(self.rate*0.005)).exp();
        let depth=1./0.5001;
        for frame in buf.chunks_exact_mut(2) {
            self.amount+=(goal-self.amount)*smoothing;
            self.wet+=((if goal>0. {1.}else{0.})-self.wet)*smoothing;
            let intensity=self.amount.powi(5)*(8192./(self.rate/44100.));
            let bank=self.flip;
            for ch in 0..2 {
                let input=f64::from(frame[ch]);
                let s=&mut self.samples[ch];
                let m=&mut self.slopes[ch];
                s[0]=input;
                for x in (1..=20).rev() {s[x]=s[x-1];}
                m[1]=(s[1]-s[2])*((s[1]-s[2])/1.3);
                for x in (2..20).rev() {m[x]=(s[x]-s[x+1])*((s[x-1]-s[x])/1.3);}
                let mut sense=(m[1]-m[2]).abs()*400.;
                for x in (1..20).rev() {
                    let mult=(m[x]-m[x+1]).abs()*400.;
                    if mult<1. {sense*=mult;}
                }
                sense=(1.+intensity*intensity*sense).min(intensity);
                self.low[ch][bank]=self.low[ch][bank]*0.5+input*0.5;
                self.ratio[ch][bank]=(self.ratio[ch][bank]*0.995+sense*0.005).min(depth);
                // Preserve upstream's A-bank gate for both alternating banks.
                let output=if self.ratio[ch][1]>1. {self.low[ch][bank]+(input-self.low[ch][bank])/self.ratio[ch][bank]}else{input};
                frame[ch]=(input+(output-input)*self.wet) as f32;
            }
            self.flip^=1;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debess_reduces_sibilance_preserves_tone_and_block_continuity() {
        for rate in [44100,48000,96000] {
            let mut seed=12345u32;
            let noise:Vec<f32>=(0..rate/2).flat_map(|_| {seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;let x=(seed as i32 as f32/i32::MAX as f32)*0.6;[x,x]}).collect();
            let mut full=noise.clone();DeBess::new(rate).process(&mut full,0.5);
            let energy=|b:&[f32]| b.iter().skip(2000).map(|x|f64::from(*x).powi(2)).sum::<f64>();
            assert!(energy(&full)<energy(&noise)*0.9,"sibilance reduction at {rate}");
            let mut blocks=noise.clone();let mut d=DeBess::new(rate);for b in blocks.chunks_mut(126){d.process(b,0.5);}assert_eq!(full,blocks);
            assert!(full.iter().all(|x|x.is_finite()));
            assert!(full.chunks_exact(2).all(|f|f[0]==f[1]));
            let mut bypass=noise.clone();DeBess::new(rate).process(&mut bypass,0.);assert_eq!(bypass,noise);
            let tone:Vec<f32>=(0..rate/2).flat_map(|i|{let x=(i as f32*std::f32::consts::TAU*220./rate as f32).sin()*0.3;[x,x]}).collect();
            let mut clean=tone.clone();DeBess::new(rate).process(&mut clean,0.5);assert_eq!(clean,tone);
            let mut impulse=vec![0.;128];impulse[0]=1.;impulse[1]=1.;DeBess::new(rate).process(&mut impulse,0.5);assert_eq!(impulse[0],1.);
        }
    }
}
