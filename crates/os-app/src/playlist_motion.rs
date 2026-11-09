// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
//! Adapted from Lemon Studio motion.rs (clip_jumped/clip_glide), upstream
//! 73e6389821c59ffd5d738f0ca43e926d5ab84a89. Visual offsets never alter clip timing.
use crate::app::{App, State};
use os_ui::Rect;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
const SNAP_TAU: f64 = 0.024;
const SNAP_ARRIVED: f32 = 0.25;
const FIRST_STEP: f64 = 0.016;
#[derive(Default)]
pub struct Motion {
    context: Option<(Option<u16>, usize)>,
    glides: HashMap<usize, Glide>,
}
struct Glide {
    left: f32,
    right: f32,
    down: f32,
    at: Instant,
}
impl Motion {
    pub fn clear(&mut self) {
        self.glides.clear();
        self.context = None;
    }
    pub fn active(&self) -> bool {
        !self.glides.is_empty()
    }
    fn context(&mut self, arr: Option<u16>, count: usize) {
        if self.context != Some((arr, count)) {
            self.clear();
            self.context = Some((arr, count));
        }
    }
    pub fn jump(
        &mut self,
        arr: Option<u16>,
        count: usize,
        key: usize,
        (left, right, down): (f32, f32, f32),
        now: Instant,
    ) {
        if crate::smooth::smoothing() <= 0. {
            self.clear();
            return;
        }
        self.context(arr, count);
        let at = now
            .checked_sub(Duration::from_secs_f64(FIRST_STEP))
            .unwrap_or(now);
        if let Some(g) = self.glides.get_mut(&key) {
            let k = (-now.saturating_duration_since(g.at).as_secs_f64() / SNAP_TAU).exp() as f32;
            (g.left, g.right, g.down, g.at) = (
                g.left * k + left,
                g.right * k + right,
                g.down * k + down,
                at,
            );
        } else {
            self.glides.insert(
                key,
                Glide {
                    left,
                    right,
                    down,
                    at,
                },
            );
        }
    }
    pub fn offset(
        &mut self,
        arr: Option<u16>,
        count: usize,
        key: usize,
        now: Instant,
    ) -> Option<(f32, f32, f32)> {
        self.context(arr, count);
        if crate::smooth::smoothing() <= 0. {
            self.clear();
            return None;
        }
        let g = self.glides.get(&key)?;
        let k = (-now.saturating_duration_since(g.at).as_secs_f64() / SNAP_TAU).exp() as f32;
        let at = (g.left * k, g.right * k, g.down * k);
        if at.0.abs().max(at.1.abs()).max(at.2.abs()) < SNAP_ARRIVED {
            self.glides.remove(&key);
            None
        } else {
            Some(at)
        }
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let was = self.active();
        self.glides.retain(|_, g| {
            let k = (-now.saturating_duration_since(g.at).as_secs_f64() / SNAP_TAU).exp() as f32;
            k * g.left.abs().max(g.right.abs()).max(g.down.abs()) >= SNAP_ARRIVED
                && crate::smooth::smoothing() > 0.
        });
        was
    }
}
pub fn tick(app: &mut App) {
    let moving = app.state.playlist_motion.borrow_mut().tick(Instant::now());
    if moving {
        app.damage_win(crate::app::Win::Playlist);
    }
}

