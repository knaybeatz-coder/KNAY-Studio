// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Where windows may go.
//!
//! Windows inside the main window (Playlist, Mixer, Piano roll, Channel rack, the channel settings window, the
//! Plugin performance monitor, the Tempo tapper and, on Windows, docked plugin windows), whatever General ▸ Advanced
//! "Don't limit windows to screen" says:
//! - one dragged by its title bar keeps at least `KEEP_W` px of its width inside the workspace (right of the browser,
//!   below the toolbar) at the left and at the right, and its top at least `KEEP_TOP` px above the workspace's bottom;
//!   its title bar never goes above the workspace's top (`keep`). The same holds when the main window or the
//!   workspace shrinks (`keep_all`; the floating Playlist, Mixer and Piano roll through `State::float_rect`).
//! - the kept strip can be dragged back: any part of a title bar that isn't a button starts a move (the views' title
//!   bar hits span the whole bar under the buttons).
//! - VIEW ▸ Into workspace moves every window that is partly outside fully back in, keeping its size where it fits
//!   (`App::move_into_workspace`).
//!
//! The check is about detached plugin windows (top-level windows of their own): off, a detached window's title bar
//! stays on the screens (`bar_on_screens`); on, they go anywhere. Windows only: on Linux and macOS plugin windows,
//! docked or detached, are not limited yet.
// spec: ui/settings-choices.md "Advanced: Don't limit windows to screen (2026-10-16)"

use os_ui::Rect;

use crate::app::{App, State, Win};
use crate::views;

/// Width a window keeps inside the workspace at its left and right edges (logical px).
pub const KEEP_W: f32 = 31.0;
/// How far above the workspace's bottom a window's top stays at least (logical px).
pub const KEEP_TOP: f32 = 15.0;

/// Top-left for a window `w` wide wanted at (`x`, `y`): moved as little as it takes to keep `KEEP_W` px of it inside
/// `work` at either side, its top `KEEP_TOP` px above the bottom and not above the top.
pub fn keep(x: f32, y: f32, w: f32, work: Rect) -> (f32, f32) {
    (x.min(work.right() - KEEP_W).max(work.x + KEEP_W - w), y.min(work.bottom() - KEEP_TOP).max(work.y))
}

/// `keep` in physical px at display scale `k`: top-left `p` of a window `w` wide in `work` ([left, top, right,
/// bottom]); docked plugin windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn keep_px(p: (i32, i32), w: i32, work: [i32; 4], k: f32) -> (i32, i32) {
    let (kw, kt) = ((KEEP_W * k).round() as i32, (KEEP_TOP * k).round() as i32);
    (p.0.min(work[2] - kw).max(work[0] + kw - w), p.1.min(work[3] - kt).max(work[1]))
}

/// `r` moved fully into `work`, as little as it takes; too wide or too tall, it shrinks to the workspace, though not
/// below `min`, and then starts at the workspace's left or top edge.
pub fn inside(r: Rect, work: Rect, min: (f32, f32)) -> Rect {
    let w = r.w.min(work.w).max(min.0);
    let h = r.h.min(work.h).max(min.1);
    Rect::new(r.x.min(work.right() - w).max(work.x), r.y.min(work.bottom() - h).max(work.y), w, h)
}

/// `inside` in physical px for a window that keeps its size: the new top-left of `r` ([left, top, right, bottom]).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn inside_px(r: [i32; 4], work: [i32; 4]) -> (i32, i32) {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    (r[0].min(work[2] - w).max(work[0]), r[1].min(work[3] - h).max(work[1]))
}

/// Where a detached window's title bar `bar` ([left, top, right, bottom], screen px) may be with "Don't limit windows
/// to screen" off: where it is while all of it is on the screens (`screens`, their work areas), else moved fully onto
/// the screen it overlaps most (the nearest one when it overlaps none). Returns the bar's top-left. The main window's
/// top at the start goes the same way (`layout::on_screens`).
pub fn bar_on_screens(bar: [i32; 4], screens: &[[i32; 4]]) -> (i32, i32) {
    if screens.is_empty() || covered(bar, screens) {
        return (bar[0], bar[1]);
    }
    let span = |a0: i32, a1: i32, b0: i32, b1: i32| i64::from((a1.min(b1) - a0.max(b0)).max(0));
    let gap = |a0: i32, a1: i32, b0: i32, b1: i32| i64::from((b0 - a1).max(a0 - b1).max(0));
    let best = screens.iter().max_by_key(|s| {
        let overlap = span(bar[0], bar[2], s[0], s[2]) * span(bar[1], bar[3], s[1], s[3]);
        let (dx, dy) = (gap(bar[0], bar[2], s[0], s[2]), gap(bar[1], bar[3], s[1], s[3]));
        (overlap, -(dx * dx + dy * dy))
    });
    let s = best.expect("screens is not empty");
    let (w, h) = (bar[2] - bar[0], bar[3] - bar[1]);
    (bar[0].min(s[2] - w).max(s[0]), bar[1].min(s[3] - h).max(s[1]))
}

/// Whether every point of `r` is on one of `screens` (all [left, top, right, bottom]).
fn covered(r: [i32; 4], screens: &[[i32; 4]]) -> bool {
    // Column by column between the screens' left and right edges: the screens spanning a column cover its height.
    let mut xs: Vec<i32> = screens.iter().flat_map(|s| [s[0], s[2]]).filter(|&x| x > r[0] && x < r[2]).collect();
    xs.extend([r[0], r[2]]);
    xs.sort_unstable();
    xs.dedup();
    xs.windows(2).all(|c| {
        let mut ys: Vec<(i32, i32)> = screens.iter().filter(|s| s[0] <= c[0] && s[2] >= c[1]).map(|s| (s[1], s[3])).collect();
        ys.sort_unstable();
        let mut y = r[1];
        for (t, b) in ys {
            if t > y {
                break;
            }
            y = y.max(b);
        }
        y >= r[3]
    })
}

