// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Piano-roll editing commands: quantize, legato, chop, glue, discard lengths, the Strum, Arpeggiate, Flam, Claw
//! machine, Limit, Flip, Scale levels, LFO, Quantize, Chop and Randomize dialogs, duplicate, mute, ghost channels, time markers, channel cycling and the
//! selection keys, with the Tools menu that lists them.
// spec: ui/piano-roll.md §3a, §10a, §11; ui/piano-keys.md

use os_model::Note;

use crate::app::{App, State};
use crate::piano;
use crate::playlist::{Cmd, Mark, PItem, Popup};

/// A piano-roll command (keyboard shortcut or Tools menu entry). Note values are `1/n` notes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NCmd {
    NoteColorMenu,NoteColor(u8),ApplyNoteColor,DrawSlide,DrawPorta,SelectedNoteType(u8),
    SessionPart(usize,u16),
    EndSession,
    Layer(u16,u16,bool),
    MenuKey(&'static str,bool,bool,bool),
    Tool(piano::Tool),
    View(&'static str),
    KeyboardStyle(bool),
    Zoom(f32),
    PlacePattern,
    Lane(usize),
    /// Move starts (and lengths unless `starts_only`) `strength` percent of the way to the snap grid.
    Quantize { strength: u8, starts_only: bool },
    Legato,
    /// Cut notes into snap-unit pieces from their own starts (Ctrl+U, spec: ui/piano-keys.md §4 rule 6; one-tick pieces
    /// without a unit, ui/piano-snap.md §6).
    QuickChop,
    /// Mute (true) or unmute the notes (Alt+M / Alt+Shift+M).
    Mute(bool),
    Glue,
    DiscardLengths,
    Duplicate,
    /// Open the Strum dialog (spec: ui/piano-strum.md).
    Strum,
    /// Open the Arpeggiate dialog (spec: ui/piano-arpeggiate.md).
    Arpeggiate,
    /// Open one of the tool window's dialogs (spec: ui/piano-flam.md, ui/piano-flip.md, ui/piano-level-scaling.md,
    /// ui/piano-limit.md, ui/piano-claw.md, ui/piano-lfo.md, ui/piano-quantize.md, ui/piano-chop.md,
    /// ui/piano-randomize.md).
    Dialog(crate::piano_dlg::Tool),
    Ghosts,
    AddMarker,
    /// The magnet button's menus (spec: ui/piano-snap.md): the snap setting, a stock length for new notes.
    Snap(crate::snap::EditorSnap),
    StockLength(crate::playlist::Snap),
    /// Make a channel rack row the piano roll's channel (the title's channel menu).
    Target(usize),
    ChannelMute,
    ChannelSolo,
    Transfer(usize, bool),
}

fn item(label: &'static str, c: NCmd) -> PItem {
    PItem::new(label, Cmd::Piano(c))
}

/// The toolbar's Tools menu (§11).
fn all_menu(s: &State, x: f32, y: f32) -> Popup {
    use crate::piano_dlg::Tool;
    use NCmd::*;
    let items = vec![
        PItem::head("File"),
        PItem::new("Import MIDI file...", Cmd::MidiImport(true)),
        PItem::new("Export as MIDI file...", Cmd::MidiExport(true)),
        PItem::head("Edit"),
        item("Duplicate", Duplicate).key("Ctrl+B"),
        item("Glue", Glue).key("Ctrl+G"),
        item("Discard lengths", DiscardLengths).key("Shift+D"),
        PItem::head("Tools"),
        PItem::new("Chord workspace...",Cmd::Harmony(crate::harmony::HCmd::Open)).key("Alt+P"),
        PItem::new("Key and scale...",Cmd::Harmony(crate::harmony::HCmd::Menu(10))),
        item("Quick quantize", Quantize { strength: 100, starts_only: false }).key("Ctrl+Q"),
        item("Quick quantize start times", Quantize { strength: 100, starts_only: true }).key("Shift+Q"),
        item("Quick quantize start times (50%)", Quantize { strength: 50, starts_only: true }),
        item("Quantize...", Dialog(Tool::Quantize)).key("Alt+Q"),
        item("Glue", Glue).key("Ctrl+G"),
        item("Quick legato", Legato).key("Ctrl+L"),
        item("Quick chop", QuickChop).key("Ctrl+U"),
        item("Chop...", Dialog(Tool::Chop)).key("Alt+U"),
        item("Strum...", Strum).key("Alt+S"),
        item("Arpeggiate...", Arpeggiate).key("Alt+A"),
        item("Flam...", Dialog(Tool::Flam)).key("Alt+F"),
        item("Claw machine...", Dialog(Tool::Claw)).key("Alt+W"),
        item("Limit...", Dialog(Tool::Limit)).key("Alt+K"),
        item("Flip...", Dialog(Tool::Flip)).key("Alt+Y"),
        item("Randomize...", Dialog(Tool::Randomize)).key("Alt+R"),
        item("Scale levels...", Dialog(Tool::Levels)).key("Alt+X"),
        item("LFO...", Dialog(Tool::Lfo)).key("Alt+O"),
        PItem::head("View"),
        item("Ghost channels", Ghosts).key("Alt+V").mark(Mark::Check(s.piano.ghosts)),
        item("Add time marker", AddMarker).key("Alt+T"),
    ];
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// Selected notes expose transfer directly in their context menu.
pub fn selected_menu(s:&State,x:f32,y:f32)->Popup {
    crate::menu_layout::Menu::PianoSelection.popup(default_selected_menu(s, x, y))
}

pub(crate) fn default_selected_menu(s:&State,x:f32,y:f32)->Popup {
    let mut menu=default_options_menu(s,x,y);
    menu.items.insert(0,crate::mpe::menu(s));
    if let Some(item)=menu.items.iter().find(|i|i.label=="Edit").and_then(|i|i.children.iter().find(|c|c.label=="Send Selected Notes To...")).cloned(){menu.items.insert(0,item);menu.items.insert(1,PItem::sep());}
    menu
}

/// Organized options use the same commands as keyboard editing and the tool dialogs.
pub fn options_menu(s:&State,x:f32,y:f32)->Popup {
    crate::menu_layout::Menu::Piano.popup(default_options_menu(s, x, y))
}

pub(crate) fn default_options_menu(s:&State,x:f32,y:f32)->Popup {
    let all=all_menu(s,x,y).items;
    let group=|name:&str| {let a=all.iter().position(|i|i.label==name).unwrap()+1;let b=all[a..].iter().position(|i|i.head).map_or(all.len(),|i|a+i);all[a..b].to_vec()};
    let k=|label,key,ctrl,shift,alt,shortcut|item(label,NCmd::MenuKey(key,ctrl,shift,alt)).key(shortcut);
    let mut edit=vec![k("Undo","z",true,false,false,"Ctrl+Z"),k("Redo","z",true,true,false,"Ctrl+Shift+Z"),PItem::sep(),k("Cut","x",true,false,false,"Ctrl+X"),k("Copy","c",true,false,false,"Ctrl+C"),k("Paste","v",true,false,false,"Ctrl+V"),k("Delete","Delete",false,false,false,"Del")];
    edit.extend(group("Edit"));
    let destinations=|copy| s.project.channels.iter().enumerate().filter(|(i,c)|*i!=s.selected_channel && !matches!(c.kind,os_model::ChannelKind::Automation)).map(|(i,_)|PItem::new(channel_name(s,i),Cmd::Piano(NCmd::Transfer(i,copy)))).collect();
    edit.push(crate::mpe::menu(s));
    edit.push(PItem::sub("Send Selected Notes To...",vec![PItem::sub("Move to channel",destinations(false)),PItem::sub("Copy to channel",destinations(true))]));
    edit.extend([PItem::sep(),k("Shift left","Left",false,true,false,"Shift+Left"),k("Shift right","Right",false,true,false,"Shift+Right"),k("Transpose up","Up",false,true,false,"Shift+Up"),k("Transpose down","Down",false,true,false,"Shift+Down"),k("Octave up","Up",true,false,false,"Ctrl+Up"),k("Octave down","Down",true,false,false,"Ctrl+Down"),item("Mute",NCmd::Mute(true)).key("Alt+M"),item("Unmute",NCmd::Mute(false)).key("Alt+Shift+M")]);
    let check=|label,key,default|item(label,NCmd::View(key)).mark(Mark::Check(view_on(key,default)));
    edit.push(check("D duplicates notes (typing keyboard off)","piano_d_duplicate",true));
    let mut helpers=group("View");helpers.retain(|i|!matches!(i.cmd,Cmd::Piano(NCmd::AddMarker)));
    helpers.extend([PItem::new("Key, scale and highlighting...",Cmd::Harmony(crate::harmony::HCmd::Menu(10))),check("Note grid highlights","piano_grid_highlights",true)]);
    let items=vec![
        PItem::sub("File",group("File")),PItem::sub("Edit",edit),
        PItem::sub("Tools",group("Tools")),
        PItem::sub("Stamp",crate::harmony::menu(s,2,x,y).items),
        PItem::sub("Select",vec![k("Select all","a",true,false,false,"Ctrl+A"),k("Deselect","d",true,false,false,"Ctrl+D"),k("Invert selection","i",false,true,false,"Shift+I"),k("Select by color","c",false,true,false,"Shift+C"),k("Select random","r",false,true,false,"Shift+R")]),
        PItem::sub("Snap",crate::piano_snap::snap_menu(s,x,y).items),
        PItem::sub("Note length",crate::piano_snap::lengths_menu(x,y).items),
        item("Note color...",NCmd::NoteColorMenu),
        item("Slide / glide notes",NCmd::DrawSlide).mark(Mark::Check(s.piano.draw_slide)),
        item("Portamento notes",NCmd::DrawPorta).mark(Mark::Check(s.piano.draw_porta)),
        PItem::sub("Draw mode",[(piano::Tool::Draw,"Pencil","P"),(piano::Tool::Paint,"Brush","B"),(piano::Tool::Slice,"Slice","C"),(piano::Tool::Select,"Select","E"),(piano::Tool::Stretch,"Stretch",""),(piano::Tool::Delete,"Delete",if view_on("piano_d_duplicate",true){""}else{"D"}),(piano::Tool::Mute,"Mute","T"),(piano::Tool::Zoom,"Zoom","Z"),(piano::Tool::Playback,"Playback","Y")].into_iter().map(|(tool,label,key)|item(label,NCmd::Tool(tool)).key(key).mark(if tool==piano::Tool::Stretch {Mark::Check(s.piano.stretch)}else{Mark::Radio(s.piano.tool==tool)})).collect()),
        PItem::sub("View",vec![PItem::sub("Keyboard style",vec![item("FL 11 style",NCmd::KeyboardStyle(true)).mark(Mark::Radio(classic_keyboard())),item("Piano-shaped",NCmd::KeyboardStyle(false)).mark(Mark::Radio(!classic_keyboard()))]),item("Zoom in",NCmd::Zoom(1.)).key(zoom_shortcut(true)),item("Zoom out",NCmd::Zoom(-1.)).key(zoom_shortcut(false)),check("Mini piano roll preview","piano_mini_preview",true),PItem::sub("Control lane",piano::Lane::ALL.iter().enumerate().map(|(i,l)|item(l.menu_name(),NCmd::Lane(i)).mark(Mark::Radio(s.piano.lane==*l))).collect())]),
        PItem::sub("Helpers",helpers),
        PItem::sub("Time markers",vec![item("Add time marker",NCmd::AddMarker).key("Alt+T")]),
        PItem::sub("MIDI parts",if let Some(session)=s.piano.clip_session.as_ref().filter(|_|piano::active_clip(s).is_some()){
            let mut rows=Vec::new();
            for i in 0..session.parts.len(){
                let Some(c)=session.clip(s,i) else{continue;};
                let os_model::ClipSource::Pattern(n)=c.source else{continue;};
                let Some(pattern)=s.project.pattern(n) else{continue;};
                for ch in pattern.notes.iter().map(|note|note.channel).collect::<std::collections::BTreeSet<_>>() {
                    if let Some(row)=s.project.channels.iter().position(|v|v.index==ch){
                        let label=format!("{} • Track {} • Tick {} • {}",crate::patterns::name_of(s,n),c.track+1,c.position,channel_name(s,row));
                        rows.push(PItem::new(label,Cmd::Piano(NCmd::SessionPart(i,ch))).mark(Mark::Radio(session.active==i && s.selected_channel==row)));
                    }
                }
            }
            rows.push(item("Return to pattern editing",NCmd::EndSession));rows
        }else{s.project.patterns.iter().flat_map(|p|{
            let channels:std::collections::BTreeSet<_>=p.notes.iter().map(|n|n.channel).collect();
            channels.into_iter().filter_map(move |ch|{
                let row=s.project.channels.iter().position(|c|c.index==ch)?;
                Some(PItem::sub(format!("{} / {}",crate::patterns::name_of(s,p.number),channel_name(s,row)),vec![
                    item("Show notes",NCmd::Layer(p.number,ch,false)).mark(Mark::Check(s.piano.layers.contains(&(p.number,ch)))),
                    item("Edit this part",NCmd::Layer(p.number,ch,true)).mark(Mark::Radio(s.pattern==p.number && row==s.selected_channel))
                ]))
            })
        }).collect()}),
        PItem::sub("Target channel",target_menu(s,x,y).items),
    ];
    Popup{x,y,items,sub:None,scroll:[0;2],cascade:Vec::new()}
}
pub fn classic_keyboard()->bool {crate::prefs::text("piano_keyboard_style").as_deref()!=Some("piano")}
pub fn view_on(key:&str,default:bool)->bool {crate::prefs::text(key).map_or(default,|v|v=="1")}
pub fn tools_menu(s:&State,x:f32,y:f32)->Popup {
    crate::menu_layout::Menu::PianoTools.popup(default_tools_menu(s, x, y))
}

pub(crate) fn default_tools_menu(s:&State,x:f32,y:f32)->Popup {
    let mut p=default_options_menu(s,x,y);
    p.items=p.items.into_iter().find(|i|i.label=="Tools").unwrap().children;
    p
}

/// The keys a held key repeats in the piano roll (spec: ui/piano-keys.md §4 rule 1): Shift / Ctrl / Alt + arrows (not
/// Ctrl+Shift+Up / Down), PgUp / PgDn, Ctrl+B and Ctrl+V.
pub fn repeats(key: &winit::keyboard::Key, ctrl: bool, alt: bool, shift: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown) => (shift || ctrl || alt) && !(ctrl && shift),
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => shift || ctrl || alt,
        Key::Named(NamedKey::PageUp | NamedKey::PageDown) => true,
        Key::Character(c) => ctrl && !alt && (c.eq_ignore_ascii_case("b") || c.eq_ignore_ascii_case("v")),
        _ => false,
    }
}

