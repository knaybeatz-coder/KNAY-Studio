// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard-first playlist tools, native cursor feedback, and the compact source menu.
use crate::app::{Action, App, Cursor, State};
use crate::icons::{self, Icon};
use crate::playlist::{self as pl, Cmd, Mark, PItem, PickKind, PlTool, Popup};
use os_model::{ChannelKind, ClipSource};
use os_ui::{Canvas, Color, Rect};

pub const GRAB: u8 = 5;
pub const GRABBING: u8 = 6;
pub const RESIZE: u8 = 7;
pub const COPYING: u8 = 8;
pub const STRETCH: u8 = 9;
pub const GAIN: u8 = 10;
pub const POP_MS: u128 = 180;

// Append-only role IDs, persisted in portable theme text.
pub const CURSOR_CHOICES: &[(&str, Cursor)] = &[
    ("Pointer", Cursor::Default),
    ("Draw", Cursor::Playlist(PlTool::Draw, 3)),
    ("Paint", Cursor::Playlist(PlTool::Paint, 3)),
    ("Slice", Cursor::Playlist(PlTool::Slice, 3)),
    ("Erase", Cursor::Playlist(PlTool::Delete, 3)),
    ("Select", Cursor::Playlist(PlTool::Select, 3)),
    ("Mute", Cursor::Playlist(PlTool::Mute, 3)),
    ("Playback", Cursor::Playlist(PlTool::Playback, 3)),
    ("Zoom", Cursor::Playlist(PlTool::Zoom, 3)),
    ("Slip", Cursor::Playlist(PlTool::Slip, 3)),
    ("Comp", Cursor::Playlist(PlTool::Comp, 3)),
    ("Resize horizontally", Cursor::EwResize),
    ("Resize vertically", Cursor::NsResize),
    ("Resize diagonal \\", Cursor::NwseResize),
    ("Resize diagonal /", Cursor::NeswResize),
    ("Point / gain", Cursor::Playlist(PlTool::Draw, GAIN)),
    ("Open hand", Cursor::Playlist(PlTool::Draw, GRAB)),
    ("Grabbed hand", Cursor::Playlist(PlTool::Draw, GRABBING)),
    ("Copy hand", Cursor::Playlist(PlTool::Draw, COPYING)),
    ("Stretch", Cursor::Playlist(PlTool::Draw, STRETCH)),
    ("Vocal Main", Cursor::Playlist(PlTool::Draw, 11)),
    ("Vocal Select", Cursor::Playlist(PlTool::Draw, 12)),
    ("Vocal Modulation", Cursor::Playlist(PlTool::Draw, 13)),
    ("Vocal Drift", Cursor::Playlist(PlTool::Draw, 14)),
    ("Vocal Transition", Cursor::Playlist(PlTool::Draw, 15)),
    ("Vocal Formant", Cursor::Playlist(PlTool::Draw, 16)),
    ("Vocal Amplitude", Cursor::Playlist(PlTool::Draw, 17)),
    ("Vocal Time", Cursor::Playlist(PlTool::Draw, 18)),
    ("Vocal Attack", Cursor::Playlist(PlTool::Draw, 19)),
    ("Vocal Split", Cursor::Playlist(PlTool::Draw, 20)),
    ("Vocal Draw", Cursor::Playlist(PlTool::Draw, 21)),
    ("Vocal Hand", Cursor::Playlist(PlTool::Draw, 22)),
    ("Vocal Zoom", Cursor::Playlist(PlTool::Draw, 23)),
];

pub fn cursor_role(cursor: Cursor) -> usize {
    let normalized = match cursor {
        Cursor::Playlist(_, RESIZE) => Cursor::EwResize,
        Cursor::Playlist(_, phase @ 5..=23) => Cursor::Playlist(PlTool::Draw, phase),
        Cursor::Playlist(tool, _) => Cursor::Playlist(tool, 3),
        other => other,
    };
    CURSOR_CHOICES.iter().position(|(_, c)| *c == normalized).unwrap_or(0)
}

pub fn resolved_cursor_pack(appearance: os_theme::appearance::Appearance, cursor: Cursor) -> u8 {
    let role = cursor_role(cursor);
    appearance.cursor_variants[role / 16][role % 16].checked_sub(1)
        .unwrap_or_else(|| appearance.cursor_family.unwrap_or(appearance.icon_pack))
}

pub const CURSOR_PACK_COUNT: u8 = crate::vintage_cursors::PACK + 1;

