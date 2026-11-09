// SPDX-License-Identifier: GPL-3.0-or-later
//! Dedicated Lemon Wave instrument view. Uses the same hit targets and host gestures
//! as the shared editor; graphs are parameter-derived, never decorative fake meters.
mod cyberpunk;
pub(crate) mod keyboard;
use crate::{assets::WaveParams, controls::*, reference_oscillator::Warp};
use lemon_small_fx::view::{Hit, Model, Target, ViewState};
use os_ui::{Canvas, Color, Font, Rect};
fn font(size: u16) -> Font {
    Font::light(crate::design::font_size(size + 1), 0).natural()
}
fn theme_bg() -> Color { crate::design::color(0x101319) }
fn theme_panel() -> Color { crate::design::color(0x202a33) }
fn theme_line() -> Color { crate::design::color(0x303a49) }
fn theme_ink() -> Color { crate::design::color(0xe8edf3) }
fn theme_dim() -> Color { crate::design::color(0x8d9baa) }
fn theme_a() -> Color { crate::design::color(0xe3df8b) }
fn theme_b() -> Color { crate::design::color(0x73dacc) }
fn theme_mod() -> Color { crate::design::color(0xbda4fb) }
pub const PRESET_FIELD: Rect = Rect::new(810.,14.,340.,34.);
pub const PRESET_PREV: Rect = Rect::new(770., 14., 32., 34.);
pub const PRESET_NEXT: Rect = Rect::new(1162., 14., 32., 34.);
pub const PRESET_INIT: Rect = Rect::new(1260., 14., 70., 34.);

pub fn osc_x(o:usize)->f32 {if o==0{8.}else{472.}}
pub fn osc_w(o:usize)->f32 {if o==0{452.}else{430.}}
pub fn edit_table_button(o:usize)->Rect {Rect::new(osc_x(o)+osc_w(o)-134.,98.,48.,22.)}
pub fn table_buttons(o:usize)->[Rect;3] {let x=osc_x(o)+20.;[Rect::new(x,605.,112.,20.),Rect::new(x+118.,605.,130.,20.),Rect::new(x+254.,605.,112.,20.)]}
pub fn detail_button(i:usize)->Rect {match i {
 0|1=>Rect::new(osc_x(i)+osc_w(i)-78.,98.,24.,22.),
 2=>Rect::new(1077.,98.,26.,22.),3=>Rect::new(1077.,465.,26.,22.),
 4=>Rect::new(1536.,141.,30.,30.),5=>Rect::new(388.,650.,26.,24.),
 _=>Rect::new(830.,650.,26.,24.)}}
