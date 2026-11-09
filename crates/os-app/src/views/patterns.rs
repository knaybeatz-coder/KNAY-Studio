// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Pattern name box (with suggestions and colour swatches) and the delete confirmation.
// spec: ui/pattern-menu.md §3, §4

use os_ui::{Color, Font, Glyph, Rect};

use super::rgb;
use crate::app::{Action, State, Ui};
use crate::patterns::{self as pt, BoxTarget, Hit, BOX_H, BOX_W, SWATCHES, WORDS};

const BG: u32 = 0x1c2023;
const SUG_W: f32 = 96.0;
const SUG_ROW: f32 = 18.0;
const SW: f32 = 22.0;

fn custom_swatches(ui: &mut Ui, p: Rect, editing: Option<&(usize, crate::dialog::Field)>) {
    let t = ui.t;
    let y = p.y + SW * 2. + 2.;
    ui.c.text(Font::new(11), p.x + 5., y + 1., t.text_dim, "Custom swatches");
    for i in 0..pt::CUSTOM_SWATCHES {
        let cell = Rect::new(p.x + 5. + i as f32 * 36., y + 16., 32., 21.);
        let color = pt::custom_swatch(i);
        let swatch = Rect::new(cell.x, cell.y, 19., cell.h);
        let hot = ui.hit(swatch, Action::Pat(Hit::CustomSwatch(i)),
            if color.is_some() { "Apply saved color to the source" } else { "Create a custom color" });
        ui.c.fill_rrect(swatch, 3., color.map_or(t.sunken, Color::rgb));
        ui.c.stroke_rrect(swatch, 3., 1., if hot { t.glyph } else { t.menu_pop_border });
        if color.is_none() {
            ui.c.text_center(Font::new(13), swatch, t.text, "+");
        } else {
            let edit = Rect::new(swatch.right(), cell.y, cell.w - swatch.w, cell.h);
            let hot = ui.hit(edit, Action::Pat(Hit::EditSwatch(i)), "Edit or clear this swatch");
            if hot { ui.c.fill_rrect(edit, 3., t.menu_pop_hi); }
            crate::icons::draw(ui.c, crate::icons::Icon::Pencil, edit.inset(1.), t.text);
        }
    }
    if let Some((i, field)) = editing {
        let row = Rect::new(p.x + 5., y + 42., p.w - 10., 22.);
        let hex = Rect::new(row.x, row.y, 94., row.h);
        ui.c.fill_rrect(hex, 3., t.sunken);
        ui.c.text_clip(Font::new(12), hex.x + 5., hex.y + 3., hex.w - 8., t.text, &field.text);
        let x = hex.x + 5. + ui.c.text_width(Font::new(12), field.before_caret()).min(hex.w - 9.);
        ui.c.fill_rect(Rect::new(x, hex.y + 3., 1., 15.), t.accent);
        let mut left = hex.right() + 4.;
        for (label, action, width) in [
            ("Save", Hit::SaveSwatch, 41.),
            ("Clear", Hit::ClearSwatch, 41.),
            ("×", Hit::CancelSwatch, 24.),
        ] {
            let button = Rect::new(left, row.y, width, row.h);
            let hot = ui.hit(button, Action::Pat(action), format!("{label} swatch {}", i + 1));
            ui.c.fill_rrect(button, 3., if hot { t.menu_pop_hi } else { t.button });
            ui.c.text_center(Font::new(11), button, t.text, label);
            left += width + 3.;
        }
    }
}

fn cross(ui: &mut Ui, r: Rect, c: Color) {
    os_ui::kit::cross(ui.c, r, c);
}

fn tick(ui: &mut Ui, r: Rect, c: Color) {
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    ui.c.line(x - 5.0, y, x - 1.0, y + 4.0, 1.5, c);
    ui.c.line(x - 1.0, y + 4.0, x + 5.0, y - 4.0, 1.5, c);
}

