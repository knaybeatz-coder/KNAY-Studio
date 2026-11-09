// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Mixer window (wide layout: strips plus the selected track's slot panel).
// spec: ui/main-window.md §7 (layout), ui/mixer.md (controls, hints, routing look); fader law: audio/gain-laws.md

mod analog;
mod clean;
mod original;
mod vintage_cap;
mod console_skin;
mod themed;
pub use console_skin::Finish;
mod material;

use std::f32::consts::PI;

use os_ui::kit::lemon as lk;
use os_ui::{Color, Font, Glyph, Rect};

use super::{led, resize_zones, window_body, window_buttons, FrameInfo};
use crate::knobs::KnobId;
use crate::app::{Action, Live, State, Ui, Win, TITLE_H};
use crate::meters;
use crate::mixer::{self as mx, MHit};
use crate::icons::{self, Icon};

mod traditional;
mod vintage_compact;
fn insertion_button(ui:&mut Ui,s:&State,area:Rect,client:Rect,n:u16,x:f32){
        if client.h<=72. || x<area.x || x>area.right()-10.0 {return;}
        let lane=Rect::new(x-4.0,client.y,8.0,client.h-52.0);
        let action=Action::Mix(MHit::InsertBefore(n));
        ui.hit(lane,Action::Mix(MHit::InsertBoundary(n)),"Pause here to insert a mixer track");
        if s.mixer.insertion_ready && s.mixer.insertion_hover.is_some_and(|(t,_,_)|t==n) && lane.outset(5.).contains(ui.mouse.0,ui.mouse.1) {
            let y=ui.mouse.1.clamp(lane.y+10.0,lane.bottom()-10.0);
            let button=Rect::new(x-9.0,y-9.0,18.0,18.0);
            ui.hit(button,action,"Insert an empty mixer track here");
            ui.c.fill_rect_gradient(button,ui.t.accent.mix(ui.t.button_hi,0.48),ui.t.button_hi,9.0);
            ui.c.stroke_rrect(button,9.0,1.0,ui.t.accent);
            icons::draw(ui.c,Icon::Plus,button.inset(3.0),ui.t.tx(0xffffff));
        }
}


pub const STRIP_W: f32 = 84.0;
pub(crate) const SECTION_SHORTCUTS: &str = "visible_mixer-section-shortcuts";
pub(crate) const STEREO_SEPARATION: &str = "visible_mixer-stereo-separation";
pub fn strip_width(s: &State) -> f32 {if mx::view(s)==crate::rack_mixer::VIEW{return s.mixer.rack_width;}if mx::traditional(s) {traditional::width(s)} else {STRIP_W}}
pub fn channel_width(s:&State,n:u16)->f32{
    let base=strip_width(s);
    if mx::view(s)==crate::rack_mixer::VIEW && crate::rack_mixer::slot(s,n).is_some(){s.mixer.rack_auto_width.get(&n).copied().unwrap_or(base).max(32.)}else{base}
}
pub fn channel_offset(s:&State,n:u16)->f32{
    if mx::view(s)!=crate::rack_mixer::VIEW{return strip_width(s)*f32::from(n.saturating_sub(1));}
    (1..n).map(|n|channel_width(s,n)).sum()
}
pub fn fader_per_px(s: &State,track:u16) -> f64 {
    let r=super::frame_rects(s,Win::Mixer,s.win_rect(Win::Mixer)).0;
    if mx::traditional(s) {
        return if s.vintage_2005 { traditional::vintage_fader_per_px(r) }
            else { 1.0 / (440.0 * f64::from(traditional::scale(r))) };
    }
    if s.vintage_2005 && mx::view(s) == mx::WIDE {
        let top = Ly::for_state(r, s).y(TRACK_Y0);
        return 1.0 / f64::from((r.bottom() - 105.0 - top).max(1.0));
    }
    if mx::view(s)==crate::rack_mixer::VIEW&&track!=0 {let ly=Ly::for_state(r,s);return 1. / f64::from((r.bottom()-78.-(ly.y(LED_Y)+12.)).max(1.));}
    FADER_PER_PX / f64::from(if mx::view(s)==crate::rack_mixer::VIEW{scale(r)*STRIP_FADER_COMPRESSION}else{fader_scale(r)})
}
/// Shorter channel-strip fader travel: 0.8 is unity, zero lands at reference y=970.
pub const FADER_PER_PX: f64 = 1.0 / 274.0;
/// Fader change per wheel notch: 4 px of drag. Fitted to the one measurement (spec: ui/mixer.md §4): three notches up
/// from unity read `103%  0.8dB  1.09`, which puts a notch between 0.0088 and 0.0092.
pub const FADER_NOTCH: f64 = 4.0 * FADER_PER_PX;

// Vertical layout at the default 1080p window (spec §7): absolute y for a window top of 83;
// bottom-anchored values are offsets from the window bottom.
const HEAD_Y0: f32 = 107.0; // header numbers row
const HEAD_Y1: f32 = 127.0;
const METER_Y0: f32 = 130.0;
const METER_Y1: f32 = 620.0;
const PANEL_Y0: f32 = 128.0; // dark head panel behind meter, name, LED, pan
const PANEL_Y1: f32 = 687.0;
const LED_Y: f32 = 652.0;
const PAN_Y: f32 = 675.5;
const SEP_Y: f32 = 706.0;
const SWAP_Y: f32 = 724.0;
const RING_Y: f32 = 742.0;
const TRACK_Y0: f32 = 704.0;
const HANDLE_0DB: f32 = 750.8; // handle centre at 0 dB (value 0.8)

/// dB marks of the level scale and their measured y positions (number centres).
const SCALE: [(&str, f32); 17] = [
    ("3", 181.0), ("0", 241.0), ("3", 301.0), ("6", 360.0), ("9", 419.0), ("12", 479.0), ("15", 535.0), ("18", 588.0),
    ("21", 639.0), ("24", 686.0), ("27", 728.0), ("30", 766.0), ("33", 799.0), ("36", 826.0), ("39", 848.0), ("42", 866.0),
    ("45", 880.0),
];

/// The "C" column's marks as (label, reference y), top down: the default scale's `SCALE`, its first mark the "+3"; with
/// `alt` ("Alternate meter scale") 0, 3 … 30 at the heights the default law puts the levels `meters::ALTERNATE` maps
/// them to, with no +3 (spec: ui/settings-choices.md "Miscellaneous: seven more checks, Default template and Startup
/// project").
pub fn scale_marks(alt: bool, floor: f32) -> Vec<(&'static str, f32)> {
    if !alt {
        return SCALE.to_vec();
    }
    SCALE[1..].iter().zip(meters::ALTERNATE).map(|(&(m, _), (_, d))| (m, meters::law_y(d, floor, false))).collect()
}

/// Height of the mixer window the absolute y values above were measured at (1080p workspace).
const REF_H: f32 = 997.0;
/// Reference y of the window top.
const REF_TOP: f32 = 83.0;
/// Height of the parts that do not scale: header, head-panel controls and the bottom controls.
const FIXED_H: f32 = 218.0;
const FADER_COMPRESSION: f32 = 0.88;
const STRIP_FADER_COMPRESSION:f32=0.55;

/// How much the mixer's stretchy parts (meter, level scale, fader) are scaled at window `r`:
/// 1 at the measured height; the rest of the strip keeps its size.
pub fn scale(r: Rect) -> f32 {
    ((r.h - FIXED_H) / (REF_H - FIXED_H)).max(0.2)
}

/// The actual fader travel scale, shared by drawing and pointer input.
pub fn fader_scale(r:Rect)->f32 {scale(r)*FADER_COMPRESSION}

/// Give the space saved from the fader to the channel controls; bottom stays fixed.
struct Ly {
    top: f32,
    k: f32,
    compression:f32,
}

impl Ly {
    fn new(r: Rect) -> Ly {
        Ly { top: r.y, k: scale(r),compression:FADER_COMPRESSION }
    }

    fn for_state(r: Rect, s: &State) -> Ly {
        let mut ly = Self::new(r);
        if mx::view(s) == crate::rack_mixer::VIEW || (s.vintage_2005 && mx::view(s) == mx::WIDE) {
            ly.compression = STRIP_FADER_COMPRESSION;
        }
        ly
    }

    fn y(&self, abs: f32) -> f32 {
        let (m0, m1, f0, f1) = (130.0 - REF_TOP, 620.0 - REF_TOP, 692.0 - REF_TOP, 981.0 - REF_TOP);
        let ry = abs - REF_TOP;
        let k = self.k;
        let panel=(m1-m0)+(f1-f0)*(1.-self.compression);
        let y = if ry < m0 {
            ry
        } else if ry < m1 {
            m0 + (ry - m0) / (m1-m0) * panel * k
        } else if ry < f0 {
            m0 + panel * k + (ry - m1)
        } else if ry < f1 {
            m0 + panel * k + (f0 - m1) + (ry - f0) * k * self.compression
        } else {
            m0 + panel * k + (f0 - m1) + (f1 - f0) * k * self.compression + (ry - f1)
        };
        self.top + y
    }
}

fn channel_lock(ui: &mut Ui, s: &State, track: u16, rect: Rect) {
    if !crate::workspace_visibility::shown("visible_mixer-channel-lock") || mx::track(s, track).is_none() { return; }
    let locked = mx::locked(s, track);
    let hot = ui.hit(rect, Action::Mix(MHit::Lock(track)),
        if locked { "Unlock Mixer channel" } else { "Lock Mixer channel" });
    let t = ui.t;
    if hot || locked {
        ui.c.fill_rrect(rect, 2., if t.style.vintage_2005 { t.sunken } else { t.button_hi });
    }
    let ink = if locked && t.style.vintage_2005 { t.led(0xa0dd48) }
        else if locked { t.accent } else if hot { t.text } else { t.text_dim };
    super::playlist::draw_padlock(ui.c, rect.cx(), rect.cy(), (rect.w / 14.).min(0.86), ink, locked);
}

pub fn fader_db_text(v: f64) -> String {
    let gain = os_engine::laws::fader_gain(v);
    if gain <= 0.0 { "-inf dB".into() } else { format!("{:+.1} dB", 20.0 * gain.log10()) }
}


pub fn paint(ui: &mut Ui, s: &State, r: Rect, f: &FrameInfo) {
    if mx::traditional(s) {traditional::paint(ui,s,r,f);return;}
    let width = strip_width(s);
    let panel_w = 221.0;
    window_body(ui, r);
    let outer = r;
    let inner = super::float_frame(ui, s, Win::Mixer, r);
    let r = inner.unwrap_or(r);
    let full_title = r;
    let left = s.vintage_2005 || mx::inspector_left();
    let inspector = Rect::new(if left { r.x } else { r.right() - panel_w }, r.y, panel_w, r.h);
    let r = if left { Rect::new(r.x + panel_w, r.y, r.w, r.h) } else { r };
    let client = super::window_frame_ex(ui, Win::Mixer, Rect::new(r.x, r.y, r.w - panel_w, r.h), "", 0.0, false, inner.is_some());
    title_tools(ui, s, r);
    if mx::view(s)==crate::rack_mixer::VIEW && mx::selected(s).len()==1 {
        let notice=Rect::new(r.x+136.,r.y+2.,(r.w-panel_w-218.).max(0.),19.);
        ui.c.text_clip(Font::new(11),notice.x,notice.y+2.,notice.w,ui.t.text_dim,"Waves: tested  ·  UAD: supported, untested");
        ui.hit(notice,Action::Swallow,"Supported Waves channel strips have been tested. Compatible resizable UAD channel strips are supported but untested.");
    }
    if r.h < 2.0 * TITLE_H {
        // Collapsed to the title bar: only the slot panel's title with the window buttons.
        s.cables.borrow_mut().clear();
        slot_panel(ui, s, inspector);
        window_buttons(ui, Win::Mixer, full_title.right() - 58.0, full_title.y, ui.t.tx(0xd4dade), ui.t.tx(0xffffff));
        return;
    }
    let ly = Ly::for_state(r,s);

    let strips = Rect::new(r.x, client.y, r.w - panel_w - 7.0, client.h);
    let fill = if ui.mixer_finish == Finish::Theme { ui.t.mixer_bg } else { ui.t.s(0x232527) };
    ui.c.fill_rect(Rect::new(r.x, client.y, r.w - panel_w, client.h), fill);
    ui.hit(strips, Action::MixerArea, "Mixer");
    let inserts = mx::inserts(s);
    let bottom = r.bottom();

    // Fixed master strip; inserts scroll immediately beside it.
    let master_header=Rect::new(r.x,ly.y(HEAD_Y0),width,HEAD_Y1-HEAD_Y0+1.);
    if ui.t.style.vintage_2005 {
        vintage_number(ui, master_header, 0, s.selected_mixer == 0);
    } else {
        if matches!(mx::view(s), mx::WIDE | crate::rack_mixer::VIEW) {
            let tint = s.project.mixer.iter().find(|m| m.index == 0).and_then(|m| m.color).map(super::rgb);
            number_plate(ui, master_header, tint);
        } else {
            ui.c.fill_rect(master_header, ui.t.s(0x2b2e30));
        }
        ui.c.text_center(console_font(ui,s,14),Rect::new(r.x,ly.y(HEAD_Y0),width,21.),ui.t.text,"M");
    }
    let lemon = super::lemon(ui);
    let area = Rect::new(r.x + 9.0 + width, client.y, strips.w - 9.0 - width, client.h);
    ui.c.fill_rect(Rect::new(r.x + width, client.y, 9.0, client.h), ui.t.s(0x1a1e20));
    for k in 0..3 {
        ui.c.fill_rect(Rect::new(r.x + 4.0 + width, ly.y(497.0 + 3.0 * k as f32), 1.0, 1.0), ui.t.s(0x3a3e41));
    }
    ui.c.push_clip(area);
    let mut end=0.;
    let mut visible=Vec::new();
    for n in 1..=inserts {
        let x=end-s.mixer_scroll;
        let w=channel_width(s,n);
        if x+w>0. && x<area.w{visible.push((n,area.x+x));}
        end+=w;
    }
    for &(n,left) in &visible {
        if mx::view(s) == mx::WIDE {
            let bounds=Rect::new(left,client.y,channel_width(s,n),client.h);
            ui.part(crate::app::Seg::MixerStrip(n as u16),bounds,|ui|strip(ui,s,n as u16,left,bottom,f,&ly));
        } else {
            strip(ui,s,n as u16,left,bottom,f,&ly);
        }
    }
    // Narrow boundary targets reveal an insertion button only while hovered.
    for &(n,x) in &visible {
        insertion_button(ui,s,area,client,n as u16,x);
    }
    // "+" after the last strip, at the fader handle's height (spec: ui/mixer.md §1).
    let px = area.x + end - s.mixer_scroll + 14.0;
    let pb = Rect::new(px - 9.0, ly.y(HANDLE_0DB) - 9.0, 18.0, 18.0);
    if px < area.right() + 10.0 {
        let hov = ui.hit(pb, Action::Mix(MHit::Add), "Add more tracks");
        if hov && lemon {
            ui.c.fill_rrect(pb, lk::r_btn(ui.t), ui.t.button_hi);
        } else if hov {
            ui.c.stroke_rrect(pb, 3.0, 1.0, ui.t.h(0xf09a28));
        }
        icons::draw(ui.c, Icon::Plus, pb.inset(3.0), if hov { ui.t.tx(0xffffff) } else { ui.t.tx(0x9aa1a5) });
    }
    ui.c.pop_clip();
    strip(ui, s, 0, r.x, bottom, f, &ly);

    // Horizontal scroll bar under the strips.
    let sb = if s.vintage_2005 {
        Rect::new(area.x + 13.0, bottom - 9.0, area.w - 20.0, 7.0)
    } else {
        Rect::new(area.x + 13.0, bottom - 14.0, area.w - 20.0, 9.0)
    };
    super::scrollbar(ui, sb, false, s.mixer_scroll, area.w, end+width, crate::app::Scroll::MixerX);
    chevron(ui, area.x + 5.0, sb.cy(), false, ui.t.s(0x5d6366));
    if crate::workspace_visibility::shown("visible_mixer-cables") {
        cables(ui, s, strips, area, bottom);
    } else {
        s.cables.borrow_mut().clear();
    }

    slot_panel(ui, s, inspector);
    window_buttons(ui, Win::Mixer, full_title.right() - 58.0, full_title.y, ui.t.tx(0xd4dade), ui.t.tx(0xffffff));
    resize_zones(ui, s, Win::Mixer, outer);
}