/// Window `w`'s rectangle while it floats (the Playlist, Mixer and Piano roll as `State::float_rect` keeps them);
/// `None` for the browser panel and docked plugin windows.
pub fn rect_of(s: &State, w: Win) -> Option<Rect> {
    Some(match w {
        Win::ChannelRack => views::rack::rect(s),
        Win::Plugin => views::channel_win::rect(s),
        Win::Perf => views::perf::rect(s),
        Win::Tapper => views::tapper::rect(s),
        Win::Touch => views::touch::rect(s),
        Win::Strum => views::strum::rect(s),
        Win::Arp => views::arpeggiate::rect(s),
        Win::PianoDlg => views::piano_dlg::rect(s),
        w if w.slot().is_some() => s.float_rect(w),
        _ => return None,
    })
}

fn set_pos(s: &mut State, w: Win, pos: (f32, f32)) {
    match w {
        Win::ChannelRack => s.rack_pos = pos,
        Win::Plugin => s.plugin_pos = pos,
        Win::Perf => s.perf.pos = pos,
        Win::Tapper => s.tapper.pos = pos,
        Win::Touch => s.touch.pos = pos,
        Win::Strum => s.strum.pos = pos,
        Win::Arp => s.arp.pos = pos,
        Win::PianoDlg => s.piano_dlg.pos[s.piano_dlg.tool.index()] = pos,
        w => {
            if let Some(i) = w.slot() {
                s.wins[i].rect.x = pos.0;
                s.wins[i].rect.y = pos.1;
            }
        }
    }
}

/// Magnetic edges in logical pixels, independent of display scaling.
const SNAP_DISTANCE: f32 = 10.0;

fn snap_position(r: Rect, work: Rect, others: impl Iterator<Item = Rect>) -> (f32, f32) {
    let mut dx = SNAP_DISTANCE + 1.0;
    let mut dy = SNAP_DISTANCE + 1.0;
    let nearest = |best: &mut f32, delta: f32| {
        if delta.abs() <= SNAP_DISTANCE && delta.abs() < best.abs() { *best = delta; }
    };
    for d in [work.x - r.x, work.right() - r.right()] { nearest(&mut dx, d); }
    for d in [work.y - r.y, work.bottom() - r.bottom()] { nearest(&mut dy, d); }
    for other in others {
        // Only nearby windows attract: distant edges sharing a coordinate must not snap.
        if r.y <= other.bottom() + SNAP_DISTANCE && r.bottom() >= other.y - SNAP_DISTANCE {
            for d in [other.x - r.right(), other.right() - r.x, other.x - r.x, other.right() - r.right()] {
                nearest(&mut dx, d);
            }
        }
        if r.x <= other.right() + SNAP_DISTANCE && r.right() >= other.x - SNAP_DISTANCE {
            for d in [other.y - r.bottom(), other.bottom() - r.y, other.y - r.y, other.bottom() - r.bottom()] {
                nearest(&mut dy, d);
            }
        }
    }
    (r.x + if dx.abs() <= SNAP_DISTANCE { dx } else { 0.0 },
     r.y + if dy.abs() <= SNAP_DISTANCE { dy } else { 0.0 })
}

/// Snap the dragged window to visible neighboring edges, then keep its title reachable.
pub fn drag_to(s: &mut State, w: Win, want: (f32, f32)) {
    if let Some(mut r) = rect_of(s, w) {
        if w.slot().is_some() { r = s.win_rect(w); }
        r.x = want.0; r.y = want.1;
        let others = s.z.iter().copied().filter(|&other| other != w).filter_map(|other| {
            if let Some(i) = other.slot() {
                if s.wins[i].maxed { return None; }
                Some(s.win_rect(other))
            } else { rect_of(s, other) }
        });
        let pos = snap_position(r, s.work_rect(), others);
        set_pos(s, w, keep(pos.0, pos.1, r.w, s.work_rect()));
    }
}