/// A popup panel: square with a 1 px border, or the rounded Lemon panel (spec: ui/lemon-design.md §9).
fn panel(ui: &mut Ui, r: Rect, fill: Color) {
    if super::lemon(ui) {
        os_ui::kit::lemon::outlined(ui.c, r, 6.0, ui.t.menu_pop_border, fill);
    } else {
        ui.c.fill_rect(r, fill);
        ui.c.stroke_rrect(r, 0.0, 1.0, ui.t.menu_pop_border);
    }
}

/// The name box's colour tab (opens the swatches). Classic: a left-pointing tab down the box's right edge, with a dark
/// rim and a lit top edge (spec: ui/pattern-menu.md §4). Lemon: a rounded colour tile beside the name field, as the
/// piano roll's title tile (spec: ui/lemon-design.md).
fn colour_tab(ui: &mut Ui, r: Rect, col: Color, hint: &'static str) {
    let t = ui.t;
    if super::lemon(ui) {
        let tile = Rect::new(r.right() - 26.0, r.y + 4.0, 22.0, 22.0);
        let hov = ui.hit(tile, Action::Pat(Hit::Color), hint);
        let rad = os_ui::kit::lemon::r_small(t).max(4.0);
        let line = if hov { t.glyph } else { col.shade(-0.4) };
        os_ui::kit::lemon::outlined(ui.c, tile, rad, line, if hov { col.shade(0.12) } else { col });
        // A small chevron: the tile opens the colours.
        let (cx, cy) = (tile.x + tile.w / 2.0, tile.y + tile.h / 2.0);
        let luma = (0.299 * f32::from(col.r) + 0.587 * f32::from(col.g) + 0.114 * f32::from(col.b)) / 255.0;
        let ink = if luma > 0.55 { Color::rgb(0x473900) } else { Color::rgb(0xffffff).with_alpha(200) };
        ui.c.line(cx - 3.0, cy - 1.5, cx, cy + 1.5, 1.5, ink);
        ui.c.line(cx, cy + 1.5, cx + 3.0, cy - 1.5, 1.5, ink);
        return;
    }
    let tab = Rect::new(r.right() - 22.0, r.y + 4.0, 18.0, r.h - 8.0);
    let hov = ui.hit(tab, Action::Pat(Hit::Color), hint);
    // The tab's outline: a body with a point at the middle of its left side.
    let shape = |k: f32| {
        let (x0, x1, y0, y1, cy) = (tab.x + 6.0 + k, tab.right() - k, tab.y + k, tab.bottom() - k, tab.y + tab.h / 2.0);
        let p = 6.0 - k * 0.6;
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, cy + p), (tab.x + k * 1.6, cy), (x0, cy - p)]
    };
    let mut g = Glyph::new();
    g.polygon(&shape(0.0));
    ui.c.glyph(if hov { t.glyph } else { Color::rgb(0x0e1012) }, &g);
    let mut g = Glyph::new();
    g.polygon(&shape(1.0));
    ui.c.glyph(if hov { col.shade(0.12) } else { col }, &g);
    // Lit top edge and a darker bottom edge on the body, like Classic's raised faces.
    ui.c.fill_rect(Rect::new(tab.x + 7.0, tab.y + 1.0, tab.w - 8.0, 1.0), col.shade(0.25));
    ui.c.fill_rect(Rect::new(tab.x + 7.0, tab.bottom() - 2.0, tab.w - 8.0, 1.0), col.shade(-0.25));
}

