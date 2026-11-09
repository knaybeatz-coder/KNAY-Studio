// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-profile presentation edits for workspace menus. Rebuild from the real menu each time
//! so command targets, shortcuts, availability and checkmarks always come from the application.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    app::State,
    playlist::{Cmd, PItem, Popup},
    settings_file,
    views::settings::MENUS,
};

pub const OPEN: &str = "Customize menus...";
pub const VISIBILITY: &str = "visible_menu_customizer";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    #[default]
    Playlist,
    AudioClip,
    PatternClip,
    AutomationClip,
    Piano,
    PianoTools,
    PianoSelection,
    Rack,
    RackChannel,
    Mixer,
    MixerTrack,
    MixerSlot,
    Browser,
    Main(usize),
}

impl Menu {
    fn menus() -> impl Iterator<Item = Self> {
        [
            Self::Playlist,
            Self::AudioClip,
            Self::PatternClip,
            Self::AutomationClip,
            Self::Piano,
            Self::PianoTools,
            Self::PianoSelection,
            Self::Rack,
            Self::RackChannel,
            Self::Mixer,
            Self::MixerTrack,
            Self::MixerSlot,
            Self::Browser,
        ]
        .into_iter()
        .chain((0..crate::views::menus::MENUS.len()).map(Self::Main))
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Playlist => "Playlist settings",
            Self::AudioClip => "Playlist · Audio clip",
            Self::PatternClip => "Playlist · Pattern clip",
            Self::AutomationClip => "Playlist · Automation clip",
            Self::Piano => "Piano Roll settings",
            Self::PianoTools => "Piano Roll · Tools",
            Self::PianoSelection => "Piano Roll · Selected notes",
            Self::Rack => "Channel Rack settings",
            Self::RackChannel => "Channel Rack · Channel",
            Self::Mixer => "Mixer settings",
            Self::MixerTrack => "Mixer · Track",
            Self::MixerSlot => "Mixer · Effect slot",
            Self::Browser => "Browser settings",
            Self::Main(i) => crate::views::menus::MENUS[i].title,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Playlist => "menu_layout_playlist",
            Self::AudioClip => "menu_layout_audio_clip",
            Self::PatternClip => "menu_layout_pattern_clip",
            Self::AutomationClip => "menu_layout_automation_clip",
            Self::Piano => "menu_layout_piano",
            Self::PianoTools => "menu_layout_piano_tools",
            Self::PianoSelection => "menu_layout_piano_selection",
            Self::Rack => "menu_layout_rack",
            Self::RackChannel => "menu_layout_rack_channel",
            Self::Mixer => "menu_layout_mixer",
            Self::MixerTrack => "menu_layout_mixer_track",
            Self::MixerSlot => "menu_layout_mixer_slot",
            Self::Browser => "menu_layout_browser",
            Self::Main(i) => [
                "menu_layout_file",
                "menu_layout_edit",
                "menu_layout_add",
                "menu_layout_patterns",
                "menu_layout_view",
                "menu_layout_options",
                "menu_layout_tools",
                "menu_layout_help",
            ][i],
        }
    }

    fn catalog(self, s: &State) -> Vec<PItem> {
        use crate::playlist::{clip_menu_items, PickKind};
        match self {
            Self::Playlist => crate::playlist::default_options_menu(s, 0., 0.).items,
            Self::AudioClip => clip_menu_items(
                PickKind::Audio,
                0,
                false,
                true,
                true,
                true,
                s.playlist.stretch,
            ),
            Self::PatternClip => clip_menu_items(
                PickKind::Pattern,
                0,
                false,
                false,
                false,
                false,
                s.playlist.stretch,
            ),
            Self::AutomationClip => clip_menu_items(
                PickKind::Automation,
                0,
                false,
                false,
                false,
                false,
                s.playlist.stretch,
            ),
            Self::Piano => crate::piano_tools::default_options_menu(s, 0., 0.).items,
            Self::PianoTools => crate::piano_tools::default_tools_menu(s, 0., 0.).items,
            Self::PianoSelection => crate::piano_tools::default_selected_menu(s, 0., 0.).items,
            Self::Rack => crate::rack::default_options_menu(s, 0., 0.).items,
            Self::RackChannel => {
                crate::rack::default_channel_menu(s, s.selected_channel, 0., 0., true).items
            }
            Self::Mixer => crate::mixer::default_options_menu(s, 0., 0.).items,
            Self::MixerTrack => crate::mixer::default_track_menu(s, s.selected_mixer, 0., 0.).items,
            Self::MixerSlot => crate::mixer::default_slot_menu(s, 0, 0., 0.).items,
            Self::Browser => crate::browser::default_options_menu(s),
            Self::Main(i) => crate::views::menus::default_organized_items(s, i, true),
        }
    }

    pub fn popup(self, mut popup: Popup) -> Popup {
        popup.items = self.apply(popup.items);
        popup
    }

    pub fn apply(self, items: Vec<PItem>) -> Vec<PItem> {
        let edits = self.edits();
        if edits.is_empty() {
            return items;
        }
        let mut tree = Tree::new(items);
        tree.replay(&edits);
        let items = render(tree.rows);
        if items.is_empty() {
            vec![PItem::head("No visible commands")]
        } else {
            items
        }
    }

    fn edits(self) -> Vec<Edit> {
        settings_file::get(self.key())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(self, edits: &[Edit]) {
        if edits.is_empty() {
            settings_file::write(self.key(), "");
        } else if let Ok(text) = serde_json::to_string(edits) {
            settings_file::write(self.key(), &text);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Edit {
    Visibility {
        id: String,
        hidden: bool,
    },
    Move {
        id: String,
        parent: String,
        before: Option<String>,
    },
    Group {
        id: String,
        name: String,
    },
    Dissolve {
        id: String,
    },
}

struct Node {
    id: String,
    item: PItem,
    children: Vec<Node>,
    folder: bool,
    opaque: bool,
    hidden: bool,
}

impl Node {
    fn decoration(&self) -> bool {
        self.item.sep || self.item.col || self.item.head
    }

    fn destination(&self) -> bool {
        self.folder && !self.opaque
    }
}

struct Tree {
    rows: Vec<Node>,
}

/// Project-generated lists are customizable as a group. Individual project names are not
/// profile preferences and can change whenever a project, plug-in list or preset is loaded.
fn opaque(item: &PItem) -> bool {
    matches!(
        item.label.as_ref(),
        "Recent projects"
            | "Select pattern"
            | "MIDI parts"
            | "Target channel"
            | "Move to channel"
            | "Copy to channel"
            | "Add instrument"
            | "Insert instrument"
            | "Replace instrument"
            | "Replace"
            | "Sidechain input"
            | "Display filter"
            | "New from template"
            | "Effect"
            | "Toolbars"
            | "Channel instruments"
    )
}

fn identity(item: &PItem) -> String {
    match &item.cmd {
        Cmd::MainMenu(menu, index) => format!("main-{menu}-{index}"),
        Cmd::Mixer(crate::mixer::MCmd::Trim { non_empty }) => format!("trim-{non_empty}"),
        Cmd::Mixer(crate::mixer::MCmd::Solo(_)) => "solo".into(),
        Cmd::Browser(crate::browser::BCmd::Detach) => "detach-browser".into(),
        Cmd::Browser(crate::browser::BCmd::StarterPack) => "starter-pack".into(),
        _ => item.label.to_string(),
    }
}

impl Tree {
    fn new(items: Vec<PItem>) -> Self {
        Self {
            rows: Self::nodes("", items),
        }
    }

    fn nodes(parent: &str, items: Vec<PItem>) -> Vec<Node> {
        let mut seen = HashMap::new();
        items
            .into_iter()
            .map(|mut item| {
                let label = identity(&item);
                let count = seen.entry(label.clone()).or_insert(0);
                let id = format!("{parent}/{}:{label}:{count}", label.len());
                *count += 1;
                let opaque = opaque(&item);
                let folder = opaque || !item.children.is_empty();
                let children = if opaque {
                    Vec::new()
                } else {
                    Self::nodes(&id, std::mem::take(&mut item.children))
                };
                Node {
                    id,
                    item,
                    children,
                    folder,
                    opaque,
                    hidden: false,
                }
            })
            .collect()
    }

    fn replay(&mut self, edits: &[Edit]) {
        for edit in edits {
            match edit {
                Edit::Visibility { id, hidden } => {
                    if let Some(node) = find_mut(&mut self.rows, id) {
                        node.hidden = *hidden;
                    }
                }
                Edit::Group { id, name } => {
                    if find(&self.rows, id).is_none() {
                        self.rows.push(Node {
                            id: id.clone(),
                            item: PItem::sub(name.clone(), Vec::new()),
                            children: Vec::new(),
                            folder: true,
                            opaque: false,
                            hidden: false,
                        });
                    }
                }
                Edit::Dissolve { id } if id.starts_with('@') => {
                    dissolve(&mut self.rows, id);
                }
                Edit::Move { id, parent, before } => {
                    let Some(node) = find(&self.rows, id) else {
                        continue;
                    };
                    if node.decoration() || parent == id || find(&node.children, parent).is_some() {
                        continue;
                    }
                    if !parent.is_empty()
                        && !find(&self.rows, parent).is_some_and(Node::destination)
                    {
                        continue;
                    }
                    let node = remove(&mut self.rows, id).expect("validated menu entry");
                    let rows = if parent.is_empty() {
                        &mut self.rows
                    } else {
                        &mut find_mut(&mut self.rows, parent)
                            .expect("validated destination")
                            .children
                    };
                    let index = before
                        .as_ref()
                        .and_then(|b| rows.iter().position(|n| &n.id == b))
                        .unwrap_or(rows.len());
                    rows.insert(index, node);
                }
                _ => {}
            }
        }
    }
}

fn find<'a>(rows: &'a [Node], id: &str) -> Option<&'a Node> {
    rows.iter().find_map(|n| {
        if n.id == id {
            Some(n)
        } else {
            find(&n.children, id)
        }
    })
}

fn find_mut<'a>(rows: &'a mut [Node], id: &str) -> Option<&'a mut Node> {
    rows.iter_mut().find_map(|n| {
        if n.id == id {
            Some(n)
        } else {
            find_mut(&mut n.children, id)
        }
    })
}

