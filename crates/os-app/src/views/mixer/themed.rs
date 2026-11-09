// SPDX-License-Identifier: GPL-3.0-or-later
//! User palette applied to the existing mixer geometry.

use super::{Color, Rect, Ui};
use crate::mixer_skin::{self, Part};
use os_theme::appearance::Finish;
use os_ui::{Font, kit};

fn surface(ui: &mut Ui, r: Rect, color: Color, radius: f32) {
    let t = ui.t;
    if t.style.vintage_2005 {return crate::vintage::metal(ui.c,r,color);}
    kit::styled_surface(
        ui.c,
        t,
        r,
        radius,
        t.window_border,
        color,
        color,
        t.appearance.panels.unwrap_or(Finish::Flat),
    );
}

pub(super) fn panel(ui: &mut Ui, r: Rect, base: Color, selected: bool) {
    if ui.t.style.vintage_2005 {
        let face = Color::rgb(0x5f686d).mix(base, 0.18);
        crate::vintage::metal(ui.c, r, if selected { face.shade(0.10) } else { face });
        return;
    }
    if ui.mixer_skin.uses(Part::Panel) {
        return mixer_skin::panel(ui.c, ui.t, ui.mixer_skin, r, selected);
    }
    let color = ui.t.mixer_strip.mix(base, 0.15);
    surface(ui, r, color, ui.t.style.radius_panel);
    if selected {
        ui.c.stroke_rrect(r.inset(1.0), ui.t.style.radius_small, 1.0, ui.t.accent);
    }
}

pub(super) fn well(ui: &mut Ui, r: Rect) {
    if ui.t.style.vintage_2005 {crate::vintage::recess(ui.c,r,Color::rgb(0x505b61));} else {surface(ui, r, ui.t.mixer_strip_dark, ui.t.style.radius_small);}
}

pub(super) fn plate(ui: &mut Ui, r: Rect) {
    surface(ui, r, ui.t.mixer_plate, ui.t.style.radius_small);
}

pub(super) fn separator(ui: &mut Ui, r: Rect) {
    ui.c.fill_rect(r, ui.t.window_border);
}

pub(super) fn lamp(ui: &mut Ui, x: f32, y: f32, on: bool, _: Color, r: f32) {
    if ui.t.style.vintage_2005 {
        let bounds = Rect::new(x - 4.0, y - 4.0, 8.0, 8.0);
        if on { crate::vintage_rack_art::led_on(ui.c, bounds); }
        else { crate::vintage_rack_art::led_off(ui.c, bounds); }
        return;
    }
    if ui.t.appearance.indicator.is_some() {
        kit::led(ui.c, ui.t, x, y, r, kit::State::NORMAL.with_on(on));
    } else {
        ui.c.fill_circle(x, y, r, ui.t.button_border);
        ui.c.fill_circle(
            x,
            y,
            (r - 1.0).max(0.0),
            if on {
                ui.t.indicator_on
            } else {
                ui.t.indicator_off
            },
        );
    }
}

pub(super) fn button(ui: &mut Ui, r: Rect, on: bool, hover: bool, _: Color) -> Color {
    if ui.t.style.vintage_2005 {
        let face = Color::rgb(if on { 0x3c5054 } else if hover { 0x69777d } else { 0x536066 });
        if on { crate::vintage::recess(ui.c, r, face); }
        else { crate::vintage::metal(ui.c, r, face); }
        if on && r.w >= 15.0 && r.h >= 13.0 {
            ui.c.fill_rect(Rect::new(r.right() - 5.0, r.y + 3.0, 2.0, 2.0), Color::rgb(0xe4a452));
        }
        return ui.t.text;
    }
    if ui.mixer_skin.uses(Part::Button) {
        let state = kit::State::new(hover, ui.pressed_here(r), on);
        return mixer_skin::button(ui.c, ui.t, ui.mixer_skin, r, state);
    }
    let t = ui.t;
    let (top, bottom) = if on {
        (t.on_top, t.on_bottom)
    } else {
        let fill = if hover { t.button_hi } else { t.button };
        (fill, fill)
    };
    kit::styled_surface(
        ui.c,
        t,
        r,
        t.style.radius_small,
        t.button_border,
        top,
        bottom,
        t.appearance
            .mixer_buttons
            .or(t.appearance.buttons)
            .unwrap_or(Finish::Flat),
    );
    if on { t.on_glyph } else { t.glyph }
}

