// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Browser behaviour: tabs, tree open/close, selection, scrolling and drag and drop into the project, and the menu
//! bar's ways in.
// spec: ui/browser.md, ui/browser-commands.md

use os_model::{ChannelKind, ClipSource};

use crate::app::{Action, App, Drag, Win};
use crate::playlist::{Cmd, Mark, PItem, Popup};
use crate::playlist::{self as pl, PickKind};
use crate::views::browser::{self as view, Item};

/// Mouse travel before a press on an item becomes a drag (spec §3).
const DRAG_START: f32 = 4.0;
/// How long a clicked sample plays (spec §7).
pub(crate) const PREVIEW_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

impl App {
    pub(crate) fn browser_auto_hide(&mut self,x:f32,y:f32) {
        let s=&mut self.state;
        let enabled=!s.native_detached.contains(&crate::app::Win::Browser) && s.show_browser && view::opt(s,view::OPT_AUTOHIDE);
        let width=s.browser_w.min(s.screen.0/2.).max(crate::views::BROWSER_W);
        let hidden=if !enabled {false} else if s.browser_auto_hidden {!(x<=8. && y>=crate::views::work_y(s))}
            else {s.browser_hovered && x>width && s.playlist.popup.is_none() && !s.browser_search_focus && !s.feature_votes.focus && s.feature_votes.forum.focus==0 && self.drag.is_none()};
        if x<=width && y>=crate::views::work_y(s) {s.browser_hovered=true;}
        if hidden {s.browser_hovered=false;}
        if hidden!=s.browser_auto_hidden {s.browser_auto_hidden=hidden;self.invalidate();}
    }

    /// Browser actions; returns whether `a` was one.
    pub(crate) fn browser_act(&mut self, a: &Action) -> bool {
        if let Action::BrowserPreview(cmd)=*a {crate::browser_preview::act(self,cmd);return true;}
        if *a == Action::BrowserScroll {
            if let Some((_, thumb)) = view::scrollbar(&self.state) {
                let y = self.mouse.1;
                let offset = if y >= thumb.y && y <= thumb.bottom() { y - thumb.y } else { thumb.h / 2. };
                self.browser_scroll_drag(y, offset);
                self.drag = Some(Drag::BrowserScroll { offset });
            }
            return true;
        }
        if *a == Action::BrowserSplitter {
            self.browser_split_press();
            return true;
        }
        if let Action::CommunityVote(id)=a {self.state.browser_sel=Some(id.clone());crate::community_samples::control(self,3);return true;}
        if let Action::Community(cmd)=*a {crate::community_samples::control(self,cmd);return true;}
        if let Action::FeatureVote(i,cmd)=*a {crate::feature_votes::act(self,i,cmd);return true;}
        if let Action::StarterPack(cmd)=*a {crate::starter_pack::act(self,cmd);return true;}
        if let Action::PluginDownload(i,cmd)=*a {crate::plugin_downloads::act(self,i,cmd);return true;}
        if matches!(a,Action::BrowserTab(_)|Action::BrowserCollapse) {crate::browser_preview::dismiss(self);}
        if *a == Action::BrowserTab(view::EXCHANGE_BUTTON) {
            crate::community_samples::refresh(&mut self.state);
        }
        if *a == Action::BrowserTab(view::PARAMETERS_TAB) { self.params_tick(); }
        let s = &mut self.state;
        match *a {
            Action::BrowserTab(i) => {
                s.browser_tab = if i <= view::EXCHANGE_BUTTON { i } else { 0 };
                s.browser_auto_hidden=false;
                if let Some(label)=match i {1=>Some("Current project"),2=>Some("Plugin database"),4=>Some("Plugin presets"),_=>None} {s.browser_open.insert(format!("{i}:{label}"));}
                s.browser_search.clear();
                s.browser_sel = None;s.browser_samples.clear();s.browser_sample_anchor=None;
                s.browser_scroll = 0.0;
            }
            Action::BrowserCollapse => {
                let tab = format!("{}:", s.browser_tab);
                s.browser_open.retain(|k| !k.starts_with(&tab));
                s.browser_scroll = 0.0;
            }
            Action::BrowserReread => reread(s),
            Action::BrowserList => {}
            Action::BrowserMenu => {
                let (x, y) = view::menu_at(s);
                let open = s.playlist.popup.as_ref().is_some_and(|p| p.x == x && p.y == y);
                s.playlist.popup = if open { None } else { Some(Popup { x, y, items: options_menu(s), sub: None, scroll: [0; 2], cascade: Vec::new() }) };
            }
            Action::BrowserSearch => s.browser_search_focus = true,
            Action::BrowserRow(i) => {
                let Some(mut row) = view::rows(s).into_iter().nth(i) else { return true };
                if matches!(s.browser_tab,0|3) && view::opt(s,view::OPT_SINGLE_FOLDER) && row.path.as_ref().is_some_and(|p|p.is_dir()) {
                    s.browser_single_folder=row.path.clone();s.browser_search.clear();s.browser_scroll=0.;s.browser_sel=None;
                    *s.browser_index.borrow_mut()=Default::default();self.invalidate();return true;
                }
                if row.open.is_some()&&row.key.starts_with("search:@") {
                    if let Some(path)=&row.path {
                        let roots=if s.browser_tab==3{favorites()}else{view::library_roots(s)};
                        if let Some(root)=roots.iter().filter(|root|path.starts_with(root)).max_by_key(|root|root.components().count()) {
                            let mut key=if s.browser_tab==3{format!("3:@{}",root.display())}else{view::disk_key(root)};
                            s.browser_open.insert(key.clone());
                            if let Ok(relative)=path.strip_prefix(root){for part in relative.components(){key.push('/');key.push_str(&part.as_os_str().to_string_lossy());s.browser_open.insert(key.clone());}}
                            s.browser_search.clear();row.key=key;s.browser_open.remove(&row.key);s.browser_scroll=0.;
                        }
                    }
                }
                let selection=sample_selection(s,&row,self.ctrl,self.shift);
                select(s, &row.key);
                s.browser_samples=selection.0;s.browser_sample_anchor=selection.1;
                if row.open.is_some() && !s.browser_open.remove(&row.key) {
                    s.browser_open.insert(row.key.clone());
                    if let Some(path)=&row.path {s.browser_dirs.borrow_mut().remove(path);}
                    *s.browser_index.borrow_mut()=Default::default();
                }
                match row.item {
                    Item::Pattern(n) => {
                        s.pattern = n;
                    }
                    Item::Channel(c) => {
                        s.selected_channel = c;
                        s.rack.select_only(c);
                    }
                    Item::History(e) => s.message = crate::history::go_to(s, e),
                    _ => {}
                }
                if let Item::Sample(path) = &row.item {
                    if s.browser_tab==view::EXCHANGE_BUTTON && !std::path::Path::new(path).is_file() {
                        crate::community_samples::fetch(s,&row.key);
                        crate::browser_preview::dismiss(self);
                    } else { self.preview_sample(path); }
                } else if let Item::Preset(path)=&row.item {
                    let preview=crate::presets::preview_path(std::path::Path::new(path));
                    if preview.is_file(){self.preview_sample(&preview.to_string_lossy());}else{crate::browser_preview::dismiss(self);}
                } else {crate::browser_preview::dismiss(self);}
                let s = &mut self.state;
                s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
                let midi=matches!(&row.item,Item::File(path) if crate::midi_file::is_midi_path(std::path::Path::new(path)));
                if midi || !matches!(row.item, Item::None | Item::File(_) | Item::History(_)) {
                    let mut item=row.item;let mut label=row.label;
                    if matches!(item,Item::Sample(_)) && s.browser_samples.contains(&row.key) && s.browser_samples.len()>1 {
                        let paths:Vec<_>=view::rows(s).into_iter().filter(|r|s.browser_samples.contains(&r.key)).filter_map(|r|if let Item::Sample(p)=r.item{Some(p)}else{None}).collect();
                        if paths.len()>1{label=format!("{} samples",paths.len());item=Item::Samples(paths);}
                    }
                    self.drag = Some(Drag::Browser { item, label, m0: self.mouse, started: false });
                }
            }
            _ => return false,
        }
        self.invalidate();
        true
    }

    pub(crate) fn browser_scroll_drag(&mut self, y: f32, offset: f32) {
        if let Some((track, thumb)) = view::scrollbar(&self.state) {
            let travel = track.h - thumb.h;
            if travel > 0. {
                self.state.browser_scroll = ((y - offset - track.y) / travel).clamp(0., 1.) * view::max_scroll(&self.state);
                self.invalidate();
            }
        }
    }

    fn browser_sample_step(&mut self, down: bool) {
        let rows = view::rows(&self.state);
        if rows.is_empty() { return; }
        let current = rows.iter().position(|r| Some(&r.key) == self.state.browser_sel.as_ref());
        let i = match current { Some(i) if down => (i + 1).min(rows.len() - 1), Some(i) => i.saturating_sub(1), None if down => 0, None => rows.len() - 1 };
        if current == Some(i) { return; }
        let row = &rows[i];
        select(&mut self.state, &row.key);
        if let Item::Sample(path) = &row.item {
            if self.state.browser_tab==view::EXCHANGE_BUTTON && !std::path::Path::new(path).is_file(){crate::community_samples::fetch(&mut self.state,&row.key);}else{self.preview_sample(path);}
        }
        else if let Item::Preset(path)=&row.item {
            let preview=crate::presets::preview_path(std::path::Path::new(path));
            if preview.is_file(){self.preview_sample(&preview.to_string_lossy());}else{crate::browser_preview::dismiss(self);}
        }else { crate::browser_preview::dismiss(self); }
        reveal(&mut self.state);
        self.invalidate();
    }

    /// Clicking a sample plays it once (spec §7): cut off after `PREVIEW_LIMIT` unless "Full sample preview" is ticked.
    pub(crate) fn preview_sample(&mut self, path: &str) {
        crate::browser_preview::select(self,path);
    }

