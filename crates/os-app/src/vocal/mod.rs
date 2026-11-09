// SPDX-License-Identifier: GPL-3.0-or-later
//! Inline monophonic vocal editor. Original audio is immutable; renders preserve source duration.
mod engine;
mod incremental;
pub mod audition;
mod view;
mod editing;
pub mod navigation;
use editing::begin;
pub use editing::drag;
pub use view::{paint,head,keys,packed_art,TOOLS};
pub use view::icon as tool_icon;
use crate::{app::{App,State,Drag},playlist as pl};
use os_model::{ClipSource,vocal::{Edit,Note,STEP}};
use std::{sync::{mpsc,Arc,atomic::{AtomicBool,Ordering}},path::PathBuf,time::{Duration,Instant},collections::HashSet};
use sha2::{Digest,Sha256};
static NEXT_CACHE_FILE:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Tool{Main,Select,Modulation,Drift,Transition,Formant,Amplitude,Time,Attack,Draw,Split,Hand,Zoom}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Field{Pitch,Modulation,Drift,Transition,Formant,Amplitude,Strength}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Command{
 Toggle(u16),Analyze,Download(u16),Cancel,Tool(Tool),Note(usize,usize),Grid(usize),Correct,Join,Reset,ResetNote,Bypass,
 Strength(i8),Variation(i8),Formant(i8),Key,Scale,Zoom(i8),Pan(i8),Undo,Redo,
 Param(Field),Boundary(usize,usize),Surface,Nav(navigation::Kind),Fit,XZoom(i8),Mute,Snap,SelectAll,SelectNone,Retry,
}
#[derive(Debug,Clone)]pub struct View{
 pub row:Option<u16>,pub tool:Tool,pub selected:Vec<(u16,usize)>,pub low:f32,pub span:f32,
 pub key:u8,pub scale:u8,pub message:String,pub busy:bool,pub ready:Option<bool>,pub last_edit:Instant,
 pub palette:Option<(f32,f32)>,pub snap:bool,pub marquee:Option<os_ui::Rect>,pub click:Option<(u16,usize,Tool,Instant)>,
 contours:std::cell::RefCell<std::collections::HashMap<u16,(String,bool,Arc<Vec<f32>>)>>,
 observed:std::collections::HashMap<u16,String>,
 failed:HashSet<String>,
}
impl Default for View{fn default()->Self{Self{row:None,tool:Tool::Main,selected:vec![],low:48.,span:18.,key:0,scale:0,message:"Choose a vocal clip to edit".into(),busy:false,ready:None,last_edit:Instant::now(),palette:None,snap:true,marquee:None,click:None,contours:Default::default(),observed:Default::default(),failed:HashSet::new()}}}
pub struct Job{rx:mpsc::Receiver<Result<Done,String>>,cancel:Arc<AtomicBool>,expected:Option<Edit>,source:u16,path:String,row:u16,key:String}
impl Drop for Job{fn drop(&mut self){self.cancel.store(true,Ordering::Relaxed);}}
enum Done{Installed,Analyzed(Edit,Arc<os_engine::Sample>),Rendered(PathBuf,Arc<os_engine::Sample>)}
#[derive(Debug,Clone)]pub struct Gesture{pub clip:usize,pub note:Option<usize>,pub before:Arc<Edit>,pub selected:Vec<usize>,pub x:f32,pub y:f32,pub last:Option<(u32,f32)>,pub tool:Tool,pub edge:i8,pub axis:i8,pub field:Option<Field>,pub related:Vec<(u16,usize,Note)>,pub strengths:Vec<(u16,f32)>}
pub const EXTRA:f32=354.;
pub fn expanded(s:&State,row:u16)->bool{s.playlist.vocal.row==Some(row)}
pub fn doc(s:&State,clip:usize)->Option<&Edit>{let ClipSource::Channel(ch)=pl::clips(s).get(clip)?.source else{return None};s.project.vocal_edits.iter().find(|d|d.channel==ch&&owns(s,d))}
fn owns(s:&State,d:&Edit)->bool{s.project.channel(d.channel).and_then(|c|c.sample_path.as_deref()).is_some_and(|p|p==d.source||p==d.rendered)}
pub fn clone_source(s:&mut State,from:u16,to:u16){if let Some(mut d)=s.project.vocal_edits.iter().find(|d|d.channel==from).cloned(){d.channel=to;s.project.vocal_edits.push(d);}}
fn contour(s:&State,d:&Edit)->Arc<Vec<f32>>{let k=key(d);let mut cache=s.playlist.vocal.contours.borrow_mut();if let Some((old,on,p))=cache.get(&d.channel){if *old==k&&*on==d.enabled{return p.clone();}}let p=Arc::new(d.pitches());if cache.len()>=8{cache.clear();}cache.insert(d.channel,(k,d.enabled,p.clone()));p}
fn doc_index(s:&State,clip:usize)->Option<usize>{let id=doc(s,clip)?.channel;s.project.vocal_edits.iter().position(|d|d.channel==id)}
fn snapshot(s:&mut State,label:&'static str){
 let mut snaps=vec![crate::history::Snap::Vocals(s.project.vocal_edits.clone())];
 for d in &s.project.vocal_edits{if let Some(r)=s.project.channels.iter().position(|c|c.index==d.channel){if let Some(v)=crate::history::Snap::sample(s,r){snaps.push(v);}}}
 crate::history::record(s,label,crate::history::Snap::Several(snaps));
}
pub(crate) fn key(d:&Edit)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(&("regional-v2",&d.source,&d.source_hash,d.rate,d.frames,&d.notes,&d.curve,&d.timing,d.strength,d.formant)).unwrap_or_default()))}
fn file_hash(path:&str)->Result<String,String>{
 use std::io::Read;use std::sync::{Mutex,OnceLock};type Entry=(u64,Option<std::time::SystemTime>,String);static HASHES:OnceLock<Mutex<std::collections::HashMap<String,Entry>>>=OnceLock::new();
 let resolved=crate::plugins::resolve_sample(path);let meta=std::fs::metadata(&resolved).map_err(|e|e.to_string())?;let stamp=(meta.len(),meta.modified().ok());let cache=HASHES.get_or_init(Default::default);
 if let Ok(map)=cache.lock(){if let Some((len,time,hash))=map.get(&resolved){if (*len,*time)==stamp&&time.is_some(){return Ok(hash.clone());}}}
 let mut file=std::fs::File::open(&resolved).map_err(|e|e.to_string())?;let mut h=Sha256::new();let mut b=[0u8;65536];loop{let n=file.read(&mut b).map_err(|e|e.to_string())?;if n==0{break;}h.update(&b[..n]);}let hash=format!("{:x}",h.finalize());let after=std::fs::metadata(&resolved).map_err(|e|e.to_string())?;if (after.len(),after.modified().ok())!=stamp{return Err("Original recording changed during analysis".into());}
 if let Ok(mut map)=cache.lock(){if map.len()>16{map.clear();}map.insert(resolved,(stamp.0,stamp.1,hash.clone()));}Ok(hash)
}

