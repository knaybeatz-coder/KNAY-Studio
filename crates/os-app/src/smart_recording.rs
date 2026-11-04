// SPDX-License-Identifier: GPL-3.0-or-later
//! Automatic MIDI and automation-clip recording when no audio input is armed.
use crate::app::{Action,App};
use crate::record::Opt;
use os_model::{AutoPoint,ClipSource};
use std::collections::HashMap;

#[derive(Debug,Default)]
pub struct Smart {
    pub initial:HashMap<u32,f64>,
    pub active:HashMap<u32,(u16,u32)>,
    pub origin:Option<u32>,
    pub last_tick:u32,
}
/// An assigned instrument's playlist row and the pattern the user currently selected.
pub(crate) fn instrument_lane(s:&crate::app::State)->Option<(u16,u16)> {
    let ch=s.project.channels.get(s.selected_channel)?.index;
    let arr=s.project.arrangement()?.number;
    s.project.instrument_lanes.iter().find(|v|v.0==ch&&v.1==arr).map(|v|(v.2,s.pattern.max(1)))
}

impl App {
    pub(crate) fn route_instrument_lane(&mut self,row:usize,mixer:u16)->bool {
        use crate::{playlist as pl,history::{self,Snap}};
        let s=&mut self.state;
        let Some(ch)=s.project.channels.get(row).cloned() else {return false;};
        if !matches!(ch.kind,os_model::ChannelKind::Plugin){return false;}
        let arr=pl::arr_mut(s).number;
        let old=s.project.instrument_lanes.iter().find(|v|v.0==ch.index&&v.1==arr).copied();
        let lane=old.map(|v|v.2).or_else(||(0..pl::TRACKS).find(|t|
            !pl::clips(s).iter().any(|c|c.track==*t) && !s.project.instrument_lanes.iter().any(|v|v.1==arr&&v.2==*t)));
        let Some(lane)=lane else {s.message=Some("No empty playlist track available".into());return true;};
        let snap=Snap::Several(vec![Snap::ChannelRoute(row,ch.mixer_track),Snap::Playlist(Box::new(pl::capture(s))),
            Snap::InstrumentLanes(s.project.instrument_lanes.clone())]);
        history::record(s,"Assign instrument to mixer and playlist",snap);
        // Routing must not create or name the next pattern, or change the user's selection.
        // Keep the legacy tuple field for project compatibility; recording uses the current pattern.
        if old.is_none() {s.project.instrument_lanes.push((ch.index,arr,lane,s.pattern.max(1)));}
        let name=crate::mixer::name(s,mixer);let color=crate::mixer::track(s,mixer).and_then(|m|m.color);
        if let Some(t)=pl::arr_mut(s).tracks.get_mut(lane as usize){t.name=Some(name);t.color=color;}
        s.project.channels[row].mixer_track=mixer;s.selected_channel=row;s.selected_mixer=mixer;
        s.playlist.track=lane;
        s.piano.dirty=true;
        self.transport.update(&crate::rack::play_project(&self.state));self.invalidate();true
    }