pub fn cursor_pack_name(pack: u8) -> &'static str {
    if pack == crate::vintage_cursors::PACK { "FL Studio 2005" } else { os_ui::icon_packs::get(pack).name }
}

fn cursor_art_pack(pack: u8) -> u8 {
    if pack == crate::vintage_cursors::PACK { os_theme::appearance::VINTAGE_ICON_PACK } else { pack }
}


/// Lightweight preview: uses the same semantic artwork without generating a native bitmap.
pub fn paint_cursor_sample(cv: &mut Canvas, cursor: Cursor, pack: u8, rect: Rect, color: Color) {
    if pack == crate::vintage_cursors::PACK && crate::vintage_cursors::preview(cv, cursor, rect) { return; }
    let pack = cursor_art_pack(pack);
    use icons::Gesture;
    let old = cv.icon_pack();
    let old_color = cv.icon_color();
    cv.set_icon_color(None);
    cv.set_icon_pack(pack, true);
    let shape = match cursor {
        Cursor::Playlist(_, GAIN) => Some(icons::gesture(Gesture::Point, pack.max(1))),
        Cursor::Playlist(_, GRAB) => Some(icons::gesture(Gesture::Grab, pack.max(1))),
        Cursor::Playlist(_, GRABBING | COPYING) => Some(icons::gesture(Gesture::Grabbed, pack.max(1))),
        Cursor::Playlist(_, phase @ 11..=23) => Some(crate::vocal::packed_art(crate::vocal::TOOLS[usize::from(phase - 11)], pack.max(1))),
        Cursor::Playlist(tool, phase) => {
            let icon = match phase { RESIZE => Icon::Slip, STRETCH => Icon::StretchBand, _ => icons::playlist_tool(tool) };
            if pack == 0 { cv.icon(icons::default_shape(icon), rect, color); None }
            else { Some(icons::artwork(icon, pack)) }
        }
        _ if pack == 0 => {
            cv.text_center(os_ui::Font::new(10), rect, color, "OS");
            None
        }
        Cursor::Default => Some(icons::gesture(Gesture::Pointer, pack)),
        Cursor::EwResize => Some(icons::artwork(Icon::Slip, pack)),
        Cursor::NsResize => Some(icons::gesture(Gesture::ResizeVertical, pack)),
        Cursor::NwseResize => Some(icons::gesture(Gesture::ResizeDiagonal, pack)),
        Cursor::NeswResize => Some(icons::gesture(Gesture::ResizeBackDiagonal, pack)),
    };
    if let Some(art) = shape { cv.art_icon(&art.shape, rect, color); }
    cv.set_icon_color(old_color);
    cv.set_icon_pack(old, false);
}