fn source(app:&App,row:u16)->Result<(u16,String),String>{
 let clip=pl::clips(&app.state).iter().find(|c|c.track==row&&pl::source_kind(&app.state,c.source)==pl::PickKind::Audio).ok_or("Add a vocal audio clip to this track first")?;
 let ClipSource::Channel(id)=clip.source else{return Err("Choose an audio clip".into())};let c=app.state.project.channel(id).ok_or("Missing audio channel")?;
 if crate::ara::is(c){return Err("Remove the attached ARA editor before using inline vocal tuning".into());}
 if c.sound.start_offset()!=0.||c.sound.fx.start()!=0.||c.sound.fx.length()!=1.||c.sound.fx.trim!=0||c.sound.fx.flags&os_model::SampleFx::REVERSE!=0||c.pitch_cents!=0||c.misc.fine!=0||c.sound.stretch_cents()!=0.{return Err("Render this clip's trim, reverse or pitch processing to audio before tuning".into());}
 Ok((id,c.sample_path.clone().ok_or("This channel has no source audio")?))
}
fn message(app:&mut App,text:String){app.state.message=Some(text.clone());app.state.playlist.vocal.message=text;app.invalidate();}
pub fn toggle(app:&mut App,row:u16){
 if app.state.playlist.vocal.row==Some(row){app.state.playlist.vocal.row=None;app.invalidate();return;}
 if let Err(e)=source(app,row){message(app,e);return;}
 app.state.playlist.vocal.row=Some(row);app.state.playlist.vocal.selected.clear();
 if app.state.playlist.vocal.ready.is_none(){app.state.playlist.vocal.ready=Some(engine::ready());}
 if app.state.playlist.vocal.ready==Some(false){ask_download(app,row);}else{ensure(app,row);}
 app.invalidate();
}
fn ask_download(app:&mut App,row:u16){
 use crate::dialog::{Msg,Kind,Answer,Then,spans};
 app.show_msg(Msg::new(Kind::Question,"Enable vocal tuning?",spans(&["Download the optional vocal-tuning engine?\n\nThis small, one-time download is under 8 MB and installs only for Lemon Studio. No large AI models are required.\n\nVocal analysis and rendering stay on your computer. The original recording is preserved."]),&[Answer::Yes,Answer::No],Then::VocalDownload(row)));
 app.invalidate();
}
pub fn download(app:&mut App,row:u16){
 if app.vocal_job.is_some(){return;}
 let(tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let stop=cancel.clone();
 app.vocal_job=Some(Job{rx,cancel,expected:None,source:0,path:String::new(),row,key:String::new()});app.state.playlist.vocal.busy=true;
 message(app,"Downloading and verifying vocal tuning...".into());
 std::thread::spawn(move||{let result=engine::install().map(|_|Done::Installed);if !stop.load(Ordering::Relaxed){let _=tx.send(result);}});
}
fn ensure(app:&mut App,row:u16){
 if app.vocal_job.is_some(){return;}
 let ids:Vec<u16>=pl::clips(&app.state).iter().filter(|c|c.track==row).filter_map(|c|if let ClipSource::Channel(id)=c.source{app.state.project.channel(id).filter(|c|c.kind.is_sampler()&&!crate::ara::is(c)).map(|_|id)}else{None}).collect();
 let next=ids.iter().find(|id|!app.state.project.vocal_edits.iter().any(|d|d.channel==**id&&owns(&app.state,d))).copied();
 let Some(id)=next else{if let Some(path)=ids.iter().find_map(|id|app.state.project.vocal_edits.iter().find(|d|d.channel==*id).map(|d|d.source.clone())){audition::preload(app,&path);}message(app,"Drag notes to tune. Edits survive slicing, trimming and moving clips.".into());return;};
 let Some(c)=app.state.project.channel(id)else{return};
 if crate::ara::is(c)||!c.kind.is_sampler(){return;}
 let Some(path)=c.sample_path.clone()else{return};
 if c.sound.start_offset()!=0.||c.sound.fx.start()!=0.||c.sound.fx.length()!=1.||c.sound.fx.trim!=0||c.sound.fx.flags&os_model::SampleFx::REVERSE!=0||c.pitch_cents!=0||c.misc.fine!=0||c.sound.stretch_cents()!=0.{message(app,"Render the clip's trim, reverse or pitch processing to audio before tuning".into());return;}
 let(tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let stop=cancel.clone();
 app.vocal_job=Some(Job{rx,cancel,expected:None,source:id,path:path.clone(),row,key:String::new()});app.state.playlist.vocal.busy=true;message(app,"Analysing the vocal's notes and pitch...".into());
 std::thread::spawn(move||{let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let sample=os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(&path))).map_err(|e|e.to_string())?;
  let source_hash=file_hash(&path)?;let f0=engine::Engine::load()?.analyze(&sample)?;let notes=segment(&f0);let duration=sample.frames()as f64/sample.rate as f64;
  Ok(Done::Analyzed(Edit{channel:id,source:path,source_hash,rendered:String::new(),render_key:String::new(),rate:sample.rate,frames:sample.frames()as u32,f0,notes,curve:vec![],timing:vec![(0.,0.),(duration,duration)],enabled:true,strength:1.,formant:0.},sample))
 })).unwrap_or_else(|_|Err("Vocal processing stopped unexpectedly. Previous audio retained; retry is available.".into()));if !stop.load(Ordering::Relaxed){let _=tx.send(result);}});
}
pub fn segment(f0:&[f32])->Vec<Note>{
 let mut notes=Vec::new();let mut i=0;
 while i<f0.len(){if f0[i]<=0.{i+=1;continue;}let start=i;let mut values=vec![69.+12.*(f0[i]/440.).log2()];i+=1;
  while i<f0.len()&&f0[i]>0.{let p=69.+12.*(f0[i]/440.).log2();let base=values.iter().rev().take(8).copied().sum::<f32>()/values.len().min(8)as f32;if (p-base).abs()>0.85&&i-start>=8{break;}values.push(p);i+=1;}
  if i-start>=6{values.sort_by(f32::total_cmp);let base=values[values.len()/2];notes.push(Note::new(start as u32,i as u32,base));}
 }
 notes
}
fn set_source(app:&mut App,id:u16,path:&str){
 if let Some(row)=app.state.project.channels.iter().position(|c|c.index==id){if app.state.project.channels[row].sample_path.as_deref()!=Some(path){app.state.project.channels[row].sample_path=Some(path.into());app.state.sample_loads.push((row,crate::plugins::resolve_sample(path)));app.state.piano.dirty=true;}}
}
pub fn tick(app:&mut App){
 audition::tick(app);
 // A failed state is blocked only until the next edit/Undo/Redo, never for the whole session.
 let revisions:Vec<_>=app.state.project.vocal_edits.iter().map(|d|(d.channel,format!("{}:{}",key(d),d.enabled))).collect();
 for(ch,revision)in revisions{if app.state.playlist.vocal.observed.get(&ch)!=Some(&revision){if let Some(d)=app.state.project.vocal_edits.iter().find(|d|d.channel==ch){app.state.playlist.vocal.failed.remove(&key(d));}app.state.playlist.vocal.observed.insert(ch,revision);}}

 if app.vocal_job.as_ref().is_some_and(|j|j.expected.as_ref().is_some_and(|old|!app.state.project.vocal_edits.iter().any(|d|d==old))){app.vocal_job=None;app.state.playlist.vocal.busy=false;}
 let done=app.vocal_job.as_ref().and_then(|j|match j.rx.try_recv(){Ok(r)=>Some(r),Err(mpsc::TryRecvError::Disconnected)=>Some(Err("Vocal worker stopped unexpectedly. Retry this correction.".into())),Err(mpsc::TryRecvError::Empty)=>None});
 if let Some(result)=done{
  let job=app.vocal_job.take().unwrap();app.state.playlist.vocal.busy=false;
  match result{
   Err(e)=>{if !job.key.is_empty(){app.state.playlist.vocal.failed.insert(job.key.clone());}message(app,format!("Vocal tuning: {e}. Previous audio retained."));},
   Ok(Done::Installed)=>{app.state.playlist.vocal.ready=Some(true);ensure(app,job.row);},
   Ok(Done::Analyzed(mut d,sample))=>{
    audition::remember(app,d.source.clone(),sample);
    if app.state.project.channel(job.source).and_then(|c|c.sample_path.as_deref())!=Some(job.path.as_str())||!pl::clips(&app.state).iter().any(|c|c.track==job.row&&c.source==ClipSource::Channel(job.source)){return;}
    let row=app.state.project.channels.len();let before=crate::history::Snap::Several(vec![crate::history::Snap::Vocals(app.state.project.vocal_edits.clone()),crate::history::Snap::Rack(Box::new(crate::history::RackSide::now(&app.state,row)))]);
    pl::snapshot_with(&mut app.state,"Analyse vocal",before);
    if pl::clips(&app.state).iter().any(|c|c.source==ClipSource::Channel(d.channel)&&c.track!=job.row){
     let Some(next)=app.state.project.channels.iter().map(|c|c.index).max().unwrap_or(0).checked_add(1)else{return};
     let mut c=app.state.project.channel(d.channel).unwrap().clone();c.index=next;c.plugin_state=None;c.plugin=None;
     for a in &mut app.state.project.arrangements{if a.number==app.state.project.current_arrangement.unwrap_or(0){for clip in &mut a.clips{if clip.track==job.row&&clip.source==ClipSource::Channel(d.channel){clip.source=ClipSource::Channel(next);}}}}
     crate::rack::push_channel(&mut app.state,c);d.channel=next;
    }
    if !d.notes.is_empty(){let lo=d.notes.iter().map(|n|n.base).fold(128f32,f32::min);let hi=d.notes.iter().map(|n|n.base).fold(0f32,f32::max);app.state.playlist.vocal.span=(hi-lo+8.).clamp(12.,36.);app.state.playlist.vocal.low=(lo-4.).clamp(0.,128.-app.state.playlist.vocal.span);}
    d.render_key=key(&d);d.rendered=d.source.clone();app.state.project.vocal_edits.retain(|old|old.channel!=d.channel);app.state.project.vocal_edits.push(d);app.state.piano.dirty=true;
    ensure(app,job.row);
   },
   Ok(Done::Rendered(path,audio))=>{
    os_engine::sample::remember_shared(&path,audio.clone());app.vocal_audio=Some(audio);
    if let Some(expected)=job.expected.as_ref(){if let Some(i)=app.state.project.vocal_edits.iter().position(|d|d==expected&&owns(&app.state,d)){let d=&mut app.state.project.vocal_edits[i];d.rendered=path.to_string_lossy().into_owned();d.render_key=job.key.clone();let(id,path)=(d.channel,d.rendered.clone());set_source(app,id,&path);message(app,"Vocal tuning ready. Split and move clips normally; corrections remain attached to the source.".into());}}
   },
  }
  app.invalidate();
 }
 // Bypass/Undo changes the channel source on the next tick without touching any clip windows.
 let paths:Vec<_>=app.state.project.vocal_edits.iter().filter(|d|owns(&app.state,d)).filter_map(|d|if !d.changed(){Some((d.channel,d.source.clone()))}else if d.render_key==key(d)&&std::path::Path::new(&crate::plugins::resolve_sample(&d.rendered)).is_file(){Some((d.channel,d.rendered.clone()))}else if !std::path::Path::new(&crate::plugins::resolve_sample(&d.rendered)).is_file(){Some((d.channel,d.source.clone()))}else{None}).collect();
 for(id,path)in paths{set_source(app,id,&path);}
 if app.vocal_job.is_some()||matches!(app.drag,Some(Drag::Vocal(_)))||app.state.playlist.vocal.last_edit.elapsed()<Duration::from_millis(15){return;}
 let next=app.state.project.vocal_edits.iter().find(|d|owns(&app.state,d)&&d.changed()&&(d.render_key!=key(d)||!std::path::Path::new(&crate::plugins::resolve_sample(&d.rendered)).is_file())&&!app.state.playlist.vocal.failed.contains(&key(d))&&app.state.project.channels.iter().any(|c|c.index==d.channel)).cloned();
 let Some(d)=next else{return};
 if app.state.playlist.vocal.ready.is_none(){app.state.playlist.vocal.ready=Some(engine::ready());}
 if app.state.playlist.vocal.ready==Some(false){return;}
 let output=crate::pattern_render::folder(&app.state).join("Vocal tuning");let k=key(&d);
 let(tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let stop=cancel.clone();
 app.vocal_job=Some(Job{rx,cancel,expected:Some(d.clone()),source:d.channel,path:d.source.clone(),row:app.state.playlist.vocal.row.unwrap_or(0),key:k.clone()});app.state.playlist.vocal.busy=false;
 std::thread::spawn(move||{let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let sample=os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(&d.source))).map_err(|e|e.to_string())?;
  if file_hash(&d.source)?!=d.source_hash{return Err("The original recording changed on disk; reanalyse before applying its tuning".into());}
  std::fs::create_dir_all(&output).map_err(|e|e.to_string())?;let path=output.join(format!("Vocal-{}-{k}.wav",d.channel));
  if path.is_file(){let audio=os_engine::sample::load_shared(&path).map_err(|e|e.to_string())?;return Ok(Done::Rendered(path,audio));}
  let(l,r)=incremental::render(&engine::Engine::load()?,sample,&d,&stop)?;
  if stop.load(Ordering::Relaxed){return Err("Superseded vocal edit".into());}
  let audio=Arc::new(os_engine::Sample{rate:d.rate,left:l,right:r});
  if !path.is_file(){let pending=output.join(format!("Vocal-{}-{k}-{}-{}.part",d.channel,std::process::id(),NEXT_CACHE_FILE.fetch_add(1,Ordering::Relaxed)));incremental::write_audio(&pending,&audio)?;std::fs::rename(pending,&path).map_err(|e|e.to_string())?;}
  Ok(Done::Rendered(path,audio))
 })).unwrap_or_else(|_|Err("Vocal processing stopped unexpectedly. Previous audio retained; retry is available.".into()));if !stop.load(Ordering::Relaxed){let _=tx.send(result);}});
}
pub(crate) fn history_changed(s:&mut State){s.playlist.vocal.failed.clear();if s.playlist.vocal.message.starts_with("Vocal tuning:"){s.playlist.vocal.message="Vocal edit restored; editing is available.".into();}}
pub fn context(app:&mut App){app.state.playlist.vocal.palette=Some(app.mouse);app.invalidate();}
pub fn command(app:&mut App,c:Command){
 app.state.playlist.vocal.palette=None;
 match c{
  Command::Retry=>{app.state.playlist.vocal.failed.clear();message(app,"Retrying vocal correction...".into());return;},
  Command::Toggle(row)=>return toggle(app,row),Command::Download(row)=>return download(app,row),
  Command::Cancel=>{if let Some(j)=&app.vocal_job{if !j.key.is_empty(){app.state.playlist.vocal.failed.insert(j.key.clone());}}app.vocal_job=None;app.state.playlist.vocal.ready=None;app.state.playlist.vocal.busy=false;message(app,"Vocal operation cancelled; previous audio retained".into());return;},
  Command::Analyze=>{if let Some(row)=app.state.playlist.vocal.row{if app.state.playlist.vocal.ready!=Some(true){ask_download(app,row);}else if pl::clips(&app.state).iter().any(|c|c.track==row&&matches!(c.source,ClipSource::Channel(id) if app.state.project.vocal_edits.iter().any(|d|d.channel==id))){
   use crate::dialog::{Msg,Kind,Answer,Then,spans};app.show_msg(Msg::new(Kind::Question,"Reanalyse vocal?",spans(&["Analyse the original vocal recording again? Existing corrections for these sources will be reset. Undo restores them."]),&[Answer::Yes,Answer::No],Then::VocalReanalyse(row)));
  }else{app.state.playlist.vocal.failed.clear();ensure(app,row);}}return;},
  Command::Tool(t)=>{app.state.playlist.vocal.tool=t;app.invalidate();return;},
  Command::Key=>{app.state.playlist.vocal.key=(app.state.playlist.vocal.key+1)%12;app.invalidate();return;},
  Command::Scale=>{app.state.playlist.vocal.scale=(app.state.playlist.vocal.scale+1)%3;app.invalidate();return;},
  Command::Zoom(n)=>{app.state.playlist.vocal.span=(app.state.playlist.vocal.span-n as f32*6.).clamp(12.,48.);app.invalidate();return;},
  Command::Pan(n)=>{app.state.playlist.vocal.low=(app.state.playlist.vocal.low+n as f32*6.).clamp(0.,120.-app.state.playlist.vocal.span);app.invalidate();return;},
  Command::Undo=>{crate::history::undo(&mut app.state);app.invalidate();return;},Command::Redo=>{crate::history::redo(&mut app.state);app.invalidate();return;},
  Command::Nav(k)=>{navigation::begin(app,k);return;},
  Command::Surface=>{navigation::begin(app,match app.state.playlist.vocal.tool{Tool::Hand=>navigation::Kind::Pan,Tool::Zoom=>navigation::Kind::Zoom,_=>navigation::Kind::Box});return;},
  Command::Fit=>{navigation::fit(app,false);return;},
  Command::XZoom(n)=>{let x=app.pl_grid().map(|g|g.cx()).unwrap_or(0.);navigation::zoom_x(app,1.4f32.powi(n as i32),x);app.invalidate();return;},
  Command::Snap=>{app.state.playlist.vocal.snap=!app.state.playlist.vocal.snap;app.invalidate();return;},
  Command::SelectNone=>{app.state.playlist.vocal.selected.clear();app.invalidate();return;},
  Command::SelectAll=>{app.state.playlist.vocal.selected=pl::clips(&app.state).iter().enumerate().filter(|(_,c)|Some(c.track)==app.state.playlist.vocal.row).filter_map(|(ci,_)|doc(&app.state,ci)).flat_map(|d|d.notes.iter().enumerate().map(move|(i,_)|(d.channel,i))).collect();app.invalidate();return;},
  Command::Param(f)=>{editing::parameter(app,f);return;},
  Command::Boundary(ci,n)=>{editing::boundary(app,ci,n);return;},
  Command::Note(clip,n)=>{begin(app,clip,Some(n));return;},Command::Grid(clip)=>{begin(app,clip,None);return;},_=>{}
 }
 let ids:Vec<_>=app.state.project.vocal_edits.iter().enumerate().filter(|(_,d)|owns(&app.state,d)&&pl::clips(&app.state).iter().any(|clip|Some(clip.track)==app.state.playlist.vocal.row&&clip.source==ClipSource::Channel(d.channel))).map(|(i,_)|i).collect();
 if ids.is_empty(){return;}
 snapshot(&mut app.state,"Edit vocal tuning");
 let selected=app.state.playlist.vocal.selected.clone();let(root,scale)=(app.state.playlist.vocal.key,app.state.playlist.vocal.scale);
 for i in ids{let d=&mut app.state.project.vocal_edits[i];let chosen:Vec<usize>=selected.iter().filter(|(ch,_)|*ch==d.channel).map(|(_,n)|*n).collect();
  match c{
   Command::Bypass=>d.enabled=!d.enabled,
   Command::Strength(n)=>d.strength=(d.strength+n as f32*0.1).clamp(0.,1.),
   Command::Formant(n)=>d.formant=(d.formant+n as f32*0.5).clamp(-12.,12.),
   Command::Reset=>{d.curve.clear();d.timing=vec![(0.,0.),(d.frames as f64/d.rate as f64,d.frames as f64/d.rate as f64)];d.formant=0.;d.strength=1.;for n in &mut d.notes{n.reset();}},
   Command::Join=>{if chosen.len()>=2{let a=*chosen.iter().min().unwrap();let b=*chosen.iter().max().unwrap();if b<d.notes.len(){let end=d.notes[b].end;d.notes[a].end=end;d.notes.drain(a+1..=b);}}},
   _=>{for(n,note)in d.notes.iter_mut().enumerate(){if !chosen.is_empty()&&!chosen.contains(&n){continue;}match c{
    Command::Correct=>{let target=nearest(note.base,root,scale);note.shift=target-note.base;},
    Command::Variation(v)=>note.variation=(note.variation+v as f32*0.1).clamp(-1.,2.),
    Command::Mute=>note.muted=!note.muted,
    Command::ResetNote=>{note.reset();d.curve.retain(|(i,_)|*i<note.start||*i>=note.end);},_=>{}
   }}}
  }
 }
 if matches!(c,Command::Join|Command::Reset){app.state.playlist.vocal.selected.clear();}
 app.state.playlist.vocal.last_edit=Instant::now();app.state.piano.dirty=true;app.invalidate();
}
fn nearest(p:f32,root:u8,scale:u8)->f32{
 let allowed:&[i32]=match scale{1=>&[0,2,4,5,7,9,11],2=>&[0,2,3,5,7,8,10],_=>&[0,1,2,3,4,5,6,7,8,9,10,11]};
 (0..=127).filter(|n|allowed.contains(&((*n-i32::from(root)).rem_euclid(12)))).min_by(|a,b|(*a as f32-p).abs().total_cmp(&(*b as f32-p).abs())).unwrap_or(60)as f32
}
fn frame_at(app:&App,clip:usize,x:f32)->Option<u32>{let grid=app.pl_grid()?;let g=pl::PGeom::new(&app.state,grid);let c=pl::clips(&app.state).get(clip)?;let d=doc(&app.state,clip)?;let r=g.clip_rect(c);let(a,b)=pl::window(&app.state,c);let dest=(a+(b-a)*f64::from((x-r.x)/r.w).clamp(0.,1.))/1000.;Some((d.original(dest)/STEP).round().clamp(0.,(d.f0.len()-1)as f64)as u32)}

