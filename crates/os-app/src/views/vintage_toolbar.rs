// SPDX-License-Identifier: GPL-3.0-or-later
//! Original-sized local FL4 modules wired to Lemon's existing actions.
use os_ui::{Color,Font,Rect};
use crate::{app::{Action,Live,State,Ui,Win}, icons::{self,Icon}, knobs::KnobId, vintage_skin as skin};
use super::{FrameInfo,menus};
use crate::workspace_visibility::{shown,WINDOW_BUTTONS};

pub fn active(s:&State,width:f32)->bool {
    s.vintage_2005 && !s.bar.editing
        && width - s.toolbar_actions.width(true) + s.toolbar_actions.meters_width() >= 1015.0
        && transport_available()
        && [("ControlToolBar", 0), ("RecordToolBar", 0), ("ShortcutToolBar", 0),
            ("SCBtns", 9), ("RecordRail", 0), ("TimeToolBar", 0), ("PatternToolBar", 0)]
            .into_iter().all(|(name, frame)| skin::has_frame(name, frame))
}
pub(super) fn transport_available() -> bool {
    skin::has_frame("TransportToolBar", 0) && skin::has_frame("TransportBtns", 8)
}

pub fn paint(ui:&mut Ui,s:&State,f:&FrameInfo) {
    skin::native(ui.c,"ControlToolBar",0,0.,0.);
    // Keep Lemon's identity and project title in the original title-display well.
    ui.c.fill_rect(Rect::new(8.,1.,246.,24.),Color::rgb(0x5f686d));
    crate::vintage::recess(ui.c,Rect::new(9.,4.,244.,20.),Color::rgb(0x344c4f));
    ui.c.fill_rect_gradient(Rect::new(10.,5.,242.,18.),Color::rgb(0x344c4f),Color::rgb(0x142528),0.);
    studio_title(ui, s, f, Rect::new(13.0, 7.0, 236.0, 14.0), Font::vintage(11).bold());
    for (x,action,hint) in [(256.,Action::Minimize,"Minimize"),(270.,Action::Maximize,"Restore / maximize"),(284.,Action::Close,"Exit Lemon Studio")] {
        let bounds = Rect::new(x, 4.0, 14.0, 15.0);
        let down = ui.down(bounds, &action);
        ui.hit(bounds, action, hint);
        if down {
            let frame = if x == 256.0 { 1 } else if x == 270.0 { 3 } else { 5 };
            skin::native(ui.c, "MinClose", frame, x, 4.0);
        }
    }
    for (i,(x0,x1,_)) in menus::bar_layout(ui.c,true).into_iter().enumerate() {
        let r=Rect::new(x0-3.,28.,x1-x0+6.,21.);
        let hot=ui.hit(r,Action::Menu(i),menus::VINTAGE_TITLES[i]);
        skin::lettering(ui.c,menus::VINTAGE_TITLES[i],x0,29.);
        if hot || s.menu_open==Some(i) {ui.c.fill_rect(Rect::new(r.x+2.,47.,r.w-4.,1.),Color::rgb(0xd7e4df));}
    }
    if shown("visible_hint") {
        ui.c.text_clip(Font::vintage(11),13.,58.,280.,Color::rgb(0xb0c0c8),if f.hint.is_empty(){s.message.as_deref().unwrap_or("FL Studio 2005")}else{&f.hint});
    }
    master_lever(ui, s, 342.0, KnobId::MasterVol, "visible_volume");
    master_lever(ui, s, 361.0, KnobId::MasterPitch, "visible_pitch");
    activity(ui, s, f, 319.0, 55.0, true);

    // The original upper shelf is Time, Monitor, CPU, then the shortcuts.
    let mut top = 376.0;
    if shown("visible_time") {
        time_panel(ui, s, f, top, 0.0);
        top += 183.0;
    }
    if shown("visible_vintage_scope") {
        scope(ui, s, top, 0.0);
        top += 145.0;
    }
    if shown("visible_vintage_cpu") {
        cpu(ui, s, top, 0.0);
        top += 107.0;
    }
    if shown("visible_utilities") && skin::has_frame("DiskBtns", 7) {
        skin::native(ui.c, "ShortcutToolBar", 0, top, 0.0);
        for (i, (action, hint)) in [
            (Action::MenuItem(0, menus::FILE_OPEN), "Open project"),
            (Action::Util(crate::app::Util::Save), "Save project"),
            (Action::MenuItem(0, menus::FILE_SAVE_AS), "Save project as"),
            (Action::MenuItem(0, menus::FILE_SAVE_VERSION), "Save a new version"),
        ].into_iter().enumerate() {
            let cell = Rect::new(top + 10.0 + (i % 2) as f32 * 18.0, 4.0 + (i / 2) as f32 * 15.0, 18.0, 15.0);
            let hot = ui.hit(cell, action, hint);
            skin::native(ui.c, "DiskBtns", i * 2 + usize::from(hot), cell.x, cell.y);
        }
        shortcut_bank(ui, s, top + 51.0, 6.0);
    } else {
        // Use the original window-only bank when file tools are hidden.
        skin::native(ui.c, "ShortcutBank", 0, top, 0.0);
        shortcut_bank(ui, s, top + 4.0, 6.0);
    }

    // Lower shelf: Transport, Pattern pad, and recording/snap.
    transport(ui, s, f, 376.0, 40.0);
    let mut lower = 573.0;
    if shown("visible_pattern") {
        pattern_panel(ui, s, lower, 40.0);
        lower += 113.0;
    }
    let recording = shown("visible_recording");
    let snap = shown("visible_snap");
    if recording && snap {
        skin::native(ui.c, "RecordToolBar", 0, lower, 40.0);
        recording_switches(ui, s, lower, 40.0);
        snap_readout(ui, s, Rect::new(lower + 130.0, 44.0, 65.0, 18.0));
        snap_switch(ui, s, Rect::new(lower + 143.0, 63.0, 52.0, 15.0), false);
        record_additions(ui, s, Rect::new(lower + 202.0, 40.0, 41.0, 40.0));
        lower += 243.0;
    } else {
        if recording {
            compact_recording(ui, s, lower, 40.0);
            lower += 168.0;
        }
        if snap {
            compact_snap(ui, s, lower, 40.0);
            lower += 91.0;
        }
    }
    utility_bank(ui, s, lower, 40.0, recording);

}

