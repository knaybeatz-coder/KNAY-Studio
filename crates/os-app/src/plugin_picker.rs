// SPDX-License-Identifier: GPL-3.0-or-later
//! Visual, searchable picker over the existing installed-plugin database.
use crate::{app::{Action,App,State,Ui,Drag},plugin_menu::{Target,PlugCmd},plugin_db::Entry};
use os_ui::{Rect,Font};
use winit::keyboard::{Key,NamedKey};
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Hit { Background,Close,Filter(u8),Favorite(bool,usize),Tile(bool,usize),Manage,Clear,Picture(bool,usize),Page(i8),Generate,CancelGeneration,PreviewReport }
#[derive(Clone,Debug)]
pub struct View {
    pub open:bool, pub target:Target, pub query:String, pub filter:u8,
    pub preview_revision:u64,
    pub images:crate::plugin_artwork::Images, pub hovered:Option<(bool,usize)>,
    pub first:usize, pub selected:usize, pub pending:Option<(Hit,(f32,f32))>,
}
impl Default for View {fn default()->Self{Self{preview_revision:0,images:Default::default(),hovered:None,open:false,target:Target::All,query:String::new(),filter:0,first:0,selected:0,pending:None}}}
fn entries(s:&State,effect:bool)->Vec<&Entry>{
    let v=&s.plugin_picker;
    if (effect && matches!(v.target,Target::Gen(_)|Target::Replace(_)|Target::Stack(_))) || (!effect && matches!(v.target,Target::Fx(_)|Target::ClipFx(..)))
        || (v.filter==1 && effect) || (v.filter==2 && !effect) {return Vec::new();}
    let q=v.query.to_lowercase();
    let list=if effect{&s.plugdb.lists.fx}else{&s.plugdb.lists.gens};
    list.iter().filter(|e| (v.filter!=3 || e.favorite) && q.split_whitespace().all(|w|
        format!("{} {} {} {}",e.label,e.vendor,e.category,crate::plugin_db::format_label(e.format)).to_lowercase().contains(w))).collect()
}
fn choices(s:&State)->Vec<(bool,usize)>{[true,false].into_iter().flat_map(|fx|entries(s,fx).into_iter().map(move|e|(fx,e.index))).collect()}
fn panes(s:&State)->Vec<bool>{
    match s.plugin_picker.target {
        Target::Gen(_)|Target::Replace(_)|Target::Stack(_)=>vec![false],Target::Fx(_)|Target::ClipFx(..)=>vec![true],
        Target::All=>match s.plugin_picker.filter{1=>vec![false],2=>vec![true],_=>vec![true,false]},
    }
}
fn geometry(s:&State)->(Rect,usize,usize){
    let (w,h)=s.screen;
    let r=Rect::new(18.,88.,(w-36.).max(200.),(h-110.).max(180.));
    let cols=((r.w-40.)/panes(s).len() as f32/172.).floor().max(1.) as usize;
    let rows=((r.h-210.)/138.).floor().max(1.) as usize;
    (r,cols,rows)
}
fn max_first(s:&State)->usize{let (_,cols,rows)=geometry(s);entries(s,true).len().max(entries(s,false).len()).div_ceil(cols).div_ceil(rows).saturating_sub(1)*rows}
pub fn open(app:&mut App,target:Target){
    app.state.menu_open=None;app.state.playlist.popup=None;app.state.plugin_picker=View{open:true,target,..Default::default()};
    app.state.plugin_picker.images=crate::plugin_artwork::load(app.state.plugdb.lists.fx.iter().chain(&app.state.plugdb.lists.gens).cloned());
    app.pressed=None;app.invalidate();
}
pub fn close(app:&mut App){
    app.state.plugin_picker.open=false;app.state.plugin_picker.pending=None;app.pressed=None;
    app.hits.retain(|h|!matches!(h.action,Action::PluginPicker(_)));app.invalidate();
}
fn choose(app:&mut App,fx:bool,index:usize){
    if !choices(&app.state).contains(&(fx,index)){return;}
    let target=app.state.plugin_picker.target;close(app);
    let cmd=match (target,fx){
        (Target::ClipFx(channel,slot),_)=>PlugCmd::ClipFx{channel,slot,index},
        (Target::Fx(slot),_)=>PlugCmd::Effect{index,slot},(_,true)=>PlugCmd::Effect{index,slot:None},
        (Target::Stack(row),false)=>PlugCmd::Stack{index,row},
        (Target::Replace(row),false)=>PlugCmd::Replace{index,row},
        (Target::Gen(after),false)=>PlugCmd::Generator{index,after},_=>PlugCmd::Generator{index,after:None},
    };
    app.state.message=crate::plugin_menu::run(app,cmd);
}
pub fn act(app:&mut App,hit:Hit){
    match hit{
        Hit::Generate=>crate::plugin_previews::start(app),
        Hit::CancelGeneration=>crate::plugin_previews::cancel(),
        Hit::PreviewReport=>crate::plugin_previews::report(app),
        Hit::Picture(fx,index)=>{let list=if fx{&app.state.plugdb.lists.fx}else{&app.state.plugdb.lists.gens};if let Some(uid)=list.iter().find(|e|e.index==index).map(|e|e.uid.clone()){crate::plugin_artwork::choose(app,uid);}},
        Hit::Page(direction)=>{let (_,_,rows)=geometry(&app.state);let max=max_first(&app.state);let v=&mut app.state.plugin_picker;v.first=v.first.saturating_add_signed(direction as isize*rows as isize).min(max);},
        Hit::Close=>close(app),
        Hit::Filter(f)=>{let v=&mut app.state.plugin_picker;if v.target==Target::All || f==0 || f==3{v.filter=f;v.first=0;v.selected=0;v.hovered=None;}},
        Hit::Clear=>{let v=&mut app.state.plugin_picker;v.query.clear();v.first=0;v.selected=0;},
        Hit::Favorite(fx,i)=>{
            let list=if fx{&app.state.plugdb.lists.fx}else{&app.state.plugdb.lists.gens};
            if let Some(uid)=list.iter().find(|e|e.index==i).map(|e|e.uid.clone()){
                app.state.plugdb.prefs.toggle_favorite(&uid);app.state.plugdb.prefs.save();app.state.plugdb.rebuild(&app.plugins);
            }
            app.state.plugin_picker.first=app.state.plugin_picker.first.min(max_first(&app.state));
        },
        Hit::Tile(fx,i)=>choose(app,fx,i),
        Hit::Manage=>{close(app);app.state.message=crate::plugin_menu::run(app,PlugCmd::Manage);},
        Hit::Background=>{},
    }
    app.invalidate();
}
pub fn mouse_down(app:&mut App)->bool{
    if !app.state.plugin_picker.open{return false;}
    if app.hits.iter().any(|h|matches!(h.action,Action::Close|Action::Minimize|Action::Maximize) && h.rect.contains(app.mouse.0,app.mouse.1)) {close(app);return false;}
    let hit=app.top_hit(app.mouse.0,app.mouse.1).and_then(|h|if let Action::PluginPicker(a)=h.action{Some(a)}else{None}).unwrap_or(Hit::Background);
    app.state.plugin_picker.pending=Some((hit,app.mouse));app.invalidate();true
}
pub fn mouse_up(app:&mut App)->bool{
    if !app.state.plugin_picker.open{return false;}
    if let Some((hit,_))=app.state.plugin_picker.pending.take(){
        if app.top_hit(app.mouse.0,app.mouse.1).is_some_and(|h|h.action==Action::PluginPicker(hit)){act(app,hit);}
    }
    true
}
pub fn mouse_move(app:&mut App,x:f32,y:f32)->bool{
    if !app.state.plugin_picker.open{return false;}
    app.mouse=(x,y);
    let hit=app.top_hit(x,y).map(|h|(h.action.clone(),h.hint.clone()));app.hover=hit.as_ref().map(|h|h.0.clone());
    app.state.plugin_picker.hovered=None;
    if let Some((action,hint))=hit{if let Action::PluginPicker(Hit::Tile(fx,i)|Hit::Favorite(fx,i))=action{
        app.state.plugin_picker.hovered=Some((fx,i));
        if let Some(n)=choices(&app.state).iter().position(|p|*p==(fx,i)){app.state.plugin_picker.selected=n;}
    }app.hint=hint;}
    app.resize_cursor=false;
    if let Some((Hit::Tile(fx,i),m0))=app.state.plugin_picker.pending{
        if (x-m0.0).hypot(y-m0.1)>5.{
            let list=if fx{&app.state.plugdb.lists.fx}else{&app.state.plugdb.lists.gens};
            if let Some(label)=list.iter().find(|e|e.index==i).map(|e|e.label.clone()){
                close(app);
                let item=if fx{crate::views::browser::Item::Effect(i)}else{crate::views::browser::Item::Generator(i)};
                app.drag=Some(Drag::Browser{item,label:label.clone(),m0,started:true});
                app.state.browser_drag=Some((label,x,y));
            }
        }
    }
    app.invalidate();true
}
pub fn wheel(app:&mut App,dy:f32)->bool{
    if !app.state.plugin_picker.open{return false;}
    let max=max_first(&app.state);let v=&mut app.state.plugin_picker;
    v.first=v.first.saturating_add_signed(if dy>0.{-1}else if dy<0.{1}else{0}).min(max);app.invalidate();true
}
pub fn key(app:&mut App,key:&Key,ctrl:bool,alt:bool)->bool{
    if !app.state.plugin_picker.open{return false;}
    match key{
        Key::Named(NamedKey::Escape)=>close(app),
        Key::Named(NamedKey::F8) if crate::shortcuts::f8(app.shift,ctrl,alt)==Some(crate::shortcuts::F8::PluginPicker)=>close(app),
        Key::Named(NamedKey::Enter)=>{let all=choices(&app.state);if let Some(&(fx,i))=all.get(app.state.plugin_picker.selected){choose(app,fx,i);}},
        Key::Named(NamedKey::ArrowDown|NamedKey::ArrowUp|NamedKey::ArrowLeft|NamedKey::ArrowRight)=>{
            app.state.plugin_picker.hovered=None;
            let all=choices(&app.state);let (_,cols,rows)=geometry(&app.state);
            let step=match key{Key::Named(NamedKey::ArrowUp)=>-(cols as isize),Key::Named(NamedKey::ArrowDown)=>cols as isize,Key::Named(NamedKey::ArrowLeft)=>-1,_=>1};
            let sel=app.state.plugin_picker.selected.saturating_add_signed(step).min(all.len().saturating_sub(1));
            app.state.plugin_picker.selected=sel;
            if let Some(&(fx,i))=all.get(sel){let row=entries(&app.state,fx).iter().position(|e|e.index==i).unwrap_or(0)/cols;
                let v=&mut app.state.plugin_picker;if row<v.first{v.first=row;}else if row>=v.first+rows{v.first=row+1-rows;}}
        },
        Key::Named(NamedKey::Backspace)=>{let v=&mut app.state.plugin_picker;v.query.pop();v.first=0;v.selected=0;},
        Key::Character(c) if ctrl && c.eq_ignore_ascii_case("a")=>{act(app,Hit::Clear);},
        Key::Character(c) if !ctrl && !alt=>{let v=&mut app.state.plugin_picker;for ch in c.chars().filter(|c|!c.is_control()){if v.query.chars().count()<128{v.query.push(ch);}}v.first=0;v.selected=0;},
        _=>{},
    }
    app.invalidate();true
}
pub fn paint(ui:&mut Ui,s:&State){
    let v=&s.plugin_picker;if !v.open{return;}
    ui.win=None;
    ui.hit(Rect::new(0.,0.,s.screen.0,s.screen.1),Action::PluginPicker(Hit::Background),"Plugin picker");
    let (r,cols,rows)=geometry(s);let panes=panes(s);
    ui.c.fill_rrect(r,10.,ui.t.s(0x182024));ui.c.stroke_rrect(r,10.,1.,ui.t.s(0x4b595e));
    let title=match v.target{Target::Gen(_)|Target::Replace(_)|Target::Stack(_)=>"Choose an instrument",Target::Fx(_)|Target::ClipFx(..)=>"Choose an effect",Target::All=>"Plugin picker"};
    let progress=crate::plugin_previews::progress();
    let caption=if progress.running{format!("{} / {} · {}",progress.done,progress.total,progress.current)}else{title.into()};
    ui.c.text_clip(Font::new(20).bold(),r.x+18.,r.y+16.,r.w-370.,ui.t.text,&caption);
    if !progress.summary.is_empty(){ui.c.text_clip(Font::new(10),r.x+18.,r.y+38.,r.w-350.,ui.t.text_dim,&progress.summary);}
    let button=|ui:&mut Ui,r:Rect,label:&str,h:Hit,on:bool|crate::feature_votes::button(ui,r,label,Action::PluginPicker(h),true,on,label);
    if cfg!(windows){button(ui,Rect::new(r.right()-324.,r.y+12.,154.,28.),if progress.running{"Cancel generation"}else{"Generate previews"},if progress.running{Hit::CancelGeneration}else{Hit::Generate},!progress.running);}
    button(ui,Rect::new(r.right()-162.,r.y+12.,108.,28.),"Manage plugins",Hit::Manage,false);
    button(ui,Rect::new(r.right()-44.,r.y+12.,28.,28.),"X",Hit::Close,false);
    let filters:Vec<(u8,&str)>=if v.target==Target::All{vec![(0,"Hybrid"),(1,"Instruments"),(2,"Effects"),(3,"Favorites")]}else{vec![(0,"All"),(3,"Favorites")]};
    let search=Rect::new(r.x+18.,r.y+52.,(r.w-58.-filters.len() as f32*98.).max(120.),32.);
    ui.c.fill_rrect(search,5.,ui.t.s(0x0e1518));
    let label=if v.query.is_empty(){"Search plugins..."}else{&v.query};
    ui.c.text_clip(Font::new(13),search.x+9.,search.y+9.,search.w-32.,ui.t.text,label);
    button(ui,Rect::new(search.right()-28.,search.y+3.,25.,26.),"X",Hit::Clear,false);
    for (i,(f,label)) in filters.iter().enumerate(){button(ui,Rect::new(search.right()+10.+i as f32*98.,search.y,92.,32.),label,Hit::Filter(*f),v.filter==*f);}
    let all=choices(s);let selected=all.get(v.selected).copied();
    let width=(r.w-36.-12.*(panes.len()-1) as f32)/panes.len() as f32;
    let cardw=(width-8.*(cols-1) as f32)/cols as f32;
    for (pane,fx) in panes.into_iter().enumerate(){
        let x=r.x+18.+pane as f32*(width+12.);let list=entries(s,fx);
        ui.c.text_clip(Font::new(12).bold(),x,r.y+100.,width,ui.t.text_dim,&format!("{}  ·  {}",if fx{"EFFECTS"}else{"INSTRUMENTS"},list.len()));
        if list.is_empty(){ui.c.text_clip(Font::new(13),x,r.y+146.,width,ui.t.text_dim,"No matching plugins");}
        for (n,e) in list.iter().skip(v.first*cols).take(rows*cols).enumerate(){
            let card=Rect::new(x+(n%cols) as f32*(cardw+8.),r.y+122.+(n/cols) as f32*138.,cardw,130.);
            let hover=ui.hit(card,Action::PluginPicker(Hit::Tile(fx,e.index)),format!("{} — click to add, or drag to {}",e.label,if fx{"the mixer"}else{"the channel rack"}));
            let accent=if fx{ui.t.h(0xe7b47b)}else{ui.t.h(0xb5d98b)};
            if hover||selected==Some((fx,e.index)){ui.c.fill_rrect(card,6.,ui.t.s(0x2b3b40));ui.c.stroke_rrect(card,6.,1.,accent);}
            let preview=Rect::new(card.x+10.,card.y+8.,card.w-20.,92.);
            if let Some(p)=v.images.get(&e.uid){crate::plugin_artwork::draw(ui,preview,p);}else{fallback(ui,preview,fx);}
            button(ui,Rect::new(card.right()-27.,card.y+4.,23.,23.),if e.favorite{"★"}else{"☆"},Hit::Favorite(fx,e.index),e.favorite);
            ui.c.text_clip(Font::new(13).bold(),card.x+8.,card.y+109.,card.w-16.,ui.t.text,&e.label);
        }
    }
    if let Some((fx,index))=v.hovered.filter(|_|v.pending.is_none()){
        if let Some(e)=entries(s,fx).into_iter().find(|e|e.index==index){
            if let Some(p)=v.images.get(&e.uid){
                let w=320_f32.min(r.w*0.46);let h=160.;
                let x=if ui.mouse.0<r.cx(){r.right()-w-18.}else{r.x+18.};
                let large=Rect::new(x,r.bottom()-h-84.,w,h);
                ui.c.fill_rrect(large,7.,ui.t.s(0x0e1518));ui.c.stroke_rrect(large,7.,1.,ui.t.s(0x50636b));
                crate::plugin_artwork::draw(ui,Rect::new(large.x+8.,large.y+8.,large.w-16.,large.h-34.),p);
                ui.c.text_center(Font::new(12),Rect::new(large.x+8.,large.bottom()-23.,large.w-16.,18.),ui.t.text,&e.label);
            }
        }
    }
    let inspector=Rect::new(r.x+18.,r.bottom()-76.,r.w-36.,66.);
    ui.c.fill_rrect(inspector,6.,ui.t.s(0x111a1e));
    let current=v.hovered.filter(|i|all.contains(i)).or(selected);
    if let Some((fx,index))=current{
        if let Some(e)=entries(s,fx).into_iter().find(|e|e.index==index){
            let pic=Rect::new(inspector.x+8.,inspector.y+5.,112.,56.);
            if let Some(p)=v.images.get(&e.uid){crate::plugin_artwork::draw(ui,pic,p);}else{fallback(ui,pic,fx);}
            let x=inspector.x+134.;let w=(inspector.w-390.).max(80.);
            ui.c.text_clip(Font::new(14).bold(),x,inspector.y+8.,w,ui.t.text,&e.label);
            ui.c.text_clip(Font::new(11),x,inspector.y+29.,w,ui.t.text_dim,&format!("{} · {}",e.vendor,e.category));
            ui.c.text_clip(Font::new(10),x,inspector.y+47.,w,ui.t.text_dim,if progress.summary.is_empty(){"Click to add · Drag to place · Esc to close"}else{&progress.summary});
            button(ui,Rect::new(inspector.right()-250.,inspector.y+18.,108.,28.),if progress.failed>0{"Preview report"}else{"Choose picture"},if progress.failed>0{Hit::PreviewReport}else{Hit::Picture(fx,index)},false);
        }
    }
    let max=max_first(s);
    button(ui,Rect::new(inspector.right()-128.,inspector.y+18.,28.,28.),"<",Hit::Page(-1),false);
    ui.c.text_center(Font::new(11),Rect::new(inspector.right()-98.,inspector.y+18.,62.,28.),ui.t.text_dim,&format!("{}/{}",v.first/rows+1,max.div_ceil(rows)+1));
    button(ui,Rect::new(inspector.right()-32.,inspector.y+18.,28.,28.),">",Hit::Page(1),false);
}
/// A neutral plugin-category symbol, clearly distinct from a real editor preview.
fn fallback(ui:&mut Ui,r:Rect,fx:bool){
    let r=Rect::new(r.cx()-48.,r.cy()-26.,96.,52.);let ink=ui.t.text_dim;
    ui.c.stroke_rrect(r,5.,1.,ink);
    if fx{for i in 0..3{let x=r.x+22.+i as f32*26.;ui.c.fill_rect(Rect::new(x,r.y+12.,2.,29.),ink);ui.c.fill_rrect(Rect::new(x-5.,r.y+16.+i as f32*5.,12.,6.),2.,ink);}}
    else{for i in 0..7{ui.c.fill_rect(Rect::new(r.x+8.+i as f32*12.,r.y+10.,10.,32.),ink);}for i in [0,1,3,4,5]{ui.c.fill_rect(Rect::new(r.x+15.+i as f32*12.,r.y+10.,7.,20.),ui.t.s(0x182024));}}
}

