//! Channel-rack group edits. Relative knob offsets survive common-limit clamping.
use crate::{app::{App,State},knobs::KnobId,history::{self,Snap}};
pub fn rows(s:&State,row:usize)->Vec<usize>{
    let rows:Vec<_>=if row==usize::MAX {(0..s.project.channels.len()).collect()}else if s.rack.sel.contains(&row){s.rack.sel.iter().copied().collect()}else{vec![row]};
    rows.into_iter().filter(|&r|s.project.channels.get(r).is_some_and(|c|c.kind!=os_model::ChannelKind::Automation)).collect()
}
pub fn all_follow(s:&State)->bool {let r=rows(s,usize::MAX);!r.is_empty()&&r.iter().all(|&r|s.project.channels[r].misc.main_pitch)}
pub fn toggle_follow(app:&mut App,row:usize){
    let rows=rows(&app.state,row);if rows.is_empty(){return;}
    let on=if row==usize::MAX {!all_follow(&app.state)}else{!app.state.project.channels[row].misc.main_pitch};
    let snaps=rows.iter().map(|&r|{let c=&app.state.project.channels[r];Snap::PitchFollow(r,c.misc.main_pitch,c.misc.pitch_follow_offset)}).collect();
    history::record(&mut app.state,"Global Pitch Follow",Snap::Several(snaps));
    let global=i32::from(app.state.project.main_pitch.unwrap_or(0));
    for r in rows {let c=&mut app.state.project.channels[r];if c.misc.main_pitch!=on {c.misc.pitch_follow_offset+=if on {-global}else{global};c.misc.main_pitch=on;}
        if c.kind.is_sampler(){app.plugins.set_param_named(r,"Affected by the main pitch",f64::from(on));}
    }
    app.state.piano.dirty=true;app.invalidate();
}
pub fn group(s:&State,id:KnobId)->Vec<(KnobId,f32)>{
    let row=match id {KnobId::ChanVol(r)|KnobId::ChanPan(r)|KnobId::ChanPitch(r)=>r,_=>return Vec::new()};
    rows(s,row).into_iter().map(|r|{let k=match id {KnobId::ChanVol(_)=>KnobId::ChanVol(r),KnobId::ChanPan(_)=>KnobId::ChanPan(r),_=>KnobId::ChanPitch(r)};(k,s.knob(k))}).collect()
}
fn units(s:&State,id:KnobId)->f32{match id{KnobId::ChanPitch(r)=>200.*s.project.channels[r].misc.pitch_range.max(1) as f32,_=>1.}}
pub fn apply(s:&mut State,id:KnobId,base:f32,to:f32,group:&[(KnobId,f32)]){
    if group.is_empty(){s.set_knob(id,to);return;}
    let mut delta=(to-base)*units(s,id);
    for &(k,v) in group {let u=units(s,k);delta=delta.clamp(-v*u,(1.-v)*u);}
    for &(k,v) in group {let next=v+delta/units(s,k);s.set_knob(k,next);}
}
pub fn snapshot(s:&mut State,id:KnobId,wheel:bool){
    let group=group(s,id);
    if group.len()<2 {history::knob(s,id,wheel);return;}
    if !crate::prefs::check(&crate::prefs::UNDO_KNOB_TWEAKS){return;}
    let snaps=group.into_iter().map(|(k,_)|Snap::Knob(k,history::knob_value(s,k))).collect();
    history::record(s,"Selected channel controls",Snap::Several(snaps));
}
pub fn toggle_enabled(s:&mut State,row:usize){
    let Some(c)=s.project.channels.get(row)else{return};let on=!c.enabled;
    let rows=rows(s,row);let snaps=rows.iter().map(|&r|Snap::ChannelEnabled(r,s.project.channels[r].enabled)).collect();
    history::record(s,"Selected channels on/off",Snap::Several(snaps));
    for r in rows {s.project.channels[r].enabled=on;}s.piano.dirty=true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Action,Win};
    #[test]
    fn picture_batch_group_controls_and_octave_wheel(){
        let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.rack.sel=[0,1].into();
        a.state.project.channels[0].misc.pitch_range=24;a.state.project.channels[1].misc.pitch_range=12;
        a.state.project.channels[0].pitch_cents=0;a.state.project.channels[1].pitch_cents=-1200;
        a.state.z=vec![Win::ChannelRack];a.state.show_browser=false;
        let mut c=os_ui::Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);
        let id=KnobId::ChanPitch(0);let r=a.hits.iter().find(|h|h.action==Action::Knob(id)).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.wheel(0.,1.,false,true);
        assert_eq!((a.state.project.channels[0].pitch_cents,a.state.project.channels[1].pitch_cents),(1200,0));
        history::undo(&mut a.state);assert_eq!((a.state.project.channels[0].pitch_cents,a.state.project.channels[1].pitch_cents),(0,-1200));
        history::redo(&mut a.state);assert_eq!(a.state.project.channels[0].pitch_cents,1200);
        for id in [KnobId::ChanVol(0),KnobId::ChanPan(0)] {
            a.state.set_knob(id,0.25);let other=match id{KnobId::ChanVol(_)=>KnobId::ChanVol(1),_=>KnobId::ChanPan(1)};a.state.set_knob(other,0.5);
            let g=group(&a.state,id);snapshot(&mut a.state,id,false);apply(&mut a.state,id,0.25,1.,&g);
            assert!((a.state.knob(id)-0.75).abs()<1e-6);assert_eq!(a.state.knob(other),1.);
            history::undo(&mut a.state);assert_eq!(a.state.knob(id),0.25);assert_eq!(a.state.knob(other),0.5);
        }
        a.state.project.channels[0].enabled=true;toggle_enabled(&mut a.state,0);assert!(!a.state.project.channels[0].enabled&&!a.state.project.channels[1].enabled);
        history::undo(&mut a.state);assert!(a.state.project.channels[0].enabled);
        a.state.project.main_pitch=Some(300);let before=a.state.project.channels[2].misc.main_pitch;
        toggle_follow(&mut a,0);assert!(!a.state.project.channels[0].misc.main_pitch&&!a.state.project.channels[1].misc.main_pitch);assert_eq!(a.state.project.channels[2].misc.main_pitch,before);
        assert_eq!(a.state.project.channels[0].misc.pitch_follow_offset,300);
        toggle_follow(&mut a,usize::MAX);assert!(all_follow(&a.state));assert_eq!(a.state.project.channels[0].misc.pitch_follow_offset,0);
    }
    #[test]
    fn picture_batch_layout_and_preset_drop(){
        let mut a=App::new(None);a.state.project=crate::app::test_project();a.plugins.available=crate::plugins::builtin();a.sync_samplers(None);
        a.state.z=vec![Win::ChannelRack];a.state.show_browser=true;a.state.rack_pos=(340.,120.);
        let mut c=os_ui::Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);
        let r=a.hits.iter().find(|h|h.action==Action::ChannelSelect(0)).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.mouse_down();a.mouse_move(100.,300.);a.mouse_up();
        assert!(a.state.message.as_deref().unwrap_or("").starts_with("Saved preset"),"{:?}",a.state.message);
        let first=a.plugins.presets[&0].clone();let bytes=std::fs::read(&first).unwrap();assert!(crate::presets::decode(&bytes).is_some());
        crate::presets::save_channel_drop(&mut a,0);let second=a.plugins.presets[&0].clone();assert_ne!(first,second);assert_eq!(std::fs::read(first).unwrap(),bytes);
        assert_eq!(a.state.browser_tab,4);
        a.state.project.channels[1].misc.main_pitch=false;
        a.paint(&mut c,None);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(&dir).join("rack-batch.png")).unwrap();}
        crate::piano_tools::run(&mut a,crate::piano_tools::NCmd::KeyboardStyle(true));assert!(crate::piano_tools::classic_keyboard());
        a.state.z=vec![Win::PianoRoll];a.state.show_browser=false;a.state.piano.scroll_y=900.;a.paint(&mut c,None);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join("piano-batch.png")).unwrap();}
        crate::piano_tools::run(&mut a,crate::piano_tools::NCmd::KeyboardStyle(false));assert!(!crate::piano_tools::classic_keyboard());assert_eq!(crate::prefs::text("piano_keyboard_style").as_deref(),Some("piano"));
        a.paint(&mut c,None);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join("piano-shaped.png")).unwrap();}
        crate::piano_tools::run(&mut a,crate::piano_tools::NCmd::KeyboardStyle(true));
    }
}
