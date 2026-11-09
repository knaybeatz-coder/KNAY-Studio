// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Command,Tool,doc,STEP};
use crate::{app::{Ui,State,Action},playlist::{self as pl,PGeom}};
use os_model::ClipSource;
use os_ui::{Rect,Color,Font,tabler::TablerIcon};
const VOCAL_MIC:TablerIcon=TablerIcon{filled:true,d:"M 6 6 C 6 2.7 8.7 0 12 0 C 15.3 0 18 2.7 18 6 L 18 12 C 18 15.3 15.3 18 12 18 C 8.7 18 6 15.3 6 12 Z M 8.8 5 L 8.8 9.3 Q 8.8 12.3 10.8 12.9 L 10.8 15 Q 10.8 16.2 12 16.2 Q 13.2 16.2 13.2 15 L 13.2 12.9 Q 15.2 12.3 15.2 9.3 L 15.2 5 Q 15.2 3.8 14 3.8 Q 12.8 3.8 12.8 5 L 12.8 9.3 Q 12.8 10.6 12 10.6 Q 11.2 10.6 11.2 9.3 L 11.2 5 Q 11.2 3.8 10 3.8 Q 8.8 3.8 8.8 5 Z M 2 12 Q 2 10.8 3.2 10.8 Q 4.4 10.8 4.4 12 C 4.4 16.2 7.8 19.6 12 19.6 C 16.2 19.6 19.6 16.2 19.6 12 Q 19.6 10.8 20.8 10.8 Q 22 10.8 22 12 C 22 17.1 18.2 21.3 13.2 21.9 L 13.2 23 Q 13.2 24 12 24 Q 10.8 24 10.8 23 L 10.8 21.9 C 5.8 21.3 2 17.1 2 12 Z"};
fn font(vintage: bool, size: u16) -> Font {
    if vintage { Font::vintage(size) } else { Font::new(size) }
}

fn control_face(ui: &mut Ui, r: Rect, on: bool, hot: bool) {
    if ui.t.style.vintage_2005 {
        let face = Color::rgb(if hot { 0x74838a } else { 0x57636a });
        if on { crate::vintage::recess(ui.c, r, Color::rgb(0x37494f)); }
        else { crate::vintage::metal(ui.c, r, face); }
    } else {
        ui.c.fill_rrect(r, 4.0, if on { ui.t.glyph.with_alpha(42) } else if hot { ui.t.button_hi } else { ui.t.s(0x2a3339) });
    }
}

pub fn head(ui:&mut Ui,s:&State,row:u16,r:Rect){
 let open=super::expanded(s,row);let tuned=pl::clips(s).iter().any(|c|c.track==row&&matches!(c.source,ClipSource::Channel(id) if s.project.vocal_edits.iter().any(|d|d.channel==id&&d.changed())));
 let hover=ui.hit(r,Action::Vocal(Command::Toggle(row)),if open{"Close vocal editor; keep tuning active"}else{"Vocal tuning: expand this track to edit notes and pitch"});
 if ui.t.style.vintage_2005 { control_face(ui, r, open, hover); }
 else if open||hover{ui.c.fill_rrect(r,3.,ui.t.glyph.with_alpha(if open{58}else{24}));}
 let glyph = if ui.t.style.vintage_2005 { r.inset(2.0) } else { r };
 crate::icons::draw_with_default(ui.c,crate::icons::Icon::Mic,&VOCAL_MIC,glyph,if open||tuned{ui.t.glyph}else{ui.t.text_faint});
 if tuned{ui.c.fill_circle(r.right()-2.,r.bottom()-2.,1.6,ui.t.glyph);}
}