pub(super) fn signal_switch(ui: &mut Ui, r: Rect, on: bool, hover: bool, swap: bool) {
    let ink = button(ui, r, on, hover, ui.t.accent);
    ui.c.text_center(if ui.t.style.vintage_2005 { Font::vintage(10) } else { Font::new(12) }, r, ink, if swap { "⇔" } else { "Ø" });
}

pub(super) fn rocker(ui: &mut Ui, r: Rect, on: bool, hover: bool) {
    let ink = button(ui, r, on, hover, ui.t.accent);
    let y = if on { r.y + 4.0 } else { r.bottom() - 4.0 };
    ui.c.line(r.x + 3.0, y, r.right() - 3.0, y, 1.0, ink);
}

pub(super) fn fader(ui: &mut Ui, x: f32, y: f32, selected: bool, hover: bool) {
    if ui.t.style.vintage_2005 {
        return crate::vintage::fader(ui.c,x,y,hover);
    }
    let cap = Rect::new(x - 8.0, y - 13.0, 16.0, 26.0);
    if ui.mixer_skin.uses(Part::Fader) {
        return mixer_skin::fader(ui.c, ui.t, ui.mixer_skin, cap, selected, hover);
    }
    let color = if selected {
        ui.t.fader_handle.mix(ui.t.accent, 0.24)
    } else {
        ui.t.fader_handle
    };
    let color = if hover { color.shade(0.12) } else { color };
    surface(ui, cap, color, ui.t.style.radius_small);
    let mark = if ui.t.style.vintage_2005 { ui.t.record.shade(-0.16) } else { ui.t.glyph };
    ui.c.line(cap.x + 2.0, y, cap.right() - 2.0, y, 1.0, mark);
}

pub(super) fn vu(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, _: bool) {
    if ui.mixer_skin.uses(Part::Meter) {
        let levels = levels.map(|v| crate::meters::vu_fraction(crate::meters::vu_db(v)));
        return mixer_skin::meter(ui.c, ui.t, ui.mixer_skin, r, levels, clipped);
    }
    if !ui.c.visible(r) {
        return;
    }
    if ui.t.style.vintage_2005 {
        return vintage_meter(ui, r, levels.map(|level| crate::meters::vu_fraction(crate::meters::vu_db(level))), clipped, "VU");
    }
    plate(ui, r);
    let face = r.inset(4.0);
    for (side, level) in levels.into_iter().enumerate() {
        let pos = crate::meters::vu_fraction(crate::meters::vu_db(level));
        let h = face.h * pos;
        let color = if clipped {
            ui.t.meter_clip
        } else if pos > 0.7 {
            ui.t.meter_high
        } else {
            ui.t.meter_low
        };
        ui.c.fill_rect(
            Rect::new(
                face.x + face.w * 0.5 * side as f32,
                face.bottom() - h,
                (face.w * 0.5 - 2.0).max(0.0),
                h,
            ),
            color,
        );
    }
}


pub(super) fn vintage_meter(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, label: &str) {
    if !ui.c.visible(r) { return; }
    crate::vintage::recess(ui.c, r, Color::rgb(0x354247));
    let glass = Rect::new(r.cx() - 14.0, r.y + 3.0, 28.0, (r.h - 14.0).max(1.0));
    crate::vintage::recess(ui.c, glass, Color::rgb(0x182426));
    let bank = glass.inset(2.0);
    for (side, level) in levels.into_iter().enumerate() {
        let width = bank.w * 0.5;
        let bar = Rect::new(bank.x + width * side as f32, bank.y, width * 0.8, bank.h);
        crate::vintage::meter_idle(ui.c, bar);
        let height = (bar.h * level.clamp(0.0, 1.0)).round();
        if height > 0.0 {
            let lit = Rect::new(bar.x, bar.bottom() - height, bar.w, height);
            ui.c.push_clip(lit);
            if !crate::vintage_skin::draw(ui.c, "FXMeterColumn", 0, bar) {
                ui.c.fill_rect_gradient(bar, Color::rgb(0xbf8d4d), Color::rgb(0x4b8a48), 0.0);
            }
            ui.c.pop_clip();
        }
    }
    ui.c.text_center(Font::vintage(7), Rect::new(r.x, r.bottom()-11.0, r.w, 10.0), ui.t.text_dim, label);
    if clipped {
        ui.c.fill_rect(Rect::new(r.right()-7.0, r.y+5.0, 3.0, 3.0), ui.t.meter_clip);
    }
}
