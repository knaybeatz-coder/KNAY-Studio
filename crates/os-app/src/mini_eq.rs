//! The mixer's compact view of its dedicated Lemon Parametric EQ instance.
use crate::app::{Action, App, State, Ui};
use crate::mixer::{self, MHit};
use crate::plugins::{fx_key, EQ_FILE_NAME, LEMON_EQ};
use lemon_parametric_eq_2::{analyzer::{Analyzer, Look}, params};
use os_engine::effects::eq::{self, Settings};
use os_ui::{Color, Font, Rect};

pub const SLOT: u8 = 10;
const N: usize = 4096;
const COLORS: [u32;7] = [0xee7771,0xeaa65b,0xd9cb66,0x88c783,0x69bac9,0x8f9ede,0xc893d0];

pub struct Scope {
    pub track: u32,
    pub rate: u32,
    samples: [f32;N],
    pos: usize,
    at: Option<std::time::Instant>,
}
impl Default for Scope {
    fn default()->Self {Self{track:u32::MAX,rate:48000,samples:[0.;N],pos:0,at:None}}
}
impl Scope {
    pub fn push(&mut self,track:u32,rate:u32,stereo:&[f32]) {
        if self.track!=track || self.rate!=rate {self.samples.fill(0.);self.pos=0;}
        self.track=track;self.rate=rate;self.at=Some(std::time::Instant::now());
        for frame in stereo.chunks_exact(2) {
            self.samples[self.pos]=(frame[0]+frame[1])*0.5;
            self.pos=(self.pos+1)%N;
        }
    }
    pub fn latest(&self)->Vec<f32> {
        if self.at.is_none_or(|at|at.elapsed().as_millis()>200) {return vec![0.;N];}
        (0..N).map(|i|self.samples[(self.pos+i)%N]).collect()
    }
}
#[derive(Default)]
pub struct View {
    pub analyzer: Analyzer,
    pub track: Option<u16>,
    pub rate: u32,
}
pub fn settings(s:&State,t:u16)->Settings {
    let state=mixer::track(s,t).and_then(|m|m.slots[SLOT as usize].as_ref());
    match state {Some(p)=>params::settings_at(&p.state,1),None=>params::settings_at(&params::default_state(),1)}
}
/// Instantiate lazily, in the reserved EQ position, preserving all ten user slots.
pub fn ensure(app:&mut App,t:u16)->Option<u8> {
    let key=fx_key(t,SLOT);
    if app.plugins.hosted.contains_key(&key) {return Some(SLOT);}
    let index=app.plugins.effect_index(LEMON_EQ)?;
    let state=mixer::track(&app.state,t)?.slots[SLOT as usize].as_ref().map(|p|p.state.clone()).unwrap_or_else(params::default_state);
    if let Err(e)=app.plugins.create_effect(t,SLOT,index,Some(&params::plugin_state_from_fl(&state))) {
        app.state.message=Some(format!("EQ: {e}"));return None;
    }
    mixer::track_mut(&mut app.state,t)?.slots[SLOT as usize]=Some(os_model::SlotPlugin{name:EQ_FILE_NAME.into(),state});
    app.plugins.added(key);
    app.transport.set_effects(app.plugins.effects());
    app.state.piano.dirty=true;
    app.params_tick();
    Some(SLOT)
}
pub fn open(app:&mut App) {
    let t=app.state.selected_mixer;
    if let Some(k)=ensure(app,t) {app.open_fx(t,k);}
}
pub fn snapshot(app:&mut App,t:u16) {
    let key=fx_key(t,SLOT);
    let snaps=(0..14).filter_map(|i|app.state.params.lists.get(&key).and_then(|l|l.params.get(i)).and_then(|p|app.plugins.param_value(key,p.id)).map(|v|crate::history::Snap::Param(key,i,v))).collect();
    crate::history::record(&mut app.state,"Mixer parametric EQ",crate::history::Snap::Several(snaps));
}
pub fn drag(app:&mut App,t:u16,band:u8,g:Rect,(x,y):(f32,f32)) {
    if band>=7 {return;}
    if ensure(app,t).is_none(){return;}
    let key=fx_key(t,SLOT);
    let hz=mixer::eq_graph_pos(g,x) as f64;
    let db=mixer::eq_graph_db(g,y) as f64;
    app.plugins.set_param_named(key,&format!("Band {} freq",band+1),hz);
    app.plugins.set_param_named(key,&format!("Band {} level",band+1),(db+18.)/36.);
    // Flush queued CLAP edits on the UI thread even when transport is stopped.
    if let Some(h)=app.plugins.hosted.get(&key) {if let Ok(mut p)=h.plugin.lock() {p.idle();}}
    app.plugins.mark_edited(key);
    sync(app,t);
}
/// Match the full EQ's wheel gesture: five percent width per notch, only over a band token.
pub fn wheel(app:&mut App,g:Rect,x:f32,y:f32,notches:f32)->bool {
    let t=app.state.selected_mixer;
    let settings=settings(&app.state,t);
    let band=settings.bands.iter().enumerate().filter_map(|(i,b)| {
        let bx=g.x+1.+b.freq as f32*(g.w-2.);
        let by=g.cy()-b.db as f32/18.*(g.h/2.-4.);
        let distance=(x-bx).hypot(y-by);
        (distance<=9.).then_some((i,distance))
    }).min_by(|a,b|a.1.total_cmp(&b.1)).map(|b|b.0);
    let Some(band)=band else {return false;};
    if notches==0. {return true;}
    if ensure(app,t).is_none() {return true;}
    let before=settings.bands[band].width;
    let after=(before+notches as f64*0.05).clamp(0.,1.);
    let key=fx_key(t,SLOT);
    if after!=before {
        crate::history::param(&mut app.state,key,14+band,Some(before),after);
        app.plugins.set_param_named(key,&format!("Band {} width",band+1),after);
        if let Some(h)=app.plugins.hosted.get(&key) {if let Ok(mut p)=h.plugin.lock() {p.idle();}}
        app.plugins.mark_edited(key);
        sync(app,t);
    }
    app.hint=std::borrow::Cow::Owned(format!("Band {} width: {:.0}%",band+1,after*100.));
    true
}

