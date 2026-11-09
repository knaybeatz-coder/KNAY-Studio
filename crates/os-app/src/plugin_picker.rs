// SPDX-License-Identifier: GPL-3.0-or-later
//! Visual, searchable picker over the existing installed-plugin database.
use crate::{app::{Action,App,State,Ui,Drag},plugin_menu::{Target,PlugCmd},plugin_db::Entry};
use os_ui::{Rect,Font};
use winit::keyboard::{Key,NamedKey};
#[path = "plugin_picker_chains.rs"]
pub(crate) mod chains;
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Hit { BulkFavorite(bool),BulkBlacklist(bool),Background,Close,Minimize,Sort,SortBy(u8),Choose,Filter(u8),Favorite(bool,usize),Tile(bool,usize),Manage,Clear,Picture(bool,usize),Page(i8),Generate,CancelGeneration,PreviewReport,
    ChainGroup(usize), ChainTile(usize), ChainStar(usize), ChainNew, ChainCreate, ChainEdit, ChainDuplicate, ChainDelete,
    ChainSave, ChainCancel, ChainMove(usize, i8), ChainRemove(usize), ChainImport, ChainExport, ChainCategory,
    ChainMovePanel, ChainResizePanel,
}
#[derive(Clone,Debug)]
pub struct View {
    pub open:bool, pub target:Target, pub query:String, pub filter:u8,
    pub minimized:bool, pub sort_open:bool, pub last_click:Option<((bool,usize),std::time::Instant)>, pub sort:u8, pub preview_revision:u64,
    pub images:crate::plugin_artwork::Images, pub hovered:Option<(bool,usize)>,
    pub multi:Vec<(bool,usize)>,pub anchor:Option<usize>,
    pub first:usize, pub selected:usize, pub pending:Option<(Hit,(f32,f32))>,
    chains: chains::View,
}
impl Default for View {fn default()->Self{Self{multi:Vec::new(),anchor:None,minimized:false,sort_open:false,last_click:None,sort:0,preview_revision:0,images:Default::default(),hovered:None,open:false,target:Target::All,query:String::new(),filter:0,first:0,selected:0,pending:None,chains:Default::default()}}}
fn entries(s:&State,effect:bool)->Vec<&Entry>{
    let v=&s.plugin_picker;
    if (effect && matches!(v.target,Target::Gen(_)|Target::Replace(_)|Target::Stack(_))) || (!effect && matches!(v.target,Target::Fx(_)|Target::ClipFx(..)))
        || (v.filter==1 && effect) || (v.filter==2 && !effect) {return Vec::new();}
    let q=v.query.to_lowercase();
    let lists=if v.filter==5{&s.plugdb.blacklisted}else{&s.plugdb.lists};let list=if effect{&lists.fx}else{&lists.gens};
    let mut rows:Vec<_>=list.iter().filter(|e| (v.filter!=3 || e.favorite) && (v.filter!=4 || !e.builtin) && q.split_whitespace().all(|w|
        format!("{} {} {} {}",e.label,e.vendor,e.category,crate::plugin_db::format_label(e.format)).to_lowercase().contains(w))).collect();
    rows.sort_by_cached_key(|e| (match v.sort {1=>e.vendor.to_lowercase(),2=>e.category.to_lowercase(),_=>String::new()},e.label.to_lowercase()));
    rows
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
    let rw=(w-36.).clamp(200.,1480.);let rh=(h-96.).clamp(180.,880.);
    let mut r=Rect::new((w-rw)/2.,(h-rh)/2.+12.,rw,rh);
    if s.plugin_picker.filter == chains::FILTER {
        r = chains::panel_rect(s, s.plugin_picker.chains.rect.unwrap_or(r));
        let (_, cols, rows) = chains::layout(r);
        return (r, cols, rows);
    }
    let cols=((r.w-40.)/panes(s).len() as f32/212.).floor().max(1.) as usize;
    let count=panes(s).into_iter().map(|fx| entries(s,fx).len()).max().unwrap_or(0);
    let rows=((r.h-273.)/138.).floor().max(1.) as usize;
    let rows=rows.min(count.div_ceil(cols).max(2));
    r.h=r.h.min(273.+138.*rows as f32);
    r.y=(h-r.h)/2.+12.;
    (r,cols,rows)
}
fn max_first(s:&State)->usize{let (_,cols,rows)=geometry(s);entries(s,true).len().max(entries(s,false).len()).div_ceil(cols).div_ceil(rows).saturating_sub(1)*rows}
pub fn open(app:&mut App,target:Target){
    app.state.menu_open=None;app.state.playlist.popup=None;app.state.plugin_picker=View{open:true,target,sort:crate::prefs::text("picker_sort").and_then(|v|v.parse::<u8>().ok()).unwrap_or(0).min(2),..Default::default()};
    app.state.plugin_picker.images=crate::plugin_artwork::load(app.state.plugdb.lists.fx.iter().chain(&app.state.plugdb.lists.gens).chain(&app.state.plugdb.blacklisted.fx).chain(&app.state.plugdb.blacklisted.gens).cloned());
    if chains::allowed(target) { app.state.plugin_picker.chains = chains::View::load(); }
    app.pressed=None;app.invalidate();
}
pub fn close(app:&mut App){
    app.state.plugin_picker.open=false;app.state.plugin_picker.pending=None;app.pressed=None;
    app.hits.retain(|h|!matches!(h.action,Action::PluginPicker(_)));app.invalidate();
}
pub fn floating(s: &State) -> bool {
    s.plugin_picker.open && s.plugin_picker.filter == chains::FILTER
}
fn choose(app:&mut App,fx:bool,index:usize){
    if app.state.plugin_picker.filter==5||!choices(&app.state).contains(&(fx,index)){return;}
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
    if chains::editing(&app.state.plugin_picker) && !matches!(hit,
        Hit::ChainSave | Hit::ChainCancel | Hit::ChainMove(..) | Hit::ChainRemove(_) | Hit::ChainCategory | Hit::Close | Hit::Minimize) {
        return;
    }
    match hit{
        Hit::ChainCreate => {
            let indices = choices(&app.state).into_iter().filter_map(|(fx, index)|
                (fx && app.state.plugin_picker.multi.contains(&(fx, index))).then_some(index)).collect();
            chains::create(app, indices);
        }
        Hit::ChainGroup(_) | Hit::ChainTile(_) | Hit::ChainStar(_) | Hit::ChainNew | Hit::ChainEdit
        | Hit::ChainDuplicate | Hit::ChainDelete | Hit::ChainSave | Hit::ChainCancel
        | Hit::ChainMove(..) | Hit::ChainRemove(_) | Hit::ChainImport | Hit::ChainExport | Hit::ChainCategory => chains::act(app, hit),
        Hit::BulkFavorite(on)|Hit::BulkBlacklist(on)=>{
            let picked=app.state.plugin_picker.multi.clone();if picked.is_empty(){return;}
            let uids:Vec<_>=picked.into_iter().filter_map(|(fx,i)|{let p=if fx{app.plugins.effects.get(i)}else{app.plugins.available.get(i)}?;Some(p.desc.uid())}).collect();
            for uid in uids{let p=&mut app.state.plugdb.prefs;if matches!(hit,Hit::BulkFavorite(_)){if p.is_favorite(&uid)!=on{p.toggle_favorite(&uid);}}else if p.is_hidden(&uid)!=on{p.toggle_hidden(&uid);}}
            app.state.plugdb.prefs.save();app.state.plugdb.rebuild(&app.plugins);let v=&mut app.state.plugin_picker;v.multi.clear();v.anchor=None;v.last_click=None;v.selected=0;v.first=0;
        },
        Hit::Generate=>crate::plugin_previews::start(app),
        Hit::CancelGeneration=>crate::plugin_previews::cancel(),
        Hit::PreviewReport=>crate::plugin_previews::report(app),
        Hit::Picture(fx,index)=>{let lists=if app.state.plugin_picker.filter==5{&app.state.plugdb.blacklisted}else{&app.state.plugdb.lists};let list=if fx{&lists.fx}else{&lists.gens};if let Some(uid)=list.iter().find(|e|e.index==index).map(|e|e.uid.clone()){crate::plugin_artwork::choose(app,uid);}},
        Hit::Page(direction)=>{let (_,_,rows)=geometry(&app.state);let max=max_first(&app.state);let v=&mut app.state.plugin_picker;v.first=v.first.saturating_add_signed(direction as isize*rows as isize).min(max);},
        Hit::Sort=>app.state.plugin_picker.sort_open=!app.state.plugin_picker.sort_open,
        Hit::Choose if app.state.plugin_picker.filter == chains::FILTER => chains::choose(app),
        Hit::Choose=>{if let Some(&(fx,index))=choices(&app.state).get(app.state.plugin_picker.selected){choose(app,fx,index);}}
        Hit::SortBy(order)=>{let v=&mut app.state.plugin_picker;v.multi.clear();v.anchor=None;v.sort_open=false;v.sort=order.min(2);crate::prefs::set_text("picker_sort",&v.sort.to_string());v.first=0;v.selected=0;v.hovered=None;v.chains.first=0;v.chains.selected=0;}
        Hit::Close=>close(app),
        Hit::Minimize=>{app.state.plugin_picker.minimized=!app.state.plugin_picker.minimized;app.state.plugin_picker.sort_open=false;},
        Hit::Filter(f)=>{let v=&mut app.state.plugin_picker;if v.target==Target::All || f==0 || f==3 || f==4||f==5 || (f==chains::FILTER && chains::allowed(v.target)){v.multi.clear();v.anchor=None;v.filter=f;v.first=0;v.selected=0;v.hovered=None;v.chains.first=0;v.chains.selected=0;v.chains.focused=true;}},
        Hit::Clear=>{let v=&mut app.state.plugin_picker;v.multi.clear();v.anchor=None;v.query.clear();v.first=0;v.selected=0;v.chains.first=0;v.chains.selected=0;},
        Hit::Favorite(fx,i)=>{
            let list=if fx{&app.plugins.effects}else{&app.plugins.available};
            if let Some(uid)=list.get(i).map(|e|e.desc.uid()){
                app.state.plugdb.prefs.toggle_favorite(&uid);app.state.plugdb.prefs.save();app.state.plugdb.rebuild(&app.plugins);
            }
            app.state.plugin_picker.first=app.state.plugin_picker.first.min(max_first(&app.state));
        },
        Hit::Tile(fx,i)=>{
            if let Some(pos)=choices(&app.state).iter().position(|p|*p==(fx,i)) {
                let all=choices(&app.state);let v=&mut app.state.plugin_picker;
                if app.ctrl{if let Some(k)=v.multi.iter().position(|p|*p==(fx,i)){v.multi.remove(k);}else{v.multi.push((fx,i));}v.anchor=Some(pos);v.selected=pos;v.last_click=None;app.invalidate();return;}
                if app.shift{let anchor=v.anchor.unwrap_or(v.selected).min(all.len().saturating_sub(1));for &item in &all[anchor.min(pos)..=anchor.max(pos)]{if !v.multi.contains(&item){v.multi.push(item);}}v.selected=pos;v.last_click=None;app.invalidate();return;}
                v.multi=vec![(fx,i)];v.anchor=Some(pos);
                let double=v.last_click.is_some_and(|(entry,when)|entry==(fx,i)&&when.elapsed().as_millis()<400);
                v.selected=pos;v.last_click=Some(((fx,i),std::time::Instant::now()));
                if double {choose(app,fx,i);}
            }
        },
        Hit::Manage=>{close(app);app.state.message=crate::plugin_menu::run(app,PlugCmd::Manage);},
        Hit::Background | Hit::ChainMovePanel | Hit::ChainResizePanel => {},
    }
    app.invalidate();
}
pub fn mouse_down(app:&mut App)->bool{
    if app.modal_open() { return false; }
    if floating(&app.state) && (app.state.playlist.popup.is_some() || app.state.menu_open.is_some()) { return false; }
    if !app.state.plugin_picker.open{return false;}
    if app.state.plugin_picker.minimized && !app.top_hit(app.mouse.0,app.mouse.1).is_some_and(|h|matches!(h.action,Action::PluginPicker(_))){return false;}
    let floating = app.state.plugin_picker.filter == chains::FILTER && !app.state.plugin_picker.minimized;
    let (rect, _, _) = geometry(&app.state);
    if floating {
        let inside = rect.contains(app.mouse.0, app.mouse.1);
        app.state.plugin_picker.chains.focused = inside;
        if !inside { return false; }
    }
    if app.hits.iter().any(|h|matches!(h.action,Action::Close|Action::Minimize|Action::Maximize) && h.rect.contains(app.mouse.0,app.mouse.1)) {close(app);return false;}
    let hit=app.top_hit(app.mouse.0,app.mouse.1).and_then(|h|if let Action::PluginPicker(a)=h.action{Some(a)}else{None}).unwrap_or(Hit::Background);
    if matches!(hit, Hit::ChainMovePanel | Hit::ChainResizePanel) {
        app.state.plugin_picker.chains.drag_start = Some(rect);
    }
    app.state.plugin_picker.pending=Some((hit,app.mouse));app.invalidate();true
}
pub fn mouse_up(app:&mut App)->bool{
    if app.modal_open() { return false; }
    if floating(&app.state) && (app.state.playlist.popup.is_some() || app.state.menu_open.is_some()) { return false; }
    if !app.state.plugin_picker.open{return false;}
    if app.state.plugin_picker.minimized && app.state.plugin_picker.pending.is_none(){return false;}
    if app.state.plugin_picker.filter == chains::FILTER && app.state.plugin_picker.pending.is_none() { return false; }
    if app.state.plugin_picker.chains.drag_start.take().is_some() {
        if let Some(r) = app.state.plugin_picker.chains.rect {
            let text = serde_json::to_string(&[r.x, r.y, r.w, r.h]).unwrap();
            crate::prefs::set_text("effect_chain_panel", &text);
        }
        app.state.plugin_picker.pending = None;
        app.resize_cursor = false;
        app.invalidate();
        return true;
    }
    if let Some((hit,_))=app.state.plugin_picker.pending.take(){
        if app.top_hit(app.mouse.0,app.mouse.1).is_some_and(|h|h.action==Action::PluginPicker(hit)){act(app,hit);}
    }
    true
}
pub fn mouse_move(app:&mut App,x:f32,y:f32)->bool{
    if app.modal_open() { return false; }
    if floating(&app.state) && (app.state.playlist.popup.is_some() || app.state.menu_open.is_some()) { return false; }
    if !app.state.plugin_picker.open || app.state.plugin_picker.minimized{return false;}
    if app.state.plugin_picker.filter == chains::FILTER {
        if let Some(start) = app.state.plugin_picker.chains.drag_start
            && let Some((hit, (x0, y0))) = app.state.plugin_picker.pending {
            let (dx, dy) = (x - x0, y - y0);
            let next = if hit == Hit::ChainResizePanel {
                Rect::new(start.x, start.y, start.w + dx, start.h + dy)
            } else {
                Rect::new(start.x + dx, start.y + dy, start.w, start.h)
            };
            app.state.plugin_picker.chains.rect = Some(chains::panel_rect(&app.state, next));
            app.mouse = (x, y);
            app.invalidate();
            return true;
        }
        if app.state.plugin_picker.pending.is_none() && !geometry(&app.state).0.contains(x, y) {
            return false;
        }
    }
    app.mouse=(x,y);
    let hit=app.top_hit(x,y).map(|h|(h.action.clone(),h.hint.clone()));app.hover=hit.as_ref().map(|h|h.0.clone());
    app.state.plugin_picker.hovered=None;
    if let Some((action,hint))=hit{if let Action::PluginPicker(Hit::Tile(fx,i)|Hit::Favorite(fx,i))=action{
        app.state.plugin_picker.hovered=Some((fx,i));

    }app.hint=hint;}
    app.resize_cursor = matches!(app.hover, Some(Action::PluginPicker(Hit::ChainResizePanel)));
    if let Some((Hit::Tile(fx,i),m0))=app.state.plugin_picker.pending{
        if !app.ctrl&&!app.shift&&app.state.plugin_picker.filter!=5&&(x-m0.0).hypot(y-m0.1)>5.{
            let list=if fx{&app.state.plugdb.lists.fx}else{&app.state.plugdb.lists.gens};
            if let Some(label)=list.iter().find(|e|e.index==i).map(|e|e.label.clone()){
                let group:Vec<_>=if fx&&app.state.plugin_picker.multi.contains(&(fx,i)){choices(&app.state).into_iter().filter(|p|p.0&&app.state.plugin_picker.multi.contains(p)).map(|p|p.1).collect()}else{vec![i]};
                let label=if fx&&group.len()>1{format!("{} effects",group.len())}else{label};
                close(app);
                let item=if fx&&group.len()>1{crate::views::browser::Item::Effects(group)}else if fx{crate::views::browser::Item::Effect(i)}else{crate::views::browser::Item::Generator(i)};
                app.drag=Some(Drag::Browser{item,label:label.clone(),m0,started:true});
                app.state.browser_drag=Some((label,x,y));
            }
        }
    }
    app.invalidate();true
}
pub fn wheel(app:&mut App,dy:f32)->bool{
    if app.modal_open() { return false; }
    if floating(&app.state) && (app.state.playlist.popup.is_some() || app.state.menu_open.is_some()) { return false; }
    if !app.state.plugin_picker.open || app.state.plugin_picker.minimized{return false;}
    if app.state.plugin_picker.filter == chains::FILTER {
        if !geometry(&app.state).0.contains(app.mouse.0, app.mouse.1) { return false; }
        let (_, cols, rows) = geometry(&app.state);
        chains::scroll(&mut app.state.plugin_picker, if dy > 0. { -1 } else if dy < 0. { 1 } else { 0 }, cols, rows);
        app.invalidate();
        return true;
    }
    let max=max_first(&app.state);let v=&mut app.state.plugin_picker;
    v.first=v.first.saturating_add_signed(if dy>0.{-1}else if dy<0.{1}else{0}).min(max);app.invalidate();true
}
pub fn key(app:&mut App,key:&Key,ctrl:bool,alt:bool)->bool{
    if app.modal_open() { return false; }
    if floating(&app.state) && (app.state.playlist.popup.is_some() || app.state.menu_open.is_some()) { return false; }
    if !app.state.plugin_picker.open || app.state.plugin_picker.minimized{return false;}
    if app.state.plugin_picker.filter == chains::FILTER && !app.state.plugin_picker.chains.focused { return false; }
    if chains::editing(&app.state.plugin_picker) {
        match key {
            Key::Named(NamedKey::Escape) => act(app, Hit::ChainCancel),
            Key::Named(NamedKey::Enter) => act(app, Hit::ChainSave),
            Key::Named(NamedKey::Backspace) => chains::name_input(&mut app.state.plugin_picker, None),
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("a") => chains::clear_name(&mut app.state.plugin_picker),
            Key::Character(c) if !ctrl && !alt => chains::name_input(&mut app.state.plugin_picker, Some(c)),
            _ => {}
        }
        app.invalidate();
        return true;
    }
    match key{
        Key::Named(NamedKey::Escape)=>close(app),
        Key::Named(NamedKey::F8) if crate::shortcuts::f8(app.shift,ctrl,alt)==Some(crate::shortcuts::F8::PluginPicker)=>close(app),
        Key::Named(NamedKey::Enter)=>act(app, Hit::Choose),
        Key::Named(NamedKey::ArrowDown|NamedKey::ArrowUp|NamedKey::ArrowLeft|NamedKey::ArrowRight)=>{
            app.state.plugin_picker.hovered=None;
            let all=choices(&app.state);let (_,cols,rows)=geometry(&app.state);
            let step=match key{Key::Named(NamedKey::ArrowUp)=>-(cols as isize),Key::Named(NamedKey::ArrowDown)=>cols as isize,Key::Named(NamedKey::ArrowLeft)=>-1,_=>1};
            if app.state.plugin_picker.filter == chains::FILTER {
                chains::navigate(&mut app.state.plugin_picker, step, cols, rows);
                app.invalidate();
                return true;
            }
            let sel=app.state.plugin_picker.selected.saturating_add_signed(step).min(all.len().saturating_sub(1));
            app.state.plugin_picker.selected=sel;app.state.plugin_picker.multi=all.get(sel).copied().into_iter().collect();app.state.plugin_picker.anchor=Some(sel);
            if let Some(&(fx,i))=all.get(sel){let row=entries(&app.state,fx).iter().position(|e|e.index==i).unwrap_or(0)/cols;
                let v=&mut app.state.plugin_picker;if row<v.first{v.first=row;}else if row>=v.first+rows{v.first=row+1-rows;}}
        },
        Key::Named(NamedKey::Backspace)=>{let v=&mut app.state.plugin_picker;v.multi.clear();v.anchor=None;v.query.pop();v.first=0;v.selected=0;v.chains.first=0;v.chains.selected=0;},
        Key::Character(c) if ctrl && c.eq_ignore_ascii_case("a")=>{app.state.plugin_picker.multi=choices(&app.state);},
        Key::Character(c) if !ctrl && !alt=>{let v=&mut app.state.plugin_picker;v.multi.clear();v.anchor=None;for ch in c.chars().filter(|c|!c.is_control()){if v.query.chars().count()<128{v.query.push(ch);}}v.first=0;v.selected=0;v.chains.first=0;v.chains.selected=0;},
        _=>{},
    }
    app.invalidate();true
}
pub fn paint(ui:&mut Ui,s:&State){
    let v=&s.plugin_picker;if !v.open{return;}
    ui.win=None;
    if v.minimized {
        let b=Rect::new(16.,s.screen.1-40.,158.,28.);
        ui.hit(b,Action::PluginPicker(Hit::Minimize),"Restore plugin picker");
        ui.c.fill_rrect(b,6.,ui.t.button);
        ui.c.text_center(Font::light(13,0),b,ui.t.text,"Restore plugin picker");
        return;
    }
    let (r,cols,rows)=geometry(s);let panes=panes(s);
    let chain_mode = v.filter == chains::FILTER;
    let backdrop = if chain_mode { r } else { Rect::new(0.,0.,s.screen.0,s.screen.1) };
    ui.hit(backdrop,Action::PluginPicker(Hit::Background),"Plugin picker");
    if chain_mode {
        ui.hit(Rect::new(r.x, r.y, r.w - 88., 49.), Action::PluginPicker(Hit::ChainMovePanel), "Drag to move Effect Chains");
    }
    use os_ui::Color;
    let lemon=Color::rgb(0xf2d451);
    ui.c.fill_rect_gradient(r,Color::rgb(0x202124),Color::rgb(0x111215),14.);
    ui.c.stroke_rrect(r,14.,1.,Color::rgb(0x46474b));
    let title=match v.target{Target::Gen(_)|Target::Replace(_)|Target::Stack(_)=>"Choose an instrument",Target::Fx(_)|Target::ClipFx(..)=>"Choose an effect",Target::All=>"Choose a plugin"};
    let progress=crate::plugin_previews::progress();
    let btn=|ui:&mut Ui,b:Rect,label:&'static str,hit:Hit,on:bool|{
        let hover=ui.hit(b,Action::PluginPicker(hit),label);
        let bg=if on {lemon} else if hover {Color::rgb(0x393a3e)} else {Color::rgb(0x2a2b2e)};
        ui.c.fill_rect_gradient(b,bg.shade(0.08),bg.shade(-0.08),b.h*0.42);
        ui.c.text_center(Font::light(12,0).natural(),b,if on {Color::rgb(0x191910)}else{ui.t.text},label);
    };
    ui.c.fill_circle(r.x+32.,r.y+29.,13.,lemon);
    for i in 0..6 {let a=i as f32*std::f32::consts::TAU/6.;ui.c.line(r.x+32.,r.y+29.,r.x+32.+11.*a.cos(),r.y+29.+11.*a.sin(),1.,Color::rgb(0x212222));}
    ui.c.text_clip(Font::light(19,0).bold(),r.x+55.,r.y+17.,145.,ui.t.text,"Lemon Studio");
    ui.c.text_clip(Font::light(15,0),r.x+203.,r.y+20.,r.w-260.,ui.t.text_dim,title);
    btn(ui,Rect::new(r.right()-81.,r.y+12.,30.,28.),"−",Hit::Minimize,false);
    btn(ui,Rect::new(r.right()-46.,r.y+12.,30.,28.),"×",Hit::Close,false);
    let mut filters:Vec<(u8,&str)>=if v.target==Target::All{vec![(0,"All"),(1,"Instruments"),(2,"Effects"),(3,"Favorites"),(4,"Installed"),(5,"Blacklist")]}else{vec![(0,"All"),(3,"Favorites"),(4,"Installed"),(5,"Blacklist")]};
    if chains::allowed(v.target) { filters.insert(filters.len() - 1, (chains::FILTER, "Effect Chains")); }
    let fw=((r.w-270.) / filters.len() as f32 - 5.).clamp(36., 88.);
    let sw=(r.w-146.-filters.len() as f32*(fw+5.)-38.).max(72.);
    let search=Rect::new(r.x+18.,r.y+54.,sw,33.);
    ui.c.fill_rrect(search,16.,Color::rgb(0x161719));ui.c.stroke_rrect(search,16.,1.,lemon.with_alpha(170));
    ui.c.stroke_circle(search.x+15.,search.cy()-2.,4.,1.2,ui.t.text_dim);
    ui.c.line(search.x+18.,search.cy()+1.,search.x+22.,search.cy()+5.,1.3,ui.t.text_dim);
    ui.c.text_clip(Font::light(13,0),search.x+29.,search.y+8.,search.w-55.,ui.t.text,if v.query.is_empty(){if v.filter==chains::FILTER{"Search chains..."}else{"Search plugins..."}}else{&v.query});
    if !v.query.is_empty(){btn(ui,Rect::new(search.right()-26.,search.y+4.,22.,24.),"×",Hit::Clear,false);}
    for (i,(f,label)) in filters.iter().enumerate(){btn(ui,Rect::new(search.right()+6.+i as f32*(fw+5.),search.y,fw,32.),label,Hit::Filter(*f),v.filter==*f);}
    let sort=Rect::new(r.right()-146.,search.y,128.,32.);
    let sort_labels = if chain_mode { ["Name", "Favorites", "Category"] } else { ["Name", "Vendor", "Category"] };
    let sort_buttons = if chain_mode { ["Sort: Name ▾", "Sort: Favorites ▾", "Sort: Category ▾"] } else { ["Sort: Name ▾", "Sort: Vendor ▾", "Sort: Category ▾"] };
    btn(ui,sort,sort_buttons[v.sort as usize],Hit::Sort,v.sort_open);
    if chain_mode {
        chains::paint(ui, s, r);
        let grip = Rect::new(r.right() - 16., r.bottom() - 16., 16., 16.);
        ui.hit(grip, Action::PluginPicker(Hit::ChainResizePanel), "Drag to resize Effect Chains");
        for offset in [4., 8., 12.] {
            ui.c.line(r.right() - 4. - offset, r.bottom() - 4., r.right() - 4., r.bottom() - 4. - offset, 1., ui.t.text_dim);
        }
        if v.sort_open {
            for (i, name) in sort_labels.into_iter().enumerate() {
                btn(ui, Rect::new(sort.x, sort.bottom()+2.+26.*i as f32, sort.w, 26.), name, Hit::SortBy(i as u8), v.sort as usize==i);
            }
        }
        return;
    }
    let all=choices(s);let selected=all.get(v.selected).copied();
    let width=(r.w-36.-12.*(panes.len()-1) as f32)/panes.len() as f32;
    let cardw=(width-8.*(cols-1) as f32)/cols as f32;
    for (pane,fx) in panes.into_iter().enumerate(){
        let x=r.x+18.+pane as f32*(width+12.);let list=entries(s,fx);
        ui.c.text_clip(Font::new(12).bold(),x,r.y+100.,width,ui.t.text_dim,&format!("{}  ·  {}",if fx{"EFFECTS"}else{"INSTRUMENTS"},list.len()));
        if list.is_empty(){ui.c.text_clip(Font::new(13),x,r.y+146.,width,ui.t.text_dim,"No matching plugins");}
        for (n,e) in list.iter().skip(v.first*cols).take(rows*cols).enumerate(){
            let card=Rect::new(x+(n%cols) as f32*(cardw+8.),r.y+122.+(n/cols) as f32*138.,cardw,130.);
            let hover=ui.hit(card,Action::PluginPicker(Hit::Tile(fx,e.index)),if v.filter==5{format!("{} - select, then Restore to show again",e.label)}else{format!("{} - select, double-click to add, or drag to {}",e.label,if fx{"the mixer"}else{"the channel rack"})});
            let accent=lemon;
            ui.c.fill_rect_gradient(card,Color::rgb(0x2d2e31),Color::rgb(0x202124),9.);
            if hover||v.multi.contains(&(fx,e.index)){ui.c.stroke_rrect(card,9.,if v.multi.contains(&(fx,e.index)){2.}else{0.8},accent);}
            let preview=Rect::new(card.x+10.,card.y+8.,card.w-20.,92.);
            if let Some(p)=v.images.get(&e.uid){crate::plugin_artwork::draw(ui,preview,p);}else{fallback(ui,preview,fx);}
            btn(ui,Rect::new(card.right()-30.,card.y+4.,26.,24.),if e.favorite{"♥"}else{"♡"},Hit::Favorite(fx,e.index),e.favorite);
            ui.c.text_clip(Font::light(14,0).natural().bold(),card.x+8.,card.y+109.,card.w-16.,ui.t.text,&e.label);
        }
    }
    let inspector=Rect::new(r.x+18.,r.bottom()-76.,r.w-36.,66.);
    ui.c.fill_rrect(inspector,6.,ui.t.s(0x111a1e));
    let current=selected;
    if let Some((fx,index))=current{
        if let Some(e)=entries(s,fx).into_iter().find(|e|e.index==index){
            let pic=Rect::new(inspector.x+8.,inspector.y+5.,112.,56.);
            if let Some(p)=v.images.get(&e.uid){crate::plugin_artwork::draw(ui,pic,p);}else{fallback(ui,pic,fx);}
            let x=inspector.x+134.;let w=(inspector.w-390.).max(80.);
            ui.c.text_clip(Font::light(15,0).natural().bold(),x,inspector.y+8.,w,ui.t.text,&e.label);
            ui.c.text_clip(Font::new(11),x,inspector.y+29.,w,ui.t.text_dim,&format!("Category: {}   |   Developer: {}",e.category,e.vendor));
            ui.c.text_clip(Font::new(10),x,inspector.y+47.,w,ui.t.text_dim,if v.filter==5{"Hidden from picker - select and Restore below"}else if progress.summary.is_empty(){"Ctrl/Cmd-click: toggle - Shift-click: range"}else{&progress.summary});
            if v.filter!=5{btn(ui,Rect::new(inspector.right()-186.,inspector.y+17.,100.,32.),"Choose",Hit::Choose,true);}
            btn(ui,Rect::new(inspector.right()-78.,inspector.y+17.,69.,32.),"Close",Hit::Close,false);
        }
    }
    let y=inspector.y-62.;let bulk=inspector.y-31.;
    btn(ui,Rect::new(r.x+18.,bulk,100.,24.),"Favorite +",Hit::BulkFavorite(true),false);
    btn(ui,Rect::new(r.x+122.,bulk,100.,24.),"Favorite −",Hit::BulkFavorite(false),false);
    btn(ui,Rect::new(r.x+226.,bulk,100.,24.),if v.filter==5{"Restore"}else{"Blacklist"},Hit::BulkBlacklist(v.filter!=5),false);
    let save_chain = v.filter != 5 && chains::allowed(v.target) && v.multi.iter().any(|&(fx, _)| fx);
    if save_chain { btn(ui,Rect::new(r.x+330.,bulk,110.,24.),"Save as chain",Hit::ChainCreate,false); }
    let summary = if save_chain { 449. } else { 335. };
    ui.c.text_clip(Font::new(10),r.x+summary,bulk+6.,(r.w-summary-20.).max(0.),ui.t.text_dim,&format!("{} selected · Ctrl/Cmd toggle · Shift range · Ctrl+A all",v.multi.len()));
    if cfg!(windows) {
        btn(ui,Rect::new(r.x+18.,y,133.,24.),if progress.running{"Cancel generation"}else{"Generate previews"},if progress.running{Hit::CancelGeneration}else{Hit::Generate},false);
    }
    btn(ui,Rect::new(r.x+156.,y,112.,24.),"Manage plugins",Hit::Manage,false);
    if let Some((fx,i))=current {
        btn(ui,Rect::new(r.x+273.,y,110.,24.),if progress.failed>0{"Preview report"}else{"Choose picture"},if progress.failed>0{Hit::PreviewReport}else{Hit::Picture(fx,i)},false);
    }
    let max=max_first(s);
    btn(ui,Rect::new(r.right()-142.,y,26.,24.),"<",Hit::Page(-1),false);
    ui.c.text_center(Font::new(11),Rect::new(r.right()-112.,y,56.,24.),ui.t.text_dim,&format!("{}/{}",v.first/rows+1,max.div_ceil(rows)+1));
    btn(ui,Rect::new(r.right()-48.,y,26.,24.),">",Hit::Page(1),false);
    if v.sort_open {
        for (i,name) in ["Name","Vendor","Category"].into_iter().enumerate(){
            btn(ui,Rect::new(sort.x,sort.bottom()+2.+26.*i as f32,sort.w,26.),name,Hit::SortBy(i as u8),v.sort as usize==i);
        }
    }
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

    #[test]fn workflow_batch_plugin_blacklist_and_favorites(){
      let mut a=app();open(&mut a,Target::All);let picked:Vec<_>=choices(&a.state).into_iter().take(3).collect();assert_eq!(picked.len(),3);a.ctrl=true;for &(fx,i) in &picked{act(&mut a,Hit::Tile(fx,i));}a.ctrl=false;assert_eq!(a.state.plugin_picker.multi.len(),3);act(&mut a,Hit::BulkFavorite(true));for(fx,i)in &picked{let list=if *fx{&a.plugins.effects}else{&a.plugins.available};assert!(a.state.plugdb.prefs.is_favorite(&list[*i].desc.uid()));}
      a.state.plugin_picker.multi=picked.clone();act(&mut a,Hit::BulkBlacklist(true));for p in &picked{assert!(!choices(&a.state).contains(p));}act(&mut a,Hit::Filter(5));for p in &picked{assert!(choices(&a.state).contains(p));}a.state.plugin_picker.multi=picked.clone();if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){let mut c=Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);c.pixmap.save_png(std::path::PathBuf::from(dir).join("plugin-blacklist.png")).unwrap();}let count=a.state.project.channels.len();let p=picked[0];choose(&mut a,p.0,p.1);assert_eq!(a.state.project.channels.len(),count);
      a.state.plugin_picker.multi=picked.clone();act(&mut a,Hit::BulkBlacklist(false));act(&mut a,Hit::Filter(0));for p in picked{assert!(choices(&a.state).contains(&p));}
      let bytes=serde_json::to_vec(&a.state.plugdb.prefs).unwrap();let back:crate::plugin_db::Prefs=serde_json::from_slice(&bytes).unwrap();assert_eq!(back,a.state.plugdb.prefs);
    }
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
        assert!(a.state.plugin_picker.open);act(&mut a,Hit::Choose);assert!(!a.state.plugin_picker.open);assert_eq!(a.state.project.channels.len(),before+1);
        open(&mut a,Target::All);a.paint(&mut c,None);
        let tile=a.hits.iter().find(|h|matches!(h.action,Action::PluginPicker(Hit::Tile(false,_)))).unwrap().rect;
        a.mouse_move(tile.cx(),tile.cy());a.mouse_down();a.mouse_move(tile.cx()+12.,tile.cy()+12.);
        assert!(!a.state.plugin_picker.open);assert!(matches!(a.drag,Some(Drag::Browser{started:true,..})));
        a.paint(&mut c,None);a.mouse_move(-20.,-20.);a.mouse_up();assert_eq!(a.state.project.channels.len(),before+1,"invalid drop does not add");
        open(&mut a,Target::Fx(Some(3)));assert!(entries(&a.state,false).is_empty());
        let effect=entries(&a.state,true)[0].index;
        act(&mut a,Hit::Tile(true,effect));act(&mut a,Hit::Choose);assert!(a.state.project.mixer.iter().find(|m|m.index==a.state.selected_mixer).unwrap().slots[3].is_some());
    }
}