fn utility_bank(ui: &mut Ui, s: &State, x: f32, y: f32, recording: bool) {
    let panel = Rect::new(x, y, 62.0, 40.0);
    if !skin::draw(ui.c, "RecordRail", 0, panel) {
        crate::vintage::metal(ui.c, panel, ui.t.toolbar);
    }
    let full = crate::mixer::view(s) == crate::mixer::WIDE;
    let mixer_mode = if full { crate::mixer::TRADITIONAL } else { crate::mixer::WIDE };
    let mut items = vec![
        (Icon::Plug, Action::PickerOpen, "Plugin picker (F8)", false),
        (Icon::Project, Action::ProjectPicker, "Project picker", false),
        (Icon::Mixer, Action::Mix(crate::mixer::MHit::View(mixer_mode)),
            if full { "Traditional mixer view" } else { "Full Original mixer view" }, full),
    ];
    if shown("visible_pattern") {
        items.insert(0, (Icon::Plus, Action::PatternAdd, "New pattern", false));
    }
    if recording {
        items.push((Icon::AutoScroll, Action::AutoScroll, "Auto scrolling", crate::autoscroll::on()));
    }
    if shown("visible_utilities") && !skin::has_frame("DiskBtns", 7) {
        items.push((Icon::Save, Action::Util(crate::app::Util::Save), "Save project", false));
    }
    for (i, (icon, action, hint, on)) in items.into_iter().enumerate() {
        let r = Rect::new(x + 3.0 + (i % 3) as f32 * 19.0, y + 4.0 + (i / 3) as f32 * 17.0, 17.0, 15.0);
        let hot = ui.hit(r, action, hint);
        crate::vintage::recess(ui.c, r, Color::rgb(if hot { 0x52676c } else { 0x37484c }));
        icons::draw(ui.c, icon, r.inset(3.0), ui.t.text);
        if on {
            ui.c.fill_rect(Rect::new(r.right()-4.0, r.bottom()-4.0, 2.0, 2.0), ui.t.accent);
        }
    }
}

