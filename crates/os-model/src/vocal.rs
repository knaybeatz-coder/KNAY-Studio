// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent, original-source-relative vocal edits. Playlist slices share source coordinates.
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Note{
 pub start:u32,pub end:u32,pub base:f32,pub shift:f32,pub variation:f32,
 #[serde(default="one")]pub drift:f32,
 #[serde(default)]pub transition:f32,
 #[serde(default)]pub formant:f32,
 #[serde(default)]pub gain:f32,
 #[serde(default)]pub muted:bool,
}
fn one()->f32{1.}
impl Note{pub fn new(start:u32,end:u32,base:f32)->Self{Self{start,end,base,shift:0.,variation:1.,drift:1.,transition:0.,formant:0.,gain:0.,muted:false}}pub fn reset(&mut self){let(start,end,base)=(self.start,self.end,self.base);*self=Self::new(start,end,base);}}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Edit{
 pub channel:u16,pub source:String,pub source_hash:String,pub rendered:String,pub render_key:String,
 pub rate:u32,pub frames:u32,pub f0:Vec<f32>,pub notes:Vec<Note>,
 pub curve:Vec<(u32,f32)>,pub timing:Vec<(f64,f64)>,
 pub enabled:bool,pub strength:f32,pub formant:f32,
}
pub const STEP:f64=0.005;
impl Edit{
 pub fn valid(&self)->bool{
  let duration=self.frames as f64/self.rate.max(1) as f64;
  (16000..=192000).contains(&self.rate)&&self.frames>0&&duration<=600.&&self.source.len()<32768&&self.rendered.len()<32768
  && self.f0.len()==(duration/STEP).floor() as usize+1 && self.f0.iter().all(|v|v.is_finite()&&(*v==0.||(40.0..=2000.).contains(v)))
  && self.notes.len()<=20000&&self.notes.iter().all(|n|n.start<n.end&&n.end as usize<=self.f0.len()&&n.base.is_finite()&&(12.0..=120.).contains(&n.base)&&n.shift.is_finite()&&n.shift.abs()<=24.&&n.variation.is_finite()&&(-1.0..=2.).contains(&n.variation)&&n.drift.is_finite()&&(-1.0..=2.).contains(&n.drift)&&n.transition.is_finite()&&(0.0..=1.).contains(&n.transition)&&n.formant.is_finite()&&n.formant.abs()<=12.&&n.gain.is_finite()&&(-60.0..=24.).contains(&n.gain))
  && self.notes.windows(2).all(|w|w[0].end<=w[1].start)
  && self.curve.len()<=self.f0.len()&&self.curve.iter().all(|(i,v)|(*i as usize)<self.f0.len()&&v.is_finite()&&v.abs()<=24.)&&self.curve.windows(2).all(|w|w[0].0<w[1].0)
  && self.timing.len()>=2&&self.timing.len()<=40002&&self.timing.iter().all(|(a,b)|a.is_finite()&&b.is_finite()&&*a>=0.&&*b>=0.)&&self.timing[0]==(0.,0.)
  && self.timing.last().is_some_and(|&(a,b)|(a-duration).abs()<0.001&&(b-duration).abs()<0.001)&&self.timing.windows(2).all(|w|w[0].0<w[1].0&&w[0].1<w[1].1)
  && self.strength.is_finite()&&(0.0..=1.).contains(&self.strength)&&self.formant.is_finite()&&self.formant.abs()<=12.
 }
 /// Separate slow intonation drift from rapid modulation, in semitones.
 pub fn pitches(&self)->Vec<f32>{
  let original:Vec<f32>=self.f0.iter().map(|v|if *v>0.{69.+12.*(v/440.).log2()}else{0.}).collect();
  if !self.enabled{return original;}
  let mut out=original.clone();
  for n in &self.notes{
   let(a,b)=(n.start as usize,n.end as usize);let mut sums=vec![0.;b-a+1];let mut counts=vec![0u32;b-a+1];
   for i in a..b{sums[i-a+1]=sums[i-a]+original[i];counts[i-a+1]=counts[i-a]+u32::from(original[i]>0.);}
   for i in a..b{if original[i]<=0.{continue;}let(lo,hi)=(i.saturating_sub(20).max(a)-a,(i+21).min(b)-a);let slow=(sums[hi]-sums[lo])/(counts[hi]-counts[lo]).max(1)as f32;
    let target=n.base+n.shift+(slow-n.base)*n.drift+(original[i]-slow)*n.variation;out[i]=original[i]+(target-original[i])*self.strength;
   }
  }
  for pair in self.notes.windows(2){let n=&pair[0];let next=&pair[1];if n.transition<=0.||next.start.saturating_sub(n.end)>12{continue;}let a=(n.end as usize).saturating_sub(10).max(n.start as usize);let b=(next.start as usize+10).min(next.end as usize-1);if b<=a||out[a]<=0.||out[b]<=0.{continue;}let(left,right)=(out[a],out[b]);for i in a..=b{if out[i]>0.{let t=(i-a)as f32/(b-a)as f32;let smooth=t*t*(3.-2.*t);out[i]+=(left+(right-left)*smooth-out[i])*n.transition*self.strength;}}}
  for &(i,delta) in &self.curve{if out[i as usize]>0.{out[i as usize]+=delta*self.strength;}}
  out
 }
 pub fn corrected(&self)->Vec<f64>{self.pitches().into_iter().map(|p|if p>0.{440.*2f64.powf(f64::from(p-69.)/12.)}else{0.}).collect()}
 pub fn destination(&self,time:f64)->f64{map(&self.timing,time,false)}
 pub fn original(&self,time:f64)->f64{map(&self.timing,time,true)}
 pub fn changed(&self)->bool{self.enabled&&(self.formant.abs()>0.001||!self.curve.is_empty()||self.notes.iter().any(|n|n.shift.abs()>0.001||(n.variation-1.).abs()>0.001||(n.drift-1.).abs()>0.001||n.transition>0.001||n.formant.abs()>0.001||n.gain.abs()>0.001||n.muted)||self.timing.iter().any(|(a,b)|(a-b).abs()>0.0001))}
}
fn map(points:&[(f64,f64)],at:f64,reverse:bool)->f64{
 if points.len()<2{return at;}
 let pair=|p:(f64,f64)|if reverse{(p.1,p.0)}else{p};
 let i=points.partition_point(|p|pair(*p).0<at).clamp(1,points.len()-1);
 let(a,b)=(pair(points[i-1]),pair(points[i]));a.1+(at-a.0)/(b.0-a.0)*(b.1-a.1)
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn vocal_drift_and_modulation_are_independent(){
  let n=Note::new(0,201,57.);
  let f0:Vec<f32>=(0..201).map(|i|{let t=i as f32/200.;440.*2f32.powf((57.+(t-0.5)*1.5+0.2*(t*std::f32::consts::TAU*7.).sin()-69.)/12.)}).collect();let mut d=Edit{channel:0,source:"x".into(),source_hash:String::new(),rendered:String::new(),render_key:String::new(),rate:16000,frames:16000,f0,notes:vec![n],curve:vec![],timing:vec![(0.,0.),(1.,1.)],enabled:true,strength:1.,formant:0.};let dry=d.pitches();d.notes[0].drift=0.;let vibrato=d.pitches();assert!((vibrato[150]-vibrato[50]).abs()<(dry[150]-dry[50]).abs());d.notes[0].variation=0.;let flat=d.pitches();assert!(flat.iter().all(|v|(*v-57.).abs()<0.0001));d.notes[0].gain=-6.;assert!(d.valid());assert!(d.changed());
 }
}
