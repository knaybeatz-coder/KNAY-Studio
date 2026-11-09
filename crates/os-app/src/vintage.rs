// SPDX-License-Identifier: GPL-3.0-or-later
//! Profile-local switch and the vintage transport readout.
use os_ui::{Canvas, Color, Font, Rect};

pub const LABEL: &str = "FL Studio 2005";
const KEY: &str = "appearance_fl_studio_2005";

pub fn enabled() -> bool {
    crate::settings_file::flag(KEY, false)
}

pub fn set_enabled(on: bool) {
    crate::settings_file::set_flag(KEY, on);
}

/// Title marquee advances in whole logical pixels and pauses at the start.
#[derive(Default)]
pub struct TitleScroll {
    label: String,
    cycle_width: f32,
    start: Option<std::time::Instant>,
    painted: f32,
}

impl TitleScroll {
    const SPEED: f32 = 24.0;
    const PAUSE: f32 = 0.85;

    fn offset(&self, now: std::time::Instant) -> f32 {
        let Some(start) = self.start else { return 0.0 };
        let seconds = now.saturating_duration_since(start).as_secs_f32();
        let period = Self::PAUSE + self.cycle_width / Self::SPEED;
        let moving = (seconds % period - Self::PAUSE).max(0.0);
        (moving * Self::SPEED).floor()
    }

    pub fn changed(&self, now: std::time::Instant) -> bool {
        self.offset(now) != self.painted
    }

    pub fn paint(&mut self, label: &str, cycle_width: f32, now: std::time::Instant) -> f32 {
        if self.label != label || self.cycle_width != cycle_width {
            self.label.clear();
            self.label.push_str(label);
            self.cycle_width = cycle_width;
            self.start = Some(now);
        }
        self.painted = self.offset(now);
        self.painted
    }
}

pub fn clock(canvas: &mut Canvas, area: Rect, bar: u32, step: u32, tick: u32) {
    let value = format!("{bar:03}:{step:02}:{tick:02}");
    use crate::vintage_skin as skin;
    let inner = area.inset(2.0);
    if inner.is_empty() {
        return;
    }
    // The wrapped toolbar has a 128 px display. Use the smaller original FL4
    // digits there, and fit extended readings before painting any glyphs.
    for (atlas, digit_w, digit_h) in [("TimeDigits_24", 18.0, 24.0), ("TimeDigits_16", 12.0, 16.0)] {
        if !skin::has_frame(atlas, 10) {
            continue;
        }
        let colon_w = digit_w / 3.0;
        let width = value.bytes().map(|b| if b == b':' { colon_w } else { digit_w }).sum::<f32>();
        let scale = (inner.w / width).min(inner.h / digit_h).min(1.0);
        if scale < 1.0 && digit_h == 24.0 && skin::has_frame("TimeDigits_16", 10) {
            continue;
        }
        let mut x = inner.right() - width * scale;
        let y = area.cy() - digit_h * scale / 2.0;
        // Paint over the fixed separators baked into the original toolbar.
        skin::draw(canvas, atlas, 0, area);
        for b in value.bytes() {
            if b == b':' {
                let dot = 2.0 * scale;
                let ink = Color::rgb(0xb4cbc3);
                canvas.fill_rect(Rect::new(x + dot, y + digit_h * scale * 0.29, dot, dot), ink);
                canvas.fill_rect(Rect::new(x + scale, y + digit_h * scale * 0.67, dot, dot), ink);
                x += colon_w * scale;
            } else {
                let cell = Rect::new(x, y, digit_w * scale, digit_h * scale);
                skin::draw(canvas, atlas, usize::from(b - b'0') + 1, cell);
                x += cell.w;
            }
        }
        return;
    }
    const DIGITS: [u8; 10] = [
        0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110,
        0b1101101, 0b1111101, 0b0000111, 0b1111111, 0b1101111,
    ];
    let text = format!("{bar}:{step:02}:{tick:02}");
    let units: f32 = text.bytes().map(|b| if b == b':' { 6.0 } else { 14.0 }).sum();
    let scale = ((area.w - 12.0) / units).min((area.h - 7.0) / 22.0).clamp(0.1, 1.0);
    let mut x = area.right() - 7.0 - units * scale;
    let y = area.cy() - 11.0 * scale;
    let lit = Color::rgb(0xcfe2d4);
    let dormant = Color::rgb(0x344643);
    for byte in text.bytes() {
        let rect = |dx, dy, w, h| Rect::new(x + dx * scale, y + dy * scale, w * scale, h * scale);
        if byte == b':' {
            canvas.fill_rect(rect(1.0, 6.0, 2.0, 2.0), lit);
            canvas.fill_rect(rect(1.0, 14.0, 2.0, 2.0), lit);
            x += 6.0 * scale;
            continue;
        }
        let mask = DIGITS[usize::from(byte - b'0')];
        for (bit, (dx, dy, w, h)) in [
            (3., 0., 7., 2.), (10., 2., 2., 8.), (10., 12., 2., 8.),
            (3., 20., 7., 2.), (1., 12., 2., 8.), (1., 2., 2., 8.), (3., 10., 7., 2.),
        ].into_iter().enumerate() {
            canvas.fill_rect(rect(dx, dy, w, h), if mask & (1 << bit) != 0 { lit } else { dormant });
        }
        x += 14.0 * scale;
    }
}