fn master_lever(ui: &mut Ui, s: &State, x: f32, id: KnobId, key: &str) {
    if !shown(key) {
        ui.c.fill_rect(Rect::new(x - 9.0, 3.0, 18.0, 74.0), Color::rgb(0x5f686d));
        return;
    }
    let hit = Rect::new(x - 9.0, 4.0, 18.0, 68.0);
    let hot = ui.hit(hit, Action::Knob(id), s.knob_text(id));
    let y = 67.0 - s.knob(id).clamp(0.0, 1.0) * 56.0;
    if id == KnobId::MasterVol && shown(crate::vintage::GAIN_MARKS) {
        let unity = (67.0 - 0.8 * 56.0_f32).round();
        ui.c.fill_rect(Rect::new(x - 13.0, unity, 4.0, 1.0), crate::vintage::UNITY_INK);
    }
    crate::vintage::fader(ui.c, x, y, hot);
}

pub(super) fn activity(ui: &mut Ui, s: &State, f: &FrameInfo, x: f32, y: f32, on_face: bool) {
    if !shown("visible_vintage_activity") {
        if on_face {
            ui.c.fill_rect(Rect::new(x - 17.0, y - 2.0, 28.0, 22.0), Color::rgb(0x5f686d));
        }
        return;
    }
    let beat = f64::from(s.project.beat_ticks().max(1));
    let sync = f.playing && !f.paused && f.tick.rem_euclid(beat) < beat * 0.12;
    for (offset, lit, live, label, hint) in [
        (0.0, sync, Live::Clock, "SYN", "Audio sync beat"),
        (10.0, s.vintage_midi_until.is_some(), Live::VintageMidi, "MIDI", "MIDI input activity"),
    ] {
        let lamp = Rect::new(x, y + offset, 7.0, 7.0);
        if !on_face {
            ui.c.text(Font::vintage(8), x - 17.0, lamp.y - 1.0, ui.t.text_dim, label);
        }
        ui.live(live, lamp);
        ui.hit(Rect::new(x - 17.0, lamp.y, 24.0, lamp.h), Action::Swallow, hint);
        if (!on_face || lit) && !skin::draw(ui.c, "RecordLed", usize::from(lit), lamp) {
            crate::vintage::recess(ui.c, lamp, if lit { ui.t.accent } else { ui.t.sunken_dark });
        }
    }
}

fn shortcut_bank(ui: &mut Ui, s: &State, x: f32, y: f32) {
    // 145 × 25 px opening in the FL4 housing. Each cell starts on an exact sprite boundary.
    for (slot, (idx, win, hint)) in [
        (0, Win::Playlist, "Playlist (F5)"), (2, Win::ChannelRack, "Channel rack (F6)"),
        (1, Win::PianoRoll, "Piano roll (F7)"), (4, Win::Browser, "Browser"), (3, Win::Mixer, "Mixer (F9)"),
    ].into_iter().enumerate() {
        let cell = Rect::new(x + slot as f32 * 29.0, y, 29.0, 25.0);
        if !shown(WINDOW_BUTTONS[idx]) {
            ui.c.fill_rect(cell, Color::rgb(0x35434a));
            continue;
        }
        let hot = ui.hit(cell, Action::ToggleWin(win), hint);
        skin::native(ui.c, "SCBtns", slot * 2 + usize::from(s.shown(win) || hot), cell.x, cell.y);
    }
}