fn remove(rows: &mut Vec<Node>, id: &str) -> Option<Node> {
    if let Some(i) = rows.iter().position(|n| n.id == id) {
        return Some(rows.remove(i));
    }
    rows.iter_mut().find_map(|n| remove(&mut n.children, id))
}

fn dissolve(rows: &mut Vec<Node>, id: &str) -> bool {
    if let Some(i) = rows.iter().position(|n| n.id == id) {
        let group = rows.remove(i);
        rows.splice(i..i, group.children);
        return true;
    }
    rows.iter_mut().any(|n| dissolve(&mut n.children, id))
}

fn render(rows: Vec<Node>) -> Vec<PItem> {
    let mut out: Vec<PItem> = Vec::new();
    for mut node in rows {
        if node.hidden {
            continue;
        }
        if node.folder && !node.opaque {
            node.item.children = render(node.children);
            if node.item.children.is_empty() && node.item.cmd == Cmd::Nothing {
                continue;
            }
        }
        if node.item.sep
            && out
                .last()
                .is_none_or(|item| item.head || item.sep || item.col)
        {
            continue;
        }
        out.push(node.item);
    }
    while out
        .last()
        .is_some_and(|item| item.sep || item.head || item.col)
    {
        out.pop();
    }
    out
}

#[derive(Debug, Default)]
pub struct Editor {
    menu: Menu,
    expanded: HashSet<String>,
    pub name: crate::dialog::Field,
    pub name_focused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Choose,
    Select(Menu),
    Expand(String),
    Toggle(String, bool),
    Reorder(String, i8),
    MovePicker(String),
    Move(String, String),
    Name,
    Add,
    Dissolve(String),
    Undo,
    Reset,
}