/// Metal surfaces drawn at the destination scale, shared across the vintage workspace.
pub fn metal(c:&mut Canvas,r:Rect,face:Color) {
    c.fill_rect(r,Color::rgb(0x202b31));
    c.fill_rect_gradient(r.inset(1.),face.shade(0.13),face.shade(-0.12),0.);
    c.fill_rect(Rect::new(r.x+1.,r.y+1.,r.w-2.,1.),face.shade(0.36));
    c.fill_rect(Rect::new(r.x+1.,r.y+2.,1.,r.h-3.),face.shade(0.22));
    c.fill_rect(Rect::new(r.right()-2.,r.y+2.,1.,r.h-3.),face.shade(-0.35));
}
pub fn recess(c:&mut Canvas,r:Rect,face:Color) {
    c.fill_rect(r,Color::rgb(0x96a3a6));
    c.fill_rect(Rect::new(r.x,r.y,r.w-1.,r.h-1.),Color::rgb(0x202d33));
    c.fill_rect(r.inset(2.),face);
}

pub fn meter_idle(c: &mut Canvas, column: Rect) {
    if !crate::vintage_skin::draw(c, "FXMeterIdle", 0, column) {
        c.fill_rect_gradient(column, Color::rgb(0x7b5a29), Color::rgb(0x102c0d), 0.0);
    }
}

pub fn window_caption(c: &mut Canvas, r: Rect) {
    c.fill_rect_gradient(r, Color::rgb(0x566167), Color::rgb(0x434e54), 0.0);
    c.fill_rect(Rect::new(r.x, r.y, r.w, 1.0), Color::rgb(0x829096));
}

pub fn mixer_digits(c: &mut Canvas, r: Rect, number: u16) {
    let label = if number == 0 { "M".into() } else { number.to_string() };
    if number != 0 && label.len() <= 3 && crate::vintage_skin::has_frame("Digits_9", 10) {
        let x = (r.cx() - 3.0 * label.len() as f32).round();
        for (i, digit) in label.bytes().enumerate() {
            crate::vintage_skin::alpha_native(c, "MixerDigits", usize::from(digit - b'0') + 1,
                x + 6.0 * i as f32, (r.cy() - 4.5).round());
        }
    } else {
        c.text_center(Font::vintage(9).bold(), r, Color::rgb(0xf3cc99), &label);
    }
}
pub const GAIN_MARKS: &str = "visible_vintage_gain_marks";
pub const UNITY_INK: Color = Color::rgb(0xf1bf80);

pub fn fader_unity(c: &mut Canvas, x: f32, y: f32, reach: f32) {
    if crate::workspace_visibility::shown(GAIN_MARKS) {
        c.fill_rect(Rect::new(x - reach - 3.0, y.round(), 3.0, 1.0), UNITY_INK);
        c.fill_rect(Rect::new(x + reach, y.round(), 3.0, 1.0), UNITY_INK);
    }
}

/// Matching compact steel switches with inset status lenses.
pub fn mixer_switch(c: &mut Canvas, r: Rect, record: bool, on: bool, hot: bool) {
    let face = Color::rgb(if hot { 0x78888d } else { 0x566367 });
    if on { recess(c, r, Color::rgb(if hot { 0x495e63 } else { 0x334449 })); }
    else { metal(c, r, face); }
    let width = if record { r.w.min(11.0) - 3.0 } else { r.w.min(18.0) - 4.0 };
    let x = (r.cx() - width / 2.0).round();
    let y = (r.cy() - 3.0).round();
    if record {
        let lens = Rect::new(x, y, width, 6.0);
        recess(c, lens, Color::rgb(if on { 0xec7758 } else { 0x553b37 }));
        if on {
            c.fill_rect(Rect::new(lens.x + 2.0, lens.y + 2.0, lens.w - 4.0, 1.0), Color::rgb(0xffb080));
        }
    } else {
        c.text_center(Font::vintage(8).bold(), Rect::new(r.x, r.y, r.w, r.h - 2.0),
            Color::rgb(if on { 0xe0e8d6 } else { 0xadb5b8 }), "FX");
        c.fill_rect(Rect::new(x, r.bottom() - 3.0, width, 1.0),
            Color::rgb(if on { 0xc3ae68 } else { 0x344348 }));
    }
}

