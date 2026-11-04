//! SnapTune's AGPL-3.0 DSP, isolated behind an owned C ABI instance.
use std::{ffi::c_void, ptr::NonNull};
unsafe extern "C" {
 fn lemon_tune_new(rate:f64)->*mut c_void;
 fn lemon_tune_delete(p:*mut c_void);
 fn lemon_tune_reset(p:*mut c_void);
 fn lemon_tune_clear_learning(p:*mut c_void);
 fn lemon_tune_stats(p:*mut c_void,out:*mut f64);
 fn lemon_tune_latency(p:*mut c_void)->i32;
 fn lemon_tune_process(p:*mut c_void,buf:*mut f32,frames:i32,strength:f32,key:i32,scale:i32,voice:i32,learn:bool);
}
#[derive(Debug)]
pub struct SnapTune { ptr:NonNull<c_void>, pub amount:f32, pub key:u8, pub scale:u8, pub voice:u8, pub learn:u32, pub enabled:bool, active:bool }
// Each native instance is exclusively owned and processed through &mut self.
unsafe impl Send for SnapTune {}
impl SnapTune {
 pub fn new(rate:u32,settings:[u16;4],learn:u32)->Self {
  let [amount,key,scale,voice]=settings;
  let ptr=NonNull::new(unsafe{lemon_tune_new(rate.max(1) as f64)}).expect("SnapTune allocation");
  Self{ptr,amount:f32::from(amount)/65535.,key:key.min(11) as u8,scale:scale.min(12) as u8,voice:voice.min(1) as u8,learn,enabled:true,active:false}
 }
 pub fn with_enabled(mut self,enabled:bool)->Self {self.enabled=enabled;self}
 pub fn configure(&mut self,amount:f32,key:u8,scale:u8,voice:u8,learn:u32,enabled:bool){
  if self.enabled!=enabled {self.active=false;}self.enabled=enabled;
  self.amount=amount;self.key=key;self.scale=scale;self.voice=voice;
  if self.learn!=learn {unsafe{lemon_tune_clear_learning(self.ptr.as_ptr())};self.learn=learn;}
 }
 pub fn suggestion(&self)->Option<(u8,u8)> {
  let mut stats=[0.;13];unsafe{lemon_tune_stats(self.ptr.as_ptr(),stats.as_mut_ptr())};
  if stats[12]<6. {return None;} estimate_key(stats[..12].try_into().unwrap())
 }
 pub fn latency(&self)->u32 {if self.enabled && self.amount>0. {unsafe{lemon_tune_latency(self.ptr.as_ptr()).max(0) as u32}}else{0}}
 pub fn process(&mut self,buf:&mut [f32]) {
  if (!self.enabled || self.amount<=0.) && self.learn==0 {self.active=false;return;}
  if !self.active {unsafe{lemon_tune_reset(self.ptr.as_ptr())};self.active=true;}
  for part in buf.chunks_mut(8192) {unsafe{lemon_tune_process(self.ptr.as_ptr(),part.as_mut_ptr(),(part.len()/2) as i32,if self.enabled{self.amount.clamp(0.,1.)}else{0.},self.key.into(),self.scale.into(),self.voice.into(),self.learn!=0)};}
 }
}
impl Drop for SnapTune {fn drop(&mut self){unsafe{lemon_tune_delete(self.ptr.as_ptr())}}}
pub const KEYS:[&str;12]=["C","C# / Db","D","D# / Eb","E","F","F# / Gb","G","G# / Ab","A","A# / Bb","B"];
pub const SCALES:[&str;13]=["Chromatic","Major","Minor","Harmonic minor","Melodic minor","Major pentatonic","Minor pentatonic","Blues","Dorian","Phrygian","Lydian","Mixolydian","Locrian"];

