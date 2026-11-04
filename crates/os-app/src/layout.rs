// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Where the windows were when Lemon Studio last closed, put back at the next start: the main window (its restored
//! frame and whether it was maximized) and, inside it, the Playlist, Mixer and Piano roll (their floating rects and
//! whether each was maximized or collapsed to its title bar), the Channel rack (its place and size), the Plugin
//! performance monitor, the Tempo tapper and the Touch controller. App-wide, kept in `settings.txt` (`settings_file`)
//! as the app closes, never in the project. Which windows are open is not kept: a start shows its usual ones.
//!
//! Keys: `window_main` (`x,y,w,h,maximized`: outer top-left and inner size in the units the window system places
//! windows in, points on macOS and pixels elsewhere), `window_playlist`, `window_mixer`, `window_piano_roll`
//! (`x,y,w,h,maximized,collapsed`), `window_channel_rack` (`x,y,w,h`), `window_plugin_performance`,
//! `window_tempo_tapper`, `window_touch_controller` (`x,y`); positions inside the main window are logical px, switches
//! `1` / `0`. A value that cannot be read leaves that window as a first start has it. The first paint keeps every
//! window's part in the workspace, should the main window be smaller now (`win_clamp`).

use os_ui::Rect;

use crate::app::{State, Win, WinState};
use crate::settings_file::{get, write};
use crate::views;

/// The Playlist, Mixer and Piano roll (`State::wins`) and their keys.
const WINS: [(Win, &str); 3] = [(Win::Playlist, "window_playlist"), (Win::Mixer, "window_mixer"), (Win::PianoRoll, "window_piano_roll")];
const RACK: &str = "window_channel_rack";
const PERF: &str = "window_plugin_performance";
const TAPPER: &str = "window_tempo_tapper";
const TOUCH: &str = "window_touch_controller";
const MAIN: &str = "window_main";

/// How tall a strip of the main window's top must be on a screen at the start, in `MainWin` units (`on_screens`).
pub const MAIN_BAR_H: i32 = 32;

/// The main window as it was left: its restored frame (outer top-left and inner size, in the units the window system
/// places windows in: points on macOS, pixels elsewhere) and whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainWin {
    pub pos: (i32, i32),
    pub size: (u32, u32),
    pub max: bool,
}

/// `key`'s value as `N` comma-separated finite numbers; `None` when it is not set or is anything else.
fn numbers<const N: usize>(key: &str) -> Option<[f32; N]> {
    let v: Vec<f32> = get(key)?.split(',').map(|n| n.trim().parse().ok().filter(|n: &f32| n.is_finite())).collect::<Option<_>>()?;
    v.try_into().ok()
}

fn keep(key: &str, v: &[f32]) {
    write(key, &v.iter().map(f32::to_string).collect::<Vec<_>>().join(","));
}

