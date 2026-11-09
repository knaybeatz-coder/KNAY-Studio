// SPDX-License-Identifier: GPL-3.0-or-later
//! Piano Roll drawing color, slide and portamento controls.
use crate::{app::{State,Ui,Action},piano,playlist::{PItem,Popup,Cmd,Mark},piano_tools::NCmd};
use os_ui::{Color,Rect,Font};
use os_model::Note;
const COLORS:[u32;16]=[0x9fcf8d,0xdca4b6,0xc5a7dd,0xaba8df,0x95b3dd,0x8bc8d5,0x8bd1bb,0xbed08d,0xd4cb88,0xd8b080,0xd99b88,0xd99b9d,0xc5aaba,0xb2b5c6,0x9bbeb7,0xc8c5a0];
pub fn current(s:&State)->u8{s.piano.last_controls.map_or(0,|n|n.group&15)}
pub fn custom_color(t: &os_theme::Palette, group: u8) -> Option<Color> {
    let group = usize::from(group & 15);
    (t.appearance.note_groups & (1 << group) != 0)
        .then(|| t.get(os_theme::appearance::NOTE_GROUPS[group]).expect("note group color"))
}
pub fn color(t: &os_theme::Palette, group: u8) -> Color {
    custom_color(t, group).unwrap_or_else(|| {
        if group & 15 == 0 { t.note_top.mix(t.note_bottom, 0.6) }
        else { Color::rgb(COLORS[usize::from(group & 15)]) }
    })
}
pub fn menu(s:&State,x:f32,y:f32)->Popup{
    let mut items=vec![PItem::head("Note color group")];
    for n in 0..16 {items.push(PItem::new(format!("●  Color {:02}",n+1),Cmd::Piano(NCmd::NoteColor(n))).mark(Mark::Radio(current(s)==n)));}
    let selected=!s.piano.selection(s.pattern,piano::channel(s)).is_empty();
    let mut edits=vec![PItem::new("Apply current color",Cmd::Piano(NCmd::ApplyNoteColor)),PItem::new("Normal notes",Cmd::Piano(NCmd::SelectedNoteType(0))),PItem::new("Slide / glide notes",Cmd::Piano(NCmd::SelectedNoteType(1))),PItem::new("Portamento notes",Cmd::Piano(NCmd::SelectedNoteType(2)))];
    for item in &mut edits{item.enabled=selected;}
    items.extend([PItem::sep(),PItem::sub("Selected notes",edits)]);
    Popup{x,y,items,sub:None,scroll:[0;2],cascade:Vec::new()}
}
pub fn choose_color(s:&mut State,n:u8){let mut t=s.piano.last_controls.unwrap_or_else(||Note::step(0,0));t.group=(t.group&0xf0)|(n&15);s.piano.last_controls=Some(t);}
pub fn apply_selection(s:&mut State,ty:Option<u8>){
    let ids=s.piano.selection(s.pattern,piano::channel(s)).to_vec();if ids.is_empty(){return;}
    let c=current(s);piano::snapshot(s);
    for i in ids{if let Some(n)=piano::pattern_mut(s).notes.get_mut(i){
        if let Some(ty)=ty{crate::hold::NoteType::after(u32::from(ty)).apply(n);}else{n.group=(n.group&0xf0)|c;}
    }}s.piano.dirty=true;
}
pub fn paint(ui:&mut Ui,s:&State,tab:Rect){
    let t=ui.t;ui.c.fill_rect(tab,t.window_title);
    let c=Rect::new(tab.x+3.,tab.y+3.,41.,19.);
    let group=color(t,current(s));ui.c.fill_rrect(c,3.,group);ui.c.stroke_rrect(c,3.,1.,group.shade(-0.25));
    ui.c.text_center(Font::new(11),Rect::new(c.x,c.y,29.,c.h),Color::rgb(0x15221a),&format!("{:02}",current(s)+1));
    os_ui::kit::triangle(ui.c,c.right()-7.,c.cy(),3.,os_ui::kit::Dir::Down,Color::rgb(0x15221a));
    ui.hit(c,Action::PianoCommand(NCmd::NoteColorMenu),"Note color: choose a color group; apply color/type to selected notes");
    let scale=Rect::new(tab.x+3.,tab.y+25.,41.,13.);ui.c.fill_rrect(scale,2.,if s.piano.harmony.scale_only{t.accent}else{t.sunken});
    ui.c.text_center(Font::new(9),scale,if s.piano.harmony.scale_only{t.sunken}else{t.text_dim},"Scale");
    ui.hit(scale,Action::Harmony(crate::harmony::HCmd::ScaleOnly),"Scale Notes Only: hide out-of-scale keys");
    for (slide,on,y,cmd,hint) in [(false,s.piano.draw_porta,tab.y+3.,NCmd::DrawPorta,"Portamento notes: toggle glide into newly drawn notes"),(true,s.piano.draw_slide,tab.y+22.,NCmd::DrawSlide,"Slide / glide notes: the note length sets the slide duration")] {
        let r=Rect::new(tab.x+47.,y,21.,16.);let hover=ui.hit(r,Action::PianoCommand(cmd),hint);
        ui.c.fill_rrect(r,3.,if on{t.accent}else if hover{t.button_hi}else{t.sunken});
        let ink=if on{t.sunken}else{t.text};
        if slide{ui.c.fill_polygon(&[(r.x+5.,r.bottom()-4.),(r.right()-5.,r.y+4.),(r.right()-5.,r.bottom()-4.)],ink);}
        else{ui.c.polyline(&[(r.x+4.,r.bottom()-5.),(r.x+8.,r.bottom()-5.),(r.x+13.,r.y+4.),(r.right()-3.,r.y+4.)],1.5,ink);}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::{App,Win},piano_tools};
    fn app()->App{let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.selected_channel=0;a}
    #[test]
    fn note_modes_draw_color_slide_portamento_and_round_trip(){
        let mut a=app();
        piano_tools::run(&mut a,NCmd::NoteColor(5));
        piano_tools::run(&mut a,NCmd::DrawSlide);
        let i=piano::add_note(&mut a.state,0,0,60);
        assert_eq!(piano::notes(&a.state)[i].group&15,5);assert!(piano::notes(&a.state)[i].slide());
        piano_tools::run(&mut a,NCmd::DrawSlide);piano_tools::run(&mut a,NCmd::DrawPorta);
        let i=piano::add_note(&mut a.state,0,96,67);
        assert!(piano::notes(&a.state)[i].porta());assert!(!piano::notes(&a.state)[i].slide());
        let f=os_flp::project::create(&a.state.project).unwrap();
        let bytes=f.to_bytes().unwrap();let f=os_flp::FlpFile::parse(&bytes).unwrap();
        let p=os_flp::project::load(&f).unwrap();
        assert_eq!(p.patterns[0].notes,a.state.project.patterns[0].notes);
        a.state.piano.reset();assert!(!a.state.piano.draw_slide&&!a.state.piano.draw_porta);
    }
    #[test]
    fn note_modes_selected_edits_preserve_flags_and_undo(){
        let mut a=app();let mut n=Note::step(0,0);n.group=Note::MUTED|Note::PORTA|3;
        piano::pattern_mut(&mut a.state).notes=vec![n,Note::step(0,96),Note::step(1,0)];
        a.state.piano.select(a.state.pattern,0,vec![0]);choose_color(&mut a.state,9);
        apply_selection(&mut a.state,None);
        assert_eq!(piano::notes(&a.state)[0].group,Note::MUTED|Note::PORTA|9);
        assert_eq!(piano::notes(&a.state)[1..],[Note::step(0,96),Note::step(1,0)]);
        crate::history::undo(&mut a.state);assert_eq!(piano::notes(&a.state)[0],n);
        a.state.piano.select(a.state.pattern,0,vec![0]);apply_selection(&mut a.state,Some(1));assert!(piano::notes(&a.state)[0].slide());assert!(!piano::notes(&a.state)[0].porta());
        crate::history::undo(&mut a.state);assert_eq!(piano::notes(&a.state)[0],n);
    }
    #[test]
    fn note_modes_controls_remain_available_with_scale_only(){
        let mut a=app();a.state.z=vec![Win::PianoRoll];a.state.piano.harmony.scale_only=true;
        for n in 0..8 {choose_color(&mut a.state,n);piano::add_note(&mut a.state,0,u32::from(n)*48,60+i32::from(n));}
        let mut c=os_ui::Canvas::new(1280,800,1.).unwrap();a.paint(&mut c,None);
        assert!(a.hits.iter().any(|h|matches!(h.action,Action::PianoCommand(NCmd::NoteColorMenu))));
        assert!(a.hits.iter().any(|h|matches!(h.action,Action::PianoCommand(NCmd::DrawSlide))));
        let m=menu(&a.state,100.,100.);assert_eq!(m.items.iter().filter(|i|matches!(i.cmd,Cmd::Piano(NCmd::NoteColor(_)))&&i.enabled).count(),16);
        if let Some(out)=std::env::var_os("LEMON_PIANO_MODES_PREVIEW"){c.pixmap.save_png(out).unwrap();}
    }
}
