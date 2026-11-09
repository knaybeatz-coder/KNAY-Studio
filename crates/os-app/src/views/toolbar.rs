// Copyright (C) 2026 ItsAlphaNeon
// Copyright (C) 2026 thacer13
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The toolbar rows, their panels wherever the layout puts them (`crate::toolbar`), and the editor's palette.
// spec: ui/main-window.md §1–3, ui/toolbar-menu.md §8

use std::ops::Range;

use crate::workspace_visibility::{shown, WINDOW_BUTTONS};

use os_ui::{Color,Font, Rect};

use super::{menus, sunken, tool_button, FrameInfo};
use crate::app::{Action, Live, State, Ui, Win};
use crate::icons::{self, Icon};
use crate::toolbar::{place, At, BarHit, Moving, Panel, Placed, Target, Tile, PANELS, ROW_H};

/// The ▶ arrow's width at the band's right end.
pub const MORE_W: f32 = 18.0;
/// The overflow arrow provides access to windows hidden by a narrow toolbar.
pub const MORE_HINT: &str = "More windows and toolbar options";

/// Where the panels of a band `width` px wide must end to show, when all of them need `full` (`toolbar::natural_width`;
/// 1440 for the classic layout, whose last utility button ends at x 1437): anywhere when all of them fit, else before
/// the arrow. A narrower band leaves out the panels that don't fit, from the right of each row, and draws the ▶ arrow
/// at its right end. Spec: ui/ui-scaling.md §1 "Toolbars scaling".
pub fn room(width: f32, full: f32) -> f32 {
    if width >= full { f32::INFINITY } else { width - MORE_W }
}

/// `room` for the band of `s`, `width` px wide.
pub fn fit(s: &State, width: f32) -> f32 {
    room(width, crate::toolbar::natural_width(&s.bar.layout, s.bar.editing))
}

/// The Toolbars menu's switches (spec: ui/toolbar-menu.md §8): app-wide, kept in `settings.txt` across runs but for
/// Hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opts {
    /// Group and row separator lines (§2).
    pub separators: bool,
    /// Square buttons without face and edges (§3).
    pub flat: bool,
    /// The compact hint field after Project Info.
    pub hint_bar: bool,
    /// Toolbar and menu bar hidden, the browser and workspace up to the window's top (§5).
    pub hidden: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { separators: true, flat: false, hint_bar: true, hidden: false }
    }
}

impl Opts {
    /// The switches kept in `settings.txt` (`toolbar_separators`, `toolbar_flat_buttons`, `toolbar_hint_bar`); the
    /// default for one that is missing. Hide is not kept: a start shows the toolbar.
    pub fn load() -> Opts {
        use crate::settings_file::flag;
        let d = Opts::default();
        Opts {
            separators: flag("toolbar_separators", d.separators),
            flat: flag("toolbar_flat_buttons", d.flat),
            hint_bar: flag("toolbar_inline_hint", d.hint_bar),
            hidden: false,
        }
    }

    /// Keeps the switches but Hide for the next start.
    pub fn save(&self) {
        use crate::settings_file::set_flag;
        set_flag("toolbar_separators", self.separators);
        set_flag("toolbar_flat_buttons", self.flat);
        set_flag("toolbar_inline_hint", self.hint_bar);
    }
}

/// The Toolbars menu entries that work (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// The toolbar editor on / off.
    Edit,
    Separators,
    Flat,
    HintBar,
    Tesla,
    CpuMemory,
    Hide,
}

/// The toolbar's rows, the bottom edge excepted, in a band `width` px wide: one band, repainted whole while the
/// separators are off (their contents are moved by a pixel after painting, `paint`).
pub fn band(s: &State, width: f32) -> Rect {
    Rect::new(0.0, 0.0, width, display_layout(s).height())
}

fn btn(x: f32, y: f32) -> Rect {
    Rect::new(x, y, 30.0, 33.0)
}

/// What a panel paints with: where it is, where the row's room ends (`room`, band px) and whether it is a palette
/// tile's picture (`preview`: nothing it paints may change the state, such as the tempo digits' places).
struct Ctx<'a> {
    s: &'a State,
    f: &'a FrameInfo,
    a: At,
    fit: f32,
    preview: bool,
}

pub(crate) fn visible_layout(s:&State)->crate::toolbar::Layout {
    let mut layout = if s.vintage_2005 {
        use Panel::*;
        crate::toolbar::Layout { rows: vec![
            vec![Caption, Hint, Recording, Volume, Pitch, Snap, Windows],
            vec![Menu, Transport, Time, Pattern, Utilities],
        ], logo: false }
    } else { s.bar.layout.clone() };
    if !s.bar.editing {
        layout.logo &= shown("visible_logo");
        for row in &mut layout.rows {
            row.retain(|panel| match panel {
                Panel::Caption => s.vintage_2005,
                Panel::Hint => s.vintage_2005,
                Panel::Cpu => false,
                Panel::Menu | Panel::Transport | Panel::Space | Panel::Flex => true,
                Panel::Windows => WINDOW_BUTTONS[..5].iter().any(|key| shown(key)),
                Panel::Tools => WINDOW_BUTTONS[5..].iter().any(|key| shown(key)),
                panel => shown(&format!("visible_{}", panel.key())),
            });
            if row.contains(&Panel::Utilities) && row.contains(&Panel::Windows) {
                row.retain(|p| *p != Panel::Utilities);
                let at = row.iter().position(|p| *p == Panel::Windows).unwrap();
                row.insert(at, Panel::Utilities);
            }
        }
    }
    layout
}

pub(super) fn display_layout(s: &State) -> &crate::toolbar::Layout {
    s.toolbar_display_layout.as_ref().unwrap_or(&s.bar.layout)
}

/// Wrap whole panels after reaching the scale floor. Only the first row shares
/// space with the fixed actions and window controls.
pub(crate) fn wrap(mut layout: crate::toolbar::Layout, width: f32, actions: f32) -> crate::toolbar::Layout {
    let mut rows = Vec::new();
    for original in layout.rows {
        let mut row = Vec::new();
        let mut used = 0.0;
        for panel in original {
            let available = if rows.is_empty() { width - actions } else { width };
            if used + panel.width() > available && (!row.is_empty() || rows.is_empty()) {
                rows.push(std::mem::take(&mut row));
                used = 0.0;
            }
            used += panel.width();
            row.push(panel);
        }
        rows.push(row);
    }
    layout.rows = rows;
    layout
}

pub(super) fn placements(s: &State, width: f32) -> Vec<Placed> {
    let mut panels = place(display_layout(s), (width - s.toolbar_actions.width(s.vintage_2005)).max(0.0), s.bar.editing);
    if s.toolbar_actions.hint > 0.0 {
        let right_inset = if s.vintage_2005 { 20.0 } else { 124.0 };
        panels.push(Placed { panel: Panel::Hint, row: 0, idx: 0,
            x: width - right_inset - Panel::Hint.width(), w: Panel::Hint.width() });
    }
    panels
}

/// Measured once so the fit calculation and the painted buttons reserve identical space.
#[derive(Clone, Copy, Default)]
pub struct ActionSizes {
    ai: f32,
    update: f32,
    tesla: f32,
    donate: f32,
    feedback: f32,
    cpu: f32,
    scope: f32,
    hint: f32,
}

impl ActionSizes {
    pub fn measure(canvas: &mut os_ui::Canvas, hint: bool, ai_active: bool, vintage: bool) -> Self {
        let font = Font::new(12);
        let button = |canvas: &mut os_ui::Canvas, key, font, label| {
            if shown(key) { canvas.text_width(font, label).ceil() + 18.0 } else { 0.0 }
        };
        Self {
            ai: if ai_active || shown("visible_ai") { 43.0 } else { 0.0 },
            update: button(canvas, "visible_update", font, "Update"),
            tesla: if shown("visible_tesla") && crate::settings_file::flag("toolbar_tesla", false) { 30.0 } else { 0.0 },
            donate: button(canvas, "visible_donate", font, "Donate"),
            feedback: button(canvas, "visible_feedback", Font::new(11), "Report a bug"),
            hint: if hint && shown("visible_hint") { Panel::Hint.width() + 6.0 } else { 0.0 },
            scope: if vintage && shown("visible_vintage_scope") { 145.0 } else { 0.0 },
            cpu: if vintage {
                if shown("visible_vintage_cpu") { 107.0 } else { 0.0 }
            } else if !shown("visible_cpu") || !crate::settings_file::flag("toolbar_cpu_visible", true) { 0.0 }
                 else if crate::settings_file::flag("toolbar_cpu_combined",true) { 245. }
                 else if crate::settings_file::flag("toolbar_cpu_classic",false) { crate::app::CPU_RECT.w+6.0 }
                 else { 54.0 },
        }
    }

    pub fn meters_width(self) -> f32 { self.cpu + self.scope }
    pub fn combined(self)->bool{self.cpu==245.}
    pub fn width(self, vintage: bool) -> f32 {
        self.ai + self.tesla + self.cpu + self.scope + self.hint + self.update + self.donate + self.feedback + 38.0 + if vintage { 0.0 } else { 104.0 }
    }
}

