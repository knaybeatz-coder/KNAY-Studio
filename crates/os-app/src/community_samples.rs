use crate::app::{Action, App, State, Ui};
use crate::icons::Icon;
use crate::views::browser::{self, Item, Row};
use os_ui::{Font, Rect};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, sync::mpsc};

const CATEGORIES: [&str; 7] = [
    "All categories",
    "Vocals",
    "Drums",
    "Bass",
    "Melody",
    "Loops",
    "FX",
];
pub const HEADER: f32 = 92.;
#[derive(Deserialize)]
struct Taxonomy {
    families: std::collections::BTreeMap<String, Vec<String>>,
    genres: Vec<String>,
}
fn taxonomy() -> &'static Taxonomy {
    static DATA: std::sync::OnceLock<Taxonomy> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../packaging/sample-exchange/taxonomy.json"
        ))
        .expect("valid sample taxonomy")
    })
}
fn types(category: usize) -> &'static [String] {
    CATEGORIES
        .get(category)
        .and_then(|c| taxonomy().families.get(*c))
        .map_or(&[], Vec::as_slice)
}
fn subtype(category: usize, index: usize) -> Option<&'static str> {
    index
        .checked_sub(1)
        .and_then(|i| types(category).get(i))
        .map(String::as_str)
}
fn genre(index: usize) -> Option<&'static str> {
    index
        .checked_sub(1)
        .and_then(|i| taxonomy().genres.get(i))
        .map(String::as_str)
}

#[derive(Clone, Deserialize, Serialize)]
struct Sample {
    id: String,
    title: String,
    creator: String,
    category: String,
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    peaks: Vec<u8>,
    votes: i64,
    time: i64,
    path: String,
}
struct Job {
    action: &'static str,
    id: String,
    result: mpsc::Receiver<Result<Value, String>>,
}
#[derive(Default)]
struct Form {
    mode: u8,
    focus: Option<usize>,
    text: [String; 2],
    file: String,
    category: usize,
    subtype: usize,
    genre: usize,
    agreed: bool,
}
#[derive(Default)]
pub struct View {
    items: Vec<Sample>,
    jobs: Vec<Job>,
    category: usize,
    subtype: usize,
    genre: usize,
    top: bool,
    token: String,
    form: Option<Form>,
    status: String,
    pub drops: Vec<(String, u16, u32)>,
}

pub fn rows(s: &State) -> Vec<Row> {
    let v = &s.community;
    let query = s.browser_search.to_lowercase();
    let mut items: Vec<_> = v
        .items
        .iter()
        .filter(|it| {
            (v.category == 0 || it.category == CATEGORIES[v.category])
                && subtype(v.category, v.subtype).is_none_or(|t| it.subtype == t)
                && genre(v.genre).is_none_or(|g| it.genre == g)
                && [&it.title, &it.creator, &it.category, &it.subtype, &it.genre]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query))
        })
        .collect();
    items.sort_by_key(|it| std::cmp::Reverse((if v.top { it.votes } else { 0 }, it.time)));
    items
        .into_iter()
        .map(|it| Row {
            key: it.id.clone(),
            depth: 0,
            label: format!(
                "{} · {} · {} · {} votes",
                it.title, it.creator, it.category, it.votes
            ),
            icon: Icon::FileAudio,
            cat: 254,
            open: None,
            bright: false,
            item: Item::Sample(it.path.clone()),
            path: Some(it.path.clone().into()),
        })
        .collect()
}

fn start(s: &mut State, action: &'static str, id: String, data: Value) {
    let v = &mut s.community;
    if v.jobs.iter().any(|job| {
        (job.action == action && job.id == id)
            || (!matches!(action, "fetch" | "catalog")
                && !matches!(job.action, "fetch" | "catalog"))
    }) {
        return;
    }
    let member = v.token.clone();
    let root = browser::samples_root();
    let (tx, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::community_service::run(action, data, root, member));
    });
    v.jobs.push(Job { action, id, result });
    v.status = match action {
        "catalog" => "Loading community samples…",
        "fetch" => "Downloading sample…",
        _ => "Working…",
    }
    .into();
}

