// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! VariAudio: the piano roll's pitch-editing mode for an audio clip (a sung part), after Cubase's VariAudio, on
//! Giannitune's engine (https://github.com/jonnygucci1/Giannitune, AGPL-3.0): its pitch detector and auto-tune
//! (`vari_dsp`) and Signalsmith Stretch for the pitch and formant changes (`os_dsp::signalsmith`). An
//! audio clip's menu ▸ VariAudio opens its sample in the piano roll: each sung note is a segment (coloured by its
//! note) at its pitch, the pitch curve drawn through it, the waveform behind. A click plays the segment; dragging it
//! moves it by semitones while it plays (Alt: freely, in cents; with Snap pitch editing, onto the editor scale). The
//! inspector on the right sets the selected segments' Correct Pitch, Straighten Curve, Shift Formant and Volume, and
//! holds the Scale Assistant. The piano roll is the ordinary piano roll again once VariAudio closes (Close, Esc).
//!
//! Non-destructive: the original file is never written. Each change renders the edited take in a worker
//! (`vari_dsp::render`, TD-PSOLA) to `<project folder>/Rendered/<name> (VariAudio <time>).wav`, which the clip's
//! channel (its Lemon Sampler) then plays. A sidecar `<rendered>.vari.json` keeps the original's path, the segments,
//! the edits and the scale, so VariAudio on the clip again (also after reopening the project) goes on from there.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use os_model::{ChannelKind, ClipSource};
use os_ui::{Color, Font, Rect};

use crate::app::{Action, App, Drag, State, Ui, Win};
use crate::playlist as pl;
use crate::vari_dsp::{self as dsp, Edit, Frame, Segment};
use crate::views::piano::TOP_KEY;

/// The inspector's width on the right of the grid.
const INSPECTOR_W: f32 = 230.0;
/// Scroll bars along the grid's bottom and right.
const BAR: f32 = 12.0;
const HEAD_H: f32 = 41.0;

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
    /// Time view: pixels per second and the time at the grid's left edge; key rows: height and scroll (px).
    pub px_per_sec: f32,
    pub scroll_sec: f32,
    pub row: f32,
    pub scroll_y: f32,
    /// Scale Assistant: the editor scale (root pitch class, minor), guides on the key rows, snap while dragging.
    pub scale: Option<(u8, bool)>,
    pub guides: bool,
    pub snap: bool,
    /// Giannitune: the voice type (the detector's prior), auto-tune on, its retune speed (0 hard .. 100 slow) and the
    /// curve it tunes the take to (`vari_dsp::autotune`).
    pub voice: dsp::Voice,
    pub autotune: bool,
    pub retune: f32,
    pub tune: Option<Arc<Vec<f32>>>,
    /// What the status line says (rendering, rendered, errors).
    pub status: String,
}

impl Session {
    /// The editor scale's pitch classes; empty (every semitone) without one.
    pub fn scale_notes(&self) -> Vec<u8> {
        self.scale.map_or_else(Vec::new, |(r, m)| dsp::scale_notes(r, m))
    }

    /// The auto-tune curve while auto-tune is on.
    pub fn base(&self) -> Option<&[f32]> {
        self.tune.as_deref().map(Vec::as_slice).filter(|_| self.autotune)
    }

    /// Recomputes the auto-tune curve (scale, retune speed or the take changed).
    fn retune_curve(&mut self) {
        self.tune = Some(Arc::new(dsp::autotune(&self.frames, &self.scale_notes(), self.retune)));
    }

    /// The sung or auto-tuned pitch of frame `f`.
    pub fn pitch_at(&self, f: usize) -> f32 {
        let p = self.frames.get(f).map_or(0.0, |fr| fr.pitch);
        self.base().and_then(|b| b.get(f).copied()).filter(|b| *b > 0.0 && p > 0.0).unwrap_or(p)
    }

    /// Segment `i`'s centre as it sounds (its edit, on top of auto-tune).
    pub fn seg_centre(&self, i: usize) -> f32 {
        let (sg, e) = (&self.segs[i], &self.edits[i]);
        let c = dsp::centre(sg, e, &self.scale_notes());
        if self.base().is_none() {
            return c;
        }
        let mut v: Vec<f32> = (sg.start..sg.end).map(|f| self.pitch_at(f)).filter(|p| *p > 0.0).collect();
        if v.is_empty() {
            return c;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        c + v[v.len() / 2] - sg.pitch
    }

    /// The selected segments, or all of them when none is.
    fn targets(&self) -> Vec<usize> {
        if self.sel.is_empty() { (0..self.segs.len()).collect() } else { self.sel.clone() }
    }
}

/// The inspector's sliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slider {
    Correct,
    Straighten,
    Formant,
    Volume,
    /// Giannitune's retune speed (the session's, not a segment's).
    Retune,
}

impl Slider {
    const ALL: [Slider; 4] = [Slider::Correct, Slider::Straighten, Slider::Formant, Slider::Volume];

    fn label(self) -> &'static str {
        match self {
            Slider::Correct => "Correct Pitch",
            Slider::Straighten => "Straighten Curve",
            Slider::Formant => "Shift Formant",
            Slider::Volume => "Volume",
            Slider::Retune => "Retune Speed",
        }
    }

    fn range(self) -> (f32, f32) {
        match self {
            Slider::Correct | Slider::Straighten => (0.0, 1.0),
            Slider::Formant => (-12.0, 12.0),
            Slider::Volume => (-24.0, 12.0),
            Slider::Retune => (0.0, 100.0),
        }
    }

    fn get(self, e: &Edit) -> f32 {
        match self {
            Slider::Correct => e.correct,
            Slider::Straighten => e.straighten,
            Slider::Formant => e.formant,
            Slider::Volume => e.gain_db,
            Slider::Retune => 0.0,
        }
    }

    fn set(self, e: &mut Edit, v: f32) {
        let (lo, hi) = self.range();
        let v = v.clamp(lo, hi);
        match self {
            Slider::Correct => e.correct = v,
            Slider::Straighten => e.straighten = v,
            Slider::Formant => e.formant = (v * 10.0).round() / 10.0,
            Slider::Volume => e.gain_db = (v * 10.0).round() / 10.0,
            Slider::Retune => {}
        }
    }

    fn text(self, v: f32) -> String {
        match self {
            Slider::Correct | Slider::Straighten => format!("{:.0} %", v * 100.0),
            Slider::Formant => format!("{v:+.1} st"),
            Slider::Volume => format!("{v:+.1} dB"),
            Slider::Retune => format!("{v:.0}"),
        }
    }
}

