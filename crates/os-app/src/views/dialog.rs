// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Message boxes, the file dialog (`dialog`, `filedlg`) and Manage shortcuts (`shortcuts`), painted over everything
//! with a veil under them. Classic draws FL's message form as measured; Lemon draws the same layout in the Lemon
//! design's rounded parts.
// spec: ui/dialogs.md §3, §4; ui/settings-choices.md "Input: Manage shortcuts (2026-10-16)"

use os_theme::design::lemon as brand;
use os_ui::kit::{self, lemon as lk};
use os_ui::{tabler, Color, Font, Rect};

use super::lemon;
use crate::app::{Action, State, Ui};
use crate::dialog::{Body, DHit, Field, Kind, Modal, Msg};
use crate::filedlg::{self as fd, FileDlg, Focus, Mode, PlaceKind, SortBy, HEAD_H, ROW_H};
use crate::icons::{self, Icon};
use crate::timesig::SigForm;

/// The frame's outer border, its colour (caption and button band), the body panel and its edge lines (spec §1a).
const BORDER: u32 = 0x16191a;
const FRAME: u32 = 0x202324;
const PANEL: u32 = 0x282c2d;
const PANEL_LINE: u32 = 0x1a1d1e;
/// Fields and lists, their outline.
const WELL: u32 = 0x1b1e1f;
const WELL_LINE: u32 = 0x141617;
/// The focus border of a field (FL's Save as form, spec §2) and a text selection.
const FOCUS_BLUE: Color = Color::rgb(0x1367b8);
const SEL_BLUE: Color = Color::rgb(0x0080ff);

/// Caption height (below it the body starts at 28).
const CAP_H: f32 = 26.0;
/// Message box: where the text starts, the icon's centre, the line pitch, width limits, button size.
const TEXT_X: f32 = 96.0;
const TEXT_R: f32 = 17.0;
const ICON_X: f32 = 47.0;
const LINE: f32 = 22.0;
const MIN_W: f32 = 260.0;
const MAX_W: f32 = 560.0;
pub(super) const BTN_W: f32 = 64.0;
pub(super) const BTN_H: f32 = 31.0;

/// Every open dialog, bottom to top, each over a veil that covers the window (and what is under it).
pub fn paint(ui: &mut Ui, s: &State) {
    let full = Rect::new(0.0, 0.0, s.screen.0, s.screen.1);
    for m in &s.dialogs {
        ui.c.fill_rect(full, Color::rgb(0).with_alpha(80));
        ui.hit(full, Action::Swallow, "");
        match &m.body {
            Body::ClipFx(channel,page)=>{let r=place(s,m,510.,386.);frame(ui,r,"Clip Effects");crate::clip_effects::paint(ui,s,r,*channel,*page);},
            Body::Warp(f)=>{let r=place(s,m,(s.screen.0-24.).clamp(600.,1000.),430.);frame(ui,r,"Audio warp markers");crate::warp::paint(ui,r,f);},
            Body::Ai => ai_box(ui,s,m),
            Body::RoutePicker(f) => route_picker_box(ui,s,m,f),
            Body::Headphones(f) => headphones_box(ui,s,m,f),
            Body::Msg(msg) => msg_box(ui, s, m, msg),
            Body::File(f) => file_box(ui, s, m, f),
            Body::Shortcuts(f) => shortcuts_box(ui, s, m, f),
            Body::TimeSig(f) => sig_box(ui, s, m, f),
            Body::Project(f) => super::project_forms::paint(ui, s, m, f),
        }
    }
}

pub(super) fn hit(ui: &mut Ui, r: Rect, h: DHit, hint: &'static str) -> bool {
    ui.hit(r, Action::Dlg(h), hint)
}

/// Where a dialog of `w` x `h` sits: where its caption was dragged, else centred; inside the window either way.
pub(super) fn place(s: &State, m: &Modal, w: f32, h: f32) -> Rect {
    let (sw, sh) = s.screen;
    let (x, y) = m.at.unwrap_or(((sw - w) / 2.0, (sh - h) / 2.0));
    let r = Rect::new(x.min(sw - w).max(0.0).round(), y.min(sh - h).max(0.0).round(), w, h);
    m.rect.set(r);
    r
}

/// Border, caption (title, ×) and the frame colour under everything else.
pub(super) fn frame(ui: &mut Ui, r: Rect, title: &str) {
    let t = ui.t;
    let lemon = lemon(ui);
    hit(ui, r, DHit::Body, "");
    if lemon {
        ui.c.fill_rrect(r.outset(4.0), lk::r_panel(t) + 4.0, Color::rgb(0).with_alpha(60));
        lk::outlined(ui.c, r, lk::r_panel(t), t.s(BORDER), t.s(FRAME));
        ui.c.fill_rect(Rect::new(r.x + lk::r_panel(t), r.y + CAP_H, r.w - 2.0 * lk::r_panel(t), 1.0), lk::hairline(t.s(FRAME)));
    } else {
        ui.c.fill_rect(r, t.s(BORDER));
        ui.c.fill_rect(r.inset(1.0), t.s(FRAME));
    }
    let f = Font::new(13);
    let lh = ui.c.line_height(f);
    ui.c.text_clip(f, r.x + if lemon { 11.0 } else { 7.0 }, (r.y + 1.0 + (CAP_H - lh) / 2.0).round(), r.w - 40.0, t.text, title);
    hit(ui, Rect::new(r.x, r.y, r.w - 30.0, CAP_H), DHit::Caption, "Drag to move");
    let x = Rect::new(r.right() - 26.0, r.y + 3.0, 21.0, 21.0);
    let hov = hit(ui, x, DHit::Close, "Close (Esc)");
    if hov && lemon {
        lk::window_button_hover(ui.c, t, x, true);
    }
    ui.c.icon(&tabler::X, x.inset(5.0), if hov { t.glyph } else { t.text_dim });
}

/// A body panel: square with darker lines along its top and bottom (Classic), an outlined rounded panel (Lemon).
pub(super) fn panel(ui: &mut Ui, r: Rect) {
    let t = ui.t;
    if lemon(ui) {
        lk::outlined(ui.c, r, lk::r_panel(t) - 2.0, t.s(PANEL_LINE), t.s(PANEL));
    } else {
        ui.c.fill_rect(r, t.s(PANEL));
        ui.c.fill_rect(Rect::new(r.x, r.y, r.w, 1.0), t.s(PANEL_LINE));
        ui.c.fill_rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), t.s(PANEL_LINE));
    }
}

// ---------------------------------------------------------------- message box

