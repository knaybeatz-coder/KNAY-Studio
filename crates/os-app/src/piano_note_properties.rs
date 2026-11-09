// SPDX-License-Identifier: GPL-3.0-or-later
//! Transactional note properties, reached by double-clicking an existing note.
use crate::{app::{Action,App,Ui},dialog::{Body,DHit,Field,Modal},piano};
use os_model::Note;
use os_ui::{Color,Font,Rect};
use winit::keyboard::{Key,NamedKey};
use std::time::Instant;
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum Command{Field(usize),Slide,Porta,Apply,Cancel}
const LABELS:[&str;8]=["Velocity (0–128)","Pan (64 = center)","Release (0–128)","Fine pitch (120 = center)","Cutoff (0–255)","Resonance (0–255)","Color group (1–16)","Length (ticks)"];
fn values(n:Note)->[u32;8]{[n.velocity.into(),n.pan.into(),n.release.into(),n.fine_pitch.into(),n.mod_x.into(),n.mod_y.into(),u32::from(n.group&15)+1,n.length]}
pub struct Form{pattern:u16,channel:u16,original:Vec<(usize,Note)>,anchor:Note,fields:[Field;8],focus:usize,slide:bool,porta:bool,message:String}
fn form(app:&mut App)->Option<&mut Form>{match &mut app.state.dialogs.last_mut()?.body{Body::NoteProperties(f)=>Some(f),_=>None}}
/// A newly drawn note does not count as the first click of a properties gesture.
pub fn double_click(app:&mut App,i:usize)->bool{
    let s=&mut app.state;let Some(&n)=piano::notes(s).get(i) else{return false};
    if Some(n.channel)!=piano::channel(s){return false;}
    let now=Instant::now();let at=app.mouse;
    let double=s.piano.properties_click.take().is_some_and(|(p,j,old,t,xy)|p==s.pattern&&j==i&&old==n&&now.duration_since(t).as_millis()<400&&(xy.0-at.0).abs()<=4.&&(xy.1-at.1).abs()<=4.);
    if double{open(app,i);true}else{s.piano.properties_click=Some((s.pattern,i,n,now,at));false}
}
pub fn open(app:&mut App,i:usize){
    let s=&mut app.state;let Some(&anchor)=piano::notes(s).get(i) else{return};
    let selected=s.piano.selection(s.pattern,Some(anchor.channel));
    let ids=if selected.contains(&i){selected}else{vec![i]};
    let original=ids.into_iter().filter_map(|j|piano::notes(s).get(j).copied().filter(|n|n.channel==anchor.channel).map(|n|(j,n))).collect();
    let fields=values(anchor).map(|v|{let mut f=Field::default();f.set(&v.to_string());f});
    s.piano.properties_click=None;
    s.dialogs.push(Modal::new(Body::NoteProperties(Form{pattern:s.pattern,channel:anchor.channel,original,anchor,fields,focus:0,slide:anchor.slide(),porta:anchor.porta(),message:String::new()})));
    app.drag=None;app.hold=None;app.modal_changed();
}
pub fn command(app:&mut App,c:Command){
    if c==Command::Cancel{if form(app).is_some(){app.state.dialogs.pop();app.drag=None;app.modal_changed();}return;}
    let Some(f)=form(app) else{return};
    match c{
        Command::Field(i)=>{if i<8{f.focus=i;f.fields[i].select_all();}},
        Command::Slide=>f.slide=!f.slide,Command::Porta=>f.porta=!f.porta,
        Command::Apply=>{
            let limits=[128,128,128,240,255,255,16,u32::MAX];
            let baseline=values(f.anchor);
            let mut v=[0;8];for j in 0..8{match f.fields[j].text.trim().parse::<u32>(){Ok(n) if n==baseline[j]||(n<=limits[j]&&(j!=6||n>=1))=>v[j]=n,_=>{f.message=format!("Check {}.",LABELS[j]);f.focus=j;app.invalidate();return;}}}
            if v[7]!=baseline[7]&&f.original.iter().any(|(_,n)|n.position.checked_add(v[7]).is_none()){f.message="Length exceeds the project timeline.".into();app.invalidate();return;}
            let Some(Modal{body:Body::NoteProperties(f),..})=app.state.dialogs.pop() else{return};
            let s=&mut app.state;
            if s.pattern!=f.pattern||piano::channel(s)!=Some(f.channel)||!f.original.iter().all(|(i,n)|piano::notes(s).get(*i)==Some(n)){
                s.message=Some("Notes changed. Reopen Note properties.".into());app.modal_changed();return;
            }
            let base=values(f.anchor);let changed=v!=base||f.slide!=f.anchor.slide()||f.porta!=f.anchor.porta();
            if changed{
                piano::snapshot_as(s,"Edit note properties");
                for (i,old) in f.original{
                    let n=&mut piano::pattern_mut(s).notes[i];
                    if v[0]!=base[0]{n.velocity=v[0] as u8;}if v[1]!=base[1]{n.pan=v[1] as u8;}
                    if v[2]!=base[2]{n.release=v[2] as u8;}if v[3]!=base[3]{n.fine_pitch=v[3] as u8;}
                    if v[4]!=base[4]{n.mod_x=v[4] as u8;}if v[5]!=base[5]{n.mod_y=v[5] as u8;}
                    if v[6]!=base[6]{n.group=(n.group&0xf0)|((v[6]-1) as u8);}if v[7]!=base[7]{n.length=v[7];}
                    if f.slide!=f.anchor.slide(){n.set_slide(f.slide);}if f.porta!=f.anchor.porta(){n.set_porta(f.porta);}
                    if old==f.anchor{let updated=*n;s.piano.last_controls=Some(updated);if updated.length>0{s.piano.last_len=updated.length;}}
                }
                s.piano.dirty=true;piano::finish_edit(s);
            }
            app.modal_changed();return;
        },Command::Cancel=>{}
    }
    app.invalidate();
}
pub fn key(app:&mut App,key:&Key,ctrl:bool,alt:bool){
    match key{
        Key::Named(NamedKey::Escape)=>command(app,Command::Cancel),
        Key::Named(NamedKey::Enter)=>command(app,Command::Apply),
        Key::Named(NamedKey::Tab)=>{let backwards=app.shift;if let Some(f)=form(app){f.focus=(f.focus+if backwards{7}else{1})%8;f.fields[f.focus].select_all();}},
        _=>{if let Some(f)=form(app){let input=&mut f.fields[f.focus];if input.len()<12||!matches!(key,Key::Character(_)){input.key(key,ctrl,alt);}}}
    }app.invalidate();
}
pub fn paint(ui:&mut Ui,r:Rect,f:&Form){
    ui.c.text(Font::new(13),r.x+18.,r.y+42.,ui.t.text,&format!("{} — {} note(s)",crate::views::piano::key_name(i32::from(f.anchor.key)),f.original.len()));
    ui.c.text(Font::new(11),r.x+18.,r.y+65.,ui.t.text_dim,"Only changed properties apply to the selected notes.");
    for (i,label) in LABELS.iter().enumerate(){
        let x=r.x+18.+(i%2) as f32*216.;let y=r.y+91.+(i/2) as f32*51.;
        ui.c.text(Font::new(11),x,y,ui.t.text_dim,label);
        let rect=Rect::new(x,y+16.,208.,28.);let on=f.focus==i;
        ui.c.fill_rrect(rect,4.,ui.t.sunken);ui.c.stroke_rrect(rect,4.,1.,if on{ui.t.accent}else{ui.t.button});
        if on&&f.fields[i].all{ui.c.fill_rrect(rect.inset(4.),2.,ui.t.button_hi);}
        ui.c.text(Font::new(13),x+8.,rect.y+6.,ui.t.text,&f.fields[i].text);
        if on{let caret=x+8.+ui.c.text_width(Font::new(13),f.fields[i].before_caret());ui.c.line(caret,rect.y+5.,caret,rect.bottom()-5.,1.,ui.t.accent);}
        ui.hit(rect,Action::Dlg(DHit::NoteProperties(Command::Field(i))),*label);
    }
    for (i,(label,on,c)) in [("Slide / glide",f.slide,Command::Slide),("Portamento",f.porta,Command::Porta)].into_iter().enumerate(){
        let rect=Rect::new(r.x+18.+i as f32*216.,r.y+304.,208.,30.);
        ui.c.fill_rrect(rect,4.,if on{ui.t.accent}else{ui.t.button});
        ui.c.text_center(Font::new(13),rect,if on{Color::rgb(0x171914)}else{ui.t.text},&format!("{}  {}",if on{"✓"}else{"○"},label));
        ui.hit(rect,Action::Dlg(DHit::NoteProperties(c)),label);
    }
    ui.c.text_clip(Font::new(11),r.x+18.,r.y+345.,r.w-36.,ui.t.text_dim,if f.message.is_empty(){"Slide length sets the glide time. Enter applies; Esc cancels."}else{&f.message});
    for (i,(label,c)) in [("Cancel",Command::Cancel),("Apply",Command::Apply)].into_iter().enumerate(){
        let rect=Rect::new(r.x+238.+i as f32*106.,r.bottom()-46.,98.,30.);let on=c==Command::Apply;
        ui.c.fill_rrect(rect,4.,if on{ui.t.accent}else{ui.t.button});ui.c.text_center(Font::new(13),rect,if on{Color::rgb(0x171914)}else{ui.t.text},label);
        ui.hit(rect,Action::Dlg(DHit::NoteProperties(c)),label);
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::{app::Win,playlist as pl};
    fn app()->App{let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.selected_channel=0;a.state.z=vec![Win::PianoRoll];a}
    fn click_note(a:&mut App,c:&mut os_ui::Canvas,i:usize){a.paint(c,None);let h=a.hits.iter().find(|h|h.action==Action::PianoNote(i)).unwrap();let r=h.rect;a.mouse_move(r.x+5.,r.cy());a.mouse_down();a.mouse_up();}
    #[test]
    fn note_workflow_double_click_properties_selected_apply_cancel_and_undo(){
        let mut a=app();let n=Note{length:96,key:60,..Note::step(0,0)};let mut other=n;other.position=192;other.velocity=33;other.key=64;
        piano::pattern_mut(&mut a.state).notes=vec![n,other,Note{channel:1,position:384,..n}];a.state.piano.select(a.state.pattern,0,vec![0,1]);
        let before=piano::notes(&a.state).to_vec();let mut canvas=os_ui::Canvas::new(1280,800,1.).unwrap();
        click_note(&mut a,&mut canvas,0);assert!(a.state.dialogs.is_empty());
        click_note(&mut a,&mut canvas,0);assert!(a.modal_open());assert!(a.drag.is_none()&&a.hold.is_none());
        assert_eq!(form(&mut a).unwrap().original.len(),2);
        a.paint(&mut canvas,None);let rect=a.hits.iter().find(|h|h.action==Action::Dlg(DHit::NoteProperties(Command::Slide))).unwrap().rect;
        a.mouse_move(rect.cx(),rect.cy());a.mouse_down();a.mouse_up();
        command(&mut a,Command::Field(6));key(&mut a,&Key::Character("6".into()),false,false);
        a.paint(&mut canvas,None);if let Some(out)=std::env::var_os("LEMON_NOTE_PROPERTIES_PREVIEW"){canvas.pixmap.save_png(out).unwrap();}
        assert!(form(&mut a).unwrap().slide);command(&mut a,Command::Apply);assert!(!a.modal_open());
        for i in 0..2{let got=piano::notes(&a.state)[i];assert!(got.slide(),"note {i}: {got:?}; message {:?}",a.state.message);assert_eq!(got.group&15,5);assert_eq!(got.position,before[i].position);assert_eq!(got.velocity,before[i].velocity);}
        assert_eq!(piano::notes(&a.state)[2],before[2]);assert_eq!(a.state.piano.last_controls.unwrap().group&15,5);
        crate::history::undo(&mut a.state);assert_eq!(piano::notes(&a.state),before);
        open(&mut a,0);command(&mut a,Command::Porta);key(&mut a,&Key::Named(NamedKey::Escape),false,false);assert_eq!(piano::notes(&a.state),before);
    }
    #[test]
    fn note_workflow_new_note_and_drag_do_not_open_properties(){
        let mut a=app();let mut canvas=os_ui::Canvas::new(1280,800,1.).unwrap();a.paint(&mut canvas,None);
        let g=piano::Geom::new(&a.state,a.hits.iter().find(|h|h.action==Action::PianoGrid).unwrap().rect);
        a.mouse_move(g.x_of(96.)+4.,g.y_of(60)+4.);a.mouse_down();a.mouse_up();assert_eq!(piano::notes(&a.state).len(),1);assert!(!a.modal_open());
        click_note(&mut a,&mut canvas,0);assert!(!a.modal_open(),"the creation click must not open properties on the next press");
        // A changed note from a drag is no longer the first press of a double-click.
        piano::pattern_mut(&mut a.state).notes[0].position+=96;click_note(&mut a,&mut canvas,0);assert!(!a.modal_open());
        open(&mut a,0);form(&mut a).unwrap().fields[6].set("17");command(&mut a,Command::Apply);assert!(a.modal_open());assert!(!form(&mut a).unwrap().message.is_empty());command(&mut a,Command::Cancel);
    }
    #[test]
    fn note_workflow_clicking_existing_notes_switches_placement_type() {
        use crate::piano_tools::{self, NCmd};

        let mut app = app();
        let normal = Note { length: 48, key: 60, ..Note::step(0, 0) };
        let mut glide = Note { position: 96, ..normal };
        glide.set_slide(true);
        let mut porta = Note { position: 192, ..normal };
        porta.set_porta(true);
        piano::pattern_mut(&mut app.state).notes = vec![normal, glide, porta];
        let mut canvas = os_ui::Canvas::new(1280, 800, 1.).unwrap();
        let place = |app: &mut App, canvas: &mut os_ui::Canvas, tick: f32| {
            app.paint(canvas, None);
            let g = app.piano_geom().unwrap();
            app.mouse_move(g.x_of(tick) + 3., g.y_of(60) + 4.);
            app.mouse_down();
            app.mouse_up();
            *piano::notes(&app.state).last().unwrap()
        };

        piano_tools::run(&mut app, NCmd::DrawSlide);
        assert!(place(&mut app, &mut canvas, 288.).slide());

        click_note(&mut app, &mut canvas, 0);
        let drawn = place(&mut app, &mut canvas, 384.);
        assert!(!drawn.slide() && !drawn.porta());

        click_note(&mut app, &mut canvas, 1);
        let drawn = place(&mut app, &mut canvas, 480.);
        assert!(drawn.slide() && !drawn.porta());

        app.ctrl = true;
        click_note(&mut app, &mut canvas, 2);
        app.ctrl = false;
        let drawn = place(&mut app, &mut canvas, 576.);
        assert!(drawn.porta() && !drawn.slide());

        assert_eq!(&piano::notes(&app.state)[..3], &[normal, glide, porta]);
    }

    #[test]
    fn note_workflow_playlist_held_ctrl_b_repeats_and_respects_focus_and_undo(){
        let mut a=app();a.state.z=vec![Win::Playlist];let bar=pl::bar(&a.state);
        pl::arr_mut(&mut a.state).clips=vec![os_model::Clip::new(0,bar,1,os_model::ClipSource::Pattern(1))];a.state.playlist.sel=vec![0];
        for i in 0..4{a.repeat=i>0;a.key(&Key::Character("b".into()),true,false);}
        a.repeat=false;assert_eq!(pl::clips(&a.state).len(),5);assert_eq!(pl::clips(&a.state).iter().map(|c|c.position).collect::<Vec<_>>(),vec![0,bar,bar*2,bar*3,bar*4]);assert_eq!(a.state.playlist.sel,vec![4]);
        crate::history::undo(&mut a.state);assert_eq!(pl::clips(&a.state).len(),4);
        a.state.z.push(Win::ChannelRack);a.repeat=true;a.key(&Key::Character("b".into()),true,false);assert_eq!(pl::clips(&a.state).len(),4);
        a.state.z=vec![Win::Playlist];piano::pattern_mut(&mut a.state).notes.push(Note::step(0,0));open(&mut a,0);assert!(a.modal_open());a.key(&Key::Character("b".into()),true,false);assert_eq!(pl::clips(&a.state).len(),4);command(&mut a,Command::Cancel);
        a.state.playlist.edit=Some(pl::TextEdit{target:pl::EditTarget::TrackName(0),text:"Vocal".into(),at:(0.,0.),sel:false});
        a.key(&Key::Character("b".into()),true,false);assert_eq!(pl::clips(&a.state).len(),4);a.state.playlist.edit=None;
        a.repeat=false;a.state.playlist.sel=vec![0];a.state.playlist.range=Some((0,bar*2));
        for i in 0..2{a.repeat=i>0;a.key(&Key::Character("b".into()),true,false);}
        assert_eq!(pl::clips(&a.state).last().unwrap().position,bar*4);assert_eq!(a.state.playlist.range,Some((bar*4,bar*6)));
    }
}
