// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, portable Mixer control art. Settings are resolved outside the painters.

use base64::{Engine, engine::general_purpose::STANDARD};
use os_theme::{Color, Palette, fl};
use os_ui::{Canvas, Font, Rect, Sprite, kit};
use std::f32::consts::PI;
use std::path::Path;

pub const OPEN: &str = "Mixer Skin Designer...";
pub const BACK: &str = "Back to theme editor";
pub const CLOSE: &str = "× Close designer";
const ENABLED: &str = "LemonStudioMixerSkin";
pub const FINISH: &str = "LemonStudioStyle.mixer";
const PREVIOUS: &str = "LemonStudioMixerPreviousFinish";
const MAX_ART: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Part {
    #[default]
    Knob,
    Fader,
    Button,
    Meter,
    Panel,
}

impl Part {
    pub const ALL: [Self; 5] = [
        Self::Knob,
        Self::Fader,
        Self::Button,
        Self::Meter,
        Self::Panel,
    ];

    pub fn label(self) -> &'static str {
        ["Knob", "Fader", "Button", "Meter", "Panel"][self as usize]
    }

    pub fn styles(self) -> [&'static str; 5] {
        match self {
            Self::Knob => ["Round", "Square", "Scalloped", "Fluted", "Disc"],
            Self::Fader => ["Smooth", "Ribbed", "Slot", "Metal", "Twin rail"],
            Self::Button => ["Flat", "Beveled", "Recessed", "Outlined", "Pilot light"],
            Self::Meter => ["Needle", "Bars", "Segments", "Twin needles", "Ladder"],
            Self::Panel => ["Solid", "Brushed", "Ribbed", "Perforated", "Enamel"],
        }
    }

    pub fn colors(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Knob => &[
                ("Cap", "skin_knob_cap"),
                ("Rim", "skin_knob_rim"),
                ("Pointer", "skin_knob_pointer"),
                ("Arc", "skin_knob_ring"),
            ],
            Self::Fader => &[
                ("Cap", "skin_fader_cap"),
                ("Edge", "skin_fader_edge"),
                ("Stripe", "skin_fader_stripe"),
            ],
            Self::Button => &[
                ("Normal", "skin_button_normal"),
                ("Hover", "skin_button_hover"),
                ("Held", "skin_button_held"),
                ("Active", "skin_button_active"),
                ("Disabled", "skin_button_disabled"),
                ("Edge", "skin_button_edge"),
            ],
            Self::Meter => &[
                ("Face", "skin_meter_face"),
                ("Scale", "skin_meter_scale"),
                ("Needle", "skin_meter_needle"),
                ("Low", "skin_meter_low"),
                ("High", "skin_meter_high"),
                ("Clip", "skin_meter_clip"),
            ],
            Self::Panel => &[
                ("Surface", "skin_panel_face"),
                ("Trim", "skin_panel_trim"),
                ("Selected", "skin_panel_selected"),
            ],
        }
    }

    fn key(self, field: &str) -> String {
        format!("LemonStudioMixer.{}.{field}", self.label())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreviewState {
    #[default]
    Normal,
    Hover,
    Held,
    Active,
    Disabled,
}

impl PreviewState {
    pub const ALL: [Self; 5] = [
        Self::Normal,
        Self::Hover,
        Self::Held,
        Self::Active,
        Self::Disabled,
    ];
    pub fn label(self) -> &'static str {
        ["Normal", "Hover", "Held", "Active", "Disabled"][self as usize]
    }
    pub fn kit(self) -> kit::State {
        kit::State::new(
            self == Self::Hover,
            self == Self::Held,
            self == Self::Active,
        )
        .with_disabled(self == Self::Disabled)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    Original,
    CleanModern,
    Analog,
    #[default]
    Theme,
}

impl Scope {
    pub const ALL: [Self; 4] = [Self::Original, Self::CleanModern, Self::Analog, Self::Theme];

    pub fn label(self) -> &'static str {
        ["Original", "Clean Modern", "Analog", "Theme colors"][self as usize]
    }

    pub fn from_finish(finish: crate::views::mixer::Finish) -> Self {
        use crate::views::mixer::Finish::*;
        match finish {
            Original => Self::Original,
            CleanModern => Self::CleanModern,
            Analog => Self::Analog,
            Theme => Self::Theme,
        }
    }

    fn key(self, key: &str) -> String {
        if self == Self::Theme {
            return key.to_owned();
        }
        format!(
            "LemonStudioMixer.{}.{}",
            self.label().replace(' ', ""),
            key.strip_prefix("LemonStudioMixer.").unwrap_or(key)
        )
    }

    pub fn select(self, text: &str) -> String {
        fl::set_key(text, FINISH, self.label())
    }

    fn color_key(self, token: &str) -> String {
        self.key(&format!("{}{token}", os_theme::appearance::COLOR_PREFIX))
    }
}