    /// A browser options-menu command (spec §4); returns a message.
    pub(crate) fn browser_cmd(&mut self, c: BCmd) -> Option<String> {
        self.invalidate();
        let s = &mut self.state;
        match c {
            BCmd::Detach => {
                if s.native_detached.contains(&crate::app::Win::Browser) {
                    s.native_detached.retain(|w| *w != crate::app::Win::Browser);
                } else {
                    s.native_detached.push(crate::app::Win::Browser);
                }
                s.browser_auto_hidden = false;
                s.show_browser = true;
                s.playlist.popup = None;
            }
            BCmd::CommunityCategory(index,kind,upload) => crate::community_samples::set_category(s,index,kind,upload),
            BCmd::CommunityGenre(index,upload) => crate::community_samples::set_genre(s,index,upload),
            BCmd::CommunityControl(cmd) => crate::community_samples::control(self,cmd),
            BCmd::SampleExchange => {
                self.browser_act(&Action::BrowserTab(view::EXCHANGE_BUTTON));
            }
            BCmd::StarterPack => crate::starter_pack::act(self,1),
            BCmd::AddFolder => {let row=s.browser_folders.len();self.search_folder_pick(row);}
            BCmd::ConfigureFolders => self.open_settings(crate::views::settings::FILE),
            BCmd::Find(dir) => return find(s, dir),
            BCmd::Toggle(k) => {
                if k==view::OPT_TREE_PLUGINS {s.browser_open.insert("0:Plugin database".into());}
                if !s.browser_opts.remove(k) {
                    s.browser_opts.insert(k);
                }
                crate::prefs::set_text(&format!("browser_option_{k}"),if view::opt(s,k) {"1"}else{"0"});
                if !view::opt(s,view::OPT_AUTOHIDE) {s.browser_auto_hidden=false;}
                if [(view::OPT_PROJECT,1),(view::OPT_PLUGINS,2),(view::OPT_PRESETS,4),(view::OPT_PARAMETERS,view::PARAMETERS_TAB)].iter().any(|(key,id)|*key==k && *id==s.browser_tab && !view::opt(s,k)) {s.browser_tab=0;}
                s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
            }
            BCmd::History(dir) => {
                let pos = s.browser_hist_pos as isize + dir;
                let Some(key) = usize::try_from(pos).ok().and_then(|p| s.browser_hist.get(p)).cloned() else {
                    return Some("No more browser history".into());
                };
                s.browser_hist_pos = pos as usize;
                s.browser_sel = Some(key);
                reveal(s);
            }
            BCmd::Send(dir) => return send(s, dir),
            BCmd::Collapse => {
                let tab = format!("{}:", s.browser_tab);
                s.browser_open.retain(|k| !k.starts_with(&tab));
                s.browser_scroll = 0.0;
            }
            BCmd::Reread => reread(s),
            // A background scan (spec: ui/plugin-manager.md §4); plugins in use stay where they are.
            BCmd::Rescan => self.start_scan(crate::plugin_db::Mode::Changed),
            BCmd::Row(c) => return self.browser_row_cmd(c),
        }
        None
    }

    /// A way into the browser from the menu bar or its keys (spec: ui/browser-commands.md): the browser is shown first
    /// when hidden.
    pub(crate) fn browser_jump(&mut self, j: Jump) {
        self.state.show_browser = true;
        self.state.browser_auto_hidden = false;
        let s = &mut self.state;
        match j {
            // §1: the Current project tab with that section alone open, and selected. The search is cleared so that it
            // shows (as Browse parameters does).
            Jump::Section(name) => {
                let Some(key) = view::section_key(name) else { return };
                if s.browser_tab != view::PROJECT_TAB {
                    s.browser_tab = view::PROJECT_TAB;
                    s.browser_scroll = 0.0;
                }
                s.browser_search.clear();
                for other in view::sections().filter(|&l| l != name).filter_map(view::section_key) {
                    s.browser_open.remove(&other);
                }
                s.browser_open.insert(view::project_root());
                s.browser_open.insert(key.clone());
                select(s, &key);
                reveal(s);
            }
            // §1-2: the tab as it was left, its open nodes and selection kept.
            Jump::PluginDb => {
                if s.browser_tab != view::PLUGIN_TAB {
                    s.browser_tab = view::PLUGIN_TAB;
                    s.browser_scroll = 0.0;
                }
            }
            // §3: exactly as a click in the search field.
            Jump::Find => {
                self.browser_act(&Action::BrowserSearch);
            }
            // §4-5: Patterns opened, the current pattern's row selected when listed (else the section's).
            Jump::Patterns => {
                let Some(key) = view::section_key("Patterns") else { return };
                if s.browser_tab != view::PROJECT_TAB {
                    s.browser_tab = view::PROJECT_TAB;
                    s.browser_scroll = 0.0;
                }
                s.browser_search.clear();
                s.browser_open.insert(view::project_root());
                s.browser_open.insert(key.clone());
                let cur = view::Item::Pattern(s.pattern);
                let row = view::rows(s).into_iter().find(|r| r.item == cur).map(|r| r.key);
                select(s, row.as_ref().unwrap_or(&key));
                reveal(s);
            }
        }
        self.invalidate();
    }

    /// Whether the mouse is over the docked browser.
    pub(crate) fn over_browser(&self) -> bool {
        let (x, y) = self.mouse;
        self.state.show_browser && (self.state.native_view.is_none() || self.state.native_view == Some(crate::app::Win::Browser)) && x >= 0. && y < self.state.screen.1 && x < crate::views::browser_w(&self.state) && y >= crate::views::work_y(&self.state)
    }

    /// Whether typed letters belong to the browser search (the typing keyboard leaves them alone).
    pub(crate) fn browser_takes_typing(&self) -> bool {
        crate::community_samples::editing(&self.state) || self.state.browser_search_focus || (view::opt(&self.state, view::OPT_TYPE) && self.over_browser())
    }

    /// Browser keys (spec §4, §5); returns whether the key was used.
    pub(crate) fn browser_key(&mut self, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if self.state.browser_tag_edit.is_some() {
            match key {
                Key::Named(NamedKey::Escape)=>{self.state.browser_tag_edit=None;self.state.browser_search_focus=false;}
                Key::Named(NamedKey::Enter)=>{
                    let (path,text)=self.state.browser_tag_edit.take().unwrap();
                    let mut tags:Vec<String>=Vec::new();
                    for name in text.split(',').map(str::trim).filter(|t|!t.is_empty()){
                        if !tags.iter().any(|t|t.eq_ignore_ascii_case(name)){tags.push(name.into());}
                    }
                    if tags.is_empty(){self.state.browser_tags.remove(&path);}else{self.state.browser_tags.insert(path,tags);}
                    save_tags(&self.state);self.state.browser_search_focus=false;
                }
                Key::Named(NamedKey::Backspace)=>{self.state.browser_tag_edit.as_mut().unwrap().1.pop();}
                Key::Character(c) if !ctrl && !alt=>{let v=&mut self.state.browser_tag_edit.as_mut().unwrap().1;if v.len()<1024{v.push_str(c);}}
                Key::Named(NamedKey::Space)=>{self.state.browser_tag_edit.as_mut().unwrap().1.push(' ');}
                _=>{}
            }
            self.invalidate();return true;
        }
        let shift = self.shift;
        let focus = self.state.browser_search_focus;
        let over = self.over_browser() || focus;
        if over && !ctrl && !alt && !shift && matches!(key, Key::Named(NamedKey::ArrowRight)) {
            let rows = view::rows(&self.state);
            if let Some(Item::Sample(path)) = rows.iter().find(|r| Some(&r.key) == self.state.browser_sel.as_ref()).map(|r| &r.item) {
                crate::browser_preview::retrigger(self, path);
                return true;
            }
        }
        if over && !ctrl && !alt && !shift && matches!(self.state.browser_tab, 0 | 3 | view::EXCHANGE_BUTTON) {
            if let Key::Named(k @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) = key {
                self.browser_sample_step(*k == NamedKey::ArrowDown);
                return true;
            }
        }
        let cmd = match key {
            Key::Named(NamedKey::F2) if !self.state.browser_search.is_empty() => Some(BCmd::Find(-1)),
            Key::Named(NamedKey::F3) if !self.state.browser_search.is_empty() => Some(BCmd::Find(1)),
            Key::Named(NamedKey::ArrowUp) if over && ctrl => Some(BCmd::Collapse),
            Key::Named(NamedKey::ArrowUp) if over && shift => Some(BCmd::Send(-1)),
            Key::Named(NamedKey::ArrowDown) if over && shift => Some(BCmd::Send(1)),
            Key::Character(c) if over && ctrl && c.eq_ignore_ascii_case("r") => Some(BCmd::Reread),
            _ => None,
        };
        if let Some(c) = cmd {
            if let Some(m) = self.browser_cmd(c) {
                self.state.message = Some(m);
            }
            return true;
        }
        let s = &mut self.state;
        if focus {
            match key {
                Key::Named(NamedKey::Escape) => {
                    s.browser_search.clear();
                    s.browser_search_focus = false;
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(m) = find(s, 1) {
                        s.message = Some(m);
                    }
                }
                Key::Named(NamedKey::Backspace) => {
                    s.browser_search.pop();
                }
                Key::Named(NamedKey::Space) => s.browser_search.push(' '),
                Key::Character(c) if !ctrl && !alt => s.browser_search.push_str(c),
                _ if ctrl || alt => return true,
                _ => return false,
            }
            s.browser_scroll = 0.0;
            self.invalidate();
            return true;
        }
        // "Type to filter": letters typed over the browser start a search.
        if let Key::Character(c) = key {
            if !ctrl && !alt && view::opt(s, view::OPT_TYPE) && self.over_browser() {
                let s = &mut self.state;
                s.browser_search_focus = true;
                s.browser_search.push_str(c);
                s.browser_scroll = 0.0;
                self.invalidate();
                return true;
            }
        }
        false
    }

    /// A press on the browser's splitter: drags it; a double click gives the browser its default width (spec §9).
    fn browser_split_press(&mut self) {
        let now = std::time::Instant::now();
        let s = &mut self.state;
        let double = s.browser_split_click.is_some_and(|t| now.duration_since(t).as_millis() < 400);
        s.browser_split_click = if double { None } else { Some(now) };
        if double {
            s.browser_w = crate::views::BROWSER_W;
            save_width(s.browser_w);
            return self.invalidate();
        }
        self.drag = Some(Drag::BrowserSplit { dx: self.mouse.0 - crate::views::browser_w(&self.state) });
    }

    /// The splitter dragged so that the browser (its splitter included) ends at `right`: whole px, from its default width
    /// to half the window. Everything right of it moves, so the whole frame is repainted.
    pub(crate) fn browser_split_drag(&mut self, right: f32) {
        let s = &mut self.state;
        let w = right.min(s.screen.0 / 2.0).max(crate::views::BROWSER_W).round();
        if w != s.browser_w {
            s.browser_w = w;
            self.invalidate();
        }
    }

