// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    morph::{self, Blend, Point, PreviewBus},
    sample::Sample,
    wavetable::Source,
};
use os_ui::{Canvas, Color, Font, Rect};

pub const LOAD: Rect = Rect::new(276., 839., 106., 30.);
pub const TIMELINE: Rect = Rect::new(32., 888., 1520., 98.);
const HEAR: Rect = Rect::new(394., 839., 105., 30.);
const REMOVE: Rect = Rect::new(505., 839., 88., 30.);
const SWEEP: Rect = Rect::new(1220., 839., 100., 30.);
const STOP: Rect = Rect::new(1328., 839., 66., 30.);
pub const BUILD: Rect = Rect::new(1404., 839., 148., 30.);
fn curve_button(i: usize) -> Rect {
    Rect::new(805. + i as f32 * 91., 839., 87., 30.)
}

#[derive(Default)]
pub struct Panel {
    points: Vec<Point>,
    selected: Option<usize>,
    drag: Option<(usize, f32)>,
    blend: Blend,
    overview: Vec<(f32, f32)>,
    message: String,
}
impl Panel {
    pub fn attach(&mut self, sample: Option<&Sample>) {
        *self = Self::default();
        if let Some(s) = sample {
            let width = s.samples.len().min(760);
            self.overview = (0..width)
                .map(|i| {
                    let a = i * s.samples.len() / width;
                    let b = (i + 1) * s.samples.len() / width;
                    s.samples[a..b]
                        .iter()
                        .fold((0f32, 0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)))
                })
                .collect();
        }
    }
    pub fn error(&mut self, msg: String) {
        self.message = msg;
    }
    fn hear(&self, bus: &PreviewBus) {
        if let Some(i) = self.selected {
            bus.play(morph::audition(&self.points[i].cycle, false));
        }
    }
    fn commit_point(&mut self, s: &Sample, t: f32, replace: Option<usize>, bus: &PreviewBus) {
        match morph::analyze(s, t) {
            Ok(point) => {
                if let Some(index) = replace {
                    self.points.remove(index);
                }
                let index = self.points.partition_point(|p| p.time < t);
                self.points.insert(index, point);
                self.selected = Some(index);
                morph::align(&mut self.points);
                self.hear(bus);
                self.message = format!(
                    "Marker {}   ·   {:.2} s   ·   {:.1} Hz detected. Drag to move it.",
                    index + 1,
                    t * s.samples.len() as f32 / s.rate as f32,
                    self.points[index].hz
                );
            }
            Err(e) => self.message = e,
        }
    }
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn moved(&mut self, x: f32) -> bool {
        if let Some((_, time)) = &mut self.drag {
            *time = ((x - TIMELINE.x) / TIMELINE.w).clamp(0., 1.);
            true
        } else {
            false
        }
    }
    pub fn release(&mut self, sample: Option<&Sample>, bus: &PreviewBus) -> bool {
        let Some((i, time)) = self.drag.take() else {
            return false;
        };
        if (time - self.points[i].time).abs() > 0.0001 {
            if self
                .points
                .iter()
                .enumerate()
                .any(|(j, p)| j != i && (p.time - time).abs() < 0.002)
            {
                self.message = "Leave a little space between markers.".into();
            } else if let Some(s) = sample {
                self.commit_point(s, time, Some(i), bus);
            }
        }
        true
    }
    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        sample: Option<&Sample>,
        bus: &PreviewBus,
    ) -> Option<Source> {
        if STOP.contains(x, y) {
            bus.stop();
            return None;
        }
        if TIMELINE.contains(x, y) {
            let Some(s) = sample else {
                self.message = "Open a WAV or reload an oscillator containing a recording.".into();
                return None;
            };
            let t = ((x - TIMELINE.x) / TIMELINE.w).clamp(0., 1.);
            if let Some((i, _)) = self
                .points
                .iter()
                .enumerate()
                .filter(|(_, p)| (p.time - t).abs() * TIMELINE.w <= 9.)
                .min_by(|a, b| (a.1.time - t).abs().total_cmp(&(b.1.time - t).abs()))
            {
                self.selected = Some(i);
                self.drag = Some((i, self.points[i].time));
                self.hear(bus);
                self.message = format!(
                    "Marker {} selected   ·   {:.1} Hz",
                    i + 1,
                    self.points[i].hz
                );
            } else if self.points.len() < morph::MAX_POINTS {
                self.commit_point(s, t, None, bus);
            } else {
                self.message =
                    "All 12 markers are in use. Select a marker to move or remove it.".into();
            }
        }
        if HEAR.contains(x, y) {
            self.hear(bus);
        }
        if REMOVE.contains(x, y) {
            if let Some(i) = self.selected {
                bus.stop();
                self.points.remove(i);
                morph::align(&mut self.points);
                self.selected = self.points.len().checked_sub(1).map(|last| i.min(last));
                self.message = "Marker removed.".into();
            }
        }
        for (i, blend) in [Blend::Linear, Blend::Smooth, Blend::Hold]
            .into_iter()
            .enumerate()
        {
            if curve_button(i).contains(x, y) {
                self.blend = blend;
                self.message =
                    "Transition updated. Hear Sweep previews the current markers.".into();
            }
        }
        if SWEEP.contains(x, y) || BUILD.contains(x, y) {
            let name = sample.map_or("Recording", |s| s.name.as_str());
            match morph::build(&self.points, self.blend, name) {
                Ok(source) => {
                    if BUILD.contains(x, y) {
                        bus.stop();
                        self.message = format!(
                            "Built {} frames. Apply to Oscillator, then scan with Position.",
                            source.frames()
                        );
                        return Some(source);
                    }
                    bus.play(morph::audition(&source.samples, true));
                    self.message = "Auditioning the morph from first marker to last.".into();
                }
                Err(e) => self.message = e,
            }
        }
        None
    }
    pub fn paint(&self, c: &mut Canvas, sample: Option<&Sample>) {
        let lime = crate::design::color(0xe3df8b);
        let cyan = crate::design::color(0x73dacc);
        let ink = Color::rgb(0xe7f1f4);
        let dim = Color::rgb(0x93a8b2);
        let f = Font::light(13, 0).natural();
        crate::instrument_view::raised(
            c,
            Rect::new(16., 824., 1552., 214.),
            10.,
            Color::rgb(0x0b151b),
        );
        c.text(
            Font::light(19, 0).natural().bold(),
            32.,
            841.,
            ink,
            "SAMPLE MORPH",
        );
        let button = |c: &mut Canvas, r: Rect, label: &str, selected: bool| {
            crate::instrument_view::raised(
                c,
                r,
                6.,
                if selected { lime } else { Color::rgb(0x102028) },
            );
            c.text_center(
                f,
                r,
                if selected { Color::rgb(0x0a1314) } else { ink },
                label,
            );
        };
        for (r, label) in [
            (LOAD, "Open WAV"),
            (HEAR, "Hear point"),
            (REMOVE, "Remove"),
            (SWEEP, "Hear sweep"),
            (STOP, "Stop"),
            (BUILD, "Build draft"),
        ] {
            button(c, r, label, r.x == BUILD.x && self.points.len() >= 2);
        }
        c.text(f, 685., 847., dim, "TRANSITION");
        for (i, (label, b)) in [
            ("Linear", Blend::Linear),
            ("Smooth", Blend::Smooth),
            ("Hold", Blend::Hold),
        ]
        .into_iter()
        .enumerate()
        {
            button(c, curve_button(i), label, self.blend == b);
        }
        c.fill_rect_gradient(TIMELINE, Color::rgb(0x030a10), Color::rgb(0x0b1b22), 5.);
        if let Some(s) = sample {
            let mid = TIMELINE.cy();
            c.fill_rect(
                Rect::new(TIMELINE.x, mid, TIMELINE.w, 1.),
                cyan.with_alpha(70),
            );
            for (i, &(lo, hi)) in self.overview.iter().enumerate() {
                let x = TIMELINE.x + i as f32 * TIMELINE.w / self.overview.len() as f32;
                c.fill_rect(
                    Rect::new(x, mid - hi * 42., 1.7, ((hi - lo) * 42.).max(1.)),
                    cyan.with_alpha(180),
                );
            }
            for (i, p) in self.points.iter().enumerate() {
                let t = self
                    .drag
                    .filter(|(j, _)| *j == i)
                    .map_or(p.time, |(_, t)| t);
                let x = TIMELINE.x + t * TIMELINE.w;
                let color = if Some(i) == self.selected { lime } else { ink };
                c.fill_rect(
                    Rect::new(
                        x.clamp(TIMELINE.x, TIMELINE.right() - 2.),
                        TIMELINE.y,
                        2.,
                        TIMELINE.h,
                    ),
                    color,
                );
                let badge = Rect::new(
                    (x - 10.).clamp(TIMELINE.x, TIMELINE.right() - 23.),
                    TIMELINE.y + 2.,
                    23.,
                    18.,
                );
                c.fill_rrect(badge, 4., color);
                c.text_center(f, badge, Color::rgb(0x081118), &(i + 1).to_string());
            }
            c.text_clip(
                f,
                32.,
                1004.,
                480.,
                dim,
                &format!(
                    "{}   ·   {:.2} s   ·   {} / 12 markers",
                    s.name,
                    s.samples.len() as f32 / s.rate as f32,
                    self.points.len()
                ),
            );
        } else {
            c.text_center(
                f,
                TIMELINE,
                dim,
                "Drop a WAV here or choose Open WAV, then mark two or more pitched moments.",
            );
        }
        c.text_clip(
            f,
            522.,
            1004.,
            1024.,
            ink,
            if self.message.is_empty() {
                "Click to add and hear a marker. Drag to reposition it."
            } else {
                &self.message
            },
        );
    }
}