/// The band in its own px, `width` of them wide (`views::paint` scales it by Toolbars scaling). In each row the panels
/// that would end past `room` are left out, from the first one that does not fit.
pub fn paint(ui: &mut Ui, s: &State, f: &FrameInfo, width: f32) {
    let t = ui.t;
    let lemon = super::lemon(ui);
    let editing = s.bar.editing;
    let l = display_layout(s);
    let action_font = if s.vintage_2005 {Font::vintage(11)}else{Font::new(12)};
    let sizes = s.toolbar_actions;
    let window_w = 104.0;
    let mini_cpu_w = sizes.cpu;
    let show_cpu = mini_cpu_w > 0.0;
    let native_vintage = super::vintage_toolbar::active(s, width);
    let actions_w = sizes.width(s.vintage_2005) - if native_vintage { sizes.meters_width() } else { 0.0 };
    let content_width = (width - actions_w).max(100.0);
    s.toolbar_content_width.set(content_width);
    let fit = if editing { room(content_width, crate::toolbar::natural_width(l, true)) } else { f32::INFINITY };
    let (rows, h) = (l.rows.len(), l.height());
    // Separator lines: off from the Toolbars menu (spec: ui/toolbar-menu.md §2).
    let seps = s.toolbar.separators;
    // Bands.
    if native_vintage {
        ui.c.fill_rect(Rect::new(0.0, 0.0, width, h), t.toolbar);
        for y in [0.0, 40.0] {
            crate::vintage_skin::draw(ui.c, "RecordRail", 0, Rect::new(0.0, y, width, 40.0));
        }
    } else if lemon {
        // Lemon (spec: ui/lemon-design.md §4): flat bands, short rounded separators.
        ui.c.fill_rect(Rect::new(0.0, 0.0, width, h), t.toolbar);
        for r in (1..rows).filter(|_| seps) {
            ui.c.fill_rect(Rect::new(0.0, ROW_H * r as f32, width, 1.0), t.toolbar_line);
        }
    } else {
        for r in 0..rows {
            let (top, bottom) = l.row_span(r);
            let (a, b) = if r == 0 { (0.025, -0.015) } else { (0.02, -0.02) };
            ui.c.fill_rect_gradient(Rect::new(0.0, top, width, bottom - top), t.toolbar.shade(a), t.toolbar.shade(b), 0.0);
            if t.style.vintage_2005 {
                ui.c.fill_rect(Rect::new(0.0, top, width, 1.0), t.toolbar_edge);
                ui.c.fill_rect(Rect::new(0.0, bottom - 1.0, width, 1.0), t.toolbar_line);
            }
            if r > 0 {
                let line = if seps { t.toolbar_line } else { t.toolbar.shade(-0.015) };
                ui.c.fill_rect(Rect::new(0.0, ROW_H * r as f32, width, 1.0), line);
            }
        }
    }
    ui.c.fill_rect(Rect::new(0.0, h, width, 2.0), t.toolbar_line);
    // Empty toolbar areas drag the window (registered first, so controls win).
    ui.hit(Rect::new(0.0, 0.0, width, h), Action::DragWindow, "");

    let placed = placements(s, width);
    if native_vintage {
        super::vintage_toolbar::paint(ui,s,f);
    } else {
    for row in 0..rows {
        for p in placed.iter().filter(|p| p.row == row) {
            let a = p.at();
            let shows = if p.panel==Panel::Hint {p.x+64.<=fit} else if p.panel.repeats() { p.x + p.w <= fit } else { a.x(p.panel.first_end()) <= fit };
            if !shows {
                break;
            }
            // Group separators (2 px gaps measured in row 1 and row 2), each at the left edge of the panel it comes
            // with, shown with that panel; none at a row's start.
            if (seps || t.style.vintage_2005) && p.panel.separator() && p.idx > 0 {
                separator(ui, p.x, p.row, lemon);
            }
            if s.vintage_2005 && !matches!(p.panel,Panel::Space|Panel::Flex) {
                let (top,bottom)=l.row_span(row);
                crate::vintage::metal(ui.c,Rect::new(p.x+1.,top+2.,p.w-2.,bottom-top-4.),t.toolbar);
            }
            let mark = ui.mark();
            let all = panel(ui, &Ctx { s, f, a, fit, preview: false }, p.panel);
            if editing {
                ui.forget(mark);
                item_frame(ui, l, p);
            }
            if !all {
                break;
            }
        }
    }
    }
    // Without separators row 1's contents move 1 px down and row 2's 1 px up, over the row line (§2). The pixels are
    // moved after painting; the band is always repainted whole then (`App::take_damage`).
    if !seps && rows == 2 && !s.vintage_2005 {
        let d = ui.c.scale.round().max(1.0) as i32;
        ui.c.shift(Rect::new(0.0, 0.0, width, 42.0), 0, d, &[]);
        ui.c.shift(Rect::new(0.0, 41.0, width, 40.0), 0, -d, &[]);
    }
    if editing {
        grips(ui, s);
    }

    {
        let cr=Rect::new(width-actions_w+2.,8.,48.,24.);
        if native_vintage {
            // The original upper shelf already owns the vintage meters.
        } else if s.vintage_2005 {
            let mut x = cr.x;
            if show_cpu {
                super::vintage_toolbar::cpu(ui, s, x, 0.0);
                x += mini_cpu_w;
            }
            if sizes.scope > 0.0 {
                super::vintage_toolbar::scope(ui, s, x, 0.0);
            }
        } else if sizes.combined() {live_monitor(ui,s,Rect::new(cr.x,3.,mini_cpu_w-6.,34.));}
        else if mini_cpu_w > 54.0 {
            let a=At{dx:cr.x-crate::app::CPU_RECT.x,dy:0.};
            classic_cpu(ui,s,a);
        } else if show_cpu {ui.c.fill_rrect(cr,3.,t.sunken);
        ui.hit(cr,Action::CpuPanel,s.cpu.hint());
        let load=s.cpu.now.load;
        let ink=if load>=100{t.meter_clip}else if load>=75{t.meter_high}else{t.text_dim};
        ui.c.text_center(Font::new(10),Rect::new(cr.x,cr.y+2.,cr.w,15.),ink,&format!("CPU {}%",load.min(999)));
        ui.c.fill_rect(Rect::new(cr.x+4.,cr.bottom()-5.,40.,2.),t.toolbar_line);
        ui.c.fill_rect(Rect::new(cr.x+4.,cr.bottom()-5.,40.*(load as f32/100.).min(1.),2.),ink);
        }
        let mut x = cr.x + if native_vintage { 0.0 } else { sizes.meters_width() };
        if sizes.ai > 0.0 {
            let ai = Rect::new(x, 8.0, sizes.ai - 6.0, 24.0);
            let hover = ui.hit(ai, Action::Ai, if !s.ai.enabled {"AI control is off — connect an assistant"} else if s.ai.client.is_empty() {"AI control enabled — waiting for an approved assistant"} else {"AI assistant connected — manage or disconnect"});
            ui.c.fill_rrect(ai, 3.0, if hover { t.button_hi } else { t.sunken });
            let chip = Rect::new(ai.x + 6.0, ai.y + 7.0, 10.0, 10.0);
            let ink = if s.ai.enabled { t.accent } else { t.text_dim };
            ui.c.stroke_rrect(chip, 2.0, 1.0, ink);
            for d in [2.0, 5.0, 8.0] {
                ui.c.line(chip.x - 2.0, chip.y + d, chip.x, chip.y + d, 1.0, ink);
                ui.c.line(chip.right(), chip.y + d, chip.right() + 2.0, chip.y + d, 1.0, ink);
            }
            ui.c.text_clip(action_font, ai.x + 22.0, ai.y + 5.0, 20.0, t.text, "AI");
            if !s.ai.client.is_empty() {
                ui.c.fill_circle(ai.right() - 3.0, ai.y + 3.0, 2.0, t.accent);
            }
            x += sizes.ai;
        }
        if sizes.tesla > 0.0 {
            let b = Rect::new(x, 8.0, sizes.tesla - 6.0, 24.0);
            ui.hit(b, Action::Util(crate::app::Util::AudioEditor), "Open Tesla audio editor");
            ui.c.fill_rrect(b, 3.0, t.sunken);
            icons::draw(ui.c, Icon::Wave, b.inset(4.0), t.accent);
            x += sizes.tesla;
        }
        if sizes.update > 0.0 {
            let r = Rect::new(x, 8.0, sizes.update - 6.0, 24.0);
            let (label, hint) = crate::updates::display();
            ui.hit(r, Action::Updates, if hint.is_empty() { "Automatic updates: check now".to_string() } else { hint });
            ui.c.fill_rrect(r, 3.0, t.sunken);
            ui.c.text_clip(action_font, r.x + 6.0, r.y + 5.0, r.w - 12.0, t.text, label);
            x += sizes.update;
        }
        if sizes.donate > 0.0 {
            let r = Rect::new(x, 8.0, sizes.donate - 6.0, 24.0);
            ui.hit(r, Action::Donate, "Donate through Michael Lane's PayPal.me page");
            ui.c.fill_rrect(r, 3.0, t.sunken);
            ui.c.text_clip(action_font, r.x + 6.0, r.y + 5.0, r.w - 12.0, t.text, "Donate");
            x += sizes.donate;
        }
        if sizes.feedback > 0.0 {
            let r = Rect::new(x, 8.0, sizes.feedback - 6.0, 24.0);
            ui.hit(r, Action::FeatureVote(0, 30), "Report a bug or send feedback: review technical details before submitting");
            ui.c.fill_rrect(r, 3.0, t.accent);
            ui.c.text_center(Font::new(11), r, t.sunken, "Report a bug");
        }
    }
    if !s.vintage_2005 {
        // Window controls remain at the far right, outside the customizable panels.
        ui.c.fill_rect(Rect::new(width-window_w-17.,11.,1.,18.),t.toolbar_line);
        for (offset, icon, action, hint) in [
            (0., Icon::Minimize, Action::Minimize, "Minimize"),
            (32., Icon::Restore, Action::Maximize, "Restore / maximize"),
            (64., Icon::Close, Action::Close, "Exit Lemon Studio"),
        ] {
            let r=Rect::new(width-window_w+offset,8.,30.,24.);
            let hover=ui.hit(r,action,hint);
            super::button_face(ui,r,if hover {super::Look::Hover}else{super::Look::Off});
            icons::draw(ui.c,icon,r.inset(6.),t.glyph);
        }
    }
    // Panels left out (spec: ui/ui-scaling.md §1 "Toolbars scaling"): the ▶ arrow at the band's right end, in the logo's
    // place. Open View so hidden window buttons remain accessible.
    let mid = (h / 2.0).floor();
    if fit.is_finite() {
        let r = Rect::new(content_width - MORE_W, 0.0, MORE_W, h);
        ui.hit(r, Action::MenuSub(menus::VIEW,11), "Toolbar options · some controls are hidden at this width");
        ui.c.text_center(Font::new(14),r,t.glyph,"…");
        return;
    }
    if !l.logo {
        return;
    }
    // Logo spot (spans the rows): the Lemon Studio mark, just right of the last panel (x 1487 in the classic layout). It
    // keeps to the right edge until the window is too narrow, then the edge crops it (spec ui/main-window.md §12); the
    // toolbar never moves.
    let end = placed.iter().map(|p| p.x + p.w).fold(0.0, f32::max);
    let lx = (content_width - 45.0).max(end + 47.0);
    let r = Rect::new(lx - 35.0, 0.0, 78.0, h - 1.0);
    if editing {
        ui.hit(r, Action::Bar(BarHit::Logo), "Logo - drag onto the palette to take it off");
        frame(ui, r.inset(2.0));
    } else {
        ui.hit(r, Action::MenuItem(menus::HELP_MENU, menus::HELP_ABOUT), "About Lemon Studio");
    }
    icons::logo(ui.c, lx, mid, 26.0 * h / 81.0, if lemon { t.toolbar_edge } else { t.toolbar_line });
}

/// A group separator at band x `x` of row `row`.
fn separator(ui: &mut Ui, x: f32, row: usize, lemon: bool) {
    let t = ui.t;
    let top = ROW_H * row as f32;
    if lemon {
        ui.c.fill_rrect(Rect::new(x, top + 9.0, 2.0, 23.0), 1.0, t.toolbar_line);
        return;
    }
    let (y, h) = if row == 0 { (2.0, 37.0) } else { (top + 3.0, 35.0) };
    ui.c.fill_rect(Rect::new(x, y, 1.0, h), t.toolbar_line);
    ui.c.fill_rect(Rect::new(x + 1.0, y, 1.0, h), t.toolbar_edge.with_alpha(110));
}

/// Paints panel `p`; false when one of its buttons did not fit (its row ends there).
fn panel(ui: &mut Ui, c: &Ctx, p: Panel) -> bool {
    match p {
        Panel::Caption => { if c.preview || c.s.vintage_2005 { caption(ui, c); } },
        Panel::Hint => hint_panel(ui, c),
        Panel::Transport => transport(ui, c),
        Panel::Recording => return recording(ui, c),
        Panel::Time => time(ui, c),
        Panel::Monitor => monitor(ui, c),
        Panel::Cpu => classic_cpu(ui,c.s,c.a),
        Panel::Utilities => return utilities(ui, c),
        Panel::Menu => menu_bar(ui, c),
        Panel::Volume => volume(ui, c),
        Panel::Pitch => pitch(ui, c),
        Panel::SongPos => song_pos(ui, c),
        Panel::Modes => return modes(ui, c),
        Panel::Snap => snap(ui, c),
        Panel::Pattern => pattern(ui, c),
        Panel::Windows => return window_buttons(ui, c, 0..5),
        Panel::Tools => return window_buttons(ui, c, 5..9),
        Panel::Space | Panel::Flex => {}
    }
    true
}

