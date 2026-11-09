// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional signed WORLD engine. No engine code is loaded before signature/hash checks.
use std::{fs,path::{Path,PathBuf},io::Read,time::Duration};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use ed25519_dalek::{Signature,VerifyingKey};
use os_model::vocal::Edit;
// WORLD calls are serialized on workers; a superseded call finishing is not an engine failure.
static NATIVE_CALL:std::sync::Mutex<()>=std::sync::Mutex::new(());
struct CallGuard(std::sync::MutexGuard<'static,()>);
impl CallGuard{fn acquire()->Result<Self>{Ok(Self(NATIVE_CALL.lock().unwrap_or_else(|e|e.into_inner())))}}
pub type Result<T>=std::result::Result<T,String>;
pub const BASE:&str="https://michaellanemusic.de/lemon-studio/components/vocal-world-v1/";
pub fn platform()->String{format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH)}
fn filename()->String{format!("LemonVocalEngine-{}{}",platform(),if cfg!(windows){".dll"}else if cfg!(target_os="macos"){".dylib"}else{".so"})}
pub fn root()->Result<PathBuf>{
 #[cfg(test)]if let Some(p)=std::env::var_os("LEMON_VOCAL_TEST_ROOT"){return Ok(PathBuf::from(p));}
 Ok(os_paths::user_dir().ok_or("No user data folder")?.join("Components").join("vocal-world-v1"))}