pub fn select(s: &mut State, tool: PlTool) {
    // KNAY: Draw (P), Slip (S) and Playback (Y) are real tools again (their mouse handling is in playlist.rs).
    s.playlist.tool = tool;
    s.playlist.tool_changed = Some(std::time::Instant::now());
    s.playlist.overlay = None;
}
pub fn animating(s: &State) -> bool {
    !matches!(s.playlist.tool, PlTool::Draw | PlTool::Paint) && s.playlist.tool_changed.is_some_and(|t| t.elapsed().as_millis() < POP_MS)
}
pub fn phase(s: &State) -> u8 {
    match s.playlist.tool_changed.map(|t| t.elapsed().as_millis()) {
        Some(0..=44) => 0, Some(45..=89) => 1, Some(90..=134) => 2, _ => 3,
    }
}
pub fn cursor(app: &App) -> Option<Cursor> {
    if app.modal_open() || app.state.settings.open || app.state.playlist.popup.is_some() || app.state.menu_open.is_some() { return None; }
    if matches!(app.drag,Some(crate::app::Drag::Vocal(_)|crate::app::Drag::VocalNav(_)))||matches!(app.hover,Some(Action::Vocal(crate::vocal::Command::Grid(_)|crate::vocal::Command::Note(..)|crate::vocal::Command::Surface|crate::vocal::Command::Boundary(..)))){
        use crate::vocal::Tool;let t=app.state.playlist.vocal.tool;
        if matches!(app.hover,Some(Action::Vocal(crate::vocal::Command::Boundary(..)))){return Some(Cursor::Playlist(PlTool::Slip,RESIZE));}
        if t==Tool::Main && !matches!(app.drag,Some(crate::app::Drag::Vocal(_)))
            && !matches!(app.hover,Some(Action::Vocal(crate::vocal::Command::Note(..)))) {
            return Some(Cursor::Playlist(PlTool::Select,3));
        }
        let(tool,phase)=match t{Tool::Draw=>(PlTool::Draw,3),Tool::Split=>(PlTool::Slice,3),Tool::Time|Tool::Attack=>(PlTool::Slip,RESIZE),Tool::Main|Tool::Hand=>(PlTool::Select,if app.drag.is_some(){GRABBING}else{GRAB}),Tool::Zoom=>(PlTool::Zoom,3),_ =>(PlTool::Select,11+crate::vocal::TOOLS.iter().position(|p|*p==t).unwrap_or(0)as u8)};return Some(Cursor::Playlist(tool,phase));
    }
    if matches!(app.drag,Some(crate::app::Drag::Pl(pl::PlDrag::Gain { .. } | pl::PlDrag::Fade { .. } | pl::PlDrag::FadeTension { .. } | pl::PlDrag::Crossfade { .. }))) {
        return Some(Cursor::Playlist(app.state.playlist.tool,GAIN));
    }
    if matches!(app.drag,Some(crate::app::Drag::NotePaint { .. })) {
        return Some(Cursor::Playlist(PlTool::Paint,3));
    }
    if app.placed_cursor_at.is_some() {
        if matches!(app.hover,Some(Action::PianoGrid|Action::PianoNote(_))) || matches!(app.drag,Some(crate::app::Drag::NoteMove { .. })) {
            let tool=if app.state.piano.tool==crate::piano::Tool::Paint {PlTool::Paint}else{PlTool::Draw};
            return Some(Cursor::Playlist(tool,3));
        }
        if app.hover==Some(Action::PlaylistArea) || matches!(app.drag,Some(crate::app::Drag::Pl(pl::PlDrag::Move { .. }))) {
            return Some(Cursor::Playlist(app.state.playlist.tool,3));
        }
    }
    // Keep the held gesture visible when the pointer leaves its original note or clip.
    match &app.drag {
        Some(crate::app::Drag::NoteMove { .. }) =>
            return Some(Cursor::Playlist(PlTool::Draw, if app.shift { COPYING } else { GRABBING })),
        Some(crate::app::Drag::NoteStretch { .. }) => return Some(Cursor::Playlist(PlTool::Slip,STRETCH)),
        Some(crate::app::Drag::NoteResize { .. }) =>
            return Some(Cursor::Playlist(PlTool::Draw, RESIZE)),
        Some(crate::app::Drag::Pl(pl::PlDrag::Move { .. })) =>
            return Some(Cursor::Playlist(app.state.playlist.tool, if app.shift { COPYING } else { GRABBING })),
        Some(crate::app::Drag::Pl(pl::PlDrag::ResizeL { .. } | pl::PlDrag::ResizeR { .. })) =>
            return Some(Cursor::Playlist(app.state.playlist.tool, RESIZE)),
        _ => {}
    }
    if matches!(app.hover,Some(Action::PianoGrid|Action::PianoNote(_))) {
        use crate::piano::Tool;
        let tool=if matches!(app.drag,Some(crate::app::Drag::NoteErase)) {PlTool::Delete}
            else if app.ctrl {PlTool::Select}
            else {match app.state.piano.tool {
                Tool::Draw=>PlTool::Draw, Tool::Paint=>PlTool::Paint, Tool::Delete=>PlTool::Delete,
                Tool::Mute=>PlTool::Mute, Tool::Slice=>PlTool::Slice, Tool::Select=>PlTool::Select,
                Tool::Stretch=>PlTool::Slip, Tool::Zoom=>PlTool::Zoom, Tool::Playback=>PlTool::Playback,
            }};
        if app.resize_cursor && matches!(tool,PlTool::Draw|PlTool::Paint|PlTool::Select) && !app.ctrl {
            let phase=if app.state.piano.stretch && matches!(app.state.piano.tool,Tool::Draw|Tool::Paint) {STRETCH}else{RESIZE};
            return Some(Cursor::Playlist(tool,phase));
        }
        if !app.ctrl && matches!(app.hover,Some(Action::PianoNote(_))) && matches!(tool,PlTool::Draw|PlTool::Paint|PlTool::Select) {
            return Some(Cursor::Playlist(tool,GRAB));
        }
        let copying=app.shift && matches!(app.drag,Some(crate::app::Drag::NoteMove{..}));
        return Some(Cursor::Playlist(tool,if copying{4}else{3}));
    }
    if app.hover != Some(Action::PlaylistArea) { return None; }
    if matches!(app.drag,Some(crate::app::Drag::Pl(pl::PlDrag::SweepErase{..}))){return Some(Cursor::Playlist(PlTool::Delete,3));}
    let tool = if app.ctrl { PlTool::Select } else { app.state.playlist.tool };
    if !app.ctrl && let Some(grid)=app.pl_grid() {
        let g=pl::PGeom::new(&app.state,grid);
        if let Some(i)=pl::clip_at(&app.state,&g,app.mouse.0,app.mouse.1)
            && pl::handle_at(&app.state,&g,i,app.mouse.0,app.mouse.1).is_some() {
            return Some(Cursor::Playlist(tool,GAIN));
        }
        if app.drag.is_none() && !matches!(tool,PlTool::Select|PlTool::Comp)
            && let Some(i)=pl::clip_at(&app.state,&g,app.mouse.0,app.mouse.1) {
            let clip=pl::clips(&app.state)[i];let r=g.clip_rect(&clip);
            let menu=app.mouse.0<r.x+14. && app.mouse.1<r.y+pl::TITLE_H && r.w>20.;
            if menu && pl::source_kind(&app.state,clip.source)==PickKind::Audio
                && !(app.resize_cursor && matches!(tool,PlTool::Draw|PlTool::Paint)) {
                return Some(Cursor::Playlist(tool,GAIN));
            }
        }
    }
    if !app.ctrl && matches!(tool, PlTool::Draw | PlTool::Paint | PlTool::Select)
        && let Some(grid) = app.pl_grid()
    {
        let g = pl::PGeom::new(&app.state, grid);
        let (x, y) = app.mouse;
        if let Some(i) = pl::clip_at(&app.state, &g, x, y) {
            let clip = &pl::clips(&app.state)[i];
            if pl::source_kind(&app.state, clip.source) == PickKind::Automation
                && (pl::point_near(&app.state, &g, i, x, y).is_some()
                    || (!app.resize_cursor && tool != PlTool::Select && y < g.body(clip).y))
            {
                return Some(Cursor::Playlist(tool, GAIN));
            }
        }
    }
    if !app.ctrl && app.resize_cursor && matches!(tool, PlTool::Draw | PlTool::Paint) { return Some(Cursor::Playlist(tool,RESIZE)); }

    if matches!(tool,PlTool::Draw|PlTool::Paint) && app.drag.is_none() {
        if let Some(grid)=app.pl_grid() {
            let g=pl::PGeom::new(&app.state,grid);
            if let Some(i)=pl::clip_at(&app.state,&g,app.mouse.0,app.mouse.1) {
                let clip=pl::clips(&app.state)[i];
                let automation=matches!(clip.source,ClipSource::Channel(id) if app.state.project.automation(id).is_some());
                let rect=g.clip_rect(&clip);
                let menu=app.mouse.0<rect.x+14. && app.mouse.1<rect.y+pl::TITLE_H && rect.w>20.;
                if !automation && !menu && pl::handle_at(&app.state,&g,i,app.mouse.0,app.mouse.1).is_none() {
                    return Some(Cursor::Playlist(tool,GRAB));
                }
            }
        }
    }
    Some(Cursor::Playlist(tool, phase(&app.state)))
}

