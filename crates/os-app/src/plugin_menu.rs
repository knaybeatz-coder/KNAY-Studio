// Copyright (C) 2026 ItsAlphaNeon
// Copyright (C) 2026 thacer13
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Plugin menus and destinations for the visual picker, channel rack's "+",
//! Add one ▸ / Insert ▸ and the mixer slots open the app's own popup listing the installed plugins, and a pick adds
//! the plugin where that menu says.
// spec: ui/plugin-menu.md

use std::collections::BTreeMap;

use crate::app::{App, State};
use crate::playlist::{Cmd, Mark, PItem, Popup};
use crate::plugin_db::{self as db, Entry, MenuView, Mode};

/// What a plugin menu entry does (`Cmd::Plugin`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlugCmd {
    /// Generator `index` (into `Plugins::available`): a new channel at the bottom of the rack, or right below row
    /// `after`.
    Generator { index: usize, after: Option<usize> },
    /// Generator `index` in place of rack row `row`'s (channel menu / plugin window menu > Replace ▸).
    Replace { index: usize, row: usize },
    Stack { index:usize,row:usize },
    ClipFx{channel:u16,slot:u8,index:usize},
    StackEdit {row:usize,child:usize,action:u8},
    /// Effect `index` (into `Plugins::effects`) into the selected mixer track: slot `slot`, else its first empty slot.
    Effect { index: usize, slot: Option<u8> },
    /// Find new plugins (a background scan).
    Find,
    Picker(Target),
    /// Settings > Plugins.
    Manage,
    OpenFolder(bool),
    View(MenuView),
}

/// Which plugins a menu lists and where a pick goes (spec §1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// F8, the toolbar button, VIEW / ADD > Plugin picker: generators and effects.
    All,
    /// Generators: a new channel at the bottom (`None`) or right below row `r`.
    Gen(Option<usize>),
    /// Generators in place of rack row `r`'s generator.
    Replace(usize),
    Stack(usize),
    ClipFx(u16,u8),
    /// Effects into the selected track: slot `k`, else its first empty slot.
    Fx(Option<u8>),
}

fn pick(target: Target, effect: bool, index: usize) -> Cmd {
    Cmd::Plugin(match (target, effect) {
        (Target::ClipFx(channel,slot),_)=>PlugCmd::ClipFx{channel,slot,index},
        (Target::Fx(slot), _) => PlugCmd::Effect { index, slot },
        (_, true) => PlugCmd::Effect { index, slot: None },
        (Target::Gen(after), false) => PlugCmd::Generator { index, after },
        (Target::Stack(row),false)=>PlugCmd::Stack{index,row},
        (Target::Replace(row), false) => PlugCmd::Replace { index, row },
        (Target::All, false) => PlugCmd::Generator { index, after: None },
    })
}

fn item(e: &Entry, target: Target, effect: bool) -> PItem {
    PItem::new(e.label.clone(), pick(target, effect, e.index))
}

/// A header with a run-time label.
fn head(label: String) -> PItem {
    PItem { head: true, ..PItem::new(label, Cmd::Nothing) }
}

#[derive(Default)]
struct Folder { items: Vec<PItem>, children: BTreeMap<String, Folder> }
impl Folder {
    fn rows(self) -> Vec<PItem> {
        let mut rows: Vec<_> = self.children.into_iter().map(|(name, folder)| PItem::sub(name, folder.rows())).collect();
        rows.extend(self.items);
        rows
    }
}

/// Optional installer-local one-time import. Never included in public packages.
fn apply_import(profile: &std::path::Path, bytes: &[u8]) -> std::io::Result<bool> {
    use sha2::{Digest,Sha256};
    use std::path::Component;
    let fingerprint=format!("{:x}",Sha256::digest(bytes));
    let marker=profile.join("plugin-database-imported.txt");
    if std::fs::read_to_string(&marker).ok().as_deref()==Some(&fingerprint) {return Ok(false);}
    let files:BTreeMap<String,String>=serde_json::from_slice(bytes).map_err(std::io::Error::other)?;
    for name in files.keys() {
        let path=std::path::Path::new(name);
        if !path.components().all(|c|matches!(c,Component::Normal(_))) ||
            !matches!(path.components().next().and_then(|c|c.as_os_str().to_str()),Some("Effects"|"Instruments")) {
            return Err(std::io::Error::other("Invalid plugin database import path"));
        }
    }
    std::fs::create_dir_all(profile)?;
    let stage=profile.join(format!("Plugin database import {}",&fingerprint[..12]));
    std::fs::create_dir_all(&stage)?;
    for (name,text) in files {
        let dest=stage.join(name);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(dest,text)?;
    }
    let live=profile.join("Plugin database");
    let backup=profile.join(format!("Plugin database before import {}",&fingerprint[..12]));
    if live.exists() {std::fs::rename(&live,&backup)?;}
    if let Err(e)=std::fs::rename(&stage,&live) {
        if backup.exists() {let _=std::fs::rename(&backup,&live);}
        return Err(e);
    }
    std::fs::write(marker,fingerprint)?;
    Ok(true)
}

