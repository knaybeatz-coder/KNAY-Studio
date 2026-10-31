// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Sample browser: Pro Samples library and persistent file/folder favorites.
// spec: ui/main-window.md §4, ui/browser.md (§4 options menu, §5 search, §6 dropped folders)

use std::path::{Path, PathBuf};
use std::rc::Rc;

use os_ui::{Color, Rect};

use crate::app::{Action, State, Ui};
use crate::icons::{self, Icon};

/// Row pitch and indent per depth level (spec: ui/browser.md §2).
pub const ROW_H: f32 = 23.0;
const INDENT: f32 = 13.0;
/// Top of the list area.
pub const LIST_Y: f32 = 142.0;
/// Footer height below the list.
pub const FOOTER_H: f32 = 28.0;

/// Category colour indices (`cat_color`); 255 = folder.
const FOLDER: u8 = 255;
/// Category index of files in dropped folders (text colour, spec §6).
const FILE: u8 = 254;
/// Height of the info panel above the footer (spec §6).
pub const INFO_H: f32 = 84.0;

/// (label, icon, category colour index). Only the roots with content: FL's other disk-folder roots (Recent files,
/// Channel presets, Packs...) are left out until Lemon Studio has content for them (owner decision); dropped folders
/// follow these. Plugin presets: the preset store, user and factory (spec: plugins/presets.md "Browser").
const ROOTS: &[(&str, Icon, u8)] = &[("Current project", Icon::Page, 0), ("Plugin database", Icon::Speaker, 1), (PLUGIN_PRESETS, Icon::FilePreset, 1)];
pub const PLUGIN_PRESETS: &str = "Plugin presets";

/// Tree key of the All tab's Plugin presets root; a plugin's node is `<this>/<plugin folder name>`.
pub fn presets_key() -> String {
    format!("4:{PLUGIN_PRESETS}")
}

/// The "Current project" tree's sections, in screenshot order (spec §2), without the one nothing is tracked in yet
/// (Remote control). History: the undo history (spec: ui/settings-choices.md "Undo history: the four checks
/// (2026-10-16)"). Initialized controls: the project's initial values, shown only while there are some (spec:
/// ui/settings-choices.md "Advanced: Initialize controls automatically"). Project data: the project's data folder, opened
/// like a folder, shown only while the project has one (spec: ui/data-folder.md §3).
const PROJECT: &[(&str, Icon)] = &[
    ("History", Icon::Undo),
    ("Patterns", Icon::Note),
    ("Effects", Icon::Speaker),
    ("Generators", Icon::Plug),
    (INITIALIZED_CONTROLS, Icon::Slide),
    (PROJECT_DATA, Icon::Folder),
    ("Samples", Icon::Wave),
];
pub const INITIALIZED_CONTROLS: &str = "Initialized controls";
pub const PROJECT_DATA: &str = "Project data";

/// The Current project and Plugin database tabs (spec §1).
pub const PROJECT_TAB: usize = 1;
pub const PLUGIN_TAB: usize = 2;

/// Tree key of the Current project tab's root.
pub fn project_root() -> String {
    format!("{PROJECT_TAB}:{}", ROOTS[0].0)
}

/// The Current project tree's sections, in order.
pub fn sections() -> impl Iterator<Item = &'static str> {
    PROJECT.iter().map(|&(l, _)| l)
}

/// Tree key of section `name` of the Current project tab; `None` for a section the tree does not list.
pub fn section_key(name: &str) -> Option<String> {
    sections().any(|l| l == name).then(|| format!("{}/{name}", project_root()))
}

/// What a row stands for; everything but `None`, `File` and `History` can be dragged (spec §3).
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    None,
    /// 1-based pattern number.
    Pattern(u16),
    /// Rack row.
    Channel(usize),
    /// Sample file path.
    Sample(String),
    /// A generator (index into `Plugins::available`).
    Generator(usize),
    /// An effect (index into `Plugins::effects`).
    Effect(usize),
    /// A non-audio file in a dropped folder (not draggable, spec §6).
    File(String),
    /// A parameter of a hosted plugin: hosted key, parameter index (spec: plugins/plugin-parameters.md §6).
    Param(usize, usize),
    /// An entry of the undo history, 0 = "Last reset" (`History::entries`): a click goes back or forward to it; not
    /// draggable.
    History(usize),
    /// A plugin preset file (any `presets::IMPORT_EXTS` type) or factory preset path: dropped, it loads (spec:
    /// plugins/presets.md "Drag and drop").
    Preset(String),
}

#[derive(Debug, Clone)]
pub struct Row {
    /// Open/closed state key (tab-qualified path).
    pub key: String,
    pub depth: u8,
    pub label: String,
    pub icon: Icon,
    pub cat: u8,
    /// `Some(open)` for an expandable row.
    pub open: Option<bool>,
    /// Drawn brighter (current pattern, opened roots).
    pub bright: bool,
    pub item: Item,
    /// Disk entry (dropped folders, spec §6): shown in the info panel.
    pub path: Option<PathBuf>,
}

/// Keys of the rows that start opened (the project tab shows Patterns, like the screenshot).
pub fn default_open() -> std::collections::HashSet<String> {
    ["1:Current project", "1:Current project/Patterns", "2:Plugin database"].iter().map(|k| k.to_string()).collect()
}

