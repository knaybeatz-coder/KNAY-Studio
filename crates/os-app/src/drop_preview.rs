// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-destructive sample drop planning shared by Browser and native file drags.
use crate::{
    app::{App, State, Win},
    playlist as pl,
};
use os_ui::{Color, Font, Rect};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
pub struct Preview {
    pub paths: Vec<String>,
    pub external: bool,
    pub surface: Option<Win>,
    pub plan: Vec<Placement>,
    pub loading: bool,
    notice: Option<(f32, f32, String)>,
    dirty: bool,
    failed: std::collections::BTreeSet<String>,
    trim: bool,
    windows: std::collections::HashMap<String, Option<(f64, f64)>>,
    job: Option<mpsc::Receiver<(String, Option<pl::Wave>, Option<(f64, f64)>)>>,
    cancel: Option<Arc<AtomicBool>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(tag: &str) -> (App, os_ui::Canvas, PathBuf, Vec<PathBuf>) {
        crate::prefs::set_text("browser_auto_trim", "0");
        let root = std::env::temp_dir().join(format!("lemon-ghost-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let files: Vec<_> = [192000, 384000, 288000]
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let path = root.join(format!(
                    "{} - {}.wav",
                    i + 1,
                    ["Drum loop", "Warm bass", "Glass keys"][i]
                ));
                let audio: Vec<_> = (0..n)
                    .flat_map(|f| {
                        let env = (1. - (f % 12000) as f32 / 12000.).powi(2);
                        let v = (f as f32 * 0.04).sin() * env * 0.65;
                        [v, v]
                    })
                    .collect();
                os_engine::wav::write_f32_stereo(
                    &mut std::fs::File::create(&path).unwrap(),
                    48000,
                    &audio,
                )
                .unwrap();
                path
            })
            .collect();
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.project.arrangements = vec![os_flp::project::default_arrangement()];
        app.state.project.current_arrangement = Some(0);
        app.state.z = vec![Win::Playlist];
        let mut c = os_ui::Canvas::new(1280, 800, 1.).unwrap();
        app.paint(&mut c, None);
        (app, c, root, files)
    }
    fn ready(app: &mut App) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            poll(app);
            if !app.state.sample_drop.loading {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "background preview stalled"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    fn cleanup(root: PathBuf) {
        assert!(
            root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("lemon-ghost-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ghost_native_batch_matches_drop_and_cancels_without_edits() {
        let (mut app, mut canvas, root, files) = setup("native");
        let tick = pl::bar(&app.state);
        app.state.project.arrangements[0]
            .clips
            .push(os_model::Clip::new(
                tick,
                tick,
                2,
                os_model::ClipSource::Pattern(1),
            ));
        app.state.project.arrangements[0].tracks[3].locked = true;
        app.paint(&mut canvas, None);
        let grid = app.pl_grid().unwrap();
        let g = pl::PGeom::new(&app.state, grid);
        let point = (g.x_of(tick as f32) + 2., g.y_of(2) + 10.);
        app.mouse_move(point.0, point.1);
        let before = app.state.project.clone();
        for p in &files {
            hover_file(&mut app, p.clone(), None);
        }
        ready(&mut app);
        let preview = app.state.sample_drop.plan.clone();
        assert_eq!(
            preview.iter().map(|p| p.track).collect::<Vec<_>>(),
            vec![4, 5, 6]
        );
        assert!(preview.iter().all(|p| p.tick == tick));
        assert_eq!(app.state.project, before, "hover never edits project");
        app.paint(&mut canvas, None);
        if let Some(out) = std::env::var_os("LEMON_GHOST_PREVIEW") {
            canvas.pixmap.save_png(out).unwrap();
        }
        app.key(
            &winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape),
            false,
            false,
        );
        assert!(!app.state.sample_drop.active());
        assert_eq!(app.state.project, before);
        for p in &files {
            hover_file(&mut app, p.clone(), None);
        }
        ready(&mut app);
        assert_eq!(app.state.sample_drop.plan, preview);
        clear(&mut app);
        app.import_stems(files.clone(), 2, tick);
        let clips = &pl::clips(&app.state)[1..];
        assert_eq!(clips.len(), preview.len());
        for (c, p) in clips.iter().zip(&preview) {
            assert_eq!((c.track, c.position, c.length), (p.track, p.tick, p.length));
        }
        assert_eq!(app.state.playlist.sel.len(), 3);
        cleanup(root);
    }
    #[test]
    fn ghost_trimmed_length_view_changes_and_pending_cancel() {
        let (mut app, mut canvas, root, files) = setup("trim");
        // Rebuild one source with long silent handles. Preview and import must agree on trimming.
        let audio: Vec<f32> = (0..48000)
            .flat_map(|i| {
                let v = if (12000..36000).contains(&i) { 0.4 } else { 0. };
                [v, v]
            })
            .collect();
        let file = root.join("trimmed.wav");
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&file).unwrap(), 48000, &audio)
            .unwrap();
        crate::prefs::set_text("browser_auto_trim", "1");
        let grid = app.pl_grid().unwrap();
        let g = pl::PGeom::new(&app.state, grid);
        app.mouse_move(g.x_of(pl::bar(&app.state) as f32), g.y_of(1) + 10.);
        hover_file(&mut app, file.clone(), None);
        ready(&mut app);
        let first = app.state.sample_drop.plan[0].clone();
        assert!(first.length < app.state.project.ppq as u32 * 2);
        let worker = app.state.sample_drop.cancel.clone().unwrap();
        app.alt = true;
        for i in 0..100 {
            app.mouse_move(grid.x + 50. + i as f32 * 0.1, grid.y + 60.);
            refresh(&mut app);
        }
        assert!(
            Arc::ptr_eq(&worker, app.state.sample_drop.cancel.as_ref().unwrap()),
            "pointer movement never restarts file analysis"
        );
        let planned = app.state.sample_drop.plan[0].clone();
        assert_ne!(planned.tick, first.tick);
        let model = app.state.project.clone();
        app.mouse_move(1., 1.);
        refresh(&mut app);
        assert!(app.state.sample_drop.plan.is_empty());
        assert_eq!(app.state.project, model);
        app.mouse_move(grid.x + 60., grid.y + 60.);
        refresh(&mut app);
        let planned = app.state.sample_drop.plan[0].clone();
        clear(&mut app);
        app.import_stems(vec![file], planned.track, planned.tick);
        let clip = pl::clips(&app.state).last().unwrap();
        assert_eq!((clip.position, clip.length), (planned.tick, planned.length));
        assert!(clip.window.is_some());
        crate::prefs::set_text("browser_auto_trim", "0");
        hover_file(&mut app, files[0].clone(), None);
        poll(&mut app);
        clear(&mut app);
        poll(&mut app);
        assert!(!app.state.sample_drop.active());
        assert!(app.state.sample_drop.plan.is_empty());
        app.paint(&mut canvas, None);
        cleanup(root);
    }
    #[test]
    fn ghost_browser_ctrl_shift_selection_and_group_drag() {
        use crate::{
            app::{Action, Drag},
            views::browser::{self as view, Item},
        };
        let (mut app, mut canvas, root, files) = setup("browser");
        app.add_browser_folder(root.clone());
        app.state.browser_open.insert(view::disk_key(&root));
        app.paint(&mut canvas, None);
        let indices: Vec<_> = files
            .iter()
            .map(|p| {
                view::rows(&app.state)
                    .iter()
                    .position(|r| r.path.as_ref() == Some(p))
                    .unwrap()
            })
            .collect();
        for (i, ctrl, shift) in [(0, false, false), (2, true, false), (0, false, true)] {
            app.ctrl = ctrl;
            app.shift = shift;
            assert!(app.browser_act(&Action::BrowserRow(indices[i])));
            app.browser_drop(Item::Sample(files[i].to_string_lossy().into_owned()), false);
            app.drag = None;
        }
        assert_eq!(app.state.browser_samples.len(), 3);
        app.ctrl = false;
        app.shift = false;
        app.browser_act(&Action::BrowserRow(indices[0]));
        let Some(Drag::Browser {
            item: Item::Samples(paths),
            ..
        }) = app.drag.clone()
        else {
            panic!("selected files must drag together")
        };
        assert_eq!(
            paths,
            files
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        );
        let grid = app.pl_grid().unwrap();
        let g = pl::PGeom::new(&app.state, grid);
        let point = (g.x_of(pl::bar(&app.state) as f32), g.y_of(1) + 10.);
        app.mouse_move(point.0, point.1);
        ready(&mut app);
        let preview = app.state.sample_drop.plan.clone();
        assert_eq!(preview.len(), 3);
        app.browser_drop(Item::Samples(paths), true);
        assert!(!app.state.sample_drop.active());
        for (c, p) in pl::clips(&app.state).iter().zip(preview) {
            assert_eq!((c.track, c.position, c.length), (p.track, p.tick, p.length));
        }
        assert_eq!(pl::clips(&app.state).len(), 3);
        cleanup(root);
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub path: String,
    pub track: u16,
    pub tick: u32,
    pub length: u32,
}
impl Preview {
    pub fn active(&self) -> bool {
        !self.paths.is_empty()
    }
    pub fn clear(&mut self) {
        if let Some(c) = self.cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        *self = Self::default();
    }
    pub fn start(&mut self, paths: Vec<String>, external: bool, surface: Option<Win>) {
        if self.paths == paths && self.external == external && self.surface == surface {
            return;
        }
        self.clear();
        self.paths = paths;
        self.external = external;
        self.surface = surface;
        self.dirty = true;
        self.trim = crate::browser_preview::auto_trim();
    }
}
pub fn hover_file(app: &mut App, path: PathBuf, surface: Option<Win>) {
    if crate::views::browser::file_icon(&path.to_string_lossy()) != crate::icons::Icon::FileAudio {
        return;
    }
    let mut paths = if app.state.sample_drop.external && app.state.sample_drop.surface == surface {
        app.state.sample_drop.paths.clone()
    } else {
        Vec::new()
    };
    let path = path.to_string_lossy().into_owned();
    if !paths.contains(&path) {
        paths.push(path);
    }
    app.state.sample_drop.start(paths, true, surface);
    app.invalidate();
}
pub fn clear(app: &mut App) {
    if app.state.sample_drop.active() {
        app.state.sample_drop.clear();
        app.invalidate();
    }
}

/// File lengths and peaks decode once off the UI thread. Mouse movement only plans/draws cached data.
pub fn poll(app: &mut App) {
    let s = &mut app.state;
    if s.sample_drop.dirty {
        s.sample_drop.dirty = false;
        let paths: Vec<_> = s
            .sample_drop
            .paths
            .iter()
            .filter(|p| {
                !s.playlist.waves.borrow().contains_key(*p)
                    || (s.sample_drop.trim && !s.sample_drop.windows.contains_key(*p))
            })
            .cloned()
            .collect();
        if !paths.is_empty() {
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let stop = cancel.clone();
            let trim = s.sample_drop.trim;
            s.sample_drop.cancel = Some(cancel);
            s.sample_drop.job = Some(rx);
            std::thread::spawn(move || {
                for path in paths {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut window = None;
                    let wave = os_engine::sample::load_shared(std::path::Path::new(&path))
                        .ok()
                        .map(|sample| {
                            if trim {
                                let (a, b) = crate::browser_preview::trim_bounds(&sample);
                                if a > 0 || b < sample.frames() {
                                    let k = 1000. / sample.rate.max(1) as f64;
                                    window = Some((a as f64 * k, b as f64 * k));
                                }
                            }
                            let per = 64;
                            let peaks = sample
                                .left
                                .chunks(per)
                                .zip(sample.right.chunks(per))
                                .map(|(l, r)| {
                                    l.iter()
                                        .zip(r)
                                        .map(|(a, b)| (a + b) * 0.5)
                                        .fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                                            (lo.min(v), hi.max(v))
                                        })
                                })
                                .collect();
                            pl::Wave {
                                fine: std::cell::RefCell::new(None),
                                rate: sample.rate,
                                frames: sample.frames(),
                                per,
                                peaks,
                            }
                        });
                    if stop.load(Ordering::Relaxed) || tx.send((path, wave, window)).is_err() {
                        break;
                    }
                }
            });
        }
    }
    if let Some(rx) = &s.sample_drop.job {
        while let Ok((path, wave, window)) = rx.try_recv() {
            s.sample_drop.windows.insert(path.clone(), window);
            if let Some(wave) = wave {
                s.playlist
                    .waves
                    .borrow_mut()
                    .insert(path, Some(Rc::new(wave)));
            } else {
                s.sample_drop.failed.insert(path);
            }
        }
    }
    refresh(app);
}
pub fn refresh(app: &mut App) {
    if !app.state.sample_drop.active() {
        return;
    }
    let paths = app.state.sample_drop.paths.clone();
    let loading = paths.iter().any(|p| {
        (!app.state.playlist.waves.borrow().contains_key(p)
            && !app.state.sample_drop.failed.contains(p))
            || (app.state.sample_drop.trim && !app.state.sample_drop.windows.contains_key(p))
    });
    let allowed = !app.modal_open()
        && app.state.sample_drop.surface == app.state.native_view
        && app.top_hit(app.mouse.0, app.mouse.1).and_then(|h| h.win) == Some(Win::Playlist);
    let plan = if allowed && !loading {
        app.playlist_drop_position()
            .map(|(track, tick)| plan(&app.state, &paths, track, tick))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let notice = if allowed && loading {
        Some((
            app.mouse.0,
            app.mouse.1,
            "Preparing sample preview...".into(),
        ))
    } else if allowed && plan.len() != paths.len() {
        Some((
            app.mouse.0,
            app.mouse.1,
            format!("{} of {} samples can be placed", plan.len(), paths.len()),
        ))
    } else {
        None
    };
    if plan != app.state.sample_drop.plan
        || loading != app.state.sample_drop.loading
        || notice != app.state.sample_drop.notice
    {
        app.state.sample_drop.notice = notice;
        app.state.sample_drop.plan = plan;
        app.state.sample_drop.loading = loading;
        app.damage_win(Win::Playlist);
    }
}
pub fn plan(s: &State, paths: &[String], from: u16, tick: u32) -> Vec<Placement> {
    let mut out = Vec::new();
    let mut from = from;
    for path in paths {
        let Some(w) = s.playlist.waves.borrow().get(path).cloned().flatten() else {
            continue;
        };
        let length = pl::audio_drop_length_for_window(
            s,
            w.secs(),
            s.sample_drop.windows.get(path).copied().flatten(),
        );
        if let Some(track) = pl::audio_drop_track(
            s,
            from,
            tick,
            length,
            &out.iter().map(|p: &Placement| p.track).collect::<Vec<_>>(),
        ) {
            out.push(Placement {
                path: path.clone(),
                track,
                tick,
                length,
            });
            from = track.saturating_add(1);
        }
    }
    out
}
pub fn paint(ui: &mut crate::app::Ui, s: &State, g: &pl::PGeom) {
    if s.sample_drop.surface != s.native_view {
        return;
    }
    let lemon = Color::rgb(0xffdc58);
    for (n, p) in s.sample_drop.plan.iter().enumerate() {
        let x = g.x_of(p.tick as f32);
        let right = g.x_of(p.tick.saturating_add(p.length) as f32);
        let r = Rect::new(
            x,
            g.y_of(p.track),
            (right - x).max(2.),
            (g.rows.height(p.track) - 3.).max(1.),
        );
        if !ui.c.visible(r) {
            continue;
        }
        ui.c.push_clip(r);
        let color = s.clip_colors.new_channel(
            s.project
                .channels
                .iter()
                .map(|c| c.index + 1)
                .max()
                .unwrap_or(0)
                .saturating_add(n as u16),
        );
        let color =
            Color::rgb((u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b));
        ui.c.fill_rrect(r.inset(0.5), 3., color.with_alpha(105));
        ui.c.fill_rect(Rect::new(r.x, r.y, r.w, 17.), lemon.with_alpha(55));
        if let Some(w) = s.playlist.waves.borrow().get(&p.path).cloned().flatten() {
            let body = Rect::new(r.x, r.y + 18., r.w, (r.h - 20.).max(0.));
            let (a, b) = s
                .sample_drop
                .windows
                .get(&p.path)
                .copied()
                .flatten()
                .unwrap_or((0., w.secs() * 1000.));
            let begin = (a / (w.secs() * 1000.)) as f32;
            let span = ((b - a) / (w.secs() * 1000.)) as f32;
            let cols = r.w.ceil().min(g.grid.w + 2.).max(0.) as usize;
            let start = r.x.max(g.grid.x);
            for col in 0..cols {
                let px = start + col as f32;
                if px >= right.min(g.grid.right()) {
                    break;
                }
                let a = ((begin + (px - r.x) / r.w * span) * w.peaks.len() as f32) as usize;
                let b = (((begin + (px + 1. - r.x) / r.w * span) * w.peaks.len() as f32).ceil()
                    as usize)
                    .max(a + 1)
                    .min(w.peaks.len());
                if let Some(peaks) = w.peaks.get(a..b) {
                    let (lo, hi) = peaks
                        .iter()
                        .fold((0f32, 0f32), |(lo, hi), &(a, b)| (lo.min(a), hi.max(b)));
                    ui.c.line(
                        px,
                        body.cy() - hi.clamp(-1., 1.) * body.h * 0.42,
                        px,
                        body.cy() - lo.clamp(-1., 1.) * body.h * 0.42,
                        1.,
                        Color::rgb(0xfff2b5).with_alpha(175),
                    );
                }
            }
        }
        let name = p.path.rsplit(['\\', '/']).next().unwrap_or(&p.path);
        ui.c.text_clip(
            Font::new(12),
            r.x.max(g.grid.x) + 6.,
            r.y + 1.,
            (r.right() - r.x.max(g.grid.x) - 10.).max(0.),
            Color::rgb(0xfff5ce),
            name,
        );
        ui.c.pop_clip();
        ui.c.stroke_rrect(r.inset(0.7), 3., 1.4, lemon.with_alpha(235));
        ui.c.fill_rect(Rect::new(r.x, r.y + 2., 2., (r.h - 4.).max(0.)), lemon);
    }
    if let Some((x, y, text)) = &s.sample_drop.notice {
        let r = Rect::new(
            (x + 16.).clamp(g.grid.x, (g.grid.right() - 226.).max(g.grid.x)),
            (y + 18.).clamp(g.grid.y, (g.grid.bottom() - 28.).max(g.grid.y)),
            224.,
            26.,
        );
        ui.c.fill_rrect(r, 5., Color::rgb(0x222923));
        ui.c.stroke_rrect(r, 5., 1., lemon.with_alpha(160));
        ui.c.text_clip(Font::new(12), r.x + 8., r.y + 5., r.w - 16., lemon, text);
    }
}
