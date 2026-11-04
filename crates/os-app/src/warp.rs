// SPDX-License-Identifier: GPL-3.0-or-later
//! Editable audio timing anchors, rendered off the audio/UI threads with the source kept intact.
use std::{cell::Cell,path::PathBuf,sync::{Arc,Mutex}};
use crate::{app::{App,Action,State,Ui},dialog::{Body,Modal,DHit},playlist as pl};
use os_model::{AudioWarp,ClipSource};
use os_ui::{Rect,Font,Color};
type Job=Arc<Mutex<Option<Result<PathBuf,String>>>>;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Command {Source,Marker(usize),Delete,Reset,Half,Double,Snap,Apply}
pub struct Form {
    pub clip:usize,pub original:os_model::Clip,pub channel:os_model::Channel,
    pub edit:AudioWarp,pub sample:Arc<os_engine::Sample>,pub selected:Option<usize>,
    pub wave:Cell<Rect>,pub snap:bool,pub tempo:f64,pub pending:Option<Job>,pub message:String,
}
impl Form {
    fn end(&self)->f64{self.edit.points.last().unwrap().1}
    fn range(&self)->(f64,f64){(self.edit.points[0].0,self.edit.points.last().unwrap().0)}
    fn moved(&mut self,i:usize,x:f32){
        if self.pending.is_some()||i==0||i+1>=self.edit.points.len(){return;}
        let r=self.wave.get();let mut beat=((x-r.x)/r.w).clamp(0.,1.) as f64*self.end();
        if self.snap{beat=(beat*4.).round()/4.;}
        let gap=((self.edit.points[i+1].1-self.edit.points[i-1].1)*0.25).min(0.001);
        self.edit.points[i].1=beat.clamp(self.edit.points[i-1].1+gap,self.edit.points[i+1].1-gap);
    }
}
pub fn open(app:&mut App,i:usize)->Result<(),String>{
    let clip=pl::clips(&app.state).get(i).cloned().ok_or("Select an audio clip")?;
    let ClipSource::Channel(id)=clip.source else{return Err("Select an audio clip".into())};
    let ch=app.state.project.channel(id).cloned().ok_or("Missing channel")?;
    if crate::ara::is(&ch){return Err("Use the ARA editor for this clip".into());}
    let path=ch.sample_path.as_deref().ok_or("The clip has no sample")?;
    if ch.sound.start_offset()!=0. || ch.sound.fx.start()!=0. || ch.sound.fx.length()!=1. || ch.sound.fx.trim!=0 || ch.sound.fx.flags & os_model::SampleFx::REVERSE!=0 {
        return Err("Render this clip to audio before warping its sampler trim, offset or reverse processing".into());
    }
    // A trimmed render is a new source window; keep that audio intact rather than
    // incorrectly mapping its milliseconds onto the earlier unwarped source.
    let resolved=crate::plugins::resolve_sample(path);
    let saved=app.state.project.audio_warps.iter().find(|w|clip.window.is_none() && (w.rendered==path || crate::plugins::resolve_sample(&w.rendered)==resolved)).cloned();
    let source=saved.as_ref().map_or(path,|w|w.source.as_str());
    let source=crate::plugins::resolve_sample(source);
    if std::fs::metadata(&source).is_ok_and(|m|m.len()>512*1024*1024){return Err("Split this large recording into smaller clips before warping".into());}
    let sample=os_engine::sample::load_shared(std::path::Path::new(&source)).map_err(|e|e.to_string())?;
    let secs=sample.frames() as f64/sample.rate.max(1) as f64;
    let (a,b)=clip.window.map(|(a,b)|(a/1000.,b/1000.)).unwrap_or((0.,secs));
    let end=clip.length as f64/app.state.project.ppq.max(1) as f64;
    if end<=0.||b.min(secs)-a.max(0.)<0.01{return Err("The clip is too short to warp".into());}
    let mut edit=saved.unwrap_or(AudioWarp{source:source.clone(),rendered:String::new(),points:vec![(a.max(0.),0.),(b.min(secs),end)]});
    edit.source=source;let scale=end/edit.points.last().unwrap().1;for p in &mut edit.points{p.1*=scale;}
    app.state.dialogs.push(Modal::new(Body::Warp(Form{clip:i,original:clip,channel:ch,edit,sample,selected:None,wave:Cell::new(Rect::new(0.,0.,1.,1.)),snap:true,tempo:app.state.project.tick_tempo(app.state.tempo),pending:None,message:"Click the source waveform to add an anchor; drag its lower handle to change timing.".into()})));
    app.invalidate();Ok(())
}
pub fn command(app:&mut App,c:Command){
    let x=app.mouse.0;
    let Some(Modal{body:Body::Warp(f),..})=app.state.dialogs.last_mut() else{return};
    if f.pending.is_some(){return;}
    match c {
        Command::Source=>{
            let r=f.wave.get();let(a,b)=f.range();let src=a+((x-r.x)/r.w).clamp(0.,1.) as f64*(b-a);
            if f.edit.points.len()>=1024{return;}
            let i=f.edit.points.partition_point(|p|p.0<src);
            if i==0||i==f.edit.points.len()||src-f.edit.points[i-1].0<0.005||f.edit.points[i].0-src<0.005{return;}
            let (l,r)=(f.edit.points[i-1],f.edit.points[i]);let dst=l.1+(src-l.0)/(r.0-l.0)*(r.1-l.1);
            f.edit.points.insert(i,(src,dst));f.selected=Some(i);
        }
        Command::Marker(i)=>{f.selected=Some(i);if i>0&&i+1<f.edit.points.len(){app.drag=Some(crate::app::Drag::WarpMarker(i));}}
        Command::Delete=>{if let Some(i)=f.selected.take().filter(|&i|i>0&&i+1<f.edit.points.len()){f.edit.points.remove(i);}}
        Command::Reset=>{let(a,b)=f.range();let end=f.end();f.edit.points=vec![(a,0.),(b,end)];f.selected=None;}
        Command::Half|Command::Double=>{let k=if c==Command::Half{0.5}else{2.};if (0.25..=4096.).contains(&(f.end()*k)){for p in &mut f.edit.points {p.1*=k;}}}
        Command::Snap=>f.snap=!f.snap,
        Command::Apply=>{
            let points:Vec<_>=f.edit.points.iter().map(|&(src,beat)|((src*f.sample.rate as f64).round() as usize,(beat*60./f.tempo*f.sample.rate as f64).round() as usize)).collect();
            let sample=f.sample.clone();let done=Arc::new(Mutex::new(None));f.pending=Some(done.clone());f.message="Rendering warp... Original audio is preserved.".into();
            let dir=crate::pattern_render::folder(&app.state).join("Warped");
            std::thread::spawn(move||{
                let result=(||{
                    let (l,r)=os_dsp::stretch::warp(&sample.left,&sample.right,sample.rate,&points)?;
                    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
                    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
                    let path=dir.join(format!("Warp-{stamp}.wav"));let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|e.to_string())?;
                    let stereo:Vec<_>=l.into_iter().zip(r).flat_map(|(l,r)|[l,r]).collect();
                    os_engine::wav::write_f32_stereo(&mut file,sample.rate,&stereo).map_err(|e|e.to_string())?;Ok(path)
                })();if let Ok(mut out)=done.lock(){*out=Some(result);}
            });
        }
    }
    app.invalidate();
}
pub fn drag(app:&mut App,i:usize,x:f32){if let Some(Modal{body:Body::Warp(f),..})=app.state.dialogs.last_mut(){f.moved(i,x);app.invalidate();}}
pub fn poll(app:&mut App){
    let Some(Modal{body:Body::Warp(f),..})=app.state.dialogs.last_mut() else{return};
    let result=f.pending.as_ref().and_then(|p|p.lock().ok()?.take());let Some(result)=result else{return};f.pending=None;
    let path=match result{Ok(p)=>p,Err(e)=>{f.message=e;app.invalidate();return}};
    let Some(Modal{body:Body::Warp(mut f),..})=app.state.dialogs.pop() else{return};
    if pl::clips(&app.state).get(f.clip)!=Some(&f.original){app.state.message=Some("The clip changed; warp was rendered but not applied".into());return;}
    let Some(index)=app.state.project.channels.iter().map(|c|c.index).max().unwrap_or(0).checked_add(1) else{app.state.message=Some("No channel IDs available".into());return};
    let row=app.state.project.channels.len();
    let before=crate::history::Snap::Several(vec![crate::history::Snap::Rack(Box::new(crate::history::RackSide::now(&app.state,row))),crate::history::Snap::AudioWarps(app.state.project.audio_warps.clone())]);
    pl::snapshot_with(&mut app.state,"Warp audio",before);
    f.channel.index=index;f.channel.sample_path=Some(path.to_string_lossy().into_owned());f.channel.plugin_state=None;f.channel.plugin=None;f.channel.kind=os_model::ChannelKind::AudioClip;
    let end=f.end();
    f.channel.sound.stretch[0]=0;f.channel.sound.stretch[2]=0;crate::loop_fit::set_fitted_time(&mut f.channel.sound,end);
    f.channel.name=Some(format!("{} (warped)",f.channel.name.as_deref().unwrap_or("Audio")));
    crate::rack::push_channel(&mut app.state,f.channel);
    f.edit.rendered=path.to_string_lossy().into_owned();app.state.project.audio_warps.push(f.edit.clone());
    let ppq=app.state.project.ppq.max(1);if let Some(c)=pl::arr_mut(&mut app.state).clips.get_mut(f.clip){c.source=ClipSource::Channel(index);c.length=(end*ppq as f64).round().max(1.) as u32;c.window=None;}
    app.state.piano.dirty=true;app.state.message=Some("Warp applied. Reopen Warp markers to edit; Ctrl+Z restores the original clip.".into());app.invalidate();
}
pub fn paint(ui:&mut Ui,r:Rect,f:&Form){
    let wave=Rect::new(r.x+20.,r.y+66.,r.w-40.,112.);f.wave.set(wave);
    ui.c.text(Font::new(12),wave.x,r.y+39.,ui.t.text,"SOURCE - click a transient to add an anchor");
    ui.c.fill_rect(wave,Color::rgb(0x171d1d));
    let(a,b)=f.range();let rate=f.sample.rate as f64;
    for col in 0..wave.w as usize {let lo=((a+(b-a)*col as f64/wave.w as f64)*rate) as usize;let hi=((a+(b-a)*(col+1) as f64/wave.w as f64)*rate) as usize;
        let peak=f.sample.left.get(lo..hi.min(f.sample.frames())).unwrap_or(&[]).iter().fold(0f32,|v,x|v.max(x.abs())).min(1.);
        ui.c.line(wave.x+col as f32,wave.cy()-peak*48.,wave.x+col as f32,wave.cy()+peak*48.,1.,Color::rgb(0x9bbca6));
    }
    ui.hit(wave,Action::Dlg(DHit::Warp(Command::Source)),"Add a timing anchor at this source position");
    let dst=Rect::new(wave.x,wave.bottom()+42.,wave.w,68.);ui.c.fill_rect(dst,Color::rgb(0x202a29));
    ui.c.text(Font::new(12),dst.x,dst.y-24.,ui.t.text,&format!("BEAT GRID - {:.2} beats at {:.2} BPM",f.end(),f.tempo));
    let step=(f.end()/32.).ceil().max(1.);for i in 0..=(f.end()/step) as usize {let x=dst.x+(i as f64*step/f.end()) as f32*dst.w;ui.c.line(x,dst.y,x,dst.bottom(),1.,Color::rgb(0x46524b));ui.c.text(Font::new(10),x+2.,dst.bottom()+4.,ui.t.text,&format!("{}",i as f64*step+1.));}
    for (i,&(src,beat)) in f.edit.points.iter().enumerate(){let x=dst.x+(beat/f.end()) as f32*dst.w;let sx=wave.x+((src-a)/(b-a)) as f32*wave.w;
        let color=Color::rgb(if f.selected==Some(i){0xf4d57b}else{0xaebd87});
        ui.c.line(sx,wave.y,sx,wave.bottom(),1.,color);ui.c.line(sx,wave.bottom(),x,dst.y,1.,color.with_alpha(110));
        let handle=Rect::new(x-5.,dst.y,10.,dst.h);ui.c.fill_rrect(handle,2.,color);ui.hit(handle,Action::Dlg(DHit::Warp(Command::Marker(i))),"Drag this anchor along the beat grid");
    }
    for (i,(label,cmd)) in [("Remove anchor",Command::Delete),("Reset anchors",Command::Reset),("Half length",Command::Half),("Double length",Command::Double),(if f.snap{"Snap: 1/16"}else{"Snap: Off"},Command::Snap),("Apply warp",Command::Apply)].into_iter().enumerate(){
        let width=(r.w-40.)/6.;let b=Rect::new(r.x+20.+i as f32*width,r.bottom()-77.,width-5.,27.);
        ui.c.fill_rrect(b,3.,Color::rgb(if cmd==Command::Apply{0x566348}else{0x3a4440}));ui.c.text_center(Font::new(12),b,ui.t.text,label);
        if f.pending.is_none(){ui.hit(b,Action::Dlg(DHit::Warp(cmd)),label);}
    }
    ui.c.text_clip(Font::new(12),r.x+20.,r.bottom()-36.,r.w-40.,ui.t.text,&f.message);
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn community_warp_roundtrip_undo_and_render(){
    let dir=std::env::temp_dir().join(format!("lemon-warp-test-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
    let source=dir.join("source.wav");let rate=8000u32;
    let audio:Vec<f32>=(0..16000).flat_map(|i|{let x=(i as f32*440.*std::f32::consts::TAU/rate as f32).sin()*0.2;[x,x]}).collect();
    os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&source).unwrap(),rate,&audio).unwrap();
    let original_bytes=std::fs::read(&source).unwrap();
    let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.project.arrangements.push(os_flp::project::default_arrangement());
    app.state.tempo=120.;app.state.project.data_folder=Some(os_model::DataFolder{choice:2,folder:dir.to_string_lossy().into_owned()});
    let ch=&mut app.state.project.channels[0];ch.sample_path=Some(source.to_string_lossy().into_owned());let id=ch.index;
    let ppq=app.state.project.ppq;pl::arr_mut(&mut app.state).clips.push(os_model::Clip::new(0,ppq as u32*4,0,ClipSource::Channel(id)));
    let before=app.state.project.clone();open(&mut app,0).unwrap();
    let Some(Modal{body:Body::Warp(f),..})=app.state.dialogs.last_mut() else{panic!()};
    f.edit.points.insert(1,(1.,1.5));f.wave.set(Rect::new(0.,0.,400.,100.));f.moved(1,200.);assert_eq!(f.edit.points[1].1,2.);
    f.edit.points[1].1=1.5;
    if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){let mut c=os_ui::Canvas::new(1280,900,1.).unwrap();app.paint(&mut c,None);c.pixmap.save_png(std::path::PathBuf::from(dir).join("warp-editor.png")).unwrap();}
    command(&mut app,Command::Apply);
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
    while matches!(app.state.dialogs.last().map(|m|&m.body),Some(Body::Warp(_))) && std::time::Instant::now()<deadline {poll(&mut app);std::thread::sleep(std::time::Duration::from_millis(10));}
    assert!(app.state.dialogs.is_empty());assert_eq!(std::fs::read(&source).unwrap(),original_bytes);
    let after=app.state.project.clone();let warped=&after.audio_warps[0];let rendered=os_engine::sample::load_shared(std::path::Path::new(&warped.rendered)).unwrap();assert_eq!(rendered.frames(),16000);
    let file=os_flp::project::create(&after).unwrap();let loaded=os_flp::project::load(&file).unwrap();assert_eq!(loaded.audio_warps,after.audio_warps);
    crate::history::undo(&mut app.state);assert_eq!(app.state.project,before);
    crate::history::redo(&mut app.state);assert_eq!(app.state.project,after);
    open(&mut app,0).unwrap();let Some(Modal{body:Body::Warp(f),..})=app.state.dialogs.last() else{panic!()};assert_eq!(f.edit.points,warped.points);
 }
}