/// Options-menu toggles (spec §4); `DEFAULT_OPTS` are on in a fresh browser.
pub const OPT_PARENTS: &str = "parents";
pub const OPT_EXPAND: &str = "expand";
pub const OPT_FOLDERS: &str = "folders";
pub const OPT_TOP: &str = "top";
pub const OPT_TYPE: &str = "type";
pub const OPT_TAGS: &str = "tags";
pub const OPT_INDEX: &str = "index";
pub const OPT_REINDEX: &str = "reindex";
pub const OPT_AUTOHIDE: &str = "autohide";
pub const OPT_HIDEDRAG: &str = "hidedrag";
pub const OPT_CONTRAST: &str = "contrast";
pub const OPT_PREVIEW: &str = "preview";
pub const OPT_PROJECT: &str = "project_tab";
pub const OPT_PLUGINS: &str = "plugins_tab";
pub const OPT_PRESETS: &str = "presets_tab";
pub const DEFAULT_OPTS: &[&str] = &[OPT_FOLDERS, OPT_TAGS, OPT_INDEX, OPT_REINDEX, OPT_HIDEDRAG];
pub fn saved_opts()->std::collections::HashSet<&'static str> {
    [OPT_PARENTS,OPT_EXPAND,OPT_FOLDERS,OPT_TOP,OPT_TYPE,OPT_TAGS,OPT_INDEX,OPT_REINDEX,OPT_AUTOHIDE,OPT_HIDEDRAG,OPT_CONTRAST,OPT_PREVIEW,OPT_PROJECT,OPT_PLUGINS,OPT_PRESETS].into_iter().filter(|k| {
        crate::prefs::text(&format!("browser_option_{k}")).map_or(DEFAULT_OPTS.contains(k),|v|v=="1")
    }).collect()
}
pub fn tabs(s:&State)->Vec<(usize,Icon,&'static str)> {
    let mut tabs=vec![(0,Icon::Wave,"Samples"),(3,Icon::Star,"Favorites")];
    for (key,id,icon,label) in [(OPT_PROJECT,1,Icon::Page,"Current project"),(OPT_PLUGINS,2,Icon::Plug,"Plugin database"),(OPT_PRESETS,4,Icon::FilePreset,"Plugin presets")] {
        if opt(s,key) || s.browser_tab==id {tabs.push((id,icon,label));}
    }
    tabs
}

pub fn opt(s: &State, k: &str) -> bool {
    s.browser_opts.contains(k)
}

/// Whether `row` matches the search text `q` (lower case), spec §5.
pub fn matches(s: &State, row: &Row, q: &str) -> bool {
    !q.is_empty() && (opt(s, OPT_FOLDERS) || row.open.is_none()) && row.label.to_lowercase().contains(q)
}

/// A node is open when the user opened it, or always while searching (search covers the whole tree).
fn is_open(s: &State, key: &str) -> bool {
    !s.browser_search.is_empty() || s.browser_open.contains(key)
}

/// The visible rows of the active tab, top to bottom (filtered while searching, spec §5).
#[derive(Default)]
pub struct SearchIndex {
    roots: Vec<PathBuf>,
    data: std::sync::Arc<std::sync::Mutex<SearchData>>,
    started: bool,
}
#[derive(Default)]
struct SearchData { rows: Vec<Row>, done: bool, changed: bool }

pub fn search_busy(s: &State) -> bool {
    let index = s.browser_index.borrow();
    index.started && index.data.lock().is_ok_and(|d| !d.done || d.changed)
}
pub fn search_changed(s: &State) -> bool {
    s.browser_index.borrow().data.lock().is_ok_and(|mut d| std::mem::take(&mut d.changed))
}

pub fn rows(s: &State) -> Vec<Row> {
    if s.browser_search.is_empty() { return tree(s); }
    if [1,2,4].contains(&s.browser_tab) {
        let q=s.browser_search.to_lowercase();
        let all=tree(s);
        let hit:Vec<_>=all.iter().map(|r|matches(s,r,&q)).collect();
        if !opt(s,OPT_PARENTS) {return all.into_iter().zip(hit).filter(|(_,keep)|*keep).map(|(mut r,_)|{r.depth=0;r}).collect();}
        let mut keep=hit.clone();
        let mut stack:Vec<usize>=Vec::new();
        for (i,r) in all.iter().enumerate() {
            while stack.last().is_some_and(|&p|all[p].depth>=r.depth) {stack.pop();}
            if hit[i] {for &p in &stack {keep[p]=true;}}
            stack.push(i);
        }
        return all.into_iter().zip(keep).filter(|(_,k)|*k).map(|(r,_)|r).collect();
    }
    let roots = if s.browser_tab == 3 { crate::browser::favorites() } else { library_roots(s) };
    let mut index = s.browser_index.borrow_mut();
    if !index.started || index.roots != roots {
        *index = SearchIndex { roots: roots.clone(), started: true, ..SearchIndex::default() };
        let data = index.data.clone();
        std::thread::spawn(move || {
            let mut pending = roots;
            let mut visited = std::collections::HashSet::new();
            while let Some(path) = pending.pop() {
                if std::sync::Arc::strong_count(&data) == 1 { return; }
                let Ok(canonical) = path.canonicalize() else { continue };
                if !visited.insert(canonical) { continue; }
                if path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&path) {
                        pending.extend(entries.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()));
                    }
                } else {
                    let name = folder_name(&path);
                    let icon = file_icon(&name);
                    if icon != Icon::FileAudio && icon != Icon::PianoRoll { continue; }
                    let label = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    let text = path.to_string_lossy().into_owned();
                    let row = Row { key: format!("search:@{text}"), depth: 0, label, icon, cat: FILE, open: None, bright: false,
                        item: if icon == Icon::FileAudio { Item::Sample(text) } else { Item::File(text) }, path: Some(path) };
                    let mut d = data.lock().unwrap(); d.rows.push(row); d.changed = true;
                }
            }
            let mut d = data.lock().unwrap();
            d.rows.sort_by_cached_key(|r| (r.label.to_lowercase(), r.key.clone()));
            d.done = true; d.changed = true;
        });
    }
    let query = s.browser_search.to_lowercase();
    let data = index.data.lock().unwrap();
    data.rows.iter().filter(|r| r.label.to_lowercase().contains(&query)).cloned().collect()
}