    /// A hovered row's scrolling label (spec §9): repainted whenever its offset moves; the scroll ends with the hover.
    pub(crate) fn marquee_tick(&mut self, now: std::time::Instant) {
        let hovered = self.state.show_browser && matches!(self.hover, Some(Action::BrowserRow(_)));
        let m = self.state.browser_marquee.get_mut();
        let Some(mq) = m.as_ref() else { return };
        if !hovered {
            *m = None;
            return;
        }
        let r = mq.rect;
        if view::marquee_offset(now.duration_since(mq.since).as_secs_f32(), mq.over) != mq.off {
            self.damage_rect(r);
        }
    }

    /// Wheel over the browser list.
    pub(crate) fn browser_wheel(&mut self, dy: f32) {
        let s = &mut self.state;
        s.catalog_hover=None; s.catalog_tooltip=None;
        s.browser_scroll = (s.browser_scroll - dy * 3.0 * view::row_h(s)).clamp(0.0, view::max_scroll(s));
        self.invalidate();
    }

    /// Mouse moved while an item is pressed: start the drag, move the label.
    pub(crate) fn browser_drag_move(&mut self, label: String, m0: (f32, f32), started: bool, item: Item) {
        let (x, y) = self.mouse;
        if !started && (x - m0.0).hypot(y - m0.1) < DRAG_START {
            return;
        }
        if !self.state.shown(Win::Playlist) && matches!(item,Item::Sample(_) | Item::Samples(_) | Item::File(_))
            && self.top_hit(x,y).is_some_and(|h|h.action==Action::ToggleWin(Win::Playlist)) {
            self.state.raise(Win::Playlist);
            self.invalidate();
        }
        let paths=match &item{Item::Sample(p)=>vec![p.clone()],Item::Samples(p)=>p.clone(),_=>Vec::new()};
        if !paths.is_empty(){self.state.sample_drop.start(paths,false,self.state.native_view);crate::drop_preview::poll(self);}
        if !started {
            self.drag = Some(Drag::Browser { item, label: label.clone(), m0, started: true });
        }
        if let Some((_, ox, oy)) = self.state.browser_drag {
            self.damage_rect(view::ghost_rect(ox, oy));
        }
        self.state.browser_drag = Some((label, x, y));
        self.damage_rect(view::ghost_rect(x, y));
    }

    /// Release: drop the dragged item on whatever is under the mouse (spec §3).
    pub(crate) fn browser_drop(&mut self, item: Item, started: bool) {
        self.state.browser_drag = None;
        self.state.sample_drop.clear();
        self.invalidate();
        if !started {
            if !self.ctrl&&!self.shift {let key=self.state.browser_sel.clone();self.state.browser_samples.retain(|k|Some(k)==key.as_ref());}
            return;
        }
        // MIDI files use the same destination-aware import as external drops.
        if let Item::File(path)=&item {
            if crate::midi_file::is_midi_path(std::path::Path::new(path)) { return self.drop_file(path.into()); }
        }
        // A preset goes into the plugin under the mouse, or a new instance of its plugin (spec: plugins/presets.md).
        if let Item::Preset(path) = &item {
            self.state.message = crate::presets::dropped(self, std::path::Path::new(path));
            return;
        }
        // Plugin windows float above the main window: a sample released over one goes into that plugin.
        #[cfg(any(windows, target_os = "macos"))]
        if let Item::Sample(path) = &item {
            if self.sample_to_plugin_window(path) {
                return;
            }
        }
        let (x, y) = self.mouse;
        let win = self.top_hit(x, y).and_then(|h| h.win);
        let grid = self.pl_grid().filter(|g| g.contains(x, y) && win == Some(Win::Playlist));
        let alt = self.alt;
        let s = &mut self.state;
        // Snapped playlist position under the mouse.
        let spot = grid.map(|grid| {
            let g = pl::PGeom::new(s, grid);
            (g.track_at(y), pl::snap_place(s, &g, g.tick_at(x), alt))
        });
        match item {
            Item::Pattern(n) => {
                if let Some((track, tick)) = spot {
                    pl::snapshot(s);
                    s.playlist.source = Some(ClipSource::Pattern(n));
                    s.playlist.picker = PickKind::Pattern;
                    let i = pl::place(s, track, tick);
                    s.playlist.sel = vec![i];
                } else if matches!(win, Some(Win::PianoRoll | Win::ChannelRack)) {
                    s.pattern = n;
                }
            }
            Item::Channel(c) => {
                let Some(ch) = s.project.channels.get(c) else { return };
                let (kind, index) = (ch.kind, ch.index);
                if let Some((track, tick)) = spot {
                    if matches!(kind, ChannelKind::AudioClip | ChannelKind::Automation) {
                        pl::snapshot(s);
                        s.playlist.source = Some(ClipSource::Channel(index));
                        s.playlist.picker = if kind == ChannelKind::AudioClip { PickKind::Audio } else { PickKind::Automation };
                        let i = pl::place(s, track, tick);
                        s.playlist.sel = vec![i];
                    } else {
                        s.message = Some("Only audio and automation channels go on the playlist; drop a pattern instead".into());
                    }
                } else if matches!(win, Some(Win::PianoRoll | Win::ChannelRack)) {
                    s.selected_channel = c;
                    s.rack.select_only(c);
                }
            }
            Item::Samples(paths)=>{
                if let Some((track,tick))=spot{self.import_stems(paths.into_iter().map(Into::into).collect(),track,tick);}
                else if win==Some(Win::ChannelRack){for path in paths{self.sample_to_channel(path,None);}}
            }
            Item::Sample(path) => {
                if let Some((track, tick)) = spot {
                    if let Some(id)=crate::community_samples::pending(s,&path){
                        s.community.drops.push((id.clone(),track,tick));
                        crate::community_samples::fetch(s,&id);
                        return;
                    }
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_string();
                    let added = pl::import_audio_clip(s, &path, track, tick);
                    s.message = Some(match added {
                        Ok(_) => format!("Added audio clip {name}"),
                        Err(reason) => format!("{reason}: {name}"),
                    });
                    // KNAY Key match: offer to shift it into the beat's key.
                    if let Ok(i) = added {
                        crate::key_match::after_drop(self, i);
                    }
                } else if win == Some(Win::ChannelRack) {
                    self.sample_to_rack(path, y);
                }
            }
            // As if picked in the F8 menu (spec: ui/plugin-menu.md §5): a new channel at the bottom of the rack.
            Item::Generator(i) => {
                if spot.is_some() || win == Some(Win::ChannelRack) {
                    if let Some(m) = crate::plugin_menu::add_generator(self, i, None) {
                        self.state.message = Some(m);
                    }
                }
            }
            // An effect dropped on the mixer goes into the slot under the mouse, else the current track's first free
            // slot (spec §3).
            Item::Effects(indices)=>{
                if win==Some(Win::Mixer){
                    let target=self.hits.iter().rev().find_map(|h|if h.rect.contains(x,y){match h.action{Action::Mix(crate::mixer::MHit::Strip(t)|crate::mixer::MHit::TrackSlot(t,_))=>Some(t),_=>None}}else{None});
                    if let Some(t)=target{crate::mixer::select(&mut self.state,t,false);}
                    let slot=match self.top_hit(x,y).map(|h|&h.action){Some(Action::Mix(crate::mixer::MHit::SlotName(k)|crate::mixer::MHit::SlotArrow(k)))=>Some(*k),Some(Action::Mix(crate::mixer::MHit::TrackSlot(t,k)))=>{let (t,k)=(*t,*k);crate::mixer::select(&mut self.state,t,false);Some(k)},_=>None};
                    self.state.message=crate::plugin_menu::add_effects(self,&indices,slot);
                }
            }
            Item::Effect(i) => {
                if win == Some(Win::Mixer) {
                    // A picker/database drop on a strip belongs to that track, even when another is selected.
                    if let Some(track)=self.hits.iter().rev().find_map(|h| {
                        if h.rect.contains(x,y) { if let Action::Mix(crate::mixer::MHit::Strip(t))=h.action {return Some(t);} } None
                    }) {crate::mixer::select(&mut self.state,track,false);}
                    let slot = match self.top_hit(x, y).map(|h| &h.action) {
                        Some(Action::Mix(crate::mixer::MHit::SlotName(k) | crate::mixer::MHit::SlotArrow(k))) => Some(*k),
                        _ => None,
                    };
                    if let Some(m) = crate::plugin_menu::add_effect(self, i, slot) {
                        self.state.message = Some(m);
                    }
                }
            }
            Item::Param(key, index) => {
                if let Some((track, tick)) = spot {
                    let msg = self.params_drop(key, index, track, tick);
                    self.state.message = msg;
                }
            }
            Item::None | Item::File(_) | Item::History(_) | Item::Preset(_) => {}
        }
    }

    /// A folder dropped on the window becomes an All-tab root, opened and selected (spec §6).
    pub(crate) fn locate_browser_sample(&mut self, path: std::path::PathBuf) {
        if !path.is_file() {
            self.state.message = Some(format!("Sample file unavailable: {}", path.display()));
            self.invalidate();
            return;
        }
        let Some(folder) = path.parent() else { return };
        let s = &mut self.state;
        s.show_browser = true;
        s.browser_auto_hidden = false;
        s.browser_tab = 0;
        s.browser_search.clear();
        if view::opt(s, view::OPT_SINGLE_FOLDER) {
            s.browser_single_folder = Some(folder.into());
        }
        let roots = view::library_roots(s);
        let root = roots
            .into_iter()
            .filter(|r| path.starts_with(r))
            .max_by_key(|r| r.components().count())
            .unwrap_or_else(|| {
                let p = folder.to_owned();
                if !s.browser_folders.contains(&p) {
                    s.browser_folders.push(p.clone());
                }
                p
            });
        let base = if view::opt(s, view::OPT_SINGLE_FOLDER) {
            folder
        } else {
            root.as_path()
        };
        let mut key = view::disk_key(base);
        if let Ok(relative) = path.strip_prefix(base) {
            for part in relative.components() {
                s.browser_open.insert(key.clone());
                key.push('/');
                key.push_str(&part.as_os_str().to_string_lossy());
            }
        }
        *s.browser_index.borrow_mut() = Default::default();
        s.browser_dirs.borrow_mut().remove(folder);
        s.browser_sel = Some(key);
        reveal(s);
        self.invalidate();
    }