pub(super) fn compact_shortcuts(ui: &mut Ui, s: &State, x: f32, y: f32) -> bool {
    if !skin::has_frame("SCBtns", 9) || !skin::native(ui.c, "ShortcutBank", 0, x + 7.0, y) {
        return false;
    }
    shortcut_bank(ui, s, x + 11.0, y + 6.0);
    true
}

pub(super) fn pattern_panel(ui: &mut Ui, s: &State, x: f32, y: f32) -> bool {
    if !skin::native(ui.c, "PatternToolBar", 0, x, y) { return false; }
    for n in 1..=9u16 {
        let key = usize::from(n - 1);
        let cell = Rect::new(x + 11.0 + (key % 3) as f32 * 16.0,
            y + 4.0 + (2 - key / 3) as f32 * 10.0, 16.0, 10.0);
        let hot = ui.hit(cell, Action::PatternDirect(n), format!("Select pattern {n}"));
        let frame = key + 9 * usize::from(s.pattern == n || hot);
        if !skin::native(ui.c, "PatternKey", frame, cell.x, cell.y) {
            crate::vintage::metal(ui.c, cell, if s.pattern == n || hot { ui.t.accent } else { ui.t.button });
            ui.c.text_center(Font::vintage(8), cell, ui.t.text, &n.to_string());
        }
    }
    let display = Rect::new(x + 68.0, y + 4.0, 34.0, 21.0);
    ui.hit(display, Action::PatternNext, crate::patterns::name_of(s, s.pattern.max(1)));
    ui.c.text_center(Font::vintage(13).bold(), display, Color::rgb(0x233a3c), &s.pattern.to_string());
    ui.hit(Rect::new(x + 65.0, y + 26.0, 41.0, 13.0), Action::PatternPrev, "Pattern options");
    true
}

fn time_panel(ui: &mut Ui, s: &State, f: &FrameInfo, x: f32, y: f32) {
    skin::native(ui.c, "TimeToolBar", 0, x, y);
    let clock = Rect::new(x + 10.0, y + 5.0, 165.0, 29.0);
    ui.live(Live::Clock, clock);
    ui.hit(clock, Action::Clock, s.clock.hint());
    let ppq = f64::from(s.project.ppq.max(1));
    let sig = s.project.signature().unwrap_or((s.project.bar().0, 4));
    let secs = f.tick / ppq * 60.0 / s.project.tick_tempo(s.tempo.max(1.0));
    let (bar, step, tick) = s.clock.reading(secs, f.tick * 96.0 / ppq, sig);
    crate::vintage::clock(ui.c, clock, bar, step, tick);
    if let Some(count) = s.rec.count.filter(|c| c.left() > 0) {
        ui.c.fill_rect(clock, Color::rgb(0x344c4f));
        ui.c.text_center(Font::vintage(12), clock, ui.t.text, &format!("COUNT IN {}", count.left()));
    }
}

fn recording_switches(ui: &mut Ui, s: &State, left: f32, top: f32) {
    use crate::record::Opt;
    for (dx,dy,on,act,hint) in [
        (9.0,3.0,s.toggles.contains(&"typing"),Action::Typing,"Typing keyboard to piano (Ctrl+T)"),
        (9.0,20.0,crate::metronome::on(),Action::Metronome,"Metronome (Ctrl+M) - right-click for sound and volume"),
        (49.0,3.0,Opt::Precount.on(),Action::RecOpt(Opt::Precount),"Recording lead-in"),
        (49.0,20.0,Opt::Wait.on(),Action::RecOpt(Opt::Wait),"Start on input"),
        (89.0,3.0,Opt::Blend.on(),Action::RecOpt(Opt::Blend),"Blend recorded notes"),
        (89.0,20.0,crate::step_edit::on(),Action::StepEdit,"Step recording (Ctrl+E)"),
    ] {
        let (x, y) = (left + dx, top + dy);
        ui.hit(Rect::new(x,y,38.,16.),act,hint);
        // The lower wells start 15 px below the upper wells; their hit rows are 17 px apart.
        let led_y = top + if dy == 3.0 { 7.0 } else { 22.0 };
        skin::native(ui.c, "RecordLed", usize::from(on), x + 2.0, led_y);
    }
}