/// Caption buttons.
fn caption(ui: &mut Ui, c: &Ctx) {
    let (t, a) = (ui.t, c.a);
    for (x, icon, action, hint) in [
        (4.0, Icon::Minimize, Action::Minimize, "Minimize"),
        (24.0, Icon::Restore, Action::Maximize, "Restore / maximize"),
        (44.0, Icon::Close, Action::Close, "Exit Lemon Studio"),
    ] {
        let r = a.r(Rect::new(x, 11.0, 19.0, 19.0));
        let hov = ui.hit(r, action, hint);
        if c.s.vintage_2005 {
            let frame = match icon { Icon::Minimize => 0, Icon::Restore => 2, _ => 4 };
            if crate::vintage_skin::native(ui.c, "MinClose", frame + usize::from(hov), (r.cx()-7.0).round(), (r.cy()-7.5).round()) {
                continue;
            }
        }
        super::button_face(ui, r, if hov { super::Look::Hover } else { super::Look::Off });
        icons::draw(ui.c, icon, r.inset(3.5), if hov { ui.t.tx(0xffffff) } else { t.glyph });
    }
}

/// Two-line hints; project/template names belong in the project title, not here.
fn hint_panel(ui: &mut Ui, c: &Ctx) {
    let (t, a, s, f) = (ui.t, c.a, c.s, c.f);
    let mut hp = a.span(72.0, 72.0+Panel::Hint.width()-7., 4.0, 36.0);
    hp.w=hp.w.min((c.fit-hp.x-2.).max(0.));
    ui.hit(hp, Action::DragWindow, "Hint bar");
    if s.vintage_2005 && hp.x < 150.0 {
        let lamps = shown("visible_vintage_activity") && hp.w >= 100.0;
        let info = Rect::new(hp.x, hp.y, hp.w - if lamps { 29.0 } else { 0.0 }, hp.h);
        super::vintage_toolbar::compact_info(ui, s, f, info);
        if lamps {
            super::vintage_toolbar::activity(ui, s, f, hp.right() - 10.0, hp.y + 7.0, false);
        }
        return;
    }
    sunken(ui, hp, t.sunken);
    let hint: &str = if !f.hint.is_empty() { &f.hint } else { s.message.as_deref().unwrap_or(if t.style.vintage_2005 { "LEMON STUDIO  /  2005" } else { "" }) };
    let mut font=Font::new(11);
    for size in (9..=11).rev() {
        font=Font::new(size);
        if ui.c.line_height(font)*2.<=hp.h-4. {break;}
    }
    let line_height=ui.c.line_height(font);
    ui.c.push_clip(hp.inset(2.));
    let chars:Vec<char>=hint.chars().collect();
    let lines=super::settings::wrap(ui,font,hint,hp.w-10.);
    for (row,(start,n)) in lines.into_iter().take(2).enumerate() {
        ui.c.text_clip(font,hp.x+5.,hp.y+2.+row as f32*line_height,hp.w-10.,t.text,&chars[start..start+n].iter().collect::<String>());
    }
    ui.c.pop_clip();
}

/// Transport group: PAT / SONG, play, stop, record and the tempo (to the tempo box's right edge, x 572).
fn transport(ui: &mut Ui, c: &Ctx) {
    if c.s.vintage_2005 && !c.preview && !c.s.bar.editing && super::vintage_toolbar::transport_available() {
        super::vintage_toolbar::transport(ui, c.s, c.f, c.a.x(379.0), c.a.y(0.0));
        return;
    }
    let (t, a, s, f) = (ui.t, c.a, c.s, c.f);
    let lemon = super::lemon(ui);
    let capsule = a.span(383.0, 483.0, 4.0, 36.0);
    if t.style.vintage_2005 {
        os_ui::kit::sunken(ui.c, t, capsule, t.sunken_dark);
    } else if lemon {
        // Lemon: one rounded-square tray, each section a rounded square inside it.
        os_ui::kit::lemon::outlined(ui.c, capsule, 7.0, t.toolbar_line.shade(-0.15), t.sunken);
    } else {
        ui.c.fill_rrect(capsule, 15.0, t.toolbar_line.shade(-0.2));
        ui.c.fill_rect_gradient(capsule.inset(1.0), t.sunken.shade(0.03), t.sunken.shade(-0.05), 14.0);
        ui.c.fill_rect(a.r(Rect::new(418.0, 6.0, 1.0, 29.0)), t.toolbar_line);
        ui.c.fill_rect(a.r(Rect::new(449.0, 6.0, 1.0, 29.0)), t.toolbar_line);
    }
    // PAT / SONG fill the whole top / bottom half of the capsule's left section.
    let pat = a.span(384.0, 418.0, 5.0, 20.0);
    let song = a.span(384.0, 418.0, 20.0, 35.0);
    ui.hit(pat, Action::PatSong(false), "Pattern mode: play the current pattern");
    ui.hit(song, Action::PatSong(true), "Song mode: play the playlist");
    for (r, label, active) in [(pat, "PAT", !s.song_mode), (song, "SONG", s.song_mode)] {
        if t.style.vintage_2005 {
            os_ui::kit::styled_surface(ui.c, t, r.inset(1.0), 0.0, t.window_border,
                if active { t.on_top } else { t.button }, t.button,
                os_theme::appearance::Finish::Beveled);
        } else if active && lemon {
            os_ui::kit::lemon::lit_in(ui.c, ui.t, Rect::new(r.x + 2.0, r.y + 1.0, r.w - 3.0, r.h - 1.5), 4.0, false);
        } else if active {
            ui.c.push_clip(r);
            ui.c.fill_rrect(capsule.inset(1.0), 14.0, t.pill);
            ui.c.pop_clip();
        }
        ui.c.text_center(if s.vintage_2005 {Font::vintage(9)}else{Font::condensed(10, 5)}, Rect::new(r.x + 1.0, r.y, r.w, r.h), if active { t.pill_text } else { t.glyph_dim }, label);
    }
    // Lit while playing, with a pause glyph (Play pauses); unlit with the play glyph when paused.
    let play_on = f.playing && !f.paused;
    for (r, icon, on, action, hint) in [
        (a.span(419.0, 449.0, 5.0, 35.0), Icon::Play, play_on, Action::Play, "Play / pause (Space plays / stops)"),
        (a.span(450.0, 482.0, 5.0, 35.0), Icon::Stop, false, Action::Stop, "Stop (Space)"),
    ] {
        let hov = ui.hit(r, action.clone(), hint);
        let on = on || ui.down(r, &action);
        // Lit / hovered sections fill their whole slot of the capsule (clipped to its round end).
        if t.style.vintage_2005 {
            let face = if on { t.on_top } else if hov { t.button_hi } else { t.button };
            os_ui::kit::styled_surface(ui.c, t, r.inset(1.0), 0.0, t.window_border, face, face,
                os_theme::appearance::Finish::Beveled);
        } else if (on || hov) && lemon {
            let seg = Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0);
            if on {
                os_ui::kit::lemon::lit_in(ui.c, ui.t, seg, 5.0, hov);
            } else {
                ui.c.fill_rrect(seg, 5.0, t.button_hi);
            }
        } else if on || hov {
            ui.c.push_clip(r);
            if on {
                ui.c.fill_rect_gradient(capsule.inset(1.0), t.on_top, t.on_bottom, 14.0);
            } else {
                ui.c.fill_rrect(capsule.inset(1.0), 14.0, t.button_hi.with_alpha(110));
            }
            ui.c.pop_clip();
        }
        let g = Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, r.h - 16.0);
        let c = if on { t.on_glyph } else { t.glyph };
        if icon == Icon::Play && s.rec.waiting {
            // Waiting for input (spec: ui/recording.md §3): three dots.
            for k in [-1.0, 0.0, 1.0] {
                ui.c.fill_circle(g.cx() + k * g.w * 0.32, g.cy(), (g.w * 0.1).max(1.5), c);
            }
        } else if icon == Icon::Play && on {
            if ui.c.icon_pack() == 0 {
                let (w, h) = (g.w * 0.22, g.h * 0.7);
                let y = g.cy() - h / 2.0;
                ui.c.fill_rrect(Rect::new(g.cx() - w * 1.6, y, w, h), 1.0, c);
                ui.c.fill_rrect(Rect::new(g.cx() + w * 0.6, y, w, h), 1.0, c);
            } else {
                icons::draw(ui.c, Icon::Pause, g, c);
            }
        } else {
            icons::draw(ui.c, icon, g, c);
        }
    }
    // Record (spec: ui/recording.md §2): armed, the whole round face fills with the record colour, a lighter rim and a
    // dark dot in the middle; disarmed, a small dimmed dot. The record colour is white under High visibility
    // (`crate::high_visibility`).
    let rec = a.span(486.0, 513.0, 7.0, 33.0);
    let hov = ui.hit(rec, Action::Record, crate::record::hint());
    let armed = s.rec.armed;
    let down = ui.down(rec, &Action::Record);
    let dot = crate::high_visibility::record_dot(t, crate::high_visibility::on());
    let face = Rect::new(rec.cx() - 14.0, rec.cy() - 14.0, 28.0, 28.0);
    if t.style.vintage_2005 {
        crate::vintage::metal(ui.c,face,if hov {t.button_hi}else{t.button});
    } else if lemon {
        os_ui::kit::button(ui.c, t, face, os_ui::kit::State::new(hov, down, false));
    } else {
        ui.c.fill_circle(rec.cx(), rec.cy(), 14.0, t.toolbar_line.shade(-0.25));
        let bg = if hov || down { t.button_hi } else { t.button.shade(-0.12) };
        ui.c.fill_circle(rec.cx(), rec.cy(), 12.6, bg);
    }
    if armed {
        let fill = if hov || down { dot.shade(0.12) } else { dot };
        ui.c.fill_circle(rec.cx(), rec.cy(), 12.0, fill);
        ui.c.stroke_circle(rec.cx(), rec.cy(), 11.4, 1.6, dot.shade(0.45));
        if ui.c.icon_pack() == 0 {
            ui.c.fill_circle(rec.cx(), rec.cy(), 4.6, dot.shade(-0.6));
        } else {
            icons::draw(ui.c, Icon::Record, face.inset(5.0), dot.shade(-0.6));
        }
    } else {
        let bg = if lemon { t.toolbar } else { t.button.shade(-0.12) };
        if ui.c.icon_pack() == 0 {
            ui.c.fill_circle(rec.cx(), rec.cy(), 5.6, if hov { dot } else { dot.mix(bg, 0.35) });
        } else {
            icons::draw(ui.c, Icon::Record, face.inset(4.0), if hov { dot } else { dot.mix(bg, 0.35) });
        }
    }
    // Tempo.
    let tb = a.span(514.0, 572.0, 5.0, 35.0);
    ui.hit(tb, Action::Tempo, "Tempo (drag up/down: the whole number, or the decimal place the drag starts on)");
    if t.style.vintage_2005 {
        crate::vintage::recess(ui.c,tb,Color::rgb(0x779394));
    } else if lemon {
        os_ui::kit::lemon::well(ui.c, t, tb, t.time_panel);
    } else {
        ui.c.fill_rrect(tb, 5.0, ui.t.s(0x5b5f61));
        ui.c.fill_rrect(tb.inset(1.0), 4.0, ui.t.s(0x5f6466));
    }
    // Lemon displays show their digits in the brand colour.
    let digits = if t.style.vintage_2005 {Color::rgb(0x23383d)}else if lemon { t.pill } else { t.glyph };
    tempo_readout(ui, s, tb, f.tempo, digits, c.preview);
}