fn tree(s: &State) -> Vec<Row> {
    let mut out = Vec::new();
    let tab = s.browser_tab;
    if [1,2,4].contains(&tab) {
        let root=match tab {1=>0,2=>1,_=>2};
        let (label,icon,cat)=ROOTS[root];
        let key=format!("{tab}:{label}");
        let open=is_open(s,&key);
        out.push(Row{key:key.clone(),depth:0,label:label.into(),icon,cat,open:Some(open),bright:open,item:Item::None,path:None});
        if open {match root {0=>project_rows(s,&key,cat,&mut out),1=>plugin_rows(s,&key,cat,&mut out),_=>preset_rows(s,&key,cat,&mut out)}}
        return out;
    }
    if tab == 3 {
        for path in crate::browser::favorites() {
            let key = format!("3:@{}", path.display());
            let label = folder_name(&path);
            let dir = path.is_dir();
            let open = dir && is_open(s, &key);
            let icon = if dir { if open { Icon::FolderOpen } else { Icon::Folder } } else { file_icon(&label) };
            let name = path.to_string_lossy().into_owned();
            let item = if dir { Item::None } else if icon == Icon::FileAudio { Item::Sample(name) }
                else if crate::presets::is_preset_file(&path) { Item::Preset(name) } else { Item::File(name) };
            out.push(Row { key: key.clone(), depth: 0, label, icon, cat: if dir { FOLDER } else { FILE }, open: dir.then_some(open), bright: true, item, path: Some(path.clone()) });
            if open { disk_rows(s, &path, &key, 1, &mut out); }
        }
        return out;
    }
    // Samples opens directly at the user's library contents, with no redundant root row.
    let root = samples_root();
    disk_rows(s, &root, &disk_key(&root), 0, &mut out);
    out.retain(|r| r.open.is_some() || matches!(r.item, Item::Sample(_)) || r.icon == Icon::PianoRoll);
    for path in library_roots(s).into_iter().filter(|p| *p != root) {
        let key=disk_key(&path);
        let open=is_open(s,&key);
        out.push(Row {key:key.clone(),depth:0,label:root_label(s,&path),icon:if open {Icon::FolderOpen}else{Icon::Folder},cat:FOLDER,open:Some(open),bright:open,item:Item::None,path:Some(path.clone())});
        if open {disk_rows(s,&path,&key,1,&mut out);}
    }
    out
}

fn section(s: &State, parent: &str, label: &str, icon: Icon, cat: u8, out: &mut Vec<Row>) -> bool {
    let key = format!("{parent}/{label}");
    let open = is_open(s, &key);
    out.push(Row { key, depth: 1, label: label.into(), icon, cat, open: Some(open), bright: open, item: Item::None, path: None });
    open
}

fn leaf(out: &mut Vec<Row>, parent: &str, label: String, icon: Icon, cat: u8, bright: bool, item: Item) {
    out.push(Row { key: format!("{parent}/{label}"), depth: 2, label, icon, cat, open: None, bright, item, path: None });
}

/// Tree key of a dropped folder root (All tab).
pub fn disk_key(path: &Path) -> String {
    format!("0:@{}", path.display())
}

/// A dropped folder's label: its name (the whole path for a drive root).
pub fn folder_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// A dropped folder root's label: its Browser name (File ▸ Browser extra search folders, `crate::search_folders`),
/// else the folder's name.
pub fn root_label(s: &State, path: &Path) -> String {
    s.browser_names.get(path).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| folder_name(path))
}

/// One directory entry of a dropped folder (spec §6).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

/// The listing of `dir`: subfolders, then files, each sorted by name, hidden/system entries left out
/// (spec §6). Cached until the structure is reread (Ctrl+R).
pub fn list_dir(s: &State, dir: &Path) -> Rc<Vec<Entry>> {
    if let Some(v) = s.browser_dirs.borrow().get(dir) {
        return v.clone();
    }
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let low = name.to_ascii_lowercase();
                    let md = e.metadata().ok()?;
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        // FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM
                        if md.file_attributes() & 0x6 != 0 {
                            return None;
                        }
                    }
                    if name.starts_with('.') || low == "desktop.ini" || low == "thumbs.db" {
                        return None;
                    }
                    Some(Entry { name, path: e.path(), dir: md.is_dir(), size: md.len(), modified: md.modified().ok() })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase()));
    let v = Rc::new(v);
    s.browser_dirs.borrow_mut().insert(dir.to_path_buf(), v.clone());
    v
}

/// Icon for a file by extension (spec §6 table).
pub fn file_icon(name: &str) -> Icon {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "wav" | "mp3" | "ogg" | "flac" | "aif" | "aiff" => Icon::FileAudio,
        "mid" | "midi" => Icon::PianoRoll,
        "flp" => Icon::FileProject,
        "txt" | "url" | "pdf" | "nfo" | "rtf" | "htm" | "html" | "md" => Icon::FileText,
        "fxp" | "fxb" | "fst" | "vstpreset" | "nmsv" | "spf" | "h2p" | "clap-preset" | "ospreset" => Icon::FilePreset,
        "zip" | "rar" | "7z" => Icon::FileZip,
        _ => Icon::FileBlank,
    }
}

/// Rows of an opened disk folder: its subfolders (recursing into opened ones), then its files.
fn disk_rows(s: &State, dir: &Path, key: &str, depth: u8, out: &mut Vec<Row>) {
    for e in list_dir(s, dir).iter() {
        let k = format!("{key}/{}", e.name);
        if e.dir {
            let open = is_open(s, &k);
            let icon = if open { Icon::FolderOpen } else { Icon::Folder };
            out.push(Row { key: k.clone(), depth, label: e.name.clone(), icon, cat: FOLDER, open: Some(open), bright: open, item: Item::None, path: Some(e.path.clone()) });
            if open {
                disk_rows(s, &e.path, &k, depth.saturating_add(1), out);
            }
        } else {
            let icon = file_icon(&e.name);
            let p = e.path.to_string_lossy().into_owned();
            let stem = || e.name.rsplit_once('.').map_or(e.name.as_str(), |(a, _)| a).to_string();
            let (label, item) = if icon == Icon::FileAudio {
                (stem(), Item::Sample(p))
            } else if crate::presets::is_preset_file(&e.path) {
                (stem(), Item::Preset(p))
            } else {
                (e.name.clone(), Item::File(p))
            };
            out.push(Row { key: k, depth, label, icon, cat: FILE, open: None, bright: false, item, path: Some(e.path.clone()) });
        }
    }
}

