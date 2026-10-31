// SPDX-License-Identifier: GPL-3.0-or-later
//! Select an input, press Play, and keep the take on its named and colored mixer track.
use crate::app::{App, State};
use crate::capture::Input;
use crate::playlist::{self as pl, Cmd, Mark, PItem, Popup};
use crate::record::Opt;
use os_model::Rgb;
use std::collections::HashMap;

#[derive(Default)]
pub struct Setup {
    pub inputs: HashMap<u16,Input>,
    pub armed: std::collections::BTreeSet<u16>,
    pub live: Vec<Destination>,
}
#[derive(Clone)]
pub struct Destination {
    pub mixer: u16,
    pub row: u16,
    pub start: u32,
    pub length: u32,
    pub name: String,
    pub color: Option<Rgb>,
}
pub fn flag(o: Opt, on: bool) { if o.on()!=on { o.toggle(); } }
pub fn audio_filter() {
    flag(Opt::Audio,true); flag(Opt::Notes,false); flag(Opt::Automation,false); flag(Opt::Wait,false);
}
pub fn auto_filter() { flag(Opt::Audio,false); flag(Opt::Notes,true); flag(Opt::Automation,false); }
pub fn input_menu(s: &State,t: u16,x: f32,y: f32)->Popup {
    use crate::mixer::MCmd;
    let now=s.mixer.recording.inputs.get(&t);
    let mut items=vec![PItem::new("None",Cmd::Mixer(MCmd::RecordInput(t,None))).mark(Mark::Radio(now.is_none()))];
    for (device,asio,channels) in crate::one_click::devices() {
        for stereo in [false,true] {
            let list:Vec<_>=Input::of(&device,asio,channels).into_iter().filter(|i|i.stereo==stereo).collect();
            if list.is_empty(){continue;}
            items.push(PItem::head(format!("{device} - {}",if stereo{"stereo"}else{"mono"})));
            for input in list {
                let selected=now==Some(&input);
                items.push(PItem::new(input.label(),Cmd::Mixer(MCmd::RecordInput(t,Some(input)))).mark(Mark::Radio(selected)));
            }
        }
    }
    items.push(PItem::head(format!("{} armed - 32-bit float WAV",s.mixer.recording.armed.len())));
    if items.len()==2 { items.push(PItem::new("No audio inputs found: check Settings > Audio",Cmd::Nothing).off()); }
    Popup{x,y,items,sub:None,scroll:[0;2]}
}
impl App {
    pub(crate) fn set_mixer_input(&mut self,t:u16,input:Option<Input>)->Option<String> {
        if self.audio_rec.is_some(){return Some("Stop the current take before changing its input".into());}
        crate::mixer::track(&self.state,t)?;
        self.input_monitor.retry();
        self.state.selected_mixer=t;
        match input {
            Some(i)=> {
                if self.state.mixer.recording.armed.len() >= 64 && !self.state.mixer.recording.armed.contains(&t) { return Some("Up to 64 inputs can be armed together".into()); }
                if self.state.mixer.recording.armed.iter().filter(|&&n| n != t).any(|n| self.state.mixer.recording.inputs.get(n).is_some_and(|a| a.device != i.device || a.asio != i.asio)) {
                    return Some("Simultaneous recording uses inputs from the same audio interface. Disarm the other device first".into());
                }
                let label=format!("{} - {}",i.device,i.label());
                crate::capture::set_last_input(&i);
                self.state.mixer.recording.inputs.insert(t,i);
                self.state.mixer.recording.armed.insert(t);
                audio_filter(); self.state.rec.armed=true; self.state.song_mode=true;
                self.invalidate();
                if let Err(e) = self.sync_input_monitor() { return Some(e); }
                Some(format!("{} ready: {label}. Monitoring on. Press Play to record audio",crate::mixer::name(&self.state,t)))
            }
            None=> {
                self.state.mixer.recording.inputs.remove(&t);
                self.state.mixer.recording.armed.remove(&t); if self.state.mixer.recording.armed.is_empty() && Opt::Smart.on(){auto_filter();}
                self.invalidate(); self.sync_input_monitor().err()
            }
        }
    }
    pub(crate) fn arm_mixer_disk(&mut self,t:u16)->Option<String> {
        if self.audio_rec.is_some(){return Some("Stop the current take before changing its recording arm".into());}
        if self.state.mixer.recording.armed.remove(&t) {
            if self.state.mixer.recording.armed.is_empty() && Opt::Smart.on(){auto_filter();}
            self.invalidate(); return self.sync_input_monitor().err();
        }
        match self.state.mixer.recording.inputs.get(&t).cloned() {
            Some(i)=>self.set_mixer_input(t,Some(i)),
            None=>{let(x,y)=self.mouse; self.state.playlist.popup=Some(input_menu(&self.state,t,x,y+10.));self.invalidate();None}
        }
    }
    /// Called before starting the transport, never by selecting an input alone.
    pub(crate) fn prepare_mixer_take(&mut self)->Result<(),String> {
        if !self.state.rec.armed || !Opt::Audio.on(){return Ok(());}
        self.sync_input_monitor()?;
        if self.audio_rec.is_some(){return if self.one_click_starting(){Ok(())}else{Err("The previous audio take is still being saved".into())};}
        let Some(t)=self.state.mixer.recording.armed.first().copied() else{return Ok(())};
        let Some(input)=self.state.mixer.recording.inputs.get(&t).cloned() else{return Err("Select an audio input first".into())};
        let stream=if input.asio {
            if let Some(device)=self.transport.opened() {
                if device.host!=crate::device::ASIO || device.name!=input.device || input.first+u16::from(input.stereo)>=device.inputs {
                    return Err("The selected ASIO input is unavailable. Check Settings > Audio and select an input again".into());
                }
            }
            None
        }else if self.input_monitor.rate.is_some() {
            None
        }else{
            let stream=crate::capture::open_input(&input.device)?;
            if input.first+u16::from(input.stereo)>=stream.channels{return Err("The selected input channel is unavailable".into());}
            Some(stream)
        };
        self.state.song_mode=true;
        let start=self.selection_loop().filter(|&(a,b)|self.state.playlist.pos<a||self.state.playlist.pos>=b).map_or(self.state.playlist.pos,|r|r.0);
        let rows=pl::Rows::new(&self.state);
        let _row=(0..pl::TRACKS).find(|&row|rows.height(row)>0.&&!pl::track_info(&self.state,row).is_some_and(|t|t.locked)&&!pl::clips(&self.state).iter().any(|c|c.track==row)).ok_or("There is no free playlist track for this take")?;
        let armed: Vec<u16> = self.state.mixer.recording.armed.iter().copied().collect();
        if armed.len() > 64 { return Err("Up to 64 mixer inputs can be recorded together".into()); }
        let free: Vec<u16> = (0..pl::TRACKS).filter(|&r| rows.height(r)>0. && !pl::track_info(&self.state,r).is_some_and(|t|t.locked) && !pl::clips(&self.state).iter().any(|c|c.track==r)).take(armed.len()).collect();
        if free.len()!=armed.len() { return Err("Not enough empty Playlist tracks for the armed inputs".into()); }
        let mut tracks = Vec::new();
        for (t,row) in armed.into_iter().zip(free) {
            let selected=self.state.mixer.recording.inputs.get(&t).cloned().ok_or("An armed track has no input")?;
            if selected.device!=input.device || selected.asio!=input.asio { return Err("Armed inputs must use the same audio interface".into()); }
            let channels = if selected.asio { self.transport.opened().map(|d| d.inputs) } else { stream.as_ref().map(|s|s.channels).or_else(||self.input_monitor.channels()) };
            if channels.is_some_and(|n| selected.first+u16::from(selected.stereo)>=n) { return Err(format!("{}: input unavailable",crate::mixer::name(&self.state,t))); }
            let color=crate::mixer::track(&self.state,t).and_then(|m|m.color);
            tracks.push((selected,Destination { mixer:t,row,start,length:1,name:crate::mixer::name(&self.state,t),color }));
        }
        self.begin_mixer_group(tracks,stream)?;
        self.invalidate();Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loop_audio_records_three_lanes_without_trimming_away_previous_passes() {
        let _lock=crate::capture::TEST_RECORDING.lock().unwrap_or_else(|e|e.into_inner());
        let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.project.tempo=Some(120.);app.state.tempo=120.;
        app.state.playlist.range=Some((0,192));flag(Opt::Loop,true);
        app.set_mixer_input(1,Some(Input{device:"Test ASIO".into(),asio:true,first:0,stereo:false}));
        app.prepare_mixer_take().unwrap();app.transport.pretend_at(0.,true);app.transport.pretend_seconds(0.);app.one_click_tick();
        for _ in 0..80 {crate::capture::push_inputs(true,600,|_,_|0.25);}
        app.transport.pretend_at(192.,true);app.transport.pretend_seconds(1.);app.one_click_tick();
        app.transport.pretend_at(0.,true);app.transport.pretend_seconds(0.);app.one_click_tick();
        for _ in 0..80 {crate::capture::push_inputs(true,600,|_,_|0.5);}
        app.transport.pretend_at(0.,true);app.one_click_tick();
        for _ in 0..40 {crate::capture::push_inputs(true,600,|_,_|0.75);}
        app.transport.stop();
        for _ in 0..500 {app.one_click_tick();if app.audio_rec.is_none(){break;}std::thread::sleep(std::time::Duration::from_millis(5));}
        assert!(app.audio_rec.is_none(),"{:?}",app.state.message);
        let p=crate::comping::for_mixer(&app.state,1).unwrap();
        let takes:Vec<_>=pl::clips(&app.state).iter().filter(|c|crate::comping::parent(&app.state,c.track)==Some(p)).collect();
        assert_eq!(takes.len(),2,"{:?}",app.state.message);
        let path=std::path::PathBuf::from(app.state.project.channels.last().unwrap().sample_path.as_ref().unwrap());
        let sample=os_engine::sample::load_shared(&path).unwrap();assert_eq!(sample.frames(),120000);
        assert_eq!(sample.left[0],0.25);assert_eq!(sample.left[50000],0.5);assert_eq!(sample.left[100000],0.75);
        let render=|project:&os_model::Project| {
            let mut library=os_engine::sample::SampleLibrary::default();
            os_engine::Engine::new(project,&mut library,48000,os_engine::PlayMode::Song).render_all(256)
        };
        let before=render(&app.state.project);
        let source=pl::clips(&app.state).iter().position(|c|crate::comping::parent(&app.state,c.track)==Some(p)&&c.window.is_some_and(|w|w.0==1000.)).unwrap();
        app.state.playlist.range=Some((96,144));assert!(crate::comping::choose(&mut app.state,source,true).starts_with("Comp updated"));
        let after=render(&app.state.project);
        let at=25000*2;assert!(before[at].abs()>0.01);assert!((after[at]/before[at]-2.0).abs()<0.01,"chosen source must actually play: before={} after={}",before[at],after[at]);
        let mut without_takes=app.state.project.clone();without_takes.arrangements[0].clips.retain(|c|c.track==p);
        assert_eq!(after,render(&without_takes),"muted source lanes never double playback");
        if let Some(dir)=std::env::var_os("LEMON_COMP_PREVIEW") {
            let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
            app.state.z=vec![crate::app::Win::Playlist];app.state.playlist_bar_w=600.;
            let mut canvas=os_ui::Canvas::new(1440,900,1.0).unwrap();app.paint(&mut canvas,None);
            canvas.pixmap.save_png(dir.join("take-lanes.png")).unwrap();
        }
        flag(Opt::Loop,false);let _=std::fs::remove_file(path);
    }

    #[test]
    fn simultaneous_inputs_stay_separate_aligned_and_restore_routes() {
        let _lock=crate::capture::TEST_RECORDING.lock().unwrap_or_else(|e|e.into_inner());
        let mut app=App::new(None); app.state.project=crate::app::test_project();
        let before=app.state.project.channels.len();
        let mut paths=Vec::new();
        for (t,first,stereo) in [(1,0,false),(2,1,false),(3,0,true)] {
            let m=crate::mixer::track_mut(&mut app.state,t).unwrap();
            m.name=Some(format!("Input {t}"));m.color=Some(Rgb::new(t as u8*50,80,130));
            app.set_mixer_input(t,Some(Input{device:"Test ASIO".into(),asio:true,first,stereo}));
        }
        assert_eq!(app.state.mixer.recording.armed.len(),3);
        assert!(!crate::capture::capture().on(),"arming does not record");
        // A different device must not silently replace any of the armed inputs.
        let error=app.set_mixer_input(4,Some(Input{device:"Other".into(),asio:true,first:0,stereo:false})).unwrap();
        assert!(error.contains("same audio interface"));assert_eq!(app.state.mixer.recording.armed.len(),3);
        for take in 0..2 {
            app.state.rec.armed=true;
            app.prepare_mixer_take().unwrap();
            assert_eq!(app.state.mixer.recording.live.len(),3);
            let rows:std::collections::BTreeSet<_>=app.state.mixer.recording.live.iter().map(|d|d.row).collect();
            assert_eq!(rows.len(),3);
            app.transport.pretend_at(0.,true);app.one_click_tick();
            // Reject an unrelated system callback, then record two distinguishable input signals.
            crate::capture::push_inputs(false,64,|_,_|9.0);
            for block in 0..8 {
                crate::capture::push_inputs(true,600,|ch,f| if ch==0 {(block*600+f) as f32/10000.}else{-0.375});
            }
            if take==0 {app.transport.stop();} else {app.state.rec.armed=false;}
            for _ in 0..500 {app.one_click_tick();if app.audio_rec.is_none(){break;}std::thread::sleep(std::time::Duration::from_millis(5));}
            assert!(app.audio_rec.is_none(),"{:?}",app.state.message);
            assert!(app.state.mixer.recording.live.is_empty());
            assert!(!crate::capture::capture().on());
            for (offset,t) in (1..=3).enumerate() {
                let c=&app.state.project.channels[before+take*3+offset];
                assert_eq!(c.mixer_track,t);
                assert_eq!(c.color,crate::mixer::track(&app.state,t).unwrap().color);
                let path=std::path::PathBuf::from(c.sample_path.as_ref().unwrap());
                assert!(!paths.contains(&path),"each take has a unique path");
                let sample=os_engine::sample::load_shared(&path).unwrap();
                assert_eq!(sample.frames(),4800);
                for f in [0,599,600,4799] {
                    let left=if t==2 {-0.375}else{f as f32/10000.};
                    let right=if t==1 {left}else{-0.375};
                    assert!((sample.left[f]-left).abs()<1e-6);
                    assert!((sample.right[f]-right).abs()<1e-6);
                }
                paths.push(path);
            }
            let groups=app.state.project.arrangement().unwrap().tracks.iter().filter(|t|t.comp.is_some()).count();
            assert_eq!(groups,if take==0 {0}else{3});
            assert_eq!(pl::clips(&app.state).len(),(take+1)*3,"no duplicate first takes");
            app.transport.stop();
        }
        let bytes=os_flp::project::create(&app.state.project).unwrap().to_bytes().unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
        for (a,b) in app.state.project.channels.iter().skip(before).zip(back.channels.iter().skip(before)) {assert_eq!(a.mixer_track,b.mixer_track);assert_eq!(a.color,b.color);}
        for p in paths {let _=std::fs::remove_file(p);}
        app.arm_mixer_disk(2);assert!(!app.state.mixer.recording.armed.contains(&2));assert_eq!(app.state.mixer.recording.armed.len(),2);
    }

    #[test]
    fn selecting_input_arms_audio_and_a_take_keeps_its_mixer_color_and_route() {
        let _lock=crate::capture::TEST_RECORDING.lock().unwrap_or_else(|e|e.into_inner());
        let mut app=App::new(None);app.state.project=crate::app::test_project();
        let color=Rgb::new(180,70,120);
        let m=crate::mixer::track_mut(&mut app.state,1).unwrap();m.name=Some("Main Vocals".into());m.color=Some(color);
        pl::place(&mut app.state,0,0);
        flag(Opt::Smart,true);
        app.set_mixer_input(1,Some(Input{device:"Test ASIO".into(),asio:true,first:0,stereo:false}));
        assert!(app.state.mixer.recording.armed.contains(&1));
        assert!(app.state.rec.armed&&Opt::Audio.on()&&!Opt::Notes.on()&&!Opt::Automation.on());
        assert!(!crate::capture::capture().on(),"selection arms but does not capture before Play");
        app.prepare_mixer_take().unwrap();
        let live=app.state.mixer.recording.live.first().unwrap();
        assert_eq!((live.row,live.color,live.name.as_str()),(1,Some(color),"Main Vocals"));
        app.transport.pretend_at(0.,true);app.one_click_tick();
        crate::capture::capture().push(4800,|ch,_|if ch==0{0.25}else{0.75});
        app.transport.stop();
        for _ in 0..400 {app.one_click_tick();if app.audio_rec.is_none(){break;}std::thread::sleep(std::time::Duration::from_millis(5));}
        assert!(app.audio_rec.is_none(),"{:?}",app.state.message);
        let c=app.state.project.channels.last().unwrap();
        assert_eq!((c.mixer_track,c.color),(1,Some(color)));
        let path=std::path::PathBuf::from(c.sample_path.as_ref().unwrap());
        let sample=os_engine::sample::load_shared(&path).unwrap();
        assert_eq!(sample.frames(),4800);
        assert!(sample.left.iter().all(|v|(*v-0.25).abs()<1e-6));assert_eq!(sample.left,sample.right,"mono input is on both sides");
        let clip=pl::clips(&app.state).last().unwrap();assert_eq!(clip.track,1);
        let row=pl::track_info(&app.state,1).unwrap();assert_eq!(row.color,Some(color));assert_eq!(row.name.as_deref(),Some("Main Vocals"));
        let project=std::env::temp_dir().join(format!("lemon-record-route-{}.flp",std::process::id()));
        app.save_to(project.clone());
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&std::fs::read(&project).unwrap()).unwrap()).unwrap();
        assert_eq!(back.channels.last().unwrap().mixer_track,1);assert_eq!(back.arrangement().unwrap().tracks[1].color,Some(color));
        app.arm_mixer_disk(1);assert!(Opt::Notes.on()&&!Opt::Automation.on()&&!Opt::Audio.on());
        let _=std::fs::remove_file(project);let _=std::fs::remove_file(path);
    }
}
