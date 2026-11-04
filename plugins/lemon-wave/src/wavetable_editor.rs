// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    assets::WaveParams,
    table_tools as tools,
    wavetable::{MAX_FRAMES, SIZE, Source},
};
use os_ui::{Canvas, Font, Rect};
pub const DRAW: Rect = Rect::new(32., 158., 720., 164.);
pub const HARM: Rect = Rect::new(32., 362., 720., 126.);
pub const SAMPLE: Rect = Rect::new(32., 560., 720., 102.);
pub const STACK: Rect = Rect::new(790., 158., 356., 292.);
pub const PREVIEW: Rect = Rect::new(790., 470., 356., 18.);
const BANK: Rect = Rect::new(790., 560., 356., 30.);
const NAME: Rect = Rect::new(790., 614., 356., 30.);
fn top(i: usize) -> Rect {
    Rect::new(
        [32., 122., 218., 348., 528., 624.][i],
        94.,
        [80., 80., 114., 164., 80., 80.][i],
        30.,
    )
}
fn op(i: usize) -> Rect {
    Rect::new(32. + i as f32 * 112., 510., 104., 28.)
}
fn thumb(i: usize) -> Rect {
    Rect::new(32. + i as f32 * 70., 726., 64., 50.)
}
fn convert(i: usize) -> Rect {
    Rect::new(
        [32., 154., 226., 298., 440., 552.][i],
        678.,
        [112., 64., 64., 134., 104., 200.][i],
        28.,
    )
}
#[derive(Clone)]
struct Snapshot {
    source: Source,
    selected: usize,
}
#[derive(Clone, Copy)]
enum Drag {
    Wave((f32, f32)),
    Harm(usize),
    Selection(f32),
    Preview,
}
pub struct Workbench {
    pub oscillator: usize,
    pub source: Source,
    pub dirty: bool,
    pub selected: usize,
    sample: Option<crate::sample::Sample>,
    selection: (f32, f32),
    count: usize,
    preview: f32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    drag: Option<Drag>,
    pub bank: String,
    pub name: String,
    text: Option<bool>,
    text_before: String,
    replace_text: bool,
    pub status: String,
}
impl Default for Workbench {
    fn default() -> Self {
        Self {
            oscillator: 0,
            source: crate::sound_browser::basic(0),
            dirty: false,
            selected: 0,
            sample: None,
            selection: (0.1, 0.5),
            count: 16,
            preview: 0.,
            undo: vec![],
            redo: vec![],
            drag: None,
            bank: "My Tables".into(),
            name: "New Table".into(),
            text: None,
            text_before: String::new(),
            replace_text: false,
            status: "Draft editor: Apply to Oscillator commits your work.".into(),
        }
    }
}
impl Workbench {
    pub fn load(&mut self, p: &WaveParams, o: usize) {
        let sources = p.sources();
        self.oscillator = o;
        self.sample = sources.samples[o].clone();
        self.source = if let Some(s) = sources.oscillators[o].clone() {
            s
        } else if p.base.v(if o == 0 {
            crate::controls::A_SOURCE
        } else {
            crate::controls::B_SOURCE
        }) == 1
        {
            crate::sound_browser::basic(1)
        } else {
            crate::dsp::editable_bank(p.base.v(o * 8) as usize)
        };
        self.name = tools::suggested_name(&self.source.name);
        self.selected = 0;
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.drag = None;
        self.text = None;
        self.preview = p.base.v(o * 8 + 1) as f32 / 1000.;
        self.status =
            "Loaded oscillator. Draw, arrange frames or edit harmonics; Apply commits the draft."
                .into();
    }
    fn frame(&self) -> &[f32] {
        &self.source.samples[self.selected * SIZE..(self.selected + 1) * SIZE]
    }
    fn frame_mut(&mut self) -> &mut [f32] {
        &mut self.source.samples[self.selected * SIZE..(self.selected + 1) * SIZE]
    }
    fn checkpoint(&mut self) {
        if self.undo.len() == 24 {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            source: self.source.clone(),
            selected: self.selected,
        });
        self.redo.clear();
        self.dirty = true;
    }
    fn history(&mut self, redo: bool) {
        let item = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(s) = item {
            let current = Snapshot {
                source: self.source.clone(),
                selected: self.selected,
            };
            if redo {
                self.undo.push(current)
            } else {
                self.redo.push(current)
            }
            self.source = s.source;
            self.selected = s.selected;
            self.dirty = true;
        }
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    fn point(x: f32, y: f32) -> (f32, f32) {
        (
            ((x - DRAW.x) / DRAW.w).clamp(0., 1.),
            (1. - 2. * (y - DRAW.y) / DRAW.h).clamp(-1., 1.),
        )
    }
    pub fn moved(&mut self, x: f32, y: f32) -> bool {
        match self.drag {
            Some(Drag::Wave(last)) => {
                let next = Self::point(x, y);
                tools::draw(self.frame_mut(), last, next);
                self.drag = Some(Drag::Wave(next));
            }
            Some(Drag::Harm(last)) => {
                let h = (((x - HARM.x) / HARM.w * 64.).floor() as usize).min(63) + 1;
                let a = (1. - (y - HARM.y) / HARM.h).clamp(0., 1.);
                for n in last.min(h)..=last.max(h) {
                    tools::harmonic(self.frame_mut(), n, a);
                }
                self.drag = Some(Drag::Harm(h));
            }
            Some(Drag::Selection(start)) => {
                self.selection = (start, ((x - SAMPLE.x) / SAMPLE.w).clamp(0., 1.));
            }
            Some(Drag::Preview) => self.preview = ((x - PREVIEW.x) / PREVIEW.w).clamp(0., 1.),
            None => return false,
        }
        true
    }
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }
    pub fn character(&mut self, c: char) -> bool {
        let Some(bank) = self.text else { return false };
        let field = if bank { &mut self.bank } else { &mut self.name };
        match c {
            '\u{1}' => self.replace_text = true,
            '\u{8}' => {
                if self.replace_text {
                    field.clear();
                    self.replace_text = false;
                } else {
                    field.pop();
                }
                if !bank {
                    self.dirty = true;
                }
            }
            '\r' | '\n' => self.text = None,
            '\u{1b}' => {
                *field = self.text_before.clone();
                self.text = None;
                self.replace_text = false;
            }
            c if !c.is_control() => {
                if self.replace_text {
                    field.clear();
                    self.replace_text = false;
                }
                if field.chars().count() < 64 {
                    field.push(c);
                    if !bank {
                        self.dirty = true;
                    }
                }
            }
            _ => {}
        }
        true
    }
    /// Returns a validated source only for an explicit Apply click.
    pub fn click(&mut self, x: f32, y: f32, p: &WaveParams) -> Option<Source> {
        if BANK.contains(x, y) || NAME.contains(x, y) {
            let bank = BANK.contains(x, y);
            self.text = Some(bank);
            self.replace_text = false;
            self.text_before = if bank {
                self.bank.clone()
            } else {
                self.name.clone()
            };
            return None;
        }
        self.text = None;
        for i in 0..6 {
            if top(i).contains(x, y) {
                match i {
                    0 | 1 => {
                        if self.dirty {
                            self.status =
                                "Apply or Reload the current draft before switching oscillators."
                                    .into();
                        } else {
                            self.load(p, i);
                        }
                    }
                    2 => self.load(p, self.oscillator),
                    3 => {
                        self.source.name = self.name.trim().chars().take(64).collect();
                        if self.source.name.is_empty() {
                            self.status = "Enter a table name before applying.".into();
                        } else if let Err(e) = self.source.validate() {
                            self.status = e;
                        } else {
                            self.dirty = false;
                            self.status="Applied. Synth Position scans these frames. Edits restart held notes.".into();
                            return Some(self.source.clone());
                        }
                    }
                    4 => self.selected = self.selected.saturating_sub(1),
                    5 => self.selected = (self.selected + 1).min(self.source.frames() - 1),
                    _ => {}
                }
                return None;
            }
        }
        if DRAW.contains(x, y) {
            self.checkpoint();
            let point = Self::point(x, y);
            tools::draw(self.frame_mut(), point, point);
            self.drag = Some(Drag::Wave(point));
            return None;
        }
        if HARM.contains(x, y) {
            self.checkpoint();
            let h = (((x - HARM.x) / HARM.w * 64.) as usize).min(63) + 1;
            self.drag = Some(Drag::Harm(h));
            self.moved(x, y);
            return None;
        }
        if SAMPLE.contains(x, y) && self.sample.is_some() {
            let a = ((x - SAMPLE.x) / SAMPLE.w).clamp(0., 1.);
            self.selection = (a, a);
            self.drag = Some(Drag::Selection(a));
            return None;
        }
        if PREVIEW.contains(x, y) {
            self.drag = Some(Drag::Preview);
            self.moved(x, y);
            return None;
        }
        for i in 0..10 {
            if op(i).contains(x, y) {
                match i {
                    0 | 1 | 5 if self.source.frames() >= MAX_FRAMES => {
                        self.status = "Maximum 128 frames.".into()
                    }
                    0 => {
                        self.checkpoint();
                        let at = (self.selected + 1) * SIZE;
                        self.source.samples.splice(at..at, vec![0.; SIZE]);
                        self.selected += 1;
                    }
                    1 => {
                        self.checkpoint();
                        let copy = self.frame().to_vec();
                        let at = (self.selected + 1) * SIZE;
                        self.source.samples.splice(at..at, copy);
                        self.selected += 1;
                    }
                    2 if self.source.frames() > 1 => {
                        self.checkpoint();
                        let at = self.selected * SIZE;
                        self.source.samples.drain(at..at + SIZE);
                        self.selected = self.selected.min(self.source.frames() - 1);
                    }
                    3 if self.selected > 0 => {
                        self.checkpoint();
                        self.swap(self.selected - 1);
                    }
                    4 if self.selected + 1 < self.source.frames() => {
                        self.checkpoint();
                        self.swap(self.selected + 1);
                    }
                    5 if self.selected + 1 < self.source.frames() => {
                        self.checkpoint();
                        let at = (self.selected + 1) * SIZE;
                        let new: Vec<_> = (0..SIZE)
                            .map(|i| {
                                (self.source.samples[at - SIZE + i] + self.source.samples[at + i])
                                    * 0.5
                            })
                            .collect();
                        self.source.samples.splice(at..at, new);
                        self.selected += 1;
                    }
                    6 => self.history(false),
                    7 => self.history(true),
                    8 => {
                        self.checkpoint();
                        tools::normalize(self.frame_mut());
                    }
                    9 => {
                        self.checkpoint();
                        tools::smooth(self.frame_mut());
                    }
                    _ => {}
                }
                return None;
            }
        }
        for i in 0..16 {
            if thumb(i).contains(x, y) {
                let frame = self.selected / 16 * 16 + i;
                if frame < self.source.frames() {
                    self.selected = frame;
                }
                return None;
            }
        }
        if Rect::new(790., 678., 170., 28.).contains(x, y) {
            match tools::save_to(&tools::root(), &self.bank, &self.name, &self.source) {
                Ok(path) => {
                    self.status = format!(
                        "Saved {}. Find it in the oscillator Wavetables dropdown.",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    )
                }
                Err(e) => self.status = e,
            }
            return None;
        }
        for i in 0..6 {
            if convert(i).contains(x, y) {
                match i {
                    0 => {
                        if let Some(sample) = &self.sample {
                            match tools::extract(sample, self.selection, self.count) {
                                Ok(source) => {
                                    self.checkpoint();
                                    self.source = source;
                                    self.name = tools::suggested_name(&self.source.name);
                                    self.selected = 0;
                                    self.status="Recording converted into periodic frames. Apply to hear the wavetable.".into();
                                }
                                Err(e) => self.status = e,
                            }
                        } else {
                            self.status =
                                "Choose a recorded sound on Synth, then Reload here.".into();
                        }
                    }
                    1 => self.count = self.count.saturating_sub(1).max(1),
                    2 => self.count = (self.count + 1).min(MAX_FRAMES),
                    3 => {
                        self.checkpoint();
                        for x in self.frame_mut() {
                            *x = -*x;
                        }
                    }
                    4 => {
                        self.checkpoint();
                        self.frame_mut().reverse();
                    }
                    5 => {
                        self.preview = 0.5;
                        self.status="Draft morph preview centered. Apply, then use Position on Synth to hear it.".into();
                    }
                    _ => {}
                }
                return None;
            }
        }
        None
    }
    fn swap(&mut self, other: usize) {
        for i in 0..SIZE {
            self.source
                .samples
                .swap(self.selected * SIZE + i, other * SIZE + i);
        }
        self.selected = other;
    }
    pub fn paint(&self, c: &mut Canvas) {
        use crate::instrument_view::raised;
        let ink = crate::design::color(0xe9edf0);
        let dim = crate::design::color(0x9bacb9);
        let accent = crate::design::color(0xe3df8b);
        let cyan = crate::design::color(0x73dacc);
        let f = Font::light(crate::design::font_size(12), 0).natural();
        raised(
            c,
            Rect::new(16., 80., 1148., 708.),
            8.,
            crate::design::color(0x25303a),
        );
        let button = |c: &mut Canvas, r: Rect, label: &str, active: bool| {
            raised(
                c,
                r,
                4.,
                crate::design::color(if active { 0x4d5940 } else { 0x2e3b46 }),
            );
            c.text_center(f, r, if active { accent } else { ink }, label);
        };
        for (i, label) in [
            "OSC A",
            "OSC B",
            "Reload",
            "Apply to Oscillator",
            "< Frame",
            "Frame >",
        ]
        .iter()
        .enumerate()
        {
            button(
                c,
                top(i),
                label,
                i == self.oscillator || i == 3 && self.dirty,
            );
        }
        c.text(
            f,
            728.,
            103.,
            dim,
            &format!(
                "FRAME {} / {}    {}",
                self.selected + 1,
                self.source.frames(),
                if self.dirty {
                    "UNAPPLIED DRAFT"
                } else {
                    "DRAFT READY"
                }
            ),
        );
        for (r, label) in [
            (DRAW, "DRAW WAVEFORM"),
            (HARM, "HARMONICS 1 - 64 / drag bars"),
            (SAMPLE, "RECORDING / drag to select a section"),
            (STACK, "STACKED FRAMES / draft morph preview"),
        ] {
            c.text(f, r.x, r.y - 20., dim, label);
            c.fill_rect_gradient(r, crate::design::color(0x080e14), crate::design::color(0x14232c), 5.);
            c.stroke_rrect(r, 5., 1., crate::design::color(0x566a75));
        }
        for i in 1..8 {
            let x = DRAW.x + DRAW.w * i as f32 / 8.;
            c.polyline(&[(x, DRAW.y), (x, DRAW.bottom())], 1., crate::design::color(0x26333e));
        }
        c.polyline(
            &[(DRAW.x, DRAW.cy()), (DRAW.right(), DRAW.cy())],
            1.,
            crate::design::color(0x50616b),
        );
        let trace: Vec<_> = self
            .frame()
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    DRAW.x + DRAW.w * i as f32 / (SIZE - 1) as f32,
                    DRAW.cy() - v * DRAW.h * 0.48,
                )
            })
            .collect();
        c.polyline(&trace, 5., accent.with_alpha(22));
        c.polyline(&trace, 1.6, accent);
        for (i, a) in tools::amplitudes(self.frame()).iter().enumerate() {
            let w = HARM.w / 64.;
            let h = (a.clamp(0., 1.) * HARM.h).max(1.);
            c.fill_rect_gradient(
                Rect::new(HARM.x + i as f32 * w + 1., HARM.bottom() - h, w - 2., h),
                cyan,
                crate::design::color(0x234f51),
                1.,
            );
        }
        let frames = self.source.frames();
        let shown = frames.min(16);
        let mut indices: Vec<_> = (0..shown)
            .map(|j| {
                if shown == 1 {
                    0
                } else {
                    j * (frames - 1) / (shown - 1)
                }
            })
            .collect();
        if !indices.contains(&self.selected) {
            let at = indices
                .iter()
                .enumerate()
                .min_by_key(|(_, i)| i.abs_diff(self.selected))
                .unwrap()
                .0;
            indices[at] = self.selected;
            indices.sort_unstable();
        }
        for &frame in indices.iter().rev() {
            let depth = if frames == 1 {
                0.
            } else {
                frame as f32 / (frames - 1) as f32
            };
            let x = STACK.x + 12. + depth * 36.;
            let y = STACK.y + 50. + depth * 195.;
            let trace: Vec<_> = (0..160)
                .map(|i| {
                    let v = self.source.samples[frame * SIZE + i * (SIZE - 1) / 159];
                    (x + i as f32 * 1.8, y - v * 27.)
                })
                .collect();
            c.polyline(
                &trace,
                if frame == self.selected { 2. } else { 1. },
                if frame == self.selected {
                    accent
                } else {
                    cyan.with_alpha(80)
                },
            );
        }
        let pos = self.preview * (frames - 1) as f32;
        let a = pos.floor() as usize;
        let b = (a + 1).min(frames - 1);
        let trace: Vec<_> = (0..160)
            .map(|i| {
                let k = i * (SIZE - 1) / 159;
                let v = self.source.samples[a * SIZE + k] * (1. - pos.fract())
                    + self.source.samples[b * SIZE + k] * pos.fract();
                (
                    STACK.x + 12. + i as f32 * 1.8,
                    STACK.y + 50. + self.preview * 195. - v * 27.,
                )
            })
            .collect();
        c.polyline(&trace, 2., ink);
        raised(c, PREVIEW, 4., crate::design::color(0x16242d));
        c.fill_rrect(
            Rect::new(
                PREVIEW.x + self.preview * (PREVIEW.w - 8.),
                PREVIEW.y,
                8.,
                PREVIEW.h,
            ),
            3.,
            accent,
        );
        for (i, label) in [
            "+ Blank",
            "Duplicate",
            "Delete",
            "Move left",
            "Move right",
            "Tween",
            "Undo",
            "Redo",
            "Normalize",
            "Smooth",
        ]
        .iter()
        .enumerate()
        {
            button(c, op(i), label, false);
        }
        if let Some(s) = &self.sample {
            let trace: Vec<_> = (0..720)
                .map(|i| {
                    let at = i * (s.samples.len() - 1) / 719;
                    (
                        SAMPLE.x + i as f32,
                        SAMPLE.cy() - s.samples[at] * SAMPLE.h * 0.45,
                    )
                })
                .collect();
            c.polyline(&trace, 1., cyan);
            let lo = self.selection.0.min(self.selection.1);
            let hi = self.selection.0.max(self.selection.1);
            c.fill_rect(
                Rect::new(
                    SAMPLE.x + lo * SAMPLE.w,
                    SAMPLE.y,
                    (hi - lo) * SAMPLE.w,
                    SAMPLE.h,
                ),
                accent.with_alpha(35),
            );
            for t in [lo, hi] {
                let x = SAMPLE.x + t * SAMPLE.w;
                c.polyline(&[(x, SAMPLE.y), (x, SAMPLE.bottom())], 2., accent);
            }
            c.text_clip(
                f,
                SAMPLE.x + 8.,
                SAMPLE.y + 8.,
                SAMPLE.w - 16.,
                dim,
                &s.name,
            );
        } else {
            c.text(
                f,
                SAMPLE.x + 16.,
                SAMPLE.cy(),
                dim,
                "Load a sample on Synth, then Reload to convert a selected section.",
            );
        }
        for (i, label) in [
            "Extract",
            "- frame",
            "+ frame",
            "Invert",
            "Reverse",
            "Center preview",
        ]
        .iter()
        .enumerate()
        {
            button(c, convert(i), label, false);
        }
        c.text(
            f,
            32.,
            710.,
            dim,
            &format!(
                "{} output frames / pitch follows the recording's Root setting",
                self.count
            ),
        );
        c.text(f, BANK.x, BANK.y - 20., dim, "USER BANK / click to type");
        c.text(f, NAME.x, NAME.y - 20., dim, "TABLE NAME / click to type");
        for (r, text, active) in [
            (BANK, &self.bank, self.text == Some(true)),
            (NAME, &self.name, self.text == Some(false)),
        ] {
            raised(c, r, 4., crate::design::color(0x17242e));
            c.text_clip(
                f,
                r.x + 9.,
                r.y + 8.,
                r.w - 18.,
                if active { accent } else { ink },
                &format!("{}{}", text, if active { " |" } else { "" }),
            );
        }
        button(
            c,
            Rect::new(790., 678., 170., 28.),
            "Save to User Bank",
            false,
        );
        for i in 0..16 {
            let n = self.selected / 16 * 16 + i;
            if n >= frames {
                break;
            }
            let r = thumb(i);
            c.fill_rrect(
                r,
                3.,
                if n == self.selected {
                    crate::design::color(0x48533a)
                } else {
                    crate::design::color(0x111c25)
                },
            );
            let trace: Vec<_> = (0..48)
                .map(|j| {
                    (
                        r.x + 8. + j as f32,
                        r.cy() - self.source.samples[n * SIZE + j * (SIZE - 1) / 47] * 15.,
                    )
                })
                .collect();
            c.polyline(&trace, 1., if n == self.selected { accent } else { cyan });
            c.text(
                Font::light(crate::design::font_size(9), 0).natural(),
                r.x + 3.,
                r.y + 3.,
                dim,
                &format!("{}", n + 1),
            );
        }
        c.text_clip(f, 20., 800., 1140., dim, &self.status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_edit_history_keyboard_and_layout() {
        let params = WaveParams::default();
        let mut w = Workbench::default();
        w.load(&params, 0);
        let original = w.source.clone();
        let click = |w: &mut Workbench, r: Rect| {
            w.click(r.cx(), r.cy(), &params);
        };
        click(&mut w, op(1));
        assert_eq!(w.source.frames(), original.frames() + 1);
        click(&mut w, op(3));
        assert_eq!(w.selected, 0);
        click(&mut w, op(6));
        assert_eq!(w.selected, 1);
        click(&mut w, op(6));
        assert_eq!(w.source.samples, original.samples);
        click(&mut w, op(7));
        assert_eq!(w.source.frames(), original.frames() + 1);
        w.click(DRAW.x + 1., DRAW.y + 5., &params);
        w.moved(DRAW.right() - 1., DRAW.bottom() - 5.);
        assert!(w.release());
        assert!(w.dirty);
        click(&mut w, NAME);
        w.character('\u{1}');
        for c in "Drawn Pad".chars() {
            w.character(c);
        }
        w.character('\r');
        assert_eq!(w.name, "Drawn Pad");
        let r = top(3);
        let applied = w.click(r.cx(), r.cy(), &params).unwrap();
        assert_eq!(applied.name, "Drawn Pad");
        assert!(!w.dirty);
        let mut rects = vec![DRAW, HARM, SAMPLE, STACK, PREVIEW, BANK, NAME];
        rects.extend((0..6).map(top));
        rects.extend((0..10).map(op));
        rects.extend((0..16).map(thumb));
        rects.extend((0..6).map(convert));
        rects.push(Rect::new(790., 678., 170., 28.));
        for (i, a) in rects.iter().enumerate() {
            assert!(a.x >= 0. && a.right() <= 1180. && a.y >= 80. && a.bottom() <= 788.);
            for b in &rects[i + 1..] {
                assert!(
                    a.right().min(b.right()) <= a.x.max(b.x)
                        || a.bottom().min(b.bottom()) <= a.y.max(b.y),
                    "Overlap {a:?} {b:?}"
                );
            }
        }
        if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
            std::fs::create_dir_all(&dir).unwrap();
            for scale in [1., 1.5] {
                let mut c = Canvas::new(1180, 820, scale).unwrap();
                w.paint(&mut c);
                c.pixmap
                    .save_png(format!("{dir}/table-workshop-{scale}.png"))
                    .unwrap();
            }
        }
    }
}