fn project_rows(s: &State, root: &str, cat: u8, out: &mut Vec<Row>) {
    let p = &s.project;
    for &(name, icon) in PROJECT {
        if name == INITIALIZED_CONTROLS && p.init_controls.is_empty() {
            continue;
        }
        if name == PROJECT_DATA {
            if let Some(dir) = crate::data_folder::folder(s) {
                data_folder_rows(s, root, &dir, cat, out);
            }
            continue;
        }
        if !section(s, root, name, icon, cat, out) {
            continue;
        }
        let parent = format!("{root}/{name}");
        match name {
            // Undone steps grey, their arrow mirrored (spec: ui/settings-choices.md "Undo history: the four checks").
            "History" => {
                for (e, step) in s.history.entries() {
                    let icon = if s.history.undone(e) { Icon::Redo } else { Icon::Undo };
                    let (bright, item) = (s.history.current(e), Item::History(e));
                    out.push(Row { key: format!("{parent}/{e}"), depth: 2, label: step.into(), icon, cat, open: None, bright, item, path: None });
                }
            }
            "Patterns" => {
                let mut pats: Vec<_> = p.patterns.iter().collect();
                pats.sort_by_key(|p| p.number);
                for pat in pats {
                    let label = pat.name.clone().unwrap_or_else(|| format!("Pattern {}", pat.number));
                    leaf(out, &parent, label, Icon::Note, cat, pat.number == s.pattern, Item::Pattern(pat.number));
                }
            }
            // Hosted plugins open into their parameters (spec: plugins/plugin-parameters.md §6).
            "Generators" => {
                for i in 0..p.channels.len() {
                    let label = channel_label(s, i).unwrap_or_default();
                    plugin_node(s, out, &parent, label, Icon::Plug, cat, i == s.selected_channel, Item::Channel(i), i);
                }
            }
            "Effects" => {
                for m in &p.mixer {
                    for k in (0..m.slots.len()).filter(|&k| m.slots[k].is_some()) {
                        let label = crate::params::effect_label(s, m.index, k as u8).unwrap_or_default();
                        plugin_node(s, out, &parent, label, Icon::Speaker, cat, false, Item::None, crate::plugins::fx_key(m.index, k as u8));
                    }
                }
            }
            // "<owner> - <setting>", as the automation clip on the control is named.
            INITIALIZED_CONTROLS => {
                for c in &p.init_controls {
                    leaf(out, &parent, crate::init::name(s, c.target), Icon::Slide, cat, false, Item::None);
                }
            }
            "Samples" => {
                for c in &p.channels {
                    if let Some(path) = c.sample_path.as_ref().filter(|p| !p.is_empty()) {
                        let file = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
                        leaf(out, &parent, file, Icon::Wave, cat, false, Item::Sample(path.clone()));
                    }
                }
            }
            _ => {}
        }
    }
}

/// The Project data section: the data folder `dir`, opened like a folder (its subfolders, then its files).
fn data_folder_rows(s: &State, root: &str, dir: &Path, cat: u8, out: &mut Vec<Row>) {
    let key = format!("{root}/{PROJECT_DATA}");
    let open = is_open(s, &key);
    let icon = if open { Icon::FolderOpen } else { Icon::Folder };
    out.push(Row { key: key.clone(), depth: 1, label: PROJECT_DATA.into(), icon, cat, open: Some(open), bright: open, item: Item::None, path: Some(dir.to_path_buf()) });
    if open {
        disk_rows(s, dir, &key, 2, out);
    }
}

/// A channel's row label under Generators.
pub fn channel_label(s: &State, i: usize) -> Option<String> {
    let c = s.project.channels.get(i)?;
    Some(c.name.clone().or_else(|| crate::project_plugins::plugin_title(c)).unwrap_or_else(|| format!("Channel {}", i + 1)))
}

/// A generator / effect row: a leaf, or a node listing its parameters when it is a hosted plugin that has some.
#[allow(clippy::too_many_arguments)]
fn plugin_node(s: &State, out: &mut Vec<Row>, parent: &str, label: String, icon: Icon, cat: u8, bright: bool, item: Item, key: usize) {
    if !s.params.has_params(key) {
        return leaf(out, parent, label, icon, cat, bright, item);
    }
    let k = format!("{parent}/{label}");
    let open = is_open(s, &k);
    out.push(Row { key: k.clone(), depth: 2, label, icon, cat, open: Some(open), bright: bright || open, item, path: None });
    if open {
        for (index, e) in s.params.visible(key) {
            out.push(Row { key: format!("{k}/{index}"), depth: 3, label: e.name.clone(), icon: Icon::Slide, cat, open: None, bright: false, item: Item::Param(key, index), path: None });
        }
    }
}