/// Center the full BPM value on one baseline. Drag boundaries follow the rendered decimal digits.
pub(super) fn tempo_readout(ui: &mut Ui, s: &State, rect: Rect, tempo: f64, ink: Color, preview: bool) {
    let tempo = if tempo.is_finite() { tempo } else { 120.0 };
    let label = format!("{tempo:.3}");
    let (size, min) = if s.vintage_2005 { (11, 8) } else { (19, 11) };
    let font_at = |size| if s.vintage_2005 { Font::vintage(size).bold() } else { Font::condensed(size, 6) };
    let mut font = font_at(size);
    for size in (min..size).rev() {
        if ui.c.text_width(font, &label) <= rect.w - 10.0 { break; }
        font = font_at(size);
    }
    let width = ui.c.text_width(font, &label);
    let x = (rect.cx() - width / 2.0).round();
    let y = (rect.cy() - ui.c.line_height(font) / 2.0).round();
    ui.c.push_clip(rect.inset(2.0));
    ui.c.text(font, x, y, ink, &label);
    ui.c.pop_clip();
    if !preview {
        let decimal = label.find('.').expect("formatted tempo has a decimal point");
        s.tempo_digits.set([1, 2, 3].map(|i| x + ui.c.text_width(font, &label[..decimal + i])));
    }
}

/// Recording options. The metronome works (spec: ui/metronome.md); the others wait for recording.
fn recording(ui: &mut Ui, c: &Ctx) -> bool {
    let a = c.a;
    if c.s.vintage_2005 && a.x(743.0) <= c.fit
        && super::vintage_toolbar::compact_recording(ui, c.s, a.x(575.0), a.y(0.0)) {
        return true;
    }
    let button = if c.s.vintage_2005 { super::vintage_toolbar::record_button } else { tool_button };
    let m = a.r(btn(580.0, 4.0));
    if m.right() > c.fit {
        return false;
    }
    button(ui, m, Icon::Metronome, crate::metronome::on(), Action::Metronome, "Metronome (Ctrl+M) - right-click for the sound and volume");
    // Start on input, Recording precount, Blend, Loop record (spec: ui/recording.md §3-4).
    use crate::record::Opt;
    let opts = [(Icon::WaitInput, Opt::Wait), (Icon::Countdown, Opt::Precount), (Icon::LoopRecord, Opt::Loop)];
    for (i, (icon, opt)) in opts.into_iter().enumerate() {
        let r = a.r(btn(613.0 + 33.0 * i as f32, 4.0));
        if r.right() > c.fit {
            return false;
        }
        button(ui, r, icon, opt.on(), Action::RecOpt(opt), crate::record::option_hint(opt));
    }
    let r = a.r(btn(712.0, 4.0));
    if r.right() > c.fit { return false; }
    button(ui, r, Icon::Slide, c.s.params.capture_armed, Action::AutomationCapture,
        "Create automation: click, then move a control; click again to cancel");
    true
}

/// Song position (B:S:T, or M:S:CS: a click toggles, right-click opens the mode menu).
fn time(ui: &mut Ui, c: &Ctx) {
    let (t, a, s, f) = (ui.t, c.a, c.s, c.f);
    let lemon = super::lemon(ui);
    let tp = a.span(745.0, 873.0, 4.0, 36.0);
    if t.style.vintage_2005 {
        os_ui::kit::styled_surface(ui.c, t, tp, 1.0, t.window_border, t.time_panel, t.time_panel,
            os_theme::appearance::Finish::Recessed);
    } else if lemon {
        os_ui::kit::lemon::well(ui.c, t, tp, t.time_panel);
    } else {
        ui.c.fill_rect(tp, ui.t.s(0x212829));
        ui.c.fill_rect(tp.inset(1.0), t.time_panel);
    }
    // spec: ui/time-display.md §2-3 (the bar's signature is the project's).
    ui.hit(tp, Action::Clock, s.clock.hint());
    ui.live(Live::Clock, tp);
    let ppq = f64::from(s.project.ppq.max(1));
    // A bar that makes no signature (3 steps per beat) counts its beats as quarters.
    let sig = s.project.signature().unwrap_or((s.project.bar().0, 4));
    let secs = f.tick / ppq * 60.0 / s.project.tick_tempo(s.tempo.max(1.0));
    let (bar, step, tick) = s.clock.reading(secs, f.tick * 96.0 / ppq, sig);
    ui.c.push_clip(tp.inset(1.0));
    let g = if lemon { t.pill } else { t.glyph };
    if let Some(target)=s.rec.preroll.filter(|target|f.playing && !f.paused && f.tick<f64::from(*target)) {
        let beats=((f64::from(target)-f.tick)/f64::from(s.project.beat_ticks().max(1))).ceil().max(1.) as u32;
        ui.c.text_center(Font::light(18,8),Rect::new(tp.x,tp.y+2.,tp.w,tp.h-2.),g,&format!("PRE-ROLL {beats}"));
    } else if let Some(c) = s.rec.count.filter(|c| c.left() > 0) {
        // A recording count-in (spec: ui/recording.md §3): the beats left.
        ui.c.text_center(Font::light(22, 8), Rect::new(tp.x, tp.y + 2.0, tp.w, tp.h - 2.0), g, &format!("REC IN {}", c.left()));
    } else if t.style.vintage_2005 {
        crate::vintage::clock(ui.c, tp, bar, step, tick);
    } else {
        // Light digits drawn piece by piece (measured: big 36 px condensed to 14, small 24 to 9),
        // right-aligned so long bar numbers grow to the left and never leave the panel.
        let (big, small) = (Font::light(32, 13), Font::light(22, 8));
        let bar_s = bar.to_string();
        let step_s = format!("{step:02}");
        let tick_s = format!("{tick:02}");
        let (bw, sw, kw) = (ui.c.text_width(big, &bar_s), ui.c.text_width(big, &step_s), ui.c.text_width(small, &tick_s));
        let right = a.x(863.0);
        let tick_x = right - kw;
        let step_x = tick_x - 8.0 - sw;
        let bar_x = (step_x - 8.0 - bw).max(tp.x + 2.0);
        ui.c.text(big, bar_x, a.y(-3.0), g, &bar_s);
        ui.c.fill_rect(Rect::new(step_x - 5.5, a.y(20.0), 2.0, 2.5), g);
        ui.c.fill_rect(Rect::new(step_x - 5.5, a.y(30.0), 2.0, 2.5), g);
        ui.c.text(big, step_x, a.y(-3.0), g, &step_s);
        ui.c.fill_rect(Rect::new(tick_x - 5.0, a.y(21.0), 2.0, 2.0), g);
        ui.c.fill_rect(Rect::new(tick_x - 5.0, a.y(29.5), 2.0, 2.0), g);
        ui.c.text(small, tick_x, a.y(9.0), g, &tick_s);
    }
    ui.c.pop_clip();
    if !t.style.vintage_2005 {
        let label = s.clock.label();
        ui.c.text_right(Font::condensed(7, 3).bold(), a.x(871.0), a.y(5.0), if lemon { t.text_dim } else { t.glyph.shade(-0.1) }, label);
    }
}

/// The output monitor (spec ui/monitor.md): a scope / analyzer panel at x 882..968 and two peak bars at x 970..1047.
fn monitor(ui: &mut Ui, c: &Ctx) {
    use crate::monitor::{Mode, COLS};
    let (a, s) = (c.a, c.s);
    let (edge, fill, ink) = (ui.t.s(0x1d2022), ui.t.s(0x25282a), ui.t.tx(0xb9bfc3));
    let m = &s.monitor;
    // Left panel: border, then 84 x 31 px of picture, bottom edge at y 36. Pictures are drawn as cells, which tile at
    // any scale (spec ui/ui-scaling.md §4).
    let lemon = super::lemon(ui);
    let (fill, ink) = if lemon { (ui.t.time_panel, ui.t.pill.mix(ui.t.time_panel, 0.15)) } else { (fill, ink) };
    let scope = a.r(Rect::new(880.0, 4.0, 86.0, 33.0));
    if lemon {
        os_ui::kit::lemon::well(ui.c, ui.t, scope, fill);
    } else {
        ui.c.fill_rect(scope, edge);
        ui.c.fill_rect(a.r(Rect::new(881.0, 5.0, 84.0, 31.0)), fill);
    }
    let (x0, bottom) = (a.x(881.0), a.y(36.0));
    match m.mode {
        Mode::Spectrum => {
            for col in 0..COLS {
                let h = m.bar(col) * 31.0;
                let whole = h.floor();
                let x = x0 + col as f32;
                if whole > 0.0 {
                    ui.c.fill_cell(Rect::new(x, bottom - whole, 1.0, whole), ink);
                }
                let part = h - whole;
                if part > 0.05 && whole < 31.0 {
                    ui.c.fill_cell(Rect::new(x, bottom - whole - 1.0, 1.0, 1.0), ink.with_alpha((part * 255.0) as u8));
                }
            }
        }
        Mode::Scope => {
            for col in 0..COLS {
                let (lo, hi) = m.wave[col];
                let (y0, y1) = (a.y(20.0) - hi * 15.0, a.y(20.0) - lo * 15.0);
                let (top, bottom) = (y0.min(y1).round(), y0.max(y1).round());
                ui.c.fill_cell(Rect::new(x0 + col as f32, top, 1.0, (bottom - top).max(1.0)), ink);
            }
        }
    }
    ui.hit(scope, Action::Monitor, "Oscilloscope / spectrum analyzer (click to switch)");

    // Right panel: two peak bars (meter range .. 0 dB), light grey fading to cream at the tip, plus the held peak.
    let peaks = a.r(Rect::new(968.0, 4.0, 77.0, 23.0));
    if lemon {
        os_ui::kit::lemon::well(ui.c, ui.t, peaks, fill);
    } else {
        ui.c.fill_rect(peaks, edge);
        ui.c.fill_rect(a.r(Rect::new(969.0, 5.0, 75.0, 21.0)), fill);
    }
    // Classic: both ends follow the theme's highlight key. Lemon: the displays' digit colour (like the scope's ink),
    // dimmed toward the panel at the left and full at the tip. The held peak takes the tip colour.
    let (lo, hi) = if lemon {
        (ui.t.pill.mix(ui.t.time_panel, 0.45), ui.t.pill)
    } else {
        (ui.t.h(0xc8ced1), ui.t.h(0xcdc7a8))
    };
    let x0 = a.x(969.0);
    for i in 0..2 {
        let y = a.y(5.0 + 11.0 * i as f32);
        let hold = (m.meter_pos(m.hold[i].0) * 75.0).round();
        if m.hold[i].1.is_some() && hold >= 1.0 {
            ui.c.fill_rect(Rect::new(x0 + hold - 1.0, y, 1.0, 10.0), hi);
        }
        let len = (m.meter_pos(m.meter_db[i]) * 75.0).round();
        if len < 1.0 {
            continue;
        }
        let mix = |k: f32, shade: f32| {
            let l = |u: u8, v: u8| (((u as f32 + (v as f32 - u as f32) * k) * shade).round() as u32).min(255);
            os_ui::Color::rgb((l(lo.r, hi.r) << 16) | (l(lo.g, hi.g) << 8) | l(lo.b, hi.b))
        };
        for x in 0..len as i32 {
            let k = x as f32 / len.max(1.0);
            // Tip column darker, left column slightly dark; top and bottom rows shaded like a raised bar.
            let side = if x == len as i32 - 1 { 0.58 } else if x == 0 { 0.82 } else { 1.0 };
            let px = x0 + x as f32;
            ui.c.fill_cell(Rect::new(px, y, 1.0, 1.0), mix(k, 0.82 * side));
            ui.c.fill_cell(Rect::new(px, y + 1.0, 1.0, 8.0), mix(k, side));
            ui.c.fill_cell(Rect::new(px, y + 9.0, 1.0, 1.0), mix(k, 0.75 * side));
        }
    }
    ui.hit(peaks, Action::Swallow, "Output level (peak L / R)");
    ui.c.stroke_circle(a.x(1037.0), a.y(33.5), 4.0, 2.0, ui.t.s(0x66737d));
}

