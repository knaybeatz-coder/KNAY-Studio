// SPDX-License-Identifier: GPL-3.0-or-later
//! Flat finish for the separately built clean full-mixer preview.
use crate::app::Ui;
use os_ui::{Color, Rect};
use std::f32::consts::PI;

const LINE: Color = Color::rgb(0x191919);
const FACE: Color = Color::rgb(0x292929);
const INK: Color = Color::rgb(0xe2e2df);
const YELLOW: Color = Color::rgb(0xe6ce65);

pub(super) fn panel(ui: &mut Ui, r: Rect, base: Color, selected: bool) {
    ui.c.fill_rect(r, LINE);
    ui.c.fill_rect(r.inset(1.), FACE.mix(base, 0.18));
    if selected {
        ui.c.stroke_rrect(r.inset(1.), 0., 1., YELLOW);
    }
}
pub(super) fn well(ui: &mut Ui, r: Rect) {
    ui.c.fill_rect(r, FACE);
}
pub(super) fn plate(ui: &mut Ui, r: Rect) {
    ui.c.fill_rect(r, LINE);
    ui.c.fill_rect(r.inset(1.), Color::rgb(0x303030));
}
pub(super) fn separator(ui: &mut Ui, r: Rect) {
    ui.c.fill_rect(r, LINE);
}
pub(super) fn lamp(ui: &mut Ui, x: f32, y: f32, on: bool, col: Color, r: f32) {
    ui.c.fill_circle(x, y, r, if on { col } else { Color::rgb(0x555555) });
}
pub(super) fn knob(ui: &mut Ui, x: f32, y: f32, r: f32, value: f32, _cap: Color, hover: bool) {
    let value = value.clamp(0., 1.);
    let start = -1.25 * PI;
    let sweep = 1.5 * PI * value;
    let a = start + sweep;
    ui.c.fill_circle(x, y, r, Color::rgb(if hover { 0x555555 } else { 0x424242 }));
    ui.c.stroke_circle(x, y, r, 0.7, Color::rgb(0x5a5a5a));
    ui.c.line(
        x + r * 0.51 * a.cos(),
        y + r * 0.51 * a.sin(),
        x + r * 0.94 * a.cos(),
        y + r * 0.94 * a.sin(),
        1.35,
        INK,
    );
}
pub(super) fn fader(ui: &mut Ui, x: f32, y: f32, _selected: bool, hover: bool) {
    let cap = Rect::new(x - 8., y - 13., 16., 26.);
    ui.c.fill_rrect(cap, 2., LINE);
    ui.c.fill_rrect(
        cap.inset(1.),
        1.5,
        Color::rgb(if hover { 0xc3c3c3 } else { 0xa3a3a3 }),
    );
    ui.c.line(cap.x + 1., y, cap.right() - 1., y, 1., FACE);
}
pub(super) fn button(ui: &mut Ui, r: Rect, on: bool, hover: bool, col: Color) -> Color {
    let face = if on {
        col
    } else if hover {
        Color::rgb(0x505050)
    } else {
        Color::rgb(0x373737)
    };
    ui.c.fill_rrect(r, 2., LINE);
    ui.c.fill_rrect(r.inset(1.), 1., face);
    if on {
        LINE
    } else {
        INK
    }
}
pub(super) fn signal_switch(ui: &mut Ui, r: Rect, on: bool, hover: bool, swap: bool) {
    let ink = button(ui, r, on, hover, YELLOW);
    let (x, y) = (r.cx(), r.cy());
    if swap {
        for s in [-1., 1.] {
            ui.c.line(x + s, y - 2.6, x + 4. * s, y, 1., ink);
            ui.c.line(x + 4. * s, y, x + s, y + 2.6, 1., ink);
        }
    } else {
        ui.c.stroke_circle(x, y, 2.7, 1., ink);
        ui.c.line(x - 3.2, y + 3.2, x + 3.2, y - 3.2, 1., ink);
    }
}
pub(super) fn rocker(ui: &mut Ui, r: Rect, on: bool, hover: bool) {
    button(ui, r, false, hover, YELLOW);
    let half = Rect::new(
        r.x + 2.,
        if on { r.y + 2. } else { r.cy() },
        r.w - 4.,
        r.h * 0.5 - 2.,
    );
    ui.c.fill_rect(half, if on { YELLOW } else { Color::rgb(0x888888) });
}
pub(super) fn vu(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, _selected: bool) {
    if !ui.c.visible(r.outset(1.)) {
        return;
    }
    ui.c.fill_rect(r, LINE);
    let face = r.inset(1.5);
    ui.c.fill_rect(face, Color::rgb(0xd6d6d1));
    ui.c.push_clip(face);
    let (cx, cy, x, y) = super::analog::meter_scale(ui, face, levels, LINE, Color::rgb(0xb4453a));
    ui.c.line(cx, cy, x, y, 1.1, LINE);
    ui.c.fill_circle(cx, cy, 1.6, LINE);
    if clipped {
        ui.c.fill_circle(
            face.right() - 4.,
            face.bottom() - 4.,
            1.7,
            Color::rgb(0xc94d45),
        );
    }
    ui.c.pop_clip();
}