/// Geometry is the attachment: separating, hiding or maximizing a window releases its edges.
/// Keep the original rectangles throughout a drag so minimum-size clamps never cause drift.
pub type Layout = Vec<(Win, Rect)>;
const JOIN_EPS: f32 = 1.0;
fn near(a: f32, b: f32) -> bool { (a-b).abs() <= JOIN_EPS }
fn overlap(a: f32, b: f32, c: f32, d: f32) -> bool { b.min(d)-a.max(c) > JOIN_EPS }
fn minimum(s: &State, w: Win) -> (f32,f32) {
    if w == Win::ChannelRack { (views::rack::min_w(s), views::rack::MIN_H) } else { w.min_size() }
}
pub fn layout(s: &State) -> Layout {
    s.z.iter().filter_map(|&w| {
        if let Some(i)=w.slot() {
            (!s.wins[i].maxed && !s.wins[i].mini).then_some((w,s.wins[i].rect))
        } else if w==Win::ChannelRack {Some((w,views::rack::rect(s)))} else {None}
    }).collect()
}
pub fn resize_layout(s:&State) -> Layout {
    layout(s).into_iter().map(|(w,r)|(w,if w.slot().is_some(){s.win_rect(w)}else{r})).collect()
}
fn put(s:&mut State,w:Win,r:Rect) {
    if let Some(i)=w.slot() {s.wins[i].rect=r;}
    else if w==Win::ChannelRack {s.rack_pos=(r.x,r.y);s.rack_size=(r.w,r.h);}
}
fn touching(a:Rect,b:Rect) -> bool {
    ((near(a.right(),b.x)||near(b.right(),a.x)) && overlap(a.y,a.bottom(),b.y,b.bottom())) ||
    ((near(a.bottom(),b.y)||near(b.bottom(),a.y)) && overlap(a.x,a.right(),b.x,b.right()))
}
/// Find the full connected shared divider, including T junctions. Unrelated collinear edges stay put.
fn divider(start:&Layout,win:Win,edge:u8) -> Vec<(usize,bool)> {
    use crate::app::{EDGE_L,EDGE_R,EDGE_T};
    let Some(seed)=start.iter().position(|(w,_)|*w==win) else {return Vec::new()};
    let horizontal=edge==EDGE_L||edge==EDGE_R;
    let leading=edge==EDGE_L||edge==EDGE_T;
    let axis=|r:Rect| if horizontal {(r.x,r.right(),r.y,r.bottom())} else {(r.y,r.bottom(),r.x,r.right())};
    let (lo,hi,_,_)=axis(start[seed].1);let at=if leading {lo}else{hi};
    let mut out=vec![(seed,leading)];
    let mut n=0;
    while n<out.len() {
        let (i,side)=out[n];let (_,_,a,b)=axis(start[i].1);
        for (j,(_,r)) in start.iter().enumerate() {
            if out.iter().any(|(k,_)|*k==j) {continue;}
            let (l,h,c,d)=axis(*r);
            let opposite=if side {near(h,at)}else{near(l,at)};
            if opposite && overlap(a,b,c,d) {out.push((j,!side));}
        }
        n+=1;
    }
    out
}
/// Magnetize the edge being resized, not the opposite edge. This includes the browser splitter
/// (workspace left), the outer workspace and nearby window edges. Reject undersized snaps.
pub fn snap_resize(s:&State,win:Win,edges:u8,mut r:Rect) -> Rect {
    use crate::app::{EDGE_L,EDGE_R,EDGE_T,EDGE_B};
    let work=s.work_rect();let min=minimum(s,win);
    let others:Vec<_>=resize_layout(s).into_iter().filter(|(w,_)|*w!=win).map(|(_,r)|r).collect();
    for edge in [EDGE_L,EDGE_R,EDGE_T,EDGE_B] {
        if edges & edge==0 {continue;}
        let horizontal=edge==EDGE_L||edge==EDGE_R;
        let at=match edge {EDGE_L=>r.x,EDGE_R=>r.right(),EDGE_T=>r.y,_=>r.bottom()};
        let mut candidates=if horizontal {vec![work.x,work.right()]}else{vec![work.y,work.bottom()]};
        for other in &others {
            if horizontal && overlap(r.y,r.bottom(),other.y,other.bottom()) {candidates.extend([other.x,other.right()]);}
            if !horizontal && overlap(r.x,r.right(),other.x,other.right()) {candidates.extend([other.y,other.bottom()]);}
        }
        let target=candidates.into_iter().filter(|v|(v-at).abs()<=SNAP_DISTANCE).filter(|v|match edge {
            EDGE_L=>r.right()-v>=min.0,EDGE_R=>v-r.x>=min.0,EDGE_T=>r.bottom()-v>=min.1,_=>v-r.y>=min.1,
        }).min_by(|a,b|(a-at).abs().total_cmp(&(b-at).abs()));
        if let Some(to)=target {match edge {
            EDGE_L=>{r.w=r.right()-to;r.x=to;},EDGE_R=>r.w=to-r.x,
            EDGE_T=>{r.h=r.bottom()-to;r.y=to;},_=>r.h=to-r.y,
        }}
    }
    r
}

/// Resize all windows sharing any dragged edge. Opposite outside edges remain fixed.
pub fn resize_joined(s:&mut State,start:&Layout,win:Win,edges:u8,wanted:Rect) -> bool {
    use crate::app::{EDGE_L,EDGE_R,EDGE_T,EDGE_B};
    let Some((_,original))=start.iter().find(|(w,_)|*w==win) else {return false};
    let mut result=start.clone();let mut joined=false;
    for edge in [EDGE_L,EDGE_R,EDGE_T,EDGE_B] {
        if edges & edge==0 {continue;}
        let members=divider(start,win,edge);
        if members.len()<2 {continue;}
        joined=true;
        let horizontal=edge==EDGE_L||edge==EDGE_R;
        let coordinate=|r:Rect|match edge {EDGE_L=>r.x,EDGE_R=>r.right(),EDGE_T=>r.y,_=>r.bottom()};
        let mut delta=coordinate(wanted)-coordinate(*original);
        let (mut low,mut high)=(f32::NEG_INFINITY,f32::INFINITY);
        for &(i,leading) in &members {
            let (w,r)=start[i];let min=minimum(s,w);let room=if horizontal {r.w-min.0}else{r.h-min.1};
            if leading {high=high.min(room.max(0.0));} else {low=low.max(-room.max(0.0));}
        }
        delta=delta.clamp(low,high);
        for (i,leading) in members {
            let r=&mut result[i].1;
            if horizontal {if leading {r.x+=delta;r.w-=delta;}else{r.w+=delta;}}
            else if leading {r.y+=delta;r.h-=delta;}else{r.h+=delta;}
        }
    }
    if !joined {return false;}
    // A corner can have one attached edge and one free edge.
    let target=result.iter_mut().find(|(w,_)|*w==win).unwrap();
    for edge in [EDGE_L,EDGE_R,EDGE_T,EDGE_B] {
        if edges & edge!=0 && divider(start,win,edge).len()<2 {
            match edge {EDGE_L=>{target.1.x=wanted.x;target.1.w=wanted.w;},EDGE_R=>target.1.w=wanted.w,EDGE_T=>{target.1.y=wanted.y;target.1.h=wanted.h;},_=>target.1.h=wanted.h}
        }
    }
    for (w,r) in result {put(s,w,r);}
    true
}
/// Reflow joined layouts when the browser changes the workspace width. The shared coordinates
/// transform together; minimum sizes can stop contraction on an unusually small screen.
pub fn reflow_workspace(s:&mut State) -> bool {
    let new=s.work_rect();let old=s.dock_work.replace(new);
    let Some(old)=old else {return false};
    if old.w<=0.0 || new.w<=0.0 || (old.x==new.x && old.w==new.w) {return false;}
    let start=layout(s);let mut visited=vec![false;start.len()];let mut changed=false;
    for seed in 0..start.len() {
        if visited[seed] {continue;}
        let mut group=vec![seed];visited[seed]=true;let mut n=0;
        while n<group.len() {
            for j in 0..start.len() {if !visited[j]&&touching(start[group[n]].1,start[j].1) {visited[j]=true;group.push(j);}}
            n+=1;
        }
        if group.len()<2 || !group.iter().any(|&i|near(start[i].1.x,old.x)) {continue;}
        let min_ratio=group.iter().map(|&i|minimum(s,start[i].0).0/start[i].1.w).fold(0.0f32,f32::max);
        let ratio=(new.w/old.w).max(min_ratio);
        for i in group {let (w,r)=start[i];put(s,w,Rect::new(new.x+(r.x-old.x)*ratio,r.y,r.w*ratio,r.h));}
        changed=true;
    }
    changed
}

