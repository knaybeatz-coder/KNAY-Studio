// SPDX-License-Identifier: GPL-3.0-or-later
//! Gesture-owned audition: source decoding off-thread, pitch updates on the existing audio stream.
use super::*;
type Loaded=Result<Arc<os_engine::Sample>,String>;
#[derive(Default)]pub struct State{pub active:Option<(u16,usize)>,pub cents:f32,pub gain:f32,path:String,bounds:(usize,usize),sounding:bool,cache:Option<(String,Arc<os_engine::Sample>)>,pending:Option<(String,mpsc::Receiver<Loaded>)>}
pub fn remember(app:&mut App,path:String,sample:Arc<os_engine::Sample>){app.vocal_audition.cache=Some((path,sample));}
pub fn preload(app:&mut App,path:&str){let a=&mut app.vocal_audition;if a.cache.as_ref().is_some_and(|c|c.0==path)||a.pending.as_ref().is_some_and(|p|p.0==path){return;}let(tx,rx)=mpsc::channel();a.pending=Some((path.into(),rx));let path=path.to_string();std::thread::spawn(move||{let result=os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(&path))).map_err(|e|e.to_string());let _=tx.send(result);});}
pub fn start(app:&mut App,clip:usize,n:usize){let Some(d)=doc(&app.state,clip)else{return};let Some(note)=d.notes.get(n)else{return};let id=d.channel;let path=d.source.clone();let a=(note.start as f64*STEP*d.rate as f64).round()as usize;let b=(note.end as f64*STEP*d.rate as f64).round()as usize;stop(app);app.vocal_audition.active=Some((id,n));app.vocal_audition.path=path.clone();app.vocal_audition.bounds=(a,b);preload(app,&path);update(app);tick(app);}
pub fn update(app:&mut App){let Some((ch,n))=app.vocal_audition.active else{return};if let Some(d)=app.state.project.vocal_edits.iter().find(|d|d.channel==ch){if let Some(note)=d.notes.get(n){let cents=if d.enabled{note.shift*d.strength*100.}else{0.};let gain=if note.muted&&d.enabled{0.}else if d.enabled{10f32.powf(note.gain/20.)}else{1.};let a=&mut app.vocal_audition;if (a.cents-cents).abs()>0.01||(a.gain-gain).abs()>0.0001{a.cents=cents;a.gain=gain;#[cfg(not(test))]app.transport.vocal_tune(cents,gain);}}}}
pub fn tick(app:&mut App){let loaded=app.vocal_audition.pending.as_ref().and_then(|(path,rx)|rx.try_recv().ok().map(|r|(path.clone(),r)));if let Some((path,result))=loaded{app.vocal_audition.pending=None;if let Ok(sample)=result{remember(app,path,sample);}}
 let a=&mut app.vocal_audition;if a.active.is_some()&&!a.sounding{if let Some((_,sample))=a.cache.as_ref().filter(|c|c.0==a.path){
  #[cfg(not(test))]app.transport.vocal_start(crate::app::empty_project(),sample.clone(),a.bounds.0,a.bounds.1,a.cents,a.gain);
  #[cfg(test)]let _=sample;
  a.sounding=true;
 }}
}
pub fn stop(app:&mut App){let a=&mut app.vocal_audition;if a.active.take().is_some(){#[cfg(not(test))]app.transport.vocal_stop();}a.sounding=false;}