/// Keep the original six switches in the 168 px recording panel even when
/// visibility settings or available width prevent using the full vintage bar.
pub(super) fn compact_recording(ui: &mut Ui, s: &State, x: f32, y: f32) -> bool {
    if !skin::native(ui.c, "RecordSwitches", 0, x, y) {
        return false;
    }
    recording_switches(ui, s, x, y);
    record_additions(ui, s, Rect::new(x + 127.0, y, 41.0, 40.0));
    true
}

/// The two Lemon controls are a fourth column of the original switch bank.
fn record_additions(ui: &mut Ui, s: &State, panel: Rect) {
    use crate::record::Opt;
    if !skin::draw(ui.c, "RecordRail", 0, panel) {
        crate::vintage::metal(ui.c, panel, ui.t.toolbar);
    }
    for (dy, icon, on, action, hint) in [
        (3.0, Icon::LoopRecord, Opt::Loop.on(), Action::RecOpt(Opt::Loop), crate::record::option_hint(Opt::Loop)),
        (20.0, Icon::Slide, s.params.capture_armed, Action::AutomationCapture,
            "Create automation: click, move a control; click again to cancel"),
    ] {
        let hit = Rect::new(panel.x + 1.0, panel.y + dy, panel.w - 2.0, 16.0);
        let hot = ui.hit(hit, action, hint);
        let socket = Rect::new(hit.x + 2.0, hit.y + 2.0, 14.0, 14.0);
        if !skin::native(ui.c, "RecordSocket", 0, socket.x, socket.y) {
            crate::vintage::recess(ui.c, socket, ui.t.sunken_dark);
        }
        if !skin::native(ui.c, "RecordLed", usize::from(on), socket.x + 2.0, socket.y + 2.0) && on {
            ui.c.fill_rect(socket.inset(4.0), ui.t.accent);
        }
        record_symbol(ui, icon, Rect::new(hit.x + 19.0, hit.y + 1.0, 17.0, 14.0),
            Color::rgb(if hot { 0xe4eeed } else { 0xc0d0d8 }));
    }
}

fn studio_title(ui: &mut Ui, s: &State, f: &FrameInfo, area: Rect, font: Font) {
    let project = f.title.strip_prefix('[').and_then(|name| name.strip_suffix(']')).unwrap_or(&f.title);
    let label = format!("Lemon Studio Michael Lane Edition  -  {project}");
    let width = ui.c.text_width(font, &label).ceil();
    if width <= area.w {
        ui.c.text_clip(font, area.x, area.y, area.w, ui.t.text, &label);
        *s.vintage_title.borrow_mut() = Default::default();
        return;
    }
    let cycle_width = width + 20.0;
    let offset = s.vintage_title.borrow_mut().paint(&label, cycle_width, std::time::Instant::now());
    ui.live(Live::VintageTitle, area.outset(1.0));
    ui.c.push_clip(area);
    for x in [area.x - offset, area.x - offset + cycle_width] {
        if x < area.right() && x + width > area.x {
            ui.c.text(font, x, area.y, ui.t.text, &label);
        }
    }
    ui.c.pop_clip();
}