#[derive(Debug, Clone)]
pub struct Designer {
    pub scope: Scope,
    pub part: Part,
    pub state: PreviewState,
    pub value: u8,
}

impl Default for Designer {
    fn default() -> Self {
        Self {
            scope: Scope::default(),
            part: Part::Knob,
            state: PreviewState::Normal,
            value: 60,
        }
    }
}

struct Layer {
    style: u8,
    detail: u8,
    edge: u8,
    inherited: bool,
    art_key: Option<String>,
    art: Option<Sprite>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            style: 0,
            detail: 55,
            edge: 50,
            inherited: false,
            art_key: None,
            art: None,
        }
    }
}

#[derive(Default)]
pub struct SkinBank {
    source: Option<String>,
    initialized: bool,
    skins: [Skin; 4],
}

impl SkinBank {
    pub fn refresh(&mut self, text: Option<&str>, base: &Palette) {
        if self.initialized
            && self.source.as_deref() == text
            && self.get(Scope::Theme).palette.as_ref() == Some(base)
        {
            return;
        }
        self.initialized = true;
        self.source = text.map(str::to_owned);
        for scope in Scope::ALL {
            self.skins[scope as usize].refresh(text.unwrap_or(""), scope, base);
        }
    }

    pub fn get(&self, scope: Scope) -> &Skin {
        &self.skins[scope as usize]
    }

    #[cfg(test)]
    pub fn active(&self) -> bool {
        self.get(Scope::Theme).active()
    }
    #[cfg(test)]
    pub fn has_art(&self, part: Part) -> bool {
        self.get(Scope::Theme).has_art(part)
    }
    #[cfg(test)]
    pub fn style(&self, part: Part) -> u8 {
        self.get(Scope::Theme).style(part)
    }
    #[cfg(test)]
    pub fn inherited(&self, part: Part) -> bool {
        self.get(Scope::Theme).inherited(part)
    }
}

#[derive(Default)]
pub struct Skin {
    enabled: bool,
    palette: Option<Palette>,
    layers: [Layer; 5],
}

impl Skin {
    pub fn palette<'a>(&'a self, fallback: &'a Palette) -> &'a Palette {
        self.palette.as_ref().unwrap_or(fallback)
    }

