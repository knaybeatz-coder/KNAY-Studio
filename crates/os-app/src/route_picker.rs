// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared miniature mixer destination picker for audio clips and channel TRACK controls.
use crate::{
    app::App,
    dialog::{Body, Field},
};
pub const COLS: usize = 12;
pub const ROWS: usize = 4;
pub struct Track {
    pub index: u16,
    pub name: String,
    pub color: Option<os_model::Rgb>,
    pub level: f64,
}
pub struct Form {
    pub row: usize,
    pub channel: String,
    pub current: u16,
    pub tracks: Vec<Track>,
    pub shown: Vec<usize>,
    pub search: Field,
    pub first: usize,
}
impl Form {
    pub fn filter(&mut self) {
        let q = self.search.text.to_lowercase();
        self.shown = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                q.split_whitespace()
                    .all(|w| format!("{} {}", t.index, t.name).to_lowercase().contains(w))
            })
            .map(|(i, _)| i)
            .collect();
        self.first = 0;
    }
    pub fn scroll(&mut self, rows: isize) {
        let max = (self.shown.len().div_ceil(COLS)).saturating_sub(ROWS);
        self.first = (self.first / COLS).saturating_add_signed(rows).min(max) * COLS;
    }
}
pub fn open(app: &mut App, row: usize) {
    let Some(ch) = app.state.project.channels.get(row) else {
        return;
    };
    let mut tracks: Vec<_> = app
        .state
        .project
        .mixer
        .iter()
        .filter(|m| m.index <= crate::mixer::max_insert(&app.state.project))
        .map(|m| Track {
            index: m.index,
            name: crate::mixer::name(&app.state, m.index),
            color: m.color,
            level: m.volume.unwrap_or(0.8),
        })
        .collect();
    tracks.sort_by_key(|t| t.index);
    let mut f = Form {
        row,
        channel: ch
            .name
            .clone()
            .unwrap_or_else(|| format!("Channel {}", row + 1)),
        current: ch.mixer_track,
        tracks,
        shown: Vec::new(),
        search: Field::default(),
        first: 0,
    };
    f.filter();
    if let Some(i) = f.tracks.iter().position(|t| t.index == f.current) {
        f.scroll((i / COLS) as isize);
    }
    app.state.playlist.popup = None;
    app.show_form(Body::RoutePicker(f));
}
