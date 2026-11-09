// SPDX-License-Identifier: GPL-3.0-or-later
//! Original sub waveforms and generated, pitchable noise textures.
use std::{f32::consts::{PI,TAU},sync::OnceLock};
pub const NOISE_NAMES:[&str;9]=["White","Pink","Brown","Blue","Air","Vinyl","Hum","Digital","User WAV"];
fn blep(t:f32,dt:f32)->f32 {if t<dt {let t=t/dt;t+t-t*t-1.}else if t>1.-dt{let t=(t-1.)/dt;t*t+t+t+1.}else{0.}}
pub fn sub(phase:f32,step:f32,shape:usize)->f32 {
 let dt=step.clamp(0.000001,0.49);
 match shape {1=>((phase*TAU).sin()*1.8).tanh()/1.8_f32.tanh(),
  2=>{let mut out=0.;for h in (1..=15).step_by(2){if h as f32*dt>=0.45{break;}out+=if h%4==1{1.}else{-1.}*(phase*TAU*h as f32).sin()/(h*h) as f32;}out*8./(PI*PI)},
  3=>phase*2.-1.-blep(phase,dt),
  4|5=>{let width=if shape==5{0.25}else{0.5};let raw=if phase<width{1.}else{-1.};(raw+blep(phase,dt)-blep((phase-width).rem_euclid(1.),dt)-(2.*width-1.))*if shape==5{0.6666667}else{1.}},
  _=>(phase*TAU).sin()}
}
pub fn library()->&'static [crate::sample::Playback] {
 static LIB:OnceLock<Vec<crate::sample::Playback>>=OnceLock::new();
 LIB.get_or_init(||(0..8).map(|kind|{
  let mut seed=0x193ba57du32;let mut low=0.;let mut slow=0.;let mut previous=0.;let mut held=0.;
  let samples=(0..44100).map(|i|{seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;let white=seed as f32/u32::MAX as f32*2.-1.;low+=0.08*(white-low);slow+=0.005*(white-slow);
   if i%24==0{held=white;}let high=white-previous;previous=white;
   let v=match kind{1=>(low*2.+slow*4.)*0.5,2=>slow*5.,3=>high*0.5,4=>high*0.4+white*0.1,5=>white*0.06+if seed%2400<3{white*0.9}else{0.},6=>(TAU*50.*i as f32/44100.).sin()*0.5+(TAU*100.*i as f32/44100.).sin()*0.15+white*0.015,7=>held,_=>white};v.clamp(-1.,1.)
  }).collect();crate::sample::Playback::compile(crate::sample::Sample{name:NOISE_NAMES[kind].into(),rate:44100,root:60.,samples}).expect("generated noise")
 }).collect())
}
pub fn noise_tick(playback:&crate::sample::Playback,cursor:&mut f64,frequency:f32,rate:f32,oneshot:bool)->f32 {
 if !oneshot && *cursor>=playback.source.samples.len() as f64 {*cursor=cursor.rem_euclid(playback.source.samples.len() as f64);}
 playback.tick(cursor,frequency,rate)
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn sub_shapes_and_noise_pitch_loop_oneshot_are_real_audio() {
  for shape in 0..6 {let samples:Vec<_>=(0..4800).map(|i|sub((i as f32/48.).fract(),1./48.,shape)).collect();assert!(samples.iter().all(|x|x.is_finite()&&x.abs()<1.1));assert!(samples.iter().map(|x|x*x).sum::<f32>()>100.);assert!(samples.iter().sum::<f32>().abs()<5.);}
  for noise in library(){let mut c=0.;let mut energy=0.;for _ in 0..48000{let x=noise_tick(noise,&mut c,261.62558,48000.,true);assert!(x.is_finite());energy+=x*x;}assert!(energy>0.01);assert!(c>=44099.);c=noise.source.samples.len() as f64;assert_eq!(noise_tick(noise,&mut c,261.62558,48000.,true),0.);noise_tick(noise,&mut c,261.62558,48000.,false);assert!(c<2.);}
  let sample=&library()[6];let mut c=0.;for _ in 0..24000{noise_tick(sample,&mut c,523.25116,48000.,true);}assert!(c>=44099.);
 }
}