pub fn packed_art(tool: Tool, family: u8) -> &'static crate::icons::Art {
    use crate::icons::{self, Gesture, Icon};
    let icon = match tool {
        Tool::Main => return icons::gesture(Gesture::Pointer, family),
        Tool::Hand => return icons::gesture(Gesture::Grab, family),
        Tool::Select => Icon::Select, Tool::Modulation => Icon::Wave, Tool::Drift => Icon::Slide,
        Tool::Transition => Icon::Link, Tool::Formant => Icon::MidiControl, Tool::Amplitude => Icon::Speaker,
        Tool::Time => Icon::Slip, Tool::Attack => Icon::Countdown, Tool::Split => Icon::Slice,
        Tool::Draw => Icon::Pencil, Tool::Zoom => Icon::Zoom,
    };
    icons::artwork(icon, family)
}
pub const TOOLS:[Tool;13]=[Tool::Main,Tool::Select,Tool::Modulation,Tool::Drift,Tool::Transition,Tool::Formant,Tool::Amplitude,Tool::Time,Tool::Attack,Tool::Split,Tool::Draw,Tool::Hand,Tool::Zoom];
pub fn label(t:Tool)->&'static str{match t{Tool::Main=>"Main: drag vertically for pitch, horizontally for time; drag note edge for duration (1)",Tool::Select=>"Pitch: drag to transpose, Alt for cents; double-click to tune (2)",Tool::Modulation=>"Pitch modulation: drag vibrato amount; double-click original/flat (3)",Tool::Drift=>"Pitch drift: drag slow intonation movement; double-click original/flat (4)",Tool::Transition=>"Pitch transition: drag to smooth the connection to the next adjacent note (T)",Tool::Formant=>"Formant: drag vocal colour per note; double-click reset (5)",Tool::Amplitude=>"Amplitude: drag note level; double-click mute (6)",Tool::Time=>"Timing: drag note position or its right edge for duration (7)",Tool::Attack=>"Time handle: drag the note's internal midpoint, keeping its edges (8)",Tool::Split=>"Separation: click to split, drag a boundary to adjust; Alt-click joins next (9)",Tool::Draw=>"Draw a pitch curve (D)",Tool::Hand=>"Hand: drag to scroll time and pitch (H)",Tool::Zoom=>"Zoom: drag right/left for time, up/down for pitch (Z)"}}
pub fn icon(t:Tool)->TablerIcon{if t==Tool::Zoom{return os_ui::tabler::ZOOM_IN;}TablerIcon{filled:false,d:match t{
 Tool::Main=>"M 5 3 L 5 20 L 10 15 L 14 22 L 17 20 L 13 13 L 20 12 Z",
 Tool::Select=>"M 3 12 L 15 12 M 9 7 L 9 17 M 18 3 L 18 21 M 15 6 L 18 3 L 21 6 M 15 18 L 18 21 L 21 18",
 Tool::Modulation=>"M 2 13 Q 5 2 8 13 Q 11 24 14 13 Q 17 2 20 13 L 22 13",
 Tool::Drift=>"M 3 17 C 8 17 8 7 13 7 C 17 7 17 12 21 12",
 Tool::Transition=>"M 2 17 L 7 17 Q 12 17 12 12 Q 12 7 17 7 L 22 7",
 Tool::Formant=>"M 3 19 L 6 10 L 9 19 M 10 19 L 14 4 L 18 19 M 18 19 L 21 12 L 23 19",
 Tool::Amplitude=>"M 4 15 L 4 9 L 8 9 L 14 4 L 14 20 L 8 15 Z M 18 8 Q 23 12 18 16",
 Tool::Time=>"M 3 12 L 21 12 M 7 8 L 3 12 L 7 16 M 17 8 L 21 12 L 17 16 M 12 5 L 12 19",
 Tool::Attack=>"M 3 18 C 6 18 7 7 10 7 C 13 7 15 18 21 18 M 12 4 L 12 20 M 10 4 L 14 4 M 10 20 L 14 20",
 Tool::Split=>"M 12 3 L 12 21 M 3 9 L 8 9 L 8 15 L 3 15 M 21 9 L 16 9 L 16 15 L 21 15",
 Tool::Draw=>"M 4 20 L 5 15 L 17 3 L 21 7 L 9 19 Z M 5 15 L 9 19",
 Tool::Hand=>"M 8 13 L 8 6 C 8 3 11 3 11 6 L 11 11 L 11 4 C 11 1 14 1 14 4 L 14 11 L 14 6 C 14 3 17 3 17 6 L 17 12 L 17 9 C 17 6 20 6 20 9 L 20 16 C 20 20 17 22 14 22 L 12 22 C 10 22 8 20 7 18 L 3 13 C 2 11 4 10 6 12 L 8 14",
 Tool::Zoom=>"M 16 16 L 22 22 M 18 10 C 18 20 2 20 2 10 C 2 0 18 0 18 10 M 6 10 L 14 10 M 10 6 L 10 14"}}}