#[cfg(test)]
mod tests{
    use super::*;
    use os_ui::Canvas;
    fn app()->App{let mut a=App::new(None);a.plugins=crate::plugins::Plugins::builtins();a.state.plugdb.rebuild(&a.plugins);a}
    #[test]
    fn plugin_picker_search_favorites_keyboard_and_layout(){
        let mut a=app();let mut c=Canvas::new(1280,900,1.).unwrap();
        a.paint(&mut c,None);open(&mut a,Target::All);a.paint(&mut c,None);
        assert!(!entries(&a.state,false).is_empty());assert!(!entries(&a.state,true).is_empty());
        let e=entries(&a.state,false)[0].clone();
        act(&mut a,Hit::Favorite(false,e.index));act(&mut a,Hit::Filter(3));
        assert!(choices(&a.state).contains(&(false,e.index)));
        assert!(entries(&a.state,false).iter().all(|e|e.favorite));
        act(&mut a,Hit::Filter(0));
        let channels=a.state.project.channels.len();
        a.key(&Key::Character("unlikely plugin xyz".into()),false,false);
        assert!(choices(&a.state).is_empty());a.key(&Key::Named(NamedKey::Enter),false,false);
        assert_eq!(a.state.project.channels.len(),channels);
        assert!(!a.typing_key(winit::keyboard::KeyCode::KeyZ,true,false,false));
        act(&mut a,Hit::Clear);
        for (w,h) in [(1280,900),(800,600)]{
            let mut c=Canvas::new(w,h,1.).unwrap();a.paint(&mut c,None);
            let (r,_,_)=geometry(&a.state);
            for hit in a.hits.iter().filter(|h|matches!(h.action,Action::PluginPicker(Hit::Tile(..)))){
                assert!(hit.rect.x>=r.x && hit.rect.right()<=r.right() && hit.rect.bottom()<r.bottom()-25.);
            }
            if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join(format!("plugin-picker-{w}.png"))).unwrap();}
        }
        a.key(&Key::Named(NamedKey::Escape),false,false);assert!(!a.state.plugin_picker.open);
    }
    #[test]
    fn plugin_picker_artwork_and_context_fill_the_view(){
        let mut a=app();let mut c=Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);
        open(&mut a,Target::All);
        assert!(a.state.plugdb.lists.gens.iter().chain(&a.state.plugdb.lists.fx).all(|e|a.state.plugin_picker.images.contains_key(&e.uid)),"all built-ins have real previews");
        let hybrid_cols=geometry(&a.state).1;
        for (target,fx) in [(Target::Gen(None),false),(Target::Replace(0),false),(Target::Fx(Some(2)),true)]{
            open(&mut a,target);assert_eq!(panes(&a.state),vec![fx]);assert!(geometry(&a.state).1>hybrid_cols);
            act(&mut a,Hit::Filter(if fx{1}else{2}));assert_eq!(panes(&a.state),vec![fx],"cannot cross instrument/effect destinations");
            a.paint(&mut c,None);
            assert!(a.hits.iter().filter_map(|h|if let Action::PluginPicker(Hit::Tile(kind,_))=h.action{Some(kind)}else{None}).all(|kind|kind==fx));
            if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join(if fx{"picker-effects.png"}else{"picker-instruments.png"})).unwrap();}
        }
        close(&mut a);a.state.z=vec![crate::app::Win::Mixer];a.paint(&mut c,None);
        let r=a.hits.iter().find(|h|h.win==Some(crate::app::Win::Mixer)).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.key(&Key::Named(NamedKey::F8),false,false);assert_eq!(a.state.plugin_picker.target,Target::Fx(None));
        close(&mut a);a.state.z=vec![crate::app::Win::ChannelRack];a.paint(&mut c,None);
        let r=a.hits.iter().find(|h|h.win==Some(crate::app::Win::ChannelRack)).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.key(&Key::Named(NamedKey::F8),false,false);assert_eq!(a.state.plugin_picker.target,Target::Gen(None));
        close(&mut a);a.act(Action::PickerOpen);assert_eq!(panes(&a.state),vec![true,false]);
    }
    #[test]
    fn plugin_picker_empty_slot_right_click_and_chrome(){
        let mut a=app();let mut c=Canvas::new(1280,900,1.).unwrap();
        a.state.z=vec![crate::app::Win::Mixer];a.paint(&mut c,None);
        let r=a.hits.iter().find(|h|h.action==Action::Mix(crate::mixer::MHit::SlotName(0))).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.mouse_down_right();a.mouse_up();
        assert!(a.state.plugin_picker.open);assert_eq!(a.state.plugin_picker.target,Target::Fx(Some(0)));
        a.paint(&mut c,None);
        let r=a.hits.iter().find(|h|h.action==Action::Close).unwrap().rect;
        a.mouse_move(r.cx(),r.cy());a.mouse_down();a.mouse_up();
        assert!(a.want_close);assert!(!a.state.plugin_picker.open);
    }
    #[test]
    fn plugin_picker_click_drag_and_context(){
        let mut a=app();let mut c=Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);
        open(&mut a,Target::Gen(None));a.paint(&mut c,None);assert!(entries(&a.state,true).is_empty());
        let tile=a.hits.iter().find(|h|matches!(h.action,Action::PluginPicker(Hit::Tile(false,_)))).unwrap().rect;
        let before=a.state.project.channels.len();a.mouse_move(tile.cx(),tile.cy());a.mouse_down();a.mouse_up();
        assert!(!a.state.plugin_picker.open);assert_eq!(a.state.project.channels.len(),before+1);
        open(&mut a,Target::All);a.paint(&mut c,None);
        let tile=a.hits.iter().find(|h|matches!(h.action,Action::PluginPicker(Hit::Tile(false,_)))).unwrap().rect;
        a.mouse_move(tile.cx(),tile.cy());a.mouse_down();a.mouse_move(tile.cx()+12.,tile.cy()+12.);
        assert!(!a.state.plugin_picker.open);assert!(matches!(a.drag,Some(Drag::Browser{started:true,..})));
        a.paint(&mut c,None);a.mouse_move(-20.,-20.);a.mouse_up();assert_eq!(a.state.project.channels.len(),before+1,"invalid drop does not add");
        open(&mut a,Target::Fx(Some(3)));assert!(entries(&a.state,false).is_empty());
        let effect=entries(&a.state,true)[0].index;
        act(&mut a,Hit::Tile(true,effect));assert!(a.state.project.mixer.iter().find(|m|m.index==a.state.selected_mixer).unwrap().slots[3].is_some());
    }
}