/// Plugin presets: a folder per plugin with presets (the user's preset folders and the factory presets' plugins, by
/// name), each listing the user's presets (sub-folders as folders), then a "Factory" folder with the factory ones.
fn preset_rows(s: &State, root: &str, cat: u8, out: &mut Vec<Row>) {
    let user_root = crate::presets::root();
    let mut plugins: Vec<String> = user_root.as_deref().map(|r| list_dir(s, r).iter().filter(|e| e.dir).map(|e| e.name.clone()).collect()).unwrap_or_default();
    for f in crate::presets::factory_plugins() {
        if !plugins.iter().any(|p| p == f) {
            plugins.push(f.to_string());
        }
    }
    plugins.sort_by_key(|p| p.to_lowercase());
    let preset = |out: &mut Vec<Row>, parent: &str, depth: u8, label: String, path: &Path| {
        let disk = (!crate::presets::is_factory(path)).then(|| path.to_path_buf());
        out.push(Row { key: format!("{parent}/{label}"), depth, label, icon: Icon::FilePreset, cat, open: None, bright: false, item: Item::Preset(path.to_string_lossy().into_owned()), path: disk });
    };
    let folder = |out: &mut Vec<Row>, key: &str, depth: u8, label: &str| -> bool {
        let open = is_open(s, key);
        let icon = if open { Icon::FolderOpen } else { Icon::Folder };
        out.push(Row { key: key.to_string(), depth, label: label.into(), icon, cat: FOLDER, open: Some(open), bright: open, item: Item::None, path: None });
        open
    };
    for plugin in plugins {
        let pk = format!("{root}/{plugin}");
        if !folder(out, &pk, 1, &plugin) {
            continue;
        }
        if let Some(dir) = user_root.as_ref().map(|r| r.join(&plugin)) {
            let entries = list_dir(s, &dir);
            for e in entries.iter().filter(|e| e.dir) {
                let k = format!("{pk}/{}", e.name);
                if folder(out, &k, 2, &e.name) {
                    for f in list_dir(s, &e.path).iter().filter(|f| !f.dir && crate::presets::is_preset_file(&f.path)) {
                        preset(out, &k, 3, crate::presets::shown_name(&f.path), &f.path);
                    }
                }
            }
            for e in entries.iter().filter(|e| !e.dir && crate::presets::is_preset_file(&e.path)) {
                preset(out, &pk, 2, crate::presets::shown_name(&e.path), &e.path);
            }
        }
        let fac = crate::presets::factory(&plugin);
        if fac.is_empty() {
            continue;
        }
        let fk = format!("{pk}/<factory>");
        if folder(out, &fk, 2, "Factory") {
            for p in &fac {
                preset(out, &fk, 3, p.name.clone(), &p.path);
            }
        }
    }
}

/// The plugin menus' generators and effects (spec: ui/plugin-menu.md §5).
fn plugin_rows(s: &State, root: &str, cat: u8, out: &mut Vec<Row>) {
    for (effect, title, icon, list) in [(false, "Generators", Icon::Plug, &s.plugdb.lists.gens), (true, "Effects", Icon::Speaker, &s.plugdb.lists.fx)] {
        if !section(s, root, title, icon, cat, out) { continue; }
        let parent = format!("{root}/{title}");
        let groups: std::collections::BTreeMap<String, Vec<String>> = crate::prefs::text(if effect { "fl_effect_groups" } else { "fl_generator_groups" }).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
        let mut used = std::collections::HashSet::new();
        for (group, names) in groups {
            let entries: Vec<_> = list.iter().filter(|e| names.iter().any(|n| crate::plugin_menu::normalized(n) == crate::plugin_menu::normalized(&e.name))).collect();
            if entries.is_empty() { continue; }
            let key = format!("{parent}/{group}");
            let open = is_open(s, &key);
            out.push(Row { key: key.clone(), depth: 2, label: group, icon: Icon::Folder, cat, open: Some(open), bright: open, item: Item::None, path: None });
            for e in entries {
                used.insert(e.uid.clone());
                if open {
                    leaf(out, &key, e.label.clone(), icon, cat, false, if effect { Item::Effect(e.index) } else { Item::Generator(e.index) });
                    if let Some(row) = out.last_mut() { row.depth = 3; }
                }
            }
        }
        for e in list.iter().filter(|e| !used.contains(&e.uid)) {
            leaf(out, &parent, e.label.clone(), icon, cat, false, if effect { Item::Effect(e.index) } else { Item::Generator(e.index) });
        }
    }
}

/// Height of the list viewport for a browser of height `h` (from `WORK_Y`).
pub fn list_h(s: &State) -> f32 {
    (s.screen.1 - list_top(s) - FOOTER_H - info_h(&rows(s), s)).max(0.0)
}

/// The selected disk entry, shown in the info panel (spec §6).
fn info_path(rows: &[Row], s: &State) -> Option<PathBuf> {
    let k = s.browser_sel.as_deref()?;
    rows.iter().find(|r| r.key == k)?.path.clone()
}

fn info_h(rows: &[Row], s: &State) -> f32 {
    if info_path(rows, s).is_some() { INFO_H } else { 0.0 }
}

/// `35 B`, `12.4 KB`, `2.29 MB` (spec §6).
pub fn size_text(n: u64) -> String {
    let f = n as f64;
    if n < 1024 {
        format!("{n} B")
    } else if f < 1024.0 * 1024.0 {
        format!("{:.2} KB", f / 1024.0)
    } else if f < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} MB", f / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", f / 1024.0 / 1024.0 / 1024.0)
    }
}

/// Local `M/D/YYYY h:mm:ss AM` (spec §6).
pub fn date_text(t: std::time::SystemTime) -> String {
    let Some((y, mo, d, h, mi, se)) = local_time(t) else { return String::new() };
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{mo}/{d}/{y} {h12}:{mi:02}:{se:02} {ap}")
}

/// A file time in the local time zone: (year, month, day, hour, minute, second).
fn local_time(t: std::time::SystemTime) -> Option<(u16, u16, u16, u16, u16, u16)> {
    use chrono::{Datelike, Timelike};
    let l = chrono::DateTime::<chrono::Local>::from(t);
    let n = |v: u32| u16::try_from(v).ok();
    Some((u16::try_from(l.year()).ok()?, n(l.month())?, n(l.day())?, n(l.hour())?, n(l.minute())?, n(l.second())?))
}

