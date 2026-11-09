// SPDX-License-Identifier: GPL-3.0-or-later
//! Visual editing of Lemon Studio's optional .flstheme extensions.

use crate::app::{Action, App, State, Ui};
use crate::dialog::{Field, Then};
use crate::filedlg::{FileDlg, Mode};
use crate::mixer_skin::{self, Designer, Part, PreviewState, Scope};
use crate::theme_library::{History, Snap, Step};
use crate::views::settings::{self, SAct};
use os_theme::{Color, Palette, Theme, appearance, fl};
use os_ui::{Font, Rect};
use std::path::Path;
use winit::keyboard::{Key, NamedKey};

pub const OPEN: &str = "Open Theme Editor...";
pub const APPLY: &str = "Apply theme";
pub const CANCEL: &str = "Cancel edits";
pub const EXPORT: &str = "Export theme...";
pub const WALLPAPER: &str = "Choose wallpaper...";
pub const CLEAR_WALLPAPER: &str = "Clear wallpaper";
const EXPORT_SKIN: &str = "LemonStudioExportSkin";
pub const VERSION_KEY: &str = "LemonStudioThemeVersion";

pub fn is_editor_theme(text: &str) -> bool {
    fl::get_key(text, VERSION_KEY) == Some("1")
}

pub fn export_skin(text: &str) -> Option<os_theme::Design> {
    fl::get_key(text, EXPORT_SKIN).and_then(os_theme::Design::parse)
}

