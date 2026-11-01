// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! VariAudio: the piano roll's pitch-editing mode for an audio clip (a sung part). An audio clip's menu ▸ VariAudio
//! opens its sample in the piano roll: each sung note is a segment at its pitch, with the pitch curve drawn through
//! it. Dragging segments moves them by semitones (Alt: freely, in cents); Quantize pitch snaps them to the nearest
//! semitone; Straighten flattens their curve (vibrato, scoops). The piano roll is the ordinary piano roll again once
//! VariAudio closes (Close, Esc).
//!
//! Non-destructive: the original file is never written. Each change renders the edited take in a worker
//! (`vari_dsp::render`, TD-PSOLA) to `<project folder>/Rendered/<name> (VariAudio <time>).wav`, and the clip's
//! channel plays that file. A sidecar `<rendered>.vari.json` keeps the original's path, the segments and the edits,
//! so opening VariAudio on the clip again (even after reopening the project) goes on from the original.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use os_model::{ChannelKind, ClipSource};
use os_ui::{Color, Font, Rect};

use crate::app::{Action, App, Drag, State, Ui, Win};
use crate::views::piano::TOP_KEY;
use crate::playlist as pl;
use crate::vari_dsp::{self as dsp, Edit, Frame, Segment};

/// An open VariAudio session.
#[derive(Debug, Clone)]
pub struct Session {
    /// The audio clip channel (its index) and its name.
    pub channel: u16,
    pub name: String,
    /// The playlist clip it was opened from (playhead), if still there.
    pub clip: Option<usize>,
    /// The original sample's path (never written).
    pub original: String,
    pub sample: Arc<os_engine::sample::Sample>,
    pub frames: Arc<Vec<Frame>>,
    pub segs: Vec<Segment>,
    pub edits: Vec<Edit>,
    pub sel: Vec<usize>,
    /// Waveform: the peak level per pitch frame (0..1).
    pub peaks: Vec<f32>,
    pub secs: f64,
    /// Time view: pixels per second, and the time at the grid's left edge.
    pub px_per_sec: f32,
    pub scroll_sec: f32,
    /// What the status strip says (rendering, rendered, errors).
    pub status: String,
}

/// A press inside the VariAudio view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VAct {
    /// The empty grid.
    Grid,
    Seg(usize),
    Button(Btn),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Quantize,
    StraightenMore,
    StraightenLess,
    Reset,
    SelectAll,
    Close,
}

impl Btn {
    const ALL: [Btn; 6] = [Btn::Quantize, Btn::StraightenMore, Btn::StraightenLess, Btn::Reset, Btn::SelectAll, Btn::Close];

    fn label(self) -> &'static str {
        match self {
            Btn::Quantize => "Quantize pitch (Q)",
            Btn::StraightenMore => "Straighten + (S)",
            Btn::StraightenLess => "Straighten -",
            Btn::Reset => "Reset (R)",
            Btn::SelectAll => "Select all",
            Btn::Close => "Close (Esc)",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Btn::Quantize => "Snap the selected segments (all when none is selected) to the nearest semitone",
            Btn::StraightenMore => "Flatten the pitch curve of the selected segments a quarter more (vibrato, scoops)",
            Btn::StraightenLess => "Give the selected segments a quarter of their own pitch curve back",
            Btn::Reset => "Back to the original pitch of the selected segments (all when none is selected)",
            Btn::SelectAll => "Select every segment (Ctrl+A)",
            Btn::Close => "Back to the piano roll; the edited take keeps playing",
        }
    }
}

/// A drag of segments: the mouse's start height and the selection's shifts then.
#[derive(Debug, Clone, PartialEq)]
pub struct VDrag {
    pub y0: f32,
    pub base: Vec<(usize, f32)>,
    pub moved: bool,
}

// ---------------------------------------------------------------------------------------------------------------
// Rendering in a worker

struct Done {
    channel: u16,
    path: Result<String, String>,
}

fn done() -> &'static Mutex<Option<Done>> {
    static D: OnceLock<Mutex<Option<Done>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

static RENDERING: AtomicBool = AtomicBool::new(false);
/// A render was asked for while one ran: run again with the newest edits when it finishes.
static AGAIN: AtomicBool = AtomicBool::new(false);