    fn refresh(&mut self, text: &str, scope: Scope, base: &Palette) {
        self.enabled = fl::get_key(text, &scope.key(ENABLED)) == Some("1");
        let mut palette = base.clone();
        // Start custom components in the selected console's material palette.
        let (cap, rim, face, ink) = match scope {
            Scope::Original => (0x484d50, 0x202426, 0x353a3d, 0xe0e6e9),
            Scope::CleanModern => (0x424242, 0x5a5a5a, 0x282828, 0xe6e6e6),
            Scope::Analog => (0x373b3e, 0x111315, 0x292d30, 0xe1ddd1),
            Scope::Theme => (0, 0, 0, 0),
        };
        if scope != Scope::Theme {
            palette.skin_knob_cap = Color::rgb(cap);
            palette.skin_knob_rim = Color::rgb(rim);
            palette.skin_knob_pointer = Color::rgb(ink);
            palette.skin_knob_ring = base.accent;
            palette.skin_fader_cap = Color::rgb(match scope {
                Scope::CleanModern => 0xa3a3a3,
                Scope::Analog => 0xb8b4a9,
                _ => 0x9ca3a7,
            });
            palette.skin_fader_edge = Color::rgb(rim);
            palette.skin_fader_stripe = Color::rgb(face);
            palette.skin_panel_selected = base.accent;
            palette.skin_button_normal = Color::rgb(face);
            palette.skin_button_hover = Color::rgb(face).shade(0.12);
            palette.skin_button_held = Color::rgb(face).shade(-0.12);
            palette.skin_button_active = base.accent;
            palette.skin_button_edge = Color::rgb(rim);
            palette.skin_panel_face = Color::rgb(face);
            palette.skin_panel_trim = Color::rgb(rim);
        }
        self.palette = Some(palette);
        for part in Part::ALL {
            let layer = &mut self.layers[part as usize];
            let number = |key, default, max| {
                fl::get_key(text, &scope.key(&part.key(key)))
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(default)
                    .min(max)
            };
            layer.style = number("Style", 0, 4);
            layer.detail = number("Detail", 55, 100);
            layer.edge = number("Edge", 50, 100);
            layer.inherited = fl::get_key(text, &scope.key(&part.key("Inherit"))) == Some("1")
                || (scope != Scope::Theme
                    && fl::get_key(text, &scope.key(&part.key("Use"))) != Some("1"));
            if scope != Scope::Theme {
                for &(_, token) in part.colors() {
                    if let Some(c) =
                        fl::get_key(text, &scope.color_key(token)).and_then(Color::parse)
                        && let Some(slot) = self.palette.as_mut().and_then(|p| p.get_mut(token))
                    {
                        *slot = c;
                    }
                }
            }
            let art = fl::get_key(text, &scope.key(&part.key("PNG")))
                .filter(|s| s.len() <= MAX_ART.div_ceil(3) * 4);
            if art != layer.art_key.as_deref() {
                layer.art = art
                    .and_then(|s| STANDARD.decode(s).ok())
                    .filter(|bytes| bytes.len() <= MAX_ART)
                    .and_then(|bytes| Sprite::png(&bytes));
                layer.art_key = art.map(str::to_owned);
            }
        }
    }

    pub fn active(&self) -> bool {
        self.enabled
    }
    pub fn inherited(&self, part: Part) -> bool {
        self.layers[part as usize].inherited
    }
    pub fn uses(&self, part: Part) -> bool {
        self.active() && !self.inherited(part)
    }
    pub fn style(&self, part: Part) -> u8 {
        self.layers[part as usize].style
    }
    pub fn detail(&self, part: Part) -> u8 {
        self.layers[part as usize].detail
    }
    pub fn edge(&self, part: Part) -> u8 {
        self.layers[part as usize].edge
    }
    pub fn has_art(&self, part: Part) -> bool {
        self.layers[part as usize].art.is_some()
    }

    fn artwork(&self, c: &mut Canvas, part: Part, r: Rect) -> bool {
        let Some(art) = &self.layers[part as usize].art else {
            return false;
        };
        c.sprite(r, art)
    }
}

pub fn same_design(a: &str, b: &str) -> bool {
    if !(a.contains("LemonStudioMixer") || b.contains("LemonStudioMixer")) {
        return true;
    }
    let same = |key: &str| fl::get_key(a, key) == fl::get_key(b, key);
    Scope::ALL.into_iter().all(|scope| {
        same(&scope.key(ENABLED))
            && Part::ALL.into_iter().all(|part| {
                part.colors()
                    .iter()
                    .all(|&(_, token)| same(&scope.color_key(token)))
                    && ["Style", "Detail", "Edge", "Inherit", "Use", "PNG"]
                        .into_iter()
                        .all(|field| same(&scope.key(&part.key(field))))
            })
    })
}

pub fn enable(text: &str, scope: Scope, on: bool) -> String {
    if scope != Scope::Theme {
        return if on {
            scope.select(&fl::set_key(text, &scope.key(ENABLED), "1"))
        } else {
            fl::remove_key(text, &scope.key(ENABLED))
        };
    }
    if on {
        let next = if fl::get_key(text, ENABLED) == Some("1") {
            text.to_owned()
        } else {
            fl::set_key(text, PREVIOUS, fl::get_key(text, FINISH).unwrap_or(""))
        };
        fl::set_key(&fl::set_key(&next, FINISH, "Theme colors"), ENABLED, "1")
    } else if fl::get_key(text, ENABLED) == Some("1") {
        let next = match fl::get_key(text, PREVIOUS).filter(|v| !v.is_empty()) {
            Some(previous) => fl::set_key(text, FINISH, previous),
            None => fl::remove_key(text, FINISH),
        };
        fl::remove_key(&fl::remove_key(&next, PREVIOUS), ENABLED)
    } else {
        text.to_owned()
    }
}