    pub(crate) fn add_browser_folder(&mut self, path: std::path::PathBuf) {
        let s = &mut self.state;
        if !s.browser_folders.contains(&path) {
            s.browser_folders.push(path.clone());
            save_folders(s);
        }
        s.browser_tab = 0;
        s.browser_search.clear();
        let key = view::disk_key(&path);
        s.browser_open.insert(key.clone());
        select(s, &key);
        reveal(s);
        s.message = Some(format!("Browser: added {}", path.display()));
        self.invalidate();
    }

    /// Right click on row `i` (spec §8): selects it and opens its menu at the mouse; returns whether there was a row.
    pub(crate) fn browser_row_menu(&mut self, i: usize, x: f32, y: f32) -> bool {
        let s = &mut self.state;
        let Some(row) = view::rows(s).into_iter().nth(i) else { return false };
        select(s, &row.key);
        s.playlist.popup = Some(Popup { x, y, items: row_menu(s, &row), sub: None, scroll: [0; 2], cascade: Vec::new() });
        self.invalidate();
        true
    }

    /// Send the clicked sample, preset or MIDI file to the current instrument.
    pub(crate) fn browser_row_middle(&mut self, i: usize) -> bool {
        let Some(row) = view::rows(&self.state).into_iter().nth(i) else { return false };
        select(&mut self.state, &row.key);
        match row.item {
            Item::Sample(_) => self.state.message = self.browser_row_cmd(RowCmd::Send),
            Item::Preset(path) => {
                let target = sample_target(&self.state);
                self.state.message = if self.plugins.hosted.contains_key(&target) {
                    crate::presets::load_into(self, target, std::path::Path::new(&path))
                } else {
                    Some("Select a compatible plugin to load this preset".into())
                };
            }
            Item::File(path) if crate::midi_file::is_midi_path(std::path::Path::new(&path)) => {
                if crate::piano::channel(&self.state).is_some() {
                    self.midi_drop(path.into(), true, None);
                } else {
                    self.state.message = Some("Select a channel to receive these MIDI notes".into());
                }
            }
            _ => {}
        }
        self.invalidate();
        true
    }

    /// A row menu command on the selected row (spec §8); returns a message.
    fn browser_row_cmd(&mut self, c: RowCmd) -> Option<String> {
        let s = &mut self.state;
        let row = view::rows(s).into_iter().find(|r| s.browser_sel.as_ref() == Some(&r.key))?;
        let sample = match &row.item {
            Item::Sample(p) => Some(p.clone()),
            _ => None,
        };
        match c {
            RowCmd::Toggle => {
                if !s.browser_open.remove(&row.key) {
                    s.browser_open.insert(row.key);
                }
                s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
            }
            // What a click on the row does, without the click's open / close.
            RowCmd::Activate => match row.item {
                Item::Pattern(n) => s.pattern = n,
                Item::Channel(c) => {
                    s.selected_channel = c;
                    s.rack.select_only(c);
                }
                Item::History(e) => return crate::history::go_to(s, e),
                _ => {}
            },
            RowCmd::Pattern(pc) => {
                let Item::Pattern(n) = row.item else { return None };
                s.pattern = n;
                return self.pattern_cmd(pc);
            }
            RowCmd::ChannelWindow => {
                let Item::Channel(i) = row.item else { return None };
                if s.hosted_rows.contains(&i) {
                    s.pending_open = Some(i);
                } else {
                    self.open_channel_window(i);
                }
            }
            RowCmd::Preview => self.preview_sample(&sample?),
            RowCmd::Send => {
                let p=sample?;
                let target=sample_target(s);
                if s.project.channels.get(target).is_some_and(|ch|ch.kind.is_sampler()){
                    return send_sample_to(s,target,&p,&row.label);
                }
                let ok=self.plugins.browser_sample_to_plugin(target,&crate::plugins::resolve_sample(&p));
                return Some(if ok {"Sample sent to plugin".into()} else {"The selected plugin did not accept this sample".into()});
            }
            RowCmd::NewChannel => {self.sample_to_channel(sample?,None); if !self.alt {self.open_channel_window(self.state.selected_channel);}}
            RowCmd::Instrument(index) => {
                let path=crate::plugins::resolve_sample(&sample?);
                if let Err(e)=os_engine::sample::load_shared(std::path::Path::new(&path)){return Some(format!("Cannot load sample: {e}"));}
                crate::plugin_menu::add_generator(self,index,None);
                if let Some((row,i))=self.state.pending_plugin {self.state.browser_plugin_sample=Some((row,i,path));}
            }
            RowCmd::AudioEditor => return self.browser_audio_editor(&crate::plugins::resolve_sample(&sample?)),
            RowCmd::PitchEditor => {
                let mut items: Vec<_> = crate::ara::choices(self).iter().enumerate()
                    .map(|(i, editor)| PItem::new(editor.name.clone(), Cmd::Browser(BCmd::Row(RowCmd::PitchWith(i)))))
                    .collect();
                if items.is_empty() { items.push(PItem::head("No ARA pitch editors found")); }
                items.push(PItem::new("Scan for ARA editors...", Cmd::AraScan));
                self.state.playlist.popup = Some(crate::playlist::Popup {
                    x: self.mouse.0, y: self.mouse.1, items, sub: None, scroll: [0; 2], cascade: Vec::new(),
                });
            }
            RowCmd::WarpEditor|RowCmd::PitchWith(_) => {
                let path=crate::plugins::resolve_sample(&sample?);
                let tick=s.playlist.pos;
                let ci=match crate::playlist::import_audio_clip(&mut self.state,&path,0,tick) {Ok(ci)=>ci,Err(e)=>return Some(e.into())};
                self.state.raise(crate::app::Win::Playlist);
                if let RowCmd::PitchWith(index) = c { return crate::ara::choose(self, ci, index); }
                return crate::warp::open(self,ci).err();
            }
            RowCmd::ParentFolder => {let p=std::path::PathBuf::from(crate::plugins::resolve_sample(&sample?));open_folder(p.parent()?);}
            RowCmd::Trash => {
                let path=std::path::PathBuf::from(crate::plugins::resolve_sample(&sample?));
                use crate::dialog::{Kind,Msg,Then,Answer};
                self.show_msg(Msg::new(Kind::Question,"Delete sample",vec![(format!("Move {} to the system trash?",path.display()),false)],&[Answer::Yes,Answer::Cancel],Then::TrashBrowserSample(path)));
            }
            RowCmd::ShellMenu => {
                #[cfg(windows)] {
                    let path=crate::plugins::resolve_sample(&sample?);
                    let result=crate::browser_files::shell_menu(std::path::Path::new(&path));
                    reread(s);
                    return result.err();
                }
            }
            RowCmd::EditTags(add) => {
                let key=crate::plugins::resolve_sample(&sample?);
                let mut text=s.browser_tags.get(&key).map_or_else(String::new,|v|v.join(", "));
                if add && !text.is_empty(){text.push_str(", ");}
                s.browser_tag_edit=Some((key,text));s.browser_search_focus=true;
                return Some("Sample tags: separate with commas; Enter saves, Esc cancels".into());
            }
            RowCmd::ToggleTag(i) => {
                let tag=tag_list(s).get(i)?.clone();let key=crate::plugins::resolve_sample(&sample?);
                let v=s.browser_tags.entry(key).or_default();
                if v.contains(&tag){v.retain(|t|t!=&tag);}else{v.push(tag);}
                save_tags(s);
            }
            RowCmd::OpenFolder => open_folder(&row.path?),
            RowCmd::ShowInFolder => crate::theme_library::show_in_folder(&sample.as_ref().map(|p|std::path::PathBuf::from(crate::plugins::resolve_sample(p))).or(row.path)?),
            // File ▸ Browser extra search folders, the root's Browser name selected for typing.
            RowCmd::Rename => {
                let i = s.browser_folders.iter().position(|f| Some(f) == row.path.as_ref())?;
                self.open_settings(crate::views::settings::FILE);
                self.search_folder_cell(i, crate::search_folders::Col::Name);
            }
            RowCmd::Favorite => {
                let path = sample.as_ref().map(|p|std::path::PathBuf::from(crate::plugins::resolve_sample(p))).or(row.path)?;
                let mut paths = favorites();
                if paths.contains(&path) { paths.retain(|p| p != &path); } else { paths.push(path); }
                crate::prefs::set_text("browser_favorites", &serde_json::to_string(&paths).unwrap_or_default());
                s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
            }
            RowCmd::Remove => return Some(remove_root(s, &row.path?)),
        }
        None
    }
}

pub(crate) fn load_tags()->std::collections::BTreeMap<String,Vec<String>> {
    crate::prefs::text("browser_sample_tags").and_then(|v|serde_json::from_str(&v).ok()).unwrap_or_default()
}
fn save_tags(s:&crate::app::State) {
    if let Ok(text)=serde_json::to_string(&s.browser_tags){crate::prefs::set_text("browser_sample_tags",&text);}
}
fn tag_list(s:&crate::app::State)->Vec<String>{
    s.browser_tags.values().flatten().cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect()
}
impl App {
    fn browser_audio_editor(&mut self,path:&str)->Option<String>{
        let file=std::path::Path::new(path);
        let wav=file.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("wav"));
        let sample=wav.then(||std::fs::read(file).ok().and_then(|b|tesla::wav::read(&b).ok())).flatten().or_else(||{
            let s=os_engine::sample::load_shared(file).ok()?;
            Some(tesla::sample::Sample::from_channels(s.rate,vec![s.left.clone(),s.right.clone()],tesla::sample::Format::Float32))
        });
        let Some(sample)=sample else {return Some("Unable to read this audio file".into())};
        if self.tesla_for_recording(0, |s| { s.sample = sample; s.path = path.into(); }) {
            Some(format!("Opened {} in Tesla", file.display()))
        } else {
            self.state.message.clone()
        }
    }
    pub(crate) fn browser_trash_sample(&mut self,path:&std::path::Path) {
        self.state.message=Some(match crate::browser_files::trash(path) {
            Ok(())=>{
                self.state.browser_tags.remove(&path.to_string_lossy().into_owned());save_tags(&self.state);
                let mut f=favorites();f.retain(|p|p!=path);
                crate::prefs::set_text("browser_favorites",&serde_json::to_string(&f).unwrap_or_default());
                reread(&mut self.state);format!("Moved {} to trash",path.display())
            }
            Err(e)=>e,
        }); self.invalidate();
    }
}

