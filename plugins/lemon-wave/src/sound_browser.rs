// SPDX-License-Identifier: GPL-3.0-or-later
use os_ui::{Canvas, Font, Rect};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub group: String,
    pub name: String,
    #[serde(default)]
    pub path: std::path::PathBuf,
    pub root: f32,
    #[serde(skip)]
    pub embedded: Option<usize>,
}
#[derive(Clone)]
pub enum Choice {
    Basic(usize),
    UserTable(std::path::PathBuf),
    Bank(usize),
    Sample(Entry),
    Category(usize),
    Group(String),
    Param(usize, i32),
}
#[derive(Default)]
pub struct Browser {
    pub oscillator: Option<usize>,
    pub menu: Option<Menu>,
    pub message: String,
}
pub struct Menu {
    pub anchor: Rect,
    pub items: Vec<(String, Choice)>,
    pub page: usize,
}
pub const BASIC: [&str; 6] = [
    "Sine",
    "Saw",
    "Square",
    "Triangle",
    "Pulse 25%",
    "Pulse 12.5%",
];
pub fn button(o: usize) -> Rect {
    Rect::new(crate::instrument_view::osc_x(o)+60.,137.,crate::instrument_view::osc_w(o)-105.,35.)
}
pub fn type_button(o: usize) -> Rect {
    Rect::new(28. + 410. * o as f32, 112., 124., 25.)
}
pub fn group_button(o: usize) -> Rect {
    Rect::new(214. + 410. * o as f32, 232., 188., 24.)
}
pub fn catalogue() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("Lemon Studio/Lemon Wave/library.json")
}
pub fn entries() -> Vec<Entry> {
    let bundled = crate::included_library::entries();
    if !bundled.is_empty() {
        return bundled;
    }
    std::fs::read(catalogue())
        .ok()
        .filter(|b| b.len() < 4 * 1024 * 1024)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn kind(params: &crate::assets::WaveParams, o: usize) -> usize {
    if params.base.v(if o == 0 {
        crate::controls::A_SOURCE
    } else {
        crate::controls::B_SOURCE
    }) == 1
    {
        return 3;
    }
    let t = params.tables.read().unwrap();
    if t.samples[o].is_some() {
        2
    } else if t.oscillators[o]
        .as_ref()
        .is_some_and(|t| BASIC.contains(&t.source.name.as_str()))
    {
        0
    } else {
        1
    }
}
pub fn group(params: &crate::assets::WaveParams, o: usize) -> String {
    let t = params.tables.read().unwrap();
    let name = t.samples[o]
        .as_ref()
        .map(|s| s.source.name.as_str())
        .unwrap_or("");
    entries()
        .iter()
        .find(|e| {
            name == format!("{} / {}", e.group, e.name)
                || e.path
                    .file_stem()
                    .is_some_and(|p| p.to_string_lossy() == name)
        })
        .map(|e| e.group.clone())
        .unwrap_or_else(|| {
            entries()
                .first()
                .map(|e| e.group.clone())
                .unwrap_or_default()
        })
}
impl Browser {
    pub fn show(&mut self, o: usize, anchor: Rect, items: Vec<(String, Choice)>) {
        self.oscillator = Some(o);
        self.menu = Some(Menu {
            anchor,
            items,
            page: 0,
        });
    }
    pub fn open(&mut self, o: usize) {
        self.show(
            o,
            button(o),
            BASIC
                .iter()
                .enumerate()
                .map(|(i, s)| (s.to_string(), Choice::Basic(i)))
                .collect(),
        );
    }
    pub fn categories(&mut self, o: usize) {
        let mut items: Vec<_> = BASIC.iter().enumerate()
            .map(|(i, name)| (format!("Basic Waves / {name}"), Choice::Basic(i))).collect();
        items.extend(["Basic Morph", "Hollow", "Glass", "Vowel"].iter().enumerate()
            .map(|(i, name)| (format!("Wavetables / {name}"), Choice::Bank(i))));
        items.push(("User Wavetables >".into(), Choice::Category(7)));
        items.push(("Recordings / Mini-Moog >".into(), Choice::Category(4)));
        items.push(("Recordings / 800DV >".into(), Choice::Category(5)));
        items.push(("Reference / Saw".into(), Choice::Category(3)));
        self.show(o, button(o), items);
    }
    pub fn user_tables(&mut self, o: usize) {
        let mut items = vec![("< All sounds".into(), Choice::Category(6))];
        items.extend(crate::table_tools::list().into_iter()
            .map(|(name, path)| (name, Choice::UserTable(path))));
        self.show(o, button(o), items);
    }
    pub fn sounds(&mut self, o: usize, kind: usize, group: &str) {
        match kind {
            0 => self.open(o),
            1 => {
                let mut items: Vec<_> = ["Basic - Sine / Saw / Square", "Hollow", "Glass", "Vowel"]
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.to_string(), Choice::Bank(i)))
                    .collect();
                items.extend(
                    crate::table_tools::list()
                        .into_iter()
                        .map(|(name, path)| (name, Choice::UserTable(path))),
                );
                self.show(o, button(o), items);
            }
            2 => self.show(
                o,
                button(o),
                entries()
                    .into_iter()
                    .filter(|e| e.group == group)
                    .map(|e| (e.name.clone(), Choice::Sample(e)))
                    .collect(),
            ),
            _ => self.show(
                o,
                button(o),
                vec![("Reference Saw".into(), Choice::Category(3))],
            ),
        }
    }
    pub fn instrument(&mut self, o: usize, instrument: &str) {
        let mut groups: Vec<_> = entries()
            .into_iter()
            .filter(|e| e.group.starts_with(instrument))
            .map(|e| e.group)
            .collect();
        groups.sort();
        groups.dedup();
        self.show(
            o,
            button(o),
            groups
                .into_iter()
                .map(|g| (g.clone(), Choice::Group(g)))
                .collect(),
        );
    }
    pub fn groups(&mut self, o: usize) {
        let mut groups: Vec<_> = entries().into_iter().map(|e| e.group).collect();
        groups.sort();
        groups.dedup();
        self.show(
            o,
            button(o),
            groups
                .into_iter()
                .map(|g| (g.clone(), Choice::Group(g)))
                .collect(),
        );
    }
    pub fn close(&mut self) {
        self.oscillator = None;
        self.menu = None;
    }
    pub fn click(&mut self, x: f32, y: f32) -> Option<Choice> {
        let m = self.menu.as_mut()?;
        let r = m.rect();
        if !r.contains(x, y) {
            self.close();
            return None;
        }
        let row = ((y - r.y - 4.) / 26.).floor() as usize;
        let n = (m.items.len() - m.page * 14).min(14);
        if row < n {
            return Some(m.items[m.page * 14 + row].1.clone());
        }
        if m.items.len() > 14 && row == n {
            if x < r.cx() {
                m.page = m.page.saturating_sub(1)
            } else if (m.page + 1) * 14 < m.items.len() {
                m.page += 1
            }
        }
        None
    }
    pub fn paint(&self, c: &mut Canvas) {
        let Some(m) = &self.menu else { return };
        let r = m.rect();
        crate::instrument_view::raised(c, r, 6., crate::design::color(0x283541));
        c.stroke_rrect(r, 6., 1., crate::design::color(0x789286));
        let f = Font::light(crate::design::font_size(12), 0).natural();
        let shown = m.items.iter().skip(m.page * 14).take(14);
        for (i, (label, _)) in shown.enumerate() {
            let row = Rect::new(r.x + 4., r.y + 4. + i as f32 * 26., r.w - 8., 25.);
            c.fill_rect(
                row,
                crate::design::color(if i % 2 == 0 { 0x26333e } else { 0x222e38 }),
            );
            c.text_clip(
                f,
                row.x + 10.,
                row.y + 6.,
                row.w - 20.,
                crate::design::color(0xe9f0e8),
                label,
            );
        }
        if m.items.is_empty() {
            c.text(
                f,
                r.x + 12.,
                r.y + 12.,
                crate::design::color(0xe9f0e8),
                "No included sounds",
            );
        }
        if m.items.len() > 14 {
            let n = (m.items.len() - m.page * 14).min(14);
            c.text_center(
                f,
                Rect::new(r.x, r.y + 4. + n as f32 * 26., r.w, 26.),
                crate::design::color(0xd5e989),
                &format!(
                    "< Previous          {} / {}          Next >",
                    m.page + 1,
                    m.items.len().div_ceil(14)
                ),
            );
        }
    }
}
impl Menu {
    pub fn rect(&self) -> Rect {
        let n = self
            .items
            .len()
            .saturating_sub(self.page * 14)
            .min(14)
            .max(1);
        let h = 8. + 26. * (n + usize::from(self.items.len() > 14)) as f32;
        let w = self.anchor.w.max(240.).min(390.);
        let x = self.anchor.x.min(crate::controls::SPEC.size.0 as f32 - 10. - w).max(10.);
        let y = if self.anchor.bottom() + h < crate::controls::SPEC.size.1 as f32 - 10. {
            self.anchor.bottom() + 3.
        } else {
            (self.anchor.y - h - 3.).max(8.)
        };
        Rect::new(x, y, w, h)
    }
}
pub fn basic(index: usize) -> crate::wavetable::Source {
    let samples = (0..crate::wavetable::SIZE)
        .map(|i| {
            let p = i as f32 / crate::wavetable::SIZE as f32;
            match index {
                0 => (p * std::f32::consts::TAU).sin(),
                1 => 2. * p - 1.,
                2 => {
                    if p < 0.5 {
                        1.
                    } else {
                        -1.
                    }
                }
                3 => 1. - 4. * (p - 0.5).abs(),
                4 => {
                    if p < 0.25 {
                        1.
                    } else {
                        -1.
                    }
                }
                _ => {
                    if p < 0.125 {
                        1.
                    } else {
                        -1.
                    }
                }
            }
        })
        .collect();
    crate::wavetable::Source {
        name: BASIC[index.min(5)].into(),
        samples,
    }
}
pub fn root_button(o: usize, i: usize) -> Rect {
    Rect::new(crate::instrument_view::osc_x(o)+20.+i as f32*94.,605.,90.,20.)
}

#[cfg(test)]
mod dropdown_tests {
    use super::*;
    #[test]
    fn dropdown_bounds_paging_and_dismiss() {
        let mut b = Browser::default();
        b.show(
            0,
            Rect::new(1000., 760., 100., 24.),
            (0..38)
                .map(|i| (format!("Patch {i}"), Choice::Bank(i)))
                .collect(),
        );
        let r = b.menu.as_ref().unwrap().rect();
        assert!(r.right() <= crate::controls::SPEC.size.0 as f32 - 10. && r.bottom() < crate::controls::SPEC.size.1 as f32 - 10. && r.y >= 8.);
        assert!(matches!(
            b.click(r.x + 10., r.y + 16.),
            Some(Choice::Bank(0))
        ));
        b.click(r.right() - 10., r.y + 4. + 14. * 26. + 10.);
        assert_eq!(b.menu.as_ref().unwrap().page, 1);
        let r = b.menu.as_ref().unwrap().rect();
        assert!(matches!(
            b.click(r.x + 10., r.y + 16.),
            Some(Choice::Bank(14))
        ));
        b.click(1., 1.);
        assert!(b.menu.is_none());
    }
}