pub(super) fn cpu(ui: &mut Ui, s: &State, x: f32, y: f32) {
    let panel = Rect::new(x, y, 107.0, 40.0);
    ui.hit(panel, Action::CpuPanel, s.cpu.hint());
    if !skin::native(ui.c, "CPUToolBar", 0, x, y) {
        crate::vintage::metal(ui.c, panel, ui.t.toolbar);
        ui.c.text(Font::vintage(9), x + 10.0, y + 20.0, ui.t.text, "CPU");
        ui.c.text(Font::vintage(9), x + 46.0, y + 28.0, ui.t.text, "POLY");
    }
    let graph = Rect::new(x + 11.0, y + 5.0, 62.0, 12.0);
    ui.c.fill_rect(graph, Color::rgb(0x131f20));
    for (i, &load) in s.cpu.history.iter().rev().take(31).enumerate() {
        let height = (load.min(100) as f32 * graph.h / 100.0).round();
        let ink = if load >= 100 { ui.t.meter_clip } else if load >= 75 { ui.t.meter_high } else { Color::rgb(0x8caca2) };
        ui.c.fill_rect(Rect::new(graph.right() - 2.0 - i as f32 * 2.0, graph.bottom() - height, 1.0, height), ink);
    }
    let reading = &s.cpu.now;
    let font = Font::vintage(9);
    let lcd = Color::rgb(0xf2c098);
    ui.c.text_center(font, Rect::new(x + 76.0, y + 4.0, 22.0, 14.0), lcd, &reading.load.min(999).to_string());
    let secondary = if s.cpu.fps_view {
        ui.c.fill_rect(Rect::new(x + 45.0, y + 27.0, 29.0, 10.0), ui.t.toolbar);
        ui.c.text(font, x + 55.0, y + 27.0, ui.t.text, "FPS");
        reading.fps as usize
    } else {
        reading.voices
    };
    ui.c.text_center(font, Rect::new(x + 76.0, y + 21.0, 22.0, 14.0), lcd, &secondary.min(999).to_string());
}

pub(super) fn scope(ui: &mut Ui, s: &State, x: f32, y: f32) {
    let panel = Rect::new(x, y, 145.0, 40.0);
    ui.hit(panel, Action::Swallow, "Live output waveform and stereo peak levels");
    ui.live(Live::VintageScope, panel);
    let plot = Rect::new(x + 11.0, y + 5.0, 65.0, 29.0);
    if !skin::native(ui.c, "MonitorToolBar", 0, x, y) {
        crate::vintage::metal(ui.c, panel, ui.t.toolbar);
        crate::vintage::recess(ui.c, plot.outset(1.0), Color::rgb(0x263e3f));
        ui.c.text(Font::vintage(9), x + 80.0, y + 30.0, ui.t.text, "MONITOR");
    }
    let wave = &s.monitor.wave;
    let mut last = None;
    let ink = Color::rgb(0xc9dca8);
    ui.c.push_clip(plot);
    for i in 0..65 {
        let (lo, hi) = wave[i * wave.len() / 65];
        let px = plot.x + i as f32;
        let top = plot.cy() - hi.clamp(-1.0, 1.0) * 13.0;
        let bottom = plot.cy() - lo.clamp(-1.0, 1.0) * 13.0;
        let mid = (top + bottom) / 2.0;
        if let Some((lx, ly)) = last {
            ui.c.line(lx, ly, px, mid, 1.0, ink);
        }
        if bottom - top > 1.0 {
            ui.c.line(px, top, px, bottom, 1.0, ink);
        }
        last = Some((px, mid));
    }
    ui.c.pop_clip();
    for ch in 0..2 {
        let meter = Rect::new(x + 81.0, y + 5.0 + ch as f32 * 11.0, 54.0, 10.0);
        let width = (s.monitor.meter_pos(s.monitor.meter_db[ch]) * meter.w).round();
        if width > 0.0 {
            let lit = Rect::new(meter.x, meter.y, width, meter.h);
            ui.c.push_clip(lit);
            if !skin::draw(ui.c, "PeakLit", 0, meter) {
                ui.c.fill_rect(lit, ink);
            }
            ui.c.pop_clip();
        }
        let (peak, held) = s.monitor.hold[ch];
        if held.is_some() {
            let peak_x = (s.monitor.meter_pos(peak) * (meter.w - 1.0)).round();
            ui.c.fill_rect(Rect::new(meter.x + peak_x, meter.y, 1.0, meter.h), ink);
        }
    }
}