pub fn reanalyse(app:&mut App,row:u16){
 let ids:Vec<_>=pl::clips(&app.state).iter().filter(|c|c.track==row).filter_map(|c|if let ClipSource::Channel(id)=c.source{Some(id)}else{None}).collect();
 snapshot(&mut app.state,"Reanalyse vocal");
 let sources:Vec<_>=app.state.project.vocal_edits.iter().filter(|d|ids.contains(&d.channel)&&owns(&app.state,d)).map(|d|(d.channel,d.source.clone())).collect();
 for(id,path)in sources{set_source(app,id,&path);}
 app.state.project.vocal_edits.retain(|d|!ids.contains(&d.channel));app.state.playlist.vocal.selected.clear();app.state.playlist.vocal.failed.clear();ensure(app,row);
}

pub fn key_input(app:&mut App,key:&winit::keyboard::Key,ctrl:bool,alt:bool)->bool{
 use winit::keyboard::{Key,NamedKey};if app.state.z.last()!=Some(&crate::app::Win::Playlist)||app.state.playlist.popup.is_some()||app.state.settings.open||app.state.menu_open.is_some(){return false;}let Some(r)=navigation::rect(app)else{return false};if !r.contains(app.mouse.0,app.mouse.1){return false;}
 let cmd=match key{Key::Character(c)if ctrl&&c.eq_ignore_ascii_case("a")=>Command::SelectAll,Key::Named(NamedKey::Escape)=>Command::SelectNone,Key::Character(c)if !ctrl&&!alt=>match c.to_ascii_lowercase().as_str(){"1"=>Command::Tool(Tool::Main),"2"=>Command::Tool(Tool::Select),"3"=>Command::Tool(Tool::Modulation),"4"=>Command::Tool(Tool::Drift),"5"=>Command::Tool(Tool::Formant),"6"=>Command::Tool(Tool::Amplitude),"7"=>Command::Tool(Tool::Time),"8"=>Command::Tool(Tool::Attack),"9"=>Command::Tool(Tool::Split),"t"=>Command::Tool(Tool::Transition),"d"=>Command::Tool(Tool::Draw),"h"=>Command::Tool(Tool::Hand),"z"=>Command::Tool(Tool::Zoom),"f"=>Command::Fit,"m"=>Command::Mute,_=>return false},_=>return false};command(app,cmd);true
}