/// A dropped folder root out of the browser (spec §6, §8), with its Browser name and its open nodes.
fn remove_root(s: &mut crate::app::State, path: &std::path::Path) -> String {
    let key = view::disk_key(path);
    s.browser_folders.retain(|f| f != path);
    s.browser_names.remove(path);
    save_folders(s);
    let below = format!("{key}/");
    s.browser_open.retain(|k| *k != key && !k.starts_with(&below));
    s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
    format!("Removed {} from the browser", path.display())
}

/// Opens folder `dir` in the system's file manager (tests: nothing).
fn open_folder(dir: &std::path::Path) {
    if cfg!(test) {
        return;
    }
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

/// `browser.txt` in the user folder: `width=` the browser's width in px (anything else, or missing, = the default).
fn width_file() -> Option<std::path::PathBuf> {
    if cfg!(test) {
        return None;
    }
    os_paths::user_file("browser.txt")
}

fn parse_width(text: &str) -> f32 {
    let w = text.lines().filter_map(|l| l.split_once('=')).find(|(k, _)| k.trim() == "width").and_then(|(_, v)| v.trim().parse::<f32>().ok());
    w.filter(|w| w.is_finite()).map_or(crate::views::BROWSER_W, |w| w.max(crate::views::BROWSER_W))
}

/// The browser's width kept from the last run (spec §9; tests: the default).
pub(crate) fn load_width() -> f32 {
    width_file().and_then(|p| std::fs::read_to_string(p).ok()).map_or(crate::views::BROWSER_W, |t| parse_width(&t))
}

/// Keeps the browser's width for the next run.
pub(crate) fn save_width(w: f32) {
    let Some(p) = width_file() else { return };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, format!("width={w}\n"));
}

/// `browser_folders.txt` in the user folder (`os_paths::user_dir`): one dropped folder per line (spec §6), its Browser
/// name after a tab when it has one (`crate::search_folders::file_text`).
fn folders_file() -> Option<std::path::PathBuf> {
    if cfg!(test) {
        return None;
    }
    os_paths::user_file("browser_folders.txt")
}

fn read_folders_from(path: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    std::fs::read_to_string(path).map(|text|crate::search_folders::parse_file(&text)).unwrap_or_default()
}
fn read_folders() -> Vec<(std::path::PathBuf, String)> {
    folders_file().map(|p|read_folders_from(&p)).unwrap_or_default()
}

pub(crate) fn load_folders() -> Vec<std::path::PathBuf> {
    read_folders().into_iter().map(|f| f.0).collect()
}

/// The dropped folders' Browser names.
pub(crate) fn load_names() -> std::collections::BTreeMap<std::path::PathBuf, String> {
    read_folders().into_iter().filter(|f| !f.1.is_empty()).collect()
}

/// Keeps the dropped folders and their names.
pub(crate) fn save_folders(s: &crate::app::State) {
    let Some(p) = folders_file() else { return };
    let _ = write_folders_to(&p,s);
}
fn write_folders_to(p: &std::path::Path, s: &crate::app::State) -> std::io::Result<()> {
    if let Some(d)=p.parent() {std::fs::create_dir_all(d)?;}
    std::fs::write(p,crate::search_folders::file_text(&s.browser_folders,&s.browser_names))
}

/// The menu bar's ways into the browser (spec: ui/browser-commands.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// VIEW's Browser section: this section of the Current project tree (§1).
    Section(&'static str),
    /// VIEW > Plugin database (Shift+F8), ADD > Browse plugin database: the Plugin database tab (§1, §2).
    PluginDb,
    /// TOOLS > Browser find... (Ctrl+F): the keyboard goes to the search field (§3).
    Find,
    /// PATTERNS > Open in project browser (Shift+Ctrl+P): the Patterns section, the other sections left as they are (§4).
    Patterns,
}

impl Jump {
    /// The jump behind entry `label` of menu `menu`. `None` for other entries, and for a section the tree does not list
    /// (History, Remote control: their entries stay greyed out).
    pub fn from_label(menu: usize, label: &str) -> Option<Jump> {
        use crate::views::menus::{ADD_MENU, TOOLS_MENU, VIEW};
        let j = match (menu, label) {
            (VIEW | crate::views::menus::EDIT_MENU, "Undo history") => Jump::Section("History"),
            (VIEW, "Patterns") => Jump::Section("Patterns"),
            (VIEW, "Generators in use") => Jump::Section("Generators"),
            (VIEW, "Effects in use") => Jump::Section("Effects"),
            (VIEW, "Remote control") => Jump::Section("Remote control"),
            (VIEW, "Plugin database") | (ADD_MENU, "Browse plugin database") => Jump::PluginDb,
            (TOOLS_MENU, "Browser find...") => Jump::Find,
            _ => return None,
        };
        match j {
            Jump::Section(name) => view::section_key(name).map(|_| j),
            _ => Some(j),
        }
    }
}

/// Options-menu commands (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BCmd {
    CommunityCategory(usize,usize,bool),
    CommunityGenre(usize,bool),
    CommunityControl(u8),
    /// Find previous (-1) / next (1).
    AddFolder,
    StarterPack,
    SampleExchange,
    ConfigureFolders,
    Detach,
    Find(isize),
    Toggle(&'static str),
    /// Navigation history back (-1) / forward (1).
    History(isize),
    /// Send previous (-1) / next (1) sample to the selected channel.
    Send(isize),
    Collapse,
    Reread,
    Rescan,
    /// A row menu command (spec §8), on the selected row (the right click selected it).
    Row(RowCmd),
}

/// What a row menu entry does (spec §8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowCmd {
    /// Open / close the node.
    Toggle,
    /// What a click on the row does: the pattern or channel selected, the project taken to the history step.
    Activate,
    /// The row's pattern made current, then this PATTERNS command.
    Pattern(crate::patterns::Cmd),
    /// The channel's window (its plugin's, Lemon Sampler's, or the channel settings).
    ChannelWindow,
    /// A sample played (§7), loaded into the selected channel, or into a new Lemon Sampler channel.
    Preview,
    Send,
    NewChannel,
    AudioEditor, WarpEditor, PitchEditor, PitchWith(usize), Instrument(usize),
    ParentFolder, Trash, EditTags(bool), ToggleTag(usize), ShellMenu,
    /// A disk folder opened in the system's file manager; a disk file shown there, selected in its folder.
    OpenFolder,
    ShowInFolder,
    /// A dropped folder root: its Browser name typed in File ▸ Browser extra search folders; out of the browser.
    Rename,
    Remove,
    Favorite,
}

/// The row menu's way to the system's file manager: a folder, a file.
#[cfg(windows)]
const FILE_MANAGER: [&str; 2] = ["Open in Explorer", "Show in Explorer"];
#[cfg(target_os = "macos")]
const FILE_MANAGER: [&str; 2] = ["Open in Finder", "Show in Finder"];
#[cfg(all(unix, not(target_os = "macos")))]
const FILE_MANAGER: [&str; 2] = ["Open in file manager", "Show in file manager"];

/// A row's menu (spec §8): Open / Close for a node, what the row's kind offers, the file manager for a disk entry, then
/// the structure commands every row has.
pub fn row_menu(s: &crate::app::State, row: &view::Row) -> Vec<PItem> {
    use crate::patterns::Cmd as P;
    use crate::rack::RCmd;
    let r = |label: &'static str, c: RowCmd| PItem::new(label, Cmd::Browser(BCmd::Row(c)));
    let sep = |items: &mut Vec<PItem>| {
        if items.last().is_some_and(|i| !i.sep) {
            items.push(PItem::sep());
        }
    };
    let mut items = Vec::new();
    if let Some(open) = row.open {
        // While searching every node shows open.
        let it = r(if open { "Close" } else { "Open" }, RowCmd::Toggle);
        items.push(if s.browser_search.is_empty() { it } else { it.off() });
    }
    match row.item {
        Item::Pattern(_) => {
            let p = |label: &'static str, c: P| r(label, RowCmd::Pattern(c));
            items.extend([
                r("Select pattern", RowCmd::Activate),
                PItem::sep(),
                p("Rename and color...", P::Rename),
                p("Change color...", P::ChangeColor),
                p("Clone", P::Clone),
                p("Delete", P::Delete),
                PItem::sep(),
                p("Select in playlist", P::SelectInPlaylist),
                p("Split by channel", P::SplitByChannel),
            ]);
        }
        Item::Channel(c) => {
            let k = |label: &'static str, rc: RCmd| PItem::new(label, Cmd::Rack(rc));
            items.extend([
                r("Select channel", RowCmd::Activate),
                r("Show window", RowCmd::ChannelWindow),
                k("Piano roll", RCmd::PianoRoll(c)),
                PItem::sep(),
                k("Rename, color and icon...", RCmd::Rename(c)),
                k("Clone", RCmd::Clone(c)),
                k("Delete...", RCmd::Delete(c)),
            ]);
        }
        Item::Sample(_) => {
            let send = r("Send to selected channel or focused plugin", RowCmd::Send);
            let target=sample_target(s);
            let accepts=s.project.channels.get(target).is_some_and(|c| c.kind.is_sampler()) || s.hosted_rows.contains(&target) || target>=crate::plugins::FX_KEY;
            items.extend([if accepts {send} else {send.off()},r("Open in new channel",RowCmd::NewChannel),
                r("Preview",RowCmd::Preview),PItem::sep(),r("Edit in audio editor (Tesla)",RowCmd::AudioEditor),
                r("Edit in pitch corrector (ARA)...",RowCmd::PitchEditor),r("Edit in time warper...",RowCmd::WarpEditor),
                r("Open in new sampler channel",RowCmd::NewChannel)]);
            for e in &s.plugdb.lists.gens {
                let name=e.name.to_lowercase();
                let chop=name=="lemon chop";
                let ext=cfg!(windows) && ["fruity granulizer","fruity slicer","slicex"].iter().any(|n| name==*n);
                if chop||ext {items.push(PItem::new(format!("Open in new {} channel",e.label),Cmd::Browser(BCmd::Row(RowCmd::Instrument(e.index)))));}
            }
            let mut system=vec![r("Open parent folder",RowCmd::ParentFolder),r("Locate file",RowCmd::ShowInFolder),r("Delete file...",RowCmd::Trash)];
            if cfg!(windows) {system.insert(0,r("Windows shell menu...",RowCmd::ShellMenu));}
            items.push(PItem::sub("System",system));
            if let Item::Sample(ref path)=row.item {
                let key=crate::plugins::resolve_sample(path);
                let tagged=s.browser_tags.get(&key);
                let fav=favorites().contains(&std::path::PathBuf::from(&key));
                let mut tags=vec![r("Favorite",RowCmd::Favorite).mark(crate::playlist::Mark::Check(fav)),PItem::sep()];
                for (i,tag) in tag_list(s).into_iter().enumerate(){
                    tags.push(PItem::new(tag.clone(),Cmd::Browser(BCmd::Row(RowCmd::ToggleTag(i)))).mark(crate::playlist::Mark::Check(tagged.is_some_and(|v|v.contains(&tag)))));
                }
                tags.extend([r("Add tags...",RowCmd::EditTags(true)),r("Edit tags...",RowCmd::EditTags(false))]);
                items.push(PItem::sub("Tags",tags));
            }
        }
        Item::Generator(index) => items.push(PItem::new("Add to channel rack", Cmd::Plugin(crate::plugin_menu::PlugCmd::Generator { index, after: None }))),
        Item::Effect(index) => items.push(PItem::new("Add to selected mixer track", Cmd::Plugin(crate::plugin_menu::PlugCmd::Effect { index, slot: None }))),
        Item::History(_) => items.push(r("Go to this step", RowCmd::Activate)),
        // Into the selected channel's plugin (spec: plugins/presets.md "Browser").
        Item::Preset(ref p) => {
            let key = s.selected_channel;
            let it = PItem::new("Load into selected channel", Cmd::Preset(crate::presets::PCmd::Load(key, p.into())));
            items.push(if s.hosted_rows.contains(&key) { it } else { it.off() });
        }
        Item::None | Item::File(_) | Item::Param(..) | Item::Effects(_) | Item::Samples(_) => {}
    }
    if let Some(path) = row.path.as_ref().filter(|_| !matches!(row.item,Item::Sample(_))) {
        sep(&mut items);
        items.push(r(if favorites().contains(path) { "Remove from Favorites" } else { "Add to Favorites" }, RowCmd::Favorite));
        items.push(if row.open.is_some() { r(FILE_MANAGER[0], RowCmd::OpenFolder) } else { r(FILE_MANAGER[1], RowCmd::ShowInFolder) });
        if s.browser_folders.contains(path) && row.key == view::disk_key(path) {
            items.extend([r("Rename...", RowCmd::Rename), r("Remove from browser", RowCmd::Remove)]);
        }
    }
    sep(&mut items);
    items.extend([
        PItem::new("Collapse structure", Cmd::Browser(BCmd::Collapse)).key("Ctrl+Up"),
        PItem::new("Reread structure", Cmd::Browser(BCmd::Reread)).key("Ctrl+R"),
    ]);
    items
}