pub fn detail_open(params:&WaveParams,i:usize)->bool {params.details.load(std::sync::atomic::Ordering::Relaxed)&(1<<i)!=0}
pub fn quick_choices(params:&WaveParams)->Vec<(usize,Rect,i32,&'static str)>{let patch=crate::fxrack::effective(std::array::from_fn(|k|params.base.v(k) as f32));quick_choices_for(params,&patch)}
fn quick_choices_for(params:&WaveParams,patch:&crate::dsp::Patch)->Vec<(usize,Rect,i32,&'static str)>{
 let l=params.lfo_tab.load(std::sync::atomic::Ordering::Relaxed).min(3);let mut result=Vec::new();
 for(i,(value,label))in[(0,"Off"),(5,"1/4"),(6,"1/8"),(7,"1/16")].into_iter().enumerate(){result.push((LFO_CONTROLS[l][1],Rect::new(626.+i as f32*38.,852.,38.,33.),value,label));}
 let scroll=params.fx_scroll.load(std::sync::atomic::Ordering::Relaxed).min(fx_max_scroll(crate::fxrack::order(&patch).len()));
 for(position,slot)in crate::fxrack::order(&patch).into_iter().enumerate(){let b=crate::fxrack::base(slot);if patch[b] as usize!=4||detail_open(params,8+slot){continue;}let r=fx_card(position,scroll);
  for(i,label)in["Stereo","Ping pong"].into_iter().enumerate(){result.push((b+5,Rect::new(r.x+15.+i as f32*82.5,r.y+48.,82.5,29.).intersect(&FX_AREA),i as i32,label));}
  for(i,label)in["1/4","1/8","1/16"].into_iter().enumerate(){result.push((b+10,Rect::new(r.x+195.+i as f32*54.6,r.y+48.,54.6,29.).intersect(&FX_AREA),1+i as i32,label));}
 }result.retain(|(_,r,_,_)|r.w>0.&&r.h>0.);result
}
pub fn fx_more(card:Rect)->Rect {Rect::new(card.right()-30.,card.y+10.,22.,22.)}
// Broad, low-opacity light fields supply bloom without hard concentric outlines.
fn soft_glow(c:&mut Canvas,r:Rect,color:Color,strength:f32){
 for i in (1..=12).rev(){let spread=i as f32*1.3;let a=((13-i) as f32*strength*1.7).min(24.) as u8;c.fill_rrect(r.inset(-spread),(r.h/2.+spread).min(24.),color.with_alpha(a));}
}
type GlassKey=[u32;10];
struct GlassCache {key:GlassKey,width:usize,height:usize,pixels:Vec<u32>}
thread_local!{static GLASS:std::cell::RefCell<std::collections::VecDeque<GlassCache>>=const {std::cell::RefCell::new(std::collections::VecDeque::new())};}
pub fn raised(c:&mut Canvas,r:Rect,radius:f32,base:Color){
 let key=[r.x.to_bits(),r.y.to_bits(),r.w.to_bits(),r.h.to_bits(),c.scale.to_bits(),radius.to_bits(),base.r as u32,((base.g as u32)<<8)|base.b as u32,c.pixmap.width(),c.pixmap.height()];
 if r.h>80.&&GLASS.with(|cache|{let mut cache=cache.borrow_mut();if let Some(i)=cache.iter().position(|entry|entry.key==key){let entry=cache.remove(i).unwrap();c.fill_rrect(Rect::new(r.x,r.y+2.,r.w,r.h),radius.min(12.),Color::rgb(0x010306).with_alpha(130));c.image(r,entry.width,entry.height,&entry.pixels);cache.push_back(entry);true}else{false}}){return;}
 let radius=radius.min(12.);c.fill_rrect(Rect::new(r.x,r.y+2.,r.w,r.h),radius,Color::rgb(0x010306).with_alpha(130));
 let large=r.h>80.;c.fill_rect_gradient(r,if large{Color::rgb(0x0b1218)}else{base},if large{Color::rgb(0x04090d)}else{base.shade(-0.025)},radius);
 c.stroke_rrect(r.inset(0.5),radius,0.9,Color::rgb(0x48606c).with_alpha(if large{110}else{145}));
 if large {
  c.push_clip(r.inset(2.));
  for j in 0..12 {let w=35.+j as f32*6.;let a=((12-j)*2/3) as u8;let x=r.right()-r.w*0.12;
   c.fill_polygon(&[(x-w,r.y+4.),(x+w,r.y+4.),(r.x-r.w*0.25+w,r.bottom()+70.),(r.x-r.w*0.25-w,r.bottom()+70.)],Color::rgb(0x1d4555).with_alpha(a));}
  c.pop_clip();c.polyline(&[(r.x+radius,r.y+1.),(r.right()-radius,r.y+1.)],0.9,Color::rgb(0x88b2c2).with_alpha(45));
  let sx=(r.x*c.scale).round() as i32;let sy=(r.y*c.scale).round() as i32;let width=(r.w*c.scale).round() as usize;let height=(r.h*c.scale).round() as usize;let cw=c.pixmap.width() as usize;let ch=c.pixmap.height() as usize;
  if sx>=0&&sy>=0&&sx as usize+width<=cw&&sy as usize+height<=ch{let raw=c.pixmap.pixels();let mut pixels=Vec::with_capacity(width*height);for y in sy as usize..sy as usize+height{for x in sx as usize..sx as usize+width{let p=raw[y*cw+x];pixels.push(((p.red() as u32)<<16)|((p.green() as u32)<<8)|p.blue() as u32);}}
   GLASS.with(|cache|{let mut cache=cache.borrow_mut();if cache.len()>=24{cache.pop_front();}cache.push_back(GlassCache{key,width,height,pixels});});}
 }
}
fn luminous(c:&mut Canvas,points:&[(f32,f32)],width:f32,color:Color){
 for (extra,alpha) in [(28.,3),(20.,4),(12.,7),(6.,12),(2.,28)]{c.polyline(points,width+extra,color.with_alpha(alpha));}
 c.polyline(points,width,color);c.polyline(points,(width*0.38).max(0.45),color.shade(0.28));
}
fn glowing_area(c:&mut Canvas,pts:&[(f32,f32)],bottom:f32,color:Color){
 if pts.len()<2{return;}
 // Fade vertically from the real curve down to its baseline.
 for band in 0..24{let lo=band as f32/24.;let hi=(band+1) as f32/24.;let mut area:Vec<_>=pts.iter().map(|&(x,y)|(x,y+(bottom-y)*lo)).collect();area.extend(pts.iter().rev().map(|&(x,y)|(x,y+(bottom-y)*hi)));c.fill_polygon(&area,color.with_alpha((43.*(1.-lo).powi(2)) as u8));}
 luminous(c,pts,1.7,color);
}
fn power(c:&mut Canvas,r:Rect,on:bool) {
    let r=r.inset(2.);
    if on {soft_glow(c,r,theme_a(),0.23);}
    c.fill_rrect(r,r.h/2.,if on{theme_a()}else{Color::rgb(0x18242c)});
    let ink=if on{Color::rgb(0x121808)}else{theme_dim()};
    c.arc(r.cx(),r.cy()+0.5,5.,-0.9,std::f32::consts::TAU-1.35,1.4,ink);
    c.polyline(&[(r.cx(),r.cy()-5.),(r.cx(),r.cy())],1.5,ink);
}
pub fn dropdown(c: &mut Canvas, r: Rect, label: &str, color: Color, hot: bool) {
    raised(c, r, 4., crate::design::color(if hot { 0x354450 } else { 0x24313b }));
    c.text_clip(font(if r.h>=29.{14}else{10}), r.x + 12., r.y + (r.h-18.)/2., r.w - 34., color, label);
    let x = r.right() - 13.;
    let y = r.cy();
    c.polyline(
        &[(x - 3., y - 1.5), (x, y + 1.5), (x + 3., y - 1.5)],
        1.5,
        color,
    );
}
struct Ui<'a> {
    live: crate::modulation::Live,
    c: &'a mut Canvas,
    m: &'a dyn Model,
    st: &'a ViewState,
    hits: Vec<Hit>,
}
impl Ui<'_> {
    fn label(&mut self, x: f32, y: f32, s: &str, size: u32, color: Color) {
        self.c.text(
            if size >= 12 {
                font(size as _).bold()
            } else {
                font(size as _)
            },
            x,
            y,
            color,
            s,
        );
    }
    fn panel(&mut self, r: Rect, title: &str, accent: Color, subtitle: &str) {
        let r = r.inset(-3.);
        raised(self.c, r, 8., theme_panel());
        self.label(r.x + 13., r.y + 9., title, 18, theme_ink());
        let _=accent;
        if !subtitle.is_empty() {
            self.label(r.x + 24., r.y + 31., subtitle, 10, theme_dim());
        }
    }
    fn hit(&mut self, r: Rect, t: Target) {
        self.hits.push(Hit { r, t });
    }
    fn hover(&self, r: Rect) -> bool {
        self.st.mouse.is_some_and(|(x, y)| r.contains(x, y))
    }
    fn knob(&mut self, k: usize, x: f32, y: f32, caption: &str, color: Color) {
        let id=SETTINGS[k].id;
        let (x,y)=crate::design::knob(id,x,y);
        let n=self.m.norm(id).clamp(0.,1.);
        let r=Rect::new(x-26.,y-37.,52.,76.);
        let hot=self.hover(r)||self.st.pressed.as_ref().is_some_and(|t|t.id()==id);
        let arc=|lo:f32,hi:f32,rad:f32|->Vec<(f32,f32)>{
            (0..=40).map(|i|{let a=(135.+270.*(lo+(hi-lo)*i as f32/40.)).to_radians();(x+rad*a.cos(),y+rad*a.sin())}).collect()
        };
        self.c.polyline(&arc(0.,1.,25.),0.7,theme_line());
        let zero=if SETTINGS[k].range.0<0 {0.5} else {0.};
        luminous(self.c,&arc(zero,n,25.),2.,color);
        if let Some(target)=mod_target(k) {
            let depth:f32=ROUTES.iter().filter(|slot|self.m.plain(SETTINGS[**slot].id)>0.&&self.m.plain(SETTINGS[*slot+1].id) as usize==target)
                .map(|slot|self.m.plain(SETTINGS[slot+2].id).abs()/1000.).sum();
            if depth>0. {
                self.c.polyline(&arc((n-depth).max(0.),(n+depth).min(1.),26.),1.,theme_mod());
                if self.live.active {
                    let offset=self.live.amounts[target];
                    let v=(if target==7 {n*(1.+offset)} else if target==6 {n+offset*0.5} else {n+offset}).clamp(0.,1.);
                    self.c.polyline(&arc(n.min(v),n.max(v),26.),2.,theme_mod());
                }
            }
        }
        self.c.fill_circle(x,y+1.,24.,Color::rgb(0x020609));
        self.c.fill_rect_gradient(Rect::new(x-23.,y-23.,46.,46.),Color::rgb(if hot{0x41525c}else{0x35434d}),Color::rgb(0x050b10),23.);
        self.c.stroke_circle(x,y,23.,0.7,Color::rgb(0x6b8490).with_alpha(45));
        self.c.polyline(&arc(0.16,0.47,22.),1.1,Color::rgb(0xb5c8d0).with_alpha(70));
        let a=(135.+270.*n).to_radians();
        self.c.polyline(&[(x+17.*a.cos(),y+17.*a.sin()),(x+21.*a.cos(),y+21.*a.sin())],1.7,color);
        let well=Rect::new(x-26.,y+25.,52.,13.);
        let text=self.m.text(id).replace(" voices","").replace(" cents","ct").replace(" deg","\u{00b0}");
        let mut value=text.replace(".0%","%").replace(".00 "," ").replace(' ',"");
        if let Some(hz)=value.strip_suffix("Hz").and_then(|v|v.parse::<f32>().ok()) {
            value=if hz>=1000. {format!("{:.1}kHz",hz/1000.).replace(".0k","k")} else {format!("{hz}Hz")};
        }
        let mut size=11;
        while size>6 && self.c.text_width(font(size),&value)>well.w-4. {size-=1;}
        self.c.text_center(font(size),well,theme_ink(),&value);
        self.c.text_center(font(11),Rect::new(x-31.,y-37.,62.,14.),theme_ink(),caption);
        self.hit(r,Target::Knob(id.into()));
    }
    fn dial(&mut self,k:usize,x:f32,y:f32,caption:&str,color:Color,radius:f32){
        let id=SETTINGS[k].id;let(x,y)=crate::design::knob(id,x,y);let n=self.m.norm(id).clamp(0.,1.);
        let hit=Rect::new(x-radius-9.,y-radius-29.,2.*radius+18.,2.*radius+56.);
        let hot=self.hover(hit)||self.st.pressed.as_ref().is_some_and(|t|t.id()==id);
        let arc=|lo:f32,hi:f32,r:f32|->Vec<(f32,f32)>{(0..=56).map(|i|{let a=(135.+270.*(lo+(hi-lo)*i as f32/56.)).to_radians();(x+r*a.cos(),y+r*a.sin())}).collect()};
        self.c.fill_circle(x,y+2.,radius+1.,Color::rgb(0x010407));
        self.c.fill_rect_gradient(Rect::new(x-radius,y-radius,2.*radius,2.*radius),Color::rgb(if hot{0x46535c}else{0x34414b}),Color::rgb(0x03090d),radius);
        for layer in 0..28 {self.c.fill_circle(x+1.,y+2.,radius-0.8-layer as f32*0.52,Color::rgb(0x02090d).with_alpha(5));}
        self.c.stroke_circle(x,y,radius-0.6,0.7,Color::rgb(0x56717e).with_alpha(70));
        for i in 0..24{let a=(205.+i as f32*4.).to_radians();let b=a+4_f32.to_radians();let shine=((i as f32/23.*std::f32::consts::PI).sin()*100.) as u8;self.c.polyline(&[(x+(radius-1.4)*a.cos(),y+(radius-1.4)*a.sin()),(x+(radius-1.4)*b.cos(),y+(radius-1.4)*b.sin())],1.8,Color::rgb(0xc2d4df).with_alpha(shine));}
        let zero=if SETTINGS[k].range.0<0{0.5}else{0.};luminous(self.c,&arc(zero,n,radius-0.4),2.1,color);
        let a=(135.+270.*n).to_radians();luminous(self.c,&[(x+(radius-9.)*a.cos(),y+(radius-9.)*a.sin()),(x+(radius-3.)*a.cos(),y+(radius-3.)*a.sin())],1.7,color);
        if let Some(target)=mod_target(k){let depth:f32=ROUTES.iter().filter(|slot|self.m.plain(SETTINGS[**slot].id)>0.&&self.m.plain(SETTINGS[*slot+1].id) as usize==target).map(|slot|self.m.plain(SETTINGS[slot+2].id).abs()/1000.).sum();if depth>0.{self.c.polyline(&arc((n-depth).max(0.),(n+depth).min(1.),radius+4.),1.,theme_mod());if self.live.active{let v=(n+self.live.amounts[target]).clamp(0.,1.);self.c.polyline(&arc(n.min(v),n.max(v),radius+4.),2.,theme_mod());}}}
        let text=self.m.text(id).replace(" voices","").replace(" cents"," ct");let size=if radius>=25.{14}else{12};
        self.c.text_center(font(size),Rect::new(x-44.,y-radius-29.,88.,20.),theme_ink(),caption);
        let mut value_size=size;while value_size>8&&self.c.text_width(font(value_size),&text)>80.{value_size-=1;}
        self.c.text_center(font(value_size),Rect::new(x-44.,y+radius+8.,88.,19.),theme_ink(),&text);
        self.hit(hit,Target::Knob(id.into()));
    }
    fn toggle(&mut self,k:usize,r:Rect,on:&str,off:&str,color:Color) {
        let active=self.m.plain(SETTINGS[k].id)>0.5;
        if on=="ON" && off=="OFF" {power(self.c,r,active);} else {
            raised(self.c,r,3.,crate::design::color(if active{0x3a4c43}else{0x202a33}));
            if active {self.c.fill_rrect(Rect::new(r.x+4.,r.bottom()-2.,r.w-8.,1.),0.5,color.with_alpha(170));}
            self.c.text_center(font(9),r,if active{color}else{theme_dim()},if active{on}else{off});
        }
        self.hit(r,Target::Choice(SETTINGS[k].id.into(),if active{0}else{1}));
    }
    fn level_fader(&mut self,k:usize,x:f32,top:f32,bottom:f32,color:Color) {
        let id=SETTINGS[k].id;
        let n=self.m.norm(id).clamp(0.,1.);
        let y=bottom-n*(bottom-top);
        for i in 0..=4 {
            let yy=top+(bottom-top)*i as f32/4.;
            self.c.fill_rect(Rect::new(x-22.,yy,7.,1.),theme_line());
            self.c.fill_rect(Rect::new(x+15.,yy,7.,1.),theme_line());
        }
        self.c.fill_rrect(Rect::new(x-4.,top-5.,8.,bottom-top+10.),4.,crate::design::color(0x070c10));
        self.c.fill_rrect(Rect::new(x-1.,y,2.,(bottom-y).max(1.)),1.,color.with_alpha(145));
        let cap=Rect::new(x-13.,y-7.,26.,14.);
        self.c.fill_rect_gradient(cap,crate::design::color(0xb1bdc4),crate::design::color(0x52636e),3.);
        self.c.stroke_rrect(cap,3.,1.,crate::design::color(0x17212a));
        self.c.fill_rect(Rect::new(x-9.,y,18.,1.),crate::design::color(0x18232a));
        self.c.text_center(font(9),Rect::new(x-29.,bottom+13.,58.,12.),theme_ink(),"Level");
        self.c.text_center(font(9),Rect::new(x-29.,bottom+26.,58.,12.),color,&self.m.text(id));
        self.hit(Rect::new(x-25.,top-8.,50.,bottom-top+16.),Target::Fader(id.into(),bottom-top));
    }
    fn select(&mut self, k: usize, r: Rect, color: Color) {
        let id = SETTINGS[k].id;
        let r = crate::design::selector(id, r);
        dropdown(self.c, r, &self.m.text(id), color, self.hover(r));
        self.hit(
            r,
            Target::Choice(id.into(), self.m.plain(id).round() as i32),
        );
    }
    fn grid(&mut self, r: Rect) {
        let r = Rect::new(r.x-6.,r.y,r.w+12.,r.h);
        self.c.fill_rect_gradient(r,Color::rgb(0x040b10),Color::rgb(0x08141d),6.);
        for i in 1..8 {
            let x = r.x + r.w * i as f32 / 8.;
            self.c
                .fill_rect(Rect::new(x, r.y + 4., 1., r.h - 8.), crate::design::color(0x202a36).with_alpha(100));
        }
        for i in 1..4 {
            let y = r.y + r.h * i as f32 / 4.;
            self.c
                .fill_rect(Rect::new(r.x + 4., y, r.w - 8., 1.), crate::design::color(0x202a36).with_alpha(100));
        }
    }
    fn env(&mut self, base: usize, r: Rect, color: Color) {
        self.grid(r);
        let reference = self.m.plain("env_engine") > 0.5;
        let key = if base == ATTACK { AMP_HOLD } else { ENV_HOLD };
        let a = seconds(self.m.plain(SETTINGS[base].id));
        let d = seconds(self.m.plain(SETTINGS[base + 1].id));
        let mut sustain = self.m.plain(SETTINGS[base + 2].id) / 1000.;
        if reference {
            sustain *= sustain;
        }
        let release = seconds(self.m.plain(SETTINGS[base + 3].id));
        let hold = if reference {
            hold_seconds(self.m.plain(SETTINGS[key].id))
        } else {
            0.
        };
        let gate = (a + d + release).max(0.1) * 0.3;
        let total = a + hold + d + gate + release;
        let shape = |i: usize| {
            if reference {
                self.m.plain(SETTINGS[key + 1 + i].id) as f64 / 1000.
            } else {
                0.5
            }
        };
        let mut pts = Vec::new();
        for (start, length, from, to, curve) in [
            (0., a, 0., 1., shape(0)),
            (a, hold, 1., 1., 0.5),
            (a + hold, d, 1., sustain, shape(1)),
            (a + hold + d, gate, sustain, sustain, 0.5),
            (a + hold + d + gate, release, sustain, 0., shape(2)),
        ] {
            for i in 0..=40 {
                let t = i as f64 / 40.;
                let v = to + (from - to) * crate::reference_envelope::falling(curve, t);
                pts.push((
                    r.x + 8. + (r.w - 16.) * (start + length * t as f32) / total,
                    r.bottom() - 9. - (r.h - 18.) * v,
                ));
            }
        }
        glowing_area(self.c,&pts,r.bottom()-9.,color);
        let i = if base == ATTACK { 0 } else { 1 };
        let stage = self.live.env_stage[i];
        let elapsed = self.live.env_elapsed[i];
        if self.live.active && stage != 0 {
            let time = match stage {
                1 => elapsed.min(a),
                5 => a + elapsed.min(hold),
                2 => a + hold + elapsed.min(d),
                3 => a + hold + d + gate * 0.5,
                4 => a + hold + d + gate + elapsed.min(release),
                _ => 0.,
            };
            let x = r.x + 8. + (r.w - 16.) * time / total;
            self.c
                .polyline(&[(x, r.y + 4.), (x, r.bottom() - 4.)], 1., theme_ink());
        }
    }
}
pub fn paint(
    c: &mut Canvas,
    m: &dyn Model,
    st: &ViewState,
    hint: &str,
    params: &WaveParams,
    cycles: [usize; 2],
    preset: Option<&str>,
) -> Vec<Hit> {
    let mut u = Ui {
        c,
        m,
        st,
        hits: Vec::new(),
        live: params.telemetry.read(),
    };
    u.c.begin_frame(None);
    u.c.clear(theme_bg());
    u.c.fill_rect_gradient(
        Rect::new(0., 0., 1584., 66.),
        crate::design::color(0x3a4851),
        crate::design::color(0x17212a),
        0.,
    );
    u.c.fill_rect(Rect::new(0., 65., 1584., 1.), crate::design::color(0x6b796f));
    u.c.text(font(27),28.,15.,theme_ink(),"LEMON");
    u.c.text(font(30),149.,12.,theme_a(),"/");u.c.text(font(27),170.,15.,theme_ink(),"WAVE");
    u.c.fill_rect(Rect::new(277.,18.,1.,26.),theme_line());u.c.text(font(12),297.,23.,theme_dim(),"LEMON STUDIO");
    let page = params.page.load(std::sync::atomic::Ordering::Relaxed);
    for (i, label) in ["SYNTH", "MOD", "TABLE EDIT"].iter().enumerate() {
        let r = page_button(i);
        if page==i {soft_glow(u.c,r,theme_a(),0.18);}
        raised(
            u.c,
            r,
            4.,
            if page == i {
                crate::design::color(0x4c5943)
            } else {
                crate::design::color(0x27343f)
            },
        );
        u.c.text_center(font(16), r, if page == i { Color::rgb(0x090e10) } else { theme_dim() }, label);
    }

    if page != 2 {
        let title = preset.unwrap_or("Preset library");
        for (r, t) in [
            (PRESET_PREV, "‹"),
            (PRESET_NEXT, "›"),
            (PRESET_INIT, "Init"),
            (crate::preset_ui::SAVE, "Save"),
        ] {
            raised(
                u.c,
                r,
                5.,
                crate::design::color(if u.hover(r) { 0x43525c } else { 0x2e3b46 }),
            );
            u.c.text_center(font(13), r, theme_ink(), t);
        }
        dropdown(u.c, PRESET_FIELD, title, theme_ink(), false);

    } else {
        u.label(
            800.,
            28.,
            "WAVETABLE WORKSHOP  /  AUDITION HERE, APPLY TO PLAY ON SYNTH",
            12,
            theme_dim(),
        );
    }
    if page == 1 {
        paint_modulation(&mut u, params);
        return u.hits;
    }
    if page == 2 {
        return u.hits;
    }
    cyberpunk::paint_synth(&mut u,params,cycles,hint);
    keyboard::paint(&mut u,params);
    u.hits
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Patch(crate::dsp::Patch);
    impl Model for Patch {
        fn plain(&self, id: &str) -> f32 {
            self.0[SPEC.index(id).unwrap()]
        }
        fn norm(&self, id: &str) -> f32 {
            let s = &SETTINGS[SPEC.index(id).unwrap()];
            (self.plain(id) - s.range.0 as f32) / (s.range.1 - s.range.0) as f32
        }
        fn text(&self, id: &str) -> String {
            crate::controls::text(SPEC.index(id).unwrap(), self.plain(id) as i32)
        }
    }
    #[test]
    fn cached_glass_and_mesh_preserve_pixels_and_follow_position() {
        let params=WaveParams::default();let mut patch=crate::dsp::defaults();patch[A_BANK]=2.;patch[B_BANK]=3.;
        let mut c=Canvas::new(1584,1056,1.).unwrap();
        paint(&mut c,&Patch(patch),&Default::default(),"",&params,[5;2],None);let first=c.pixmap.data().to_vec();
        paint(&mut c,&Patch(patch),&Default::default(),"",&params,[5;2],None);assert_eq!(first,c.pixmap.data());
        let original=patch[A_POSITION];patch[A_POSITION]=123.;paint(&mut c,&Patch(patch),&Default::default(),"",&params,[5;2],None);assert_ne!(first,c.pixmap.data());
        patch[A_POSITION]=original;params.details.store(127,std::sync::atomic::Ordering::Relaxed);paint(&mut c,&Patch(patch),&Default::default(),"",&params,[5;2],None);
        params.details.store(0,std::sync::atomic::Ordering::Relaxed);paint(&mut c,&Patch(patch),&Default::default(),"",&params,[5;2],None);assert_eq!(first,c.pixmap.data());
    }
    #[test]
    fn modular_rack_scrolled_controls_stay_inside_the_panel_and_clear_each_other() {
        let mut p=crate::dsp::defaults();p[RACK_MODE]=1.;
        for (slot,kind) in [4,5,1,2,3,6,7,8,9,10,11,12,13,4,5,1].into_iter().enumerate(){let b=crate::fxrack::base(slot);p[b]=kind as f32;p[b+6..b+12].copy_from_slice(&crate::fxrack::initial(kind));}
        let m=Patch(p);let params=WaveParams::default();
        for scale in [1.,1.5]{for scroll in (0..=fx_max_scroll(16)).step_by(46) {
            params.fx_scroll.store(scroll,std::sync::atomic::Ordering::Relaxed);
            let mut c=Canvas::new((1584.*scale) as u32,(1056.*scale) as u32,scale).unwrap();
            let hits=paint(&mut c,&m,&Default::default(),"",&params,[5;2],None);
            let mut rects:Vec<_>=hits.iter().filter(|h|SPEC.index(h.t.id()).is_some_and(|k|(RACK_START..RACK_END).contains(&k))).map(|h|h.r).collect();
            for position in 0..16{for action in 0..6 {let r=fx_button(fx_card(position,scroll),action).intersect(&FX_AREA);if r.w>0.&&r.h>0.{rects.push(r);}}}
            for (i,a) in rects.iter().enumerate(){assert!(a.x>=FX_AREA.x&&a.y>=FX_AREA.y&&a.right()<=FX_AREA.right()+0.01&&a.bottom()<=FX_AREA.bottom()+0.01);
                for b in &rects[i+1..]{assert!(a.right().min(b.right())<=a.x.max(b.x)+0.01||a.bottom().min(b.bottom())<=a.y.max(b.y)+0.01,"overlap {a:?} {b:?}");}}
        }}
    }
    #[test]
    fn reference_view_has_accessible_nonoverlapping_targets_at_both_scales() {
        let mut p = crate::dsp::defaults();
        p[ENV_ENGINE] = 1.;
        p[AMP_HOLD] = 300.;
        p[AMP_ATK_CURVE] = 700.;
        p[AMP_DEC_CURVE] = 650.;
        p[ENV_DEC_CURVE] = 750.;
        p[FILTER_MODEL] = 2.;
        p[A_SOURCE] = 1.;
        p[A_MODE] = 9.;
        p[A_AMOUNT] = 650.;
        p[A_UNISON] = 5.;
        p[A_RANDOM] = 350.;
        p[B_SOURCE] = 1.;
        p[B_LEVEL] = 400.;
        p[B_MODE] = 4.;
        p[B_AMOUNT] = 400.;
        p[CUTOFF] = 700.;
        p[FILTER] = 1.;
        p[MOD1_SOURCE] = 1.;
        p[MOD1_TARGET] = 2.;
        p[MOD1_AMOUNT] = 350.;
        p[MOD2_SOURCE] = 2.;
        p[MOD2_TARGET] = 1.;
        p[MOD2_AMOUNT] = 280.;
        let model = Patch(p);
        let params = WaveParams::default();
        let mut cases=Vec::new();
        for scale in [1.,1.5] {for env in 0..2 {for lfo in 0..4 {cases.push((0,scale,env,lfo));}} cases.push((1,scale,0,0));}
        for (page, scale, env, lfo) in cases {
          for details in [0,127] {params.details.store(details,std::sync::atomic::Ordering::Relaxed);
            params.env_tab.store(env,std::sync::atomic::Ordering::Relaxed);
            params.lfo_tab.store(lfo,std::sync::atomic::Ordering::Relaxed);
            params
                .page
                .store(page, std::sync::atomic::Ordering::Relaxed);
            let mut c = Canvas::new((1584. * scale) as u32, (1056. * scale) as u32, scale).unwrap();
            let hits = paint(
                &mut c,
                &model,
                &Default::default(),
                "Drag to edit · Ctrl for fine control · Right-click for automation",
                &params,
                [5; 2],
                None,
            );
            let mut all: Vec<_> = hits.iter().map(|h| h.r).collect();
            all.extend([page_button(0),page_button(1),page_button(2),PRESET_FIELD,PRESET_PREV,PRESET_NEXT,PRESET_INIT]);
            if page==0 {all.extend([filter_tab(0),filter_tab(1),NOISE_IMPORT,NOISE_CLEAR]);for o in 0..2 {all.push(crate::sound_browser::button(o));all.push(edit_table_button(o));}}
            all.extend(source_buttons(page,&params).into_iter().map(|(_, r, _)| r));
            if page==0 {
                all.extend((0..2).map(envelope_tab));all.extend((0..4).map(lfo_tab));
                all.extend((0..7).map(synth_shape_button));if details==0{all.push(synth_lfo_graph());all.push(synth_env_graph());}all.extend((0..7).map(detail_button));
            }
            if page == 1 {
                for i in 0..4 {
                    all.push(lfo_graph(i));
                    for j in 0..5 {
                        all.push(shape_button(i, j));
                    }
                }
                for i in 0..8 {
                    all.push(clear_route(i));
                }
            }
            for (i, a) in all.iter().enumerate() {
                assert!(a.x >= 0. && a.y >= 0. && a.right() <= 1584. && a.bottom() <= 1056.);
                for b in &all[i + 1..] {
                    let w = a.right().min(b.right()) - a.x.max(b.x);
                    let h = a.bottom().min(b.bottom()) - a.y.max(b.y);
                    assert!(w <= 0.01 || h <= 0.01, "overlap page{page}: {a:?} {b:?}");
                }
            }
            for (i, a) in hits.iter().enumerate() {
                for b in &hits[i + 1..] {
                    let w = a.r.right().min(b.r.right()) - a.r.x.max(b.r.x);
                    let h = a.r.bottom().min(b.r.bottom()) - a.r.y.max(b.r.y);
                    assert!(
                        w <= 0.01 || h <= 0.01,
                        "overlap: {} / {}",
                        a.t.id(),
                        b.t.id()
                    );
                }
            }
            if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
                std::fs::create_dir_all(&dir).unwrap();
                c.pixmap
                    .save_png(format!("{dir}/modulation-page{page}-{scale}.png"))
                    .unwrap();
            }
          }
        }
    }
}

