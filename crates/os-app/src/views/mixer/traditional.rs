// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
//! Traditional stock mixer geometry, sharing the current mixer actions and audio state.
use super::*;
fn layout_index(s: &State) -> u8 {
    if s.vintage_2005 { 0 } else { s.mixer.traditional_layout }
}
pub fn width(s: &State) -> f32 {if s.vintage_2005 { return 30.0; } [40.,48.,40.,44.,64.,80.,56.,132.][layout_index(s).min(7) as usize]}
// Vertical layout at the default 1080p window (spec §7): absolute y for a window top of 83;
// bottom-anchored values are offsets from the window bottom.
const HEAD_Y0: f32 = 107.0; // header numbers row
const HEAD_Y1: f32 = 127.0;
const METER_Y0: f32 = 130.0;
const METER_Y1: f32 = 453.0;
const PANEL_Y0: f32 = 128.0; // dark head panel behind meter, name, LED, pan
const PANEL_Y1: f32 = 508.0;
const LED_Y: f32 = 467.0;
const PAN_Y: f32 = 496.5;
const SEP_Y: f32 = 527.0;
const SWAP_Y: f32 = 545.0;
const RING_Y: f32 = 563.0;
const TRACK_Y0: f32 = 525.0;
const HANDLE_0DB: f32 = 618.0; // handle centre at 0 dB (value 0.8)

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

/// How much the mixer's stretchy parts (meter, level scale, fader) are scaled at window `r`:
/// 1 at the measured height; the rest of the strip keeps its size.
pub fn scale(r: Rect) -> f32 {
    ((r.h - FIXED_H) / (REF_H - FIXED_H)).max(0.2)
}

pub fn vintage_fader_per_px(r: Rect) -> f64 {
    let (top, bottom) = if super::vintage_compact::supported(r) {
        super::vintage_compact::travel(r)
    } else {
        (Ly::new(r, true).y(TRACK_Y0), r.bottom() - 99.0)
    };
    1.0 / f64::from((bottom - top).max(1.0))
}

/// Maps the measured (absolute, window top 83) y values to the window `r`: the meter (130-453) and
/// the fader (525-981) scale, the controls between them and below them keep their size.
struct Ly {
    top: f32,
    k: f32,
    vintage: bool,
}

impl Ly {
    fn new(r: Rect, vintage: bool) -> Ly {
        Ly { top: r.y, k: scale(r), vintage }
    }

    fn y(&self, abs: f32) -> f32 {
        let (m0, m1, f0, f1) = (130.0 - REF_TOP, 453.0 - REF_TOP, 525.0 - REF_TOP, 981.0 - REF_TOP);
        let ry = abs - REF_TOP;
        let k = self.k;
        if self.vintage {
            let fader_k = k * 0.85;
            let meter_k = k + (k - fader_k) * (f1 - f0) / (m1 - m0);
            let y = if ry < m0 { ry }
                else if ry < m1 { m0 + (ry-m0)*meter_k }
                else if ry < f0 { m0 + (m1-m0)*meter_k + ry-m1 }
                else { m0 + (m1-m0)*meter_k + f0-m1 + (ry-f0)*fader_k };
            return self.top + y;
        }
        let y = if ry < m0 {
            ry
        } else if ry < m1 {
            m0 + (ry - m0) * k
        } else if ry < f0 {
            m0 + (m1 - m0) * k + (ry - m1)
        } else if ry < f1 {
            m0 + (m1 - m0) * k + (f0 - m1) + (ry - f0) * k
        } else {
            m0 + (m1 - m0) * k + (f0 - m1) + (f1 - f0) * k + (ry - f1)
        };
        self.top + y
    }
}

