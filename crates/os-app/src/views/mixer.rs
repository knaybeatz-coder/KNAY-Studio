// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Mixer window (wide layout: strips plus the selected track's slot panel).
// spec: ui/main-window.md §7 (layout), ui/mixer.md (controls, hints, routing look); fader law: audio/gain-laws.md

use std::f32::consts::PI;

use os_ui::kit::lemon as lk;
use os_ui::{Color, Font, Glyph, Rect};

use super::{led, resize_zones, window_body, window_buttons, FrameInfo};
use crate::knobs::KnobId;
use crate::app::{Action, Live, State, Ui, Win, TITLE_H};
use crate::meters;
use crate::mixer::{self as mx, MHit};
use crate::icons::{self, Icon};

pub const STRIP_W: f32 = 84.0;
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
}

impl Ly {
    fn new(r: Rect) -> Ly {
        Ly { top: r.y, k: scale(r) }
    }

    fn y(&self, abs: f32) -> f32 {
        let (m0, m1, f0, f1) = (130.0 - REF_TOP, 620.0 - REF_TOP, 692.0 - REF_TOP, 981.0 - REF_TOP);
        let ry = abs - REF_TOP;
        let k = self.k;
        let panel=(m1-m0)+(f1-f0)*(1.-FADER_COMPRESSION);
        let y = if ry < m0 {
            ry
        } else if ry < m1 {
            m0 + (ry - m0) / (m1-m0) * panel * k
        } else if ry < f0 {
            m0 + panel * k + (ry - m1)
        } else if ry < f1 {
            m0 + panel * k + (f0 - m1) + (ry - f0) * k * FADER_COMPRESSION
        } else {
            m0 + panel * k + (f0 - m1) + (f1 - f0) * k * FADER_COMPRESSION + (ry - f1)
        };
        self.top + y
    }
}

pub fn fader_db_text(v: f64) -> String {
    let gain = os_engine::laws::fader_gain(v);
    if gain <= 0.0 { "-inf dB".into() } else { format!("{:+.1} dB", 20.0 * gain.log10()) }
}


pub fn paint(ui: &mut Ui, s: &State, r: Rect, f: &FrameInfo) {
    let panel_w = 221.0;
    window_body(ui, r);
    let outer = r;
    let inner = super::float_frame(ui, s, Win::Mixer, r);
    let r = inner.unwrap_or(r);
    let full_title = r;
    let left = mx::inspector_left();
    let inspector = Rect::new(if left { r.x } else { r.right() - panel_w }, r.y, panel_w, r.h);
    let r = if left { Rect::new(r.x + panel_w, r.y, r.w, r.h) } else { r };
    let client = super::window_frame_ex(ui, Win::Mixer, Rect::new(r.x, r.y, r.w - panel_w, r.h), "", 0.0, false, inner.is_some());
    title_tools(ui, r);
    if r.h < 2.0 * TITLE_H {
        // Collapsed to the title bar: only the slot panel's title with the window buttons.
        s.cables.borrow_mut().clear();
        slot_panel(ui, s, inspector);
        window_buttons(ui, Win::Mixer, full_title.right() - 58.0, full_title.y, ui.t.tx(0xd4dade), ui.t.tx(0xffffff));
        return;
    }
    let ly = Ly::new(r);

    let strips = Rect::new(r.x, client.y, r.w - panel_w - 7.0, client.h);
    ui.c.fill_rect(Rect::new(r.x, client.y, r.w - panel_w, client.h), ui.t.s(0x232527));
    ui.hit(strips, Action::MixerArea, "Mixer");
    let inserts = mx::inserts(s);
    let bottom = r.bottom();

    // Fixed master strip; inserts scroll immediately beside it.
    ui.c.fill_rect(Rect::new(r.x, ly.y(HEAD_Y0), STRIP_W, HEAD_Y1 - HEAD_Y0 + 1.0), ui.t.s(0x2b2e30));
    ui.c.text_center(Font::new(17), Rect::new(r.x, ly.y(HEAD_Y0), STRIP_W, 21.0), ui.t.s(0x6d7275), "M");
    let lemon = super::lemon(ui);
    let area = Rect::new(r.x + 9.0 + STRIP_W, client.y, strips.w - 9.0 - STRIP_W, client.h);
    ui.c.fill_rect(Rect::new(r.x + STRIP_W, client.y, 9.0, client.h), ui.t.s(0x1a1e20));
    for k in 0..3 {
        ui.c.fill_rect(Rect::new(r.x + 4.0 + STRIP_W, ly.y(497.0 + 3.0 * k as f32), 1.0, 1.0), ui.t.s(0x3a3e41));
    }
    ui.c.push_clip(area);
    let first = (s.mixer_scroll / STRIP_W) as usize;
    let count = (area.w / STRIP_W).ceil() as usize + 2;
    for n in (first + 1..first + 1 + count).filter(|n| *n <= usize::from(inserts)) {
        let left = area.x + STRIP_W * (n - 1) as f32 - s.mixer_scroll;
        strip(ui, s, n as u16, left, bottom, f, &ly);
    }
    // Narrow boundary targets reveal an insertion button only while hovered.
    for n in (first+1..first+1+count).filter(|n|*n<=usize::from(inserts)) {
        let x=area.x+STRIP_W*(n-1) as f32-s.mixer_scroll;
        if x<area.x || x>area.right()-10.0 {continue;}
        let lane=Rect::new(x-4.0,client.y,8.0,client.h-52.0);
        let action=Action::Mix(MHit::InsertBefore(n as u16));
        if ui.hit(lane,action.clone(),"Insert an empty mixer track here") {
            let y=ui.mouse.1.clamp(lane.y+10.0,lane.bottom()-10.0);
            let button=Rect::new(x-9.0,y-9.0,18.0,18.0);
            ui.hit(button,action,"Insert an empty mixer track here");
            ui.c.fill_rrect(button,4.0,ui.t.button_hi);
            ui.c.stroke_rrect(button,4.0,1.0,ui.t.tx(0xc4cbd0));
            icons::draw(ui.c,Icon::Plus,button.inset(3.0),ui.t.tx(0xffffff));
        }
    }
    // "+" after the last strip, at the fader handle's height (spec: ui/mixer.md §1).
    let px = area.x + STRIP_W * f32::from(inserts) - s.mixer_scroll + 14.0;
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
    let sb = Rect::new(area.x + 13.0, bottom - 14.0, area.w - 20.0, 9.0);
    super::scrollbar(ui, sb, false, s.mixer_scroll, area.w, (f32::from(inserts) + 1.0) * STRIP_W, crate::app::Scroll::MixerX);
    chevron(ui, area.x + 5.0, bottom - 9.5, false, ui.t.s(0x5d6366));
    cables(ui, s, strips, area, bottom);

    slot_panel(ui, s, inspector);
    window_buttons(ui, Win::Mixer, full_title.right() - 58.0, full_title.y, ui.t.tx(0xd4dade), ui.t.tx(0xffffff));
    resize_zones(ui, s, Win::Mixer, outer);
}

