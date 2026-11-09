// SPDX-License-Identifier: GPL-3.0-or-later
//! Reuse source-relative audio and only resynthesise the contour/colour/level that changed.
use super::*;
use std::sync::{Mutex,OnceLock};
#[cfg(test)]static FRAMES:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
struct Cache{source:Arc<os_engine::Sample>,edit:Edit,audio:os_engine::Sample}
fn cache()->&'static Mutex<Option<Cache>>{static CACHE:OnceLock<Mutex<Option<Cache>>>=OnceLock::new();CACHE.get_or_init(Default::default)}
fn controls(d:&Edit)->Vec<(f64,f32,f32)>{let pitch=d.corrected();let mut out:Vec<_>=pitch.into_iter().map(|p|(p,if d.enabled{d.formant}else{0.},1.)).collect();if d.enabled{for n in &d.notes{for i in n.start as usize..n.end as usize{out[i].1=(d.formant+n.formant).clamp(-12.,12.);out[i].2=if n.muted{0.}else{10f32.powf(n.gain/20.)};}}}out}
pub fn render(engine:&engine::Engine,s:Arc<os_engine::Sample>,d:&Edit,cancel:&AtomicBool)->Result<(Vec<f32>,Vec<f32>),String>{
 if !d.valid()||d.frames as usize!=s.frames()||d.rate!=s.rate{return Err("Vocal source changed; reanalyse this recording".into());}
 #[cfg(test)]FRAMES.store(0,Ordering::Relaxed);
 let mut guard=match cache().lock(){Ok(g)=>g,Err(poisoned)=>{let mut g=poisoned.into_inner();*g=None;cache().clear_poison();g}};
 if guard.as_ref().is_none_or(|c|!Arc::ptr_eq(&c.source,&s)||c.edit.source_hash!=d.source_hash){let mut dry=d.clone();dry.enabled=false;*guard=Some(Cache{source:s.clone(),edit:dry,audio:(*s).clone()});}
 let c=guard.as_mut().unwrap();let before=controls(&c.edit);let after=controls(d);let mut spans=Vec::<(usize,usize)>::new();let mut i=0;
 while i<after.len(){if before[i]==after[i]{i+=1;continue;}let a=i;i+=1;while i<after.len()&&before[i]!=after[i]{i+=1;}let(a,b)=(a.saturating_sub(12),(i+12).min(after.len()));if let Some(last)=spans.last_mut().filter(|last|last.1>=a){last.1=last.1.max(b);}else{spans.push((a,b));}}
 // Cancellation never leaves a half-updated cache labelled as the old edit.
 let mut next=c.audio.clone();
 let mut regions=Vec::new();for(a,b)in spans{let mut p=a;while p<b{let end=(p+400).min(b);regions.push((p,end));if end==b{break;}p=end-12;}}
 for(a,b)in regions{
  if cancel.load(Ordering::Relaxed){return Err("Superseded vocal edit".into());}
  let begin=a.saturating_sub(24);let end=(b+24).min(d.f0.len());let start=(begin as f64*STEP*s.rate as f64).round()as usize;let stop=((end as f64*STEP*s.rate as f64).round()as usize).min(s.frames());if stop<=start{continue;}
  let region=os_engine::Sample{rate:s.rate,left:s.left[start..stop].to_vec(),right:s.right[start..stop].to_vec()};let frames=(((stop-start)as f64/s.rate as f64/STEP).floor()as usize)+1;
  let f0=(0..frames).map(|j|d.f0[(begin+j).min(d.f0.len()-1)]).collect();let duration=(stop-start)as f64/s.rate as f64;
  let mut local=Edit{channel:d.channel,source:d.source.clone(),source_hash:d.source_hash.clone(),rendered:String::new(),render_key:String::new(),rate:d.rate,frames:(stop-start)as u32,f0,notes:vec![],curve:vec![],timing:vec![(0.,0.),(duration,duration)],enabled:d.enabled,strength:d.strength,formant:d.formant};
  local.notes=d.notes.iter().filter_map(|n|{let mut n=n.clone();let start=(n.start as usize).max(begin);let end=(n.end as usize).min(begin+frames);if end<=start{return None;}n.start=(start-begin)as u32;n.end=(end-begin)as u32;Some(n)}).collect();local.curve=d.curve.iter().filter(|(j,_)|(*j as usize)>=begin&&(*j as usize)<begin+frames).map(|(j,v)|(*j-begin as u32,*v)).collect();
  #[cfg(test)]FRAMES.fetch_add(region.frames(),Ordering::Relaxed);
  let(l,r)=engine.render(&region,&local)?;let lo=(a as f64*STEP*s.rate as f64).round()as usize;let hi=((b as f64*STEP*s.rate as f64).round()as usize).min(s.frames());let fade=(s.rate as f32*0.005).max(1.);
  for j in lo..hi{let blend=(((j-lo).min(hi-1-j))as f32/fade).clamp(0.,1.);next.left[j]+=(l[j-start]-next.left[j])*blend;next.right[j]+=(r[j-start]-next.right[j])*blend;}
 }
 if cancel.load(Ordering::Relaxed){return Err("Superseded vocal edit".into());}
 c.audio=next;c.edit=d.clone();
 if d.enabled&&d.timing.iter().any(|(a,b)|(a-b).abs()>0.0001){let anchors:Vec<_>=d.timing.iter().map(|(a,b)|((a*d.rate as f64).round()as usize,(b*d.rate as f64).round()as usize)).collect();os_dsp::stretch::warp(&c.audio.left,&c.audio.right,d.rate,&anchors)}else{Ok((c.audio.left.clone(),c.audio.right.clone()))}
}