/// The piano roll's keyboard shortcuts (spec: ui/piano-roll.md §3a, §9, §10a, §11; ui/piano-keys.md). Returns true
/// when the key was taken. Called while the piano roll is shown and the playlist is not in front of it.
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    if event_key(app, key, ctrl, alt) { return true; }
    let shift = app.shift;
    if ctrl && shift && !alt && matches!(key, Key::Named(NamedKey::Enter)) {
        app.state.message = run(app, NCmd::PlacePattern);
        return true;
    }
    if let Key::Named(NamedKey::PageUp | NamedKey::PageDown) = key {
        let modifiers = match crate::prefs::index(&crate::prefs::PIANO_ZOOM_KEYS) {
            1 => (true, false, false),
            2 => (false, true, false),
            3 => (false, false, true),
            _ => (false, false, false),
        };
        if (ctrl, alt, shift) == modifiers {
            return zoom_key(app, if matches!(key, Key::Named(NamedKey::PageUp)) { 1. } else { -1. });
        }
    }
    let is = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    if alt&&!ctrl&&matches!(key,Key::Character(c) if is(c,"p")){crate::harmony::run(app,crate::harmony::HCmd::Open);return true;}
    let cmd = match key {
        Key::Character(c) if ctrl && is(c, "q") => NCmd::Quantize { strength: 100, starts_only: false },
        Key::Character(c) if shift && !ctrl && !alt && is(c, "q") => NCmd::Quantize { strength: 100, starts_only: true },
        Key::Character(c) if ctrl && !alt && is(c, "l") => NCmd::Legato,
        Key::Character(c) if ctrl && !alt && is(c, "u") => NCmd::QuickChop,
        Key::Character(c) if ctrl && !alt && is(c, "g") => NCmd::Glue,
        Key::Character(c) if ctrl && !alt && is(c, "b") => NCmd::Duplicate,
        Key::Character(c) if !ctrl && !alt && !shift && is(c,"d") && !app.typing_on() && view_on("piano_d_duplicate",true) => NCmd::Duplicate,
        Key::Character(c) if shift && !ctrl && !alt && is(c, "d") => NCmd::DiscardLengths,
        Key::Character(c) if alt && !ctrl && is(c, "m") => NCmd::Mute(!shift),
        Key::Character(c) if alt && !ctrl && is(c, "q") => NCmd::Dialog(crate::piano_dlg::Tool::Quantize),
        Key::Character(c) if alt && !ctrl && is(c, "u") => NCmd::Dialog(crate::piano_dlg::Tool::Chop),
        Key::Character(c) if alt && !ctrl && is(c, "s") => NCmd::Strum,
        Key::Character(c) if alt && !ctrl && is(c, "a") => NCmd::Arpeggiate,
        Key::Character(c) if alt && !ctrl && is(c, "f") => NCmd::Dialog(crate::piano_dlg::Tool::Flam),
        Key::Character(c) if alt && !ctrl && is(c, "w") => NCmd::Dialog(crate::piano_dlg::Tool::Claw),
        Key::Character(c) if alt && !ctrl && is(c, "k") => NCmd::Dialog(crate::piano_dlg::Tool::Limit),
        Key::Character(c) if alt && !ctrl && is(c, "y") => NCmd::Dialog(crate::piano_dlg::Tool::Flip),
        Key::Character(c) if alt && !ctrl && is(c, "x") => NCmd::Dialog(crate::piano_dlg::Tool::Levels),
        Key::Character(c) if alt && !ctrl && is(c, "o") => NCmd::Dialog(crate::piano_dlg::Tool::Lfo),
        Key::Character(c) if alt && !ctrl && is(c, "r") => NCmd::Dialog(crate::piano_dlg::Tool::Randomize),
        Key::Character(c) if alt && !ctrl && is(c, "v") => NCmd::Ghosts,
        Key::Character(c) if alt && !ctrl && !shift && is(c, "t") => NCmd::AddMarker,
        Key::Character(c) if ctrl && !alt && is(c, "d") => {
            app.state.piano.clear_selection();
            app.invalidate();
            return true;
        }
        Key::Character(c) if ctrl && !alt && is(c, "x") => {
            let s = &mut app.state;
            let sel = s.piano.selection(s.pattern, piano::channel(s));
            if sel.is_empty() {
                return true;
            }
            let all = piano::notes(s);
            s.piano.clipboard = sel.iter().filter_map(|&i| all.get(i).copied()).collect();
            piano::snapshot(s);
            piano::delete(s, sel);
            app.invalidate();
            return true;
        }
        // Selection keys (spec: ui/piano-keys.md §4 rules 11, 12, 14).
        Key::Character(c) if shift && !ctrl && !alt && matches!(c.to_ascii_lowercase().as_str(), "i" | "r" | "m" | "c") => {
            return pick(app, &c.to_ascii_lowercase());
        }
        Key::Named(NamedKey::Enter) if ctrl && !alt && !shift => {
            span_to_range(&mut app.state);
            app.invalidate();
            return true;
        }
        // Channel cycling (§11): Ctrl+K and (typing keyboard off) G / K = previous / next channel, H / J = previous / next
        // channel with notes in the pattern; Shift+K / Shift+H = next channel / next with notes, also with the typing
        // keyboard on (spec: ui/piano-keys.md §4 rule 13).
        Key::Character(c) if ctrl && !alt && !shift && is(c, "k") => return cycle_channel(app, 1, false),
        Key::Character(c) if shift && !ctrl && !alt && is(c, "k") => return cycle_channel(app, 1, false),
        Key::Character(c) if shift && !ctrl && !alt && is(c, "h") => return cycle_channel(app, 1, true),
        Key::Character(c) if !ctrl && !alt && !shift && !app.typing_on() && matches!(c.to_ascii_lowercase().as_str(), "g" | "k" | "h" | "j") => {
            let l = c.to_ascii_lowercase();
            let dir = if l == "g" || l == "h" { -1 } else { 1 };
            return cycle_channel(app, dir, l == "h" || l == "j");
        }
        // The global snap on / off (spec: ui/global-snap.md §1).
        Key::Named(NamedKey::Backspace) if !ctrl && !alt => {
            app.state.message = Some(crate::snap::toggle(&mut app.state));
            app.invalidate();
            return true;
        }
        _ => return false,
    };
    if let Some(m) = run(app, cmd) {
        app.state.message = Some(m);
    }
    app.invalidate();
    true
}