fn msg_box(ui: &mut Ui, s: &State, m: &Modal, msg: &Msg) {
    let t = ui.t;
    let lemon = lemon(ui);
    let font = Font::new(14);
    let lines = wrap(ui, font, &msg.text, MAX_W - TEXT_X - TEXT_R);
    let text_w = lines.iter().map(|l| line_width(ui, font, l)).fold(0.0, f32::max);
    let bf = Font::new(13);
    let widths: Vec<f32> = (0..msg.buttons.len()).map(|i| (ui.c.text_width(bf, msg.label(i)) + 28.0).max(BTN_W).round()).collect();
    let buttons_w = widths.iter().sum::<f32>() + 6.0 * widths.len().saturating_sub(1) as f32;
    // Remember my choice sits left of the buttons (spec: ui/settings-choices.md "Hints and warnings: Manage messages
    // and Project data size warning (2026-10-16)").
    let remember_w = if msg.remember.is_some() { super::remember_width(ui) + 24.0 } else { 0.0 };
    let w = (text_w + TEXT_X + TEXT_R).max(buttons_w + 24.0 + remember_w).max(MIN_W).ceil();
    let choices = if let crate::dialog::Then::StemChoose(_,mask)=msg.then {Some(mask)} else {None};
    let body_h = (27.0 + lines.len() as f32 * LINE).max(91.0) + if choices.is_some() {150.0} else {0.0};
    let r = place(s, m, w, 28.0 + body_h + 63.0);
    frame(ui, r, &msg.title);
    let body = Rect::new(r.x + 5.0, r.y + 28.0, r.w - 10.0, body_h);
    panel(ui, body);
    icon(ui, msg.kind, r.x + ICON_X, body.y + 45.0);
    for (k, line) in lines.iter().enumerate() {
        let (mut x, y) = (r.x + TEXT_X, body.y + 13.0 + k as f32 * LINE);
        for (run, hi) in line {
            let col = match (*hi, lemon) {
                (true, true) => t.brand_lit(brand::MAIN),
                (true, false) => t.h(0xff9e00),
                _ => t.text,
            };
            x += ui.c.text(font, x, y, col, run);
        }
    }
    if let Some(mask)=choices {
        for (n,label) in ["Vocals", "Drums", "Bass", "Instruments", "All stems"].iter().enumerate() {
            let y=body.bottom()-145.0+n as f32*28.0;
            let b=Rect::new(r.x+TEXT_X,y+4.0,15.0,15.0);
            let on=if n==4 {mask==15} else {mask & (1<<n)!=0};
            let hov=hit(ui,Rect::new(b.x-4.0,y,240.0,24.0),DHit::StemToggle(n),"Choose stems (1-4; A for all)");
            ui.c.fill_rrect(b,2.0,t.s(0x141516));
            ui.c.stroke_rrect(b,2.0,1.0,if hov {t.glyph_dim} else {t.text_dim});
            if on {kit::tick(ui.c,b,t.glyph);}
            ui.c.text(font,b.x+24.0,y+2.0,t.text,label);
        }
    }
    let by = body.bottom() + 20.0;
    if let Some((_, ticked)) = msg.remember {
        super::remember_check(ui, r.x + 14.0, by + BTN_H / 2.0, ticked, Action::Dlg(DHit::Remember));
    }
    let mut bx = r.right() - 12.0 - buttons_w;
    for (i, bw) in widths.into_iter().enumerate() {
        let b = Rect::new(bx, by, bw, BTN_H);
        let act = Action::Dlg(DHit::Button(i));
        let down = ui.down(b, &act);
        let enabled=!(choices==Some(0) && msg.buttons[i]==crate::dialog::Answer::Ok);
        let hov = enabled && ui.hit(b, act, msg.label(i));
        button(ui, b, msg.label(i), i == msg.focus, hov, down, enabled);
        bx += bw + 6.0;
    }
}

/// Breaks the text runs into lines at most `max_w` wide (at spaces; a longer word anywhere) and at "\n".
fn wrap(ui: &mut Ui, font: Font, text: &[(String, bool)], max_w: f32) -> Vec<Vec<(String, bool)>> {
    fn push(lines: &mut [Vec<(String, bool)>], s: &str, hi: bool) {
        let line = lines.last_mut().expect("a line");
        match line.last_mut() {
            Some((t, h)) if *h == hi => t.push_str(s),
            _ => line.push((s.to_string(), hi)),
        }
    }
    let mut lines = vec![Vec::new()];
    let mut w = 0.0;
    for (run, hi) in text {
        for (k, para) in run.split('\n').enumerate() {
            if k > 0 {
                lines.push(Vec::new());
                w = 0.0;
            }
            for word in para.split_inclusive(' ') {
                let ww = ui.c.text_width(font, word.trim_end());
                if w > 0.0 && w + ww > max_w {
                    lines.push(Vec::new());
                    w = 0.0;
                }
                let word = if w == 0.0 { word.trim_start() } else { word };
                if ww > max_w {
                    for ch in word.chars() {
                        let s = ch.to_string();
                        let cw = ui.c.text_width(font, &s);
                        if w > 0.0 && w + cw > max_w {
                            lines.push(Vec::new());
                            w = 0.0;
                        }
                        push(&mut lines, &s, *hi);
                        w += cw;
                    }
                } else if !word.is_empty() {
                    push(&mut lines, word, *hi);
                    w += ui.c.text_width(font, word);
                }
            }
        }
    }
    lines
}

fn line_width(ui: &mut Ui, font: Font, line: &[(String, bool)]) -> f32 {
    let n = line.len();
    line.iter().enumerate().map(|(i, (s, _))| ui.c.text_width(font, if i + 1 == n { s.trim_end() } else { s })).sum()
}

/// The icon in the body's left part: question, exclamation and the discs with a knocked-out "!" or "i". Classic draws
/// it light grey on the panel; Lemon on a rounded tile (spec §1b, §3).
fn icon(ui: &mut Ui, kind: Kind, cx: f32, cy: f32) {
    let t = ui.t;
    let (glyph, back, k) = if lemon(ui) {
        let tile_c = if kind == Kind::Error { t.record.mix(Color::rgb(0x161616), 0.78) } else { t.brand_lit(brand::COLORED_DARK) };
        ui.c.fill_rrect(Rect::new(cx - 28.0, cy - 28.0, 56.0, 56.0), lk::r_btn(t) + 4.0, tile_c);
        (if kind == Kind::Error { t.record } else { t.brand_lit(brand::MAIN) }, tile_c, 0.72)
    } else {
        (t.tx(0x98a2a8), t.s(PANEL), 1.0)
    };
    let c = &mut *ui.c;
    match kind {
        Kind::Question => {
            // A hook from the left over the top, down to the stem, and the dot.
            let (w, r, oy) = (7.0 * k, 10.0 * k, cy - 14.0 * k);
            c.arc(cx, oy, r, std::f32::consts::PI, 1.25 * std::f32::consts::PI, w, glyph);
            let end = (cx + r * 0.7071, oy + r * 0.7071);
            c.fill_circle(end.0, end.1, w / 2.0, glyph);
            c.line(end.0, end.1, cx, cy + 0.5 * k, w, glyph);
            c.fill_rrect(Rect::new(cx - w / 2.0, cy - 1.5 * k, w, 9.0 * k), w / 2.0, glyph);
            c.fill_circle(cx, cy + 19.0 * k, 4.5 * k, glyph);
        }
        Kind::Warning => {
            c.fill_rrect(Rect::new(cx - 4.0 * k, cy - 27.0 * k, 8.0 * k, 35.0 * k), 4.0 * k, glyph);
            c.fill_circle(cx, cy + 19.0 * k, 5.0 * k, glyph);
        }
        Kind::Error => {
            c.fill_circle(cx, cy, 27.0 * k, glyph);
            c.fill_rrect(Rect::new(cx - 3.5 * k, cy - 17.0 * k, 7.0 * k, 22.0 * k), 3.5 * k, back);
            c.fill_circle(cx, cy + 12.0 * k, 4.0 * k, back);
        }
        Kind::Info => {
            c.fill_circle(cx, cy, 27.0 * k, glyph);
            c.fill_circle(cx, cy - 13.0 * k, 4.0 * k, back);
            c.fill_rrect(Rect::new(cx - 3.5 * k, cy - 5.0 * k, 7.0 * k, 22.0 * k), 3.5 * k, back);
        }
    }
}