fn sync(app:&mut App,t:u16)->bool {
    let Some(state)=app.plugins.eq_file_state(fx_key(t,SLOT)) else {return false;};
    let Some(p)=mixer::track_mut(&mut app.state,t).and_then(|m|m.slots[SLOT as usize].as_mut()) else {return false;};
    if p.state==state {return false;}
    p.state=state;
    true
}
pub fn refresh(app:&mut App)->bool {
    let t=app.state.selected_mixer;
    let changed=sync(app,t);
    app.transport.select_eq_scope(Some(t));
    let (samples,rate)=app.transport.eq_scope(t).unwrap_or_else(||(vec![0.;N],app.transport.sample_rate().max(44100)));
    let v=&mut app.state.mixer.mini_eq;
    if v.track!=Some(t) {v.analyzer=Analyzer::default();v.track=Some(t);}
    v.rate=rate;
    let old=v.analyzer.cols.clone();
    v.analyzer.update(&samples,rate as f32,208,&Look{precision:1,slope:0,smoothing:2,average:false});
    changed || old!=v.analyzer.cols
}
pub fn position(app:&mut App) {
    let t=app.state.selected_mixer;
    let pre=mixer::track(&app.state,t).is_some_and(|m|m.parametric_pre);
    crate::history::record(&mut app.state,"EQ Pre/Post",crate::history::Snap::EqPosition(t,pre));
    if let Some(m)=mixer::track_mut(&mut app.state,t) {m.parametric_pre=!pre;}
    app.state.piano.dirty=true;
    app.invalidate();
}
pub fn draw(ui:&mut Ui,s:&State,r:Rect) {
    let t=s.selected_mixer;
    let m=mixer::track(s,t);
    let pre=m.is_some_and(|m|m.parametric_pre);
    let button=Rect::new(r.right()-61.,r.y,61.,20.);
    let hov=ui.hit(button,Action::Mix(MHit::OpenEq),"Open the full Lemon Parametric EQ — same settings");
    ui.c.fill_rrect(button,3.,if hov{ui.t.button_hi}else{ui.t.button});
    ui.c.text_center(Font::new(11),button,ui.t.text,"Open EQ");
    let pos=Rect::new(r.x,r.y,49.,20.);
    let hov=ui.hit(pos,Action::Mix(MHit::EqPosition),if pre{"EQ before all plugin slots. Click for Post."}else{"EQ after all plugin slots. Click for Pre."});
    ui.c.fill_rrect(pos,3.,if hov{ui.t.button_hi}else{ui.t.button});
    ui.c.text_center(Font::new(11),pos,ui.t.text,if pre{"Pre"}else{"Post"});
    ui.c.text(Font::new(10),r.x+55.,r.y+4.,ui.t.text,"EQ");
    if r.h<55. {return;}
    let g=Rect::new(r.x,r.y+24.,r.w,(r.h-38.).max(20.));
    ui.hit(g,Action::Mix(MHit::EqGraph),mixer::eq_graph_hint(s,g,ui.mouse.0));
    ui.c.fill_rrect(g,3.,ui.t.s(0x1c2427));
    ui.c.push_clip(g.inset(1.));
    for hz in [100.,1000.,10000.] {
        let x=g.x+1.+eq::freq_pos(hz) as f32*(g.w-2.);
        ui.c.fill_rect(Rect::new(x,g.y,1.,g.h),ui.t.s(0x334044));
    }
    for db in [-12.,0.,12.] {
        let y=g.cy()-db/18.*(g.h/2.-4.);
        ui.c.fill_rect(Rect::new(g.x,y,g.w,1.),ui.t.s(0x334044));
    }
    let view=&s.mixer.mini_eq;
    if view.track==Some(t) && !view.analyzer.cols.is_empty() {
        let pts:Vec<_>=view.analyzer.cols.iter().enumerate().map(|(i,db)|
            (g.x+i as f32/(view.analyzer.cols.len()-1) as f32*g.w,g.bottom()-((*db+84.)/84.).clamp(0.,1.)*g.h)).collect();
        ui.c.polyline(&pts,1.,Color::rgb(0x557d85));
    }
    let settings=settings(s,t);
    let design=m.and_then(|m|m.slots[10].as_ref()).map(|p|eq::Options::from_image(&p.state).design(view.rate.max(44100) as f64)).unwrap_or(eq::Design::Ordinary(view.rate.max(44100) as f64));
    let curve=settings.curve(design);
    let active=m.is_none_or(|m|m.slot_on[10]);
    let pts:Vec<_>=(0..g.w as usize).map(|i|{
        let p=i as f64/(g.w-1.) as f64;
        let db=curve.db(eq::freq_hz(p)) as f32;
        (g.x+i as f32,g.cy()-db.clamp(-18.,18.)/18.*(g.h/2.-4.))
    }).collect();
    ui.c.polyline(&pts,1.6,Color::rgb(if active{0xe4dca3}else{0x73797a}));
    for (i,b) in settings.bands.iter().enumerate() {
        let x=g.x+1.+b.freq as f32*(g.w-2.);
        let y=g.cy()-b.db as f32/18.*(g.h/2.-4.);
        ui.c.fill_circle(x,y,5.5,Color::rgb(if settings.audible(i){COLORS[i]}else{0x64696b}));
        ui.c.text_center(Font::new(8),Rect::new(x-5.,y-5.,10.,10.),Color::rgb(0x142023),&format!("{}",i+1));
    }
    ui.c.pop_clip();
    ui.c.text(Font::new(9),g.x,g.bottom()+2.,ui.t.text,"20 Hz");
    ui.c.text_center(Font::new(9),Rect::new(g.cx()-20.,g.bottom()+1.,40.,12.),ui.t.text,if active{"0 dB"}else{"Bypass"});
    ui.c.text(Font::new(9),g.right()-30.,g.bottom()+2.,ui.t.text,"20 kHz");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mini_eq_scope_is_track_specific_and_finds_sine() {
        let mut scope=Scope::default();
        let signal:Vec<f32>=(0..N).flat_map(|i|{let x=(std::f32::consts::TAU*1000.*i as f32/48000.).sin()*0.5;[x,x]}).collect();
        scope.push(2,48000,&signal);
        let mut analyzer=Analyzer::default();
        analyzer.update(&scope.latest(),48000.,208,&Look{precision:1,slope:0,smoothing:0,average:false});
        let peak=analyzer.cols.iter().enumerate().max_by(|a,b|a.1.total_cmp(b.1)).unwrap().0;
        let hz=eq::freq_hz(peak as f64/207.);
        assert!((hz-1000.).abs()<100.,"peak {hz}");
        scope.push(3,48000,&[0.;64]);
        assert!(scope.latest().iter().all(|v|*v==0.));
    }
}