pub fn paint(ui: &mut Ui, s: &State, r: Rect, f: &FrameInfo) {
    if s.vintage_2005 && super::vintage_compact::supported(crate::views::frame_rects(s,Win::Mixer,r).0) {
        return super::vintage_compact::paint(ui,s,r);
    }
    let width=width(s);
    let panel_w = 221.0;
    window_body(ui, r);
    let outer = r;
    let inner = crate::views::float_frame(ui, s, Win::Mixer, r);
    let r = inner.unwrap_or(r);
    let full_title=r;
    let left=s.vintage_2005 || mx::inspector_left();
    let inspector=Rect::new(if left {r.x} else {r.right()-panel_w},r.y,panel_w,r.h);
    let r=if left {Rect::new(r.x+panel_w,r.y,r.w,r.h)} else {r};
    let client = crate::views::window_frame_ex(ui, Win::Mixer, Rect::new(r.x, r.y, r.w - panel_w, r.h), "", 0.0, false, inner.is_some());
    super::title_tools(ui, s, r);
    if r.h < 2.0 * TITLE_H {
        // Collapsed to the title bar: only the slot panel's title with the window buttons.
        s.cables.borrow_mut().clear();
        slot_panel(ui, s, inspector);
        layout_picker(ui,s,full_title);
        window_buttons(ui, Win::Mixer, full_title.right()-58.,full_title.y,ui.t.tx(0xd4dade),ui.t.tx(0xffffff));
        return;
    }
    let ly = Ly::new(r, s.vintage_2005);

    let strips = Rect::new(r.x, client.y, r.w - panel_w - 7.0, client.h);
    ui.c.fill_rect(Rect::new(r.x, client.y, r.w - panel_w, client.h), ui.t.s(0x232527));
    ui.hit(strips, Action::MixerArea, "Mixer");
    let inserts = mx::inserts(s);
    let bottom = r.bottom();

    // Header row: "C" (scale) and "M" (master) on a lighter band.
    ui.c.fill_rect(Rect::new(r.x + 1.0, ly.y(HEAD_Y0), 44.0+width, HEAD_Y1 - HEAD_Y0 + 1.0), ui.t.s(0x2b2e30));
    ui.c.text_center(Font::new(17), Rect::new(r.x + 12.0, ly.y(HEAD_Y0), 20.0, 21.0), ui.t.s(0x6d7275), "C");
    ui.c.text_center(Font::new(17), Rect::new(r.x + 44.0, ly.y(HEAD_Y0), width, 21.0), ui.t.s(0x6d7275), "M");

    // "C" column: the level scale with the current track's meter (spec: ui/mixer.md §9).
    let col = Rect::new(r.x + 9.0, ly.y(METER_Y0), 28.0, bottom - 156.0 - ly.y(METER_Y0));
    let cfg = &s.mixer.meter_cfg;
    let floor = cfg.v[meters::k::FLOOR_DB];
    // The column is the law's whole range (130-924 at the measured size) squeezed evenly into its height, so marks and
    // bars keep their proportions at any mixer height (the strips' fixed-size controls do not apply here).
    let law_h = meters::LAW_BOTTOM - meters::LAW_TOP;
    let cy = |abs: f32| col.y + (abs - meters::LAW_TOP) / law_h * col.h;
    let alt = crate::prefs::check(&crate::prefs::ALTERNATE_METER_SCALE);
    let hint = if col.contains(ui.mouse.0, ui.mouse.1) {
        // The level at the mouse (the scale is not linear in y: invert it through the law).
        let db = meters::db_at((col.bottom() - ui.mouse.1) / col.h.max(1.0), floor, alt);
        format!("Current  {db:.2}dB  {:.2}", 10f32.powf(db / 20.0))
    } else {
        "Level scale".to_string()
    };
    ui.hit(col, Action::Mix(MHit::Scale), hint);
    let lemon = crate::views::lemon(ui);
    if lemon {
        lk::outlined(ui.c, col, lk::r_btn(ui.t), ui.t.s(0x191d1e), ui.t.s(0x222527));
    } else {
        ui.c.fill_rrect(col, 3.0, ui.t.s(0x191d1e));
        ui.c.fill_rrect(col.inset(1.0), 2.5, ui.t.s(0x222527));
    }
    // Marks closer than a label height are skipped when the window is squished: "0" always shows, "+3" only with room
    // above it, the rest downward from "0". The alternate scale has no "+3".
    let scale = scale_marks(alt, floor);
    let (plus, below) = if alt { (None, &scale[..]) } else { (Some(scale[0]), &scale[1..]) };
    let zero = cy(below[0].1);
    let mut marks: Vec<(&str, f32)> = Vec::new();
    if let Some((m, y)) = plus.map(|(m, y)| (m, cy(y))).filter(|&(_, y)| zero - y >= 14.0) {
        marks.push((m, y));
    }
    marks.push((below[0].0, zero));
    let mut last = zero;
    for &(m, y) in &below[1..] {
        let y = cy(y);
        if y - last >= 14.0 {
            marks.push((m, y));
            last = y;
        }
    }
    for (m, y) in marks {
        ui.c.text_center(Font::new(12), Rect::new(col.x, y - 8.0, col.w, 15.0), ui.t.s(0x7f8a90), m);
        ui.c.fill_rect(Rect::new(col.x + 4.0, y + 7.0, col.w - 8.0, 1.0), ui.t.s(0x111315));
    }
    // Four unlabelled ticks below 45 (the alternate scale has none below its 30).
    for k in (0..4).filter(|_| !alt) {
        let y = cy(893.0 + 7.5 * k as f32);
        if y - last >= 10.0 {
            ui.c.fill_rect(Rect::new(col.x + 4.0, y, col.w - 8.0, 1.0), ui.t.s(0x111315));
        }
    }
    // The bars cover the labels (measured: two 13 px bars filling the column inside its 1 px border).
    ui.live(Live::Meter(s.selected_mixer), col);
    if let Some(m) = s.mixer.meters.get(usize::from(s.selected_mixer)) {
        let ys = |db: f32| cy(meters::law_y(db, floor, alt)).min(col.bottom() - 1.0);
        for i in 0..2 {
            meter_bar(ui, col.x + 1.0 + 13.0 * i as f32, 13.0, col.bottom() - 1.0, m.db[i], m.hold[i], cfg, &ys);
        }
    }

    // Inserts scroll; the master strip and the scale stay put.
    let area = Rect::new(r.x + 53.0+width, client.y, strips.w - 53.0-width, client.h);
    ui.c.fill_rect(Rect::new(r.x + 44.0+width, client.y, 9.0, client.h), ui.t.s(0x1a1e20));
    for k in 0..3 {
        ui.c.fill_rect(Rect::new(r.x + 48.0+width, ly.y(497.0 + 3.0 * k as f32), 1.0, 1.0), ui.t.s(0x3a3e41));
    }
    ui.c.push_clip(area);
    let first = (s.mixer_scroll / width) as usize;
    let count = (area.w / width).ceil() as usize + 2;
    for n in (first + 1..first + 1 + count).filter(|n| *n <= usize::from(inserts)) {
        let left = area.x + width * (n - 1) as f32 - s.mixer_scroll;
        strip(ui, s, n as u16, left, bottom, f, &ly);
    }
    for n in (first+1..first+1+count).filter(|n|*n<=usize::from(inserts)){
        insertion_button(ui,s,area,client,n as u16,area.x+width*(n-1) as f32-s.mixer_scroll);
    }
    // "+" after the last strip, at the fader handle's height (spec: ui/mixer.md §1).
    let px = area.x + width * f32::from(inserts) - s.mixer_scroll + 14.0;
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
    strip(ui, s, 0, r.x + 44.0, bottom, f, &ly);

    // Horizontal scroll bar under the strips.
    let sb = if s.vintage_2005 {
        Rect::new(area.x + 13.0, bottom - 9.0, area.w - 20.0, 7.0)
    } else {
        Rect::new(area.x + 13.0, bottom - 14.0, area.w - 20.0, 9.0)
    };
    crate::views::scrollbar(ui, sb, false, s.mixer_scroll, area.w, (f32::from(inserts) + 1.0) * width, crate::app::Scroll::MixerX);
    chevron(ui, area.x + 5.0, sb.cy(), false, ui.t.s(0x5d6366));
    cables(ui, s, strips, area, bottom);

    slot_panel(ui, s, inspector);
        layout_picker(ui,s,full_title);
        window_buttons(ui, Win::Mixer, full_title.right()-58.,full_title.y,ui.t.tx(0xd4dade),ui.t.tx(0xffffff));
    resize_zones(ui, s, Win::Mixer, outer);
}