/// The routing cables (spec: ui/mixer.md §5): one from the current track's ⬇ into each of its send knobs, sagging
/// and swinging (crate::cable). Sockets on strips scrolled out of view still hold their cable's end.
fn cables(ui: &mut Ui, s: &State, strips: Rect, area: Rect, bottom: f32) {
    let socket_x = |n: u16| match n {
        0 => strips.x + STRIP_W / 2.0,
        n => area.x + STRIP_W * f32::from(n - 1) - s.mixer_scroll + STRIP_W / 2.0,
    };
    // Each cable is plugged in under the middle of its ⬇ and its knob, which are drawn again over the cables' ends.
    let (ay, ky) = (bottom - 38.0, bottom - 40.0);
    let cur = s.selected_mixer;
    let from = (socket_x(cur), ay);
    let sends: Vec<(u16, f64)> = (0..=mx::inserts(s)).filter(|&n| n != cur).filter_map(|n| Some((n, mx::send(s, cur, n)?))).collect();
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

fn title_tools(ui: &mut Ui, r: Rect) {
    let y = r.y + 11.5;
    let ob = Rect::new(r.x + 3.0, y - 9.0, 16.0, 18.0);
    let hov = ui.hit(ob, Action::Mix(MHit::Options), "Mixer options");
    chevron(ui, r.x + 11.0, y, true, if hov { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
    let pb = Rect::new(r.x + 27.0, y - 8.0, 16.0, 16.0);
    let hov = ui.hit(pb, Action::Mix(MHit::AddAfter), "Add a new track after the selected track");
    icons::draw(ui.c, Icon::Plus, pb.inset(1.0), if hov { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
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

/// One strip; `left` is its left edge (44 px pitch). Columns: knobs at +22, meter / fader at +32,
/// name and small controls at +12 (spec §7).
fn strip(ui: &mut Ui, s: &State, n: u16, left: f32, bottom: f32, f: &FrameInfo, ly: &Ly) {
    let t = ui.t;
    let track = s.project.mixer.iter().find(|m| m.index == n);
    let selected = s.selected_mixer == n;
    let also = !selected && mx::is_selected(s, n);
    let (kx, mx, sx) = (left + STRIP_W / 2.0, left + STRIP_W / 2.0 + 10.0, left + STRIP_W / 2.0 - 10.0);
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
    if lemon {
        // Lemon (spec: ui/lemon-design.md §8): each strip is a rounded card on the dark gap colour.
        let card = Rect::new(left, ly.y(HEAD_Y1) + 1.0, STRIP_W, bottom - ly.y(HEAD_Y1) - 1.0);
        ui.c.fill_rect(card, ui.t.s(0x131517));
        ui.c.fill_rrect(Rect::new(card.x + 1.5, card.y + 1.0, card.w - 3.0, card.h - 4.0), lk::r_panel(t), body);
    } else {
        ui.c.fill_rect(Rect::new(left, ly.y(HEAD_Y1) + 1.0, STRIP_W, bottom - ly.y(HEAD_Y1) - 1.0), body);
        ui.c.fill_rect(Rect::new(left + STRIP_W - 1.0, ly.y(HEAD_Y1) + 1.0, 1.0, bottom - ly.y(HEAD_Y1) - 1.0), ui.t.s(0x1c1f21));
    }
    // The whole strip selects (Ctrl adds, Shift renames); controls register on top of it (spec: ui/mixer.md §2).
    ui.hit(Rect::new(left, ly.y(HEAD_Y0), STRIP_W, bottom - ly.y(HEAD_Y0)), Action::Mix(MHit::Strip(n)), mx::name(s, n));
    let hov = ui.hovering(Rect::new(left, ly.y(HEAD_Y0), STRIP_W, ly.y(PANEL_Y1) - ly.y(HEAD_Y0)));

    // Header number (insert strips only; the master's "M" sits in the fixed header band).
    if n != 0 {
        let hr = Rect::new(left, ly.y(HEAD_Y0), STRIP_W, ly.y(HEAD_Y1) - ly.y(HEAD_Y0) + 1.0);
        ui.c.fill_rect(hr, header);
        if selected && lemon {
            lk::lit_in(ui.c, ui.t, Rect::new(hr.x + 3.0, hr.y + 2.0, hr.w - 6.0, hr.h - 4.0), lk::r_btn(t), false);
        } else if selected {
            // A quiet solid selection band replaces fourteen decorative hatch strokes.
            ui.c.fill_rect(hr.inset(2.0), ui.t.s(0x414b50));
        }
        let c = if selected && lemon { lk::ink_on_lit(ui.t) } else if selected || hov { ui.t.tx(0xe1e8eb) } else { ui.t.tx(0x939fa5) };
        ui.c.text_center(Font::new(14), Rect::new(left, ly.y(HEAD_Y0), STRIP_W, 21.0), c, &n.to_string());
    }

    // Dark head panel (name, LED, pan) with a rounded bottom.
    ui.c.fill_rrect(Rect::new(left + 1.0, ly.y(PANEL_Y0), STRIP_W - 2.0, ly.y(PANEL_Y1) - ly.y(PANEL_Y0)), 9.0, ui.t.s(0x1d1f21));
    ui.c.fill_rect(Rect::new(left + 1.0, ly.y(PANEL_Y0), STRIP_W - 2.0, 12.0), ui.t.s(0x1d1f21));

    let standard=n!=0 && track.is_some_and(|m|m.channel_layout==0);
    let meter = Rect::new(left + 6.0, ly.y(METER_Y0) + 4.0, STRIP_W - 12.0,
        (ly.y(METER_Y1) - ly.y(METER_Y0) - 26.0).clamp(38.0, 61.6));
    let meter=if standard {Rect::new(left+14.,ly.y(PANEL_Y0)+29.,STRIP_W-28.,(ly.y(LED_Y)-ly.y(PANEL_Y0)-45.).max(40.))}else{meter};
    ui.live(Live::Meter, meter.outset(2.0));
    let levels = s.mixer.meters.get(usize::from(n)).map_or([0.0; 2], |m| m.vu);
    let clipped = s.mixer.meters.get(usize::from(n)).is_some_and(|m| m.hold.iter().any(|&db| db >= 0.0));
    if standard || s.mixer.peak_view.contains(&n) {
        peak_window(ui, meter, s.mixer.meters.get(usize::from(n)), clipped);
    } else { analog_vu(ui, meter, levels, clipped, selected); }
    let db = levels.map(meters::vu_db);
    ui.hit(meter, Action::Mix(MHit::Meter(n)), format!("Click to switch VU / peak meter | VU L {:+.1} / R {:+.1}", db[0], db[1]));
    let name = track.and_then(|m| m.name.clone()).unwrap_or_else(|| if n == 0 { "Master".into() } else { format!("Insert {n}") });
    let name_c = track.and_then(|m| m.color).map(|c| mix(super::rgb(c), ui.t.tx(0xe1e7eb), 0.22)).unwrap_or(if selected || also { ui.t.text } else { ui.t.tx(0xaab2b7) });
    if bus && !standard {
        let badge = Rect::new(left + 23.0, meter.bottom() + 23.0, STRIP_W - 46.0, 9.0);
        let accent = tint.unwrap_or(ui.t.tx(0xa6bbc5));
        ui.c.fill_rrect(badge, 3.0, mix(ui.t.s(0x252b2f), accent, 0.25));
        ui.c.stroke_rrect(badge, 3.0, 1.0, mix(ui.t.s(0x62676b), accent, 0.45));
        ui.c.text_center(Font::new(7), badge, ui.t.tx(0xe1e7eb), "MIX BUS");
        ui.c.fill_rect(Rect::new(left + 4.0, ly.y(PANEL_Y1) + 2.0, STRIP_W - 8.0, 3.0), accent);
    }
    let name_rect = Rect::new(left + 5.0, if standard {ly.y(PANEL_Y0)+5.}else{meter.bottom()+5.}, STRIP_W - 10.0, 18.0);
    ui.c.push_clip(name_rect);
    ui.c.text_center(Font::new(12), name_rect, name_c, &name);
    ui.c.pop_clip();
    channel_controls(ui, s, n, Rect::new(left + 4.0, meter.bottom() + 36.0, STRIP_W - 8.0, (ly.y(LED_Y) - 16.0 - meter.bottom() - 36.0).max(0.0)));
    let enabled = track.is_none_or(|m| m.enabled);
    ui.hit(Rect::new(kx - 7.0, ly.y(LED_Y) - 7.0, 14.0, 14.0), Action::Mix(MHit::Led(n)), "Mute / solo");
    led(ui, kx, ly.y(LED_Y), enabled);
    let pan = KnobId::MixPan(n);
    ui.hit(Rect::new(kx - 14.0, ly.y(PAN_Y) - 14.0, 28.0, 28.0), Action::Knob(pan), s.knob_text(pan));
    mixer_knob(ui, kx, ly.y(PAN_Y), 10.5, 15.0, s.knob(pan), Some(true));

    // Reverse polarity (⇕) and swap left / right (⇔) buttons, lit when on, then the separation knob (§4).
    // They keep their size and spacing below the fader's top at any mixer height (FL's short mixer stacks them the
    // same way), so they are placed from the fader's top, not scaled with it.
    let (pol, swp) = (track.is_some_and(|m| m.polarity), track.is_some_and(|m| m.swap));
    let top = ly.y(TRACK_Y0);
    let (sep_y, swap_y, ring_y) = (top + SEP_Y - TRACK_Y0, top + SWAP_Y - TRACK_Y0, top + RING_Y - TRACK_Y0);
    let pr = Rect::new(sx - 8.0, sep_y - 8.0, 16.0, 16.0);
    let hp = ui.hit(pr, Action::Mix(MHit::Polarity(n)), "Reverse polarity");
    let mut g = Glyph::new();
    g.polygon(&[(sx - 5.0, sep_y - 1.5), (sx, sep_y - 6.0), (sx + 5.0, sep_y - 1.5)]);
    g.polygon(&[(sx - 5.0, sep_y + 1.5), (sx, sep_y + 6.0), (sx + 5.0, sep_y + 1.5)]);
    ui.c.glyph(if pol { ui.t.h(0xf3b234) } else if hp { ui.t.tx(0xffffff) } else { ui.t.tx(0xb7c3ca) }, &g);
    let wr = Rect::new(sx - 8.0, swap_y - 8.0, 16.0, 16.0);
    let hw = ui.hit(wr, Action::Mix(MHit::Swap(n)), "Swap left & right channels");
    let mut g = Glyph::new();
    g.polygon(&[(sx - 1.0, swap_y - 6.0), (sx - 7.0, swap_y), (sx - 1.0, swap_y + 6.0)]);
    g.polygon(&[(sx + 1.5, swap_y - 6.0), (sx + 7.5, swap_y), (sx + 1.5, swap_y + 6.0)]);
    ui.c.glyph(if swp { Color::rgb(0x5fb6f0) } else if hw { ui.t.tx(0xffffff) } else { ui.t.tx(0xb7c3ca) }, &g);
    ui.c.fill_rect(Rect::new(sx - 0.5, swap_y - 6.0, 1.0, 12.0), ui.t.s(0x5a6266));
    let sep = KnobId::MixSep(n);
    ui.hit(Rect::new(sx - 9.0, ring_y - 9.0, 18.0, 18.0), Action::Knob(sep), s.knob_text(sep));
    mixer_knob(ui, sx, ring_y, 6.0, 9.5, s.knob(sep), Some(true));

    // Fader: track, gain marks, handle (green on the selected strip, with the track lit below it).
    let track_y1 = bottom - 105.0;
    if lemon {
        ui.c.fill_rrect(Rect::new(mx - 2.0, ly.y(TRACK_Y0), 4.0, track_y1 - ly.y(TRACK_Y0)), 2.0, ui.t.s(0x141618));
    } else {
        ui.c.fill_rect(Rect::new(mx - 1.5, ly.y(TRACK_Y0), 3.0, track_y1 - ly.y(TRACK_Y0)), ui.t.s(0x141618));
    }
    fader_marks(ui, mx, track_y1, ly);
    let v = track.and_then(|m| m.volume).unwrap_or(0.8) as f32;
    // max/min, not clamp: a very short window gives track_y1 < ly.y(TRACK_Y0) (clamp would panic).
    let hy = (ly.y(HANDLE_0DB + (0.8 - v) * 274.0)).min(track_y1 - 5.0).max(ly.y(TRACK_Y0) + 5.0);
    if selected {
        if lemon {
            ui.c.fill_rrect(Rect::new(mx - 1.5, hy, 3.0, track_y1 - hy), 1.5, t.accent);
        } else {
            ui.c.fill_rect(Rect::new(mx - 1.0, hy, 2.0, track_y1 - hy), ui.t.a(0x8ec63a));
        }
    }
    let hov = ui.hit(Rect::new(mx - 9.0, ly.y(TRACK_Y0) - 5.0, 18.0, track_y1 - ly.y(TRACK_Y0) + 10.0), Action::Fader(n), mx::level_text("Volume", f64::from(v)));
    fader_handle(ui, mx, hy, selected, hov);

    // Record arm and FX share the fader centerline; navigation stays on the left.
    let master = n == 0;
    let by = bottom;
    let fx_on = !s.mixer.all_fx_bypassed && track.is_none_or(|m| m.slots_enabled);
    let fx_button = Rect::new(mx - 15.0, by - 79.0, 30.0, 17.0);
    let fx_hover = ui.hit(fx_button, Action::Mix(MHit::Effects(n)), if fx_on { "Effects enabled — click to bypass this channel's plugins" } else { "Effects bypassed — click to enable this channel's plugins" });
    ui.c.fill_rrect(fx_button, 3.0, if fx_on { ui.t.a(0x465d49) } else { ui.t.s(0x272d30) });
    ui.c.stroke_rrect(fx_button, 3.0, 1.0, if fx_hover { ui.t.accent } else { ui.t.s(0x64776a) });
    ui.c.text_center(Font::new(11), fx_button, if fx_on { ui.t.tx(0xe4efd8) } else { ui.t.text_dim }, "FX");
    // One aligned navigation column, visually separate from record arm and FX.
    for (i, section) in mx::Section::ALL.into_iter().enumerate() {
        let b = Rect::new(left + 8.0, by - 150.0 - if master { 30.0 } else { 0.0 } + i as f32 * 16.0, 22.0, 12.0);
        let target = section.target(s);
        let hover = if let Some(dst) = target {
            ui.hit(b, Action::Mix(MHit::Jump(n, section)), format!("Scroll to {}: {} (keeps current channel selected)", section.label(), mx::name(s,dst)))
        } else {
            ui.hit(b, Action::Swallow, format!("No {} section found in this project", section.label()));
            false
        };
        ui.c.fill_rrect(b, 2.0, if hover { ui.t.button_hi } else { ui.t.s(0x252d32) });
        ui.c.stroke_rrect(b, 2.0, 1.0, if hover { ui.t.accent } else { ui.t.s(0x3b474d) });
        ui.c.text_center(Font::new(8), b, if target.is_some() {ui.t.tx(0xb9c8d0)} else {ui.t.text_dim}, section.label());
    }
    if master {
        let all = Rect::new(mx - 25.0, by - 98.0, 50.0, 17.0);
        let on = !s.mixer.all_fx_bypassed && s.project.mixer.iter().any(|m|m.slots_enabled);
        let hover = ui.hit(all, Action::Mix(MHit::AllEffects), if on { "Bypass ALL mixer effects (keeps your individual choices)" } else { "Restore ALL mixer effects to your previous choices" });
        ui.c.fill_rrect(all, 3.0, if on { ui.t.a(0x50694a) } else { ui.t.s(0x665037) });
        ui.c.stroke_rrect(all, 3.0, 1.0, if hover { ui.t.accent } else { ui.t.s(0x899182) });
        ui.c.text_center(Font::new(10), all, ui.t.tx(0xe4efd8), if on { "ALL FX" } else { "FX OFF" });
    }
    if !master {
    let armed=s.mixer.recording.armed.contains(&n);
    let record = Rect::new(mx - 15.0, by - 98.0, 30.0, 17.0);
    let rec_hover = ui.hit(record, Action::Mix(MHit::Disk(n)), if armed { "Disarm disk recording" } else { "Arm disk recording (select an input to arm automatically)" });
    ui.c.fill_rrect(record, 3.0, if armed { ui.t.h(0x8f292e) } else { ui.t.s(0x2b3337) });
    ui.c.stroke_rrect(record, 3.0, 1.0, if rec_hover || armed { ui.t.h(0xff6969) } else { ui.t.s(0x667278) });
    ui.c.fill_circle(mx, record.cy(), 4.5, if armed { ui.t.h(0xff6868) } else { ui.t.h(0xb85858) });

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
        let hov = ui.hit(Rect::new(kx - 8.0, ay - 8.0, 16.0, 14.0), Action::Mix(MHit::Route(n)), hint);
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
    let border = if selected { ui.t.accent } else if also { ui.t.tx(0x8b999f) } else { ui.t.s(0x464f54) };
    ui.c.stroke_rrect(Rect::new(left + 0.75, top + 0.75, STRIP_W - 1.5, height - 1.5), 0.0, 1.0, border);
    if mx::has_separator(s, n) {
        ui.c.fill_rect(Rect::new(left, top, 6.0, height), ui.t.s(0x919da3));
    }

}

/// The current track's green ⬇ (centred on `ay`): where its cables leave.
fn down_arrow(ui: &mut Ui, kx: f32, ay: f32) {
    let a = [(kx - 3.0, ay - 6.0), (kx + 3.0, ay - 6.0), (kx + 3.0, ay - 1.0), (kx + 7.0, ay - 1.0), (kx, ay + 6.0), (kx - 7.0, ay - 1.0), (kx - 3.0, ay - 1.0)];
    ui.c.fill_polygon(&a, ui.t.accent);
}

/// The send knob from the current track into track `n`, with its level arc (none at zero).
fn send_knob(ui: &mut Ui, s: &State, n: u16, kx: f32, ky: f32, level: f64) {
    let id = KnobId::Send(s.selected_mixer, n);
    mixer_knob(ui, kx, ky, 10.0, 13.0, s.knob(id), None);
    if level > 0.0 {
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
    ui.c.fill_rect(r, ui.t.s(0x2e3133));
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
    ui.c.text_clip(Font::new(17), r.x + 1.0, r.y + 2.0, r.w - if mx::inspector_left() { 4.0 } else { 70.0 }, ui.t.tx(0xd4dade), &title);
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
    ui.c.text(Font::new(9).bold(), r.x + 6.0, r.y + 43.0, ui.t.s(0x40464a), "EQ");
    let combo=Rect::new(r.x+27.,r.y+28.,159.,23.);
    let label=s.mixer.recording.inputs.get(&n).map(|i|format!("{} - {}",i.label(),i.device)).unwrap_or_else(||"(none)".into());
    let hov=ui.hit(combo,Action::Mix(MHit::Input(n)),format!("Audio input: {label}. Selecting an input arms audio recording"));
    ui.c.fill_rect(combo,if hov{t.button}else{input_bg});
    ui.c.text_clip(Font::new(13),combo.x+4.,combo.y+3.,combo.w-20.,t.text,&label);
    chevron(ui,combo.right()-8.,combo.cy(),true,t.text);
    // Ten effect slots.
    for i in 0..10 {
        let row = Rect::new(r.x + 1.0, r.y + 61.0 + 33.0 * kp * i as f32, r.w - 2.0, 28.0 * kp);
        let k = i as u8;
        let plugin = mx::slot_name(s, n, k);
        let fs = (18.0 * kp.max(0.62)).round() as u16;
        let hint = format!("FX slot {} options", i + 1);
        let ha = ui.hit(Rect::new(row.x, row.y, 19.0, row.h), Action::Mix(MHit::SlotArrow(k)), hint.clone());
        let hn = ui.hit(Rect::new(row.x + 19.0, row.y, row.w - 64.0, row.h), Action::Mix(MHit::SlotName(k)), hint);
        let hov = ha || hn;
        if super::lemon(ui) {
            let face = if hov { ui.t.s(0x2f3335) } else { ui.t.s(0x2a2d2f) };
            lk::outlined(ui.c, Rect::new(row.x + 2.0, row.y, row.w - 4.0, row.h), lk::r_btn(t).min(row.h / 2.0), ui.t.s(0x1f2224), face);
        } else {
            ui.c.fill_rect(row, ui.t.s(0x1f2224));
            ui.c.fill_rect(row.inset(1.0), if hov { ui.t.s(0x2f3335) } else { ui.t.s(0x2a2d2f) });
        }
        chevron(ui, row.x + 10.0, row.cy(), true, if ha { ui.t.tx(0xffffff) } else { ui.t.tx(0xc4ccd0) });
        let (f, name, ink) = match &plugin {
            Some(p) => (Font::new(fs), p.to_string(), ui.t.tx(0xc9d0d4)),
            None => (Font::new(fs).italic(), format!("Slot {}", i + 1), ui.t.s(0x50575b)),
        };
        // Lemon centres the name's capitals on the row, level with the chevron, knob and LED: the baseline sits half a
        // cap height (the face's is about 0.72 of its size) under the middle, measured on the face drawn (the italic
        // one has its own ascent). Classic keeps FL's place.
        let ty = if super::lemon(ui) { (row.cy() + 0.36 * f32::from(fs) - ui.c.ascent(f)).round() } else { row.y + 4.0 * kp };
        ui.c.text_clip(f, row.x + 19.0, ty, 140.0, ink, &name);
        let mix = KnobId::SlotMix(n, k);
        ui.hit(Rect::new(row.right() - 46.0, row.y, 26.0, row.h), Action::Knob(mix), s.knob_text(mix));
        mixer_knob(ui, row.right() - 33.0, row.cy(), 10.5 * q, 12.0 * q, s.knob(mix), Some(false));
        ui.c.stroke_circle(row.right() - 33.0, row.cy(), 11.8 * q, 1.5, ui.t.s(0x55595c));
        ui.hit(Rect::new(row.right() - 18.0, row.y, 18.0, row.h), Action::Mix(MHit::SlotLed(k)), "Mute / solo  (ctrl)0..9");
        led(ui, row.right() - 10.0, row.cy(), track.is_none_or(|m| m.slot_on[i]));
    }
    let sep = r.y + 61.0 + 330.0 * kp;
    ui.c.fill_rect(Rect::new(r.x, sep, r.w, 1.0), ui.t.s(0x1f2224));

    crate::mini_eq::draw(ui, s, Rect::new(r.x + 4.0, sep + 6.0, r.w - 8.0, (r.bottom() - 72.0 - sep - 6.0).max(30.0)));
    let b = r.bottom();

    // Output rows at the bottom: audio outputs are not available yet, greyed out.
    ui.c.fill_rect(Rect::new(r.x, b - 66.0, r.w, 1.0), ui.t.s(0x1f2224));
    ui.c.fill_rect(Rect::new(r.x, b - 33.0, r.w, 1.0), ui.t.s(0x1f2224));
    let mut g = Glyph::new();
    g.ring(r.x + 14.0, b - 48.5, 8.0, 1.5).rect(r.x + 13.3, b - 53.0, 1.4, 5.0).rect(r.x + 13.3, b - 49.2, 4.0, 1.4);
    ui.c.glyph(ui.t.s(0x40464a), &g);
    ui.off(Rect::new(r.x + 25.0, b - 60.0, r.w - 28.0, 22.0), "Audio output target");
    ui.c.text(Font::new(17), r.x + 30.0, b - 59.0, ui.t.tx(0xd4dade), "(none)");
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

fn channel_controls(ui:&mut Ui,s:&State,n:u16,r:Rect) {
    if n==0 || r.h<80.0 || mx::track(s,n).is_some_and(|m|m.channel_layout==0) {return;}
    let vocal=mx::track(s,n).is_some_and(|t|t.is_vocal());
    let row=(r.h/if vocal {8.0}else{6.0}).min(57.0);
    let radius=(row*0.22).min((row-13.0)*0.5).clamp(3.0,11.0);
    let controls=[("LOW CUT",None,0xc8c2ad),("DRIVE",None,0xd1a77e),("HIGH",Some(2),0xc98b73),("MID",Some(1),0x89b593),("LOW",Some(0),0x84a7c1),("COMP",None,0xcbb578),("DS",None,0x91b9c9),("TUNE",None,0xb9a0d1)];
    let order:&[usize]=if vocal {&[7,0,1,2,3,4,5,6]}else{&[0,1,2,3,4,5]};
    for (position,&i) in order.iter().enumerate() {
        let (label,band,color)=&controls[i];
        let y=r.y+position as f32*row;
        ui.c.fill_rrect(Rect::new(r.x,y,r.w,row-2.0),3.0,ui.t.s(0x292d2d));
        if i==0 || i==5 {
            let left=Rect::new(r.x,y,r.w*0.54,10.0);
            ui.c.text_center(Font::new(8),left,Color::rgb(*color),if i==5 {"COMP"}else{label});
            ui.c.text_center(Font::new(8),Rect::new(r.x+r.w*0.54,y,r.w*0.46,10.0),Color::rgb(*color),if i==0 {"GATE"}else{"TAME"});
            if i==5 {ui.c.fill_polygon(&[(left.right()-5.0,y+3.0),(left.right()-1.0,y+3.0),(left.right()-3.0,y+6.0)],Color::rgb(*color));}
            if i==5 {let profile=mx::track(s,n).map(|t|t.strip[5]).unwrap_or(0).min(5) as usize;ui.hit(left,Action::Mix(MHit::CompProfile(n)),format!("Compressor profile: {}",os_dsp::channel_strip::PROFILES[profile].name));}
        }else{
            let scale=if band.is_some(){eq_knob_layout(row).0}else{1.};
            ui.c.text_center(Font::new((8.*scale).round().max(2.) as u16),Rect::new(r.x,y,if i==7 {r.w*0.35}else if i==1 {r.w*0.54}else{r.w},10.*scale),Color::rgb(*color),label);
        }
        let cy=y+10.0+(row-12.0)*0.45;
        if i==1 {
            let kind=mx::track(s,n).map(|t|t.strip[6]).unwrap_or(0).min(3) as usize;
            let bh=(row-16.0).clamp(10.0,20.0);
            let b=Rect::new(r.x+r.w*0.54,cy-bh*0.5,r.w*0.46-1.0,bh);
            let hover=ui.hit(b,Action::Mix(MHit::DriveType(n)),format!("Drive type: {}",os_dsp::channel_strip::DRIVE_TYPES[kind]));
            ui.c.fill_rrect(b,3.0,if hover{ui.t.button_hi}else{ui.t.s(0x202729)});
            ui.c.stroke_rrect(b,3.0,1.0,ui.t.s(0x59636a));
            ui.c.text_center(Font::new(8),Rect::new(b.x,b.y,b.w-6.0,b.h),Color::rgb(*color),os_dsp::channel_strip::DRIVE_TYPES[kind]);
            ui.c.fill_polygon(&[(b.right()-6.0,b.cy()-1.0),(b.right()-2.0,b.cy()-1.0),(b.right()-4.0,b.cy()+2.0)],Color::rgb(*color));
        }
        if i==7 {
            let m=mx::track(s,n).unwrap();
            let power=Rect::new(r.x+r.w*0.37,y+1.,11.,7.);
            ui.hit(power.outset(1.),Action::Mix(MHit::TunePower(n)),if m.tune_enabled {"Tune ON: click to bypass without changing settings"}else{"Tune OFF: no tuning delay. Click to enable"});
            ui.c.fill_rrect(power,3.,if m.tune_enabled {Color::rgb(0x94be79)}else{Color::rgb(0x51585c)});
            ui.c.fill_circle(if m.tune_enabled{power.right()-3.5}else{power.x+3.5},power.cy(),2.3,Color::rgb(0xe1e5dc));
            let label=if m.tune_learn!=0 {"Listening".to_owned()}else if m.tune[2]==0 {"Learn".to_owned()}else{
                let scale=match m.tune[2] {1=>"maj",2=>"min",3=>"h.min",4=>"m.min",5=>"M.pent",6=>"m.pent",7=>"blues",8=>"dor",9=>"phr",10=>"lyd",11=>"mix",_=>"loc"};
                format!("{} {}",os_dsp::snaptune::KEYS[m.tune[1].min(11) as usize].split(" /").next().unwrap(),scale)
            };
            let b=Rect::new(r.x+r.w*0.49,cy-7.,r.w*0.50,14.);
            let hover=ui.hit(b,Action::Mix(MHit::TuneLearn(n)),if m.tune_learn!=0 {"Listening to this vocal: play a varied phrase (6 seconds of voiced notes). Click to cancel.".into()}else{format!("Suggested/manual key: {} {}. Click to learn; right-click to set key / scale.",os_dsp::snaptune::KEYS[m.tune[1].min(11) as usize],os_dsp::snaptune::SCALES[m.tune[2].min(12) as usize])});
            ui.c.fill_rrect(b,3.,if hover{ui.t.button_hi}else{ui.t.s(0x202729)});
            ui.c.stroke_rrect(b,3.,1.,ui.t.s(0x59636a));
            ui.c.text_center(Font::new(7),b,Color::rgb(*color),&label);
        }
        let ids:Vec<KnobId>=if let Some(b)=band {vec![KnobId::Eq(n,*b,0),KnobId::Eq(n,*b,1)]}else{match i {7=>vec![KnobId::Strip(n,8)],6=>vec![KnobId::Strip(n,7)],1=>vec![KnobId::Strip(n,2)],0=>vec![KnobId::Strip(n,0),KnobId::Strip(n,3)],_=>vec![KnobId::Strip(n,1),KnobId::Strip(n,4)]}};
        for (k,id) in ids.iter().copied().enumerate() {
            let cx=if i==6{r.cx()}else if ids.len()==1{r.x+r.w*0.27}else{r.x+r.w*(if k==0{0.27}else{0.73})};
            let (cy,radius,scale)=if band.is_some() {
                let (scale,cy,radius)=eq_knob_layout(row);
                ui.c.text_center(Font::new((7.*scale).round().max(2.) as u16),Rect::new(cx-10.0,y,20.0,8.*scale),ui.t.text_dim,if k==0 {"dB"}else{"Hz"});
                (y+cy,radius,scale)
            }else{(y+10.0+(row-12.0)*0.45,radius,1.)};
            ui.hit(Rect::new(cx-radius-3.*scale,y+10.*scale,radius*2.+6.*scale,(row-12.*scale).max(1.)),Action::Knob(id),s.knob_text(id));
            let cap=Color::rgb(*color);
            ui.c.fill_circle(cx,cy+scale,radius+1.5*scale,Color::rgb(0x141718));
            ui.c.fill_circle(cx,cy,radius+0.5*scale,mix(cap,Color::rgb(0x202627),0.35));
            ui.c.fill_circle(cx,cy-0.7*scale,radius-1.2*scale,mix(cap,Color::rgb(0x515550),0.24));
            let angle=(-225.0+270.0*s.knob(id))*PI/180.0;
            let tip=(cx+angle.cos()*(radius-1.4*scale),cy+angle.sin()*(radius-1.4*scale));
            ui.c.polyline(&[(cx+angle.cos()*radius*0.25,cy+angle.sin()*radius*0.25),tip],(1.8*scale).max(0.5),Color::rgb(0xf3edd9));
            let v=s.knob(id);
            let text=match id {
                KnobId::Eq(_,_,0)=>format!("{:+.0}dB",(v*2.0-1.0)*18.0),
                KnobId::Eq(..)=>{let hz=os_dsp::mixer::eq_frequency((v*65536.0).round() as i32);if hz>=1000.0 {format!("{:.1}kHz",hz/1000.0)}else{format!("{hz:.0}Hz")}},
                KnobId::Strip(_,0)=>if v==0.0{"OUT".into()}else{format!("{:.0}Hz",os_dsp::channel_strip::cut_hz(v))},
                _=>if v==0.0{"OUT".into()}else{format!("{:.0}%",v*100.0)},
            };
            if row>=50.0 {ui.c.text_center(Font::new(8),Rect::new(cx-18.0,y+row-12.0,36.0,9.0),ui.t.tx(0xd7d4c6),&text);}
        }
    }
}