/// The windows placed by position (Channel rack, channel settings, Plugin performance monitor, Tempo tapper) keep their
/// part in the workspace as it is now (the main window or the workspace shrank); the floating Playlist, Mixer and Piano
/// roll are kept where they are shown (`State::float_rect`). Returns whether a window moved.
pub fn keep_all(s: &mut State) -> bool {
    let work = s.work_rect();
    if work.w <= 0.0 || work.h <= 0.0 {
        return false;
    }
    let mut moved = false;
    for w in [Win::ChannelRack, Win::Plugin, Win::Perf, Win::Tapper, Win::Touch, Win::Strum, Win::Arp, Win::PianoDlg] {
        let Some(r) = rect_of(s, w) else { continue };
        let pos = keep(r.x, r.y, r.w, work);
        if pos != (r.x, r.y) {
            set_pos(s, w, pos);
            moved = true;
        }
    }
    moved
}

impl App {
    /// VIEW ▸ Into workspace: every window partly outside the workspace moves fully back in, as little as it takes,
    /// keeping its size where it fits. One wider or taller than the workspace shrinks to it (the Playlist, Mixer and
    /// Piano roll not below their smallest size, the Channel rack in whole steps and rows); one that can't shrink goes
    /// to the workspace's left or top edge. Hidden windows too, so they open inside; a maximized window's floating
    /// rect too.
    pub(crate) fn move_into_workspace(&mut self) {
        let s = &mut self.state;
        let work = s.work_rect();
        for w in [Win::Playlist, Win::Mixer, Win::PianoRoll] {
            if let Some(i) = w.slot() {
                s.wins[i].rect = inside(s.float_rect(w), work, w.min_size());
            }
        }
        // The rack's side that doesn't fit, in whole steps or rows (`snap_size` rounds to the nearest: half one less).
        let r = views::rack::rect(s);
        let fit = views::rack::snap_size(s, work.w - views::rack::STEP_W / 2.0, work.h - views::rack::ROW_H / 2.0);
        if r.w > work.w {
            s.rack_size.0 = fit.0;
        }
        if r.h > work.h {
            s.rack_size.1 = fit.1;
        }
        for w in [Win::ChannelRack, Win::Plugin, Win::Perf, Win::Tapper, Win::Touch, Win::Strum, Win::Arp, Win::PianoDlg] {
            if let Some(r) = rect_of(s, w) {
                let to = inside(r, work, (r.w, r.h));
                set_pos(s, w, (to.x, to.y));
            }
        }
        #[cfg(all(plugin_windows, windows))]
        self.plugins.move_into_workspace();
        self.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use os_ui::Canvas;

    #[test]
    fn resize_edges_snap_to_browser_screen_and_windows() {
        use crate::app::{EDGE_L,EDGE_R,EDGE_B};
        let mut app=App::new(None);let mut c=Canvas::new(1920,1200,1.0).unwrap();
        app.state.z=vec![Win::Playlist,Win::Mixer];
        for ws in &mut app.state.wins {ws.maxed=false;ws.mini=false;}
        app.paint(&mut c,None);let work=app.state.work_rect();
        put(&mut app.state,Win::Playlist,Rect::new(work.x+70.0,work.y+20.0,800.0,300.0));
        put(&mut app.state,Win::Mixer,Rect::new(work.x,work.y+500.0,work.w,650.0));
        app.paint(&mut c,None);
        // Real left-edge drag: stop six pixels short of the browser; the edge lands flush.
        let a=app.state.win_rect(Win::Playlist);let (x,y)=(a.x,a.y+100.0);
        app.mouse_move(x,y);app.mouse_down();app.mouse_move(work.x+6.0,y);app.mouse_up();
        let b=app.state.win_rect(Win::Playlist);assert_eq!(b.x,work.x);assert_eq!(b.right(),a.right());
        app.paint(&mut c,None);
        app.mouse_move(b.right(),y);app.mouse_down();app.mouse_move(work.right()-7.0,y);app.mouse_up();
        assert_eq!(app.state.win_rect(Win::Playlist).right(),work.right());
        let r=Rect::new(work.x,work.y+20.0,work.w,474.0);
        assert_eq!(snap_resize(&app.state,Win::Playlist,EDGE_B,r).bottom(),work.y+500.0);
        let away=Rect::new(work.x+20.0,work.y+20.0,800.0,300.0);
        assert_eq!(snap_resize(&app.state,Win::Playlist,EDGE_L,away),away);
        let small=Rect::new(work.right()-515.0,work.y,520.0,300.0);
        assert_eq!(snap_resize(&app.state,Win::Playlist,EDGE_R,small),small,"do not snap below minimum width");
        app.state.show_browser=false;
        let r=Rect::new(6.0,work.y,800.0,300.0);
        assert_eq!(snap_resize(&app.state,Win::Playlist,EDGE_L,r).x,0.0);
    }

    #[test]
    fn joined_dividers_resize_neighbors_and_reflow_browser() {
        use crate::app::{Cursor,EDGE_R};
        let mut canvas=Canvas::new(1920,1200,1.0).unwrap();
        let mut app=App::new(None);app.state.z=vec![Win::Playlist,Win::Mixer];
        for ws in &mut app.state.wins {ws.maxed=false;ws.mini=false;}
        app.paint(&mut canvas,None);
        let work=app.state.work_rect();
        app.state.wins[0].rect=Rect::new(work.x,work.y,800.0,work.h);
        app.state.wins[1].rect=Rect::new(work.x+800.0,work.y,work.w-800.0,work.h);
        app.paint(&mut canvas,None);
        let (x,y)=(work.x+800.0,work.y+400.0);
        app.mouse_move(x,y);assert_eq!(app.cursor(),Cursor::EwResize);
        app.mouse_down();app.mouse_move(x+100.0,y);app.mouse_up();
        let a=app.state.win_rect(Win::Playlist);let b=app.state.win_rect(Win::Mixer);
        assert_eq!(a.w,900.0);assert_eq!(a.right(),b.x);assert_eq!(b.right(),work.right());
        app.paint(&mut canvas,None);
        app.act(Action::ToggleWin(Win::Browser));app.paint(&mut canvas,None);
        let a=app.state.win_rect(Win::Playlist);let b=app.state.win_rect(Win::Mixer);
        assert_eq!(a.x,0.0);assert!((a.right()-b.x).abs()<0.01);assert!((b.right()-1920.0).abs()<0.01);
        assert!(a.w>900.0);
        app.act(Action::ToggleWin(Win::Browser));app.paint(&mut canvas,None);
        assert!((app.state.win_rect(Win::Playlist).w-900.0).abs()<0.01);
        let width=app.state.browser_w;
        app.state.browser_w=width+1.0;app.paint(&mut canvas,None);
        assert!((app.state.win_rect(Win::Playlist).x-(work.x+1.0)).abs()<0.01);
        app.state.browser_w=width;app.paint(&mut canvas,None);
        let start=resize_layout(&app.state);let a=app.state.win_rect(Win::Playlist);
        assert!(resize_joined(&mut app.state,&start,Win::Playlist,EDGE_R,Rect{w:5000.0,..a}));
        assert!((app.state.win_rect(Win::Mixer).w-Win::Mixer.min_size().0).abs()<0.01);
        assert!((app.state.win_rect(Win::Playlist).right()-app.state.win_rect(Win::Mixer).x).abs()<0.01);
        // Release and snap back to the neighbor's side, then separate it by its title.
        let b=app.state.win_rect(Win::Mixer);let a=app.state.win_rect(Win::Playlist);
        drag_to(&mut app.state,Win::Mixer,(a.right()+6.0,b.y));
        assert_eq!(app.state.win_rect(Win::Mixer).x,a.right());
        drag_to(&mut app.state,Win::Mixer,(a.right()+40.0,b.y+40.0));
        assert_eq!(divider(&resize_layout(&app.state),Win::Playlist,EDGE_R).len(),1);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR") {
            put(&mut app.state,Win::Mixer,b);app.paint(&mut canvas,None);
            canvas.pixmap.save_png(std::path::PathBuf::from(dir).join("joined-window-layout.png")).unwrap();
        }
    }