fn rendered_dir(s: &State) -> std::path::PathBuf {
    s.path.as_ref().and_then(|p| p.parent()).map(|d| d.join("Rendered")).unwrap_or_else(|| std::env::temp_dir().join("Lemon Studio").join("Rendered"))
}

fn sidecar(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{path}.vari.json"))
}

/// What a sidecar holds: the original path, the segments and their edits.
fn read_sidecar(path: &str) -> Option<(String, Vec<Segment>, Vec<Edit>)> {
    let text = std::fs::read_to_string(sidecar(&crate::plugins::resolve_sample(path))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let original = v.get("original")?.as_str()?.to_string();
    let mut segs = Vec::new();
    let mut edits = Vec::new();
    for s in v.get("segments")?.as_array()? {
        let f = |k: &str| s.get(k).and_then(serde_json::Value::as_f64);
        segs.push(Segment { start: f("start")? as usize, end: f("end")? as usize, pitch: f("pitch")? as f32 });
        edits.push(Edit { shift: f("shift").unwrap_or(0.0) as f32, straighten: f("straighten").unwrap_or(0.0) as f32 });
    }
    Some((original, segs, edits))
}

fn write_sidecar(rendered: &std::path::Path, original: &str, segs: &[Segment], edits: &[Edit]) -> std::io::Result<()> {
    let segments: Vec<serde_json::Value> = segs
        .iter()
        .zip(edits)
        .map(|(s, e)| serde_json::json!({ "start": s.start, "end": s.end, "pitch": s.pitch, "shift": e.shift, "straighten": e.straighten }))
        .collect();
    let v = serde_json::json!({ "format": "KNAY VariAudio 1", "original": original, "segments": segments });
    std::fs::write(sidecar(&rendered.to_string_lossy()), serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Renders the session's edits in a worker; `tick` puts the result on the channel. All edits undone: the channel
/// goes back to the original at once.
fn render(app: &mut App) {
    let Some(v) = app.state.piano.vari.as_deref() else { return };
    if v.edits.iter().all(Edit::is_none) {
        let (channel, original) = (v.channel, v.original.clone());
        set_sample(app, channel, original);
        if let Some(v) = app.state.piano.vari.as_deref_mut() {
            v.status = "Original pitch".into();
        }
        return;
    }
    if RENDERING.swap(true, Ordering::AcqRel) {
        AGAIN.store(true, Ordering::Release);
        return;
    }
    let dir = rendered_dir(&app.state);
    let (channel, original, sample, frames, segs, edits) = (v.channel, v.original.clone(), v.sample.clone(), v.frames.clone(), v.segs.clone(), v.edits.clone());
    let stem = std::path::Path::new(&original).file_stem().map_or_else(|| "Audio".into(), |s| s.to_string_lossy().into_owned());
    std::thread::spawn(move || {
        let path = (|| -> Result<String, String> {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mono = dsp::mono(&sample.left, &sample.right);
            let out = dsp::render(&[&sample.left, &sample.right], &mono, sample.rate, &frames, &segs, &edits);
            let interleaved: Vec<f32> = out[0].iter().zip(&out[1]).flat_map(|(l, r)| [*l, *r]).collect();
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let file = dir.join(format!("{stem} (VariAudio {stamp}).wav"));
            let mut w = std::io::BufWriter::new(std::fs::File::create(&file).map_err(|e| e.to_string())?);
            os_engine::wav::write_f32_stereo(&mut w, sample.rate, &interleaved).map_err(|e| e.to_string())?;
            drop(w);
            write_sidecar(&file, &original, &segs, &edits).map_err(|e| e.to_string())?;
            Ok(file.to_string_lossy().into_owned())
        })();
        if let Ok(mut d) = done().lock() {
            *d = Some(Done { channel, path });
        }
    });
    if let Some(v) = app.state.piano.vari.as_deref_mut() {
        v.status = "Rendering...".into();
    }
}

fn set_sample(app: &mut App, channel: u16, path: String) {
    let s = &mut app.state;
    if let Some(c) = s.project.channels.iter_mut().find(|c| c.index == channel) {
        c.sample_path = Some(path);
        s.piano.dirty = true;
    }
    app.invalidate();
}

/// Periodic: a finished render goes on its channel (and a newer one starts when edits came in meanwhile).
pub fn tick(app: &mut App) {
    let Some(d) = done().lock().ok().and_then(|mut d| d.take()) else { return };
    RENDERING.store(false, Ordering::Release);
    match d.path {
        Ok(path) => {
            set_sample(app, d.channel, path);
            if let Some(v) = app.state.piano.vari.as_deref_mut().filter(|v| v.channel == d.channel) {
                v.status = "Edited take playing".into();
            }
        }
        Err(e) => {
            app.state.message = Some(format!("VariAudio: could not render: {e}"));
            if let Some(v) = app.state.piano.vari.as_deref_mut() {
                v.status = format!("Render failed: {e}");
            }
        }
    }
    if AGAIN.swap(false, Ordering::AcqRel) {
        render(app);
    }
    app.invalidate();
}

// ---------------------------------------------------------------------------------------------------------------
// Opening and closing

/// Audio clip `i`'s menu entry.
pub fn menu_item(s: &State, i: usize) -> Option<pl::PItem> {
    let c = pl::clips(s).get(i)?;
    let ClipSource::Channel(ch) = c.source else { return None };
    s.project.channels.iter().find(|c| c.index == ch && c.kind == ChannelKind::AudioClip)?;
    Some(pl::PItem::new("VariAudio (edit pitch)...", pl::Cmd::Vari(i)))
}

/// Opens VariAudio on audio clip `i` in the piano roll.
pub fn open(app: &mut App, i: usize) -> Option<String> {
    let s = &app.state;
    let clip = *pl::clips(s).get(i)?;
    let ClipSource::Channel(ch) = clip.source else { return None };
    let c = s.project.channels.iter().find(|c| c.index == ch && c.kind == ChannelKind::AudioClip)?;
    let path = c.sample_path.clone()?;
    let name = c.name.clone().unwrap_or_else(|| "Audio".into());
    // An edited take goes on from its original and its edits.
    let saved = read_sidecar(&path);
    let original = saved.as_ref().map_or(path.clone(), |s| s.0.clone());
    let sample = match os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(&original))) {
        Ok(s) => s,
        Err(e) => return Some(format!("VariAudio: could not read {original}: {e}")),
    };
    let mono = dsp::mono(&sample.left, &sample.right);
    let frames = dsp::track(&mono, sample.rate);
    let (segs, edits) = match saved {
        Some((_, segs, edits)) if segs.iter().all(|s| s.end <= frames.len()) => (segs, edits),
        _ => {
            let segs = dsp::segments(&frames);
            let n = segs.len();
            (segs, vec![Edit::default(); n])
        }
    };
    if segs.is_empty() {
        return Some(format!("VariAudio: no sung notes found in {name} (it needs a single voice or instrument)"));
    }
    let per = (f64::from(sample.rate) / dsp::FRAME_RATE).max(1.0) as usize;
    let peaks: Vec<f32> = mono.chunks(per).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect();
    let top = peaks.iter().cloned().fold(1e-6f32, f32::max);
    let peaks = peaks.into_iter().map(|p| p / top).collect();
    let secs = sample.left.len() as f64 / f64::from(sample.rate.max(1));
    let grid_w = app.piano_grid.map_or(900.0, |g| g.w);
    let px_per_sec = (grid_w / secs.max(0.5) as f32).clamp(20.0, 400.0);
    let mut keys: Vec<f32> = segs.iter().map(|s| s.pitch).collect();
    keys.sort_by(|a, b| a.total_cmp(b));
    let centre = keys[keys.len() / 2];
    let n = segs.len();
    let s = &mut app.state;
    s.raise(Win::PianoRoll);
    s.piano.harmony.open = false;
    let row = s.piano.row_h;
    let grid_h = app.piano_grid.map_or(500.0, |g| g.h);
    s.piano.scroll_y = ((TOP_KEY as f32 - centre) * row - grid_h / 2.0).max(0.0);
    s.piano.vari = Some(Box::new(Session {
        channel: ch,
        name: name.clone(),
        clip: Some(i),
        original,
        sample,
        frames: Arc::new(frames),
        segs,
        edits,
        sel: Vec::new(),
        peaks,
        secs,
        px_per_sec,
        scroll_sec: 0.0,
        status: "Drag segments up or down; Alt for cents".into(),
    }));
    app.invalidate();
    Some(format!("VariAudio: {name}, {n} segments"))
}

pub fn close(app: &mut App) {
    app.state.piano.vari = None;
    app.drag = None;
    app.invalidate();
}

pub fn is_open(s: &State) -> bool {
    s.piano.vari.is_some()
}

// ---------------------------------------------------------------------------------------------------------------
// Editing

/// The selected segments, or all of them when none is.
fn targets(v: &Session) -> Vec<usize> {
    if v.sel.is_empty() { (0..v.segs.len()).collect() } else { v.sel.clone() }
}

fn button(app: &mut App, b: Btn) {
    let Some(v) = app.state.piano.vari.as_deref_mut() else { return };
    let ts = targets(v);
    let msg = match b {
        Btn::Close => return close(app),
        Btn::SelectAll => {
            v.sel = (0..v.segs.len()).collect();
            app.invalidate();
            return;
        }
        Btn::Quantize => {
            for &i in &ts {
                let target = (v.segs[i].pitch + v.edits[i].shift).round();
                v.edits[i].shift = target - v.segs[i].pitch;
            }
            format!("Quantized {} segment(s) to semitones", ts.len())
        }
        Btn::StraightenMore | Btn::StraightenLess => {
            let d = if b == Btn::StraightenMore { 0.25 } else { -0.25 };
            for &i in &ts {
                v.edits[i].straighten = (v.edits[i].straighten + d).clamp(0.0, 1.0);
            }
            let pct = ts.first().map_or(0.0, |&i| v.edits[i].straighten * 100.0);
            format!("Straighten {pct:.0}% on {} segment(s)", ts.len())
        }
        Btn::Reset => {
            for &i in &ts {
                v.edits[i] = Edit::default();
            }
            format!("Reset {} segment(s)", ts.len())
        }
    };
    v.status = msg;
    render(app);
    app.invalidate();
}

/// A press on the VariAudio view.
pub fn press(app: &mut App, a: VAct) {
    let shift = app.shift;
    let ctrl = app.ctrl;
    let y = app.mouse.1;
    match a {
        VAct::Button(b) => button(app, b),
        VAct::Grid => {
            if let Some(v) = app.state.piano.vari.as_deref_mut() {
                v.sel.clear();
            }
            app.invalidate();
        }
        VAct::Seg(i) => {
            let Some(v) = app.state.piano.vari.as_deref_mut() else { return };
            if shift || ctrl {
                if let Some(p) = v.sel.iter().position(|&j| j == i) {
                    v.sel.remove(p);
                } else {
                    v.sel.push(i);
                }
            } else if !v.sel.contains(&i) {
                v.sel = vec![i];
            }
            let base = v.sel.iter().map(|&j| (j, v.edits[j].shift)).collect();
            app.drag = Some(Drag::Vari(VDrag { y0: y, base, moved: false }));
            app.invalidate();
        }
    }
}

/// The mouse moved while segments are dragged: whole semitones, or cents with Alt.
pub fn drag_move(app: &mut App, d: VDrag) {
    let row = app.state.piano.row_h.max(1.0);
    let alt = app.alt;
    let y = app.mouse.1;
    let Some(v) = app.state.piano.vari.as_deref_mut() else { return };
    let raw = (d.y0 - y) / row;
    let delta = if alt { (raw * 100.0).round() / 100.0 } else { raw.round() };
    for &(i, s0) in &d.base {
        if let Some(e) = v.edits.get_mut(i) {
            e.shift = s0 + delta;
        }
    }
    if delta != 0.0 && !d.moved {
        app.drag = Some(Drag::Vari(VDrag { moved: true, ..d }));
    }
    if let Some(v) = app.state.piano.vari.as_deref_mut() {
        v.status = format!("{}{:.2} semitones", if delta >= 0.0 { "+" } else { "" }, delta);
    }
    app.invalidate();
}

/// The drag ended: the take renders with the moved segments.
pub fn drag_end(app: &mut App, d: VDrag) {
    if d.moved {
        render(app);
    }
}

/// Keys while VariAudio has the piano roll. True: taken.
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let is = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    match key {
        Key::Named(NamedKey::Escape) => close(app),
        Key::Character(c) if ctrl && is(c, "a") => button(app, Btn::SelectAll),
        Key::Character(c) if !ctrl && !alt && is(c, "q") => button(app, Btn::Quantize),
        Key::Character(c) if !ctrl && !alt && is(c, "s") => button(app, Btn::StraightenMore),
        Key::Character(c) if !ctrl && !alt && is(c, "r") => button(app, Btn::Reset),
        Key::Named(k @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
            let Some(v) = app.state.piano.vari.as_deref_mut() else { return true };
            let d = if *k == NamedKey::ArrowUp { 1.0 } else { -1.0 } * if alt { 0.1 } else { 1.0 };
            for i in targets(v) {
                v.edits[i].shift += d;
            }
            render(app);
            app.invalidate();
        }
        _ => return false,
    }
    true
}