pub fn use_part(text: &str, scope: Scope, part: Part) -> String {
    let text = fl::remove_key(text, &scope.key(&part.key("Inherit")));
    let text = if scope == Scope::Theme {
        text
    } else {
        fl::set_key(&text, &scope.key(&part.key("Use")), "1")
    };
    enable(&text, scope, true)
}

pub fn color(text: &str, scope: Scope, token: &str) -> Option<Color> {
    fl::get_key(text, &scope.color_key(token)).and_then(Color::parse)
}

pub fn set_color(text: &str, scope: Scope, part: Part, token: &str, color: Color) -> String {
    let value = format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    use_part(
        &fl::set_key(text, &scope.color_key(token), &value),
        scope,
        part,
    )
}

pub fn edit(text: &str, scope: Scope, part: Part, field: &str, value: u8) -> String {
    use_part(
        &fl::set_key(text, &scope.key(&part.key(field)), &value.to_string()),
        scope,
        part,
    )
}

pub fn reset(text: &str, scope: Scope, part: Part) -> String {
    let mut next = ["Style", "Detail", "Edge", "Use", "PNG"]
        .into_iter()
        .fold(text.to_owned(), |text, field| {
            fl::remove_key(&text, &scope.key(&part.key(field)))
        });
    for &(_, token) in part.colors() {
        next = fl::remove_key(&next, &scope.color_key(token));
    }
    fl::set_key(&next, &scope.key(&part.key("Inherit")), "1")
}

pub fn remove_art(text: &str, scope: Scope, part: Part) -> String {
    fl::remove_key(text, &scope.key(&part.key("PNG")))
}

pub fn import_art(text: &str, scope: Scope, part: Part, path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_ART as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ART || Sprite::png(&bytes).is_none() {
        return Err("Choose a PNG up to 512 × 512 pixels and 64 KB.".into());
    }
    Ok(use_part(
        &fl::set_key(text, &scope.key(&part.key("PNG")), &STANDARD.encode(bytes)),
        scope,
        part,
    ))
}

fn depth(s: &Skin, part: Part) -> f32 {
    f32::from(s.detail(part)) / 100.0
}

pub fn knob(
    c: &mut Canvas,
    t: &Palette,
    skin: &Skin,
    x: f32,
    y: f32,
    r: f32,
    value: f32,
    bipolar: bool,
    hover: bool,
) {
    let box_rect = Rect::new(x - r, y - r, 2.0 * r, 2.0 * r);
    if !c.visible(box_rect) {
        return;
    }
    let start = 0.75 * PI;
    let sweep = 1.5 * PI;
    let angle = start + sweep * value.clamp(0.0, 1.0);
    let d = depth(skin, Part::Knob);
    let cap = if hover {
        t.skin_knob_cap.shade(0.11)
    } else {
        t.skin_knob_cap
    };
    c.fill_circle(x, y, r, t.skin_knob_rim);
    let edge = f32::from(skin.edge(Part::Knob)) / 100.0;
    let inner = (r - (r * (0.06 + edge * 0.09)).clamp(0.6, 1.8)).max(0.0);
    let face = Rect::new(x - inner, y - inner, 2.0 * inner, 2.0 * inner);
    c.fill_circle(x, y, inner, cap);
    if !skin.artwork(c, Part::Knob, face) {
        match skin.style(Part::Knob) {
            1 => c.fill_rect_gradient(face, cap.shade(0.30 * d), cap.shade(-0.25 * d), 1.5),
            2 | 3 => {
                c.fill_rect_gradient(face, cap.shade(0.18 * d), cap.shade(-0.18 * d), inner);
                let count = if r < 10.0 { 8 } else { 16 };
                let fluted = skin.style(Part::Knob) == 3;
                for i in 0..count {
                    let a = i as f32 * 2.0 * PI / count as f32;
                    let from = if fluted { 0.78 } else { 0.91 };
                    c.line(
                        x + a.cos() * inner * from,
                        y + a.sin() * inner * from,
                        x + a.cos() * inner * 0.98,
                        y + a.sin() * inner * 0.98,
                        (r * 0.07).clamp(0.5, 1.0),
                        cap.shade(-0.35 * d),
                    );
                }
            }
            4 => {
                c.fill_rect_gradient(face, cap.shade(0.12 * d), cap.shade(-0.10 * d), inner);
                c.stroke_circle(x, y, inner * 0.82, 0.6, cap.shade(0.2 * d));
            }
            _ => c.fill_rect_gradient(face, cap.shade(0.25 * d), cap.shade(-0.23 * d), inner),
        }
    }
    let neutral = if bipolar { start + sweep * 0.5 } else { start };
    if d > 0.0 && (angle - neutral).abs() > 0.01 {
        c.arc(
            x,
            y,
            (r - 0.65).max(0.0),
            angle.min(neutral),
            (angle - neutral).abs(),
            0.7 + d * 1.3,
            t.skin_knob_ring,
        );
    }
    c.line(
        x + r * 0.22 * angle.cos(),
        y + r * 0.22 * angle.sin(),
        x + r * 0.72 * angle.cos(),
        y + r * 0.72 * angle.sin(),
        1.4,
        t.skin_knob_pointer,
    );
}