pub(crate) fn import_installed_database() {
    if cfg!(test) {return;}
    let Some(profile)=os_paths::user_dir() else {return};
    let Some(exe)=std::env::current_exe().ok() else {return};
    let Some(parent)=exe.parent() else {return};
    if let Ok(bytes)=std::fs::read(parent.join("Personal plugin database import.json")) {
        match apply_import(&profile,&bytes) {
            Ok(true)=>{crate::prefs::set_text("browser_option_plugins_tab","0");},
            Ok(false)=>{},
            Err(e)=>{let _=std::fs::write(profile.join("plugin-database-import-error.txt"),e.to_string());},
        }
    }
}

/// Each profile owns its editable database; no dependency on another DAW's folders.
fn user_folder(effect: bool) -> Option<std::path::PathBuf> {
    os_paths::user_file("Plugin database").map(|p|p.join(if effect {"Effects"} else {"Instruments"}))
}

fn personal_groups(effect: bool) -> Option<BTreeMap<String, Vec<String>>> {
    if cfg!(test) { return None; }
    folder_groups(&user_folder(effect)?).ok()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PluginLink { uid: String, name: String }

/// Only add new identities. Moving, copying or removing a link survives future scans.
fn sync_folder(root: &std::path::Path, list: &[Entry], effect: bool) -> std::io::Result<()> {
    use std::collections::BTreeSet;
    std::fs::create_dir_all(root)?;
    let ledger = root.join(".known-plugins.json");
    let mut known: BTreeSet<String> = match std::fs::read(&ledger) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => BTreeSet::new(),
        Err(e) => return Err(e),
    };
    for names in folder_groups(root)?.values() {
        for name in names {
            if let Some(uid)=name.strip_prefix('@') { known.insert(uid.to_owned()); }
            else { for entry in list.iter().filter(|e|normalized(&e.name)==normalized(name)) {known.insert(entry.uid.clone());} }
        }
    }
    let defaults: BTreeMap<String,Vec<String>> = serde_json::from_str(if effect {
        include_str!("../assets/native-effects-groups.json")
    } else {include_str!("../assets/native-generators-groups.json")}).unwrap_or_default();
    for entry in list {
        if known.contains(&entry.uid) {continue;}
        let group = defaults.iter().find(|(_,names)|names.contains(&entry.name)).map(|(g,_)|g.as_str()).unwrap_or("Unsorted");
        let dir=root.join(group); std::fs::create_dir_all(&dir)?;
        let label: String=entry.label.chars().map(|c|if c.is_control()||"<>:\"/\\|?*".contains(c){'_'}else{c}).take(90).collect();
        let id=entry.uid.bytes().fold(14695981039346656037u64,|h,b|(h^u64::from(b)).wrapping_mul(1099511628211));
        let path=dir.join(format!("{} - {id:016x}.plugin.json",label.trim_end_matches(['.',' '])));
        let link=PluginLink{uid:entry.uid.clone(),name:entry.name.clone()};
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
        std::io::Write::write_all(&mut file,&serde_json::to_vec_pretty(&link).map_err(std::io::Error::other)?)?;
        known.insert(entry.uid.clone());
    }
    std::fs::write(ledger,serde_json::to_vec_pretty(&known).map_err(std::io::Error::other)?)?;
    let readme=root.join("How to organize plugins.txt");
    if !readme.exists() {
        std::fs::write(readme,"Create folders and move or copy the .plugin.json links into them. Reopen the plugin menu to see changes in Categories view. The files are references, not plugin binaries. Scan results add new plugins under Unsorted without moving your existing links. Unlisted installed plugins remain reachable under Other installed plugins. Keep the .known-plugins.json file so removed links are not recreated.\n")?;
    }
    Ok(())
}

pub(crate) fn sync_user_folders(s: &State) {
    if cfg!(test) {return;}
    for (effect,list) in [(true,&s.plugdb.lists.fx),(false,&s.plugdb.lists.gens)] {
        if let Some(root)=user_folder(effect) {
            if let Err(e)=sync_folder(&root,list,effect) {eprintln!("Plugin folders: {e}");}
        }
    }
}

fn open_folder(effect: bool) -> Option<String> {
    let root=user_folder(effect)?;
    #[cfg(windows)] let program="explorer.exe";
    #[cfg(target_os="macos")] let program="open";
    #[cfg(not(any(windows,target_os="macos")))] let program="xdg-open";
    std::process::Command::new(program).arg(root).spawn().err().map(|e|format!("Could not open plugin folder: {e}"))
}