/// The info panel above the footer: title, date, size, path of the selected disk entry (spec §6).
fn paint_info(ui: &mut Ui, s: &State, path: &Path, r: Rect) {
    let t = ui.t;
    ui.c.fill_rect(r, t.browser_tabs);
    icons::draw(ui.c, Icon::FolderPlus, Rect::new(r.right() - 52.0, r.bottom() - 50.0, 44.0, 44.0), t.browser_tabs.shade(0.1));
    let md = std::fs::metadata(path).ok();
    let dir = md.as_ref().is_some_and(|m| m.is_dir());
    let size = if dir { list_dir(s, path).iter().filter(|e| !e.dir).map(|e| e.size).sum() } else { md.as_ref().map_or(0, |m| m.len()) };
    let date = md.and_then(|m| m.modified().ok()).map(date_text).unwrap_or_default();
    let f = os_ui::Font::new(13);
    let (lx, vx, lh) = (r.x + 5.0, r.x + 40.0, 15.0);
    let vw = r.right() - vx - 4.0;
    let mut y = r.y + 3.0;
    let title = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    for (k, v) in [("Title", title), ("Date", date), ("Size", size_text(size))] {
        ui.c.text(f, lx, y, t.text_dim, k);
        ui.c.text_clip(f, vx, y, vw, t.text, &v);
        y += lh;
    }
    ui.c.text(f, lx, y, t.text_dim, "Path");
    // Wrap the path at any character (FL breaks mid-word too).
    let mut line = String::new();
    for ch in path.display().to_string().chars() {
        line.push(ch);
        if ui.c.text_width(f, &line) > vw {
            let last = line.pop().unwrap_or(' ');
            ui.c.text(f, vx, y, t.text, &line);
            y += lh;
            line = last.to_string();
            if y + lh > r.bottom() {
                return;
            }
        }
    }
    ui.c.text(f, vx, y, t.text, &line);
}

/// Top of the rows: below the search box when "Search box on top" is on (spec §4).
pub fn list_top(s: &State) -> f32 {
    let lift = super::work_y(s) - super::WORK_Y;
    lift + LIST_Y + if opt(s,OPT_TOP) {SEARCH_ROW} else {0.}
}

/// Height of the search row above the list ("Search box on top").
const SEARCH_ROW: f32 = 27.0;

/// Largest useful scroll offset.
pub fn max_scroll(s: &State) -> f32 {
    (rows(s).len() as f32 * ROW_H - list_h(s)).max(0.0)
}

/// A hovered row whose label does not fit scrolls it slowly to its end and back (spec §9): the row, when the hover
/// began, the label's rect, how much of the label is cut and the offset last painted.
#[derive(Debug, Clone, PartialEq)]
pub struct Marquee {
    pub key: String,
    pub since: std::time::Instant,
    pub rect: Rect,
    pub over: f32,
    pub off: f32,
}

/// The scroll's pause at either end (s) and speed (px/s).
const MARQUEE_WAIT: f32 = 1.0;
const MARQUEE_SPEED: f32 = 30.0;

/// How far a label cut by `over` px is scrolled `t` s into the hover: still, to its end, still, back to its start, and
/// again; whole px.
pub fn marquee_offset(t: f32, over: f32) -> f32 {
    if over <= 0.0 {
        return 0.0;
    }
    let run = over / MARQUEE_SPEED;
    let p = t.max(0.0) % (2.0 * (MARQUEE_WAIT + run));
    let out = (p - MARQUEE_WAIT).clamp(0.0, run);
    let back = (p - 2.0 * MARQUEE_WAIT - run).clamp(0.0, run);
    ((out - back) * MARQUEE_SPEED).round()
}

