// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    assets::WaveParams,
    factory_index::FACTORY,
    presets::{self, CATEGORIES, Origin},
};
use nih_plug::prelude::PluginState;
use os_ui::{Canvas, Color, Font, Rect};
use std::path::PathBuf;

pub const SAVE: Rect = Rect::new(1201., 14., 53., 34.);
const PANEL: Rect = Rect::new(738., 66., 654., 426.);
const FACTORY_TAB: Rect = Rect::new(755., 80., 112., 32.);
const USER_TAB: Rect = Rect::new(873., 80., 125., 32.);
const CLOSE: Rect = Rect::new(1351., 80., 25., 32.);
const SUBMIT: Rect = Rect::new(940., 291., 150., 37.);
fn category_rect(i: usize) -> Rect {
    Rect::new(755., 128. + i as f32 * 32., 164., 28.)
}
fn item_rect(i: usize) -> Rect {
    Rect::new(
        936. + (i % 2) as f32 * 220.,
        129. + (i / 2) as f32 * 39.,
        213.,
        32.,
    )
}
const PREV: Rect = Rect::new(938., 452., 68., 25.);
const NEXT: Rect = Rect::new(1305., 452., 68., 25.);
#[derive(Clone)]
enum Item {
    Factory(usize),
    User(String),
}
impl Item {
    fn name(&self) -> &str {
        match self {
            Self::Factory(i) => FACTORY[*i].1,
            Self::User(name) => name,
        }
    }
}
#[derive(PartialEq)]
enum Mode {
    Closed,
    Browse,
    Save,
}
pub struct Library {
    root: PathBuf,
    mode: Mode,
    user: bool,
    category: usize,
    page: usize,
    items: Vec<Item>,
    name: String,
    selected_text: bool,
    confirm: bool,
    status: String,
}
impl Default for Library {
    fn default() -> Self {
        Self::new(presets::root())
    }
}
impl Library {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            mode: Mode::Closed,
            user: false,
            category: 1,
            page: 0,
            items: Vec::new(),
            name: String::new(),
            selected_text: false,
            confirm: false,
            status: String::new(),
        }
    }
    pub fn active(&self) -> bool {
        self.mode != Mode::Closed
    }
    pub fn close(&mut self) {
        self.mode = Mode::Closed;
        self.confirm = false;
    }
    fn refresh(&mut self) {
        self.items.clear();
        self.status.clear();
        self.page = 0;
        self.confirm = false;
        let category = CATEGORIES[self.category];
        if self.user {
            match presets::users(&self.root, category) {
                Ok(names) => self.items = names.into_iter().map(Item::User).collect(),
                Err(e) => self.status = e,
            }
        } else {
            self.items = FACTORY
                .iter()
                .enumerate()
                .filter(|(_, p)| p.0 == category)
                .map(|(i, _)| Item::Factory(i))
                .collect();
            self.items.sort_by(|a, b| a.name().cmp(b.name()));
        }
    }
    pub fn open(&mut self, p: &WaveParams, saving: bool) {
        let origin = p.origin.lock().unwrap().clone();
        if let Some(o) = &origin {
            self.category = CATEGORIES
                .iter()
                .position(|c| *c == o.category)
                .unwrap_or(1);
            self.user = o.bank == "User";
        }
        if saving {
            self.user = true;
            self.name = origin
                .as_ref()
                .map(|o| o.name.clone())
                .unwrap_or_else(|| "My patch".into());
            self.selected_text = true;
        }
        self.mode = if saving { Mode::Save } else { Mode::Browse };
        self.refresh();
        if let Some(o) = origin {
            if let Some(i) = self.items.iter().position(|entry| entry.name() == o.name) {
                self.page = i / 16;
            }
        }
    }
    fn load(&mut self, entry: &Item) -> Option<PluginState> {
        let state = match entry {
            Item::Factory(i) => Ok(presets::factory(*i)),
            Item::User(name) => presets::read_user(&self.root, CATEGORIES[self.category], name),
        };
        match state {
            Ok(s) => {
                self.close();
                Some(s)
            }
            Err(e) => {
                self.mode = Mode::Browse;
                self.status = e;
                None
            }
        }
    }
    pub fn adjacent(&mut self, p: &WaveParams, forward: bool) -> Option<PluginState> {
        self.open(p, false);
        let origin = p.origin.lock().unwrap().clone();
        if self.category == 0 && !self.user {
            self.category = 1;
            self.refresh();
        }
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let current =
            origin.and_then(|o| self.items.iter().position(|entry| entry.name() == o.name));
        let next = current.map_or(if forward { 0 } else { n - 1 }, |i| {
            if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            }
        });
        self.load(&self.items[next].clone())
    }
    fn submit(&mut self, p: &WaveParams) {
        let origin = Origin {
            bank: "User".into(),
            category: CATEGORIES[self.category].into(),
            name: self.name.trim().into(),
        };
        let path = match presets::path_for(&self.root, &origin.category, &origin.name) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if path.exists() && !self.confirm {
            self.confirm = true;
            self.status =
                "This name already exists in this category. Click Replace to overwrite it.".into();
            return;
        }
        match presets::save(&self.root, p, &origin, self.confirm) {
            Ok(()) => self.close(),
            Err(e) => {
                self.confirm = false;
                self.status = e;
            }
        }
    }
    pub fn character(&mut self, c: char, p: &WaveParams) -> bool {
        if !self.active() {
            return false;
        }
        if c == '\u{1b}' {
            self.close();
            return true;
        }
        if self.mode == Mode::Save {
            match c {
                '\r' | '\n' => {
                    if !self.confirm {
                        self.submit(p);
                    }
                }
                '\u{1}' => self.selected_text = true,
                '\u{8}' => {
                    if self.selected_text {
                        self.name.clear();
                    } else {
                        self.name.pop();
                    }
                    self.selected_text = false;
                    self.confirm = false;
                    self.status.clear();
                }
                c if !c.is_control() => {
                    if self.selected_text {
                        self.name.clear();
                        self.selected_text = false;
                    }
                    if self.name.chars().count() < 52 {
                        self.name.push(c);
                    }
                    self.confirm = false;
                    self.status.clear();
                }
                _ => (),
            }
        }
        true
    }
    pub fn click(&mut self, x: f32, y: f32, p: &WaveParams) -> Option<PluginState> {
        if !PANEL.contains(x, y) || CLOSE.contains(x, y) {
            self.close();
            return None;
        }
        for i in 0..CATEGORIES.len() {
            if category_rect(i).contains(x, y) {
                self.category = i;
                self.refresh();
                return None;
            }
        }
        if self.mode == Mode::Save {
            if SUBMIT.contains(x, y) {
                self.submit(p);
            }
            return None;
        }
        if FACTORY_TAB.contains(x, y) || USER_TAB.contains(x, y) {
            self.user = USER_TAB.contains(x, y);
            self.refresh();
            return None;
        }
        if PREV.contains(x, y) {
            self.page = self.page.saturating_sub(1);
            return None;
        }
        if NEXT.contains(x, y) {
            self.page = (self.page + 1).min(self.items.len().saturating_sub(1) / 16);
            return None;
        }
        for i in 0..16 {
            if item_rect(i).contains(x, y) {
                if let Some(item) = self.items.get(self.page * 16 + i).cloned() {
                    return self.load(&item);
                }
            }
        }
        None
    }
    pub fn paint(&self, c: &mut Canvas) {
        use crate::instrument_view::raised;
        let ink = Color::rgb(0xe7f2f5);
        let dim = Color::rgb(0x9cabb4);
        let lime = crate::design::color(0xe3df8b);
        let font = Font::light(15, 0).natural();
        raised(c, PANEL, 10., Color::rgb(0x0a141b));
        let button = |c: &mut Canvas, r: Rect, label: &str, active: bool| {
            raised(c, r, 5., if active { lime } else { Color::rgb(0x14232c) });
            c.text_center(
                font,
                r,
                if active { Color::rgb(0x0a1010) } else { ink },
                label,
            );
        };
        if self.mode == Mode::Save {
            c.text(font.bold(), 756., 87., ink, "SAVE USER PRESET");
        } else {
            button(c, FACTORY_TAB, "Factory", !self.user);
            button(c, USER_TAB, "My presets", self.user);
            if !self.user {
                c.text(
                    font.bold(),
                    1020.,
                    87.,
                    dim,
                    "NIGHT SHIFT  /  FACTORY LIBRARY",
                );
            }
        }
        button(c, CLOSE, "×", false);
        for (i, cat) in CATEGORIES.iter().enumerate() {
            button(c, category_rect(i), cat, self.category == i);
        }
        if self.mode == Mode::Save {
            c.text(font, 940., 153., dim, "PRESET NAME");
            let field = Rect::new(940., 185., 427., 42.);
            raised(c, field, 5., Color::rgb(0x172631));
            c.text_clip(
                font,
                field.x + 12.,
                field.y + 11.,
                402.,
                if self.selected_text { lime } else { ink },
                &self.name,
            );
            c.text(font, 940., 243., dim, "Type a name and choose a category.");
            button(
                c,
                SUBMIT,
                if self.confirm {
                    "Replace preset"
                } else {
                    "Save preset"
                },
                true,
            );
        } else {
            for (i, item) in self.items.iter().skip(self.page * 16).take(16).enumerate() {
                button(c, item_rect(i), item.name(), false);
            }
            if self.items.is_empty() {
                c.text(font, 942., 158., dim, "No presets in this category yet.");
            }
            button(c, PREV, "‹", false);
            button(c, NEXT, "›", false);
            c.text_center(
                font,
                Rect::new(1008., 452., 294., 25.),
                dim,
                &format!(
                    "{} sounds   ·   {} / {}",
                    self.items.len(),
                    self.page + 1,
                    self.items.len().div_ceil(16).max(1)
                ),
            );
        }
        if !self.status.is_empty() {
            c.text_clip(
                Font::light(11, 0).natural(),
                938.,
                433.,
                433.,
                lime,
                &self.status,
            );
        }
    }
}