fn folder_groups(root: &std::path::Path) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    fn visit(dir: &std::path::Path, parts: &mut Vec<String>, groups: &mut BTreeMap<String, Vec<String>>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                parts.push(entry.file_name().to_string_lossy().into_owned());
                visit(&entry.path(), parts, groups)?;
                parts.pop();
            } else if kind.is_file() && entry.file_name().to_string_lossy().ends_with(".plugin.json") {
                if let Ok(bytes)=std::fs::read(entry.path()) {
                    if let Ok(link)=serde_json::from_slice::<PluginLink>(&bytes) {
                        groups.entry(parts.join(" / ")).or_default().push(if link.uid.is_empty() {link.name} else {format!("@{}",link.uid)});
                    }
                }
            } else if kind.is_file() && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("fst")) {
                if let Some(name) = entry.path().file_stem() {
                    groups.entry(parts.join(" / ")).or_default().push(name.to_string_lossy().into_owned());
                }
            }
        }
        Ok(())
    }
    let mut groups = BTreeMap::new();
    visit(root, &mut Vec::new(), &mut groups)?;
    for names in groups.values_mut() { names.sort(); names.dedup(); }
    Ok(groups)
}

/// One section's entries (spec §2): favourites, a separator, then the rest by display mode. `nested`: the list is a
/// submenu itself, so "By vendor" uses headers instead of submenus (popups open one submenu level).
fn section(s: &State, effect: bool, target: Target, nested: bool) -> Vec<PItem> {
    let list = if effect { &s.plugdb.lists.fx } else { &s.plugdb.lists.gens };
    if list.is_empty() {
        return vec![PItem::new(if effect { "(no effect plugins found)" } else { "(no instruments found)" }, Cmd::Nothing).off()];
    }
    let mut v: Vec<PItem> = list.iter().filter(|e| e.favorite).map(|e| item(e, target, effect)).collect();
    let rest: Vec<&Entry> = list.iter().filter(|e| !e.favorite).collect();
    if !v.is_empty() && !rest.is_empty() {
        v.push(PItem::sep());
    }
    let personal = personal_groups(effect);
    if s.plugdb.prefs.menu_view == MenuView::Categories {
        let defaults = if effect { include_str!("../assets/native-effects-groups.json") } else { include_str!("../assets/native-generators-groups.json") };
        let groups: BTreeMap<String, Vec<String>> = personal.unwrap_or_else(|| crate::prefs::text(if effect { "fl_effect_groups" } else { "fl_generator_groups" })
            .and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_else(|| serde_json::from_str(defaults).unwrap_or_default()));
        if !groups.is_empty() {
            v.clear();
            let mut used = std::collections::HashSet::new();
            let mut folders = Folder::default();
            for (group, names) in groups {
                let es: Vec<_> = list.iter().filter(|e| names.iter().any(|n| n.strip_prefix('@').map_or_else(||normalized(n)==normalized(&e.name),|uid|uid==e.uid))).collect();
                let unavailable: Vec<_> = names.iter().filter(|n|!n.starts_with('@') && !es.iter().any(|e|normalized(n)==normalized(&e.name))).collect();
                if es.is_empty() && unavailable.is_empty() { continue; }
                let mut folder = &mut folders;
                for part in group.split(" / ").filter(|p| !p.is_empty()) { folder = folder.children.entry(part.to_owned()).or_default(); }
                for e in es { used.insert(e.uid.clone()); folder.items.push(item(e, target, effect)); }
                for name in unavailable {folder.items.push(PItem::new(format!("{name} (not available)"),Cmd::Nothing).off());}
            }
            let other: Vec<_> = list.iter().filter(|e| !used.contains(&e.uid)).map(|e| item(e, target, effect)).collect();
            if !other.is_empty() { folders.children.entry("Other installed plugins".into()).or_default().items = other; }
            v.extend(folders.rows());
            return v;
        }
    }
    match s.plugdb.prefs.menu_view {
        MenuView::Simple => v.extend(rest.iter().map(|e| item(e, target, effect))),
        MenuView::Categories => {
            for cat in db::categories(effect) {
                let in_cat: Vec<&&Entry> = rest.iter().filter(|e| e.category == cat).collect();
                if !in_cat.is_empty() {
                    v.push(PItem::head(cat));
                    v.extend(in_cat.iter().map(|e| item(e, target, effect)));
                }
            }
        }
        MenuView::Vendor => {
            let mut by: BTreeMap<String, (String, Vec<&Entry>)> = BTreeMap::new();
            for e in &rest {
                by.entry(e.vendor.to_lowercase()).or_insert_with(|| (e.vendor.clone(), Vec::new())).1.push(e);
            }
            for (_, (vendor, es)) in by {
                let items: Vec<PItem> = es.iter().map(|e| item(e, target, effect)).collect();
                if nested {
                    v.push(head(vendor));
                    v.extend(items);
                } else {
                    v.push(PItem { children: items, ..PItem::new(vendor, Cmd::Nothing) });
                }
            }
        }
    }
    v
}