/// A dialog button (spec §1d, §3). `main`: the focused (message box) or default (file dialog) button. `accel`: the first
/// letter gets the dotted underline.
pub(super) fn button(ui: &mut Ui, b: Rect, label: &str, main: bool, hov: bool, down: bool, accel: bool) {
    let t = ui.t;
    let ink = if lemon(ui) {
        let rad = lk::r_btn(t);
        if main {
            lk::lit_in(ui.c, ui.t, b, rad, hov);
            lk::ink_on_lit(ui.t)
        } else {
            lk::outlined(ui.c, b, rad, t.s(0x161819), lk::face(t.s(0x2a2d2f), kit::State::new(hov, down, false)));
            if hov { t.glyph } else { t.text }
        }
    } else {
        ui.c.fill_rrect(b, 10.0, t.s(0x181b1c));
        let inner = b.inset(1.0);
        if main {
            let (top, bottom) = if down { (t.s(0x202425), t.s(0x232728)) } else if hov { (t.s(0x2f3334), t.s(0x26292a)) } else { (t.s(0x2a2e2f), t.s(0x232728)) };
            ui.c.fill_rect_gradient(inner, top, bottom, 9.0);
            ui.c.fill_rect(Rect::new(inner.x + 8.0, inner.y, inner.w - 16.0, 1.0), t.s(0x343839));
        } else {
            ui.c.fill_rrect(inner, 9.0, t.s(if down { 0x1a1d1e } else if hov { 0x232627 } else { 0x1d2021 }));
        }
        if hov { t.glyph } else { t.text }
    };
    let f = Font::new(13);
    let tw = ui.c.text_width(f, label);
    let lh = ui.c.line_height(f);
    let (x, y) = ((b.x + (b.w - tw) / 2.0).round(), (b.cy() - lh / 2.0).round());
    ui.c.text(f, x, y, ink, label);
    if accel {
        let first: String = label.chars().take(1).collect();
        let w = ui.c.text_width(f, &first);
        let base = (y + ui.c.ascent(f) + 2.0).round();
        let mut dx = 0.0;
        while dx < w - 0.5 {
            ui.c.fill_rect(Rect::new(x + dx, base, 1.0, 1.0), ink);
            dx += 2.0;
        }
    }
}

// ---------------------------------------------------------------- time signature dialog

/// PATTERNS > Set time signature... (spec: ui/pattern-time-signature.md §1): Numerator and Denominator value boxes, whose
/// lists are the app's popups (`App::sig_list`), and Accept, in FL's 156 x 143 (wider when the labels need it).
fn sig_box(ui: &mut Ui, s: &State, m: &Modal, f: &SigForm) {
    let t = ui.t;
    let font = Font::new(13);
    let labels = ["Numerator", "Denominator"];
    let label_w = labels.iter().map(|l| ui.c.text_width(font, l)).fold(0.0, f32::max).ceil();
    let box_w = 48.0;
    let r = place(s, m, (12.0 + label_w + 10.0 + box_w + 12.0).max(156.0), 143.0);
    frame(ui, r, "Time signature change");
    panel(ui, Rect::new(r.x + 5.0, r.y + 28.0, r.w - 10.0, 72.0));
    let lh = ui.c.line_height(font);
    let open = crate::timesig::list_open(s);
    for (i, (label, value)) in labels.into_iter().zip([f.num, f.den]).enumerate() {
        let den = i == 1;
        let cy = r.y + if den { 75.0 } else { 49.0 };
        ui.c.text(font, r.x + 12.0, (cy - lh / 2.0).round(), t.text, label);
        let b = Rect::new(r.right() - 12.0 - box_w, (cy - 10.0).round(), box_w, 20.0);
        let hov = hit(ui, b, DHit::SigValue(den), if den { "Denominator: click to choose" } else { "Numerator: click to choose" });
        kit::combo(ui.c, t, b, &value.to_string(), font, kit::State::new(hov, open == Some(den), false));
    }
    let bw = (ui.c.text_width(font, "Accept") + 28.0).max(BTN_W).round();
    let b = Rect::new(r.right() - 12.0 - bw, r.y + 105.0, bw, BTN_H);
    let act = Action::Dlg(DHit::Accept);
    let down = ui.down(b, &act);
    let hov = ui.hit(b, act, "Enter");
    button(ui, b, "Accept", true, hov, down, false);
}

// ---------------------------------------------------------------- file dialog

/// Where the file dialog's parts sit (spec §4a).
struct Layout {
    panel: Rect,
    back: Rect,
    up: Rect,
    new_folder: Option<Rect>,
    path: Rect,
    places: Rect,
    list: Rect,
    name_y: f32,
    name: Rect,
    combo: Option<Rect>,
}

fn layout(r: Rect, f: &FileDlg) -> Layout {
    let panel = Rect::new(r.x + 8.0, r.y + 32.0, r.w - 16.0, r.h - 32.0 - 44.0);
    let ty = panel.y + 30.0;
    let back = Rect::new(panel.x + 8.0, ty, 26.0, 26.0);
    let up = Rect::new(back.right() + 4.0, ty, 26.0, 26.0);
    let new_folder = (f.mode != Mode::Open).then(|| Rect::new(panel.right() - 34.0, ty, 26.0, 26.0));
    let path = Rect::span(up.right() + 6.0, new_folder.map_or(panel.right() - 8.0, |n| n.x - 4.0), ty, ty + 26.0);
    let name_y = panel.bottom() - 34.0;
    let combo = (f.mode != Mode::Folder && !f.filters.is_empty()).then(|| Rect::new(panel.right() - 238.0, name_y, 230.0, 26.0));
    let name = Rect::span(panel.x + 84.0, combo.map_or(panel.right() - 8.0, |c| c.x - 8.0), name_y, name_y + 26.0);
    let (y0, y1) = (ty + 34.0, name_y - 8.0);
    let places = Rect::span(panel.x + 8.0, panel.x + 158.0, y0, y1);
    let list = Rect::span(places.right() + 8.0, panel.right() - 8.0, y0, y1);
    Layout { panel, back, up, new_folder, path, places, list, name_y, name, combo }
}

