// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::performance::Keyboard;
use std::sync::atomic::Ordering::Relaxed;

const BED: Rect = Rect::new(17., 997., 1550., 53.);
const NATURALS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const PREV: Rect = Rect::new(1452., 963., 31., 26.);
const NEXT: Rect = Rect::new(1536., 963., 31., 26.);
fn bounds(index: usize, black: bool) -> Rect {
    let width = BED.w / 42.;
    if black {
        Rect::new(
            BED.x + (index + 1) as f32 * width - width * 0.30,
            BED.y,
            width * 0.6,
            BED.h * 0.61,
        )
    } else {
        Rect::new(BED.x + index as f32 * width, BED.y, width - 0.8, BED.h)
    }
}
fn pitch(base: u8, index: usize) -> u8 {
    base * 12 + (index / 7) as u8 * 12 + NATURALS[index % 7]
}
fn has_black(index: usize) -> bool {
    !matches!(index % 7, 2 | 6)
}
fn key_at(base: u8, x: f32, y: f32) -> Option<u8> {
    if !BED.contains(x, y) {
        return None;
    }
    for i in 0..42 {
        if has_black(i) && bounds(i, true).contains(x, y) {
            return Some(pitch(base, i) + 1);
        }
    }
    let i = (((x - BED.x) / BED.w * 42.) as usize).min(41);
    Some(pitch(base, i))
}
pub(crate) fn slide(keyboard: &Keyboard, x: f32, y: f32) {
    keyboard.set_note(key_at(keyboard.octave.load(Relaxed), x, y).unwrap_or(255));
}
pub(crate) fn press(keyboard: &Keyboard, x: f32, y: f32) -> bool {
    let base = keyboard.octave.load(Relaxed);
    if PREV.contains(x, y) || NEXT.contains(x, y) {
        keyboard.release();
        keyboard.octave.store(
            if PREV.contains(x, y) {
                base.saturating_sub(1)
            } else {
                (base + 1).min(4)
            },
            Relaxed,
        );
        return true;
    }
    if let Some(note) = key_at(base, x, y) {
        keyboard.set_note(note);
        keyboard.dragging.store(true, Relaxed);
        return true;
    }
    false
}
pub(super) fn paint(u: &mut Ui, p: &WaveParams) {
    let keyboard = &p.keyboard;
    u.c.fill_rect_gradient(
        Rect::new(8., 951., 1568., 102.),
        Color::rgb(0x101d25),
        Color::rgb(0x050b10),
        8.,
    );
    u.c.fill_rect(Rect::new(18., 952., 1548., 1.), theme_b().with_alpha(80));
    u.toggle(
        CHORD_ON,
        Rect::new(20., 964., 80., 26.),
        "CHORD ON",
        "CHORD OFF",
        theme_a(),
    );
    u.toggle(
        ARP_ON,
        Rect::new(758., 964., 76., 26.),
        "ARP ON",
        "ARP OFF",
        theme_mod(),
    );
    for (k, x, w, label) in [
        (CHORD_KEY, 110., 65., "KEY"),
        (CHORD_SCALE, 185., 152., "SCALE"),
        (CHORD_SHAPE, 347., 190., "CHORD STRUCTURE"),
        (CHORD_INV, 547., 180., "VOICING"),
        (ARP_ORDER, 844., 140., "PATTERN"),
        (ARP_RATE, 994., 110., "DIVISION"),
        (ARP_OCT, 1114., 125., "RANGE"),
        (ARP_GATE, 1249., 94., "GATE"),
    ] {
        u.c.text(font(8), x + 4., 953., theme_dim(), label);
        u.select(k, Rect::new(x, 966., w, 24.), theme_ink());
    }
    let base = keyboard.octave.load(Relaxed);
    u.c.text_center(font(14), PREV, theme_b(), "‹");
    u.c.text_center(font(14), NEXT, theme_b(), "›");
    u.c.text_center(
        font(12),
        Rect::new(1484., 963., 51., 26.),
        theme_ink(),
        &format!("C{}", base as i32 - 1),
    );
    let lights = keyboard.lights.each_ref().map(|v| v.load(Relaxed));
    let mouse = keyboard.note();
    for black in [false, true] {
        for i in 0..42 {
            if black && !has_black(i) {
                continue;
            }
            let note = pitch(base, i) + black as u8;
            let r = bounds(i, black);
            let lit = lights[note as usize / 64] & (1u64 << (note % 64)) != 0;
            let (top, bottom) = if note == mouse {
                (theme_a(), Color::rgb(0xa5b332))
            } else if lit {
                (theme_b(), Color::rgb(0x277d88))
            } else if black {
                (Color::rgb(0x25353e), Color::rgb(0x05090e))
            } else {
                (Color::rgb(0x82949f), Color::rgb(0xd6e0e3))
            };
            u.c.fill_rect_gradient(r, top, bottom, if black { 3. } else { 2. });
            if black {
                u.c.stroke_rrect(r, 3., 1., Color::rgb(0x030a0d));
            } else if i % 7 == 0 {
                u.c.text(
                    font(9),
                    r.x + 5.,
                    r.bottom() - 15.,
                    Color::rgb(0x233641),
                    &format!("C{}", note as i32 / 12 - 1),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_editor::TableEditor;
    use lemon_small_fx::editor::Extension;
    use std::sync::Arc;
    #[test]
    fn piano_uses_black_key_priority_drags_and_releases_on_close() {
        let params = Arc::new(WaveParams::default());
        let editor = TableEditor::new(params.clone());
        let c = bounds(0, false);
        let sharp = bounds(0, true);
        let base = pitch(params.keyboard.octave.load(Relaxed), 0);
        assert!(editor.click(sharp.cx(), sharp.cy(), None));
        assert_eq!(params.keyboard.note(), base + 1);
        assert!(editor.moved(c.cx(), c.bottom() - 2.));
        assert_eq!(params.keyboard.note(), base);
        editor.release(None);
        assert_eq!(params.keyboard.note(), 255);
        editor.click(NEXT.cx(), NEXT.cy(), None);
        editor.click(c.cx(), c.bottom() - 2., None);
        assert_eq!(params.keyboard.note(), base + 12);
        editor.editor_closed();
        assert_eq!(params.keyboard.note(), 255);
        assert!(!editor.dragging());
    }
}