/// Clipboard/delete keys in channel event lanes must never silently edit notes.
fn event_key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let s=&mut app.state;
    if !s.piano.event_focus || !s.piano.lane.is_channel() || alt { return false; }
    let command=match key {
        Key::Character(c) if ctrl => c.to_ascii_lowercase(),
        Key::Named(NamedKey::Delete) if !ctrl => "delete".into(),
        _ => return false,
    };
    if !matches!(command.as_str(), "a"|"c"|"x"|"v"|"delete") { return false; }
    let (Some(ch),Some(param))=(piano::channel(s),s.piano.lane.param()) else { return true; };
    let range=s.piano.range.map(|(a,b)|(a.min(b),a.max(b)));
    let points:Vec<_>=piano::controls(s).iter().filter(|c|c.channel()==ch && c.param()==param
        && (command=="a" || range.is_none_or(|(a,b)|a<=c.position && c.position<b))).copied().collect();
    match command.as_str() {
        "a" => {
            s.piano.clear_selection();
            if let (Some(a),Some(b))=(points.iter().map(|c|c.position).min(),points.iter().map(|c|c.position).max()) {
                s.piano.range=Some((a,b.saturating_add(1)));
            }
        }
        "c"|"x" => {
            if !points.is_empty() { s.piano.event_clipboard=points.clone(); }
            s.message=Some(format!("Copied {} {} events",points.len(),s.piano.lane.name()));
            if command=="x" && !points.is_empty() {
                piano::snapshot_as(s,"Cut piano roll events");
                piano::pattern_mut(s).controls.retain(|c|!points.contains(c));
            }
        }
        "delete" => {
            if !points.is_empty() {
                piano::snapshot_as(s,"Delete piano roll events");
                piano::pattern_mut(s).controls.retain(|c|!points.contains(c));
            }
        }
        "v" => {
            let clip=s.piano.event_clipboard.clone();
            if clip.first().is_some_and(|c|c.param()!=param) {
                s.message=Some("Choose the same channel control lane to paste its events".into());
            } else if let Some(first)=clip.iter().map(|c|c.position).min() {
                let at=if s.piano.marker_placed {s.piano.marker} else {range.map_or(first-piano::bar_start(s,first),|(a,_)|a)};
                piano::snapshot_as(s,"Paste piano roll events");
                for c in &clip {piano::set_control(s,ch,param,at.saturating_add(c.position-first),c.value);}
                let end=clip.iter().map(|c|at.saturating_add(c.position-first).saturating_add(1)).max().unwrap_or(at);
                s.piano.range=Some((at,end));
            }
        }
        _=>{}
    }
    app.invalidate();
    true
}