fn file_box(ui: &mut Ui, s: &State, m: &Modal, f: &FileDlg) {
    let t = ui.t;
    let (sw, sh) = s.screen;
    let w = 760.0_f32.min(sw - 80.0).max(560.0_f32.min(sw));
    let h = 500.0_f32.min(sh - 80.0).max(360.0_f32.min(sh));
    let r = place(s, m, w, h);
    frame(ui, r, &f.title);
    let l = layout(r, f);
    section(ui, l.panel, "Location");
    tool(ui, l.back, Icon::Back, DHit::Back, "Back (Alt+Left)", f.can_go_back());
    tool(ui, l.up, Icon::ArrowUp, DHit::Up, "Up to the folder above (Alt+Up)", f.can_go_up());
    if let Some(n) = l.new_folder {
        tool(ui, n, Icon::FolderPlus, DHit::NewFolder, "New folder", true);
    }
    field(ui, l.path, &f.path, f.focus == Focus::Path, DHit::PathField, "The folder shown: type a path and press Enter to go there");
    places(ui, f, l.places);
    list(ui, f, l.list);
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    let label = if f.mode == Mode::Folder { "Folder" } else { "File name" };
    ui.c.text(font, l.panel.x + 10.0, (l.name_y + 13.0 - lh / 2.0).round(), t.text, label);
    field(ui, l.name, &f.name, f.focus == Focus::Name, DHit::NameField, "");
    if let Some(c) = l.combo {
        let hov = hit(ui, c, DHit::Filter, "The type of file listed");
        let label = f.chosen().map(fd::Filter::label).unwrap_or_default();
        kit::combo(ui.c, t, c, &label, font, kit::State::new(hov, f.filter_open, false));
    }
    // The band: the status, then the main button and Cancel.
    let main = match f.mode {
        Mode::Open => "Open",
        Mode::Save => "Save",
        Mode::Folder => "Select folder",
    };
    let bw = |ui: &mut Ui, s: &str| (ui.c.text_width(font, s) + 28.0).max(84.0).round();
    let (mw, cw) = (bw(ui, main), bw(ui, "Cancel"));
    let cancel = Rect::new(r.right() - 12.0 - cw, r.bottom() - 37.0, cw, 30.0);
    let ok = Rect::new(cancel.x - 6.0 - mw, cancel.y, mw, 30.0);
    let col = if f.status.is_some() { t.h(0xff9e00) } else { t.text_dim };
    if matches!(f.then,crate::dialog::Then::RackSample(_)) {
        let b=Rect::new(r.x+14.,cancel.y,116.,cancel.h);
        let hov=hit(ui,b,DHit::SamplePreview,"Audition the selected sample without loading it into the channel");
        button(ui,b,if f.preview {"Preview: On"}else{"Preview: Off"},f.preview,hov,false,false);
    }else{
    ui.c.text_clip(font, r.x + 14.0, (cancel.cy() - lh / 2.0).round(), ok.x - r.x - 26.0, col, &f.status_text());
    }
    for (b, label, h, is_main) in [(ok, main, DHit::Accept, true), (cancel, "Cancel", DHit::Cancel, false)] {
        let act = Action::Dlg(h);
        let down = ui.down(b, &act);
        let hov = ui.hit(b, act, if is_main { "Enter" } else { "Esc" });
        button(ui, b, label, is_main, hov, down, false);
    }
    if let (Some(c), true) = (l.combo, f.filter_open) {
        filter_list(ui, f, c, r);
    }
}

// ---------------------------------------------------------------- Manage shortcuts

/// Manage shortcuts: the window's width, a row's pitch, a box's size, a list entry's height.
const SC_W: f32 = 380.0;
const SC_ROW: f32 = 27.0;
const SC_BOX: (f32, f32) = (116.0, 21.0);
const SC_ITEM_H: f32 = 22.0;

/// The height of a Manage shortcuts group of `n` rows: its header, its rows.
fn sc_group_h(n: usize) -> f32 {
    30.0 + n as f32 * SC_ROW + 4.0
}

/// Manage shortcuts: a group per section, a combo per row with the row's label beside it, then Reset all and Apply; the
/// open list over everything.
fn shortcuts_box(ui: &mut Ui, s: &State, m: &Modal, f: &crate::shortcuts::Form) {
    use crate::shortcuts::{GROUPS, ROWS, TITLE};
    let t = ui.t;
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    let groups_h: f32 = GROUPS.iter().map(|g| sc_group_h(g.1) + 6.0).sum();
    let r = place(s, m, SC_W+340., 28.0 + 4.0 + groups_h + 47.0);
    frame(ui, r, TITLE);
    let mut y = r.y + 32.0;
    let mut row = 0;
    let mut open_box = None;
    for (title, n) in GROUPS {
        let p = Rect::new(r.x + 8.0, y, SC_W - 16.0, sc_group_h(n));
        section(ui, p, title);
        for k in 0..n {
            let b = Rect::new(p.x + 10.0, p.y + 30.0 + k as f32 * SC_ROW, SC_BOX.0, SC_BOX.1);
            let open = f.open == Some(row);
            let hov = hit(ui, b, DHit::Shortcut(row), ROWS[row].label);
            kit::combo(ui.c, t, b, f.text(row), font, kit::State::new(hov, open, false));
            ui.c.text(font, b.right() + 10.0, (b.cy() - lh / 2.0).round(), t.text, ROWS[row].label);
            if open {
                open_box = Some((row, b));
            }
            row += 1;
        }
        y = p.bottom() + 6.0;
    }
    let commands=crate::shortcuts::COMMANDS;
    let p=Rect::new(r.x+SC_W,r.y+32.,328.,groups_h-6.);
    section(ui,p,"Common commands — click a key to change it");
    let start=f.page*crate::shortcuts::COMMAND_PAGE;
    for (i,(_,label,_)) in commands.iter().enumerate().skip(start).take(crate::shortcuts::COMMAND_PAGE) {
        let row=Rect::new(p.x+10.,p.y+32.+(i-start) as f32*32.,p.w-20.,29.);
        ui.c.text(font,row.x,row.y+7.,t.text,label);
        let b=Rect::new(row.right()-132.,row.y,132.,row.h);
        let hov=hit(ui,b,DHit::CommandShortcut(i),"Press a new shortcut; Backspace clears it");
        let text=if f.capture==Some(i){"Press key…"}else if f.bindings[i].is_empty(){"Unassigned"}else{&f.bindings[i]};
        button(ui,b,text,f.capture==Some(i),hov,false,false);
    }
    let page=Rect::new(p.x+10.,p.bottom()-65.,120.,25.);
    let hov=hit(ui,page,DHit::CommandPage,"Next page of commands");
    button(ui,page,&format!("Page {} / {}",f.page+1,commands.len().div_ceil(crate::shortcuts::COMMAND_PAGE)),false,hov,false,false);
    ui.c.text_clip(Font::new(11),p.x+10.,p.bottom()-38.,p.w-20.,t.text,&f.notice);
    let bw = |ui: &mut Ui, s: &str| (ui.c.text_width(font, s) + 28.0).max(BTN_W).round();
    let (rw, aw) = (bw(ui, "Reset all"), bw(ui, "Apply"));
    let by = r.bottom() - 8.0 - BTN_H;
    let reset = Rect::new(r.x + 12.0, by, rw, BTN_H);
    let apply = Rect::new(r.right() - 12.0 - aw, by, aw, BTN_H);
    for (b, label, h, main, hint) in [
        (reset, "Reset all", DHit::ResetAll, false, "Put the default shortcuts in the form (Apply keeps them)"),
        (apply, "Apply", DHit::Apply, true, "Apply the shortcuts and close"),
    ] {
        let act = Action::Dlg(h);
        let down = ui.down(b, &act);
        let hov = ui.hit(b, act, hint);
        button(ui, b, label, main, hov, down, false);
    }
    if let Some((row, b)) = open_box {
        shortcut_list(ui, f, row, b, r);
    }
}