    #[test]
    fn joined_t_junction_and_vertical_boundaries_keep_minimums() {
        use crate::app::{Cursor,EDGE_R,EDGE_B};
        let mut canvas=Canvas::new(1920,1400,1.0).unwrap();let mut app=App::new(None);
        app.state.z=vec![Win::Playlist,Win::PianoRoll,Win::Mixer];
        for ws in &mut app.state.wins {ws.maxed=false;ws.mini=false;}
        app.paint(&mut canvas,None);let work=app.state.work_rect();
        put(&mut app.state,Win::Playlist,Rect::new(work.x,work.y,700.0,1100.0));
        put(&mut app.state,Win::PianoRoll,Rect::new(work.x+700.0,work.y,900.0,400.0));
        put(&mut app.state,Win::Mixer,Rect::new(work.x+700.0,work.y+400.0,900.0,700.0));
        let start=resize_layout(&app.state);let a=app.state.win_rect(Win::Playlist);
        assert!(resize_joined(&mut app.state,&start,Win::Playlist,EDGE_R,Rect{w:800.0,..a}));
        for w in [Win::PianoRoll,Win::Mixer] {let r=app.state.win_rect(w);assert_eq!(r.x,work.x+800.0);assert_eq!(r.w,800.0);}
        app.paint(&mut canvas,None);
        let (x,y)=(work.x+1200.0,work.y+400.0);app.mouse_move(x,y);assert_eq!(app.cursor(),Cursor::NsResize);
        app.mouse_down();app.mouse_move(x,y+400.0);app.mouse_up();
        let p=app.state.win_rect(Win::PianoRoll);let m=app.state.win_rect(Win::Mixer);
        assert_eq!(p.h,665.0);assert_eq!(m.h,435.0);assert_eq!(p.bottom(),m.y);assert_eq!(m.bottom(),work.y+1100.0);
        assert_eq!(app.state.win_rect(Win::Playlist).h,1100.0);
        app.state.hide(Win::Mixer);
        assert_eq!(divider(&resize_layout(&app.state),Win::PianoRoll,EDGE_B).len(),1);
        app.state.raise(Win::Mixer);app.state.wins[1].maxed=true;
        assert_eq!(divider(&resize_layout(&app.state),Win::PianoRoll,EDGE_B).len(),1);
        // Rack remains usable as a member of the layout, without quantization leaving gaps.
        app.state.z=vec![Win::Playlist,Win::ChannelRack];
        put(&mut app.state,Win::Playlist,Rect::new(work.x,work.y,700.0,1100.0));
        put(&mut app.state,Win::ChannelRack,Rect::new(work.x+700.0,work.y,900.0,400.0));
        let start=resize_layout(&app.state);let a=app.state.win_rect(Win::Playlist);
        assert!(resize_joined(&mut app.state,&start,Win::Playlist,EDGE_R,Rect{w:800.0,..a}));
        assert_eq!(views::rack::rect(&app.state).x,app.state.win_rect(Win::Playlist).right());
        assert_eq!(app.state.rack_size.0,800.0);
    }