#[cfg(test)]mod tests{
 use super::*;use crate::app::{Action,Win};use os_ui::{Canvas,Rect};
 fn fixture()->(App,PathBuf,usize){
  let dir=std::env::temp_dir().join(format!("lemon-vocal-ui-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();let source=dir.join("Vocal.wav");let rate=24000u32;
  let audio:Vec<f32>=(0..rate*2).flat_map(|i|{let t=i as f32/rate as f32;let hz=if t<0.25{220.}else if (0.4..0.8).contains(&t){261.626}else if (1.05..1.5).contains(&t){329.628}else{0.};let x=if hz>0.{(t*hz*std::f32::consts::TAU).sin()*0.2}else{0.};[x,x]}).collect();
  os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&source).unwrap(),rate,&audio).unwrap();
  let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.tempo=120.;app.state.project.data_folder=Some(os_model::DataFolder{choice:2,folder:dir.to_string_lossy().into_owned()});
  app.state.playlist_bar_w=480.;let ci=pl::add_audio_clip(&mut app.state,&source.to_string_lossy(),2,0).unwrap();
  let ClipSource::Channel(id)=pl::clips(&app.state)[ci].source else{panic!()};
  let mut f0=vec![0.;401];for(i,hz)in f0.iter_mut().enumerate(){*hz=if i<50{220.}else if (80..160).contains(&i){261.626}else if (210..300).contains(&i){329.628}else{0.};}
  let notes=segment(&f0);let mut d=Edit{channel:id,source:source.to_string_lossy().into_owned(),source_hash:file_hash(&source.to_string_lossy()).unwrap(),rendered:source.to_string_lossy().into_owned(),render_key:String::new(),rate,frames:rate*2,f0,notes,curve:vec![],timing:vec![(0.,0.),(2.,2.)],enabled:true,strength:1.,formant:0.};assert!(d.valid());d.render_key=key(&d);app.state.project.vocal_edits.push(d);
  app.state.playlist.vocal.ready=Some(true);app.state.playlist.vocal.row=Some(2);app.state.playlist.vocal.low=48.;app.state.playlist.vocal.span=24.;app.state.screen=(1280.,900.);app.state.raise(Win::Playlist);if let Some(i)=Win::Playlist.slot(){app.state.wins[i].maxed=true;}
  (app,dir,ci)
 }
 fn paint(app:&mut App,c:&mut Canvas){app.paint(c,None);}
 fn hit(app:&App,action:Action)->Rect{app.hits.iter().rev().find(|h|h.action==action).unwrap_or_else(||panic!("missing hit {action:?}")).rect}
 #[test]fn vocal_inline_input_split_save_undo_and_curve_tools(){
  let(mut app,dir,ci)=fixture();let original_audio=std::fs::read(dir.join("Vocal.wav")).unwrap();let mut canvas=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut canvas);
  let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let old=app.state.playlist.row_h;
  assert_eq!(g.rows.height(2),old+EXTRA);assert_eq!(g.rows.height(1),old);
  let button=hit(&app,Action::Vocal(Command::Toggle(2)));assert!(button.y<g.y_of(2)+old);
  let note=hit(&app,Action::Vocal(Command::Note(ci,1)));let r=view::pitch_rect(&g,2);let baseline=app.state.project.vocal_edits.clone();
  app.mouse_move(note.cx(),note.cy());app.mouse_down();assert!(matches!(app.drag,Some(Drag::Vocal(_))));app.mouse_move(note.cx(),note.cy()-2.*r.h/24.);app.mouse_up();
  assert_eq!(app.state.project.vocal_edits[0].notes[1].shift,2.);
  crate::history::undo(&mut app.state).unwrap();assert_eq!(app.state.project.vocal_edits,baseline);
  crate::history::redo(&mut app.state).unwrap();assert_eq!(app.state.project.vocal_edits[0].notes[1].shift,2.);
  command(&mut app,Command::Tool(Tool::Draw));paint(&mut app,&mut canvas);let note=hit(&app,Action::Vocal(Command::Note(ci,1)));
  app.mouse_move(note.cx(),note.cy());app.mouse_down();app.mouse_move(note.cx()+12.,note.cy()-r.h/24.);app.mouse_up();assert!(!app.state.project.vocal_edits[0].curve.is_empty());
  let edit=&app.state.project.vocal_edits[0];let(frame,_) = edit.curve.last().unwrap();let actual=69.+12.*(edit.corrected()[*frame as usize]as f32/440.).log2();let expected=app.state.playlist.vocal.low+(r.bottom()-(note.cy()-r.h/24.))/r.h*app.state.playlist.vocal.span;assert!((actual-expected).abs()<0.1,"drawing sets the visible pitch after a note shift");
  crate::history::undo(&mut app.state).unwrap();assert!(app.state.project.vocal_edits[0].curve.is_empty());
  command(&mut app,Command::Tool(Tool::Split));paint(&mut app,&mut canvas);let note=hit(&app,Action::Vocal(Command::Note(ci,1)));let count=app.state.project.vocal_edits[0].notes.len();app.mouse_move(note.cx(),note.cy());app.mouse_down();app.mouse_up();assert_eq!(app.state.project.vocal_edits[0].notes.len(),count+1);
  crate::history::undo(&mut app.state).unwrap();command(&mut app,Command::Tool(Tool::Time));paint(&mut app,&mut canvas);let note=hit(&app,Action::Vocal(Command::Note(ci,1)));
  app.alt=true;app.mouse_move(note.cx(),note.cy());app.mouse_down();app.mouse_move(note.cx()+12.,note.cy());app.mouse_up();app.alt=false;
  assert!(app.state.project.vocal_edits[0].timing.len()>2);assert!(app.state.project.vocal_edits[0].valid());crate::history::undo(&mut app.state).unwrap();
  let before=app.state.project.vocal_edits.clone();pl::snapshot(&mut app.state);let c=pl::clips(&app.state)[ci];let split=pl::split(&mut app.state,ci,c.position+c.length/2).unwrap();
  pl::arr_mut(&mut app.state).clips[split].position+=192;
  assert_eq!(doc(&app.state,ci),doc(&app.state,split));assert_eq!(app.state.project.vocal_edits,before);
  let file=os_flp::project::create(&app.state.project).unwrap();let loaded=os_flp::project::load(&file).unwrap();assert_eq!(loaded.vocal_edits,app.state.project.vocal_edits);assert_eq!(loaded.arrangement().unwrap().clips.len(),pl::clips(&app.state).len());for(a,b)in loaded.arrangement().unwrap().clips.iter().zip(pl::clips(&app.state)){assert_eq!((a.position,a.length,a.track,a.source,a.window),(b.position,b.length,b.track,b.source,b.window));}
  command(&mut app,Command::Toggle(2));assert_eq!(app.state.playlist.vocal.row,None);assert_eq!(pl::Rows::new(&app.state).height(2),old);assert_eq!(app.state.project.vocal_edits,before);
  command(&mut app,Command::Toggle(2));assert_eq!(app.state.playlist.vocal.row,Some(2));
  if let Some(out) = std::env::var_os("OS_SHOT_DIR") {
    std::fs::create_dir_all(&out).unwrap();
    command(&mut app, Command::Tool(Tool::Select));
    paint(&mut app, &mut canvas);
    canvas.pixmap.save_png(PathBuf::from(&out).join("vocal-inline.png")).unwrap();
    app.state.vintage_2005 = true;
    let mut vintage = Canvas::new(1600, 1125, 1.25).unwrap();
    paint(&mut app, &mut vintage);
    vintage.pixmap.save_png(PathBuf::from(out).join("vocal-inline-vintage.png")).unwrap();
  }
  assert_eq!(std::fs::read(dir.join("Vocal.wav")).unwrap(),original_audio);
 }
 #[test]fn vocal_download_requires_consent_and_cancel_keeps_project(){
  let(mut app,_,_)=fixture();app.state.playlist.vocal.row=None;app.state.playlist.vocal.ready=Some(false);let before=app.state.project.clone();toggle(&mut app,2);
  assert!(app.vocal_job.is_none());assert_eq!(app.state.project,before);
  assert!(matches!(app.state.dialogs.last().map(|d|&d.body),Some(crate::dialog::Body::Msg(m))if m.then==crate::dialog::Then::VocalDownload(2)));
  app.state.dialogs.clear();command(&mut app,Command::Toggle(2));assert_eq!(app.state.project,before);
 }
 #[test]fn vocal_fresh_download_analyze_render_isolate_and_stale_undo(){
  if std::env::var("LEMON_VOCAL_DOWNLOAD_TEST").as_deref()!=Ok("1"){return;}
  let(mut app,dir,ci)=fixture();let root=engine::root().unwrap();assert!(!root.exists(),"Provide a fresh LEMON_VOCAL_TEST_ROOT for the optional download test");
  app.state.project.vocal_edits.clear();app.state.playlist.vocal=Default::default();
  let shared=pl::clips(&app.state)[ci].source;let mut other=pl::clips(&app.state)[ci];other.track=5;pl::arr_mut(&mut app.state).clips.push(other);
  let original=std::fs::read(dir.join("Vocal.wav")).unwrap();
  let mut canvas=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut canvas);
  let h=hit(&app,Action::Vocal(Command::Toggle(2)));app.mouse_move(h.cx(),h.cy());app.mouse_down();app.mouse_up();
  assert!(!engine::ready());assert!(app.vocal_job.is_none());assert!(!app.state.dialogs.is_empty());
  app.state.dialogs.clear();app.dialog_done(crate::dialog::Then::VocalDownload(2),crate::dialog::Reply::Button(crate::dialog::Answer::Yes));
  fn wait(app:&mut App){let until=Instant::now()+Duration::from_secs(60);while app.vocal_job.is_some(){tick(app);assert!(Instant::now()<until,"{}",app.state.playlist.vocal.message);std::thread::sleep(Duration::from_millis(10));}}
  wait(&mut app);assert!(engine::ready(),"{}",app.state.playlist.vocal.message);assert_eq!(app.state.project.vocal_edits.len(),1,"{}",app.state.playlist.vocal.message);
  assert_ne!(pl::clips(&app.state)[ci].source,shared);assert_eq!(pl::clips(&app.state).last().unwrap().source,shared);
  assert!(app.state.project.vocal_edits[0].notes.len()>=3);paint(&mut app,&mut canvas);
  let note=hit(&app,Action::Vocal(Command::Note(ci,1)));let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let pr=view::pitch_rect(&g,2);
  app.mouse_move(note.cx(),note.cy());app.mouse_down();app.mouse_move(note.cx(),note.cy()-3.*pr.h/app.state.playlist.vocal.span);app.mouse_up();
  std::thread::sleep(Duration::from_millis(190));tick(&mut app);wait(&mut app);
  let d=app.state.project.vocal_edits[0].clone();assert_eq!(d.render_key,key(&d),"{}",app.state.playlist.vocal.message);assert_ne!(d.rendered,d.source);
  let rendered=os_engine::sample::load_shared(std::path::Path::new(&d.rendered)).unwrap();assert!(Arc::ptr_eq(&rendered,app.vocal_audio.as_ref().unwrap()),"generated audio should be reused without decoding");assert_eq!((rendered.frames(),rendered.rate),(d.frames as usize,d.rate));
  assert_eq!(app.state.project.channel(d.channel).unwrap().sample_path.as_deref(),Some(d.rendered.as_str()));
  command(&mut app,Command::Formant(1));std::thread::sleep(Duration::from_millis(190));tick(&mut app);assert!(app.vocal_job.is_some());
  crate::history::undo(&mut app.state).unwrap();wait(&mut app);assert_eq!(app.state.project.vocal_edits[0],d);assert_eq!(app.state.project.channel(d.channel).unwrap().sample_path.as_deref(),Some(d.rendered.as_str()));
  let new=pl::make_unique(&mut app.state,ci).unwrap();assert_ne!(new,d.channel);let unique=doc(&app.state,ci).unwrap();assert_eq!(unique.source,d.source);assert_eq!(unique.notes,d.notes);
  let mut order:Vec<_>=(0..app.state.project.channels.len()).collect();order.reverse();crate::rack::reorder(&mut app,&order);assert!(doc(&app.state,ci).is_some());
  let docs=app.state.project.vocal_edits.clone();let id=doc(&app.state,ci).unwrap().channel;let row=app.state.project.channels.iter().position(|c|c.index==id).unwrap();
  crate::history::delete_channels(&mut app.state,vec![row],vec![]);let keep:Vec<_>=(0..app.state.project.channels.len()).filter(|r|*r!=row).collect();crate::rack::reorder(&mut app,&keep);assert_eq!(app.state.project.vocal_edits.len(),docs.len()-1);crate::history::undo(&mut app.state).unwrap();assert_eq!(app.state.project.vocal_edits,docs);
  let old=app.state.project.channels[row].sample_path.clone();app.state.project.channels[row].sample_path=Some(dir.join("Replacement.wav").to_string_lossy().into_owned());tick(&mut app);assert_ne!(app.state.project.channels[row].sample_path,old);assert!(doc(&app.state,ci).is_none());app.state.project.channels[row].sample_path=old;assert!(doc(&app.state,ci).is_some());
  let file=os_flp::project::create(&app.state.project).unwrap();let reopened=os_flp::project::load(&file).unwrap();assert_eq!(reopened.vocal_edits,app.state.project.vocal_edits);
  assert_eq!(std::fs::read(dir.join("Vocal.wav")).unwrap(),original);

 }

 #[test]fn vocal_polished_tools_navigation_and_group_inspector(){
  let(mut app,_,ci)=fixture();let mut c=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut c);
  let legacy:Note=serde_json::from_str(r#"{"start":0,"end":201,"base":57.0,"shift":0.0,"variation":1.0}"#).unwrap();assert_eq!(legacy,Note::new(0,201,57.));
  for t in TOOLS{assert!(hit(&app,Action::Vocal(Command::Tool(t))).w>20.);}
  let r=navigation::rect(&app).unwrap();let(x,y)=(r.x+r.w*0.4,r.cy());app.mouse_move(x,y);let old=app.state.playlist.vocal.low;app.wheel(0.,1.,false,false);assert!(app.state.playlist.vocal.low>old);
  let at=(app.state.playlist_scroll.0+x-app.pl_grid().unwrap().x)/app.state.playlist_bar_w;let z=app.state.playlist_bar_w;app.wheel(0.,1.,false,true);assert!(app.state.playlist_bar_w>z);let now=(app.state.playlist_scroll.0+x-app.pl_grid().unwrap().x)/app.state.playlist_bar_w;assert!((at-now).abs()<0.001);
  let pitch=app.state.playlist.vocal.low+(r.bottom()-y)/r.h*app.state.playlist.vocal.span;let span=app.state.playlist.vocal.span;app.wheel(0.,1.,true,true);assert!(app.state.playlist.vocal.span<span);let pitch2=app.state.playlist.vocal.low+(r.bottom()-y)/r.h*app.state.playlist.vocal.span;assert!((pitch-pitch2).abs()<0.001);
  let saved_zoom=app.state.playlist_bar_w;for _ in 0..12{app.wheel(0.,1.,false,true);}assert!(app.state.playlist_bar_w>saved_zoom*3.);let scroll=app.state.playlist_scroll.0;app.wheel(0.,-2.,true,false);assert!(app.state.playlist_scroll.0>scroll);command(&mut app,Command::Fit);paint(&mut app,&mut c);
  command(&mut app,Command::Tool(Tool::Zoom));paint(&mut app,&mut c);app.mouse_move(x,y);app.mouse_down_right();app.mouse_up();assert!(app.state.playlist.vocal.palette.is_some());paint(&mut app,&mut c);let tool=hit(&app,Action::Vocal(Command::Tool(Tool::Drift)));app.mouse_move(tool.cx(),tool.cy());app.mouse_down();app.mouse_up();assert_eq!(app.state.playlist.vocal.tool,Tool::Drift);assert!(app.state.playlist.vocal.palette.is_none());
  command(&mut app,Command::Tool(Tool::Main));paint(&mut app,&mut c);let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let note=view::note_rect(&app.state,&g,ci,1).unwrap();let grid=navigation::rect(&app).unwrap();
  app.mouse_move(grid.x+1.,grid.y+2.);app.mouse_down();app.mouse_move(note.right()+2.,grid.bottom()-1.);app.mouse_up();assert!(app.state.playlist.vocal.selected.len()>=2);assert!(app.state.playlist.vocal.marquee.is_none());
  for(tool,field)in [(Tool::Modulation,Field::Modulation),(Tool::Drift,Field::Drift),(Tool::Formant,Field::Formant),(Tool::Amplitude,Field::Amplitude),(Tool::Transition,Field::Transition)]{
   command(&mut app,Command::Tool(tool));paint(&mut app,&mut c);let before=app.state.project.vocal_edits.clone();let rr=hit(&app,Action::Vocal(Command::Param(field)));app.mouse_move(rr.cx(),rr.cy());app.mouse_down();app.mouse_move(rr.cx(),rr.cy()-24.);app.mouse_up();assert_ne!(app.state.project.vocal_edits,before);crate::history::undo(&mut app.state).unwrap();assert_eq!(app.state.project.vocal_edits,before);
  }
  command(&mut app,Command::Tool(Tool::Time));paint(&mut app,&mut c);let edge=hit(&app,Action::Vocal(Command::Boundary(ci,1)));app.alt=true;app.mouse_move(edge.cx(),edge.cy());app.mouse_down();app.mouse_move(edge.cx()+8.,edge.cy());app.mouse_up();app.alt=false;assert!(app.state.project.vocal_edits[0].timing.len()>2);assert!(app.state.project.vocal_edits[0].valid());crate::history::undo(&mut app.state).unwrap();
  command(&mut app,Command::Tool(Tool::Attack));paint(&mut app,&mut c);let note=hit(&app,Action::Vocal(Command::Note(ci,1)));app.alt=true;app.mouse_move(note.cx(),note.cy());app.mouse_down();app.mouse_move(note.cx()+6.,note.cy());app.mouse_up();app.alt=false;assert!(app.state.project.vocal_edits[0].timing.len()>2);assert!(app.state.project.vocal_edits[0].valid());crate::history::undo(&mut app.state).unwrap();
  for size in [(1280,900),(900,760)]{app.state.screen=(size.0 as f32,size.1 as f32);let mut canvas=Canvas::new(size.0,size.1,1.).unwrap();paint(&mut app,&mut canvas);let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let pr=view::pitch_rect(&g,2);for tool in TOOLS{let rr=hit(&app,Action::Vocal(Command::Tool(tool)));assert!(rr.bottom()<pr.y&&rr.right()<g.grid.right());}let rr=hit(&app,Action::Vocal(Command::Param(Field::Pitch)));assert!(rr.bottom()<pr.y);if let Some(out)=std::env::var_os("OS_SHOT_DIR"){std::fs::create_dir_all(&out).unwrap();canvas.pixmap.save_png(PathBuf::from(out).join(format!("vocal-polished-{}.png",size.0))).unwrap();}}
  let n=&mut app.state.project.vocal_edits[0].notes[1];n.drift=0.4;n.variation=0.7;n.formant=-2.;n.gain=-3.;n.transition=0.5;n.muted=true;let file=os_flp::project::create(&app.state.project).unwrap();let loaded=os_flp::project::load(&file).unwrap();assert_eq!(loaded.vocal_edits,app.state.project.vocal_edits);

 }

 #[test]fn vocal_hold_auditions_source_and_tracks_drag_without_changing_transport(){
  let(mut app,dir,ci)=fixture();let source=dir.join("Vocal.wav");let audio=os_engine::sample::load_shared(&source).unwrap();audition::remember(&mut app,source.to_string_lossy().into_owned(),audio);let before=app.state.playlist.pos;let mut canvas=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut canvas);let rr=hit(&app,Action::Vocal(Command::Note(ci,1)));let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let r=view::pitch_rect(&g,2);app.mouse_move(rr.cx(),rr.cy());app.mouse_down();assert!(app.vocal_audition.active.is_some());app.mouse_move(rr.cx(),rr.cy()-2.*r.h/24.);assert!((app.vocal_audition.cents-200.).abs()<0.1);assert_eq!(app.state.playlist.pos,before);app.mouse_up();assert!(app.vocal_audition.active.is_none());app.state.playlist.vocal.click=None;paint(&mut app,&mut canvas);let rr=hit(&app,Action::Vocal(Command::Note(ci,1)));app.mouse_move(rr.cx(),rr.cy());app.mouse_down();assert!(app.vocal_audition.active.is_some());app.cancel_mouse_audition();assert!(app.vocal_audition.active.is_none());
 }

 #[test]fn vocal_main_grab_matches_hand_footprint(){
  let(mut app,_,ci)=fixture();let original=app.state.project.vocal_edits.clone();let mut c=Canvas::new(1280,900,1.).unwrap();
  for span in [24.,72.] {app.state.playlist.vocal.span=span;
   for point in 0..6 {
    app.state.project.vocal_edits=original.clone();app.state.playlist.vocal.click=None;paint(&mut app,&mut c);
    let g=pl::PGeom::new(&app.state,app.pl_grid().unwrap());let rr=view::note_rect(&app.state,&g,ci,1).unwrap();let pr=view::pitch_rect(&g,2);
    let(x,y)=match point {0=>(rr.cx(),rr.y-10.),1=>(rr.cx(),rr.bottom()+10.),2=>(rr.x-10.,rr.cy()),3=>(rr.right()+10.,rr.cy()),4=>(rr.x+1.,rr.y+1.),_=>(rr.cx(),rr.cy())};
    app.mouse_move(x,y);assert_eq!(app.top_hit(x,y).unwrap().action,Action::Vocal(Command::Note(ci,1)),"span {span}, point {point}");assert_eq!(app.cursor(),crate::app::Cursor::Playlist(pl::PlTool::Select,crate::playlist_tools::GRAB));
    app.mouse_down();assert!(matches!(app.drag,Some(Drag::Vocal(ref g))if g.note==Some(1)&&g.edge==0));app.mouse_move(x,y-2.*pr.h/span);assert_eq!(app.cursor(),crate::app::Cursor::Playlist(pl::PlTool::Select,crate::playlist_tools::GRABBING));app.mouse_up();assert!((app.state.project.vocal_edits[0].notes[1].shift-2.).abs()<0.01);crate::history::undo(&mut app.state).unwrap();assert_eq!(app.state.project.vocal_edits,original);
   }
  }
  app.state.playlist.vocal.span=24.;paint(&mut app,&mut c);let edge=hit(&app,Action::Vocal(Command::Boundary(ci,1)));app.mouse_move(edge.cx(),edge.cy());assert_eq!(app.top_hit(edge.cx(),edge.cy()).unwrap().action,Action::Vocal(Command::Boundary(ci,1)));
  let pr=navigation::rect(&app).unwrap();app.mouse_move(pr.right()-30.,pr.y+10.);assert!(!matches!(app.cursor(),crate::app::Cursor::Playlist(_,crate::playlist_tools::GRAB)));app.mouse_down();assert!(matches!(app.drag,Some(Drag::VocalNav(ref g))if g.kind==navigation::Kind::Box));app.mouse_up();
 }
 #[test]fn vocal_stability_deadzone_scrollbar_cache_and_icons(){
  let(mut app,_,ci)=fixture();let mut canvas=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut canvas);let rr=hit(&app,Action::Vocal(Command::Note(ci,1)));let before=app.state.project.vocal_edits.clone();app.mouse_move(rr.cx(),rr.cy());app.mouse_down();app.mouse_move(rr.cx()+1.,rr.cy()-1.);assert_eq!(app.state.project.vocal_edits,before);app.mouse_up();
  let d=&app.state.project.vocal_edits[0];let a=contour(&app.state,d);let b=contour(&app.state,d);assert!(Arc::ptr_eq(&a,&b));
  let r=navigation::rect(&app).unwrap();let low=app.state.playlist.vocal.low;let span=app.state.playlist.vocal.span;let y=r.y+(128.-low-span/2.)/128.*r.h;app.mouse_move(r.right()+5.,y);command(&mut app,Command::Nav(navigation::Kind::Vertical));assert!((app.state.playlist.vocal.low-low).abs()<0.001);app.mouse_up();
  app.mouse_move(r.x+15.,r.bottom()+10.);let scroll=app.state.playlist_scroll.0;command(&mut app,Command::Nav(navigation::Kind::Horizontal));assert!((app.state.playlist_scroll.0-scroll).abs()<0.001);app.mouse_up();
  if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){std::fs::create_dir_all(&dir).unwrap();let mut c=Canvas::new(780,170,1.).unwrap();c.clear(os_ui::Color::rgb(0x202b31));for(j,t)in TOOLS.into_iter().enumerate(){let x=10.+j as f32*59.;c.icon_stroke(&tool_icon(t),os_ui::Rect::new(x,15.,24.,24.),os_ui::Color::rgb(0xffdc85),1.45);c.icon_stroke(&tool_icon(t),os_ui::Rect::new(x,65.,44.,44.),os_ui::Color::rgb(0xddebf0),1.7);c.text(os_ui::Font::new(10),x,132.,os_ui::Color::rgb(0xddebf0),&format!("{:?}",t));}c.pixmap.save_png(PathBuf::from(dir).join("vocal-tool-icons.png")).unwrap();}
 }

 #[test]fn vocal_failure_disconnect_undo_redo_and_retry_recover(){
  let(mut app,_,_)=fixture();app.state.playlist.vocal.ready=Some(false);command(&mut app,Command::Formant(1));tick(&mut app);let k=key(&app.state.project.vocal_edits[0]);let(tx,rx)=mpsc::channel();tx.send(Err("injected failure".into())).unwrap();drop(tx);app.vocal_job=Some(Job{rx,cancel:Arc::new(AtomicBool::new(false)),expected:None,source:0,path:String::new(),row:2,key:k.clone()});tick(&mut app);assert!(app.vocal_job.is_none());assert!(app.state.playlist.vocal.failed.contains(&k));
  crate::history::undo(&mut app.state).unwrap();crate::history::redo(&mut app.state).unwrap();tick(&mut app);assert!(!app.state.playlist.vocal.failed.contains(&k),"redo must not remain blacklisted");
  let(tx,rx)=mpsc::channel();drop(tx);app.vocal_job=Some(Job{rx,cancel:Arc::new(AtomicBool::new(false)),expected:None,source:0,path:String::new(),row:2,key:k.clone()});tick(&mut app);assert!(app.vocal_job.is_none(),"disconnected worker must not freeze the scheduler");assert!(app.state.playlist.vocal.message.contains("unexpectedly"));command(&mut app,Command::Retry);assert!(app.state.playlist.vocal.failed.is_empty());
 }
 #[test]fn vocal_compressed_timing_handle_is_safe(){
  let(mut app,_,ci)=fixture();app.state.project.vocal_edits[0].timing=vec![(0.,0.),(0.4,0.5),(0.8,0.505),(2.,2.)];command(&mut app,Command::Tool(Tool::Attack));let mut c=Canvas::new(1280,900,1.).unwrap();paint(&mut app,&mut c);let r=hit(&app,Action::Vocal(Command::Note(ci,1)));app.mouse_move(r.cx(),r.cy());app.mouse_down();app.mouse_move(r.cx()+30.,r.cy());app.mouse_up();assert!(app.drag.is_none());
 }

 #[test]fn vocal_stretch_extremes_undo_redo_stay_renderable(){
  let(mut app,_,ci)=fixture();let mut c=Canvas::new(1280,900,1.).unwrap();command(&mut app,Command::Tool(Tool::Time));app.alt=true;
  for k in 0..24{paint(&mut app,&mut c);let r=hit(&app,Action::Vocal(Command::Boundary(ci,1)));app.mouse_move(r.cx(),r.cy());app.mouse_down();app.mouse_move(r.cx()+if k%2==0{5000.}else{-5000.},r.cy());app.mouse_up();assert!(engine::timing_valid(&app.state.project.vocal_edits[0]));crate::history::undo(&mut app.state);crate::history::redo(&mut app.state);assert!(engine::timing_valid(&app.state.project.vocal_edits[0]));}
 }

}
