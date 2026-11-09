// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional local FL4 cursor resources, never bundled with Lemon.
use crate::{app::Cursor, playlist::PlTool, playlist_tools as tools};
use os_ui::{Canvas, Color, Rect};
use std::{collections::HashMap, sync::OnceLock};
use winit::window::{CustomCursor, CustomCursorSource};

pub const PACK: u8 = os_theme::appearance::VINTAGE_CURSOR_PACK;
const SIZE: usize = 32;

struct Bitmap {
    rgba: Vec<u8>,
    hotspot: [u16; 2],
}

fn decode(data: &[u8]) -> Option<Bitmap> {
    // RT_CURSOR hotspot, BITMAPINFOHEADER, black/white palette, XOR and AND masks.
    if data.len() != 308 || data[4..24] != [40, 0, 0, 0, 32, 0, 0, 0, 64, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0] {
        return None;
    }
    let hotspot = [
        u16::from_le_bytes(data[0..2].try_into().ok()?),
        u16::from_le_bytes(data[2..4].try_into().ok()?),
    ];
    if hotspot.iter().any(|&p| p >= SIZE as u16) { return None; }
    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = (SIZE - 1 - y) * 4 + x / 8;
            let bit = 0x80 >> (x % 8);
            let xor = data[52 + index] & bit != 0;
            let and = data[180 + index] & bit != 0;
            let palette = 44 + usize::from(xor) * 4;
            rgba.extend_from_slice(&[data[palette + 2], data[palette + 1], data[palette], if and && !xor { 0 } else { 255 }]);
        }
    }
    rgba.chunks_exact(4).any(|p| p[3] != 0).then_some(Bitmap { rgba, hotspot })
}

fn bitmap(cursor: Cursor) -> Option<&'static Bitmap> {
    let name = match cursor {
        Cursor::EwResize | Cursor::Playlist(_, tools::RESIZE | tools::STRETCH) => "SIZEH",
        Cursor::NsResize => "MOVEV",
        Cursor::Playlist(_, tools::GRAB | tools::GRABBING) => "MOVE",
        Cursor::Playlist(_, tools::COPYING | 4) => "32763",
        Cursor::Playlist(_, tools::GAIN) => "32761",
        Cursor::Playlist(_, 12) => "EESEL",
        Cursor::Playlist(_, 20) => "EECUT",
        Cursor::Playlist(_, 21) => "EEPEN",
        Cursor::Playlist(_, 22) => "MOVE",
        Cursor::Playlist(_, 23) => "EEZOOM",
        Cursor::Playlist(tool, 0..=3) => match tool {
            PlTool::Draw => "EEPEN",
            PlTool::Paint => "EEBRUSH",
            PlTool::Slice => "EECUT",
            PlTool::Delete => "EEDEL",
            PlTool::Select => "EESEL",
            PlTool::Mute => "32767",
            PlTool::Zoom => "EEZOOM",
            PlTool::Slip => "MOVEH",
            _ => return None,
        },
        _ => return None,
    };
    static IMAGES: OnceLock<HashMap<&'static str, Bitmap>> = OnceLock::new();
    IMAGES.get_or_init(|| {
        ["SIZEH", "MOVEV", "MOVE", "32763", "32761", "EEPEN", "EEBRUSH", "EECUT",
         "EEDEL", "EESEL", "32767", "EEZOOM", "MOVEH"].into_iter().filter_map(|name| {
            let bitmap = crate::vintage_skin::directories().into_iter().find_map(|dir| {
                let path = dir.join("Cursors").join(format!("{name}.bin"));
                if std::fs::metadata(&path).ok()?.len() != 308 { return None; }
                decode(&std::fs::read(path).ok()?)
            })?;
            Some((name, bitmap))
        }).collect()
    }).get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vintage_cursor_pack_is_default_and_portable() {
        let appearance = os_theme::design::vintage_2005().appearance;
        assert_eq!(tools::resolved_cursor_pack(appearance, Cursor::Default), PACK);
        let appearance = os_theme::fl::FlTheme::parse(
            "LemonStudioStyle.cursor-family=28\nLemonStudioStyle.cursor-1-family=29",
        ).appearance;
        assert_eq!(appearance.cursor_family, Some(PACK));
        assert_eq!(appearance.cursor_variants[0][1], PACK + 1);
    }
}

pub fn preview(canvas: &mut Canvas, cursor: Cursor, rect: Rect) -> bool {
    let Some(bitmap) = bitmap(cursor) else { return false };
    canvas.image_rgba(rect, SIZE, SIZE, &bitmap.rgba);
    true
}

pub fn source(cursor: Cursor, scale: f32) -> Option<CustomCursorSource> {
    let bitmap = bitmap(cursor)?;
    let side = (SIZE as f32 * scale).ceil() as usize;
    let tint = if crate::vintage::enabled() { None } else { os_theme::current().appearance.cursor_color };
    let mut rgba = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let i = ((y * SIZE / side) * SIZE + x * SIZE / side) * 4;
            let p = &bitmap.rgba[i..i + 4];
            if let Some(Color { r, g, b, .. }) = tint.filter(|_| p[0] > 127) {
                rgba.extend_from_slice(&[r, g, b, p[3]]);
            } else {
                rgba.extend_from_slice(p);
            }
        }
    }
    let hot = bitmap.hotspot.map(|p| (usize::from(p) * side / SIZE) as u16);
    CustomCursor::from_rgba(rgba, side as u16, side as u16, hot[0], hot[1]).ok()
}