/// The wheel over the VariAudio view: rows (as the piano roll), Shift = time, Ctrl = time zoom around the mouse.
pub fn wheel(app: &mut App, dx: f32, dy: f32, shift: bool, ctrl: bool) -> Option<Win> {
    let grid = app.piano_grid?;
    let mx = app.mouse.0;
    let row = app.state.piano.row_h;
    let s = &mut app.state;
    let v = s.piano.vari.as_deref_mut()?;
    if ctrl {
        let at = v.scroll_sec + (mx - grid.x) / v.px_per_sec;
        v.px_per_sec = (v.px_per_sec * 1.15f32.powf(dy + dx)).clamp(10.0, 2000.0);
        v.scroll_sec = (at - (mx - grid.x) / v.px_per_sec).max(0.0);
    } else if shift || dx != 0.0 {
        v.scroll_sec = (v.scroll_sec - (dy + dx) * 120.0 / v.px_per_sec).clamp(0.0, v.secs as f32);
    } else {
        s.piano.scroll_y = (s.piano.scroll_y - dy * row * 3.0).clamp(0.0, (TOP_KEY + 1) as f32 * row - grid.h);
    }
    Some(Win::PianoRoll)
}

// ---------------------------------------------------------------------------------------------------------------
// Painting

/// Paints VariAudio in the piano roll's client rect `r` (under its title bar).
pub fn paint(ui: &mut Ui, s: &State, r: Rect, playing: Option<f64>) {
    let Some(v) = s.piano.vari.as_deref() else { return };
    let t = ui.t;
    let l = crate::views::piano::layout(r, s.piano.split);
    let g = crate::piano::Geom::new(s, l.grid);
    let row = g.row;
    let y_of = |p: f32| g.grid.y + (TOP_KEY as f32 - p) * row - g.scroll_y + row / 2.0;
    let x_of = |secs: f64| g.grid.x + (secs as f32 - v.scroll_sec) * v.px_per_sec;
    let accent = Color::rgb(0xf0a830);
    let seg_fill = Color::rgb(0x3d8fd1);
    let seg_sel = Color::rgb(0xf0a830);

    // Toolbar strip over the ruler: the clip's name, the buttons and the status.
    let bar = Rect::new(l.tab.x, l.tab.y, l.ruler.right() - l.tab.x, l.grid.y - l.tab.y);
    ui.c.fill_rect(bar, t.window_title);
    let font = Font::new(13);
    ui.c.text(Font::new(14), bar.x + 6.0, bar.y + 4.0, accent, "VariAudio");
    ui.c.text_clip(font, bar.x + 6.0, bar.y + 22.0, 140.0, t.text_dim, &v.name);
    let mut x = bar.x + 150.0;
    for b in Btn::ALL {
        let w = ui.c.text_width(font, b.label()) + 14.0;
        let br = Rect::new(x, bar.y + 6.0, w, 22.0);
        let hov = ui.hit(br, Action::Vari(VAct::Button(b)), b.hint());
        ui.c.fill_rrect(br, 4.0, if hov { t.text_faint.with_alpha(90) } else { t.text_faint.with_alpha(40) });
        ui.c.text(font, br.x + 7.0, br.y + 3.0, t.text, b.label());
        x += w + 6.0;
    }
    ui.c.text_clip(font, x + 8.0, bar.y + 10.0, (bar.right() - x - 12.0).max(0.0), t.text_dim, &v.status);

    // Keyboard on the left, as in the piano roll.
    crate::views::piano::keyboard(ui, &l, &g, &[]);

    // Grid: key rows, then second lines.
    ui.c.push_clip(l.grid);
    let (lo, hi) = g.visible_keys();
    for key in lo..=hi {
        let y = g.y_of(key);
        let fill = if crate::views::piano::is_black(key) { t.pr_row_black } else { t.pr_row };
        ui.c.fill_rect(Rect::new(l.grid.x, y, l.grid.w, row - 1.0), fill);
        ui.c.fill_rect(Rect::new(l.grid.x, y + row - 1.0, l.grid.w, 1.0), fill.shade(-0.06));
    }
    let step = if v.px_per_sec > 120.0 { 0.5 } else if v.px_per_sec > 30.0 { 1.0 } else { 5.0 };
    let mut sec = (f64::from(v.scroll_sec) / step).floor() * step;
    while x_of(sec) < l.grid.right() {
        let xx = x_of(sec).round();
        ui.c.fill_rect(Rect::new(xx, l.grid.y, 1.0, l.grid.h), t.text_faint.with_alpha(if (sec % 5.0).abs() < 1e-6 { 70 } else { 30 }));
        ui.c.text(Font::new(10), xx + 3.0, l.grid.y + 2.0, t.text_faint, &format!("{sec:.1}s"));
        sec += step;
    }
    ui.hit(l.grid, Action::Vari(VAct::Grid), "VariAudio: click a segment to select it, drag it up or down to change its pitch (Alt: cents)");

    // Waveform, faint, along the bottom of the grid.
    let wave_h = (l.grid.h * 0.18).min(60.0);
    let base = l.grid.bottom() - 4.0;
    let first = dsp::frame_of(f64::from(v.scroll_sec));
    let last = dsp::frame_of(f64::from(v.scroll_sec + l.grid.w / v.px_per_sec)).min(v.peaks.len());
    let stride = ((last.saturating_sub(first)) / (l.grid.w as usize).max(1)).max(1);
    for f in (first..last).step_by(stride) {
        let h = v.peaks[f] * wave_h;
        ui.c.fill_rect(Rect::new(x_of(dsp::secs_of(f)), base - h, (stride as f32 * v.px_per_sec / dsp::FRAME_RATE as f32).max(1.0), h), t.text_faint.with_alpha(45));
    }

    // Segments: a block at the edited centre pitch, the pitch curve through it.
    for (i, (sg, e)) in v.segs.iter().zip(&v.edits).enumerate() {
        let (x0, x1) = (x_of(dsp::secs_of(sg.start)), x_of(dsp::secs_of(sg.end)));
        if x1 < l.grid.x || x0 > l.grid.right() {
            continue;
        }
        let centre = sg.pitch + e.shift;
        let yc = y_of(centre);
        let block = Rect::new(x0, yc - row / 2.0, (x1 - x0).max(3.0), row);
        let selected = v.sel.contains(&i);
        let col = if selected { seg_sel } else { seg_fill };
        ui.c.fill_rrect(block, 4.0f32.min(row / 2.0), col.with_alpha(if e.is_none() { 110 } else { 170 }));
        ui.c.stroke_rrect(block, 4.0f32.min(row / 2.0), 1.0, col);
        let hint = format!(
            "{}{}{}",
            crate::views::piano::key_name(centre.round() as i32),
            if (centre - centre.round()).abs() >= 0.01 { format!(" {:+.0} cents", (centre - centre.round()) * 100.0) } else { String::new() },
            if e.is_none() { String::new() } else { format!("  (shift {:+.2}, straighten {:.0}%)", e.shift, e.straighten * 100.0) }
        );
        ui.hit(block, Action::Vari(VAct::Seg(i)), hint);
        let pts: Vec<(f32, f32)> = (sg.start..sg.end.min(v.frames.len()))
            .filter(|&f| v.frames[f].voiced())
            .map(|f| (x_of(dsp::secs_of(f)), y_of(dsp::edited_pitch(v.frames[f].pitch, sg, e))))
            .collect();
        if pts.len() > 1 {
            ui.c.polyline(&pts, 1.5, Color::rgb(0xffffff).with_alpha(220));
        }
        if block.w > 30.0 && row >= 12.0 {
            ui.c.text_clip(Font::new(10), block.x + 4.0, block.y + (row - 12.0) / 2.0, block.w - 6.0, Color::rgb(0x101418), &crate::views::piano::key_name(centre.round() as i32));
        }
    }
    if let Some(secs) = playing {
        let xx = x_of(secs);
        ui.c.fill_rect(Rect::new(xx, l.grid.y, 1.5, l.grid.h), t.playhead);
    }
    ui.c.pop_clip();
}