fn hex(bytes:&[u8])->String{bytes.iter().map(|v|format!("{v:02x}")).collect()}
fn unhex(s:&str)->Result<Vec<u8>>{if !s.is_ascii()||s.len()%2!=0{return Err("Invalid component signature".into());}(0..s.len()).step_by(2).map(|i|u8::from_str_radix(&s[i..i+2],16).map_err(|e|e.to_string())).collect()}
#[derive(Serialize,Deserialize)]struct Signed{payload:String,signature:String}
#[derive(Serialize,Deserialize)]struct Manifest{revision:u32,platform:String,file:String,sha256:String,size:u64}
fn verify(data:&[u8])->Result<Manifest>{
 if data.len()>8192{return Err("Component manifest is too large".into());}
 let signed:Signed=serde_json::from_slice(data).map_err(|e|e.to_string())?;
 let key: [u8;32]=unhex(include_str!("../update-public-key.hex").trim())?.try_into().map_err(|_|"Invalid component public key")?;
 VerifyingKey::from_bytes(&key).map_err(|e|e.to_string())?.verify_strict(signed.payload.as_bytes(),&Signature::from_slice(&unhex(&signed.signature)?).map_err(|e|e.to_string())?).map_err(|_|"Vocal engine signature check failed")?;
 let m:Manifest=serde_json::from_str(&signed.payload).map_err(|e|e.to_string())?;
 if m.revision!=1||m.platform!=platform()||m.file!=filename()||m.size==0||m.size>8*1024*1024||unhex(&m.sha256)?.len()!=32{return Err("Wrong vocal engine package for this computer".into());}Ok(m)
}
fn checked(dir:&Path,m:&Manifest)->Result<PathBuf>{
 let p=dir.join(&m.file);let meta=fs::symlink_metadata(&p).map_err(|e|e.to_string())?;
 if !meta.is_file()||meta.file_type().is_symlink()||meta.len()!=m.size{return Err("Vocal engine is missing or damaged".into());}
 let data=fs::read(&p).map_err(|e|e.to_string())?;if hex(&Sha256::digest(&data))!=m.sha256{return Err("Vocal engine checksum failed".into());}Ok(p)
}
pub fn ready()->bool{root().and_then(|d|{let m=verify(&fs::read(d.join("manifest.json")).map_err(|e|e.to_string())?)?;checked(&d,&m)}).is_ok()}
pub fn install()->Result<()>{install_at(&root()?)}
pub fn install_at(dir:&Path)->Result<()>{
 fs::create_dir_all(dir).map_err(|e|e.to_string())?;
 let agent=ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(20)).timeout_read(Duration::from_secs(30)).build();
 let fetch=|url:&str,limit:u64|->Result<Vec<u8>>{let mut data=Vec::new();agent.get(url).call().map_err(|e|format!("Download failed: {e}"))?.into_reader().take(limit+1).read_to_end(&mut data).map_err(|e|e.to_string())?;if data.len()as u64>limit{return Err("Component download exceeded its size limit".into());}Ok(data)};
 let signed=fetch(&format!("{BASE}{}.json",platform()),8192)?;let m=verify(&signed)?;
 if checked(dir,&m).is_err(){let data=fetch(&format!("{BASE}{}",m.file),m.size)?;if data.len()as u64!=m.size||hex(&Sha256::digest(&data))!=m.sha256{return Err("Downloaded vocal engine failed verification".into());}
  let pending=dir.join(format!("{}.download",m.file));fs::write(&pending,data).map_err(|e|e.to_string())?;if dir.join(&m.file).exists(){fs::remove_file(dir.join(&m.file)).map_err(|e|e.to_string())?;}fs::rename(pending,dir.join(&m.file)).map_err(|e|e.to_string())?;}
 let temp=dir.join("manifest.pending");fs::write(&temp,signed).map_err(|e|e.to_string())?;if dir.join("manifest.json").exists(){fs::remove_file(dir.join("manifest.json")).map_err(|e|e.to_string())?;}fs::rename(temp,dir.join("manifest.json")).map_err(|e|e.to_string())?;checked(dir,&m)?;Ok(())
}
type Analyze=unsafe extern "C" fn(*const f32,*const f32,i32,i32,*mut f64,i32)->i32;
type Render=unsafe extern "C" fn(*const f32,*const f32,i32,i32,*const f64,*const f64,i32,f64,*mut f32,*mut f32)->i32;
pub struct Engine{_library:libloading::Library,analyze:Analyze,render:Render}
impl Engine{
 pub fn load()->Result<Self>{let dir=root()?;let m=verify(&fs::read(dir.join("manifest.json")).map_err(|e|e.to_string())?)?;Self::open(&checked(&dir,&m)?)}
 #[cfg(test)]pub(super)fn test_open(path:&Path)->Result<Self>{Self::open(path)}
 fn open(path:&Path)->Result<Self>{unsafe{
  let lib=libloading::Library::new(path).map_err(|e|format!("Cannot load the vocal engine: {e}"))?;
  let version=lib.get::<unsafe extern "C" fn()->i32>(b"lemon_vocal_version\0").map_err(|e|e.to_string())?;
  if version()!=1{return Err("Unsupported vocal engine version".into());}
  let analyze=*lib.get::<Analyze>(b"lemon_vocal_analyze\0").map_err(|e|e.to_string())?;
  let render=*lib.get::<Render>(b"lemon_vocal_render\0").map_err(|e|e.to_string())?;
  Ok(Self{_library:lib,analyze,render})
 }}
 pub fn analyze(&self,s:&os_engine::Sample)->Result<Vec<f32>>{
  let _guard=CallGuard::acquire()?;validate(s)?;let mut f0=vec![0.;(s.frames() as f64/s.rate as f64/0.005).floor() as usize+1];
  let n=unsafe{(self.analyze)(s.left.as_ptr(),s.right.as_ptr(),s.frames() as i32,s.rate as i32,f0.as_mut_ptr(),f0.len() as i32)};
  if n<=0||n as usize!=f0.len(){return Err("Vocal pitch analysis failed".into());}Ok(f0.into_iter().map(|v|v as f32).collect())
 }
 pub fn render(&self,s:&os_engine::Sample,d:&Edit)->Result<(Vec<f32>,Vec<f32>)>{
  let _guard=CallGuard::acquire()?;validate(s)?;if !d.valid()||d.frames as usize!=s.frames()||d.rate!=s.rate{return Err("Vocal source changed; reanalyse this recording".into());}
  if !d.changed(){return Ok((s.left.clone(),s.right.clone()));}
  let original:Vec<f64>=d.f0.iter().map(|v|f64::from(*v)).collect();let target=d.corrected();if target.iter().any(|v|!v.is_finite()||*v<0.||*v>4000.){return Err("Requested pitch exceeds this engine's range. Reduce the pitch or modulation and retry.".into());}let(mut l,mut r)=(vec![0.;s.frames()],vec![0.;s.frames()]);
  let result=unsafe{(self.render)(s.left.as_ptr(),s.right.as_ptr(),s.frames() as i32,s.rate as i32,original.as_ptr(),target.as_ptr(),original.len()as i32,f64::from(d.formant),l.as_mut_ptr(),r.as_mut_ptr())};
  if result!=0||l.iter().chain(&r).any(|v|!v.is_finite()){return Err("Vocal rendering failed; previous audio retained".into());}
  // Per-note spectral edits use short padded regions, not one full-file render per note.
  for note in &d.notes{if note.formant.abs()<=0.001{continue;}
   let begin=(note.start as usize).saturating_sub(20);let end=(note.end as usize+20).min(original.len());
   let start=(begin as f64*0.005*s.rate as f64).round()as usize;let stop=((end as f64*0.005*s.rate as f64).round()as usize).min(s.frames());if stop<=start{continue;}
   let frames=(((stop-start)as f64/s.rate as f64/0.005).floor()as usize)+1;
   let f:Vec<_>=(0..frames).map(|i|original[(begin+i).min(original.len()-1)]).collect();let t:Vec<_>=(0..frames).map(|i|target[(begin+i).min(target.len()-1)]).collect();let(mut nl,mut nr)=(vec![0.;stop-start],vec![0.;stop-start]);
   let result=unsafe{(self.render)(s.left[start..].as_ptr(),s.right[start..].as_ptr(),(stop-start)as i32,s.rate as i32,f.as_ptr(),t.as_ptr(),frames as i32,f64::from((d.formant+note.formant).clamp(-12.,12.)),nl.as_mut_ptr(),nr.as_mut_ptr())};
   if result!=0||nl.iter().chain(&nr).any(|v|!v.is_finite()){return Err("Note formant rendering failed; previous audio retained".into());}
   let a=(note.start as f64*0.005*s.rate as f64).round()as usize;let b=((note.end as f64*0.005*s.rate as f64).round()as usize).min(s.frames());let fade=(s.rate as f32*0.005).max(1.);
   for i in a..b{let mix=(((i-a).min(b-1-i))as f32/fade).clamp(0.,1.);l[i]+=(nl[i-start]-l[i])*mix;r[i]+=(nr[i-start]-r[i])*mix;}
  }
  for note in &d.notes{if !note.muted&&note.gain.abs()<0.001{continue;}
   let gain=if note.muted{0.}else{10f32.powf(note.gain/20.)};let a=(note.start as f64*0.005*s.rate as f64).round()as usize;let b=((note.end as f64*0.005*s.rate as f64).round()as usize).min(s.frames());let fade=(s.rate as f32*0.005).max(1.);
   for i in a..b{let mix=(((i-a).min(b-1-i))as f32/fade).clamp(0.,1.);let g=1.+(gain-1.)*mix;l[i]*=g;r[i]*=g;}
  }
  if d.timing.iter().any(|(a,b)|(a-b).abs()>0.0001){let anchors:Vec<_>=d.timing.iter().map(|(a,b)|((a*s.rate as f64).round()as usize,(b*s.rate as f64).round()as usize)).collect();os_dsp::stretch::warp(&l,&r,s.rate,&anchors)}else{Ok((l,r))}
 }
}
pub(super)fn timing_valid(d:&Edit)->bool{d.timing.len()<=1024&&d.timing.windows(2).all(|w|{let frame=|v:f64|(v*d.rate as f64).round()as usize;let(a,b,x,y)=(frame(w[0].0),frame(w[1].0),frame(w[0].1),frame(w[1].1));b>a&&y>x&&(0.0625..=16.).contains(&((y-x)as f64/(b-a)as f64))})}
fn validate(s:&os_engine::Sample)->Result<()>{if s.left.len()!=s.right.len()||s.frames()==0||!(16000..=192000).contains(&s.rate)||(s.frames()>s.rate as usize*600||s.frames()>33554432){return Err("Choose a vocal recording up to 10 minutes at 16–192 kHz".into());}Ok(())}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn vocal_native_engine_changes_pitch_and_keeps_dry_audio_exact(){
  let Ok(path)=std::env::var("LEMON_VOCAL_TEST_ENGINE")else{return};let engine=Engine::open(Path::new(&path)).unwrap();
  let rate=24000;let frames=rate as usize*2;let l:Vec<f32>=(0..frames).map(|i|{let t=i as f32/rate as f32;((t*220.*std::f32::consts::TAU).sin()+0.3*(t*440.*std::f32::consts::TAU).sin())*0.2}).collect();
  let s=os_engine::Sample{rate,left:l.clone(),right:l};let f0=engine.analyze(&s).unwrap();assert!(f0.iter().filter(|v|(**v-220.).abs()<5.).count()>200);
  let mut d=Edit{channel:0,source:"test".into(),source_hash:String::new(),rendered:String::new(),render_key:String::new(),rate,frames:frames as u32,f0,notes:vec![],curve:vec![],timing:vec![(0.,0.),(2.,2.)],enabled:true,strength:1.,formant:0.};
  assert_eq!(engine.render(&s,&d).unwrap(),(s.left.clone(),s.right.clone()));
  d.notes=vec![os_model::vocal::Note{start:0,end:d.f0.len()as u32,base:57.,shift:3.,variation:1.,..os_model::vocal::Note::new(0,1,57.)}];
  let(l,r)=engine.render(&s,&d).unwrap();let tuned=os_engine::Sample{rate,left:l,right:r};let result=engine.analyze(&tuned).unwrap();let voiced:Vec<_>=result.into_iter().filter(|v|*v>0.).collect();let near=voiced.iter().filter(|v|(**v-261.626).abs()<6.).count();assert!(near>voiced.len()/2,"rendered pitch must move by three semitones");
 }
 #[test]fn vocal_manifest_rejects_unsigned_payload(){assert!(verify(br#"{"payload":"{}","signature":"00"}"#).is_err());}
 #[test]fn vocal_note_level_mute_formant_and_timing_are_local(){
  let Ok(path)=std::env::var("LEMON_VOCAL_TEST_ENGINE")else{return};let e=Engine::open(Path::new(&path)).unwrap();let rate=24000;let l:Vec<f32>=(0..48000).map(|i|(i as f32*220.*std::f32::consts::TAU/rate as f32).sin()*0.2).collect();let s=os_engine::Sample{rate,left:l.clone(),right:l};let f0=e.analyze(&s).unwrap();let mut d=Edit{channel:0,source:"test".into(),source_hash:String::new(),rendered:String::new(),render_key:String::new(),rate,frames:48000,f0,notes:vec![os_model::vocal::Note::new(100,200,57.)],curve:vec![],timing:vec![(0.,0.),(2.,2.)],enabled:true,strength:1.,formant:0.};
  d.notes[0].gain=-6.;let(l,_)=e.render(&s,&d).unwrap();assert_eq!(&l[..12000],&s.left[..12000]);for i in 13000..23000{assert!((l[i]-s.left[i]*10f32.powf(-6./20.)).abs()<0.0001);}
  d.notes[0].muted=true;let(l,_)=e.render(&s,&d).unwrap();assert!(l[13000..23000].iter().all(|v|v.abs()<0.0001));d.notes[0].muted=false;d.notes[0].gain=0.;d.notes[0].formant=3.;let(l,r)=e.render(&s,&d).unwrap();assert_eq!(&l[..12000],&s.left[..12000]);assert_eq!(&l[24000..],&s.left[24000..]);assert!(l.iter().chain(&r).all(|v|v.is_finite()));assert!(l[13000..23000].iter().zip(&s.left[13000..23000]).any(|(a,b)|(a-b).abs()>0.01));
  d.timing=vec![(0.,0.),(0.5,0.55),(1.,1.05),(2.,2.)];let(l,r)=e.render(&s,&d).unwrap();assert_eq!((l.len(),r.len()),(48000,48000));assert!(l.iter().chain(&r).all(|v|v.is_finite()));
 }

}
