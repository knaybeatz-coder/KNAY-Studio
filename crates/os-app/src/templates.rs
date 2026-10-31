// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Settings > General ▸ Miscellaneous ▸ Default template and Startup project.
//!
//! The Default template is what FILE ▸ New gives (the FILE menu's first item names it, "New (<name>)"): the built-in
//! "Empty" (`app::default_project`), or a template found in the `Templates` folder of Lemon Studio's user folder, laid
//! out as FL's: a folder per category, holding a folder per template with `<name>.flp` in it. A template is opened as a
//! project and the result is untitled; a picked template whose file has gone gives Empty. The pick is a program setting
//! kept as free text (`prefs::text`): "Empty" or "<category>/<template>".
//!
//! The Startup project (`prefs::STARTUP_PROJECT`) is the project Lemon Studio starts with when no project file is
//! passed to it: the built-in project, the Default template, or the project file last opened or saved (kept as a
//! program setting too; one that is missing or does not load gives the Default template).
// spec: ui/settings-choices.md "Miscellaneous: seven more checks, Default template and Startup project", "General"

use std::path::{Path, PathBuf};

use crate::app::App;
use crate::prefs;

/// The built-in template: Lemon Studio's own new project (one Lemon Sampler channel, 130 BPM).
pub const EMPTY: &str = "Empty";
/// The settings file's keys: the picked template and the project file last opened or saved.
const TEMPLATE_KEY: &str = "default_template";
const LAST_KEY: &str = "last_project";

/// Install this custom build's factory template into the native user library once.
/// Existing templates and explicit user choices, including Empty, are preserved.
pub(crate) fn install_factory_template() -> std::io::Result<()> {
    let Some(root) = root() else { return Ok(()) };
    let path=file_in(&root,"Lemon Studio","Lemon Starter");
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
        use std::io::Write;
        match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut f)=>f.write_all(include_bytes!("../assets/templates/Lemon Starter.flp"))?,
            Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},
            Err(e)=>return Err(e),
        }
    }
    if prefs::text(TEMPLATE_KEY).is_none() {
        prefs::set_text(TEMPLATE_KEY, "Lemon Studio/Lemon Starter");
        crate::settings_file::write("startup_show_mixer", "1");
    }
    Ok(())
}

/// The Templates folder: in Lemon Studio's user folder; in tests, the one set in `TEST_DIR` (else none).
pub fn root() -> Option<PathBuf> {
    #[cfg(test)]
    return TEST_DIR.with(|d| d.borrow().clone());
    #[cfg(not(test))]
    os_paths::user_dir().map(|d| d.join("Templates"))
}

#[cfg(test)]
thread_local! {
    pub static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// A category folder of templates and the templates in it, by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub name: String,
    pub templates: Vec<String>,
}

/// Template `name`'s file in category `cat`.
pub(crate) fn file_in(root: &Path, cat: &str, name: &str) -> PathBuf {
    root.join(cat).join(name).join(format!("{name}.flp"))
}