fn hotspot(tool: PlTool, scale: f32) -> (u16,u16) {
    let (x,y)=match tool { PlTool::Draw | PlTool::Paint | PlTool::Slice => (6.,26.), _ => (6.,6.) };
    ((x*scale).round() as u16,(y*scale).round() as u16)
}

/// The tool is the pointer: its tip stays on the edit position through the small size animation.
/// Canvas icons use an opaque-surface RGB compositor, so render a coverage mask first and explicitly
/// build valid premultiplied RGBA. Drawing them straight onto transparent pixels loses their alpha.
#[cfg(test)]
pub fn cursor_canvas(tool: PlTool, phase: u8, scale: f32) -> Option<Canvas> {
    packed_cursor_canvas(tool, phase, scale, 0)
}

fn default_cursor_canvas(tool: PlTool, phase: u8, scale: f32) -> Option<Canvas> {
    let scale = scale.clamp(0.5,4.0);
    let px = (32.0*scale).ceil() as u32;
    let mut mask = Canvas::new(px,px,scale).ok()?;
    mask.set_icon_pack(0, true);
    mask.begin_frame(None);
    mask.clear(Color::rgb(0x000000));
    let icon = match tool { PlTool::Draw => Icon::Pencil, PlTool::Paint => Icon::Brush, PlTool::Slice => Icon::Slice, PlTool::Select | PlTool::Comp => Icon::Select, PlTool::Delete => Icon::Delete, PlTool::Mute => Icon::Mute, PlTool::Zoom => Icon::Zoom, PlTool::Slip => Icon::Slip, PlTool::Playback => Icon::Play };
    let size = [19.5,21.5,21.2,21.0][usize::from(phase.min(3))];
    let (tip_x,tip_y,at_x,at_y)=match tool {
        PlTool::Draw | PlTool::Paint => (3.,21.,6.,26.),
        PlTool::Slice => (3.,19.,6.,26.),
        _ => (4.,4.,6.,6.),
    };
    let r=Rect::new(at_x-tip_x*size/24.,at_y-tip_y*size/24.,size,size);
    if phase>=11 {
        let t=crate::vocal::TOOLS[usize::from(phase-11).min(crate::vocal::TOOLS.len()-1)];mask.icon_stroke(&crate::vocal::tool_icon(t),Rect::new(4.,3.,24.,24.),Color::rgb(0xffffff),1.55);
    } else if phase>=GRAB {
        use os_ui::tabler::TablerIcon;
        let hand=TablerIcon { filled:false, d: if phase==GAIN {
            "M 9 12 L 9 4 Q 9 2 11 2 Q 13 2 13 4 L 13 11 L 14 10 Q 15 9 16 11 Q 18 10 19 12 Q 22 12 21 16 L 20 20 Q 19 22 16 22 L 11 22 Q 9 22 7 19 L 4 15 Q 3 13 5 12 Q 6 12 9 15 Z"
        } else if phase==GRAB {
            "M 8 12 L 8 5 Q 8 2 10 3 Q 11 3 11 5 L 11 11 L 11 3 Q 11 1 13 2 Q 14 2 14 4 L 14 11 L 14 5 Q 14 3 16 4 Q 17 4 17 6 L 17 12 L 17 8 Q 17 6 19 7 Q 20 7 20 9 L 20 16 Q 20 21 16 22 L 11 22 Q 9 21 7 18 L 3 13 Q 2 10 5 11 L 8 14"
        } else {
            "M 7 12 L 7 9 Q 7 7 9 7 Q 11 7 11 9 L 11 11 L 11 8 Q 11 6 13 6 Q 15 6 15 8 L 15 11 L 15 9 Q 15 7 17 8 Q 18 8 18 10 L 18 12 Q 21 11 21 14 L 21 17 Q 21 21 17 22 L 11 22 Q 8 21 6 18 L 3 14 Q 2 11 5 11 L 8 14"
        }};
        let rect=Rect::new(4.,3.,24.,24.);
        if phase==STRETCH {mask.icon_stroke(icons::default_shape(Icon::StretchBand),rect,Color::rgb(0xffffff),1.55);}
        else if phase==RESIZE {mask.icon_stroke(&os_ui::tabler::ARROWS_HORIZONTAL,rect,Color::rgb(0xffffff),1.55);}
        else{mask.icon_stroke(&hand,rect,Color::rgb(0xffffff),1.55);}
    } else {
        // Cursor-only weight: toolbar icons keep their existing artwork and stroke.
        let brush=os_ui::tabler::TablerIcon{filled:false,d:"M 3.0000 21.0000 L 3.3600 17.3600 C 3.5588 15.3498 5.3498 13.5588 7.3600 13.3600 C 9.3702 13.1612 10.8388 14.6298 10.6400 16.6400 C 10.4412 18.6502 8.6502 20.4412 6.6400 20.6400 L 3.0000 21.0000 M 21.0000 3.0000 C 15.6377 4.2467 10.8145 8.2515 8.4340 13.4340 M 21.0000 3.0000 C 19.7533 8.3623 15.7485 13.1855 10.5660 15.5660 M 10.9960 9.3960 C 12.6834 10.0374 13.9626 11.3166 14.6040 13.0040"};
        let shape=if tool==PlTool::Paint{&brush}else{icons::default_shape(icon)};
        mask.icon_stroke(shape,r,Color::rgb(0xffffff),1.55*size/24.);
    }
    if phase==4 || phase==COPYING {
        mask.fill_rrect(Rect::new(19.,17.,12.,13.),2.,Color::rgb(0));
        mask.fill_polygon(&[(25.,18.),(30.,23.),(27.,23.),(27.,28.),(23.,28.),(23.,23.),(20.,23.)],Color::rgb(0xffffff));
    }
    cursor_rgba(mask, 0)
}