fn toolbar_height(g:&PGeom)->f32{let mut x=0.;let mut rows=1.;for w in [28.,28.,28.,28.,28.,28.,28.,28.,28.,28.,28.,28.,28.,46.,70.,28.,28.,28.,28.,28.,28.,28.,28.,28.]{if x+w>g.grid.w-12.{rows+=1.;x=0.;}x+=w+3.;}rows*31.+7.}
fn inspector_height(g:&PGeom)->f32{if g.grid.w<480.{68.}else{36.}}
pub fn pitch_rect(g:&PGeom,row:u16)->Rect{let top=toolbar_height(g)+inspector_height(g)+7.;Rect::new(g.grid.x+1.,g.y_of(row)+g.rows.base_height(row)+top,(g.grid.w-16.).max(1.),(super::EXTRA-top-47.).max(40.))}
pub fn note_rect(s:&State,g:&PGeom,ci:usize,ni:usize)->Option<Rect>{let clip=pl::clips(s).get(ci)?;let d=doc(s,ci)?;let n=d.notes.get(ni)?;let r=pitch_rect(g,clip.track);let cr=g.clip_rect(clip);let(a,b)=pl::window(s,clip);let sx=|t:f64|cr.x+((d.destination(t)*1000.-a)/(b-a))as f32*cr.w;let x=sx(n.start as f64*STEP);let end=sx(n.end as f64*STEP);let y=r.bottom()-(n.base+n.shift*d.strength-s.playlist.vocal.low)/s.playlist.vocal.span*r.h;Some(Rect::new(x,y-(r.h/s.playlist.vocal.span*0.39).max(3.),(end-x).max(2.),(r.h/s.playlist.vocal.span*0.78).max(6.)))}
pub fn keys(ui:&mut Ui,s:&State,g:&PGeom,row:u16,header:Rect){
 if !super::expanded(s,row){return;}
 let p=pitch_rect(g,row);
 let r=Rect::new(header.x,p.y,header.w,p.h);
 let visible = r.intersect(&Rect::new(header.x, g.grid.y, header.w, g.grid.h));
 if visible.is_empty() { return; }
 let vintage = ui.t.style.vintage_2005;
 ui.hit(visible,Action::Vocal(Command::Nav(super::navigation::Kind::Pan)),"Drag to scroll pitch; wheel over notes to scroll, Ctrl+Shift+wheel to zoom pitch");
 ui.c.push_clip(visible);
 ui.c.fill_rect(r,ui.t.s(0x344047));
 let(low,span)=(s.playlist.vocal.low,s.playlist.vocal.span);let h=r.h/span;
 let key_font = font(vintage, 10);
 let key_height = ui.c.line_height(key_font);
 for p in low.floor()as i32..=(low+span).ceil()as i32{let y=r.bottom()-(p as f32-low)/span*r.h;let black=[1,3,6,8,10].contains(&p.rem_euclid(12));ui.c.fill_rect(Rect::new(r.x,y-h/2.,if black{r.w*0.63}else{r.w},h),ui.t.s(if black{0x1b2328}else{0x46535a}));ui.c.line(r.x,y+h/2.,r.right(),y+h/2.,0.6,ui.t.s(0x2b3439));if h>=10.||p.rem_euclid(12)==0{let names=["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];let text_y = if vintage { (y - key_height / 2.0).clamp(r.y + 1.0, r.bottom() - key_height - 1.0) } else { y - 6.0 };ui.c.text(key_font,r.right()-29.,text_y,ui.t.text,&format!("{}{}",names[p.rem_euclid(12)as usize],p/12-1));}}
 ui.c.pop_clip();
}
fn button(ui:&mut Ui,r:Rect,cmd:Command,hint:&str,on:bool,shape:&TablerIcon){let hot=ui.hit(r,Action::Vocal(cmd),hint.to_string());control_face(ui,r,on,hot);let ink=if on{ui.t.glyph}else{ui.t.text};
let themed = match (ui.c.icon_pack(), cmd) {
    (0, _) => None,
    (_, Command::Tool(tool)) => Some(packed_art(tool, ui.c.icon_pack())),
    (_, Command::Snap) => Some(crate::icons::artwork(crate::icons::Icon::Magnet, ui.c.icon_pack())),
    (_, Command::Mute) => Some(crate::icons::artwork(crate::icons::Icon::Mute, ui.c.icon_pack())),
    (_, Command::Undo) => Some(crate::icons::artwork(crate::icons::Icon::Undo, ui.c.icon_pack())),
    (_, Command::Redo) => Some(crate::icons::artwork(crate::icons::Icon::Redo, ui.c.icon_pack())),
    (_, Command::Analyze) => Some(crate::icons::artwork(crate::icons::Icon::Wave, ui.c.icon_pack())),
    (_, Command::Join) => Some(crate::icons::artwork(crate::icons::Icon::Link, ui.c.icon_pack())),
    (_, Command::Correct) => Some(crate::icons::artwork(crate::icons::Icon::Tick, ui.c.icon_pack())),
    (_, Command::Bypass) => Some(crate::icons::artwork(crate::icons::Icon::Power, ui.c.icon_pack())),
    (_, Command::ResetNote) => Some(crate::icons::artwork(crate::icons::Icon::Repeat, ui.c.icon_pack())),
    _ => None,
};
if let Some(art)=themed { ui.c.art_icon(&art.shape, r.inset(5.), ink); }
else { ui.c.icon_stroke(shape,r.inset(5.),ink,1.45); }if on {
    let mark = Rect::new(r.x+7.,r.bottom()-3.,r.w-14.,1.0);
    if ui.t.style.vintage_2005 { ui.c.fill_rect(mark, Color::rgb(0xddb473)); }
    else { ui.c.fill_rrect(Rect::new(mark.x, mark.y, mark.w, 1.5), 0.7, ui.t.glyph); }
}}
fn text_btn(ui:&mut Ui,r:Rect,label:&str,cmd:Command,on:bool){let hot=ui.hit(r,Action::Vocal(cmd),label.to_string());if ui.t.style.vintage_2005 {control_face(ui,r,on,hot);}
else {ui.c.fill_rrect(r,4.,if on{ui.t.glyph.with_alpha(40)}else if hot{ui.t.button_hi}else{ui.t.s(0x2a3339)});}
ui.c.text_center(font(ui.t.style.vintage_2005,11),r,ui.t.text,label);}
fn sym(d:&'static str)->TablerIcon{TablerIcon{filled:false,d}}
pub fn paint(ui:&mut Ui,s:&State,g:&PGeom){
    let Some(row) = s.playlist.vocal.row else { return };
    if g.rows.strip(row) || g.rows.height(row) == 0.0 { return; }
    let y = g.y_of(row) + g.rows.base_height(row);
    let area = Rect::new(g.grid.x, y, g.grid.w, super::EXTRA - 3.0);
    let visible = area.intersect(&g.grid);
    if visible.is_empty() || !ui.c.visible(visible) { return; }
    ui.c.push_clip(visible);
    ui.clip_hit_rect(visible, |ui| editor(ui, s, g, row, area));
    ui.c.pop_clip();
}

fn editor(ui: &mut Ui, s: &State, g: &PGeom, row: u16, area: Rect) {
 let y = area.y;
 let vintage = ui.t.style.vintage_2005;
 if vintage {
    crate::vintage::recess(ui.c, area, Color::rgb(0x424d53));
 } else {
    ui.c.fill_rect(area, ui.t.s(0x171f24));
 }
 ui.hit(area,Action::Swallow,"Vocal editor");
 let mut x=area.x+5.;let mut by=y+4.;let next=|x:&mut f32,by:&mut f32,w:f32|{if *x+w>area.right()-5.{*x=area.x+5.;*by+=31.;}let r=Rect::new(*x,*by,w,27.);*x+=w+3.;r};
 for tool in TOOLS{let rr=next(&mut x,&mut by,28.);button(ui,rr,Command::Tool(tool),label(tool),s.playlist.vocal.tool==tool,&icon(tool));}
 let key=["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"][s.playlist.vocal.key as usize];let scale=["Chromatic","Major","Minor"][s.playlist.vocal.scale as usize];
 text_btn(ui,next(&mut x,&mut by,46.),&format!("Key {key}"),Command::Key,false);text_btn(ui,next(&mut x,&mut by,70.),scale,Command::Scale,false);
 let extras=[(Command::Correct,"Correct selected notes to key / scale","M 4 12 L 9 17 L 20 5"),(Command::Join,"Join selected notes","M 3 9 L 21 9 L 21 15 L 3 15 Z M 9 6 L 9 18 M 15 6 L 15 18"),(Command::Snap,"Pitch and time snapping; Alt temporarily bypasses","M 5 4 L 5 13 C 5 23 19 23 19 13 L 19 4 L 15 4 L 15 13 C 15 17 9 17 9 13 L 9 4 Z"),(Command::Mute,"Mute / unmute selected notes","M 4 9 L 8 9 L 13 4 L 13 20 L 8 15 L 4 15 Z M 17 9 L 23 15 M 23 9 L 17 15"),(Command::Bypass,"Bypass tuning for this track","M 12 2 L 12 12 M 6 5 C 0 9 2 22 12 22 C 22 22 24 9 18 5"),(Command::ResetNote,"Reset selected notes","M 5 5 L 5 11 L 11 11 M 5 11 C 8 1 22 5 21 14 C 20 21 11 23 6 18"),(Command::Analyze,"Reanalyse original vocal","M 3 11 L 3 16 M 7 6 L 7 20 M 12 3 L 12 22 M 17 7 L 17 18 M 21 11 L 21 15"),(Command::Undo,"Undo vocal edit","M 9 5 L 3 11 L 9 17 M 3 11 L 14 11 Q 21 11 21 20"),(Command::Redo,"Redo vocal edit","M 15 5 L 21 11 L 15 17 M 21 11 L 10 11 Q 3 11 3 20")];
 for(cmd,hint,path)in extras{let on=cmd==Command::Snap&&s.playlist.vocal.snap||cmd==Command::Bypass&&pl::clips(s).iter().enumerate().any(|(ci,c)|c.track==row&&doc(s,ci).is_some_and(|d|!d.enabled));button(ui,next(&mut x,&mut by,28.),cmd,hint,on,&sym(path));}
 // A compact inspector replaces paired +/- buttons. Values are draggable; Alt-click resets.
 let fields=[(super::Field::Pitch,"PITCH"),(if s.playlist.vocal.tool==Tool::Transition{super::Field::Transition}else{super::Field::Modulation},if s.playlist.vocal.tool==Tool::Transition{"TRANSITION"}else{"MODULATION"}),(super::Field::Drift,"DRIFT"),(super::Field::Formant,"FORMANT"),(super::Field::Amplitude,"LEVEL"),(super::Field::Strength,"CORRECTION")];
 let chosen:Vec<_>=s.project.vocal_edits.iter().flat_map(|d|d.notes.iter().enumerate().filter(|(ni,_)|s.playlist.vocal.selected.contains(&(d.channel,*ni))).map(move|(_,n)|(d,n))).collect();let cols=if g.grid.w<480.{3}else{6};let width=(area.w-10.)/cols as f32;let iy=y+toolbar_height(g);
 for(j,(field,name))in fields.into_iter().enumerate(){let rr=Rect::new(area.x+5.+(j%cols)as f32*width,iy+(j/cols)as f32*32.,width-4.,30.);let enabled=!chosen.is_empty()||field==super::Field::Strength;if vintage {crate::vintage::recess(ui.c,rr,Color::rgb(0x29363c));}else{ui.c.fill_rrect(rr,3.,ui.t.s(0x222c32));}if enabled{ui.hit(rr,Action::Vocal(Command::Param(field)),"Drag value up/down; Alt for fine changes, Alt-click resets");}ui.c.text(font(vintage,9),rr.x+6.,rr.y+2.,ui.t.text_dim,name);
  let values:Vec<f32>=chosen.iter().map(|(d,n)|match field{super::Field::Pitch=>n.base+n.shift,super::Field::Modulation=>n.variation*100.,super::Field::Drift=>n.drift*100.,super::Field::Transition=>n.transition*100.,super::Field::Formant=>n.formant,super::Field::Amplitude=>n.gain,super::Field::Strength=>d.strength*100.}).collect();let mixed=values.windows(2).any(|w|(w[0]-w[1]).abs()>0.01);let value=values.first().copied().or_else(||if field==super::Field::Strength{pl::clips(s).iter().enumerate().find_map(|(ci,c)|(c.track==row).then(||doc(s,ci).map(|d|d.strength*100.)).flatten())}else{None});let value=if mixed{"Mixed".into()}else if let Some(v)=value{match field{super::Field::Pitch=>{let p=v.round()as i32;let names=["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];format!("{}{} {:+.0} ct",names[p.rem_euclid(12)as usize],p/12-1,(v-p as f32)*100.)},super::Field::Modulation|super::Field::Drift|super::Field::Transition|super::Field::Strength=>format!("{v:.0}%"),super::Field::Formant=>format!("{v:+.1} st"),super::Field::Amplitude=>format!("{v:+.1} dB")}}else{"—".into()};ui.c.text_clip(font(vintage,12),rr.x+6.,rr.y+13.,rr.w-10.,if enabled{ui.t.text}else{ui.t.text_faint},&value);
 }
 let r=pitch_rect(g,row);let(low,span)=(s.playlist.vocal.low,s.playlist.vocal.span);let py=|p:f32|r.bottom()-(p-low)/span*r.h;ui.c.push_clip(r);ui.c.fill_rect(r,ui.t.s(0x202b31));ui.hit(r,Action::Vocal(Command::Surface),"Wheel: pitch scroll · Shift: time scroll · Ctrl: time zoom · Ctrl+Shift: pitch zoom · drag empty space to select");
 for p in low.floor()as i32..=(low+span).ceil()as i32{let yy=py(p as f32);let h=r.h/span;let allowed=super::nearest(p as f32,s.playlist.vocal.key,s.playlist.vocal.scale)==p as f32;ui.c.fill_rect(Rect::new(r.x,yy-h/2.,r.w,h),if vintage {
    Color::rgb(if !allowed {0x222532} else if [1,3,6,8,10].contains(&p.rem_euclid(12)) {0x2a2e3b} else {0x343846})
   } else {ui.t.s(if !allowed{0x182127}else if [1,3,6,8,10].contains(&p.rem_euclid(12)){0x1c262c}else{0x253138})});ui.c.line(r.x,yy+h/2.,r.right(),yy+h/2.,0.6,ui.t.s(if p.rem_euclid(12)==0{0x50616a}else{0x34434c}));}
 let step=(f32::from(s.project.ppq.max(4))/4.*g.ppt).max(1.);let stride=(18./step).ceil().max(1.);let beat=step*stride;let offset=(s.playlist_scroll.0/beat).floor()*beat-s.playlist_scroll.0;let mut xx=r.x+offset;while xx<r.right(){ui.c.line(xx,r.y,xx,r.bottom(),0.5,ui.t.s(0x2c3a42));xx+=beat;}
 let ink=ui.t.glyph.mix(Color::rgb(0xffcc70),0.82);let mut any=false;
 for(ci,clip)in pl::clips(s).iter().enumerate().filter(|(_,c)|c.track==row){let Some(d)=doc(s,ci)else{continue};any=true;let cr=g.clip_rect(clip);let(a,b)=pl::window(s,clip);let(a,b)=(a/1000.,b/1000.);if b<=a{continue;}let sx=|src:f64|cr.x+((d.destination(src)-a)/(b-a))as f32*cr.w;let bounds=Rect::new(cr.x.max(r.x),r.y,(cr.right().min(r.right())-cr.x.max(r.x)).max(0.),r.h);if bounds.w<=0.{continue;}ui.c.push_clip(bounds);ui.hit(bounds,Action::Vocal(Command::Grid(ci)),"Drag empty space to select; choose Draw to paint pitch");let pitches=super::contour(s,d);
  for(ni,note)in d.notes.iter().enumerate(){let Some(rr)=note_rect(s,g,ci,ni)else{continue};if rr.intersect(&bounds).is_empty(){continue;}let selected=s.playlist.vocal.selected.contains(&(d.channel,ni));let color=if note.muted{ui.t.text_faint}else{ink};let opacity=if !d.enabled{45}else if selected{200}else{105};
   let radius = if vintage { 0.0 } else { rr.h / 2.0 };
   ui.c.fill_rrect(rr,radius,color.with_alpha(opacity));ui.c.stroke_rrect(rr,radius,0.8,color.with_alpha(if selected{255}else{170}));
   if note.muted{ui.c.line(rr.x+3.,rr.cy(),rr.right()-3.,rr.cy(),1.,ui.t.text_dim);}
   ui.hit(rr.intersect(&bounds),Action::Vocal(Command::Note(ci,ni)),label(s.playlist.vocal.tool));if matches!(s.playlist.vocal.tool,Tool::Main|Tool::Time|Tool::Split)&&(rr.w>12.||s.playlist.vocal.tool==Tool::Time){let edge=Rect::new(rr.right()-4.,rr.y,4.,rr.h).intersect(&bounds);ui.hit(edge,Action::Vocal(Command::Boundary(ci,ni)),if s.playlist.vocal.tool==Tool::Split{"Drag the note separation"}else{"Drag the note's duration"});if selected{ui.c.line(rr.right()-3.,rr.y+2.,rr.right()-3.,rr.bottom()-2.,1.,color);}}
  }
  let lo=(d.original(a).max(0.)/STEP)as usize;let hi=((d.original(b).max(0.)/STEP)as usize+1).min(d.f0.len());let stride=((hi.saturating_sub(lo))as f32/cr.w.max(1.)).ceil().max(1.)as usize;let mut previous=None;let mut dry=None;
  for i in (lo..hi).step_by(stride){let x=sx(i as f64*STEP);if d.f0[i]>0.{let yy=py(pitches[i]);let original=py(69.+12.*(d.f0[i]/440.).log2());if let Some((px,prev))=dry{ui.c.line(px,prev,x,original,0.75,ui.t.text.with_alpha(48));}if let Some((px,prev))=previous{ui.c.line(px,prev,x,yy,1.25,Color::rgb(0xffe5ac));}previous=Some((x,yy));dry=Some((x,original));}else{previous=None;dry=None;}}
  ui.c.pop_clip();
 }
 if !any{ui.c.text(font(vintage,13),r.x+14.,r.cy()-8.,ui.t.text_dim,"Analyse a solo vocal to edit notes, pitch and expression here.");}
 if let Some(boxr)=s.playlist.vocal.marquee{ui.c.fill_rect(boxr,ink.with_alpha(25));ui.c.stroke_rrect(boxr,1.,1.,ink.with_alpha(160));}ui.c.pop_clip();
 let rail=Rect::new(r.right()+3.,r.y,9.,r.h);if vintage {crate::vintage::recess(ui.c,rail,Color::rgb(0x303b42));}else{ui.c.fill_rrect(rail,4.,ui.t.s(0x0e1519));}ui.hit(rail,Action::Vocal(Command::Nav(super::navigation::Kind::Vertical)),"Drag pitch scrollbar");let thumb=Rect::new(rail.x+1.,rail.y+(128.-low-span)/128.*rail.h,7.,(span/128.*rail.h).max(8.));if vintage {crate::vintage::metal(ui.c,thumb,Color::rgb(0x657780));}else{ui.c.fill_rrect(thumb,3.,ui.t.s(0x657780));}
 let overview=Rect::new(r.x,r.bottom()+5.,(r.w-145.).max(30.),14.);if vintage {crate::vintage::recess(ui.c,overview,Color::rgb(0x303b42));}else{ui.c.fill_rrect(overview,3.,ui.t.s(0x0f171c));}ui.hit(overview,Action::Vocal(Command::Nav(super::navigation::Kind::Horizontal)),"Drag time navigator; the Playlist timeline stays aligned");let total=(pl::clips(s).iter().map(|c|c.end()).max().unwrap_or(0)as f32/(f32::from(s.project.ppq.max(4))*4.)*s.playlist_bar_w+r.w).max(r.w);let tw=(r.w/total*overview.w).max(12.);let tx=(s.playlist_scroll.0/total*overview.w).clamp(0.,overview.w-tw);let thumb=Rect::new(overview.x+tx,overview.y+2.,tw,10.);if vintage {crate::vintage::metal(ui.c,thumb,Color::rgb(0x657780));}else{ui.c.fill_rrect(thumb,3.,ui.t.s(0x526772));}
 for(j,(label,cmd))in [("−",Command::XZoom(-1)),("+",Command::XZoom(1)),("Fit",Command::Fit)].into_iter().enumerate(){text_btn(ui,Rect::new(r.right()-136.+j as f32*45.,r.bottom()+3.,41.,19.),label,cmd,false);}
 let count=s.playlist.vocal.selected.len();let status=if s.playlist.vocal.busy||s.playlist.vocal.message.starts_with("Vocal tuning:")||s.playlist.vocal.message.contains("cancelled"){s.playlist.vocal.message.clone()}else{format!("{} selected  ·  {}  ·  Wheel: pitch   Shift: time   Ctrl: zoom   Ctrl+Shift: pitch zoom",count,if s.playlist.vocal.snap{"Snap on"}else{"Free"})};ui.c.text_clip(font(vintage,10),area.x+6.,area.bottom()-14.,area.w-76.,ui.t.text_dim,&status);if s.playlist.vocal.busy{text_btn(ui,Rect::new(area.right()-68.,area.bottom()-25.,63.,21.),"Cancel",Command::Cancel,false);}else if !s.playlist.vocal.failed.is_empty(){text_btn(ui,Rect::new(area.right()-68.,area.bottom()-25.,63.,21.),"Retry",Command::Retry,false);}
 if let Some((px,py))=s.playlist.vocal.palette{let w=330f32.min(area.w-10.);let h=230.;let menu=Rect::new(px.clamp(area.x+4.,(area.right()-w-4.).max(area.x+4.)),py.clamp(area.y+4.,(area.bottom()-h-4.).max(area.y+4.)),w,h);if vintage {crate::vintage::metal(ui.c,menu,Color::rgb(0x465259));}else{ui.c.fill_rrect(menu,6.,ui.t.s(0x10191f));ui.c.stroke_rrect(menu,6.,1.,ui.t.s(0x667881));}ui.hit(menu,Action::Swallow,"Vocal tools");let names=["Main","Pitch","Modulation","Drift","Transition","Formant","Amplitude","Timing","Time handle","Separation","Draw","Hand","Zoom"];
  for(j,t)in TOOLS.into_iter().enumerate(){let rr=Rect::new(menu.x+5.+(j%2)as f32*(w-10.)/2.,menu.y+6.+(j/2)as f32*27.,(w-12.)/2.,25.);let hot=ui.hit(rr,Action::Vocal(Command::Tool(t)),label(t));if hot||s.playlist.vocal.tool==t{if vintage {control_face(ui,rr,s.playlist.vocal.tool==t,hot);}else{ui.c.fill_rrect(rr,3.,ui.t.glyph.with_alpha(35));}}let glyph=Rect::new(rr.x+4.,rr.y+3.,19.,19.);
  if ui.c.icon_pack()==0{ui.c.icon_stroke(&icon(t),glyph,ui.t.text,1.4);}
  else{ui.c.art_icon(&packed_art(t,ui.c.icon_pack()).shape,glyph,ui.t.text);}ui.c.text(font(vintage,12),rr.x+30.,rr.y+5.,ui.t.text,names[j]);}
  for(j,(name,cmd))in [("Correct",Command::Correct),("Join",Command::Join),("Reset note",Command::ResetNote),("Reset all",Command::Reset)].into_iter().enumerate(){text_btn(ui,Rect::new(menu.x+5.+j as f32*(w-10.)/4.,menu.bottom()-30.,(w-14.)/4.,24.),name,cmd,false);}
 }
}