    #[test]
    fn magnetic_edges_snap_and_release() {
        let work = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let neighbor = Rect::new(400.0, 200.0, 200.0, 200.0);
        let snap = |r| snap_position(r, work, std::iter::once(neighbor));
        assert_eq!(snap(Rect::new(207.0, 203.0, 200.0, 100.0)), (200.0, 200.0));
        assert_eq!(snap(Rect::new(594.0, 203.0, 200.0, 100.0)), (600.0, 200.0));
        assert_eq!(snap(Rect::new(405.0, 106.0, 200.0, 100.0)), (400.0, 100.0));
        assert_eq!(snap(Rect::new(405.0, 394.0, 200.0, 100.0)), (400.0, 400.0));
        assert_eq!(snap(Rect::new(220.0, 230.0, 200.0, 100.0)), (220.0, 230.0));
        assert_eq!(snap(Rect::new(207.0, 600.0, 200.0, 100.0)), (207.0, 600.0));
        assert_eq!(snap(Rect::new(7.0, 5.0, 200.0, 100.0)), (0.0, 0.0));
        assert_eq!(snap(Rect::new(794.0, 695.0, 200.0, 100.0)), (800.0, 700.0));
    }

    const WORK: Rect = Rect { x: 209.0, y: 83.0, w: 1711.0, h: 997.0 };

    /// Spec "Lemon Studio": at least 31 px of the width stays inside the workspace at the left and at the right, the
    /// top at least 15 px above the workspace's bottom, the title bar never above the workspace's top.
    #[test]
    fn keep_holds_each_edge() {
        let w = 500.0;
        // Inside: unchanged.
        assert_eq!(keep(400.0, 300.0, w, WORK), (400.0, 300.0));
        // Left: the right edge 31 px right of the workspace's left edge.
        let (x, _) = keep(-4000.0, 300.0, w, WORK);
        assert_eq!(x + w, WORK.x + 31.0);
        // Right: the left edge 31 px left of the workspace's right edge.
        assert_eq!(keep(9000.0, 300.0, w, WORK).0, WORK.right() - 31.0);
        // Down: the top 15 px above the bottom.
        assert_eq!(keep(400.0, 9000.0, w, WORK).1, WORK.bottom() - 15.0);
        // Up: never above the workspace's top.
        assert_eq!(keep(400.0, -50.0, w, WORK).1, WORK.y);
        // Partly out, past the limits nowhere: unchanged (only the title bar inside at the right was kept by FL too).
        assert_eq!(keep(WORK.right() - 106.0, 300.0, w, WORK), (WORK.right() - 106.0, 300.0));
        // The same in physical px at 150 %: 47 and 23 px.
        let work = [314, 125, 2880, 1620];
        assert_eq!(keep_px((-9000, -9000), 750, work, 1.5), (314 + 47 - 750, 125));
        assert_eq!(keep_px((9000, 9000), 750, work, 1.5), (2880 - 47, 1620 - 23));
    }

    /// Spec "Lemon Studio": Into workspace moves a window fully back in, keeping its size where it fits.
    #[test]
    fn inside_moves_and_keeps_the_size() {
        let r = Rect::new(-300.0, 200.0, 500.0, 400.0);
        assert_eq!(inside(r, WORK, (0.0, 0.0)), Rect::new(WORK.x, 200.0, 500.0, 400.0), "off the left: at the left edge");
        let r = Rect::new(1700.0, 900.0, 500.0, 400.0);
        assert_eq!(inside(r, WORK, (0.0, 0.0)), Rect::new(WORK.right() - 500.0, WORK.bottom() - 400.0, 500.0, 400.0));
        let big = Rect::new(100.0, 100.0, 3000.0, 2000.0);
        assert_eq!(inside(big, WORK, (520.0, 220.0)), WORK, "too big: as big as the workspace");
        assert_eq!(inside(Rect::new(0.0, 0.0, 600.0, 300.0), Rect::new(10.0, 20.0, 400.0, 200.0), (520.0, 220.0)), Rect::new(10.0, 20.0, 520.0, 220.0));
        assert_eq!(inside_px([-100, 50, 400, 450], [0, 100, 1000, 800]), (0, 100));
    }

    /// Spec "Lemon Studio", detached windows with the check off: the title bar stays on the screens. Two side-by-side
    /// screens (as measured), the right one shorter.
    #[test]
    fn detached_title_bars_stay_on_the_screens() {
        let screens = [[0, 0, 1920, 1040], [1920, 0, 3840, 1000]];
        let bar = |x: i32, y: i32| [x, y, x + 600, y + 27];
        assert_eq!(bar_on_screens(bar(300, 300), &screens), (300, 300));
        // Across the two screens, all of it on them: left alone.
        assert_eq!(bar_on_screens(bar(1700, 300), &screens), (1700, 300));
        // Off the outer edges, above the top, below the bottom: back onto the screen.
        assert_eq!(bar_on_screens(bar(-200, 300), &screens), (0, 300));
        assert_eq!(bar_on_screens(bar(3500, 300), &screens), (3840 - 600, 300));
        assert_eq!(bar_on_screens(bar(300, -10), &screens), (300, 0));
        assert_eq!(bar_on_screens(bar(300, 1030), &screens), (300, 1040 - 27));
        // Under the shorter screen's bottom, mostly on it: onto it.
        assert_eq!(bar_on_screens(bar(2400, 990), &screens), (2400, 1000 - 27));
        // Off every screen: onto the nearest.
        assert_eq!(bar_on_screens(bar(5000, 5000), &screens), (3840 - 600, 1000 - 27));
        assert_eq!(bar_on_screens(bar(5000, 5000), &[]), (5000, 5000));
    }

    fn canvas(w: u32, h: u32) -> Option<Canvas> {
        Canvas::new(w, h, 1.0).ok()
    }