pub fn packed_cursor_canvas(tool: PlTool, phase: u8, scale: f32, pack: u8) -> Option<Canvas> {
    if pack == 0 {
        return default_cursor_canvas(tool, phase, scale);
    }
    use icons::Gesture;
    let cursor_pack = pack;
    let pack = cursor_art_pack(pack);
    let scale = scale.clamp(0.5, 4.0);
    let px = (32.0 * scale).ceil() as u32;
    let mut mask = Canvas::new(px, px, scale).ok()?;
    mask.set_icon_pack(pack, true);
    mask.begin_frame(None);
    mask.clear(Color::rgb(0));
    let white = Color::rgb(0xffffff);
    let draw = |mask: &mut Canvas, art: &icons::Art, r: Rect| mask.art_icon(&art.shape, r, white);
    let aligned = |art: &icons::Art, size: f32, target: (f32, f32)| {
        let (x, y) = art.hotspot.expect("cursor artwork has an anchor");
        Rect::new(target.0 - x * size / 24.0, target.1 - y * size / 24.0, size, size)
    };
    let center = Rect::new(4.0, 4.0, 24.0, 24.0);
    match phase {
        GAIN => {
            let art = icons::gesture(Gesture::Point, pack);
            draw(&mut mask, art, aligned(art, 23.0, (15.0, 5.0)));
        }
        GRAB => draw(&mut mask, icons::gesture(Gesture::Grab, pack), center),
        GRABBING | COPYING => draw(&mut mask, icons::gesture(Gesture::Grabbed, pack), center),
        RESIZE => draw(&mut mask, icons::artwork(Icon::Slip, pack), center),
        STRETCH => draw(&mut mask, icons::artwork(Icon::StretchBand, pack), center),
        11..=u8::MAX => {
            let t = crate::vocal::TOOLS[usize::from(phase - 11).min(crate::vocal::TOOLS.len() - 1)];
            draw(&mut mask, crate::vocal::packed_art(t, pack), center);
        }
        _ => {
            let icon = icons::playlist_tool(tool);
            let art = icons::artwork(icon, pack);
            let size = [20.0, 21.8, 21.4, 21.0][usize::from(phase.min(3))];
            if art.hotspot.is_some() {
                let target = if matches!(tool, PlTool::Draw | PlTool::Paint | PlTool::Slice) {
                    (6.0, 26.0)
                } else {
                    (6.0, 6.0)
                };
                draw(&mut mask, art, aligned(art, size, target));
            } else {
                // The small pointer marks the real click location. The adjacent glyph names the active tool.
                let pointer = icons::gesture(Gesture::Pointer, pack);
                draw(&mut mask, pointer, aligned(pointer, 15.0, (6.0, 6.0)));
                draw(&mut mask, art, Rect::new(12.0, 10.0, 18.0, 18.0));
            }
        }
    }
    if phase == 4 || phase == COPYING {
        mask.fill_rrect(Rect::new(21.0, 19.0, 10.0, 11.0), 2.0, Color::rgb(0));
        draw(&mut mask, icons::artwork(Icon::Plus, pack), Rect::new(21.0, 19.0, 10.0, 11.0));
    }
    cursor_rgba(mask, cursor_pack)
}