pub fn write_audio(path:&std::path::Path,s:&os_engine::Sample)->Result<(),String>{
 use std::io::Write;let mut file=std::io::BufWriter::with_capacity(256*1024,std::fs::File::create(path).map_err(|e|e.to_string())?);os_engine::wav::write_f32_stereo_header(&mut file,s.rate,s.frames()as u64).map_err(|e|e.to_string())?;
 let mut bytes=Vec::with_capacity(65536*8);for(a,b)in s.left.chunks(65536).zip(s.right.chunks(65536)){bytes.clear();for(&l,&r)in a.iter().zip(b){bytes.extend_from_slice(&l.to_le_bytes());bytes.extend_from_slice(&r.to_le_bytes());}file.write_all(&bytes).map_err(|e|e.to_string())?;}file.flush().map_err(|e|e.to_string())?;Ok(())
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn vocal_incremental_reuses_untouched_audio_and_coalesces_edits(){
  let Ok(path)=std::env::var("LEMON_VOCAL_TEST_ENGINE")else{return};let e=engine::Engine::test_open(std::path::Path::new(&path)).unwrap();let rate=24000;let frames=rate*10;let l:Vec<f32>=(0..frames).map(|i|(i as f32*220.*std::f32::consts::TAU/rate as f32).sin()*0.2).collect();let s=Arc::new(os_engine::Sample{rate,left:l.clone(),right:l});
  let mut d=Edit{channel:0,source:"cache-test".into(),source_hash:"stable".into(),rendered:String::new(),render_key:String::new(),rate,frames,f0:vec![220.;2001],notes:vec![Note::new(100,200,57.),Note::new(1800,1900,57.)],curve:vec![],timing:vec![(0.,0.),(10.,10.)],enabled:true,strength:1.,formant:0.};d.notes[0].shift=3.;let stop=AtomicBool::new(false);
  let _=std::thread::spawn(||{let _guard=cache().lock().unwrap_or_else(|e|e.into_inner());panic!("injected cache panic");}).join();
  let at=Instant::now();let full=e.render(&s,&d).unwrap();let full_time=at.elapsed();let at=Instant::now();let first=render(&e,s.clone(),&d,&stop).unwrap();let first_time=at.elapsed();let count=FRAMES.load(Ordering::Relaxed);assert!(count<rate as usize*2,"one note must not resynthesise the ten-second file");assert_eq!(&first.0[rate as usize*2..],&s.left[rate as usize*2..]);assert!(full.0.iter().chain(&first.0).all(|v|v.is_finite()));
  let again=render(&e,s.clone(),&d,&stop).unwrap();assert_eq!(FRAMES.load(Ordering::Relaxed),0);assert_eq!(again,first);
  d.notes[1].shift=-2.;let at=Instant::now();let second=render(&e,s.clone(),&d,&stop).unwrap();let second_time=at.elapsed();assert_eq!(&first.0[..rate as usize*8],&second.0[..rate as usize*8]);assert!(FRAMES.load(Ordering::Relaxed)<rate as usize*2);
  d.notes[0].shift=5.;stop.store(true,Ordering::Relaxed);assert!(render(&e,s.clone(),&d,&stop).is_err());stop.store(false,Ordering::Relaxed);let next=render(&e,s,&d,&stop).unwrap();assert_eq!(&next.0[rate as usize*8..],&second.0[rate as usize*8..]);
  let path=std::env::temp_dir().join(format!("lemon-vocal-buffered-{}.wav",std::process::id()));let audio=os_engine::Sample{rate,left:next.0.clone(),right:next.1.clone()};let io=Instant::now();write_audio(&path,&audio).unwrap();let io_time=io.elapsed();let read=os_engine::sample::load_shared(&path).unwrap();assert_eq!(*read,audio);eprintln!("Buffered 10 s audio cache write: {:?}",io_time);
  eprintln!("Vocal edit benchmark: full 10 s {:?}; first local {:?} ({} source frames); second local {:?}",full_time,first_time,count,second_time);
 }
}
