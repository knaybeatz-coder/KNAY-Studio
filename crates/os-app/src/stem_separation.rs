// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in, private stem runtime. Downloads and inference never run on the UI/audio thread.
use crate::{
    app::{App, State},
    dialog::{spans, Answer, Body, Kind, Msg, Then},
    playlist as pl,
};
use os_model::{Channel, Clip, ClipSource, PlaylistTrack};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
const REV: &str = "demucs-4.0.1-torch-2.5.1-v1";
const STEMS: [&str; 4] = ["vocals", "drums", "bass", "other"];
const LABELS: [&str; 4] = ["Vocals", "Drums", "Bass", "Instruments"];
fn indices(selected: u8) -> impl Iterator<Item = usize> {
    (0..4).filter(move |n| selected & (1 << n) != 0)
}
fn names(selected: u8) -> String {
    indices(selected)
        .map(|n| LABELS[n])
        .collect::<Vec<_>>()
        .join(", ")
}
pub fn toggle(selected: &mut u8, n: usize) {
    if n == 4 {
        *selected = if *selected == 15 { 0 } else { 15 };
    } else if n < 4 {
        *selected ^= 1 << n;
    }
}
const RUNNER: &str = include_str!("stem_worker.py");

pub struct Job {
    rx: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    clip: usize,
    original: Clip,
    channel: Channel,
    started: Instant,
    phase: String,
    shown: String,
    selected: u8,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
enum Event {
    Phase(String),
    Done(Result<PathBuf>),
}
fn root() -> Result<PathBuf> {
    Ok(os_paths::user_dir()
        .ok_or("No user data folder")?
        .join("Stem separation")
        .join(REV))
}
fn python(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "environment/Scripts/python.exe"
    } else {
        "environment/bin/python"
    })
}
fn ready(root: &Path) -> bool {
    python(root).is_file()
        && fs::read_to_string(root.join("ready.txt")).is_ok_and(|s| s == REV)
        && root.join("models/955717e8-8726e21a.th").is_file()
}
fn source(s: &State, i: usize) -> Result<(Clip, Channel)> {
    let c = *pl::clips(s)
        .get(i)
        .ok_or("The sample is no longer in the playlist")?;
    if pl::source_kind(s, c.source) != pl::PickKind::Audio {
        return Err("Choose an audio clip".into());
    }
    let ClipSource::Channel(n) = c.source else {
        return Err("Choose an audio clip".into());
    };
    let ch = s
        .project
        .channels
        .iter()
        .find(|ch| ch.index == n)
        .ok_or("Sample channel not found")?
        .clone();
    let path = ch.sample_path.as_deref().ok_or("This clip has no sample")?;
    if !Path::new(&crate::plugins::resolve_sample(path)).is_file() {
        return Err("The sample file is missing. Locate it before separating stems.".into());
    }
    Ok((c, ch))
}
fn destinations(s: &State, after: u16, count: usize) -> Result<Vec<u16>> {
    let rows: Vec<_> = (after.saturating_add(1)..pl::TRACKS)
        .chain(0..=after.min(pl::TRACKS - 1))
        .filter(|&r| {
            !pl::clips(s).iter().any(|c| c.track == r)
                && pl::track_info(s, r).is_none_or(|t| {
                    t.enabled
                        && !t.locked
                        && !t.grouped
                        && !t.collapsed
                        && t.comp.is_none()
                        && t.name.is_none()
                })
                && !s.playlist.sampler_rows.values().any(|&v| v == r)
                && !s.project.instrument_lanes.iter().any(|l| l.2 == r)
        })
        .take(count)
        .collect();
    if rows.len() != count {
        return Err(format!(
            "{count} empty playlist track(s) are needed for the selected stems"
        ));
    }
    Ok(rows)
}
fn error(app: &mut App, text: &str) {
    app.tell(Kind::Error, "Stem separation", spans(&[text]));
    app.invalidate();
}
pub fn request(app: &mut App, i: usize) {
    if app.stem_job.is_some() {
        return;
    }
    if let Err(e) = source(&app.state, i) {
        return error(app, &e);
    }
    if let Err(e) = uv_asset() {
        return error(app, &e);
    }
    choose(app, i, 15);
}
fn choose(app: &mut App, i: usize, selected: u8) {
    app.show_msg(
        Msg::new(
            Kind::Question,
            "Stem separation",
            spans(&[
                "Choose the stems you want to keep.\nSelect any combination, then click Separate.",
            ]),
            &[Answer::Ok, Answer::Cancel],
            Then::StemChoose(i, selected),
        )
        .labelled(&["Separate", "Cancel"]),
    );
    app.invalidate();
}
pub fn selected(app: &mut App, i: usize, selected: u8) {
    let selected = selected & 15;
    if selected == 0 {
        return choose(app, i, selected);
    }
    let (clip, _) = match source(&app.state, i) {
        Ok(v) => v,
        Err(e) => return error(app, &e),
    };
    if let Err(e) = destinations(&app.state, clip.track, selected.count_ones() as usize) {
        return error(app, &e);
    }
    let r = match root() {
        Ok(r) => r,
        Err(e) => return error(app, &e),
    };
    if ready(&r) {
        start(app, i, false, selected);
    } else {
        app.show_msg(Msg::new(Kind::Question,"Download stem separation?",spans(&[
            "Would you like to download the stem separation engine and AI model?\n\nThis one-time setup needs internet and several GB of free space. It installs only for Lemon Studio.\n\nThen your selected stems will be saved as separate tracks. Your audio stays on your computer."
        ]),&[Answer::Yes,Answer::No],Then::StemDownload(i, selected)));
        app.invalidate();
    }
}
pub fn start(app: &mut App, i: usize, install: bool, selected: u8) {
    if app.stem_job.is_some() {
        return;
    }
    if selected == 0 || selected & !15 != 0 {
        return choose(app, i, 0);
    }
    let (clip, channel) = match source(&app.state, i) {
        Ok(v) => v,
        Err(e) => return error(app, &e),
    };
    let r = match root() {
        Ok(v) => v,
        Err(e) => return error(app, &e),
    };
    let input = PathBuf::from(crate::plugins::resolve_sample(
        channel.sample_path.as_deref().unwrap(),
    ));
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let (tx, rx) = mpsc::channel();
    app.stem_job = Some(Job {
        rx,
        cancel,
        clip: i,
        original: clip,
        channel,
        started: Instant::now(),
        phase: "Preparing stem separation...".into(),
        shown: String::new(),
        selected,
    });
    app.show_msg(Msg::new(
        Kind::Info,
        "Stem separation",
        spans(&["Preparing stem separation..."]),
        &[Answer::Cancel],
        Then::StemCancel,
    ));
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            work(&r, &input, install, &stop, &tx, selected)
        }))
        .unwrap_or_else(|_| {
            Err("The separation worker stopped unexpectedly. Please try again.".into())
        });
        let _ = tx.send(Event::Done(result));
    });
    app.invalidate();
}
pub fn cancel(app: &mut App) {
    if let Some(job) = &app.stem_job {
        job.cancel.store(true, Ordering::Relaxed);
        // Keep the project protected until the child has stopped; no late import into another project.
        app.show_msg(Msg::new(
            Kind::Info,
            "Stem separation",
            spans(&["Stopping stem separation..."]),
            &[],
            Then::StemCancel,
        ));
    }
    app.invalidate();
}
pub fn tick(app: &mut App) {
    let Some(job) = &mut app.stem_job else { return };
    let mut done = None;
    loop {
        match job.rx.try_recv() {
            Ok(Event::Phase(s)) => job.phase = s,
            Ok(Event::Done(r)) => {
                done = Some(r);
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                done = Some(Err("The stem worker disconnected".into()));
                break;
            }
            Err(mpsc::TryRecvError::Empty) => break,
        }
    }
    if let Some(result) = done {
        let job = app.stem_job.take().unwrap();
        app.state
            .dialogs
            .retain(|m| !matches!(&m.body,Body::Msg(msg) if msg.then==Then::StemCancel));
        if job.cancel.load(Ordering::Relaxed) {
            app.state.message =
                Some("Stem separation cancelled; the original sample is unchanged".into());
        } else {
            match result.and_then(|out| import(&mut app.state,job.clip,job.original,&job.channel,&out,job.selected)) {
            Ok(())=>app.state.message=Some(format!("Stems ready: {}. Original clip muted; Undo restores it.", names(job.selected))),
            Err(e)=>error(app,&format!("{e}\n\nYour original sample is unchanged. Choose Stem separation again to retry. Details are in the Stem separation folder's setup.log.")),
        }
        }
        app.modal_changed();
        return;
    }
    let text = if job.cancel.load(Ordering::Relaxed) {
        "Stopping stem separation...".into()
    } else {
        format!(
            "{}\n\nElapsed: {}:{:02}\nYour audio stays on this computer. You can cancel safely.",
            job.phase,
            job.started.elapsed().as_secs() / 60,
            job.started.elapsed().as_secs() % 60
        )
    };
    if job.shown != text {
        job.shown = text.clone();
        if let Some(modal) = app.state.dialogs.last_mut() {
            if let Body::Msg(m) = &mut modal.body {
                m.text = spans(&[&text]);
            }
        }
        app.invalidate();
    }
}
fn import(
    s: &mut State,
    i: usize,
    original: Clip,
    channel: &Channel,
    out: &Path,
    selected: u8,
) -> Result<()> {
    if pl::clips(s).get(i) != Some(&original)
        || s.project.channels.iter().find(|c| c.index == channel.index) != Some(channel)
    {
        return Err("The source clip changed during separation. The completed WAV files are saved in the Stem separation folder.".into());
    }
    if selected == 0 || selected & !15 != 0 {
        return Err("Choose at least one stem".into());
    }
    let count = selected.count_ones() as usize;
    let rows = destinations(s, original.track, count)?;
    let next = s
        .project
        .channels
        .iter()
        .map(|c| c.index)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("No free audio channels")?;
    if next > u16::MAX - 4 {
        return Err("No free audio channels".into());
    }
    for n in indices(selected) {
        let stem = STEMS[n];
        if !out.join(format!("{stem}.wav")).is_file() {
            return Err("The separator did not produce all selected stems".into());
        }
    }
    let first = s.project.channels.len();
    let extra = crate::history::Snap::Tail(Box::new(crate::history::Tail::now(s, first)));
    pl::snapshot_with(s, "Stem separation", extra);
    pl::arr_mut(s).clips[i].muted = true;
    for (offset, (n, row)) in indices(selected).zip(rows.iter().copied()).enumerate() {
        let mut ch = channel.clone();
        ch.index = next + offset as u16;
        ch.kind = os_model::ChannelKind::AudioClip;
        ch.plugin = None;
        ch.plugin_state = None;
        ch.name = Some(format!(
            "{} - {}",
            channel.name.as_deref().unwrap_or("Sample"),
            LABELS[n]
        ));
        ch.sample_path = Some(
            out.join(format!("{}.wav", STEMS[n]))
                .to_string_lossy()
                .into_owned(),
        );
        let mut c = original;
        c.source = ClipSource::Channel(ch.index);
        c.track = row;
        let arr = pl::arr_mut(s);
        let number = u32::from(row) + 1;
        if let Some(t) = arr.tracks.iter_mut().find(|t| t.number == number) {
            t.name = ch.name.clone();
            t.color = ch.color;
        } else {
            let mut t = PlaylistTrack::new(number);
            t.name = ch.name.clone();
            t.color = ch.color;
            arr.tracks.push(t);
        }
        arr.clips.push(c);
        s.project.channels.push(ch);
    }
    s.playlist.sel = (pl::clips(s).len() - count..pl::clips(s).len()).collect();
    s.piano.dirty = true;
    Ok(())
}
fn check(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Relaxed) {
        Err("Cancelled".into())
    } else {
        Ok(())
    }
}
fn phase(tx: &mpsc::Sender<Event>, s: &str) {
    let _ = tx.send(Event::Phase(s.into()));
}
fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|e| e.to_string())
}
fn command(exe: &Path, root: &Path) -> Command {
    let mut c = Command::new(exe);
    c.current_dir(root)
        .env("UV_PYTHON_INSTALL_DIR", root.join("python"))
        .env("UV_CACHE_DIR", root.join("cache"))
        .env("UV_PYTHON_PREFERENCE", "only-managed")
        .env("UV_NO_CONFIG", "1")
        .env("PYTHONNOUSERSITE", "1")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env("TORCH_HOME", root.join("torch"))
        .env("OMP_NUM_THREADS", "4");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c
}
fn run(mut c: Command, root: &Path, stop: &AtomicBool) -> Result<()> {
    check(stop)?;
    let log = io(fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("setup.log")))?;
    c.stdout(Stdio::from(io(log.try_clone())?))
        .stderr(Stdio::from(log))
        .stdin(Stdio::null());
    let mut child = io(c.spawn())?;
    #[cfg(windows)]
    let _guard = ProcessGroup::attach(&child);
    loop {
        if check(stop).is_err() {
            #[cfg(windows)]
            {
                let mut kill = command(Path::new("taskkill.exe"), root);
                let _ = kill
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-TERM", "--", &format!("-{}", child.id())])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled".into());
        }
        if let Some(status) = io(child.try_wait())? {
            return if status.success() {
                Ok(())
            } else {
                Err("The separation engine could not finish this step. Check the internet connection and available disk space.".into())
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn uv_asset() -> Result<(&'static str, &'static str)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok((
            "uv-x86_64-pc-windows-msvc.zip",
            "515dc53d7553f1357d0abc1f70acd921fbb9e30230b1d9a08737236daa6ee920",
        )),
        ("linux", "x86_64") => Ok((
            "uv-x86_64-unknown-linux-gnu.tar.gz",
            "2cf10babba653310606f8b49876cfb679928669e7ddaa1fb41fb00ce73e64f66",
        )),
        ("macos", "aarch64") => Ok((
            "uv-aarch64-apple-darwin.tar.gz",
            "dc098ff224d78ed418e121fd374f655949d2c7031a70f6f6604eaf016a130433",
        )),

        _ => Err("Stem separation is not yet available for this system".into()),
    }
}
fn download(
    url: &str,
    dest: &Path,
    expected: &str,
    stop: &AtomicBool,
    tx: &mpsc::Sender<Event>,
    label: &str,
) -> Result<()> {
    check(stop)?;
    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(30))
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response
        .header("Content-Length")
        .and_then(|n| n.parse::<u64>().ok());
    let mut r = response.into_reader();
    let part = dest.with_extension("download");
    let mut out = io(fs::File::create(&part))?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut count = 0u64;
    let mut last = Instant::now();
    loop {
        check(stop)?;
        let n = io(r.read(&mut buf))?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > 512 * 1024 * 1024 {
            return Err("Download exceeded the expected size".into());
        }
        io(out.write_all(&buf[..n]))?;
        hash.update(&buf[..n]);
        if last.elapsed() > Duration::from_millis(250) {
            phase(
                tx,
                &format!(
                    "{label}: {} MB{}",
                    count / 1048576,
                    total
                        .map(|t| format!(" / {} MB", t / 1048576))
                        .unwrap_or_default()
                ),
            );
            last = Instant::now();
        }
    }
    let actual = format!("{:x}", hash.finalize());
    if actual != expected {
        return Err("Download verification failed. Please try again.".into());
    }
    io(out.sync_all())?;
    drop(out);
    if dest.exists() {
        io(fs::remove_file(dest))?;
    }
    io(fs::rename(part, dest))
}
fn install(root: &Path, stop: &AtomicBool, tx: &mpsc::Sender<Event>) -> Result<()> {
    let (asset, hash) = uv_asset()?;
    let archive = root.join(asset);
    let uv = root.join(if cfg!(windows) { "uv.exe" } else { "uv" });
    phase(tx, "Downloading the setup tool...");
    // Re-verify the bootstrap archive on each repair; never execute an unchecked download.
    download(
        &format!("https://github.com/astral-sh/uv/releases/download/0.9.5/{asset}"),
        &archive,
        hash,
        stop,
        tx,
        "Downloading setup tool",
    )?;
    if cfg!(windows) {
        let mut z =
            zip::ZipArchive::new(io(fs::File::open(&archive))?).map_err(|e| e.to_string())?;
        let mut member = z.by_name("uv.exe").map_err(|e| e.to_string())?;
        io(std::io::copy(&mut member, &mut io(fs::File::create(&uv))?))?;
    } else {
        let member = format!("{}/uv", asset.trim_end_matches(".tar.gz"));
        let mut c = command(Path::new("tar"), root);
        c.arg("-xzf")
            .arg(&archive)
            .arg("--strip-components=1")
            .arg(&member);
        run(c, root, stop)?;
    }
    phase(tx, "Downloading the private Python runtime...");
    let mut c = command(&uv, root);
    c.args(["python", "install", "3.11.14", "--no-bin"]);
    run(c, root, stop)?;
    if !python(root).is_file() {
        let mut c = command(&uv, root);
        c.args(["venv", "--python", "3.11.14"])
            .arg(root.join("environment"));
        run(c, root, stop)?;
    }
    phase(
        tx,
        "Downloading and installing the audio engine (this can take several minutes)...",
    );
    let mut c = command(&uv, root);
    c.args(["pip", "install", "--python"])
        .arg(python(root))
        .arg("--constraint")
        .arg("constraints.txt")
        .args([
            "--index-url",
            "https://pypi.org/simple",
            "torch==2.5.1",
            "torchaudio==2.5.1",
        ]);
    if !cfg!(target_os = "macos") {
        c.args(["--torch-backend", "cpu"]);
    }
    run(c, root, stop)?;
    let mut c = command(&uv, root);
    c.args(["pip", "install", "--python"])
        .arg(python(root))
        .arg("--constraint")
        .arg("constraints.txt")
        .args([
            "--index-url",
            "https://pypi.org/simple",
            "demucs==4.0.1",
            "numpy==1.26.4",
            "soundfile==0.13.1",
            "torch==2.5.1",
            "torchaudio==2.5.1",
        ]);
    run(c, root, stop)?;
    io(fs::create_dir_all(root.join("models")))?;
    phase(tx, "Downloading the four-stem AI model...");
    download(
        "https://dl.fbaipublicfiles.com/demucs/hybrid_transformer/955717e8-8726e21a.th",
        &root.join("models/955717e8-8726e21a.th"),
        "8726e21a993978c7ba086d3872e7608d7d5bfca646ca4aca459ffda844faa8b4",
        stop,
        tx,
        "Downloading AI model",
    )?;
    phase(tx, "Checking the separation engine...");
    let mut c = command(&python(root), root);
    c.arg(root.join("separate.py")).arg("--check");
    run(c, root, stop)?;
    check(stop)?;
    io(fs::write(root.join("ready.txt"), REV))
}
fn work(
    root: &Path,
    input: &Path,
    allow_install: bool,
    stop: &AtomicBool,
    tx: &mpsc::Sender<Event>,
    selected: u8,
) -> Result<PathBuf> {
    check(stop)?;
    io(fs::create_dir_all(root))?;
    let lock = io(fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("runtime.lock")))?;
    lock.try_lock().map_err(|_| {
        "Stem separation is already running in another Lemon Studio window".to_string()
    })?;
    io(fs::write(root.join("separate.py"), RUNNER))?;
    io(fs::write(
        root.join("constraints.txt"),
        include_str!("stem_constraints.txt"),
    ))?;
    if !ready(root) {
        if !allow_install {
            return Err(
                "The separation engine needs repair. Choose Stem separation again to download it."
                    .into(),
            );
        }
        install(root, stop, tx)?;
    }
    check(stop)?;
    phase(tx, "Preparing the sample...");
    let sample = os_engine::sample::load_shared(input)
        .map_err(|e| format!("Could not decode the sample: {e}"))?;
    if sample.frames() == 0 {
        return Err("The sample is empty".into());
    }
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let out = root
        .join("Separated")
        .join(format!("{}-{id}", std::process::id()));
    io(fs::create_dir_all(&out))?;
    let wav = out.join("source.wav");
    let mut f = std::io::BufWriter::new(io(fs::File::create(&wav))?);
    io(os_engine::wav::write_f32_stereo_header(
        &mut f,
        sample.rate,
        sample.frames() as u64,
    ))?;
    for (n, (&l, &r)) in sample.left.iter().zip(sample.right.iter()).enumerate() {
        if n % 65536 == 0 {
            check(stop)?;
        }
        io(f.write_all(&l.to_le_bytes()))?;
        io(f.write_all(&r.to_le_bytes()))?;
    }
    io(f.flush())?;
    drop(f);
    drop(sample);
    phase(tx, &format!("Separating {}...", names(selected)));
    let mut c = command(&python(root), root);
    c.arg(root.join("separate.py")).arg(&wav).arg(&out).arg(
        indices(selected)
            .map(|n| STEMS[n])
            .collect::<Vec<_>>()
            .join(","),
    );
    let result = run(c, root, stop);
    let _ = fs::remove_file(wav);
    if result.is_err() && !stop.load(Ordering::Relaxed) {
        let _ = fs::remove_file(root.join("ready.txt"));
    }
    result?;
    check(stop)?;
    // Never import a partial/corrupt run. Each output must have exactly the source duration.
    let reference = os_engine::sample::load_shared(input).map_err(|e| e.to_string())?;
    for n in indices(selected) {
        let stem = STEMS[n];
        let s = os_engine::sample::load_shared(&out.join(format!("{stem}.wav")))
            .map_err(|e| e.to_string())?;
        if s.frames() != reference.frames()
            || s.rate != reference.rate
            || s.left.iter().chain(s.right.iter()).any(|v| !v.is_finite())
        {
            return Err("The separated audio failed validation".into());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (App, PathBuf, usize) {
        let dir = std::env::temp_dir().join(format!(
            "lemon-stem-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("input.wav");
        let audio: Vec<f32> = (0..44100)
            .flat_map(|n| {
                let v = (n as f32 * 440. * std::f32::consts::TAU / 44100.).sin() * 0.1;
                [v, v]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut fs::File::create(&wav).unwrap(), 44100, &audio)
            .unwrap();
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        let i = pl::add_audio_clip(&mut a.state, wav.to_str().unwrap(), 8, 384).unwrap();
        (a, dir, i)
    }
    #[test]
    fn stems_menu_decline_import_alignment_and_undo_redo() {
        let (mut a, dir, i) = fixture();
        assert!(pl::clip_menu(&a.state, i, 0., 0.)
            .items
            .iter()
            .flat_map(|x| std::iter::once(x).chain(x.children.iter()))
            .any(|x| x.label == "Stem separation..." && x.cmd == pl::Cmd::SeparateStems(i)));
        let before = a.state.project.clone();
        a.dialog_done(
            Then::StemDownload(i, 15),
            crate::dialog::Reply::Button(Answer::No),
        );
        assert!(a.stem_job.is_none());
        assert_eq!(before, a.state.project);
        let mut original = pl::clips(&a.state)[i];
        original.window = Some((125., 750.));
        original.length = 120;
        pl::arr_mut(&mut a.state).clips[i] = original;
        let (_, ch) = source(&a.state, i).unwrap();
        let before = a.state.project.clone();
        for stem in STEMS {
            fs::copy(dir.join("input.wav"), dir.join(format!("{stem}.wav"))).unwrap();
        }
        import(&mut a.state, i, original, &ch, &dir, 15).unwrap();
        assert!(pl::clips(&a.state)[i].muted);
        for c in pl::clips(&a.state).iter().rev().take(4) {
            assert_eq!(c.position, 384);
            assert_eq!(c.length, 120);
            assert_eq!(c.window, original.window);
            assert!(!c.muted);
            assert_ne!(c.track, original.track);
        }
        let after = a.state.project.clone();
        assert_eq!(after.channels.len(), before.channels.len() + 4);
        crate::history::undo(&mut a.state);
        assert_eq!(before.channels, a.state.project.channels);
        assert_eq!(before.arrangements, a.state.project.arrangements);
        crate::history::redo(&mut a.state);
        assert_eq!(after.channels, a.state.project.channels);
        assert_eq!(after.arrangements, a.state.project.arrangements);
        let changed = Clip {
            position: 999,
            ..original
        };
        assert!(import(&mut a.state, i, changed, &ch, &dir, 15).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cancellation_and_incomplete_runtime_never_mark_ready() {
        let (mut a, dir, i) = fixture();
        fs::write(dir.join("ready.txt"), REV).unwrap();
        assert!(!ready(&dir));
        let stop = AtomicBool::new(true);
        let (tx, _) = mpsc::channel();
        assert!(work(&dir, &dir.join("input.wav"), false, &stop, &tx, 15).is_err());
        let (original, ch) = source(&a.state, i).unwrap();
        let before = a.state.project.clone();
        assert!(import(&mut a.state, i, original, &ch, &dir, 15).is_err());
        assert_eq!(before, a.state.project);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn selected_stems_dialog_and_import() {
        for mask in [1u8, 2, 4, 8, 5, 10, 15] {
            let (mut app, dir, i) = fixture();
            let before = app.state.project.clone();
            request(&mut app, i);
            assert!(
                matches!(&app.state.dialogs.last().unwrap().body,Body::Msg(m) if m.then==Then::StemChoose(i,15))
            );
            app.dialog_click(crate::dialog::DHit::StemToggle(4));
            assert!(
                matches!(&app.state.dialogs.last().unwrap().body,Body::Msg(m) if m.then==Then::StemChoose(i,0))
            );
            app.dialog_click(crate::dialog::DHit::Button(0));
            assert!(app.stem_job.is_none());
            assert_eq!(app.state.project, before);
            for n in indices(mask) {
                app.dialog_click(crate::dialog::DHit::StemToggle(n));
            }
            assert!(
                matches!(&app.state.dialogs.last().unwrap().body,Body::Msg(m) if m.then==Then::StemChoose(i,mask))
            );
            app.dialog_click(crate::dialog::DHit::Button(1));
            assert!(app.state.dialogs.is_empty());
            assert!(app.stem_job.is_none());
            let (original, ch) = source(&app.state, i).unwrap();
            // Only selected output files exist: importing must not require the others.
            for n in indices(mask) {
                fs::copy(dir.join("input.wav"), dir.join(format!("{}.wav", STEMS[n]))).unwrap();
            }
            import(&mut app.state, i, original, &ch, &dir, mask).unwrap();
            let added = &app.state.project.channels[before.channels.len()..];
            assert_eq!(added.len(), mask.count_ones() as usize);
            for (channel, n) in added.iter().zip(indices(mask)) {
                assert!(channel
                    .sample_path
                    .as_ref()
                    .unwrap()
                    .ends_with(&format!("{}.wav", STEMS[n])));
            }
            let after = app.state.project.clone();
            crate::history::undo(&mut app.state);
            assert_eq!(app.state.project.channels, before.channels);
            assert_eq!(app.state.project.arrangements, before.arrangements);
            crate::history::redo(&mut app.state);
            assert_eq!(app.state.project.channels, after.channels);
            assert_eq!(app.state.project.arrangements, after.arrangements);
            fs::remove_dir_all(dir).unwrap();
        }
        let (mut app, dir, i) = fixture();
        let (original, ch) = source(&app.state, i).unwrap();
        let tracks=&mut pl::arr_mut(&mut app.state).tracks;
        for t in tracks.iter_mut() {t.name=Some("Reserved".into());}
        tracks[20].name=None;
        assert_eq!(
            destinations(&app.state, original.track, 1).unwrap(),
            vec![20]
        );
        assert!(destinations(&app.state, original.track, 4).is_err());
        fs::copy(dir.join("input.wav"), dir.join("vocals.wav")).unwrap();
        import(&mut app.state, i, original, &ch, &dir, 1).unwrap();
        assert_eq!(pl::clips(&app.state).last().unwrap().track, 20);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore]
    fn real_selected_outputs_only() {
        let root = PathBuf::from(
            std::env::var_os("LEMON_STEM_TEST_ROOT").expect("existing scratch runtime"),
        );
        assert!(ready(&root));
        let (_app, dir, _i) = fixture();
        let stop = AtomicBool::new(false);
        let (tx, _) = mpsc::channel();
        for mask in [1u8, 2, 9] {
            let out = work(&root, &dir.join("input.wav"), false, &stop, &tx, mask).unwrap();
            for (n, stem) in STEMS.iter().enumerate() {
                assert_eq!(
                    out.join(format!("{stem}.wav")).exists(),
                    mask & (1 << n) != 0
                );
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// Explicit opt-in network/inference check; never runs with the ordinary unit suite.
    #[test]
    #[ignore]
    fn fresh_download_and_real_cpu_separation() {
        let root = PathBuf::from(
            std::env::var_os("LEMON_STEM_TEST_ROOT").expect("dedicated scratch runtime"),
        );
        let (mut app, dir, i) = fixture();
        let stop = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel();
        let progress = std::thread::spawn(move || {
            while let Ok(e) = rx.recv() {
                if let Event::Phase(s) = e {
                    eprintln!("{s}");
                }
            }
        });
        let result = work(&root, &dir.join("input.wav"), true, &stop, &tx, 15);
        drop(tx);
        progress.join().unwrap();
        let out = result.unwrap();
        assert!(ready(&root));
        let first = fs::metadata(root.join("ready.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let (tx, _) = mpsc::channel();
        work(&root, &dir.join("input.wav"), false, &stop, &tx, 15).unwrap();
        assert_eq!(
            first,
            fs::metadata(root.join("ready.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            "second use must not reinstall"
        );
        let mut energy = 0.;
        for stem in STEMS {
            let s = os_engine::sample::load_shared(&out.join(format!("{stem}.wav"))).unwrap();
            assert_eq!(s.frames(), 44100);
            energy += s.left.iter().map(|v| v.abs() as f64).sum::<f64>();
        }
        assert!(energy > 0.1);
        let (original, ch) = source(&app.state, i).unwrap();
        import(&mut app.state, i, original, &ch, &out, 15).unwrap();
        assert!(pl::clips(&app.state)[i].muted);
        // A real subprocess must stop promptly, including its descendants.
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            flag.store(true, Ordering::Relaxed);
        });
        let mut c = command(&python(&root), &root);
        c.args(["-c", "import time; time.sleep(60)"]);
        let began = Instant::now();
        assert!(run(c, &root, &stopped).is_err());
        trigger.join().unwrap();
        assert!(
            began.elapsed() < Duration::from_secs(8),
            "Cancel must stop the child promptly"
        );
        assert!(
            ready(&root),
            "cancelling does not invalidate a healthy installation"
        );
        fs::remove_dir_all(dir).unwrap();
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

/// Lemon Chop uses the same installed runtime and validation as playlist separation.
/// Called only by the instrument's background worker; never opens hardware or downloads.
pub(crate) fn separate_for_chop(input: &str, stop: &AtomicBool) -> Result<[String; 4]> {
    let root = root()?;
    if !ready(&root) { return Err("Set up Stem separation from an audio clip in Lemon Studio first, then retry here.".into()); }
    let (tx, _rx) = mpsc::channel();
    let out = work(&root, Path::new(input), false, stop, &tx, 15)?;
    Ok(std::array::from_fn(|n| out.join(format!("{}.wav", STEMS[n])).to_string_lossy().into_owned()))
}