/// The routing cables (spec: ui/mixer.md §5): one from the current track's ⬇ into each of its send knobs, sagging
/// and swinging (crate::cable). Sockets on strips scrolled out of view still hold their cable's end.
fn cables(ui: &mut Ui, s: &State, strips: Rect, area: Rect, bottom: f32) {
    let width=width(s);
    let socket_x = |n: u16| match n {
        0 => strips.x + 44.0 + width/2.0,
        n => area.x + width * f32::from(n - 1) - s.mixer_scroll + width/2.0,
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

fn strip(ui: &mut Ui, s: &State, n: u16, left: f32, bottom: f32, f: &FrameInfo, ly: &Ly) {
    let width=width(s);
    let t = ui.t;
    let track = s.project.mixer.iter().find(|m| m.index == n);
    let selected = s.selected_mixer == n;
    let also = !selected && mx::is_selected(s, n);
    let extra=layout_index(s)==7;
    let cx=if extra {28.} else {width/2.};
    let offset = if s.vintage_2005 { 7.0 } else { 10.0 };
    let (mx, sx) = (left + cx + offset, left + cx - offset);
    let _ = f;

    // Body and the strip divider.
    // The current strip's body is a shade lighter (measured: 0x26292b vs 0x232527).
    let body = if selected { ui.t.s(0x26292b) } else { ui.t.s(0x232527) };
    let tint=track.and_then(|m|m.color).map(crate::views::rgb);
    let body=tint.map_or(body,|c|body.mix(c,0.22));
    let lemon = crate::views::lemon(ui);
    if t.style.vintage_2005 {
        let strip = Rect::new(left, ly.y(HEAD_Y1) + 1.0, width, bottom - ly.y(HEAD_Y1) - 1.0);
        let steel = t.mixer_strip.mix(body, 0.35);
        os_ui::kit::styled_surface(ui.c, t, strip, 0.0, t.window_border, steel, steel.shade(-0.03),
            os_theme::appearance::Finish::Beveled);
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
        if t.style.vintage_2005 {
            crate::vintage::metal(ui.c, hr, t.mixer_strip.mix(body, 0.35));
            crate::vintage::mixer_digits(ui.c, hr, n);
        } else {
            ui.c.fill_rect(hr, tint.map_or(ui.t.s(0x2b2e30),|c|ui.t.s(0x2b2e30).mix(c,0.35)));
        }
        if !t.style.vintage_2005 && selected && lemon {
            lk::lit_in(ui.c, ui.t, Rect::new(hr.x + 3.0, hr.y + 2.0, hr.w - 6.0, hr.h - 4.0), lk::r_btn(t), false);
        } else if !t.style.vintage_2005 && selected {
            // Hatched header for the selected strip.
            ui.c.push_clip(hr);
            for k in 0..14 {
                let x = hr.x - 20.0 + 5.0 * k as f32;
                ui.c.line(x, hr.bottom(), x + 21.0, hr.y, 1.0, ui.t.s(0x34383a));
            }
            ui.c.pop_clip();
        }
        if !t.style.vintage_2005 {
            let c = if selected && lemon { lk::ink_on_lit(ui.t) } else if selected || hov { ui.t.tx(0x9aa1a5) } else { ui.t.s(0x6d7275) };
            ui.c.text_center(Font::new(17), Rect::new(left, ly.y(HEAD_Y0), width, 21.0), c, &n.to_string());
        }
    }

    // Dark head panel (name, LED, pan) with a rounded bottom.
    let well = Rect::new(left + 1.0, ly.y(PANEL_Y0), width - 2.0, ly.y(PANEL_Y1) - ly.y(PANEL_Y0));
    if t.style.vintage_2005 {
        os_ui::kit::bordered(ui.c, well, 0.0, t.window_border, t.mixer_strip_dark);
    } else {
        ui.c.fill_rrect(well, 9.0, t.s(0x1d1f21));
    }
    ui.c.fill_rect(Rect::new(left + 1.0, ly.y(PANEL_Y0), width - 2.0, 12.0), ui.t.s(0x1d1f21));

    // Meter slot with its tick marks; the selected strip gets a green frame.
    let alt=matches!(layout_index(s),2|6);
    let meter = Rect::new(mx - 7.5, ly.y(METER_Y0), 16.0, ly.y(METER_Y1) - ly.y(METER_Y0)-if alt {20.} else {0.});
    if lemon {
        ui.c.fill_rrect(meter, lk::r_small(t), ui.t.s(0x191d1e));
        ui.c.fill_rrect(Rect::new(meter.x + 2.0, meter.y + 2.0, 12.0, meter.h - 4.0), 2.5, ui.t.s(0x232729));
    } else {
        ui.c.fill_rrect(meter, 2.0, ui.t.s(0x191d1e));
        ui.c.fill_rect(Rect::new(meter.x + 2.0, meter.y + 2.0, 11.0, meter.h - 4.0), ui.t.s(0x232729));
    }
    if t.style.vintage_2005 && crate::workspace_visibility::shown("visible_mixer-meters") {
        for side in 0..2 {
            crate::vintage::meter_idle(ui.c, Rect::new(meter.x + 2.0 + 6.0 * side as f32, meter.y + 2.0, 5.0, meter.h - 4.0));
        }
    } else {
        for y in [181.0, 230.0, 278.0, 327.0, 375.0, 423.0] {
            ui.c.fill_rect(Rect::new(meter.x + 3.0, ly.y(y), 9.0, 1.0), ui.t.s(0x111315));
        }
    }
    // Per-track peak meter (spec: ui/mixer.md §9): two 6 px bars on the same scale as the "C" column.
    ui.live(Live::Meter(n), meter);
    if let Some(m) = s.mixer.meters.get(usize::from(n)) {
        let cfg = &s.mixer.meter_cfg;
        let (top, bot) = (meter.y + 2.0, meter.bottom() - 2.0);
        let floor = cfg.v[meters::k::FLOOR_DB];
        let alt = crate::prefs::check(&crate::prefs::ALTERNATE_METER_SCALE);
        let ys = |db: f32| bot - meters::frac(db, floor, alt) * (bot - top);
        for i in 0..2 {
            meter_bar(ui, meter.x + 2.0 + 6.0 * i as f32, 6.0, bot, m.db[i], m.hold[i], cfg, &ys);
        }
    }
    if selected {
        ui.c.stroke_rrect(Rect::new(meter.x - 1.0, meter.y - 2.0, meter.w + 2.0, meter.h + 4.0), if lemon { lk::r_btn(t) } else { 1.0 }, 2.0, t.accent);
    }

    // Name, rotated, reading upward from the bottom of the meter.
    let name = track.and_then(|m| m.name.clone()).unwrap_or_else(|| if n == 0 { "Master".into() } else { format!("Insert {n}") });
    let name_c = track
        .and_then(|m| m.color)
        .map(crate::views::rgb)
        .map_or(if selected { ui.t.tx(0xaab2b7) } else if also { ui.t.tx(0xe4e8ea) } else { ui.t.s(0x7f868a) }, |c| {
            if also { c.mix(ui.t.tx(0xffffff), 0.5) } else { c.mix(ui.t.s(0x7f868a), 0.4) }
        });
    if alt {
        let font = label_font(ui.c, Font::new(11), &name, width - 4.);
        ui.c.text_clip(font,left+2.,meter.bottom()+3.,width-4.,name_c,&name);
    } else {
        ui.c.push_clip(Rect::new(left + 2.0, ly.y(PANEL_Y0), cx-2., ly.y(METER_Y1) - ly.y(PANEL_Y0)));
        let font = label_font(ui.c, Font::new(15), &name, meter.bottom() - ly.y(PANEL_Y0) - 2.);
        ui.c.text_vertical(font, sx - 7.0, meter.bottom() - 1.0, name_c, &name);
        ui.c.pop_clip();
    }
    if extra {plugin_list(ui,s,n,Rect::new(left+57.,ly.y(PANEL_Y0)+4.,width-61.,bottom-ly.y(PANEL_Y0)-175.));}
    let kx=left+cx;
    let enabled = track.is_none_or(|m| m.enabled);
    ui.hit(Rect::new(kx - 7.0, ly.y(LED_Y) - 7.0, 14.0, 14.0), Action::Mix(MHit::Led(n)), "Mute / solo");
    led(ui, kx, ly.y(LED_Y), enabled);
    let lock = if kx + 22. <= left + width {
        Rect::new(kx + 8., ly.y(LED_Y) - 7., 14., 14.)
    } else {
        Rect::new(kx - 7., ly.y(LED_Y) - 23., 14., 14.)
    };
    channel_lock(ui, s, n, lock);
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
    let track_y1 = bottom - 99.0;
    if lemon {
        ui.c.fill_rrect(Rect::new(mx - 2.0, ly.y(TRACK_Y0), 4.0, track_y1 - ly.y(TRACK_Y0)), 2.0, ui.t.s(0x141618));
    } else {
        ui.c.fill_rect(Rect::new(mx - 1.5, ly.y(TRACK_Y0), 3.0, track_y1 - ly.y(TRACK_Y0)), ui.t.s(0x141618));
    }
    let v = track.and_then(|m| m.volume).unwrap_or(0.8) as f32;
    // max/min, not clamp: a very short window gives track_y1 < ly.y(TRACK_Y0) (clamp would panic).
    let position = |value: f32| track_y1 - value.clamp(0.0, 1.0) * (track_y1 - top).max(1.0);
    let hy = if t.style.vintage_2005 { position(v) }
        else { (ly.y(HANDLE_0DB + (0.8 - v) * 440.0)).min(track_y1 - 5.0).max(top + 5.0) };
    if t.style.vintage_2005 {
        crate::vintage::fader_unity(ui.c, mx, position(0.8), 6.0);
    } else {
        fader_marks(ui, mx, track_y1, ly);
    }
    if selected {
        if lemon {
            ui.c.fill_rrect(Rect::new(mx - 1.5, hy, 3.0, track_y1 - hy), 1.5, t.accent);
        } else {
            ui.c.fill_rect(Rect::new(mx - 1.0, hy, 2.0, track_y1 - hy), ui.t.a(0x8ec63a));
        }
    }
    let hov = ui.hit(Rect::new(mx - 9.0, ly.y(TRACK_Y0) - 5.0, 18.0, track_y1 - ly.y(TRACK_Y0) + 10.0), Action::Fader(n), mx::level_text("Volume", f64::from(v)));
    fader_handle(ui, mx, hy, selected, hov);

    let master=n==0;
    let by=bottom;
    if matches!(layout_index(s),1|4|5|7) {
        let readout=Rect::new(left+2.,by-103.,if extra {52.} else {width-4.},18.);
        let text=fader_db_text(f64::from(v));
        let mut font=Font::new(10);
        for size in (8..=10).rev() {
            font=Font::new(size);
            if ui.c.text_width(font,&text)<=readout.w-2. {break;}
        }
        ui.c.fill_rect(readout,body);
        ui.c.push_clip(readout);
        ui.c.text_center(font,readout,ui.t.text,&text);
        ui.c.pop_clip();
    }
    let fx=Rect::new(left+cx-12.,by-79.,24.,16.);
    let on=!s.mixer.all_fx_bypassed && track.is_none_or(|m|m.slots_enabled);
    ui.hit(fx,Action::Mix(MHit::Effects(n)),"Enable / bypass track effects");
    if t.style.vintage_2005 {
        os_ui::kit::styled_surface(ui.c, t, fx, 0.0, t.window_border, t.button, t.button,
            os_theme::appearance::Finish::Beveled);
        ui.c.text_center(Font::new(10), fx, if on { t.accent } else { t.text_dim }, "FX");
    } else {
        ui.c.fill_rrect(fx,2.,if on {ui.t.accent} else {ui.t.button_hi});
        ui.c.text_center(Font::new(10),fx,if on {Color::rgb(0x000000)} else {ui.t.text},"FX");
    }
    if !master {
        let rec=Rect::new(sx-7.,by-122.,14.,14.);
        ui.hit(rec,Action::Mix(MHit::Disk(n)),"Arm / disarm disk recording");
        ui.c.fill_circle(rec.cx(),rec.cy(),4.,if s.mixer.recording.armed.contains(&n){ui.t.h(0xff6868)}else{ui.t.text_dim});
    }
    // Routing (spec: ui/mixer.md §5): the current track's green ⬇ (its cables are drawn over all strips: `cables`),
    // a send knob where the current track sends, else the ▲ "route to this track" button; an orange "^" marks tracks that receive a route.
    let kx=left+width/2.;
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
    if selected||also {ui.c.stroke_rrect(Rect::new(left+0.75,ly.y(HEAD_Y0)+0.75,width-1.5,bottom-ly.y(HEAD_Y0)-17.5),0.,1.,ui.t.accent);}
    if mx::has_separator(s,n) {
        let r = Rect::new(left, ly.y(HEAD_Y0), 4.0, bottom - ly.y(HEAD_Y0) - 16.0);
        if t.style.vintage_2005 { crate::vintage::mixer_divider(ui.c, r); }
        else { ui.c.fill_rect(r, ui.t.text_dim); }
    }
}

fn fader_marks(ui: &mut Ui, mx: f32, track_y1: f32, ly: &Ly) {
    let marks = [0.0, -6.0, -12.0, -18.0]
        .map(|db| ly.y(HANDLE_0DB + (0.8 - fader_value(db)) * 440.0).floor());
    let ys = &marks[..marks.iter().take_while(|&&y| y < track_y1 - 1.0).count()];
    let (tick, rail) = (ui.t.s(0x181a1c), ui.t.s(0x202324));
    for w in ys.windows(2) {
        let (a, b) = (w[0] + 2.0, w[1] - 1.0);
        if b > a {
            ui.c.fill_rect(Rect::new(mx - 7.5, a, 4.0, b - a), rail);
            ui.c.fill_rect(Rect::new(mx + 3.5, a, 4.0, b - a), rail);
        }
    }
    for &y in ys {
        ui.c.fill_rect(Rect::new(mx - 9.5, y, 6.0, 1.0), tick);
        ui.c.fill_rect(Rect::new(mx + 3.5, y, 6.0, 1.0), tick);
    }
}


fn layout_picker(ui:&mut Ui,s:&State,r:Rect) {
    if s.vintage_2005 { return; }
    let b=Rect::new(r.right()-208.,r.y+2.,112.,20.);
    let hover=ui.hit(b,Action::Mix(MHit::Layout),"Traditional mixer layout");
    ui.c.fill_rrect(b,3.,if hover {ui.t.button_hi} else {ui.t.sunken});
    ui.c.text_clip(Font::new(13),b.x+6.,b.y+2.,b.w-22.,ui.t.text,mx::TRADITIONAL_LAYOUTS[layout_index(s) as usize]);
    ui.c.fill_polygon(&[(b.right()-13.,b.y+8.),(b.right()-5.,b.y+8.),(b.right()-9.,b.y+12.)],ui.t.text);
}
fn plugin_list(ui:&mut Ui,s:&State,n:u16,r:Rect) {
    let rows=((r.h/19.).floor() as usize).min(10);
    for i in 0..rows {
        let k=mx::visible_user_slot(s,n,i);
        let b=Rect::new(r.x,r.y+i as f32*19.,r.w,18.);
        let name=mx::track(s,n).and_then(|t|t.slots[k as usize].as_ref()).map(crate::project_plugins::slot_title).unwrap_or_else(||format!("Slot {}",i+1));
        ui.c.fill_rrect(b,2.,ui.t.sunken);
        let font = label_font(ui.c, Font::new(10), &name, b.w - 6.);
        ui.c.text_clip(font,b.x+3.,b.y+3.,b.w-6.,ui.t.text,&name);
        ui.hit(b,Action::Mix(MHit::TrackSlot(n,k)),name);
    }
}