pub(crate) fn zoom_shortcut(inward: bool) -> &'static str {
    const KEYS: [[&str; 2]; 4] = [
        ["Page Up", "Page Down"], ["Ctrl+Page Up", "Ctrl+Page Down"],
        ["Alt+Page Up", "Alt+Page Down"], ["Shift+Page Up", "Shift+Page Down"],
    ];
    KEYS[crate::prefs::index(&crate::prefs::PIANO_ZOOM_KEYS)][usize::from(!inward)]
}

/// PgUp / PgDn: zoom time around the mouse when it is over the grid, else around the grid's left edge (§3a).
fn zoom_key(app: &mut App, notches: f32) -> bool {
    let Some(grid) = app.piano_grid else { return false };
    let g = piano::Geom::new(&app.state, grid);
    let x = if grid.contains(app.mouse.0, app.mouse.1) { app.mouse.0 } else { grid.x };
    piano::zoom_time(&mut app.state, &g, x, notches);
    app.invalidate();
    true
}

/// Shift+I / R / M / C (spec: ui/piano-keys.md §4 rules 11 and 14): invert the selection among the channel's notes,
/// select one of them or about half of them at random, or the ones of the current colour (the colour group new notes
/// get, from the last note touched).
fn pick(app: &mut App, letter: &str) -> bool {
    let s = &mut app.state;
    let Some(ch) = piano::channel(s) else { return true };
    let sel = s.piano.selection(s.pattern, Some(ch));
    let all = piano::notes(s);
    let mine: Vec<usize> = (0..all.len()).filter(|&i| all[i].channel == ch).collect();
    let mut rng = s.piano.rng;
    let one = |rng: &mut u64| mine.get((next(rng) % mine.len().max(1) as u64) as usize).copied();
    let colour = |n: &Note| n.group & 0x0F; // the colour group: byte 19's low nibble
    let now = s.piano.last_controls.map_or(0, |n| colour(&n));
    let new: Vec<usize> = match letter {
        "i" => mine.iter().copied().filter(|i| !sel.contains(i)).collect(),
        "r" => one(&mut rng).into_iter().collect(),
        "m" => {
            let half: Vec<usize> = mine.iter().copied().filter(|_| next(&mut rng) & 1 == 1).collect();
            if half.is_empty() { one(&mut rng).into_iter().collect() } else { half }
        }
        _ => mine.iter().copied().filter(|&i| colour(&all[i]) == now).collect(),
    };
    s.piano.rng = rng;
    s.piano.select(s.pattern, ch, new);
    app.invalidate();
    true
}

