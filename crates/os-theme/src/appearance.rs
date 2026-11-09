// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional appearance extensions in a Lemon Studio .flstheme. Legacy themes inherit their skin.

use crate::{Color, Palette, Style};

pub const COLOR_PREFIX: &str = "LemonStudioColor.";
pub const STYLE_PREFIX: &str = "LemonStudioStyle.";

pub const NOTE_GROUPS: [&str; 16] = [
    "note_group_01",
    "note_group_02",
    "note_group_03",
    "note_group_04",
    "note_group_05",
    "note_group_06",
    "note_group_07",
    "note_group_08",
    "note_group_09",
    "note_group_10",
    "note_group_11",
    "note_group_12",
    "note_group_13",
    "note_group_14",
    "note_group_15",
    "note_group_16",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Flat,
    Beveled,
    Recessed,
}

impl Finish {
    pub const CHOICES: [&str; 4] = ["Inherit", "Flat", "Beveled", "Recessed"];

    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "flat" => Some(Self::Flat),
            "beveled" => Some(Self::Beveled),
            "recessed" => Some(Self::Recessed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    Round,
    Square,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    Dial,
    Ring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lettering {
    Bold,
    Italic,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mixer {
    Original,
    CleanModern,
    Analog,
    Theme,
}

/// Resolved rendering options. Each stays absent until a theme opts in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Appearance {
    pub buttons: Option<Finish>,
    pub panels: Option<Finish>,
    pub playlist_buttons: Option<Finish>,
    pub piano_buttons: Option<Finish>,
    pub rack_steps: Option<Finish>,
    pub mixer_buttons: Option<Finish>,
    pub indicator: Option<Indicator>,
    pub knob: Option<Knob>,
    pub mixer: Option<Mixer>,
    pub title_text: Option<Lettering>,
    pub icon_pack: u8,
    pub icon_mixing: bool,
    pub icon_color: Option<Color>,
    pub icon_variants: [[u8; 16]; 5],
    pub icon_colors: [[Option<Color>; 16]; 5],
    pub cursor_color: Option<Color>,
    pub cursor_size: Option<f32>,
    pub cursor_family: Option<u8>,
    /// Stable cursor roles: 0 inherits, otherwise family + 1 (including Default).
    pub cursor_variants: [[u8; 16]; 3],
    pub ui_font: u8,
    pub bevel: Option<f32>,
    pub border: Option<f32>,
    pub button_radius: Option<f32>,
    pub small_radius: Option<f32>,
    pub panel_radius: Option<f32>,
    pub playlist_lines: Option<f32>,
    pub piano_lines: Option<f32>,
    pub themed_buttons: bool,
    pub note_groups: u16,
}

pub const CUSTOM_ICON_PACKS: u8 = 27;
pub const VINTAGE_ICON_PACK: u8 = 27;
pub const VINTAGE_CURSOR_PACK: u8 = 28;

impl Appearance {
    pub(crate) fn parse(get: impl Fn(&str) -> Option<String>) -> Self {
        let option = |key: &str| get(key).unwrap_or_default();
        let num = |key: &str, max: f32| {
            get(key)?
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .map(|n| n.clamp(0.0, max))
        };
        Self {
            buttons: Finish::parse(&option("buttons")),
            panels: Finish::parse(&option("panels")),
            playlist_buttons: Finish::parse(&option("playlist-buttons")),
            piano_buttons: Finish::parse(&option("piano-buttons")),
            rack_steps: Finish::parse(&option("rack-steps")),
            mixer_buttons: Finish::parse(&option("mixer-buttons")),
            indicator: match option("indicator").as_str() {
                "Round" => Some(Indicator::Round),
                "Square" => Some(Indicator::Square),
                "Bar" => Some(Indicator::Bar),
                _ => None,
            },
            knob: match option("knob").as_str() {
                "Dial" => Some(Knob::Dial),
                "Ring" => Some(Knob::Ring),
                _ => None,
            },
            mixer: match option("mixer").as_str() {
                "Original" => Some(Mixer::Original),
                "Clean Modern" => Some(Mixer::CleanModern),
                "Analog" => Some(Mixer::Analog),
                "Theme colors" => Some(Mixer::Theme),
                _ => None,
            },
            // The previous numeric setting described weight and tint recipes. Only the
            // explicit icon-family setting opts a theme into the authored artwork.
            icon_pack: option("icon-family").parse::<u8>().ok().filter(|id| *id <= CUSTOM_ICON_PACKS).unwrap_or(0),
            icon_mixing: option("icon-mixing") == "1",
            icon_color: Color::parse(&option("icon-color")),
            // IDs are append-only positions in the semantic icon catalogue.
            icon_variants: std::array::from_fn(|row| std::array::from_fn(|col| {
                option(&format!("icon-{}-family", row * 16 + col)).parse::<u8>()
                    .ok().filter(|id| *id <= CUSTOM_ICON_PACKS + 1).unwrap_or(0)
            })),
            icon_colors: std::array::from_fn(|row| std::array::from_fn(|col| {
                Color::parse(&option(&format!("icon-{}-color", row * 16 + col)))
            })),
            cursor_color: Color::parse(&option("cursor-color")),
            cursor_size: num("cursor-size", 200.0).map(|n| n.clamp(50.0, 200.0)),
            cursor_family: option("cursor-family").parse::<u8>().ok().filter(|id| *id <= VINTAGE_CURSOR_PACK),
            cursor_variants: std::array::from_fn(|row| std::array::from_fn(|col| {
                option(&format!("cursor-{}-family", row * 16 + col)).parse::<u8>()
                    .ok().filter(|id| *id <= VINTAGE_CURSOR_PACK + 1).unwrap_or(0)
            })),
            ui_font: num("ui-font", 4.0).unwrap_or(0.0) as u8,
            title_text: match option("title-text").as_str() {
                "Bold" => Some(Lettering::Bold),
                "Italic" => Some(Lettering::Italic),
                "Light" => Some(Lettering::Light),
                _ => None,
            },
            bevel: num("bevel", 40.0).map(|n| n / 100.0),
            border: num("border", 3.0),
            button_radius: num("button-radius", 12.0),
            small_radius: num("small-radius", 8.0),
            panel_radius: num("panel-radius", 20.0),
            playlist_lines: num("playlist-lines", 200.0).map(|n| n / 100.0),
            piano_lines: num("piano-lines", 200.0).map(|n| n / 100.0),
            themed_buttons: false,
            note_groups: 0,
        }
    }

    pub(crate) fn style(self, base: Style) -> Style {
        Style {
            radius: self.button_radius.unwrap_or(base.radius),
            radius_small: self.small_radius.unwrap_or(base.radius_small),
            radius_panel: self.panel_radius.unwrap_or(base.radius_panel),
            ..base
        }
    }

    pub(crate) fn apply(mut self, palette: &mut Palette, colors: &[(&'static str, Color)]) {
        for &(name, color) in colors {
            if let Some(slot) = palette.get_mut(name) {
                *slot = color;
            }
            if let Some(group) = NOTE_GROUPS.iter().position(|&key| key == name) {
                self.note_groups |= 1 << group;
            }
            if matches!(name, "indicator_on" | "indicator_off") {
                self.indicator.get_or_insert(Indicator::Round);
            }
            if matches!(name, "knob_cap" | "knob_pointer" | "knob_ring") {
                self.knob.get_or_insert(Knob::Ring);
            }
            self.themed_buttons |= matches!(
                name,
                "button"
                    | "button_hi"
                    | "button_pressed"
                    | "button_disabled"
                    | "button_border"
                    | "on_top"
                    | "on_bottom"
            );
        }
        palette.style = self.style(palette.style);
        palette.pl_lines = self.playlist_lines.unwrap_or(palette.pl_lines);
        palette.pr_lines = self.piano_lines.unwrap_or(palette.pr_lines);
        palette.appearance = self;
    }
}

#[cfg(test)]
mod icon_family_tests {
    use super::*;

    #[test]
    fn icon_families_require_explicit_selection() {
        let parse = |old: Option<&str>, family: Option<&str>| Appearance::parse(|key| {
            match key {
                "icon-pack" => old,
                "icon-family" => family,
                _ => None,
            }.map(str::to_owned)
        }).icon_pack;
        assert_eq!(parse(None, None), 0);
        assert_eq!(parse(Some("7"), None), 0);
        assert_eq!(parse(Some("2"), Some("0")), 0);
        assert_eq!(parse(Some("2"), Some("13")), 13);
        assert_eq!(parse(None, Some("28")), 0);
        assert_eq!(parse(None, Some("1.2")), 0);
    }
}