/// The open list of a Manage shortcuts box: its choices under the box (above it when there is no room), the chosen one
/// marked.
fn shortcut_list(ui: &mut Ui, f: &crate::shortcuts::Form, row: usize, b: Rect, dlg: Rect) {
    let t = ui.t;
    let choices = crate::shortcuts::ROWS[row].choices;
    let h = choices.len() as f32 * SC_ITEM_H + 6.0;
    let below = Rect::new(b.x, b.bottom() + 2.0, b.w, h);
    let p = if below.bottom() > dlg.bottom() - 4.0 { Rect::new(b.x, b.y - 2.0 - h, b.w, h) } else { below };
    if lemon(ui) {
        lk::popup(ui.c, t, p);
    } else {
        ui.c.fill_rect(p, t.menu_pop_border);
        ui.c.fill_rect(p.inset(1.0), t.menu_pop);
    }
    hit(ui, p, DHit::Body, "");
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    for (i, ch) in choices.iter().enumerate() {
        let rr = Rect::new(p.x + 1.0, p.y + 3.0 + i as f32 * SC_ITEM_H, p.w - 2.0, SC_ITEM_H);
        let hov = hit(ui, rr, DHit::ShortcutChoice(i), "");
        if hov {
            if lemon(ui) {
                lk::row_highlight(ui.c, t, rr);
            } else {
                ui.c.fill_rect(rr, t.menu_pop_hi);
            }
        }
        let col = if i == f.vals[row] { if lemon(ui) { t.brand_sel(brand::MAIN) } else { t.glyph } } else { t.text };
        ui.c.text_clip(font, rr.x + 10.0, (rr.cy() - lh / 2.0).round(), rr.w - 16.0, col, ch);
    }
}

/// A section panel with its header tab (as the Settings window's groups and FL's Save as form).
pub(super) fn section(ui: &mut Ui, p: Rect, title: &str) {
    let t = ui.t;
    let f = Font::new(13).bold();
    if lemon(ui) {
        lk::outlined(ui.c, p, lk::r_panel(t) - 2.0, t.s(PANEL_LINE), t.s(PANEL));
        ui.c.text(f, p.x + 10.0, p.y + 6.0, t.accent, title);
    } else {
        ui.c.fill_rect(p, t.s(PANEL_LINE));
        ui.c.fill_rect(p.inset(1.0), t.s(0x25292b));
        let tw = ui.c.text_width(f, title);
        let hdr = Rect::new(p.x + 1.0, p.y + 1.0, tw + 14.0, 22.0);
        ui.c.fill_rect(hdr, t.s(0x1f2324));
        for k in 0..6 {
            let k = k as f32;
            ui.c.fill_rect(Rect::new(hdr.right() + k, hdr.y, 1.0, hdr.h - k * 4.0), t.s(0x1f2324));
        }
        ui.c.text(f, p.x + 6.0, p.y + 4.0, t.text, title);
    }
}

/// A square tool button with an icon; `on` = it can be used now.
pub(super) fn tool(ui: &mut Ui, r: Rect, icon: Icon, h: DHit, hint: &'static str, on: bool) {
    let t = ui.t;
    let act = Action::Dlg(h);
    let down = on && ui.down(r, &act);
    let hov = on && ui.hit(r, act, hint);
    if lemon(ui) {
        let st = kit::State { hover: hov, pressed: down, on: false, disabled: !on };
        lk::outlined(ui.c, r, lk::r_btn(t), t.s(0x161819), lk::face(t.s(0x2a2d2f), st));
    } else {
        ui.c.fill_rect(r, t.s(WELL_LINE));
        ui.c.fill_rect(r.inset(1.0), t.s(if down { 0x1a1d1e } else if hov { 0x2c3133 } else { 0x23272a }));
    }
    let col = if !on { t.text_faint.with_alpha(110) } else if hov { t.glyph } else { t.text };
    icons::draw(ui.c, icon, r.inset(6.0), col);
}

/// A dark field or list body.
fn well(ui: &mut Ui, r: Rect, line: Option<Color>) {
    let t = ui.t;
    if lemon(ui) {
        lk::outlined(ui.c, r, lk::r_btn(t).min(r.h / 2.0), line.unwrap_or(t.s(WELL_LINE)), t.s(WELL));
    } else {
        let (c, w) = match line {
            Some(c) => (c, 2.0),
            None => (t.s(WELL_LINE), 1.0),
        };
        ui.c.fill_rect(r, c);
        ui.c.fill_rect(r.inset(w), t.s(WELL));
    }
}

/// A one-line text field: the text (scrolled so the caret shows), the selection or the caret while focused.
pub(super) fn field(ui: &mut Ui, r: Rect, fl: &Field, focused: bool, h: DHit, hint: &'static str) {
    let t = ui.t;
    let lemon = lemon(ui);
    hit(ui, r, h, hint);
    well(ui, r, focused.then_some(if lemon { t.brand_sel(brand::MAIN) } else { FOCUS_BLUE }));
    let f = Font::new(13);
    let lh = ui.c.line_height(f);
    let inner = Rect::new(r.x + 7.0, r.y + 2.0, r.w - 14.0, r.h - 4.0);
    let caret_x = ui.c.text_width(f, fl.before_caret());
    let shift = (caret_x - (inner.w - 2.0)).max(0.0);
    let (x, y) = (inner.x - shift, (r.cy() - lh / 2.0).round());
    ui.c.push_clip(inner);
    let mut ink = t.text;
    if focused && fl.all {
        let w = ui.c.text_width(f, &fl.text);
        ui.c.fill_rect(Rect::new(x - 1.0, y, w + 2.0, lh), if lemon { t.brand_sel(brand::COLORED_DARK) } else { SEL_BLUE });
        ink = if lemon { t.brand_sel(brand::HIGHLIGHT) } else { Color::rgb(0xffffff) };
    }
    ui.c.text(f, x, y, ink, &fl.text);
    if focused && !fl.all {
        ui.c.fill_rect(Rect::new((x + caret_x).round(), y, 1.0, lh), t.glyph);
    }
    ui.c.pop_clip();
}

/// A row's background: selected or hovered.
fn row_bg(ui: &mut Ui, row: Rect, sel: bool, hov: bool) {
    let t = ui.t;
    if lemon(ui) {
        if sel {
            ui.c.fill_rrect(Rect::new(row.x + 3.0, row.y + 1.0, row.w - 6.0, row.h - 2.0), 4.0, t.brand_sel(brand::COLORED_DARK));
        } else if hov {
            ui.c.fill_rrect(Rect::new(row.x + 3.0, row.y + 1.0, row.w - 6.0, row.h - 2.0), 4.0, t.s(0x262a2c));
        }
    } else if sel {
        ui.c.fill_rect(row, t.s(0x2f4a63));
    } else if hov {
        ui.c.fill_rect(row, t.s(0x24292b));
    }
}

/// Row text colour.
fn row_ink(ui: &Ui, sel: bool, hov: bool) -> Color {
    let t = ui.t;
    if sel && lemon(ui) {
        t.brand_sel(brand::HIGHLIGHT)
    } else if sel || hov {
        t.glyph
    } else {
        t.text
    }
}