/// Ctrl+Enter (spec: ui/piano-keys.md §4 rule 12): the time selection becomes the selected notes' span (first start to
/// last end) and the play-start marker goes to its start; nothing selected: nothing.
pub fn span_to_range(s: &mut State) {
    let sel = s.piano.selection(s.pattern, piano::channel(s));
    let all = piano::notes(s);
    let picked: Vec<&Note> = sel.iter().filter_map(|&i| all.get(i)).collect();
    let (Some(a), Some(b)) = (picked.iter().map(|n| n.position).min(), picked.iter().map(|n| n.position + piano::shown_len(s, n)).max()) else { return };
    s.piano.range = Some((a, b));
    s.piano.marker = a;
}

/// Select the channel `dir` rows away, wrapping; `with_notes` skips channels without notes in the pattern (§11).
pub(crate) fn cycle_channel(app: &mut App, dir: i32, with_notes: bool) -> bool {
    let s = &mut app.state;
    let n = s.project.channels.len();
    if n == 0 {
        return true;
    }
    let mut row = s.selected_channel.min(n - 1);
    for _ in 0..n {
        row = (row as i32 + dir).rem_euclid(n as i32) as usize;
        if !with_notes || has_notes(s, row) {
            s.message = Some(target(s, row));
            break;
        }
    }
    app.invalidate();
    true
}

/// Whether a channel rack row has notes in the current pattern.
fn has_notes(s: &State, row: usize) -> bool {
    let ch = s.project.channels[row].index;
    piano::notes(s).iter().any(|nt| nt.channel == ch)
}

fn channel_name(s: &State, row: usize) -> String {
    s.project.channels[row].name.clone().unwrap_or_else(|| format!("Channel {}", row + 1))
}

/// Make `row` the piano roll's channel; the rack selects it. Returns the status message.
pub fn target(s: &mut State, row: usize) -> String {
    s.selected_channel = row;
    s.rack.select_only(row);
    s.piano.clear_selection();
    format!("Piano roll: {}", channel_name(s, row))
}