/// The display modes as their radio items name them (spec §2-3); ADD's radios are the same.
pub const VIEWS: &[(&str, MenuView)] = &[("Categories", MenuView::Categories), ("Simple", MenuView::Simple), ("By vendor", MenuView::Vendor)];

/// After the lists (spec §2): Find new plugins, Manage plugins..., the display modes.
fn footer(s: &State) -> Vec<PItem> {
    let view = s.plugdb.prefs.menu_view;
    let scanning = s.plugdb.progress.is_some();
    let find = PItem::new(if scanning { "Find new plugins (scanning...)" } else { "Find new plugins" }, Cmd::Plugin(PlugCmd::Find));
    let mut v = vec![PItem::sep(), if scanning { find.off() } else { find }, PItem::new("Manage plugins...", Cmd::Plugin(PlugCmd::Manage)), PItem::sep()];
    v.push(PItem::sub("Organize plugin folders",vec![
        PItem::new("Effects",Cmd::Plugin(PlugCmd::OpenFolder(true))),
        PItem::new("Instruments",Cmd::Plugin(PlugCmd::OpenFolder(false))),
    ]));
    v.extend(VIEWS.iter().map(|&(label, m)| PItem::new(label, Cmd::Plugin(PlugCmd::View(m))).mark(Mark::Radio(view == m))));
    v
}

/// The entries of a plugin menu for `target`; `nested` when they fill a submenu.
pub fn items(s: &State, target: Target, nested: bool) -> Vec<PItem> {
    let mut v = vec![PItem::new("Open plugin picker", Cmd::Plugin(PlugCmd::Picker(target))), PItem::sep()];
    match target {
        Target::All => {
            v.push(PItem::head("Generators"));
            v.extend(section(s, false, target, nested));
            v.push(PItem::col());
            v.push(PItem::head("Effects"));
            v.extend(section(s, true, target, nested));
        }
        Target::Gen(_) | Target::Replace(_) | Target::Stack(_) => v.extend(section(s, false, target, nested)),
        Target::Fx(_) | Target::ClipFx(..) => v.extend(section(s, true, target, nested)),
    }
    v.extend(footer(s));
    if !cfg!(test) {
        if let Some(file)=os_paths::user_file("plugin-menu-report.txt") {
            let _=std::fs::write(file,format!("profile={:?}\nview={:?}\nmenu={:?}\n",os_paths::user_dir(),s.plugdb.prefs.menu_view,v.iter().map(|r|r.label.to_string()).collect::<Vec<_>>()));
        }
    }
    v
}