pub fn paint(ui: &mut Ui, s: &State) {
    let t = ui.t;
    let font = Font::new(14);
    let lemon = super::lemon(ui);
    let rb = if lemon { 5.0 } else { 2.0 };
    if let Some(b) = s.pat_ui.name_box.as_ref() {
        let r = Rect::new(b.at.0, b.at.1, BOX_W, BOX_H);
        panel(ui, r, Color::rgb(BG));
        ui.hit(r, Action::Pat(Hit::Inside), "");
        let named = matches!(b.target, BoxTarget::Name(_));
        // The colour tab: patterns and channels (spec: ui/channel-rack.md §13).
        let tabbed = named || matches!(b.target, BoxTarget::Rack(crate::rack::RBox::Channel(_)));
        let field = Rect::new(r.x + 4.0, r.y + 4.0, r.w - if !tabbed { 8.0 } else if lemon { 34.0 } else { 30.0 }, 22.0);
        if lemon {
            ui.c.fill_rrect(field, 4.0, t.tempo_box);
        } else {
            ui.c.fill_rect(field, t.tempo_box);
        }
        let tw = ui.c.text_width(font, &b.text);
        if b.fresh && !b.text.is_empty() {
            ui.c.fill_rect(Rect::new(field.x + 3.0, field.y + 3.0, tw + 2.0, 16.0), Color::rgb(0x3d6a8a));
        }
        ui.c.text_clip(font, field.x + 4.0, field.y + 3.0, field.w - 6.0, t.glyph, &b.text);
        if !b.fresh {
            let cx = (field.x + 5.0 + tw).min(field.right() - 2.0);
            ui.c.fill_rect(Rect::new(cx, field.y + 3.0, 1.0, 16.0), t.glyph);
        }
        let row = r.y + 29.0;
        let mut x = r.x + 4.0;
        if named {
            let a = Rect::new(x, row, 18.0, 18.0);
            let hov = ui.hit(a, Action::Pat(Hit::Suggestions), "Name suggestions");
            ui.c.fill_rrect(a, rb, if hov { t.menu_pop_hi } else { t.menu_pop });
            crate::icons::draw(ui.c, crate::icons::Icon::ArrowRight, a.inset(3.0), t.glyph);
            x += 20.0;
        }
        let ok = Rect::new(x, row, 18.0, 18.0);
        let hov = ui.hit(ok, Action::Pat(Hit::Accept), "Accept");
        ui.c.fill_rrect(ok, rb, if hov { t.menu_pop_hi } else { t.menu_pop });
        tick(ui, ok, t.glyph);
        let no = Rect::new(x + 20.0, row, 18.0, 18.0);
        let hov = ui.hit(no, Action::Pat(Hit::Cancel), "Cancel");
        ui.c.fill_rrect(no, rb, if hov { t.menu_pop_hi } else { t.menu_pop });
        cross(ui, no, t.glyph);
        let caption = match b.target {
            BoxTarget::Name(n) => format!("Pattern {n} name"),
            BoxTarget::Transpose(_) => "Semitones".to_string(),
            BoxTarget::Rack(rb) => crate::rack::box_caption(s, rb),
        };
        ui.c.text_clip(Font::new(12), x + 44.0, row + 2.0, r.right() - x - 74.0, t.glyph_dim, &caption);
        let tab_col = match b.target {
            BoxTarget::Name(n) => Some(s.project.pattern(n).and_then(|p| p.color)),
            BoxTarget::Rack(crate::rack::RBox::Channel(r)) => Some(s.project.channels.get(r).and_then(|c| c.color)),
            _ => None,
        };
        if let Some(col) = tab_col {
            let col = col.map_or(Color::rgb(0x925755), rgb);
            let hint = if named { "Pattern colour" } else { "Channel colour" };
            colour_tab(ui, r, col, hint);
        }
        if b.suggestions {
            let cols: Vec<Vec<String>> = std::iter::once(s.pat_ui.recent.clone())
                .chain(WORDS.iter().map(|c| c.iter().map(|w| w.to_string()).collect()))
                .collect();
            let rows = cols.iter().map(Vec::len).max().unwrap_or(0) as f32;
            let p = Rect::new(r.x, r.bottom(), SUG_W * cols.len() as f32 + 2.0, rows * SUG_ROW + 2.0);
            panel(ui, p, t.menu_pop);
            ui.hit(p, Action::Pat(Hit::Inside), "");
            for (ci, col) in cols.iter().enumerate() {
                let cx = p.x + 1.0 + ci as f32 * SUG_W;
                if ci > 0 {
                    ui.c.fill_rect(Rect::new(cx - 1.0, p.y + 1.0, 1.0, p.h - 2.0), t.menu_pop_head);
                }
                for (ri, w) in col.iter().enumerate() {
                    let rr = Rect::new(cx, p.y + 1.0 + ri as f32 * SUG_ROW, SUG_W - 1.0, SUG_ROW);
                    if ui.hit(rr, Action::Pat(Hit::Suggest(ci, ri)), "Use this name") {
                        if lemon {
                            os_ui::kit::row_highlight(ui.c, t, rr);
                        } else {
                            ui.c.fill_rect(rr, t.menu_pop_hi);
                        }
                    }
                    ui.c.text_clip(Font::new(13), rr.x + 6.0, rr.y + 1.0, rr.w - 8.0, t.menu_pop_text, w);
                }
            }
        }
        if b.palette {
            let custom = crate::workspace_visibility::shown("visible_playlist-custom-swatches");
            let w = SW * 10. + 2.;
            let h = SW * 2. + 2. + if !custom { 0. } else if b.custom.is_some() { 68. } else { 43. };
            let y = if r.bottom() + h > s.screen.1 && r.y >= h { r.y - h } else { r.bottom() };
            let p = Rect::new((r.right() - w).clamp(0., (s.screen.0 - w).max(0.)), y, w, h);
            ui.c.fill_rect(p, t.menu_pop);
            ui.hit(p, Action::Pat(Hit::Inside), "");
            for (i, &c) in SWATCHES.iter().enumerate() {
                let sr = Rect::new(p.x + 1.0 + (i % 10) as f32 * SW, p.y + 1.0 + (i / 10) as f32 * SW, SW, SW);
                let hov = ui.hit(sr, Action::Pat(Hit::Swatch(i)), "Set the colour");
                ui.c.fill_rect(sr.inset(2.0), Color::rgb(c));
                if hov {
                    ui.c.stroke_rrect(sr.inset(1.0), 0.0, 1.0, t.glyph);
                }
            }
            if custom {
                custom_swatches(ui, p, b.custom.as_ref());
            }
        }
    }
    if s.pat_ui.confirm {
        let (w, h) = (380.0, 120.0);
        let r = Rect::new(((s.screen.0 - w) / 2.0).round(), ((s.screen.1 - h) / 2.0).round(), w, h);
        ui.c.fill_rect(Rect::new(0.0, 0.0, s.screen.0, s.screen.1), Color::rgb(0).with_alpha(80));
        ui.hit(Rect::new(0.0, 0.0, s.screen.0, s.screen.1), Action::Swallow, "");
        panel(ui, r, Color::rgb(BG));
        ui.hit(r, Action::Pat(Hit::Inside), "");
        ui.c.text(Font::new(13), r.x + 12.0, r.y + 8.0, t.glyph_dim, "Confirm");
        // spec: ui/channel-rack.md §18 (channels), ui/pattern-menu.md (patterns)
        let msg = match &s.rack.pending_delete {
            // spec: ui/unused-channels.md §4
            Some(rows) if s.rack.purging => format!("{} unused audio clip channel(s) are about to be deleted.", rows.len()),
            Some(rows) if rows.len() == 1 => "The selected channel is about to be deleted.".to_string(),
            Some(rows) => format!("The {} selected channels are about to be deleted.", rows.len()),
            None => format!("{} is about to be deleted.", pt::name_of(s, s.pattern.max(1))),
        };
        ui.c.text_clip(font, r.x + 12.0, r.y + 40.0, r.w - 24.0, t.glyph, &msg);
        // spec: ui/settings-choices.md "Hints and warnings: Manage messages and Project data size warning (2026-10-16)"
        super::remember_check(ui, r.x + 12.0, r.bottom() - 22.0, s.pat_ui.remember, Action::Pat(Hit::ConfirmRemember));
        for (i, (label, hit)) in [("Ok", Hit::ConfirmOk), ("Cancel", Hit::ConfirmCancel)].into_iter().enumerate() {
            let br = Rect::new(r.right() - 180.0 + i as f32 * 86.0, r.bottom() - 34.0, 80.0, 24.0);
            let hov = ui.hit(br, Action::Pat(hit), label);
            ui.c.fill_rrect(br, rb, if hov { t.menu_pop_hi } else { t.menu_pop });
            let lw = ui.c.text_width(font, label);
            ui.c.text(font, br.x + (br.w - lw) / 2.0, br.y + 4.0, t.glyph, label);
        }
    }
}