/// The browser options menu, from the FL screenshot (spec §4).
pub fn options_menu(s: &crate::app::State) -> Vec<PItem> {
    crate::menu_layout::Menu::Browser.apply(default_options_menu(s))
}

pub(crate) fn default_options_menu(s: &crate::app::State) -> Vec<PItem> {
    let b = |label: &'static str, c: BCmd| PItem::new(label, Cmd::Browser(c));
    let t = |label: &'static str, k: &'static str| b(label, BCmd::Toggle(k)).mark(Mark::Check(view::opt(s, k)));
    let searching=!s.browser_search.is_empty();
    let find=|label,key,dir| {let item=b(label,BCmd::Find(dir)).key(key);if searching {item}else{item.off()}};
    vec![
        #[cfg(windows)]
        b("Community Samples",BCmd::SampleExchange),
        b("Add sample folder...",BCmd::AddFolder),
        PItem::sub("Content",vec![
            t("Single Folder View",view::OPT_SINGLE_FOLDER),
            t("Show generators and effects in Samples",view::OPT_TREE_PLUGINS),
            PItem::sub("Browser tabs",vec![t("Current project",view::OPT_PROJECT),t("Plugin database",view::OPT_PLUGINS),t("Plugin presets",view::OPT_PRESETS),t("Parameters",view::OPT_PARAMETERS)]),
            b("Configure sample folders...",BCmd::ConfigureFolders),
            b("Refresh plugin list",BCmd::Rescan),
            b(if crate::starter_pack::busy(){"Cancel Starter Pack download"}else{"Download Starter Pack"},BCmd::StarterPack),
        ]),
        PItem::sub("Audio preview",vec![t("Show preview panel",view::OPT_PANEL),t("Play full sample / track",view::OPT_PREVIEW)]),
        b(if s.native_detached.contains(&crate::app::Win::Browser) {"Dock Browser"} else {"Detach Browser"}, BCmd::Detach),
        PItem::sub("Appearance",vec![t("Auto-hide browser",view::OPT_AUTOHIDE),t("Search box on top",view::OPT_TOP),t("High contrast text",view::OPT_CONTRAST)]),
        PItem::sub("Search",vec![find("Find previous","F2",-1),find("Find next","F3",1),PItem::sep(),t("Type to filter",view::OPT_TYPE),t("Show parent folders",view::OPT_PARENTS),t("Include folders",view::OPT_FOLDERS)]),
        PItem::sub("Navigation",vec![b("Back",BCmd::History(-1)),b("Forward",BCmd::History(1)),PItem::sep(),b("Collapse folders",BCmd::Collapse).key("Ctrl+Up"),b("Refresh folders",BCmd::Reread).key("Ctrl+R")]),
        PItem::sub("Send to selected channel",vec![b("Previous sample",BCmd::Send(-1)).key("Shift+Up"),b("Next sample",BCmd::Send(1)).key("Shift+Down")]),
    ]
}

/// Reread structure (spec §4): folders are read from disk again; open nodes and the selection stay.
fn reread(s: &mut crate::app::State) {
    s.browser_dirs.borrow_mut().clear();
    *s.browser_index.borrow_mut() = Default::default();
    s.browser_scroll = s.browser_scroll.min(view::max_scroll(s));
}

/// Select a row and record it in the navigation history (spec §4).
pub(crate) fn select(s: &mut crate::app::State, key: &str) {
    s.browser_samples.clear();s.browser_sample_anchor=Some(key.to_string());
    s.browser_sel = Some(key.to_string());
    if s.browser_hist.get(s.browser_hist_pos).map(String::as_str) != Some(key) {
        let keep = if s.browser_hist.is_empty() { 0 } else { s.browser_hist_pos + 1 };
        s.browser_hist.truncate(keep);
        s.browser_hist.push(key.to_string());
        s.browser_hist_pos = s.browser_hist.len() - 1;
    }
}

/// Ctrl+Z with "Show undo history in the browser when undoing changes": the browser shows Current project ▸ History,
/// opened, where the project now is selected (spec: ui/settings-choices.md "Undo history: the four checks
/// (2026-10-16)"). The All tab shows it too; the others switch to the project's.
pub(crate) fn show_history(s: &mut crate::app::State) {
    s.show_browser = true;
    s.browser_auto_hidden = false;
    s.browser_tab = 1;
    s.browser_search.clear();
    let root = format!("{}:Current project", s.browser_tab);
    let folder = format!("{root}/History");
    s.browser_sel = Some(format!("{folder}/{}", s.history.at()));
    s.browser_open.extend([root, folder]);
    reveal(s);
}

/// Scroll the selected row into view.
pub(crate) fn reveal(s: &mut crate::app::State) {
    let rows = view::rows(s);
    let Some(i) = rows.iter().position(|r| Some(&r.key) == s.browser_sel.as_ref()) else { return };
    let y = i as f32 * view::row_h(s);
    let h = view::list_h(s);
    if y < s.browser_scroll {
        s.browser_scroll = y;
    } else if y + view::row_h(s) > s.browser_scroll + h {
        s.browser_scroll = y + view::row_h(s) - h;
    }
    s.browser_scroll = s.browser_scroll.clamp(0.0, view::max_scroll(s));
}

/// Index of the next row after the selection (wrapping) for which `ok` holds.
fn step(s: &crate::app::State, rows: &[view::Row], dir: isize, ok: impl Fn(&view::Row) -> bool) -> Option<usize> {
    let n = rows.len() as isize;
    let cur = rows.iter().position(|r| Some(&r.key) == s.browser_sel.as_ref()).map_or(if dir > 0 { -1 } else { n }, |i| i as isize);
    (1..=n).map(|k| (cur + dir * k).rem_euclid(n.max(1)) as usize).find(|&i| ok(&rows[i]))
}

/// Find previous / next match of the search text.
fn find(s: &mut crate::app::State, dir: isize) -> Option<String> {
    let q = s.browser_search.to_lowercase();
    if q.is_empty() {
        return None;
    }
    let rows = view::rows(s);
    let Some(i) = step(s, &rows, dir, |r| view::matches(s, r, &q)) else {
        return Some(format!("\"{}\" not found", s.browser_search));
    };
    let key = rows[i].key.clone();
    select(s, &key);
    reveal(s);
    None
}

/// Send the previous / next sample row to the selected channel (`send_sample`).
fn send(s: &mut crate::app::State, dir: isize) -> Option<String> {
    let rows = view::rows(s);
    let Some(i) = step(s, &rows, dir, |r| matches!(r.item, Item::Sample(_))) else {
        return Some("No samples in the browser to send".into());
    };
    let Item::Sample(path) = rows[i].item.clone() else { return None };
    let key = rows[i].key.clone();
    select(s, &key);
    reveal(s);
    send_sample(s, &path, &rows[i].label)
}