fn cursor_rgba(mask: Canvas, pack: u8) -> Option<Canvas> {
    let scale = mask.scale;
    let px = mask.pixmap.width();
    let style = os_ui::icon_packs::get(cursor_art_pack(pack));
    let stock = Color::rgb(if pack == crate::vintage_cursors::PACK { 0xf5f5f5 } else { style.tint });
    let tint = if pack == 0 || crate::vintage::enabled() { stock } else {
        os_theme::current().appearance.cursor_color.unwrap_or(stock)
    };
    let coverage:Vec<u8>=mask.pixmap.pixels().iter().map(|p| p.red().max(p.green()).max(p.blue())).collect();
    let mut c=Canvas::new(px,px,scale).ok()?;
    let radius=(scale * if style.shadow { 1.5 } else { 1.0 }).round().max(1.) as i32;
    let shadow = if pack == crate::vintage_cursors::PACK { [0; 3] } else { [16, 19, 22] };
    let w=px as i32;
    for y in 0..w { for x in 0..w {
        let fg=u32::from(coverage[(y*w+x) as usize]);
        let mut outline=0u32;
        for dy in -radius..=radius { for dx in -radius..=radius {
            if dx*dx+dy*dy>radius*radius { continue; }
            let (nx,ny)=(x+dx,y+dy);
            if nx>=0 && ny>=0 && nx<w && ny<w { outline=outline.max(u32::from(coverage[(ny*w+nx) as usize])); }
        }}
        let back=outline*(255-fg)/255;
        let alpha=fg+back;
        let i=(y*w+x) as usize*4;
        let pixel=&mut c.pixmap.data_mut()[i..i+4];
        for (ch,(tint,shadow)) in [tint.r, tint.g, tint.b].map(u32::from).into_iter().zip(shadow).enumerate() {
            pixel[ch]=((tint*fg+shadow*back)/255) as u8;
        }
        pixel[3]=alpha as u8;
    }}
    Some(c)
}
fn native_cursor_scale(scale: f32) -> f32 {
    // Winit uses the bitmap dimensions as AppKit points. Compensate for Retina backing
    // pixels and make the editor pointer smaller than the original logical artwork.
    let scale = if cfg!(target_os = "macos") { scale * 0.425 } else { scale };
    scale.clamp(0.5, 4.0)
}