/// The routing cables (spec: ui/mixer.md §5): one from the current track's ⬇ into each of its send knobs, sagging
/// and swinging (crate::cable). Sockets on strips scrolled out of view still hold their cable's end.
fn cables(ui: &mut Ui, s: &State, strips: Rect, area: Rect, bottom: f32) {
    let width = strip_width(s);
    let socket_x = |n: u16| match n {
        0 => strips.x + width / 2.0,
        n => area.x + channel_offset(s,n) - s.mixer_scroll + channel_width(s,n) / 2.0,
    };
    // Each cable is plugged in under the middle of its ⬇ and its knob, which are drawn again over the cables' ends.
    let (ay, ky) = (bottom - 38.0, bottom - 40.0);
    let cur = s.selected_mixer;
    let from = (socket_x(cur), ay);
    let sends: Vec<(u16, f64)> = (0..=mx::inserts(s)).filter(|&n| n != cur).filter_map(|n| Some((n, mx::send(s, cur, n)?))).collect();
    if s.vintage_2005 {
        s.cables.borrow_mut().clear();
        if crate::workspace_visibility::shown("visible_mixer-cables") {
            for &(n,level) in &sends {
                ui.c.push_clip(if cur==0 || n==0 {strips}else{area});
                crate::vintage::cable(ui.c,from,(socket_x(n),ky),bottom - 10.0,if level>0. {ui.t.accent}else{ui.t.text_dim});
                ui.c.pop_clip();
            }
        }
        if !sends.is_empty() && crate::workspace_visibility::shown("visible_mixer-cables") {
            ui.c.push_clip(if cur==0 {strips}else{area});
            down_arrow(ui,from.0,ay);ui.c.pop_clip();
            for &(n,level) in &sends {
                ui.c.push_clip(if n==0 {strips}else{area});
                send_knob(ui,s,n,socket_x(n),ky,level);ui.c.pop_clip();
            }
        }
        return;
    }
    let mut cables = s.cables.borrow_mut();
    cables.begin(cur);
    for &(n, level) in &sends {
        // A send at zero is dimmed, as its knob.
        let col = if level <= 0.0 { ui.t.accent.mix(ui.t.s(0x26292b), 0.65) } else { ui.t.accent };
        cables.connect(n, from, (socket_x(n), ky), cur == 0 || n == 0, col);
    }
    cables.end();
    // Cables between inserts are clipped where the inserts scroll; those into the master are not.
    for (master, pts, col) in cables.each() {
        ui.c.push_clip(if master { strips } else { area });
        ui.c.polyline(pts, 2.0, col);
        ui.c.pop_clip();
    }
    drop(cables);
    if sends.is_empty() {
        return;
    }
    let clip = |n: u16| if n == 0 { strips } else { area };
    ui.c.push_clip(clip(cur));
    down_arrow(ui, from.0, ay);
    ui.c.pop_clip();
    for (n, level) in sends {
        ui.c.push_clip(clip(n));
        send_knob(ui, s, n, socket_x(n), ky, level);
        ui.c.pop_clip();
    }
}

