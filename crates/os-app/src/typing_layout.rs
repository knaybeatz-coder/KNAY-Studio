// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    chords,
    harmony::{Harmony, ROOTS},
    playlist::{Cmd, Mark, PItem, Popup},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Root(u8),
    Scale(usize),
    Follow,
}

#[derive(Default, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Layout {
    independent: bool,
    root: u8,
    scale: usize, // 0 is chromatic, 67..=88 are the named scales in the chord library.
}

impl Layout {
    pub fn load() -> Self {
        crate::prefs::text("typing_keyboard_layout")
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .filter(|s| s.root < 12 && (s.scale == 0 || (67..=88).contains(&s.scale)))
            .unwrap_or_default()
    }
    pub fn set(&mut self, change: Change) {
        match change {
            Change::Root(root) if root < 12 => self.root = root,
            Change::Scale(scale) if scale == 0 || (67..=88).contains(&scale) => self.scale = scale,
            Change::Follow => {}
            _ => return,
        }
        self.independent = change != Change::Follow;
        if let Ok(value) = serde_json::to_string(self) {
            crate::prefs::set_text("typing_keyboard_layout", &value);
        }
    }
    pub fn note(self, key: i32, harmony: &Harmony) -> u16 {
        if !self.independent {
            return if harmony.typing_snap {
                harmony.snap(key, 127)
            } else {
                key as u16
            };
        }
        let transposed = (key + i32::from(self.root)).min(127);
        if self.scale == 0 {
            return transposed as u16;
        }
        let allowed = chords::classes(self.scale, self.root);
        (0..=127)
            .filter(|n| allowed[(n % 12) as usize])
            .min_by_key(|n| ((n - transposed).abs(), *n))
            .unwrap_or(transposed) as u16
    }
    pub fn menu(self, x: f32, y: f32) -> Popup {
        let item = |name: &str, change, checked| {
            PItem::new(name.to_string(), Cmd::TypingLayout(change)).mark(Mark::Check(checked))
        };
        let scale_name = |scale| {
            if scale == 0 {
                "Chromatic"
            } else {
                chords::label(scale)
            }
        };
        let roots = ROOTS
            .iter()
            .enumerate()
            .map(|(i, name)| item(name, Change::Root(i as u8), i == self.root as usize))
            .collect();
        let scales = std::iter::once(0)
            .chain(67..=88)
            .map(|i| item(scale_name(i), Change::Scale(i), i == self.scale))
            .collect();
        Popup {
            x,
            y,
            items: vec![
                PItem::sub(format!("Key: {}", ROOTS[self.root as usize]), roots),
                PItem::sub(format!("Scale: {}", scale_name(self.scale)), scales),
                PItem::sep(),
                item(
                    "Follow Piano Roll typing snap",
                    Change::Follow,
                    !self.independent,
                ),
            ],
            sub: None,
            scroll: [0; 2],
            cascade: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toolbar_typing_layout_is_independent_and_persistent() {
        use crate::app::{Action, App};
        use winit::keyboard::KeyCode;
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        if !app.typing_on() {
            app.toggle_typing();
        }
        let mut canvas = os_ui::Canvas::new(1920, 1080, 1.).unwrap();
        app.paint(&mut canvas, None);
        let r = app
            .hits
            .iter()
            .find(|h| h.action == Action::Typing)
            .unwrap()
            .rect;
        app.mouse_move(r.cx(), r.cy());
        app.mouse_down_right();
        let popup = app.state.playlist.popup.take().unwrap();
        assert!(popup.items[0].label.starts_with("Key:"));
        let harmony = &mut app.state.piano.harmony;
        harmony.typing_snap = true;
        harmony.root = 0;
        harmony.scale = 67;
        crate::playlist::run(&mut app, Cmd::TypingLayout(Change::Root(2)));
        crate::playlist::run(&mut app, Cmd::TypingLayout(Change::Scale(73)));
        assert!(app.typing_key(KeyCode::KeyZ, true, false, false));
        assert_eq!(app.state.piano.typed.last().unwrap().1, 50);
        assert_eq!(Layout::load().note(52, &app.state.piano.harmony), 53);
        assert_eq!(app.state.piano.harmony.root, 0);
        assert_eq!(app.state.piano.harmony.scale, 67);
        app.typing_layout.set(Change::Follow);
        assert!(app.typing_key(KeyCode::KeyZ, false, false, false));
        assert_eq!(Layout::load().note(49, &app.state.piano.harmony), 48);
    }
}
