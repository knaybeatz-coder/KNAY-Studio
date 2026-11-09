// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Original themed artwork for Lemon Studio's controls.

use os_ui::{Canvas, Color, Glyph, Rect};

#[path = "icons_art.rs"]
mod art;
#[path = "icons_default.rs"]
mod original;
pub(crate) use original::default_shape;
pub(crate) use art::{Art, Gesture, artwork, gesture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Minimize,
    Restore,
    Close,
    Play,
    Pause,
    Stop,
    Record,
    Metronome,
    WaitInput,
    Countdown,
    Blend,
    LoopRecord,
    Undo,
    /// `Undo` mirrored: an undone step of the undo history (the browser's History).
    Redo,
    Cut,
    Mic,
    Help,
    Save,
    SaveRender,
    Download,
    Exchange,
    Vote,
    Info,
    TypingKeyboard,
    AutoScroll,
    StepEdit,
    Slide,
    Link,
    MidiControl,
    Magnet,
    Playlist,
    PianoRoll,
    ChannelRack,
    Mixer,
    Browser,
    Project,
    Plug,
    Kick,
    Touch,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    Back,
    Wave,
    Page,
    Star,
    Folder,
    Search,
    Pencil,
    Brush,
    Delete,
    Mute,
    Slip,
    StretchBand,
    Slice,
    Select,
    Zoom,
    Speaker,
    Plus,
    Note,
    FolderOpen,
    FolderPlus,
    FileAudio,
    FileProject,
    FileText,
    FilePreset,
    FileZip,
    FileBlank,
    Gear,
    Wrench,
    User,
    Drive,
    Envelope,
    Repeat,
    Tick,
    Power,
}

