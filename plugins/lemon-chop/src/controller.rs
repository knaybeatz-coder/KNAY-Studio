// SPDX-License-Identifier: GPL-3.0-or-later
//! SLAB general MIDI mode, from AlphaTheta's published MIDI message list (E1).
//! No proprietary handshake, display or LED protocol is claimed.
use crate::params::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Button(&'static str),
    Set(usize, f32),
    Delta(usize, f32),
}
pub fn pad_note(channel: u8, note: u8) -> Option<u8> {
    if channel != 7 || note >= 32 {
        return None;
    }
    Some(36 + (note / 16) * 16 + (3 - (note % 16) / 4) * 4 + note % 4)
}
pub fn button(channel: u8, note: u8) -> Option<Action> {
    Some(match (channel, note) {
        (0, 81) => Action::Button("Undo"),
        (0, 82) => Action::Button("Redo"),
        (0, 94) => Action::Button("Play/stop"),
        (0, 100) => Action::Button("Region <"),
        (0, 101) => Action::Button("Region >"),
        (6, 112) => Action::Set(KEYBOARD, 0.),
        (6, 114) => Action::Set(KEYBOARD, 1.),
        (6, 88) => Action::Button("Size -"),
        (6, 90) => Action::Button("Size +"),
        _ => return None,
    })
}
pub fn cc(channel: u8, cc: u8, value: u8) -> Option<Action> {
    if channel != 0 {
        return None;
    }
    let delta = match value {
        1..=63 => value as f32,
        65..=127 => -((value - 64) as f32),
        _ => 0.,
    };
    match cc {
        16..=19 => Some(Action::Delta(12 + (cc - 16) as usize, delta * 0.01)),
        60 => Some(Action::Delta(KEY, delta)),
        61 => Some(Action::Delta(DETUNE, delta)),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slab_pages_and_relative_encoders() {
        assert_eq!(pad_note(7, 12), Some(36));
        assert_eq!(pad_note(7, 0), Some(48));
        assert_eq!(pad_note(7, 28), Some(52));
        assert_eq!(pad_note(0, 12), None);
        assert_eq!(cc(0, 16, 65), Some(Action::Delta(12, -0.01)));
        assert_eq!(button(6, 114), Some(Action::Set(KEYBOARD, 1.)));
    }
}