    /// An app with the floating windows open: Playlist, Piano roll and Mixer floating, the Channel rack, the first
    /// channel's settings, the performance monitor and the tapper.
    fn app_with_windows(c: &mut Canvas) -> App {
        let mut app = App::new(None);
        for i in 0..3 {
            app.state.wins[i].maxed = false;
            app.state.wins[i].mini = false;
        }
        for w in [Win::Playlist, Win::PianoRoll, Win::Mixer, Win::ChannelRack, Win::Plugin] {
            app.state.raise(w);
        }
        app.act(Action::ToggleWin(Win::Perf));
        app.act(Action::ToggleWin(Win::Tapper));
        app.paint(c, None);
        app
    }

    const WINDOWS: [Win; 7] = [Win::Playlist, Win::PianoRoll, Win::Mixer, Win::ChannelRack, Win::Plugin, Win::Perf, Win::Tapper];

    /// The pixels (centres) of `w`'s title bar within `area`: those its move hits cover, whatever is on top.
    fn bar_points(app: &App, w: Win, area: Rect) -> Vec<(f32, f32)> {
        let bars: Vec<Rect> = app.hits.iter().filter(|h| h.action == Action::WinDrag(w)).map(|h| h.rect).collect();
        let mut out = Vec::new();
        for b in &bars {
            let (x0, x1) = (b.x.max(area.x).floor() as i32, b.right().min(area.right()).ceil() as i32);
            let (y0, y1) = (b.y.max(area.y).floor() as i32, b.bottom().min(area.bottom()).ceil() as i32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = (x as f32 + 0.5, y as f32 + 0.5);
                    if area.contains(p.0, p.1) && b.contains(p.0, p.1) && !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        }
        out
    }

    /// A point of `w`'s title bar within `area` that starts a move (the top hit there).
    fn grip(app: &App, w: Win, area: Rect) -> Option<(f32, f32)> {
        bar_points(app, w, area).into_iter().find(|&(x, y)| app.top_hit(x, y).is_some_and(|h| h.action == Action::WinDrag(w)))
    }

    /// Brings `w` to the front, then drags its title bar by (`dx`, `dy`) from a point of it within `area`.
    fn drag(app: &mut App, c: &mut Canvas, w: Win, area: Rect, d: (f32, f32)) {
        app.state.raise(w);
        app.paint(c, None);
        let (x, y) = grip(app, w, area).unwrap_or_else(|| panic!("{w:?}: no part of the title bar in {area:?} moves it"));
        app.mouse_move(x, y);
        app.mouse_down();
        app.mouse_move(x + d.0 / 2.0, y + d.1 / 2.0);
        app.mouse_move(x + d.0, y + d.1);
        app.mouse_up();
        // Two clicks on the same spot would be a double click (maximize).
        app.title_click = None;
        app.paint(c, None);
    }

    fn rect(app: &App, w: Win) -> Rect {
        rect_of(&app.state, w).expect("a floating window")
    }

    /// Spec "Lemon Studio": dragged by the title bar far past each edge, every window keeps its part in the
    /// workspace: 31 px at the left and right, its top 15 px above the bottom, never above the top.
    #[test]
    fn dragged_windows_keep_their_part() {
        let Some(mut c) = canvas(1920, 1080) else { return };
        let mut app = app_with_windows(&mut c);
        let all = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        for w in WINDOWS {
            let work = app.state.work_rect();
            drag(&mut app, &mut c, w, all, (-5000.0, 0.0));
            assert_eq!(rect(&app, w).right(), work.x + KEEP_W, "{w:?} past the left");
            drag(&mut app, &mut c, w, all, (9000.0, 0.0));
            assert_eq!(rect(&app, w).x, work.right() - KEEP_W, "{w:?} past the right");
            drag(&mut app, &mut c, w, all, (-6000.0, 5000.0));
            assert_eq!(rect(&app, w).y, work.bottom() - KEEP_TOP, "{w:?} past the bottom");
            drag(&mut app, &mut c, w, all, (0.0, -5000.0));
            assert_eq!(rect(&app, w).y, work.y, "{w:?} past the top");
        }
    }

    /// Spec "Lemon Studio": the strip kept at the left (and at the right, and at the bottom) can be grabbed: every part
    /// of the title bar in it that isn't a button starts a move, and dragging it brings the window back.
    #[test]
    fn the_kept_strip_drags_back() {
        let Some(mut c) = canvas(1920, 1080) else { return };
        let mut app = app_with_windows(&mut c);
        // Without the browser the workspace starts at the screen's left edge: only the strip is visible there.
        app.state.show_browser = false;
        let all = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        // What may take a press on a title bar instead of the move: its controls (buttons, menus, tools, knobs, boxes:
        // all small) and the frame's resize edges.
        let control = |a: &Action, r: Rect| matches!(a, Action::WinResize(..) | Action::RackResize(_)) || (*a != Action::Swallow && r.w <= 100.0 && r.h <= 32.0);
        for w in WINDOWS {
            let work = app.state.work_rect();
            for (out, strip, back) in [
                ((-5000.0, 0.0), Rect::new(work.x, work.y, KEEP_W, work.h), (400.0, 0.0)),
                ((9000.0, 0.0), Rect::new(work.right() - KEEP_W, work.y, KEEP_W, work.h), (-400.0, 0.0)),
                ((0.0, 5000.0), Rect::new(work.x, work.bottom() - KEEP_TOP, work.w, KEEP_TOP), (0.0, -300.0)),
            ] {
                drag(&mut app, &mut c, w, all, out);
                // In the strip, the title bar's hits are the move and the controls.
                let points = bar_points(&app, w, strip);
                assert!(!points.is_empty(), "{w:?}: no title bar in {strip:?}");
                for (x, y) in points {
                    let h = app.top_hit(x, y).map(|h| (h.action.clone(), h.rect));
                    assert!(h.as_ref().is_some_and(|(a, r)| *a == Action::WinDrag(w) || control(a, *r)), "{w:?} at ({x}, {y}): {h:?}");
                }
                let before = rect(&app, w);
                drag(&mut app, &mut c, w, strip, back);
                let after = rect(&app, w);
                assert_eq!((after.x - before.x, after.y - before.y), back, "{w:?} dragged back from {strip:?}");
            }
        }
    }

    /// Spec "Lemon Studio": VIEW ▸ Into workspace moves every window that is partly outside fully back in, keeping its
    /// size where it fits; windows inside stay where they are.
    #[test]
    fn into_workspace_brings_every_window_back() {
        use crate::views::menus::{item_at, works, INTO_WORKSPACE_LABEL, VIEW};
        let Some(mut c) = canvas(1920, 1080) else { return };
        let mut app = app_with_windows(&mut c);
        let all = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        drag(&mut app, &mut c, Win::Playlist, all, (-5000.0, 0.0));
        drag(&mut app, &mut c, Win::ChannelRack, all, (9000.0, 300.0));
        drag(&mut app, &mut c, Win::Tapper, all, (-5000.0, 5000.0));
        drag(&mut app, &mut c, Win::Perf, all, (0.0, 5000.0));
        // The mixer wider than the workspace (as tall as the screen allows), the piano roll inside.
        app.state.wins[1].rect = Rect::new(150.0, 90.0, 1900.0, 700.0);
        app.state.wins[2].rect = Rect::new(600.0, 200.0, 700.0, 400.0);
        let before: Vec<Rect> = WINDOWS.iter().map(|&w| rect(&app, w)).collect();
        let i = (0..60).find(|&i| item_at(VIEW, i).is_some_and(|it| it.label == INTO_WORKSPACE_LABEL)).expect("VIEW > Into workspace");
        assert!(works(VIEW, item_at(VIEW, i).unwrap()), "not greyed out");
        app.menu_item(VIEW, i);
        app.paint(&mut c, None);
        let work = app.state.work_rect();
        for (w, b) in WINDOWS.iter().zip(before) {
            let r = rect(&app, *w);
            assert!(r.x >= work.x && r.right() <= work.right() && r.y >= work.y && r.bottom() <= work.bottom(), "{w:?} {r:?} in {work:?}");
            if *w == Win::Mixer {
                assert_eq!((r.x, r.w, r.y, r.h), (work.x, work.w, b.y, b.h), "too wide: as wide as the workspace");
            } else {
                assert_eq!((r.w, r.h), (b.w, b.h), "{w:?} keeps its size");
            }
        }
        assert_eq!(rect(&app, Win::Playlist).x, work.x, "off the left: at the workspace's left edge");
        assert_eq!(rect(&app, Win::PianoRoll), Rect::new(600.0, 200.0, 700.0, 400.0), "inside: left alone");
        // A channel rack bigger than a small workspace shrinks to it in whole steps and rows.
        let Some(mut small) = canvas(1000, 600) else { return };
        app.state.rack_size = (1500.0, 900.0);
        app.paint(&mut small, None);
        app.menu_item(VIEW, i);
        let (work, r) = (app.state.work_rect(), rect(&app, Win::ChannelRack));
        assert!(r.x >= work.x && r.right() <= work.right() && r.y >= work.y && r.bottom() <= work.bottom(), "{r:?} in {work:?}");
        assert!(r.w > work.w - crate::views::rack::STEP_W && r.h > work.h - crate::views::rack::ROW_H, "{r:?} fills {work:?}");
        assert_eq!(app.state.rack_size, crate::views::rack::snap_size(&app.state, r.w, r.h));
    }

    /// Spec "Lemon Studio": the clamp holds whatever "Don't limit windows to screen" says; the check is declared, off
    /// by default.
    #[test]
    fn the_check_leaves_inner_windows_alone() {
        use crate::prefs::{check, set_check, DONT_LIMIT_WINDOWS};
        const { assert!(!DONT_LIMIT_WINDOWS.default) };
        assert!(!check(&DONT_LIMIT_WINDOWS));
        let Some(mut c) = canvas(1920, 1080) else { return };
        let all = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let moves = [(-5000.0, 0.0), (9000.0, 0.0), (0.0, 5000.0), (0.0, -5000.0)];
        let mut ends = Vec::new();
        for on in [false, true] {
            set_check(&DONT_LIMIT_WINDOWS, on);
            let mut app = app_with_windows(&mut c);
            for w in WINDOWS {
                for d in moves {
                    drag(&mut app, &mut c, w, all, d);
                    ends.push((on, w, rect(&app, w)));
                }
            }
        }
        set_check(&DONT_LIMIT_WINDOWS, false);
        let (off, on): (Vec<_>, Vec<_>) = ends.into_iter().partition(|e| !e.0);
        assert!(!off.is_empty());
        for (a, b) in off.iter().zip(&on) {
            assert_eq!((a.1, a.2), (b.1, b.2));
        }
    }

    /// Spec "Lemon Studio": a smaller main window or a wider browser keeps every window's part in the workspace.
    #[test]
    fn a_shrinking_workspace_keeps_the_part() {
        let Some(mut c) = canvas(1920, 1080) else { return };
        let mut app = app_with_windows(&mut c);
        let all = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        for w in WINDOWS {
            drag(&mut app, &mut c, w, all, (9000.0, 5000.0));
        }
        let Some(mut small) = canvas(1200, 700) else { return };
        app.paint(&mut small, None);
        let work = app.state.work_rect();
        assert_eq!(work.right(), 1200.0);
        for w in WINDOWS {
            let r = rect(&app, w);
            assert_eq!((r.x, r.y), (work.right() - KEEP_W, work.bottom() - KEEP_TOP), "{w:?}");
        }
        // Off the left with the browser hidden; the browser comes back.
        app.state.show_browser = false;
        app.paint(&mut small, None);
        for w in WINDOWS {
            drag(&mut app, &mut small, w, Rect::new(0.0, 0.0, 1200.0, 700.0), (-9000.0, 0.0));
        }
        app.state.show_browser = true;
        app.paint(&mut small, None);
        let work = app.state.work_rect();
        assert!(work.x > 0.0);
        for w in WINDOWS {
            assert_eq!(rect(&app, w).right(), work.x + KEEP_W, "{w:?}");
        }
    }
}