/// CPU panel (spec: ui/main-window.md §13): audio load (bar, percent, recent history or the Misc counts), memory,
/// voices (or frames per second), dropouts. The hint is the reading in words (`App::tick`).
fn live_monitor(ui: &mut Ui, s: &State, r: Rect) {
    let t = ui.t;
    ui.hit(r, Action::CpuPanel, s.cpu.hint());
    ui.c.fill_rrect(r, 4., t.sunken);
    ui.c.stroke_rrect(r, 4., 1., t.toolbar_line);
    let font = Font::new(11);
    let reading = &s.cpu.now;
    ui.c.text(font, r.x + 5., r.y + 2., t.text_dim, "Audio load");
    ui.c.text(font, r.x + 5., r.y + 17., t.text_dim, "Memory");
    ui.c.text_right(
        font,
        r.x + 99.,
        r.y + 2.,
        t.text,
        &format!("{}%", reading.load),
    );
    let memory = reading
        .mem_mb
        .map_or_else(|| "— MB".to_string(), |m| format!("{m} MB"));
    ui.c.text_right(font, r.x + 99., r.y + 17., t.text, &memory);
    if reading.dropouts > 0 {
        ui.c.text(
            Font::new(9),
            r.x + 102.,
            r.y + 4.,
            t.meter_clip,
            &format!("!{}", reading.dropouts.min(9999)),
        );
    }
    let plot = Rect::span(r.x + 128., r.right() - 21., r.y + 3., r.bottom() - 3.);
    let grid = t.toolbar_line.with_alpha(120);
    for i in 0..7 {
        let x = plot.x + plot.w * i as f32 / 6.;
        ui.c.line(x, plot.y, x, plot.bottom(), 1., grid);
    }
    ui.c.line(plot.x, plot.cy(), plot.right(), plot.cy(), 1., grid);
    let mut last = None;
    for (i, &(lo, hi)) in s.monitor.wave.iter().enumerate() {
        let x = plot.x + i as f32 * plot.w / (crate::monitor::COLS - 1) as f32;
        let y = plot.cy() - (lo + hi).clamp(-2., 2.) * plot.h * 0.25;
        if let Some((px, py)) = last {
            ui.c.line(px, py, x, y, 1., os_ui::Color::rgb(0xffdf44));
        }
        last = Some((x, y));
    }
    for ch in 0..2 {
        let level = s.monitor.meter_pos(s.monitor.meter_db[ch]);
        let peak = s.monitor.meter_pos(s.monitor.hold[ch].0);
        for i in 0..10 {
            let mark = (i + 1) as f32 / 10.;
            let active = level >= mark || (peak >= mark && peak < mark + 0.1);
            let ink = if !active {
                grid
            } else if i >= 8 {
                t.meter_high
            } else {
                os_ui::Color::rgb(0x98d752)
            };
            ui.c.fill_rect(
                Rect::new(
                    r.right() - 17. + 7. * ch as f32,
                    r.bottom() - 10. - 2.2 * i as f32,
                    5.,
                    2.,
                ),
                ink,
            );
        }
        ui.c.text_center(
            Font::new(7),
            Rect::new(r.right() - 18. + 7. * ch as f32, r.bottom() - 8., 7., 8.),
            t.text_dim,
            if ch == 0 { "L" } else { "R" },
        );
    }
}

fn classic_cpu(ui:&mut Ui,s:&State,a:At) {
    let t=ui.t;
    let cpu = a.r(crate::app::CPU_RECT);
    sunken(ui, cpu, t.sunken_dark);
    ui.hit(cpu, Action::CpuPanel, s.cpu.hint());
    let r = &s.cpu.now;
    let level = |load: u32| match load {
        0..75 => t.glyph,
        75..100 => t.meter_high,
        _ => t.meter_clip,
    };
    let k = (r.load as f32 / 100.0).min(1.0);
    ui.c.fill_rect(a.r(Rect::new(1055.0, 6.0, 1.5, 12.0)), t.glyph.with_alpha(60));
    if k > 0.0 {
        ui.c.fill_cell(a.r(Rect::new(1055.0, 6.0 + 12.0 * (1.0 - k), 1.5, 12.0 * k)), level(r.load));
    }
    ui.c.text_right(Font::new(17), a.x(1136.0), a.y(3.0), level(r.load), &format!("{}%", r.load));
    if let Some(m) = r.misc {
        // Misc: handles, threads, fonts in the history's place.
        ui.c.text(Font::new(13), a.x(1058.0), a.y(20.0), t.glyph, &format!("{} H  {} T  {} F", m.handles, m.threads, m.fonts));
    } else {
        // One 2 px column per reading, newest on the right, full height at 100 %.
        for (i, &h) in s.cpu.history.iter().rev().enumerate() {
            let hh = (11.0 * h as f32 / 100.0).min(11.0).round();
            if hh > 0.0 {
                ui.c.fill_cell(a.r(Rect::new(1128.0 - 2.0 * i as f32, 32.0 - hh, 2.0, hh)), level(h).with_alpha(170));
            }
        }
        ui.c.line(a.x(1055.0), a.y(32.0), a.x(1130.0), a.y(32.0), 1.0, t.glyph.with_alpha(160));
    }
    let mem = r.mem_mb.map_or("- MB".to_string(), |m| format!("{m} MB"));
    ui.c.text_right(Font::condensed(18, 6), a.x(1199.0), a.y(3.0), t.glyph, &mem);
    if s.cpu.fps_view {
        let unit = Font::new(13);
        let uw = ui.c.text_width(unit, "fps");
        ui.c.text_right(unit, a.x(1166.0), a.y(21.0), t.glyph, "fps");
        ui.c.text_right(Font::condensed(18, 6), a.x(1166.0) - uw - 2.0, a.y(17.0), t.glyph, &r.fps.to_string());
    } else {
        ui.c.text_right(Font::condensed(18, 6), a.x(1166.0), a.y(17.0), t.glyph, &r.voices.to_string());
    }
    if r.dropouts > 0 {
        ui.c.text_right(Font::condensed(18, 6), a.x(1199.0), a.y(17.0), t.meter_clip, &format!("!{}", r.dropouts.min(9999)));
    }
}

/// Utility buttons: "left / right" hints, right clicks in `App::toolbar_right` (spec: ui/toolbar-buttons.md §2).
/// FL's last one, the content library's download, is left out (spec: ui/main-window.md §2).
fn utilities(ui: &mut Ui, c: &Ctx) -> bool {
    let button = c.a.r(btn(1209.0, 4.0));
    if button.right() > c.fit { return false; }
    tool_button(ui, button, Icon::SaveRender, false, Action::MenuItem(0, menus::FILE_EXPORT), "Export audio... (Ctrl+R)");
    if !c.s.bar.layout.rows.iter().flatten().any(|panel| *panel == Panel::Modes) {
        let keyboard = c.a.r(btn(1242.0, 4.0));
        if keyboard.right() > c.fit { return false; }
        tool_button(ui, keyboard, Icon::TypingKeyboard, c.s.toggles.contains(&"typing"), Action::Typing, TYPING_HINT);
    }
    true
}

/// Menu bar.
fn menu_bar(ui: &mut Ui, c: &Ctx) {
    let (t, a, s) = (ui.t, c.a, c.s);
    let mb = a.span(4.0, 296.0, 45.0, 77.0);
    let lemon = super::lemon(ui);
    if t.style.vintage_2005 {
        crate::vintage::recess(ui.c,mb,t.menu_panel);
    } else if lemon {
        os_ui::kit::lemon::panel(ui.c, mb, 6.0, t.menu_panel);
    } else {
        ui.c.fill_rect_gradient(mb, t.menu_panel.shade(-0.04), t.menu_panel.shade(0.02), 2.5);
        ui.c.stroke_rrect(mb, 2.5, 1.0, t.toolbar_line.shade(-0.1));
    }
    let layout = menus::bar_layout(ui.c,s.vintage_2005);
    for (i, (m, (x0, x1, width))) in menus::MENUS.iter().zip(layout).enumerate() {
        let title = if s.vintage_2005 {menus::VINTAGE_TITLES[i]}else if i==menus::HELP_MENU && ui.c.size().0 < 1300. {"…"} else {m.title};
        let r = a.r(Rect::new(x0 - 3.0, 47.0, x1 - x0 + 6.0, 29.0));
        let hov = ui.hit(r, Action::Menu(i), title);
        let open = s.menu_open == Some(i) && !c.preview;
        if lemon && open {
            os_ui::kit::lemon::lit_in(ui.c, ui.t, r.inset(1.0), 4.0, false);
        } else if hov || open {
            ui.c.fill_rrect(if lemon { r.inset(1.0) } else { r }, if lemon { 4.0 } else { 2.0 }, t.menu_drop_hi);
        }
        if s.vintage_2005 && crate::vintage_skin::available() {
            crate::vintage_skin::lettering(ui.c,title,a.x(x0).round(),a.y(49.));
        } else {
        ui.c.text(if s.vintage_2005 {Font::vintage(width as u16)}else{Font::condensed(menus::BAR_FONT.size, width as u16)}, if s.vintage_2005 {a.x(x0).round()}else{a.x(x0)}, a.y(if s.vintage_2005 {53.0}else{51.0}), if lemon && open { t.pill_text } else { t.menu_text }, title);
        }
    }
}

fn vintage_slider(ui: &mut Ui, x: f32, y: f32, value: f32, mark_unity: bool) {
    let t = ui.t;
    ui.c.fill_rect(Rect::new(x - 2.0, y - 14.0, 4.0, 28.0), t.sunken_dark);
    for offset in [-12.0, -6.0, 0.0, 6.0, 12.0] {
        ui.c.fill_rect(Rect::new(x + 5.0, y + offset, 4.0, 1.0), t.text_dim);
    }
    if mark_unity {
        crate::vintage::fader_unity(ui.c, x, y + 11.0 - 0.8 * 22.0, 7.0);
    }
    let cap = Rect::new(x - 6.0, y + 11.0 - value.clamp(0.0, 1.0) * 22.0 - 3.0, 12.0, 6.0);
    os_ui::kit::styled_surface(ui.c, t, cap, 0.0, t.window_border, t.fader_handle, t.fader_handle,
        os_theme::appearance::Finish::Beveled);
    ui.c.fill_rect(Rect::new(cap.x + 2.0, cap.cy(), cap.w - 4.0, 1.0), t.record);
}

/// Main volume (project value; spec: audio/mixer.md "Main volume").
fn volume(ui: &mut Ui, c: &Ctx) {
    use crate::knobs::KnobId;
    let (t, a, s) = (ui.t, c.a, c.s);
    ui.hit(a.span(303.0, 337.0, 45.0, 77.0), Action::Knob(KnobId::MasterVol), s.knob_text(KnobId::MasterVol));
    if t.style.vintage_2005 {
        vintage_slider(ui, a.x(319.0), a.y(61.0), s.knob(KnobId::MasterVol), true);
    } else {
        super::knob(ui, a.x(319.0), a.y(61.0), 14.0, s.knob(KnobId::MasterVol), false, t.glyph, t.button_hi);
    }
}

/// Main pitch (project value; spec: audio/voice-levels.md).
fn pitch(ui: &mut Ui, c: &Ctx) {
    use crate::knobs::KnobId;
    let (t, a, s) = (ui.t, c.a, c.s);
    let id = if t.style.vintage_2005 { KnobId::MasterPitch } else { KnobId::Varispeed };
    ui.hit(a.span(340.0, 376.0, 45.0, 77.0), Action::Knob(id), s.knob_text(id));
    let (cx, cy) = (a.x(357.0), a.y(61.0));
    if t.style.vintage_2005 {
        vintage_slider(ui, cx, cy, s.knob(id), false);
        return;
    }
    super::knob(ui, cx, cy, 14.0, s.knob(KnobId::Varispeed), true, t.glyph, t.button_hi);
    for k in 0..20 {
        let ang = 0.75 * std::f32::consts::PI + 1.5 * std::f32::consts::PI * k as f32 / 19.0;
        ui.c.fill_circle(cx + 17.5 * ang.cos(), cy + 17.5 * ang.sin(), 0.7, t.glyph_dim);
    }
}

