// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Shared, params::*, view};
use nih_plug::prelude::*;
use os_plugkit::{MenuDef, vals::Vals};
use os_ui::Rect;
use os_ui::kit::menu::{MenuItem, MenuStyle};
use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicIsize, AtomicU32, Ordering},
    },
};
pub struct ChopEditor {
    pub params: Arc<ChopParams>,
    pub shared: Arc<Shared>,
    /// DPI scale as f32 bits.
    pub scale: AtomicU32,
    /// The open editor window (0 = closed): parameter changes post a repaint to it.
    #[cfg_attr(not(windows), allow(dead_code))]
    hwnd: Arc<AtomicIsize>,
    /// Linux and macOS: the open editor's thread waker, for the same.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    slot: os_plugkit::gui::Slot,
}

impl ChopEditor {
    pub fn new(params: Arc<ChopParams>, shared: Arc<Shared>) -> Self {
        ChopEditor {
            params,
            shared,
            scale: AtomicU32::new(1f32.to_bits()),
            hwnd: Arc::new(AtomicIsize::new(0)),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            slot: Default::default(),
        }
    }

    fn notify(&self) {
        #[cfg(windows)]
        win32::post_changed(self.hwnd.load(Ordering::Acquire));
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        os_plugkit::gui::notify(&self.slot);
    }
}

impl Editor for ChopEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send> {
        let scale = f32::from_bits(self.scale.load(Ordering::Relaxed));
        match parent {
            #[cfg(windows)]
            ParentWindowHandle::Win32Hwnd(hwnd) => Box::new(win32::open(
                hwnd,
                scale,
                self.params.clone(),
                self.shared.clone(),
                context,
                self.hwnd.clone(),
            )),
            #[cfg(target_os = "linux")]
            ParentWindowHandle::X11Window(window) => Box::new(gui::open(
                window,
                scale,
                self.params.clone(),
                self.shared.clone(),
                context,
                &self.slot,
            )),
            // macOS hosts set no scale (sizes are points): the editor paints at its window's backing scale.
            #[cfg(target_os = "macos")]
            ParentWindowHandle::AppKitNsView(view) => {
                let scale = os_plugkit::gui::backing_scale(view as usize);
                Box::new(gui::open(
                    view as usize,
                    scale,
                    self.params.clone(),
                    self.shared.clone(),
                    context,
                    &self.slot,
                ))
            }
            // Elsewhere no editor opens yet.
            _ => {
                let _ = (context, scale);
                Box::new(())
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        (view::W, view::H)
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        self.scale.store(factor.to_bits(), Ordering::Relaxed);
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        self.notify();
    }
    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        self.notify();
    }
    fn param_values_changed(&self) {
        self.notify();
    }
}