pub(super) fn compact_info(ui: &mut Ui, s: &State, f: &FrameInfo, panel: Rect) {
    if panel.w < 10.0 { return; }
    let title = Rect::new(panel.x, panel.y, panel.w, 14.0);
    crate::vintage::recess(ui.c, title, Color::rgb(0x344c4f));
    skin::draw(ui.c, "CaptionGlass", 0, title.inset(2.0));
    studio_title(ui, s, f, Rect::new(title.x + 4.0, title.y + 2.0, title.w - 8.0, 11.0), Font::vintage(9));
    let hint = Rect::new(panel.x, panel.y + 17.0, panel.w, 15.0);
    crate::vintage::recess(ui.c, hint, Color::rgb(0x2f4647));
    let message = if f.hint.is_empty() { s.message.as_deref().unwrap_or("Ready") } else { &f.hint };
    ui.c.text_clip(Font::vintage(9), hint.x + 4.0, hint.y + 2.0, hint.w - 8.0, ui.t.text_dim, message);
}

pub(super) fn compact_snap(ui: &mut Ui, s: &State, x: f32, y: f32) {
    let readout = Rect::new(x + 5.0, y + 4.0, 81.0, 19.0);
    crate::vintage::recess(ui.c, readout, Color::rgb(0x091314));
    snap_readout(ui, s, readout.inset(2.0));
    snap_switch(ui, s, Rect::new(x + 31.0, y + 24.0, 55.0, 14.0), true);
}

fn snap_readout(ui: &mut Ui, s: &State, rect: Rect) {
    let hot = ui.hit(rect, Action::GlobalSnap, crate::snap::HINT);
    ui.c.text_clip(Font::vintage(10).bold(), rect.x + 3.0, rect.cy() - 6.0,
        rect.w - 13.0, ui.t.text, s.global_snap.now.label());
    os_ui::kit::triangle(ui.c, rect.right() - 6.0, rect.cy(), 2.5, os_ui::kit::Dir::Down,
        if hot { ui.t.glyph } else { ui.t.text_dim });
}

fn snap_switch(ui: &mut Ui, s: &State, rect: Rect, draw_label: bool) {
    let hot = ui.hit(rect, Action::GlobalSnapToggle, crate::snap::MAGNET_HINT);
    let on = s.global_snap.now != crate::playlist::Snap::None;
    let lamp = Rect::new(rect.x + 2.0, rect.cy().round() - 4.0, 9.0, 9.0);
    if !skin::native(ui.c, "RecordLed", usize::from(on), lamp.x, lamp.y) {
        crate::vintage::recess(ui.c, lamp, if on { ui.t.accent } else { ui.t.sunken_dark });
    }
    if draw_label {
        ui.c.text_right(Font::vintage(10).bold().italic(), rect.right() - 2.0, rect.y + 1.0,
            if hot { ui.t.glyph } else { ui.t.text_dim }, "SNAP");
    }
}

/// Recording buttons used by the wrapped toolbar and the Lemon additions.
pub(super) fn record_button(ui: &mut Ui, r: Rect, icon: Icon, on: bool, action: Action, hint: &'static str) {
    let hot = ui.hit(r, action.clone(), hint) || ui.down(r, &action);
    crate::vintage::metal(ui.c, r, if hot { ui.t.button_hi } else { ui.t.button });
    let ink = Color::rgb(0xc0d0d8);
    let (x, y) = (r.cx().round(), (r.cy() - 1.0).round());
    if matches!(icon, Icon::Slide | Icon::LoopRecord) {
        record_symbol(ui, icon, Rect::new(r.x, r.y - 1.0, r.w, r.h), ink);
        if on {
            ui.c.fill_rect(Rect::new(x - 6.0, r.bottom() - 3.0, 12.0, 1.0), ui.t.accent);
        }
        return;
    }
    let glyph = match icon {
        Icon::Metronome => Some(("RecordMetronome", 22.0, 18.0)),
        Icon::WaitInput => Some(("RecordWait", 20.0, 18.0)),
        Icon::Countdown => Some(("RecordCountdown", 21.0, 15.0)),
        _ => None,
    };
    let drawn = glyph.is_some_and(|(name, w, h)| skin::native(ui.c, name, 0, (x-w/2.0).round(), (y-h/2.0).round()));
    if !drawn {
        icons::draw(ui.c, icon, r.inset(5.0), ink);
    }
    if on {
        ui.c.fill_rect(Rect::new(x-6.0, r.bottom()-3.0, 12.0, 1.0), ui.t.accent);
    }
}