fn places(ui: &mut Ui, f: &FileDlg, r: Rect) {
    let t = ui.t;
    well(ui, r, None);
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    let now = f.place_now();
    let clip = r.inset(1.0);
    ui.c.push_clip(clip);
    let mut y = r.y + 4.0;
    for (i, p) in f.places.iter().enumerate() {
        if i > 0 && p.kind == PlaceKind::Drive && f.places[i - 1].kind != PlaceKind::Drive {
            ui.c.fill_rect(Rect::new(r.x + 8.0, y + 3.0, r.w - 16.0, 1.0), t.s(0x2a2e30));
            y += 7.0;
        }
        if y > clip.bottom() {
            break;
        }
        let row = Rect::new(clip.x, y, clip.w, 22.0);
        let hov = ui.hit(row.intersect(&clip), Action::Dlg(DHit::Place(i)), p.path.display().to_string());
        let sel = now == Some(i);
        row_bg(ui, row, sel, hov);
        let shape = match p.kind {
            PlaceKind::Home => &tabler::USER,
            PlaceKind::Desktop => &tabler::LAYOUT_ROWS,
            PlaceKind::Documents => &tabler::FILE_TEXT,
            PlaceKind::Downloads => &tabler::DOWNLOAD,
            PlaceKind::Music => &tabler::MUSIC,
            PlaceKind::Start => &tabler::FOLDER_OPEN,
            PlaceKind::Drive => &tabler::BOX,
        };
        let ink = row_ink(ui, sel, hov);
        let ic = if sel && lemon(ui) { t.brand_sel(brand::MAIN) } else if sel || hov { t.glyph } else { t.text_dim };
        ui.c.icon(shape, Rect::new(row.x + 8.0, row.y + 3.0, 16.0, 16.0), ic);
        ui.c.text_clip(font, row.x + 30.0, (row.cy() - lh / 2.0).round(), row.w - 36.0, ink, &p.label);
        y += 22.0;
    }
    ui.c.pop_clip();
}

/// The icon a file row shows.
fn file_icon(name: &str) -> Icon {
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "lmn" | "flp" => Icon::FileProject,
        "wav" | "mp3" | "flac" | "ogg" | "aif" | "aiff" => Icon::FileAudio,
        "zip" => Icon::FileZip,
        "txt" | "flstheme" => Icon::FileText,
        e if e == crate::presets::EXT => Icon::FilePreset,
        _ => Icon::FileBlank,
    }
}

fn list(ui: &mut Ui, f: &FileDlg, r: Rect) {
    let t = ui.t;
    let lemon = lemon(ui);
    well(ui, r, None);
    hit(ui, r, DHit::List, "");
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    let head = Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, HEAD_H);
    let rows = Rect::span(r.x + 1.0, r.right() - 1.0, head.bottom(), r.bottom() - 1.0);
    f.view.set(rows);
    let total = f.entries.len() as f32 * ROW_H;
    let scroll = f.scroll.min((total - rows.h).max(0.0)).max(0.0);
    let bar = total > rows.h;
    let body = if bar { Rect::new(rows.x, rows.y, rows.w - 11.0, rows.h) } else { rows };
    let x_mod = body.right() - 128.0;
    let x_size = x_mod - 76.0;
    // Header: the sort column shows its direction.
    if lemon {
        ui.c.fill_rrect(head, lk::r_btn(t) - 1.0, t.s(0x222527));
        ui.c.fill_rect(Rect::new(head.x, head.bottom() - 4.0, head.w, 4.0), t.s(0x222527));
    } else {
        ui.c.fill_rect(head, t.s(0x23272a));
    }
    ui.c.fill_rect(Rect::new(head.x, head.bottom() - 1.0, head.w, 1.0), t.s(WELL_LINE));
    for (by, label, x0, x1, right) in [(SortBy::Name, "Name", head.x, x_size, false), (SortBy::Size, "Size", x_size, x_mod, true), (SortBy::Modified, "Modified", x_mod, head.right(), false)] {
        let hr = Rect::span(x0, x1, head.y, head.bottom() - 1.0);
        let hov = hit(ui, hr, DHit::Header(by), "Sort by this column (again: the other way round)");
        let col = if hov { t.glyph } else { t.text_dim };
        let tw = ui.c.text_width(font, label);
        let tx = if right { hr.right() - 8.0 - tw } else { hr.x + if by == SortBy::Name { 30.0 } else { 6.0 } };
        let ty = (hr.cy() - lh / 2.0).round();
        ui.c.text(font, tx, ty, col, label);
        if f.sort == by {
            let cx = if right { tx - 8.0 } else { tx + tw + 8.0 };
            kit::chevron(ui.c, cx, hr.cy(), 3.0, if f.reversed { kit::Dir::Up } else { kit::Dir::Down }, if lemon { t.brand_lit(brand::MAIN) } else { t.text_dim });
        }
        if by != SortBy::Name {
            ui.c.fill_rect(Rect::new(x0, hr.y + 4.0, 1.0, hr.h - 8.0), t.s(WELL_LINE));
        }
    }
    // Rows.
    ui.c.push_clip(body);
    let first = (scroll / ROW_H).floor() as usize;
    let shown = (body.h / ROW_H).ceil() as usize + 1;
    for i in first..(first + shown).min(f.entries.len()) {
        let e = &f.entries[i];
        let row = Rect::new(body.x, (body.y + i as f32 * ROW_H - scroll).round(), body.w, ROW_H);
        let hov = ui.hit(row.intersect(&body), Action::Dlg(DHit::Row(i)), "");
        let sel = f.sel == Some(i);
        row_bg(ui, row, sel, hov);
        let ink = row_ink(ui, sel, hov);
        let (ic, icol) = if e.dir { (Icon::Folder, if lemon { t.brand_lit(brand::MAIN) } else { t.browser_folder }) } else { (file_icon(&e.name), if sel || hov { t.glyph } else { t.text_dim }) };
        icons::draw(ui.c, ic, Rect::new(row.x + 8.0, row.y + 2.0, 16.0, 16.0), icol);
        let ty = (row.cy() - lh / 2.0).round();
        ui.c.text_clip(font, row.x + 30.0, ty, x_size - row.x - 36.0, ink, &e.name);
        if !e.dir {
            let size = fd::size_text(e.size);
            let sw = ui.c.text_width(font, &size);
            ui.c.text(font, x_mod - 8.0 - sw, ty, if sel { ink } else { t.text_dim }, &size);
        }
        if let Some(m) = e.modified {
            ui.c.text_clip(font, x_mod + 6.0, ty, body.right() - x_mod - 8.0, if sel { ink } else { t.text_dim }, &fd::time_text(m));
        }
    }
    if f.entries.is_empty() {
        let note = match &f.error {
            Some(e) => format!("This folder cannot be read: {e}"),
            None if f.mode == Mode::Folder => "No folders here".to_string(),
            None => "Nothing to list here".to_string(),
        };
        ui.c.text_clip(font, body.x + 12.0, body.y + 10.0, body.w - 24.0, if f.error.is_some() { t.h(0xff9e00) } else { t.text_dim }, &note);
    }
    ui.c.pop_clip();
    if bar {
        let track = Rect::new(body.right() + 1.0, rows.y + 1.0, 9.0, rows.h - 2.0);
        let act = Action::Dlg(DHit::Bar);
        let down = ui.down(track, &act);
        let hov = ui.hit(track, act, "");
        kit::scrollbar(ui.c, t, track, true, scroll, rows.h, total, kit::State::new(hov, down, false));
    }
}

