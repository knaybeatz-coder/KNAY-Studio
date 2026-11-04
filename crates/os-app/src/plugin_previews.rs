// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in thumbnail generation. Each editor runs in a disposable, time-limited process.
use crate::app::App;
use std::{
    collections::{HashSet, VecDeque},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
const ARG: &str = "--capture-plugin-preview";
const TIMEOUT: Duration = Duration::from_secs(25);
#[derive(Clone, Default)]
pub struct Progress {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub created: usize,
    pub failed: usize,
    pub current: String,
    pub summary: String,
    pub revision: u64,
}
#[derive(Default)]
struct Job {
    progress: Progress,
    cancel: Arc<AtomicBool>,
    ready: VecDeque<String>,
}
fn job() -> &'static Mutex<Job> {
    static JOB: OnceLock<Mutex<Job>> = OnceLock::new();
    JOB.get_or_init(Default::default)
}
pub fn progress() -> Progress {
    job().lock().unwrap().progress.clone()
}
pub fn cancel() {
    job().lock().unwrap().cancel.store(true, Ordering::Release);
}
#[derive(Clone)]
struct Task {
    uid: String,
    name: String,
    path: PathBuf,
    format: os_host::Format,
    id: String,
}
fn tasks(app: &App) -> Vec<Task> {
    let mut seen = HashSet::new();
    app.state
        .plugdb
        .lists
        .gens
        .iter()
        .map(|e| (false, e))
        .chain(app.state.plugdb.lists.fx.iter().map(|e| (true, e)))
        .filter(|(_, e)| seen.insert(e.uid.clone()) && !crate::plugin_artwork::has_picture(e))
        .filter_map(|(fx, e)| {
            let a = if fx {
                app.plugins.effects.get(e.index)
            } else {
                app.plugins.available.get(e.index)
            }?;
            Some(Task {
                uid: e.uid.clone(),
                name: e.label.clone(),
                path: a.desc.path.clone(),
                format: a.desc.format,
                id: a.desc.id.clone(),
            })
        })
        .collect()
}
pub fn start(app: &mut App) {
    if !cfg!(windows) {
        app.state.message =
            Some("Automatic plugin pictures are currently available on Windows.".into());
        return;
    }
    if app.transport.playing() {
        app.state.message = Some("Stop playback before generating plugin pictures.".into());
        return;
    }
    let mut j = job().lock().unwrap();
    if j.progress.running {
        return;
    }
    let queue = tasks(app);
    let total = queue.len();
    let empty = app.state.plugdb.lists.gens.is_empty() && app.state.plugdb.lists.fx.is_empty();
    let revision = j.progress.revision + 1;
    j.cancel = Arc::new(AtomicBool::new(false));
    let cancel = j.cancel.clone();
    j.progress = Progress {
        running: total > 0,
        total,
        revision,
        summary: if empty {
            "No plugins listed yet. Use Manage plugins to find them first.".into()
        } else if total == 0 {
            "All listed plugins already have pictures.".into()
        } else {
            "Preparing plugin pictures…".into()
        },
        ..Default::default()
    };
    drop(j);
    app.invalidate();
    if total == 0 {
        return;
    }
    std::thread::spawn(move || run(queue, cancel));
}
fn update(f: impl FnOnce(&mut Progress)) {
    let mut j = job().lock().unwrap();
    f(&mut j.progress);
    j.progress.revision += 1;
}
fn run(queue: Vec<Task>, cancel: Arc<AtomicBool>) {
    let mut report=String::from("Lemon Studio automatic plugin previews\nOnly this capture's editor window is photographed. No desktop or microphone capture.\n\n");
    for task in queue {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        update(|p| p.current = task.name.clone());
        let result = capture_task(&task, &cancel);
        if cancel.load(Ordering::Acquire) && result.is_err() {
            break;
        }
        match result {
            Ok(()) => {
                let mut j = job().lock().unwrap();
                j.ready.push_back(task.uid.clone());
                j.progress.created += 1;
                report.push_str(&format!("Saved: {}\n", task.name));
            }
            Err(e) => {
                update(|p| p.failed += 1);
                report.push_str(&format!("Skipped: {} — {}\n", task.name, e));
            }
        }
        update(|p| {
            p.done += 1;
            p.summary = format!(
                "{} / {} checked · {} saved · {} skipped",
                p.done, p.total, p.created, p.failed
            );
        });
    }
    let p = progress();
    let summary = format!(
        "{}{} pictures saved · {} skipped. {}",
        if cancel.load(Ordering::Acquire) {
            "Cancelled — "
        } else {
            "Finished — "
        },
        p.created,
        p.failed,
        if p.failed > 0 {
            "Generate again to retry missing pictures."
        } else {
            "Existing pictures are kept."
        }
    );
    report.push_str(&format!("\n{summary}\n"));
    if let Some(path) = os_paths::user_file("Plugin pictures/Preview report.txt") {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, report);
    }
    update(|p| {
        p.running = false;
        p.current.clear();
        p.summary = summary;
    });
}
pub fn tick(app: &mut App) {
    if app.transport.playing() {
        cancel();
    }
    let mut j = job().lock().unwrap();
    let revision = j.progress.revision;
    if revision == app.state.plugin_picker.preview_revision {
        return;
    }
    let ready: Vec<_> = j.ready.drain(..).collect();
    drop(j);
    for uid in ready {
        if let Some(image) = crate::plugin_artwork::custom_picture(&uid) {
            app.state.plugin_picker.images.insert(uid, image);
        }
    }
    app.state.plugin_picker.preview_revision = revision;
    app.invalidate();
}
pub fn report(app: &mut App) {
    if let Some(path) = os_paths::user_file("Plugin pictures/Preview report.txt") {
        if path.is_file() {
            #[cfg(windows)]
            let result = Command::new("notepad.exe").arg(path).spawn();
            #[cfg(target_os = "macos")]
            let result = Command::new("open").arg(path).spawn();
            #[cfg(not(any(windows, target_os = "macos")))]
            let result = Command::new("xdg-open").arg(path).spawn();
            if let Err(e) = result {
                app.state.message = Some(format!("Cannot open preview report: {e}"));
            }
        }
    }
}
fn capture_task(task: &Task, cancel: &AtomicBool) -> Result<(), String> {
    let dest = crate::plugin_artwork::custom_path(&task.uid).ok_or("Cannot find picture folder")?;
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = dest.with_extension("capture.png");
    let error = temp.with_extension("error.txt");
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(&error);
    let mut cmd = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    cmd.arg(ARG)
        .arg(task.format.as_str())
        .arg(&task.path)
        .arg(&task.id)
        .arg(&temp);
    let result = supervise(&mut cmd, cancel, TIMEOUT).and_then(|()| {
        let image = crate::plugin_artwork::read(&temp)
            .ok_or("Editor did not produce a readable picture")?;
        if !usable(&image) {
            return Err(
                "Editor returned a blank picture (may require activation or a visible GPU window)"
                    .into(),
            );
        }
        // Never replace an existing user picture, even if it was assigned while this job ran.
        if crate::plugin_artwork::read(&dest).is_none() {
            crate::plugin_artwork::save_picture(&temp, &dest)?;
        }
        Ok(())
    });
    let result = result.map_err(|fallback| {
        let mut text = String::new();
        let _ = std::fs::File::open(&error).and_then(|f| f.take(2048).read_to_string(&mut text));
        if text.trim().is_empty() {
            fallback
        } else {
            text.chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .take(400)
                .collect()
        }
    });
    let _ = std::fs::remove_file(temp);
    let _ = std::fs::remove_file(error);
    result
}
fn supervise(cmd: &mut Command, cancel: &AtomicBool, timeout: Duration) -> Result<(), String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Cannot start preview helper: {e}"))?;
    #[cfg(windows)]
    let _group = match ProcessGroup::attach(&child) {
        Some(g) => g,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cannot isolate preview helper".into());
        }
    };
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) || start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Acquire) {
                "Cancelled"
            } else {
                "Editor timed out"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("Plugin preview helper failed or crashed".into())
                }
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}
fn usable(image: &crate::plugin_artwork::Image) -> bool {
    let (w, h, p) = image;
    if *w < 32 || *h < 24 || *w > 4096 || *h > 4096 || p.len() != w * h {
        return false;
    }
    let colors: HashSet<_> = p
        .iter()
        .step_by((p.len() / 4096).max(1))
        .map(|p| p & 0x00ffffff)
        .collect();
    colors.len() >= 16
}
/// Called before normal application/profile/audio startup in the disposable worker.
pub fn child_main(args: &[String]) -> ! {
    let result: Result<(), String> = if args.len() == 4 {
        #[cfg(windows)]
        {
            capture_window(args)
        }
        #[cfg(not(windows))]
        {
            Err("Automatic preview capture is not supported on this platform yet".to_string())
        }
    } else {
        Err("Invalid preview-helper arguments".into())
    };
    if let Err(e) = &result {
        if let Some(path) = args.get(3) {
            let _ = std::fs::write(Path::new(path).with_extension("error.txt"), e);
        }
    }
    // Plugin destructors may themselves hang: the parent watchdog covers cleanup too.
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}
#[cfg(windows)]
fn capture_window(args: &[String]) -> Result<(), String> {
    use std::ffi::c_void;
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, Diagnostics::Debug::*, LibraryLoader::GetModuleHandleW},
        UI::WindowsAndMessaging::*,
    };
    unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let name: Vec<u16> = "LemonPluginPreview\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err("Cannot create preview window class".into());
        }
        let host = Window(CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            name.as_ptr(),
            name.as_ptr(),
            WS_POPUP | WS_CLIPCHILDREN,
            -20000,
            -20000,
            640,
            480,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        ));
        if host.0.is_null() {
            return Err("Cannot create preview window".into());
        }
        let format = os_host::Format::parse(&args[0]).ok_or("Unknown plugin format")?;
        let library = crate::plugins::load_file(Path::new(&args[1]), format)?;
        // Select the exact class, including each individual Waves shell entry.
        if !library.plugins().iter().any(|d| d.id == args[2]) {
            return Err("Plugin class is no longer installed; rescan plugins".into());
        }
        let plugin = library.create(&args[2])?;
        {
            let mut p = plugin.lock().map_err(|_| "Plugin failed while loading")?;
            if !p.has_editor() {
                return Err("Plugin has no graphical editor".into());
            }
            p.activate(48000);
            let (w, h) = p.open_editor(host.0 as *mut c_void, 1.)?;
            if w < 32 || h < 24 || w > 4096 || h > 4096 {
                return Err("Unsupported editor size".into());
            }
            SetWindowPos(
                host.0,
                std::ptr::null_mut(),
                -20000,
                -20000,
                w as i32,
                h as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        ShowWindow(host.0, SW_SHOWNOACTIVATE);
        let mut last = None;
        let start = Instant::now();
        let mut attempted = 0;
        while start.elapsed() < Duration::from_secs(6) {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            {
                let mut p = plugin.lock().map_err(|_| "Plugin editor failed")?;
                p.idle();
                if let Some((w, h)) = p.editor_size() {
                    if w >= 32 && h >= 24 && w <= 4096 && h <= 4096 {
                        SetWindowPos(
                            host.0,
                            std::ptr::null_mut(),
                            -20000,
                            -20000,
                            w as i32,
                            h as i32,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            if start.elapsed() >= Duration::from_millis(1500 + attempted * 1500) {
                attempted += 1;
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(host.0, &mut r);
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                if w < 32 || h < 24 || w > 4096 || h > 4096 {
                    return Err("Unsupported editor size".into());
                }
                let dc = GetDC(host.0);
                if dc.is_null() {
                    return Err("Cannot read preview window".into());
                }
                let mem = CreateCompatibleDC(dc);
                let mut bmi: BITMAPINFO = std::mem::zeroed();
                bmi.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let mut bits = std::ptr::null_mut();
                let bmp =
                    CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
                if mem.is_null() || bmp.is_null() || bits.is_null() {
                    if !bmp.is_null() {
                        DeleteObject(bmp);
                    }
                    if !mem.is_null() {
                        DeleteDC(mem);
                    }
                    ReleaseDC(host.0, dc);
                    return Err("Cannot allocate preview image".into());
                }
                let old = SelectObject(mem, bmp);
                std::ptr::write_bytes(bits, 0, (w * h * 4) as usize);
                let ok = windows_sys::Win32::Storage::Xps::PrintWindow(host.0, mem, 3) != 0;
                let pixels =
                    std::slice::from_raw_parts(bits as *const u32, (w * h) as usize).to_vec();
                SelectObject(mem, old);
                DeleteObject(bmp);
                DeleteDC(mem);
                ReleaseDC(host.0, dc);
                let image = (w as usize, h as usize, pixels);
                if ok && usable(&image) {
                    last = Some(image);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        let image = last.ok_or(
            "Editor returned a blank picture; activation or a visible GPU window may be required",
        )?;
        let ratio = (480. / image.0 as f32).min(300. / image.1 as f32).min(1.);
        let (w, h) = (
            (image.0 as f32 * ratio).round().max(1.) as u32,
            (image.1 as f32 * ratio).round().max(1.) as u32,
        );
        let mut canvas = os_ui::Canvas::new(w, h, 1.)?;
        canvas.begin_frame(None);
        canvas.image(
            os_ui::Rect::new(0., 0., w as f32, h as f32),
            image.0,
            image.1,
            &image.2,
        );
        canvas
            .pixmap
            .save_png(&args[3])
            .map_err(|e| e.to_string())?;
        plugin
            .lock()
            .map_err(|_| "Plugin editor failed")?
            .close_editor();
        Ok(())
    }
}

/// Kill only this worker and its descendants if Lemon exits during setup/inference.
#[cfg(windows)]
struct ProcessGroup(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ProcessGroup {
    fn attach(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(h, child.as_raw_handle()) == 0
            {
                CloseHandle(h);
                return None;
            }
            Some(Self(h))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(mode: &str) -> Command {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "plugin_previews::tests::preview_child_fixture",
            "--ignored",
            "--nocapture",
        ]);
        cmd.env("LEMON_PREVIEW_FIXTURE", mode);
        cmd
    }
    #[test]
    #[ignore = "Only invoked as a disposable child by supervisor tests"]
    fn preview_child_fixture() {
        match std::env::var("LEMON_PREVIEW_FIXTURE").as_deref() {
            Ok("hang") => std::thread::sleep(Duration::from_secs(20)),
            Ok("fail") => std::process::exit(7),
            Ok("ok") => {}
            _ => panic!("fixture mode missing"),
        }
    }
    #[test]
    fn preview_supervisor_success_failure_timeout_and_cancel() {
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(supervise(&mut fixture("ok"), &cancel, Duration::from_secs(4)).is_ok());
        assert!(
            supervise(&mut fixture("fail"), &cancel, Duration::from_secs(4))
                .unwrap_err()
                .contains("failed")
        );
        let start = Instant::now();
        assert!(
            supervise(&mut fixture("hang"), &cancel, Duration::from_millis(200))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(4));
        let signal = cancel.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            signal.store(true, Ordering::Release);
        });
        let start = Instant::now();
        assert_eq!(
            supervise(&mut fixture("hang"), &cancel, Duration::from_secs(10)).unwrap_err(),
            "Cancelled"
        );
        assert!(start.elapsed() < Duration::from_secs(4));
        trigger.join().unwrap();
    }
    #[test]
    fn preview_images_reject_blank_and_queue_uses_class_identity() {
        assert!(!usable(&(80, 80, vec![0; 6400])));
        assert!(!usable(&(80, 80, vec![0xeeeeee; 6400])));
        let image =
            os_ui::Canvas::decode_png(include_bytes!("../assets/plugin-picker/citrus_default.png"))
                .unwrap();
        assert!(usable(&image));
        let mut app = App::new(None);
        app.plugins = crate::plugins::Plugins::builtins();
        for id in ["shell-class-a", "shell-class-b"] {
            let mut p = crate::plugin_db::fake(id, os_host::Kind::Effect);
            p.desc.id = id.into();
            p.desc.path = PathBuf::from("SameShell.dll");
            app.plugins.effects.push(p);
        }
        app.state.plugdb.rebuild(&app.plugins);
        let queue = tasks(&app);
        assert_eq!(
            queue.len(),
            2,
            "bundled images skipped; each shell class retained"
        );
        assert_ne!(queue[0].uid, queue[1].uid);
        assert_ne!(queue[0].id, queue[1].id);
        assert_eq!(queue[0].path, queue[1].path);
    }
    #[test]
    fn preview_generator_button_does_not_run_on_picker_open() {
        let mut app = App::new(None);
        let mut c = os_ui::Canvas::new(1280, 900, 1.).unwrap();
        crate::plugin_picker::open(&mut app, crate::plugin_menu::Target::All);
        app.paint(&mut c, None);
        assert!(!progress().running);
        if cfg!(windows) {
            assert!(app.hits.iter().any(|h| h.action
                == crate::app::Action::PluginPicker(crate::plugin_picker::Hit::Generate)));
        }
        if let Some(dir) = std::env::var_os("OS_SHOT_DIR") {
            c.pixmap
                .save_png(std::path::PathBuf::from(dir).join("automatic-preview-button.png"))
                .unwrap();
        }
    }
}
