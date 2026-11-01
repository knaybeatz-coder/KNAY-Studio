// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard-first playlist tools, native cursor feedback, and the compact source menu.
use crate::app::{Action, App, Cursor, State};
use crate::icons::{self, Icon};
use crate::playlist::{self as pl, Cmd, Mark, PItem, PickKind, PlTool, Popup};
use os_model::{ChannelKind, ClipSource};
use os_ui::{Canvas, Color, Rect};

pub const POP_MS: u128 = 180;

pub fn select(s: &mut State, tool: PlTool) {
    let tool = match tool { PlTool::Draw => PlTool::Paint, t => t };
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
    if app.hover != Some(Action::PlaylistArea) { return None; }
    let tool = if app.ctrl { PlTool::Select } else { app.state.playlist.tool };
    if !app.ctrl && app.resize_cursor && matches!(tool, PlTool::Draw | PlTool::Paint | PlTool::Select) { return Some(Cursor::EwResize); }
    if matches!(tool, PlTool::Draw | PlTool::Paint) { return Some(Cursor::Default); }
    Some(Cursor::Playlist(tool, phase(&app.state)))
}

fn hotspot(tool: PlTool, scale: f32) -> (u16,u16) {
    let (x,y)=match tool { PlTool::Draw | PlTool::Paint | PlTool::Slice => (6.,26.), _ => (6.,6.) };
    ((x*scale).round() as u16,(y*scale).round() as u16)
}

/// The tool is the pointer: its tip stays on the edit position through the small size animation.
/// Canvas icons use an opaque-surface RGB compositor, so render a coverage mask first and explicitly
/// build valid premultiplied RGBA. Drawing them straight onto transparent pixels loses their alpha.
pub fn cursor_canvas(tool: PlTool, phase: u8, scale: f32) -> Option<Canvas> {
    let scale = scale.clamp(0.5,4.0);
    let px = (32.0*scale).ceil() as u32;
    let mut mask = Canvas::new(px,px,scale).ok()?;
    mask.begin_frame(None);
    mask.clear(Color::rgb(0x000000));
    let icon = match tool { PlTool::Draw | PlTool::Paint => Icon::Brush, PlTool::Slice => Icon::Slice, PlTool::Select | PlTool::Comp => Icon::Select, PlTool::Delete => Icon::Delete, PlTool::Mute => Icon::Mute, PlTool::Zoom => Icon::Zoom, PlTool::Slip => Icon::Slip, PlTool::Playback => Icon::Play };
    let size = [24.0,26.5,26.2,26.0][usize::from(phase.min(3))];
    let (tip_x,tip_y,at_x,at_y)=match tool {
        PlTool::Draw | PlTool::Paint => (3.,21.,6.,26.),
        PlTool::Slice => (3.,19.,6.,26.),
        _ => (4.,4.,6.,6.),
    };
    let r=Rect::new(at_x-tip_x*size/24.,at_y-tip_y*size/24.,size,size);
    icons::draw(&mut mask,icon,r,Color::rgb(0xffffff));
    let coverage:Vec<u8>=mask.pixmap.pixels().iter().map(|p| p.red().max(p.green()).max(p.blue())).collect();
    let mut c=Canvas::new(px,px,scale).ok()?;
    let radius=scale.round().max(1.) as i32;
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
        for (ch,(tint,shadow)) in [(244u32,16u32),(228,19),(182,22)].into_iter().enumerate() {
            pixel[ch]=((tint*fg+shadow*back)/255) as u8;
        }
        pixel[3]=alpha as u8;
    }}
    Some(c)
}
pub fn cursor_source(tool: PlTool, phase: u8, scale: f32) -> Option<winit::window::CustomCursorSource> {
    let c=cursor_canvas(tool,phase,scale)?;
    let bytes: Vec<u8> = c.pixmap.pixels().iter().flat_map(|p| { let p=p.demultiply(); [p.red(),p.green(),p.blue(),p.alpha()] }).collect();
    let (hx,hy)=hotspot(tool,scale.clamp(0.5,4.0));
    winit::window::CustomCursor::from_rgba(bytes,c.pixmap.width() as u16,c.pixmap.height() as u16,hx,hy).ok()
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
    let items=[(PickKind::Pattern,"Patterns"),(PickKind::Audio,"Audio clips"),(PickKind::Automation,"Automation clips")].into_iter().map(|(kind,label)| {
        let mut entries: Vec<_> = sources(s,kind).into_iter().map(|src| PItem::new(pl::source_name(s,src),Cmd::ChooseSource(src)).mark(Mark::Radio(pl::source(s)==src))).collect();
        if entries.is_empty() { entries.push(PItem::new("No clips yet",Cmd::Nothing).off()); }
        PItem::sub(label,entries)
    }).collect();
    Popup{x,y,items,sub:None,scroll:[0;2], cascade: Vec::new()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_tool_cursors_are_transparent_and_scale_correctly() {
        for tool in [PlTool::Paint,PlTool::Slice,PlTool::Select] {
            for scale in [1.0,1.5,2.0,4.0] {
                for phase in 0..4 {
                    let c=cursor_canvas(tool,phase,scale).unwrap();
                    let opaque=c.pixmap.pixels().iter().filter(|p| p.alpha()>100).count();
                    assert!(opaque as f32>40.*scale*scale,"{tool:?} phase {phase} scale {scale}: visible tool artwork, not just a hotspot");
                    assert!(c.pixmap.pixels().iter().all(|p| p.red()<=p.alpha() && p.green()<=p.alpha() && p.blue()<=p.alpha()),"valid premultiplied pixels");
                    let (hx,hy)=hotspot(tool,scale);
                    assert!(c.pixmap.pixels()[usize::from(hy)*c.pixmap.width() as usize+usize::from(hx)].alpha()>0,"the tip stays on the edit position");
                    assert_eq!(c.pixmap.pixels().last().unwrap().alpha(),0);
                    assert!(cursor_source(tool,phase,scale).is_some());
                    if phase==3 && let Ok(dir)=std::env::var("LEMON_CURSOR_PREVIEW_DIR") {
                        std::fs::create_dir_all(&dir).unwrap();
                        c.pixmap.save_png(std::path::Path::new(&dir).join(format!("{tool:?}-{scale}.png"))).unwrap();
                    }
                }
            }
        }
    }
}