/// The type combo's list, under the combo (above it when there is no room).
fn filter_list(ui: &mut Ui, f: &FileDlg, c: Rect, dlg: Rect) {
    let t = ui.t;
    let h = f.filters.len() as f32 * 22.0 + 6.0;
    let below = Rect::new(c.x, c.bottom() + 2.0, c.w, h);
    let p = if below.bottom() > dlg.bottom() - 4.0 { Rect::new(c.x, c.y - 2.0 - h, c.w, h) } else { below };
    if lemon(ui) {
        lk::popup(ui.c, t, p);
    } else {
        ui.c.fill_rect(p, t.menu_pop_border);
        ui.c.fill_rect(p.inset(1.0), t.menu_pop);
    }
    hit(ui, p, DHit::Body, "");
    let font = Font::new(13);
    let lh = ui.c.line_height(font);
    for (i, flt) in f.filters.iter().enumerate() {
        let row = Rect::new(p.x + 1.0, p.y + 3.0 + i as f32 * 22.0, p.w - 2.0, 22.0);
        let hov = hit(ui, row, DHit::FilterItem(i), "");
        if hov {
            if lemon(ui) {
                lk::row_highlight(ui.c, t, row);
            } else {
                ui.c.fill_rect(row, t.menu_pop_hi);
            }
        }
        let col = if i == f.filter { if lemon(ui) { t.brand_sel(brand::MAIN) } else { t.glyph } } else { t.text };
        ui.c.text_clip(font, row.x + 10.0, (row.cy() - lh / 2.0).round(), row.w - 16.0, col, &flt.label());
    }
}


fn headphones_box(ui:&mut Ui,s:&State,m:&Modal,f:&crate::headphones::Form){
 let r=place(s,m,620f32.min(s.screen.0),505f32.min(s.screen.1));
 frame(ui,r,"Headphone correction");
 let ink=ui.t.text;let dim=ui.t.text_dim;
 ui.c.text_clip(Font::new(13).bold(),r.x+18.,r.y+40.,r.w-36.,ink,"Choose your headphones");
 ui.c.text_clip(Font::new(11),r.x+18.,r.y+62.,r.w-36.,dim,"Monitor only — never applied to exports, stems or bounced tracks.");
 let search=Rect::new(r.x+18.,r.y+86.,r.w-36.,29.);
 field(ui,search,&f.search,true,DHit::HeadphoneSearch,"Search model, brand, impedance or variant. Enter loads the highlighted profile.");
 if f.search.text.is_empty(){ui.c.text_clip(Font::new(12),search.x+8.,search.y+7.,search.w-16.,dim,"Search 6,033 headphone models...");}
 let row_h=((r.h-245.)/10.).clamp(15.,26.);
 let list=Rect::new(r.x+18.,r.y+126.,r.w-36.,row_h*10.);
 ui.c.fill_rrect(list,5.,ui.t.s(0x151c20));
 let cat=crate::headphones::catalog();
 for (n,&i) in f.results.iter().enumerate().skip(f.scroll).take(10){
  let row=Rect::new(list.x+3.,list.y+(n-f.scroll) as f32*row_h,list.w-6.,row_h);
  let (name,path)=cat[i];let source=path.split('/').next().unwrap_or("AutoEq").replace("%20"," ");
  let hover=ui.hit(row,Action::Dlg(DHit::Headphone(i)),format!("{name} | Measurement: {source} | Click to download and enable. Match your exact headphone variant."));
  if hover || n==f.selected {ui.c.fill_rrect(row,3.,if hover{ui.t.button_hi}else{ui.t.s(0x263b40)});}
  ui.c.text_clip(Font::new(12),row.x+8.,row.y+5.,row.w-145.,if s.headphones.name==name{ui.t.accent}else{ink},name);
  ui.c.text_clip(Font::new(10),row.right()-128.,row.y+6.,120.,dim,&source);
 }
 if f.results.is_empty(){ui.c.text_clip(Font::new(12),list.x+12.,list.y+18.,list.w-24.,dim,"No measured profile found. Try the brand or model number.");}
 let y=list.bottom()+8.;
 ui.c.text_clip(Font::new(10),r.x+18.,y+6.,r.w-180.,dim,&format!("{} matches | Scroll or use arrow keys",f.results.len()));
 for (next,x,label) in [(false,r.right()-148.,"Previous"),(true,r.right()-80.,"Next")]{let b=Rect::new(x,y,62.,23.);let hover=hit(ui,b,DHit::HeadphonePage(next),"Browse headphone profiles");ui.c.fill_rrect(b,4.,if hover{ui.t.button_hi}else{ui.t.s(0x303b40)});ui.c.text_center(Font::new(10),b,ink,label);}
 let on=s.headphones.on;let b=Rect::new(r.x+18.,y+37.,76.,26.);
 let hover=hit(ui,b,DHit::HeadphoneToggle,"Enable / bypass the selected headphone correction");
 ui.c.fill_rrect(b,4.,if on{ui.t.s(0x416c52)}else if hover{ui.t.button_hi}else{ui.t.s(0x303b40)});ui.c.text_center(Font::new(11).bold(),b,ink,if on{"ON"}else{"OFF"});
 ui.c.text_clip(Font::new(12),r.x+106.,y+42.,r.w-124.,ink,if s.headphones.name.is_empty(){"No headphones selected"}else{&s.headphones.name});
 let status=if s.headphones.status.is_empty(){"Profiles download once, then work offline. AutoEq / measured-model correction."}else{&s.headphones.status};
 let sr=Rect::new(r.x+18.,y+69.,r.w-36.,20.);ui.hit(sr,Action::Swallow,status.to_string());
 ui.c.text_clip(Font::new(10),sr.x,sr.y,sr.w,dim,status);
 ui.c.text_clip(Font::new(10),r.x+18.,r.bottom()-20.,r.w-98.,dim,"AutoEq targets neutral listening balance; fit and individual headphones vary.");
 let cr=Rect::new(r.right()-78.,r.bottom()-24.,60.,18.);hit(ui,cr,DHit::HeadphoneCredits,"AutoEq source and license");ui.c.text_center(Font::new(10),cr,ui.t.accent,"Credits");
}


