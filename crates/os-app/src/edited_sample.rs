// SPDX-License-Identifier: GPL-3.0-or-later
use lemon_sampler::sample_drop::Snapshot;
use std::{io::BufWriter, path::PathBuf, sync::Mutex};
use winit::window::Window;

static TARGET: Mutex<Option<[i32; 4]>> = Mutex::new(None);
static CHANNEL_PITCH: Mutex<Vec<(usize, f64)>> = Mutex::new(Vec::new());
static RESULTS: Mutex<Vec<Result<PathBuf, String>>> = Mutex::new(Vec::new());

pub fn update_target(app: &crate::app::App, target: Option<(&Window, f64, f64)>) {
    let mut pitches = CHANNEL_PITCH.lock().unwrap();
    pitches.clear();
    for (row, hosted) in &app.plugins.hosted {
        let Some(window) = &hosted.window else {
            continue;
        };
        let Some(ch) = app.state.project.channels.get(*row) else {
            continue;
        };
        if ch.kind.is_sampler() && ch.sound.channel_pitch_applies() {
            let main = if ch.misc.main_pitch {
                i32::from(app.state.project.main_pitch.unwrap_or(0))
            } else {
                0
            };
            pitches.push((
                window.area() as usize,
                f64::from(ch.pitch_cents) + f64::from(main + ch.misc.pitch_follow_offset),
            ));
        }
    }
    drop(pitches);
    *TARGET.lock().unwrap() = target.and_then(|(window, top, width)| {
        if window.is_minimized() == Some(true) {
            return None;
        }
        let origin = window.inner_position().ok()?;
        Some([
            origin.x,
            origin.y + top.round() as i32,
            origin.x + width.round() as i32,
            origin.y + window.inner_size().height as i32,
        ])
    });
    lemon_sampler::sample_drop::connect(accepts, save);
}

fn accepts() -> bool {
    let Some([left, top, right, bottom]) = *TARGET.lock().unwrap() else {
        return false;
    };
    #[cfg(windows)]
    let point = unsafe {
        use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let mut p: POINT = std::mem::zeroed();
        (GetCursorPos(&mut p) != 0).then_some((p.x, p.y))
    };
    #[cfg(not(windows))]
    let point = crate::plugin_window::pointer();
    point.is_some_and(|(x, y)| x >= left && x < right && y >= top && y < bottom)
}

fn save(mut sample: Snapshot) {
    if let Some((_, cents)) = CHANNEL_PITCH
        .lock()
        .unwrap()
        .iter()
        .find(|(parent, _)| *parent == sample.editor_parent)
    {
        sample.add_channel_pitch(*cents);
    }
    std::thread::spawn(move || {
        let result = write(sample);
        RESULTS.lock().unwrap().push(result);
        os_host::wake();
    });
}

fn write(sample: Snapshot) -> Result<PathBuf, String> {
    let dir = os_paths::user_dir()
        .ok_or("No user sample folder")?
        .join("Samples")
        .join("Edited");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = sample
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Sample");
    for version in 1.. {
        let name = if version == 1 {
            format!("{stem} - edited.wav")
        } else {
            format!("{stem} - edited {version}.wav")
        };
        let path = dir.join(name);
        let file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        };
        let result = {
            let mut writer = BufWriter::new(file);
            sample.write(&mut writer)
        };
        match result {
            Ok(()) => return Ok(path),
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
        }
    }
    unreachable!()
}

pub fn finish(app: &mut crate::app::App) {
    for result in std::mem::take(&mut *RESULTS.lock().unwrap()) {
        app.state.message = Some(match result {
            Ok(path) => {
                let mut favorites = crate::browser::favorites();
                if !favorites.contains(&path) {
                    favorites.push(path.clone());
                }
                crate::prefs::set_text(
                    "browser_favorites",
                    &serde_json::to_string(&favorites).unwrap(),
                );
                app.state.browser_dirs.borrow_mut().clear();
                *app.state.browser_index.borrow_mut() = Default::default();
                format!(
                    "Saved {} to Favorites",
                    path.file_name().unwrap().to_string_lossy()
                )
            }
            Err(e) => format!("Could not save edited sample: {e}"),
        });
        app.invalidate();
    }
}
