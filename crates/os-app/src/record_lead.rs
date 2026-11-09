// SPDX-License-Identifier: GPL-3.0-or-later
//! Recording pre-roll plays existing material, gating takes until the original start point.
use crate::{app::{App,State},record::{Opt,RecCmd},playlist::{Cmd,PItem,Mark}};
pub fn mode()->bool{crate::settings_file::flag("rec_lead_preroll",false)}
pub fn bars()->u32{crate::settings_file::get("rec_preroll_bars").and_then(|v|v.parse::<u32>().ok()).filter(|n|[1,2,4,8].contains(n)).unwrap_or(2)}
pub fn set_bars(n:u32){crate::settings_file::write("rec_preroll_bars",&if [1,2,4,8].contains(&n){n}else{2}.to_string());}
pub fn set_mode(preroll:bool,on:bool){crate::settings_file::set_flag("rec_lead_preroll",preroll);crate::settings_file::set_flag("rec_precount",on);}
pub fn menu()->Vec<PItem>{
    let enabled=Opt::Precount.on();let roll=mode();let count=crate::record::precount_bars();let length=bars();
    let mut items=vec![PItem::head("Recording lead-in"),PItem::new("Off",Cmd::Rec(RecCmd::LeadOff)).mark(Mark::Radio(!enabled)),PItem::new("Count-in (clicks only)",Cmd::Rec(RecCmd::LeadMode(false))).mark(Mark::Radio(enabled&&!roll)),PItem::new("Pre-roll (play before recording)",Cmd::Rec(RecCmd::LeadMode(true))).mark(Mark::Radio(enabled&&roll)),PItem::sep(),PItem::head("Precount duration")];
    for n in [1,2]{items.push(PItem::new(format!("{n} bar{}",if n==1{""}else{"s"}),Cmd::Rec(RecCmd::Precount(n))).mark(Mark::Radio(!roll&&enabled&&count==n)));}
    items.extend([PItem::sep(),PItem::head("Pre-roll duration")]);
    for n in [1,2,4,8]{items.push(PItem::new(format!("{n} bar{}",if n==1{""}else{"s"}),Cmd::Rec(RecCmd::PrerollBars(n))).mark(Mark::Radio(roll&&enabled&&length==n)));}
    items
}
/// Subtract bar durations across time-signature regions, preserving a partial bar at the cursor.
pub fn before(s:&State,target:u32,bars:u32)->u32{
    let regions=if s.song_mode{s.project.marker_bars(crate::playlist::markers(s))}else{s.project.pattern_bars(s.pattern)};
    let mut at=f64::from(target);let mut left=f64::from(bars);
    for &(start,bar,_) in regions.iter().rev(){
        if at<=f64::from(start){continue;}
        let available=(at-f64::from(start))/f64::from(bar.max(1));let take=left.min(available);
        at-=take*f64::from(bar.max(1));left-=take;if left<1e-9{break;}
    }at.max(0.).round() as u32
}
pub fn begin(s:&mut State,target:f64)->f64{
    s.rec.preroll=None;
    if !s.rec.armed||!Opt::Precount.on()||!mode(){return target;}
    let target=target.max(0.).round() as u32;let start=before(s,target,bars());
    if start<target{s.rec.preroll=Some(target);}f64::from(start)
}
impl App{
    pub(crate) fn pre_roll_active(&self)->bool{self.state.rec.preroll.is_some_and(|target|self.transport.tick()<f64::from(target))}
}