/// The folders in `dir`, by name, sorted without regard to case.
fn folders(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|r| r.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

/// The templates in `root`: its category folders with the template folders that hold their `<name>.flp`. Categories
/// without one are left out.
pub fn scan(root: &Path) -> Vec<Category> {
    folders(root)
        .into_iter()
        .filter_map(|cat| {
            let templates: Vec<String> = folders(&root.join(&cat)).into_iter().filter(|t| file_in(root, &cat, t).is_file()).collect();
            (!templates.is_empty()).then_some(Category { name: cat, templates })
        })
        .collect()
}

/// The pick as kept: "Empty" or "<category>/<template>".
fn picked() -> String {
    prefs::text(TEMPLATE_KEY).unwrap_or_else(|| EMPTY.into())
}

/// The picked template's file; `None` for Empty, and for a template whose file has gone (FILE ▸ New gives Empty then).
pub fn file() -> Option<PathBuf> {
    let pick = picked();
    let (cat, name) = pick.split_once('/')?;
    Some(file_in(&root()?, cat, name)).filter(|f| f.is_file())
}

/// The Default template as the FILE menu ("New (<name>)") and the settings page show it: the template's name, or
/// "Empty".
pub fn name() -> String {
    match file() {
        Some(_) => picked().split_once('/').map_or_else(|| EMPTY.into(), |(_, n)| n.to_string()),
        None => EMPTY.into(),
    }
}

/// The project file last opened or saved (`None` when there is none yet).
pub fn last_project() -> Option<PathBuf> {
    prefs::text(LAST_KEY).map(PathBuf::from)
}

/// `path` was opened or saved: it is the last used project.
pub(crate) fn remember(path: &Path) {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    prefs::set_text(LAST_KEY, &path.to_string_lossy());
}

/// The last used project as kept, to put back with `restore_last` after writing a copy or opening a template.
pub(crate) fn last_kept() -> Option<String> {
    prefs::text(LAST_KEY)
}

pub(crate) fn restore_last(kept: Option<String>) {
    prefs::set_text(LAST_KEY, kept.as_deref().unwrap_or(""));
}

impl App {
    /// The Default template combo was clicked: its tree under the box (`x`, `y`: the box's bottom left), "Empty" then a
    /// submenu per category, the current pick marked.
    pub(crate) fn template_menu(&mut self, x: f32, y: f32) {
        use crate::playlist::{Cmd, Mark, PItem, Popup};
        let now = if file().is_some() { picked() } else { EMPTY.into() };
        let mut items = vec![PItem::new(EMPTY, Cmd::Template(EMPTY.into())).mark(Mark::Radio(now == EMPTY))];
        for c in root().map(|r| scan(&r)).unwrap_or_default() {
            let children = c
                .templates
                .iter()
                .map(|t| {
                    let pick = format!("{}/{t}", c.name);
                    let on = now == pick;
                    PItem::new(t.clone(), Cmd::Template(pick)).mark(Mark::Radio(on))
                })
                .collect();
            items.push(PItem { children, ..PItem::new(c.name, Cmd::Nothing) });
        }
        self.state.playlist.popup = Some(Popup { x, y: y + 1.0, items, sub: None, scroll: [0; 2] });
        self.invalidate();
    }

    /// A row of the Default template tree was picked (`Cmd::Template`): kept. Returns the message.
    pub(crate) fn set_template(&mut self, pick: &str) -> Option<String> {
        prefs::set_text(TEMPLATE_KEY, pick);
        self.invalidate();
        Some(format!("Default template: {}", name()))
    }

    /// FILE ▸ New with a template picked: the template opened as an untitled project (Save asks where), started now.
    /// False for Empty and when the template does not load: the caller makes the built-in project then.
    pub(crate) fn new_from_template(&mut self) -> bool {
        let Some(path) = file() else { return false };
        self.new_from_file(&path, &name())
    }

    /// The template file `path` (named `name`) opened as an untitled project (FILE ▸ New, New from template ▸). False
    /// when it does not load.
    pub(crate) fn new_from_file(&mut self, path: &Path, name: &str) -> bool {
        let path = path.to_path_buf();
        // A template is not a recent project either (spec: ui/recent-projects.md §2).
        let (kept, recent) = (last_kept(), crate::recent::list());
        self.state.path = None;
        self.open(path.clone());
        restore_last(kept);
        crate::recent::restore(&recent);
        if self.state.path.as_deref() != Some(path.as_path()) {
            return false;
        }
        let s = &mut self.state;
        s.path = None;
        for m in &mut s.project.mixer { m.slots_enabled = m.is_fx_return(); }
        crate::project_info::created(s);
        if s.project.info.title.as_deref().is_none_or(str::is_empty) {
            s.project.info.title = Some(name.to_string());
        }
        s.message = Some(format!("New project from the template {name}"));
        self.mark_saved();
        true
    }

    /// At startup, when no project file was passed to Lemon Studio: the Startup project's choice. Empty project keeps
    /// the built-in project `App::new` made; Last used project opens the project file last opened or saved, else (and
    /// for Default template) FILE ▸ New's project.
    pub fn startup_project(&mut self) {
        let choice = prefs::index(&prefs::STARTUP_PROJECT);
        if choice == 0 {
            return;
        }
        if choice == 2 {
            if let Some(p) = last_project().filter(|p| p.is_file()) {
                self.open(p.clone());
                if self.state.path.as_deref() == Some(p.as_path()) {
                    return;
                }
            }
        }
        if self.new_from_template() {
            // A fresh startup mix starts at unity, including templates from older builds.
            // Opening an existing project above returns before this default is applied.
            for m in &mut self.state.project.mixer { m.volume = Some(0.8); }
            for c in &mut self.state.project.init_controls {
                if matches!(os_model::target::decode(c.target), os_model::target::Target::MixerVolume(_)) { c.value = 12800; }
            }
            self.mark_saved();
            if crate::settings_file::get("startup_show_mixer").as_deref() == Some("1") {
                self.state.hide(crate::app::Win::ChannelRack);
                self.state.raise(crate::app::Win::Mixer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::playlist::{Cmd, Mark};
    use crate::views::settings::{self, SAct};

    #[test]
    fn startup_mix_buses_are_unity() {
        let dir=std::env::temp_dir().join(format!("lemon-unity-{}",std::process::id()));
        let path=file_in(&dir,"Michael Lane","TECHNO MAIN");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let source=std::env::var_os("LEMON_UNITY_TEMPLATE").map(PathBuf::from);
        let bytes=source.map(|p|std::fs::read(p).unwrap()).unwrap_or_else(||include_bytes!("../assets/templates/Lemon Starter.flp").to_vec());
        let mut file=os_flp::FlpFile::parse(&bytes).unwrap();
        let mut project=os_flp::project::load(&file).unwrap();
        for m in &mut project.mixer { m.volume=Some(0.5); m.slots_enabled=true; }
        for c in &mut project.init_controls {
            if matches!(os_model::target::decode(c.target),os_model::target::Target::MixerVolume(_)) { c.value=8000; }
        }
        os_flp::project::apply(&project,&mut file).unwrap();
        std::fs::write(&path,file.to_bytes().unwrap()).unwrap();
        TEST_DIR.with(|d| *d.borrow_mut()=Some(dir));
        prefs::set_text(TEMPLATE_KEY,"Michael Lane/TECHNO MAIN");
        crate::settings_file::write("startup_project","Default template");
        let mut app=App::new(None);
        app.startup_project();
        crate::init::apply(&mut app.state);
        assert!(!app.unsaved(),"fresh startup defaults are not unsaved edits");
        for m in &app.state.project.mixer {
            assert_eq!(m.volume,Some(0.8),"{:?} after startup",m.name);
            assert_eq!(m.slots_enabled,m.is_fx_return(),"only FX returns enabled by default");
            assert!(m.slot_on[10],"dedicated EQ stays independently enabled");
        }
        crate::mixer::hit(&mut app,crate::mixer::MHit::Effects(3));
        assert!(app.state.project.mixer.iter().all(|m|m.slots_enabled==(m.index==3 || m.is_fx_return())));
        crate::mixer::hit(&mut app,crate::mixer::MHit::Effects(3));
        crate::mixer::hit(&mut app,crate::mixer::MHit::AllEffects);
        assert!(app.state.mixer.all_fx_bypassed);
        crate::history::undo(&mut app.state);
        assert!(!app.state.mixer.all_fx_bypassed);
        assert!(app.state.project.mixer.iter().all(|m|m.slots_enabled==m.is_fx_return()));
        app.open(path);
        assert!(app.state.project.mixer.iter().all(|m|m.slots_enabled),"saved effect choices preserved");
        assert!(app.state.project.mixer.iter().all(|m|m.volume==Some(0.5)),"saved projects retain their mix");
    }

    #[test]
    fn factory_template_installs_in_empty_native_library() {
        let dir = std::env::temp_dir().join(format!("lemon_factory_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TEST_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
        install_factory_template().unwrap();
        let mut app = App::new(None);
        app.startup_project();
        assert_eq!(app.title(), "[Lemon Starter]");
        assert_eq!(app.state.tempo, 120.0);
        assert_eq!(app.state.project.mixer.len(), 127);
        assert!(app.state.shown(crate::app::Win::Mixer));
        let path = file().unwrap();
        std::fs::write(&path, b"user edited template").unwrap();
        prefs::set_text(TEMPLATE_KEY, EMPTY);
        install_factory_template().unwrap();
        assert_eq!(picked(), EMPTY);
        assert_eq!(std::fs::read(&path).unwrap(), b"user edited template");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A fresh Templates folder for this test (this thread's `root`), with "Minimal/Basic with limiter" (a 140 BPM
    /// project with six channels: the test project's five and a copy), "Utility/Scoring", and folders that are no
    /// templates.
    fn templates(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("os_templates_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (cat, name) in [("Minimal", "Basic with limiter"), ("Utility", "Scoring")] {
            let mut a = App::new(None);
            a.state.tempo = 140.0;
            let ch = a.state.project.channels[0].clone();
            a.state.project.channels.push(ch);
            let at = dir.join(cat).join(name);
            std::fs::create_dir_all(&at).unwrap();
            a.save_to(at.join(format!("{name}.flp")));
            assert!(a.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", a.state.message);
        }
        std::fs::create_dir_all(dir.join("Other").join("No file")).unwrap();
        std::fs::create_dir_all(dir.join("Minimal").join("Wrong name")).unwrap();
        std::fs::write(dir.join("Minimal").join("Wrong name").join("Other.flp"), b"x").unwrap();
        TEST_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
        // Saving those made them the last used project: start this test without one.
        restore_last(None);
        dir
    }

    /// The FILE menu's first item as painted.
    fn file_new_label() -> String {
        crate::views::menus::label_of(0, crate::views::menus::item_at(0, crate::views::menus::FILE_NEW).unwrap()).into_owned()
    }

    /// The tree: "Empty" first, then a submenu per category holding its templates; folders that hold no `<name>.flp`
    /// are left out (spec: ui/settings-choices.md "Miscellaneous: seven more checks, Default template and Startup
    /// project").
    #[test]
    fn the_template_tree() {
        let dir = templates("tree");
        assert_eq!(
            scan(&dir),
            vec![
                Category { name: "Minimal".into(), templates: vec!["Basic with limiter".into()] },
                Category { name: "Utility".into(), templates: vec!["Scoring".into()] },
            ]
        );
        let mut app = App::new(None);
        assert_eq!((name(), file_new_label()), ("Empty".to_string(), "New (Empty)".to_string()));
        app.open_settings(settings::GENERAL);
        app.act(Action::Settings(SAct::Combo(settings::DEFAULT_TEMPLATE, 10.0, 20.0)));
        let p = app.state.playlist.popup.as_ref().expect("the tree");
        let labels: Vec<&str> = p.items.iter().map(|i| i.label.as_ref()).collect();
        assert_eq!(labels, ["Empty", "Minimal", "Utility"]);
        assert_eq!((p.items[0].mark, p.items[1].children.len()), (Mark::Radio(true), 1));
        let pick = p.items[1].children[0].clone();
        assert_eq!((pick.label.as_ref(), &pick.cmd), ("Basic with limiter", &Cmd::Template("Minimal/Basic with limiter".into())));
        let msg = crate::playlist::run(&mut app, pick.cmd);
        assert_eq!(msg.as_deref(), Some("Default template: Basic with limiter"));
        assert_eq!(name(), "Basic with limiter");
        assert_eq!(file_new_label(), "New (Basic with limiter)");
        app.act(Action::Settings(SAct::Combo(settings::DEFAULT_TEMPLATE, 10.0, 20.0)));
        let p = app.state.playlist.popup.as_ref().unwrap();
        assert_eq!((p.items[0].mark, p.items[1].children[0].mark), (Mark::Radio(false), Mark::Radio(true)));
        // The settings page's combo shows the pick and opens the tree.
        if let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) {
            app.state.playlist.popup = None;
            app.state.settings.folded.extend(["Display", "Language", "Input", "Undo history", "Hints and warnings"]);
            app.paint(&mut c, None);
            let h = app.hits.iter().rev().find(|h| matches!(&h.action, Action::Settings(SAct::Combo(l, ..)) if *l == settings::DEFAULT_TEMPLATE));
            assert!(h.is_some(), "the combo works");
            let page = settings::page(settings::GENERAL, &app.state);
            let shown = page.iter().flat_map(|g| &g.rows).find_map(|r| match r {
                settings::Row::Combo(v, _, l) if *l == settings::DEFAULT_TEMPLATE => Some(v.clone()),
                _ => None,
            });
            assert_eq!(shown.as_deref(), Some("Basic with limiter"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FILE ▸ New opens the picked template as an untitled project, which is not the last used project; Empty, or a
    /// template whose file has gone, gives the built-in project (spec: as above).
    #[test]
    fn file_new_uses_the_default_template() {
        let dir = templates("new");
        let mut app = App::new(None);
        app.set_template("Minimal/Basic with limiter");
        app.menu_item(0, crate::views::menus::FILE_NEW);
        assert!(app.state.dialogs.is_empty(), "nothing to discard");
        assert_eq!(app.state.tempo, 140.0);
        assert_eq!(app.state.project.channels.len(), 6);
        assert_eq!(app.state.path, None, "untitled: Save asks where");
        assert!(app.state.file.is_some(), "the template's events are kept for the save");
        assert!(!app.unsaved());
        assert_eq!(app.title(), "[untitled]");
        assert_eq!(app.state.message.as_deref(), Some("New project from the template Basic with limiter"));
        assert_eq!(last_project(), None);
        let w = app.state.project.work_time.expect("started");
        assert_eq!(w.spent, 0.0);
        // Saved where asked, it becomes that file; the template stays as it was.
        let saved = dir.join("Song.flp");
        let before = std::fs::read(file().unwrap()).unwrap();
        app.save_to(saved.clone());
        assert_eq!(app.state.path.as_deref(), Some(saved.as_path()));
        assert_eq!(std::fs::read(file().unwrap()).unwrap(), before);
        assert_eq!(last_project(), Some(std::path::absolute(&saved).unwrap()));
        // The template's file gone: Empty.
        std::fs::remove_dir_all(dir.join("Minimal")).unwrap();
        assert_eq!(name(), "Empty");
        app.menu_item(0, crate::views::menus::FILE_NEW);
        assert_eq!((app.state.tempo, app.state.project.channels.len(), app.state.file.is_none()), (130.0, 1, true));
        assert_eq!(app.state.message.as_deref(), Some("New project"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Startup project: exactly FL's three choices, Default template by default; the last used project is the file
    /// last opened or saved (not a backup copy); a missing one gives the default template (spec: as above).
    #[test]
    fn startup_project_choices() {
        assert_eq!(prefs::STARTUP_PROJECT.choices, ["Empty project", "Default template", "Last used project"]);
        assert_eq!(prefs::choice(&prefs::STARTUP_PROJECT), "Default template");
        let dir = templates("startup");
        let mut app = App::new(None);
        app.set_template("Utility/Scoring");
        // Default template.
        let mut a = App::new(None);
        crate::settings_file::write("startup_show_mixer", "1");
        a.startup_project();
        assert_eq!((a.state.tempo, a.state.path.clone()), (140.0, None));
        assert_eq!(a.title(), "[Scoring]");
        assert!(a.state.shown(crate::app::Win::Playlist));
        assert!(a.state.shown(crate::app::Win::Mixer));
        assert!(!a.state.shown(crate::app::Win::ChannelRack));
        // Empty project.
        prefs::set_choice(&prefs::STARTUP_PROJECT, 0);
        let mut a = App::new(None);
        a.startup_project();
        assert_eq!((a.state.tempo, a.state.project.channels.len(), a.state.file.is_none()), (130.0, 5, true), "App::new's project, untouched");
        // Last used project: the file last saved; a backup copy does not count.
        prefs::set_choice(&prefs::STARTUP_PROJECT, 2);
        let song = dir.join("Last.flp");
        app.state.tempo = 111.0;
        app.save_to(song.clone());
        assert_eq!(last_project(), Some(std::path::absolute(&song).unwrap()));
        assert!(app.save_copy(dir.join("copy.flp")).is_some_and(|r| r.is_ok()));
        assert_eq!(last_project(), Some(std::path::absolute(&song).unwrap()), "a copy is not the last used project");
        let mut a = App::new(None);
        a.startup_project();
        assert_eq!((a.state.tempo, a.state.path.as_deref()), (111.0, Some(std::path::absolute(&song).unwrap().as_path())));
        // Opening another file makes it the last used project.
        let other = dir.join("Other.flp");
        std::fs::copy(&song, &other).unwrap();
        a.open(other.clone());
        assert_eq!(last_project(), Some(std::path::absolute(&other).unwrap()));
        // Gone, or not a project: the default template.
        std::fs::remove_file(&other).unwrap();
        let mut a = App::new(None);
        a.startup_project();
        assert_eq!((a.state.tempo, a.state.path.clone()), (140.0, None));
        std::fs::write(&other, b"not a project").unwrap();
        let mut a = App::new(None);
        a.startup_project();
        assert_eq!((a.state.tempo, a.state.path.clone()), (140.0, None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod send_setup_tests {
    fn set(p: &mut dyn os_host::Plugin, name: &str, value: f64) -> String {
        let id = (0..p.param_count()).filter_map(|i| p.param_info(i)).find(|i| i.name == name).expect(name).id;
        assert!(p.set_param(id,value));
        p.process_fx(48000, &mut [0.0;256], &mut [0.0;256]); p.idle();
        p.param_text(id,value).unwrap()
    }
    fn number(p: &mut dyn os_host::Plugin, name: &str, wanted: f64) {
        if (name=="Mix" || name=="mix") && wanted==100.0 {set(p,name,1.0);return;}
        let (mut lo,mut hi)=(0.0,1.0);
        for _ in 0..24 {
            let mid=(lo+hi)/2.0;
            let text=set(p,name,mid);
            let mut actual: f64=text.trim().trim_start_matches('+').split(|c:char| !c.is_ascii_digit() && c!='.' && c!='-').next().unwrap().parse().expect(&text);
            if name=="Space" && text.contains("ms") {actual/=1000.0;}
            if (actual-wanted).abs() <= (wanted.abs()*0.002).max(0.001) { println!("  {name} = {text}"); return; }
            if actual<wanted {lo=mid;} else {hi=mid;}
        }
        panic!("unable to set {name} to {wanted}");
    }
    fn choice(p:&mut dyn os_host::Plugin,name:&str,wanted:&str) {
        let mut seen=std::collections::BTreeSet::new();
        for i in 0..=200 {let text=set(p,name,i as f64/200.0);seen.insert(text.clone());if text.trim().eq_ignore_ascii_case(wanted) {println!("  {name} = {text}");return;}}
        panic!("missing choice {name}={wanted}: {seen:?}");
    }
    #[test]
    #[ignore]
    fn saved_send_templates_render_audio() {
        let cache:crate::plugin_db::Cache=serde_json::from_slice(&std::fs::read(std::env::var("LEMON_SETUP_CACHE").unwrap()).unwrap()).unwrap();
        for name in ["TECHNO MAIN","TRANSFER MAIN"] {
            let bytes=std::fs::read(std::path::PathBuf::from(std::env::var("LEMON_SETUP_OUT").unwrap()).join(format!("{name}.flp"))).unwrap();
            let project=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
            let mut plugins=crate::plugins::Plugins::builtins();plugins.merge(&cache);
            for m in project.mixer.iter().filter(|m| ["VERB LONG","VERB SHORT","VOCAL VERB","ANALOG VERB","SLAP","1/8","1/4","1/2","DELAY VERB"].contains(&m.name.as_deref().unwrap_or(""))) {
                let reference=os_flp::wrapper::decode(&m.slots[0].as_ref().unwrap().state).unwrap();
                let index=plugins.effects.iter().position(|a|a.desc.name==reference.name).unwrap();
                plugins.create_effect(m.index,0,index,Some(&reference.state)).unwrap();
                let mut p=plugins.hosted[&crate::plugins::fx_key(m.index,0)].plugin.lock().unwrap();
                let param=|p:&dyn os_host::Plugin,name:&str| {let id=(0..p.param_count()).filter_map(|i|p.param_info(i)).find(|i|i.name==name).unwrap().id;p.param_text(id,p.param_value(id).unwrap()).unwrap()};
                let label=m.name.as_deref().unwrap();
                if ["1/8","1/4","1/2"].contains(&label) {
                    assert_eq!(param(&*p,"DelayStyle").trim(),"PingPong");assert_eq!(param(&*p,"DelayLNote").trim(),label);assert_eq!(param(&*p,"DelayLSync").trim(),"Note");
                }
                if label=="VOCAL VERB" {assert_eq!(param(&*p,"Dynamics Mode").trim(),"Duck");assert_eq!(param(&*p,"Dynamics On/Off").trim(),"On");}
                let mut energy=0.0f64;
                for block in 0..750 {
                    let mut l=[0.0f32;256];let mut r=[0.0f32;256];
                    if block<48 {for i in 0..256 {l[i]=0.2*(std::f32::consts::TAU*440.0*(block as usize*256+i) as f32/48000.0).sin();r[i]=l[i];}}
                    p.transport_fx(&os_engine::TransportInfo{playing:true,tempo:120.0,frame:block*256,beats:block as f64*256.0/24000.0,bar_start:0.0,bar:0,numerator:4,denominator:4,looping:None});
                    p.process_fx(48000,&mut l,&mut r);
                    assert!(l.iter().chain(&r).all(|v|v.is_finite()));
                    if block>48 {energy+=l.iter().chain(&r).map(|v|(*v as f64).powi(2)).sum::<f64>();}
                }
                assert!(energy>1e-9,"silent send {name} {label}");println!("{name} {label}: saved state rendered energy={energy}");
            }
        }
    }

    #[test]
    #[ignore]
    fn prepare_send_templates() {
        let cache:crate::plugin_db::Cache=serde_json::from_slice(&std::fs::read(std::env::var("LEMON_SETUP_CACHE").unwrap()).unwrap()).unwrap();
        let out=std::path::PathBuf::from(std::env::var("LEMON_SETUP_OUT").unwrap());
        std::fs::create_dir_all(&out).unwrap();
        for template in ["TECHNO MAIN","TRANSFER MAIN"] {
            let bytes=if template=="TECHNO MAIN" {std::fs::read(std::env::var("LEMON_PRIVATE_TEMPLATE").unwrap()).unwrap()} else {std::fs::read(format!(r"E:\FL STUDIO STUDIO\FL Studio\FL Studio\Projects\Templates\{template}\{template}.flp")).unwrap()};
            let mut project=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
            let original=project.clone();
            let mut plugins=crate::plugins::Plugins::builtins();plugins.merge(&cache);
            for label in ["VERB LONG","VERB SHORT","VOCAL VERB","ANALOG VERB","SLAP","1/8","1/4","1/2","DELAY VERB"] {
                let track=project.mixer.iter_mut().find(|m|m.name.as_deref()==Some(label)).unwrap();
                let name=match label {"VOCAL VERB"=>"H-Reverb Stereo","SLAP"|"1/8"|"1/4"|"1/2"=>"ValhallaDelay","VERB SHORT"=>"ValhallaRoom",_=>"ValhallaVintageVerb"};
                println!("SETUP {template} {label}: {name}");
                let index=plugins.effects.iter().position(|a|a.desc.name==name).unwrap();
                plugins.create_effect(track.index,0,index,None).unwrap();
                let key=crate::plugins::fx_key(track.index,0);
                {
                    let mut locked=plugins.hosted[&key].plugin.lock().unwrap();let p=&mut *locked;
                    if name=="ValhallaDelay" {
                        number(p,"Mix",100.0);number(p,"Feedback",if label=="SLAP" {12.0} else {30.0});
                        number(p,"HighCut",6500.0);number(p,"LowCut",180.0);number(p,"Width",100.0);
                        if label=="SLAP" {choice(p,"DelayStyle","Single");choice(p,"DelayLSync","Msec");number(p,"DelayL_Ms",110.0);}
                        else {choice(p,"DelayStyle","PingPong");choice(p,"DelayLSync","Note");choice(p,"DelayRSync","Note");choice(p,"DelayLNote",label);choice(p,"DelayRNote",label);}
                    } else if name=="H-Reverb Stereo" {
                        set(p,"Dry/Wet",1.0);number(p,"ReverbTime",1.8);number(p,"Predelay Free",30.0);
                        choice(p,"PreDelay Mode","Free");choice(p,"Dynamics On/Off","On");choice(p,"Dynamics Mode","Duck");
                        number(p,"Dynamics Threshold",-24.0);number(p,"Dynamics Range",-9.0);number(p,"Dynamics Recovery",300.0);
                        number(p,"EQ HiShelf Gain",-6.0);number(p,"EQ HiShelf Freq",6500.0);

                    } else if name=="ValhallaRoom" {
                        number(p,"mix",100.0);number(p,"decay",0.7);number(p,"predelay",8.0);number(p,"HiCut",7000.0);number(p,"LoCut",180.0);
                    } else {
                        number(p,"Mix",if label=="DELAY VERB" {20.0} else {100.0});
                        number(p,"Decay",match label {"VERB LONG"=>3.2,"ANALOG VERB"=>1.6,_=>1.2});
                        number(p,"PreDelay",if label=="VERB LONG" {25.0} else {10.0});
                        number(p,"LowCut",180.0);number(p,"HighCut",if label=="ANALOG VERB" {5500.0} else {7500.0});
                        choice(p,"ReverbMode",if label=="ANALOG VERB" {"Plate"} else {"Concert Hall"});
                    }
                    let state=p.save_state().unwrap();assert!(!state.is_empty());
                    let expected:Vec<_>=(0..p.param_count()).filter_map(|i|p.param_info(i)).filter(|i| name!="H-Reverb Stereo" || ["Dry/Wet","ReverbTime","Predelay Free","PreDelay Mode","Dynamics On/Off","Dynamics Mode","Dynamics Threshold","Dynamics Range","Dynamics Recovery","EQ HiShelf Gain","EQ HiShelf Freq"].contains(&i.name.as_str())).map(|i|(i.id,p.param_text(i.id,p.param_value(i.id).unwrap()).unwrap())).collect();
                    assert!(p.load_state(&state));
                    for (id,value) in expected {assert_eq!(p.param_text(id,p.param_value(id).unwrap()).unwrap(),value,"state recall {name} {id}");}
                    let reference=crate::project_plugins::plugin_ref(p.desc(),state);
                    track.slots[0]=Some(os_model::SlotPlugin{name:os_flp::wrapper::WRAPPER.into(),state:os_flp::wrapper::encode(None,&reference)});
                    track.slots_enabled=true;track.slot_on[0]=true;track.slot_mix[0]=1.0;
                }
            }
            let mut file=os_flp::FlpFile::parse(&bytes).unwrap();os_flp::project::apply(&project,&mut file).unwrap();let bytes=file.to_bytes().unwrap();
            let back=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
            for m in &back.mixer {let old=original.mixer.iter().find(|x|x.index==m.index).unwrap();assert_eq!(m.routes,old.routes);assert_eq!(m.color,old.color);assert_eq!(m.name,old.name);}
            for m in &back.mixer {let old=original.mixer.iter().find(|x|x.index==m.index).unwrap();if !["VERB LONG","VERB SHORT","VOCAL VERB","ANALOG VERB","SLAP","1/8","1/4","1/2","DELAY VERB"].contains(&m.name.as_deref().unwrap_or("")) {assert_eq!(m.slots,old.slots);}}
            std::fs::write(out.join(format!("{template}.flp")),bytes).unwrap();
        }
    }
    #[test]
    #[ignore]
    fn inspect_send_plugins() {
        let cache: crate::plugin_db::Cache = serde_json::from_slice(&std::fs::read(std::env::var("LEMON_SETUP_CACHE").unwrap()).unwrap()).unwrap();
        let mut plugins = crate::plugins::Plugins::builtins();
        plugins.merge(&cache);
        for name in ["H-Reverb Stereo"] {
            let index = plugins.effects.iter().position(|a| a.desc.name == name).unwrap();
            plugins.create_effect(42, 0, index, None).unwrap();
            let key = crate::plugins::fx_key(42,0);
            let p = plugins.hosted[&key].plugin.lock().unwrap();
            println!("PLUGIN {name}");
            for i in 0..p.param_count() {
                let info = p.param_info(i).unwrap();
                let value=p.param_value(info.id).unwrap_or(0.0);
                println!("{} {} [{},{}] default={} value={} text={:?} mid={:?} max={:?}",info.id,info.name,info.min,info.max,info.default,value,p.param_text(info.id,value),p.param_text(info.id,0.5),p.param_text(info.id,1.0));
            }
        }
        for name in ["TECHNO MAIN", "TRANSFER MAIN"] {
            let path = format!(r"E:\FL STUDIO STUDIO\FL Studio\FL Studio\Projects\Templates\{name}\{name}.flp");
            let file=os_flp::FlpFile::parse(&std::fs::read(path).unwrap()).unwrap();
            let project=os_flp::project::load(&file).unwrap();
            for m in &project.mixer { if m.name.is_some() { println!("TRACK {name} {} {:?} routes={:?} slots={:?}",m.index,m.name,m.routes,m.slots.iter().map(|s|s.as_ref().map(|s|&s.name)).collect::<Vec<_>>()); } }
        }
    }
}

#[cfg(test)]
mod public_tests {
    use os_engine::effects::{delay as d,reverb as r,dynamics as c};
    fn slot(name:&str,state:Vec<u8>)->os_model::SlotPlugin {
        let reference=os_model::PluginRef{format:"clap".into(),name:name.into(),vendor:"Lemon Studio".into(),path:format!("{name}.clap"),state,..Default::default()};
        os_model::SlotPlugin{name:os_flp::wrapper::WRAPPER.into(),state:os_flp::wrapper::encode(None,&reference)}
    }
    #[test]
    #[ignore]
    fn generate_public_template() {
        let bytes=std::fs::read(std::env::var("LEMON_PRIVATE_TEMPLATE").unwrap()).unwrap();
        let personal=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
        let mut p=crate::app::default_project();p.tempo=personal.tempo;p.ppq=personal.ppq;
        p.arrangements=vec![os_model::Arrangement{number:0,name:Some("Arrangement".into()),tracks:(1..=500).map(os_model::PlaylistTrack::new).collect(),..Default::default()}];
        p.mixer=personal.mixer.clone();
        for m in &mut p.mixer {m.slots=Default::default();m.slot_on=[true;11];m.slot_mix=[1.0;11];m.slots_enabled=true;m.origin=None;m.latency_in=0.0;m.latency_out=0.0;}
        let vocal=p.mixer.iter().find(|m|m.name.as_deref().is_some_and(|n|n.eq_ignore_ascii_case("vocal mix")||n.eq_ignore_ascii_case("vox mix")||n.eq_ignore_ascii_case("vocals"))).map(|m|m.index).unwrap_or(6);
        println!("Ducking source: {} {:?}",vocal,p.mixer[vocal as usize].name);
        let vr=p.mixer.iter().find(|m|m.name.as_deref()==Some("VOCAL VERB")).unwrap().index;
        if !p.mixer[vocal as usize].routes.contains(&vr){p.mixer[vocal as usize].routes.push(vr);}
        p.mixer[vocal as usize].send_levels.retain(|(t,_)|*t!=vr);p.mixer[vocal as usize].send_levels.push((vr,0.0));
        let port=p.mixer.iter().filter(|m|m.index!=vr&&m.routes.contains(&vr)).position(|m|m.index==vocal).unwrap()+1;
        for m in &mut p.mixer {
            let label=m.name.as_deref().unwrap_or("");
            if ["VERB LONG","VERB SHORT","VOCAL VERB","ANALOG VERB","DELAY VERB"].contains(&label) {
                let mut settings=r::Settings::default();
                settings.knobs[r::DRY]=if label=="DELAY VERB" {128}else{0};
                settings.knobs[r::WET]=if label=="DELAY VERB" {32}else{128};
                settings.knobs[r::EARLY]=if label=="DELAY VERB" {12}else{32};
                settings.knobs[r::LOW_CUT]=180;
                settings.knobs[r::HIGH_CUT]=if label=="ANALOG VERB" {55}else{75};
                settings.knobs[r::DAMPING]=if label=="ANALOG VERB" {40}else{60};
                settings.knobs[r::DECAY]=match label {"VERB LONG"=>32,"VERB SHORT"=>7,"VOCAL VERB"=>18,"ANALOG VERB"=>16,_=>12};
                settings.knobs[r::PREDELAY]=match label {"VERB LONG"=>10,"VOCAL VERB"=>12,_=>3};
                settings.knobs[r::ROOM]=if label=="VERB SHORT" {30}else{70};
                m.slots[0]=Some(slot("Lemon Reverb",lemon_reverb::params::plugin_state_from_fl(&settings.to_state())));
                if label=="VOCAL VERB" {
                    let mut raw=lemon_limiter::params::default_state();
                    let mut values=c::DEFAULTS;values[c::LIM_ATT]=0;values[c::CEILING]=2000;
                    values[c::COMP_THRES]=450;values[c::COMP_RATIO]=700;values[c::COMP_KNEE]=300;
                    values[c::COMP_ATT]=2000;values[c::COMP_REL]=8000;
                    for (k,v) in values.into_iter().enumerate(){raw[(k+1)*4..(k+2)*4].copy_from_slice(&v.to_le_bytes());}
                    raw[164..168].copy_from_slice(&1i32.to_le_bytes());
                    let mut fx=slot("Lemon Limiter",lemon_limiter::params::plugin_state_from_fl(&raw));
                    fx.state=os_flp::wrapper::set_input_port(&fx.state,1,port as i32).unwrap();m.slots[1]=Some(fx);
                }
            } else if ["SLAP","1/8","1/4","1/2"].contains(&label) {
                let mut settings=d::Settings::default();settings.v[d::DRY]=0;settings.v[d::WET]=6000;
                settings.v[d::TYPE]=if label=="SLAP" {d::TRUE_STEREO}else{d::PING_PONG};
                settings.v[d::SYNC]=if label=="SLAP" {0}else{1};
                settings.v[d::TIME]=match label {"1/8"=>96,"1/4"=>192,"1/2"=>384,_=>339};
                settings.v[d::FEEDBACK]=if label=="SLAP" {576}else{1440};settings.v[d::CUTOFF]=4500;settings.v[d::RESONANCE]=0;
                m.slots[0]=Some(slot("Lemon Delay",lemon_delay::params::plugin_state_from_fl(&settings.to_state())));
            }
        }
        let bytes=os_flp::project::create(&p).unwrap().to_bytes().unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();assert_eq!(back.mixer.len(),p.mixer.len());assert_eq!(back.mixer.iter().flat_map(|m|m.slots.iter().flatten()).count(),10);
        std::fs::write(std::env::var("LEMON_PUBLIC_OUT").unwrap(),bytes).unwrap();
    }
    #[test]
    fn public_template_native_sends() {
        let bytes=include_bytes!("../assets/templates/Lemon Starter.flp");
        let p=os_flp::project::load(&os_flp::FlpFile::parse(bytes).unwrap()).unwrap();
        assert!(p.channels.iter().all(|c|c.sample_path.is_none()&&c.plugin.is_none()));
        assert!(p.arrangements.iter().all(|a|a.clips.is_empty()));
        let mut plugins=crate::plugins::Plugins::builtins();let mut count=0;
        for m in &p.mixer {for (k,fx) in m.slots.iter().enumerate().filter_map(|(k,s)|s.as_ref().map(|s|(k,s))) {
            let reference=os_flp::wrapper::decode(&fx.state).unwrap();assert!(!reference.path.contains(':'));
            assert!(["Lemon Reverb","Lemon Delay","Lemon Limiter"].contains(&reference.name.as_str()));
            let index=plugins.effects.iter().position(|a|a.desc.name==reference.name).unwrap();
            plugins.create_effect(m.index,k as u8,index,Some(&reference.state)).unwrap();
            let label=m.name.as_deref().unwrap();
            if reference.name=="Lemon Delay" {
                let raw=lemon_delay::params::fl_from_plugin_state(&reference.state).unwrap();let settings=d::Settings::from_state(&raw).unwrap();
                if label!="SLAP" {assert_eq!(settings.v[d::TYPE],d::PING_PONG);assert_eq!(settings.v[d::SYNC],1);
                    let beats=match label {"1/8"=>0.5,"1/4"=>1.0,"1/2"=>2.0,_=>panic!()};
                    for bpm in [90.0,120.0,150.0] {assert!((d::delay_seconds(d::norm(d::TIME,settings.v[d::TIME]),true,bpm)-beats*60.0/bpm).abs()<1e-7);}
                }
            }
            if reference.name=="Lemon Limiter" {
                let ports=os_flp::wrapper::input_ports(&fx.state);assert!(ports[1]>0);
                let incoming:Vec<_>=p.mixer.iter().filter(|t|t.index!=m.index&&t.routes.contains(&m.index)).collect();
                assert_eq!(incoming[ports[1] as usize-1].name.as_deref(),Some("Vocal Mix"));
                let mut fx=plugins.hosted[&crate::plugins::fx_key(m.index,k as u8)].plugin.lock().unwrap();
                let mut dry_energy=0.0;let mut duck_energy=0.0;
                for keyed in [false,true] {
                    fx.load_state(&reference.state);fx.reset_fx();
                    for block in 0..200 {
                        let mut l=[0.2;256];let mut r=[0.2;256];let key=[if keyed{0.5}else{0.0};256];
                        fx.process_fx_keyed(48000,&mut l,&mut r,(&key,&key));
                        if block>100 {let energy=l.iter().map(|v|(*v as f64).powi(2)).sum::<f64>();if keyed{duck_energy+=energy;}else{dry_energy+=energy;}}
                    }
                }
                assert!(duck_energy<dry_energy*0.5,"duck {duck_energy} vs unkeyed {dry_energy}");
            }
            if k==0 {
                let mut fx=plugins.hosted[&crate::plugins::fx_key(m.index,k as u8)].plugin.lock().unwrap();let mut energy=0.0;
                for block in 0..600 {let mut l=[0.0;256];let mut r=[0.0;256];if block==0 {l[0]=0.5;r[0]=0.5;}fx.process_fx(48000,&mut l,&mut r);
                    assert!(l.iter().chain(&r).all(|v|v.is_finite()));if block>0{energy+=l.iter().chain(&r).map(|x|(*x as f64).powi(2)).sum::<f64>();}}
                assert!(energy>1e-9,"silent send {label}");println!("{label}: native state loads and tail renders");
            }
            count+=1;
        }}
        assert_eq!(count,10);
    }
}