#[derive(Debug)]
pub struct Session {
    original: Theme,
    snapshot: Snap,
    history: History,
    pub query: Field,
    pub groups: Vec<(&'static str, Vec<Row>)>,
    pub mixer_designer: Option<Designer>,
    pub identity: bool,
    field: Field,
    focus: Option<FieldFor>,
}

impl Session {
    pub(crate) fn original_for(&self, snap: &Snap) -> Option<Theme> {
        (snap.text == self.snapshot.text
            && snap.base == self.snapshot.base
            && snap.design == self.snapshot.design)
            .then(|| self.original.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFor {
    Search,
    Hex(&'static str),
    Number(usize),
    IdentityValue(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Field(FieldFor),
    Color(&'static str),
    Choice(usize, &'static str),
    ResetColor(&'static str),
    ResetStyle(usize),
    ResetGroup(&'static str),
    Mixer(MixerHit),
    Identity(IdentityHit),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixerHit {
    Scope(Scope),
    Part(Part),
    State(PreviewState),
    Value(u8),
    Style(u8),
    Detail(u8),
    Edge(u8),
    Enabled(bool),
    Color(&'static str),
    ImportArt,
    RemoveArt,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IdentityHit {
    Pack(u8),
    CursorPack(u8),
    CursorColor,
    ResetCursorColor,
    FollowIcons,
    IconFamily(u8, u8),
    CursorFamily(u8, u8),
    ResetArtwork,
    Font(u8),
    Import,
    Remove,
}

#[derive(Debug, Clone, Copy)]
pub enum IdentityRow {
    Packs(u8),
    CursorPacks(u8),
    Icon(usize),
    Cursor(usize),
    Appearance,
    FollowIcons,
    Fonts,
    Preview,
    Files,
}

#[derive(Debug, Clone, Copy)]
pub enum MixerRow {
    Target,
    Components,
    Preview,
    States,
    Value,
    Style,
    Detail,
    Edge,
    Colors,
    Artwork,
}

#[derive(Debug, Clone, Copy)]
pub enum Row {
    Search,
    Preview,
    Color(&'static str),
    Style(usize),
    Reset(&'static str),
    Mixer(MixerRow),
    Identity(IdentityRow),
}

impl Row {
    pub fn height(self) -> f32 {
        match self {
            Self::Preview => 76.0,
            Self::Mixer(MixerRow::Preview) => 128.0,
            Self::Mixer(_) => 34.0,
            Self::Identity(IdentityRow::Preview) => 170.0,
            Self::Identity(IdentityRow::Appearance) => 100.0,
            Self::Identity(_) => 36.0,
            Self::Search => 35.0,
            _ => 29.0,
        }
    }
}

enum Values {
    Choices(&'static [&'static str]),
    Number(f32),
}

struct StyleField {
    key: &'static str,
    label: &'static str,
    values: Values,
}

const STYLES: &[StyleField] = &[
    StyleField {
        key: "buttons",
        label: "Button finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
    StyleField {
        key: "panels",
        label: "Panel finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
    StyleField {
        key: "indicator",
        label: "Indicator shape",
        values: Values::Choices(&["Inherit", "Round", "Square", "Bar"]),
    },
    StyleField {
        key: "knob",
        label: "Knob style",
        values: Values::Choices(&["Inherit", "Dial", "Ring"]),
    },
    StyleField {
        key: "mixer",
        label: "Mixer finish",
        values: Values::Choices(&[
            "Inherit",
            "Original",
            "Clean Modern",
            "Analog",
            "Theme colors",
        ]),
    },
    StyleField {
        key: "button-radius",
        label: "Button radius (0–12 px)",
        values: Values::Number(12.0),
    },
    StyleField {
        key: "small-radius",
        label: "Small detail radius (0–8 px)",
        values: Values::Number(8.0),
    },
    StyleField {
        key: "panel-radius",
        label: "Panel radius (0–20 px)",
        values: Values::Number(20.0),
    },
    StyleField {
        key: "bevel",
        label: "Bevel depth (0–40%)",
        values: Values::Number(40.0),
    },
    StyleField {
        key: "border",
        label: "Border width (0–3 px)",
        values: Values::Number(3.0),
    },
    StyleField {
        key: "title-text",
        label: "Window title lettering",
        values: Values::Choices(&["Inherit", "Bold", "Italic", "Light"]),
    },
    StyleField {
        key: "playlist-lines",
        label: "Playlist grid contrast (0–200%)",
        values: Values::Number(200.0),
    },
    StyleField {
        key: "piano-lines",
        label: "Piano Roll grid contrast (0–200%)",
        values: Values::Number(200.0),
    },
    StyleField {
        key: "playlist-buttons",
        label: "Playlist tool finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
    StyleField {
        key: "piano-buttons",
        label: "Piano Roll tool finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
    StyleField {
        key: "rack-steps",
        label: "Channel Rack step finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
    StyleField {
        key: "mixer-buttons",
        label: "Themed Mixer button finish",
        values: Values::Choices(&appearance::Finish::CHOICES),
    },
];

const SECTIONS: &[(&str, &str)] = &[
    ("Toolbar & shared controls", "toolbar"),
    ("Browser", "browser_title"),
    ("Window frames", "splitter"),
    ("Playlist", "picker"),
    ("Mixer", "mixer_bg"),
    ("Channel Rack", "rack_row_box"),
    ("Menus & dialogs", "menu_drop"),
    ("Piano Roll", "pr_row"),
    ("Control finishes", "button_pressed"),
    ("Piano Roll note groups", "note_group_01"),
];

fn section(name: &str) -> &'static str {
    if name.starts_with("skin_") {
        return "Mixer";
    }
    match name {
        "workspace" | "watermark" | "version_text" => return "Workspace",
        "clip" => return "Playlist",
        "mixer_plate" => return "Mixer",
        _ => {}
    }
    let mut area = SECTIONS[0].0;
    for &key in Palette::NAMES {
        if let Some(&(title, _)) = SECTIONS.iter().find(|(_, start)| *start == key) {
            area = title;
        }
        if key == name {
            break;
        }
    }
    area
}

fn matches(query: &str, label: &str, hint: &str, group: &str) -> bool {
    query.is_empty() || {
        let text = format!("{label} {hint} {group}")
            .to_ascii_lowercase()
            .replace('_', " ");
        query.split_whitespace().all(|part| text.contains(part))
    }
}

fn color_label(key: &str) -> String {
    let mut label = key.replace('_', " ");
    label[..1].make_ascii_uppercase();
    label
}

pub fn groups(query: &str) -> Vec<(&'static str, Vec<Row>)> {
    let query = query.trim().to_ascii_lowercase();
    let mut result = Vec::new();
    let styles = STYLES
        .iter()
        .enumerate()
        .filter(|(_, def)| matches(&query, def.label, def.key, "Shape & finish"))
        .map(|(i, _)| Row::Style(i))
        .collect::<Vec<_>>();
    if !styles.is_empty() {
        result.push(("Shape & finish", styles));
    }
    for title in SECTIONS
        .iter()
        .map(|&(title, _)| title)
        .chain(["Workspace"])
    {
        let mut rows = Palette::NAMES
            .iter()
            .copied()
            .filter(|key| {
                section(key) == title && matches(&query, key, Palette::description(key), title)
            })
            .map(Row::Color)
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            rows.push(Row::Reset(title));
            result.push((title, rows));
        }
    }
    result
}

fn color_key(token: &str) -> String {
    format!("{}{token}", appearance::COLOR_PREFIX)
}

fn style_key(index: usize) -> String {
    format!("{}{}", appearance::STYLE_PREFIX, STYLES[index].key)
}

pub fn set_color(text: &str, token: &str, color: Color) -> String {
    fl::set_key(text, &color_key(token), &hex(color))
}

fn hex(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn value_label(t: &Palette, field: FieldFor, text: &str) -> String {
    match field {
        FieldFor::IdentityValue(key) => fl::get_key(text, key).unwrap_or("").into(),
        FieldFor::Search => "Find a color or style...".into(),
        FieldFor::Hex(token) => t.get(token).map(hex).unwrap_or_default(),
        FieldFor::Number(i) => fl::get_key(text, &style_key(i)).unwrap_or("Inherit").into(),
    }
}

fn button(ui: &mut Ui, rect: Rect, hit: Hit, label: &str, hint: &str, selected: bool) {
    let hover = ui.hit(
        rect,
        Action::Settings(SAct::ThemeEditor(hit)),
        hint.to_owned(),
    );
    let t = ui.t;
    let fill = if selected {
        t.accent
    } else if hover {
        t.button_hi
    } else {
        t.button
    };
    let ink = if selected {
        os_ui::kit::lemon::ink_on(fill)
    } else {
        t.text
    };
    ui.c.fill_rrect(rect, 3.0, fill);
    ui.c.text_center(Font::new(12), rect, ink, label);
}

fn field(ui: &mut Ui, r: Rect, session: &Session, kind: FieldFor, text: &str) {
    let label = if kind == FieldFor::Search && !session.query.text.is_empty() {
        session.query.text.clone()
    } else {
        value_label(ui.theme_preview.unwrap_or(ui.t), kind, text)
    };
    let hover = ui.hit(
        r,
        Action::Settings(SAct::ThemeEditor(Hit::Field(kind))),
        "Click to edit",
    );
    ui.c.fill_rrect(r, 3.0, ui.t.sunken);
    let typing = (session.focus == Some(kind)).then_some(if kind == FieldFor::Search {
        &session.query
    } else {
        &session.field
    });
    settings::cell_text(ui, r, &label, typing, hover);
}

pub fn paint(ui: &mut Ui, s: &State, row: Row, r: Rect) {
    let Some(session) = &s.settings.advanced_theme else {
        return;
    };
    let t = ui.theme_preview.unwrap_or(ui.t);
    let text = s.fl_text.as_deref().unwrap_or("");
    let control = Rect::new(r.x + 228.0, r.y + 2.0, 90.0, 24.0);
    let label = |ui: &mut Ui, title: &str| {
        ui.c.text_clip(Font::new(13), r.x, r.y + 6.0, 220.0, ui.t.text, title)
    };
    match row {
        Row::Search => field(
            ui,
            Rect::new(r.x, r.y + 3.0, 315.0, 28.0),
            session,
            FieldFor::Search,
            text,
        ),
        Row::Preview => preview(ui, r),
        Row::Mixer(row) => paint_mixer(ui, session, row, r),
        Row::Identity(row) => paint_identity(ui, s, row, r),
        Row::Color(token) => {
            let color = t.get(token).expect("palette color");
            label(ui, &color_label(token));
            let chip = Rect::new(control.x, control.y, 34.0, control.h);
            let hint = Palette::description(token);
            ui.hit(
                chip,
                Action::Settings(SAct::ThemeEditor(Hit::Color(token))),
                hint,
            );
            ui.c.fill_rrect(chip, 3.0, t.window_border);
            ui.c.fill_rrect(chip.inset(2.0), 2.0, color);
            field(
                ui,
                Rect::new(chip.right() + 6.0, chip.y, 82.0, chip.h),
                session,
                FieldFor::Hex(token),
                text,
            );
            if fl::get_key(text, &color_key(token)).is_some() {
                button(
                    ui,
                    Rect::new(chip.x + 128.0, chip.y, 58.0, chip.h),
                    Hit::ResetColor(token),
                    "Reset",
                    "Use the inherited color",
                    false,
                );
            }
            ui.c.text_clip(
                Font::new(11),
                r.x + 422.0,
                r.y + 7.0,
                (r.w - 422.0).max(0.0),
                ui.t.text_dim,
                hint,
            );
        }
        Row::Style(i) => {
            let def = &STYLES[i];
            label(ui, def.label);
            match def.values {
                Values::Choices(choices) => {
                    let mut x = control.x;
                    let current = fl::get_key(text, &style_key(i))
                        .unwrap_or("Inherit")
                        .to_string();
                    for &choice in choices {
                        let w = ui.c.text_width(Font::new(12), choice).ceil() + 16.0;
                        let b = Rect::new(x, control.y, w, control.h);
                        button(
                            ui,
                            b,
                            Hit::Choice(i, choice),
                            choice,
                            def.label,
                            current.eq_ignore_ascii_case(choice),
                        );
                        x += w + 3.0;
                    }
                }
                Values::Number(_) => {
                    field(ui, control, session, FieldFor::Number(i), text);
                    if fl::get_key(text, &style_key(i)).is_some() {
                        button(
                            ui,
                            Rect::new(control.right() + 6.0, control.y, 58.0, control.h),
                            Hit::ResetStyle(i),
                            "Reset",
                            "Use the inherited value",
                            false,
                        );
                    }
                }
            }
        }
        Row::Reset(group) => {
            if Palette::NAMES
                .iter()
                .any(|key| section(key) == group && fl::get_key(text, &color_key(key)).is_some())
            {
                button(
                    ui,
                    Rect::new(r.x, r.y + 2.0, 120.0, 24.0),
                    Hit::ResetGroup(group),
                    "Reset section colors",
                    "Remove custom colors in this section",
                    false,
                );
            }
        }
    }
}

fn preview(ui: &mut Ui, r: Rect) {
    use os_ui::kit::{self, State};
    let t = ui.theme_preview.unwrap_or(ui.t);
    let body = Rect::new(r.x, r.y + 5.0, r.w - 2.0, 59.0);
    if !kit::panel_finish(ui.c, t, body, t.toolbar) {
        ui.c.fill_rrect(body, t.style.radius_panel, t.toolbar);
    }
    ui.hit(
        body,
        Action::Settings(SAct::ThemeEditor(Hit::Color("toolbar"))),
        "Edit toolbar color",
    );
    let mut x = body.x + 12.0;
    for (label, token, state) in [
        ("Normal", "button", State::NORMAL),
        ("Hover", "button_hi", State::hover(true)),
        ("Held", "button_pressed", State::new(true, true, false)),
        ("Active", "on_top", State::NORMAL.with_on(true)),
        (
            "Disabled",
            "button_disabled",
            State::NORMAL.with_disabled(true),
        ),
    ] {
        let b = Rect::new(x, body.y + 14.0, 68.0, 30.0);
        ui.hit(
            b,
            Action::Settings(SAct::ThemeEditor(Hit::Color(token))),
            "Edit this state",
        );
        kit::button(ui.c, t, b, state);
        ui.c.text_center(Font::new(11), b, kit::ink(t, state), label);
        x += 73.0;
    }
    let knob = Rect::new(x + 10.0, body.y + 14.0, 30.0, 30.0);
    ui.hit(
        knob,
        Action::Settings(SAct::ThemeEditor(Hit::Color("knob_cap"))),
        "Edit knob cap",
    );
    kit::knob(
        ui.c,
        t,
        knob.cx(),
        knob.cy(),
        13.0,
        0.72,
        false,
        kit::KnobLook::Ring,
        State::NORMAL,
    );
    let led = Rect::new(knob.right() + 16.0, knob.y, 22.0, knob.h);
    ui.hit(
        led,
        Action::Settings(SAct::ThemeEditor(Hit::Color("indicator_on"))),
        "Edit indicator color",
    );
    kit::led(
        ui.c,
        t,
        led.cx(),
        led.cy(),
        8.0,
        State::NORMAL.with_on(true),
    );
}

// Activate mixing deliberately, without reviving hidden overrides from an older
// Default-based theme when the user edits just one icon.
fn explicit_icon_mix(text: &str) -> String {
    let mut next = text.to_owned();
    if fl::get_key(text, "LemonStudioStyle.icon-mixing") != Some("1")
        && fl::get_key(text, "LemonStudioStyle.icon-family").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0) == 0
    {
        for role in 0..crate::icons::CHOICES.len() {
            for field in ["family", "color"] {
                next = fl::remove_key(&next, &format!("LemonStudioStyle.icon-{role}-{field}"));
            }
        }
    }
    fl::set_key(&next, "LemonStudioStyle.icon-mixing", "1")
}

fn paint_identity(ui: &mut Ui, s: &State, row: IdentityRow, r: Rect) {
    use crate::theme_identity as id;
    let theme = ui.theme_preview.unwrap_or(ui.t);
    let current = theme.appearance;
    let text = s.fl_text.as_deref().unwrap_or("");
    let name = fl::get_key(text, id::NAME);
    let mut add = |slot: usize, title: &str, hit, active| {
        let box_ = Rect::new(r.x + slot as f32 * 111.0, r.y + 3.0, 105.0, 27.0);
        button(ui, box_, Hit::Identity(hit), title, title, active);
    };
    match row {
        IdentityRow::FollowIcons => {
            add(0, "Match icons", IdentityHit::FollowIcons, current.cursor_family.is_none());
            button(ui, Rect::new(r.x + 120.0, r.y + 3.0, 150.0, 27.0),
                Hit::Identity(IdentityHit::ResetArtwork), "Reset all artwork", "Reset all icon and cursor choices and colors; Apply or Cancel in the header", false);
            let name = crate::playlist_tools::cursor_pack_name(current.cursor_family.unwrap_or(current.icon_pack));
            ui.c.text(Font::new(12), r.x + 285.0, r.y + 10.0, ui.t.text, &format!("Cursor pack: {name}"));
        }
        IdentityRow::Icon(index) => {
            let (icon, label, key) = crate::icons::CHOICES[index];
            let choice = current.icon_variants[index / 16][index % 16];
            let count = os_ui::icon_packs::PACKS.len() as u8 + 1;
            let name = choice.checked_sub(1).map(|id| os_ui::icon_packs::get(id).name).unwrap_or("Follow pack");
            ui.c.text_clip(Font::new(12), r.x + 30.0, r.y + 10.0, 125.0, ui.t.text, label);
            ui.c.set_icon_pack(current.icon_pack, false);
            ui.c.set_icon_overrides(current);
            crate::icons::draw(ui.c, icon, Rect::new(r.x + 3.0, r.y + 6.0, 22.0, 22.0), theme.glyph);
            ui.c.set_icon_pack(0, false);
            for (x, title, next) in [(160.0, "<", (choice + count - 1) % count), (315.0, ">", (choice + 1) % count)] {
                button(ui, Rect::new(r.x + x, r.y + 3.0, 28.0, 27.0),
                    Hit::Identity(IdentityHit::IconFamily(index as u8, next)), title, "Choose this icon's family", false);
            }
            ui.c.text_center(Font::new(11), Rect::new(r.x + 189.0, r.y + 3.0, 124.0, 27.0), ui.t.text, name);
            button(ui, Rect::new(r.x + 350.0, r.y + 3.0, 65.0, 27.0),
                Hit::Identity(IdentityHit::IconFamily(index as u8, 1)), "Default", "Use this icon's original design", choice == 1);
            button(ui, Rect::new(r.x + 420.0, r.y + 3.0, 55.0, 27.0),
                Hit::Identity(IdentityHit::IconFamily(index as u8, 0)), "Pack", "Follow the icon pack", choice == 0);
            if let Some(session) = s.settings.advanced_theme.as_ref() {
                field(ui, Rect::new(r.x + 485.0, r.y + 3.0, 80.0, 27.0), session, FieldFor::IdentityValue(key), text);
            }
        }
        IdentityRow::Cursor(index) => {
            let (label, cursor) = crate::playlist_tools::CURSOR_CHOICES[index];
            let choice = current.cursor_variants[index / 16][index % 16];
            let count = crate::playlist_tools::CURSOR_PACK_COUNT + 1;
            let name = choice.checked_sub(1).map(crate::playlist_tools::cursor_pack_name).unwrap_or("Follow pack");
            let pack = crate::playlist_tools::resolved_cursor_pack(current, cursor);
            crate::playlist_tools::paint_cursor_sample(ui.c, cursor, pack,
                Rect::new(r.x + 2.0, r.y + 4.0, 25.0, 25.0), current.cursor_color.unwrap_or(theme.glyph));
            ui.c.text_clip(Font::new(12), r.x + 32.0, r.y + 10.0, 145.0, ui.t.text, label);
            for (x, title, next) in [(185.0, "<", (choice + count - 1) % count), (350.0, ">", (choice + 1) % count)] {
                button(ui, Rect::new(r.x + x, r.y + 3.0, 28.0, 27.0),
                    Hit::Identity(IdentityHit::CursorFamily(index as u8, next)), title, "Choose this cursor's pack", false);
            }
            ui.c.text_center(Font::new(11), Rect::new(r.x + 215.0, r.y + 3.0, 133.0, 27.0), ui.t.text, name);
            button(ui, Rect::new(r.x + 390.0, r.y + 3.0, 70.0, 27.0),
                Hit::Identity(IdentityHit::CursorFamily(index as u8, 1)), "Default", "Use the original cursor", choice == 1);
            button(ui, Rect::new(r.x + 468.0, r.y + 3.0, 70.0, 27.0),
                Hit::Identity(IdentityHit::CursorFamily(index as u8, 0)), "Pack", "Follow the cursor pack", choice == 0);
        }
        IdentityRow::Appearance => {
            let Some(session) = s.settings.advanced_theme.as_ref() else { return };
            for (i, (label, key)) in [
                ("Icon color", "LemonStudioStyle.icon-color"),
                ("Cursor color", "LemonStudioStyle.cursor-color"),
                ("Cursor size %", "LemonStudioStyle.cursor-size"),
            ].into_iter().enumerate() {
                let y = r.y + i as f32 * 32.0;
                ui.c.text(Font::new(13), r.x, y + 7.0, ui.t.text, label);
                if key == "LemonStudioStyle.cursor-color" {
                    let family = current.cursor_family.unwrap_or(current.icon_pack);
                    let color = current.cursor_color.unwrap_or(Color::rgb(os_ui::icon_packs::get(family).tint));
                    let b = Rect::new(r.x + 130.0, y, 150.0, 28.0);
                    button(ui, b, Hit::Identity(IdentityHit::CursorColor), "Choose color…", "Choose a cursor color visually", false);
                    ui.c.fill_rrect(Rect::new(b.x + 6.0, b.y + 7.0, 14.0, 14.0), 3.0, color);
                    button(ui, Rect::new(b.right() + 8.0, y, 105.0, 28.0),
                        Hit::Identity(IdentityHit::ResetCursorColor), "Pack color", "Restore the cursor pack's color", false);
                } else {
                    field(ui, Rect::new(r.x + 130.0, y, 150.0, 28.0), session, FieldFor::IdentityValue(key), text);
                }
            }
        }
        IdentityRow::CursorPacks(first) => {
            for (slot, pack) in (first..crate::playlist_tools::CURSOR_PACK_COUNT).take(5).enumerate() {
                add(slot, crate::playlist_tools::cursor_pack_name(pack), IdentityHit::CursorPack(pack),
                    current.cursor_family.unwrap_or(current.icon_pack) == pack);
            }
        }
        IdentityRow::Packs(first) => {
            for (slot, pack) in (first..os_ui::icon_packs::PACKS.len() as u8).take(5).enumerate() {
                add(
                    slot,
                    os_ui::icon_packs::get(pack).name,
                    IdentityHit::Pack(pack),
                    current.icon_pack == pack,
                );
            }
        }
        IdentityRow::Fonts => {
            for (slot, title) in id::FONTS.into_iter().enumerate() {
                if slot != 4 || name.is_some() {
                    add(
                        slot,
                        title,
                        IdentityHit::Font(slot as u8),
                        current.ui_font as usize == slot,
                    );
                }
            }
        }
        IdentityRow::Files => {
            add(0, "Import font...", IdentityHit::Import, false);
            if let Some(name) = name {
                add(1, "Remove font", IdentityHit::Remove, false);
                ui.c.text_clip(
                    Font::new(12),
                    r.x + 230.0,
                    r.y + 8.0,
                    315.0,
                    ui.t.text,
                    name,
                );
            } else {
                ui.c.text(
                    Font::new(12),
                    r.x + 125.0,
                    r.y + 8.0,
                    ui.t.text_dim,
                    "TTF / OTF · 512 KB maximum",
                );
            }
        }
        IdentityRow::Preview => {
            ui.c.mute_theme_font(false);
            ui.c.set_icon_pack(current.icon_pack, false);
            ui.c.set_icon_color(current.icon_color);
            ui.c.set_icon_overrides(current);
            let box_ = Rect::new(r.x, r.y + 4.0, r.w - 2.0, r.h - 14.0);
            ui.c.fill_rrect(box_, 6.0, theme.toolbar);
            use crate::icons::{self, Icon};
            let samples = [
                Icon::Play,
                Icon::Metronome,
                Icon::WaitInput,
                Icon::Countdown,
                Icon::LoopRecord,
                Icon::PianoRoll,
                Icon::ChannelRack,
                Icon::Mixer,
                Icon::Playlist,
                Icon::Magnet,
                Icon::Cut,
                Icon::Pencil,
                Icon::Brush,
                Icon::Slice,
                Icon::Delete,
                Icon::StretchBand,
                Icon::Select,
                Icon::Mute,
                Icon::Speaker,
                Icon::Zoom,
            ];
            for (i, icon) in samples.into_iter().enumerate() {
                let rect = Rect::new(
                    box_.x + 13.0 + (i % 10) as f32 * 37.0,
                    box_.y + 10.0 + (i / 10) as f32 * 34.0,
                    25.0,
                    25.0,
                );
                icons::draw(ui.c, icon, rect, theme.glyph);
            }
            let cursor_area = box_.right() - 134.0;
            for (i, cursor) in [crate::app::Cursor::Default, crate::app::Cursor::Playlist(crate::playlist::PlTool::Draw, crate::playlist_tools::GRAB)].into_iter().enumerate() {
                let pack = crate::playlist_tools::resolved_cursor_pack(current, cursor);
                crate::playlist_tools::paint_cursor_sample(ui.c, cursor, pack,
                    Rect::new(cursor_area + 10.0 + i as f32 * 58.0, box_.y + 8.0, 28.0, 28.0),
                    current.cursor_color.unwrap_or(theme.glyph));
            }
            let caption = "Independent cursors";
            ui.c.text_clip(
                Font::new(11),
                cursor_area + 11.0,
                box_.y + 50.0,
                108.0,
                theme.text_dim,
                caption,
            );
            ui.c.text_clip(
                Font::new(21).bold(),
                box_.x + 14.0,
                box_.y + 87.0,
                box_.w - 28.0,
                theme.text,
                "Lemon Studio   01:23   Mixer",
            );
            ui.c.text_clip(
                Font::new(13),
                box_.x + 14.0,
                box_.y + 118.0,
                box_.w - 28.0,
                theme.text_dim,
                "Recording  ·  Playlist  ·  Piano Roll  ·  Effects  ·  120 BPM",
            );
            ui.c.mute_theme_font(true);
            ui.c.set_icon_pack(0, false);
        }
    }
}

fn paint_mixer(ui: &mut Ui, session: &Session, row: MixerRow, r: Rect) {
    let Some(design) = &session.mixer_designer else {
        return;
    };
    let part = design.part;
    let skin = ui.mixer_skins.get(design.scope);
    let paint_buttons = |ui: &mut Ui, choices: &[(&str, MixerHit, bool)]| {
        let mut x = r.x;
        for &(label, hit, active) in choices {
            let w = ui.c.text_width(Font::new(12), label).ceil() + 16.0;
            let b = Rect::new(x, r.y + 3.0, w, 26.0);
            button(ui, b, Hit::Mixer(hit), label, label, active);
            x += w + 5.0;
        }
    };
    match row {
        MixerRow::Target => {
            let choices = Scope::ALL
                .map(|scope| (scope.label(), MixerHit::Scope(scope), scope == design.scope));
            paint_buttons(ui, &choices);
            ui.c.text(
                Font::new(12),
                r.right() - 133.0,
                r.y + 8.0,
                ui.t.text_dim,
                &format!(
                    "Live Mixer · {}",
                    Scope::from_finish(ui.mixer_finish).label()
                ),
            );
        }
        MixerRow::Components => {
            let buttons = Part::ALL.map(|p| (p.label(), MixerHit::Part(p), p == part));
            paint_buttons(ui, &buttons);
            let active = skin.active();
            let label = if active {
                "Use in Mixer: On"
            } else {
                "Use in Mixer: Off"
            };
            button(
                ui,
                Rect::new(r.right() - 129.0, r.y + 3.0, 128.0, 26.0),
                Hit::Mixer(MixerHit::Enabled(!active)),
                label,
                "Toggle custom components on the Mixer",
                active,
            );
        }
        MixerRow::Preview => {
            let theme = skin.palette(ui.theme_preview.unwrap_or(ui.t));
            mixer_skin::preview(
                ui.c,
                theme,
                skin,
                design,
                Rect::new(r.x, r.y + 2.0, r.w, r.h - 8.0),
            );
        }
        MixerRow::States => {
            let states: &[PreviewState] = match part {
                Part::Button => &PreviewState::ALL,
                Part::Knob => &[PreviewState::Normal, PreviewState::Hover],
                Part::Fader => &[
                    PreviewState::Normal,
                    PreviewState::Hover,
                    PreviewState::Active,
                ],
                Part::Panel => &[PreviewState::Normal, PreviewState::Active],
                Part::Meter => &[],
            };
            let choices: Vec<_> = states
                .iter()
                .map(|&state| (state.label(), MixerHit::State(state), design.state == state))
                .collect();
            paint_buttons(ui, &choices);
        }
        MixerRow::Value | MixerRow::Detail | MixerRow::Edge => {
            let (label, value, change): (&str, u8, fn(u8) -> MixerHit) = match row {
                MixerRow::Value => ("Preview value", design.value, MixerHit::Value),
                MixerRow::Edge => ("Edge / weight", skin.edge(part), MixerHit::Edge),
                _ => ("Surface detail", skin.detail(part), MixerHit::Detail),
            };
            let t = ui.t;
            ui.c.text(
                Font::new(13),
                r.x,
                r.y + 7.0,
                t.text,
                &format!("{label}  {value}%"),
            );
            for (i, n) in [0, 20, 40, 60, 80, 100].into_iter().enumerate() {
                let b = Rect::new(r.x + 136.0 + i as f32 * 47.0, r.y + 3.0, 42.0, 26.0);
                button(
                    ui,
                    b,
                    Hit::Mixer(change(n)),
                    &n.to_string(),
                    label,
                    value == n,
                );
            }
        }
        MixerRow::Style => {
            let choices: Vec<_> = part
                .styles()
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    (
                        label,
                        MixerHit::Style(i as u8),
                        skin.style(part) as usize == i && !skin.inherited(part),
                    )
                })
                .collect();
            paint_buttons(ui, &choices);
        }
        MixerRow::Colors => {
            let t = skin.palette(ui.theme_preview.unwrap_or(ui.t));
            for (i, &(label, key)) in part.colors().iter().enumerate() {
                let b = Rect::new(r.x + i as f32 * 90.0, r.y + 3.0, 83.0, 26.0);
                button(
                    ui,
                    b,
                    Hit::Mixer(MixerHit::Color(key)),
                    label,
                    "Edit this layer's color",
                    false,
                );
                if let Some(color) = t.get(key) {
                    ui.c.fill_rrect(Rect::new(b.x + 4.0, b.y + 8.0, 9.0, 10.0), 1.0, color);
                }
            }
        }
        MixerRow::Artwork => {
            let mut choices = vec![
                ("Import PNG...", MixerHit::ImportArt, false),
                ("Reset component", MixerHit::Reset, false),
            ];
            if skin.has_art(part) {
                choices.insert(1, ("Remove artwork", MixerHit::RemoveArt, false));
            }
            paint_buttons(ui, &choices);
            ui.c.text(
                Font::new(11),
                r.right() - 178.0,
                r.y + 9.0,
                ui.t.text_dim,
                "PNG · 512 × 512 max · 64 KB",
            );
        }
    }
}

impl App {
    pub(crate) fn start_theme_editor(&mut self) {
        if self.state.settings.advanced_theme.is_some() {
            return;
        }
        self.state.settings.advanced_theme = Some(Session {
            original: self.theme.clone(),
            snapshot: self.snap_theme(),
            history: std::mem::take(&mut self.state.settings.history),
            query: Field::default(),
            groups: groups(""),
            mixer_designer: None,
            identity: false,
            field: Field::default(),
            focus: None,
        });
        self.state.settings.scroll = 0.0;
        self.invalidate();
    }

    pub(crate) fn finish_theme_editor(&mut self, apply: bool) {
        let Some(session) = self.state.settings.advanced_theme.take() else {
            return;
        };
        self.picker_close();
        self.state.settings.scroll = 0.0;
        self.state.settings.history = session.history;
        if apply && self.state.fl_text != session.snapshot.text {
            self.theme_mark_snapshot(Step::Other, session.snapshot);
            if let Some(text) = &mut self.state.fl_text {
                *text = fl::set_key(text, VERSION_KEY, "1");
            }
            self.theme_store();
            self.state.message = Some("Theme changes applied".into());
        } else {
            self.theme = session.original;
            self.state.fl_text = session.snapshot.text;
            self.state.theme_base = session.snapshot.base;
            os_theme::set_current(&self.theme.colors);
        }
        self.sync_preset();
        self.invalidate();
    }

    pub(crate) fn theme_editor_blur(&mut self) {
        if let Some(edit) = self.state.settings.advanced_theme.as_mut() {
            edit.focus = None;
        }
    }

    pub(crate) fn theme_editor_focused(&self) -> bool {
        let st = &self.state.settings;
        st.open
            && st.tab == settings::THEME
            && st
                .advanced_theme
                .as_ref()
                .is_some_and(|edit| edit.focus.is_some())
    }

    pub(crate) fn editor_source_text(&self) -> String {
        let mut text = crate::theme_adjust::applied_text(&self.state, self.design());
        if self.state.fl_text.is_none()
            && self.theme.colors != os_theme::design::palette(self.design(), None)
        {
            // A native theme.txt has no legacy text. Preserve the colors the user sees as the editable base.
            for &name in Palette::NAMES {
                text.push_str(&format!(
                    "{}={}\n",
                    color_key(name),
                    hex(self.theme.colors.get(name).expect("palette color"))
                ));
            }
        }
        text
    }

    fn apply_editor_text(&mut self, text: String) {
        if text != self.editor_source_text() {
            self.theme_mark(Step::Other);
            self.theme_edit(text);
        }
    }

    pub(crate) fn theme_editor_act(&mut self, hit: Hit) {
        if let Hit::Mixer(hit) = hit {
            self.mixer_designer_act(hit);
            return;
        }
        if let Hit::Identity(hit) = hit {
            self.theme_identity_act(hit);
            return;
        }
        let text = self.editor_source_text();
        let value = if let Hit::Field(kind) = hit {
            value_label(&self.theme.colors, kind, &text)
        } else {
            String::new()
        };
        let Some(session) = self.state.settings.advanced_theme.as_mut() else {
            return;
        };
        session.focus = None;
        let next = match hit {
            Hit::Field(kind) => {
                session.focus = Some(kind);
                if kind == FieldFor::Search {
                    session.query.select_all();
                } else {
                    session
                        .field
                        .set(if value == "Inherit" { "" } else { &value });
                    session.field.select_all();
                }
                self.invalidate();
                return;
            }
            Hit::Color(token) => {
                self.open_editor_swatch(token);
                return;
            }
            Hit::Choice(i, "Inherit") | Hit::ResetStyle(i) => fl::remove_key(&text, &style_key(i)),
            Hit::Choice(i, value) => fl::set_key(&text, &style_key(i), value),
            Hit::ResetColor(token) => fl::remove_key(&text, &color_key(token)),
            Hit::Mixer(_) | Hit::Identity(_) => unreachable!("Designer hits dispatched above"),
            Hit::ResetGroup(group) => Palette::NAMES
                .iter()
                .filter(|key| section(key) == group)
                .fold(text, |text, token| fl::remove_key(&text, &color_key(token))),
        };
        self.apply_editor_text(next);
        self.invalidate();
    }

    pub(crate) fn theme_editor_key(&mut self, key: &Key, ctrl: bool, alt: bool) -> bool {
        if !self.theme_editor_focused() {
            return false;
        }
        let st = &mut self.state.settings;
        let edit = st.advanced_theme.as_mut().unwrap();
        let Some(kind) = edit.focus else { return false };
        match key {
            Key::Named(NamedKey::Escape) => edit.focus = None,
            Key::Named(NamedKey::Enter | NamedKey::Tab) => {
                let input = edit.field.text.trim().to_string();
                edit.focus = None;
                let text = self.editor_source_text();
                let next = match kind {
                    FieldFor::IdentityValue(key) => {
                        if input.is_empty() {
                            fl::remove_key(&text, key)
                        } else if key.ends_with("size") {
                            let Some(value) = input.parse::<f32>().ok().filter(|v| v.is_finite() && (50.0..=200.0).contains(v)) else {
                                self.state.message = Some("Enter a cursor size from 50 to 200, or clear to reset.".into());
                                return true;
                            };
                            fl::set_key(&text, key, &value.to_string())
                        } else {
                            let Some(color) = Color::parse(&input) else {
                                self.state.message = Some("Enter a color such as #E8D5A0, or clear to reset.".into());
                                return true;
                            };
                            let text = if key.starts_with("LemonStudioStyle.icon-") && key != "LemonStudioStyle.icon-color" {
                                explicit_icon_mix(&text)
                            } else { text };
                            fl::set_key(&text, key, &hex(color))
                        }
                    }
                    FieldFor::Search => return true,
                    FieldFor::Hex(token) => {
                        let input = input.trim_start_matches('#');
                        if input.len() != 6 || !input.bytes().all(|b| b.is_ascii_hexdigit()) {
                            self.state.message =
                                Some("Enter six hex digits, for example #647363".into());
                            return true;
                        }
                        set_color(
                            &text,
                            token,
                            Color::parse(&format!("#{input}")).expect("validated hex"),
                        )
                    }
                    FieldFor::Number(i) => {
                        let Values::Number(max) = STYLES[i].values else {
                            return true;
                        };
                        let Some(n) = input
                            .parse::<f32>()
                            .ok()
                            .filter(|n| n.is_finite() && (0.0..=max).contains(n))
                        else {
                            self.state.message = Some(format!("Enter a number from 0 to {max}"));
                            return true;
                        };
                        fl::set_key(&text, &style_key(i), &n.to_string())
                    }
                };
                self.apply_editor_text(next);
            }
            _ => {
                let field = if kind == FieldFor::Search {
                    &mut edit.query
                } else {
                    &mut edit.field
                };
                if field.len() < 128
                    || !matches!(key, Key::Character(_) | Key::Named(NamedKey::Space))
                    || field.all
                    || ctrl
                {
                    field.key(key, ctrl, alt);
                }
                if kind == FieldFor::Search {
                    edit.groups = groups(&edit.query.text);
                    st.scroll = 0.0;
                }
            }
        }
        true
    }

    pub(crate) fn mixer_designer_open(&mut self, on: bool) {
        if self.state.settings.advanced_theme.is_none() {
            self.start_theme_editor();
        }
        let scope = Scope::from_finish(crate::views::mixer::Finish::current(
            self.theme.colors.appearance,
        ));
        let st = &mut self.state.settings;
        if let Some(edit) = st.advanced_theme.as_mut() {
            edit.identity = false;
            edit.mixer_designer = on.then(|| Designer {
                scope,
                ..Designer::default()
            });
            edit.focus = None;
            st.scroll = 0.0;
        }
        self.invalidate();
    }

    fn mixer_designer_act(&mut self, hit: MixerHit) {
        let text = self.editor_source_text();
        let Some(designer) = self
            .state
            .settings
            .advanced_theme
            .as_mut()
            .and_then(|s| s.mixer_designer.as_mut())
        else {
            return;
        };
        let (scope, part) = (designer.scope, designer.part);
        let next = match hit {
            MixerHit::Scope(next) => {
                designer.scope = next;
                designer.state = PreviewState::Normal;
                Some(next.select(&text))
            }
            MixerHit::Part(p) => {
                designer.part = p;
                None
            }
            MixerHit::State(state) => {
                designer.state = state;
                None
            }
            MixerHit::Value(value) => {
                designer.value = value.min(100);
                None
            }
            MixerHit::Style(style) => {
                Some(mixer_skin::edit(&text, scope, part, "Style", style.min(4)))
            }
            MixerHit::Detail(detail) => Some(mixer_skin::edit(
                &text,
                scope,
                part,
                "Detail",
                detail.min(100),
            )),
            MixerHit::Edge(edge) => {
                Some(mixer_skin::edit(&text, scope, part, "Edge", edge.min(100)))
            }
            MixerHit::Enabled(on) => Some(mixer_skin::enable(&text, scope, on)),
            MixerHit::Color(key) => {
                let color =
                    mixer_skin::color(&text, scope, key).or_else(|| self.theme.colors.get(key));
                if let Some(color) = color {
                    self.open_editor_swatch_color(key, color);
                }
                return;
            }
            MixerHit::ImportArt => {
                self.file_dialog(
                    FileDlg::new(
                        Mode::Open,
                        "Import Mixer artwork",
                        "Mixer artwork",
                        Then::MixerSkinArt(part),
                    )
                    .filter("Transparent PNG", &["png"]),
                );
                return;
            }
            MixerHit::RemoveArt => Some(mixer_skin::remove_art(&text, scope, part)),
            MixerHit::Reset => Some(mixer_skin::reset(&text, scope, part)),
        };
        if let Some(next) = next {
            self.apply_editor_text(next);
        }
        self.invalidate();
    }

    pub(crate) fn mixer_designer_import(&mut self, part: Part, path: &Path) {
        let Some(design) = self
            .state
            .settings
            .advanced_theme
            .as_ref()
            .and_then(|s| s.mixer_designer.as_ref())
        else {
            return;
        };
        match mixer_skin::import_art(&self.editor_source_text(), design.scope, part, path) {
            Ok(text) => {
                self.apply_editor_text(text);
                self.state.message = Some(format!("{} artwork imported", part.label()));
            }
            Err(error) => self.state.message = Some(error),
        }
    }

    pub(crate) fn theme_identity_open(&mut self) {
        self.start_theme_editor();
        if let Some(edit) = self.state.settings.advanced_theme.as_mut() {
            edit.identity = true;
            edit.mixer_designer = None;
            edit.focus = None;
        }
        self.state.settings.scroll = 0.0;
        self.invalidate();
    }

    fn theme_identity_act(&mut self, hit: IdentityHit) {
        use crate::theme_identity as id;
        let text = self.editor_source_text();
        let next = match hit {
            IdentityHit::CursorColor => {
                let appearance = self.theme.colors.appearance;
                let family = appearance.cursor_family.unwrap_or(appearance.icon_pack);
                let color = appearance.cursor_color.unwrap_or(Color::rgb(os_ui::icon_packs::get(family).tint));
                self.open_editor_swatch_color("LemonStudioStyle.cursor-color", color);
                return;
            }
            IdentityHit::ResetCursorColor => fl::remove_key(&text, "LemonStudioStyle.cursor-color"),
            IdentityHit::FollowIcons => fl::remove_key(&text, "LemonStudioStyle.cursor-family"),
            IdentityHit::IconFamily(role, family) => {
                let key = format!("LemonStudioStyle.icon-{role}-family");
                let text = explicit_icon_mix(&text);
                if family == 0 { fl::remove_key(&text, &key) } else { fl::set_key(&text, &key, &family.to_string()) }
            }
            IdentityHit::CursorFamily(role, family) => {
                let key = format!("LemonStudioStyle.cursor-{role}-family");
                if family == 0 { fl::remove_key(&text, &key) } else { fl::set_key(&text, &key, &family.to_string()) }
            }
            IdentityHit::ResetArtwork => {
                let mut next = text.clone();
                for key in ["icon-family", "icon-mixing", "icon-color", "cursor-family", "cursor-color", "cursor-size"] {
                    next = fl::remove_key(&next, &format!("LemonStudioStyle.{key}"));
                }
                for role in 0..crate::icons::CHOICES.len() {
                    for field in ["family", "color"] {
                        next = fl::remove_key(&next, &format!("LemonStudioStyle.icon-{role}-{field}"));
                    }
                }
                for role in 0..crate::playlist_tools::CURSOR_CHOICES.len() {
                    next = fl::remove_key(&next, &format!("LemonStudioStyle.cursor-{role}-family"));
                }
                next
            }
            IdentityHit::CursorPack(pack) => fl::set_key(&text, "LemonStudioStyle.cursor-family", &pack.to_string()),
            IdentityHit::Pack(0) => fl::remove_key(&text, id::ICON),
            IdentityHit::Pack(pack) => fl::set_key(&text, id::ICON, &pack.to_string()),
            IdentityHit::Font(4) if fl::get_key(&text, id::DATA).is_none() => return,
            IdentityHit::Font(font) => fl::set_key(&text, id::FONT, &font.min(4).to_string()),
            IdentityHit::Import => {
                self.file_dialog(
                    FileDlg::new(Mode::Open, "Import UI font", "UI font", Then::ThemeFont)
                        .filter("TrueType/OpenType font", &["ttf", "otf"]),
                );
                return;
            }
            IdentityHit::Remove => {
                let text = fl::remove_key(&fl::remove_key(&text, id::DATA), id::NAME);
                if fl::get_key(&text, id::FONT) == Some("4") {
                    fl::remove_key(&text, id::FONT)
                } else {
                    text
                }
            }
        };
        self.apply_editor_text(next);
        self.invalidate();
    }

    pub(crate) fn theme_identity_import(&mut self, path: &Path) {
        if self
            .state
            .settings
            .advanced_theme
            .as_ref()
            .is_none_or(|s| !s.identity)
        {
            return;
        }
        match crate::theme_identity::import(&self.editor_source_text(), path) {
            Ok(text) => {
                self.apply_editor_text(text);
                self.state.message = Some("UI font imported".into());
            }
            Err(error) => self.state.message = Some(error),
        }
    }

    pub(crate) fn theme_editor_file(&mut self, export: bool) {
        let dialog = if export {
            FileDlg::new(
                Mode::Save,
                "Export theme",
                "theme export",
                Then::ExportTheme,
            )
            .name("My Theme.flstheme")
            .filter("Lemon Studio theme", &["flstheme"])
        } else {
            FileDlg::new(
                Mode::Open,
                "Choose wallpaper",
                "theme wallpaper",
                Then::ThemeWallpaper,
            )
            .filter("Pictures", &["png", "jpg", "jpeg"])
        };
        self.file_dialog(dialog);
    }

    pub(crate) fn theme_editor_wallpaper(&mut self, path: Option<&Path>) {
        let text = self.editor_source_text();
        let next = if let Some(path) = path {
            if crate::wallpaper::read(path).is_none() {
                self.state.message = Some("Choose a readable PNG or JPEG wallpaper".into());
                return;
            }
            let name = path.to_string_lossy();
            if name.contains(['\r', '\n']) {
                return;
            }
            fl::set_key(
                &fl::set_key(&text, "BackMode", "2"),
                "BackPicFilename",
                &name,
            )
        } else {
            fl::remove_key(&text, "BackPicFilename")
        };
        self.apply_editor_text(next);
    }

    pub(crate) fn export_theme(&mut self, path: &Path) {
        let path = path.with_extension("flstheme");
        let text = self.editor_source_text();
        let text = fl::set_key(&text, VERSION_KEY, "1");
        let text = fl::set_key(&text, EXPORT_SKIN, self.design().name());
        let image = self.theme.colors.wallpaper.as_deref();
        self.state.message = Some(match write_export(&path, &text, image) {
            Ok(true) => format!(
                "Theme exported to {} with its wallpaper. Keep both files together.",
                path.display()
            ),
            Ok(false) => format!("Theme exported to {}", path.display()),
            Err(e) => format!("Could not export theme: {e}"),
        });
    }
}

/// A portable wallpaper sits next to the exported theme. Existing images are never overwritten.
fn write_export(path: &Path, text: &str, wallpaper: Option<&str>) -> std::io::Result<bool> {
    use std::{fs, io};
    let Some(name) = wallpaper else {
        fs::write(path, text)?;
        return Ok(false);
    };
    let source = crate::wallpaper::resolve(name, crate::app::theme_dir().as_deref());
    let suffix = match source
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpg",
        Some("png") => "png",
        _ => return Err(io::Error::other("Wallpaper must be a PNG or JPEG")),
    };
    let mut from = fs::File::open(source)?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let image = (1..=1000)
        .find_map(|i| {
            let name = if i == 1 {
                format!("{stem} Wallpaper.{suffix}")
            } else {
                format!("{stem} Wallpaper {i}.{suffix}")
            };
            let target = path.with_file_name(name);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(file) => Some(Ok((target, file))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e)),
            }
        })
        .ok_or_else(|| io::Error::other("Too many wallpaper files with this name"))??;
    let (target, mut to) = image;
    let result = io::copy(&mut from, &mut to);
    drop(to);
    let result = result.and_then(|_| {
        let relative = target.file_name().unwrap_or_default().to_string_lossy();
        if relative.contains(['\r', '\n']) {
            return Err(io::Error::other("Invalid wallpaper filename"));
        }
        fs::write(path, fl::set_key(text, "BackPicFilename", &relative))
    });
    if result.is_err() {
        let _ = fs::remove_file(target);
    }
    result.map(|_| true)
}