#[cfg(test)]
pub fn cursor_source(tool: PlTool, phase: u8, scale: f32) -> Option<winit::window::CustomCursorSource> {
    packed_cursor_source(tool, phase, scale, 0)
}

pub fn packed_cursor_source(tool: PlTool, phase: u8, scale: f32, pack: u8) -> Option<winit::window::CustomCursorSource> {
    let scale = native_cursor_scale(scale);
    if pack == crate::vintage_cursors::PACK {
        if let Some(source) = crate::vintage_cursors::source(Cursor::Playlist(tool, phase), scale) { return Some(source); }
    }
    let c=packed_cursor_canvas(tool,phase,scale,pack)?;
    let bytes: Vec<u8> = c.pixmap.pixels().iter().flat_map(|p| { let p=p.demultiply(); [p.red(),p.green(),p.blue(),p.alpha()] }).collect();
    let (hx, hy) = if phase == GAIN {
        ((15. * scale).round() as u16, (5. * scale).round() as u16)
    } else if phase >= GRAB {
        let mid = (16. * scale).round() as u16;
        (mid, mid)
    } else {
        hotspot(tool, scale)
    };
    winit::window::CustomCursor::from_rgba(bytes,c.pixmap.width() as u16,c.pixmap.height() as u16,hx,hy).ok()
}

pub fn window_cursor_source(cursor: Cursor, scale: f32, pack: u8) -> Option<winit::window::CustomCursorSource> {
    if pack == 0 { return None; }
    use icons::Gesture;
    let scale = native_cursor_scale(scale);
    if pack == crate::vintage_cursors::PACK {
        if let Some(source) = crate::vintage_cursors::source(cursor, scale) { return Some(source); }
    }
    let cursor_pack = pack;
    let pack = cursor_art_pack(pack);
    let px = (32.0 * scale).ceil() as u32;
    let mut mask = Canvas::new(px, px, scale).ok()?;
    mask.set_icon_pack(pack, true);
    mask.clear(Color::rgb(0));
    let art = match cursor {
        Cursor::Default => icons::gesture(Gesture::Pointer, pack),
        Cursor::EwResize => icons::artwork(Icon::Slip, pack),
        Cursor::NsResize => icons::gesture(Gesture::ResizeVertical, pack),
        Cursor::NwseResize => icons::gesture(Gesture::ResizeDiagonal, pack),
        Cursor::NeswResize => icons::gesture(Gesture::ResizeBackDiagonal, pack),
        Cursor::Playlist(..) => return None,
    };
    let hot = art.hotspot.map_or((16.0, 16.0), |(x, y)| (x + 4.0, y + 4.0));
    mask.art_icon(&art.shape, Rect::new(4.0, 4.0, 24.0, 24.0), Color::rgb(0xffffff));
    let image = cursor_rgba(mask, cursor_pack)?;
    let bytes: Vec<u8> = image.pixmap.pixels().iter().flat_map(|p| {
        let p = p.demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }).collect();
    winit::window::CustomCursor::from_rgba(bytes, px as u16, px as u16,
        (hot.0 * scale).round() as u16, (hot.1 * scale).round() as u16).ok()
}