/// Duration-weighted pitch-class profile matching. Only major/minor are inferred;
/// other scales remain explicit menu choices. Relative keys can be ambiguous on a melody alone.
fn estimate_key(h:[f64;12])->Option<(u8,u8)> {
 let total:f64=h.iter().sum();
 if total<=0. || h.iter().filter(|&&x|x>total*0.035).count()<4 {return None;}
 let profiles=[[6.35,2.23,3.48,2.33,4.38,4.09,2.52,5.19,2.39,3.66,2.29,2.88],
 [6.33,2.68,3.52,5.38,2.60,3.53,2.54,4.75,3.98,2.69,3.34,3.17]];
 let mean=total/12.;let mut best=(-2.,0,1);
 for (mode,p) in profiles.iter().enumerate(){let pm=p.iter().sum::<f64>()/12.;
  for key in 0..12 {let mut dot=0.;let mut a=0.;let mut b=0.;
   for i in 0..12 {let x=h[(i+key)%12]-mean;let y=p[i]-pm;dot+=x*y;a+=x*x;b+=y*y;}
   let score=dot/(a*b).sqrt().max(1e-12);
   if score>best.0 {best=(score,key as u8,(mode+1) as u8);}
  }
 }
 if best.0<0.45 {None}else{Some((best.1,best.2))}
}

#[cfg(test)]
mod tests {
 use super::*;
 fn tone(rate:u32,hz:f32,seconds:f32)->Vec<f32>{(0..(rate as f32*seconds) as usize).flat_map(|i|{let x=0.2*(std::f32::consts::TAU*hz*i as f32/rate as f32).sin();[x,x]}).collect()}
 fn frequency(buf:&[f32],rate:u32)->f64{
  let mono:Vec<_>=buf.chunks_exact(2).map(|f|f[0]).collect();
  let crossings:Vec<_>=mono.windows(2).enumerate().filter_map(|(i,w)|if w[0]<=0. && w[1]>0.{Some(i as f64 + f64::from(-w[0]/(w[1]-w[0])))}else{None}).collect();
  (crossings.len()-1) as f64 *rate as f64/(crossings.last().unwrap()-crossings[0])
 }
 #[test] fn snaptune_audio_and_learning_contract(){
  for rate in [44100,48000,96000] {
   let input=tone(rate,224.,1.5);let mut out=input.clone();let mut off=SnapTune::new(rate,[0;4],0);
   off.process(&mut out);assert_eq!(out,input);assert_eq!(off.latency(),0);
   let mut tune=SnapTune::new(rate,[65535,0,0,0],0);assert!(tune.latency()>0);
   tune.process(&mut out);assert!(out.iter().all(|v|v.is_finite()));
   let hz=frequency(&out[(rate as usize)*2..],rate);assert!((hz-220.).abs()<1.,"{rate}: corrected {hz}");
   assert!(out.chunks_exact(2).all(|f|f[0]==f[1]));
   let mut sliced=input.clone();let mut other=SnapTune::new(rate,[65535,0,0,0],0);
   for part in sliced.chunks_mut(74){other.process(part);}
   assert!(out.iter().zip(sliced).all(|(a,b)|(a-b).abs()<1e-6),"block invariance");
  }
  let mut switched=SnapTune::new(48000,[55000,9,2,1],0).with_enabled(false);
  let mut original=tone(48000,450.,1.);let dry=original.clone();switched.process(&mut original);assert_eq!(original,dry);assert_eq!(switched.latency(),0);
  switched.configure(switched.amount,9,2,1,0,true);switched.process(&mut original);assert_ne!(original,dry);assert!(switched.latency()>0);
  switched.configure(switched.amount,9,2,1,1,false);let mut learning=dry.clone();switched.process(&mut learning);assert_eq!(learning,dry);assert_eq!(switched.latency(),0);assert!(switched.amount>0.8);
  let mut female=SnapTune::new(48000,[65535,0,0,1],0);let mut high=tone(48000,450.,1.5);female.process(&mut high);
  assert!((frequency(&high[96000..],48000)-440.).abs()<1.);
  let mut learner=SnapTune::new(48000,[0;4],1);
  let mut silence=vec![0.;48000*2*8];learner.process(&mut silence);assert_eq!(learner.suggestion(),None);
  // Weighted C-major phrase; analysis must leave original audio byte-for-byte unchanged.
  for (note,duration) in [(60,2.),(64,1.5),(67,1.5),(62,0.7),(65,0.7),(69,0.7),(71,0.7),(60,2.)] {
   let mut phrase=tone(48000,440.*2f32.powf((note as f32-69.)/12.),duration);let dry=phrase.clone();learner.process(&mut phrase);assert_eq!(phrase,dry);
  }
  assert_eq!(learner.suggestion(),Some((0,1)));
  learner.configure(0.,0,0,0,2,true);assert_eq!(learner.suggestion(),None,"new learning clears old notes");
  assert_eq!(estimate_key([1.;12]),None);assert_eq!(estimate_key([0.;12]),None);
 }
}