pub const CHOICES: &[(Icon, &str, &str)] = &[
    (Icon::Minimize, "Minimize", "LemonStudioStyle.icon-0-color"),
    (Icon::Restore, "Restore", "LemonStudioStyle.icon-1-color"),
    (Icon::Close, "Close", "LemonStudioStyle.icon-2-color"),
    (Icon::Play, "Play", "LemonStudioStyle.icon-3-color"),
    (Icon::Pause, "Pause", "LemonStudioStyle.icon-4-color"),
    (Icon::Stop, "Stop", "LemonStudioStyle.icon-5-color"),
    (Icon::Record, "Record", "LemonStudioStyle.icon-6-color"),
    (Icon::Metronome, "Metronome", "LemonStudioStyle.icon-7-color"),
    (Icon::WaitInput, "Start on input", "LemonStudioStyle.icon-8-color"),
    (Icon::Countdown, "Recording lead-in", "LemonStudioStyle.icon-9-color"),
    (Icon::Blend, "Blend", "LemonStudioStyle.icon-10-color"),
    (Icon::LoopRecord, "Loop Record", "LemonStudioStyle.icon-11-color"),
    (Icon::Undo, "Undo", "LemonStudioStyle.icon-12-color"),
    (Icon::Redo, "Redo", "LemonStudioStyle.icon-13-color"),
    (Icon::Cut, "Cut", "LemonStudioStyle.icon-14-color"),
    (Icon::Mic, "Mic", "LemonStudioStyle.icon-15-color"),
    (Icon::Help, "Help", "LemonStudioStyle.icon-16-color"),
    (Icon::Save, "Save", "LemonStudioStyle.icon-17-color"),
    (Icon::SaveRender, "Save Render", "LemonStudioStyle.icon-18-color"),
    (Icon::Download, "Download", "LemonStudioStyle.icon-19-color"),
    (Icon::Exchange, "Exchange", "LemonStudioStyle.icon-20-color"),
    (Icon::Vote, "Vote", "LemonStudioStyle.icon-21-color"),
    (Icon::Info, "Info", "LemonStudioStyle.icon-22-color"),
    (Icon::TypingKeyboard, "Typing Keyboard", "LemonStudioStyle.icon-23-color"),
    (Icon::AutoScroll, "Auto Scroll", "LemonStudioStyle.icon-24-color"),
    (Icon::StepEdit, "Step Edit", "LemonStudioStyle.icon-25-color"),
    (Icon::Slide, "Slide", "LemonStudioStyle.icon-26-color"),
    (Icon::Link, "Link", "LemonStudioStyle.icon-27-color"),
    (Icon::MidiControl, "Midi Control", "LemonStudioStyle.icon-28-color"),
    (Icon::Magnet, "Snap", "LemonStudioStyle.icon-29-color"),
    (Icon::Playlist, "Playlist", "LemonStudioStyle.icon-30-color"),
    (Icon::PianoRoll, "Piano Roll", "LemonStudioStyle.icon-31-color"),
    (Icon::ChannelRack, "Channel Rack", "LemonStudioStyle.icon-32-color"),
    (Icon::Mixer, "Mixer", "LemonStudioStyle.icon-33-color"),
    (Icon::Browser, "Browser", "LemonStudioStyle.icon-34-color"),
    (Icon::Project, "Project", "LemonStudioStyle.icon-35-color"),
    (Icon::Plug, "Plug", "LemonStudioStyle.icon-36-color"),
    (Icon::Kick, "Kick", "LemonStudioStyle.icon-37-color"),
    (Icon::Touch, "Touch", "LemonStudioStyle.icon-38-color"),
    (Icon::ArrowRight, "Arrow Right", "LemonStudioStyle.icon-39-color"),
    (Icon::ArrowLeft, "Arrow Left", "LemonStudioStyle.icon-40-color"),
    (Icon::ArrowUp, "Arrow Up", "LemonStudioStyle.icon-41-color"),
    (Icon::ArrowDown, "Arrow Down", "LemonStudioStyle.icon-42-color"),
    (Icon::Back, "Back", "LemonStudioStyle.icon-43-color"),
    (Icon::Wave, "Wave", "LemonStudioStyle.icon-44-color"),
    (Icon::Page, "Page", "LemonStudioStyle.icon-45-color"),
    (Icon::Star, "Star", "LemonStudioStyle.icon-46-color"),
    (Icon::Folder, "Folder", "LemonStudioStyle.icon-47-color"),
    (Icon::Search, "Search", "LemonStudioStyle.icon-48-color"),
    (Icon::Pencil, "Pencil", "LemonStudioStyle.icon-49-color"),
    (Icon::Brush, "Brush", "LemonStudioStyle.icon-50-color"),
    (Icon::Delete, "Eraser / delete", "LemonStudioStyle.icon-51-color"),
    (Icon::Mute, "Mute", "LemonStudioStyle.icon-52-color"),
    (Icon::Slip, "Slip", "LemonStudioStyle.icon-53-color"),
    (Icon::StretchBand, "Stretch", "LemonStudioStyle.icon-54-color"),
    (Icon::Slice, "Slice", "LemonStudioStyle.icon-55-color"),
    (Icon::Select, "Select", "LemonStudioStyle.icon-56-color"),
    (Icon::Zoom, "Zoom", "LemonStudioStyle.icon-57-color"),
    (Icon::Speaker, "Audition / playback", "LemonStudioStyle.icon-58-color"),
    (Icon::Plus, "Plus", "LemonStudioStyle.icon-59-color"),
    (Icon::Note, "Note", "LemonStudioStyle.icon-60-color"),
    (Icon::FolderOpen, "Folder Open", "LemonStudioStyle.icon-61-color"),
    (Icon::FolderPlus, "Folder Plus", "LemonStudioStyle.icon-62-color"),
    (Icon::FileAudio, "File Audio", "LemonStudioStyle.icon-63-color"),
    (Icon::FileProject, "File Project", "LemonStudioStyle.icon-64-color"),
    (Icon::FileText, "File Text", "LemonStudioStyle.icon-65-color"),
    (Icon::FilePreset, "File Preset", "LemonStudioStyle.icon-66-color"),
    (Icon::FileZip, "File Zip", "LemonStudioStyle.icon-67-color"),
    (Icon::FileBlank, "File Blank", "LemonStudioStyle.icon-68-color"),
    (Icon::Gear, "Gear", "LemonStudioStyle.icon-69-color"),
    (Icon::Wrench, "Wrench", "LemonStudioStyle.icon-70-color"),
    (Icon::User, "User", "LemonStudioStyle.icon-71-color"),
    (Icon::Drive, "Drive", "LemonStudioStyle.icon-72-color"),
    (Icon::Envelope, "Envelope", "LemonStudioStyle.icon-73-color"),
    (Icon::Repeat, "Repeat", "LemonStudioStyle.icon-74-color"),
    (Icon::Tick, "Tick", "LemonStudioStyle.icon-75-color"),
    (Icon::Power, "Power", "LemonStudioStyle.icon-76-color"),
];