pub fn choose(s: &mut State, path: &Path) {
    if let Some(f) = s.community.form.as_mut() {
        f.file = path.to_string_lossy().into_owned();
        if f.text[0].is_empty() {
            f.text[0] = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
    }
}
pub fn newsletter(s: &mut State, subscribe: bool) {
    start(
        s,
        "newsletter",
        String::new(),
        json!({"subscribe":subscribe}),
    );
}
pub fn refresh(s: &mut State) {
    start(s, "catalog", String::new(), json!({}));
}
pub fn fetch(s: &mut State, id: &str) {
    if let Some(item) = s.community.items.iter().find(|it| it.id == id).cloned() {
        start(s, "fetch", id.to_owned(), json!({"item":item}));
    }
}
pub fn set_category(s: &mut State, family: usize, kind: usize, upload: bool) {
    if family >= CATEGORIES.len() || kind > types(family).len() {
        return;
    }
    if upload {
        if let Some(f) = s.community.form.as_mut() {
            f.category = family;
            f.subtype = kind;
        }
    } else {
        s.community.category = family;
        s.community.subtype = kind;
        s.browser_scroll = 0.;
    }
}
pub fn set_genre(s: &mut State, index: usize, upload: bool) {
    if index > taxonomy().genres.len() {
        return;
    }
    if upload {
        if let Some(f) = s.community.form.as_mut() {
            f.genre = index;
        }
    } else {
        s.community.genre = index;
        s.browser_scroll = 0.;
    }
}
pub fn control(app: &mut App, command: u8) {
    if command == 12 {
        let dlg = crate::filedlg::FileDlg::new(
            crate::filedlg::Mode::Open,
            "Choose community audio",
            "audio",
            crate::dialog::Then::CommunitySample,
        )
        .filter(
            "Audio",
            &[
                "wav", "wave", "mp3", "flac", "ogg", "oga", "aif", "aiff", "aifc", "wv",
            ],
        );
        app.file_dialog(dlg);
        return;
    }
    if command == 7 {
        app.tell(
            crate::dialog::Kind::Info,
            "Royalty-free upload terms",
            crate::dialog::spans(&[include_str!(
                "../../../packaging/sample-exchange/license.txt"
            )]),
        );
        return;
    }
    if matches!(command, 0 | 13 | 17 | 18 | 19) {
        use crate::browser::BCmd;
        use crate::playlist::{Cmd, PItem, Popup};
        let upload = matches!(command, 13 | 18);
        let entry = |label: &str, cmd| PItem::new(label.to_owned(), Cmd::Browser(cmd));
        let mut items = Vec::new();
        if command == 19 {
            if app.state.community.token.is_empty() {
                items.push(entry("Sign in", BCmd::CommunityControl(5)));
                items.push(entry("Create account", BCmd::CommunityControl(6)));
            } else {
                items.push(entry("Email preferences", BCmd::CommunityControl(8)));
                items.push(entry("Sign out", BCmd::CommunityControl(5)));
            }
        } else if matches!(command, 17 | 18) {
            if !upload {
                items.push(entry("All genres", BCmd::CommunityGenre(0, false)));
            }
            for (i, g) in taxonomy().genres.iter().enumerate() {
                items.push(entry(g, BCmd::CommunityGenre(i + 1, upload)));
            }
        } else {
            if !upload {
                items.push(entry("All sounds", BCmd::CommunityCategory(0, 0, false)));
            }
            for (i, family) in CATEGORIES.iter().enumerate().skip(1) {
                let mut group = Vec::new();
                if !upload {
                    group.push(entry(
                        &format!("All {family}"),
                        BCmd::CommunityCategory(i, 0, false),
                    ));
                }
                for (k, name) in types(i).iter().enumerate() {
                    group.push(entry(name, BCmd::CommunityCategory(i, k + 1, upload)));
                }
                items.push(PItem::sub(*family, group));
            }
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
        return;
    }
    let s = &mut app.state;
    let v = &mut s.community;
    if command >= 10 {
        edit_form(s, command);
        app.invalidate();
        return;
    }
    match command {
        1 => {
            v.top = !v.top;
            s.browser_scroll = 0.;
        }
        2 => refresh(s),
        3 | 4 | 8 if v.token.is_empty() => v.status = "Sign in first.".into(),
        3 => {
            if let Some(id) = s.browser_sel.clone() {
                start(s, "vote", id.clone(), json!({"id":id}));
            } else {
                v.status = "Select a sample to vote.".into();
            }
        }
        5 if !v.token.is_empty() => {
            start(s, "logout", String::new(), json!({}));
            let v = &mut s.community;
            v.token.clear();
            v.status = "Signed out.".into();
        }
        4..=6 => {
            v.form = Some(Form {
                mode: command,
                focus: Some(0),
                category: v.category,
                subtype: v.subtype,
                genre: v.genre,
                ..Default::default()
            });
            s.browser_search_focus = false;
            v.status = String::new();
        }
        8 => start(s, "me", String::new(), json!({})),
        _ => {}
    }
    app.invalidate();
}
fn edit_form(s: &mut State, command: u8) {
    let v = &mut s.community;
    let Some(f) = v.form.as_mut() else {
        return;
    };
    if v.jobs
        .iter()
        .any(|j| matches!(j.action, "login" | "register" | "upload" | "choose"))
    {
        return;
    }
    match command {
        10 | 11 => f.focus = Some(usize::from(command - 10)),

        14 => f.agreed = !f.agreed,
        15 => {
            if f.text.iter().any(|t| t.trim().is_empty()) {
                v.status = "Complete both fields.".into();
                return;
            }
            let (action, data) = if f.mode == 4 {
                let (Some(kind), Some(style)) = (subtype(f.category, f.subtype), genre(f.genre))
                else {
                    v.status = "Choose a sound type and genre.".into();
                    return;
                };
                if !f.agreed || f.file.is_empty() {
                    v.status = "Choose audio and accept the license.".into();
                    return;
                }
                (
                    "upload",
                    json!({"title":f.text[0],"creator":f.text[1],"path":f.file,"category":CATEGORIES[f.category],"subtype":kind,"genre":style,"agree":true}),
                )
            } else {
                (
                    if f.mode == 5 { "login" } else { "register" },
                    json!({"email":f.text[0],"password":f.text[1]}),
                )
            };
            start(s, action, String::new(), data);
        }
        16 => v.form = None,
        _ => {}
    }
}
pub fn editing(s: &State) -> bool {
    s.show_browser
        && s.browser_tab == browser::EXCHANGE_BUTTON
        && s.community.form.as_ref().and_then(|f| f.focus).is_some()
}
pub fn blur(s: &mut State) {
    if let Some(f) = s.community.form.as_mut() {
        f.focus = None;
    }
}
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    if !editing(&app.state) {
        return false;
    }
    let f = app.state.community.form.as_mut().unwrap();
    let field = f.focus.unwrap();
    match key {
        Key::Named(NamedKey::Escape) => control(app, 16),
        Key::Named(NamedKey::Tab) => f.focus = Some(1 - field),
        Key::Named(NamedKey::Enter) => {
            if field == 0 {
                f.focus = Some(1);
            } else {
                control(app, 15);
            }
        }
        Key::Named(NamedKey::Backspace) => {
            f.text[field].pop();
        }
        Key::Named(NamedKey::Space) => f.text[field].push(' '),
        Key::Character(text) if !ctrl && !alt && f.text[field].len() < 256 => {
            f.text[field].push_str(text)
        }
        _ => {}
    }
    app.invalidate();
    true
}
pub fn height(s: &State) -> f32 {
    match s.community.form.as_ref().map(|f| f.mode) {
        Some(4) => 212.,
        Some(_) => 126.,
        None => HEADER,
    }
}
fn form_button(ui: &mut Ui, r: Rect, cmd: u8, label: &str, selected: bool) {
    let hover = ui.hit(r, Action::Community(cmd), label.to_owned());
    ui.c.fill_rrect(
        r,
        3.,
        if hover || selected {
            ui.t.menu_drop_hi
        } else {
            ui.t.browser_tabs
        },
    );
    ui.c.text_clip(
        Font::new(12),
        r.x + 4.,
        r.y + 2.,
        r.w - 8.,
        ui.t.glyph,
        label,
    );
}
fn paint_form(ui: &mut Ui, s: &State, f: &Form, width: f32, y: f32) {
    let line = |n: usize| Rect::new(3., y + n as f32 * 21., width - 6., 20.);
    let upload = f.mode == 4;
    let name = if upload {
        "Upload audio"
    } else if f.mode == 5 {
        "Sign in"
    } else {
        "Create account"
    };
    ui.c.text(Font::new(13), 5., y, ui.t.text, name);
    for i in 0..2 {
        let label = if upload {
            ["Title", "Creator"][i]
        } else {
            ["Email", "Password"][i]
        };
        let value = if !upload && i == 1 {
            "•".repeat(f.text[i].chars().count())
        } else {
            f.text[i].clone()
        };
        form_button(
            ui,
            line(i + 1),
            10 + i as u8,
            &format!("{label}: {value}"),
            f.focus == Some(i),
        );
    }
    if upload {
        let file = Path::new(&f.file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Choose audio…");
        form_button(ui, line(3), 12, file, false);
        form_button(
            ui,
            line(4),
            13,
            &format!(
                "Sound: {}",
                subtype(f.category, f.subtype).unwrap_or("Choose type…")
            ),
            false,
        );
        form_button(
            ui,
            line(5),
            18,
            &format!("Genre: {}", genre(f.genre).unwrap_or("Choose genre…")),
            false,
        );
        form_button(
            ui,
            line(6),
            14,
            if f.agreed {
                "[x] Royalty-free terms accepted"
            } else {
                "[ ] Accept royalty-free terms"
            },
            f.agreed,
        );
    }
    let r = line(if upload { 7 } else { 3 });
    let half = (r.w - 2.) / 2.;
    form_button(
        ui,
        Rect::new(r.x, r.y, half, r.h),
        15,
        if upload { "Publish sample" } else { name },
        false,
    );
    form_button(
        ui,
        Rect::new(r.x + half + 2., r.y, half, r.h),
        16,
        "Cancel",
        false,
    );
    let status = line(if upload { 8 } else { 4 });
    ui.c.text_clip(
        Font::new(11),
        status.x,
        status.y + 2.,
        status.w,
        ui.t.text_dim,
        &s.community.status,
    );
}
pub fn tick(app: &mut App) {
    let mut done = Vec::new();
    app.state
        .community
        .jobs
        .retain(|job| match job.result.try_recv() {
            Err(mpsc::TryRecvError::Empty) => true,
            reply => {
                done.push((
                    job.action,
                    job.id.clone(),
                    reply.unwrap_or_else(|_| Err("Community service stopped.".into())),
                ));
                false
            }
        });
    for (action, id, result) in done {
        let s = &mut app.state;
        match result {
            Err(e) => {
                s.community.status = e.clone();
                s.message = Some(e);
                if action == "fetch" {
                    s.community.drops.retain(|(key, _, _)| key != &id);
                }
            }
            Ok(reply) if action == "catalog" => {
                match serde_json::from_value(reply["items"].clone()) {
                    Ok(items) => {
                        s.community.items = items;
                        s.community.status = format!(
                            "{} samples · Click to preview or drag to Playlist",
                            s.community.items.len()
                        );
                    }
                    Err(_) => s.community.status = "Invalid catalog response.".into(),
                }
            }
            Ok(reply) if action == "fetch" => {
                if let Some(path) = reply["path"].as_str() {
                    let drops = std::mem::take(&mut s.community.drops);
                    for (key, track, tick) in drops {
                        if key == id {
                            if let Err(e) = crate::playlist::import_audio_clip(s, path, track, tick)
                            {
                                s.message = Some(e.into());
                            }
                        } else {
                            s.community.drops.push((key, track, tick));
                        }
                    }
                    s.community.status = "Ready · Drag to Playlist".into();
                    if s.browser_tab == browser::EXCHANGE_BUTTON
                        && s.browser_sel.as_deref() == Some(&id)
                    {
                        app.preview_sample(path);
                    }
                }
            }
            Ok(reply) => {
                if action == "me" {
                    let next = !reply["newsletter"].as_bool().unwrap_or(false);
                    let question = if next {
                        "Subscribe to Lemon Studio news by email?"
                    } else {
                        "Unsubscribe from Lemon Studio news by email?"
                    };
                    use crate::dialog::{spans, Answer, Kind, Msg, Then};
                    app.show_msg(Msg::new(
                        Kind::Question,
                        "Email preferences",
                        spans(&[question]),
                        &[Answer::Yes, Answer::No],
                        Then::CommunityNewsletter(next),
                    ));
                    app.invalidate();
                    continue;
                }
                if matches!(action, "register" | "login" | "upload") {
                    s.community.form = None;
                }
                if let Some(token) = reply["token"].as_str() {
                    s.community.token = token.to_owned();
                }
                s.community.status = reply["message"].as_str().unwrap_or("Ready").into();
                if matches!(action, "upload" | "vote") && reply["message"].is_string() {
                    refresh(s);
                }
            }
        }
        app.invalidate();
    }
}
pub fn pending(s: &State, path: &str) -> Option<String> {
    if s.browser_tab != browser::EXCHANGE_BUTTON || Path::new(path).is_file() {
        return None;
    }
    s.community
        .items
        .iter()
        .find(|it| it.path == path)
        .map(|it| it.id.clone())
}
pub fn paint(ui: &mut Ui, s: &State, width: f32, y: f32) {
    let v = &s.community;
    if let Some(f) = &v.form {
        paint_form(ui, s, f, width, y);
        return;
    }
    let half = (width - 8.) * 0.5;
    let cell = |col: f32, row: f32| Rect::new(3. + col * (half + 2.), y + row * 22., half, 20.);
    form_button(
        ui,
        cell(0., 0.),
        0,
        subtype(v.category, v.subtype).unwrap_or(CATEGORIES[v.category]),
        v.category != 0,
    );
    form_button(
        ui,
        cell(1., 0.),
        17,
        genre(v.genre).unwrap_or("All genres"),
        v.genre != 0,
    );
    form_button(
        ui,
        cell(0., 1.),
        1,
        if v.top {
            "Sort: Top voted"
        } else {
            "Sort: Latest"
        },
        false,
    );
    form_button(ui, cell(1., 1.), 2, "Refresh list", false);
    form_button(ui, cell(0., 2.), 4, "+ Upload audio", false);
    form_button(ui, cell(1., 2.), 19, "Account", false);
    let r = Rect::new(3., y + 66., width - 6., 21.);
    ui.hit(r, Action::BrowserList, v.status.clone());
    ui.c.text_clip(Font::new(11), r.x, r.y + 2., r.w, ui.t.text_dim, &v.status);
}

pub fn paint_row(ui: &mut Ui, s: &State, key: &str, r: Rect) {
    let Some(it) = s.community.items.iter().find(|it| it.id == key) else {
        return;
    };
    let x = r.x + 5.;
    let w = (r.w - 19.).max(1.);
    ui.c.text_clip(
        Font::new(15),
        x,
        r.y + 1.,
        (w - 42.).max(1.),
        ui.t.text,
        &it.title,
    );
    let vote = Rect::new(x + w - 41., r.y + 1., 40., 17.);
    let hover = ui.hit(
        vote,
        Action::CommunityVote(it.id.clone()),
        "Vote for this sample",
    );
    ui.c.fill_rrect(
        vote,
        3.,
        if hover {
            ui.t.menu_drop_hi
        } else {
            ui.t.browser_tabs
        },
    );
    ui.c.text_center(Font::new(11), vote, ui.t.glyph, &format!("+ {}", it.votes));
    let center = r.y + 31.;
    let step = w / it.peaks.len().max(1) as f32;
    for (i, &peak) in it.peaks.iter().enumerate() {
        let h = (f32::from(peak) / 255. * 19.).max(1.);
        ui.c.fill_rect(
            Rect::new(x + i as f32 * step, center - h / 2., step.max(1.), h),
            ui.t.browser_search,
        );
    }
    ui.c.text_clip(
        Font::new(11),
        x,
        r.y + 43.,
        w,
        ui.t.text_dim,
        &format!("{} · {} · {}", it.subtype, it.genre, it.creator),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Win, playlist as pl};

    #[test]
    fn community_browser_preview_and_first_playlist_drop() {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.project.arrangements.clear();
        app.state.z = vec![Win::Playlist];
        app.state.show_browser = true;
        let mut canvas = os_ui::Canvas::new(1920, 1080, 1.).unwrap();
        app.state.browser_tab = browser::EXCHANGE_BUTTON;
        let path =
            std::env::temp_dir().join(format!("community-test-{}.wav", getrandom::u64().unwrap()));
        let sample = json!({"id":"demo-2","title":"Test","creator":"Test","category":"Drums","subtype":"Kick","genre":"Cross-genre","votes":1,"time":1,"path":path,"peaks":[0,128,255]});
        app.state.community.items = serde_json::from_value(json!([sample])).unwrap();
        let (tx, result) = mpsc::channel();
        app.state.community.jobs.push(Job {
            action: "fetch",
            id: "demo-2".into(),
            result,
        });
        let drums = CATEGORIES.iter().position(|s| *s == "Drums").unwrap();
        let kick = types(drums).iter().position(|s| s == "Kick").unwrap() + 1;
        set_category(&mut app.state, drums, kick, false);
        assert!(rows(&app.state).iter().any(|row| row.key == "demo-2"));
        assert!(rows(&app.state).iter().all(|row| row.key != "demo-1"));
        let deep_house = taxonomy()
            .genres
            .iter()
            .position(|s| s == "Deep house")
            .unwrap()
            + 1;
        set_genre(&mut app.state, deep_house, false);
        assert!(!rows(&app.state).iter().any(|row| row.key == "demo-2"));
        set_genre(&mut app.state, 0, false);
        set_category(&mut app.state, 0, 0, false);
        for sample in app
            .state
            .community
            .items
            .iter()
            .filter(|s| s.id.starts_with("demo-"))
        {
            assert!(!sample.peaks.is_empty());
            assert!(sample.peaks.iter().min() < sample.peaks.iter().max());
        }
        let row = rows(&app.state)
            .into_iter()
            .next()
            .expect("catalog must load in Browser");
        assert!(
            !row.path.as_ref().unwrap().exists(),
            "run in a fresh test profile"
        );
        app.state.browser_search = "non-matching test search".into();
        assert!(rows(&app.state).is_empty());
        app.state.browser_search.clear();
        app.paint(&mut canvas, None);
        assert!(app.hits.iter().any(|h| h.action == Action::BrowserRow(0)));
        app.browser_act(&Action::BrowserRow(0));
        let grid = app.pl_grid().unwrap();
        let geom = pl::PGeom::new(&app.state, grid);
        let tick_pos = pl::bar(&app.state);
        app.mouse_move(geom.x_of(tick_pos as f32), geom.y_of(2) + 12.);
        app.browser_drop(row.item, true);
        let mut file = std::fs::File::create(&path).unwrap();
        os_engine::wav::write_f32_stereo(&mut file, 48000, &[0.1; 4800]).unwrap();
        drop(file);
        tx.send(Ok(json!({"path":path}))).unwrap();
        tick(&mut app);
        let clip = pl::clips(&app.state)
            .last()
            .expect("release before download still places audio");
        assert_eq!((clip.track, clip.position), (2, tick_pos));
        assert!(row.path.as_ref().unwrap().is_file());
        assert!(app.state.browser_preview.path.is_some());
        control(&mut app, 0);
        assert_eq!(app.state.playlist.popup.take().unwrap().items.len(), 7);
        control(&mut app, 1);
        assert!(app.state.community.top);
        control(&mut app, 5);
        assert!(editing(&app.state));
        assert!(key(
            &mut app,
            &winit::keyboard::Key::Character("sample".into()),
            false,
            false
        ));
        assert_eq!(app.state.community.form.as_ref().unwrap().text[0], "sample");
        control(&mut app, 16);
        assert!(app.state.community.form.is_none());
        let _ = std::fs::remove_file(path);
    }
}