#[derive(Debug)]
pub struct Entry {
    pub id: String,
    pub label: String,
    pub depth: usize,
    pub hidden: bool,
    pub parent_hidden: bool,
    pub folder: bool,
    pub expanded: bool,
    pub up: bool,
    pub down: bool,
}

pub fn entries(s: &State) -> (Vec<Entry>, bool) {
    let editor = &s.settings.menu_editor;
    let edits = editor.menu.edits();
    let mut tree = Tree::new(editor.menu.catalog(s));
    tree.replay(&edits);
    let mut out = Vec::new();
    collect_entries(&tree.rows, editor, 0, false, &mut out);
    (out, !edits.is_empty())
}

fn collect_entries(
    rows: &[Node],
    editor: &Editor,
    depth: usize,
    parent_hidden: bool,
    out: &mut Vec<Entry>,
) {
    let mut iter = rows.iter().filter(|n| !n.decoration()).peekable();
    let mut up = false;
    while let Some(n) = iter.next() {
        let folder = n.destination();
        let expanded = editor.expanded.contains(&n.id);
        let label = match &n.item.cmd {
            Cmd::Mixer(crate::mixer::MCmd::Trim { non_empty: false }) => {
                "Unlinked tracks only".into()
            }
            Cmd::Mixer(crate::mixer::MCmd::Trim { non_empty: true }) => {
                "Unlinked and non-empty tracks".into()
            }
            _ => n.item.label.to_string(),
        };
        out.push(Entry {
            id: n.id.clone(),
            label,
            depth,
            hidden: n.hidden,
            parent_hidden,
            folder,
            expanded,
            up,
            down: iter.peek().is_some(),
        });
        if folder && expanded {
            collect_entries(
                &n.children,
                editor,
                depth + 1,
                parent_hidden || n.hidden,
                out,
            );
        }
        up = true;
    }
}