/// Song position slider (spec: ui/song-position-slider.md): the handle follows the playhead while playing.
fn song_pos(ui: &mut Ui, c: &Ctx) {
    let (t, a, s, f) = (ui.t, c.a, c.s, c.f);
    let lemon = super::lemon(ui);
    let sp = a.r(crate::song_pos::PANEL);
    let range = crate::song_pos::range(s);
    let pos = crate::song_pos::shown(s, f.playing, f.song, f.tick, f.song_pos_drag);
    ui.live(Live::Clock, sp);
    match range {
        Some(_) => {
            ui.hit(sp, Action::SongPos, crate::song_pos::hint(s, pos));
        }
        // An empty playlist in SONG mode: nothing to move through.
        None => {
            ui.hit(sp, Action::Swallow, "Song position: the playlist is empty");
        }
    }
    let hx = range.map(|len| a.x(crate::song_pos::handle_x(pos, len).round()));
    let y = |v: f32| a.y(v);
    if lemon {
        let x0 = a.x(390.0);
        ui.c.fill_rrect(Rect::new(x0, y(60.0), 180.0, 4.0), 2.0, t.sunken_dark);
        match hx {
            Some(hx) => {
                use os_theme::design::lemon as brand;
                ui.c.fill_rrect(Rect::new(x0, y(60.0), hx - x0, 4.0), 2.0, t.brand_lit(brand::MAIN));
                ui.c.fill_rrect(Rect::new(hx - 6.0, y(55.0), 12.0, 14.0), 3.0, t.button);
                ui.c.fill_rect(Rect::new(hx - 1.0, y(58.0), 2.0, 8.0), t.brand_lit(brand::MAIN));
            }
            None => ui.c.fill_rect(a.r(Rect::new(394.0, 58.0, 2.0, 8.0)), t.button),
        }
    } else {
        // Classic: the measured colours (§1).
        let x0 = a.x(394.0);
        ui.c.fill_rect(Rect::new(x0, y(60.0), 172.0, 4.0), ui.t.s(0x343739));
        match hx {
            Some(hx) => {
                let green = ui.t.a(0xa8e44a);
                ui.c.fill_rect(Rect::new(x0, y(60.0), hx - x0, 4.0), green);
                ui.c.fill_circle(hx + 0.5, y(62.0), 6.5, ui.t.s(0x1c1f21));
                ui.c.fill_circle(hx + 0.5, y(62.0), 5.5, ui.t.s(0x26292b));
                ui.c.fill_rect(Rect::new(hx, y(56.0), 1.0, 2.0), ui.t.s(0x9a9d9f));
                ui.c.fill_rect(Rect::new(hx, y(58.0), 1.0, 7.0), green);
                ui.c.fill_rect(Rect::new(hx, y(65.0), 1.0, 3.0), ui.t.s(0x686b6d));
            }
            None => ui.c.fill_rect(a.r(Rect::new(395.0, 56.0, 1.0, 12.0)), ui.t.s(0x282b2d)),
        }
    }
}

/// Mode buttons (spec: ui/main-window.md §3). Typing keyboard, auto scrolling (spec: ui/auto-scrolling.md), step edit
/// (spec: ui/step-edit.md), enable groups (spec: ui/enable-groups.md) and multilink (spec: ui/remote-control.md §4)
/// work.
const TYPING_HINT: &str = "Typing keyboard to piano (Ctrl+T) - right-click for key and scale";

fn modes(ui: &mut Ui, c: &Ctx) -> bool {
    let (a, s) = (c.a, c.s);
    for (i, (icon, hint)) in [
        (Icon::TypingKeyboard, TYPING_HINT),
        (Icon::AutoScroll, "Auto scrolling - right-click for continuous scrolling"),
        (Icon::StepEdit, "Step editing (Ctrl+E)"),
        (Icon::Link, "Enable groups (Shift+Alt+G)"),
        (Icon::MidiControl, "Multilink to controllers (Ctrl+J)"),
    ]
    .into_iter()
    .enumerate()
    {
        let r = a.r(btn(580.0 + 33.0 * i as f32, 45.0));
        if r.right() > c.fit {
            return false;
        }
        let (on, action) = match i {
            0 => (s.toggles.contains(&"typing"), Action::Typing),
            1 => (crate::autoscroll::on(), Action::AutoScroll),
            2 => (crate::step_edit::on(), Action::StepEdit),
            3 => (crate::enable_groups::on(), Action::EnableGroups),
            _ => (s.multilink.on, Action::Multilink),
        };
        tool_button(ui, r, icon, on, action, hint);
    }
    true
}

/// Snap panel (spec: ui/global-snap.md §1): the magnet turns snapping on / off, the combo holds the global snap.
fn snap(ui: &mut Ui, c: &Ctx) {
    let (t, a, s) = (ui.t, c.a, c.s);
    if s.vintage_2005 {
        super::vintage_toolbar::compact_snap(ui, s, a.x(743.0), a.y(41.0));
        return;
    }
    let lemon = super::lemon(ui);
    let global = s.global_snap.now;
    let on = global != crate::playlist::Snap::None;
    ui.hit(a.span(746.0, 767.0, 50.0, 72.0), Action::GlobalSnapToggle, crate::snap::MAGNET_HINT);
    let magnet = if lemon {
        if on { t.glyph_dim } else { t.glyph_dim.mix(t.toolbar, 0.55) }
    } else {
        t.tx(if on { 0x8b959c } else { 0x52585c })
    };
    icons::draw(ui.c, Icon::Magnet, a.span(748.0, 766.0, 52.0, 70.0), magnet);
    let snap = a.span(768.0, 830.0, 50.0, 72.0);
    let hov = ui.hit(snap, Action::GlobalSnap, crate::snap::HINT);
    if lemon {
        os_ui::kit::lemon::well(ui.c, t, snap, if hov { t.sunken.mix(t.button_hi, 0.5) } else { t.sunken });
    } else {
        ui.c.fill_rrect(snap, 2.0, t.sunken_dark);
    }
    // The longest labels ("1/6 step") are narrowed to fit before the arrow.
    let label = global.label();
    let room = snap.w - 5.0 - 14.0;
    let font = if s.vintage_2005 {Font::vintage(11)}else if ui.c.text_width(Font::new(16), label) <= room { Font::new(16) } else { Font::condensed(16, 6) };
    ui.c.text(font, snap.x + 5.0, a.y(if s.vintage_2005 {55.0}else{51.0}), t.glyph, label);
    // The arrow points down while the mouse is over the combo (§1).
    let dir = if hov { os_ui::kit::Dir::Down } else { os_ui::kit::Dir::Right };
    os_ui::kit::triangle(ui.c, snap.right() - 8.0, a.y(61.0), 3.5, dir, t.glyph_dim);
}

/// Pattern selector (to the "+", x 962).
fn pattern(ui: &mut Ui, c: &Ctx) {
    let (t, a, s) = (ui.t, c.a, c.s);
    if s.vintage_2005 && super::vintage_toolbar::pattern_panel(ui, s, a.x(834.0), a.y(41.0)) {
        let add = a.r(Rect::new(949.0, 46.0, 17.0, 28.0));
        let hot = ui.hit(add, Action::PatternAdd, "New pattern (find first / next empty)");
        crate::vintage::metal(ui.c, add, if hot { t.button_hi } else { t.button });
        icons::draw(ui.c, Icon::Plus, add.inset(3.0), t.glyph);
        return;
    }
    let lemon = super::lemon(ui);
    let prev = a.span(838.0, 851.0, 47.0, 76.0);
    ui.hit(prev, Action::PatternPrev, "Pattern options"); // spec: ui/pattern-menu.md §1
    ui.c.fill_rrect(prev, if lemon { 4.0 } else { 2.0 }, t.button);
    icons::draw(ui.c, Icon::ArrowRight, prev.inset(2.0), t.glyph);
    let pb = a.span(855.0, 948.0, 47.0, 76.0);
    let hov = ui.hit(pb, Action::PatternNext, crate::patterns::name_of(s, s.pattern.max(1)));
    if t.style.vintage_2005 {
        crate::vintage::recess(ui.c,pb,Color::rgb(0x344c4f));
    } else if lemon {
        os_ui::kit::lemon::well(ui.c, t, pb, if hov { t.sunken.mix(t.button_hi, 0.5) } else { t.sunken });
    } else {
        ui.c.fill_rrect(pb, 2.0, t.tempo_box.shade(if hov { 0.16 } else { 0.08 }));
    }
    let name = s
        .project
        .patterns
        .iter()
        .find(|p| p.number == s.pattern)
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| format!("Pattern {}", s.pattern));
    // Right-aligned before the spin arrow; a name too long for that keeps its start and is cut at the arrow, as FL
    // cuts long labels (long names were not measured here: Lemon Studio's choice).
    let pf = if t.style.vintage_2005 {Font::vintage(12).bold()}else{Font::condensed(21, 7)};
    let end = pb.right() - 11.0;
    if ui.c.text_width(pf, &name) <= end - (pb.x + 2.0) {
        ui.c.push_clip(pb.inset(2.0));
        ui.c.text_right(pf, end, a.y(49.0), t.glyph, &name);
        ui.c.pop_clip();
    } else {
        ui.c.text_clip(pf, pb.x + 4.0, a.y(49.0), end - (pb.x + 4.0), t.glyph, &name);
    }
    let add = a.span(951.0, 962.0, 50.0, 72.0);
    ui.hit(add, Action::PatternAdd, "New pattern (find first / next empty)");
    icons::draw(ui.c, Icon::Plus, add, t.glyph);
}

/// Window buttons `which` of the row of nine (spec: ui/toolbar-buttons.md §2; right clicks in `App::toolbar_right`):
/// the window buttons are the first five, the pickers and tools the rest.
fn window_buttons(ui: &mut Ui, c: &Ctx, which: Range<usize>) -> bool {
    let a = c.a;
    if c.s.vintage_2005 && which.start == 0 && a.x(1136.0) <= c.fit
        && super::vintage_toolbar::compact_shortcuts(ui, c.s, a.x(967.0), a.y(41.0)) {
        return true;
    }
    let wins = [
        (Icon::Playlist, Action::ToggleWin(Win::Playlist), "Playlist / Maximize Play list (F5)"),
        (Icon::PianoRoll, Action::ToggleWin(Win::PianoRoll), "Piano roll / Maximize Piano roll (F7)"),
        (Icon::ChannelRack, Action::ToggleWin(Win::ChannelRack), "Channel rack (F6)"),
        (Icon::Mixer, Action::ToggleWin(Win::Mixer), "Mixer (F9)"),
        (Icon::Browser, Action::ToggleWin(Win::Browser), "Browser / Plugin picker (Alt+F8)"),
        (Icon::Project, Action::ProjectPicker, "Project picker / Plugin picker (Ctrl+F8)"),
        (Icon::Plug, Action::PickerOpen, "Plugin picker (F8)"),
        (Icon::Kick, Action::ToggleWin(Win::Tapper), "Tempo tapper (Ctrl+F5)"),
        (Icon::Touch, Action::ToggleWin(Win::Touch), "Touch controller (Alt+F7)"),
        // FL's store button is hidden: its stand-in, a repository of free and open-source plugins (spec:
        // ui/main-window.md §3), had the plugin picker's icon. It is to come back with a shop icon.
    ];
    let legacy = crate::settings_file::flag("declutter_toolbar", false);
    let buttons = which.clone().filter(|&i| {
        (!legacy || ![5, 6, 8].contains(&i))
            && (c.preview || c.s.bar.editing || shown(WINDOW_BUTTONS[i]))
    });
    for (slot, i) in buttons.enumerate() {
        let (icon, ref action, hint) = wins[i];
        let r = a.r(btn(973.0 + 33.0 * (which.start + slot) as f32, 45.0));
        if r.right() > c.fit {
            return false;
        }
        let hov = ui.hit(r, action.clone(), hint) || ui.down(r, action);
        if c.s.vintage_2005 && i < 5 {
            // FL4 orders Playlist, Rack, Piano Roll, Browser and Mixer.
            let frame = [0, 4, 2, 8, 6][i] + usize::from(hov);
            if crate::vintage_skin::native(ui.c, "SCBtns", frame, (r.cx()-14.5).round(), (r.cy()-12.5).round()) {
                continue;
            }
        }
        let ink = super::tool_face(ui, r, if hov { super::Look::Hot } else { super::Look::Off });
        if c.s.vintage_2005 {
            ui.c.fill_rrect(r.inset(1.),12.,Color::rgb(0x23353b));
            ui.c.fill_rect_gradient(r.inset(2.),Color::rgb(0x87999e),Color::rgb(0x344d55),11.);
            ui.c.stroke_rrect(r.inset(4.),9.,1.,Color::rgb(0x293f46));
            icons::draw(ui.c,icon,Rect::new(r.x+7.,r.y+8.,16.,16.),if hov {ui.t.accent}else{Color::rgb(0xc3d7d0)});
        } else {icons::draw(ui.c, icon, Rect::new(r.x + 5.0, r.y + 6.5, 20.0, 20.0), ink);}
    }
    // FL's news panel (x 1304..1470, with its separator) is left out: it suggested an online service Lemon Studio
    // does not have. A tips panel may take its place (spec: ui/main-window.md §3).
    true
}