/// The title's channel menu (spec: ui/piano-target-channel.md §2, §5): the channels with notes in the pattern,
/// then one section per rack filter group and Unsorted; a ✓ marks the channels with notes.
pub fn target_menu(s: &State, x: f32, y: f32) -> Popup {
    let n = s.project.channels.len();
    let row_item = |row: usize| {
        let it = PItem::new(channel_name(s, row), Cmd::Piano(NCmd::Target(row)));
        if has_notes(s, row) { it.mark(Mark::Check(true)) } else { it }
    };
    let head = |name: String| PItem { label: name.into(), ..PItem::head("") };
    let mut items = vec![
        item("Mute channel",NCmd::ChannelMute).mark(Mark::Check(s.project.channels.get(s.selected_channel).is_some_and(|c|!c.enabled))),
        item("Solo channel",NCmd::ChannelSolo).mark(Mark::Check(s.rack.soloed==Some(s.selected_channel))),
        PItem::head("Current pattern")];
    items.extend((0..n).filter(|&r| has_notes(s, r)).map(row_item));
    for g in &s.rack.groups {
        let rows: Vec<usize> = g.members.iter().copied().filter(|&r| r < n).collect();
        if !rows.is_empty() {
            items.push(head(g.name.clone()));
            items.extend(rows.into_iter().map(row_item));
        }
    }
    let unsorted: Vec<usize> = (0..n).filter(|r| !s.rack.groups.iter().any(|g| g.members.contains(r))).collect();
    if !unsorted.is_empty() {
        items.push(PItem::head("Unsorted"));
        items.extend(unsorted.into_iter().map(row_item));
    }
    items.push(PItem::sep());
    items.push(PItem::not_yet("Auto locate"));
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// Run a command; returns a status message.
pub fn run(app: &mut App, cmd: NCmd) -> Option<String> {
    app.state.playlist.popup = None;
    if let NCmd::MenuKey(key,ctrl,shift,alt)=cmd {
        use winit::keyboard::{Key,NamedKey};
        let key=match key {"Delete"=>Key::Named(NamedKey::Delete),"Left"=>Key::Named(NamedKey::ArrowLeft),"Right"=>Key::Named(NamedKey::ArrowRight),"Up"=>Key::Named(NamedKey::ArrowUp),"Down"=>Key::Named(NamedKey::ArrowDown),v=>Key::Character(v.into())};
        let previous=(app.ctrl,app.shift,app.alt);app.shift=shift;
        app.key(&key,ctrl,alt);
        (app.ctrl,app.shift,app.alt)=previous;
        return None;
    }
    let s = &mut app.state;
    // Quick quantize, Discard lengths and Quick chop use the piano roll's snap (spec: ui/piano-snap.md §6,
    // ui/global-snap.md §3).
    let unit = crate::piano_snap::unit(s);
    let done = |changed: bool, what: &str| if changed { format!("Piano roll: {what}") } else { format!("Piano roll: {what}: nothing to change") };
    let msg = match cmd {
        NCmd::MenuKey(..)=>unreachable!(),
        NCmd::NoteColorMenu=>{s.playlist.popup=Some(crate::piano_note_mode::menu(s,app.mouse.0,app.mouse.1));app.invalidate();return None;},
        NCmd::NoteColor(n)=>{crate::piano_note_mode::choose_color(s,n);app.invalidate();return None;},
        NCmd::ApplyNoteColor=>{crate::piano_note_mode::apply_selection(s,None);app.invalidate();return None;},
        NCmd::SelectedNoteType(n)=>{crate::piano_note_mode::apply_selection(s,Some(n));app.invalidate();return None;},
        NCmd::DrawSlide=>{s.piano.draw_slide=!s.piano.draw_slide;app.invalidate();return None;},
        NCmd::DrawPorta=>{s.piano.draw_porta=!s.piano.draw_porta;app.invalidate();return None;},
        NCmd::EndSession=>{s.piano.clip_session=None;s.piano.scroll_x=0.;s.piano.clear_selection();app.invalidate();return None;}
        NCmd::SessionPart(i,ch)=>{
            let clip=s.piano.clip_session.as_ref()?.clip(s,i)?;
            let os_model::ClipSource::Pattern(n)=clip.source else{return None;};
            let row=s.project.channels.iter().position(|c|c.index==ch)?;
            s.piano.clip_session.as_mut()?.active=i;
            crate::patterns::select(s,n);s.selected_channel=row;s.rack.select_only(row);s.piano.clear_selection();s.piano.dirty=true;
            app.invalidate();return None;
        }
        NCmd::Layer(p,ch,edit)=>{
            s.piano.clip_session=None;
            if edit {
                if let Some(row)=s.project.channels.iter().position(|c|c.index==ch){
                    if let Some(prev)=piano::channel(s){s.piano.layers.insert((s.pattern,prev));}
                    crate::patterns::select(s,p);s.selected_channel=row;s.rack.select_only(row);s.piano.select(p,ch,Vec::new());s.piano.layers.insert((p,ch));s.piano.dirty=true;
                }
            }else if !s.piano.layers.remove(&(p,ch)){s.piano.layers.insert((p,ch));}
            app.invalidate();return None;
        }
        NCmd::ChannelMute=>{let row=s.selected_channel;crate::rack::toggle_mute(s,row);return None;},
        NCmd::ChannelSolo=>{let row=s.selected_channel;crate::rack::solo(s,row);return None;},
        NCmd::Transfer(row,copy)=>return Some(transfer(s,row,copy)),
        NCmd::Tool(tool)=>{s.piano.choose_tool(tool);return None;},
        NCmd::KeyboardStyle(classic)=>{crate::prefs::set_text("piano_keyboard_style",if classic{"classic"}else{"piano"});app.invalidate();return None;},
        NCmd::View(key)=>{crate::prefs::set_text(key,if view_on(key,true){"0"}else{"1"});return None;},
        NCmd::Zoom(n)=>{zoom_key(app,n);return None;},
        NCmd::PlacePattern => return Some(crate::playlist::place_current_pattern(app)),
        NCmd::Lane(i)=>{s.piano.lane=piano::Lane::ALL[i.min(piano::Lane::ALL.len()-1)];s.piano.event_focus=true;return None;},
        NCmd::Quantize { strength, starts_only } => {
            let k = f32::from(strength.min(100)) / 100.0;
            // Without a unit ((none), Events, Markers) nothing changes.
            let ch = unit.is_some_and(|u| apply(s, |_, ns| ns.into_iter().map(|n| quantize(n, u, k, starts_only)).collect()));
            done(ch, if starts_only { "quantized start times" } else { "quantized" })
        }
        NCmd::Legato => {
            let end = piano::pattern_end(s);
            done(apply(s, |_, ns| legato(ns, end)), "legato")
        }
        NCmd::QuickChop => done(crate::mpe::chop(s,unit.unwrap_or(1)), "chopped"),
        NCmd::Mute(on) => {
            let f = |ns: Vec<Note>| ns.into_iter().map(|mut n| { n.set_muted(on); n }).collect();
            done(apply(s, |_, ns| f(ns)), if on { "muted" } else { "unmuted" })
        }
        NCmd::Glue => done(apply(s, |_, ns| glue(ns)), "glued"),
        NCmd::DiscardLengths => done(apply(s, |_, ns| ns.into_iter().map(|n| Note { length: unit.unwrap_or(0), ..n }).collect()), "discarded lengths"),
        NCmd::Duplicate => {
            duplicate(s);
            return None;
        }
        NCmd::Strum => {
            app.open_strum();
            return None;
        }
        NCmd::Arpeggiate => {
            app.open_arp();
            return None;
        }
        NCmd::Dialog(tool) => {
            app.open_piano_tool(tool);
            return None;
        }
        NCmd::Ghosts => {
            s.piano.ghosts = !s.piano.ghosts;
            format!("Ghost channels {}", if s.piano.ghosts { "on" } else { "off" })
        }
        NCmd::AddMarker => {
            let tick = if app.transport.playing() && !app.transport.song() { app.transport.tick() as u32 } else { app.state.piano.marker };
            return Some(add_marker(&mut app.state, tick));
        }
        NCmd::Snap(p) => return Some(crate::piano_snap::set(s, p)),
        NCmd::StockLength(sn) => return Some(crate::piano_snap::set_length(s, sn)),
        NCmd::Target(row) => {
            if row >= s.project.channels.len() {
                return None;
            }
            let m = target(s, row);
            app.invalidate();
            return Some(m);
        }
    };
    s.piano.dirty = true;
    Some(msg)
}

/// Add a time marker to the current pattern at `tick` snapped down to a step (§10a). Saved with the pattern
/// (flp/events/patterns.md, metadata block), kept sorted by position.
pub fn add_marker(s: &mut State, tick: u32) -> String {
    let step = u32::from(s.project.ppq.max(4)) / 4;
    let tick = tick - tick % step;
    let markers = &mut piano::pattern_mut(s).markers;
    let n = markers.iter().filter(|m| m.kind == 0).count() + 1;
    let name = format!("Marker {n}");
    let at = markers.partition_point(|m| m.position <= tick);
    markers.insert(at, os_model::TimeMarker::new(tick, &name));
    let bar = tick / (step * 16) + 1;
    format!("Piano roll: added \"{name}\" at bar {bar}")
}

/// Ctrl+B (§9): copy the selection (or every note of the channel) right after its end. With a time selection (§10) the
/// copies move by the range's length instead, so the space before and after the notes inside it is kept, and the range
/// moves on over them (as in the playlist); an empty range only moves on.
pub fn duplicate(s: &mut State) {
    let range = s.piano.range.filter(|(a, b)| b > a);
    let idx = match range {
        Some(_) => s.piano.selection(s.pattern, piano::channel(s)),
        None => piano::targets(s),
    };
    let all = piano::notes(s);
    let clip: Vec<Note> = idx.iter().filter_map(|&i| all.get(i).copied()).collect();
    let start = clip.iter().map(|n| n.position).min();
    let end = clip.iter().map(|n| n.position + piano::shown_len(s, n)).max();
    if let Some((r0, r1)) = range {
        let span = r1 - r0;
        if let Some(a) = start {
            piano::paste(s, &clip, a + span);
        }
        s.piano.range = Some((r0 + span, r1 + span));
        s.piano.dirty = true;
    } else if let (Some(a), Some(b)) = (start, end) {
        piano::paste(s, &clip, b.max(a + 1));
        s.piano.dirty = true;
    }
}

/// Replace the target notes (the selection, or every note of the channel) with `f(state, targets)`, as one undo
/// step; a selection then holds the resulting notes. Returns false when nothing changed.
fn apply(s: &mut State, f: impl FnOnce(&State, Vec<Note>) -> Vec<Note>) -> bool {
    apply_as(s, f, Some("Piano roll edit"))
}

/// `apply`, its undo step named `step`; none for a tool's preview (spec: ui/piano-strum.md §1).
pub(crate) fn apply_as(s: &mut State, f: impl FnOnce(&State, Vec<Note>) -> Vec<Note>, step: Option<&'static str>) -> bool {
    let Some(ch) = piano::channel(s) else { return false };
    let idx = piano::targets(s);
    if idx.is_empty() {
        return false;
    }
    let had_sel = !s.piano.selection(s.pattern, Some(ch)).is_empty();
    let all = piano::notes(s);
    let picked: Vec<Note> = idx.iter().filter_map(|&i| all.get(i).copied()).collect();
    let out = f(s, picked.clone());
    if out == picked {
        return false;
    }
    if let Some(name) = step {
        piano::snapshot_as(s, name);
    }
    let p = piano::pattern_mut(s);
    let mut notes: Vec<Note> = p.notes.iter().enumerate().filter(|(i, _)| !idx.contains(i)).map(|(_, n)| *n).collect();
    let start = notes.len();
    notes.extend(out);
    let end = notes.len();
    p.notes = notes;
    if had_sel {
        s.piano.select(s.pattern, ch, (start..end).collect());
    } else {
        s.piano.clear_selection();
    }
    piano::finish_edit(s);
    s.piano.dirty = true;
    true
}

fn len_of(n: &Note, step: u32) -> u32 {
    if n.length == 0 { step } else { n.length }
}

/// Move the start `k` (0..=1) of the way to the nearest `step` line; unless `starts_only`, round the length to
/// whole steps the same way (at least one step).
pub fn quantize(mut n: Note, step: u32, k: f32, starts_only: bool) -> Note {
    let st = step as f32;
    let pos = n.position as f32;
    n.position = (pos + ((pos / st).round() * st - pos) * k).round().max(0.0) as u32;
    if !starts_only && n.length > 0 {
        let len = n.length as f32;
        let q = ((len / st).round() * st).max(st);
        n.length = (len + (q - len) * k).round().max(1.0) as u32;
    }
    n
}

/// Each note lasts until the next later start among `notes`, the last ones until `end` (the pattern's end); slide notes
/// keep their length and are no next start (spec: ui/piano-keys.md §4 rule 5).
pub fn legato(notes: Vec<Note>, end: u32) -> Vec<Note> {
    let mut starts: Vec<u32> = notes.iter().filter(|n| !n.slide()).map(|n| n.position).collect();
    starts.sort_unstable();
    starts.dedup();
    notes
        .into_iter()
        .map(|mut n| {
            let next = starts.iter().copied().find(|&t| t > n.position).unwrap_or(end);
            if !n.slide() && next > n.position {
                n.length = next - n.position;
            }
            n
        })
        .collect()
}

/// Cut every note into `size`-tick pieces counted from its own start, the last piece keeping what is left (spec:
/// ui/piano-keys.md §4 rule 6).
pub fn chop_from_start(notes: Vec<Note>, size: u32) -> Vec<Note> {
    let size = size.max(1);
    let mut out = Vec::new();
    for n in notes {
        let end = n.position + n.length;
        let mut at = n.position;
        while at + size < end {
            out.push(Note { position: at, length: size, ..n });
            at += size;
        }
        out.push(Note { position: at, length: end.max(at) - at, ..n });
    }
    out
}

/// Join notes of the same key that touch or overlap into one (the first note's properties).
pub fn glue(mut notes: Vec<Note>) -> Vec<Note> {
    notes.sort_by_key(|n| (n.key, n.position));
    let mut out: Vec<Note> = Vec::new();
    for n in notes {
        if let Some(last) = out.last_mut().filter(|l| l.key == n.key && l.length > 0 && n.position <= l.position + l.length) {
            let end = (last.position + last.length).max(n.position + n.length);
            last.length = end - last.position;
            continue;
        }
        out.push(n);
    }
    out
}

/// splitmix64: a small, good-enough generator for humanizing.
pub(crate) fn next(rng: &mut u64) -> u64 {
    *rng = rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *rng;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(pos: u32, len: u32, key: u16) -> Note {
        Note { length: len, key, ..Note::step(0, pos) }
    }

    fn spans(ns: &[Note]) -> Vec<(u32, u32, u16)> {
        ns.iter().map(|n| (n.position, n.length, n.key)).collect()
    }

    #[test] fn batch2_channel_controls_and_transfer_preserve_notes_and_undo(){
        let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.selected_channel=0;
        let mut n=note(37,93,64);n.velocity=81;n.pan=19;n.mod_x=55;n.fine_pitch=123;
        piano::pattern_mut(&mut a.state).notes=vec![n,note(200,24,67)];
        a.state.piano.select(a.state.pattern,0,vec![0]);
        let before=piano::notes(&a.state).to_vec();transfer(&mut a.state,1,true);
        assert!(piano::notes(&a.state).contains(&n));assert!(piano::notes(&a.state).contains(&Note{channel:1,..n}));
        crate::history::undo(&mut a.state);assert_eq!(piano::notes(&a.state),before);
        a.state.piano.select(a.state.pattern,0,vec![0]);transfer(&mut a.state,1,false);
        assert!(!piano::notes(&a.state).contains(&n));assert!(piano::notes(&a.state).contains(&Note{channel:1,..n}));
        crate::history::undo(&mut a.state);assert_eq!(piano::notes(&a.state),before);
        run(&mut a,NCmd::ChannelMute);assert!(!a.state.project.channels[0].enabled);assert!(a.state.piano.dirty);
        run(&mut a,NCmd::ChannelMute);assert!(a.state.project.channels[0].enabled);
        run(&mut a,NCmd::ChannelSolo);assert!(a.state.project.channels[0].enabled);assert!(a.state.project.channels[1..].iter().all(|c|!c.enabled));
        assert_eq!(piano::notes(&a.state),before);
    }
    #[test]
    fn quantize_moves_to_the_grid() {
        let n = quantize(note(30, 40, 60), 24, 1.0, false);
        assert_eq!((n.position, n.length), (24, 48));
        let n = quantize(note(30, 40, 60), 24, 1.0, true);
        assert_eq!((n.position, n.length), (24, 40), "start times only");
        let n = quantize(note(40, 10, 60), 24, 0.5, false);
        assert_eq!((n.position, n.length), (44, 17), "half way: 40 -> 48, 10 -> 24");
    }

    #[test]
    fn legato_chop_glue() {
        let l = legato(vec![note(0, 10, 60), note(0, 10, 64), note(48, 10, 62), note(96, 5, 60)], 384);
        assert_eq!(spans(&l), vec![(0, 48, 60), (0, 48, 64), (48, 48, 62), (96, 288, 60)], "the last note: to the pattern's end");
        // Ctrl+U counts from each note's start (FL 26: F5 581+30 -> 581+24, 605+6; D6 520+72 -> three of 24).
        let c = chop_from_start(vec![note(581, 30, 65), note(520, 72, 74), note(0, 24, 62), note(48, 0, 60)], 24);
        assert_eq!(spans(&c), vec![(581, 24, 65), (605, 6, 65), (520, 24, 74), (544, 24, 74), (568, 24, 74), (0, 24, 62), (48, 0, 60)]);
        let g = glue(vec![note(24, 24, 60), note(0, 24, 60), note(60, 10, 60), note(48, 10, 62)]);
        assert_eq!(spans(&g), vec![(0, 48, 60), (60, 10, 60), (48, 10, 62)]);
    }

    /// Ctrl+L as FL 26 does it (spec: ui/piano-keys.md §2 "Editing"): slide notes stay and are no next start; the
    /// last notes last until the pattern's end.
    #[test]
    fn legato_skips_slides_and_ends_at_the_pattern_end() {
        let mut slide = note(96, 24, 64);
        slide.set_slide(true);
        let l = legato(vec![note(0, 96, 60), slide, note(384, 48, 72), note(800, 48, 60)], 1152);
        assert_eq!(spans(&l), vec![(0, 384, 60), (96, 24, 64), (384, 416, 72), (800, 352, 60)]);
    }
    #[test]
    fn piano_channel_event_clipboard_isolated_undo_and_ppq() {
        use winit::keyboard::{Key,NamedKey};
        use os_model::ControlPoint as C;
        let mut a=App::new(None);a.state.project=crate::app::test_project();
        a.state.z=vec![crate::app::Win::PianoRoll];
        a.state.piano.event_focus=true;a.state.piano.lane=piano::Lane::ChanVolume;
        let notes=vec![Note::step(0,0),Note::step(1,0)];
        piano::pattern_mut(&mut a.state).notes=notes.clone();
        let original=vec![C::new(12,0,C::VOLUME,1000),C::new(36,0,C::VOLUME,8000),C::new(36,0,C::PAN,7000),C::new(12,1,C::VOLUME,9000)];
        piano::pattern_mut(&mut a.state).controls=original.clone();
        a.state.piano.range=Some((0,24));
        a.key(&Key::Character("c".into()),true,false);
        assert_eq!(a.state.piano.event_clipboard,vec![original[0]]);
        a.state.piano.marker=96;a.state.piano.marker_placed=true;
        a.key(&Key::Character("v".into()),true,false);
        assert!(piano::controls(&a.state).contains(&C::new(96,0,C::VOLUME,1000)));
        assert_eq!(piano::notes(&a.state),notes);
        crate::history::undo(&mut a.state);assert_eq!(piano::controls(&a.state),original);
        a.state.piano.range=Some((0,24));
        a.key(&Key::Character("x".into()),true,false);
        assert_eq!(piano::controls(&a.state).len(),3);
        crate::history::undo(&mut a.state);assert_eq!(piano::controls(&a.state),original);
        a.key(&Key::Character("a".into()),true,false);
        a.key(&Key::Named(NamedKey::Delete),false,false);
        assert_eq!(piano::controls(&a.state),&original[2..]);
        assert_eq!(piano::notes(&a.state),notes);
        crate::history::undo(&mut a.state);
        piano::rescale(&mut a.state,96,192);
        assert_eq!(a.state.piano.event_clipboard[0].position,24);
        // A note-grid focus still copies all note properties unchanged.
        a.state.piano.event_focus=false;
        let p=piano::pattern_mut(&mut a.state);p.notes[0].velocity=23;p.notes[0].pan=99;p.notes[0].fine_pitch=88;p.notes[0].mod_x=17;
        a.state.piano.select(1,0,vec![0]);
        a.key(&Key::Character("c".into()),true,false);
        assert_eq!(a.state.piano.clipboard,vec![piano::notes(&a.state)[0]]);
    }

}

