// Copyright (C) 2026 Lemon Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio.
//! Project-local import provenance, persistent compatibility report and explicit recovery choices.
use crate::{
    app::App,
    dialog::{Answer, Kind, Msg, Then},
    playlist::{Cmd, PItem, Popup},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const TAG: os_flp::Tag = os_flp::Tag(*b"LIMP");
#[derive(Default)]
pub struct Report {
    pub source: Option<PathBuf>,
    pub approved: bool,
    pub substitutes: BTreeMap<usize, String>,
    pub notes: Vec<String>,
    pub pending: bool,
}
impl Report {
    pub fn opened(file: &os_flp::FlpFile, path: &Path) -> Self {
        if let Some(data) = file
            .chunks
            .iter()
            .find(|c| c.tag == TAG)
            .and_then(|c| {
                if let os_flp::ChunkBody::Raw(d) = &c.body {
                    Some(d)
                } else {
                    None
                }
            })
            .filter(|d| d.len() < 2_000_000)
            && let Ok(v) = serde_json::from_slice::<serde_json::Value>(data)
        {
            return Self {
                source: v["source"].as_str().map(PathBuf::from),
                approved: v["approved"].as_bool().unwrap_or(false),
                notes: v["notes"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Self::default()
            };
        }
        let foreign = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("flp"))
            && !file
                .chunks
                .iter()
                .any(|c| matches!(&c.tag.0, b"LPCH" | b"LSCL" | b"LSKB"));
        Self {
            source: foreign.then(|| path.to_owned()),
            pending: foreign,
            ..Self::default()
        }
    }
    pub fn protects(&self, path: &Path) -> bool {
        self.source.as_deref() == Some(path)
    }
    pub fn store(&self, file: &mut os_flp::FlpFile) {
        file.chunks.retain(|c| c.tag != TAG);
        if self.source.is_none() {
            return;
        }
        if let Ok(data) = serde_json::to_vec(
            &serde_json::json!({"version":1,"source":self.source,"approved":self.approved,"notes":self.notes}),
        ) {
            file.chunks.push(os_flp::Chunk {
                tag: TAG,
                body: os_flp::ChunkBody::Raw(data),
            });
        }
    }
}
pub fn lines(app: &App) -> Vec<String> {
    let mut items = Vec::new();
    for (row, path) in crate::missing::missing_samples(&app.state) {
        let candidates = os_engine::sample::candidates(&path);
        items.push(if candidates.len() > 1 {
            format!(
                "Sample {}: {} files match. Choose the correct file in Resolve.",
                row + 1,
                candidates.len()
            )
        } else {
            format!("Missing sample: {path}")
        });
    }
    for n in app.missing.plugin_names() {
        items.push(format!("Plugin unavailable: {n}"));
    }
    for n in app.import_report.substitutes.values() {
        items.push(format!("Available substitute: {n}. The original preset cannot be transferred. Use Resolve to approve."));
    }
    for h in app.plugins.hosted.values() {
        if h.stand_in {
            items.push(format!("{}: a substitute is playing its defaults; the original imported settings are retained until edited.",h.name));
        }
        if h.keep_file_state {
            items.push(format!("{}: original imported state protected. Explicit edits will be saved separately in the Lemon working copy.",h.name));
        }
        if h.native_import_edited {
            items.push(format!("{}: edited native settings will be saved. This plugin normalizes imported data; verify on reopening.",h.name));
        }
    }
    for a in &app.state.project.automations {
        if !app
            .state
            .project
            .arrangements
            .iter()
            .flat_map(|r| &r.clips)
            .any(|c| !c.muted && c.source == os_model::ClipSource::Channel(a.channel))
        {
            continue;
        }
        for &target in &a.targets {
            if matches!(
                os_model::target::decode(target),
                os_model::target::Target::Other(_)
            ) {
                let name = app
                    .state
                    .project
                    .channels
                    .iter()
                    .find(|c| c.index == a.channel)
                    .and_then(|c| c.name.as_deref())
                    .unwrap_or("Automation");
                items.push(format!("{name}: an automation link is retained but cannot play yet (target {target:#010x})."));
            }
        }
    }
    items.extend(app.import_report.notes.clone());
    items.sort();
    items.dedup();
    items
}
pub fn show(app: &mut App, page: usize) {
    let mut items = lines(app);
    if items.is_empty() {
        items.push("No unresolved files or automation links were detected. This is not a guarantee of identical playback in every plugin.".into());
    }
    let pages = items.len().div_ceil(2);
    let page = page.min(pages - 1);
    let source = app
        .import_report
        .source
        .as_deref()
        .and_then(Path::file_name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Current project".into());
    let mut text = format!("{source} — page {} of {pages}\n\n", page + 1);
    for line in items.iter().skip(page * 2).take(2) {
        let mut text_line: String = line.chars().take(220).collect();
        if line.chars().count() > 220 {
            text_line.push_str("...");
        }
        text.push_str(&text_line);
        text.push_str("\n\n");
    }
    let mut msg = Msg::new(
        Kind::Info,
        "Project import report",
        vec![(text, false)],
        &[Answer::Yes, Answer::No, Answer::Cancel, Answer::Ok],
        Then::ImportReport(page),
    )
    .labelled(&["Previous", "Next", "Close", "Resolve..."]);
    msg.focus = 2;
    app.show_msg(msg);
}
pub fn answer(app: &mut App, page: usize, answer: Answer) {
    match answer {
        Answer::Yes => show(app, page.saturating_sub(1)),
        Answer::No => show(app, page + 1),
        Answer::Ok => options(app),
        _ => {}
    }
}
pub fn options(app: &mut App) {
    let mut items = vec![
        PItem::new("Show import report", Cmd::ImportReport),
        PItem::new("Export full report...", Cmd::ExportImportReport),
        PItem::new("Find missing samples...", Cmd::FindMissingSamples),
    ];
    if !app.import_report.substitutes.is_empty() {
        items.push(PItem::new(
            "Use available plugin substitutes (default presets)",
            Cmd::ApproveSubstitutes,
        ));
    }
    for (row, path) in crate::missing::missing_samples(&app.state) {
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path);
        items.push(PItem::new(
            format!("Locate {name} (channel {})...", row + 1),
            Cmd::LocateMissingSample(row as u16),
        ));
    }
    app.state.playlist.popup = Some(Popup {
        x: app.mouse.0,
        y: app.mouse.1,
        items,
        sub: None,
        scroll: [0; 2],
        cascade: Vec::new(),
    });
    app.invalidate();
}
pub fn approve(app: &mut App) {
    app.import_report.approved = true;
    for notice in app.import_report.substitutes.values() {
        app.import_report.notes.push(format!("Approved substitute: {notice}. Its default preset is used until edited; retain the original FL project for recovery."));
    }
    app.plugins.edit_revision = app.plugins.edit_revision.wrapping_add(1);
    let keys: Vec<_> = app.import_report.substitutes.keys().copied().collect();
    app.wrap_failed.retain(|k| !keys.contains(k));
    for key in keys {
        app.missing.wrapped.remove(&key);
    }
    app.import_report.substitutes.clear();
    app.state.piano.dirty = true;
    app.invalidate();
}
pub fn locate(app: &mut App, row: usize) {
    let Some(old) = app
        .state
        .project
        .channels
        .get(row)
        .and_then(|c| c.sample_path.clone())
    else {
        return;
    };
    let candidates = os_engine::sample::candidates(&old);
    if candidates.len() > 1 {
        let mut items = vec![PItem::new(
            "Browse for another file...",
            Cmd::BrowseMissingSample(row as u16),
        )];
        for path in candidates.into_iter().take(32) {
            items.push(PItem::new(
                path.to_string_lossy().into_owned(),
                Cmd::UseMissingSample(row as u16, old.clone(), path),
            ));
        }
        app.state.playlist.popup = Some(Popup {
            x: app.mouse.0,
            y: app.mouse.1,
            items,
            sub: None,
            scroll: [0; 2],
            cascade: Vec::new(),
        });
        app.invalidate();
    } else {
        browse(app, row);
    }
}
pub fn browse(app: &mut App, row: usize) {
    let Some(old) = app
        .state
        .project
        .channels
        .get(row)
        .and_then(|c| c.sample_path.clone())
    else {
        return;
    };
    let folder = os_engine::sample::candidates(&old)
        .first()
        .and_then(|p| p.parent().map(Path::to_owned));
    let dlg = crate::filedlg::FileDlg::new(
        crate::filedlg::Mode::Open,
        "Choose the missing sample",
        "audio",
        Then::RelinkSample(row, old),
    )
    .filter(
        "Audio",
        &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"],
    )
    .start(folder.as_deref());
    app.file_dialog(dlg);
}
pub fn relink(app: &mut App, row: usize, old: &str, path: &Path) {
    if !path.is_file()
        || app
            .state
            .project
            .channels
            .get(row)
            .and_then(|c| c.sample_path.as_deref())
            != Some(old)
    {
        return;
    }
    crate::playlist::snapshot(&mut app.state);
    let new = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_owned())
        .to_string_lossy()
        .into_owned();
    for (i, ch) in app.state.project.channels.iter_mut().enumerate() {
        if ch.sample_path.as_deref() == Some(old) {
            ch.sample_path = Some(new.clone());
            if ch.kind.is_sampler() {
                app.state.sample_loads.push((i, new.clone()));
            }
        }
    }
    for warp in &mut app.state.project.audio_warps {
        if warp.rendered == old {
            warp.rendered = new.clone();
        }
    }
    app.state.piano.dirty = true;
    app.invalidate();
}