/// The editor's outline around something that can be dragged.
fn frame(ui: &mut Ui, r: Rect) {
    let c = ui.t.glyph_dim.with_alpha(130);
    ui.c.stroke_rrect(r, 3.0, 1.0, c);
}

/// While editing, a panel on the toolbar is one thing to drag: its controls' hits are dropped (`Ui::forget`) for one of
/// its own, and it is framed. The spaces show as hatched boxes.
fn item_frame(ui: &mut Ui, l: &crate::toolbar::Layout, p: &Placed) {
    let r = p.rect(l);
    let name = p.panel.name();
    ui.hit(r, Action::Bar(BarHit::Item(p.row, p.idx)), format!("{name} - drag to move it, onto the palette to take it off"));
    if p.panel.repeats() {
        space_picture(ui, r.inset(3.0), p.panel);
    }
    frame(ui, r.inset(1.0));
}

/// A space: a hatched box, a flexible one with arrows to both sides.
fn space_picture(ui: &mut Ui, r: Rect, p: Panel) {
    let t = ui.t;
    ui.c.push_clip(r);
    let mut x = r.x - r.h;
    while x < r.right() {
        ui.c.line(x, r.bottom(), x + r.h, r.y, 1.0, t.glyph_dim.with_alpha(60));
        x += 6.0;
    }
    ui.c.pop_clip();
    if p == Panel::Flex && r.w >= 24.0 {
        use os_ui::kit::{triangle, Dir};
        triangle(ui.c, r.x + 6.0, r.cy(), 3.5, Dir::Left, t.glyph_dim);
        triangle(ui.c, r.right() - 6.0, r.cy(), 3.5, Dir::Right, t.glyph_dim);
    }
}

/// While editing, each row starts with a grip (a column of short dashes) that drags the row.
fn grips(ui: &mut Ui, s: &State) {
    let t = ui.t;
    let l = &s.bar.layout;
    for row in 0..l.rows.len() {
        let (top, bottom) = l.row_span(row);
        let r = Rect::new(0.0, top, crate::toolbar::GRIP_W, bottom - top);
        let hov = ui.hit(r, Action::Bar(BarHit::Grip(row)), "Row - drag onto the other row to swap them");
        let c = if hov { t.glyph } else { t.glyph_dim };
        let mut y = top + 6.0;
        while y + 2.0 <= bottom - 5.0 {
            ui.c.fill_rect(Rect::new(3.0, y, 4.0, 1.0), c);
            y += 4.0;
        }
    }
}

/// Palette spacing (band px): padding, the gap between tiles, the label under a tile, a line of tiles, the bottom bar.
const PAD: f32 = 10.0;
const GAP: f32 = 14.0;
const LABEL_H: f32 = 16.0;
const LINE_H: f32 = ROW_H + LABEL_H + 8.0;
const BAR_H: f32 = 30.0;

/// The editor's palette under the band (band px): where it is, its tiles (each the picture's rect, its name under it)
/// and its bottom bar.
pub struct PaletteLayout {
    pub rect: Rect,
    pub tiles: Vec<(Tile, Rect)>,
    pub bar: Rect,
}

/// A tile's picture width.
fn tile_w(t: Tile) -> f32 {
    match t {
        Tile::Panel(p) => p.width().max(64.0),
        Tile::Logo => 92.0,
    }
}

/// The palette of a band `width` px wide: every panel and the logo as tiles, line after line, the bottom bar below.
pub fn palette(s: &State, width: f32) -> PaletteLayout {
    let top = s.bar.layout.band_h();
    let (mut x, mut y) = (PAD, top + PAD);
    let mut tiles = Vec::new();
    for tile in PANELS.iter().map(|&p| Tile::Panel(p)).chain([Tile::Logo]) {
        let w = tile_w(tile);
        if x > PAD && x + w > width - PAD {
            x = PAD;
            y += LINE_H;
        }
        tiles.push((tile, Rect::new(x, y, w, ROW_H)));
        x += w + GAP;
    }
    let bar = Rect::new(0.0, y + ROW_H + LABEL_H + PAD, width, BAR_H);
    PaletteLayout { rect: Rect::new(0.0, top, width, bar.bottom() - top), tiles, bar }
}

/// Whether tile `tile` is on the toolbar already (dimmed: it cannot be added twice).
fn placed_already(s: &State, tile: Tile) -> bool {
    match tile {
        Tile::Panel(p) => !p.repeats() && s.bar.layout.has(p),
        Tile::Logo => s.bar.layout.logo,
    }
}

/// The toolbar editor's palette (spec: ui/toolbar-menu.md §8), right under the band, the window's width: a tile for
/// every panel (a picture of it, its name under it), dimmed when it is on the toolbar already; at the bottom, what to
/// do, the layout's preset with its menu, the rows and Done. Then what a drag carries, under the mouse.
pub fn paint_palette(ui: &mut Ui, s: &State, f: &FrameInfo, width: f32) {
    let t = ui.t;
    let p = palette(s, width);
    let bg = t.toolbar.shade(-0.06);
    ui.c.fill_rect(p.rect, bg);
    ui.c.fill_rect(Rect::new(0.0, p.rect.bottom() - 1.0, width, 1.0), t.toolbar_line);
    ui.hit(p.rect, Action::Bar(BarHit::Palette), "");
    for &(tile, r) in &p.tiles {
        let name = match tile {
            Tile::Panel(p) => p.name(),
            Tile::Logo => "Logo",
        };
        ui.c.fill_rrect(r, 3.0, t.toolbar);
        ui.c.push_clip(r);
        match tile {
            Tile::Panel(pn) if pn.repeats() => space_picture(ui, r.inset(4.0), pn),
            Tile::Panel(pn) => {
                let (row, x0, x1) = pn.home();
                // Centred in a tile wider than the panel.
                let dx = ((r.w - (x1 - x0)) / 2.0).floor();
                let a = At { dx: r.x + dx - x0, dy: r.y - ROW_H * row as f32 };
                let mark = ui.mark();
                panel(ui, &Ctx { s, f, a, fit: f32::INFINITY, preview: true }, pn);
                ui.forget(mark);
            }
            Tile::Logo => icons::logo(ui.c, r.cx(), r.cy(), 13.0, if super::lemon(ui) { t.toolbar_edge } else { t.toolbar_line }),
        }
        ui.c.pop_clip();
        let label = Rect::new(r.x - GAP / 2.0, r.bottom() + 2.0, r.w + GAP, LABEL_H - 2.0);
        if placed_already(s, tile) {
            ui.veil(r.outset(1.0), bg);
            ui.hit(r, Action::Bar(BarHit::Palette), format!("{name} is on the toolbar"));
            ui.c.text_center(Font::new(12), label, t.text_dim.mix(bg, 0.5), name);
        } else {
            let hov = ui.hit(r, Action::Bar(BarHit::Tile(tile)), format!("{name} - drag onto the toolbar"));
            frame(ui, r.outset(1.0));
            ui.c.text_center(Font::new(12), label, if hov { t.text } else { t.text_dim }, name);
        }
    }

    // The bottom bar.
    let b = p.bar;
    ui.c.fill_rect(b, t.toolbar.shade(-0.1));
    ui.c.fill_rect(Rect::new(0.0, b.y, width, 1.0), t.toolbar_line);
    let font = Font::new(13);
    let text = "Drag panels to and from the toolbar";
    ui.c.text(font, PAD, b.y + 7.0, t.text_dim, text);
    let x = PAD + ui.c.text_width(font, text) + 24.0;
    let name = format!("Preset: {}", s.bar.name());
    let pr = Rect::new(x, b.y + 4.0, ui.c.text_width(font, &name) + 30.0, BAR_H - 8.0);
    let hov = ui.hit(pr, Action::Bar(BarHit::Presets), "The toolbar presets, Save preset as... and Delete preset");
    os_ui::kit::combo(ui.c, t, pr, &name, font, os_ui::kit::State::hover(hov));
    let done = Rect::new(width - PAD - 70.0, b.y + 4.0, 70.0, BAR_H - 8.0);
    let hov = ui.hit(done, Action::Bar(BarHit::Done), "Done: the toolbar keeps this layout");
    os_ui::kit::button(ui.c, t, done, os_ui::kit::State::hover(hov));
    ui.c.text_center(font, done, t.text, "Done");
    let rows = s.bar.layout.rows.len();
    for (i, label) in ["1 row", "2 rows"].into_iter().enumerate() {
        let r = Rect::new(done.x - 24.0 - 64.0 * (2 - i) as f32, b.y + 5.0, 60.0, BAR_H - 10.0);
        let hov = ui.hit(r, Action::Bar(BarHit::Rows(i + 1)), format!("The toolbar in {label}"));
        os_ui::kit::pill(ui.c, t, r, label, Font::new(12), os_ui::kit::State::new(hov, false, rows == i + 1));
    }

    if let Some(d) = f.bar_drag {
        ghost(ui, s, &d, &p, width);
    }
}