/// Upstream's magnet-follow rule: free movement stays under the pointer; visible snap/row jumps glide.
pub fn jumped(s: &State, g: &crate::playlist::PGeom, from: Vec<(usize, Rect)>, alt: bool) {
    const NO_STEP: f32 = 3.;
    let stepped =
        !alt && crate::playlist::grid_snap(s).ticks(g.ppq, g.bar_w) as f32 * g.ppt >= NO_STEP;
    let count = crate::playlist::clips(s).len();
    let now = Instant::now();
    for (i, r0) in from {
        let Some(c) = crate::playlist::clips(s).get(i) else {
            continue;
        };
        let r1 = g.clip_rect(c);
        let (left, right, down) = (r0.x - r1.x, r0.right() - r1.right(), r0.y - r1.y);
        let (left, right) = if stepped { (left, right) } else { (0., 0.) };
        if left != 0. || right != 0. || down != 0. {
            s.playlist_motion.borrow_mut().jump(
                s.project.current_arrangement,
                count,
                i,
                (left, right, down),
                now,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_clip_glide_settles_and_respects_smoothing_and_identity() {
        crate::prefs::set_slider(&crate::prefs::SMOOTHING, 128);
        let mut motion = Motion::default();
        let now = Instant::now();
        motion.jump(Some(0), 3, 1, (-120., -120., 48.), now);
        let first = motion.offset(Some(0), 3, 1, now).unwrap();
        assert!(first.0 < 0. && first.0 > -120. && first.2 > 0.);
        let later = motion
            .offset(Some(0), 3, 1, now + Duration::from_millis(24))
            .unwrap();
        assert!(later.0.abs() < first.0.abs());
        assert!(
            motion
                .offset(Some(0), 3, 1, now + Duration::from_secs(1))
                .is_none()
        );
        assert!(!motion.active());
        motion.jump(Some(0), 3, 1, (100., 100., 0.), now);
        assert!(
            motion.offset(Some(1), 3, 1, now).is_none(),
            "another arrangement never inherits a glide"
        );
        motion.jump(Some(1), 3, 1, (100., 100., 0.), now);
        assert!(
            motion.offset(Some(1), 4, 1, now).is_none(),
            "added/deleted clip resets identity"
        );
        crate::prefs::set_slider(&crate::prefs::SMOOTHING, 0);
        motion.jump(Some(1), 4, 1, (100., 100., 0.), now);
        assert!(!motion.active());
    }
    #[test]
    fn upstream_event_marker_dragging_preserves_groups_and_scroll_anchor() {
        use crate::playlist as pl;
        use crate::snap::EditorSnap;
        use os_model::{Clip, ClipSource, TimeMarker};
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.project.arrangements = vec![os_flp::project::default_arrangement()];
        app.state.project.current_arrangement = Some(0);
        pl::arr_mut(&mut app.state).clips = vec![
            Clip::new(96, 96, 1, ClipSource::Pattern(1)),
            Clip::new(500, 100, 2, ClipSource::Pattern(1)),
            Clip::new(142, 96, 3, ClipSource::Pattern(1)),
        ];
        let grid = Rect::new(200., 100., 1000., 600.);
        let g = pl::PGeom::new(&app.state, grid);
        let orig = vec![(0, pl::clips(&app.state)[0]), (2, pl::clips(&app.state)[2])];
        let drag = pl::PlDrag::Move {
            x0: 300.,
            y0: g.y_of(1) + 10.,
            scroll_x: g.scroll_x,
            track0: 1,
            orig: orig.clone(),
            fresh: false,
        };
        // Keep the custom scroll anchor while using upstream's nearest edge catch.
        app.state.playlist_scroll.0 = 40.;
        app.mouse = (300. + 365. * g.ppt - 40., g.y_of(1) + 10.);
        app.state.playlist.snap = EditorSnap::Events;
        pl::drag_move(&mut app, drag.clone(), grid);
        assert_eq!(pl::clips(&app.state)[0].position, 500);
        assert_eq!(pl::clips(&app.state)[2].position, 546);
        assert_eq!(pl::clips(&app.state)[1].position, 500);
        app.alt = true;
        pl::drag_move(&mut app, drag.clone(), grid);
        assert_eq!(pl::clips(&app.state)[0].position, 461);
        app.alt = false;
        app.state.playlist.snap = EditorSnap::Markers;
        pl::arr_mut(&mut app.state)
            .markers
            .push(TimeMarker::new(480, "Verse"));
        pl::drag_move(&mut app, drag.clone(), grid);
        assert_eq!(pl::clips(&app.state)[0].position, 480);
        assert_eq!(pl::clips(&app.state)[2].position, 526);
        let g = pl::PGeom::new(&app.state, grid);
        let near = 480. + 3. / g.ppt;
        assert_eq!(pl::snap_place(&app.state, &g, near, false), 480);
        assert_ne!(pl::snap_place(&app.state, &g, near, true), 480);
        let menu = pl::snap_menu(&app.state, 0., 0.);
        assert!(
            menu.items
                .iter()
                .filter(|i| i.label == "Events" || i.label == "Markers")
                .all(|i| i.enabled)
        );
    }
}