pub fn current(s: &State) -> &'static str {
    s.settings.menu_editor.menu.label()
}

fn picker(s: &mut State, at: (f32, f32), items: Vec<PItem>) {
    s.playlist.popup = Some(Popup {
        x: at.0,
        y: at.1,
        items,
        sub: None,
        scroll: [0; 2],
        cascade: Vec::new(),
    });
}

fn pick(label: impl Into<std::borrow::Cow<'static, str>>, action: Action) -> PItem {
    PItem::new(label, Cmd::MenuLayout(action))
}

fn destinations(rows: &[Node], id: &str, prefix: &str, out: &mut Vec<PItem>) {
    for n in rows.iter().filter(|n| n.destination() && n.id != id) {
        let name = format!("{prefix}{}", n.item.label);
        out.push(pick(name.clone(), Action::Move(id.into(), n.id.clone())));
        destinations(&n.children, id, &format!("{name} / "), out);
    }
}

fn sibling_move(rows: &[Node], id: &str, dir: i8, parent: &str) -> Option<Edit> {
    let editable: Vec<_> = rows.iter().filter(|n| !n.decoration()).collect();
    if let Some(i) = editable.iter().position(|n| n.id == id) {
        let before = if dir < 0 {
            Some(editable.get(i.checked_sub(1)?)?.id.clone())
        } else {
            editable.get(i + 1)?;
            editable.get(i + 2).map(|n| n.id.clone())
        };
        return Some(Edit::Move {
            id: id.into(),
            parent: parent.into(),
            before,
        });
    }
    rows.iter()
        .find_map(|n| sibling_move(&n.children, id, dir, &n.id))
}

pub fn act(s: &mut State, action: Action, at: (f32, f32)) {
    if !s.settings.open || s.settings.tab != MENUS {
        return;
    }
    let editor = &mut s.settings.menu_editor;
    editor.name_focused = action == Action::Name;
    let menu = editor.menu;
    match action {
        Action::Choose => {
            let items = Menu::menus()
                .map(|m| {
                    pick(m.label(), Action::Select(m)).mark(crate::playlist::Mark::Radio(m == menu))
                })
                .collect();
            return picker(s, at, items);
        }
        Action::Select(m) => {
            *editor = Editor {
                menu: m,
                ..Editor::default()
            };
            s.settings.scroll = 0.;
            return;
        }
        Action::Name => return editor.name.select_all(),
        Action::Expand(id) => {
            if !editor.expanded.remove(&id) {
                editor.expanded.insert(id);
            }
            return;
        }
        _ => {}
    }

    let mut edits = menu.edits();
    let edit = match action {
        Action::Undo => {
            edits.pop();
            menu.save(&edits);
            return;
        }
        Action::Reset => {
            menu.save(&[]);
            s.settings.scroll = 0.;
            return;
        }
        Action::Add => {
            let name: String = editor.name.text.trim().chars().take(48).collect();
            if name.is_empty() {
                return;
            }
            editor.name.set("");
            Edit::Group {
                id: format!("@{}", edits.len()),
                name,
            }
        }
        Action::Toggle(id, hidden) => Edit::Visibility { id, hidden },
        Action::Move(id, parent) => {
            if !parent.is_empty() {
                editor.expanded.insert(parent.clone());
            }
            Edit::Move {
                id,
                parent,
                before: None,
            }
        }
        Action::Dissolve(id) => Edit::Dissolve { id },
        Action::MovePicker(id) => {
            let mut tree = Tree::new(menu.catalog(s));
            tree.replay(&edits);
            let mut items = vec![pick("First menu", Action::Move(id.clone(), String::new()))];
            destinations(&tree.rows, &id, "", &mut items);
            return picker(s, at, items);
        }
        Action::Reorder(id, dir) => {
            let mut tree = Tree::new(menu.catalog(s));
            tree.replay(&edits);
            let Some(edit) = sibling_move(&tree.rows, &id, dir, "") else {
                return;
            };
            edit
        }
        _ => return,
    };
    edits.push(edit);
    menu.save(&edits);
}