#[cfg(test)]
mod tests{
    use super::*;use crate::{app::{Action,Win},playlist as pl};
    fn app()->App{let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.selected_channel=0;a.state.project.patterns.iter_mut().for_each(|p|p.notes.clear());a.state.project.automations.clear();pl::arr_mut(&mut a.state).clips.clear();a}
    #[test]
    fn lead_fix_modes_timing_note_automation_gates_and_stop(){
        let mut a=app();a.state.project.ppq=96;a.state.rec.armed=true;a.state.piano.marker=1536;
        set_bars(2);set_mode(true,true);a.play_counted_if_armed();assert_eq!(a.transport.tick(),768.);assert_eq!(a.state.rec.preroll,Some(1536));assert!(a.state.rec.count.is_none());
        crate::settings_file::set_flag("rec_notes",true);crate::settings_file::set_flag("rec_automation",true);
        use crate::step_edit::Src;
        let src=Src::Midi(60);
        a.record_down_with(src,60,100,crate::record::Extra::default());a.record_up_with(src,None);a.record_knob(crate::knobs::KnobId::ChanVol(0));
        assert!(crate::piano::notes(&a.state).is_empty());assert!(a.state.project.pattern(1).unwrap().controls.is_empty());
        a.transport.pretend_at(1536.,false);a.record_tick();assert!(a.state.rec.preroll.is_none());
        a.record_down_with(src,60,100,crate::record::Extra::default());a.transport.pretend_at(1584.,false);a.record_up_with(src,None);a.record_knob(crate::knobs::KnobId::ChanVol(0));
        assert_eq!(crate::piano::notes(&a.state).len(),1);assert_eq!(crate::piano::notes(&a.state)[0].position,1536);assert!(!a.state.project.pattern(1).unwrap().controls.is_empty());
        a.act(Action::Stop);a.record_tick();assert!(a.state.rec.preroll.is_none());
        a.state.rec.armed=false;assert_eq!(begin(&mut a.state,1536.),1536.);assert!(a.state.rec.preroll.is_none());
        set_mode(false,true);a.state.rec.armed=true;a.play_counted_if_armed();assert!(a.state.rec.count.is_some());assert!(a.state.rec.preroll.is_none());a.act(Action::Stop);
        assert!(menu().iter().any(|p|p.cmd==Cmd::Rec(RecCmd::PrerollBars(8))));
        set_mode(true,false);assert!(mode());assert!(!Opt::Precount.on());
    }
    #[test]
    fn lead_fix_clamp_and_time_signatures_and_selection_start(){
        let mut a=app();a.state.project.ppq=96;a.state.project.bar=Some((3,4));assert_eq!(before(&a.state,900,2),324);assert_eq!(before(&a.state,100,8),0);
        a.state.project.bar=Some((4,4));a.state.project.patterns[0].markers.push(os_model::TimeMarker{color:None,position:768,kind:os_model::TimeMarker::SIGNATURE,name:String::new(),numerator:3,denominator:4,scale:0});
        assert_eq!(before(&a.state,1056,2),384);
        a.state.song_mode=true;a.state.playlist.range=Some((1536,2304));a.state.playlist.pos=0;a.state.rec.armed=true;set_bars(1);set_mode(true,true);a.play_counted_if_armed();assert_eq!(a.state.rec.preroll,Some(1536));assert_eq!(a.transport.tick(),1152.);assert_eq!(a.state.rec.smart.origin,Some(1536));a.act(Action::Stop);
    }
    #[test]
    fn lead_fix_empty_automation_menu_and_playlist_corner_zoom(){
        let mut a=app();a.state.project.instrument_lanes.clear();a.state.playlist.sel=vec![123];pl::automation_menu(&mut a.state,8,(100.,100.));assert!(a.state.playlist.popup.is_none());pl::run(&mut a,Cmd::TrackAutomationToggle(8));assert_eq!(a.state.playlist.sel,vec![123]);
        a.state.z=vec![Win::Playlist];let mut c=os_ui::Canvas::new(1280,800,1.).unwrap();
        for small in [false,true]{crate::prefs::set_check(&crate::prefs::SMALL_SCROLLBARS,small);a.paint(&mut c,None);let r=a.hits.iter().find(|h|h.action==Action::PlTrackZoom).unwrap().rect;assert!(r.right()<=a.state.win_rect(Win::Playlist).right());let initial=a.state.playlist.row_h;a.mouse_move(r.cx(),r.cy());assert_eq!(a.cursor(),crate::app::Cursor::NsResize);a.mouse_down();a.mouse_move(r.cx(),r.cy()-80.);a.mouse_up();assert!(a.state.playlist.row_h>initial);a.paint(&mut c,None);a.mouse_move(r.cx(),r.cy());a.alt=true;a.mouse_down();a.mouse_up();a.alt=false;assert_eq!(a.state.playlist.row_h,pl::ROW_H);}
        if let Some(out)=std::env::var_os("LEMON_LEAD_PLAYLIST_PREVIEW"){a.paint(&mut c,None);c.pixmap.save_png(out).unwrap();}
        let _=pl::create_automation(&mut a.state,0,"Volume",0.5);pl::automation_menu(&mut a.state,8,(100.,100.));assert!(a.state.playlist.popup.as_ref().is_some_and(|p|!p.items.is_empty()));
    }
}
