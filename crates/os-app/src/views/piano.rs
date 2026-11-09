// Copyright (C) 2026 ItsAlphaNeon
// Copyright (C) 2026 thacer13
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Piano roll window: toolbar, keyboard, note grid, ruler, overview and the velocity panel.
// spec: ui/piano-roll.md

use os_model::Note;
use os_ui::kit::lemon as lk;
use os_ui::{Color, Font, Rect};

use super::{resize_zones, window_body, FrameInfo};
use crate::app::{Action, Live, Scroll, State, Ui, Win};
use crate::icons::{self, Icon};
use crate::piano::{self, Geom, Lane, Overlay, Tool};

/// Default grid row pitch: 17 px of fill plus a 1 px separator (§3); the vertical zoom changes it (§3a).
pub const ROW: f32 = 18.0;
/// Highest key shown (B10); the lowest is 0 (C0) (§3).
pub const TOP_KEY: i32 = 131;

/// Key name in FL's naming (key 60 = C5) (§3), as the settings choose it (`crate::note_names`).
pub fn key_name(key: i32) -> String {
    crate::note_names::note_name(key)
}

pub fn is_black(key: i32) -> bool {
    matches!(key.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
}

/// `c` minus `d` on every channel (the grid's line colors are the row fill minus a constant, §3), the constant scaled by
/// the theme's grid line strength (`PRGridContrast`, spec: ui/flstheme.md §3).
fn sub(t: &os_theme::Palette, c: Color, d: u8) -> Color {
    let d = (f32::from(d) * t.pr_lines).round().clamp(0.0, 255.0) as u8;
    Color { r: c.r.saturating_sub(d), g: c.g.saturating_sub(d), b: c.b.saturating_sub(d), a: c.a }
}

/// The bar line colour at the theme's grid line strength (spec: ui/flstheme.md §3, PRGridContrast).
fn bar_line(t: &os_theme::Palette) -> Color {
    t.pr_line(t.pr_bar)
}

/// The regions of the window for a given window rect `r` (§1; offsets from the client corner at
/// x 209, y 83).
pub struct Layout {
    pub tab: Rect,
    pub overview: Rect,
    pub ruler: Rect,
    pub keys: Rect,
    pub grid: Rect,
    pub vscroll: Rect,
    pub splitter: Rect,
    pub label: Rect,
    pub panel: Rect,
}

/// `split`: how the height under the ruler is shared by the grid and the event panel (spec:
/// ui/event-panel-splitter.md; the default puts the splitter at bottom − 185, §1).
///
/// The overview (the horizontal scroll bar) is 22 px tall and the vertical scroll bar's column 22 px wide; with
/// "Small scrollbars in editors" 11 and 12 px (about half, as FL's playlist measured; spec: ui/settings-choices.md
/// "Miscellaneous"), the room going to the ruler and the grid.
pub fn layout(r: Rect, split: crate::piano_split::Split) -> Layout {
    use crate::piano_split::BAND;
    let (hbar, vcol) = if super::small_scrollbars() { (11.0, 12.0) } else { (22.0, 22.0) };
    let bottom = r.bottom();
    let gx = r.x + 73.0;
    let gr = r.right() - vcol - 1.0;
    let gy = r.y + 64.0;
    let split = gy + split.grid_h(bottom - gy - BAND);
    Layout {
        tab: Rect::new(r.x, r.y + 23.0, 71.0, 41.0),
        overview: Rect::new(r.x + 72.0, r.y + 23.0, gr - r.x - 72.0 + 2.0, hbar),
        ruler: Rect::new(r.x + 72.0, r.y + 23.0 + hbar, gr - r.x - 72.0 + 2.0, 41.0 - hbar),
        keys: Rect::new(r.x, gy, 71.0, split - gy),
        grid: Rect::new(gx, gy, gr - gx, split - gy),
        vscroll: Rect::new(gr + 1.0, r.y + 23.0, r.right() - gr - 1.0, split - r.y - 23.0),
        splitter: Rect::new(r.x, split, r.w, BAND),
        label: Rect::new(r.x, split + BAND, 71.0, bottom - split - BAND),
        panel: Rect::new(gx, split + BAND, gr - gx, bottom - split - BAND),
    }
}

/// The note grid the piano roll would paint now (where it opens, before it is painted).
pub fn grid_rect(s: &State) -> Rect {
    let (r, _) = super::frame_rects(s, Win::PianoRoll, s.win_rect(Win::PianoRoll));
    layout(r, s.piano.split).grid
}

fn harmony_width(r: Rect) -> f32 {
    if r.w>=740. {196.} else if r.w>=585. {132.} else {0.}
}

fn harmony_boxes(ui:&mut Ui,s:&State,x:f32,y:f32,w:f32,h:f32) {
    let harmony=&s.piano.harmony;
    let key_w=40.;
    for (b,text,menu,hint) in [
        (Rect::new(x,y,key_w,h),crate::harmony::ROOTS[harmony.root as usize],0,"Choose piano roll key"),
        (Rect::new(x+key_w+3.,y,w-key_w-6.,h),crate::chords::label(harmony.scale),1,"Choose piano roll scale"),
    ] {
        ui.hit(b,Action::Harmony(crate::harmony::HCmd::Menu(menu)),hint);
        super::sunken(ui,b,ui.t.sunken);
        ui.c.text_clip(Font::new(11),b.x+4.,b.cy()-6.,b.w-13.,ui.t.text,text);
        os_ui::kit::triangle(ui.c,b.right()-5.,b.cy(),2.5,os_ui::kit::Dir::Down,ui.t.text_dim);
    }
}

pub fn paint(ui: &mut Ui, s: &State, r: Rect, f: &FrameInfo) {
    let t = ui.t;
    let channel = s.project.channels.get(s.selected_channel);
    let name = channel.and_then(|c| c.name.clone()).unwrap_or_else(|| "(none)".into());
    let title = "";
    window_body(ui, r);
    let outer = r;
    let floating = !s.wins[Win::PianoRoll.slot().unwrap_or(0)].maxed && r.h >= 2.0 * crate::app::TITLE_H;
    let (r, _) = super::framed(ui, s, Win::PianoRoll, r, title, 408.0, true);
    // The target channel: name and arrow, orange with a ▾ while hovered (spec: ui/piano-target-channel.md §1).
    let name=format!("Piano roll: {name}");
    let font=Font::new(12);
    let nx=r.x+harmony_width(r)+if r.w>=572. {459.}else{415.};
    if r.w>=572. {
        for (label,cmd,on,x) in [("M",crate::piano_tools::NCmd::ChannelMute,channel.is_some_and(|c|!c.enabled),409.),("S",crate::piano_tools::NCmd::ChannelSolo,s.rack.soloed==Some(s.selected_channel),433.)] {
            let b=Rect::new(r.x+harmony_width(r)+x,r.y+2.,20.,18.);
            let hint=format!("{} channel: {}",if label=="M"{"Mute"}else{"Solo"},name);
            let hover=ui.hit(b,Action::PianoCommand(cmd),hint);
            ui.c.fill_rrect(b,3.,if on{t.accent}else if hover{t.button_hi}else{t.sunken});
            ui.c.text_center(Font::new(10),b,if on{t.sunken}else{t.text},label);
        }
    }
    // KNAY: room for the CHORDS and BASS buttons after the channel's name.
    let nw=(ui.c.text_width(font,&name)+22.).min((r.right()-68.-125.-nx).max(0.));
    if nw>24. {
        let b=Rect::new(nx,r.y+2.,nw,18.);
        let hot=ui.hit(b,Action::PianoTarget,"Target channel (wheel: channels with notes; right-click: settings)");
        ui.c.text_clip(font,b.x+4.,b.y+1.,b.w-18.,if hot {t.accent}else{t.text},&name);
        os_ui::kit::triangle(ui.c,b.right()-7.,b.cy(),3.,os_ui::kit::Dir::Down,t.text_dim);
    }
    // KNAY: the chord and bass tools straight from the piano roll: CHORDS (KNAY Congolese Chords) and BASS (Congo Bass).
    let orange = t.h(0xfb9531);
    let mut bx = nx + nw.max(0.) + 8.0;
    for (label, cmd, hint) in [
        ("CHORDS", crate::harmony::HCmd::Open, "KNAY Congolese Chords: rumba and seben progressions, arpeggio styles (Alt+P)"),
        ("BASS", crate::harmony::HCmd::OpenBass, "KNAY Congo Bass: generate a rumba / seben bass line for this channel"),
    ] {
        let f = Font::new(12);
        let b = Rect::new(bx, r.y + 3.0, ui.c.text_width(f, label) + 14.0, 17.0);
        if b.right() > r.right() - 68.0 {
            break;
        }
        let hov = ui.hit(b, Action::Harmony(cmd), hint);
        ui.c.fill_rrect(b, 3.0, if hov { orange } else { t.h(0xc8962e) });
        ui.c.text(f, b.x + 7.0, b.y + 1.0, Color::rgb(0x111111), label);
        bx = b.right() + 5.0;
    }
    let shift = if floating { -4.0 } else { 0.0 };
    toolbar(ui, s, r, shift);
    if r.h < 2.0 * crate::app::TITLE_H {
        return; // collapsed to the title bar
    }
    if s.piano.harmony.open {crate::harmony::paint(ui,s,r);return;}
    let mut l = layout(r, s.piano.split);
    if harmony_width(r)==0. && l.overview.w>=180. {
        let w=134.;
        harmony_boxes(ui,s,l.overview.x,l.overview.y,w,l.overview.h);
        l.overview.x+=w;
        l.overview.w-=w;
    }
    let g = Geom::new(s, l.grid);
    let ch = channel.map(|c| c.index);
    let pat = s.project.patterns.iter().find(|p| p.number == s.pattern);
    let notes: &[Note] = pat.map_or(&[], |p| &p.notes);
    let sel = s.piano.selection(s.pattern, ch);
    // Only a ruler time selection shades the grid, and only its columns (§10).
    let range = s.piano.range.map(|(a, b)| (g.x_of(a as f32), g.x_of(b as f32)));

    let lemon = super::lemon(ui);
    crate::piano_note_mode::paint(ui,s,l.tab);

    let active_clip=piano::active_clip(s);
    let song_grid=active_clip.map(|_|Geom::timeline(s,l.grid));
    let timeline=song_grid.as_ref().unwrap_or(&g);
    overview(ui, s, &l, timeline, notes, ch);
    // A time-signature marker shows its signature (spec: ui/pattern-time-signature.md §2).
    let markers: Vec<(u32, String)> =
        s.project.pattern(s.pattern).map_or(&[][..], |p| &p.markers).iter().map(|m| (m.position, super::playlist::marker_label(m))).collect();
    if let Some(c)=active_clip{
        let marker=(f64::from(c.position)+(f64::from(s.piano.marker)-crate::playlist::window(s,&c).0)/c.pattern_rate()).max(0.) as u32;
        ruler(ui,&l,timeline,range,marker,&[]);
    }else{ruler(ui,&l,&g,range,s.piano.marker,&markers);}
    // Lit keys: the one held with the mouse and this channel's notes playing now (§4).
    let mut lit: Vec<i32> = s.piano.held.into_iter().collect();
    lit.extend(s.piano.typed.iter().filter(|&&(c, _)| Some(c) == ch).map(|&(_, k)| k));
    lit.extend(f.sounding.iter().filter(|&&(c, _)| Some(c) == ch).map(|&(_, k)| i32::from(k)));
    ui.live(Live::Sounding, l.keys);
    keyboard(ui, &l, &g, &lit, i32::from(s.piano.harmony.root % 12));
    if s.piano.harmony.shade || s.piano.harmony.scale_only {
        ui.c.push_clip(l.keys);
        for &key in &g.keys {
            if key.rem_euclid(12)==i32::from(s.piano.harmony.root) {
                ui.c.fill_rect(Rect::new(l.keys.right()-3.,g.y_of(key),2.,(g.row-1.).max(1.)),ui.t.accent);
            }
        }
        ui.c.pop_clip();
    }
    grid(ui, s, &l, timeline, range);

    // Notes: ghosts of the other channels first (when shown, Alt+V), then this channel's (§5).
    ui.hit(l.grid, Action::PianoGrid, "Piano roll: click to add notes; Alt+wheel resizes rows; right-click empty space to deselect with Pencil or Brush");
    ui.c.push_clip(l.grid);
    let ghost = t.text_faint.with_alpha(110);
    let ghosts = s.piano.ghosts && crate::workspace_visibility::shown("visible_piano-ghost");
    for n in notes.iter().filter(|n| ghosts && Some(n.channel) != ch) {
        if let Some(nr) = g.note_rect(n).filter(|nr| nr.intersect(&l.grid).w > 0.0) {
            ui.c.stroke_rrect(nr.inset(1.5), if lemon { 3.0 } else { 2.0 }, 1.0, ghost);
        }
    }
    if let Some(session)=s.piano.clip_session.as_ref().filter(|_|active_clip.is_some()){
        for i in 0..session.parts.len(){
            if i==session.active{continue;}
            let Some(c)=session.clip(s,i) else{continue;};
            let os_model::ClipSource::Pattern(n)=c.source else{continue;};
            let mut projected=Geom::timeline(s,l.grid);projected.for_clip(s,&c);
            let bounds=Rect::span(timeline.x_of(c.position as f32),timeline.x_of(c.end() as f32),l.grid.y,l.grid.bottom()).intersect(&l.grid);
            if bounds.is_empty(){continue;}
            ui.c.push_clip(bounds);
            if let Some(p)=s.project.pattern(n){for note in &p.notes {
                if let Some(r)=projected.note_rect(note){
                    let hit=r.intersect(&bounds);
                    if !hit.is_empty(){
                        ui.c.stroke_rrect(r.inset(1.),2.,1.,t.accent.with_alpha(155));
                        ui.hit(hit,Action::PianoCommand(crate::piano_tools::NCmd::SessionPart(i,note.channel)),"Click to edit this Playlist clip");
                    }
                }
            }}
            ui.c.pop_clip();
        }
    }
    for &(pattern,channel) in &s.piano.layers {
        if active_clip.is_some(){break;}
        if pattern==s.pattern && (Some(channel)==ch || ghosts) {continue;}
        if let Some(p)=s.project.pattern(pattern){for n in p.notes.iter().filter(|n|n.channel==channel){
            if let Some(r)=g.note_rect(n).filter(|r|r.intersect(&l.grid).w>0.){ui.c.stroke_rrect(r.inset(1.),2.,1.,t.accent.with_alpha(145));}
        }}
    }
    let label_font = Font::new(11);
    for (i, n) in notes.iter().enumerate().filter(|(_, n)| Some(n.channel) == ch) {
        let Some(nr) = g.note_rect(n) else { continue };
        let vis = nr.intersect(&l.grid);
        if vis.w <= 0.0 || vis.h <= 0.0 {
            continue;
        }
        let selected = sel.contains(&i);
        let hint = format!("{}  -  velocity {}%", key_name(i32::from(n.key)), (u32::from(n.velocity) * 100 + 64) / 128);
        ui.hit(vis, Action::PianoNote(i), hint);
        let (top, bottom, rim, text) = if selected {
            (t.note_sel_top, t.note_sel_bottom, t.note_sel_rim, t.note_sel_text)
        } else if n.muted() {
            // Muted (§9): grey body, measured from FL.
            (ui.t.s(0x6d7276), ui.t.s(0x63686c), ui.t.s(0x474a4d), ui.t.tx(0xc4c8cc))
        } else {
            if n.group & 15 == 0 && t.appearance.note_groups & 1 == 0 {(t.note_top,t.note_bottom,t.note_rim,t.note_text)}else{
                let c=crate::piano_note_mode::color(t,n.group&15);
                (c.shade(0.08),c.shade(-0.08),c.shade(-0.30),
                    crate::piano_note_mode::custom_color(t, n.group).map_or(Color::rgb(0x152019), lk::ink_on))
            }
        };
        let body = Rect::new(nr.x + 1.0, nr.y + 1.0, (nr.w - 2.0).max(1.0), nr.h - 2.0);
        if lemon {
            // Rounded-square notes: rim, then the body as a soft top-light gradient.
            let rad = 3.5f32.min(nr.w / 2.0).min(nr.h / 2.0);
            ui.c.fill_rrect(nr, rad, rim);
            ui.c.fill_rect_gradient(body, top, bottom, (rad - 1.0).max(0.0));
        } else {
            ui.c.fill_rrect(nr, 2.0, rim);
            ui.c.fill_rect_gradient(body, top, bottom, 1.0);
        }
        if nr.w > 6.0 && !lemon {
            let hi = top.mix(bottom, 0.45);
            ui.c.fill_rect(Rect::new(body.x, body.y + 1.0, 1.0, body.h - 2.0), hi);
            ui.c.fill_rect(Rect::new(body.right() - 1.0, body.y + 1.0, 1.0, body.h - 2.0), hi);
        }
        // A repeating note shows a tick at every repeat interval inside its body (spec §7a).
        if let Some(iv) = n.repeat_ticks(s.project.ppq) {
            let end = n.position + piano::shown_len(s, n);
            let mut at = n.position + iv;
            let px = g.x_of((n.position + iv) as f32) - g.x_of(n.position as f32);
            while at < end && px >= 2.0 {
                let x = g.x_of(at as f32).floor();
                if x >= l.grid.x && x < l.grid.right() {
                    ui.c.fill_rect(Rect::new(x, body.y, 1.0, body.h), rim);
                }
                at += iv;
            }
        }
        // A slide note shows a small filled triangle at its right end, a note with its own portamento switch a small
        // curved stroke (spec: ui/settings-choices.md "Input: Click-and-hold and both mouse buttons (2026-10-16)";
        // flp/events/patterns.md "Note flags"; audio/note-rules.md). The key name keeps clear of them.
        let mut room = nr.w;
        if (n.slide() || n.porta()) && body.w >= 8.0 && body.h >= 4.0 {
            let h = (body.h - 4.0).clamp(3.0, 9.0);
            let w = (h * 0.7).min(body.w / 2.0 - 1.0);
            let (mut x1, cy) = (body.right() - 2.0, body.y + body.h / 2.0);
            if n.slide() {
                ui.c.fill_polygon(&[(x1 - w, cy - h / 2.0), (x1, cy), (x1 - w, cy + h / 2.0)], text);
                room = x1 - w - 2.0 - nr.x;
                x1 -= w + 2.0;
            }
            if n.porta() && x1 - w > body.x {
                // A quarter of an ellipse rising from the lower left to the upper right.
                let pts: Vec<(f32, f32)> = (0..=8)
                    .map(|k| {
                        let a = k as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
                        (x1 - w * a.cos(), cy + h / 2.0 - h * a.sin())
                    })
                    .collect();
                ui.c.polyline(&pts, 1.5, text);
                room = x1 - w - 2.0 - nr.x;
            }
        }
        if room > 12.0 && g.row >= 13.0 {
            let label = key_name(i32::from(n.key));
            let w = ui.c.text_width(label_font, &label);
            let shown = if w <= room - 5.0 { label } else { format!("{}..", &label[..1]) };
            ui.c.text_clip(label_font, nr.x + if lemon { 4.0 } else { 3.0 }, nr.y + 2.0, room - 4.0, text, &shown);
        }
    }
    // Tool overlays (§9): marquee and zoom box (orange, rounded), slice and scrub lines.
    match s.piano.overlay {
        Some(Overlay::Marquee(m) | Overlay::ZoomBox(m)) => {
            let orange = t.pill;
            ui.c.fill_rrect(m, 4.0, orange.with_alpha(48));
            ui.c.stroke_rrect(m.inset(0.5), 4.0, 1.5, orange);
        }
        Some(Overlay::Slice(tick)) => {
            let x = g.x_of(tick as f32).round();
            ui.c.fill_rect(Rect::new(x - 1.0, l.grid.y, 2.0, l.grid.h), t.pill);
        }
        Some(Overlay::Scrub(tick)) => {
            let x = g.x_of(tick).round();
            ui.c.fill_rect(Rect::new(x, l.grid.y, 1.0, l.grid.h), ui.t.tx(0xffffff).with_alpha(200));
        }
        None => {}
    }
    // Playhead: the pattern's own in pattern mode; in song mode where the song is in this pattern, while a clip of
    // it plays there.
    let head = if f.playing {
        piano::playhead_x(s, &g, f.piano_tick, f.song)
    } else if active_clip.is_some() && s.song_mode {
        piano::playhead_x(s, &g, f64::from(s.playlist.pos), true)
    } else {
        None
    };
    if let Some(x) = head {
        ui.c.fill_rect(Rect::new(x, l.grid.y, 1.5, l.grid.h), t.playhead);
    } else if let Some(tick) = (!f.playing && s.song_mode).then(|| piano::pattern_tick_in_song(s, f64::from(s.playlist.pos))).flatten() {
        // Stopped in song mode on a place where this pattern plays: a grey line there.
        let x = g.x_of(tick as f32).round();
        ui.c.fill_rect(Rect::new(x - 1.0, l.grid.y, 3.0, l.grid.h), ui.t.s(0x38373b));
        ui.c.fill_rect(Rect::new(x, l.grid.y, 1.0, l.grid.h), ui.t.s(0x53585c));
    } else if !f.playing {
        // Stopped: a grey line at the play-start marker (§10).
        let x = g.x_of(s.piano.marker as f32).round();
        ui.c.fill_rect(Rect::new(x - 1.0, l.grid.y, 3.0, l.grid.h), ui.t.s(0x38373b));
        ui.c.fill_rect(Rect::new(x, l.grid.y, 1.0, l.grid.h), ui.t.s(0x53585c));
    }
    ui.c.pop_clip();

    // Vertical scroll bar: arrow buttons at its ends; small scroll bars have none and run along the grid.
    ui.c.fill_rect(Rect::new(l.vscroll.x, l.ruler.y, l.vscroll.w, l.grid.bottom() - l.ruler.y), t.scrollbar);
    let track = if super::small_scrollbars() {
        Rect::new(l.vscroll.x + 1.0, l.grid.y, l.vscroll.w - 2.0, l.grid.h)
    } else {
        let up = Rect::new(l.vscroll.x, l.grid.y - 19.0, l.vscroll.w, 19.0);
        let down = Rect::new(l.vscroll.x, l.grid.bottom() - 19.0, l.vscroll.w, 19.0);
        arrow_button(ui, up, 2, Scroll::PianoY);
        arrow_button(ui, down, 3, Scroll::PianoY);
        Rect::new(l.vscroll.x + 1.0, up.bottom(), l.vscroll.w - 2.0, (down.y - up.bottom()).max(0.0))
    };
    super::scrollbar(ui, track, true, s.piano.scroll_y, l.grid.h, g.keys.len() as f32 * g.row, Scroll::PianoY);
    if crate::workspace_visibility::shown("visible_piano-row-zoom") {
        let zoom = Rect::new(l.vscroll.x, l.vscroll.y, l.vscroll.w, 21.);
        let hot = ui.hit(zoom, Action::PianoVerticalZoom, "Vertical zoom: drag up/down or use the wheel. Right-click resets");
        ui.c.fill_rrect(zoom.inset(1.), 3., if hot { t.button_hi } else { t.button });
        let ink = if hot { t.accent } else { t.text_dim };
        let (x, y) = (zoom.cx(), zoom.cy());
        ui.c.line(x - 3., y, x + 3., y, 1., ink);
        ui.c.polyline(&[(x - 2.5, y - 4.), (x, y - 6.5), (x + 2.5, y - 4.)], 1., ink);
        ui.c.polyline(&[(x - 2.5, y + 4.), (x, y + 6.5), (x + 2.5, y + 4.)], 1., ink);
    }

    velocity_panel(ui, s, &l, &g, notes, ch, &sel);
    resize_zones(ui, s, Win::PianoRoll, outer);
}

/// Scroll arrow button (§1a): raised face with a 1 px dark rim and a chevron pointing `dir`
/// (0 left, 1 right, 2 up, 3 down); a click steps `target` that way.
fn arrow_button(ui: &mut Ui, b: Rect, dir: u8, target: Scroll) {
    use os_ui::kit::{self, Dir};
    let (d, step) = match dir {
        0 => (Dir::Left, -1),
        1 => (Dir::Right, 1),
        2 => (Dir::Up, -1),
        _ => (Dir::Down, 1),
    };
    let act = Action::ScrollStep(target, step);
    let hov = ui.hit(b, act.clone(), "Scroll");
    let st = kit::State::new(hov, ui.down(b, &act), false);
    kit::scroll_arrow_button(ui.c, ui.t, b, d, st);
}

/// The corner button opens the same working options menu as the title triangle.
fn corner_button(ui: &mut Ui, b: Rect) {
    use os_ui::kit;
    ui.hit(b,Action::PianoOptions,"Piano roll options");
    kit::flat_button(ui.c, ui.t, b, kit::State::NORMAL);
    let (cx, cy) = ((b.x + b.w / 2.0).round(), (b.y + b.h / 2.0).round());
    ui.c.fill_rect(Rect::new(cx - 2.0, cy - 2.0, 3.0, 3.0), ui.t.s(0x000000));
}

/// Toolbar icons in the title bar: menus and helpers, undo, then editing tools. `shift` moves the icons (the floating window draws them 4 px further left, §1a).
fn toolbar(ui: &mut Ui, s: &State, r: Rect, shift: f32) {
    let t = ui.t;
    let active = s.piano.tool;
    let ox = r.x - 209.0 + shift;
    let items: [(f32, Icon, &'static str, Option<Action>); 16] = [
        (222.0, Icon::ArrowRight, "Piano roll options", Some(Action::PianoOptions)),
        (242.0, Icon::MidiControl, "Piano roll tools: quantize, chop, glue, strum, arpeggiate, randomize...", Some(Action::PianoToolsMenu)),
        (263.0, Icon::Magnet, "", Some(Action::PianoSnapMenu)),
        (283.0, Icon::Note, "Key and scale: drawing and keyboard snapping", Some(Action::Harmony(crate::harmony::HCmd::Menu(10)))),
        (305.0, Icon::Blend, "Chord workspace (Alt+P)", Some(Action::Harmony(crate::harmony::HCmd::Open))),
        (327.0, Icon::Undo, "Undo (Ctrl+Z)", Some(Action::PianoUndo)),
        (362.0, Icon::Pencil, "Draw (P)", Some(Action::PianoTool(Tool::Draw))),
        (384.0, Icon::Brush, "Paint (B): drag to paint notes along the row", Some(Action::PianoTool(Tool::Paint))),
        (406.0, Icon::Slice, "Slice (C): drag a line to cut notes", Some(Action::PianoTool(Tool::Slice))),
        (428.0, Icon::Delete, "Delete (D): click or swipe to delete notes", Some(Action::PianoTool(Tool::Delete))),
        (450.0, Icon::Mute, "Mute (T): click or swipe to mute / unmute notes", Some(Action::PianoTool(Tool::Mute))),
        (472.0, Icon::Select, "Select (E): drag a box to select notes, Ctrl adds", Some(Action::PianoTool(Tool::Select))),
        (494.0, Icon::StretchBand, "Stretch toggle: drag note ends to scale selected timing and lengths; drag note bodies to move; right-click empty grid to turn off", Some(Action::PianoTool(Tool::Stretch))),
        (515.0, Icon::Playlist, "Place Pattern to Playlist (Ctrl+Shift+Enter)", Some(Action::PianoCommand(crate::piano_tools::NCmd::PlacePattern))),
        (536.0, Icon::Zoom, "Zoom (Z): drag a box to zoom, right-click zooms out / back", Some(Action::PianoTool(Tool::Zoom))),
        (558.0, Icon::Speaker, "Playback (Y): drag to hear the notes under the line", Some(Action::PianoTool(Tool::Playback))),
    ];
    for (x, icon, hint, action) in items {
        let visible = match &action {
            Some(Action::PianoToolsMenu) => crate::workspace_visibility::shown("visible_piano-tools"),
            Some(Action::Harmony(crate::harmony::HCmd::Open)) => crate::workspace_visibility::shown("visible_piano-chords"),
            Some(Action::PianoUndo) => crate::workspace_visibility::shown("visible_piano-undo"),
            Some(Action::PianoCommand(crate::piano_tools::NCmd::PlacePattern)) =>
                crate::workspace_visibility::shown("visible_piano-place-pattern"),
            _ => true,
        };
        if !visible { continue; }
        let snap = action == Some(Action::PianoSnapMenu);
        let offset = if x > 263.0 { 43.0+harmony_width(r) } else { 0.0 };
        let b = Rect::new(ox + x + offset - 9.0, r.y + 2.0, if snap { 61.0 } else { 18.0 }, 18.0);
        if snap {
            ui.hit(b, Action::PianoSnapMenu, crate::piano_snap::hint(s));
            super::sunken(ui, b, t.sunken);
            if ui.c.icon_pack() == 0 {
                ui.c.text_center(os_ui::Font::new(11), Rect::new(b.x,b.y,b.w-9.0,b.h), t.text, s.piano.snap.label());
            } else {
                icons::draw(ui.c, Icon::Magnet, Rect::new(b.x+2.0,b.y+3.0,12.0,12.0), t.glyph);
                let label = Rect::new(b.x+15.0,b.y,b.w-24.0,b.h);
                let font = ui.c.fit_font(os_ui::Font::new(11), s.piano.snap.label(), label.w);
                ui.c.text_center(font, label, t.text, s.piano.snap.label());
            }
            os_ui::kit::triangle(ui.c,b.right()-5.0,b.cy(),2.5,os_ui::kit::Dir::Down,t.text_dim);
            let width=harmony_width(r);
            if width>0. {harmony_boxes(ui,s,b.right()+3.,b.y,width,b.h);}
            continue;
        }
        let Some(action) = action else {
            ui.off(b, hint);
            icons::draw(ui.c, icon, b, t.text_faint);
            continue;
        };
        let on = action == Action::PianoTool(active) || (action==Action::PianoTool(Tool::Stretch) && s.piano.stretch) || (action==Action::Harmony(crate::harmony::HCmd::Menu(10)) && (s.piano.harmony.edit_snap||s.piano.harmony.midi_snap||s.piano.harmony.typing_snap));
        // The magnet's hint names the current snap (spec: ui/piano-snap.md §8).
        let hint: std::borrow::Cow<'static, str> = if action == Action::PianoSnapMenu { crate::piano_snap::hint(s).into() } else { hint.into() };
        let hov = ui.hit(b, action, hint);
        let state = os_ui::kit::State::hover(hov).with_on(on);
        let c = if os_ui::kit::custom_button_in(ui.c, t, b, state, t.appearance.piano_buttons) {
            os_ui::kit::ink(t, state)
        } else if super::lemon(ui) {
            // Lemon: the active tool is a lit rounded square with a dark glyph.
            let bg = b.outset(1.0);
            if on {
                lk::lit_in(ui.c, ui.t, bg, lk::r_btn(t), hov);
            } else if hov {
                ui.c.fill_rrect(bg, lk::r_btn(t), t.button_hi);
            }
            if on { lk::ink_on_lit(ui.t) } else { t.glyph }
        } else if on {
            t.pill
        } else if hov {
            ui.t.tx(0xffffff)
        } else {
            t.glyph
        };
        icons::draw(ui.c, icon, b, c);
    }
    for x in [348.,519.] {
        let x=ox+x+43.+harmony_width(r);
        ui.c.line(x,r.y+3.,x,r.y+20.,1.5,t.text_dim);
    }
}

fn overview(ui: &mut Ui, s: &State, l: &Layout, g: &Geom, notes: &[Note], ch: Option<u16>) {
    let t = ui.t;
    let o = l.overview;
    // Leave half a screen of edit space after the content. A view placed further out by a
    // zoom or seek may still be shown, without adding more scroll range every time it moves.
    let view = l.grid.w / g.ppt;
    let len = if let Some(session)=s.piano.clip_session.as_ref().filter(|_|piano::active_clip(s).is_some()){
        (0..session.parts.len()).filter_map(|i|session.clip(s,i)).map(|c|c.end()).max().unwrap_or(0) as f32
    }else{piano::pattern_length(s,s.pattern) as f32};
    let span = s.piano.bar_span.unwrap_or((len.max(view) + view / 2.0).max(g.scroll_x + view));
    // Scroll arrows at both ends (§1a); the bar runs between them (small scroll bars: no arrows).
    let arrow = if super::small_scrollbars() { 0.0 } else { 19.0 };
    if arrow > 0.0 {
        arrow_button(ui, Rect::new(o.x, o.y, arrow, o.h), 0, Scroll::PianoX);
        arrow_button(ui, Rect::new(l.grid.right() - arrow, o.y, arrow, o.h), 1, Scroll::PianoX);
    }
    corner_button(ui, Rect::new(l.vscroll.x, o.y, l.vscroll.w, o.h));
    let track = Rect::new(o.x + arrow, o.y, (l.grid.right() - o.x - 2.0 * arrow).max(0.0), o.h);
    super::scrollbar(ui, track, false, g.scroll_x * g.ppt, l.grid.w, span * g.ppt, Scroll::PianoX);
    // Mini-map: one dash per note of the channel.
    if !crate::workspace_visibility::shown("visible_piano-map") || !crate::piano_tools::view_on("piano_mini_preview", true) {
        return;
    }
    let dash = t.note_top.mix(t.note_bottom, 0.5).mix(t.text, 0.25);
    let channel_notes = notes.iter().filter(|n| Some(n.channel) == ch);
    let (low, high) = channel_notes.clone().fold((u16::MAX, 0), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
    let center = (f32::from(low) + f32::from(high)) / 2.0;
    let pitch_span = f32::from(high.saturating_sub(low)).max(12.0);
    let clip = piano::active_clip(s);
    ui.c.push_clip(Rect::new(track.x,track.y+1.,track.w,(track.h-2.).max(0.)));
    for n in channel_notes {
        let position = clip.map_or(f64::from(n.position), |c| {
            f64::from(c.position) + (f64::from(n.position) - crate::playlist::window(s, &c).0) / c.pattern_rate()
        });
        let x = track.x + position as f32 / span * track.w;
        let w = (piano::shown_len(s, n) as f32 / span * track.w).max(1.0);
        let y = o.cy() - 1.0 - (f32::from(n.key) - center) / pitch_span * (o.h - 6.0);
        ui.c.fill_rect(Rect::new(x, y.round(), w, 2.0), dash);
    }
    ui.c.pop_clip();
}

fn ruler(ui: &mut Ui, l: &Layout, g: &Geom, range: Option<(f32, f32)>, marker: u32, markers: &[(u32, String)]) {
    let t = ui.t;
    ui.hit(Rect::new(l.grid.x, l.ruler.y, l.grid.w, l.ruler.h), Action::PianoRuler, "Drag: move the play start - right-drag: select a time range");
    // The content keeps to the bottom 19 px; small scroll bars make the ruler taller, the extra band plain.
    ui.c.fill_rect(Rect::new(l.ruler.x, l.ruler.y, l.ruler.w, l.ruler.h - 19.0), t.ruler);
    let rr = Rect::new(l.ruler.x, l.ruler.bottom() - 19.0, l.ruler.w, 19.0);
    ui.c.fill_rect(Rect::new(rr.x, rr.y, rr.w, 17.0), t.ruler);
    ui.c.fill_rect(Rect::new(rr.x, rr.y + 17.0, rr.w, 1.0), ui.t.s(0x16191c));
    ui.c.fill_rect(Rect::new(rr.x, rr.y + 18.0, rr.w, 1.0), ui.t.s(0x090c0f));
    ui.c.push_clip(Rect::new(l.grid.x, rr.y, l.grid.w, rr.h));
    // Time selection: red band (§10, colours sampled from FL).
    if let Some((x0, x1)) = range {
        let b = Rect::span(x0, x1, rr.y + 1.0, rr.y + 19.0);
        ui.c.fill_rect_gradient(b, Color::rgb(0x9c5353), Color::rgb(0x944a4b), 0.0);
        ui.c.fill_rect(Rect::new(b.x, b.y, b.w, 1.0), Color::rgb(0xbf7576));
        ui.c.fill_rect(Rect::new(b.x, b.bottom() - 2.0, b.w, 1.0), Color::rgb(0x9f5556));
        ui.c.fill_rect(Rect::new(b.x, b.bottom() - 1.0, b.w, 1.0), Color::rgb(0xb06667));
    }
    // Bar numbers on the pattern's own bars (its time-signature markers, spec: ui/pattern-time-signature.md §2).
    let bar_px = g.ppt * g.bar as f32;
    let step = if bar_px < 30.0 { 4 } else if bar_px < 60.0 { 2 } else { 1 };
    for (tick, n) in g.bar_lines(g.scroll_x + l.grid.w / g.ppt) {
        let x = g.x_of(tick as f32);
        if (n - 1) % step != 0 || x < l.grid.x - 40.0 {
            continue;
        }
        ui.c.text(Font::new(13), x + 2.0, rr.y + 1.0, t.text_dim, &format!("{n}"));
    }
    // Time markers (§10a): a flag line and the name, drawn like the playlist's marker tabs.
    for (tick, name) in markers {
        let (tick, name) = (*tick, name.as_str());
        let x = g.x_of(tick as f32).round();
        if x < l.grid.x - 200.0 || x > l.grid.right() {
            continue;
        }
        let w = ui.c.text_width(Font::new(12), name) + 8.0;
        let tab = Rect::new(x, rr.y + 1.0, w, 15.0);
        ui.c.fill_rect(tab, t.window_title);
        ui.c.fill_rect(Rect::new(x, rr.y + 1.0, 1.0, rr.h - 1.0), t.text);
        ui.c.text(Font::new(12), x + 4.0, rr.y + 2.0, t.text, name);
    }
    // Play-start marker: an orange downward triangle (§10, colours sampled from FL).
    play_start_marker(ui,g.x_of(marker as f32),rr.y);
    ui.c.pop_clip();
}

pub(super) fn play_start_marker(ui:&mut Ui,x:f32,y:f32){
    let mx = x.round() + 0.5;
    let tri = [(mx - 7.5, y + 2.0), (mx + 7.5, y + 2.0), (mx, y + 11.0)];
    ui.c.fill_polygon(&tri, ui.t.s(0x2f1818));
    let inner = [(mx - 5.5, y + 3.0), (mx + 5.5, y + 3.0), (mx, y + 9.5)];
    ui.c.fill_polygon(&inner, ui.t.h(0xdd9561));
    ui.c.fill_rect(Rect::new(mx - 5.0, y + 3.0, 10.0, 1.0), ui.t.h(0xffc08b));
}

/// `root`: the pitch class of the key chosen in the toolbar (0 = C): its keys are the grey, labelled ones, as in FL Studio.
fn keyboard(ui: &mut Ui, l: &Layout, g: &Geom, lit: &[i32], root: i32) {
    if !crate::piano_tools::classic_keyboard() { keyboard_shaped(ui,l,g,lit,root); return; }
    let k=l.keys;
    ui.hit(k,Action::PianoKeys,"Keyboard");ui.c.push_clip(k);
    // Equal-width chromatic rows, including sharp labels, as in the classic piano roll.
    for &key in &g.keys {
        let r=Rect::new(k.x,g.y_of(key),k.w-1.,g.row);
        if r.bottom()<k.y || r.y>k.bottom(){continue;}
        let black=is_black(key);let pressed=lit.contains(&key);
        let (left,right)=if pressed {(ui.t.key_lit_left,ui.t.key_lit)}
            else if ui.t.style.vintage_2005 && black {(Color::rgb(0x454f55),Color::rgb(0x252e34))}
            else if ui.t.style.vintage_2005 {(Color::rgb(0xd0d6d5),Color::rgb(0x9ba8ac))}
            else if black {(Color::rgb(0x44494e),Color::rgb(0x60666c))}
            else {(Color::rgb(0xcbd0d5),Color::rgb(0xaeb5bc))};
        ui.c.fill_rect_gradient(r,left,right,0.);
        // KNAY: the root's keys in grey, as FL Studio marks the key's root.
        if key.rem_euclid(12)==root && !pressed {ui.c.fill_rect(r,Color::rgb(0x8d949b).with_alpha(if black{150}else{200}));}
        if ui.t.style.vintage_2005 && g.row >= 8.0 {
            ui.c.fill_rect(Rect::new(r.x, r.y, r.w, 1.0), left.shade(0.12));
            ui.c.fill_rect(Rect::new(r.right() - 1.0, r.y + 1.0, 1.0, r.h - 2.0), right.shade(-0.24));
        }
        ui.c.fill_rect(Rect::new(r.x,r.bottom()-1.,r.w,1.),Color::rgb(0x656b71));
        if g.row>=10. {
            let size = (g.row - 3.0).clamp(8.0, 12.0) as u16;
            let font = if ui.t.style.vintage_2005 { Font::vintage(size) } else { Font::new(size) };
            let text_y=r.cy()-ui.c.line_height(font)/2.;
            ui.c.text_right(font,r.right()-4.,text_y,if pressed||!black {Color::rgb(0x20252a)}else{Color::rgb(0xf4f5f6)},&key_name(key));
        }
    }
    ui.c.pop_clip();
    ui.c.fill_rect(Rect::new(k.right(),k.y,1.,k.h),Color::rgb(0x131718));
}

fn keyboard_shaped(ui: &mut Ui, l: &Layout, g: &Geom, lit: &[i32], root: i32) {
    if g.keys.len() < (TOP_KEY+1) as usize {
        ui.c.push_clip(l.keys);
        for &key in &g.keys {
            let row=Rect::new(l.keys.x,g.y_of(key),l.keys.w,g.row-1.0);
            if row.bottom()<l.keys.y || row.y>l.keys.bottom(){continue;}
            let color=if lit.contains(&key){ui.t.key_lit}else if is_black(key){ui.t.key_black_left}else{ui.t.key_white_left};
            ui.c.fill_rect(row,color);
            if g.row>=10.0 {ui.c.text_right(Font::new(10),row.right()-4.0,row.y,ui.t.key_label,&key_name(key));}
        }
        ui.c.pop_clip();
        return;
    }

    let t = ui.t;
    let k = l.keys;
    ui.hit(k, Action::PianoKeys, "Keyboard");
    ui.c.push_clip(k);
    // White keys: a stepped horizontal gradient (§4), with a dark right edge. Lemon: flat keys.
    let lemon = super::lemon(ui);
    let bands = if lemon { 1 } else { 6 };
    for i in 0..bands {
        let x = k.x + (k.w - 1.0) * i as f32 / bands as f32;
        let w = (k.w - 1.0) / bands as f32 + 1.0;
        let c = if lemon { t.key_white } else { t.key_white_left.mix(t.key_white, i as f32 / (bands - 1) as f32) };
        ui.c.fill_rect(Rect::new(x, k.y, w, k.h), c);
    }
    ui.c.fill_rect(Rect::new(k.right() - 1.0, k.y, 1.0, k.h), ui.t.s(0x7e8289));
    let (lo, hi) = g.visible_keys();
    // Lit white keys: the row plus half of each neighbouring black row (the white-key shape).
    let (row, half) = (g.row, (g.row / 2.0).floor());
    for &key in lit.iter().filter(|&&key| !is_black(key) && (lo..=hi).contains(&key)) {
        let y = g.y_of(key);
        let top = if is_black(key + 1) { y - half } else { y };
        let bottom = if is_black(key - 1) { y + row - 1.0 + half } else { y + row - 1.0 };
        for i in 0..bands {
            let x = k.x + (k.w - 1.0) * i as f32 / bands as f32;
            let w = (k.w - 1.0) / bands as f32 + 1.0;
            let c = if lemon { t.key_lit } else { t.key_lit_left.mix(t.key_lit, i as f32 / (bands - 1) as f32) };
            ui.c.fill_rect(Rect::new(x, top, w, bottom - top), c);
        }
        ui.c.fill_rect(Rect::new(k.right() - 1.0, top, 1.0, bottom - top), t.key_lit_edge);
    }
    let sep = t.key_white_left.shade(-0.12);
    for key in lo..=hi {
        if !g.keys.contains(&key) { continue; }
        let y = g.y_of(key);
        match key.rem_euclid(12) {
            0 if root != 0 => {
                // KNAY: another key's root is the grey one; C keeps only its octave label.
                if row >= 10.0 {
                    let font = if t.style.vintage_2005 { Font::vintage(11).bold() } else { Font::new(12) };
                    ui.c.text_right(font, k.right() - 4.0, y + 1.0 + (row - ROW) / 2.0, t.key_label, &key_name(key));
                }
                ui.c.fill_rect(Rect::new(k.x, y + row - 1.0, k.w - 1.0, 1.0), sep);
            }
            r if r == root && !is_black(key) => {
                // KNAY: the chosen key's root, grey in the white key's shape (half of each black neighbour), labelled.
                let top = if is_black(key + 1) { y - half } else { y };
                let bottom = if is_black(key - 1) { y + row - 1.0 + half } else { y + row - 1.0 };
                if !lit.contains(&key) && lemon {
                    ui.c.fill_rect(Rect::new(k.x, top, k.w - 1.0, bottom - top), t.key_c);
                } else if !lit.contains(&key) {
                    let left = t.key_white_left.shade(-0.16);
                    ui.c.fill_rect_gradient(Rect::new(k.x, y, 45.0, row - 1.0), left, left, 0.0);
                    ui.c.fill_rect(Rect::new(k.x + 45.0, top, k.w - 46.0, bottom - top), t.key_c);
                }
                if row >= 10.0 {
                    let font = if t.style.vintage_2005 { Font::vintage(11).bold() } else { Font::new(12) };
                    ui.c.text_right(font, k.right() - 4.0, y + 1.0 + (row - ROW) / 2.0, t.key_label, &key_name(key));
                }
                if r == 5 || r == 0 {
                    ui.c.fill_rect(Rect::new(k.x, y + row - 1.0, k.w - 1.0, 1.0), sep);
                }
            }
            0 => {
                // C: darker key, extended half a row up on the right; octave label.
                if !lit.contains(&key) && lemon {
                    ui.c.fill_rect(Rect::new(k.x, y - half, k.w - 1.0, row - 1.0 + half), t.key_c);
                } else if !lit.contains(&key) {
                    let left = t.key_white_left.shade(-0.16);
                    ui.c.fill_rect_gradient(Rect::new(k.x, y, 45.0, row - 1.0), left, left, 0.0);
                    ui.c.fill_rect(Rect::new(k.x + 45.0, y - half, k.w - 46.0, row - 1.0 + half), t.key_c);
                }
                if row >= 10.0 {
                    let font = if t.style.vintage_2005 { Font::vintage(11).bold() } else { Font::new(12) };
                    ui.c.text_right(font, k.right() - 4.0, y + 1.0 + (row - ROW) / 2.0, t.key_label, &key_name(key));
                }
                ui.c.fill_rect(Rect::new(k.x, y + row - 1.0, k.w - 1.0, 1.0), sep);
            }
            5 => ui.c.fill_rect(Rect::new(k.x, y + row - 1.0, k.w - 1.0, 1.0), sep),
            _ => {}
        }
    }
    for key in lo..=hi {
        if !g.keys.contains(&key) { continue; }
        if is_black(key) {
            let y = g.y_of(key);
            let (left, right) = if lit.contains(&key) { (t.key_black_lit_left, t.key_black_lit) } else { (t.key_black_left, t.key_black) };
            if t.style.vintage_2005 {
                let face = if lit.contains(&key) { right } else { Color::rgb(0x39454b) };
                crate::vintage::metal(ui.c, Rect::new(k.x, y, 46.0_f32.min(k.w), row), face);
                continue;
            }
            if lemon {
                // Rounded at the playing end; the left end runs under the clip.
                ui.c.fill_rrect(Rect::new(k.x - 6.0, y, 51.0, row), 3.0f32.min(row / 2.0), right);
                continue;
            }
            for i in 0..6 {
                let c = left.mix(right, i as f32 / 5.0);
                let x = k.x + if i == 0 { 0.0 } else { 10.0 + 8.0 * (i - 1) as f32 };
                let w = if i == 0 { 10.0 } else if i == 5 { 3.0 } else { 8.0 };
                ui.c.fill_rect(Rect::new(x, y, w, row), c);
            }
        }
    }
    // KNAY: a black root (C#, D#, F#, G#, A#) is marked grey too, labelled when there is room.
    if is_black(root) {
        for key in lo..=hi {
            if !g.keys.contains(&key) || key.rem_euclid(12) != root || lit.contains(&key) { continue; }
            let y = g.y_of(key);
            let w = if lemon { 45.0 } else { 46.0_f32.min(k.w) };
            ui.c.fill_rect(Rect::new(k.x, y + 1.0, w, (row - 2.0).max(1.0)), t.key_c.with_alpha(220));
            if row >= 10.0 {
                ui.c.text_right(Font::new(10), k.x + w - 3.0, y + (row - ROW) / 2.0, t.key_label, &key_name(key));
            }
        }
    }
    ui.c.pop_clip();
    // Divider between keyboard and grid.
    ui.c.fill_rect(Rect::new(k.right(), l.grid.y, 1.0, k.h), ui.t.s(0x131718));
    ui.c.fill_rect(Rect::new(k.right() + 1.0, l.grid.y, 1.0, k.h), ui.t.s(0x000000));
}

/// Vertical step / beat / bar lines over `area`, in `line(fill)` colors per horizontal band.
fn columns(ui: &mut Ui, s: &State, g: &Geom, area: Rect, bands: &[(f32, f32, Color)], bar: Color) {
    let snap = s.piano.snap.grid(s.global_snap.now);
    let unit = crate::piano_snap::unit(s).unwrap_or(g.step);
    let bars_only = snap == Some(crate::playlist::Snap::Bar);
    let first = g.tick_at(area.x).floor().max(0.0) as u32;
    let end = g.tick_at(area.right()).ceil().max(0.0) as u32;
    let mut draw = |tick, strength| {
        let x = g.x_of(tick as f32).floor();
        if x >= area.x && x < area.right() {
            for &(y, h, c) in bands {
                ui.c.fill_rect(Rect::new(x, y, 1.0, h), sub(ui.t, c, strength));
            }
        }
    };
    if !bars_only {
        if unit as f32 * g.ppt >= 5.0 {
            for tick in (first - first % unit..end).step_by(unit as usize) {
                if !g.line_at(tick).0 {
                    draw(tick, if g.line_at(tick).1 { 0x12 } else { 0x0a });
                }
            }
        } else {
            for (i, &(start, _, beat)) in g.bars.iter().enumerate() {
                let stop = g.bars.get(i + 1).map_or(end, |r| r.0.min(end));
                if stop <= first || beat as f32 * g.ppt < 5.0 { continue; }
                let offset = first.saturating_sub(start);
                for tick in (start + offset - offset % beat..stop).step_by(beat as usize) {
                    draw(tick, 0x12);
                }
            }
        }
    }
    for (i, &(start, length, _)) in g.bars.iter().enumerate() {
        let stop = g.bars.get(i + 1).map_or(end, |r| r.0.min(end));
        if stop <= first { continue; }
        let offset = first.saturating_sub(start);
        for tick in (start + offset - offset % length..stop).step_by(length as usize) {
            let x = g.x_of(tick as f32).floor();
            if x >= area.x && x < area.right() {
                ui.c.fill_rect(Rect::new(x, area.y, 1.0, area.h), bar);
            }
        }
    }
}

fn grid(ui: &mut Ui, s: &State, l: &Layout, g: &Geom, range: Option<(f32, f32)>) {
    grid_rows(ui, s, l, g, l.grid, false);
    if let Some((x0, x1)) = range {
        let r = Rect::span(x0, x1, l.grid.y, l.grid.bottom()).intersect(&l.grid);
        if r.w > 0.0 {
            grid_rows(ui, s, l, g, r, true);
        }
    }
}

/// The note-grid rows and columns, clipped to `clip` (`selecting` = the red time-selection tint).
fn grid_rows(ui: &mut Ui, s: &State, l: &Layout, g: &Geom, clip: Rect, selecting: bool) {
    let t = ui.t;
    let gr = l.grid;
    let (white, black) = if selecting { (t.pr_row_sel, t.pr_row_black_sel) } else { (t.pr_row, t.pr_row_black) };
    ui.c.push_clip(clip);
    if !selecting {
        ui.c.fill_rect(gr, white.with_alpha(255));
    }
    let scale = ui.c.scale;
    let pixel = 1.0 / scale;
    let (lo, hi) = g.visible_keys();
    let mut bands = Vec::with_capacity((hi - lo + 1).max(0) as usize);
    for key in lo..=hi {
        if !g.keys.contains(&key) { continue; }
        let y = g.y_of(key);
        let fill = if s.piano.harmony.shade {if s.piano.harmony.allows(key){white}else{black.shade(-0.18)}}else if is_black(key) && crate::piano_tools::view_on("piano_grid_highlights",true) { black } else { white };
        let bottom = ((y + g.row) * scale).round() / scale;
        let line_y = bottom - pixel;
        ui.c.fill_rect(Rect::new(gr.x, y, gr.w, g.row), fill);
        ui.c.fill_rect(Rect::new(gr.x, line_y, gr.w, pixel), sub(t, white, 0x0a));
        bands.push((y, (line_y - y).max(0.0), fill));
    }
    let bar = if selecting { ui.t.s(0x0e080c) } else { bar_line(t) };
    columns(ui, s, g, gr, &bands, bar);
    ui.c.pop_clip();
}

fn velocity_panel(ui: &mut Ui, s: &State, l: &Layout, g: &Geom, notes: &[Note], ch: Option<u16>, sel: &[usize]) {
    let t = ui.t;
    // Splitter with a grip (§1): drag to share the height, double-click to hide / show the event panel (spec:
    // ui/event-panel-splitter.md). The grip stays on a collapsed band so that it can be found.
    ui.c.fill_rect(l.splitter, t.window_title);
    ui.hit(l.splitter, Action::PianoSplitter, "Event panel: drag to resize, double-click to hide or show");
    let gx = l.grid.x + l.grid.w / 2.0 - 8.0;
    for i in 0..5 {
        ui.c.fill_rect(Rect::new(gx + i as f32 * 3.0, l.splitter.y + 4.0, 1.0, 1.0), t.text_faint);
    }
    if l.panel.h < 1.0 {
        // Collapsed: only its (empty) place, which the app reads for the window's extent.
        ui.hit(l.panel, Action::PianoVelocity, "");
        return;
    }
    // Label column.
    ui.c.fill_rect(l.label, t.window_title);
    let btn = Rect::new(l.label.x, l.label.y, l.label.w, 22.0);
    let hov = ui.hit(btn, Action::PianoLaneMenu, "Event panel: choose which note property to edit");
    ui.c.text(Font::new(16), l.label.x + 12.0, l.label.y + 2.0, if hov || s.lane_menu { ui.t.tx(0xffffff) } else { t.text }, "Control");
    ui.c.fill_polygon(&[(l.label.x + 61.0, l.label.y + 9.0), (l.label.x + 65.0, l.label.y + 12.5), (l.label.x + 61.0, l.label.y + 16.0)], t.text_dim);
    ui.c.fill_rect(Rect::new(l.panel.x - 2.0, l.panel.y, 1.0, l.panel.h), ui.t.s(0x131718));
    ui.c.fill_rect(Rect::new(l.panel.x - 1.0, l.panel.y, 1.0, l.panel.h), ui.t.s(0x000205));

    if s.mpe.open{crate::mpe::paint(ui,s,l.panel,notes,sel);return;}
    // Panel grid: horizontal lines every 22 px, then the time columns (§7).
    let p = l.panel;
    ui.hit(p, Action::PianoVelocity, "Event panel: drag to set this property of notes");
    let lane = s.piano.lane;
    ui.c.push_clip(p);
    ui.c.fill_rect(p, t.pr_row);
    if let Some((a, b)) = s.piano.range {
        let r = Rect::span(g.x_of(a as f32), g.x_of(b as f32), p.y, p.bottom());
        ui.c.fill_rect(r, t.pr_row_sel);
    }
    let mut bands = Vec::new();
    let mut y = p.y;
    let mut i = 0;
    while y < p.bottom() {
        let h = 21.0f32.min(p.bottom() - y);
        bands.push((y, h, t.pr_row));
        if y + 21.0 < p.bottom() {
            ui.c.fill_rect(Rect::new(p.x, y + 21.0, p.w, 1.0), sub(t, t.pr_row, if i % 2 == 1 { 0x12 } else { 0x0a }));
        }
        y += 22.0;
        i += 1;
    }
    columns(ui, s, g, p, &bands, bar_line(t));
    ui.c.text(Font::new(16), p.x + 7.0, p.y + 2.0, t.text_faint.shade(-0.35), lane.name());

    if lane.is_channel() {
        control_bars(ui, s, g, p, lane, ch);
    }
    ui.c.pop_clip();
    if lane == Lane::ChanPitch {
        pitch_scroll(ui, s, l);
    }
    ui.c.push_clip(p);
    // Lollipops.
    for (idx, n) in notes.iter().enumerate().filter(|(_, n)| !lane.is_channel() && Some(n.channel) == ch) {
        let x = g.x_of(n.position as f32).floor();
        if x > p.right() + 4.0 {
            continue;
        }
        let end = g.x_of((n.position + piano::shown_len(s, n)) as f32);
        if end < p.x - 4.0 {
            continue;
        }
        let top = lane.y_of(p, lane.get(n)).round();
        // Bipolar lanes hang the stem from the centre line.
        let base = lane.center().map_or(p.bottom(), |c| lane.y_of(p, c).round());
        let (stem, dot) = if sel.contains(&idx) {
            (t.note_sel_top.mix(t.note_sel_bottom, 0.4), t.note_sel_rim)
        } else if n.muted() {
            (ui.t.s(0x80858a), ui.t.s(0x474a4d))
        } else {
            (t.velocity, t.velocity_dot)
        };
        ui.c.fill_rect(Rect::new(x, top.min(base), 2.0, (base - top).abs()), stem);
        ui.c.fill_rect(Rect::new(x + 4.0, top, (end - x - 4.0).max(0.0), 1.0), stem);
        ui.c.fill_circle(x + 1.0, top + 0.5, 3.5, stem);
        ui.c.fill_circle(x + 1.0, top + 0.5, 2.5, dot);
    }
    ui.c.pop_clip();
}

/// The pitch lane's own vertical scroll bar with arrow buttons, right of the panel (spec §7b): the window
/// is 482 cents tall over ±4800 cents and each arrow click moves it 455 cents. In the vertical scroll bar's column.
fn pitch_scroll(ui: &mut Ui, s: &State, l: &Layout) {
    let t = ui.t;
    let col = Rect::new(l.vscroll.x + 2.0, l.panel.y, l.vscroll.w - 3.0, l.panel.h);
    // Small scroll bars: no arrow buttons.
    let btn = if super::small_scrollbars() { 0.0 } else { 20.0_f32.min(col.h / 3.0) };
    let (up, down) = (Rect::new(col.x, col.y, col.w, btn), Rect::new(col.x, col.bottom() - btn, col.w, btn));
    let track = Rect::new(col.x, up.bottom(), col.w, (down.y - up.bottom()).max(1.0));
    // Content in cents: the top edge of the window is `4800 - pos`.
    let total = (2 * piano::PITCH_RANGE) as f32;
    let view = 482.0;
    let top_edge = (s.piano.pitch_center as f32 + view / 2.0).min(piano::PITCH_RANGE as f32);
    super::scrollbar(ui, track, true, piano::PITCH_RANGE as f32 - top_edge, view, total, Scroll::PitchY);
    let arrows = if btn > 0.0 { vec![(up, 1, "Scroll the lane up"), (down, -1, "Scroll the lane down")] } else { Vec::new() };
    for (r, dir, name) in arrows {
        let hov = ui.hit(r, Action::PitchStep(dir), name);
        if t.style.vintage_2005 {
            use os_ui::kit;
            kit::scroll_arrow_button(ui.c, t, r, if dir > 0 { kit::Dir::Up } else { kit::Dir::Down }, kit::State::hover(hov));
            continue;
        }
        ui.c.fill_rect(r, if hov { t.scrollbar.shade(0.2) } else { t.scrollbar.shade(0.1) });
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let d = if dir > 0 { -2.0 } else { 2.0 };
        // Light under High visibility (`os_ui::kit::scroll_arrow`).
        let col = os_ui::kit::scroll_arrow(t, os_ui::kit::State::hover(hov), crate::high_visibility::on()).unwrap_or(t.text_dim);
        ui.c.fill_polygon(&[(cx - 4.0, cy - d), (cx, cy + d), (cx + 4.0, cy - d)], col);
    }
}

/// Channel control lanes (spec: ui/piano-roll.md §7b): a step graph of the lane's points; each bar runs from its
/// point to the next (the last to the panel's end) and is filled from the lane's baseline to the value.
fn control_bars(ui: &mut Ui, s: &State, g: &Geom, p: Rect, lane: Lane, ch: Option<u16>) {
    let (Some(ch), Some(param)) = (ch, lane.param()) else { return };
    // Fills measured from FL; the edge of the volume bars was measured, the others are derived from their fill.
    let (fill, edge) = match lane {
        Lane::ChanVolume => (Color::rgb(0x6e4a4c), Color::rgb(0x895556)),
        Lane::ChanPan => (ui.t.a(0x57643f), ui.t.a(0x6f7d51)),
        _ => (Color::rgb(0x6b4669), Color::rgb(0x865c83)),
    };
    let kind = os_model::ControlPoint::new(0, ch, param, 0).kind;
    let points: Vec<_> = piano::controls(s).iter().filter(|c| c.kind == kind).collect();
    let center = s.piano.pitch_center;
    let base = lane.control_base(p, center).round();
    for (i, c) in points.iter().enumerate() {
        let x0 = g.x_of(c.position as f32).floor();
        let x1 = points.get(i + 1).map_or(p.right(), |n| g.x_of(n.position as f32).floor());
        if x1 < p.x || x0 > p.right() {
            continue;
        }
        let y = lane.control_y(p, c.value, center).round();
        let r = Rect::new(x0, y.min(base), (x1 - x0).max(1.0), (base - y).abs().max(1.0));
        ui.c.fill_rect(r, fill);
        ui.c.fill_rect(Rect::new(x0, r.y, 1.0, r.h), edge);
    }
}

/// Paint after editor frames so the Control menu can extend beyond the Piano Roll.
pub(super) fn paint_lane_menu(ui: &mut Ui, s: &State) {
    if !s.lane_menu || !s.z.contains(&Win::PianoRoll)
        || s.native_view.map_or(s.native_detached.contains(&Win::PianoRoll), |w| w!=Win::PianoRoll) {return;}
    let (r,_)=super::frame_rects(s,Win::PianoRoll,s.win_rect(Win::PianoRoll));
    let l=layout(r,s.piano.split);
    let (w,h)=ui.c.size();
    // Preserve the chosen popup size where it fits; keep every row available on small surfaces.
    let scale=super::popup_scale(ui.c.scale)
        .min(ui.c.scale*(w/316.).max(0.1))
        .min(ui.c.scale*(h/188.).max(0.1));
    ui.scaled(scale,|ui,k|lane_menu(ui,s,&l,k));
}

/// The event panel's Control menu (spec: ui/piano-roll.md §7a): same look as the menu bar dropdowns. `k`: the canvas is
/// at the Pop-ups scaling, a window position `x` is `x / k` (see `Ui::scaled`).
fn lane_menu(ui: &mut Ui, s: &State, l: &Layout, k: f32) {
    let t = ui.t;
    const ROW: f32 = 19.0;
    const HEAD: f32 = 16.0;
    let (w0, w1) = (170.0, 124.0);
    let h = HEAD + Lane::NOTE_LANES as f32 * ROW;
    let (win_w,win_h) = ui.c.size();
    let (width,height)=(w0+w1+6.,h+4.);
    let below=(l.label.y+24.)/k;
    let y=if below+height<=win_h-8. {below}else{l.label.y/k-height};
    let r=Rect::new(((l.label.x+9.)/k).clamp(8.,(win_w-width-8.).max(8.)),
        y.clamp(8.,(win_h-height-8.).max(8.)),width,height);
    ui.c.fill_rect(r, t.menu_pop);
    ui.c.stroke_rrect(r.inset(1.0), 0.0, 2.0, t.menu_pop_border);
    ui.hit(r, Action::Swallow, "");
    let body = r.inset(2.0);
    let font = Font::new(14);
    let head = |ui: &mut Ui, x: f32, w: f32, text: &str| {
        ui.c.fill_rect(Rect::new(x, body.y, w, HEAD), t.menu_pop_head);
        ui.c.text(Font::new(12), x + 4.0, body.y + 1.0, t.menu_pop_head_text, text);
    };
    head(ui, body.x, w0, "Note properties");
    head(ui, body.x + w0 + 2.0, w1, "Channel controls");
    ui.c.fill_rect(Rect::new(body.x + w0, body.y, 2.0, body.h), t.menu_pop_border);
    for (i, lane) in Lane::ALL.iter().enumerate() {
        // Note properties fill the left column, channel controls the right one.
        let rr = if i < Lane::NOTE_LANES {
            Rect::new(body.x, body.y + HEAD + i as f32 * ROW, w0, ROW)
        } else {
            Rect::new(body.x + w0 + 2.0, body.y + HEAD + (i - Lane::NOTE_LANES) as f32 * ROW, w1, ROW)
        };
        if ui.hit(rr, Action::PianoLanePick(i), lane.menu_name()) || *lane == s.piano.lane {
            ui.c.fill_rect(rr, t.menu_pop_hi);
        }
        ui.c.text(font, rr.x + 20.0, rr.y + 2.0, t.menu_pop_text, lane.menu_name());
    }
}

#[cfg(test)]
mod overview_clip_tests{
    use super::*;
    #[test]
    fn piano_row_seams_stay_consistent_while_scrolling() {
        let mut app = crate::app::App::new(None);
        app.state.project = crate::app::test_project();
        crate::piano::pattern_mut(&mut app.state).notes.clear();
        app.state.z = vec![Win::PianoRoll];
        app.state.vintage_2005 = true;
        let scale = 1.25;
        let mut canvas = os_ui::Canvas::new(1400, 900, scale).unwrap();
        let mut expected = None;
        for offset in 100..108 {
            app.state.piano.scroll_y = offset as f32;
            app.invalidate();
            app.paint(&mut canvas, None);
            let grid = grid_rect(&app.state);
            let g = Geom::new(&app.state, grid);
            let (lo, hi) = g.visible_keys();
            let x = (grid.x * scale).round() as u32 + 9;
            for key in lo + 1..hi {
                let y = ((g.y_of(key) + g.row) * scale).round() as u32 - 1;
                let seam = canvas.pixmap.pixel(x, y).unwrap();
                assert_eq!(seam, *expected.get_or_insert(seam), "seam at scroll {offset}, key {key}");
            }
        }
        assert!(expected.is_some());
        if let Some(path) = std::env::var_os("LEMON_VINTAGE_PIANO_PREVIEW") {
            canvas.pixmap.save_png(path).unwrap();
        }
    }

    #[test]
    fn lead_fix_piano_overview_notes_never_paint_over_scroll_arrows(){
        let mut a=crate::app::App::new(None);a.state.project=crate::app::test_project();a.state.selected_channel=0;a.state.z=vec![Win::PianoRoll];
        crate::piano::pattern_mut(&mut a.state).notes=vec![Note{length:8192,key:120,..Note::step(0,0)}];
        let mut c=os_ui::Canvas::new(1280,800,1.).unwrap();
        crate::prefs::set_text("piano_mini_preview","0");a.invalidate();a.paint(&mut c,None);let mut off=vec![0;1280*800];c.copy_to_xrgb(&mut off);
        let arrows:Vec<_>=a.hits.iter().filter(|h|matches!(h.action,Action::ScrollStep(crate::app::Scroll::PianoX,_))).map(|h|h.rect).collect();assert_eq!(arrows.len(),2);
        crate::prefs::set_text("piano_mini_preview","1");a.invalidate();a.paint(&mut c,None);let mut on=vec![0;1280*800];c.copy_to_xrgb(&mut on);assert_ne!(off,on,"mini-preview must actually draw");
        for r in arrows{for y in r.y as usize..r.bottom() as usize{for x in r.x as usize..r.right() as usize{assert_eq!(off[y*1280+x],on[y*1280+x],"preview touched arrow at {x},{y}");}}}
        if let Some(out)=std::env::var_os("LEMON_PIANO_OVERVIEW_PREVIEW"){c.pixmap.save_png(out).unwrap();}
    }
}
