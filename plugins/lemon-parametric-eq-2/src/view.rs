// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Lemon Parametric EQ editor: painting and hit regions (platform independent), built from `os_ui::kit` widgets.
//!
//! Resizable graph-led Lemon editor with floating contextual band controls. Window size and control scale are independent.
// spec: plugins/lemon-parametric-eq-2.md (§1 window, §2 graph, §3 faders and band column, §4 bottom row);
// ui/lemon-design.md §11.1

use std::f32::consts::PI;

use os_engine::effects::eq::{self, BANDS, Options, Settings, has_level};
use os_theme::Color;
use os_ui::kit;
use os_ui::kit::lemon;
use os_ui::{Canvas, Font, Rect};

use crate::params::{self, Opt, byte, flag};

pub const W: u32 = 1000;
pub const H: u32 = 620;
pub const MIN_W: u32 = 640;
pub const MIN_H: u32 = 420;
mod modern;
pub use modern::{Layout, q_of, width_of_q};
pub use modern::{available as modern_available, visible as modern_visible};
pub fn resized(_current: (u32, u32), wanted: (u32, u32)) -> (u32, u32) {
    (wanted.0.clamp(MIN_W, 2400), wanted.1.clamp(MIN_H, 1600))
}
pub fn gain_ring() -> (f32, f32) {
    let l = Layout::new((W as f32, H as f32), 1.);
    let p = l.panel(0.5, false);
    (p.x + 229., p.y + 65.)
}
pub fn resize_grip(x: f32, y: f32) -> bool {
    x >= W as f32 - 12. && y >= H as f32 - 12. && x <= W as f32 && y <= H as f32
}
pub fn paint(c: &mut Canvas, look: &Look, vs: &ViewState) -> Vec<Hit> {
    modern::paint(c, look, vs)
}

/// Analyzer resolution independent of window pixels.
pub const COLS: usize = 500;

/// The band colours: the original's hues as light pastels on the dark greys.
pub const BAND_COLORS: [u32; BANDS] = [
    0xc6a6f2, 0xf2a6d4, 0xf5b29a, 0xe9d27e, 0xacdd90, 0x92d9c7, 0xa1c0f2,
];

pub fn band_color(k: usize) -> Color {
    Color::rgb(BAND_COLORS[k % BANDS])
}

/// What a region of the editor does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Graph,
    Q(usize),
    Undo,
    Redo,
    PrevBand,
    NextBand,
    DeleteBand,
    Solo(usize),
    Audition(usize),
    BandCard,
    Details,
    ExpandDynamics,
    AutoDynamics,
    SpectralBand,
    UiScale,
    DisplayRange,
    Dynamic(usize),
    DynamicToggle,
    BandMenu(usize),
    HarshPanel,
    Token(usize),
    /// Band fader 0..6, 7 = main level.
    Fader(usize),
    TypeTab(usize),
    OrderMark(usize),
    Switch(usize),
    Freq(usize),
    Bw(usize),
    Options,
    Lin,
    Hq,
    Tokens,
    Phones,
    /// 0 off, 1 input, 2 output.
    Monitor(u8),
    Analyzer,
    GainMatch,
    Store,
    Flip,
}

pub const MAIN_FADER: usize = BANDS;

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub r: Rect,
    pub t: Target,
}

/// Editor-only state (not saved).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewState {
    pub gain_match: bool,
    pub compare_b: bool,
    pub size: (f32, f32),
    pub ui_scale: f32,
    pub range_db: f32,
    pub selection: u8,
    pub created: u8,
    pub marquee: Option<Rect>,
    pub spectral: Vec<f32>,
    pub trigger: [f32; 7],
    pub threshold: [f32; 7],
    pub selected: usize,
    pub show_dynamics: bool,
    pub details_open: bool,
    pub hide_band_card: bool,
    pub audition: Option<usize>,
    pub harsh_open: bool,
    pub reduction: [f32; 8],
    pub mouse: Option<(f32, f32)>,
    /// Band whose token is hovered or dragged (its curve and readout show).
    pub active: Option<usize>,
    pub pressed: Option<Target>,
}

/// What the editor shows.
pub struct Look<'a> {
    /// Shown values (pending edits included), with the enable mask.
    pub s: Settings,
    /// The state image (options).
    pub extra: &'a [u8],
    /// Spectrum level per column (dB), when monitoring.
    pub spectrum: Option<&'a [f32]>,
    pub rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_mappings_roundtrip_at_every_window_shape() {
        for size in [(640., 420.), (1000., 620.), (1400., 480.), (720., 1000.)] {
            let l = Layout::new(size, 1.25);
            for n in [0., 0.1, 0.5, 0.9, 1.] {
                assert!((l.n(l.x(n)) - n).abs() < 1e-6);
            }
            for db in [-18., -6., 0., 6., 18.] {
                assert!((l.db(l.y(db)) - db).abs() < 1e-5);
            }
        }
    }
}