/// A press inside the VariAudio view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VAct {
    /// The empty grid.
    Grid,
    Seg(usize),
    Button(Btn),
    Slider(Slider),
    /// The time and row scroll bars.
    HBar,
    VBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    ZoomOutH,
    ZoomInH,
    ZoomOutV,
    ZoomInV,
    Fit,
    QuantizePitches,
    Reset,
    SelectAll,
    /// Scale Assistant: the root (Off, C .. B), major / minor, guides, snap, detect from the take.
    ScaleRoot,
    ScaleMode,
    Guides,
    Snap,
    DetectScale,
    /// KNAY: the piano roll's key and scale (♪ menu) as the editor scale, snapping on.
    PianoKey,
    /// Giannitune: the next voice type (analyses the take again), auto-tune on / off.
    Voice,
    AutoTune,
    Close,
}

/// A drag in the VariAudio view.
#[derive(Debug, Clone)]
pub enum VDrag {
    /// Segments up or down: the mouse's start height, the segments' shifts then, the delta last played.
    Segs { y0: f32, base: Vec<(usize, f32)>, last: f32, moved: bool },
    /// An inspector slider: the mouse's start x, the slider's width and value then.
    Slider { which: Slider, x0: f32, w: f32, v0: f32 },
    HBar { x0: f32, s0: f32 },
    VBar { y0: f32, s0: f32 },
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

/// What a sidecar holds.
struct Saved {
    original: String,
    segs: Vec<Segment>,
    edits: Vec<Edit>,
    scale: Option<(u8, bool)>,
    voice: dsp::Voice,
    autotune: bool,
    retune: f32,
}

fn read_sidecar(path: &str) -> Option<Saved> {
    let text = std::fs::read_to_string(sidecar(&crate::plugins::resolve_sample(path))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let original = v.get("original")?.as_str()?.to_string();
    let mut segs = Vec::new();
    let mut edits = Vec::new();
    for s in v.get("segments")?.as_array()? {
        let f = |k: &str| s.get(k).and_then(serde_json::Value::as_f64);
        segs.push(Segment { start: f("start")? as usize, end: f("end")? as usize, pitch: f("pitch")? as f32 });
        let g = |k: &str| f(k).unwrap_or(0.0) as f32;
        edits.push(Edit { shift: g("shift"), correct: g("correct"), straighten: g("straighten"), formant: g("formant"), gain_db: g("gain_db") });
    }
    let scale = v.get("scale_root").and_then(serde_json::Value::as_u64).map(|r| ((r % 12) as u8, v.get("scale_minor").and_then(serde_json::Value::as_bool).unwrap_or(false)));
    let voice = dsp::Voice::ALL.into_iter().find(|x| v.get("voice").and_then(serde_json::Value::as_str) == Some(x.name())).unwrap_or_default();
    let autotune = v.get("autotune").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let retune = v.get("retune").and_then(serde_json::Value::as_f64).unwrap_or(20.0) as f32;
    Some(Saved { original, segs, edits, scale, voice, autotune, retune })
}

#[allow(clippy::too_many_arguments)]
fn write_sidecar(rendered: &std::path::Path, original: &str, segs: &[Segment], edits: &[Edit], scale: Option<(u8, bool)>, voice: dsp::Voice, autotune: bool, retune: f32) -> std::io::Result<()> {
    let segments: Vec<serde_json::Value> = segs
        .iter()
        .zip(edits)
        .map(|(s, e)| {
            serde_json::json!({ "start": s.start, "end": s.end, "pitch": s.pitch, "shift": e.shift, "correct": e.correct,
                "straighten": e.straighten, "formant": e.formant, "gain_db": e.gain_db })
        })
        .collect();
    let mut v = serde_json::json!({ "format": "KNAY VariAudio 3 (Giannitune)", "original": original, "segments": segments,
        "voice": voice.name(), "autotune": autotune, "retune": retune });
    if let Some((root, minor)) = scale {
        v["scale_root"] = serde_json::json!(root);
        v["scale_minor"] = serde_json::json!(minor);
    }
    std::fs::write(sidecar(&rendered.to_string_lossy()), serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Renders the session's edits in a worker; `tick` puts the result on the channel. All edits undone: the channel
/// goes back to the original at once.
fn render(app: &mut App) {
    let Some(v) = app.state.piano.vari.as_deref() else { return };
    if v.edits.iter().all(Edit::is_none) && v.base().is_none() {
        let (channel, original) = (v.channel, v.original.clone());
        set_sample(app, channel, original);
        status(app, "Original take playing");
        return;
    }
    if RENDERING.swap(true, Ordering::AcqRel) {
        AGAIN.store(true, Ordering::Release);
        return;
    }
    let dir = rendered_dir(&app.state);
    let scale_notes = v.scale_notes();
    let (channel, original, sample, frames, segs, edits, scale) = (v.channel, v.original.clone(), v.sample.clone(), v.frames.clone(), v.segs.clone(), v.edits.clone(), v.scale);
    let (voice, autotune, retune, base) = (v.voice, v.autotune, v.retune, v.base().map(<[f32]>::to_vec));
    let stem = std::path::Path::new(&original).file_stem().map_or_else(|| "Audio".into(), |s| s.to_string_lossy().into_owned());
    std::thread::spawn(move || {
        let path = (|| -> Result<String, String> {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mono = dsp::mono(&sample.left, &sample.right);
            let out = dsp::render(&[&sample.left, &sample.right], &mono, sample.rate, &frames, &segs, &edits, &scale_notes, base.as_deref(), dsp::Shifter::External(&os_dsp::signalsmith::render));
            let interleaved: Vec<f32> = out[0].iter().zip(&out[1]).flat_map(|(l, r)| [*l, *r]).collect();
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let file = dir.join(format!("{stem} (VariAudio {stamp}).wav"));
            let mut w = std::io::BufWriter::new(std::fs::File::create(&file).map_err(|e| e.to_string())?);
            os_engine::wav::write_f32_stereo(&mut w, sample.rate, &interleaved).map_err(|e| e.to_string())?;
            drop(w);
            write_sidecar(&file, &original, &segs, &edits, scale, voice, autotune, retune).map_err(|e| e.to_string())?;
            Ok(file.to_string_lossy().into_owned())
        })();
        if let Ok(mut d) = done().lock() {
            *d = Some(Done { channel, path });
        }
    });
    status(app, "Rendering...");
}

fn status(app: &mut App, text: &str) {
    if let Some(v) = app.state.piano.vari.as_deref_mut() {
        v.status = text.into();
    }
    app.invalidate();
}

/// Channel `channel` plays `path`: the project's channel, and its Lemon Sampler (which plays audio clips), which
/// loads the file (`sample_loads`); playback takes it at once.
fn set_sample(app: &mut App, channel: u16, path: String) {
    let s = &mut app.state;
    if let Some(row) = s.project.channels.iter().position(|c| c.index == channel) {
        s.project.channels[row].sample_path = Some(path.clone());
        s.sample_loads.push((row, path));
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
            if app.state.piano.vari.as_deref().is_some_and(|v| v.channel == d.channel) {
                status(app, "Edited take playing");
            }
        }
        Err(e) => {
            app.state.message = Some(format!("VariAudio: could not render: {e}"));
            status(app, &format!("Render failed: {e}"));
        }
    }
    if AGAIN.swap(false, Ordering::AcqRel) {
        render(app);
    }
    app.invalidate();
}

// ---------------------------------------------------------------------------------------------------------------
// Audition

static AUDITION: AtomicUsize = AtomicUsize::new(0);

/// Plays segment `i` as edited now, once, on the preview stream (a click, and each step of a drag).
fn audition(app: &mut App, i: usize) {
    let Some(v) = app.state.piano.vari.as_deref() else { return };
    let scale = v.scale_notes();
    let (l, r) = dsp::render_segment(&v.sample.left, &v.sample.right, v.sample.rate, &v.frames, &v.segs, &v.edits, i, &scale, v.base(), dsp::Shifter::External(&os_dsp::signalsmith::render));
    if l.is_empty() {
        return;
    }
    let n = AUDITION.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join("Lemon Studio").join("VariAudio audition");
    let _ = std::fs::create_dir_all(&dir);
    // A new name each time: the sample cache knows a file by its path.
    let file = dir.join(format!("segment {}-{n}.wav", std::process::id()));
    let _ = std::fs::remove_file(dir.join(format!("segment {}-{}.wav", std::process::id(), n.wrapping_sub(2))));
    let interleaved: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
    let ok = std::fs::File::create(&file).and_then(|f| {
        let mut w = std::io::BufWriter::new(f);
        os_engine::wav::write_f32_stereo(&mut w, v.sample.rate, &interleaved)
    });
    if ok.is_ok() {
        let mut p = crate::app::preview_project(&file.to_string_lossy());
        p.tempo = Some(app.state.tempo);
        app.transport.preview_for(p, None);
    }
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

/// The view's areas in the piano roll's client rect `r`: head strip, keyboard, grid, scroll bars, inspector.
struct Areas {
    head: Rect,
    keys: Rect,
    grid: Rect,
    hbar: Rect,
    vbar: Rect,
    inspector: Rect,
}

fn areas(r: Rect) -> Areas {
    let top = r.y + 23.0;
    let body = top + HEAD_H;
    let right = r.right() - INSPECTOR_W;
    let grid = Rect::new(r.x + 73.0, body, (right - BAR - 1.0 - (r.x + 73.0)).max(50.0), (r.bottom() - BAR - body).max(40.0));
    Areas {
        head: Rect::new(r.x, top, r.w, HEAD_H),
        keys: Rect::new(r.x, body, 71.0, grid.h),
        hbar: Rect::new(grid.x, grid.bottom(), grid.w, BAR),
        vbar: Rect::new(grid.right(), grid.y, BAR, grid.h),
        inspector: Rect::new(right, top + HEAD_H, INSPECTOR_W, r.bottom() - top - HEAD_H),
        grid,
    }
}

/// The grid VariAudio paints now (the piano roll window's client rect, as `views::piano` lays it out).
fn grid_now(app: &App) -> Option<Rect> {
    let s = &app.state;
    let (r, _) = crate::views::frame_rects(s, Win::PianoRoll, s.win_rect(Win::PianoRoll));
    Some(areas(r).grid)
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
    let original = saved.as_ref().map_or(path.clone(), |s| s.original.clone());
    let sample = match os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(&original))) {
        Ok(s) => s,
        Err(e) => return Some(format!("VariAudio: could not read {original}: {e}")),
    };
    let mono = dsp::mono(&sample.left, &sample.right);
    let voice = saved.as_ref().map_or_else(dsp::Voice::default, |s| s.voice);
    let frames = dsp::track_voice(&mono, sample.rate, voice);
    let (autotune, retune) = saved.as_ref().map_or((false, 20.0), |s| (s.autotune, s.retune));
    let (segs, edits, scale) = match saved {
        Some(sv) if sv.segs.iter().all(|s| s.end <= frames.len()) && sv.segs.len() == sv.edits.len() => (sv.segs, sv.edits, sv.scale),
        _ => {
            let segs = dsp::segments(&frames);
            let n = segs.len();
            (segs, vec![Edit::default(); n], None)
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
    app.state.raise(Win::PianoRoll);
    app.state.piano.harmony.open = false;
    // KNAY: with no scale of its own yet, the take follows the piano roll's key and scale (the ♪ menu) when one is lit.
    let scale = scale.or_else(|| piano_key(&app.state.piano.harmony));
    let grid = grid_now(app).unwrap_or(Rect::new(0.0, 0.0, 900.0, 500.0));
    // About twelve seconds across, from just before the first segment; rows tall enough to read.
    let px_per_sec = (grid.w / (secs.min(12.0).max(1.0)) as f32).clamp(20.0, 600.0);
    let scroll_sec = (dsp::secs_of(segs[0].start) as f32 - 0.5).max(0.0);
    let mut keys: Vec<f32> = segs.iter().map(|s| s.pitch).collect();
    keys.sort_by(|a, b| a.total_cmp(b));
    let centre = keys[keys.len() / 2];
    let row = 18.0;
    let scroll_y = ((TOP_KEY as f32 - centre) * row - grid.h / 2.0).max(0.0);
    let n = segs.len();
    app.state.piano.vari = Some(Box::new(Session {
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
        scroll_sec,
        row,
        scroll_y,
        scale,
        guides: scale.is_some(),
        snap: scale.is_some(),
        voice,
        autotune,
        retune,
        tune: None,
        status: "Click a segment to hear it; drag it up or down (Alt: cents)".into(),
    }));
    if let Some(v) = app.state.piano.vari.as_deref_mut() {
        v.retune_curve();
    }
    app.invalidate();
    Some(format!("VariAudio: {name}, {n} segments"))
}

pub fn close(app: &mut App) {
    app.transport.stop_preview();
    app.state.piano.vari = None;
    app.drag = None;
    app.invalidate();
}

// ---------------------------------------------------------------------------------------------------------------
// Editing

fn zoom_h(v: &mut Session, k: f32, at_x: f32, grid: Rect) {
    let at = v.scroll_sec + (at_x - grid.x) / v.px_per_sec;
    v.px_per_sec = (v.px_per_sec * k).clamp(8.0, 3000.0);
    v.scroll_sec = (at - (at_x - grid.x) / v.px_per_sec).clamp(0.0, v.secs as f32);
}

fn zoom_v(v: &mut Session, k: f32, at_y: f32, grid: Rect) {
    let key = TOP_KEY as f32 - (at_y - grid.y + v.scroll_y) / v.row;
    v.row = (v.row * k).clamp(8.0, 48.0);
    v.scroll_y = ((TOP_KEY as f32 - key) * v.row - (at_y - grid.y)).clamp(0.0, ((TOP_KEY + 1) as f32 * v.row - grid.h).max(0.0));
}

fn button(app: &mut App, b: Btn) {
    let grid = grid_now(app).unwrap_or(Rect::new(0.0, 0.0, 900.0, 500.0));
    let Some(v) = app.state.piano.vari.as_deref_mut() else { return };
    let ts = v.targets();
    let mut rerender = false;
    let msg: String = match b {
        Btn::Close => return close(app),
        Btn::ZoomInH | Btn::ZoomOutH => {
            zoom_h(v, if b == Btn::ZoomInH { 1.5 } else { 1.0 / 1.5 }, grid.x + grid.w / 2.0, grid);
            return app.invalidate();
        }
        Btn::ZoomInV | Btn::ZoomOutV => {
            zoom_v(v, if b == Btn::ZoomInV { 1.25 } else { 0.8 }, grid.y + grid.h / 2.0, grid);
            return app.invalidate();
        }
        Btn::Fit => {
            v.px_per_sec = (grid.w / v.secs.max(1.0) as f32).clamp(8.0, 3000.0);
            v.scroll_sec = 0.0;
            return app.invalidate();
        }
        Btn::SelectAll => {
            v.sel = (0..v.segs.len()).collect();
            format!("{} segments selected", v.sel.len())
        }
        Btn::QuantizePitches => {
            for &i in &ts {
                v.edits[i].correct = 1.0;
            }
            rerender = true;
            format!("Quantized {} segment(s) to {}", ts.len(), if v.scale.is_some() { "the editor scale" } else { "semitones" })
        }
        Btn::Reset => {
            for &i in &ts {
                v.edits[i] = Edit::default();
            }
            rerender = true;
            format!("Reset {} segment(s)", ts.len())
        }
        Btn::Voice => {
            let i = dsp::Voice::ALL.iter().position(|x| *x == v.voice).unwrap_or(0);
            v.voice = dsp::Voice::ALL[(i + 1) % dsp::Voice::ALL.len()];
            let mono = dsp::mono(&v.sample.left, &v.sample.right);
            v.frames = Arc::new(dsp::track_voice(&mono, v.sample.rate, v.voice));
            v.segs = dsp::segments(&v.frames);
            v.edits = vec![Edit::default(); v.segs.len()];
            v.sel.clear();
            v.retune_curve();
            rerender = true;
            format!("Voice type: {} (the take was analysed again; segment edits start over)", v.voice.name())
        }
        Btn::AutoTune => {
            v.autotune = !v.autotune;
            v.retune_curve();
            rerender = true;
            if v.autotune {
                format!("Giannitune auto-tune on: {} at retune speed {:.0}", scale_name(v.scale).trim_start_matches("Editor scale: "), v.retune)
            } else {
                "Giannitune auto-tune off".into()
            }
        }
        Btn::ScaleRoot => {
            v.scale = match v.scale {
                None => Some((0, false)),
                Some((11, _)) => None,
                Some((r, m)) => Some((r + 1, m)),
            };
            v.guides = v.scale.is_some() && (v.guides || v.scale == Some((0, false)));
            v.retune_curve();
            rerender = v.autotune || v.edits.iter().any(|e| e.correct > 0.0);
            scale_name(v.scale)
        }
        Btn::ScaleMode => {
            v.scale = Some(v.scale.map_or((0, true), |(r, m)| (r, !m)));
            v.retune_curve();
            rerender = v.autotune || v.edits.iter().any(|e| e.correct > 0.0);
            scale_name(v.scale)
        }
        Btn::Guides => {
            v.guides = !v.guides;
            return app.invalidate();
        }
        Btn::Snap => {
            v.snap = !v.snap;
            (if v.snap { "Snap pitch editing on: segments land on the editor scale" } else { "Snap pitch editing off" }).to_string()
        }
        Btn::PianoKey => {
            let h = &app.state.piano.harmony;
            let (root, scale) = (h.root, h.scale);
            match piano_key(h) {
                Some(k) => {
                    v.scale = Some(k);
                    v.guides = true;
                    v.snap = true;
                    v.retune_curve();
                    rerender = v.autotune || v.edits.iter().any(|e| e.correct > 0.0);
                    format!("Piano roll key: {} {} — snap to scale on", crate::harmony::ROOTS[usize::from(root % 12)], crate::chords::label(scale))
                }
                None => "Pick a root note and scale in the piano roll's ♪ menu first".into(),
            }
        }
        Btn::DetectScale => {
            let c = crate::key_detect::chroma(&v.sample.left, &v.sample.right, v.sample.rate);
            match crate::key_detect::key_of(&c) {
                Some(d) => {
                    v.scale = Some((d.key.root, d.key.minor));
                    v.guides = true;
                    v.retune_curve();
                    rerender = v.autotune || v.edits.iter().any(|e| e.correct > 0.0);
                    format!("Detected {}{}", d.key.name(), if d.confident() { "" } else { " (not certain)" })
                }
                None => "No key found in this take".into(),
            }
        }
    };
    v.status = msg;
    if rerender {
        render(app);
    }
    app.invalidate();
}

/// The piano roll's key (♪ menu) as VariAudio's editor scale (root, minor): minor when the scale has a minor third.
fn piano_key(h: &crate::harmony::Harmony) -> Option<(u8, bool)> {
    if !h.shade && !h.edit_snap {
        return None;
    }
    let pcs = crate::chords::classes(h.scale, h.root);
    let minor = pcs[usize::from((h.root + 3) % 12)] && !pcs[usize::from((h.root + 4) % 12)];
    Some((h.root % 12, minor))
}

fn scale_name(s: Option<(u8, bool)>) -> String {
    match s {
        None => "Editor scale: off (chromatic)".into(),
        Some((r, m)) => format!("Editor scale: {} {}", crate::key_detect::NAMES[usize::from(r % 12)], if m { "minor" } else { "major" }),
    }
}

/// A press on the VariAudio view.
pub fn press(app: &mut App, a: VAct) {
    let (shift, ctrl) = (app.shift, app.ctrl);
    let (x, y) = app.mouse;
    match a {
        VAct::Button(b) => button(app, b),
        VAct::Grid => {
            if let Some(v) = app.state.piano.vari.as_deref_mut() {
                v.sel.clear();
                v.status = "Nothing selected: the inspector changes every segment".into();
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
            app.drag = Some(Drag::Vari(VDrag::Segs { y0: y, base, last: 0.0, moved: false }));
            audition(app, i);
            app.invalidate();
        }
        VAct::Slider(which) => {
            let Some(v) = app.state.piano.vari.as_deref() else { return };
            let v0 = if which == Slider::Retune { v.retune } else { v.targets().first().map_or(0.0, |&i| which.get(&v.edits[i])) };
            let w = INSPECTOR_W - 24.0;
            app.drag = Some(Drag::Vari(VDrag::Slider { which, x0: x, w, v0 }));
        }
        VAct::HBar => {
            let Some(v) = app.state.piano.vari.as_deref() else { return };
            app.drag = Some(Drag::Vari(VDrag::HBar { x0: x, s0: v.scroll_sec }));
        }
        VAct::VBar => {
            let Some(v) = app.state.piano.vari.as_deref() else { return };
            app.drag = Some(Drag::Vari(VDrag::VBar { y0: y, s0: v.scroll_y }));
        }
    }
}

/// The mouse moved during a VariAudio drag.
pub fn drag_move(app: &mut App, d: VDrag) {
    let (x, y) = app.mouse;
    let alt = app.alt;
    let grid = grid_now(app).unwrap_or(Rect::new(0.0, 0.0, 900.0, 500.0));
    let Some(v) = app.state.piano.vari.as_deref_mut() else { return };
    match d {
        VDrag::Segs { y0, base, last, .. } => {
            let raw = (y0 - y) / v.row.max(1.0);
            let delta = if alt { (raw * 100.0).round() / 100.0 } else { raw.round() };
            let scale = v.scale_notes();
            for &(i, s0) in &base {
                let (Some(sg), Some(e)) = (v.segs.get(i), v.edits.get_mut(i)) else { continue };
                let mut shift = s0 + delta;
                if v.snap && !alt && !scale.is_empty() && delta != 0.0 {
                    // The row the mouse is on, or the next note of the scale beyond it in the drag's direction.
                    let dir = if delta > 0.0 { 1 } else { -1 };
                    let mut k = (sg.pitch + shift).round() as i32;
                    while !scale.contains(&(k.rem_euclid(12) as u8)) {
                        k += dir;
                    }
                    shift = k as f32 - sg.pitch;
                }
                e.shift = shift;
            }
            v.status = format!("{}{:.2} semitones", if delta >= 0.0 { "+" } else { "" }, delta);
            let first = base.first().map(|b| b.0);
            app.drag = Some(Drag::Vari(VDrag::Segs { y0, base, last: delta, moved: delta != 0.0 || last != 0.0 }));
            // Each new pitch plays at once.
            if (delta - last).abs() >= 0.5 || (alt && (delta - last).abs() >= 0.1) {
                if let Some(i) = first {
                    audition(app, i);
                }
            } else if let Some(Drag::Vari(VDrag::Segs { last: l, .. })) = app.drag.as_mut() {
                *l = last;
            }
        }
        VDrag::Slider { which, x0, w, v0 } => {
            let (lo, hi) = which.range();
            let val = v0 + (x - x0) / w.max(1.0) * (hi - lo);
            if which == Slider::Retune {
                v.retune = val.clamp(lo, hi).round();
                v.retune_curve();
            } else {
                for i in v.targets() {
                    which.set(&mut v.edits[i], val);
                }
            }
            v.status = format!("{}: {}", which.label(), which.text(val.clamp(lo, hi)));
        }
        VDrag::HBar { x0, s0 } => {
            let per = v.secs as f32 / grid.w.max(1.0);
            v.scroll_sec = (s0 + (x - x0) * per).clamp(0.0, (v.secs as f32 - grid.w / v.px_per_sec).max(0.0));
        }
        VDrag::VBar { y0, s0 } => {
            let total = (TOP_KEY + 1) as f32 * v.row;
            v.scroll_y = (s0 + (y - y0) * total / grid.h.max(1.0)).clamp(0.0, (total - grid.h).max(0.0));
        }
    }
    app.invalidate();
}

/// A VariAudio drag ended: moved segments and slider changes render; the audition stops.
pub fn drag_end(app: &mut App, d: VDrag) {
    match d {
        VDrag::Segs { moved, .. } => {
            app.transport.stop_preview();
            if moved {
                render(app);
            }
        }
        VDrag::Slider { .. } => render(app),
        VDrag::HBar { .. } | VDrag::VBar { .. } => {}
    }
}

/// Keys while VariAudio has the piano roll. True: taken.
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let is = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    match key {
        Key::Named(NamedKey::Escape) => close(app),
        Key::Character(c) if ctrl && is(c, "a") => button(app, Btn::SelectAll),
        Key::Character(c) if !ctrl && !alt && is(c, "q") => button(app, Btn::QuantizePitches),
        Key::Character(c) if !ctrl && !alt && is(c, "r") => button(app, Btn::Reset),
        Key::Character(c) if !ctrl && !alt && is(c, "f") => button(app, Btn::Fit),
        Key::Character(c) if !ctrl && !alt && is(c, "s") => {
            let Some(v) = app.state.piano.vari.as_deref_mut() else { return true };
            for i in v.targets() {
                v.edits[i].straighten = (v.edits[i].straighten + 0.25).min(1.0);
            }
            v.status = "Straighten Curve +25%".into();
            render(app);
        }
        Key::Named(k @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
            let Some(v) = app.state.piano.vari.as_deref_mut() else { return true };
            let d = if *k == NamedKey::ArrowUp { 1.0 } else { -1.0 } * if alt { 0.1 } else { 1.0 };
            let ts = v.targets();
            for &i in &ts {
                v.edits[i].shift += d;
            }
            if let Some(&i) = ts.first().filter(|_| ts.len() < v.segs.len()) {
                audition(app, i);
            }
            render(app);
        }
        _ => return false,
    }
    app.invalidate();
    true
}

/// The wheel over the VariAudio view: rows; Shift = time; Ctrl = time zoom, Alt = row zoom (around the mouse).
pub fn wheel(app: &mut App, dx: f32, dy: f32, shift: bool, ctrl: bool) -> Option<Win> {
    let grid = grid_now(app)?;
    let (mx, my) = app.mouse;
    let alt = app.alt;
    let v = app.state.piano.vari.as_deref_mut()?;
    let notches = dy + dx;
    if ctrl {
        zoom_h(v, 1.2f32.powf(notches), mx, grid);
    } else if alt {
        zoom_v(v, 1.12f32.powf(notches), my, grid);
    } else if shift || dx != 0.0 {
        v.scroll_sec = (v.scroll_sec - notches * 120.0 / v.px_per_sec).clamp(0.0, v.secs as f32);
    } else {
        v.scroll_y = (v.scroll_y - dy * v.row * 3.0).clamp(0.0, ((TOP_KEY + 1) as f32 * v.row - grid.h).max(0.0));
    }
    Some(Win::PianoRoll)
}

// ---------------------------------------------------------------------------------------------------------------
// Painting

/// A colour per note, as Cubase gives each pitch its own.
fn note_colour(pitch: f32) -> Color {
    const C: [u32; 12] = [0xd9534f, 0xe8743b, 0xe8a33b, 0xd9c23b, 0x8fc23b, 0x4fb35c, 0x3bb3a0, 0x3b9fd9, 0x4f74d9, 0x7a5cd9, 0xb35cd9, 0xd95c9f];
    Color::rgb(C[(pitch.round() as i32).rem_euclid(12) as usize])
}

fn small_button(ui: &mut Ui, r: Rect, label: &str, on: bool, act: Btn, hint: &'static str) {
    let t = ui.t;
    let hov = ui.hit(r, Action::Vari(VAct::Button(act)), hint);
    let fill = if on { Color::rgb(0xf0a830).with_alpha(200) } else if hov { t.text_faint.with_alpha(90) } else { t.text_faint.with_alpha(40) };
    ui.c.fill_rrect(r, 4.0, fill);
    let font = Font::new(12);
    let w = ui.c.text_width(font, label);
    ui.c.text(font, r.x + (r.w - w) / 2.0, r.y + (r.h - 15.0) / 2.0, if on { Color::rgb(0x15181b) } else { t.text }, label);
}

/// Paints VariAudio in the piano roll's client rect `r` (under its title bar); `playing`: the take's playhead.
pub fn paint(ui: &mut Ui, s: &State, r: Rect, playing: Option<f64>) {
    let Some(v) = s.piano.vari.as_deref() else { return };
    let t = ui.t;
    let a = areas(r);
    let mut g = crate::piano::Geom::new(s, a.grid);
    g.row = v.row;
    g.scroll_y = v.scroll_y;
    let row = v.row;
    let y_of = |p: f32| a.grid.y + (TOP_KEY as f32 - p) * row - v.scroll_y + row / 2.0;
    let x_of = |secs: f64| a.grid.x + (secs as f32 - v.scroll_sec) * v.px_per_sec;
    let accent = Color::rgb(0xf0a830);
    let scale = v.scale_notes();

    // Head: the name, the zoom buttons, the status.
    ui.c.fill_rect(a.head, t.window_title);
    ui.c.text(Font::new(14), a.head.x + 6.0, a.head.y + 3.0, accent, "VariAudio");
    ui.c.text_clip(Font::new(12), a.head.x + 6.0, a.head.y + 21.0, 140.0, t.text_dim, &v.name);
    let mut x = a.head.x + 150.0;
    for (label, b, hint) in [
        ("H -", Btn::ZoomOutH, "Zoom out in time (Ctrl+wheel)"),
        ("H +", Btn::ZoomInH, "Zoom in in time (Ctrl+wheel)"),
        ("V -", Btn::ZoomOutV, "Smaller rows (Alt+wheel)"),
        ("V +", Btn::ZoomInV, "Taller rows (Alt+wheel)"),
        ("Fit", Btn::Fit, "The whole take across the window (F)"),
    ] {
        small_button(ui, Rect::new(x, a.head.y + 9.0, 40.0, 22.0), label, false, b, hint);
        x += 44.0;
    }
    ui.c.text_clip(Font::new(12), x + 10.0, a.head.y + 12.0, (a.inspector.x - x - 16.0).max(0.0), t.text_dim, &v.status);

    // Keyboard, as in the piano roll.
    let l = crate::views::piano::Layout {
        tab: Rect::new(r.x, a.head.y, 71.0, HEAD_H),
        overview: a.head,
        ruler: a.head,
        keys: a.keys,
        grid: a.grid,
        vscroll: a.vbar,
        splitter: Rect::new(r.x, r.bottom(), r.w, 0.0),
        label: Rect::new(r.x, r.bottom(), 0.0, 0.0),
        panel: Rect::new(a.grid.x, r.bottom(), a.grid.w, 0.0),
    };
    crate::views::piano::keyboard(ui, &l, &g, &[]);

    // Grid: key rows (notes out of the editor scale darker with the guides on), second lines and times.
    ui.c.push_clip(a.grid);
    let (lo, hi) = g.visible_keys();
    for key in lo..=hi {
        let y = g.y_of(key);
        let out_of_scale = v.guides && !scale.is_empty() && !scale.contains(&(key.rem_euclid(12) as u8));
        let base = if crate::views::piano::is_black(key) { t.pr_row_black } else { t.pr_row };
        let fill = if out_of_scale { base.shade(-0.25) } else { base };
        ui.c.fill_rect(Rect::new(a.grid.x, y, a.grid.w, row - 1.0), fill);
        ui.c.fill_rect(Rect::new(a.grid.x, y + row - 1.0, a.grid.w, 1.0), base.shade(-0.08));
    }
    let span = a.grid.w / v.px_per_sec;
    let step = [0.1f32, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0].into_iter().find(|s| span / s <= 16.0).unwrap_or(60.0);
    let mut sec = (v.scroll_sec / step).floor() * step;
    while x_of(f64::from(sec)) < a.grid.right() {
        let xx = x_of(f64::from(sec)).round();
        ui.c.fill_rect(Rect::new(xx, a.grid.y, 1.0, a.grid.h), t.text_faint.with_alpha(28));
        ui.c.text(Font::new(10), xx + 3.0, a.grid.y + 2.0, t.text_faint, &format!("{sec:.1}s"));
        sec += step;
    }
    ui.hit(a.grid, Action::Vari(VAct::Grid), "VariAudio: click a segment to hear and select it, drag it up or down (Alt: cents); wheel scrolls, Ctrl+wheel zooms time, Alt+wheel zooms rows");

    // The waveform behind, centred, as Cubase draws it.
    let mid = a.grid.y + a.grid.h / 2.0;
    let wave_h = a.grid.h * 0.42;
    let first = dsp::frame_of(f64::from(v.scroll_sec));
    let last = dsp::frame_of(f64::from(v.scroll_sec + span)).min(v.peaks.len());
    let px = (v.px_per_sec / dsp::FRAME_RATE as f32).max(0.01);
    let stride = ((1.0 / px).ceil() as usize).max(1);
    let wave = t.text_faint.with_alpha(38);
    let mut f = first;
    while f < last {
        let p = v.peaks[f..(f + stride).min(last)].iter().cloned().fold(0.0f32, f32::max) * wave_h;
        ui.c.fill_rect(Rect::new(x_of(dsp::secs_of(f)), mid - p, (stride as f32 * px).max(1.0), 2.0 * p), wave);
        f += stride;
    }

    // Segments: a rounded block in the note's colour at the edited centre, the pitch curve through it.
    let label_font = Font::new(11);
    for (i, (sg, e)) in v.segs.iter().zip(&v.edits).enumerate() {
        let (x0, x1) = (x_of(dsp::secs_of(sg.start)), x_of(dsp::secs_of(sg.end)));
        if x1 < a.grid.x || x0 > a.grid.right() {
            continue;
        }
        let c = v.seg_centre(i);
        let block = Rect::new(x0, y_of(c) - row / 2.0 + 1.0, (x1 - x0).max(3.0), row - 2.0);
        let selected = v.sel.contains(&i);
        let col = note_colour(c);
        let rad = 3.0f32.min(block.h / 2.0);
        ui.c.fill_rrect(block, rad, col.with_alpha(if selected { 235 } else { 185 }));
        if selected {
            ui.c.stroke_rrect(block.inset(-1.0), rad + 1.0, 2.0, Color::rgb(0xffffff));
        }
        let off = (c - c.round()) * 100.0;
        let hint = format!(
            "{}{}  -  Correct {:.0}%, Straighten {:.0}%, Formant {:+.1}, Volume {:+.1} dB",
            crate::views::piano::key_name(c.round() as i32),
            if off.abs() >= 1.0 { format!(" {off:+.0} cents") } else { String::new() },
            e.correct * 100.0,
            e.straighten * 100.0,
            e.formant,
            e.gain_db
        );
        ui.hit(block, Action::Vari(VAct::Seg(i)), hint);
        let pts: Vec<(f32, f32)> = (sg.start..sg.end.min(v.frames.len()))
            .filter(|&f| v.frames[f].voiced())
            .map(|f| (x_of(dsp::secs_of(f)), y_of(dsp::edited_pitch(v.pitch_at(f), sg, e, &scale))))
            .collect();
        if pts.len() > 1 {
            ui.c.polyline(&pts, 1.4, Color::rgb(0xf4f4f4).with_alpha(230));
        }
        if block.w > 34.0 && row >= 14.0 {
            ui.c.text_clip(label_font, block.x + 4.0, block.y + (block.h - 14.0) / 2.0, block.w - 6.0, Color::rgb(0x111417), &crate::views::piano::key_name(c.round() as i32));
        }
    }
    if let Some(secs) = playing {
        let xx = x_of(secs);
        ui.c.fill_rect(Rect::new(xx, a.grid.y, 1.5, a.grid.h), t.playhead);
    }
    ui.c.pop_clip();

    // Scroll bars.
    ui.c.fill_rect(a.hbar, t.scrollbar);
    ui.c.fill_rect(a.vbar, t.scrollbar);
    let total_w = (v.secs as f32).max(0.001);
    let hthumb = Rect::new(a.hbar.x + v.scroll_sec / total_w * a.hbar.w, a.hbar.y + 2.0, (span / total_w * a.hbar.w).clamp(16.0, a.hbar.w), BAR - 4.0);
    ui.c.fill_rrect(hthumb, 3.0, t.text_faint.with_alpha(130));
    ui.hit(a.hbar, Action::Vari(VAct::HBar), "Drag to move through the take");
    let total_h = (TOP_KEY + 1) as f32 * row;
    let vthumb = Rect::new(a.vbar.x + 2.0, a.vbar.y + v.scroll_y / total_h * a.vbar.h, BAR - 4.0, (a.grid.h / total_h * a.vbar.h).clamp(16.0, a.vbar.h));
    ui.c.fill_rrect(vthumb, 3.0, t.text_faint.with_alpha(130));
    ui.hit(a.vbar, Action::Vari(VAct::VBar), "Drag to move through the notes");

    inspector(ui, v, a.inspector);
}

/// The inspector on the right (Cubase's VariAudio and Scale Assistant sections).
fn inspector(ui: &mut Ui, v: &Session, r: Rect) {
    let t = ui.t;
    ui.c.fill_rect(r, t.window_title.shade(-0.06));
    ui.c.fill_rect(Rect::new(r.x, r.y, 1.0, r.h), t.text_faint.with_alpha(60));
    let (x, w) = (r.x + 12.0, r.w - 24.0);
    let mut y = r.y + 8.0;
    let head = |ui: &mut Ui, y: f32, text: &str| {
        ui.c.fill_rect(Rect::new(r.x + 4.0, y, r.w - 8.0, 20.0), t.text_faint.with_alpha(35));
        ui.c.text(Font::new(13), r.x + 10.0, y + 2.0, Color::rgb(0xf0a830), text);
    };
    head(ui, y, "VariAudio");
    y += 26.0;
    let ts = v.targets();
    let which = if v.sel.is_empty() { format!("All {} segments", v.segs.len()) } else { format!("{} of {} segments selected", v.sel.len(), v.segs.len()) };
    ui.c.text(Font::new(11), x, y, t.text_dim, &which);
    y += 18.0;
    for sl in Slider::ALL {
        let val = ts.first().map_or(0.0, |&i| sl.get(&v.edits[i]));
        let mixed = ts.iter().any(|&i| (sl.get(&v.edits[i]) - val).abs() > 1e-3);
        ui.c.text(Font::new(12), x, y, t.text, sl.label());
        let txt = if mixed { "(mixed)".to_string() } else { sl.text(val) };
        let tw = ui.c.text_width(Font::new(12), &txt);
        ui.c.text(Font::new(12), x + w - tw, y, t.text_dim, &txt);
        y += 17.0;
        let bar = Rect::new(x, y, w, 8.0);
        let (lo, hi) = sl.range();
        let k = ((val - lo) / (hi - lo)).clamp(0.0, 1.0);
        ui.c.fill_rrect(bar, 4.0, t.text_faint.with_alpha(50));
        let zero = ((0.0f32.clamp(lo, hi) - lo) / (hi - lo)).clamp(0.0, 1.0);
        let (a, b) = if k >= zero { (zero, k) } else { (k, zero) };
        ui.c.fill_rrect(Rect::new(x + a * w, y, ((b - a) * w).max(2.0), 8.0), 4.0, Color::rgb(0x3b9fd9));
        ui.c.fill_circle(x + k * w, y + 4.0, 6.0, Color::rgb(0xe8e8e8));
        ui.hit(Rect::new(x - 6.0, y - 6.0, w + 12.0, 20.0), Action::Vari(VAct::Slider(sl)), format!("{}: drag left or right (the selected segments, all when none is)", sl.label()));
        y += 24.0;
    }
    let bw = (w - 6.0) / 2.0;
    small_button(ui, Rect::new(x, y, bw, 22.0), "Select all", false, Btn::SelectAll, "Select every segment (Ctrl+A)");
    small_button(ui, Rect::new(x + bw + 6.0, y, bw, 22.0), "Reset", false, Btn::Reset, "Back to the original take for the selected segments (R)");
    y += 34.0;

    // Giannitune: voice type, auto-tune to the editor scale, retune speed.
    head(ui, y, "Giannitune");
    y += 28.0;
    ui.c.text(Font::new(12), x, y + 3.0, t.text, "Voice");
    small_button(ui, Rect::new(x + 60.0, y, w - 60.0, 22.0), v.voice.name(), false, Btn::Voice, "The singer's range, for the pitch detector: click for the next (the take is analysed again)");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 22.0), if v.autotune { "Auto-Tune: On" } else { "Auto-Tune: Off" }, v.autotune, Btn::AutoTune, "Giannitune's auto-tune: every sung note to the editor scale (chromatic without one), glides left natural");
    y += 30.0;
    ui.c.text(Font::new(12), x, y, t.text, Slider::Retune.label());
    let txt = if v.retune < 1.0 { "0 (hard tune)".to_string() } else { Slider::Retune.text(v.retune) };
    let tw = ui.c.text_width(Font::new(12), &txt);
    ui.c.text(Font::new(12), x + w - tw, y, t.text_dim, &txt);
    y += 17.0;
    let k = (v.retune / 100.0).clamp(0.0, 1.0);
    ui.c.fill_rrect(Rect::new(x, y, w, 8.0), 4.0, t.text_faint.with_alpha(50));
    ui.c.fill_rrect(Rect::new(x, y, (k * w).max(2.0), 8.0), 4.0, if v.autotune { Color::rgb(0xf0a830) } else { t.text_faint.with_alpha(120) });
    ui.c.fill_circle(x + k * w, y + 4.0, 6.0, Color::rgb(0xe8e8e8));
    ui.hit(Rect::new(x - 6.0, y - 6.0, w + 12.0, 20.0), Action::Vari(VAct::Slider(Slider::Retune)), "Retune Speed: 0 snaps at once (the hard-tune sound), 100 glides in slowly (natural)");
    y += 26.0;

    head(ui, y, "Scale Assistant");
    y += 28.0;
    ui.c.text(Font::new(12), x, y + 3.0, t.text, "Editor scale");
    let root = v.scale.map_or("Off".to_string(), |(r, _)| crate::key_detect::NAMES[usize::from(r % 12)].to_string());
    small_button(ui, Rect::new(x + 82.0, y, 44.0, 22.0), &root, v.scale.is_some(), Btn::ScaleRoot, "The scale's root: click for the next (Off, C .. B)");
    let mode = if v.scale.is_some_and(|s| s.1) { "Minor" } else { "Major" };
    small_button(ui, Rect::new(x + 130.0, y, w - 130.0, 22.0), mode, false, Btn::ScaleMode, "Major or minor");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 22.0), "Detect scale from this take", false, Btn::DetectScale, "Finds the take's key and makes it the editor scale");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 22.0), "Use piano roll key + snap", false, Btn::PianoKey, "The key and scale chosen in the piano roll's ♪ menu, with Snap to scale on");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 22.0), "Show scale note guides", v.guides, Btn::Guides, "Darkens the rows of notes outside the editor scale");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 22.0), "Snap pitch editing", v.snap, Btn::Snap, "Dragged segments land on notes of the editor scale (Alt: free)");
    y += 28.0;
    small_button(ui, Rect::new(x, y, w, 26.0), "Quantize Pitches (Q)", false, Btn::QuantizePitches, "Correct Pitch 100%: the selected segments onto the nearest note of the editor scale (every semitone without one)");
    y += 40.0;
    if y + 26.0 <= r.bottom() {
        small_button(ui, Rect::new(x, r.bottom() - 34.0, w, 26.0), "Close VariAudio (Esc)", false, Btn::Close, "Back to the piano roll; the edited take keeps playing");
    }
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

    /// A sung line (three notes) on the playlist: VariAudio opens on it in the piano roll, a click plays a segment, a
    /// drag moves it two semitones up, the take renders to a new file the clip's channel and its Lemon Sampler then
    /// play, with a sidecar to go on from; the inspector's Volume slider and the Scale Assistant work; Esc gives the
    /// piano roll back.
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
        let row = v.row;
        // Zoom buttons change the view.
        let before = (v.px_per_sec, v.row);
        button(&mut app, Btn::ZoomInH);
        button(&mut app, Btn::ZoomInV);
        let v = app.state.piano.vari.as_deref().unwrap();
        assert!(v.px_per_sec > before.0 && v.row > before.1);
        let row = v.row.max(row);
        // Drag the middle segment up two rows.
        app.mouse = (0.0, 300.0);
        press(&mut app, VAct::Seg(1));
        assert_eq!(app.state.piano.vari.as_deref().unwrap().sel, vec![1]);
        let Some(Drag::Vari(d)) = app.drag.clone() else { panic!("a drag") };
        app.mouse = (0.0, 300.0 - 2.0 * row - 1.0);
        drag_move(&mut app, d);
        let Some(Drag::Vari(d)) = app.drag.take() else { panic!("a drag") };
        assert_eq!(app.state.piano.vari.as_deref().unwrap().edits[1].shift, 2.0);
        drag_end(&mut app, d);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            tick(&mut app);
            let ch_row = app.state.project.channels.iter().position(|c| c.index == channel).unwrap();
            let p = app.state.project.channels[ch_row].sample_path.clone().unwrap();
            if p.contains("VariAudio") {
                assert!(app.state.sample_loads.iter().any(|(r, q)| *r == ch_row && *q == p), "the Lemon Sampler loads the edited take");
                let saved = read_sidecar(&p).expect("a sidecar");
                assert_eq!(saved.original, path.to_string_lossy());
                assert_eq!((saved.segs.len(), saved.edits[1].shift), (3, 2.0));
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
        // The Volume slider: dragging right raises the selected segment.
        app.mouse = (100.0, 0.0);
        press(&mut app, VAct::Slider(Slider::Volume));
        let Some(Drag::Vari(d)) = app.drag.clone() else { panic!("a slider drag") };
        app.mouse = (100.0 + (INSPECTOR_W - 24.0) / 6.0, 0.0);
        drag_move(&mut app, d);
        assert!((app.state.piano.vari.as_deref().unwrap().edits[1].gain_db - 6.0).abs() < 0.2);
        app.drag = None;
        // Scale Assistant: C major, then snap a drag onto it.
        button(&mut app, Btn::ScaleRoot);
        assert_eq!(app.state.piano.vari.as_deref().unwrap().scale, Some((0, false)));
        button(&mut app, Btn::Snap);
        app.mouse = (0.0, 300.0);
        press(&mut app, VAct::Seg(0));
        let Some(Drag::Vari(d)) = app.drag.clone() else { panic!("a drag") };
        app.mouse = (0.0, 300.0 - row - 1.0); // C up one: C# is not in C major, so D
        drag_move(&mut app, d);
        app.drag = None;
        let v = app.state.piano.vari.as_deref().unwrap();
        assert_eq!(v.segs[0].pitch.round() + v.edits[0].shift.round(), 62.0);
        // Giannitune auto-tune: on, its curve follows the scale; the retune slider changes it.
        button(&mut app, Btn::AutoTune);
        let v = app.state.piano.vari.as_deref().unwrap();
        assert!(v.autotune && v.base().is_some_and(|b| b.len() == v.frames.len()));
        let f = (v.segs[1].start + v.segs[1].end) / 2;
        assert!((v.pitch_at(f) - 64.0).abs() < 0.3, "E is in C major: {}", v.pitch_at(f));
        app.mouse = (100.0, 0.0);
        press(&mut app, VAct::Slider(Slider::Retune));
        let Some(Drag::Vari(d)) = app.drag.clone() else { panic!("a slider drag") };
        app.mouse = (100.0 + (INSPECTOR_W - 24.0) / 2.0, 0.0);
        drag_move(&mut app, d);
        app.drag = None;
        assert!((app.state.piano.vari.as_deref().unwrap().retune - 70.0).abs() < 1.0);
        // Esc: the piano roll again.
        app.key(&winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape), false, false);
        assert!(app.state.piano.vari.is_none());
        let _ = std::fs::remove_file(&path);
    }
}