pub fn fader(c: &mut Canvas, t: &Palette, skin: &Skin, r: Rect, selected: bool, hover: bool) {
    let d = depth(skin, Part::Fader);
    let mut color = t.skin_fader_cap;
    if selected {
        color = color.mix(t.skin_panel_selected, 0.20);
    }
    if hover {
        color = color.shade(0.12);
    }
    c.fill_rrect(r, 2.0, t.skin_fader_edge);
    let edge = f32::from(skin.edge(Part::Fader)) / 100.0;
    let face = r.inset(0.5 + edge);
    c.fill_rect_gradient(face, color.shade(0.27 * d), color.shade(-0.22 * d), 1.5);
    if !skin.artwork(c, Part::Fader, face) {
        match skin.style(Part::Fader) {
            1 => {
                for y in [-8.0, -5.0, 5.0, 8.0] {
                    let y = face.cy() + y * face.h / 24.0;
                    c.line(
                        face.x + 1.0,
                        y,
                        face.right() - 1.0,
                        y,
                        1.0,
                        color.shade(-0.42 * d),
                    );
                }
            }
            2 => c.fill_rrect(face.inset(face.w * 0.24), 1.0, color.shade(-0.34 * d)),
            3 => {
                c.fill_rect_gradient(face, color.shade(-0.38 * d), color.shade(0.52 * d), 1.5);
                c.fill_rect(
                    Rect::new(face.x, face.y + face.h * 0.33, face.w, face.h * 0.20),
                    color.shade(0.32 * d),
                );
            }
            4 => {
                for x in [face.x + 2.0, face.right() - 2.0] {
                    c.line(
                        x,
                        face.y + 2.0,
                        x,
                        face.bottom() - 2.0,
                        1.0,
                        color.shade(-0.45 * d),
                    );
                }
            }
            _ => {}
        }
    }
    c.line(
        face.x + 1.0,
        face.cy(),
        face.right() - 1.0,
        face.cy(),
        1.0,
        t.skin_fader_stripe,
    );
}