/// A plugin menu for `target` at (x, y).
pub fn menu(s: &State, target: Target, x: f32, y: f32) -> Popup {
    Popup { x, y, items: items(s, target, false), sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// Whether the open popup is a plugin menu (F8 closes it).
pub fn is_open(s: &State) -> bool {
    s.plugin_picker.open || s.playlist.popup.as_ref().is_some_and(|p| p.items.iter().any(|i| i.cmd == Cmd::Plugin(PlugCmd::Manage)))
}

impl App {
    /// Opens the plugin menu for `target` at (x, y); F8 while one is open closes it.
    pub fn open_plugin_menu(&mut self, target: Target, at: (f32, f32)) {
        if target == Target::All { crate::plugin_picker::open(self,target); return; }
        self.state.menu_open = None;
        self.state.playlist.popup = Some(menu(&self.state, target, at.0, at.1));
        self.invalidate();
    }

    /// F8 (spec §1): the all-plugins menu at the mouse, or closes an open plugin menu.
    pub fn toggle_plugin_menu(&mut self) {
        if is_open(&self.state) {
            crate::plugin_picker::close(self);
            self.state.playlist.popup = None;
            self.invalidate();
        } else {
            let target=match self.top_hit(self.mouse.0,self.mouse.1).and_then(|h|h.win){
                Some(crate::app::Win::Mixer)=>Target::Fx(None),Some(crate::app::Win::ChannelRack)=>Target::Gen(None),_=>Target::All,
            };
            crate::plugin_picker::open(self,target);
        }
    }
}

/// Runs a plugin menu command. Returns a status message.
pub fn run(app: &mut App, c: PlugCmd) -> Option<String> {
    let m = match c {
        PlugCmd::Picker(target) => { crate::plugin_picker::open(app,target); None },
        PlugCmd::Generator { index, after } => add_generator(app, index, after),
        PlugCmd::ClipFx{channel,slot,index}=>crate::clip_effects::add(app,channel,slot,index),
        PlugCmd::Stack{index,row}=>crate::instrument_stack::add(app,row,index),
        PlugCmd::StackEdit{row,child,action}=>crate::instrument_stack::edit(app,row,child,action),
        PlugCmd::Replace { index, row } => replace_generator(app, index, row),
        PlugCmd::Effect { index, slot } => add_effect(app, index, slot),
        PlugCmd::Find => {
            app.start_scan(Mode::Changed);
            None
        }
        PlugCmd::Manage => {
            app.open_settings(crate::views::settings::PLUGINS);
            None
        }
        PlugCmd::OpenFolder(effect) => { sync_user_folders(&app.state); open_folder(effect) }
        PlugCmd::View(v) => {
            app.state.plugdb.prefs.menu_view = v;
            app.prefs_changed();
            None
        }
    };
    app.invalidate();
    m
}

/// A new channel for generator `index` (spec §1): at the bottom of the rack, or right below row `after`; selected and
/// in view. Lemon Sampler makes an empty sampler channel (hosted on the next tick); any other generator a plugin
/// channel hosting it. Its plugin window opens once it is hosted.
pub fn add_generator(app: &mut App, index: usize, after: Option<usize>) -> Option<String> {
    let d = app.plugins.available.get(index)?.desc.clone();
    let s = &mut app.state;
    let sampler = d.name == crate::plugins::LEMON_SAMPLER && crate::project_plugins::is_builtin(&d);
    // One step, at the bottom (`history::add_channel`) or inserted higher up (`history::insert_channel`).
    let plugin = (!sampler).then_some(index);
    match after.filter(|&r| r + 1 < s.project.channels.len()) {
        Some(r) => crate::history::insert_channel(s, r + 1, plugin),
        None => crate::history::add_channel(s, plugin),
    }
    let mut ch = if sampler { crate::app::add_channel(&mut s.project) } else { crate::app::add_plugin_channel(&mut s.project, &d.name) };
    if sampler {
        s.pending_open = Some(ch);
    } else {
        s.pending_plugin = Some((ch, index));
    }
    s.piano.dirty = true;
    if let Some(r) = after.filter(|&r| r + 1 < ch) {
        let order: Vec<usize> = (0..=r).chain([ch]).chain(r + 1..ch).collect();
        crate::rack::reorder(app, &order);
        ch = r + 1;
    }
    let s = &mut app.state;
    s.selected_channel = ch;
    s.rack.select_only(ch);
    crate::rack::added_instrument(s,ch);
    if after.is_none() {
        // Keep the new row in view.
        let r = crate::views::rack::rect(s);
        let view_h = crate::views::rack::panel(r).h;
        s.rack_scroll = (crate::views::rack::content_h(s) - view_h).max(0.0);
    }
    sampler.then(|| "Added a sampler channel (Lemon Sampler)".into())
}

/// Generator `index` in place of rack row `row`'s (channel menu and plugin window menu > Replace ▸; FL: "the plugin list
/// of Add one", spec ui/channel-rack.md §18). The channel keeps its row, notes and steps, mixer track, levels and
/// Misc settings; its old instance goes, the new one starts from its defaults, and the channel takes the new
/// plugin's name. Lemon Studio choice where FL was not measured: the name follows the plugin.
pub fn replace_generator(app: &mut App, index: usize, row: usize) -> Option<String> {
    let d = app.plugins.available.get(index)?.desc.clone();
    app.state.project.channels.get(row)?;
    let sampler = d.name == crate::plugins::LEMON_SAMPLER && crate::project_plugins::is_builtin(&d);
    // One step, with the old instrument's state (`history::replace_channel`).
    let old = app.plugin_copy(row);
    crate::history::replace_channel(&mut app.state, row, old, (!sampler).then_some(index));
    app.plugins.drop_key(row);
    app.state.hosted_rows.retain(|&r| r != row);
    app.transport.set_instruments(app.plugins.instruments());
    let s = &mut app.state;
    let ch = &mut s.project.channels[row];
    ch.plugin_state = None;
    if sampler {
        ch.kind = os_model::ChannelKind::Sampler;
        ch.plugin = None;
        ch.clear_sample();
        ch.name = Some(d.name.clone());
        s.pending_open = Some(row);
    } else {
        ch.kind = os_model::ChannelKind::Plugin;
        ch.plugin = Some(crate::osc3::file_name(&d.name).to_string());
        ch.name = Some(d.name.clone());
        // A Lemon Trio channel's sample is its custom shape's (spec: plugins/lemon-3x-osc.md §6): the new instance has
        // none, so neither does the channel.
        if crate::osc3::is_osc3(ch) {
            ch.sample_path = None;
        }
        s.pending_plugin = Some((row, index));
    }
    s.piano.dirty = true;
    s.selected_channel = row;
    s.rack.select_only(row);
    Some(format!("Replaced with {}", d.name))
}

/// Effect `index` into the selected mixer track (spec §1): slot `slot`, else the first empty one.
pub fn add_effect(app: &mut App, index: usize, slot: Option<u8>) -> Option<String> {
    let s = &app.state;
    let t = s.selected_mixer;
    let slot = slot.or_else(|| s.project.mixer.iter().find(|m| m.index == t).and_then(|m| m.slots.iter().enumerate().find(|(k,s)|*k!=10&&s.is_none()).map(|(k,_)|k)).map(|k| k as u8));
    match slot {
        Some(k) => crate::mixer::run(app, crate::mixer::MCmd::AddEffect(k, index)),
        None => Some("Every slot of this track is in use".into()),
    }
}

pub(crate) fn normalized(s: &str) -> String {
 s.to_lowercase().replace(" (vst3)", "").replace(" (vst2)", "").chars().filter(|c| c.is_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_import_runs_in_active_profile_once_and_preserves_backup() {
        let root=std::env::temp_dir().join(format!("lemon-active-import-{}",std::process::id()));
        std::fs::create_dir_all(root.join("Plugin database/Effects")).unwrap();
        std::fs::write(root.join("Plugin database/Effects/old.txt"),"old").unwrap();
        let files=BTreeMap::from([("Effects/00 GO TO/test.plugin.json",r#"{"uid":"test","name":"Test"}"#)]);
        let bytes=serde_json::to_vec(&files).unwrap();
        assert!(apply_import(&root,&bytes).unwrap());
        assert!(root.join("Plugin database/Effects/00 GO TO/test.plugin.json").exists());
        std::fs::write(root.join("Plugin database/Effects/user-edit.txt"),"keep").unwrap();
        assert!(!apply_import(&root,&bytes).unwrap());
        assert!(root.join("Plugin database/Effects/user-edit.txt").exists());
        assert!(std::fs::read_dir(&root).unwrap().map(Result::unwrap).any(|e|e.file_name().to_string_lossy().starts_with("Plugin database before import") && e.path().join("Effects/old.txt").exists()));
        assert!(apply_import(&root,br#"{"Effects/../../escape":"bad"}"#).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn factory_folders_are_organized_on_first_install() {
        let root=std::env::temp_dir().join(format!("lemon-factory-layout-{}",std::process::id()));
        let mut app=App::new(None);app.plugins=crate::plugins::Plugins::builtins();app.state.plugdb.rebuild(&app.plugins);
        for(effect,list)in[(false,&app.state.plugdb.lists.gens),(true,&app.state.plugdb.lists.fx)] {
            let dir=root.join(if effect{"Effects"}else{"Instruments"});sync_folder(&dir,list,effect).unwrap();sync_folder(&dir,list,effect).unwrap();let groups=folder_groups(&dir).unwrap();
            assert!(!groups.contains_key("Unsorted"),"Every factory plugin needs a folder: {groups:?}");
            for entry in list{let uid=format!("@{}",entry.uid);assert_eq!(groups.values().flatten().filter(|x|**x==uid).count(),1,"{}",entry.name);}
            if !effect {let wave=list.iter().find(|e|e.name=="Lemon Wave").unwrap();assert!(groups["Synths"].contains(&format!("@{}",wave.uid)));}
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editable_folders_survive_rescan_and_keep_plugin_identity() {
        let root=std::env::temp_dir().join(format!("lemon-editable-folders-{}",std::process::id()));
        let mut app=App::new(None);
        app.plugins.effects.push(db::fake("A Test Effect",os_host::Kind::Effect));
        app.plugins.available.push(db::fake("A Test Instrument",os_host::Kind::Instrument));
        app.state.plugdb.rebuild(&app.plugins);
        for effect in [true,false] {
            let dir=root.join(if effect {"Effects"} else {"Instruments"});
            let list=if effect {&app.state.plugdb.lists.fx} else {&app.state.plugdb.lists.gens};
            sync_folder(&dir,list,effect).unwrap();
            let source=std::fs::read_dir(dir.join("Unsorted")).unwrap().map(Result::unwrap).map(|e|e.path()).find(|p|p.extension().is_some_and(|e|e=="json")).unwrap();
            let link:PluginLink=serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
            let nested=dir.join("00 GO TO/Personal");std::fs::create_dir_all(&nested).unwrap();
            let moved=nested.join("My chosen name.plugin.json");std::fs::rename(&source,&moved).unwrap();
            sync_folder(&dir,list,effect).unwrap();
            assert!(!source.exists());
            let groups=folder_groups(&dir).unwrap();
            assert!(groups["00 GO TO / Personal"].contains(&format!("@{}",link.uid)));
            crate::prefs::set_text(if effect {"fl_effect_groups"} else {"fl_generator_groups"},&serde_json::to_string(&groups).unwrap());
            let target=if effect {Target::Fx(Some(5))} else {Target::Gen(None)};
            let menu=items(&app.state,target,false);
            let go=menu.iter().find(|r|r.label=="00 GO TO").unwrap();
            let entry=&go.children[0].children[0];
            let index=list.iter().find(|e|e.uid==link.uid).unwrap().index;
            assert_eq!(entry.cmd,pick(target,effect,index));
            std::fs::remove_file(&moved).unwrap();
            sync_folder(&dir,list,effect).unwrap();
            assert!(!source.exists() && !moved.exists());
        }
        app.plugins.effects.push(db::fake("New Scan Effect",os_host::Kind::Effect));
        app.state.plugdb.rebuild(&app.plugins);
        sync_folder(&root.join("Effects"),&app.state.plugdb.lists.fx,true).unwrap();
        let groups=folder_groups(&root.join("Effects")).unwrap();
        let new=app.state.plugdb.lists.fx.iter().find(|e|e.name=="New Scan Effect").unwrap();
        assert!(groups["Unsorted"].contains(&format!("@{}",new.uid)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_folders_drive_actual_mixer_slot_menu() {
        let root = std::env::temp_dir().join(format!("lemon-folder-menu-{}", std::process::id()));
        std::fs::create_dir_all(root.join("00 GO TO")).unwrap();
        std::fs::create_dir_all(root.join("01 VOCALS/02 Cleanup")).unwrap();
        std::fs::write(root.join("00 GO TO/CLA-2A Mono.fst"), []).unwrap();
        std::fs::write(root.join("01 VOCALS/02 Cleanup/CLA-2A Mono.FST"), []).unwrap();
        std::fs::write(root.join("Root Effect.fst"), []).unwrap();
        std::fs::create_dir_all(root.join("Unavailable")).unwrap();
        std::fs::write(root.join("Unavailable/Missing.plugin.json"),br#"{"uid":"","name":"Missing Plugin"}"#).unwrap();
        let groups = folder_groups(&root).unwrap();
        let mut app = App::new(None);
        crate::prefs::set_text("fl_effect_groups", &serde_json::to_string(&groups).unwrap());
        for name in ["CLA-2A Mono", "Root Effect"] { app.plugins.effects.push(db::fake(name, os_host::Kind::Effect)); }
        app.state.plugdb.rebuild(&app.plugins);
        let menu = crate::mixer::effect_menu(&app.state, 3, 0.0, 0.0);
        let go = menu.items.iter().find(|r|r.label=="00 GO TO").unwrap();
        assert!(matches!(go.children[0].cmd, Cmd::Plugin(PlugCmd::Effect{slot:Some(3),..})));
        let vocals = menu.items.iter().find(|r|r.label=="01 VOCALS").unwrap();
        assert_eq!(vocals.children[0].label,"02 Cleanup");
        assert_eq!(vocals.children[0].children[0].label,"CLA-2A Mono");
        assert!(menu.items.iter().any(|r|r.label=="Root Effect"));
        let missing=menu.items.iter().find(|r|r.label=="Unavailable").unwrap();
        assert_eq!(missing.children[0].label,"Missing Plugin (not available)");
        assert_eq!(missing.children[0].cmd,Cmd::Nothing);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_default_tree_contains_every_shipped_effect() {
        let mut app=App::new(None);app.plugins=crate::plugins::Plugins::builtins();app.state.plugdb.rebuild(&app.plugins);
        let rows=items(&app.state,Target::Fx(Some(2)),false);
        assert!(rows.iter().any(|r|r.label=="Reverb and Delay"));
        assert!(!rows.iter().any(|r|r.label=="Other installed plugins"));
        fn found(rows:&[PItem],name:&str)->bool {rows.iter().any(|r|r.label==name||found(&r.children,name))}
        for e in &app.plugins.effects {assert!(found(&rows,&e.desc.name),"missing {}",e.desc.name);}
    }

    #[test]
    fn nested_plugin_folders_stay_visible_and_switch_branches() {
        let mut app=App::new(None);
        crate::prefs::set_text("fl_effect_groups",r#"{"Dynamics / Compression / Waves":["CLA-2A Mono"],"Dynamics / Limiting":["CLA-2A Mono"],"Vocals":["CLA-2A Mono"]}"#);
        app.plugins.effects.push(db::fake("CLA-2A Mono",os_host::Kind::Effect));
        app.state.plugdb.rebuild(&app.plugins);
        let original=crate::mixer::effect_menu(&app.state,4,150.0,150.0);
        let parent=original.items.iter().position(|r|r.label=="Dynamics").unwrap();
        let mut canvas=os_ui::Canvas::new(1600,900,1.0).unwrap();
        fn click(app:&mut App,canvas:&mut os_ui::Canvas,action:crate::app::Action) {
            app.paint(canvas,None);
            let r=app.hits.iter().rev().find(|h|h.action==action).expect("visible clickable menu item").rect;
            app.mouse_move(r.cx(),r.cy());app.mouse_down();app.mouse_up();
        }
        app.state.playlist.popup=Some(original.clone());
        click(&mut app,&mut canvas,crate::app::Action::PlPopup(parent));
        click(&mut app,&mut canvas,crate::app::Action::PlPopupSub(0));
        click(&mut app,&mut canvas,crate::app::Action::PlPopupDeep(2,0));
        let p=app.state.playlist.popup.as_ref().unwrap();
        assert_eq!(p.items,original.items,"root never replaced by a Back page");
        assert_eq!(p.sub,Some(parent));
        assert_eq!(p.cascade,vec![(0,0),(0,0)]);
        app.paint(&mut canvas,None);
        let actions=[crate::app::Action::PlPopup(parent),crate::app::Action::PlPopupSub(0),crate::app::Action::PlPopupDeep(2,0),crate::app::Action::PlPopupDeep(3,0)];
        let rects:Vec<_>=actions.iter().map(|a|app.hits.iter().find(|h|h.action==*a).expect("all ancestor rows remain clickable").rect).collect();
        for pair in rects.windows(2) {assert!(pair[0].right()<=pair[1].x+2.0 || pair[1].right()<=pair[0].x+2.0,"menus do not cover parent rows when room is available");}
        let leaf=&app.state.playlist.popup.as_ref().unwrap().items[parent].children[0].children[0].children[0];
        assert!(matches!(leaf.cmd,Cmd::Plugin(PlugCmd::Effect{slot:Some(4),..})));
        // Change to a sibling directly in the still-visible parent submenu.
        click(&mut app,&mut canvas,crate::app::Action::PlPopupSub(1));
        assert_eq!(app.state.playlist.popup.as_ref().unwrap().cascade,vec![(1,0)]);
        click(&mut app,&mut canvas,crate::app::Action::PlPopupDeep(2,0));
        assert!(app.state.playlist.popup.is_none(),"leaf selection closes the entire cascade");
        // Reopen near the right edge: root position stays fixed as deeper menus open.
        let mut edge=original;edge.x=1480.0;app.state.playlist.popup=Some(edge);
        app.act(crate::app::Action::PlPopup(parent));app.paint(&mut canvas,None);
        let before=app.hits.iter().find(|h|h.action==crate::app::Action::PlPopup(parent)).unwrap().rect;
        app.act(crate::app::Action::PlPopupSub(0));app.paint(&mut canvas,None);
        let after=app.hits.iter().find(|h|h.action==crate::app::Action::PlPopup(parent)).unwrap().rect;
        assert_eq!(before,after);
        assert!(app.hits.iter().filter(|h|matches!(h.action,crate::app::Action::PlPopupDeep(..))).all(|h|h.rect.x>=0.0 && h.rect.right()<=1600.0));
    }

    #[test]
    fn personal_folders_preserve_slot_targets() {
        let mut app = App::new(None);
        crate::prefs::set_text("fl_effect_groups",include_str!("../assets/fl-effects-groups.json"));
        for name in ["CLA-2A Mono", "ValhallaVintageVerb", "Uncategorized Test Effect"] {
            app.plugins.effects.push(db::fake(name, os_host::Kind::Effect));
        }
        app.state.plugdb.rebuild(&app.plugins);
        let rows = items(&app.state, Target::Fx(Some(4)), false);
        let go = rows.iter().find(|r| r.label == "00 GO TO").expect("FL favorites folder");
        assert!(go.children.iter().any(|r| r.label == "CLA-2A Mono"));
        fn find<'a>(rows: &'a [PItem], name: &str) -> Option<&'a PItem> {
            rows.iter().find(|r| r.label == name).or_else(|| rows.iter().find_map(|r| find(&r.children,name)))
        }
        let space = rows.iter().find(|r| r.label == "05 SPACE").expect("FL space folder");
        assert!(find(&space.children,"ValhallaVintageVerb").is_some());
        for name in ["CLA-2A Mono", "ValhallaVintageVerb", "Uncategorized Test Effect"] {
            let entry = find(&rows, name).expect("every plugin reachable");
            let index = app.plugins.effects.iter().position(|a| a.desc.name == name).unwrap();
            assert_eq!(entry.cmd, Cmd::Plugin(PlugCmd::Effect { index, slot: Some(4) }));
        }
    }
}