pub fn playlist_tool(tool: crate::playlist::PlTool) -> Icon {
    use crate::playlist::PlTool;
    match tool {
        PlTool::Draw => Icon::Pencil,
        PlTool::Paint => Icon::Brush,
        PlTool::Slice => Icon::Slice,
        PlTool::Select => Icon::Select,
        PlTool::Comp => Icon::Blend,
        PlTool::Delete => Icon::Delete,
        PlTool::Mute => Icon::Mute,
        PlTool::Zoom => Icon::Zoom,
        PlTool::Slip => Icon::Slip,
        PlTool::Playback => Icon::Speaker,
    }
}

pub fn chrome(cv: &mut Canvas, icon: os_ui::ChromeIcon, r: Rect, color: Color) -> bool {
    let key = match icon {
        os_ui::ChromeIcon::Left => Icon::ArrowLeft,
        os_ui::ChromeIcon::Right => Icon::ArrowRight,
        os_ui::ChromeIcon::Up => Icon::ArrowUp,
        os_ui::ChromeIcon::Down => Icon::ArrowDown,
        os_ui::ChromeIcon::Check => Icon::Tick,
        os_ui::ChromeIcon::Close => Icon::Close,
    };
    let (choice, ink) = cv.icon_override(key as usize);
    if cv.icon_pack() == 0 && choice == 0 && ink.is_none() { return false; }
    draw(cv, key, r, color);
    true
}

/// Draw a family-specific icon at the normal control bounds.
pub fn draw(cv: &mut Canvas, icon: Icon, r: Rect, c: Color) {
    draw_with_default(cv, icon, default_shape(icon), r, c);
}

/// Keep the original artwork at sites that used a separate symbol for the same action.
pub fn draw_with_default(
    cv: &mut Canvas, icon: Icon, original: &os_ui::tabler::TablerIcon, r: Rect, c: Color,
) {
    let (choice, color) = cv.icon_override(icon as usize);
    if cv.icon_pack() == 0 && choice == 0 && color.is_none() {
        cv.icon(original, r, c);
    } else {
        let family = cv.icon_pack();
        let selected = choice.checked_sub(1).unwrap_or(family);
        if selected==os_theme::appearance::VINTAGE_ICON_PACK && color.is_none() && cv.icon_color().is_none()
            && crate::vintage_skin::icon(cv,icon,r) {return;}
        cv.set_icon_pack(selected, false);
        let ink = color.map(|ink| ink.with_alpha(c.a)).unwrap_or(c);
        if selected == 0 {
            cv.icon(original, r, ink);
        } else {
            let global = cv.icon_color();
            if color.is_some() { cv.set_icon_color(color); }
            cv.art_icon(&artwork(icon, selected).shape, r, ink);
            cv.set_icon_color(global);
        }
        cv.set_icon_pack(family, false);
    }
}