fn title_tools(ui: &mut Ui, s: &State, r: Rect) {
    let count=mx::selected(s).len();
    if count>1{ui.c.text_clip(Font::new(12),r.x+136.,r.y+3.,(r.w-224.).max(0.),ui.t.accent,&format!("{count} tracks selected"));}
    let y = r.y + 11.5;
    let ob = Rect::new(r.x + 3.0, y - 9.0, 16.0, 18.0);
    let hov = ui.hit(ob, Action::Mix(MHit::Options), "Mixer options");
    chevron(ui, r.x + 11.0, y, true, if hov { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
    let pb = Rect::new(r.x + 27.0, y - 8.0, 16.0, 16.0);
    let hov = ui.hit(pb, Action::Mix(MHit::AddAfter), "Add a new track after the selected track");
    icons::draw(ui.c, Icon::Plus, pb.inset(1.0), if hov { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
    for (i, mode, label) in [(0, mx::WIDE, "Full mixer view"), (1, mx::TRADITIONAL+s.mixer.traditional_layout, "Traditional FL-style mixer view"),(2,crate::rack_mixer::VIEW,"Channel Strip mixer view")] {
        if s.vintage_2005 && mode == crate::rack_mixer::VIEW { continue; }
        let mode = if s.vintage_2005 && mode >= mx::TRADITIONAL { mx::TRADITIONAL } else { mode };
        let b=Rect::new(r.x+50.0+i as f32*26.0,y-9.0,23.0,18.0);
        let hover=ui.hit(b,Action::Mix(MHit::View(mode)),label);
        let active=mx::view(s)==mode;
        ui.c.fill_rrect(b,3.0,if active {ui.t.accent} else if hover {ui.t.button_hi} else {ui.t.s(0x292f32)});
        let ink=if active {ui.t.s(0x182024)} else {ui.t.text};
        if mode==crate::rack_mixer::VIEW {
            // One vertical console module with stacked knobs, deliberately unlike either grid icon.
            ui.c.stroke_rrect(Rect::new(b.x+7.,b.y+2.,9.,14.),1.,1.,ink);
            for y in [5.,8.,11.] {ui.c.fill_circle(b.x+11.5,b.y+y,1.2,ink);}
            ui.c.fill_rect(Rect::new(b.x+9.,b.y+14.,5.,1.),ink);
            continue;
        }
        let count=if mode>=mx::TRADITIONAL {3} else {2};
        let w=if mode>=mx::TRADITIONAL {4.0} else {7.0};
        for col in 0..count {
            let x=b.x+3.0+col as f32*(w+2.0);
            ui.c.stroke_rrect(Rect::new(x,b.y+3.0,w,12.0),0.0,1.0,ink);
            ui.c.fill_rect(Rect::new(x,b.y+8.0,w,2.0),ink);
        }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u32;
    Color::rgb(ch(a.r, b.r) << 16 | ch(a.g, b.g) << 8 | ch(a.b, b.b))
}

fn shade(c: Color, k: f32) -> Color {
    mix(Color::rgb(0), c, k)
}

/// One meter bar (spec: ui/mixer.md §9, measured from FL): the colour depends on the height, not on the level: green up
/// to "Yellow from", yellow-orange up to "Red from", red above; each zone is a slight vertical gradient. The outer
/// columns are darker, the next ones a touch lighter; a 1 px dark cap sits on the top and the peak-hold line is a
/// 1 px orange line. `ys` maps a level to y; `bottom` is the bar's foot.
#[allow(clippy::too_many_arguments)]
fn analog_vu(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, selected: bool) {
    let ink = Color::rgb(0x302c24);
    let red = Color::rgb(0xa13d2d);
    ui.c.fill_rrect(r.outset(2.0), 5.0, Color::rgb(0x101213));
    ui.c.stroke_rrect(r.outset(1.0), 4.0, 1.0, if selected { ui.t.accent } else { Color::rgb(0x55534a) });
    ui.c.push_clip(r);
    ui.c.fill_rect(r, Color::rgb(0xe8d9af));
    for row in 0..r.h.ceil() as usize {
        let color = Color::rgb(0xf6e9bf).mix(Color::rgb(0xd8c89c), row as f32 / r.h);
        // Overlap rows on an opaque base: fractional DPI must not expose dark seams.
        ui.c.fill_rect(Rect::new(r.x, r.y + row as f32, r.w, 2.0), color);
    }
    let cx = r.cx();
    let cy = r.bottom() - 13.0;
    let radius = (r.w * 0.40).min(r.h * 0.78);
    let point = |fraction: f32, radius: f32| {
        let angle = (-150.0 + 120.0 * fraction) * PI / 180.0;
        (cx + radius * angle.cos(), cy + radius * angle.sin())
    };
    for i in 0..70 {
        let a = i as f32 / 70.0;
        let b = (i + 1) as f32 / 70.0;
        let (x0, y0) = point(a, radius);
        let (x1, y1) = point(b, radius);
        ui.c.line(x0, y0, x1, y1, if a > meters::vu_fraction(0.0) { 2.5 } else { 1.0 }, if a > meters::vu_fraction(0.0) { red } else { ink });
    }
    for db in [-20.0, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0] {
        let f = meters::vu_fraction(db);
        let major = matches!(db, -20.0 | -5.0 | 0.0 | 3.0);
        let (x0, y0) = point(f, radius - if major { 6.0 } else { 3.5 });
        let (x1, y1) = point(f, radius);
        let color = if db > 0.0 { red } else { ink };
        ui.c.line(x0, y0, x1, y1, 1.0, color);
        if major && r.h > 48.0 {
            let (x, y) = point(f, radius + 8.0);
            let x = x.clamp(r.x + 9.0, r.right() - 9.0);
            let label = if db > 0.0 { format!("+{db:.0}") } else { format!("{db:.0}") };
            ui.c.text_center(Font::new(9), Rect::new(x - 9.0, y - 5.0, 18.0, 10.0), color, &label);
        }
    }
    ui.c.text_center(Font::new(12), Rect::new(cx - 15.0, cy - 26.0, 30.0, 14.0), ink, "VU");
    // Combine rectified, smoothed stereo levels; anti-phase audio cannot cancel.
    let level = (levels[0] + levels[1]) * 0.5;
    let (x, y) = point(meters::vu_fraction(meters::vu_db(level)), radius - 2.0);
    ui.c.line(cx + 1.0, cy + 1.0, x + 1.0, y + 1.0, 1.5, Color::rgb(0x8c805f).with_alpha(100));
    ui.c.line(cx, cy, x, y, 1.3, ink);
    ui.c.fill_circle(cx, cy, 3.5, ink);
    ui.c.fill_circle(cx, cy, 1.5, Color::rgb(0x84795b));
    ui.c.fill_circle(r.right() - 7.0, r.bottom() - 7.0, 2.5, if clipped { Color::rgb(0xf44030) } else { Color::rgb(0x685743) });
    ui.c.fill_polygon(&[(r.x, r.y), (r.right(), r.y), (r.right(), r.y + r.h * 0.15), (r.x, r.y + r.h * 0.38)], Color::rgb(0xffffff).with_alpha(26));
    ui.c.stroke_rrect(r.inset(0.8), 2.0, 1.0, Color::rgb(0xffffff).with_alpha(75));
    ui.c.pop_clip();
}

fn vu_bar(ui: &mut Ui, r: Rect, amplitude: f32) {
    let height = meters::vu_fraction(meters::vu_db(amplitude)) * r.h;
    let zero = meters::vu_fraction(0.0) * r.h;
    ui.c.fill_rect(Rect::new(r.x, r.bottom() - height.min(zero), r.w, height.min(zero)), ui.t.a(0xd6c16e));
    if height > zero {
        ui.c.fill_rect(Rect::new(r.x, r.bottom() - height, r.w, height - zero), ui.t.a(0xd87848));
    }
}

fn meter_bar(ui: &mut Ui, x: f32, w: f32, bottom: f32, db: f32, hold: f32, cfg: &meters::Settings, ys: &dyn Fn(f32) -> f32) {
    use meters::k;
    let t = ui.t;
    let full_top = ys(12.0);
    let grad = cfg.v[k::GRADIENT];
    let edge = cfg.v[k::EDGE];
    // Zone colours at the zone's top and bottom (measured 0xa0db40 / 0xb6db4d, 0xeec757 / 0xfac050).
    let green = (ui.t.a(0xa0db40), ui.t.a(0xb6db4d));
    let yellow = (ui.t.h(0xeec757), ui.t.h(0xfac050));
    let red = (t.meter_clip, t.meter_clip);
    let y_yel = ys(cfg.v[k::YELLOW_DB]);
    let y_red = ys(cfg.v[k::RED_DB].max(cfg.v[k::YELLOW_DB]));
    if db > meters::SILENT {
        let top = ys(db).min(bottom);
        for (z0, z1, (ct, cb)) in [(y_yel, bottom, green), (y_red, y_yel, yellow), (full_top, y_red, red)] {
            let (a, b) = (top.max(z0), z1.min(bottom));
            if b - a < 0.5 {
                continue;
            }
            // Colours at a and b along the zone's gradient (scaled by the gradient setting).
            let at = |y: f32| mix(ct, mix(ct, cb, grad), (y - z0) / (z1 - z0).max(1.0));
            let (ca, cb2) = (at(a), at(b));
            let r = Rect::new(x, a, w, b - a);
            if t.style.vintage_2005 {
                let mut y = bottom - ((bottom - b) / 4.0).ceil() * 4.0;
                while y > a {
                    ui.c.fill_rect(Rect::new(x, (y - 3.0).max(a), w, (y - a).min(3.0)), at(y));
                    y -= 4.0;
                }
                continue;
            }
            ui.c.fill_rect_gradient(r, ca, cb2, 0.0);
            if w >= 4.0 {
                let hl = |c: Color| mix(c, Color::rgb(0xffffff), 0.04);
                ui.c.fill_rect_gradient(Rect::new(x + 1.0, a, 1.0, b - a), hl(ca), hl(cb2), 0.0);
                ui.c.fill_rect_gradient(Rect::new(x + w - 2.0, a, 1.0, b - a), hl(ca), hl(cb2), 0.0);
                ui.c.fill_rect_gradient(Rect::new(x, a, 1.0, b - a), shade(ca, edge), shade(cb2, edge), 0.0);
                ui.c.fill_rect_gradient(Rect::new(x + w - 1.0, a, 1.0, b - a), shade(ca, edge), shade(cb2, edge), 0.0);
            }
        }
        if cfg.v[k::CAP] >= 0.5 && top - 1.0 > full_top {
            ui.c.fill_rect(Rect::new(x, top - 1.0, w, 1.0), ui.t.s(0x181c1e));
        }
    }
    if cfg.v[k::HOLD_ON] >= 0.5 && hold > meters::SILENT {
        let y = ys(hold).min(bottom - 1.0);
        let c = ui.t.h(0xe16e2f);
        ui.c.fill_rect(Rect::new(x, y, w, 1.0), c);
        if w >= 4.0 {
            ui.c.fill_rect(Rect::new(x, y, 1.0, 1.0), shade(c, edge));
            ui.c.fill_rect(Rect::new(x + w - 1.0, y, 1.0, 1.0), shade(c, edge));
        }
    }
}

fn number_plate(ui: &mut Ui, rect: Rect, tint: Option<Color>) {
    console_skin::plate(ui, rect);
    if matches!(ui.mixer_finish, Finish::CleanModern | Finish::Analog) {
        if let Some(color) = tint {
            ui.c.fill_rrect(rect.inset(1.5), 2., color.with_alpha(62));
        }
    }
}

fn vintage_number(ui: &mut Ui, r: Rect, number: u16, selected: bool) {
    crate::vintage::metal(ui.c, r, Color::rgb(if selected { 0x77848a } else { 0x5f686d }));
    crate::vintage::mixer_digits(ui.c, r, number);
}

/// One strip; `left` is its left edge (44 px pitch). Columns: knobs at +22, meter / fader at +32,
/// name and small controls at +12 (spec §7).
fn strip(ui: &mut Ui, s: &State, n: u16, left: f32, bottom: f32, f: &FrameInfo, ly: &Ly) {
    let show_meters = crate::workspace_visibility::shown("visible_mixer-meters");
    let full=mx::view(s)==mx::WIDE;
    let vintage_full = full && ui.t.style.vintage_2005;
    let console=full||mx::view(s)==crate::rack_mixer::VIEW;
    let width=channel_width(s,n);
    let vertical=mx::view(s)==crate::rack_mixer::VIEW && n!=0;
    let t = ui.t;
    let track = s.project.mixer.iter().find(|m| m.index == n);
    let selected = s.selected_mixer == n;
    let also = !selected && mx::is_selected(s, n);
    let kx=left+width/2.;
    let (mx,sx)=if vertical {(kx+1.,(kx-27.).max(left+17.))}else if full {(kx+10.,left+15.)}else{(kx+10.,kx-10.)};
    let _ = f;

    // Body and the strip divider.
    // The current strip's body is a shade lighter (measured: 0x26292b vs 0x232527).
    let label = mx::name(s, n).to_ascii_lowercase();
    let bus = n != 0 && (label.contains("mix") || label.contains("bus"));
    let tint = track.and_then(|m| m.color).map(super::rgb);
    let base = if selected { ui.t.s(0x343c40) } else { ui.t.s(0x292f32) };
    let body = tint.map_or(base, |c| mix(base, c, 0.12));
    let header = tint.map_or(ui.t.s(0x343b40), |c| mix(ui.t.s(0x303639), c, 0.24));
    let lemon = super::lemon(ui);
    if console || t.style.vintage_2005 {
        console_skin::panel(ui,Rect::new(left,ly.y(HEAD_Y1)+1.,width,bottom-ly.y(HEAD_Y1)-1.),body,selected);
    } else if lemon {
        // Lemon (spec: ui/lemon-design.md §8): each strip is a rounded card on the dark gap colour.
        let card = Rect::new(left, ly.y(HEAD_Y1) + 1.0, width, bottom - ly.y(HEAD_Y1) - 1.0);
        ui.c.fill_rect(card, ui.t.s(0x131517));
        ui.c.fill_rrect(Rect::new(card.x + 1.5, card.y + 1.0, card.w - 3.0, card.h - 4.0), lk::r_panel(t), body);
    } else {
        ui.c.fill_rect(Rect::new(left, ly.y(HEAD_Y1) + 1.0, width, bottom - ly.y(HEAD_Y1) - 1.0), body);
        ui.c.fill_rect(Rect::new(left + width - 1.0, ly.y(HEAD_Y1) + 1.0, 1.0, bottom - ly.y(HEAD_Y1) - 1.0), ui.t.s(0x1c1f21));
    }
    // The whole strip selects (Ctrl adds, Shift renames); controls register on top of it (spec: ui/mixer.md §2).
    ui.hit(Rect::new(left, ly.y(HEAD_Y0), width, bottom - ly.y(HEAD_Y0)), Action::Mix(MHit::Strip(n)), mx::name(s, n));
    let hov = ui.hovering(Rect::new(left, ly.y(HEAD_Y0), width, ly.y(PANEL_Y1) - ly.y(HEAD_Y0)));

    // Header number (insert strips only; the master's "M" sits in the fixed header band).
    if n != 0 {
        let hr = Rect::new(left, ly.y(HEAD_Y0), width, ly.y(HEAD_Y1) - ly.y(HEAD_Y0) + 1.0);
        if full && t.style.vintage_2005 {
            vintage_number(ui, hr, n, selected || also);
        } else {
            if console {
                number_plate(ui, hr, tint);
                if ui.mixer_finish==Finish::Original {
                    original::screw(ui,hr.x+7.,hr.cy());
                    original::screw(ui,hr.right()-7.,hr.cy());
                }
            } else {ui.c.fill_rect(hr,header);}
            if console && selected {
                if ui.mixer_finish==Finish::Original {
                    original::button(ui,Rect::new(hr.cx()-14.,hr.y+2.,28.,hr.h-4.),true,false,Color::rgb(0x999c76));
                } else {ui.c.stroke_rrect(hr.inset(1.),3.,1.,Color::rgb(0xe4c74f));}
            } else if selected && lemon {
                lk::lit_in(ui.c, ui.t, Rect::new(hr.x + 3.0, hr.y + 2.0, hr.w - 6.0, hr.h - 4.0), lk::r_btn(t), false);
            } else if selected {
                // A quiet solid selection band replaces fourteen decorative hatch strokes.
                ui.c.fill_rect(hr.inset(2.0), ui.t.s(0x414b50));
            }
            let c = if selected && lemon && !console { lk::ink_on_lit(ui.t) } else if selected || hov { ui.t.tx(0xe1e8eb) } else { ui.t.tx(0x939fa5) };
            ui.c.text_center(console_font(ui,s,14), Rect::new(left, ly.y(HEAD_Y0), width, 21.0), c, &n.to_string());
        }
    }

    if vertical {
        let pane=vertical_rect(left,width,ly);
        console_skin::panel(ui,Rect::new(left,pane.bottom()+1.,width,bottom-pane.bottom()-17.),body,selected);
        let deck=Rect::new(kx-(width-8.).min(112.)/2.,ly.y(LED_Y)-10.,(width-8.).min(112.),bottom-ly.y(LED_Y)-8.);
        console_skin::well(ui,deck);
        crate::rack_mixer::paint_slot(ui,s,n,pane);
        crate::rack_mixer::paint_pager(ui,s,n,pane);
        let name=Rect::new(left+3.,pane.bottom()+18.,width-6.,18.);
        console_skin::plate(ui,name);
        let padding = if width > 110. && ui.mixer_finish == Finish::Original { 11. } else { 3. };
        let font = console_font(ui,s,11);
        fitted_center(ui.c, font, Rect::new(name.x + padding, name.y, name.w - 2. * padding, name.h),
            ui.t.text, &mx::name(s,n));
        if width>110. && ui.mixer_finish==Finish::Original {original::screw(ui,name.x+5.,name.cy());original::screw(ui,name.right()-5.,name.cy());}
    } else {
    // Dark head panel (name, LED, pan) with a rounded bottom.
    ui.c.fill_rrect(Rect::new(left + 1.0, ly.y(PANEL_Y0), width - 2.0, ly.y(PANEL_Y1) - ly.y(PANEL_Y0)+if full {5.}else{0.}), 9.0, ui.t.s(0x1d1f21));
    ui.c.fill_rect(Rect::new(left + 1.0, ly.y(PANEL_Y0), width - 2.0, 12.0), ui.t.s(0x1d1f21));

    if console {console_skin::well(ui,Rect::new(left+3.,ly.y(PANEL_Y0)+2.,width-6.,ly.y(PANEL_Y1)-ly.y(PANEL_Y0)+1.));}
    let standard=n!=0 && track.is_some_and(|m|m.channel_layout==0);
    let meter_top = ly.y(METER_Y0) + 4.0;
    let meter_max = if vintage_full {
        ((ly.y(LED_Y) - meter_top) * 0.42).clamp(64.0, 210.0)
    } else if full { 64.0 } else { 61.6 };
    let meter = Rect::new(left + 6.0, meter_top, width - 12.0,
        (ly.y(METER_Y1) - ly.y(METER_Y0) - 26.0).clamp(38.0, meter_max));
    let meter=if standard {Rect::new(left+14.,ly.y(PANEL_Y0)+29.,width-28.,(ly.y(LED_Y)-ly.y(PANEL_Y0)-45.).max(40.))}else{meter};
    if show_meters {
        ui.live(Live::Meter(n), meter.outset(if full {4.0}else{2.0}));
        let levels = s.mixer.meters.get(usize::from(n)).map_or([0.0; 2], |m| m.vu);
        let clipped = s.mixer.meters.get(usize::from(n)).is_some_and(|m| m.hold.iter().any(|&db| db >= 0.0));
        let custom_meter = ui.mixer_skin.uses(crate::mixer_skin::Part::Meter);
        if (standard && !custom_meter) || s.mixer.peak_view.contains(&n) {
            peak_window(ui, meter, s.mixer.meters.get(usize::from(n)), clipped);
        } else if console {console_skin::vu(ui,meter,levels,clipped,selected);} else { analog_vu(ui, meter, levels, clipped, selected); }
        let db = levels.map(meters::vu_db);
        ui.hit(meter, Action::Mix(MHit::Meter(n)), format!("Click to switch VU / peak meter | VU L {:+.1} / R {:+.1}", db[0], db[1]));
    }
    let name = track.and_then(|m| m.name.clone()).unwrap_or_else(|| if n == 0 { "Master".into() } else { format!("Insert {n}") });
    let name_c = track.and_then(|m| m.color).map(|c| mix(super::rgb(c), ui.t.tx(0xe1e7eb), 0.22)).unwrap_or(if selected || also { ui.t.text } else { ui.t.tx(0xaab2b7) });
    if bus && !standard {
        let badge = Rect::new(left + 23.0, meter.bottom() + 23.0, width - 46.0, 9.0);
        let accent = tint.unwrap_or(ui.t.tx(0xa6bbc5));
        if vintage_full {
            crate::vintage::recess(ui.c, badge, Color::rgb(0x415055));
        } else {
            ui.c.fill_rrect(badge, 3.0, mix(ui.t.s(0x252b2f), accent, 0.25));
            ui.c.stroke_rrect(badge, 3.0, 1.0, mix(ui.t.s(0x62676b), accent, 0.45));
        }
        ui.c.text_center(console_font(ui,s,7), badge, ui.t.tx(0xe1e7eb), "MIX BUS");
        ui.c.fill_rect(Rect::new(left + 4.0, ly.y(PANEL_Y1) + 2.0, width - 8.0, 3.0), accent);
    }
    let name_rect = Rect::new(left + 5.0, if standard {ly.y(PANEL_Y0)+5.}else{meter.bottom()+5.}, width - 10.0, 18.0);
    if vintage_full {
        crate::vintage::recess(ui.c, name_rect, Color::rgb(0x435055));
        let width = name_rect.w - 10.0;
        let font = label_font(ui.c, Font::vintage(11).bold(), &name, width);
        let ink = Color::rgb(0xc0ccd0).mix(name_c, 0.36);
        let y = name_rect.cy() - ui.c.line_height(font) / 2.0;
        ui.c.text_clip(font, name_rect.x + 5.0, y, width, ink, &name);
    } else {
        if console {console_skin::plate(ui,name_rect);}
        fitted_center(ui.c, console_font(ui,s,12), name_rect.inset(2.), name_c, &name);
    }
    if n==0 {
        let y=(meter.bottom()+28.).min(ly.y(LED_Y)-95.);
        let short=ly.y(LED_Y)-y<145.;
        headphones_controls(ui,s,Rect::new(left+8.,y,width-16.,80.),short);
        let below=y+if short {46.}else{90.};
        if crate::master_assistant::AVAILABLE {master_assistant_controls(ui,s,Rect::new(left+7.,below,width-14.,42.),false);}
    }
    channel_controls(ui, s, n, f.active_knob, Rect::new(left + 4.0, meter.bottom() + 36.0, width - 8.0, (ly.y(LED_Y) - 16.0 - meter.bottom() - 36.0).max(0.0)));
    if n!=0 && !standard {knobs_switch(ui,s,n,Rect::new(left+7.,ly.y(LED_Y)-14.,18.,44.));}
    }
    let standard=n!=0 && track.is_some_and(|m|m.channel_layout==0);
    let control_x=if vertical {sx}else if n==0 {kx}else{kx+6.};
    let enabled = track.is_none_or(|m| m.enabled);
    let led_x=control_x+if full {23.}else{0.};
    let hit_radius=if full {8.5} else {7.};
    let led_y=ly.y(if full {PAN_Y}else{LED_Y});
    ui.hit(Rect::new(led_x-hit_radius,led_y-hit_radius,2.*hit_radius,2.*hit_radius), Action::Mix(MHit::Led(n)), "Mute / solo");
    if console {if full {console_skin::lamp(ui,led_x,led_y,enabled,Color::rgb(0xb7d98b),5.);} else {console_skin::lamp(ui,led_x,led_y,enabled,Color::rgb(0xb7d98b),4.);}}else{led(ui, control_x, ly.y(LED_Y), enabled);}
    let lock = if led_x + 23. <= left + width {
        Rect::new(led_x + 9., led_y - 7., 14., 14.)
    } else {
        Rect::new(led_x - 7., led_y - 23., 14., 14.)
    };
    channel_lock(ui, s, n, lock);
    if full {ui.c.text_center(console_font(ui,s,9),Rect::new(control_x-17.,ly.y(PAN_Y)-25.,34.,12.),Color::rgb(0xc5cccf),"PAN");}
    let pan = KnobId::MixPan(n);
    ui.hit(Rect::new(control_x - 14.0, ly.y(PAN_Y) - 14.0, 28.0, 28.0), Action::Knob(pan), s.knob_text(pan));
    if console {console_skin::knob(ui,pan,control_x,ly.y(PAN_Y),10.5,s.knob(pan),Color::rgb(0xb5b7a7),ui.hovering(Rect::new(control_x-14.,ly.y(PAN_Y)-14.,28.,28.)));}else{mixer_knob(ui, control_x, ly.y(PAN_Y), 10.5, 15.0, s.knob(pan), Some(true));}

    // Reverse polarity (⇕) and swap left / right (⇔) buttons, lit when on, then the separation knob (§4).
    // Give the Full Mixer's stereo controls clearance below the panel edge.
    let (pol, swp) = (track.is_some_and(|m| m.polarity), track.is_some_and(|m| m.swap));
    let top = if vertical {ly.y(LED_Y)+12.}else{ly.y(TRACK_Y0)};
    let (sep_y, swap_y, ring_y) = if vertical {
        let polarity_y=ly.y(PAN_Y)+30.;
        (polarity_y,polarity_y+20.,polarity_y+44.)
    } else {
        let control_top = top + if full { 5.0 } else { 0.0 };
        (control_top + SEP_Y - TRACK_Y0, control_top + SWAP_Y - TRACK_Y0,
            control_top + RING_Y - TRACK_Y0 + if console { 4.0 } else { 0.0 })
    };
    let pr = Rect::new(sx - 8.0, sep_y - 8.0, 16.0, 16.0);
    let hp = ui.hit(pr, Action::Mix(MHit::Polarity(n)), "Reverse polarity");
    if console {console_skin::signal_switch(ui,pr,pol,hp,false);} else {
    let mut g = Glyph::new();
    g.polygon(&[(sx - 5.0, sep_y - 1.5), (sx, sep_y - 6.0), (sx + 5.0, sep_y - 1.5)]);
    g.polygon(&[(sx - 5.0, sep_y + 1.5), (sx, sep_y + 6.0), (sx + 5.0, sep_y + 1.5)]);
    ui.c.glyph(if pol { ui.t.h(0xf3b234) } else if hp { ui.t.tx(0xffffff) } else { ui.t.tx(0xb7c3ca) }, &g);
    }
    let wr = Rect::new(sx - 8.0, swap_y - 8.0, 16.0, 16.0);
    let hw = ui.hit(wr, Action::Mix(MHit::Swap(n)), "Swap left & right channels");
    if console {console_skin::signal_switch(ui,wr,swp,hw,true);} else {
    let mut g = Glyph::new();
    g.polygon(&[(sx - 1.0, swap_y - 6.0), (sx - 7.0, swap_y), (sx - 1.0, swap_y + 6.0)]);
    g.polygon(&[(sx + 1.5, swap_y - 6.0), (sx + 7.5, swap_y), (sx + 1.5, swap_y + 6.0)]);
    ui.c.glyph(if swp { Color::rgb(0x5fb6f0) } else if hw { ui.t.tx(0xffffff) } else { ui.t.tx(0xb7c3ca) }, &g);
    ui.c.fill_rect(Rect::new(sx - 0.5, swap_y - 6.0, 1.0, 12.0), ui.t.s(0x5a6266));
    }
    if !full || crate::workspace_visibility::shown(STEREO_SEPARATION) {
        let sep = KnobId::MixSep(n);
        let sep_hit = Rect::new(sx - 11.0, ring_y - 11.0, 22.0, 22.0);
        ui.hit(if console { sep_hit } else { sep_hit.inset(2.0) }, Action::Knob(sep), s.knob_text(sep));
        if console {
            console_knob(ui, sep, sx, ring_y, 8.0, s.knob(sep), Color::rgb(0xa7c4bd), ui.hovering(sep_hit));
            for x in [sx - 2.0, sx + 2.0] {
                ui.c.stroke_circle(x, ring_y + 14.0, 2.6, 0.8, Color::rgb(0xa7c4bd));
            }
        } else {
            mixer_knob(ui, sx, ring_y, 6.0, 9.5, s.knob(sep), Some(true));
        }
    }

    // Fader: track, gain marks, handle (green on the selected strip, with the track lit below it).
    let track_y1 = bottom - if vertical {78.}else{105.};
    if vertical && show_meters {
        // A compact, visibly segmented stereo meter beside the fader, including
        // dim unlit LEDs so silence never looks like an unexplained black box.
        let r=Rect::new(mx+14.,top,8.,(bottom-100.-top).max(1.));
        ui.live(Live::Meter(n),r.outset(2.));
        ui.c.stroke_rrect(r.outset(1.),1.,1.,Color::rgb(0x687166));
        let db=s.mixer.meters.get(usize::from(n)).map_or([meters::SILENT;2],|m|m.db);
        let steps=(r.h/5.).floor().max(1.) as usize;
        for side in 0..2 {for step in 0..steps {
            let threshold=-60.+60.*(step+1) as f32/steps as f32;
            let lit=db[side]>=threshold;
            let color=if threshold>=-3. {0xda715c}else if threshold>=-12. {0xc7b46e}else{0x9ab77c};
            let color=if lit{Color::rgb(color)}else{Color::rgb(color).mix(Color::rgb(0x252b25),0.87)};
            let h=r.h/steps as f32;
            ui.c.fill_rect(Rect::new(r.x+side as f32*4.,r.bottom()-(step+1) as f32*h,3.,(h-1.).max(1.)),color);
        }}
    }

    if vintage_full {
        let plate = Rect::new(mx - 14.0, top - 3.0, 28.0, (track_y1 - top + 7.0).max(4.0));
        crate::vintage::recess(ui.c, plate, Color::rgb(0x546167));
        crate::vintage::fader_guide(ui.c, mx, top, track_y1);
    } else if lemon {
        ui.c.fill_rrect(Rect::new(mx - 2.0, top, 4.0, track_y1 - top), 2.0, ui.t.s(0x141618));
    } else {
        ui.c.fill_rect(Rect::new(mx - 1.5, top, 3.0, track_y1 - top), ui.t.s(0x141618));
    }
    if console && !vintage_full {
        for db in [6.,0.,-6.,-12.,-18.,-30.] {
            let y=if vertical {top+(1.-fader_value(db))*(track_y1-top)}else{ly.y(HANDLE_0DB+(0.8-fader_value(db))*274.)}.floor();
            if y>=top && y<track_y1-2. {
                let ink=Color::rgb(if db==0. {0xe8dbb6}else{0x9a9f93});
                ui.c.line(mx-8.,y,mx-3.,y,1.,ink);ui.c.line(mx+3.,y,mx+8.,y,1.,ink);
                if !vertical {ui.c.text_center(console_font(ui,s,7),Rect::new(mx+9.,y-4.,16.,8.),ink,&format!("{db:.0}"));}
            }
        }
    } else if !console { fader_marks(ui, mx, track_y1, ly); }
    let v = track.and_then(|m| m.volume).unwrap_or(0.8) as f32;
    // max/min, not clamp: a very short window gives track_y1 < top.
    let position = |value: f32| if vertical || vintage_full {
        top + (1.0 - value.clamp(0.0, 1.0)) * (track_y1 - top).max(1.0)
    } else {
        ly.y(HANDLE_0DB + (0.8 - value) * 274.0).min(track_y1 - 5.0).max(top + 5.0)
    };
    let hy = position(v);
    if vintage_full {
        crate::vintage::fader_unity(ui.c, mx, position(0.8), 10.0);
    }
    if selected && console && !t.style.vintage_2005 && ui.mixer_finish != Finish::Original {
        let unity_y = position(0.8);
        ui.c.fill_rect(Rect::new(mx-1.,hy.min(unity_y),2.,(hy-unity_y).abs()), Color::rgb(0xe8c74f));
    } else if selected && !t.style.vintage_2005 {
        if lemon {
            ui.c.fill_rrect(Rect::new(mx - 1.5, hy, 3.0, track_y1 - hy), 1.5, t.accent);
        } else {
            ui.c.fill_rect(Rect::new(mx - 1.0, hy, 2.0, track_y1 - hy), ui.t.a(0x8ec63a));
        }
    }
    let hov = ui.hit(Rect::new(mx - if console {11.}else{9.}, top - if vertical {13.}else{5.}, if console {22.}else{18.}, track_y1 - top + if vertical {26.}else{10.}), Action::Fader(n), mx::level_text("Volume", f64::from(v)));
    if console {console_skin::fader(ui,mx,hy,selected,hov);}else{fader_handle(ui, mx, hy, selected, hov);}

    // The strip deck keeps recording/FX on the right, clear of the long fader and routing controls.
    let master = n == 0;
    let by = bottom;
    let fx_on = !s.mixer.all_fx_bypassed && track.is_none_or(|m| m.slots_enabled);
    let button_x=if vertical {(kx+28.).min(left+width-15.)}else{mx};
    let button_w=if vertical {24.}else{30.};
    let fx_button = Rect::new(button_x-button_w/2., by - if vertical {67.}else{79.}, button_w, 17.0);
    let fx_hover = ui.hit(fx_button, Action::Mix(MHit::Effects(n)), if fx_on { "Effects enabled — click to bypass this channel's plugins" } else { "Effects bypassed — click to enable this channel's plugins" });
    if vintage_full {
        crate::vintage::mixer_switch(ui.c, fx_button, false, fx_on, fx_hover);
    } else {
        ui.c.fill_rrect(fx_button, 3.0, if fx_on { ui.t.a(0x465d49) } else { ui.t.s(0x272d30) });
        ui.c.stroke_rrect(fx_button, 3.0, 1.0, if fx_hover { ui.t.accent } else { ui.t.s(0x64776a) });
        if console {console_skin::button(ui,fx_button,fx_on,fx_hover,Color::rgb(if ui.mixer_finish==Finish::Original {0x90ae75} else {0xe8c74f}));}
        ui.c.text_center(console_font(ui,s,11), fx_button, if console && fx_on {Color::rgb(0x161a1d)}else if fx_on { ui.t.tx(0xe4efd8) } else { ui.t.text_dim }, "FX");
    }
    // One aligned navigation column, visually separate from record arm and FX.
    let navigation_top=ring_y+22.;
    let navigation_step=((by-40.-navigation_top)/4.).clamp(13.,16.);
    let full_top = (by - 132.0).max(navigation_top);
    let full_step = ((by - 13.0 - full_top) / mx::Section::ALL.len() as f32).min(16.0);
    for (i, section) in mx::Section::ALL.into_iter().enumerate() {
        if !crate::workspace_visibility::shown(SECTION_SHORTCUTS) { break; }
        if !crate::workspace_visibility::shown(section.visibility()) { continue; }
        let b = if full {
            // Keep the left column clear of the knob and beside the footer controls.
            if full_step < 10.0 { break; }
            Rect::new(left + 8.0, full_top + i as f32 * full_step, 18.0, full_step - 2.0)
        } else if vertical {
            Rect::new(sx - 10.0, navigation_top + i as f32 * navigation_step, 20.0, 12.0)
        } else {
            Rect::new(left + 8.0, by - 150.0 - if master { 30.0 } else { 0.0 } + i as f32 * 16.0, 22.0, 12.0)
        };
        let target = section.target(s);
        let hover = if let Some(dst) = target {
            ui.hit(b, Action::Mix(MHit::Jump(n, section)), format!("Scroll to {}: {} (keeps current channel selected)", section.label(), mx::name(s,dst)))
        } else {
            ui.hit(b, Action::Swallow, format!("No {} section found in this project", section.label()));
            false
        };
        ui.c.fill_rrect(b, 2.0, if hover { ui.t.button_hi } else { ui.t.s(0x252d32) });
        ui.c.stroke_rrect(b, 2.0, 1.0, if hover { ui.t.accent } else { ui.t.s(0x3b474d) });
    if console {console_skin::button(ui,b,false,hover,Color::rgb(0x91a290));}
        let font = if full && !vintage_full {Font::light(8,0).natural()}else{console_font(ui,s,8)};
        fitted_center(ui.c, font,
            b.inset(1.), if target.is_some() {ui.t.tx(0xb9c8d0)} else {ui.t.text_dim}, section.label());
    }
    if master {
        let mut all = Rect::new(if full {left + 28.0} else {mx - 25.0}, by - 98.0, 50.0, 17.0);
        // In a short Channel Strip deck the wider master button must clear the width knob.
        if console && all.y < ring_y+18. && all.bottom()>ring_y-12. {all.x=(sx+13.).min(left+width-52.);}
        let on = !s.mixer.all_fx_bypassed && s.project.mixer.iter().any(|m|m.slots_enabled);
        let hover = ui.hit(all, Action::Mix(MHit::AllEffects), if on { "Bypass ALL mixer effects (keeps your individual choices)" } else { "Restore ALL mixer effects to your previous choices" });
        ui.c.fill_rrect(all, 3.0, if on { ui.t.a(0x50694a) } else { ui.t.s(0x665037) });
        ui.c.stroke_rrect(all, 3.0, 1.0, if hover { ui.t.accent } else { ui.t.s(0x899182) });
    if console {console_skin::button(ui,all,on,hover,Color::rgb(if ui.mixer_finish==Finish::Original {0x9caf75} else {0xe8c74f}));}
        ui.c.text_center(console_font(ui,s,10), all, if console && on {Color::rgb(0x161a1d)}else{ui.t.tx(0xe4efd8)}, if on { "ALL FX" } else { "FX OFF" });
    }
    if !master {
    let armed=s.mixer.recording.armed.contains(&n);
    let record = Rect::new(button_x-button_w/2., by - if vertical {88.}else{98.}, button_w, 17.0);
    let rec_hover = ui.hit(record, Action::Mix(MHit::Disk(n)), if armed { "Disarm disk recording" } else { "Arm disk recording (select an input to arm automatically)" });
    if vintage_full {
        crate::vintage::mixer_switch(ui.c, record, true, armed, rec_hover);
    } else {
        ui.c.fill_rrect(record, 3.0, if armed { ui.t.h(0x8f292e) } else { ui.t.s(0x2b3337) });
        ui.c.stroke_rrect(record, 3.0, 1.0, if rec_hover || armed { ui.t.h(0xff6969) } else { ui.t.s(0x667278) });
        if console {console_skin::button(ui,record,armed,rec_hover,Color::rgb(0xbd6053));}
        ui.c.fill_circle(record.cx(), record.cy(), 4.5, if armed { ui.t.h(0xff6868) } else { ui.t.h(0xb85858) });
    }

    }

    // Routing (spec: ui/mixer.md §5): the current track's green ⬇ (its cables are drawn over all strips: `cables`),
    // a send knob where the current track sends, else the ▲ "route to this track" button; an orange "^" marks tracks that receive a route.
    let ay = by - 38.0;
    let cur = s.selected_mixer;
    let cur_name = mx::name(s, cur);
    if selected {
        down_arrow(ui, kx, ay);
    } else if let Some(level) = mx::send(s, cur, n) {
        // Send knob (fader law) with its arc, the green "^" that removes the route; its cable is drawn with the others.
        let ky = by - 40.0;
        let id = KnobId::Send(cur, n);
        ui.hit(Rect::new(kx - 13.0, ky - 13.0, 26.0, 26.0), Action::Knob(id), s.knob_text(id));
        send_knob(ui, s, n, kx, ky, level);
        let ur = Rect::new(kx - 6.0, by - 61.0, 12.0, 8.0);
        let hov = ui.hit(ur, Action::Mix(MHit::Unroute(n)), format!("Remove the send from {cur_name} to {}", mx::name(s, n)));
        ui.c.fill_polygon(&[(kx - 4.0, by - 55.0), (kx, by - 59.0), (kx + 4.0, by - 55.0)], if hov { ui.t.tx(0xffffff) } else { t.accent });
    } else if !(master && cur == 0) {
        let hint = format!("Enable send from {cur_name} to {}  Ctrl/Shift", mx::name(s, n));
        let hov = ui.hit(route_hit_rect(kx, ay, width), Action::Mix(MHit::Route(n)), hint);
        let mut g = Glyph::new();
        g.polygon(&[(kx - 6.5, ay + 3.0), (kx, ay - 3.5), (kx + 6.5, ay + 3.0)]);
        ui.c.glyph(if hov { t.accent } else { ui.t.tx(0xd8e0e6) }, &g);
        if !master && mx::receives(s, n) {
            ui.c.fill_polygon(&[(kx - 3.5, ay - 9.0), (kx, ay - 12.5), (kx + 3.5, ay - 9.0)], ui.t.h(0xf09a28));
        }
    }
    // Quiet dividers; selection and explicit group boundaries carry the emphasis.
    let top = ly.y(HEAD_Y0);
    let height = bottom - top - 16.0;
    let border = if selected || also {
        if full && t.style.vintage_2005 { Color::rgb(0x92a2a7) } else { ui.t.accent }
    } else { ui.t.s(0x464f54) };
    ui.c.stroke_rrect(Rect::new(left + 0.75, top + 0.75, width - 1.5, height - 1.5), 0.0, 1.0, border);
    if mx::has_separator(s, n) {
        if vintage_full {crate::vintage::mixer_divider(ui.c,Rect::new(left,top,6.,height));}
        else if console {console_skin::separator(ui,Rect::new(left,top,6.,height));}else{ui.c.fill_rect(Rect::new(left, top, 6.0, height), ui.t.s(0x919da3));}
    }

}


fn route_hit_rect(x: f32, y: f32, strip_width: f32) -> Rect {
    let width = 28.0_f32.min(strip_width);
    Rect::new(x - width / 2.0, y - 10.0, width, 22.0)
}

fn down_arrow(ui: &mut Ui, kx: f32, ay: f32) {
    let a = [(kx - 3.0, ay - 6.0), (kx + 3.0, ay - 6.0), (kx + 3.0, ay - 1.0), (kx + 7.0, ay - 1.0), (kx, ay + 6.0), (kx - 7.0, ay - 1.0), (kx - 3.0, ay - 1.0)];
    ui.c.fill_polygon(&a, ui.t.accent);
}

/// The send knob from the current track into track `n`, with its level arc (none at zero).
fn send_knob(ui: &mut Ui, s: &State, n: u16, kx: f32, ky: f32, level: f64) {
    let id = KnobId::Send(s.selected_mixer, n);
    if mx::view(s)==mx::WIDE||mx::view(s)==crate::rack_mixer::VIEW {console_knob(ui,id,kx,ky,10.,s.knob(id),Color::rgb(0x9cae84),ui.hovering(Rect::new(kx-13.,ky-13.,26.,26.)));}else{mixer_knob(ui, kx, ky, 10.0, 13.0, s.knob(id), None);}
    if level > 0.0 && (!matches!(mx::view(s),mx::WIDE|crate::rack_mixer::VIEW) || ui.mixer_finish == Finish::Original) {
        // From the knob's zero to its pointer (the arc takes a sweep, not an end angle).
        let sweep = (level as f32).min(1.0) * 1.5 * PI;
        ui.c.arc(kx, ky, 12.5, 0.75 * PI, sweep, 2.0, ui.t.accent);
    }
}

/// The dark round mixer knob: a shaded ball inside a darker halo.
/// `arc`: Some(true) draws the red value arc from the top (pan style), Some(false) from the start.
fn mixer_knob(ui: &mut Ui, cx: f32, cy: f32, r: f32, halo: f32, value: f32, arc: Option<bool>) {
    let hov = ui.hovering(Rect::new(cx - halo, cy - halo, 2.0 * halo, 2.0 * halo));
    let look = os_ui::kit::KnobLook::Mixer { halo, arc: arc.is_some() };
    os_ui::kit::knob(ui.c, ui.t, cx, cy, r, value, arc.unwrap_or(false), look, os_ui::kit::State::hover(hov));
}

/// Gain marks beside a fader track (spec: ui/mixer.md §5, measured from FL): 6 px ticks on both sides at 0, −6, −12 and
/// −18 dB (2 px clear of the track), and faint 4 px rails between consecutive ticks (1 px clear of each tick). They
/// belong to the track: the handle covers them as it moves.
fn fader_marks(ui: &mut Ui, mx: f32, track_y1: f32, ly: &Ly) {
    let ys: Vec<f32> = [0.0, -6.0, -12.0, -18.0]
        .iter()
        .map(|&db| ly.y(HANDLE_0DB + (0.8 - fader_value(db)) * 274.0).floor())
        .filter(|&y| y < track_y1 - 1.0)
        .collect();
    let (tick, rail) = (ui.t.s(0x181a1c), ui.t.s(0x202324));
    for w in ys.windows(2) {
        let (a, b) = (w[0] + 2.0, w[1] - 1.0);
        if b > a {
            ui.c.fill_rect(Rect::new(mx - 7.5, a, 4.0, b - a), rail);
            ui.c.fill_rect(Rect::new(mx + 3.5, a, 4.0, b - a), rail);
        }
    }
    for y in ys {
        ui.c.fill_rect(Rect::new(mx - 9.5, y, 6.0, 1.0), tick);
        ui.c.fill_rect(Rect::new(mx + 3.5, y, 6.0, 1.0), tick);
    }
}

/// Fader value (0..1, 0.8 = 0 dB) of a gain in dB: the inverse of `laws::fader_gain`.
fn fader_value(db: f32) -> f32 {
    0.8 * (10.0 * 10f32.powf(db / 20.0) + 1.0).ln() / 11f32.ln()
}

/// Fader handle centred on (cx, cy): 14 × 23.
fn fader_handle(ui: &mut Ui, cx: f32, cy: f32, selected: bool, hov: bool) {
    os_ui::kit::fader_handle(ui.c, ui.t, cx, cy, selected, os_ui::kit::State::hover(hov));
}

fn slot_panel(ui: &mut Ui, s: &State, r: Rect) {
    let t = ui.t;
    if s.vintage_2005 {
        let title = format!("Mixer - {}", mx::name(s, s.selected_mixer));
        let caption = Rect::new(r.x, r.y, r.w, 23.0);
        crate::vintage::window_caption(ui.c, caption);
        ui.c.text_clip(Font::vintage(12), r.x + 4.0, r.y + 4.0, r.w - 8.0, t.text, &title);
        ui.hit(caption, Action::WinDrag(Win::Mixer), "Drag to move");
        if r.h >= 2.0 * TITLE_H {
            vintage_compact::inspector(ui, s, r);
        }
        return;
    }
    let themed = ui.mixer_finish == Finish::Theme;
    if themed {
        if !os_ui::kit::panel_finish(ui.c, t, r, t.mixer_panel) {
            ui.c.fill_rect(r, t.mixer_panel);
        }
    } else if mx::view(s) == mx::WIDE {
        console_skin::panel(ui, r, Color::rgb(0x25282b), false);
    } else {
        ui.c.fill_rect(r, t.s(0x2e3133));
    }
    ui.c.fill_rect(Rect::new(r.x - 1.0, r.y, 1.0, r.h), ui.t.s(0x1b1e20));
    ui.hit(r, Action::Swallow, "");
    let n = s.selected_mixer;
    let track = s.project.mixer.iter().find(|m| m.index == n);
    let title = match track.and_then(|m| m.name.clone()) {
        Some(nm) => format!("Mixer - {nm}"),
        None if n == 0 => "Mixer - Master".into(),
        None => format!("Mixer - Insert {n}"),
    };
    ui.c.fill_rect(Rect::new(r.x, r.y, r.w, 23.0), t.window_title);
    if !mx::traditional(s) || mx::inspector_left() {
    let font = os_ui::kit::title_font(t, console_font(ui,s,17));
    let width = r.w - if mx::inspector_left() { 4.0 } else { 70.0 };
    let font = label_font(ui.c, font, &title, width);
    ui.c.text_clip(font, r.x + 1.0, r.y + 2.0, width, ui.t.tx(0xd4dade), &title);
    }
    ui.hit(Rect::new(r.x, r.y, r.w, 23.0), Action::WinDrag(Win::Mixer), "Drag to move");

    if r.h < 2.0 * TITLE_H {
        return;
    }
    // Squish: the slot rows and the equalizer graph shrink with the height (the knob rows under
    // the graph are dropped when they no longer fit); the top and bottom rows keep their size.
    let room = r.h - 125.0;
    let kp = ((room - 15.0) / 503.0).clamp(0.3, 1.0);
    let q = kp.max(0.55);
    // Input row: EQ glyph, "(none)" combo, latency clock. Audio inputs are not available yet: greyed out.
    let input_bg = ui.t.s(0x2b2e30);
    ui.c.fill_rect(Rect::new(r.x, r.y + 23.0, r.w, 36.0), input_bg);
    let mut g = Glyph::new();
    g.polygon(&[(r.x + 7.0, r.y + 32.0), (r.x + 13.0, r.y + 32.0), (r.x + 18.0, r.y + 37.5), (r.x + 13.0, r.y + 43.0), (r.x + 7.0, r.y + 43.0)]);
    ui.c.glyph(ui.t.s(0x40464a), &g);
    ui.c.text(console_font(ui,s,9).bold(), r.x + 6.0, r.y + 43.0, ui.t.s(0x40464a), "EQ");
    let combo=Rect::new(r.x+27.,r.y+28.,159.,23.);
    let label=s.mixer.recording.inputs.get(&n).map(|i|format!("{} - {}",i.label(),i.device)).unwrap_or_else(||"(none)".into());
    let hov=ui.hit(combo,Action::Mix(MHit::Input(n)),format!("Audio input: {label}. Selecting an input arms audio recording"));
    ui.c.fill_rect(combo,if hov{t.button}else{input_bg});
    let font = console_font(ui,s,13);
    let font = label_font(ui.c, font, &label, combo.w - 20.);
    ui.c.text_clip(font,combo.x+4.,combo.y+3.,combo.w-20.,t.text,&label);
    chevron(ui,combo.right()-8.,combo.cy(),true,t.text);
    let count=mx::slot_count(s,n);
    let first=s.mixer.slot_scroll.get(&n).copied().unwrap_or(0).min(count.saturating_sub(10));
    ui.hit(Rect::new(r.x,r.y+61.,r.w,330.*kp),Action::Mix(MHit::SlotScroll),"Scroll effect slots");
    for visible in 0..10 {
        let i=first+visible;
        let row = Rect::new(r.x + 1.0, r.y + 61.0 + 30.0 * kp * visible as f32, r.w - 2.0, 28.0 * kp);
        let k = mx::visible_user_slot(s,n,i);
        let plugin = mx::slot_name(s, n, k);
        let fs = ((if mx::view(s)==mx::WIDE {12.0}else{18.0}) * kp.max(0.62)).round() as u16;
        let hint = format!("FX slot {} options", i + 1);
        let ha = ui.hit(Rect::new(row.x, row.y, 19.0, row.h), Action::Mix(MHit::SlotArrow(k)), hint.clone());
        let hn = ui.hit(Rect::new(row.x + 19.0, row.y, row.w - 64.0, row.h), Action::Mix(MHit::SlotName(k)), hint);
        let hov = ha || hn;
        if themed {
            let fill = if hov { t.slot.shade(0.10) } else { t.slot };
            os_ui::kit::bordered(ui.c, row, t.style.radius_small.min(row.h * 0.5), t.window_border, fill);
        } else if ui.mixer_finish == Finish::CleanModern && mx::view(s)==mx::WIDE {
            ui.c.fill_rect(row.inset(1.), Color::rgb(if hov {0x383838} else {0x2c2c2c}));
        } else if super::lemon(ui) {
            let face = if hov { ui.t.s(0x2f3335) } else { ui.t.s(0x2a2d2f) };
            lk::outlined(ui.c, Rect::new(row.x + 2.0, row.y, row.w - 4.0, row.h), lk::r_btn(t).min(row.h / 2.0), ui.t.s(0x1f2224), face);
        } else {
            ui.c.fill_rect(row, ui.t.s(0x1f2224));
            ui.c.fill_rect(row.inset(1.0), if hov { ui.t.s(0x2f3335) } else { ui.t.s(0x2a2d2f) });
        }
        chevron(ui, row.x + 10.0, row.cy(), true, if ha { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
        let (f, name, ink) = match &plugin {
            Some(p) => (console_font(ui,s,fs), p.to_string(), ui.t.tx(0xc9d0d4)),
            None => (console_font(ui,s,fs).italic(), format!("Slot {}", i + 1), if mx::view(s)==mx::WIDE {Color::rgb(0x848c91)}else{ui.t.s(0x50575b)}),
        };
        let ink = if themed { if plugin.is_some() { t.slot_text_used } else { t.slot_text } } else { ink };
        // Lemon centres the name's capitals on the row, level with the chevron, knob and LED: the baseline sits half a
        // cap height (the face's is about 0.72 of its size) under the middle, measured on the face drawn (the italic
        // one has its own ascent). Classic keeps FL's place.
        let f = label_font(ui.c, f, &name, row.w - 66.);
        let ty = if super::lemon(ui) { (row.cy() + 0.36 * f32::from(f.size) - ui.c.ascent(f)).round() } else { row.y + 4.0 * kp };
        ui.c.text_clip(f, row.x + 19.0, ty, row.w-66., ink, &name);
        let mix = KnobId::SlotMix(n, k);
        ui.hit(Rect::new(row.right() - 46.0, row.y, 26.0, row.h), Action::Knob(mix), s.knob_text(mix));
        if mx::view(s)==mx::WIDE {console_skin::knob(ui,mix,row.right()-33.,row.cy(),8.5*q,s.knob(mix),Color::rgb(0xc2c7c9),false);}else{mixer_knob(ui, row.right() - 33.0, row.cy(), 10.5 * q, 12.0 * q, s.knob(mix), Some(false));}
        ui.c.stroke_circle(row.right() - 33.0, row.cy(), 11.8 * q, 1.5, ui.t.s(0x55595c));
        ui.hit(Rect::new(row.right() - 18.0, row.y, 18.0, row.h), Action::Mix(MHit::SlotLed(k)), "Mute / solo  (ctrl)0..9");
        led(ui, row.right() - 10.0, row.cy(), track.is_none_or(|m| m.slot_on[k as usize]));
    }
    let add=Rect::new(r.x+3.,r.y+61.+300.*kp,r.w-6.,28.*kp);
    let hover=ui.hit(add,Action::Mix(MHit::AddSlot),"Add another effect slot (up to 63)");
    ui.c.fill_rect(add,if hover {t.button}else{ui.t.s(0x24292a)});
    ui.c.text(console_font(ui,s,12),add.x+6.,add.y+2.,t.text,&format!("+  Add slot       {}–{} / {}",first+1,(first+10).min(count),count));
    let sep = r.y + 61.0 + 330.0 * kp;
    ui.c.fill_rect(Rect::new(r.x, sep, r.w, 1.0), ui.t.s(0x1f2224));

    if crate::workspace_visibility::shown("visible_mixer-eq") {
        crate::mini_eq::draw(ui, s, Rect::new(r.x + 4.0, sep + 6.0, r.w - 8.0, (r.bottom() - 72.0 - sep - 6.0).max(30.0)));
    }
    let b = r.bottom();

    // Output rows at the bottom: audio outputs are not available yet, greyed out.
    ui.c.fill_rect(Rect::new(r.x, b - 66.0, r.w, 1.0), ui.t.s(0x1f2224));
    ui.c.fill_rect(Rect::new(r.x, b - 33.0, r.w, 1.0), ui.t.s(0x1f2224));
    let mut g = Glyph::new();
    g.ring(r.x + 14.0, b - 48.5, 8.0, 1.5).rect(r.x + 13.3, b - 53.0, 1.4, 5.0).rect(r.x + 13.3, b - 49.2, 4.0, 1.4);
    ui.c.glyph(ui.t.s(0x40464a), &g);
    ui.off(Rect::new(r.x + 25.0, b - 60.0, r.w - 28.0, 22.0), "Audio output target");
    ui.c.text(console_font(ui,s,17), r.x + 30.0, b - 59.0, ui.t.tx(0xd4dade), "(none)");
    chevron(ui, r.right() - 11.0, b - 48.5, true, ui.t.tx(0x8c969c));
    ui.veil(Rect::span(r.x, r.right(), b - 65.0, b - 34.0), ui.t.s(0x2e3133));
    let mut g = Glyph::new();
    g.ring(r.x + 13.0, b - 16.0, 8.0, 1.6);
    g.erase(true).rect(r.x + 13.0, b - 26.0, 10.0, 20.0);
    g.erase(false).polygon(&[(r.x + 11.0, b - 20.5), (r.x + 16.0, b - 20.5), (r.x + 21.0, b - 16.0), (r.x + 16.0, b - 11.5), (r.x + 11.0, b - 11.5)]);
    ui.c.glyph(ui.t.tx(0xc4ccd0), &g);
    ui.veil(Rect::new(r.x + 3.0, b - 26.0, 20.0, 20.0), ui.t.s(0x2e3133));
    off_combo(ui, Rect::new(r.x + 26.0, b - 28.0, 189.0, 24.0), "Audio output target", ui.t.s(0x2e3133));
}

/// An input / output combo that is not available yet: drawn greyed out, its hit only says so.
fn off_combo(ui: &mut Ui, r: Rect, name: &'static str, bg: Color) {
    ui.off(r, name);
    let st = os_ui::kit::State::NORMAL.with_disabled(true);
    os_ui::kit::combo(ui.c, ui.t, r, "(none)", Font::new(17), st);
    ui.veil(r, bg);
}

/// Small solid chevron centred on (x, y), pointing right or left.
pub fn chevron(ui: &mut Ui, x: f32, y: f32, right: bool, c: Color) {
    let d = if right { 1.0 } else { -1.0 };
    let mut g = Glyph::new();
    g.polygon(&[(x - 2.5 * d, y - 4.5), (x + 2.5 * d, y), (x - 2.5 * d, y + 4.5)]);
    ui.c.glyph(c, &g);
}

fn peak_window(ui:&mut Ui,r:Rect,m:Option<&meters::Meter>,clipped:bool) {
    if ui.t.style.vintage_2005 {
        let levels = m.map_or([0.0; 2], |meter| {
            meter.db.map(|db| (((db + 60.0) / 4.0).floor() + 1.0).clamp(0.0, 16.0) / 16.0)
        });
        themed::vintage_meter(ui, r, levels, clipped, "PEAK");
        return;
    }
    ui.c.fill_rrect(r,4.0,Color::rgb(0x171e1a));
    ui.c.stroke_rrect(r,4.0,1.0,Color::rgb(0x8b8573));
    for (side,tag) in ["L","R"].iter().enumerate() {
        let db=m.map_or(-100.0,|m|m.db[side]);
        let x=r.x+12.0+side as f32*(r.w-25.0)*0.5;
        let w=(r.w-32.0)*0.5;let h=r.h-20.0;
        for i in 0..16 {let lit=db>=-60.0+i as f32*4.0;
            let c=if i>=15{0xe9654c}else if i>=13{0xe8c65c}else{0x8fba71};
            ui.c.fill_rect(Rect::new(x,r.bottom()-12.0-(i+1) as f32*h/16.0,w,(h/16.0-1.0).max(1.0)),if lit{Color::rgb(c)}else{Color::rgb(0x2a352c)});
        }
        ui.c.text_center(Font::new(8),Rect::new(x,r.bottom()-11.0,w,10.0),Color::rgb(0xddd9c2),tag);
    }
    ui.c.text(Font::new(8),r.x+3.0,r.y+2.0,Color::rgb(0xddd9c2),"0");
    ui.c.fill_circle(r.right()-5.0,r.y+5.0,2.0,if clipped{Color::rgb(0xf05c43)}else{Color::rgb(0x4d4936)});
    ui.c.fill_rect(Rect::new(r.x+3.0,r.y+3.0,r.w-6.0,1.0),Color::rgb(0x69736a));
}

// One continuous geometry for every EQ strip. Value visibility never changes knob size.
fn eq_knob_layout(row:f32)->(f32,f32,f32) {
    let scale=(row/50.).min(1.);
    (scale,24.*scale,11.*scale)
}

#[cfg(test)]
mod eq_label_tests {
    #[test]
    fn compact_mixer_gives_fader_space_to_knobs() {
        use super::*;
        for height in [435.,650.,800.,997.] {
            let r=Rect::new(0.,83.,900.,height);
            let ly=Ly::new(r);
            let k=scale(r);
            assert!(((ly.y(981.)-ly.y(692.))/(289.*k)-0.88).abs()<0.00001);
            assert!(ly.y(620.) > 130.+490.*k);
            assert!((ly.y(981.)-(130.+779.*k+72.)).abs()<0.001,"bottom remains fixed");
            let gap=ly.y(PAN_Y)-ly.y(LED_Y);
            assert!(gap>=21. && gap<29.5,"mute and pan targets remain separate");
            let travel=ly.y(750.8+27.4)-ly.y(750.8);
            assert!((travel-27.4*fader_scale(r)).abs()<0.001,"drag scale follows rendered fader");
        }
    }

    #[test]
    fn eq_scaling_is_continuous_and_never_hides_knobs() {
        let mut previous=0.;
        // Includes the former 22, 34 and 50 px transitions and the smallest vocal rows.
        for step in 100..=570 {
            let row=step as f32/10.;
            let (scale,cy,radius)=super::eq_knob_layout(row);
            assert!(radius>0. && radius>=previous,"knob shrank while row grew at {row}");
            if step>100 {assert!(radius-previous<0.023,"size jump at {row}");}
            assert!(8.*scale < cy-radius-0.5*scale,"label overlaps knob");
            assert!(cy+radius+2.5*scale < row,"knob leaves row");
            if row>=50. {assert!(cy+radius+2.5*scale <= row-12.,"knob overlaps value");}
            previous=radius;
        }
    }
}

fn console_font(_ui:&Ui,s:&State,size:u16)->Font {if s.vintage_2005 {return Font::vintage(size.min(12)).bold();} if mx::view(s)==mx::WIDE {Font::light(if size>=7 && size<=10 {size+1}else{size},0).natural()}else{Font::new(size)}}

pub(crate) fn label_font(canvas: &mut os_ui::Canvas, font: Font, label: &str, width: f32) -> Font {
    let mut fitted = font;
    while fitted.size > 5 && canvas.text_width(fitted, label) > width {
        fitted.size -= 1;
        if font.width != 0 {
            fitted.width = (font.width * fitted.size / font.size).max(1);
        }
    }
    canvas.fit_font(fitted, label, width)
}

fn fitted_center(canvas: &mut os_ui::Canvas, font: Font, area: Rect, ink: Color, label: &str) {
    let font = label_font(canvas, font, label, area.w);
    canvas.push_clip(area);
    if canvas.text_width(font, label) <= area.w {
        canvas.text_center(font, area, ink, label);
    } else {
        let height = canvas.line_height(font);
        canvas.text(font, area.x, area.cy() - height / 2., ink, label);
    }
    canvas.pop_clip();
}

fn channel_controls(ui:&mut Ui,s:&State,n:u16,active:Option<KnobId>,r:Rect) {
    let mut readout:Option<(Rect,String,Option<&str>)>=None;
    if n==0 || r.h<80.0 || mx::track(s,n).is_some_and(|m|m.channel_layout==0) {return;}
    let vintage = s.vintage_2005 && mx::view(s) == mx::WIDE;
    let vocal=mx::track(s,n).is_some_and(|t|t.is_vocal());
    let row=(r.h/if vocal {8.0}else{6.0}).min(if mx::view(s)==mx::WIDE {74.0}else{57.0});
    let radius=(row*0.22).min((row-13.0)*0.5).clamp(3.0,if mx::view(s)==mx::WIDE {13.0}else{11.0});
    let controls=[("LOW CUT",None,0xc8c2ad),("DRIVE",None,0xd1a77e),("HIGH",Some(2),0xc98b73),("MID",Some(1),0x89b593),("LOW",Some(0),0x84a7c1),("COMP",None,0xcbb578),("DS",None,0x91b9c9),("TUNE",None,0xb9a0d1)];
    let order:&[usize]=if vocal {&[7,0,1,2,3,4,5,6]}else{&[0,1,2,3,4,5]};
    for (position,&i) in order.iter().enumerate() {
        let (label,band,color)=&controls[i];
        let y=r.y+position as f32*row;
        if mx::view(s)==mx::WIDE {ui.c.line(r.x+3.,y+row-1.,r.right()-3.,y+row-1.,0.5,Color::rgb(0x363b3f));}else{ui.c.fill_rrect(Rect::new(r.x,y,r.w,row-2.0),3.0,ui.t.s(0x292d2d));}
        if i==0 || i==5 {
            let left=Rect::new(r.x,y,r.w*0.54,10.0);
            ui.c.text_center(console_font(ui,s,8),left,Color::rgb(*color),if i==5 {"COMP"}else{label});
            ui.c.text_center(console_font(ui,s,8),Rect::new(r.x+r.w*0.54,y,r.w*0.46,10.0),Color::rgb(*color),if i==0 {"GATE"}else{"TAME"});
            if i==5 {ui.c.fill_polygon(&[(left.right()-5.0,y+3.0),(left.right()-1.0,y+3.0),(left.right()-3.0,y+6.0)],Color::rgb(*color));}
            if i==5 {let profile=mx::track(s,n).map(|t|t.strip[5]).unwrap_or(0).min(5) as usize;ui.hit(left,Action::Mix(MHit::CompProfile(n)),format!("Compressor profile: {}",os_dsp::channel_strip::PROFILES[profile].name));}
        }else{
            let scale=if band.is_some(){eq_knob_layout(row).0}else{1.};
            ui.c.text_center(console_font(ui,s,(8.*scale).round().max(2.) as u16),Rect::new(r.x,y,if i==7 {r.w*0.35}else if i==1 {r.w*0.54}else{r.w},10.*scale),Color::rgb(*color),label);
        }
        let cy=y+10.0+(row-12.0)*0.45;
        if i==1 {
            let kind=mx::track(s,n).map(|t|t.strip[6]).unwrap_or(0).min(3) as usize;
            let bh=(row-16.0).clamp(10.0,20.0);
            let b=Rect::new(r.x+r.w*0.54,cy-bh*0.5,r.w*0.46-1.0,bh);
            let hover=ui.hit(b,Action::Mix(MHit::DriveType(n)),format!("Drive type: {}",os_dsp::channel_strip::DRIVE_TYPES[kind]));
            ui.c.fill_rrect(b,3.0,if hover{ui.t.button_hi}else{ui.t.s(0x202729)});
            ui.c.stroke_rrect(b,3.0,1.0,ui.t.s(0x59636a));
    if mx::view(s)==mx::WIDE {console_skin::button(ui,b,false,hover,Color::rgb(*color));}
            ui.c.text_center(console_font(ui,s,8),Rect::new(b.x,b.y,b.w-6.0,b.h),Color::rgb(*color),os_dsp::channel_strip::DRIVE_TYPES[kind]);
            ui.c.fill_polygon(&[(b.right()-6.0,b.cy()-1.0),(b.right()-2.0,b.cy()-1.0),(b.right()-4.0,b.cy()+2.0)],Color::rgb(*color));
        }
        if i==7 {
            let m=mx::track(s,n).unwrap();
            let power=Rect::new(r.x+r.w*0.37,y+1.,11.,7.);
            let hot = ui.hit(power.outset(1.),Action::Mix(MHit::TunePower(n)),if m.tune_enabled {"Tune ON: click to bypass without changing settings"}else{"Tune OFF: no tuning delay. Click to enable"});
            if s.vintage_2005 {
                crate::vintage::recess(ui.c, power, Color::rgb(if hot {0x53676d} else {0x303d42}));
                let lamp = Rect::new(power.cx() - 2.0, power.cy() - 1.0, 4.0, 2.0);
                ui.c.fill_rect(lamp, Color::rgb(if m.tune_enabled {0xb7d573} else {0x566448}));
            } else {
                ui.c.fill_rrect(power,3.,if m.tune_enabled {Color::rgb(0x94be79)}else{Color::rgb(0x51585c)});
                ui.c.fill_circle(if m.tune_enabled{power.right()-3.5}else{power.x+3.5},power.cy(),2.3,Color::rgb(0xe1e5dc));
            }
            let label=if m.tune_learn!=0 {"Listening".to_owned()}else if m.tune[2]==0 {"Learn".to_owned()}else{
                let scale=match m.tune[2] {1=>"maj",2=>"min",3=>"h.min",4=>"m.min",5=>"M.pent",6=>"m.pent",7=>"blues",8=>"dor",9=>"phr",10=>"lyd",11=>"mix",_=>"loc"};
                format!("{} {}",os_dsp::snaptune::KEYS[m.tune[1].min(11) as usize].split(" /").next().unwrap(),scale)
            };
            let b=Rect::new(r.x+r.w*0.49,cy-7.,r.w*0.50,14.);
            let hover=ui.hit(b,Action::Mix(MHit::TuneLearn(n)),if m.tune_learn!=0 {"Listening to this vocal: play a varied phrase (6 seconds of voiced notes). Click to cancel.".into()}else{format!("Suggested/manual key: {} {}. Click to learn; right-click to set key / scale.",os_dsp::snaptune::KEYS[m.tune[1].min(11) as usize],os_dsp::snaptune::SCALES[m.tune[2].min(12) as usize])});
            ui.c.fill_rrect(b,3.,if hover{ui.t.button_hi}else{ui.t.s(0x202729)});
            ui.c.stroke_rrect(b,3.,1.,ui.t.s(0x59636a));
    if mx::view(s)==mx::WIDE {console_skin::button(ui,b,m.tune_learn!=0,hover,Color::rgb(*color));}
            if s.vintage_2005 {
                fitted_center(ui.c, console_font(ui,s,7), b.inset(2.0), Color::rgb(0xc0ccd0), &label);
            } else {
                ui.c.text_center(console_font(ui,s,7),b,Color::rgb(*color),&label);
            }
        }
        let ids:Vec<KnobId>=if let Some(b)=band {vec![KnobId::Eq(n,*b,0),KnobId::Eq(n,*b,1)]}else{match i {7=>vec![KnobId::Strip(n,8)],6=>vec![KnobId::Strip(n,7)],1=>vec![KnobId::Strip(n,2)],0=>vec![KnobId::Strip(n,0),KnobId::Strip(n,3)],_=>vec![KnobId::Strip(n,1),KnobId::Strip(n,4)]}};
        for (k,id) in ids.iter().copied().enumerate() {
            let cx=if i==6{r.cx()}else if ids.len()==1{r.x+r.w*0.27}else{r.x+r.w*(if k==0{0.27}else{0.73})};
            let (cy,radius,scale)=if vintage {
                let label_height = if band.is_some() { 10.0 * eq_knob_layout(row).0 } else { 10.0 };
                let top = (label_height + 1.0).min(row * 0.55);
                let bottom = row - if row >= 50.0 { 12.0 } else { 2.0 };
                let radius = ((bottom - top) * 0.5 - 1.0).clamp(2.5, 10.0);
                (y + (top + bottom) * 0.5, radius, 1.0)
            } else if band.is_some() {
                let (scale,cy,radius)=if mx::view(s)==mx::WIDE {let scale=(row/50.).min(1.);(scale,row*0.47,13.*scale)}else{eq_knob_layout(row)};
                if mx::view(s)!=mx::WIDE {ui.c.text_center(console_font(ui,s,(7.*scale).round().max(2.) as u16),Rect::new(cx-10.0,y,20.0,8.*scale),ui.t.text_dim,if k==0 {"dB"}else{"Hz"});}
                (y+cy,radius,scale)
            }else{(y+10.0+(row-12.0)*0.45,radius,1.)};
            let hit = if vintage {
                Rect::new(cx - radius - 2.0, cy - radius - 2.0, 2.0 * radius + 4.0, 2.0 * radius + 4.0)
                    .intersect(&Rect::new(r.x, y, r.w, row))
            } else {
                Rect::new(cx-radius-3.*scale,y+10.*scale,radius*2.+6.*scale,(row-12.*scale).max(1.))
            };
            ui.hit(hit,Action::Knob(id),s.knob_text(id));
            let cap=Color::rgb(*color);
            if mx::view(s)==mx::WIDE {console_skin::knob(ui,id,cx,cy,radius,s.knob(id),cap,ui.hovering(Rect::new(cx-radius,cy-radius,2.*radius,2.*radius)));}else{
            ui.c.fill_circle(cx,cy+scale,radius+1.5*scale,Color::rgb(0x141718));
            ui.c.fill_circle(cx,cy,radius+0.5*scale,mix(cap,Color::rgb(0x202627),0.35));
            ui.c.fill_circle(cx,cy-0.7*scale,radius-1.2*scale,mix(cap,Color::rgb(0x515550),0.24));
            let angle=(-225.0+270.0*s.knob(id))*PI/180.0;
            let tip=(cx+angle.cos()*(radius-1.4*scale),cy+angle.sin()*(radius-1.4*scale));
            ui.c.polyline(&[(cx+angle.cos()*radius*0.25,cy+angle.sin()*radius*0.25),tip],(1.8*scale).max(0.5),Color::rgb(0xf3edd9));
            }
            let v=s.knob(id);
            let text=match id {
                KnobId::Eq(_,_,0)=>format!("{:+.0}dB",(v*2.0-1.0)*18.0),
                KnobId::Eq(..)=>{let hz=os_dsp::mixer::eq_frequency((v*65536.0).round() as i32);if hz>=1000.0 {format!("{:.1}kHz",hz/1000.0)}else{format!("{hz:.0}Hz")}},
                KnobId::Strip(_,0)=>if v==0.0{"OUT".into()}else{format!("{:.0}Hz",os_dsp::channel_strip::cut_hz(v))},
                _=>if v==0.0{"OUT".into()}else{format!("{:.0}%",v*100.0)},
            };
            if row>=50.0 {ui.c.text_center(console_font(ui,s,8),Rect::new(cx-18.0,y+row-12.0,36.0,9.0),ui.t.tx(0xd7d4c6),&text);}
            if active==Some(id) && (row<50. || matches!(id,KnobId::Strip(_,3))) {
                match id {
                    KnobId::Eq(_,_,k) => {
                        let label=if k==0 {format!("{:+.1} dB",(v*2.-1.)*18.)}else{text};
                        let width=(ui.c.text_width(console_font(ui,s,10).bold(),&label)+8.).min(r.w);
                        readout=Some((Rect::new((cx-width/2.).clamp(r.x,r.right()-width),cy+radius+3.*scale,width,16.),label,None));
                    }
                    KnobId::Strip(_,0|3) => {
                        let label=if v==0. {"OFF".into()}else if id==KnobId::Strip(n,0) {format!("{:.0}",os_dsp::channel_strip::cut_hz(v))}else{format!("{:.0}%",v*100.)};
                        let unit=if v>0. && id==KnobId::Strip(n,0){Some("Hz")}else{None};
                        let width=(ui.c.text_width(console_font(ui,s,9).bold(),&label)+10.).ceil().clamp(22.,r.w);
                        readout=Some((Rect::new(r.cx()-width/2.,cy-9.,width,18.),label,unit));
                    }
                    _=>{}
                }
            }
        }
    }
    // Draw after all rows so the temporary value stays readable over tight row spacing.
    // No hit target: the readout never steals the ongoing drag or changes knob geometry.
    if let Some((box_r,label,unit))=readout {
        let value_ink = if vintage {
            crate::vintage::metal(ui.c, box_r, Color::rgb(0x68777c));
            ui.c.fill_rect(box_r.inset(2.0), Color::rgb(0x26383c));
            Color::rgb(0xc4d9cf)
        } else {
            ui.c.fill_rrect(box_r,3.,Color::rgb(0xe8c74f));
            Color::rgb(0x171717)
        };
        ui.c.push_clip(box_r);
        if let Some(unit)=unit {
            ui.c.text_center(console_font(ui,s,9).bold(),Rect::new(box_r.x,box_r.y,box_r.w,11.),value_ink,&label);
            ui.c.text_center(console_font(ui,s,7),Rect::new(box_r.x,box_r.y+10.,box_r.w,7.),value_ink,unit);
        }else{ui.c.text_center(console_font(ui,s,if box_r.h==18. {9}else{10}).bold(),box_r,value_ink,&label);}
        ui.c.pop_clip();
    }
}

fn knobs_switch(ui:&mut Ui,s:&State,n:u16,r:Rect){
 let bypass=mx::track(s,n).is_some_and(|m|m.strip_bypassed);
 let hover=ui.hit(r,Action::Mix(MHit::StripBypass(n)),if bypass {"STRIP bypassed (down) — click to restore the channel knobs. Plugin slots and parametric EQ are separate."}else{"STRIP active (up) — click to bypass the channel knobs, keeping their settings. Plugin slots and parametric EQ are separate."});
 let ink=if bypass {ui.t.h(0xe4b583)}else{ui.t.h(0xa7d994)};
 ui.c.text_center(console_font(ui,s,if r.w<20. {7}else{8}).bold(),Rect::new(r.x-2.,r.y,r.w+4.,10.),ui.t.text,"STRIP");
 // A two-ended rocker in a small faceplate, rather than a sliding thumb.
 let plate=Rect::new(r.x+1.,r.y+12.,r.w-2.,23.);
 if ui.t.style.vintage_2005 {
     console_skin::button(ui, plate, !bypass, hover, ink);
     console_skin::lamp(ui, plate.cx(), plate.cy(), !bypass, ink, 3.0);
     ui.c.text_center(Font::vintage(7).bold(), Rect::new(r.x-2.,r.y+36.,r.w+4.,8.), ink, if bypass { "BYP" } else { "ON" });
     return;
 }
 ui.c.fill_rrect(plate.outset(1.),2.,ui.t.s(0x10181a));
 ui.c.fill_rrect(plate,2.,if hover {ui.t.tx(0xb0b9b5)}else{ui.t.s(0x798682)});
 let rocker=Rect::new(plate.x+2.,plate.y+2.,plate.w-4.,plate.h-4.);
 ui.c.fill_rect(rocker,ui.t.tx(0xa7b1ac));
 let middle=rocker.cy();
 let (raised_y,pressed_y)=if bypass {(rocker.y,middle)}else{(middle,rocker.y)};
 ui.c.fill_rect(Rect::new(rocker.x,pressed_y,rocker.w,rocker.h/2.),ui.t.tx(0x8d9a93));
 ui.c.fill_rect(Rect::new(rocker.x,raised_y,rocker.w,rocker.h/2.),ui.t.tx(0xe3e6da));
 ui.c.line(rocker.x,middle,rocker.right(),middle,1.,ui.t.s(0x65766d));
 // The raised end casts a shallow edge; opposite end sits inside the frame.
 let edge=if bypass {rocker.y}else{rocker.bottom()-2.};
 ui.c.fill_rect(Rect::new(rocker.x,edge,rocker.w,2.),ui.t.tx(0xbac3b6));
 ui.c.fill_rect(Rect::new(rocker.cx()-1.,if bypass {rocker.bottom()-5.}else{rocker.y+3.},2.,2.),ink);
 if mx::view(s)==mx::WIDE {console_skin::rocker(ui,plate,!bypass,hover);}
 ui.c.text_center(console_font(ui,s,7).bold(),Rect::new(r.x-2.,r.y+36.,r.w+4.,8.),ink,if bypass{"BYP"}else{"ON"});
}


fn headphones_controls(ui:&mut Ui,s:&State,r:Rect,compact:bool){
 let on=s.headphones.on;
 let icon=Rect::new(r.x, r.y,if compact{19.}else{38.},if compact{24.}else{34.});
 let hint=if s.headphones.name.is_empty(){"Choose headphones — searchable AutoEq correction for monitoring only".into()}else{format!("{} | {} | Click to choose headphones. Monitoring only; never exported.",s.headphones.name,if on{"ON"}else{"OFF"})};
 let hover=ui.hit(icon,Action::Mix(MHit::Headphones),hint);
 ui.c.fill_rrect(icon,4.,if hover{ui.t.button_hi}else{ui.t.s(0x263237)});
    if mx::view(s)==mx::WIDE {console_skin::button(ui,icon,on,hover,Color::rgb(0x9fb986));}
 let cx=icon.cx();let cy=icon.cy()-1.;let k=if compact{0.65}else{1.};let c=if on{ui.t.accent}else{ui.t.text};
 // Draw a familiar headband with two ear cups using vector primitives.
 let pts:Vec<_>=(0..=16).map(|i|{let a=std::f32::consts::PI+i as f32*std::f32::consts::PI/16.;(cx+a.cos()*10.*k,cy+a.sin()*10.*k)}).collect();
 for p in pts.windows(2){ui.c.line(p[0].0,p[0].1,p[1].0,p[1].1,2.*k,c);}
 for side in [-1.,1.]{ui.c.fill_rrect(Rect::new(cx+side*9.*k-2.5*k,cy-2.*k,5.*k,10.*k),2.*k,c);}
 let sw=Rect::new(r.right()-if compact{13.}else{22.},r.y+1.,if compact{12.}else{20.},if compact{22.}else{31.});
 let hover=ui.hit(sw,Action::Mix(MHit::HeadphonesToggle),if on{"Headphone correction ON (up) — click to bypass"}else{"Headphone correction OFF (down) — click to enable"});
 ui.c.fill_rrect(sw,2.,if hover{ui.t.tx(0xb0b9b5)}else{ui.t.s(0x798682)});
 let inner=sw.inset(2.);let half=inner.h/2.;
 ui.c.fill_rect(Rect::new(inner.x,inner.y,inner.w,half),ui.t.tx(if on{0x8d9a93}else{0xe3e6da}));
 ui.c.fill_rect(Rect::new(inner.x,inner.y+half,inner.w,half),ui.t.tx(if on{0xe3e6da}else{0x8d9a93}));
 ui.c.fill_rect(Rect::new(inner.cx()-1.,if on{inner.y+3.}else{inner.bottom()-5.},2.,2.),if on{ui.t.accent}else{ui.t.s(0x3f4943)});
    if mx::view(s)==mx::WIDE {console_skin::rocker(ui,sw,on,hover);}
 if !compact {
  ui.c.text_center(console_font(ui,s,8).bold(),Rect::new(r.x,r.y+39.,r.w,12.),ui.t.text,if on{"PHONES ON"}else{"PHONES OFF"});
  ui.c.text_center(console_font(ui,s,8),Rect::new(r.x,r.y+54.,r.w,12.),ui.t.text_dim,"MONITOR ONLY");
 }
}

fn master_assistant_controls(ui:&mut Ui,s:&State,r:Rect,compact:bool) {
    let on=crate::master_assistant::enabled(s);
    ui.c.text_center(console_font(ui,s,if compact {7}else{8}).bold(),Rect::new(r.x,r.y,r.w,11.),ui.t.text,if compact {"MASTER"}else{"MASTER ASSIST"});
    let power=Rect::new(r.x,r.y+14.,if compact {14.}else{23.},23.);
    let open=Rect::new(power.right()+3.,power.y,r.right()-power.right()-3.,23.);
    let hover=ui.hit(power,Action::Mix(MHit::MasterAssistantToggle),if on {"Master Assistant ON — click to bypass in playback and export"}else{"Master Assistant OFF — click to enable in playback and export"});
    ui.c.fill_rrect(power,3.,if hover {ui.t.button_hi}else if on {ui.t.a(0x50694a)}else{ui.t.s(0x303b33)});
    if mx::view(s)==mx::WIDE {console_skin::button(ui,power,on,hover,Color::rgb(0x9fb986));}
    ui.c.text_center(console_font(ui,s,if compact {7}else{8}).bold(),power,if on {ui.t.accent}else{ui.t.text_dim},if compact {if on {"I"}else{"O"}}else if on {"ON"}else{"OFF"});
    let hover=ui.hit(open,Action::Mix(MHit::MasterAssistant),"Open Master Assistant controls — oXygen engine; normal song exports include it when enabled");
    ui.c.fill_rrect(open,3.,if hover {ui.t.button_hi}else{ui.t.s(0x34443a)});
    ui.c.stroke_rrect(open,3.,1.,ui.t.s(0x65765d));
    if mx::view(s)==mx::WIDE {console_skin::button(ui,open,false,hover,Color::rgb(0x9fb986));}
    ui.c.text_center(console_font(ui,s,if compact {7}else{9}).bold(),open,ui.t.text,if compact {"..."}else{"OPEN"});
}

fn vertical_rect(left:f32,width:f32,ly:&Ly)->Rect {
    Rect::new(left+3.,ly.y(PANEL_Y0)+3.,width-6.,(ly.y(LED_Y)-46.-ly.y(PANEL_Y0)-3.).max(0.))
}
/// Same inspector, fixed master, frame and scrolling origin as the ordinary Full mixer.
pub(crate) fn vertical_panes(s:&State,outer:Rect)->Vec<crate::rack_mixer::Pane>{
    let r=super::frame_rects(s,Win::Mixer,outer).0;if r.h<2.*TITLE_H{return vec![];}
    let width=strip_width(s);let left=s.vintage_2005 || mx::inspector_left();let x=r.x+if left{221.}else{0.};
    let area=Rect::new(x+9.+width,r.y+TITLE_H,r.w-221.-9.-width-7.,r.h-TITLE_H);let ly=Ly::for_state(r,s);
    let mut x=area.x-s.mixer_scroll;
    (1..=mx::inserts(s)).filter_map(|n|{let w=channel_width(s,n);let rect=vertical_rect(x,w,&ly);x+=w;let clip=rect.intersect(&area);(!clip.is_empty()).then_some(crate::rack_mixer::Pane{track:n,rect,clip})}).collect()
}

pub(crate) fn vertical_pane_height(outer:Rect)->f32{{let mut ly=Ly::new(outer);ly.compression=STRIP_FADER_COMPRESSION;vertical_rect(0.,128.,&ly).h}}

pub(crate) fn mixer_height_for_editor(height:f32)->f32{FIXED_H+(height+23.)*(REF_H-FIXED_H)/(490.+289.*(1.-STRIP_FADER_COMPRESSION))}

// Native channel-strip and full-mixer controls share the selected finish.
fn console_knob(ui: &mut Ui, id: KnobId, x:f32, y:f32, r:f32, value:f32, color:Color, hover:bool) {
    console_skin::knob(ui,id,x,y,r,value,color,hover);
}
