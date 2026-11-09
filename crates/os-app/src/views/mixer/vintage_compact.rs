// SPDX-License-Identifier: GPL-3.0-or-later
//! Vintage strip bank beside a fixed left insert panel; existing actions and DSP are shared.
use super::*;
use crate::{vintage as skin,vintage_rack_art as art};

pub(super) fn supported(r:Rect)->bool {r.w>=440. && r.h>=390.}
fn meter_extra(r: Rect) -> f32 {
    ((r.h - 263.0) * 0.45).clamp(0.0, 230.0)
}

pub(super) fn travel(r: Rect) -> (f32, f32) {
    (r.y + 173.0 + meter_extra(r), r.bottom() - 114.0)
}

pub(super) fn paint(ui:&mut Ui,s:&State,outer:Rect) {
    window_body(ui,outer);
    let r=super::super::float_frame(ui,s,Win::Mixer,outer).unwrap_or(outer);
    let title=format!("Mixer - {}",mx::name(s,s.selected_mixer));
    super::super::window_frame_ex(ui,Win::Mixer,r,"",0.,false,true);
    title_tools(ui,s,r);
    ui.c.text_clip(Font::vintage(12),r.x+108.,r.y+4.,(r.w-178.).max(0.),ui.t.text,&title);
    window_buttons(ui,Win::Mixer,r.right()-64.,r.y,ui.t.text_dim,ui.t.glyph);
    inspector(ui, s, Rect::new(r.x, r.y, 221.0, r.h));
    let bank=Rect::new(r.x+230.,r.y+24.,r.w-233.,r.h-72.);
    ui.c.fill_rect(bank,Color::rgb(0x3d4c54));
    ui.hit(bank,Action::MixerArea,"Mixer");
    let width=strip_width(s);
    let area=Rect::new(bank.x+width+4.,bank.y,bank.w-width-4.,bank.h);
    let first=(s.mixer_scroll/width) as u16+1;
    let count=(area.w/width).ceil() as u16+1;
    ui.c.push_clip(area);
    ui.clip_hit_rect(area,|ui| {
    for n in first..first.saturating_add(count).min(mx::inserts(s)+1) {
        let x=area.x+f32::from(n-1)*width-s.mixer_scroll;
        strip(ui,s,n,Rect::new(x,bank.y,width,bank.h),r);
        insertion_button(ui,s,area,bank,n,x);
    }
    });
    ui.c.pop_clip();
    strip(ui,s,0,Rect::new(bank.x,bank.y,width,bank.h),r);
    // Cables are drawn directly from live routes, without simulation or a timer.
    s.cables.borrow_mut().clear();
    if crate::workspace_visibility::shown("visible_mixer-cables") {
        let socket=|n:u16|if n==0 {bank.x+width*0.5}else{area.x+f32::from(n-1)*width-s.mixer_scroll+width*0.5};
        let cur=s.selected_mixer;
        let clip=Rect::new(bank.x,bank.y,bank.w,r.bottom()-14.0-bank.y);
        for n in 0..=mx::inserts(s) {
            if n==cur {continue;}
            if let Some(level)=mx::send(s,cur,n) {
                let bounds=if n==0 || cur==0 {clip}else{Rect::new(area.x,clip.y,area.w,clip.h)};
                ui.c.push_clip(bounds);
                skin::cable(ui.c,(socket(cur),bank.bottom()-2.),(socket(n),bank.bottom()-2.),r.bottom()-14.0,if level>0. {ui.t.accent}else{ui.t.text_dim});
                ui.c.pop_clip();
            }
        }
    }
    let scroll=Rect::new(area.x,r.bottom()-12.,area.w,8.);
    super::super::scrollbar(ui,scroll,false,s.mixer_scroll,area.w,f32::from(mx::inserts(s))*width,crate::app::Scroll::MixerX);
    resize_zones(ui,s,Win::Mixer,outer);
}
fn strip(ui:&mut Ui,s:&State,n:u16,r:Rect,window:Rect) {
    let track=mx::track(s,n);
    let chosen=s.selected_mixer==n;
    let name=mx::name(s,n);
    ui.hit(r,Action::Mix(MHit::Strip(n)),name.clone());
    let base=track.and_then(|m|m.color).map(super::super::rgb).map_or(Color::rgb(0x5f686d),|c|c.mix(Color::rgb(0x5f686d),0.75));
    skin::metal(ui.c,r,base);
    let header = Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, 54.0);
    let shade = Color::rgb(0x222c32);
    ui.c.fill_rect_gradient(header, shade.with_alpha(164), shade.with_alpha(0), 0.0);
    if n != 0 {
        ui.c.text_center(Font::vintage(8).bold(), Rect::new(r.x, r.y + 1.0, r.w, 10.0),
            Color::rgb(0xb5c2c4), "FX");
    }
    skin::mixer_digits(ui.c, Rect::new(r.x, r.y + 10.0, r.w, 11.0), n);
    let extra = meter_extra(window);
    let meter = Rect::new(r.cx() - 5.0, r.y + 23.0, 10.0, 19.0 + extra);
    skin::recess(ui.c, meter.inset(-2.0), Color::rgb(0x1b282a));
    if crate::workspace_visibility::shown("visible_mixer-meters") {
        ui.live(Live::Meter(n), meter);
        for i in 0..2 {
            skin::meter_idle(ui.c, Rect::new(meter.x + i as f32 * 5.0, meter.y, 4.0, meter.h));
        }
        if let Some(m) = s.mixer.meters.get(n as usize) {
            let cfg = &s.mixer.meter_cfg;
            let fraction = |db| meters::frac(db, cfg.v[meters::k::FLOOR_DB], false);
            for i in 0..2 {
                let x = meter.x + i as f32 * 5.0;
                let height = (meter.h * fraction(m.db[i])).round();
                if crate::vintage_skin::has_frame("FXMeterColumn", 0) {
                    if height > 0.0 {
                        let column = Rect::new(x, meter.y, 4.0, meter.h);
                        ui.c.push_clip(Rect::new(x, meter.bottom() - height, 4.0, height));
                        crate::vintage_skin::draw(ui.c, "FXMeterColumn", 0, column);
                        ui.c.pop_clip();
                    }
                    if fraction(m.hold[i]) > 0.0 {
                        let y = meter.bottom() - (meter.h * fraction(m.hold[i])).round();
                        ui.c.fill_rect(Rect::new(x, y, 4.0, 1.0), Color::rgb(0xb3ac72));
                    }
                } else {
                    let ys = |db| meter.bottom() - fraction(db) * meter.h;
                    meter_bar(ui, x, 4.0, meter.bottom(), m.db[i], m.hold[i], cfg, &ys);
                }
            }
        }
    }
    ui.c.push_clip(Rect::new(r.x+2.,r.y+57.+extra,r.w-4.,49.));
    ui.c.text_vertical(Font::vintage(10),r.cx()-6.,r.y+104.+extra,ui.t.text,&name);
    ui.c.pop_clip();
    let mute=Rect::new(r.cx()-7.,r.y+108.+extra,14.,14.);
    let has_lock = crate::workspace_visibility::shown("visible_mixer-channel-lock") && track.is_some();
    let lamp = if has_lock { Rect::new(mute.x - 3., mute.y, mute.w, mute.h) } else { mute };
    ui.hit(if has_lock {lamp.inset(2.)} else {lamp}, Action::Mix(MHit::Led(n)), "Mute / solo");
    if track.is_none_or(|m|m.enabled) {art::led_on(ui.c,lamp);}else{art::led_off(ui.c,lamp);}
    channel_lock(ui, s, n, Rect::new(r.right()-12., mute.y+1., 11., 12.));
    let pan=KnobId::MixPan(n);
    let knob=Rect::new(r.cx()-9.,r.y+123.+extra,18.,18.);
    ui.hit(knob,Action::Knob(pan),s.knob_text(pan));art::knob(ui.c,knob,s.knob(pan));
    let (top,bottom)=travel(window);
    let separator = mx::has_separator(s, n);
    let x = r.cx() + if separator { 2.5 } else { 0.0 };
    skin::fader_guide(ui.c, x, top, bottom);
    let v=track.and_then(|m|m.volume).unwrap_or(0.8) as f32;
    let position = |value: f32| bottom - value.clamp(0.0, 1.0) * (bottom - top);
    skin::fader_unity(ui.c, x, position(0.8), 8.0);
    let hot=ui.hit(Rect::new(x-10.,top-10.,20.,bottom-top+20.),Action::Fader(n),mx::level_text("Volume",f64::from(v)));
    skin::fader(ui.c, x, position(v), hot);
    let fx = Rect::new(x - 10.0, r.bottom() - 54.0, 20.0, 16.0);
    let enabled = !s.mixer.all_fx_bypassed && track.is_none_or(|m| m.slots_enabled);
    let hot = ui.hit(fx, Action::Mix(MHit::Effects(n)), if enabled { "Bypass effects" } else { "Enable effects" });
    skin::mixer_switch(ui.c, fx, false, enabled, hot);
    if n != 0 {
        let disk = Rect::new(fx.x, fx.bottom() + 3.0, fx.w, fx.h);
        let armed = s.mixer.recording.armed.contains(&n);
        let hot = ui.hit(disk, Action::Mix(MHit::Disk(n)), if armed { "Disarm disk recording" } else { "Arm disk recording" });
        skin::mixer_switch(ui.c, disk, true, armed, hot);
    }
    let route=Rect::new(r.x+5.,r.bottom()-16.,r.w-10.,14.);
    if chosen {ui.c.text_center(Font::vintage(10),route,ui.t.accent,"v");}
    else if let Some(_level)=mx::send(s,s.selected_mixer,n) {
        let id=KnobId::Send(s.selected_mixer,n);
        let send=Rect::new(route.x,route.y,route.w-8.,route.h);
        ui.hit(send,Action::Knob(id),s.knob_text(id));
        let remove=Rect::new(send.right(),route.y,8.,route.h);
        ui.hit(remove,Action::Mix(MHit::Unroute(n)),format!("Remove send to {name}"));
        ui.c.text_center(Font::vintage(8),remove,ui.t.text_dim,"x");
        ui.c.fill_rect(send.inset(2.),Color::rgb(0x40555a));
        ui.c.text_center(Font::vintage(9),send,Color::rgb(0xc6d9a7),"S");
    } else {ui.hit(route,Action::Mix(MHit::Route(n)),format!("Route to {name}"));chevron(ui,route.cx(),route.cy(),true,ui.t.text_dim);}
    if separator {
        skin::mixer_divider(ui.c, Rect::new(r.x, r.y, 5.0, r.h));
    }
    if chosen || mx::is_selected(s,n) {ui.c.stroke_rrect(r.inset(1.),0.,1.,Color::rgb(if chosen {0xabb7bb}else{0x7e8b91}));}
}
/// The same insert bank and controls in both vintage mixer views.
pub(super) fn inspector(ui: &mut Ui, s: &State, window: Rect) {
    let panel = Rect::new(window.x + 3.0, window.y + 26.0, window.w - 6.0, window.h - 30.0);
    if panel.h <= 0.0 {
        return;
    }
    skin::metal(ui.c, panel, Color::rgb(0x5d6b72));
    let fx_h = (panel.h * 0.55).clamp(208.0, 330.0).min((panel.h - 62.0).max(0.0));
    ui.c.push_clip(panel);
    ui.clip_hit_rect(panel, |ui| {
        ui.hit(panel, Action::Swallow, "");
        if fx_h >= 22.0 {
            effects(ui, s, Rect::new(panel.x + 3.0, panel.y + 3.0, panel.w - 6.0, fx_h));
        }
        selected(ui, s, Rect::new(panel.x + 4.0, panel.y + fx_h + 8.0, panel.w - 8.0, panel.h - fx_h - 12.0));
    });
    ui.c.pop_clip();
}
fn effects(ui:&mut Ui,s:&State,r:Rect) {
    let n=s.selected_mixer;
    let count=mx::slot_count(s,n);
    let first=s.mixer.slot_scroll.get(&n).copied().unwrap_or(0).min(count.saturating_sub(10));
    ui.hit(r,Action::Mix(MHit::SlotScroll),"Scroll effect slots");
    let h=(r.h-22.)/10.;
    let scale = ui.c.scale;
    let pixel = 1.0 / scale;
    let edge = |i: usize| ((r.y + i as f32 * h) * scale).round() / scale;
    ui.c.fill_rect(Rect::span(r.x, r.right(), edge(0), edge(10) + pixel), Color::rgb(0x202d33));
    for i in 0..10 {
        let k=mx::visible_user_slot(s,n,first+i);
        let row=Rect::new(r.x,r.y+i as f32*h,r.w,h-1.);
        // Adjacent rows share a snapped edge, so every bevel has the same pixel thickness.
        let y = edge(i);
        ui.c.fill_rect(Rect::span(r.x + 2.0, r.right() - 1.0, y + pixel, edge(i + 1)), Color::rgb(0x4d6066));
        ui.c.fill_rect(Rect::new(r.x + 2.0, y, r.w - 3.0, pixel), Color::rgb(0x202d33));
        ui.c.fill_rect(Rect::new(r.x + 2.0, y + pixel, r.w - 3.0, pixel), Color::rgb(0x96a3a6));
        let name=mx::slot_name(s,n,k).unwrap_or_else(||format!("Slot {}",first+i+1));
        ui.hit(Rect::new(row.x,row.y,18.,row.h),Action::Mix(MHit::SlotArrow(k)),"Effect slot options");
        ui.hit(Rect::new(row.x+18.,row.y,row.w-60.,row.h),Action::Mix(MHit::SlotName(k)),name.clone());
        chevron(ui,row.x+9.,row.cy(),true,ui.t.text_dim);
        ui.c.text_clip(Font::vintage(11),row.x+20.,row.cy()-6.,row.w-64.,ui.t.text,&name);
        let id=KnobId::SlotMix(n,k);
        let kh=row.h.min(18.);
        let kr=Rect::new(row.right()-40.,row.cy()-kh*0.5,18.,kh);
        ui.hit(kr,Action::Knob(id),s.knob_text(id));mixer_knob(ui,kr.cx(),kr.cy(),6.,8.,s.knob(id),Some(false));
        let led=Rect::new(row.right()-17.,row.cy()-7.,14.,14.);
        ui.hit(led,Action::Mix(MHit::SlotLed(k)),"Enable / bypass slot");
        if mx::track(s,n).is_none_or(|m|m.slot_on[k as usize]) {art::led_on(ui.c,led);}else{art::led_off(ui.c,led);}
    }
    let add=Rect::new(r.x,r.bottom()-20.,r.w,20.);
    ui.hit(add,Action::Mix(MHit::AddSlot),"Add another effect slot");
    skin::metal(ui.c,add,Color::rgb(0x63767c));
    ui.c.text(Font::vintage(11),add.x+6.,add.y+3.,ui.t.text,&format!("+ Add slot     {}–{} / {}",first+1,(first+10).min(count),count));
}
fn selected(ui:&mut Ui,s:&State,r:Rect) {
    let n=s.selected_mixer;
    let track=mx::track(s,n);
    let input=Rect::new(r.x,r.y,r.w,22.);
    skin::recess(ui.c,input,Color::rgb(0x34474d));
    ui.hit(input,Action::Mix(MHit::Input(n)),"Audio input / recording source");
    let label=s.mixer.recording.inputs.get(&n).map(|i|i.label()).unwrap_or_else(||"Input: (none)".into());
    ui.c.text_clip(Font::vintage(11),input.x+5.,input.y+4.,input.w-15.,ui.t.text,&label);
    chevron(ui,input.right()-8.,input.cy(),true,ui.t.text_dim);
    for (i,(label,action,on)) in [("POL",MHit::Polarity(n),track.is_some_and(|m|m.polarity)),("SWAP",MHit::Swap(n),track.is_some_and(|m|m.swap))].into_iter().enumerate() {
        let b=Rect::new(r.x+i as f32*49.,r.y+28.,45.,20.);
        ui.hit(b,Action::Mix(action),if i==0 {"Reverse polarity"}else{"Swap left / right"});
        skin::metal(ui.c,b,Color::rgb(if on {0xa58561}else{0x697a80}));ui.c.text_center(Font::vintage(10),b,ui.t.text,label);
    }
    let sep=KnobId::MixSep(n);let k=Rect::new(r.right()-25.,r.y+26.,22.,24.);
    ui.hit(k,Action::Knob(sep),s.knob_text(sep));art::knob(ui.c,k,s.knob(sep));
    ui.c.text_clip(Font::vintage(9),r.x+101.,r.y+32.,(r.w-130.).max(0.),ui.t.text_dim,"STEREO");
    if crate::workspace_visibility::shown("visible_mixer-eq") {crate::mini_eq::draw(ui,s,Rect::new(r.x,r.y+54.,r.w,(r.h-54.).max(30.)));}
}