pub fn button(c: &mut Canvas, t: &Palette, skin: &Skin, r: Rect, st: kit::State) -> Color {
    let fill = if st.disabled {
        t.skin_button_disabled
    } else if st.pressed {
        t.skin_button_held
    } else if st.on {
        t.skin_button_active
    } else if st.hover {
        t.skin_button_hover
    } else {
        t.skin_button_normal
    };
    let style = skin.style(Part::Button);
    let edge = f32::from(skin.edge(Part::Button)) / 100.0;
    let d = depth(skin, Part::Button) * 0.27;
    let radius = t.style.radius_small.min(r.h * 0.5);
    c.fill_rrect(r, radius, t.skin_button_edge);
    let face = r.inset(0.5 + edge);
    c.fill_rrect(face, (radius - 1.0).max(0.0), fill);
    if !skin.artwork(c, Part::Button, face) {
        let slope = match style {
            1 => d,
            2 => -d,
            _ => 0.0,
        };
        c.fill_rect_gradient(
            face,
            fill.shade(slope),
            fill.shade(-slope),
            (radius - 1.0).max(0.0),
        );
        if style == 3 {
            c.stroke_rrect(
                face.inset(0.8),
                (radius - 1.0).max(0.0),
                1.0 + edge,
                t.skin_button_edge,
            );
        } else if style == 4 && face.w > 12.0 {
            c.fill_rrect(
                Rect::new(
                    face.x + 3.0,
                    face.y + 2.0,
                    (face.w - 6.0).max(0.0),
                    1.0 + edge,
                ),
                0.6,
                t.skin_button_active
                    .with_alpha(if st.on { 255 } else { 70 }),
            );
        }
    } else if st.disabled || st.pressed || st.hover || st.on {
        c.fill_rrect(face, (radius - 1.0).max(0.0), fill.with_alpha(95));
    }
    kit::ink(t, st)
}

pub fn panel(c: &mut Canvas, t: &Palette, skin: &Skin, r: Rect, selected: bool) {
    let d = depth(skin, Part::Panel);
    c.fill_rrect(r, t.style.radius_panel, t.skin_panel_trim);
    let edge = f32::from(skin.edge(Part::Panel)) / 100.0;
    let face = r.inset(0.5 + edge);
    let color = t.skin_panel_face;
    let textured = skin.layers[Part::Panel as usize]
        .art
        .as_ref()
        .is_some_and(|art| c.sprite_on_solid(face, art, color));
    if !textured {
        c.fill_rect_gradient(
            face,
            color.shade(0.13 * d),
            color.shade(-0.14 * d),
            (t.style.radius_panel - 1.0).max(0.0),
        );
        let step = match skin.style(Part::Panel) {
            1 => 4.0,
            2 => 9.0,
            _ => 0.0,
        };
        if skin.style(Part::Panel) == 3 && face.w > 14.0 && face.h > 14.0 {
            // A few columns only. The detail survives narrow mixer strips and stays cheap.
            for i in 1..(face.h / 12.0).min(45.0) as usize {
                for x in [face.x + 5.0, face.right() - 5.0] {
                    c.fill_circle(x, face.y + i as f32 * 12.0, 1.1, color.shade(-0.35 * d));
                }
            }
        } else if skin.style(Part::Panel) == 4 {
            c.stroke_rrect(
                face.inset(1.0),
                t.style.radius_panel,
                1.0,
                color.shade(0.25 * d),
            );
        }
        if step > 0.0 && face.h > 12.0 {
            for i in 1..(face.h / step).min(100.0) as usize {
                let y = face.y + i as f32 * step;
                c.line(
                    face.x + 2.0,
                    y,
                    face.right() - 2.0,
                    y,
                    1.0,
                    color.shade(-0.16 * d),
                );
            }
        }
    }
    if selected {
        c.stroke_rrect(
            r.inset(1.0),
            t.style.radius_small,
            1.0,
            t.skin_panel_selected,
        );
    }
}