pub fn export(app: &mut App) {
    let dlg = crate::filedlg::FileDlg::new(
        crate::filedlg::Mode::Save,
        "Export import report",
        "report",
        Then::SaveImportReport,
    )
    .filter("Text", &["txt"])
    .name("Project import report.txt");
    app.file_dialog(dlg);
}
pub fn save_report(app: &mut App, path: &Path) {
    let text = format!(
        "Lemon Studio project import report\nOriginal: {}\n\n{}\n",
        app.import_report
            .source
            .as_deref()
            .map_or_else(|| "Current project".into(), |p| p.display().to_string()),
        lines(app).join("\n\n")
    );
    if let Err(e) = std::fs::write(path, text) {
        app.state.message = Some(format!("Could not export report: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_copy_report_and_explicit_sample_choice_survive_save_reload() {
        let root =
            std::env::temp_dir().join(format!("lemon-import-workflow-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("Source.flp");
        let copy = root.join("Source.lmn");
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        let mut file = os_flp::project::create(&app.state.project).unwrap();
        file.chunks
            .retain(|c| !matches!(&c.tag.0, b"LPCH" | b"LSCL" | b"LSKB"));
        let bytes = file.to_bytes().unwrap();
        std::fs::write(&original, &bytes).unwrap();
        app.import_report = Report::opened(&file, &original);
        app.state.file = Some(file);
        app.state.path = Some(original.clone());
        assert!(app.import_report.protects(&original));
        crate::dialog::fake::live(true);
        app.act(crate::app::Action::MenuItem(
            0,
            crate::views::menus::FILE_SAVE,
        ));
        assert!(matches!(
            app.state.dialogs.last().unwrap().body,
            crate::dialog::Body::File(_)
        ));
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        app.state.dialogs.clear();
        let a = root.join("A");
        let b = root.join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("Kick.wav"), b"a").unwrap();
        std::fs::write(b.join("Kick.wav"), b"b").unwrap();
        let old = root.join("gone/Kick.wav").to_string_lossy().into_owned();
        app.state.project.channels[0].sample_path = Some(old.clone());
        os_engine::sample::set_thread_search_folders(vec![a.clone(), b.clone()]);
        let before = app.state.project.channels[0].clone();
        locate(&mut app, 0);
        assert_eq!(
            app.state
                .playlist
                .popup
                .as_ref()
                .unwrap()
                .items
                .iter()
                .filter(|i| matches!(i.cmd, Cmd::UseMissingSample(..)))
                .count(),
            2
        );
        relink(&mut app, 0, &old, &b.join("Kick.wav"));
        assert!(
            app.state.project.channels[0]
                .sample_path
                .as_ref()
                .unwrap()
                .contains("B")
        );
        assert_eq!(
            app.state.project.channels[0].mixer_track,
            before.mixer_track
        );
        assert_eq!(app.state.project.channels[0].volume, before.volume);
        assert_eq!(app.state.project.channels[0].sound, before.sound);
        app.import_report.notes.push("Preserved import note".into());
        app.state.playlist.popup = None;
        app.save_to(copy.clone());
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        let saved = os_flp::FlpFile::parse(&std::fs::read(&copy).unwrap()).unwrap();
        let report = Report::opened(&saved, &copy);
        assert_eq!(report.source, Some(original));
        assert!(!report.protects(&copy));
        assert_eq!(report.notes, vec!["Preserved import note"]);
        assert!(
            root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("lemon-import-workflow-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn report_names_retained_unsupported_automation_and_has_bounded_pages() {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        let a = os_flp::project::new_automation(0, 0x000e000f, 0.5, 4.);
        app.state.project.automations.push(a);
        app.state
            .project
            .arrangements
            .push(os_flp::project::default_arrangement());
        app.state
            .project
            .arrangements
            .last_mut()
            .unwrap()
            .clips
            .push(os_model::Clip::new(
                0,
                384,
                0,
                os_model::ClipSource::Channel(0),
            ));
        assert!(lines(&app).iter().any(|s| s.contains("0x000e000f")));
        for i in 0..10 {
            app.import_report
                .notes
                .push(format!("Pending item {i}: {}", "long filename ".repeat(20)));
        }
        show(&mut app, 0);
        let mut canvas = os_ui::Canvas::new(640, 420, 1.).unwrap();
        app.paint(&mut canvas, None);
        if let Some(path)=std::env::var_os("LEMON_IMPORT_REPORT_PREVIEW"){canvas.pixmap.save_png(path).unwrap();}
        if let crate::dialog::Body::Msg(msg) = &app.state.dialogs.last().unwrap().body {
            assert!(msg.plain().chars().count() < 600);
            assert_eq!(msg.escape(), Answer::Cancel);
        } else {
            panic!("report not visible");
        }
    }
}