/// Sample `path` (shown as `label`) into the selected channel, which takes the file's name while "Auto name channels"
/// is on (`rack::auto_name`). One history step (`history::load_sample`).
fn sample_target(s:&crate::app::State)->usize {
    match s.z.last() {Some(crate::app::Win::Hosted(row))=>*row,_=>s.selected_channel}
}
fn send_sample(s: &mut crate::app::State, path: &str, label: &str) -> Option<String> {
    send_sample_to(s,s.selected_channel,path,label)
}
fn send_sample_to(s:&mut crate::app::State,row:usize,path:&str,label:&str)->Option<String>{
    if !s.project.channels.get(row)?.kind.is_sampler() {
        return Some("The selected channel is not a sampler".into());
    }
    // With Copy used samples to the project folder: the sample's copy in the data folder (`data_folder::adopt`).
    let path = crate::data_folder::adopt(s, path);
    crate::sample_route::begin(s, Some(row), std::path::Path::new(&path));
    let ch = &mut s.project.channels[row];
    crate::rack::auto_name(ch, std::path::Path::new(&path));
    crate::rack::read_tags(ch, std::path::Path::new(&path));
    ch.sample_path = Some(path.clone());
    s.sample_loads.push((row, path));
    s.piano.dirty = true;
    Some(format!("Sent {label} to the selected channel"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `browser.txt` (spec: ui/browser.md §9): the dragged width, never below the default; anything else is the default.
    #[test]
    fn kept_width() {
        assert_eq!(parse_width("width=359\n"), 359.0);
        assert_eq!(parse_width("width=120"), crate::views::BROWSER_W);
        assert_eq!(parse_width("width=wide\n"), crate::views::BROWSER_W);
        assert_eq!(parse_width(""), crate::views::BROWSER_W);
    }
    #[test]
    fn sample_menu_tags_and_editors_use_the_selected_file() {
        use winit::keyboard::{Key, NamedKey};
        let folder = std::env::temp_dir().join(format!("lemon-sample-menu-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("tone.wav");
        let mut wav = Vec::new();
        os_engine::wav::write_f32_stereo_header(&mut wav, 48000, 4800).unwrap();
        wav.resize(wav.len() + 4800 * 8, 0);
        std::fs::write(&file, wav).unwrap();
        let mut app = App::new(None);
        app.add_browser_folder(folder.clone());
        let row = view::rows(&app.state).into_iter().find(|r| r.label == "tone.wav" || matches!(&r.item, Item::Sample(p) if std::path::Path::new(p) == file)).unwrap();
        app.state.browser_sel = Some(row.key.clone());
        app.browser_row_cmd(RowCmd::EditTags(false));
        assert!(app.browser_key(&Key::Character("Dry, Kick, dry".into()), false, false));
        assert!(app.browser_key(&Key::Named(NamedKey::Enter), false, false));
        assert_eq!(load_tags()[file.to_str().unwrap()], ["Dry", "Kick"]);
        assert!(view::matches(&app.state, &row, "kick"));
        app.browser_row_cmd(RowCmd::PitchEditor);
        assert!(app.state.playlist.popup.is_some());
        app.state.playlist.popup = None;
        let count = crate::playlist::clips(&app.state).len();
        assert_eq!(app.browser_row_cmd(RowCmd::WarpEditor), None);
        assert_eq!(crate::playlist::clips(&app.state).len(), count + 1);
        assert!(matches!(app.state.dialogs.last().map(|d| &d.body), Some(crate::dialog::Body::Warp(_))));
        app.browser_row_cmd(RowCmd::Trash);
        assert!(file.exists());
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }

}

/// Favorites are paths, so folders and samples keep working after a restart.
pub(crate) fn favorites() -> Vec<std::path::PathBuf> {
    crate::prefs::text("browser_favorites").and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

#[cfg(test)]
mod favorite_tests {
    use super::*;
    #[test]
    fn sample_browser_scrollbar_and_arrow_preview() {
        use winit::keyboard::{Key, NamedKey};
        let dir = std::env::temp_dir().join(format!("lemon-browser-arrows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..80 { std::fs::write(dir.join(format!("Sample{i:03}.wav")), []).unwrap(); }
        let mut app = App::new(None);
        app.state.screen = (1000., 650.);
        app.state.show_browser = true;
        app.state.browser_tab = 0;
        app.add_browser_folder(dir.clone());
        app.mouse = (30., view::list_top(&app.state) + 10.);
        let rows = view::rows(&app.state);
        let first = rows.iter().find(|r| matches!(r.item, Item::Sample(_))).unwrap();
        app.state.browser_sel = Some(first.key.clone());
        assert!(app.browser_key(&Key::Named(NamedKey::ArrowDown), false, false));
        assert_ne!(app.state.browser_sel.as_ref(), Some(&first.key));
        assert!(app.state.browser_preview.path.as_ref().unwrap().ends_with("Sample001.wav"));
        assert!(app.drag.is_none());
        assert!(app.browser_key(&Key::Named(NamedKey::ArrowUp), false, false));
        assert!(app.state.browser_preview.path.as_ref().unwrap().ends_with("Sample000.wav"));
        app.state.browser_preview.position=1.0;
        assert!(app.browser_key(&Key::Named(NamedKey::ArrowRight),false,false));
        assert_eq!(app.state.browser_preview.position,0.0);
        app.shift=true;
        let row=app.state.selected_channel;
        assert!(app.browser_key(&Key::Named(NamedKey::ArrowDown),false,false));
        assert!(app.state.project.channels[row].sample_path.as_ref().unwrap().ends_with("Sample001.wav"));
        assert!(app.browser_key(&Key::Named(NamedKey::ArrowUp),false,false));
        assert!(app.state.project.channels[row].sample_path.as_ref().unwrap().ends_with("Sample000.wav"));
        app.shift=false;
        let (track, thumb) = view::scrollbar(&app.state).unwrap();
        app.browser_scroll_drag(track.bottom() + 500., thumb.h / 2.);
        assert_eq!(app.state.browser_scroll, view::max_scroll(&app.state));
        app.browser_scroll_drag(track.y - 500., 0.);
        assert_eq!(app.state.browser_scroll, 0.);
        app.mouse.0 = 700.;
        assert!(!app.browser_key(&Key::Named(NamedKey::ArrowDown), false, false));
        // No device is opened: preview decoding is asynchronous and is not pumped.
        app.state.browser_folders.clear();
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn personal_library_browsing_search_and_restart() {
        let dir=std::env::temp_dir().join(format!("lemon-personal-library-{}",std::process::id()));
        let primary=dir.join("Primary");let added=dir.join("My Samples ü");
        std::fs::create_dir_all(&primary).unwrap();
        std::fs::create_dir_all(added.join("Closed/Deep")).unwrap();
        let sample=added.join("Closed/Deep/Unique Snare.wav");std::fs::write(&sample,[]).unwrap();
        let mut app=App::new(None);crate::prefs::set_text("browser_samples_root",&primary.to_string_lossy());
        app.add_browser_folder(added.clone());app.state.browser_names.insert(added.clone(),"My drum library".into());
        let rows=view::rows(&app.state);let root=rows.iter().find(|r|r.path.as_ref()==Some(&added)).unwrap();
        assert_eq!(root.label,"My drum library");assert_eq!(root.open,Some(true));
        let file=dir.join("settings/browser_folders.txt");write_folders_to(&file,&app.state).unwrap();
        let saved=read_folders_from(&file);assert_eq!(saved,vec![(added.clone(),"My drum library".into())]);
        let mut restarted=App::new(None);restarted.state.browser_folders=saved.iter().map(|v|v.0.clone()).collect();restarted.state.browser_names=saved.into_iter().collect();
        assert!(view::rows(&restarted.state).iter().any(|r|r.label=="My drum library" && r.open==Some(false)));
        restarted.state.browser_search="unique snare".into();
        let start=std::time::Instant::now();
        let found=loop {let rows=view::rows(&restarted.state);view::search_changed(&restarted.state);if !view::search_busy(&restarted.state){break rows;}assert!(start.elapsed().as_secs()<10);std::thread::sleep(std::time::Duration::from_millis(10));};
        assert_eq!(found.len(),1);assert_eq!(found[0].path.as_ref(),Some(&sample));assert!(matches!(found[0].item,Item::Sample(_)));
        restarted.state.browser_folders.clear();
        let start=std::time::Instant::now();
        loop {let rows=view::rows(&restarted.state);view::search_changed(&restarted.state);if !view::search_busy(&restarted.state){assert!(rows.is_empty());break;}assert!(start.elapsed().as_secs()<10);std::thread::sleep(std::time::Duration::from_millis(10));}
        assert!(options_menu(&app.state).iter().any(|p|matches!(p.cmd,Cmd::Browser(BCmd::AddFolder))));
        app.browser_cmd(BCmd::ConfigureFolders);assert_eq!(app.state.settings.tab,crate::views::settings::FILE);
        crate::prefs::set_text("browser_samples_root","");std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sample_name_search_finds_closed_folders_and_favorites() {
        let dir=std::env::temp_dir().join(format!("lemon-name-search-{}",std::process::id()));
        std::fs::create_dir_all(dir.join("closed/deep")).unwrap();
        std::fs::write(dir.join("closed/deep/Warm Kick.wav"),[]).unwrap();
        std::fs::write(dir.join("closed/deep/KICK Notes.txt"),[]).unwrap();
        let mut app=App::new(None);
        crate::prefs::set_text("browser_samples_root",&dir.to_string_lossy());
        app.state.browser_search="kIcK".into();
        fn wait(s:&crate::app::State) -> Vec<view::Row> {
            let start=std::time::Instant::now();
            loop {let rows=view::rows(s);view::search_changed(s);if !view::search_busy(s) {return rows;}assert!(start.elapsed().as_secs()<10);std::thread::sleep(std::time::Duration::from_millis(10));}
        }
        let rows=wait(&app.state);assert_eq!(rows.len(),1);assert_eq!(rows[0].label,"Warm Kick");
        assert!(matches!(rows[0].item,Item::Sample(_)));assert!(app.state.browser_open.is_empty() || !app.state.browser_open.contains(&view::disk_key(&dir)));
        crate::prefs::set_text("browser_favorites",&serde_json::to_string(&vec![dir.join("closed")]).unwrap());
        app.state.browser_tab=3;assert_eq!(wait(&app.state).len(),1);
        app.state.browser_opts.insert(view::OPT_TOP.into());
        app.state.show_browser=true;
        let mut c=os_ui::Canvas::new(1280,900,1.25).unwrap();app.paint(&mut c,None);
        assert!(app.hits.iter().any(|h|h.action==Action::BrowserSearch && h.rect.y<200.0));
        if let Some(out)=std::env::var_os("OS_SHOT_DIR") {c.pixmap.save_png(std::path::PathBuf::from(out).join("sample-name-search.png")).unwrap();}
        crate::prefs::set_text("browser_samples_root","");crate::prefs::set_text("browser_favorites","[]");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test] fn batch2_single_folder_view_navigates_and_preserves_tree(){
        let root=std::env::temp_dir().join(format!("lemon-single-folder-{}",std::process::id()));
        std::fs::create_dir_all(root.join("A/Sub")).unwrap();std::fs::create_dir_all(root.join("B")).unwrap();
        std::fs::write(root.join("A/one.wav"),[]).unwrap();std::fs::write(root.join("B/two.wav"),[]).unwrap();
        let mut app=App::new(None);app.state.browser_folders=vec![root.clone()];
        app.browser_cmd(BCmd::Toggle(view::OPT_SINGLE_FOLDER));app.state.browser_single_folder=Some(root.join("A"));
        let rows=view::rows(&app.state);assert!(rows.iter().any(|r|r.label=="one"));assert!(!rows.iter().any(|r|r.label=="two"));
        let sub=rows.iter().position(|r|r.label=="Sub").unwrap();app.browser_act(&Action::BrowserRow(sub));
        assert_eq!(app.state.browser_single_folder,Some(root.join("A/Sub")));
        app.browser_act(&Action::BrowserRow(0));assert_eq!(app.state.browser_single_folder,Some(root.join("A")));
        app.browser_cmd(BCmd::Toggle(view::OPT_SINGLE_FOLDER));assert!(!view::opt(&App::new(None).state,view::OPT_SINGLE_FOLDER));
        // Test-owned scratch fixture only.
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn browser_menu_restores_tabs_and_auto_hide() {
        let mut app=App::new(None);
        app.state.show_browser=true;
        let tab_ids = |state: &crate::app::State| view::tabs(state).into_iter().map(|tab| tab.0).collect::<Vec<_>>();
        let fixed = [0, 3, view::EXCHANGE_BUTTON, 5, 6];
        assert_eq!(tab_ids(&app.state), [fixed.as_slice(), &[4]].concat());
        app.browser_cmd(BCmd::Toggle(view::OPT_PRESETS));
        assert_eq!(tab_ids(&app.state), fixed);
        app.browser_act(&Action::BrowserMenu);
        assert!(app.state.playlist.popup.is_some());
        for key in [view::OPT_PROJECT,view::OPT_PLUGINS,view::OPT_PRESETS] {app.browser_cmd(BCmd::Toggle(key));}
        let expanded = [fixed.as_slice(), &[1, 2, 4]].concat();
        assert_eq!(tab_ids(&app.state), expanded);
        let restarted=App::new(None);
        assert_eq!(tab_ids(&restarted.state), expanded);
        for (tab,label) in [(1,"Current project"),(2,"Plugin database"),(4,"Plugin presets")] {
            app.browser_act(&Action::BrowserTab(tab));
            assert!(view::rows(&app.state).iter().any(|r|r.label==label && r.open==Some(true)));
        }
        app.browser_cmd(BCmd::Toggle(view::OPT_PRESETS));
        assert_eq!(app.state.browser_tab,0);
        app.state.playlist.popup=None;
        assert!(!view::opt(&app.state,view::OPT_AUTOHIDE));
        app.browser_cmd(BCmd::Toggle(view::OPT_AUTOHIDE));
        app.browser_auto_hide(100.,300.);
        app.browser_auto_hide(900.,300.);
        assert!(app.state.browser_auto_hidden);
        assert_eq!(crate::views::browser_w(&app.state),8.);
        app.browser_auto_hide(3.,300.);
        assert!(!app.state.browser_auto_hidden);
        app.browser_cmd(BCmd::Toggle(view::OPT_AUTOHIDE));
        app.browser_auto_hide(900.,300.);
        assert!(!app.state.browser_auto_hidden);
    }

    #[test]
    fn samples_browser_has_only_library_and_favorites_tabs() {
        let mut app=App::new(None);
        assert_eq!(view::samples_root(),std::path::PathBuf::from(r"E:\Pro Samples"));
        let rows=view::rows(&app.state);
        for label in ["FX","GO TO KIT","FILLZ"] {assert!(rows.iter().any(|r|r.label==label),"{label}");}
        assert!(rows.iter().all(|r|r.path.as_ref().is_some_and(|p|p.starts_with(view::samples_root()))));
        let mut c=os_ui::Canvas::new(1280,900,1.25).unwrap();
        app.state.show_browser=true;app.paint(&mut c,None);
        let tabs:Vec<_>=app.hits.iter().filter_map(|h|if let Action::BrowserTab(i)=h.action {Some(i)}else{None}).collect();
        assert_eq!(tabs,vec![0,view::EXCHANGE_BUTTON,3,5,6]);
        app.state.browser_search="nothing".into();app.browser_act(&Action::BrowserTab(0));
        assert!(app.state.browser_search.is_empty());
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR") {app.paint(&mut c,None);c.pixmap.save_png(std::path::PathBuf::from(dir).join("samples-browser.png")).unwrap();}
    }

    #[test]
    fn favorite_folder_and_sample_support_preview_and_removal() {
        let dir = std::env::temp_dir().join(format!("lemon-favorites-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = dir.join("kick.wav");
        std::fs::write(&sample, []).unwrap();
        let mut app = App::new(None);
        crate::prefs::set_text("browser_samples_root", &dir.to_string_lossy());
        crate::prefs::set_text("browser_favorites", "[]");
        app.state.browser_open.insert(view::disk_key(&dir));
        let row = view::rows(&app.state).into_iter().find(|r| r.path.as_ref() == Some(&sample)).unwrap();
        app.state.browser_sel = Some(row.key);
        app.browser_row_cmd(RowCmd::Favorite);
        assert_eq!(favorites(), vec![sample.clone()]);
        app.state.browser_tab = 3;
        let rows = view::rows(&app.state);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].item, Item::Sample(_)));
        assert_eq!(rows[0].path.as_ref(), Some(&sample));
        app.state.browser_sel = Some(rows[0].key.clone());
        app.browser_row_cmd(RowCmd::Favorite);
        assert!(favorites().is_empty());
        crate::prefs::set_text("browser_favorites", &serde_json::to_string(&vec![dir.clone()]).unwrap());
        let row = view::rows(&app.state).remove(0);
        app.state.browser_open.insert(row.key);
        assert!(view::rows(&app.state).iter().any(|r| r.path.as_ref() == Some(&sample)));
        crate::prefs::set_text("browser_samples_root", "");
        crate::prefs::set_text("browser_favorites", "[]");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)] mod community_update_browser_tests {
 use super::*;
 #[test] fn community_update_browser_refresh_and_preview_lifecycle(){
  let dir=std::env::temp_dir().join(format!("lemon-refresh-{}",std::process::id()));let folder=dir.join("New drums");std::fs::create_dir_all(&folder).unwrap();
  let mut app=App::new(None);app.state.browser_tab=0;app.state.browser_folders=vec![dir.clone()];crate::prefs::set_text("browser_samples_root",&dir.to_string_lossy());
  let key=view::disk_key(&dir);app.state.browser_open.insert(format!("{key}/New drums"));
  assert!(view::list_dir(&app.state,&folder).is_empty());
  std::fs::write(folder.join("new.wav"),b"fixture").unwrap();std::fs::write(dir.join("readme.txt"),b"fixture").unwrap();
  let i=view::rows(&app.state).iter().position(|r|r.path.as_ref()==Some(&folder)).unwrap();
  app.state.browser_preview.path=Some("selected.wav".into());
  app.browser_act(&Action::BrowserRow(i));assert!(!crate::browser_preview::visible(&app.state));
  app.browser_act(&Action::BrowserRow(i));assert!(view::rows(&app.state).iter().any(|r|r.label=="new"));
  assert!(view::rows(&app.state).iter().any(|r|r.label=="readme.txt"));
  std::fs::write(folder.join("another.wav"),b"fixture").unwrap();app.browser_cmd(BCmd::Reread);assert!(view::rows(&app.state).iter().any(|r|r.label=="another"));
  let missing=dir.join("Later");assert!(view::list_dir(&app.state,&missing).is_empty());std::fs::create_dir(&missing).unwrap();std::fs::write(missing.join("now.wav"),b"fixture").unwrap();assert_eq!(view::list_dir(&app.state,&missing).len(),1);
  app.state.browser_opts.insert(view::OPT_FOLDERS);app.state.browser_search="New drums".into();
  let start=std::time::Instant::now();loop {let _=view::rows(&app.state);view::search_changed(&app.state);if !view::search_busy(&app.state){break;}assert!(start.elapsed().as_secs()<10);std::thread::sleep(std::time::Duration::from_millis(10));}
  let i=view::rows(&app.state).iter().position(|r|r.path.as_ref()==Some(&folder)).unwrap();app.browser_act(&Action::BrowserRow(i));assert!(app.state.browser_search.is_empty());assert!(view::rows(&app.state).iter().any(|r|r.label=="another"));
  app.state.browser_preview.path=Some("selected.wav".into());app.browser_act(&Action::BrowserTab(3));assert!(!crate::browser_preview::visible(&app.state));
  crate::prefs::set_text("browser_samples_root","");std::fs::remove_dir_all(dir).unwrap();
 }
}

/// Ctrl toggles individual audio files; Shift extends a contiguous range in visible Browser order.
fn sample_selection(s:&crate::app::State,row:&view::Row,ctrl:bool,shift:bool)->(std::collections::BTreeSet<String>,Option<String>){
    let mut selected=s.browser_samples.clone();let anchor=s.browser_sample_anchor.clone().or_else(||s.browser_sel.clone());
    if !matches!(row.item,Item::Sample(_)){return(Default::default(),Some(row.key.clone()));}
    if shift {
        let rows=view::rows(s);let a=anchor.as_ref().and_then(|k|rows.iter().position(|r|&r.key==k));let b=rows.iter().position(|r|r.key==row.key);
        if !ctrl{selected.clear();}
        if let (Some(a),Some(b))=(a,b){for r in &rows[a.min(b)..=a.max(b)]{if matches!(r.item,Item::Sample(_)){selected.insert(r.key.clone());}}}
        else{selected.insert(row.key.clone());}
        (selected,anchor)
    }else if ctrl{
        if !selected.remove(&row.key){selected.insert(row.key.clone());}(selected,Some(row.key.clone()))
    }else{
        if !selected.contains(&row.key){selected.clear();selected.insert(row.key.clone());}
        (selected,Some(row.key.clone()))
    }
}