    fn smart_automation_live(&self)->bool {
        Opt::Smart.on() && Opt::Automation.on() && self.state.rec.armed
            && self.state.rec.count.is_none() && self.transport.playing() && !self.transport.paused()
    }
    pub(crate) fn smart_begin_host(&mut self,a:&Action) {
        if !self.smart_automation_live(){return;}
        if let Some((target,_,v))=self.automation_target(a){self.state.rec.smart.initial.entry(target).or_insert(v);}
    }
    pub(crate) fn smart_record_host(&mut self,a:&Action) {
        if !self.smart_automation_live() && !self.state.params.capture_armed {return;}
        if let Some((target,name,v))=self.automation_target(a){
            if !self.capture_control(target,&name,v) {self.smart_record_value(target,name,v);}
        }
    }
    pub(crate) fn smart_record_value(&mut self,target:u32,name:String,value:f64) {
        if !self.smart_automation_live() || !value.is_finite(){return;}
        let tick=self.transport.tick().max(0.).round() as u32;
        let origin=self.state.rec.smart.origin.unwrap_or(self.state.playlist.pos).min(tick);
        let (ch,start)=match self.state.rec.smart.active.get(&target).copied() {
            Some(a)=>a,
            None=> {
                let baseline=self.state.rec.smart.initial.get(&target).copied().unwrap_or(value);
                let range=self.state.playlist.range.take();
                self.state.playlist.range=Some((origin,tick.saturating_add(1).max(origin+1)));
                let ch=crate::playlist::create_automation(&mut self.state,target,&name,baseline);
                self.state.playlist.range=range;
                let beat=f64::from(tick-origin)/f64::from(self.state.project.ppq.max(1));
                if let Some(a)=self.state.project.automation_mut(ch){
                    a.points=vec![AutoPoint::new(0.,baseline)];
                    if beat>0. {a.points.push(AutoPoint::new((beat-1./192.).max(0.),baseline));}
                }
                self.state.rec.smart.active.insert(target,(ch,origin));
                (ch,origin)
            }
        };
        let beat=f64::from(tick.saturating_sub(start))/f64::from(self.state.project.ppq.max(1));
        if let Some(a)=self.state.project.automation_mut(ch) {
            let v=value.clamp(0.,1.);
            if let Some(last)=a.points.last_mut().filter(|p|(p.beat-beat).abs()<1e-9){last.value=v;}
            else {a.points.push(AutoPoint::new(beat,v));}
        }
        self.state.piano.dirty=true;
        self.smart_record_tick();
        self.invalidate();
    }
    pub(crate) fn smart_record_tick(&mut self) {
        let live=self.state.rec.armed&&self.transport.playing()&&!self.transport.paused();
        if live {self.state.rec.smart.last_tick=self.transport.tick().max(0.).round() as u32;}
        let tick=self.state.rec.smart.last_tick;
        let active:Vec<_>=self.state.rec.smart.active.values().copied().collect();
        for (ch,start) in active {
            let len=tick.saturating_sub(start).max(1);
            for arr in &mut self.state.project.arrangements {for clip in &mut arr.clips {
                if clip.source==ClipSource::Channel(ch)&&clip.position==start {clip.length=len;}
            }}
            if !live {
                let beat=f64::from(len)/f64::from(self.state.project.ppq.max(1));
                if let Some(a)=self.state.project.automation_mut(ch) {
                    if let Some(last)=a.points.last().copied().filter(|p|p.beat<beat){a.points.push(AutoPoint::new(beat,last.value));}
                }
            }
        }
        if !self.state.rec.smart.active.is_empty(){self.state.piano.dirty=true;self.damage_win(crate::app::Win::Playlist);}
        if !live {self.state.rec.smart=Smart::default();}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_instruments_preserves_selected_pattern_and_names() {
        let mut app=App::new(None);app.state.project=crate::app::test_project();
        let mut second=app.state.project.channels[0].clone();second.index=1;
        app.state.project.channels=vec![app.state.project.channels[0].clone(),second];
        app.state.project.patterns=vec![os_model::Pattern{number:1,..Default::default()},os_model::Pattern{number:2,..Default::default()}];
        for (row,name,pat) in [(0,"Diva",1),(1,"Vital",2)] {
            app.state.project.channels[row].kind=os_model::ChannelKind::Plugin;
            app.state.project.channels[row].name=Some(name.into());
            app.state.pattern=pat;app.state.project.current_pattern=Some(pat);
            assert!(app.route_instrument_lane(row,10+row as u16));
            assert_eq!(app.state.pattern,pat);
            assert_eq!(app.state.project.current_pattern,Some(pat));
            assert_eq!(instrument_lane(&app.state).unwrap().1,pat);
            assert_eq!(app.state.project.patterns.len(),2);
            assert!(app.state.project.patterns.iter().all(|p|p.name.is_none()));
        }
        // An old saved lane association cannot redirect a newly chosen pattern.
        app.state.pattern=1;
        app.state.project.patterns[0].name=Some("My verse".into());
        assert!(app.route_instrument_lane(1,12));
        assert_eq!(instrument_lane(&app.state).unwrap().1,1);
        assert_eq!(app.state.project.patterns[0].name.as_deref(),Some("My verse"));
        assert_eq!(app.state.project.patterns.len(),2);
    }

    #[test]
    fn instrument_lane_records_midi_and_roundtrips() {
        use crate::{playlist as pl,record::{Opt,Extra},step_edit::Src};
        let mut app=App::new(None);app.state.project=crate::app::test_project();
        app.state.project.channels[0].kind=os_model::ChannelKind::Plugin;
        app.state.project.channels[0].name=Some("Citrus".into());
        app.state.project.channels[0].plugin=Some("Sytrus".into());
        pl::arr_mut(&mut app.state).clips.clear();
        pl::arr_mut(&mut app.state).clips.push(os_model::Clip::new(0,384,0,ClipSource::Pattern(99)));
        assert!(app.route_instrument_lane(0,10));
        let (lane,pat)=instrument_lane(&app.state).unwrap();assert_eq!(lane,1);
        assert_eq!(app.state.project.channels[0].mixer_track,10);
        assert!(pl::track_info(&app.state,lane).unwrap().name.is_some());
        let count=app.state.project.patterns.len();
        app.route_instrument_lane(0,9);assert_eq!(app.state.project.patterns.len(),count);
        // Even a legacy starts-playback preference must not start playback on arm.
        crate::mixer_recording::flag(Opt::StartsPlayback,true);
        app.toggle_record();assert!(app.state.rec.armed);assert!(!app.transport.playing());
        app.toggle_record();
        // No output device: simulate running transport and inject synthetic MIDI events.
        app.transport.pretend_at(0.,true);app.toggle_record();
        assert!(Opt::Notes.on()&&!Opt::Automation.on()&&!Opt::Audio.on());
        assert!(app.state.rec.armed&&app.state.playlist.popup.is_none());
        app.record_down_with(Src::Midi(60),60,100,Extra::default());
        app.transport.pretend_at(96.,true);app.record_up_with(Src::Midi(60),None);
        assert!(app.state.project.pattern(pat).unwrap().notes.iter().any(|n|n.key==60&&n.length>0));
        assert!(pl::clips(&app.state).iter().any(|c|c.track==lane&&c.source==ClipSource::Pattern(pat)));
        // Stop, then record beyond the existing clip: preserve old notes and the
        // timeline gap, extend one clip (including a held note crossing a bar).
        app.toggle_record();app.transport.stop();
        let bar=app.state.project.bar_ticks();
        app.toggle_record();assert!(!app.transport.playing());
        app.transport.pretend_at(f64::from(3*bar),true);
        app.record_down_with(Src::Midi(64),64,100,Extra::default());
        app.transport.pretend_at(f64::from(4*bar+96),true);
        app.record_up_with(Src::Midi(64),None);
        app.end_take();
        let notes=&app.state.project.pattern(pat).unwrap().notes;
        assert!(notes.iter().any(|n|n.key==60&&n.position==0));
        assert!(notes.iter().any(|n|n.key==64&&n.position==3*bar&&n.length==bar+96));
        let clips:Vec<_>=pl::clips(&app.state).iter().filter(|c|c.source==ClipSource::Pattern(pat)).collect();
        assert_eq!(clips.len(),1);assert_eq!(clips[0].position,0);assert_eq!(clips[0].length,5*bar);
        let f=os_flp::project::create(&app.state.project).unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&f.to_bytes().unwrap()).unwrap()).unwrap();
        assert_eq!(back.instrument_lanes,app.state.project.instrument_lanes);
        let mut applied=f.clone();os_flp::project::apply(&back,&mut applied).unwrap();
        assert_eq!(os_flp::project::load(&applied).unwrap().instrument_lanes,back.instrument_lanes);
        app.toggle_record();
        pl::run(&mut app,pl::Cmd::Record(Opt::Automation));
        app.toggle_record();assert!(Opt::Automation.on()&&Opt::Notes.on());
    }

    #[test]
    fn smart_mode_arms_without_a_question_and_writes_automation_clips() {
        let mut app=App::new(None);app.state.project=crate::app::test_project();
        crate::mixer_recording::flag(Opt::Smart,true);
        app.transport.pretend_at(0.,true);
        crate::playlist::run(&mut app,crate::playlist::Cmd::Record(Opt::Automation));
        app.toggle_record();assert!(app.state.rec.armed&&app.state.playlist.popup.is_none());
        assert!(Opt::Notes.on()&&Opt::Automation.on()&&!Opt::Audio.on());
        app.state.rec.smart.origin=Some(0);app.transport.pretend_at(96.,true);
        app.smart_begin_host(&Action::Fader(1));
        crate::mixer::track_mut(&mut app.state,1).unwrap().volume=Some(0.25);
        app.smart_record_host(&Action::Fader(1));
        assert_eq!(app.state.project.automations.len(),1);
        let ch=app.state.project.automations[0].channel;
        let original=crate::playlist_controls::original(&app.state,ch).unwrap();assert!((original-0.8).abs()<1e-9);
        assert_eq!(app.state.project.automations[0].points.last().unwrap().value,0.25);
        app.transport.pretend_at(192.,true);
        crate::mixer::track_mut(&mut app.state,1).unwrap().volume=Some(0.7);
        app.smart_record_host(&Action::Fader(1));
        assert_eq!(app.state.project.automations.len(),1,"same target continues its take");
        app.transport.pretend_at(288.,true);app.smart_record_tick();app.transport.stop();app.smart_record_tick();
        assert!(app.state.rec.smart.active.is_empty());
        assert_eq!(crate::playlist::clips(&app.state).last().unwrap().length,288);
        assert_eq!(app.state.project.automations[0].points.last().unwrap().value,0.7);
        app.state.rec.armed=false;app.transport.pretend_at(384.,true);
        let count=app.state.project.automations.len();app.smart_record_value(os_model::target::TEMPO,"Tempo".into(),0.5);
        assert_eq!(app.state.project.automations.len(),count,"Record off means no automatic capture");
    }
    #[test]
    fn smart_mode_records_midi_notes_without_an_audio_input() {
        let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.song_mode=false;
        crate::mixer_recording::flag(Opt::Smart,true);app.transport.pretend_at(0.,false);app.toggle_record();
        app.transport.pretend_at(0.,false);
        app.record_down_with(crate::step_edit::Src::Midi(60),60,100,crate::record::Extra::default());
        app.transport.pretend_at(96.,false);
        app.record_up_with(crate::step_edit::Src::Midi(60),None);
        assert!(app.state.project.pattern(1).unwrap().notes.iter().any(|n|n.key==60&&n.length>0));
        assert!(app.audio_rec.is_none());
        assert!(app.state.project.automations.is_empty(),"notes do not create unrelated automation");
    }
}