/// What a drag carries, under the mouse, and where it would land: a caret between the panels of a row.
fn ghost(ui: &mut Ui, s: &State, d: &crate::toolbar::BarDrag, p: &PaletteLayout, width: f32) {
    let t = ui.t;
    let l = &s.bar.layout;
    let (name, w, h) = match d.moving {
        Moving::Item(r, i) => match l.rows.get(r).and_then(|row| row.get(i)) {
            Some(&pn) => (pn.name(), pn.width().max(40.0), ROW_H),
            None => return,
        },
        Moving::Tile(tile @ Tile::Panel(pn)) => (pn.name(), tile_w(tile), ROW_H),
        Moving::Tile(Tile::Logo) | Moving::Logo => ("Logo", tile_w(Tile::Logo), ROW_H),
        Moving::Row(_) => ("Row", width, ROW_H),
    };
    let (mx, my) = ui.mouse;
    let r = Rect::new(mx - d.grab.0, my - d.grab.1, w, h);
    ui.c.fill_rrect(r, 4.0, t.pill.with_alpha(50));
    ui.c.stroke_rrect(r, 4.0, 1.5, t.pill);
    ui.c.text_center(Font::new(13), r, t.text, name);
    let to = crate::toolbar::drop_at(&s.bar, width, d, (mx, my), (p.rect.y, p.rect.bottom()));
    let Target::Insert { row, idx } = to else { return };
    if matches!(d.moving, Moving::Row(_) | Moving::Logo | Moving::Tile(Tile::Logo)) {
        return;
    }
    let skip = match d.moving {
        Moving::Item(r, i) => Some((r, i)),
        _ => None,
    };
    let others: Vec<Placed> = place(l, width, true).into_iter().filter(|q| q.row == row && Some((q.row, q.idx)) != skip).collect();
    let x = others.get(idx).map_or_else(|| others.last().map_or(crate::toolbar::GRIP_W, |q| q.x + q.w), |q| q.x);
    let (top, bottom) = l.row_span(row);
    ui.c.fill_rect(Rect::new(x - 1.0, top + 2.0, 2.0, bottom - top - 4.0), t.pill);
}

/// Minutes, seconds and centiseconds of a position in seconds.
pub fn msc_ticks(secs: f64) -> (u32, u32, u32) {
    let cs = (secs.max(0.0) * 100.0) as u32;
    (cs / 6000, cs / 100 % 60, cs % 100)
}

/// Bar:step:tick (1-based bar and step, 16 steps per bar, `ppq / 4` ticks per step).
#[cfg(test)]
pub fn bst(seconds: f64, tempo: f64, ppq: u32) -> (u32, u32, u32) {
    bst_ticks(seconds * tempo / 60.0 * f64::from(ppq))
}

/// Bar:step:tick of a position given in ticks of a 96-per-quarter clock (the display's own resolution).
pub fn bst_ticks(ticks: f64) -> (u32, u32, u32) {
    let ticks = ticks.max(0.0) as u32;
    let per_step = 96 / 4;
    let steps = ticks / per_step;
    (steps / 16 + 1, steps % 16 + 1, ticks % per_step)
}

#[cfg(test)]
mod tests {
    use super::bst;

    #[test]
    fn bar_step_tick() {
        assert_eq!(bst(0.0, 120.0, 96), (1, 1, 0));
        // 120 BPM: one bar (4 beats) = 2 s.
        assert_eq!(bst(2.0, 120.0, 96), (2, 1, 0));
        assert_eq!(bst(0.125, 120.0, 96), (1, 2, 0));
    }

    use crate::app::{Action, App, Win};
    use crate::prefs::{choice, set_choice, TOOLBARS_SCALING};
    use os_ui::{Canvas, Rect};

    fn hit(app: &App, a: &Action) -> Rect {
        app.hits.iter().rev().find(|h| &h.action == a).map(|h| h.rect).unwrap_or_else(|| panic!("no hit for {a:?}"))
    }

    fn has(app: &App, a: &Action) -> bool {
        app.hits.iter().any(|h| &h.action == a)
    }

    fn click(app: &mut App, c: &mut Canvas, x: f32, y: f32) {
        app.mouse_move(x, y);
        app.mouse_down();
        app.mouse_up();
        app.paint(c, None);
    }

    fn twice(r: Rect) -> Rect {
        Rect::new(r.x * 2.0, r.y * 2.0, r.w * 2.0, r.h * 2.0)
    }

    /// Toolbars scaling (spec: ui/ui-scaling.md §1), chosen on the settings page: at 200 % on a 100 % window the top band
    /// is twice as tall at once and its controls twice as big; the browser, the workspace and the windows in it keep
    /// their places below the band.
    #[test]
    fn toolbars_scaling_scales_the_top_band() {
        let Ok(mut c) = Canvas::new(4600, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.state.show_browser = true;
        app.paint(&mut c, None);
        assert_eq!(choice(&TOOLBARS_SCALING), "Main");
        assert_eq!((crate::views::band_h(&app.state), app.state.work_rect().y), (83.0, 83.0));
        let (clock, tab) = (hit(&app, &Action::Clock), hit(&app, &Action::BrowserTab(1)));
        assert_eq!(app.state.win_rect(Win::Playlist).y, 83.0, "docked under the band");

        app.open_settings(crate::views::settings::GENERAL);
        app.paint(&mut c, None);
        let combo = app
            .hits
            .iter()
            .rev()
            .find(|h| matches!(&h.action, Action::Settings(crate::views::settings::SAct::Combo(l, ..)) if *l == TOOLBARS_SCALING.label))
            .map(|h| h.rect)
            .expect("the combo works");
        click(&mut app, &mut c, combo.cx(), combo.cy());
        let labels: Vec<String> = app.state.playlist.popup.as_ref().expect("its menu").items.iter().map(|it| it.label.to_string()).collect();
        assert_eq!(labels, TOOLBARS_SCALING.choices);
        let item = hit(&app, &Action::PlPopup(5));
        click(&mut app, &mut c, item.cx(), item.cy());
        assert_eq!((choice(&TOOLBARS_SCALING), app.state.message.as_deref()), ("200%", Some("Toolbars scaling: 200%")));
        app.state.settings.open = false;
        app.paint(&mut c, None);

        // The band: 166 px instead of 83, its controls doubled; the rest keeps its place under it.
        assert_eq!((crate::views::band_h(&app.state), app.state.work_rect().y), (166.0, 166.0));
        assert_eq!(hit(&app, &Action::Clock), twice(clock));
        let drag = app.hits.iter().find(|h| h.action == Action::DragWindow).map(|h| h.rect);
        assert_eq!(drag, Some(Rect::new(0.0, 0.0, 4600.0, 162.0)));
        assert_eq!(hit(&app, &Action::BrowserTab(1)), Rect { y: tab.y + 83.0, ..tab });
        assert_eq!(app.state.win_rect(Win::Playlist).y, 166.0, "docked under the band");
        // A floating window keeps its distance to the band when the band shrinks back.
        app.state.wins[0].maxed = false;
        app.state.wins[0].rect = Rect::new(400.0, 300.0, 900.0, 500.0);
        app.state.raise(Win::Playlist);
        app.paint(&mut c, None);
        assert_eq!(app.pref_choose(&TOOLBARS_SCALING, 0).as_deref(), Some("Toolbars scaling: Main"));
        app.paint(&mut c, None);
        assert_eq!((crate::views::band_h(&app.state), app.state.win_rect(Win::Playlist).y), (83.0, 217.0));
        assert_eq!(hit(&app, &Action::Clock), clock);
    }

    /// At 200 % the band's controls answer where they are drawn: the metronome, the menu bar (whose FILE menu keeps
    /// Pop-ups scaling, rows 19 px at Main on a 100 % window) and the tempo's digits; repaints cover the scaled panels.
    #[test]
    fn band_controls_are_hit_where_drawn() {
        let Ok(mut c) = Canvas::new(4600, 1080, 1.0) else { return };
        let mut app = App::new(None);
        set_choice(&TOOLBARS_SCALING, 5);
        app.paint(&mut c, None);
        // The metronome (580, 4, 30 x 33 in the band's px), pressed near its bottom-right corner, where the Main layout
        // has another button.
        let m = hit(&app, &Action::Metronome);
        assert_eq!(m, Rect::new(1160.0, 8.0, 60.0, 66.0));
        let was = crate::metronome::on();
        click(&mut app, &mut c, m.right() - 4.0, m.bottom() - 4.0);
        assert_eq!(crate::metronome::on(), !was);
        // FILE opens its menu under its title, at Pop-ups scaling.
        let file = hit(&app, &Action::Menu(0));
        assert!(file.y >= 90.0 && file.h > 50.0, "{file:?}");
        click(&mut app, &mut c, file.cx(), file.cy());
        assert_eq!(app.state.menu_open, Some(0));
        let save = hit(&app, &crate::views::menus::pick(0, crate::views::menus::FILE_SAVE));
        app.state.menu_open = None;
        // Under its title, which moved: at Main the same menu opens 78 px higher (the bottom row ends at 78, not 156)
        // and 4 px further left (its left edge, 4 px, doubled); same rows, same width.
        set_choice(&TOOLBARS_SCALING, 0);
        app.paint(&mut c, None);
        let main = hit(&app, &Action::Menu(0));
        click(&mut app, &mut c, main.cx(), main.cy());
        let save0 = hit(&app, &crate::views::menus::pick(0, crate::views::menus::FILE_SAVE));
        assert_eq!((save.h, save0.h, save.w, save.x - save0.x, save.y - save0.y), (19.0, 19.0, save0.w, 4.0, 78.0), "{save:?} {save0:?}");
        app.state.menu_open = None;
        set_choice(&TOOLBARS_SCALING, 5);
        app.paint(&mut c, None);
        // The tempo: the first decimal steps by 0.1.
        let tempo = hit(&app, &Action::Tempo);
        let digits = app.state.tempo_digits.get();
        assert!(digits[0].is_finite() && tempo.contains(2.0 * digits[0] + 2.0, tempo.cy()));
        app.mouse_move(2.0 * digits[0] + 2.0, tempo.cy());
        app.mouse_down();
        assert!(matches!(app.drag, Some(crate::app::Drag::Tempo { step, .. }) if step == 0.1));
        app.mouse_up();
        // A change in the hint panel repaints the panel where it is drawn.
        app.paint(&mut c, None);
        let _ = app.take_damage();
        app.damage_band(crate::app::HINT_RECT);
        let panel = crate::views::band_rect(&app.state, crate::app::HINT_RECT);
        let damage = app.take_damage().flatten().expect("rects");
        assert!(damage.iter().any(|r| r.contains(panel.x, panel.y) && r.contains(panel.right() - 1.0, panel.bottom() - 1.0)), "{damage:?}");
        set_choice(&TOOLBARS_SCALING, 0);
    }

    #[test]
    fn window_buttons_stay_clickable_after_resizing() {
        let mut app = App::new(None);
        let windows = [Win::Playlist, Win::PianoRoll, Win::ChannelRack, Win::Mixer, Win::Browser];
        app.state.toolbar.hint_bar = true;
        for (width, scale) in [(2400, 1.0), (2400, 2.0), (1440, 2.0), (800, 1.0), (2400, 1.0)] {
            let mut canvas = Canvas::new(width, 1000, scale).unwrap();
            app.paint(&mut canvas, None);
            let ratio = crate::views::band_k(&app.state);
            let preferred = crate::views::toolbar_scale(scale) / scale;
            assert!(ratio >= preferred * 0.8 && ratio <= preferred);
            if width == 800 {
                assert!((ratio - preferred * 0.8).abs() < 0.001);
                assert!(super::display_layout(&app.state).rows.len() > 1);
            }
            let hint = app.hits.iter().find(|h| h.hint == "Hint bar").unwrap().rect;
            let report = hit(&app, &Action::FeatureVote(0,30));
            assert!(hint.x >= report.right() && hint.right() <= hit(&app, &Action::Minimize).x);
            assert!(!has(&app, &Action::MenuItem(super::menus::OPTIONS_MENU, super::menus::OPT_INFO)));
            let mut last_right = 0.0;
            for win in windows {
                let button = hit(&app, &Action::ToggleWin(win));
                assert!(button.x >= last_right && button.right() <= app.state.screen.0);
                assert!(button.w > 0.0 && button.bottom() <= crate::views::band_h(&app.state));
                last_right = button.right();
            }
            let browser = hit(&app, &Action::ToggleWin(Win::Browser));
            let was_open = app.state.show_browser;
            click(&mut app, &mut canvas, browser.cx(), browser.cy());
            assert_ne!(app.state.show_browser, was_open);
        }
    }

}
