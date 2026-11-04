// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent stereo oscillator racks. All delay storage is allocated at setup.
use crate::{controls::*, dsp::Patch};
use std::f32::consts::{PI, TAU};
pub fn needs_lanes(p:&Patch)->bool {
    [DIST_MIX,OTT_MIX,CHORUS_MIX,DELAY_MIX,REVERB_MIX].iter().enumerate().any(|(row,k)| {
        p[*k]>0. && if row==2 || row==3 {FX_ROUTES[row].iter().any(|k|p[*k]<0.5)}
        else {FX_ROUTES[row].iter().any(|k|p[*k]>0.5)}
    })
}
struct Line { data: Vec<f32>, head: usize }
impl Line {
    fn new(n: usize) -> Self { Self { data: vec![0.;n.max(4)], head:0 } }
    fn read(&self, delay:f32)->f32 {
        let at=(self.head as f32-delay.clamp(1.,self.data.len() as f32-2.)).rem_euclid(self.data.len() as f32);
        let i=at as usize%self.data.len(); let f=at-at.floor();
        self.data[i]*(1.-f)+self.data[(i+1)%self.data.len()]*f
    }
    fn push(&mut self,x:f32) { self.data[self.head]=x; self.head=(self.head+1)%self.data.len(); }
    fn clear(&mut self) { self.data.fill(0.);self.head=0; }
}
pub struct Rack {
    rate:f32, phase:f32,
    chorus:[Line;2], delay:[Line;2], reverb:[Line;8],
    damping:[f32;8], tone:[f32;2], low:[f32;2], high:[f32;2], envelope:[f32;3],
    low_coeff:f32, high_coeff:f32,
}
impl Rack {
    pub fn new(rate:f32)->Self {
        Self { rate, phase:0., chorus:std::array::from_fn(|_|Line::new((rate*0.06) as usize+2)),
            delay:std::array::from_fn(|_|Line::new((rate*1.2) as usize+2)),
            reverb:std::array::from_fn(|i|Line::new((rate*(0.031+i as f32*0.0047)*2.) as usize+4)),
            damping:[0.;8],tone:[0.;2],low:[0.;2],high:[0.;2],envelope:[0.;3],
            low_coeff:1.-(-TAU*200./rate).exp(), high_coeff:1.-(-TAU*2500./rate).exp() }
    }
    pub fn reset(&mut self) {
        for line in self.chorus.iter_mut().chain(self.delay.iter_mut()).chain(self.reverb.iter_mut()) {line.clear();}
        self.phase=0.;self.damping.fill(0.);self.tone.fill(0.);self.low.fill(0.);self.high.fill(0.);self.envelope.fill(0.);
    }
    pub fn tick(&mut self, mut x:[f32;2], p:&Patch,o:usize)->[f32;2] {
        let amount=|row:usize,k:usize|p[k]/1000.*p[FX_ROUTES[row][o]].clamp(0.,1.);
        // Saturation with a low-pass tone control and dry/wet blend.
        let mix=amount(0,DIST_MIX);let drive=1.+p[DIST_DRIVE]*0.024;
        let tone=1.-(-TAU*(500.+p[DIST_TONE]*15.)/self.rate).exp();
        for ch in 0..2 {
            let wet=(x[ch]*drive).tanh()/drive.sqrt();
            self.tone[ch]+=tone*(wet-self.tone[ch]);
            x[ch]+=mix*(self.tone[ch]-x[ch]);
        }
        // Complementary three-band split, stereo-linked upward/downward compression.
        let bands: [[f32;2];3] = {
            let mut b=[[0.;2];3];
            for ch in 0..2 {self.low[ch]+=self.low_coeff*(x[ch]-self.low[ch]);self.high[ch]+=self.high_coeff*(x[ch]-self.high[ch]);
                b[0][ch]=self.low[ch];b[1][ch]=self.high[ch]-self.low[ch];b[2][ch]=x[ch]-self.high[ch];}
            b
        };
        let mut compressed=[0.;2];
        let release=1.-(-1./((0.02+p[OTT_TIME]*0.0005)*self.rate)).exp();
        let attack=1.-(-1./(0.002*self.rate)).exp();
        for band in 0..3 {
            let peak=bands[band][0].abs().max(bands[band][1].abs());
            self.envelope[band]+=if peak>self.envelope[band] {attack} else {release}*(peak-self.envelope[band]);
            let env=self.envelope[band].max(1e-8);
            let gain=if env>0.063 { (0.063/env).powf(0.75) } else { (0.063/env).sqrt().min(4.) };
            let gain=1.+(gain-1.)*p[OTT_DEPTH]/1000.;
            for ch in 0..2 {compressed[ch]+=bands[band][ch]*gain;}
        }
        let mix=amount(1,OTT_MIX);
        for ch in 0..2 { x[ch]+=mix*(compressed[ch]-x[ch]); }
        self.phase=(self.phase+(0.05+p[CHORUS_RATE]/200.)/self.rate).fract();
        let mix=amount(2,CHORUS_MIX)*0.5;
        for ch in 0..2 {
            let ms=12.+(self.phase*TAU+ch as f32*PI/2.).sin()*p[CHORUS_DEPTH]/100.;
            let wet=self.chorus[ch].read(ms*0.001*self.rate);
            self.chorus[ch].push(x[ch]);x[ch]+=mix*(wet-x[ch]);
        }
        let mix=amount(3,DELAY_MIX);let route=p[FX_ROUTES[3][o]].clamp(0.,1.);
        let wet: [f32;2]=std::array::from_fn(|ch|self.delay[ch].read((50.+p[DELAY_TIME])*0.001*self.rate));
        for ch in 0..2 {
            self.delay[ch].push((x[ch]*route+wet[1-ch]*p[DELAY_FEEDBACK]/1000.*0.85).tanh());
            x[ch]+=wet[ch]*mix*0.6;
        }
        // Eight-line Householder feedback network; unequal lengths diffuse stereo echoes.
        let size=0.55+p[REVERB_SIZE]/1000.*1.3;
        let taps:[f32;8]=std::array::from_fn(|i|self.reverb[i].read(self.rate*(0.031+i as f32*0.0047)*size));
        let damp=1.-(-TAU*(1200.+(1.-p[REVERB_DAMP]/1000.)*10000.)/self.rate).exp();
        for i in 0..8 {self.damping[i]+=damp*(taps[i]-self.damping[i]);}
        let mean=self.damping.iter().sum::<f32>()*0.25;
        let feedback=0.65+p[REVERB_SIZE]/1000.*0.30;
        let route=p[FX_ROUTES[4][o]].clamp(0.,1.);
        for i in 0..8 {self.reverb[i].push(x[i%2]*route*0.25+(mean-self.damping[i])*feedback);}
        let wet=[taps[0]+taps[2]-taps[4]+taps[6],taps[1]+taps[3]-taps[5]+taps[7]];
        let mix=amount(4,REVERB_MIX);
        for ch in 0..2 {x[ch]=x[ch]*(1.-mix*0.35)+wet[ch]*mix*0.7;}
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_routing_bypass_tails_and_extreme_settings() {
        for rate in [8000.,48000.,96000.] {
            for (row,mix) in [DIST_MIX,OTT_MIX,CHORUS_MIX,DELAY_MIX,REVERB_MIX].into_iter().enumerate() {
                let mut p=crate::dsp::defaults(); p[mix]=1000.;p[FX_ROUTES[row][1]]=0.;
                let mut a=Rack::new(rate);let mut b=Rack::new(rate);
                let mut difference=0.;let mut tail=0.;
                for n in 0..(rate*1.5) as usize {
                    let input=if n<512 {[(n as f32*0.1).sin()*0.2;2]} else {[0.;2]};
                    let wa=a.tick(input,&p,0);let wb=b.tick(input,&p,1);
                    for ch in 0..2 {assert!((wb[ch]-input[ch]).abs()<1e-6,"unassigned oscillator changed");assert!(wa[ch].is_finite()&&wa[ch].abs()<8.);difference+=(wa[ch]-input[ch]).abs();if n>512 {tail+=wa[ch].abs();}}
                }
                assert!(difference>0.01,"effect {row} did not process");
                if row>=2 {assert!(tail>0.001,"missing effect tail {row}");}
                a.reset();for _ in 0..100 {assert_eq!(a.tick([0.;2],&p,0),[0.;2]);}
            }
            let mut rack=Rack::new(rate);let mut p=crate::dsp::defaults();
            for k in [DIST_DRIVE,DIST_MIX,OTT_DEPTH,OTT_MIX,CHORUS_DEPTH,CHORUS_MIX,DELAY_FEEDBACK,DELAY_MIX,REVERB_SIZE,REVERB_MIX] {p[k]=1000.;}
            for n in 0..rate as usize {let y=rack.tick([(n as f32*0.07).sin()*0.8;2],&p,0);assert!(y.iter().all(|v|v.is_finite()&&v.abs()<16.));}
        }
    }
}