fn switch(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// The windows inside the main window back where `save` left them; one not kept (or unreadable) stays as it is.
pub fn restore(s: &mut State) {
    s.startup_layout_pending = get("window_playlist").is_none() && get("window_mixer").is_none()
        && !["window_playlist_detached", "window_mixer_detached"].iter().any(|k|get(k).as_deref()==Some("1"));
    s.mixer.traditional_layout=get("mixer_traditional_layout").and_then(|v|v.parse::<u8>().ok()).filter(|v|*v<8).unwrap_or(3);
    crate::mixer::set_layout(s,match get("mixer_view").as_deref() {
        Some("channel-strip")=>crate::rack_mixer::VIEW,
        Some("compact")=>crate::mixer::COMPACT,
        Some("traditional")=>crate::mixer::TRADITIONAL+s.mixer.traditional_layout,
        _=>crate::mixer::WIDE,
    });
    s.native_detached=WINS.iter().filter(|(_,k)|get(&format!("{k}_detached")).as_deref()==Some("1")).map(|(w,_)|*w).collect();
    for (w,k) in WINS {if s.native_detached.contains(&w)&&get(&format!("{k}_detached_open")).as_deref()==Some("1"){s.raise(w);}}
    for (w, key) in WINS {
        let (Some(i), Some([x, y, rw, rh, max, mini])) = (w.slot(), numbers(key)) else { continue };
        if rw > 0.0 && rh > 0.0 {
            s.wins[i] = WinState { rect: Rect::new(x, y, rw, rh), maxed: max == 1.0, mini: mini == 1.0 };
        }
    }
    if let Some([x, y, w, h]) = numbers(RACK) {
        s.rack_pos = (x, y);
        // Not below the smallest size a resize leaves it (`views::rack::snap_size`).
        s.rack_size = (w.max(views::rack::min_w(s)), h.max(views::rack::MIN_H));
    }
    // Placed: they open there, not centred.
    for (key, pos, placed) in [(PERF, &mut s.perf.pos, &mut s.perf.placed), (TAPPER, &mut s.tapper.pos, &mut s.tapper.placed), (TOUCH, &mut s.touch.pos, &mut s.touch.placed)] {
        if let Some([x, y]) = numbers(key) {
            *pos = (x, y);
            *placed = true;
        }
    }
    // A reopened project's layout takes precedence over the last application session.
    restore_project_layout(s);
}

/// First-run default only; defer until the main canvas supplies the actual display size.
pub fn apply_startup_layout(s: &mut State) {
    if !s.startup_layout_pending || s.native_view.is_some() { return; }
    let work=s.work_rect();
    if work.w<=0. || work.h<=0. { return; }
    s.startup_layout_pending=false;
    let min_mix=Win::Mixer.min_size().1;
    let min_playlist=Win::Playlist.min_size().1;
    let height=work.h.max(min_mix+min_playlist);
    let mix_h=(height*0.62).round().clamp(min_mix,height-min_playlist);
    s.wins[Win::Mixer.slot().unwrap()]=WinState {rect:Rect::new(work.x,work.y,work.w,mix_h),maxed:false,mini:false};
    s.wins[Win::Playlist.slot().unwrap()]=WinState {rect:Rect::new(work.x,work.y+mix_h,work.w,height-mix_h),maxed:false,mini:false};
    s.hide(Win::ChannelRack);s.hide(Win::PianoRoll);
    s.raise(Win::Playlist);s.raise(Win::Mixer);
    s.dock_work=Some(work);
}

/// Keeps where the windows inside the main window are, for the next start. The monitor, tapper and touch controller only
/// once they have a place (opened in this run or an earlier one).
pub fn save(s: &State) {
    write("mixer_view",if s.mixer.layout==crate::rack_mixer::VIEW {"channel-strip"}else if crate::mixer::traditional(s) {"traditional"} else if s.mixer.layout==crate::mixer::COMPACT {"compact"} else {"full"});
    write("mixer_traditional_layout",&s.mixer.traditional_layout.to_string());
    for (w,k) in WINS {write(&format!("{k}_detached"),if s.native_detached.contains(&w){"1"}else{"0"});write(&format!("{k}_detached_open"),if s.shown(w){"1"}else{"0"});}
    for (w, key) in WINS {
        if let Some(i) = w.slot() {
            let ws = s.wins[i];
            keep(key, &[ws.rect.x, ws.rect.y, ws.rect.w, ws.rect.h, switch(ws.maxed), switch(ws.mini)]);
        }
    }
    keep(RACK, &[s.rack_pos.0, s.rack_pos.1, s.rack_size.0, s.rack_size.1]);
    for (key, pos, placed) in [(PERF, s.perf.pos, s.perf.placed), (TAPPER, s.tapper.pos, s.tapper.placed), (TOUCH, s.touch.pos, s.touch.placed)] {
        if placed {
            keep(key, &[pos.0, pos.1]);
        }
    }
}

/// The main window as the last run left it, `None` when not kept.
pub fn main_win() -> Option<MainWin> {native_win(MAIN)}
pub fn native_win(key: &str) -> Option<MainWin> {
    let [x, y, w, h, max] = numbers(key)?;
    (w >= 1.0 && h >= 1.0).then(|| MainWin { pos: (x.round() as i32, y.round() as i32), size: (w.round() as u32, h.round() as u32), max: max == 1.0 })
}

/// Keeps the main window's frame for the next start.
pub fn save_main(m: MainWin) {save_native(MAIN,m);}
pub fn save_native(key: &str,m: MainWin) {
    let (x, y, w, h) = (m.pos.0 as f32, m.pos.1 as f32, m.size.0 as f32, m.size.1 as f32);
    keep(key, &[x, y, w, h, switch(m.max)]);
}

/// `m` on the screens there are now (`screens`: [left, top, right, bottom] in `MainWin` units): where it was while its
/// top strip (`MAIN_BAR_H`) is on them, else moved as little as it takes for that strip to be fully on the screen it
/// overlaps most (the nearest when none: a screen that is gone), and no larger than that screen
/// (`win_clamp::bar_on_screens`).
pub fn on_screens(m: MainWin, screens: &[[i32; 4]]) -> MainWin {
    let w = m.size.0.min(i32::MAX as u32) as i32;
    let bar = [m.pos.0, m.pos.1, m.pos.0.saturating_add(w), m.pos.1.saturating_add(MAIN_BAR_H)];
    let to = crate::win_clamp::bar_on_screens(bar, screens);
    if to == m.pos {
        return m;
    }
    let on = screens.iter().find(|s| (s[0]..s[2]).contains(&to.0) && (s[1]..s[3]).contains(&to.1));
    let size = on.map_or(m.size, |s| (m.size.0.min((s[2] - s[0]).max(1) as u32), m.size.1.min((s[3] - s[1]).max(1) as u32)));
    MainWin { pos: to, size, ..m }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn first_run_layout_mixer_above_playlist_and_saved_layout_wins() {
        for (w,h) in [(1280,720),(1920,1080),(2560,1440)] {
            let mut a=App::new(None);restore(&mut a.state);
            assert!(a.state.startup_layout_pending);
            let mut c=os_ui::Canvas::new(w,h,1.).unwrap();a.paint(&mut c,None);
            let work=a.state.work_rect();let m=a.state.win_rect(Win::Mixer);let p=a.state.win_rect(Win::Playlist);
            assert_eq!((m.x,m.y,m.w),(work.x,work.y,work.w));
            assert_eq!((p.x,p.y,p.w,p.bottom()),(work.x,m.bottom(),work.w,work.bottom()));
            assert!(m.h>=Win::Mixer.min_size().1 && p.h>=Win::Playlist.min_size().1);
            assert!(a.state.shown(Win::Mixer)&&a.state.shown(Win::Playlist));
            assert!(!a.state.shown(Win::ChannelRack)&&!a.state.shown(Win::PianoRoll));
            assert!(!a.state.startup_layout_pending);
            if w==1920 {if let Ok(path)=std::env::var("LEMON_STARTUP_LAYOUT_SHOT"){c.pixmap.save_png(std::path::Path::new(&path)).unwrap();}}
        }
        let mut a=App::new(None);a.state.wins[1]=WinState{rect:Rect::new(350.,150.,900.,700.),maxed:false,mini:false};save(&a.state);
        let mut b=App::new(None);restore(&mut b.state);assert!(!b.state.startup_layout_pending);
        let kept=b.state.wins[1];let mut c=os_ui::Canvas::new(1920,1080,1.).unwrap();b.paint(&mut c,None);assert_eq!(b.state.wins[1].rect,kept.rect);
    }

    /// Moved, resized, maximized and collapsed windows come back as they were at the next start; the monitor, tapper and
    /// touch controller open where they were left instead of centred.
    #[test]
    fn windows_come_back_where_they_were() {
        let mut app = App::new(None);
        let s = &mut app.state;
        s.wins[0] = WinState { rect: Rect::new(400.0, 150.5, 900.0, 500.0), maxed: false, mini: false };
        s.wins[1] = WinState { rect: Rect::new(-200.0, 600.0, 1400.0, 420.0), maxed: false, mini: true };
        s.wins[2] = WinState { rect: Rect::new(300.0, 200.0, 1100.0, 700.0), maxed: true, mini: false };
        s.rack_pos = (820.0, 330.0);
        s.rack_size = (900.0, 400.0);
        s.perf.pos = (500.0, 300.0);
        s.perf.placed = true;
        s.tapper.pos = (700.0, 420.0);
        s.tapper.placed = true;
        crate::mixer::set_layout(s,crate::mixer::TRADITIONAL);
        save(&app.state);

        let mut next = App::new(None);
        assert!(next.state.wins.iter().all(|w| w.maxed), "a new App starts with the defaults");
        restore(&mut next.state);
        let (a, b) = (&app.state, &next.state);
        assert_eq!(b.mixer.layout,crate::mixer::TRADITIONAL);
        for i in 0..3 {
            assert_eq!((b.wins[i].rect, b.wins[i].maxed, b.wins[i].mini), (a.wins[i].rect, a.wins[i].maxed, a.wins[i].mini), "window {i}");
        }
        assert_eq!((b.rack_pos, b.rack_size), ((820.0, 330.0), (900.0, 400.0)));
        assert_eq!((b.perf.pos, b.perf.placed, b.tapper.pos, b.tapper.placed), ((500.0, 300.0), true, (700.0, 420.0), true));
        assert!(!b.touch.placed, "never opened: still centred the first time");
    }

    /// A value that cannot be read leaves its window as a first start has it; a rack kept smaller than a resize allows
    /// comes back at the smallest size.
    #[test]
    fn unreadable_values_keep_the_defaults() {
        write("window_playlist", "10,20,abc,40,0,0");
        write("window_mixer", "10,20,30");
        write("window_piano_roll", "10,20,0,400,0,0");
        write("window_tempo_tapper", "NaN,5");
        write("window_channel_rack", "100,120,10,10");
        let mut app = App::new(None);
        let before: Vec<_> = app.state.wins.iter().map(|w| (w.rect, w.maxed, w.mini)).collect();
        restore(&mut app.state);
        let after: Vec<_> = app.state.wins.iter().map(|w| (w.rect, w.maxed, w.mini)).collect();
        assert_eq!(after, before);
        assert!(!app.state.tapper.placed);
        let min_w = views::rack::min_w(&app.state);
        assert_eq!((app.state.rack_pos, app.state.rack_size), ((100.0, 120.0), (min_w, views::rack::MIN_H)));
        assert_eq!(main_win(), None);
    }

    /// The main window's frame and maximized state are kept as given.
    #[test]
    fn the_main_window_is_kept() {
        let m = MainWin { pos: (-1700, 40), size: (1600, 900), max: true };
        save_main(m);
        assert_eq!(get("window_main").as_deref(), Some("-1700,40,1600,900,1"));
        assert_eq!(main_win(), Some(m));
    }

    /// On a screen it stays; on one that is gone it moves to the nearest one there is, as small as it.
    #[test]
    fn the_main_window_stays_on_the_screens() {
        let screens = [[0, 0, 1920, 1080], [1920, 0, 3200, 800]];
        let at = |x, y, w, h| MainWin { pos: (x, y), size: (w, h), max: false };
        assert_eq!(on_screens(at(100, 50, 1200, 800), &screens), at(100, 50, 1200, 800));
        assert_eq!(on_screens(at(1500, 50, 1200, 800), &screens), at(1500, 50, 1200, 800), "across the two");
        assert_eq!(on_screens(at(100, -100, 1200, 800), &screens), at(100, 0, 1200, 800), "above the top");
        // The screen it was on (to the left) is gone: onto the nearest, its top-left at the screen's edge.
        let gone = MainWin { pos: (-2500, 100), size: (2400, 1300), max: true };
        assert_eq!(on_screens(gone, &screens), MainWin { pos: (0, 100), size: (1920, 1080), max: true });
        assert_eq!(on_screens(gone, &[]), gone, "no screens known: as it was");
    }
}

/// Local maintenance: execute in the same process/profile context as the Development app.
/// No project, plugin, audio or UI initialization is needed.
pub fn reset_preview_layout() -> Result<String,String> {
    let root=os_paths::user_dir().ok_or("No profile folder")?;
    if root.file_name().and_then(|s|s.to_str())!=Some("Lemon Studio Public Preview") {
        return Err("Layout reset is restricted to the public preview profile".into());
    }
    let file=root.join("settings.txt");
    let text=std::fs::read_to_string(&file).map_err(|e|e.to_string())?;
    let kept=without_window_layout(&text);
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let backup=root.join(format!("settings-before-window-reset-{stamp}.txt"));
    std::fs::write(&backup,&text).map_err(|e|e.to_string())?;
    if std::fs::read_to_string(&backup).map_err(|e|e.to_string())?!=text {return Err("Backup verification failed".into());}
    std::fs::write(&file,&kept).map_err(|e|e.to_string())?;
    if std::fs::read_to_string(&file).map_err(|e|e.to_string())?!=kept {return Err("Reset verification failed".into());}
    Ok(format!("Preview layout reset verified. Removed {} saved layout entries. Backup: {}",text.lines().count().saturating_sub(kept.lines().count()),backup.display()))
}
fn without_window_layout(text:&str)->String {
    text.lines().filter(|line|!matches!(line.split('=').next().unwrap_or("").trim(),
        "window_playlist"|"window_mixer"|"window_piano_roll"|"window_channel_rack"|
        "window_playlist_detached"|"window_mixer_detached"|"window_piano_roll_detached"|
        "window_playlist_detached_open"|"window_mixer_detached_open"|"window_piano_roll_detached_open"))
        .map(|line|format!("{line}\n")).collect()
}
#[cfg(test)]
mod preview_reset_tests {
    #[test]
    fn preview_reset_keeps_other_settings() {
        let text="default_template=Lemon Studio/Lemon Starter\nwindow_mixer=1,2,3,4,0,0\nwindow_playlist_detached=1\nbrowser_favorites=[x]\nwindow_main=1,2,3,4,0\nheadphones_on=true\n";
        assert_eq!(super::without_window_layout(text),"default_template=Lemon Studio/Lemon Starter\nbrowser_favorites=[x]\nwindow_main=1,2,3,4,0\nheadphones_on=true\n");
    }
}
// Project-specific window positions. An optional private chunk leaves legacy projects unchanged.
const PROJECT_LAYOUT_TAG:os_flp::Tag=os_flp::Tag(*b"LMUI");
pub fn project_snapshot(s:&State,plugins:&crate::plugins::Plugins)->serde_json::Value {
    let wins:Vec<_>=WINS.iter().map(|(w,_)|{let v=s.wins[w.slot().unwrap()];serde_json::json!([v.rect.x,v.rect.y,v.rect.w,v.rect.h,v.maxed,v.mini,s.shown(*w)])}).collect();
    let mut editors:Vec<serde_json::Value>=Vec::new();
    #[cfg(windows)] for (&key,h) in &plugins.hosted {
        let pos=h.window.as_ref().and_then(|w|w.position()).or(h.placed);
        if let (Some(pos),Some(d))=(pos,plugins.descriptor(key)){editors.push(serde_json::json!({"key":key,"uid":d.uid(),"position":[pos.0,pos.1],"detached":h.window.as_ref().map_or(h.detached,|w|w.detached()),"open":h.window.is_some()}));}
    }
    #[cfg(not(windows))] let _=plugins;
    let detached:Vec<_>=WINS.iter().map(|(w,_)|s.native_detached.contains(w)).collect();
    let frames:Vec<_>=WINS.iter().map(|(w,_)|s.native_frames.iter().find(|(k,_)|k==w).map(|(_,f)|serde_json::json!([f.pos.0,f.pos.1,f.size.0,f.size.1,f.max]))).collect();
    serde_json::json!({"version":1,"windows":wins,"rack":[s.rack_pos.0,s.rack_pos.1,s.rack_size.0,s.rack_size.1,s.shown(Win::ChannelRack)],"editors":editors,"detached":detached,"frames":frames})
}
pub fn write_project_layout(file:&mut os_flp::FlpFile,value:serde_json::Value){
    file.chunks.retain(|c|c.tag!=PROJECT_LAYOUT_TAG);
    if let Ok(body)=serde_json::to_vec(&value){file.chunks.push(os_flp::Chunk{tag:PROJECT_LAYOUT_TAG,body:os_flp::ChunkBody::Raw(body)});}
}
fn project_layout(s:&State)->Option<serde_json::Value>{
    let c=s.file.as_ref()?.chunks.iter().find(|c|c.tag==PROJECT_LAYOUT_TAG)?;
    let os_flp::ChunkBody::Raw(bytes)=&c.body else{return None};
    if bytes.len()>1024*1024{return None;}
    let v:serde_json::Value=serde_json::from_slice(bytes).ok()?;
    (v["version"]==1).then_some(v)
}
pub fn restore_project_layout(s:&mut State){
    let Some(v)=project_layout(s)else{return};
    if let Some(flags)=v["detached"].as_array().filter(|a|a.len()==WINS.len()) {
        s.native_layout_generation=s.native_layout_generation.wrapping_add(1);
        s.native_detached=WINS.iter().zip(flags).filter(|(_,on)|on.as_bool()==Some(true)).map(|((w,_),_)|*w).collect();
        s.native_frames.clear();
        for (i,(w,_)) in WINS.iter().enumerate(){let a=&v["frames"][i];
            let pos=|n:usize|a[n].as_i64().and_then(|v|i32::try_from(v).ok()).filter(|v|v.unsigned_abs()<100000);
            let size=|n:usize|a[n].as_u64().filter(|v|*v>0&&*v<=32768).map(|v|v as u32);
            if let (Some(x),Some(y),Some(width),Some(height))=(pos(0),pos(1),size(2),size(3)){s.native_frames.push((*w,MainWin{pos:(x,y),size:(width,height),max:a[4].as_bool().unwrap_or(false)}));}
        }
    }
    let rect=|a:&serde_json::Value|->Option<Rect>{let a=a.as_array()?;let f=|i:usize|a.get(i)?.as_f64().filter(|n|n.is_finite()&&n.abs()<100000.).map(|n|n as f32);let r=Rect::new(f(0)?,f(1)?,f(2)?,f(3)?);(r.w>=100.&&r.h>=50.).then_some(r)};
    for (i,(w,_)) in WINS.iter().enumerate(){let a=&v["windows"][i];if let Some(r)=rect(a){s.wins[w.slot().unwrap()]=WinState{rect:r,maxed:a[4].as_bool().unwrap_or(false),mini:a[5].as_bool().unwrap_or(false)};if a[6].as_bool()==Some(true){s.raise(*w);}else{s.hide(*w);}}}
    if let Some(r)=rect(&v["rack"]){s.rack_pos=(r.x,r.y);s.rack_size=(r.w,r.h);if v["rack"][4].as_bool()==Some(true){s.raise(Win::ChannelRack);}else{s.hide(Win::ChannelRack);}}
    s.startup_layout_pending=false;
}
pub fn restore_project_plugins(app:&mut crate::app::App){
    #[cfg(windows)] {
        let Some(v)=project_layout(&app.state)else{return};let Some(editors)=v["editors"].as_array()else{return};
        for e in editors.iter().take(512){
            let (Some(key),Some(x),Some(y),Some(uid))=(e["key"].as_u64().and_then(|n|usize::try_from(n).ok()),e["position"][0].as_i64().and_then(|n|i32::try_from(n).ok()),e["position"][1].as_i64().and_then(|n|i32::try_from(n).ok()),e["uid"].as_str())else{continue};
            if app.plugins.descriptor(key).is_none_or(|d|d.uid()!=uid){continue;}
            if let Some(h)=app.plugins.hosted.get_mut(&key){h.placed=Some((x,y));h.detached=e["detached"].as_bool().unwrap_or(false);}
            if e["open"].as_bool()!=Some(true)||app.plugins.owner==0{continue;}
            let info=if key>=crate::plugins::FX_KEY{crate::mixer::fx_info(&app.state.project,&app.state.mixer,key)}else{crate::app::channel_info(&app.state.project,key)};
            if let Some(info)=info{if app.plugins.open_window(key,info).is_ok(){if let Some(w)=app.plugins.hosted.get(&key).and_then(|h|h.window.as_ref()).filter(|w|w.detached()){w.move_into_workspace();}app.state.raise(Win::Hosted(key));}}
        }
    }
    #[cfg(not(windows))] let _=app;
}