pub fn osc_power(o:usize)->Rect {Rect::new(osc_x(o)+osc_w(o)-53.,96.,36.,26.)}
pub fn filter_tab(i:usize)->Rect {Rect::new(1182.+192.*i as f32,94.,184.,30.)}
pub fn filter_input(i:usize)->Rect {Rect::new(1192.+42.*i as f32,258.,36.,25.)}
pub const NOISE_IMPORT:Rect=Rect::new(930.,781.,104.,26.);
pub const NOISE_CLEAR:Rect=Rect::new(1040.,781.,104.,26.);
pub const FX_AREA:Rect=Rect::new(1182.,528.,378.,406.);
pub const FX_ADD:Rect=Rect::new(1490.,486.,72.,29.);
pub const FX_CARD_HEIGHT:f32=204.;
pub fn fx_card(position:usize,scroll:usize)->Rect {Rect::new(1183.,529.+position as f32*FX_CARD_HEIGHT-scroll as f32,374.,194.)}
pub fn fx_button(card:Rect,action:usize)->Rect {Rect::new(card.x+164.+action as f32*25.,card.y+10.,22.,22.)}
pub fn fx_menu_button(kind:usize)->Rect {Rect::new(1188.+((kind-1)%2) as f32*184.,537.+((kind-1)/2) as f32*48.,176.,36.)}
pub fn fx_max_scroll(count:usize)->usize {((count.saturating_sub(1)) as f32*FX_CARD_HEIGHT+195.-FX_AREA.h).max(0.) as usize}
struct FxModel(crate::dsp::Patch);
impl Model for FxModel {
 fn plain(&self,id:&str)->f32 {self.0[SPEC.index(id).unwrap()]}
 fn norm(&self,id:&str)->f32 {let k=SPEC.index(id).unwrap();let s=&SETTINGS[k];(self.0[k]-s.range.0 as f32)/(s.range.1-s.range.0) as f32}
 fn text(&self,id:&str)->String {let k=SPEC.index(id).unwrap();if (RACK_START..RACK_END).contains(&k) {let b=crate::fxrack::base((k-RACK_START)/RACK_STRIDE);let kind=self.0[b] as usize;let col=k-b;
  if col==5{return crate::fxrack::modes(kind)[(self.0[k] as usize).min(crate::fxrack::modes(kind).len()-1)].into();}
  if col>=6{return crate::fxrack::value(kind,col-6,self.0[k]);}
 }crate::controls::text(k,self.0[k] as i32)}
}
pub fn page_button(page: usize) -> Rect {
    Rect::new(
        444. + page as f32 * 104.,
        16.,
        88.,
        30.,
    )
}
pub fn lfo_panel(i: usize) -> Rect {
    Rect::new(
        16. + (i % 2) as f32 * 580.,
        80. + (i / 2) as f32 * 198.,
        568.,
        186.,
    )
}
pub fn lfo_graph(i: usize) -> Rect {
    let p = lfo_panel(i);
    Rect::new(p.x + 12., p.y + 49., 330., 100.)
}
pub fn envelope_nodes(env:usize,value:impl Fn(usize)->f32)->Vec<(usize,Option<usize>,f32,f32)> {
    let base=if env==0 {ATTACK} else {ENV_ATTACK};
    let hold_key=if env==0 {AMP_HOLD} else {ENV_HOLD};
    let curved=value(ENV_ENGINE)>0.5;
    let a=seconds(value(base));let d=seconds(value(base+1));let release=seconds(value(base+3));
    let h=if curved {hold_seconds(value(hold_key))} else {0.};
    let raw=value(base+2)/1000.;let sustain=if curved {raw*raw} else {raw};
    let gate=(a+d+release).max(0.1)*0.3;let total=a+h+d+gate+release;
    let r=synth_env_graph();
    let point=|k,second,time,v|(k,second,r.x+8.+(r.w-16.)*time/total,r.bottom()-9.-(r.h-18.)*v);
    let mut nodes=vec![point(base,None,a,1.),point(base+1,Some(base+2),a+h+d,sustain),point(base+3,None,a+h+d+gate+release*0.5,sustain*crate::reference_envelope::falling(if curved {value(hold_key+3) as f64/1000.} else {0.5},0.5))];
    if curved && (r.w-16.)*h/total>12. {nodes.push(point(hold_key,None,a+h,1.));}
    nodes
}
pub fn envelope_tab(i:usize)->Rect {Rect::new(320.+i as f32*30.,650.,26.,24.)}
pub fn lfo_tab(i:usize)->Rect {Rect::new(552.+i as f32*29.,650.,25.,24.)}
pub fn synth_env_graph()->Rect {Rect::new(24.,695.,420.,96.)}
pub fn synth_lfo_graph()->Rect {Rect::new(486.,695.,400.,96.)}
pub fn synth_shape_button(i:usize)->Rect {Rect::new(486.+i as f32*57.,924.,53.,18.)}
pub fn source_buttons(page: usize, params: &WaveParams) -> Vec<(usize, Rect, &'static str)> {
    if page == 0 {
        let e=params.env_tab.load(std::sync::atomic::Ordering::Relaxed).min(1);
        let l=params.lfo_tab.load(std::sync::atomic::Ordering::Relaxed).min(3);
        return vec![([12,2][e],Rect::new(424.,650.,28.,24.),["ENV 1","ENV 2"][e]),
            ([1,5,6,7][l],Rect::new(864.,650.,28.,24.),["LFO 1","LFO 2","LFO 3","LFO 4"][l])];
    }
    let mut v:Vec<_>=[1,5,6,7].into_iter().enumerate().map(|(i,s)| {
        let p=lfo_panel(i);
        (s,Rect::new(p.right()-40.,p.y+7.,28.,22.),["LFO 1","LFO 2","LFO 3","LFO 4"][i])
    }).collect();
    for i in 0..4 {
        v.push((8+i,Rect::new(916.+(i%2) as f32*148.,506.+(i/2) as f32*125.,28.,22.),["Macro 1","Macro 2","Macro 3","Macro 4"][i]));
    }
    v
}
fn paint_source_handle(c:&mut Canvas,r:Rect,_label:&str,hot:bool) {
    let color=if hot {theme_ink()} else {theme_mod()};
    if hot { c.fill_rrect(r,4.,crate::design::color(0x35434e)); }
    for row in 0..3 { for col in 0..2 {
        c.fill_circle(r.x+7.+col as f32*4.,r.y+6.+row as f32*4.,1.,color.with_alpha(180));
    }}
    c.polyline(&[(r.x+17.,r.y+15.),(r.x+23.,r.y+7.),(r.x+18.,r.y+7.)],1.3,color);
    c.polyline(&[(r.x+23.,r.y+7.),(r.x+23.,r.y+12.)],1.3,color);
}
pub fn shape_button(i: usize, shape: usize) -> Rect {
    let p = lfo_panel(i);
    Rect::new(p.x + 12. + shape as f32 * 66., p.y + 157., 62., 21.)
}
pub fn clear_route(i: usize) -> Rect {
    Rect::new(790., 510. + i as f32 * 32., 22., 24.)
}
fn paint_modulation(u: &mut Ui, params: &WaveParams) {
    let curves = *params.curves.read().unwrap();
    for i in 0..4 {
        let p = lfo_panel(i);
        u.panel(p, ["LFO 1", "LFO 2", "LFO 3", "LFO 4"][i], theme_mod(), "");
        let r = lfo_graph(i);
        u.grid(r);
        let pts: Vec<_> = (0..=128)
            .map(|j| {
                let phase = j as f32 / 128.;
                let y = if i == 0 && u.m.plain("lfo_draw") < 0.5 {
                    match u.m.plain("lfo_shape") as usize {
                        1 => 1. - 4. * (phase - 0.5).abs(),
                        2 => phase * 2. - 1.,
                        3 => {
                            if phase < 0.5 {
                                1.
                            } else {
                                -1.
                            }
                        }
                        _ => (phase * std::f32::consts::TAU).sin(),
                    }
                } else {
                    curves.sample(i, phase)
                };
                (r.x + phase * r.w, r.y + (1. - y) * r.h * 0.5)
            })
            .collect();
        u.c.polyline(&pts, 1.6, theme_mod());
        if u.live.active {
            let x = r.x + r.w * u.live.phases[i];
            u.c.polyline(&[(x, r.y), (x, r.bottom())], 1., theme_ink());
        }
        let c = LFO_CONTROLS[i];
        u.knob(c[0], p.x + 384., p.y + 82., "Free Hz", theme_mod());
        for (j, k) in c[1..].iter().enumerate() {
            u.select(
                *k,
                Rect::new(p.x + 430., p.y + 49. + j as f32 * 32., 126., 25.),
                theme_mod(),
            );
        }
        for (j, label) in ["Sine", "Triangle", "Ramp", "Square", "Undo"]
            .iter()
            .enumerate()
        {
            let r = shape_button(i, j);
            raised(u.c, r, 3., crate::design::color(0x293741));
            u.c.text_center(font(9), r, theme_dim(), label);
        }
        if i == 0 {
            u.select(LFO_DRAW, Rect::new(p.x + 350., p.y + 157., 206., 21.), theme_mod());
        } else {
            u.label(p.x + 356., p.y + 161., "DRAW IN THE GRAPH", 9, theme_dim());
        }
    }
    u.panel(
        Rect::new(16., 476., 808., 312.),
        "MODULATION MATRIX",
        theme_mod(),
        "",
    );
    for (i, slot) in ROUTES.into_iter().enumerate() {
        let y = 510. + i as f32 * 32.;
        u.label(28., y + 5., &format!("{}", i + 1), 10, theme_dim());
        u.select(slot, Rect::new(48., y, 178., 24.), theme_mod());
        u.select(slot + 1, Rect::new(238., y, 278., 24.), theme_ink());
        let r = Rect::new(528., y, 250., 24.);
        raised(u.c, r, 4., crate::design::color(0x293741));
        let v = u.m.plain(SETTINGS[slot + 2].id) / 1000.;
        u.c.text_center(font(16), r, theme_mod(), &format!("Depth {:+.1}%", v * 100.));
        u.c.fill_rect(
            Rect::new(
                r.x + 125_f32.min(125. + v * 114.),
                y + 20.,
                (v * 114.).abs().max(1.),
                2.,
            ),
            theme_mod(),
        );
        u.hit(r, Target::Knob(SETTINGS[slot + 2].id.into()));
        let r = clear_route(i);
        raised(u.c, r, 3., crate::design::color(0x293741));
        u.c.text_center(font(12), r, theme_dim(), "×");
    }
    u.panel(Rect::new(836., 476., 328., 312.), "MACROS", theme_a(), "");
    for i in 0..4 {
        u.knob(
            MACRO1 + i,
            930. + (i % 2) as f32 * 148.,
            568. + (i / 2) as f32 * 125.,
            ["Macro 1", "Macro 2", "Macro 3", "Macro 4"][i],
            theme_a(),
        );
    }
    for (_, r, label) in source_buttons(1,params) { paint_source_handle(u.c,r,label,u.hover(r)); }
    u.label(20.,800.,"Draw a shape • Drag a source onto a synth knob (hover SYNTH while dragging) • Drag Depth to adjust",10,theme_dim());
}