pub fn fader_guide(c: &mut Canvas, x: f32, top: f32, bottom: f32) {
    let height = (bottom - top).max(0.0);
    if height < 2.0 { return; }
    recess(c, Rect::new(x - 3.0, top, 6.0, height), Color::rgb(0x23323b));
    for i in 0..=(height / 8.0) as usize {
        let y = top + i as f32 * 8.0;
        let length = if i % 4 == 0 { 5.0 } else { 3.0 };
        c.fill_rect(Rect::new(x - 5.0 - length, y, length, 1.0), Color::rgb(0x3c464b));
        c.fill_rect(Rect::new(x + 5.0, y, length, 1.0), Color::rgb(0x879297));
    }
}

pub fn fader(c:&mut Canvas,x:f32,y:f32,hover:bool) {
    if crate::vintage_skin::alpha_native(c,"VSlider",usize::from(hover),(x-9.).round(),(y-8.).round()) {return;}
    let cap=Rect::new(x-7.,y-10.,14.,20.);
    metal(c,cap,Color::rgb(if hover {0x9aa5a8}else{0x7c898f}));
    for dy in [-5.,-3.,3.,5.] {
        c.fill_rect(Rect::new(cap.x+2.,y+dy,cap.w-4.,1.),Color::rgb(0x394b54));
    }
    c.fill_rect(Rect::new(cap.x+2.,y-1.,cap.w-4.,2.),Color::rgb(0xc78478));
}

/// A slim beveled channel divider with an inset center seam.
pub fn mixer_divider(c: &mut Canvas, r: Rect) {
    metal(c, r, Color::rgb(0x34383b));
    c.fill_rect(Rect::new(r.x + 2.0, r.y + 3.0, 1.0, (r.h - 6.0).max(0.0)), Color::rgb(0x222527));
    c.fill_rect(Rect::new(r.x + 3.0, r.y + 3.0, 1.0, (r.h - 6.0).max(0.0)), Color::rgb(0x4c5154));
}

/// Node-and-line artwork shared by the compact and large Playlist controls.
pub fn automation_glyph(c: &mut Canvas, r: Rect, ink: Color) {
    let unit = r.w.min(r.h) / 12.0;
    let (x, y) = (r.cx().round(), r.cy().round());
    let points = [(x - 4.0 * unit, y + 2.5 * unit), (x, y - 2.7 * unit), (x + 4.0 * unit, y - 0.5 * unit)];
    let shade = Color::rgb(0x243139);
    c.polyline(&points, 2.3 * unit, shade);
    c.polyline(&points, 1.15 * unit, ink);
    for (px, py) in points {
        c.fill_rect(Rect::new(px - 1.25 * unit, py - 1.25 * unit, 2.5 * unit, 2.5 * unit), shade);
        c.fill_rect(Rect::new(px - 0.7 * unit, py - 0.7 * unit, 1.4 * unit, 1.4 * unit), ink);
    }
}

/// A static one-device-pixel wire with the full slack that fits above the scroll rail.
pub fn cable(c: &mut Canvas, from: (f32, f32), to: (f32, f32), floor: f32, color: Color) {
    let Some(clip) = c.paint_bounds() else { return };
    let (a, b) = if from.0 <= to.0 { (from, to) } else { (to, from) };
    let scale = c.scale;
    let px = 1.0 / scale;
    let left = (a.0 * scale).round() as i32;
    let right = (b.0 * scale).round() as i32;
    let width = (right - left).max(1) as f32;
    let first = left.max((clip.x * scale).floor() as i32);
    let last = right.min((clip.right() * scale).ceil() as i32);
    let sag = (floor - a.1.max(b.1) - 2.0 * px).clamp(0.0, 58.0).min(width * px * 0.58);
    let y_at = |x: i32| {
        let t = ((x - left) as f32 / width).clamp(0.0, 1.0);
        ((a.1 + (b.1 - a.1) * t + 4.0 * t * (1.0 - t) * sag) * scale).round() * px
    };
    for x in first..=last {
        let y = y_at(x);
        let next = y_at(x + 1);
        c.fill_rect(Rect::new(x as f32 * px, y.min(next), px, (next - y).abs() + px), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vintage_clock_keeps_the_complete_leading_zero_in_the_wrapped_toolbar() {
        use crate::vintage_skin as skin;
        if !skin::has_frame("TimeDigits_16", 10) {
            return;
        }
        let mut clock_face = Canvas::new(128, 32, 1.0).unwrap();
        clock(&mut clock_face, Rect::new(0.0, 0.0, 128.0, 32.0), 1, 1, 0);
        let mut original_zero = Canvas::new(12, 16, 1.0).unwrap();
        assert!(skin::native(&mut original_zero, "TimeDigits_16", 1, 0.0, 0.0));
        // 001:01:00 takes 92 px in the small atlas, with 2 px of right padding.
        for y in 0..16 {
            let start = ((y + 8) * 128 + 34) * 4;
            assert_eq!(&clock_face.pixmap.data()[start..start + 48], &original_zero.pixmap.data()[y * 48..(y + 1) * 48]);
        }
    }
}