pub fn sources(s: &State, kind: PickKind) -> Vec<ClipSource> {
    match kind {
        PickKind::Pattern => {
            let mut nums=crate::patterns::listed(s);
            if !nums.contains(&s.pattern.max(1)) { nums.push(s.pattern.max(1)); }
            nums.sort_unstable(); nums.dedup(); nums.into_iter().map(ClipSource::Pattern).collect()
        }
        PickKind::Audio => s.project.channels.iter().filter(|c| c.kind==ChannelKind::AudioClip).map(|c| ClipSource::Channel(c.index)).collect(),
        PickKind::Automation => s.project.automations.iter().map(|a| ClipSource::Channel(a.channel)).collect(),
    }
}
pub fn clips_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items:Vec<_>=[(PickKind::Pattern,"Patterns"),(PickKind::Audio,"Audio clips"),(PickKind::Automation,"Automation clips")].into_iter().map(|(kind,label)| {
        let mut entries: Vec<_> = sources(s,kind).into_iter().map(|src| PItem::new(pl::source_name(s,src),Cmd::ChooseSource(src)).mark(Mark::Radio(pl::source(s)==src))).collect();
        if entries.is_empty() { entries.push(PItem::new("No clips yet",Cmd::Nothing).off()); }
        PItem::sub(label,entries)
    }).collect();
    items.push(PItem::new("Show clip picker panel",Cmd::TogglePicker).mark(Mark::Check(s.playlist.picker_visible)));
    Popup{x,y,items,sub:None,scroll:[0;2], cascade: Vec::new()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn individual_cursor_choices_resolve_independently() {
        let mut a = os_theme::appearance::Appearance::default();
        assert_eq!(CURSOR_CHOICES.len(), 33);
        a.icon_pack = 22;
        a.cursor_family = Some(23);
        for (role, (_, cursor)) in CURSOR_CHOICES.iter().enumerate() {
            assert_eq!(cursor_role(*cursor), role);
            assert_eq!(resolved_cursor_pack(a, *cursor), 23);
            a.cursor_variants[role / 16][role % 16] = 27;
            assert_eq!(resolved_cursor_pack(a, *cursor), 26);
            a.cursor_variants[role / 16][role % 16] = 1;
            assert_eq!(resolved_cursor_pack(a, *cursor), 0);
            a.cursor_variants[role / 16][role % 16] = 0;
        }
        assert_eq!(cursor_role(Cursor::Playlist(PlTool::Paint, RESIZE)), 11);
        assert_eq!(cursor_role(Cursor::Playlist(PlTool::Delete, GRAB)), 16);
        for phase in 0..5 {
            assert_eq!(cursor_role(Cursor::Playlist(PlTool::Draw, phase)), 1);
        }
        a.icon_pack = 0;
        a.cursor_family = None;
        a.cursor_variants[0][1] = 23;
        assert_eq!(resolved_cursor_pack(a, Cursor::Playlist(PlTool::Draw, 3)), 22);
        assert_eq!(resolved_cursor_pack(a, Cursor::Default), 0);
    }

    #[test]
    fn native_tool_cursors_are_transparent_and_scale_correctly() {
        #[cfg(target_os = "macos")]
        assert_eq!(native_cursor_scale(2.0), 0.85);
        for tool in [PlTool::Draw,PlTool::Paint,PlTool::Slice,PlTool::Select,PlTool::Delete,PlTool::Mute,PlTool::Zoom,PlTool::Slip,PlTool::Playback,PlTool::Comp] {
            for scale in [0.85,1.0,1.5,2.0,4.0] {
                for phase in [0,1,2,3,4,GRAB,GRABBING,RESIZE,COPYING,STRETCH,GAIN] {
                    let c=cursor_canvas(tool,phase,scale).unwrap();
                    let opaque=c.pixmap.pixels().iter().filter(|p| p.alpha()>100).count();
                    assert!(opaque as f32>40.*scale*scale,"{tool:?} phase {phase} scale {scale}: visible tool artwork, not just a hotspot");
                    assert!(c.pixmap.pixels().iter().all(|p| p.red()<=p.alpha() && p.green()<=p.alpha() && p.blue()<=p.alpha()),"valid premultiplied pixels");
                    let (hx,hy)=hotspot(tool,scale);
                    if phase<GRAB && matches!(tool,PlTool::Draw|PlTool::Paint|PlTool::Slice|PlTool::Select|PlTool::Comp) {assert!(c.pixmap.pixels()[usize::from(hy)*c.pixmap.width() as usize+usize::from(hx)].alpha()>0,"the tip stays on the edit position");}
                    assert_eq!(c.pixmap.pixels().last().unwrap().alpha(),0);
                    assert!(cursor_source(tool,phase,scale).is_some());
                    if phase>=3 && let Ok(dir)=std::env::var("LEMON_CURSOR_PREVIEW_DIR") {
                        std::fs::create_dir_all(&dir).unwrap();
                        c.pixmap.save_png(std::path::Path::new(&dir).join(format!("{tool:?}-{phase}-{scale}.png"))).unwrap();
                    }
                }
            }
        }
    }
}