pub fn paint(ui: &mut Ui, s: &State, r: Rect) {
    let t = ui.t;
    if s.browser_auto_hidden {
        ui.c.fill_rect(r,t.browser_title);
        ui.hit(r,Action::Swallow,"Move here to show the browser");
        ui.c.fill_rect(Rect::new(r.x+2.,r.y+8.,3.,28.),t.text_dim);
        return;
    }
    // Measured with the browser top at `WORK_Y`; it moves up with a hidden toolbar (spec: ui/toolbar-menu.md §5).
    let dy = r.y - super::WORK_Y;
    // The panel left of the splitter: 201 px measured, wider as the splitter is dragged (§9).
    let bw = r.w - SPLITTER;
    ui.hit(r, Action::Swallow, "");
    // Title bar.
    ui.c.fill_rect(Rect::span(0.0, bw - 1.0, 83.0 + dy, 105.0 + dy), t.browser_title);
    let at = menu_at(s);
    let menu_open = s.playlist.popup.as_ref().is_some_and(|p| p.x == at.0 && p.y == at.1);
    let mhov = ui.hit(Rect::new(2.0, 85.0 + dy, 16.0, 18.0), Action::BrowserMenu, "Browser options");
    if mhov || menu_open {
        ui.c.stroke_rrect(Rect::new(3.0, 86.0 + dy, 15.0, 16.0), 2.0, 1.2, t.browser_tab_outline);
    }
    icons::draw(ui.c, Icon::ArrowRight, Rect::new(5.0, 87.0 + dy, 12.0, 14.0), if mhov || menu_open { t.glyph } else { t.text_dim });
    // Collapse structure and Reread structure (spec §4, "Title-bar icons"): outlined like the ▸ while hovered.
    for (r, icon, action, hint) in [
        (Rect::new(17.0, 85.0 + dy, 16.0, 18.0), Icon::ArrowUp, Action::BrowserCollapse, "Collapse structure (Ctrl+Up)"),
        (Rect::new(33.0, 85.0 + dy, 15.0, 18.0), Icon::Back, Action::BrowserReread, "Reread structure (Ctrl+R)"),
    ] {
        let hov = ui.hit(r, action, hint);
        if hov {
            ui.c.stroke_rrect(Rect::new(r.x + 1.0, 86.0 + dy, r.w - 1.0, 16.0), 2.0, 1.2, t.browser_tab_outline);
        }
        icons::draw(ui.c, icon, Rect::new(r.x + 1.0, 87.0 + dy, 12.0, 14.0), if hov { t.glyph } else { t.text_dim });
    }
    ui.c.text(os_ui::Font::new(14), 52.0, 85.0 + dy, t.text_dim, "Browser");
    // Tabs (spec ui/browser.md §1): FL's five without Online content, sharing the strip.
    ui.c.fill_rect(Rect::span(0.0, bw - 1.0, 106.0 + dy, 141.0 + dy), t.browser_tabs);
    let tabs=tabs(s);
    let step=((bw-6.)/tabs.len() as f32).min(50.);
    for (slot,(i,icon,tip)) in tabs.into_iter().enumerate() {
        let cx = 3. + step*(slot as f32+0.5);
        let rr = Rect::new(cx - 18.0, 108.0 + dy, 36.0, 30.0);
        let hov = ui.hit(rr, Action::BrowserTab(i), tip);
        let active = s.browser_tab == i;
        if active { ui.c.stroke_rrect(rr, 4.0, 1.5, t.browser_tab_outline); }
        let c = if hov || active { t.glyph } else { t.glyph.shade(-0.1) };
        icons::draw(ui.c, icon, Rect::new(cx - 9.0, 114.0 + dy, 18.0, 18.0), c);
    }
    // List.
    let top = list_top(s);
    let search_on_top = opt(s,OPT_TOP);
    if search_on_top {
        ui.c.fill_rect(Rect::new(0.0, LIST_Y + dy, bw, SEARCH_ROW), t.browser_footer);
        search_box(ui, s, Rect::new(13.0, LIST_Y + dy + 4.0, bw - 38.0, 20.0));
    }
    let rows = rows(s);
    let info = info_path(&rows, s);
    let ih = if info.is_some() { INFO_H } else { 0.0 };
    let list = Rect::span(13.0, bw - 1.0, top, r.bottom() - FOOTER_H - ih);
    ui.c.fill_rect(Rect::new(0.0, top, 13.0, list.h), t.browser_tabs.shade(0.02));
    ui.c.fill_rect(list, t.browser_list);
    ui.hit(Rect::new(0.0, top, bw, list.h), Action::BrowserList, "");
    ui.c.push_clip(Rect::new(0.0, top, bw, list.h));
    let cats: [Color; 2] = [t.cat_project, t.cat_plugin_db];
    let scroll = s.browser_scroll.min((rows.len() as f32 * ROW_H - list.h).max(0.0)).max(0.0);
    let sel = s.browser_sel.as_deref();
    let mut marquee = None;
    for (i, row) in rows.iter().enumerate() {
        let y = top + ROW_H * i as f32 - scroll;
        if y + ROW_H < list.y || y > list.bottom() {
            continue;
        }
        let x = 13.0 + INDENT * f32::from(row.depth);
        let rect = Rect::new(13.0, y, bw - 13.0, ROW_H);
        // A parameter row's hint carries its value (spec: plugins/plugin-parameters.md §6); read for the hovered row only.
        let hint = match row.item {
            Item::Param(key, index) if ui.hovering(rect) => crate::params::row_hint(s, key, index),
            _ => row.label.clone(),
        };
        let hov = ui.hit(rect, Action::BrowserRow(i), hint);
        if sel == Some(row.key.as_str()) || hov {
            ui.c.fill_rect(rect, t.browser_row_hi);
        }
        let base = match row.cat {
            FOLDER => t.browser_folder,
            FILE => t.text.shade(-0.08),
            c => cats[usize::from(c).min(1)],
        };
        let c = if row.bright { base.shade(0.12) } else if row.depth > 0 { base.shade(-0.06) } else { base };
        let c = if opt(s,OPT_CONTRAST) {t.text} else {c};
        let c = if matches!(row.item, Item::History(e) if s.history.undone(e)) { t.text_faint } else { c };
        // Expander: only an opened node shows its ▾ in the gutter (FL screenshots, spec §2).
        if row.open == Some(true) && s.browser_search.is_empty() {
            let (ax, cy) = (x - 9.0, y + ROW_H / 2.0);
            for k in 0..4 {
                let k = k as f32;
                ui.c.fill_rect(Rect::new(ax + k, cy - 2.0 + k, 7.0 - 2.0 * k, 1.0), t.text_dim);
            }
        }
        // File-type tints (spec §6): audio / project orange, MIDI blue.
        let ic = match row.icon {
            Icon::FileAudio | Icon::FileProject => t.browser_search,
            Icon::PianoRoll if row.cat == FILE => t.cat_plugin_db,
            _ => c,
        };
        icons::draw(ui.c, row.icon, Rect::new(x + 6.0, y + 3.0, 17.0, 17.0), ic);
        let (f, w) = (os_ui::Font::new(17), bw - 1.0 - x - 27.0);
        let over = ui.c.text_width(f, &row.label) - w;
        // Hovered, a label that does not fit scrolls (§9) rather than being cut.
        if hov && over > 0.0 {
            let now = std::time::Instant::now();
            let since = s.browser_marquee.borrow().as_ref().filter(|m| m.key == row.key).map_or(now, |m| m.since);
            let off = marquee_offset(now.duration_since(since).as_secs_f32(), over);
            let lr = Rect::new(x + 25.0, y, w, ROW_H);
            ui.c.push_clip(lr);
            ui.c.text(f, lr.x - off, y, c, &row.label);
            ui.c.pop_clip();
            marquee = Some(Marquee { key: row.key.clone(), since, rect: lr, over, off });
            continue;
        }
        let label = if row.path.is_some() { middle_cut(ui, f, &row.label, w) } else { row.label.clone() };
        ui.c.text_clip(f, x + 25.0, y, w, c, &label);
    }
    *s.browser_marquee.borrow_mut() = marquee;
    let empty = if !s.browser_search.is_empty() { if search_busy(s) { "Searching samples..." } else { "No matches" } } else if s.browser_tab == 3 { "No favorites yet" } else { "Pro Samples unavailable or empty" };
    if rows.is_empty() {
        ui.c.text(os_ui::Font::new(14).italic(), 22.0, top + 6.0, t.text_faint, empty);
    }
    ui.c.pop_clip();
    if let Some(p) = &info {
        paint_info(ui, s, p, Rect::new(0.0, list.bottom(), bw, ih));
    }
    // Footer: the search field takes the width the splitter adds.
    let fy = r.bottom() - 27.0;
    ui.c.fill_rect(Rect::new(0.0, fy, bw, 27.0), t.browser_footer);
    let field = Rect::new(13.0, fy + 4.0, bw - 38.0, 20.0);
    if !search_on_top {
        search_box(ui, s, field);
    } else {
        ui.c.fill_rrect(field, 3.0, t.sunken);
    }
    icons::draw(ui.c, Icon::Folder, Rect::new(field.right() - 18.0, fy + 7.0, 14.0, 14.0), t.text_faint);
    let glass = Rect::new(bw - 23.0, fy + 3.0, 21.0, 21.0);
    let hov = ui.hit(glass, Action::BrowserSearch, "Search the browser (Enter = find next, Esc = clear)");
    icons::draw(ui.c, Icon::Search, Rect::new(glass.x + 2.0, fy + 5.0, 17.0, 17.0), if hov { t.browser_search.shade(0.2) } else { t.browser_search });
    // Splitter: drags the browser's width (§9).
    ui.c.fill_rect(Rect::new(bw, r.y, 6.0, r.h), t.splitter);
    ui.c.fill_rect(Rect::new(bw + 6.0, r.y, 2.0, r.h), t.splitter_dark);
    ui.hit(Rect::new(bw, r.y, SPLITTER, r.h), Action::BrowserSplitter, "Browser: drag to resize, double-click for the default width");
}