/// Where the clip's take plays now (seconds into the original), when the song is playing over the clip.
pub fn playhead(s: &State, playing: bool, song: bool, tick: f64) -> Option<f64> {
    let v = s.piano.vari.as_deref()?;
    if !playing || !song {
        return None;
    }
    let c = pl::clips(s).get(v.clip?)?;
    if c.source != ClipSource::Channel(v.channel) || tick < f64::from(c.position) || tick >= f64::from(c.position + c.length) {
        return None;
    }
    let p = &s.project;
    let bpm = p.tick_tempo(p.tempo.unwrap_or(140.0).max(1.0));
    Some((tick - f64::from(c.position)) / f64::from(p.ppq.max(1)) * 60.0 / bpm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hz(key: f64) -> f64 {
        440.0 * 2f64.powf((key - 69.0) / 12.0)
    }

    /// A sung line (three notes) on the playlist: VariAudio opens on it in the piano roll, a drag moves a segment two
    /// semitones up, the take renders to a new file the clip's channel then plays, with a sidecar to go on from;
    /// Esc gives the piano roll back.
    #[test]
    fn open_drag_render_and_close() {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.playlist.reset();
        let rate = 44100u32;
        let mut audio = Vec::new();
        for key in [60.0, 64.0, 67.0] {
            let mut phase = 0.0f64;
            for _ in 0..(rate as usize * 6 / 10) {
                phase += 2.0 * std::f64::consts::PI * hz(key) / f64::from(rate);
                let v = (0.3 * (phase.sin() + 0.5 * (2.0 * phase).sin())) as f32;
                audio.push(v);
                audio.push(v);
            }
        }
        let path = std::env::temp_dir().join(format!("knay-vari-{}.wav", std::process::id()));
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), rate, &audio).unwrap();
        let i = pl::add_audio_clip(&mut app.state, &path.to_string_lossy(), 0, 0).expect("audio clip");
        assert!(menu_item(&app.state, i).is_some());
        open(&mut app, i).expect("a message");
        assert!(app.state.shown(Win::PianoRoll));
        let v = app.state.piano.vari.as_deref().expect("VariAudio open");
        let keys: Vec<i32> = v.segs.iter().map(|s| s.pitch.round() as i32).collect();
        assert_eq!(keys, vec![60, 64, 67]);
        let channel = v.channel;
        // Drag the middle segment up two rows.
        let row = app.state.piano.row_h;
        app.mouse = (0.0, 300.0);
        press(&mut app, VAct::Seg(1));
        let Some(Drag::Vari(d)) = app.drag.clone() else { panic!("a drag") };
        app.mouse = (0.0, 300.0 - 2.0 * row - 1.0);
        drag_move(&mut app, d);
        let Some(Drag::Vari(d)) = app.drag.take() else { panic!("a drag") };
        assert_eq!(app.state.piano.vari.as_deref().unwrap().edits[1].shift, 2.0);
        drag_end(&mut app, d);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            tick(&mut app);
            let p = app.state.project.channels.iter().find(|c| c.index == channel).unwrap().sample_path.clone().unwrap();
            if p.contains("VariAudio") {
                let (original, segs, edits) = read_sidecar(&p).expect("a sidecar");
                assert_eq!(original, path.to_string_lossy());
                assert_eq!((segs.len(), edits[1].shift), (3, 2.0));
                let y = os_engine::sample::load(std::path::Path::new(&p)).unwrap();
                let t = dsp::track(&dsp::mono(&y.left, &y.right), y.rate);
                let mut mid: Vec<f32> = t[dsp::frame_of(0.75)..dsp::frame_of(1.05)].iter().filter(|f| f.voiced()).map(|f| f.pitch).collect();
                mid.sort_by(|a, b| a.total_cmp(b));
                assert!((mid[mid.len() / 2] - 66.0).abs() < 0.3, "{}", mid[mid.len() / 2]);
                let _ = std::fs::remove_file(sidecar(&p));
                let _ = std::fs::remove_file(&p);
                break;
            }
            assert!(std::time::Instant::now() < until, "rendered in time");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Esc: the piano roll again.
        app.key(&winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape), false, false);
        assert!(app.state.piano.vari.is_none());
        let _ = std::fs::remove_file(&path);
    }
}