/// Transfer only explicitly selected notes in the same pattern, preserving their complete records.
fn transfer(s:&mut State,row:usize,copy:bool)->String {
    let Some(source)=piano::channel(s) else{return "No source channel".into()};
    let Some(dest)=s.project.channels.get(row).filter(|c|c.index!=source && !matches!(c.kind,os_model::ChannelKind::Automation)).map(|c|c.index) else{return "Choose another instrument channel".into()};
    let selected=s.piano.selection(s.pattern,Some(source)).clone();
    let notes=piano::notes(s);
    let indices:Vec<usize>=selected.into_iter().filter(|&i|notes.get(i).is_some_and(|n|n.channel==source)).collect();
    if indices.is_empty(){return "Select notes to send first".into();}
    let picked:Vec<Note>=indices.iter().map(|&i|Note{channel:dest,..notes[i]}).collect();
    let count=picked.len();
    piano::snapshot_as(s,if copy{"Copy notes to channel"}else{"Move notes to channel"});
    let pattern=piano::pattern_mut(s);
    if !copy {pattern.notes=pattern.notes.iter().enumerate().filter(|(i,_)|!indices.contains(i)).map(|(_,n)|*n).collect();}
    pattern.notes.extend(picked);
    s.piano.clear_selection();piano::finish_edit(s);s.piano.dirty=true;
    format!("{} {count} notes to {}",if copy{"Copied"}else{"Moved"},channel_name(s,row))
}