/// The splitter's width right of the browser panel.
const SPLITTER: f32 = 8.0;

/// A long disk name shortened in the middle with ".." to fit `w` (FL screenshots, spec §6).
fn middle_cut(ui: &mut Ui, f: os_ui::Font, text: &str, w: f32) -> String {
    if ui.c.text_width(f, text) <= w {
        return text.to_string();
    }
    let ch: Vec<char> = text.chars().collect();
    for keep in (1..ch.len()).rev() {
        let head = keep.div_ceil(2);
        let s: String = ch[..head].iter().chain([&'.', &'.']).chain(&ch[ch.len() - (keep - head)..]).collect();
        if ui.c.text_width(f, &s) <= w {
            return s;
        }
    }
    String::new()
}

/// The primary library plus all user-added folders; shared by browsing and recursive search.
fn library_roots(s: &State) -> Vec<PathBuf> {
    let mut roots=vec![samples_root()];
    for path in &s.browser_folders {if !roots.contains(path) {roots.push(path.clone());}}
    roots[1..].sort_by_key(|p|root_label(s,p).to_lowercase());
    roots
}

/// Preserve the existing personal library where present; fresh computers use their user Samples folder.
pub fn samples_root() -> PathBuf {
    crate::prefs::text("browser_samples_root").filter(|p| !p.trim().is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| {
            let legacy=PathBuf::from(r"E:\Pro Samples");
            if legacy.is_dir() {legacy} else {os_paths::user_dir().unwrap_or_else(|| PathBuf::from(".")).join("Samples")}
        })
}

/// Where the options menu opens (just below the title bar's ▸, spec §4).
pub fn menu_at(s: &State) -> (f32, f32) {
    (1.0, 105.0 + super::work_y(s) - super::WORK_Y)
}

/// The search field (spec §5): typed text, a caret while focused; click focuses.
fn search_box(ui: &mut Ui, s: &State, r: Rect) {
    let t = ui.t;
    ui.hit(r, Action::BrowserSearch, "Search the browser (Enter = find next, Esc = clear)");
    ui.c.fill_rrect(r, 3.0, t.sunken);
    if s.browser_search_focus {
        ui.c.stroke_rrect(r, 3.0, 1.0, t.browser_search);
    }
    let f = os_ui::Font::new(14);
    let text_w = r.w - 26.0;
    // Show the end of a long query.
    let mut shown: &str = &s.browser_search;
    while ui.c.text_width(f, shown) > text_w && !shown.is_empty() {
        let cut = shown.char_indices().nth(1).map_or(shown.len(), |(i, _)| i);
        shown = &shown[cut..];
    }
    if shown.is_empty() && !s.browser_search_focus {
        ui.c.text_clip(f, r.x + 5.0, r.y + 1.0, text_w, t.text_faint, "Search samples...");
    } else {
        ui.c.text_clip(f, r.x + 5.0, r.y + 1.0, text_w, t.glyph, shown);
    }
    if s.browser_search_focus {
        let cx = r.x + 6.0 + ui.c.text_width(f, shown);
        ui.c.fill_rect(Rect::new(cx, r.y + 4.0, 1.0, r.h - 8.0), t.glyph);
    }
}

/// Rect of the drag label at mouse position (x, y).
pub fn ghost_rect(x: f32, y: f32) -> Rect {
    Rect::new(x + 12.0, y + 6.0, 190.0, 22.0)
}

/// The label that follows the cursor during a browser drag (spec §3).
pub fn paint_ghost(ui: &mut Ui, s: &State) {
    let Some((label, x, y)) = &s.browser_drag else { return };
    let t = ui.t;
    let r = ghost_rect(*x, *y);
    let f = os_ui::Font::new(15);
    let w = (ui.c.text_width(f, label) + 14.0).min(r.w);
    let r = Rect::new(r.x, r.y, w, r.h);
    ui.c.fill_rrect(r, 3.0, t.browser_row_hi.shade(0.08));
    ui.c.stroke_rrect(r, 3.0, 1.0, t.browser_tab_outline);
    ui.c.text_clip(f, r.x + 7.0, r.y + 2.0, r.w - 10.0, t.glyph, label);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File dates read in the local time zone on every platform (spec §6): within a day of the UTC date.
    #[test]
    fn dates_are_local_and_twelve_hour() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000); // 2023-11-14 22:13:20 UTC
        let text = date_text(t);
        assert!(text.contains("/2023 ") && (text.starts_with("11/14/") || text.starts_with("11/15/")), "{text}");
        assert!(text.ends_with(" AM") || text.ends_with(" PM"), "{text}");
        assert_eq!(size_text(1536), "1.50 KB");
    }

    /// A long label's scroll (§9): still for a second, 30 px/s to its end, still, back to its start, and again.
    #[test]
    fn long_labels_scroll_to_the_end_and_back() {
        let over = 60.0;
        let at = |t: f32| marquee_offset(t, over);
        assert_eq!([at(0.5), at(2.0), at(3.5), at(5.0), at(6.0), at(8.0)], [0.0, 30.0, 60.0, 30.0, 0.0, 30.0]);
        assert_eq!(marquee_offset(3.0, 0.0), 0.0, "a label that fits stays");
    }
}