pub fn meter(c: &mut Canvas, t: &Palette, skin: &Skin, r: Rect, levels: [f32; 2], clipped: bool) {
    if !c.visible(r) || r.w < 12.0 || r.h < 12.0 {
        return;
    }
    c.fill_rrect(r, 2.0, t.skin_panel_trim);
    let face = r.inset(2.0);
    c.fill_rect(face, t.skin_meter_face);
    skin.artwork(c, Part::Meter, face);
    c.push_clip(face);
    let d = depth(skin, Part::Meter);
    let style = skin.style(Part::Meter);
    let edge = f32::from(skin.edge(Part::Meter)) / 100.0;
    if matches!(style, 0 | 3) {
        let (x, y) = (face.cx(), face.bottom() - face.h * 0.13);
        let radius = (face.h * 0.76).min(face.w * 0.58);
        let start = -2.58;
        let sweep = 2.02;
        for i in 0..=10 {
            let a = start + sweep * i as f32 / 10.0;
            let inner = radius * (if i % 2 == 0 { 0.79 - d * 0.10 } else { 0.88 });
            let color = if i >= 8 {
                t.skin_meter_clip
            } else {
                t.skin_meter_scale
            };
            c.line(
                x + a.cos() * inner,
                y + a.sin() * inner,
                x + a.cos() * radius,
                y + a.sin() * radius,
                1.0,
                color,
            );
        }
        let value = levels.into_iter().fold(0.0_f32, f32::max).clamp(0.0, 1.0);
        let a = start + sweep * value;
        if style == 3 {
            let right = start + sweep * levels[1].clamp(0.0, 1.0);
            c.line(
                x,
                y,
                x + right.cos() * radius * 0.91,
                y + right.sin() * radius * 0.91,
                0.6 + edge,
                t.skin_meter_needle.with_alpha(105),
            );
        }
        c.line(
            x,
            y,
            x + a.cos() * radius * 0.98,
            y + a.sin() * radius * 0.98,
            0.7 + edge * 1.4,
            t.skin_meter_needle,
        );
        c.fill_circle(x, y, 2.0, t.skin_meter_scale);
    } else {
        let segmented = matches!(style, 2 | 4);
        let steps = if segmented {
            8 + (d * 12.0).round() as usize
        } else {
            1
        };
        let h = face.h - 8.0;
        let w = (face.w - 10.0) * 0.5;
        for (side, level) in levels.into_iter().enumerate() {
            let x = face.x + 4.0 + side as f32 * (w + 2.0);
            if segmented {
                for i in 0..steps {
                    let pos = i as f32 / steps as f32;
                    let color = meter_color(t, pos);
                    let color = if level > pos {
                        color
                    } else {
                        color.mix(t.skin_meter_face, 0.87)
                    };
                    c.fill_rect(
                        Rect::new(
                            x,
                            face.bottom() - 4.0 - (pos + 1.0 / steps as f32) * h,
                            w,
                            (h / steps as f32 - if style == 4 { 1.0 + edge * 2.0 } else { 1.0 })
                                .max(1.0),
                        ),
                        color,
                    );
                }
            } else {
                let level = level.clamp(0.0, 1.0);
                c.fill_rect(
                    Rect::new(x, face.bottom() - 4.0 - level * h, w, level * h),
                    meter_color(t, level),
                );
            }
        }
    }
    if clipped {
        c.fill_circle(face.right() - 4.0, face.y + 4.0, 2.0, t.skin_meter_clip);
    }
    c.pop_clip();
}

fn meter_color(t: &Palette, level: f32) -> Color {
    if level > 0.92 {
        t.skin_meter_clip
    } else if level > 0.72 {
        t.skin_meter_high
    } else {
        t.skin_meter_low
    }
}

pub fn preview(c: &mut Canvas, t: &Palette, skin: &Skin, designer: &Designer, rect: Rect) {
    let value = f32::from(designer.value) / 100.0;
    let state = designer.state.kit();
    c.fill_rrect(rect, 5.0, t.mixer_bg);
    let (x, y) = (rect.cx(), rect.cy());
    match designer.part {
        Part::Knob => knob(c, t, skin, x, y, 39.0, value, false, state.hover),
        Part::Fader => {
            c.fill_rrect(
                Rect::new(x - 4.0, rect.y + 16.0, 8.0, rect.h - 32.0),
                3.0,
                t.mixer_strip_dark,
            );
            let y = y + (0.5 - value) * (rect.h - 65.0);
            fader(
                c,
                t,
                skin,
                Rect::new(x - 17.0, y - 23.0, 34.0, 46.0),
                state.on,
                state.hover,
            );
        }
        Part::Button => {
            let r = Rect::new(x - 65.0, y - 24.0, 130.0, 48.0);
            let ink = button(c, t, skin, r, state);
            c.text_center(Font::new(15).bold(), r, ink, "FX ENABLE");
        }
        Part::Meter => meter(
            c,
            t,
            skin,
            Rect::new(x - 72.0, y - 40.0, 144.0, 80.0),
            [value, value * 0.8],
            value > 0.94,
        ),
        Part::Panel => panel(
            c,
            t,
            skin,
            Rect::new(x - 98.0, y - 40.0, 196.0, 80.0),
            state.on,
        ),
    }
}