fn route_picker_box(ui:&mut Ui,s:&State,m:&Modal,f:&crate::route_picker::Form){
 use crate::route_picker::{COLS,ROWS};
 let r=place(s,m,840f32.min(s.screen.0-12.),462f32.min(s.screen.1-12.));
 frame(ui,r,"Send to mixer track");
 ui.c.text_clip(Font::new(12).bold(),r.x+14.,r.y+36.,r.w-28.,ui.t.text,&format!("Choose a destination for {}",f.channel));
 ui.c.text_clip(Font::new(10),r.x+14.,r.y+55.,r.w-28.,ui.t.text_dim,"Shared sample clips use the same routing. Use Make Unique first for independent routing.");
 field(ui,Rect::new(r.x+14.,r.y+75.,r.w-28.,26.),&f.search,true,DHit::RouteSearch,"Filter by track name or number");
 if f.search.text.is_empty(){ui.c.text_clip(Font::new(11),r.x+23.,r.y+81.,r.w-46.,ui.t.text_dim,"Find a track by name or number...");}
 let grid=Rect::new(r.x+14.,r.y+110.,r.w-28.,r.h-151.);
 let (cw,ch)=(grid.w/COLS as f32,grid.h/ROWS as f32);
 for (pos,&i) in f.shown.iter().skip(f.first).take(COLS*ROWS).enumerate(){
  let t=&f.tracks[i];let cell=Rect::new(grid.x+(pos%COLS) as f32*cw+2.,grid.y+(pos/COLS) as f32*ch+2.,cw-4.,ch-4.);
  let current=t.index==f.current;let color=t.color.map(super::rgb).unwrap_or(ui.t.accent);
  let hover=ui.hit(cell,Action::Dlg(DHit::RoutePick(t.index)),format!("{} — {}{} | Click to route",t.index,t.name,if current{" (current)"}else{""}));
  ui.c.fill_rrect(cell,4.,if hover{ui.t.button_hi}else{ui.t.s(0x252d30)});
  ui.c.stroke_rrect(cell,4.,1.,if current||hover{ui.t.accent}else{ui.t.s(0x3b464b)});
  ui.c.text_center(Font::new(11).bold(),Rect::new(cell.x,cell.y+1.,cell.w,14.),if current{ui.t.accent}else{ui.t.text},&t.index.to_string());
  let strip=Rect::new(cell.cx()-11.,cell.y+18.,22.,(ch-50.).clamp(12.,28.));
  ui.c.fill_rrect(strip,2.,ui.t.s(0x101719));ui.c.fill_rect(Rect::new(strip.x,strip.y,strip.w,2.),color);
  ui.c.line(strip.cx(),strip.y+5.,strip.cx(),strip.bottom()-3.,1.,ui.t.s(0x718087));
  let y=strip.bottom()-4.-t.level.clamp(0.,1.) as f32*(strip.h-10.);
  ui.c.fill_rrect(Rect::new(strip.cx()-5.,y-2.,10.,4.),1.,color);
  let font=Font::new(10);let mut lines=vec![String::new()];
  for word in t.name.split_whitespace(){let last=lines.last().unwrap();let next=if last.is_empty(){word.into()}else{format!("{last} {word}")};if !last.is_empty()&&ui.c.text_width(font,&next)>cell.w-6.&&lines.len()<2{lines.push(word.into());}else{*lines.last_mut().unwrap()=next;}}
  for (n,line) in lines.iter().take(2).enumerate(){ui.c.text_clip(font,cell.x+3.,strip.bottom()+4.+n as f32*11.,cell.w-6.,ui.t.text,line);}
 }
 if f.shown.is_empty(){ui.c.text_clip(Font::new(12),grid.x+10.,grid.y+20.,grid.w-20.,ui.t.text_dim,"No tracks match. Try another name or number.");}
 let footer=Rect::new(r.x+14.,r.bottom()-30.,r.w-28.,22.);
 ui.c.text_clip(Font::new(10),footer.x,footer.y+5.,footer.w-156.,ui.t.text_dim,&format!("{} tracks | Current: {} | Scroll to browse",f.shown.len(),f.current));
 for (next,x,label) in [(false,r.right()-153.,"Previous"),(true,r.right()-82.,"Next")]{let b=Rect::new(x,footer.y,66.,22.);let hover=hit(ui,b,DHit::RoutePage(next),"Browse mixer tracks");ui.c.fill_rrect(b,3.,if hover{ui.t.button_hi}else{ui.t.s(0x303b40)});ui.c.text_center(Font::new(10),b,ui.t.text,label);}
}


fn ai_box(ui:&mut Ui,s:&State,m:&Modal){
 let r=place(s,m,600f32.min(s.screen.0-12.),410f32.min(s.screen.1-12.));frame(ui,r,"AI control");
 let a=&s.ai;let ink=ui.t.text;let dim=ui.t.text_dim;
 ui.c.text_clip(Font::new(19).bold(),r.x+22.,r.y+43.,r.w-44.,ink,"Your studio. Your assistant.");
 ui.c.text_clip(Font::new(12),r.x+22.,r.y+74.,r.w-44.,dim,"Connect a compatible AI assistant to read and edit your project.");
 ui.c.text_clip(Font::new(11),r.x+22.,r.y+94.,r.w-44.,dim,"Off by default. No model downloads or background AI processing.");
 let button=|ui:&mut Ui,b:Rect,h:DHit,label:&str,lit:bool|{let hover=hit(ui,b,h,"");ui.c.fill_rrect(b,5.,if lit{ui.t.s(0x486c45)}else if hover{ui.t.button_hi}else{ui.t.s(0x303b40)});ui.c.text_center(Font::new(12).bold(),b,ui.t.text,label);};
 button(ui,Rect::new(r.x+22.,r.y+126.,155.,32.),DHit::AiEnable,if a.enabled{"Turn AI control OFF"}else{"Enable AI control"},a.enabled);
 ui.c.text_clip(Font::new(12),r.x+193.,r.y+135.,r.w-215.,if a.enabled{ui.t.accent}else{dim},if !a.enabled{"Disconnected"}else if a.client.is_empty(){"Waiting for assistant"}else{&a.client});
 if !a.pending.is_empty(){
  ui.c.fill_rrect(Rect::new(r.x+20.,r.y+174.,r.w-40.,121.),6.,ui.t.s(0x283c42));
  ui.c.text_clip(Font::new(13).bold(),r.x+32.,r.y+186.,r.w-64.,ink,&format!("{} wants to connect",a.pending));
  ui.c.text_clip(Font::new(11),r.x+32.,r.y+207.,r.w-64.,dim,"Allow this assistant to read and change the open project?");
  let check=Rect::new(r.x+32.,r.y+229.,240.,20.);hit(ui,check,DHit::AiRemember,"Remember this assistant and its private local connection key");ui.c.text_clip(Font::new(11),check.x,check.y,check.w,ink,if a.remember{"[x] Remember this assistant"}else{"[ ] Remember this assistant"});
  button(ui,Rect::new(r.right()-220.,r.y+251.,90.,29.),DHit::AiApprove(false),"Decline",false);
  button(ui,Rect::new(r.right()-120.,r.y+251.,88.,29.),DHit::AiApprove(true),"Allow",true);
 }else{
  button(ui,Rect::new(r.x+22.,r.y+181.,210.,31.),DHit::AiSetup,"Save setup on Desktop",false);
  button(ui,Rect::new(r.x+244.,r.y+181.,170.,31.),DHit::AiForget,"Forget assistant",false);
  ui.c.text_clip(Font::new(11),r.x+22.,r.y+231.,r.w-44.,dim,"1. Enable AI.  2. Import the setup into your assistant.  3. Approve it here.");
  ui.c.text_clip(Font::new(11),r.x+22.,r.y+252.,r.w-44.,dim,"Your chosen assistant may send project data to its AI provider.");
  ui.c.text_clip(Font::new(11),r.x+22.,r.y+273.,r.w-44.,dim,"Turning this off immediately revokes access. Audio keeps running.");
 }
 let status=Rect::new(r.x+22.,r.y+313.,r.w-44.,45.);ui.hit(status,Action::Swallow,a.status.clone());
 let mut y=status.y;let mut line=String::new();for word in a.status.split_whitespace(){let next=if line.is_empty(){word.into()}else{format!("{line} {word}")};if ui.c.text_width(Font::new(11),&next)>status.w&&!line.is_empty(){ui.c.text_clip(Font::new(11),status.x,y,status.w,ink,&line);y+=16.;line=word.into();if y>status.bottom(){break;}}else{line=next;}}if y<=status.bottom(){ui.c.text_clip(Font::new(11),status.x,y,status.w,ink,&line);}
 ui.c.text_clip(Font::new(10),r.x+22.,r.bottom()-26.,r.w-44.,dim,&format!("Local connection only  |  {} commands this session  |  MCP compatible",a.calls));
}
