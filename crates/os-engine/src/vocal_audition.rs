// SPDX-License-Identifier: GPL-3.0-or-later
//! A small, independent, click-free vocal-note audition voice. No decoding or rendering in the callback.
use std::sync::Arc;
use crate::Sample;
pub struct Voice{sample:Arc<Sample>,start:usize,end:usize,seam:usize,pos:f64,step:f64,target:f64,gain:f32,wanted:f32,fade:f32,releasing:bool,rate:u32}
impl Voice{
 pub fn new(sample:Arc<Sample>,start:usize,end:usize,cents:f32,gain:f32,rate:u32)->Option<Self>{let mut end=end.min(sample.frames()).min(sample.right.len());let mut start=start;if start+8>=end{return None;}
 let window=(sample.rate as usize/50).min((end-start)/4);let crossing=|i:usize|sample.left[i-1]<=0.&&sample.left[i]>0.;
 if let(Some(a),Some(b))=((start+1..start+window).find(|i|crossing(*i)),(end-window..end).rev().find(|i|crossing(*i))){if b>a+8{start=a;end=b;}}
 let period=(start+1..(start+window).min(end)).find(|i|crossing(*i)).map(|i|i-start);let seam=period.filter(|p|*p>=sample.rate as usize/2000&&*p<=(end-start)/4).unwrap_or((sample.rate as usize/200).min((end-start)/4).max(1));
 let mut v=Self{sample,start,end,seam,pos:start as f64,step:1.,target:1.,gain:0.,wanted:gain,fade:0.,releasing:false,rate};v.tune(cents,gain);v.step=v.target;Some(v)}
 pub fn tune(&mut self,cents:f32,gain:f32){if cents.is_finite()&&gain.is_finite(){self.target=2f64.powf(f64::from(cents.clamp(-2400.,2400.))/1200.)*f64::from(self.sample.rate)/f64::from(self.rate.max(1));self.wanted=gain.clamp(0.,4.);}}
 pub fn release(&mut self){self.releasing=true;}
 pub fn active(&self)->bool{!self.releasing||self.fade>0.}
 pub fn mix(&mut self,out:&mut[f32]){let ramp=1./(self.rate.max(1)as f32*0.005);let smoothing=1.-(-1./(self.rate.max(1)as f64*0.008)).exp();let len=self.end-self.start;let seam=self.seam;
  let read=|data:&[f32],p:f64|{let i=(p.floor()as usize).clamp(self.start,self.end-1);let j=(i+1).min(self.end-1);data[i]+(data[j]-data[i])*(p-i as f64)as f32};
  for frame in out.chunks_exact_mut(2){self.fade=(self.fade+if self.releasing{-ramp}else{ramp}).clamp(0.,1.);self.step+=(self.target-self.step)*smoothing;self.gain+=(self.wanted-self.gain)*smoothing as f32;let mut l=read(&self.sample.left,self.pos);let mut r=read(&self.sample.right,self.pos);
   if self.pos>(self.end-seam)as f64{let t=((self.pos-(self.end-seam)as f64)/seam as f64).clamp(0.,1.)as f32;let other=self.start as f64+self.pos-(self.end-seam)as f64;l+=(read(&self.sample.left,other)-l)*t;r+=(read(&self.sample.right,other)-r)*t;}
   let gain=self.gain*self.fade*0.7;frame[0]+=l*gain;frame[1]+=r*gain;self.pos+=self.step;if self.pos>=self.end as f64{self.pos=self.start as f64+seam as f64+(self.pos-self.end as f64)%(len-seam)as f64;}
  }
 }
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn vocal_audition_tracks_pitch_without_restarting_and_releases(){let rate=24000;let x:Vec<_>=(0..rate).map(|i|(i as f32*220.*std::f32::consts::TAU/rate as f32).sin()*0.3).collect();let s=Arc::new(Sample{rate,left:x.clone(),right:x});let mut v=Voice::new(s,0,rate as usize,0.,1.,rate).unwrap();let mut first=vec![0.;rate as usize];v.mix(&mut first);let pos=v.pos;v.tune(1200.,1.);assert_eq!(v.pos,pos);let mut out=vec![0.;rate as usize];v.mix(&mut out);let left:Vec<_>=out.chunks_exact(2).skip(2400).map(|f|f[0]).collect();let crossings=left.windows(2).filter(|w|w[0]<=0.&&w[1]>0.).count();let hz=crossings as f32*rate as f32/left.len()as f32;assert!((hz-440.).abs()<8.,"{hz}");assert!(out.iter().all(|v|v.is_finite()));v.release();v.mix(&mut vec![0.;2400]);assert!(!v.active());}
 #[test]fn vocal_loop_aligns_periods_and_avoids_boundary_spikes(){let rate=24000;let x:Vec<_>=(0..13700).map(|i|((i as f32+17.)*220.*std::f32::consts::TAU/rate as f32).sin()*0.3).collect();let sample=Arc::new(Sample{rate,left:x.clone(),right:x});let mut v=Voice::new(sample,13,13681,0.,1.,rate).unwrap();assert!((v.seam as f32-rate as f32/220.).abs()<2.);let mut out=vec![0.;rate as usize*8];v.mix(&mut out);let left:Vec<_>=out.chunks_exact(2).map(|f|f[0]).collect();let jump=left.windows(2).map(|w|(w[1]-w[0]).abs()).fold(0f32,f32::max);assert!(jump<0.04,"loop boundary spike: {jump}");}

}