#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Left,
    Right,
    Middle,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Button(&'static str),
    Param(usize),
    Pad(usize),
    Wave,
    Overview,
    Start,
    End,
    Marker(usize),
    Beat(f64),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Repaint,
    Lock,
    Menu(Target, f32, f32),
    TypeIn(String, f32, f32),
    Automate(usize),
}
#[derive(Clone, Debug)]
pub enum Cmd {
    None,
    Rename,
    Favorite,
    Delete,
    Copy,
    Paste,
    Automate(usize),
    Reset(usize),
    Scale(usize, f32),
    TriggerEnvelope,
}
#[derive(Clone)]
struct Snapshot {
    doc: Document,
    values: Vec<f32>,
    selection: u32,
}
pub struct Logic {
    pub vals: Vals,
    pub params: Arc<ChopParams>,
    pub shared: Arc<Shared>,
    pub hint: String,
    pub hosted: bool,
    pub hits: Vec<(Rect, Target)>,
    pub cursor: f64,
    pub zoom: f64,
    pub offset: f64,
    pub snap: bool,
    pub frozen: bool,
    pub grid: bool,
    pub mode: usize,
    pub beats: f64,
    pub selection: u32,
    drag: Option<(Target, f32, f32)>,
    // Preserve sub-step movement instead of starting each delta from a rounded parameter.
    drag_value: f64,
    hover: Option<Target>,
    held: Option<u8>,
    keys: [bool; 32],
    history: Vec<Snapshot>,
    future: Vec<Snapshot>,
    copy: Option<(Cue, [f32; STRIDE])>,
    analysis: u64,
    pub auto_analyse: bool,
    drop_pad: Option<usize>,
    drag_copy: bool,
    grid_backup: Option<(Document, f32)>,
    taps: Vec<f64>,
    observed_path: String,
    grid_drag: Option<(Document, f64)>,
    slicer_active: bool,
}
impl Drop for Logic {
    fn drop(&mut self) {
        self.release_keys();
        self.shared.cancel.store(true, Ordering::Release);
    }
}
impl Logic {
    pub fn new(
        params: Arc<ChopParams>,
        shared: Arc<Shared>,
        ctx: Option<Arc<dyn GuiContext>>,
    ) -> Self {
        let vals = Vals::new(&*params, params.live.clone(), ctx);
        let selection = 1 << params.document().selected;
        let observed_path = params.document().path.clone();
        Self {
            vals,
            params,
            shared,
            hint: String::new(),
            hosted: false,
            hits: vec![],
            cursor: 0.,
            zoom: 1.,
            offset: 0.,
            snap: true,
            frozen: false,
            grid: false,
            mode: 0,
            beats: 1.,
            selection,
            drag: None,
            drag_value: 0.,
            hover: None,
            held: None,
            keys: [false; 32],
            history: vec![],
            future: vec![],
            copy: None,
            analysis: 0,
            auto_analyse: false,
            drop_pad: None,
            drag_copy: false,
            grid_backup: None,
            taps: Vec::new(),
            observed_path,
            grid_drag: None,
            slicer_active: false,
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            doc: (*self.params.document()).clone(),
            selection: self.selection,
            values: self
                .params
                .defs
                .iter()
                .map(|d| self.vals.plain(&d.id))
                .collect(),
        }
    }
    fn remember(&mut self) {
        let s = self.snapshot();
        self.history.push(s);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.future.clear();
    }
    fn restore(&mut self, s: Snapshot) {
        self.params.clear_captures();
        self.auto_analyse = false;
        self.grid = false;
        self.grid_backup = None;
        self.selection = s.selection;
        self.params.set_doc(s.doc);
        for (i, v) in s.values.into_iter().enumerate() {
            self.set(i, v);
        }
        self.shared.stop.store(true, Ordering::Relaxed);
    }
    fn touch(&mut self) {
        let v = self.vals.norm("master");
        self.vals.edit("master", v);
    }
    pub fn set(&mut self, k: usize, v: f32) {
        let id = self.params.defs[k].id.clone();
        self.vals.edit_plain(&id, v);
    }
    pub fn control_value(&self, k: usize) -> f32 {
        let value = self.vals.plain(&self.params.defs[k].id);
        if k >= GLOBAL && self.selection.count_ones() > 1 {
            let field = (k - GLOBAL) % STRIDE;
            if (0..32).any(|n| {
                self.selection & (1 << n) != 0
                    && (self.vals.plain(&self.params.defs[pad(n, field)].id) - value).abs() > 0.0001
            }) {
                return self.params.defs[k].default;
            }
        }
        value
    }
    pub fn poll(&mut self) {
        self.vals.expire();
        self.shared.snap.store(self.snap, Ordering::Relaxed);
        let played = self.shared.played_pad.swap(u32::MAX, Ordering::Acquire);
        if played < 32 && self.selection.count_ones() == 1 {
            let mut d = (*self.params.document()).clone();
            if d.selected != played as usize {
                d.selected = played as usize;
                self.params.set_doc(d);
            }
            self.selection = 1 << played;
        }
        if !self.frozen && self.drag.is_none() && self.shared.sounding.load(Ordering::Relaxed) != 0
        {
            self.cursor = f64::from_bits(self.shared.position.load(Ordering::Relaxed));
        }
        self.shared
            .capture_cursor
            .store(self.cursor.to_bits(), Ordering::Relaxed);
        let doc = self.params.document();
        if doc.path != self.observed_path {
            self.slicer_active = false;
            // Host-delivered Explorer drops bypass this editor's Load action.
            self.observed_path = doc.path.clone();
            self.selection = 1 << doc.selected;
            self.grid = false;
            self.grid_backup = None;
            self.cursor = 0.;
            self.offset = 0.;
            self.zoom = 1.;
            self.taps.clear();
            self.auto_analyse = false;
        }
        if self.selection & (1 << doc.selected) == 0 {
            self.selection = 1 << doc.selected;
        }
        while let Some(a) = self.shared.controller.pop() {
            use crate::controller::Action;
            match a {
                Action::Button(name) => self.button(name, Mods::default()),
                Action::Set(i, v) => {
                    self.remember();
                    self.set(i, v);
                }
                Action::Delta(i, d) => {
                    self.remember();
                    self.set(i, self.params.v(i) + d);
                }
            }
        }
        let n = self.shared.analysis_version.load(Ordering::Acquire);
        if n != self.analysis || self.auto_analyse {
            self.analysis = n;
            let a = self.shared.audio.lock().unwrap().clone();
            if let Some(a) = a.filter(|a| a.path == self.params.document().path) {
                if self.auto_analyse
                    || (!self.params.document().analysis_complete
                        && !self.params.document().cues.iter().any(|c| c.enabled))
                {
                    self.set(SOURCE_BPM, a.bpm);
                    let mut d = (*self.params.document()).clone();
                    d.key = a.key.clone();
                    d.analysis_complete = true;
                    d.autoset(0, a.duration(), a.bpm as f64, 0., 1., &a.onsets, 1);
                    self.params.set_doc(d);
                    self.touch();
                    self.auto_analyse = false;
                } else if !self.params.document().analysis_complete {
                    let mut d = (*self.params.document()).clone();
                    d.analysis_complete = true;
                    self.params.set_doc(d);
                }
            }
        }
        self.hint = self.shared.status.lock().unwrap().clone();
    }
    pub fn duration(&self) -> f64 {
        self.shared
            .audio
            .lock()
            .unwrap()
            .as_ref()
            .filter(|a| a.path == self.params.document().path)
            .map(|a| a.duration())
            .unwrap_or(1.)
    }
    pub fn load(&mut self, path: String) {
        self.remember();
        self.params.clear_captures();
        self.slicer_active = false;
        self.observed_path = path.clone();
        self.release_keys();
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cancel.store(true, Ordering::Release);
        *self.shared.audio.lock().unwrap() = None;
        self.shared.waveform.lock().unwrap().clear();
        self.grid = false;
        self.grid_backup = None;
        self.selection = 1;
        self.taps.clear();
        let previous = self.params.document();
        self.params.set_doc(Document {
            path,
            slab: previous.slab,
            glide_trigger: previous.glide_trigger,
            ..Default::default()
        });
        self.shared.reload.fetch_add(1, Ordering::Release);
        for i in GLOBAL..self.params.p.len() {
            self.set(i, self.params.defs[i].default);
        }
        self.cursor = 0.;
        self.offset = 0.;
        self.zoom = 1.;
        self.auto_analyse = true;
        self.touch();
    }
    pub fn tap(&mut self, now: f64) {
        if self
            .taps
            .last()
            .is_some_and(|last| now - last > 3. || now <= *last)
        {
            self.taps.clear();
        }
        if self.taps.is_empty() {
            self.remember();
        }
        self.taps.push(now);
        if self.taps.len() > 8 {
            self.taps.remove(0);
        }
        if self.taps.len() > 1 {
            let bpm = 60. * (self.taps.len() - 1) as f64 / (now - self.taps[0]);
            self.edit_param(SOURCE_BPM, bpm.clamp(20., 400.) as f32);
        }
    }
    fn hit(&self, x: f32, y: f32) -> Option<Target> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, t)| t.clone())
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some() || self.held.is_some()
    }
    pub fn visual_state(&self, target: &Target, on: bool) -> os_ui::kit::State {
        os_ui::kit::State::new(
            self.hover.as_ref() == Some(target),
            self.drag.as_ref().is_some_and(|(t, _, _)| t == target),
            on,
        )
    }
    pub fn pad_drop_target(&self) -> Option<usize> {
        self.drop_pad
    }
    pub fn interaction_hint(&self) -> String {
        let target = self
            .drag
            .as_ref()
            .map(|(t, _, _)| t)
            .or(self.hover.as_ref());
        match target {
            Some(Target::Param(k)) => format!("{}  •  Drag vertically  •  Ctrl: fine adjustment  •  Double-click: type value  •  Right-click: options", self.params.defs[*k].name),
            Some(Target::Pad(_)) => "Click to play  •  Ctrl-click: select multiple  •  Drag: move / swap  •  Alt-drag: copy  •  Right-click: pad options".into(),
            Some(Target::Start | Target::End) => "Drag to trim the selected cue  •  Snap follows the beat grid".into(),
            Some(Target::Marker(_) | Target::Beat(_)) => "Drag to adjust this beat  •  Ctrl-drag: slip the whole grid".into(),
            Some(Target::Wave | Target::Overview) => "Click or drag to seek  •  Scroll to zoom  •  Freeze keeps the view in place".into(),
            _ => self.hint.clone(),
        }
    }
    fn at(&self, x: f32, overview: bool) -> f64 {
        let duration = self.duration();
        let width = duration / if overview { 1. } else { self.zoom };
        let offset = if overview { 0. } else { self.offset };
        (offset + ((x - 16.) / 1008.).clamp(0., 1.) as f64 * width).clamp(0., duration)
    }
    fn quantized(&self, t: f64) -> f64 {
        if !self.snap {
            return t;
        }
        let d = self.params.document();
        let bpm = self.params.v(SOURCE_BPM) as f64;
        d.time_at((d.beat_at(t, bpm) * 8.).round() / 8., bpm)
            .clamp(0., self.duration())
    }
    pub fn down(&mut self, x: f32, y: f32, btn: Btn, m: Mods, dbl: bool) -> Vec<Act> {
        self.hover = self.hit(x, y);
        let Some(t) = self.hit(x, y) else {
            return vec![];
        };
        if btn == Btn::Right {
            return vec![Act::Menu(t, x, y)];
        }
        if self.grid && matches!(t, Target::Wave | Target::Marker(_) | Target::Beat(_)) {
            self.remember();
            self.grid_drag = Some(((*self.params.document()).clone(), self.at(x, false)));
        }
        match t.clone() {
            Target::Param(k) => {
                if (12..16).contains(&k) && !dbl {
                    let ready = self
                        .shared
                        .audio
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|a| a.stems.iter().all(Option::is_some));
                    if !ready {
                        self.shared.message("Choose Separate stems first");
                        return vec![Act::Repaint];
                    }
                    self.remember();
                    if m.shift {
                        for j in 12..16 {
                            self.set(j, if j == k { 1. } else { 0. });
                        }
                    } else {
                        self.set(k, if self.params.v(k) > 0. { 0. } else { 1. });
                    }
                    return vec![Act::Repaint];
                }
                if dbl {
                    return vec![Act::TypeIn(self.params.defs[k].id.clone(), x, y)];
                }
                self.remember();
                if m.alt || m.shift {
                    self.edit_param(k, self.params.defs[k].default);
                } else if self.params.defs[k].max - self.params.defs[k].min == 1.
                    && self.params.defs[k].step == 1.
                {
                    self.edit_param(k, 1. - self.params.v(k));
                } else {
                    self.drag_value = self.control_value(k) as f64;
                    self.drag = Some((t, x, y));
                    return vec![Act::Lock];
                }
            }
            Target::Pad(n) => {
                self.drop_pad = None;
                self.drag_copy = m.alt;
                if m.shift {
                    self.remember();
                    self.params.clear_capture(n);
                    let mut d = (*self.params.document()).clone();
                    d.cues[n] = Cue::default();
                    self.params.set_doc(d);
                    self.touch();
                } else if m.ctrl {
                    self.selection ^= 1 << n;
                    let mut d = (*self.params.document()).clone();
                    if self.selection == 0 {
                        self.selection = 1 << n;
                    }
                    d.selected = if self.selection & (1 << n) != 0 {
                        n
                    } else {
                        self.selection.trailing_zeros() as usize
                    };
                    self.params.set_doc(d);
                } else {
                    let mut d = (*self.params.document()).clone();
                    if !d.cues[n].enabled {
                        self.remember();
                        let start = self.quantized(self.cursor);
                        d.cues[n] = Cue {
                            start,
                            end: self.duration(),
                            enabled: start < self.duration(),
                            name: format!("Chop {:02}", n + 1),
                            favorite: false,
                            color: None,
                        };
                    }
                    d.selected = n;
                    self.selection = 1 << n;
                    self.cursor = d.cues[n].start;
                    self.params.set_doc(d);
                    self.touch();
                    let note = if self.params.v(KEYBOARD) > 0.5 {
                        60
                    } else {
                        (36 + n) as u8
                    };
                    self.held = Some(note);
                    let _ = self.shared.notes.push((note, 1., true));
                    self.drag = Some((t, x, y));
                }
            }
            Target::Wave | Target::Overview => {
                self.cursor = self.at(x, t == Target::Overview);
                if !self.grid {
                    self.shared
                        .preview_cursor
                        .store(self.cursor.to_bits(), Ordering::Relaxed);
                    self.shared.seek_preview.store(true, Ordering::Release);
                }
                if t == Target::Overview {
                    self.offset = (self.cursor - self.duration() / self.zoom / 2.)
                        .clamp(0., (self.duration() - self.duration() / self.zoom).max(0.));
                }
                self.drag = Some((t, x, y));
            }
            Target::Start | Target::End | Target::Marker(_) | Target::Beat(_) => {
                if !self.grid || matches!(t, Target::Start | Target::End) {
                    self.remember();
                }
                self.drag = Some((t, x, y));
            }
            Target::Button(name) => self.button(name, m),
        }
        vec![Act::Repaint]
    }
    pub fn delta(&mut self, _dx: f32, dy: f32, m: Mods) -> Vec<Act> {
        if let Some((Target::Param(k), _, _)) = self.drag.clone() {
            let d = &self.params.defs[k];
            let step = -(dy as f64 / 200.) * (d.max - d.min) as f64 * if m.ctrl { 0.1 } else { 1. };
            self.drag_value = (self.drag_value + step).clamp(d.min as f64, d.max as f64);
            self.edit_param(k, self.drag_value as f32);
        }
        vec![Act::Repaint]
    }
    fn edit_param(&mut self, k: usize, value: f32) {
        if k == SOURCE_BPM {
            let value = value.clamp(20., 400.);
            let mut d = (*self.params.document()).clone();
            d.scale_grid(self.params.v(SOURCE_BPM) as f64 / value as f64);
            self.params.set_doc(d);
        }
        if k >= GLOBAL {
            let field = (k - GLOBAL) % STRIDE;
            for n in 0..32 {
                if self.selection & (1 << n) != 0 {
                    self.set(pad(n, field), value);
                }
            }
        } else {
            self.set(k, value);
        }
    }
    pub fn moved(&mut self, x: f32, y: f32, m: Mods) -> Vec<Act> {
        self.hover = self.hit(x, y);
        if let Some((t, _, _)) = self.drag.clone() {
            if let Some((original, at)) = self.grid_drag.clone() {
                let time = self.at(x, false);
                let mut d = original;
                if m.ctrl {
                    d.slip_grid(time - at);
                } else if let Some(beat) = match t {
                    Target::Beat(b) => Some(b),
                    Target::Marker(i) => Some(d.marker_beat(i)),
                    _ => None,
                } {
                    if d.markers.is_empty() {
                        d.set_anchor(d.grid_start, 0.);
                    }
                    d.set_anchor(time, beat);
                } else {
                    self.cursor = time;
                    return vec![Act::Repaint];
                }
                self.params.set_doc(d);
                self.touch();
                return vec![Act::Repaint];
            }
            match t {
                Target::Pad(_) => {
                    self.drop_pad = if let Some(Target::Pad(n)) = self.hit(x, y) {
                        Some(n)
                    } else {
                        None
                    };
                }
                Target::Start | Target::End => {
                    let mut d = (*self.params.document()).clone();
                    let time = self.quantized(self.at(x, false));
                    let c = &mut d.cues[d.selected];
                    if t == Target::Start {
                        c.start = time.min((c.end - 0.0001).max(0.));
                    } else {
                        c.end = time.max(c.start + 0.0001).min(self.duration());
                    }
                    self.params.set_doc(d);
                    self.touch();
                }
                Target::Marker(i) => {
                    let mut d = (*self.params.document()).clone();
                    let lo = if i > 0 { d.markers[i - 1] + 0.6 } else { 0. };
                    let hi = d
                        .markers
                        .get(i + 1)
                        .map_or(self.duration(), |t| t - 0.6)
                        .max(lo);
                    d.markers[i] = self.at(x, false).clamp(lo, hi);
                    if i == 0 {
                        d.grid_start = d.markers[0];
                    }
                    self.params.set_doc(d);
                    self.touch();
                }
                Target::Wave => {
                    self.cursor = self.at(x, false);
                    self.shared
                        .preview_cursor
                        .store(self.cursor.to_bits(), Ordering::Relaxed);
                    self.shared.seek_preview.store(true, Ordering::Release);
                }
                Target::Overview => {
                    self.cursor = self.at(x, true);
                    self.shared
                        .preview_cursor
                        .store(self.cursor.to_bits(), Ordering::Relaxed);
                    self.shared.seek_preview.store(true, Ordering::Release);
                    self.offset = (self.cursor - self.duration() / self.zoom / 2.)
                        .clamp(0., (self.duration() - self.duration() / self.zoom).max(0.));
                }
                _ => {}
            }
        }
        let _ = y;
        vec![Act::Repaint]
    }
    pub fn up(&mut self, _: Btn) -> Vec<Act> {
        if let (Some((Target::Pad(source), _, _)), Some(dest)) =
            (self.drag.clone(), self.drop_pad.take())
        {
            if source != dest {
                self.remember();
                let mut d = (*self.params.document()).clone();
                let a: [f32; STRIDE] = std::array::from_fn(|k| self.params.v(pad(source, k)));
                let b: [f32; STRIDE] = std::array::from_fn(|k| self.params.v(pad(dest, k)));
                if self.drag_copy {
                    d.cues[dest] = d.cues[source].clone();
                } else {
                    d.cues.swap(source, dest);
                }
                for k in 0..STRIDE {
                    self.set(pad(dest, k), a[k]);
                    if !self.drag_copy {
                        self.set(pad(source, k), b[k]);
                    }
                }
                d.selected = dest;
                self.selection = 1 << dest;
                self.params.set_doc(d);
                self.touch();
            }
        }
        if let Some(n) = self.held.take() {
            let _ = self.shared.notes.push((n, 0., false));
        }
        self.drag = None;
        self.grid_drag = None;
        vec![Act::Repaint]
    }
    pub fn leave(&mut self) -> Vec<Act> {
        self.hover = None;
        vec![Act::Repaint]
    }
    pub fn wheel(&mut self, x: f32, y: f32, n: f32, m: Mods) -> Vec<Act> {
        match self.hit(x, y) {
            Some(Target::Param(k)) => {
                self.remember();
                let d = &self.params.defs[k];
                let step = d.step.max((d.max - d.min) / 100.) * if m.ctrl { 0.1 } else { 1. };
                self.edit_param(k, self.params.v(k) + n * step);
            }
            Some(Target::Wave | Target::Start | Target::End) => {
                let anchor = self.at(x, false);
                self.zoom = (self.zoom * 2f64.powf(n as f64 * 0.5)).clamp(1., 512.);
                self.offset = (anchor - ((x - 16.) / 1008.) as f64 * self.duration() / self.zoom)
                    .clamp(0., (self.duration() - self.duration() / self.zoom).max(0.));
            }
            _ => {}
        }
        vec![Act::Repaint]
    }
    pub fn release_keys(&mut self) {
        for i in 0..32 {
            if self.keys[i] {
                let _ = self.shared.notes.push(((36 + i) as u8, 0., false));
                self.keys[i] = false;
            }
        }
        self.up(Btn::Left);
    }
    pub fn key(&mut self, key: u32, on: bool) {
        let map = b"ZXCVBNMASDFGHJKLQWERTYUI123456789";
        let upper = char::from_u32(key).unwrap_or('\0').to_ascii_uppercase() as u8;
        if let Some(i) = map.iter().position(|c| *c == upper) {
            if self.keys[i] != on {
                self.keys[i] = on;
                let _ = self
                    .shared
                    .notes
                    .push(((36 + i) as u8, if on { 1. } else { 0. }, on));
            }
        }
        if on && key == 32 {
            self.button("Stop", Mods::default());
        }
    }
    pub fn button(&mut self, name: &str, m: Mods) {
        match name {
            "SLAB" => {
                self.remember();
                let mut d = (*self.params.document()).clone();
                d.slab = !d.slab;
                self.params.set_doc(d);
                self.touch();
                self.shared.message(
                    "SLAB MIDI profile: pages 1-2 play pads; keep this editor open for controls",
                );
            }
            "Play/stop" => {
                if self.shared.sounding.load(Ordering::Relaxed) == 0 {
                    self.button("Preview", m);
                } else {
                    self.button("Stop", m);
                }
            }
            "Import cues" => {
                if !self
                    .shared
                    .audio
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|a| a.path == self.params.document().path)
                {
                    self.shared
                        .message("Wait for the source to finish loading before importing cues");
                    return;
                }
                let mut d = (*self.params.document()).clone();
                match crate::cues::import(
                    std::path::Path::new(&d.path.clone()),
                    &mut d,
                    self.duration(),
                ) {
                    Ok(n) => {
                        self.remember();
                        self.params.set_doc(d);
                        self.touch();
                        self.shared
                            .message(format!("Imported {n} DJ cues; favorites preserved"));
                    }
                    Err(e) => self.shared.message(e),
                }
            }
            "Load" | "Locate" => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["wav", "aiff", "aif", "flac", "mp3", "ogg", "m4a", "wv"],
                    )
                    .pick_file()
                {
                    if name == "Locate" {
                        self.remember();
                        let mut d = (*self.params.document()).clone();
                        d.path = p.to_string_lossy().into_owned();
                        d.stems = Default::default();
                        self.params.set_doc(d);
                        self.auto_analyse = false;
                        self.grid = false;
                        self.grid_backup = None;
                        self.shared.reload.fetch_add(1, Ordering::Release);
                        self.shared.stop.store(true, Ordering::Relaxed);
                        self.touch();
                    } else {
                        self.load(p.to_string_lossy().into_owned());
                    }
                }
            }
            "Undo" => {
                if let Some(s) = self.history.pop() {
                    self.future.push(self.snapshot());
                    self.restore(s);
                }
            }
            "Redo" => {
                if let Some(s) = self.future.pop() {
                    self.history.push(self.snapshot());
                    self.restore(s);
                }
            }
            "Separate" => self.shared.separate(self.params.clone()),
            "Cancel" => {
                self.shared.cancel.store(true, Ordering::Release);
                self.shared.cancel_analysis.store(true, Ordering::Release);
                self.auto_analyse = false;
            }
            "Eject" => {
                self.load(String::new());
                self.auto_analyse = false;
            }
            "Glide mode" => {
                self.remember();
                let mut d = (*self.params.document()).clone();
                d.glide_trigger = !d.glide_trigger;
                self.params.set_doc(d);
                self.touch();
            }
            "Tap" => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                self.tap(now);
            }
            "Click" => {
                let on = self.shared.metronome.load(Ordering::Relaxed);
                self.shared.metronome.store(!on, Ordering::Relaxed);
            }
            "Grid save" => {
                self.grid_backup = None;
                self.grid = false;
                self.touch();
            }
            "Grid cancel" => {
                if let Some((d, bpm)) = self.grid_backup.take() {
                    let mut now = (*self.params.document()).clone();
                    now.grid_start = d.grid_start;
                    now.markers = d.markers;
                    now.marker_beats = d.marker_beats;
                    self.params.set_doc(now);
                    self.set(SOURCE_BPM, bpm);
                    self.touch();
                }
                self.grid = false;
            }
            "Remove marker" => {
                self.remember();
                let mut d = (*self.params.document()).clone();
                if let Some((i, _)) = d.markers.iter().enumerate().min_by(|a, b| {
                    (a.1 - self.cursor)
                        .abs()
                        .total_cmp(&(b.1 - self.cursor).abs())
                }) {
                    d.markers.remove(i);
                    d.marker_beats.remove(i);
                    if let Some(first) = d.markers.first() {
                        d.grid_start = *first;
                    }
                }
                self.params.set_doc(d);
                self.touch();
            }

            "Stop" => {
                self.release_keys();
                self.shared.stop.store(true, Ordering::Relaxed);
            }
            "Preview" => {
                self.shared
                    .preview_cursor
                    .store(self.cursor.to_bits(), Ordering::Relaxed);
                let _ = self.shared.notes.push((127, 1., true));
            }
            "Snap" => self.snap = !self.snap,
            "Slip <" | "Slip >" | "Adjust <" | "Adjust >" => {
                self.remember();
                let mut d = (*self.params.document()).clone();
                let delta =
                    if name.ends_with('<') { -0.01 } else { 0.01 } * if m.ctrl { 0.1 } else { 1. };
                if name.starts_with("Slip") {
                    d.slip_grid(delta);
                } else {
                    let bpm = self.params.v(SOURCE_BPM) as f64;
                    let beat = d.beat_at(self.cursor, bpm).round().max(1.);
                    let time = d.time_at(beat, bpm);
                    if d.markers.is_empty() {
                        d.set_anchor(d.grid_start, 0.);
                    }
                    d.set_anchor(time + delta, beat);
                }
                self.params.set_doc(d);
                self.touch();
            }
            "Freeze" => self.frozen = !self.frozen,
            "Grid" => {
                if self.grid {
                    self.button("Grid save", m);
                } else {
                    self.remember();
                    self.grid_backup =
                        Some(((*self.params.document()).clone(), self.params.v(SOURCE_BPM)));
                    self.grid = true;
                }
            }
            "Zoom +" => self.zoom = (self.zoom * 2.).min(512.),
            "Zoom -" => {
                self.zoom = (self.zoom / 2.).max(1.);
                self.offset = self
                    .offset
                    .min(self.duration() - self.duration() / self.zoom);
            }
            "Mode" => self.mode = (self.mode + 1) % 4,
            "Size -" => self.beats = (self.beats / 2.).max(0.0625),
            "Size +" => self.beats = (self.beats * 2.).min(16.),
            "Region <" => {
                let d = self.params.document();
                let bpm = self.params.v(SOURCE_BPM) as f64;
                self.cursor = d
                    .time_at(d.beat_at(self.cursor, bpm) - 32. * self.beats, bpm)
                    .clamp(0., self.duration());
            }
            "Region >" => {
                let d = self.params.document();
                let bpm = self.params.v(SOURCE_BPM) as f64;
                self.cursor = d
                    .time_at(d.beat_at(self.cursor, bpm) + 32. * self.beats, bpm)
                    .clamp(0., self.duration());
            }
            "Autoset" => {
                self.remember();
                let a = self.shared.audio.lock().unwrap().clone();
                if let Some(a) = a {
                    let mut d = (*self.params.document()).clone();
                    let source = d.selected;
                    let source_values: [f32; STRIDE] =
                        std::array::from_fn(|k| self.params.v(pad(source, k)));
                    let pitch = self.params.v(pad(source, PITCH));
                    d.autoset(
                        self.mode,
                        a.duration(),
                        self.params.v(SOURCE_BPM) as f64,
                        self.cursor,
                        self.beats,
                        &a.onsets,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos() as u64,
                    );
                    if self.mode == 3 {
                        for n in 0..32 {
                            if !d.cues[n].favorite {
                                for k in 0..STRIDE {
                                    self.set(pad(n, k), source_values[k]);
                                }
                                self.set(pad(n, PITCH), (pitch + n as f32 - 12.).clamp(-24., 24.));
                            }
                        }
                    }
                    self.params.set_doc(d);
                    self.slicer_active = self.mode == 1;
                    self.touch();
                }
            }
            "All" => {
                self.selection = if self.selection == u32::MAX {
                    1 << self.params.document().selected
                } else {
                    u32::MAX
                }
            }
            "Delete" | "Favorite" | "Set start" | "Set end" | "Grid start" | "Grid marker"
            | "Clear grid" => {
                self.remember();
                if name == "Delete" {
                    self.params.clear_captures();
                }
                let mut d = (*self.params.document()).clone();
                match name {
                    "Grid start" => {
                        d.grid_start = self.cursor;
                        d.markers = vec![self.cursor];
                        d.marker_beats = vec![0.];
                    }
                    "Grid marker" => {
                        let beat = d
                            .beat_at(self.cursor, self.params.v(SOURCE_BPM) as f64)
                            .round();
                        if d.markers.is_empty() {
                            d.set_anchor(d.grid_start, 0.);
                        }
                        d.set_anchor(self.cursor, beat);
                    }
                    "Clear grid" => {
                        d.grid_start = 0.;
                        d.markers.clear();
                        d.marker_beats.clear();
                    }
                    _ => {
                        for n in 0..32 {
                            if self.selection & (1 << n) == 0 {
                                continue;
                            }
                            let c = &mut d.cues[n];
                            match name {
                                "Delete" => *c = Cue::default(),
                                "Favorite" => c.favorite = !c.favorite,
                                "Set start" => {
                                    c.start =
                                        self.quantized(self.cursor).min((c.end - 0.0001).max(0.));
                                }
                                "Set end" => {
                                    c.end = self
                                        .quantized(self.cursor)
                                        .max(c.start + 0.0001)
                                        .min(self.duration())
                                }
                                _ => {}
                            }
                        }
                    }
                }
                self.params.set_doc(d);
                self.touch();
            }
            "BPM /2" | "BPM x2" => {
                self.remember();
                self.edit_param(
                    SOURCE_BPM,
                    self.params.v(SOURCE_BPM) * if name == "BPM /2" { 0.5 } else { 2. },
                );
            }
            "Export" => {
                if self.shared.busy.load(Ordering::Acquire) {
                    self.shared
                        .message("Wait for the current job or cancel it first");
                    return;
                }
                let a = self.shared.audio.lock().unwrap().clone();
                let Some(a) = a.filter(|a| a.path == self.params.document().path) else {
                    self.shared.message("Load audio before exporting");
                    return;
                };
                let d = self.params.document();
                if !d.cues[d.selected].enabled {
                    self.shared.message("Select a populated pad to export");
                    return;
                }
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("WAV", &["wav"])
                    .set_file_name("Lemon Chop.wav")
                    .save_file()
                {
                    let values = self
                        .params
                        .defs
                        .iter()
                        .enumerate()
                        .map(|(i, _)| self.params.v(i))
                        .collect::<Vec<_>>();
                    let tempo = f64::from_bits(self.shared.tempo.load(Ordering::Relaxed));
                    let sh = self.shared.clone();
                    sh.busy.store(true, Ordering::Release);
                    sh.cancel.store(false, Ordering::Release);
                    sh.message("Rendering pad with pitch, tempo, filter and envelopes...");
                    std::thread::spawn(move || {
                        let temp = file.with_extension(format!(
                            "{}-{}.rendering",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos()
                        ));
                        let mut created = false;
                        let result = (|| -> Result<(), String> {
                            let f = std::fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(&temp)
                                .map_err(|e| e.to_string())?;
                            created = true;
                            let mut out = std::io::BufWriter::new(f);
                            crate::export::render(&mut out, a, &d, &values, tempo, &|| {
                                sh.cancel.load(Ordering::Acquire)
                            })?;
                            drop(out);
                            if sh.cancel.load(Ordering::Acquire) {
                                return Err("Export cancelled".into());
                            }
                            crate::export::commit(&temp, &file).map_err(|e| e.to_string())?;
                            Ok(())
                        })();
                        if created {
                            let _ = std::fs::remove_file(&temp);
                        }
                        sh.message(result.map_or_else(|e| e, |_| "Processed pad exported".into()));
                        sh.busy.store(false, Ordering::Release);
                    });
                }
            }
            _ => {
                let _ = m;
            }
        }
        if self.slicer_active
            && self.mode == 1
            && matches!(name, "Size -" | "Size +" | "Region <" | "Region >")
        {
            self.button("Autoset", m);
        }
    }
    pub fn menu(&self, t: &Target) -> MenuDef<Cmd> {
        let mut menu = MenuDef::new(MenuStyle::Context);
        match t {
            Target::Pad(_) => {
                for (name, cmd) in [
                    ("Rename", Cmd::Rename),
                    ("Favorite / lock", Cmd::Favorite),
                    ("Copy pad", Cmd::Copy),
                    ("Paste pad", Cmd::Paste),
                    ("Delete", Cmd::Delete),
                ] {
                    menu.add(MenuItem::item(name), cmd);
                }
            }
            Target::Param(k) => {
                menu.add(MenuItem::item("Reset"), Cmd::Reset(*k));
                if *k == SOURCE_BPM
                    || *k == TARGET_BPM
                    || (*k >= GLOBAL && (*k - GLOBAL) % STRIDE == SPEED)
                {
                    menu.add(MenuItem::item("Half speed / tempo"), Cmd::Scale(*k, 0.5));
                    menu.add(MenuItem::item("Double speed / tempo"), Cmd::Scale(*k, 2.));
                }
                if *k == HOLD {
                    menu.add(
                        MenuItem::item(if self.params.document().trigger_decay {
                            "Use legacy end-of-cue release"
                        } else {
                            "Use attack/release Trigger envelope"
                        }),
                        Cmd::TriggerEnvelope,
                    );
                }
                menu.add(MenuItem::item("Create automation clip"), Cmd::Automate(*k));
            }
            _ => {}
        }
        menu
    }
    pub fn pick(&mut self, t: &Target, cmd: Cmd, x: f32, y: f32) -> Vec<Act> {
        if let Target::Pad(n) = t {
            let mut d = (*self.params.document()).clone();
            d.selected = *n;
            self.params.set_doc(d);
            self.selection = 1 << n;
        }
        let n = self.params.document().selected;
        match cmd {
            Cmd::Scale(k, factor) => {
                self.remember();
                self.edit_param(k, self.control_value(k) * factor);
            }
            Cmd::TriggerEnvelope => {
                self.remember();
                let mut d = (*self.params.document()).clone();
                d.trigger_decay = !d.trigger_decay;
                self.params.set_doc(d);
                self.touch();
            }
            Cmd::Rename => return vec![Act::TypeIn("name".into(), x, y)],
            Cmd::Favorite => self.button("Favorite", Mods::default()),
            Cmd::Delete => self.button("Delete", Mods::default()),
            Cmd::Copy => {
                self.copy = Some((
                    self.params.document().cues[n].clone(),
                    std::array::from_fn(|k| self.params.v(pad(n, k))),
                ))
            }
            Cmd::Paste => {
                if let Some((cue, values)) = self.copy.clone() {
                    self.remember();
                    let mut d = (*self.params.document()).clone();
                    d.cues[n] = cue;
                    self.params.set_doc(d);
                    for k in 0..STRIDE {
                        self.set(pad(n, k), values[k]);
                    }
                    self.touch();
                }
            }
            Cmd::Reset(k) => {
                self.remember();
                self.edit_param(k, self.params.defs[k].default);
            }
            Cmd::Automate(i) => return vec![Act::Automate(i)],
            Cmd::None => {}
        }
        vec![Act::Repaint]
    }
    pub fn value_text(&self, id: &str) -> String {
        if id == "name" {
            let d = self.params.document();
            d.cues[d.selected].name.clone()
        } else {
            self.vals.text(id)
        }
    }
    pub fn typed(&mut self, id: &str, text: &str) {
        self.remember();
        if id == "name" {
            let mut d = (*self.params.document()).clone();
            d.cues[d.selected].name = text.chars().take(64).collect();
            self.params.set_doc(d);
            self.touch();
        } else if let Some(v) = self.vals.parse(id, text) {
            if let Some(k) = self.params.defs.iter().position(|d| d.id == id) {
                // Parsing returns normalized units; group edits require plain units.
                let plain = unsafe { self.params.p[k].as_ptr().preview_plain(v) };
                self.edit_param(k, plain);
            }
        }
    }
}
#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicIsize, Ordering};

    use nih_plug::prelude::GuiContext;
    use os_plugkit::win::{self as pw, CursorLock};
    use os_theme::Palette;
    use os_ui::Canvas;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::{Act, Btn, Logic, Mods};
    use crate::Shared;
    use crate::params::ChopParams;
    use crate::view;

    const TIMER: usize = 1;
    const WM_CHANGED: u32 = WM_APP + 1;

    struct Win {
        hwnd: HWND,
        canvas: Canvas,
        buf: Vec<u32>,
        scale: f32,
        /// Painted size in device px.
        size: (u32, u32),
        logic: Logic,
        palette: Palette,
        last_hint: String,
        lock: CursorLock,
        tracking: bool,
        slot: Arc<AtomicIsize>,
    }

    /// Owns the window; dropping it closes the editor.
    pub struct Handle(isize, Arc<AtomicIsize>, Arc<Shared>);

    // SAFETY: the handle is only an HWND value; the wrapper drops it on the GUI thread.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            self.1.store(0, Ordering::Release);
            self.2.editor_open.store(false, Ordering::Relaxed);
            unsafe { DestroyWindow(self.0 as HWND) };
        }
    }

    pub fn post_changed(hwnd: isize) {
        if hwnd != 0 {
            unsafe { PostMessageW(hwnd as HWND, WM_CHANGED, 0, 0) };
        }
    }

    fn mods() -> Mods {
        let (ctrl, shift, alt) = pw::mods();
        Mods { ctrl, shift, alt }
    }

    pub fn open(
        parent: *mut c_void,
        scale: f32,
        params: Arc<ChopParams>,
        shared: Arc<Shared>,
        ctx: Arc<dyn GuiContext>,
        slot: Arc<AtomicIsize>,
    ) -> Handle {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let px = |v: u32| (v as f32 * scale).round() as u32;
        let size = (px(view::W), px(view::H));
        shared.editor_open.store(true, Ordering::Relaxed);
        unsafe {
            let class = pw::register("LemonChopView", Some(wndproc));
            let win = Box::new(Win {
                hwnd: std::ptr::null_mut(),
                canvas: Canvas::new(size.0, size.1, scale).expect("canvas"),
                buf: vec![0; (size.0 * size.1) as usize],
                scale,
                size,
                logic: Logic::new(params, shared.clone(), Some(ctx)),
                palette: os_plugkit::palette(),
                last_hint: String::new(),
                lock: CursorLock::default(),
                tracking: false,
                slot: slot.clone(),
            });
            let title = pw::wide("Lemon Chop");
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                parent as HWND,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                Box::into_raw(win) as *const c_void,
            );
            slot.store(hwnd as isize, Ordering::Release);
            // About 33 checks a second (edits the host confirms, the hint).
            windows_sys::Win32::UI::Shell::DragAcceptFiles(hwnd, 1);
            SetTimer(hwnd, TIMER, 30, None);
            Handle(hwnd as isize, slot, shared)
        }
    }

    unsafe fn state(hwnd: HWND) -> Option<&'static mut Win> {
        let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Win;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *p })
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            if msg == WM_NCCREATE {
                let cs = lp as *const CREATESTRUCTW;
                let w = (*cs).lpCreateParams as *mut Win;
                (*w).hwnd = hwnd;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, w as isize);
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            if msg == WM_NCDESTROY {
                let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Win;
                if !p.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    (*p).slot.store(0, Ordering::Release);
                    drop(Box::from_raw(p));
                }
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let Some(w) = state(hwnd) else {
                return DefWindowProcW(hwnd, msg, wp, lp);
            };
            if msg == os_hostlink::message() {
                return w.host_link(wp, lp);
            }
            match msg {
                WM_PAINT => {
                    w.paint();
                    0
                }
                WM_DROPFILES => {
                    use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW};
                    let drop = wp as _;
                    let count = DragQueryFileW(drop, 0, std::ptr::null_mut(), 0);
                    let mut path = vec![0u16; count as usize + 1];
                    DragQueryFileW(drop, 0, path.as_mut_ptr(), path.len() as u32);
                    DragFinish(drop);
                    w.logic
                        .load(String::from_utf16_lossy(&path[..count as usize]));
                    w.invalidate();
                    0
                }
                WM_KEYDOWN | WM_KEYUP => {
                    if msg == WM_KEYUP || lp & (1 << 30) == 0 {
                        w.logic.key(wp as u32, msg == WM_KEYDOWN);
                    }
                    w.invalidate();
                    0
                }
                WM_KILLFOCUS => {
                    w.logic.release_keys();
                    0
                }
                WM_ERASEBKGND => 1,
                WM_TIMER | WM_CHANGED => {
                    w.poll();
                    0
                }
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK
                | WM_MBUTTONDOWN => {
                    SetFocus(hwnd);
                    let (x, y) = w.mouse(lp);
                    let btn = match msg {
                        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => Btn::Right,
                        WM_MBUTTONDOWN => Btn::Middle,
                        _ => Btn::Left,
                    };
                    let acts = w.logic.down(x, y, btn, mods(), msg == WM_LBUTTONDBLCLK);
                    if w.logic.dragging() && !acts.contains(&Act::Lock) {
                        SetCapture(hwnd);
                    }
                    w.run(acts);
                    0
                }
                WM_MOUSEMOVE => {
                    if !w.tracking {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        TrackMouseEvent(&mut tme);
                        w.tracking = true;
                    }
                    if let Some((dx, dy)) = w.lock.delta() {
                        if dx != 0 || dy != 0 {
                            let acts =
                                w.logic
                                    .delta(dx as f32 / w.scale, dy as f32 / w.scale, mods());
                            w.run(acts);
                        }
                        return 0;
                    }
                    let (x, y) = w.mouse(lp);
                    let acts = w.logic.moved(x, y, mods());
                    w.run(acts);
                    0
                }
                WM_MOUSELEAVE => {
                    w.tracking = false;
                    if !w.lock.locked() && !w.logic.dragging() {
                        let acts = w.logic.leave();
                        w.run(acts);
                    }
                    0
                }
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                    w.lock.unlock();
                    ReleaseCapture();
                    let acts = w.logic.up(Btn::Left);
                    w.run(acts);
                    0
                }
                WM_CAPTURECHANGED => {
                    // Another window took the mouse (not this one capturing again): end the drag.
                    if w.logic.dragging() && lp as HWND != hwnd {
                        w.lock.unlock();
                        let acts = w.logic.up(Btn::Left);
                        w.run(acts);
                    }
                    0
                }
                WM_MOUSEWHEEL => {
                    // Wheel positions are screen coordinates.
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut r);
                    let (sx, sy) = pw::xy(lp);
                    let (x, y) = (
                        (sx - r.left) as f32 / w.scale,
                        (sy - r.top) as f32 / w.scale,
                    );
                    let notches = ((wp >> 16) & 0xffff) as u16 as i16 as f32 / 120.0;
                    let acts = w.logic.wheel(x, y, notches, mods());
                    w.run(acts);
                    0
                }
                WM_DESTROY => {
                    KillTimer(hwnd, TIMER);
                    w.lock.unlock();
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }

    impl Win {
        fn mouse(&self, l: LPARAM) -> (f32, f32) {
            let (x, y) = pw::xy(l);
            (x as f32 / self.scale, y as f32 / self.scale)
        }

        fn screen(&self, x: f32, y: f32) -> (i32, i32) {
            pw::to_screen(self.hwnd, (x * self.scale) as i32, (y * self.scale) as i32)
        }

        fn invalidate(&self) {
            unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
        }

        /// Timer / parameter change: expire confirmed overlay values and repaint.
        fn poll(&mut self) {
            self.logic.poll();
            self.invalidate();
            if self.logic.hint != self.last_hint {
                self.last_hint = self.logic.hint.clone();
                if self.logic.hosted {
                    os_hostlink::to_host(self.hwnd as isize, os_hostlink::Cmd::Changed, 0);
                }
            }
        }

        fn run(&mut self, acts: Vec<Act>) {
            let mut repaint = false;
            for a in acts {
                match a {
                    Act::Repaint => repaint = true,
                    Act::Lock => self.lock.lock(self.hwnd),
                    Act::Menu(req, x, y) => {
                        let def = self.logic.menu(&req);
                        let (sx, sy) = self.screen(x, y);
                        if let Some(cmd) =
                            pw::popup(self.hwnd, sx, sy, &def, &self.palette, self.scale)
                        {
                            let acts = self.logic.pick(&req, cmd, x, y);
                            self.run(acts);
                        }
                        repaint = true;
                    }
                    Act::TypeIn(id, x, y) => {
                        let (sx, sy) = self.screen(x, y);
                        let cur = self.logic.value_text(&id);
                        if let Some(t) = pw::type_in(self.hwnd, sx, sy, &cur, self.scale) {
                            self.logic.typed(&id, &t);
                        }
                        repaint = true;
                    }
                    Act::Automate(i) => {
                        os_hostlink::to_host(
                            self.hwnd as isize,
                            os_hostlink::Cmd::Automate,
                            i as isize,
                        );
                    }
                }
            }
            if repaint {
                self.invalidate();
            }
        }

        /// Answers the host (spec `plugins/host-link.md`): one page, the hint line. Unknown commands return 0.
        unsafe fn host_link(&mut self, wp: WPARAM, lp: LPARAM) -> LRESULT {
            use os_hostlink::{Cmd, PageIcon};
            let Some(cmd) = Cmd::from_usize(wp) else {
                return 0;
            };
            match cmd {
                Cmd::Hello => {
                    self.logic.hosted = true;
                    self.invalidate();
                    os_hostlink::VERSION as LRESULT
                }
                Cmd::PageCount => 1,
                Cmd::PageIcon => PageIcon::Plugin as LRESULT,
                Cmd::GetPage | Cmd::SetPage => 0,
                Cmd::Metrics => os_hostlink::pack_size(view::W, view::H),
                Cmd::Hint => {
                    if lp != 0 {
                        // SAFETY: the host passes a `HintBuf` of its own process (spec: host-link.md).
                        unsafe { &mut *(lp as *mut os_hostlink::HintBuf) }.set(&self.logic.hint);
                    }
                    1
                }
                Cmd::Changed
                | Cmd::Note
                | Cmd::Automate
                | Cmd::Audio
                | Cmd::Resizable
                | Cmd::BrowseSample
                | Cmd::SetSize => 0,
            }
        }

        fn paint(&mut self) {
            let (hdc, ps) = pw::begin_paint(self.hwnd);
            self.logic.paint(&mut self.canvas, &self.palette);
            self.buf.resize((self.size.0 * self.size.1) as usize, 0);
            self.canvas.copy_to_xrgb(&mut self.buf);
            pw::blit(hdc, &self.buf, self.size.0, self.size.1);
            pw::end_paint(self.hwnd, &ps);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod gui {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use nih_plug::prelude::GuiContext;
    use os_hostlink::{Cmd, PageIcon, Peer};
    use os_plugkit::gui::{self as px, Button, Input, Window};
    use os_theme::Palette;
    use os_ui::Canvas;

    use super::{Act, Btn, Logic, Mods};
    use crate::Shared;
    use crate::params::ChopParams;
    use crate::view;

    /// The editor on its GUI thread.
    struct Ed {
        win: Window,
        canvas: Canvas,
        buf: Vec<u32>,
        scale: f32,
        size: (u32, u32),
        logic: Logic,
        palette: Palette,
        last_hint: String,
        dirty: bool,
        /// The host link: the host finds the editor by the window it gave it (spec: plugins/host-link.md).
        link: Arc<Peer>,
    }

    impl Drop for Ed {
        /// The editor closes: the host no longer finds it.
        fn drop(&mut self) {
            self.link.unregister();
        }
    }

    /// Owns the editor thread; dropping it closes the editor.
    pub struct Handle(Option<px::EditorThread>, Arc<Shared>);

    impl Drop for Handle {
        fn drop(&mut self) {
            self.0.take();
            self.1.editor_open.store(false, Ordering::Relaxed);
        }
    }

    pub fn open(
        parent: px::Parent,
        scale: f32,
        params: Arc<ChopParams>,
        shared: Arc<Shared>,
        ctx: Arc<dyn GuiContext>,
        slot: &px::Slot,
    ) -> Handle {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let size = (
            (view::W as f32 * scale).round() as u32,
            (view::H as f32 * scale).round() as u32,
        );
        let sh = shared.clone();
        // One page; joined, the editor keeps its size (it has no strip row of its own).
        let wake = slot.clone();
        let link = Peer::register(
            parent as u64,
            vec![PageIcon::Plugin],
            (view::W, view::H),
            move || px::notify(&wake),
        );
        let peer = link.clone();
        let thread = px::spawn(parent, size, slot, move |mut win| {
            win.key_releases();
            #[cfg(target_os = "macos")]
            win.accept_files();
            let Ok(canvas) = Canvas::new(size.0, size.1, scale) else {
                return;
            };
            let logic = Logic::new(params, shared, Some(ctx));
            let ed = Ed {
                win,
                canvas,
                buf: Vec::new(),
                scale,
                size,
                logic,
                palette: os_plugkit::palette(),
                last_hint: String::new(),
                dirty: true,
                link: peer,
            };
            ed.run();
        });
        if thread.is_none() {
            link.unregister();
        }
        sh.editor_open.store(thread.is_some(), Ordering::Relaxed);
        Handle(thread, sh)
    }

    fn btn(b: Button) -> Btn {
        match b {
            Button::Left => Btn::Left,
            Button::Right => Btn::Right,
            Button::Middle => Btn::Middle,
        }
    }

    impl Ed {
        fn run(mut self) {
            while let Some(input) = self.win.next() {
                match input {
                    Input::Expose => self.dirty = true,
                    Input::Tick => self.poll(),
                    Input::Down { x, y, btn: b, dbl } => {
                        let (x, y) = (x / self.scale, y / self.scale);
                        let acts =
                            self.logic
                                .down(x, y, btn(b), self.mods(), dbl && b == Button::Left);
                        self.act(acts);
                    }
                    Input::Move { x, y } => {
                        let acts = self
                            .logic
                            .moved(x / self.scale, y / self.scale, self.mods());
                        self.act(acts);
                    }
                    Input::Delta { dx, dy } => {
                        let acts = self
                            .logic
                            .delta(dx / self.scale, dy / self.scale, self.mods());
                        self.act(acts);
                    }
                    Input::Up { .. } => {
                        self.win.unlock();
                        let acts = self.logic.up(Btn::Left);
                        self.act(acts);
                    }
                    Input::Leave => {
                        if !self.win.locked() && !self.logic.dragging() {
                            let acts = self.logic.leave();
                            self.act(acts);
                        }
                    }
                    Input::Wheel { x, y, notches } => {
                        let acts =
                            self.logic
                                .wheel(x / self.scale, y / self.scale, notches, self.mods());
                        self.act(acts);
                    }
                    Input::Drop { path } => {
                        self.logic.load(path);
                        self.dirty = true;
                    }
                    Input::Key { sym, pressed, .. } => {
                        self.logic.key(sym, pressed);
                        self.dirty = true;
                    }
                }
                if self.dirty && self.win.idle() {
                    self.paint();
                }
            }
        }

        fn mods(&self) -> Mods {
            let (ctrl, shift, alt) = self.win.mods();
            Mods { ctrl, shift, alt }
        }

        fn screen(&self, x: f32, y: f32) -> (i32, i32) {
            self.win
                .to_root((x * self.scale) as i32, (y * self.scale) as i32)
        }

        /// Timer / parameter change: expire confirmed overlay values and repaint.
        fn poll(&mut self) {
            self.logic.poll();
            self.dirty = true;
            if self.logic.hint != self.last_hint {
                self.last_hint.clone_from(&self.logic.hint);
                // The host shows the hint in its title bar (hosted mode).
                self.link.set_hint(&self.logic.hint);
                self.link.to_host(Cmd::Changed, 0);
            }
            // Joined the strip: "Create automation clip" works and the host shows the hint.
            self.logic.hosted = self.link.hosted();
        }

        fn act(&mut self, acts: Vec<Act>) {
            for a in acts {
                match a {
                    Act::Repaint => self.dirty = true,
                    Act::Lock => self.win.lock(),
                    Act::Menu(req, x, y) => {
                        let def = self.logic.menu(&req);
                        let (sx, sy) = self.screen(x, y);
                        if let Some(cmd) = self.win.popup(sx, sy, &def, &self.palette, self.scale) {
                            let acts = self.logic.pick(&req, cmd, x, y);
                            self.act(acts);
                        }
                        self.dirty = true;
                    }
                    Act::TypeIn(id, x, y) => {
                        let (sx, sy) = self.screen(x, y);
                        let cur = self.logic.value_text(&id);
                        if let Some(t) = self.win.type_in(sx, sy, &cur, self.scale) {
                            self.logic.typed(&id, &t);
                        }
                        self.dirty = true;
                    }
                    // The host makes the clip; the index is the parameter's list position.
                    Act::Automate(i) => {
                        self.link.to_host(Cmd::Automate, i as isize);
                    }
                }
            }
        }

        fn paint(&mut self) {
            self.dirty = false;
            self.logic.paint(&mut self.canvas, &self.palette);
            self.buf.resize((self.size.0 * self.size.1) as usize, 0);
            self.canvas.copy_to_xrgb(&mut self.buf);
            self.win.blit(&self.buf, self.size.0, self.size.1);
        }
    }
}

#[cfg(test)]
mod chop_tests {
    use super::*;
    #[test]
    fn redesigned_layout_targets_hover_and_cancelled_pad_drop() {
        let p = Arc::new(ChopParams::default());
        let sh = Arc::new(Shared::default());
        let mut l = Logic::new(p.clone(), sh, None);
        let mut canvas = os_ui::Canvas::new(view::W, view::H, 1.).unwrap();
        for grid in [false, true] {
            l.grid = grid;
            l.paint(&mut canvas, &os_plugkit::palette());
            for (r, target) in &l.hits {
                assert!(
                    r.x >= 0.
                        && r.y >= 0.
                        && r.x + r.w <= view::W as f32
                        && r.y + r.h <= view::H as f32,
                    "{target:?}"
                );
                assert_eq!(
                    l.hit(r.cx(), r.cy()),
                    Some(target.clone()),
                    "covered control: {target:?}"
                );
            }
        }
        let (r, _) = l
            .hits
            .iter()
            .find(|(_, t)| *t == Target::Pad(0))
            .unwrap()
            .clone();
        l.moved(r.cx(), r.cy(), Mods::default());
        assert!(l.visual_state(&Target::Pad(0), false).hover);
        assert!(l.interaction_hint().contains("Ctrl-click"));
        l.leave();
        assert!(!l.visual_state(&Target::Pad(0), false).hover);
        l.drag = Some((Target::Pad(0), r.cx(), r.cy()));
        let (dest, _) = l
            .hits
            .iter()
            .find(|(_, t)| *t == Target::Pad(1))
            .unwrap()
            .clone();
        l.moved(dest.cx(), dest.cy(), Mods::default());
        assert_eq!(l.pad_drop_target(), Some(1));
        l.moved(0., 0., Mods::default());
        assert_eq!(l.pad_drop_target(), None);
        let before = (*p.document()).clone();
        l.up(Btn::Left);
        assert_eq!(*p.document(), before);
    }

    #[test]
    fn pitch_drag_accumulates_small_movements_and_reverses_at_limits() {
        let p = Arc::new(ChopParams::default());
        let mut l = Logic::new(p.clone(), Arc::new(Shared::default()), None);
        let normal = Mods::default();
        let fine = Mods {
            ctrl: true,
            ..Default::default()
        };
        for k in [pad(0, PITCH), KEY] {
            l.set(k, 0.);
            l.hits = vec![(Rect::new(0., 0., 20., 20.), Target::Param(k))];
            l.down(10., 10., Btn::Left, normal, false);
            let mut last = 0.;
            for _ in 0..25 {
                l.delta(0., -1., normal);
                let v = p.v(k);
                assert!(v >= last && v - last <= 1.);
                last = v;
            }
            assert_eq!(p.v(k), 6.);
            l.up(Btn::Left);
            l.set(k, 0.);
            l.down(10., 10., Btn::Left, normal, false);
            l.delta(0., -25., normal);
            assert_eq!(p.v(k), 6., "event batching must not change pitch travel");
            for _ in 0..250 {
                l.delta(0., 1., fine);
            }
            assert_eq!(p.v(k), 0.);
            l.delta(0., -1000., normal);
            assert_eq!(p.v(k), 24.);
            for _ in 0..5 {
                l.delta(0., 1., normal);
            }
            assert_eq!(p.v(k), 23., "no accumulated overshoot at the limit");
            l.up(Btn::Left);
        }
        l.set(pad(0, PITCH), 0.);
        l.button("All", normal);
        l.hits = vec![(Rect::new(0., 0., 20., 20.), Target::Param(pad(0, PITCH)))];
        l.down(10., 10., Btn::Left, normal, false);
        for _ in 0..25 {
            l.delta(0., -1., normal);
        }
        l.up(Btn::Left);
        assert!((0..32).all(|n| p.v(pad(n, PITCH)) == 6.));
        l.button("Undo", normal);
        assert!((0..32).all(|n| p.v(pad(n, PITCH)) == 0.));
    }
    #[test]
    fn group_edits_reopen_selection_and_source_undo() {
        let p = Arc::new(ChopParams::default());
        let sh = Arc::new(Shared::default());
        let mut d = Document::default();
        d.selected = 5;
        p.set_doc(d);
        let mut l = Logic::new(p.clone(), sh, None);
        assert_eq!(l.selection, 1 << 5);
        l.button("All", Mods::default());
        for (field, text, expected) in [
            (FILTER, "-0.5", -0.5),
            (ATTACK, "0.25", 0.25),
            (RELEASE, "0.4", 0.4),
            (PITCH, "7", 7.),
            (SPEED, "0.5", 0.5),
        ] {
            l.typed(&p.defs[pad(5, field)].id, text);
            assert!((0..32).all(|n| (p.v(pad(n, field)) - expected).abs() < 0.001));
        }
        let r = Rect::new(0., 0., 20., 20.);
        l.hits = vec![(r, Target::Param(pad(5, REVERSE)))];
        l.down(10., 10., Btn::Left, Mods::default(), false);
        assert!((0..32).all(|n| p.v(pad(n, REVERSE)) == 1.));
        l.pick(
            &Target::Param(pad(5, PITCH)),
            Cmd::Reset(pad(5, PITCH)),
            0.,
            0.,
        );
        assert!((0..32).all(|n| p.v(pad(n, PITCH)) == 0.));
        l.button("Undo", Mods::default());
        assert!((0..32).all(|n| p.v(pad(n, PITCH)) == 7.));
        l.selection = (1 << 5) | (1 << 7);
        l.typed(&p.defs[pad(5, FILTER)].id, "0.5");
        assert_eq!(p.v(pad(4, FILTER)), -0.5);
        assert_eq!(p.v(pad(7, FILTER)), 0.5);
        l.hits = vec![(r, Target::Pad(5))];
        l.down(
            10.,
            10.,
            Btn::Left,
            Mods {
                ctrl: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(p.document().selected, 7);
        l.button("Grid", Mods::default());
        l.load("new-source.wav".into());
        assert_eq!(l.selection, 1);
        assert!(!l.grid);
        l.button("Undo", Mods::default());
        assert!(!l.auto_analyse);
        assert_eq!(l.selection, 1 << 7);
        assert_eq!(p.v(pad(7, FILTER)), 0.5);
        p.set_doc(Document {
            path: "host-drop.wav".into(),
            selected: 3,
            ..Default::default()
        });
        l.poll();
        assert_eq!(l.selection, 1 << 3);
        assert!(!l.grid);
    }
    #[test]
    fn chop_editor_state_gestures_favorites_and_undo() {
        let p = Arc::new(ChopParams::default());
        let shared = Arc::new(Shared::default());
        let mut l = Logic::new(p.clone(), shared, None);
        let mut d = Document::default();
        d.cues[0] = Cue {
            start: 0.,
            end: 0.5,
            enabled: true,
            name: "Kick".into(),
            ..Default::default()
        };
        p.set_doc(d);
        let mut c = os_ui::Canvas::new(view::W, view::H, 1.).unwrap();
        l.paint(&mut c, &os_plugkit::palette());
        assert_eq!(
            l.hits
                .iter()
                .filter(|(_, t)| matches!(t, Target::Pad(_)))
                .count(),
            32
        );
        for (r, _) in &l.hits {
            assert!(
                r.x >= 0.
                    && r.y >= 0.
                    && r.x + r.w <= view::W as f32
                    && r.y + r.h <= view::H as f32
            );
        }
        let center = |l: &Logic, t: Target| {
            let r = l.hits.iter().find(|(_, v)| *v == t).unwrap().0;
            (r.cx(), r.cy())
        };
        let (x, y) = center(&l, Target::Pad(0));
        l.down(x, y, Btn::Left, Mods::default(), false);
        let (x, y) = center(&l, Target::Pad(5));
        l.moved(x, y, Mods::default());
        l.up(Btn::Left);
        assert_eq!(p.document().cues[5].name, "Kick");
        assert!(!p.document().cues[0].enabled);
        l.button("Undo", Mods::default());
        assert_eq!(p.document().cues[0].name, "Kick");
        l.button("Redo", Mods::default());
        assert_eq!(p.document().cues[5].name, "Kick");
        l.typed("name", "Snare");
        assert_eq!(p.document().cues[5].name, "Snare");
        l.button("Favorite", Mods::default());
        assert!(p.document().cues[5].favorite);
        let mut d = (*p.document()).clone();
        let saved = d.cues[5].clone();
        d.autoset(2, 10., 120., 0., 1., &[], 44);
        assert_eq!(d.cues[5], saved);
        assert!(d.cues.iter().filter(|x| x.enabled).count() >= 31);
        p.set_doc(d);
        let q = ChopParams::default();
        q.deserialize_fields(&p.serialize_fields());
        assert_eq!(*q.document(), *p.document());
        l.selection = u32::MAX;
        l.edit_param(pad(5, LEVEL), -6.);
        assert!((0..32).all(|n| p.v(pad(n, LEVEL)) == -6.));
        l.key(b'Z' as u32, true);
        l.key(b'Z' as u32, true);
        l.key(b'Z' as u32, false);
        assert!(!l.keys[0]);
    }
}