/// Lemon Studio logo (greyscale mark, `logo.rs`) in one tint, fitted into a circle of radius `r`.
pub fn logo(cv: &mut Canvas, cx: f32, cy: f32, r: f32, c: Color) {
    use crate::logo::{PATHS, VIEW_H, VIEW_W};
    let k = 2.0 * r / VIEW_W.max(VIEW_H);
    let (ox, oy) = (cx - VIEW_W * k / 2.0, cy - VIEW_H * k / 2.0);
    let mut g = Glyph::new();
    for d in PATHS {
        g.commands(d, ox, oy, k, None);
    }
    cv.glyph(c, &g);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlTool;

    #[test]
    fn default_icons_use_the_original_rendering() {
        let rect = Rect::new(4.0, 4.0, 18.0, 18.0);
        let ink = Color::rgb(0xece8dd);
        let mut actual = Canvas::new(26, 26, 1.0).unwrap();
        let mut expected = Canvas::new(26, 26, 1.0).unwrap();
        actual.set_chrome_art(chrome);
        assert!(!actual.chrome_icon(os_ui::ChromeIcon::Right, rect, ink));
        assert!(crate::playlist_tools::window_cursor_source(crate::app::Cursor::Default, 1.0, 0).is_none());

        for icon in [Icon::Metronome, Icon::ChannelRack, Icon::StretchBand, Icon::Delete, Icon::Vote] {
            actual.clear(Color::rgb(0));
            expected.clear(Color::rgb(0));
            draw(&mut actual, icon, rect, ink);
            expected.icon(default_shape(icon), rect, ink);
            assert_eq!(actual.pixmap.data(), expected.pixmap.data(), "{icon:?}");
        }
    }

    #[test]
    fn individual_icons_mix_on_the_default_base() {
        let mut a = os_theme::appearance::Appearance::default();
        let role = Icon::Pencil as usize;
        a.icon_variants[role / 16][role % 16] = 23;
        let mut actual = Canvas::new(28, 28, 1.0).unwrap();
        let mut expected = Canvas::new(28, 28, 1.0).unwrap();
        let r = Rect::new(3.0, 3.0, 22.0, 22.0);
        let ink = Color::rgb(0xffffff);
        actual.set_icon_overrides(a);
        assert_eq!(actual.icon_override(role), (0, None), "legacy Default themes keep original artwork");
        a.icon_mixing = true;
        actual.set_icon_overrides(a);
        draw(&mut actual, Icon::Pencil, r, ink);
        expected.set_icon_pack(22, false);
        expected.art_icon(&artwork(Icon::Pencil, 22).shape, r, ink);
        assert_eq!(actual.pixmap.data(), expected.pixmap.data());
        assert_eq!(actual.icon_pack(), 0);
        actual.set_chrome_art(chrome);
        assert!(!actual.chrome_icon(os_ui::ChromeIcon::Close, r, ink));
        let role = Icon::Close as usize;
        a.icon_variants[role / 16][role % 16] = 24;
        actual.set_icon_overrides(a);
        assert!(actual.chrome_icon(os_ui::ChromeIcon::Close, r, ink));
    }

    #[test]
    fn authored_icon_atlas_covers_all_families_and_tools() {
        let all = [
            Icon::Minimize,
            Icon::Restore,
            Icon::Close,
            Icon::Play,
            Icon::Pause,
            Icon::Stop,
            Icon::Record,
            Icon::Metronome,
            Icon::WaitInput,
            Icon::Countdown,
            Icon::Blend,
            Icon::LoopRecord,
            Icon::Undo,
            Icon::Redo,
            Icon::Cut,
            Icon::Mic,
            Icon::Help,
            Icon::Save,
            Icon::SaveRender,
            Icon::Download,
            Icon::Exchange,
            Icon::Vote,
            Icon::Info,
            Icon::TypingKeyboard,
            Icon::AutoScroll,
            Icon::StepEdit,
            Icon::Slide,
            Icon::Link,
            Icon::MidiControl,
            Icon::Magnet,
            Icon::Playlist,
            Icon::PianoRoll,
            Icon::ChannelRack,
            Icon::Mixer,
            Icon::Browser,
            Icon::Project,
            Icon::Plug,
            Icon::Kick,
            Icon::Touch,
            Icon::ArrowRight,
            Icon::ArrowLeft,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::Back,
            Icon::Wave,
            Icon::Page,
            Icon::Star,
            Icon::Folder,
            Icon::Search,
            Icon::Pencil,
            Icon::Brush,
            Icon::Delete,
            Icon::Mute,
            Icon::Slip,
            Icon::StretchBand,
            Icon::Slice,
            Icon::Select,
            Icon::Zoom,
            Icon::Speaker,
            Icon::Plus,
            Icon::Note,
            Icon::FolderOpen,
            Icon::FolderPlus,
            Icon::FileAudio,
            Icon::FileProject,
            Icon::FileText,
            Icon::FilePreset,
            Icon::FileZip,
            Icon::FileBlank,
            Icon::Gear,
            Icon::Wrench,
            Icon::User,
            Icon::Drive,
            Icon::Envelope,
            Icon::Repeat,
            Icon::Tick,
            Icon::Power,
        ];
        let mut c = Canvas::new(24, 24, 1.0).unwrap();
        for icon in all {
            for family in 1..os_ui::icon_packs::PACKS.len() as u8 {
                let art = artwork(icon, family);
                // Universal controls may share geometry: ornament is not a
                // substitute for a readable action at toolbar size.
                assert!(!art.shape.d.is_empty(), "{icon:?}, family {family}");
                c.set_icon_pack(family, false);
                c.begin_frame(None);
                c.clear(Color::rgb(0));
                draw(
                    &mut c,
                    icon,
                    Rect::new(3.0, 3.0, 18.0, 18.0),
                    Color::rgb(0xffffff),
                );
                assert!(
                    c.pixmap.pixels().iter().filter(|p| p.red() > 25).count() >= 9,
                    "{icon:?}, family {family}: readable mark at toolbar size"
                );
            }
        }

        let tools = [
            PlTool::Draw,
            PlTool::Paint,
            PlTool::Slice,
            PlTool::Delete,
            PlTool::Select,
            PlTool::Mute,
            PlTool::Playback,
            PlTool::Zoom,
            PlTool::Slip,
            PlTool::Comp,
        ];
        for family in 1..os_ui::icon_packs::PACKS.len() as u8 {
            let eraser = crate::playlist_tools::packed_cursor_canvas(PlTool::Delete, 3, 1.0, family).unwrap();
            assert!(
                eraser.pixmap.pixels().chunks_exact(32).skip(18).take(10)
                    .any(|row| row[18..29].iter().any(|pixel| pixel.alpha() > 128)),
                "eraser badge must stay inside the cursor, family {family}"
            );
            for tool in tools {
                for phase in [
                    3,
                    crate::playlist_tools::STRETCH,
                    crate::playlist_tools::GAIN,
                    crate::playlist_tools::GRAB,
                    crate::playlist_tools::COPYING,
                    crate::playlist_tools::RESIZE,
                ] {
                    assert!(
                        crate::playlist_tools::packed_cursor_source(tool, phase, 1.5, family)
                            .is_some()
                    );
                }
            }
            for cursor in [
                crate::app::Cursor::Default,
                crate::app::Cursor::EwResize,
                crate::app::Cursor::NsResize,
                crate::app::Cursor::NwseResize,
                crate::app::Cursor::NeswResize,
            ] {
                assert!(crate::playlist_tools::window_cursor_source(cursor, 1.5, family).is_some());
            }
        }

        if let Ok(path) = std::env::var("LEMON_ICON_ATLAS_PATH") {
            let mut sheet = Canvas::new(916, 16 + 48 * os_ui::icon_packs::PACKS.len() as u32, 1.0).unwrap();
            sheet.clear(Color::rgb(0x202326));
            let icons = [
                Icon::Metronome,
                Icon::WaitInput,
                Icon::Countdown,
                Icon::LoopRecord,
                Icon::PianoRoll,
                Icon::ChannelRack,
                Icon::Mixer,
                Icon::Playlist,
                Icon::Magnet,
                Icon::Cut,
                Icon::Pencil,
                Icon::Brush,
                Icon::Slice,
                Icon::Delete,
                Icon::StretchBand,
                Icon::Select,
                Icon::Mute,
                Icon::Speaker,
                Icon::Zoom,
            ];
            for family in 0..os_ui::icon_packs::PACKS.len() as u8 {
                sheet.set_icon_pack(family, false);
                let y = 11.0 + f32::from(family) * 48.0;
                sheet.fill_rrect(Rect::new(8.0, y, 898.0, 42.0), 4.0, Color::rgb(0x2c3032));
                sheet.text(
                    os_ui::Font::new(14),
                    17.0,
                    y + 13.0,
                    Color::rgb(0xdbe1df),
                    os_ui::icon_packs::get(family).name,
                );
                for (i, icon) in icons.into_iter().enumerate() {
                    draw(
                        &mut sheet,
                        icon,
                        Rect::new(92.0 + i as f32 * 36.0, y + 12.0, 18.0, 18.0),
                        Color::rgb(0xe9ebe7),
                    );
                }
                for (i, tool) in [PlTool::Draw, PlTool::Delete, PlTool::Select]
                    .into_iter()
                    .enumerate()
                {
                    let cursor =
                        crate::playlist_tools::packed_cursor_canvas(tool, 3, 1.0, family).unwrap();
                    let px: Vec<u8> = cursor
                        .pixmap
                        .pixels()
                        .iter()
                        .flat_map(|p| {
                            let p = p.demultiply();
                            [p.red(), p.green(), p.blue(), p.alpha()]
                        })
                        .collect();
                    sheet.image_rgba(
                        Rect::new(785.0 + i as f32 * 38.0, y + 4.0, 32.0, 32.0),
                        32,
                        32,
                        &px,
                    );
                }
            }
            sheet.pixmap.save_png(path).unwrap();
        }
    }
}