/// Small, engraved symbols with the same light and shadow as the FL4 switches.
fn record_symbol(ui: &mut Ui, icon: Icon, rect: Rect, ink: Color) {
    let (x, y) = (rect.cx().round(), rect.cy().round());
    let shade = Color::rgb(0x263137);
    let line = |ui: &mut Ui, points: &[(f32, f32)]| {
        ui.c.polyline(points, 3.0, shade);
        ui.c.polyline(points, 1.5, ink);
    };
    if icon == Icon::LoopRecord {
        line(ui, &[(x-5.0,y+2.0),(x-5.0,y-3.0),(x+5.0,y-3.0)]);
        line(ui, &[(x+5.0,y-1.0),(x+5.0,y+4.0),(x-5.0,y+4.0)]);
        ui.c.fill_polygon(&[(x+3.0,y-6.0),(x+7.0,y-3.0),(x+3.0,y)], ink);
        ui.c.fill_polygon(&[(x-3.0,y+1.0),(x-7.0,y+4.0),(x-3.0,y+7.0)], ink);
    } else {
        let points = [(x-5.0,y+4.0),(x,y-4.0),(x+5.0,y+1.0)];
        line(ui, &points);
        for (px, py) in points {
            ui.c.fill_rect(Rect::new(px,py,3.0,3.0), shade);
            ui.c.fill_rect(Rect::new(px-1.0,py-1.0,3.0,3.0), ink);
        }
    }
}

/// Paint the original transport group in either toolbar layout.
pub(super) fn transport(ui: &mut Ui, s: &State, f: &FrameInfo, x: f32, y: f32) {
    skin::native(ui.c, "TransportToolBar", 0, x, y);
    // The 112 x 22 strip sits inside the original recessed well.
    let button_y = y + 4.0;
    skin::native(ui.c, "TransportBtns", usize::from(s.song_mode), x + 11.0, button_y);
    ui.hit(Rect::new(x + 11.0, button_y, 28.0, 11.0), Action::PatSong(false), "Pattern mode");
    ui.hit(Rect::new(x + 11.0, button_y + 11.0, 28.0, 11.0), Action::PatSong(true), "Song mode");
    let play_frame = if f.playing && !f.paused { 3 } else { 2 };
    let record_frame = if s.rec.armed { 8 } else { 7 };
    for (offset, frame, action, hint) in [
        (39.0, play_frame, Action::Play, "Play / pause (Space)"),
        (67.0, 5, Action::Stop, "Stop"),
        (95.0, record_frame, Action::Record, "Record: arm/disarm"),
    ] {
        let button = Rect::new(x + offset, button_y, 28.0, 22.0);
        let down = ui.down(button, &action);
        ui.hit(button, action, hint);
        skin::native(ui.c, "TransportBtns", if down && frame == 5 { 6 } else { frame }, button.x, button.y);
    }
    let tempo = Rect::new(x + 130.0, y + 5.0, 55.0, 19.0);
    ui.hit(tempo, Action::Tempo, "Tempo: drag whole number or decimal digit; right-click for options");
    super::toolbar::tempo_readout(ui, s, tempo, f.tempo, Color::rgb(0x263e40), false);
}
