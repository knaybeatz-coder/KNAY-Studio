// SPDX-License-Identifier: GPL-3.0-or-later
//! Guide-relative vocal timing. Analyze and render off-thread, then apply the batch in one undo step.
mod onsets;
use crate::{
    app::{Action, App, State, Ui},
    dialog::{Body, DHit, Modal},
    history::{Snap, Tail},
    playlist as pl,
};
use os_model::{AudioWarp, Channel, ChannelKind, Clip, ClipSource, SampleFx};
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}},
};

const HOP: f64 = 0.025;
const MAX_SECONDS: f64 = 90.;
const MAX_TAKES: usize = 12;

#[derive(Clone)]
struct Input {
    index: usize,
    clip: Clip,
    channel: Channel,
}

struct Batch {
    inputs: Vec<Input>,
    tempo: f64,
    ppq: u16,
    arrangement: Option<u16>,
    directory: PathBuf,
}

pub struct Job {
    receiver: mpsc::Receiver<Update>,
    cancelled: Arc<AtomicBool>,
}

enum Update {
    Guide(Vec<f32>),
    Take(usize, Vec<f32>),
    Complete(Result<Preview, String>),
}

pub struct Form {
    batch: Arc<Batch>,
    amount: u8,
    analyzed: Option<u8>,
    take: usize,
    preview: Option<Preview>,
    status: String,
    guide_wave: Vec<f32>,
    take_waves: Vec<Vec<f32>>,
    busy: bool,
    failed: bool,
    audition: Option<Command>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command { Amount(u8), Previous, Next, Analyze, Guide, Original, Aligned, Review(usize), Stop, Apply, Close }

struct Preview {
    guide: Vec<f32>,
    guide_path: String,
    guide_range: (f64, f64),
    takes: Vec<Rendered>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// A failed or cancelled batch deletes only the files it created.
struct Rendered {
    file: Option<PathBuf>,
    warp: AudioWarp,
    original: Vec<f32>,
    aligned: Vec<f32>,
    review: Vec<f64>,
}

impl Drop for Rendered {
    fn drop(&mut self) {
        if let Some(path) = self.file.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn audio_clip(s: &State, index: usize) -> Option<(Clip, &Channel)> {
    let clip = *pl::clips(s).get(index)?;
    let ClipSource::Channel(id) = clip.source else { return None };
    let channel = s.project.channel(id)?;
    (channel.kind == ChannelKind::AudioClip && channel.sample_path.is_some())
        .then_some((clip, channel))
}

fn input(s: &State, index: usize) -> Option<Input> {
    let (clip, channel) = audio_clip(s, index)?;
    Some(Input { index, clip, channel: channel.clone() })
}

pub fn available(s: &State, guide: usize) -> bool {
    s.playlist.sel.contains(&guide) && s.playlist.sel.len() >= 2
        && s.playlist.sel.iter().all(|&i| audio_clip(s, i).is_some())
}

fn name(input: &Input) -> &str {
    input.channel.name.as_deref().unwrap_or("Audio clip")
}

fn overlaps(s: &State, take: &Input, guide: Clip) -> bool {
    pl::clips(s).iter().enumerate().any(|(i, c)| {
        i != take.index && c.track == take.clip.track
            && c.position < guide.end() && c.end() > guide.position
            && pl::source_kind(s, c.source) != pl::PickKind::Automation
    })
}

impl Batch {
    fn new(s: &State, guide: usize) -> Result<Self, String> {
        if !available(s, guide) {
            return Err("Select two or more Playlist audio clips, then open the guide clip's title menu".into());
        }
        if s.playlist.sel.len() > MAX_TAKES + 1 {
            return Err(format!("Select a guide and up to {MAX_TAKES} takes at a time"));
        }
        let mut inputs = vec![input(s, guide).unwrap()];
        inputs.extend(s.playlist.sel.iter().copied().filter(|&i| i != guide).filter_map(|i| input(s, i)));
        let guide = inputs[0].clip;
        guide.position.checked_add(guide.length).ok_or("Guide position is out of range")?;
        let mut rows = std::collections::BTreeSet::from([guide.track]);
        for take in &inputs[1..] {
            if !rows.insert(take.clip.track) {
                return Err("Put the guide and each take on separate Playlist tracks".into());
            }
            if overlaps(s, take, guide) {
                return Err(format!("{} would overlap another clip on its track", name(take)));
            }
        }
        for take in &inputs {
            let sound = &take.channel.sound;
            if crate::ara::is(&take.channel) || sound.start_offset() != 0. || sound.fx.start() != 0.
                || sound.fx.length() != 1. || sound.fx.trim != 0 || sound.fx.flags & SampleFx::REVERSE != 0
                || sound.stretch_cents() != 0. || sound.stretch_multiplier() != 1. {
                return Err(format!("Render {} to audio before aligning its existing processing", name(take)));
            }
        }
        let tempo = s.project.tick_tempo(s.tempo);
        let ppq = s.project.ppq.max(1);
        if s.project.automations.iter().any(|a| a.targets.contains(&os_model::target::TEMPO)
            && pl::clips(s).iter().any(|c| c.source == ClipSource::Channel(a.channel))) {
            return Err("Vocal Align currently requires a constant project tempo".into());
        }
        if !tempo.is_finite() || tempo <= 0. {
            return Err("The project tempo is invalid".into());
        }
        let seconds = f64::from(guide.length) * 60. / (f64::from(ppq) * tempo);
        if !(0.35..=MAX_SECONDS).contains(&seconds) {
            return Err("Use a guide between 0.35 and 90 seconds long".into());
        }
        Ok(Self {
            inputs, tempo, ppq, arrangement: s.project.current_arrangement,
            directory: crate::pattern_render::folder(s).join("Vocal Align"),
        })
    }

    fn current(&self, s: &State) -> bool {
        self.tempo == s.project.tick_tempo(s.tempo) && self.ppq == s.project.ppq.max(1)
            && self.arrangement == s.project.current_arrangement
            && self.inputs.iter().all(|v| pl::clips(s).get(v.index) == Some(&v.clip)
                && s.project.channel(v.channel.index) == Some(&v.channel))
            && self.inputs[1..].iter().all(|v| !overlaps(s, v, self.inputs[0].clip))
    }
}

pub fn open(app: &mut App, guide: usize) -> Result<(), String> {
    let batch = Arc::new(Batch::new(&app.state, guide)?);
    app.state.dialogs.push(Modal::new(Body::VocalAlign(Form {
        batch, amount: 75, analyzed: None, take: 0, preview: None,
        status: String::new(), audition: None, guide_wave: Vec::new(), take_waves: Vec::new(),
        busy: false, failed: false,
    })));
    app.modal_changed();
    command(app, Command::Analyze);
    Ok(())
}

fn start(work: Arc<Batch>, amount: u8) -> Result<Job, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = cancelled.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new().name("Vocal Align".into()).spawn(move || {
        let result = render_with_progress(&work, &stop, amount, |update| {
            let _ = sender.send(update);
            os_host::wake();
        });
        let _ = sender.send(Update::Complete(result));
        os_host::wake();
    }).map_err(|e| e.to_string())?;
    Ok(Job { receiver, cancelled })
}

pub fn closed(app: &mut App) {
    app.vocal_align = None;
    app.transport.stop_preview();
}

pub fn command(app: &mut App, action: Command) {
    let Some(Modal { body: Body::VocalAlign(form), .. }) = app.state.dialogs.last_mut() else { return };
    match action {
        Command::Close => {
            closed(app);
            app.state.dialogs.pop();
            app.modal_changed();
        }
        Command::Apply if app.vocal_align.is_none() && form.analyzed == Some(form.amount) => {
            let Some(preview) = form.preview.take() else { return };
            let batch = form.batch.clone();
            closed(app);
            app.state.dialogs.pop();
            app.modal_changed();
            app.state.message = Some(match apply(&mut app.state, &batch, preview.takes) {
                Ok(n) => format!("Aligned {n} takes. Ctrl+Z restores the original timing"),
                Err(error) => error,
            });
        }
        Command::Analyze if app.vocal_align.is_none() => {
            app.transport.stop_preview();
            form.preview = None;
            form.audition = None;
            form.analyzed = None;
            form.guide_wave.clear();
            form.take_waves = vec![Vec::new(); form.batch.inputs.len() - 1];
            form.failed = false;
            match start(form.batch.clone(), form.amount) {
                Ok(job) => {
                    app.vocal_align = Some(job);
                    form.busy = true;
                    form.status = "Analyzing phrasing and preparing A/B audio…".into();
                }
                Err(error) => {
                    form.failed = true;
                    form.status = error;
                }
            }
        }
        Command::Amount(amount) if app.vocal_align.is_none() => {
            form.amount = amount;
            if form.analyzed != Some(amount) && !form.failed { form.status = "Analyze again to hear this setting".into(); }
        }
        Command::Previous | Command::Next => {
            let len = form.batch.inputs.len() - 1;
            form.take = if action == Command::Next { (form.take + 1) % len } else { (form.take + len - 1) % len };
            form.audition = None;
            app.transport.stop_preview();
        }
        Command::Stop => {
            app.transport.stop_preview();
            form.audition = None;
        }
        Command::Guide | Command::Original | Command::Aligned | Command::Review(_) => {
            let Some(preview) = &form.preview else { return };
            let take = &preview.takes[form.take];
            let last = take.warp.points.last().unwrap();
            let aligned_end = last.1 * 60. / form.batch.tempo;
            let (path, start, end) = match action {
                Command::Guide => (preview.guide_path.as_str(), preview.guide_range.0, preview.guide_range.1),
                Command::Original => (take.warp.source.as_str(), take.warp.points[0].0, last.0),
                Command::Review(i) => {
                    let Some(&time) = take.review.get(i) else { return };
                    (take.warp.rendered.as_str(), (time - 0.35).max(0.), (time + 0.75).min(aligned_end))
                }
                _ => (take.warp.rendered.as_str(), 0., aligned_end),
            };
            let project = audition_project(path, start, end);
            crate::browser_preview::dismiss(app);
            app.transport.preview_song(project);
            if let Some(Modal { body: Body::VocalAlign(form), .. }) = app.state.dialogs.last_mut() {
                form.audition = Some(action);
            }
        }
        _ => {}
    }
    app.invalidate();
}

fn audition_project(path: &str, start: f64, end: f64) -> os_model::Project {
    let mut project = crate::app::preview_project(path);
    project.tempo = Some(120.);
    project.channels[0].kind = ChannelKind::AudioClip;
    project.channels[0].sound.sampler_flags = Some(os_model::ChannelSound::AUDIO_CLIP_FLAGS);
    let mut clip = Clip::new(0, ((end - start) * 2. * f64::from(project.ppq)).ceil() as u32,
        0, ClipSource::Channel(0));
    clip.window = Some((start * 1000., end * 1000.));
    project.patterns.clear();
    project.arrangements = vec![os_model::Arrangement { clips: vec![clip], ..Default::default() }];
    project
}

pub fn poll(app: &mut App) {
    let Some(job) = app.vocal_align.as_ref() else { return };
    let Some(Modal { body: Body::VocalAlign(form), .. }) = app.state.dialogs.last_mut() else { return };
    let mut changed = false;
    loop {
        let update = match job.receiver.try_recv() {
            Ok(update) => update,
            Err(mpsc::TryRecvError::Empty) => break,
            Err(_) => Update::Complete(Err("The vocal alignment worker stopped. Try Analyze again.".into())),
        };
        changed = true;
        match update {
            Update::Guide(wave) => form.guide_wave = wave,
            Update::Take(i, wave) => {
                form.take_waves[i] = wave;
                form.status = format!("Aligning take {} of {}…", i + 1, form.take_waves.len());
            }
            Update::Complete(result) => {
                form.busy = false;
                match result {
                    Ok(preview) => {
                        form.preview = Some(preview);
                        form.analyzed = Some(form.amount);
                        form.status = "Preview ready · Compare Original and Aligned before applying".into();
                    }
                    Err(error) => {
                        form.status = error;
                        form.failed = true;
                    }
                }
                app.vocal_align = None;
                break;
            }
        }
    }
    if changed { app.invalidate(); }
}

fn apply(s: &mut State, batch: &Batch, renders: Vec<Rendered>) -> Result<usize, String> {
    if !batch.current(s) { return Err("The clips changed during alignment. Try again".into()); }
    let first = s.project.channels.len();
    if first + renders.len() > u16::MAX as usize {
        return Err("There are no free audio channel IDs".into());
    }
    let guide = batch.inputs[0].clip;
    let count = renders.len();
    let extra = Snap::Several(vec![
        Snap::Tail(Box::new(Tail::now(s, first))),
        Snap::AudioWarps(s.project.audio_warps.clone()),
    ]);
    pl::snapshot_with(s, "Vocal Align", extra);
    for (input, mut render) in batch.inputs[1..].iter().zip(renders) {
        let id = s.project.channels.len() as u16;
        let mut channel = input.channel.clone();
        channel.sample_path = Some(render.warp.rendered.clone());
        channel.name = Some(format!("{} (aligned)", name(input)));
        channel.sound.stretch = [0; 4];
        crate::loop_fit::set_fitted_time(&mut channel.sound, f64::from(guide.length) / f64::from(batch.ppq));
        crate::rack::push_channel(s, channel);
        let clip = &mut pl::arr_mut(s).clips[input.index];
        clip.source = ClipSource::Channel(id);
        clip.position = guide.position;
        clip.length = guide.length;
        clip.window = None;
        s.project.audio_warps.push(render.warp.clone());
        render.file.take();
    }
    s.piano.dirty = true;
    Ok(count)
}

struct Audio {
    sample: Arc<os_engine::Sample>,
    path: String,
    start: usize,
    end: usize,
}

fn read(input: &Input, batch: &Batch) -> Result<Audio, String> {
    let path = crate::plugins::resolve_sample(input.channel.sample_path.as_deref().unwrap());
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 128 * 1024 * 1024) {
        return Err(format!("Split {} into shorter clips first", name(input)));
    }
    let sample = os_engine::sample::load_shared(Path::new(&path))
        .map_err(|e| format!("Cannot read {}: {e}", name(input)))?;
    let rate = f64::from(sample.rate);
    let frames = sample.frames();
    let (start, end) = input.clip.window.map(|(a, b)| {
        (((a * rate / 1000.).round() as usize).min(frames), ((b * rate / 1000.).round() as usize).min(frames))
    }).unwrap_or((0, frames));
    let playback = f64::from(input.clip.length) * 60. / (f64::from(batch.ppq) * batch.tempo);
    let factor = input.channel.sound.stretch_factor(frames as f64 / rate, batch.tempo);
    let end = end.min(start.saturating_add((playback * rate / factor).round() as usize));
    if !(0.35..=MAX_SECONDS).contains(&(end.saturating_sub(start) as f64 / rate)) {
        return Err(format!("{} must be between 0.35 and 90 seconds", name(input)));
    }
    Ok(Audio { sample, path, start, end })
}

#[cfg(test)]
fn render(batch: &Batch, stop: &AtomicBool, amount: u8) -> Result<Preview, String> {
    render_with_progress(batch, stop, amount, |_| {})
}

fn render_with_progress(batch: &Batch, stop: &AtomicBool, amount: u8, mut progress: impl FnMut(Update)) -> Result<Preview, String> {
    let guide = read(&batch.inputs[0], batch)?;
    let guide_beats = f64::from(batch.inputs[0].clip.length) / f64::from(batch.ppq);
    let seconds = guide_beats * 60. / batch.tempo;
    let guide_rate = f64::from(guide.sample.rate);
    let guide_factor = batch.inputs[0].channel.sound.stretch_factor(guide.sample.frames() as f64 / guide_rate, batch.tempo);
    let guide_audio_seconds = (guide.end - guide.start) as f64 / guide_rate * guide_factor;
    let guide_wave = peaks(&guide.sample.left[guide.start..guide.end], &guide.sample.right[guide.start..guide.end]);
    progress(Update::Guide(guide_wave.clone()));
    let mut reference: Option<Vec<Feature>> = None;
    let mut guide_onsets = None;
    let mut rendered = Vec::new();
    for (i, take) in batch.inputs[1..].iter().enumerate() {
        if stop.load(Ordering::Relaxed) { return Err("Vocal alignment cancelled".into()); }
        let audio = read(take, batch)?;
        let rate = audio.sample.rate;
        let frames = audio.end - audio.start;
        let output = (seconds * f64::from(rate)).round() as usize;
        let audible = ((guide_audio_seconds * f64::from(rate)).round() as usize).min(output);
        let left = &audio.sample.left[audio.start..audio.end];
        let right = &audio.sample.right[audio.start..audio.end];
        let original = peaks(left, right);
        progress(Update::Take(i, original.clone()));
        let same = rate == guide.sample.rate
            && left == &guide.sample.left[guide.start..guide.end]
            && right == &guide.sample.right[guide.start..guide.end];
        let (points, review) = if same {
            (vec![(0, 0), (frames, audible)], Vec::new())
        } else {
            let target = match &reference {
                Some(target) => target,
                None => reference.insert(features(&guide).map_err(|e| format!("Guide: {e}"))?),
            };
            let source = features(&audio).map_err(|e| format!("{}: {e}", name(take)))?;
            let coarse = anchors(&source, target, frames, audible, amount)
                .map_err(|error| format!("{}: {error}", name(take)))?;
            let reference_attacks = guide_onsets.get_or_insert_with(|| onsets::detect(&guide));
            let (refined, review) = onsets::refine(&coarse, &onsets::detect(&audio), reference_attacks,
                rate, (guide.end - guide.start) as f64 / guide_rate, amount);
            (protect_attacks(&refined, &source, frames, rate), review)
        };
        let (mut left, mut right) = os_dsp::stretch::vocal_warp(left, right, rate, &points)?;
        left.resize(output, 0.);
        right.resize(output, 0.);
        if stop.load(Ordering::Relaxed) { return Err("Vocal alignment cancelled".into()); }
        std::fs::create_dir_all(&batch.directory).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let path = batch.directory.join(format!("Align-{stamp}-{}.wav", take.index));
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
        let entry = Rendered { file: Some(path.clone()),
            original,
            aligned: peaks(&left, &right),
            review,
            warp: AudioWarp {
            source: audio.path, rendered: path.to_string_lossy().into_owned(),
            points: points.iter().map(|&(x, y)| ((x + audio.start) as f64 / f64::from(rate),
                y as f64 / output as f64 * guide_beats)).collect(),
        }};
        let stereo: Vec<_> = left.into_iter().zip(right).flat_map(|(l, r)| [l, r]).collect();
        let result = {
            let mut writer = BufWriter::new(&mut file);
            os_engine::wav::write_f32_stereo(&mut writer, rate, &stereo).and_then(|_| writer.flush())
        }.and_then(|_| file.sync_all());
        drop(file);
        result.map_err(|e| e.to_string())?;
        rendered.push(entry);
    }
    Ok(Preview {
        guide: guide_wave,
        guide_path: guide.path,
        guide_range: (guide.start as f64 / guide_rate, guide.end as f64 / guide_rate),
        takes: rendered,
    })
}

fn peaks(left: &[f32], right: &[f32]) -> Vec<f32> {
    let n = left.len().min(right.len());
    let bins = n.min(220);
    (0..bins).map(|i| {
        let range = i * n / bins..(i + 1) * n / bins;
        left[range.clone()].iter().chain(&right[range]).map(|v| v.abs()).fold(0., f32::max)
    }).collect()
}

type Feature = [f32; 5];

fn features(audio: &Audio) -> Result<Vec<Feature>, String> {
    let n = audio.end - audio.start;
    let rate = audio.sample.rate as f32;
    let count = (n as f64 / f64::from(rate) / HOP).round().max(2.) as usize;
    let low_alpha = 1. - (-std::f32::consts::TAU * 900. / rate).exp();
    let mid_alpha = 1. - (-std::f32::consts::TAU * 2600. / rate).exp();
    let dc_alpha = 1. - (-std::f32::consts::TAU * 70. / rate).exp();
    let mut dc = [0.; 2];
    let mut low = [0.; 2];
    let mut mid = [0.; 2];
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        let a = audio.start + i * n / count;
        let b = audio.start + (i + 1) * n / count;
        let mut total = 0.;
        let mut high = 0.;
        let mut band = 0.;
        for j in a..b {
            for (ch, x) in [audio.sample.left[j], audio.sample.right[j]].into_iter().enumerate() {
                dc[ch] += dc_alpha * (x - dc[ch]);
                let x = x - dc[ch];
                low[ch] += low_alpha * (x - low[ch]);
                mid[ch] += mid_alpha * (x - mid[ch]);
                total += x * x;
                high += (x - mid[ch]).powi(2);
                band += (mid[ch] - low[ch]).powi(2);
            }
        }
        rows.push([total / (2 * (b - a)) as f32, high / total.max(1e-12), 0.,
            band / total.max(1e-12), 0.]);
    }
    let peak = rows.iter().map(|r| r[0]).fold(0., f32::max);
    if peak < 1e-8 { return Err("The clip is silent or too quiet to align".into()); }
    let mut previous = 0.;
    let mut previous_high = 0.;
    for row in &mut rows {
        let level = ((row[0] / peak).max(1e-5).log10() * 0.2 + 1.).clamp(0., 1.);
        row[1] = row[1].sqrt().min(1.) * level;
        row[2] = level - previous;
        row[3] = row[3].sqrt().min(1.) * level;
        row[4] = (row[1] - previous_high).max(0.);
        row[0] = level;
        previous = level;
        previous_high = row[1];
    }
    if rows.iter().map(|r| r[0]).fold(1., f32::min) > 0.88 {
        return Err("The clip has too little timing contrast to align reliably".into());
    }
    Ok(rows)
}

fn distance(a: Feature, b: Feature) -> f32 {
    (a[0] - b[0]).powi(2) + 0.45 * (a[1] - b[1]).powi(2) + 0.65 * (a[2] - b[2]).powi(2)
        + 0.35 * (a[3] - b[3]).powi(2) + 0.75 * (a[4] - b[4]).powi(2)
}

/// Bounded dynamic time warping with a slope limit. The backtracked path advances on
/// both axes, so every resulting stretch segment has positive length.
fn anchors(src: &[Feature], dst: &[Feature], frames: usize, output: usize, amount: u8) -> Result<Vec<(usize, usize)>, String> {
    let (n, m) = (src.len(), dst.len());
    if !(0.5..=2.).contains(&(m as f64 / n as f64)) {
        return Err("Choose takes with similar phrase lengths".into());
    }
    let band = 80.min(m);
    let width = band * 2 + 1;
    let left = |i: usize| (i * m / n).saturating_sub(band);
    let mut trace = vec![0u8; (n + 1) * width];
    let mut cost = vec![f32::INFINITY; (n + 1) * width];
    let index = |i: usize, j: usize| j.checked_sub(left(i)).filter(|&k| k < width).map(|k| i * width + k);
    cost[0] = 0.;
    for i in 1..=n {
        for j in left(i).max(1)..=(left(i) + width - 1).min(m) {
            let here = distance(src[i - 1], dst[j - 1]);
            let k = index(i, j).unwrap();
            for (code, di, dj, penalty) in [(1, 1, 1, 0.), (2, 1, 2, 0.045), (3, 2, 1, 0.045)] {
                if i < di || j < dj { continue; }
                let Some(prev) = index(i - di, j - dj) else { continue };
                let skipped = if di == 2 { distance(src[i - 2], dst[j - 1]) }
                    else if dj == 2 { distance(src[i - 1], dst[j - 2]) } else { 0. };
                let score = cost[prev] + here + skipped + penalty;
                if score < cost[k] { cost[k] = score; trace[k] = code; }
            }
        }
    }
    let score = cost[index(n, m).unwrap()] / n.max(m) as f32;
    if score > 0.15 { return Err("Could not find a reliable timing match. Try a shorter matching phrase".into()); }
    let mut i = n;
    let mut j = m;
    let mut path = vec![(n, m)];
    while i > 0 && j > 0 {
        match trace[index(i, j).unwrap()] {
            1 => { i -= 1; j -= 1; }
            2 => { i -= 1; j -= 2; }
            3 => { i -= 2; j -= 1; }
            _ => return Err("Could not match the takes".into()),
        }
        path.push((i, j));
    }
    path.reverse();
    let mut result = vec![(0, 0)];
    let mut last = 0;
    for (k, &(i, j)) in path.iter().enumerate().skip(1) {
        if i >= last + 5 && i + 5 < n {
            let (p, q) = (path[k.saturating_sub(3)], path[(k + 3).min(path.len() - 1)]);
            let softened = p.1 as f64 + (q.1 - p.1) as f64 * (i - p.0) as f64 / (q.0 - p.0) as f64;
            let local = path[k.saturating_sub(2)..=(k + 2).min(path.len() - 1)].iter()
                .map(|&(i, j)| distance(src[i.saturating_sub(1)], dst[j.saturating_sub(1)]))
                .sum::<f32>() / 5.;
            let weight = f64::from(amount.min(100)) / 100. * f64::from((1. - (local - 0.07).max(0.) * 3.).clamp(0.35, 1.));
            let x = i * frames / n;
            let natural = x as f64 * output as f64 / frames as f64;
            let target = softened * output as f64 / m as f64;
            let y = (natural + (target - natural) * weight).round() as usize;
            let previous = result.last().unwrap().1;
            result.push((x, y.clamp(previous + 1, output - 1)));
            last = i;
        }
    }
    result.push((frames, output));
    Ok(result)
}

fn protect_attacks(points: &[(usize, usize)], features: &[Feature], frames: usize, rate: u32) -> Vec<(usize, usize)> {
    let radius = (rate as usize / 80).max(1);
    let mut result = vec![points[0]];
    for segment in points.windows(2) {
        let [(a, x), (b, y)] = [segment[0], segment[1]];
        let n = features.len();
        // Reserve a small un-stretched island around the strongest onset in this span.
        let range = (a * n / frames)..(b * n / frames).min(n);
        if let Some(i) = range.filter(|&i| features[i][2] > 0.33 || features[i][4] > 0.19)
            .max_by(|&i, &j| (features[i][2] + features[i][4]).total_cmp(&(features[j][2] + features[j][4]))) {
            let at = i * frames / n;
            let dest = x + (at.saturating_sub(a) * (y - x)) / (b - a);
            if at > a + 3 * radius && at + 3 * radius < b
                && dest > x + 3 * radius && dest + 3 * radius < y {
                result.extend([(at - radius, dest - radius), (at + radius, dest + radius)]);
            }
        }
        result.push((b, y));
    }
    // The shared marker format has a fixed anchor budget.
    if result.len() > 1024 { points.to_vec() } else { result }
}

pub fn paint(ui: &mut Ui, r: os_ui::Rect, form: &Form) {
    use os_ui::{Color, Font, Rect};
    let ink = Color::rgb(0xeaf0f3);
    let dim = Color::rgb(0xa4b0bd);
    let mint = Color::rgb(0x65e2c3);
    let gold = Color::rgb(0xf1d45a);
    let purple = Color::rgb(0xa9a0f8);
    let x = r.x + 18.;
    let width = r.w - 36.;
    let button = |ui: &mut Ui, rect: Rect, label: &str, action: Command, selected: bool, enabled: bool| {
        let hover = enabled && ui.hit(rect, Action::Dlg(DHit::VocalAlign(action)), label.to_owned());
        let fill = if selected && enabled { gold } else if hover { Color::rgb(0x35464e) } else { Color::rgb(0x232e35) };
        ui.c.fill_rrect(rect, 8., fill);
        ui.c.text_center(Font::light(12, 0).natural().bold(), rect,
            if selected && enabled { Color::rgb(0x181a1a) } else if enabled { ink } else { dim.with_alpha(105) }, label);
    };
    let face = Rect::new(r.x + 4., r.y + 28., r.w - 8., r.h - 33.);
    ui.c.fill_rect_gradient(face, Color::rgb(0x1e3038), Color::rgb(0x141a22), 10.);
    let heading = Font::light(22, 0).natural().bold();
    ui.c.text_clip(heading, x + 5., r.y + 43., width - 12., ink, "Vocal timing");
    ui.c.text_clip(Font::light(12, 0).natural(), x + 5., r.y + 74., width - 12., dim,
        &format!("Guide: {}   ·   Preserve pitch and consonant attacks", name(&form.batch.inputs[0])));
    let top = r.y + 106.;
    let header = Rect::new(x, top, width, 34.);
    ui.c.fill_rrect(header, 8., Color::rgb(0x202a33));
    let count = form.batch.inputs.len() - 1;
    let take = &form.batch.inputs[form.take + 1];
    let review = form.preview.as_ref().map(|p| p.takes[form.take].review.as_slice())
        .filter(|_| crate::workspace_visibility::shown("visible_playlist-vocal-align-review")).unwrap_or_default();
    let review_label = if review.is_empty() { String::new() } else { format!("   ·   {} to review", review.len()) };
    ui.c.text_clip(Font::light(13, 0).natural().bold(), x + 10., top + 10., width - 113., ink,
        &format!("TAKE {} / {}   ·   {}{}", form.take + 1, count, name(take), review_label));
    button(ui, Rect::new(header.right() - 74., top + 4., 30., 26.), "‹", Command::Previous, false, count > 1);
    button(ui, Rect::new(header.right() - 37., top + 4., 30., 26.), "›", Command::Next, false, count > 1);
    let empty: &[f32] = &[];
    let original_input = form.take_waves.get(form.take).map_or(empty, Vec::as_slice);
    let (guide, original, aligned) = form.preview.as_ref().map_or((&form.guide_wave[..], original_input, empty), |p| {
        (&p.guide[..], &p.takes[form.take].original[..], &p.takes[form.take].aligned[..])
    });
    for (i, (label, line, wave, action)) in [
        ("Guide", mint, guide, Command::Guide),
        ("Original", purple, original, Command::Original),
        ("Aligned", gold, aligned, Command::Aligned),
    ].into_iter().enumerate() {
        let y = top + 43. + i as f32 * 56.;
        let lane = Rect::new(x, y, width, 50.);
        ui.c.fill_rrect(lane, 8., Color::rgb(0x17232c));
        ui.c.fill_rrect(Rect::new(x + 7., y + 8., 3., 34.), 1.5, line);
        ui.c.text_clip(Font::light(12, 0).natural().bold(), x + 16., y + 17., 66., line, label);
        let graph = Rect::new(x + 90., y + 6., width - 165., 38.);
        for tick in 0..=8 {
            let px = graph.x + graph.w * tick as f32 / 8.;
            ui.c.line(px, graph.y, px, graph.bottom(), 1., Color::rgb(0x283740));
        }
        ui.c.line(graph.x, graph.cy(), graph.right(), graph.cy(), 1., line.with_alpha(65));
        for (bin, &peak) in wave.iter().enumerate() {
            let px = graph.x + graph.w * (bin as f32 + 0.5) / wave.len() as f32;
            let height = peak.sqrt().min(1.) * (graph.h - 4.) / 2.;
            ui.c.line(px, graph.cy() - height, px, graph.cy() + height, 1.2, line);
        }
        if action == Command::Aligned {
            let seconds = f64::from(form.batch.inputs[0].clip.length) * 60.
                / (f64::from(form.batch.ppq) * form.batch.tempo);
            for (index, &time) in review.iter().enumerate() {
                let px = graph.x + graph.w * (time / seconds).clamp(0., 1.) as f32;
                let mark = Rect::new(px - 5., graph.y, 10., graph.h);
                let active = form.audition == Some(Command::Review(index));
                let hot = ui.hit(mark, Action::Dlg(DHit::VocalAlign(Command::Review(index))),
                    format!("Listen around {:.2}s · uncertain timing match", time));
                let color = if active || hot { ink } else { Color::rgb(0xffa45f) };
                ui.c.fill_polygon(&[(px - 4., graph.y), (px + 4., graph.y), (px, graph.y + 6.)], color);
                ui.c.line(px, graph.y + 6., px, graph.bottom(), 1., color.with_alpha(160));
            }
        }
        button(ui, Rect::new(lane.right() - 65., y + 10., 55., 30.),
            "Listen", action, form.audition == Some(action), form.preview.is_some());
    }
    let options_y = top + 222.;
    ui.c.text_clip(Font::light(11, 0).natural().bold(), x + 2., options_y + 9., 80., dim, "TIGHTNESS");
    let busy = form.busy;
    for (i, (label, value)) in [("Natural", 45), ("Balanced", 75), ("Tight", 100)].into_iter().enumerate() {
        button(ui, Rect::new(x + 82. + i as f32 * 83., options_y, 78., 30.),
            label, Command::Amount(value), value == form.amount, !busy);
    }
    button(ui, Rect::new(r.right() - 82., options_y, 64., 30.), "Stop", Command::Stop, false, form.audition.is_some());
    let footer = r.bottom() - 53.;
    ui.c.line(x, footer - 10., x + width, footer - 10., 1., Color::rgb(0x394751));
    let status_font = Font::light(12, 0).natural();
    let status_width = width - 279.;
    let status_color = if form.failed { Color::rgb(0xffa899) } else { dim };
    let mut line = String::new();
    let mut y = footer + 2.;
    for word in form.status.split_whitespace() {
        let joined = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if !line.is_empty() && ui.c.text_width(status_font, &joined) > status_width {
            ui.c.text_clip(status_font, x + 3., y, status_width, status_color, &line);
            y += 15.;
            line = word.to_owned();
            if y > footer + 18. { break; }
        } else {
            line = joined;
        }
    }
    if y <= footer + 18. { ui.c.text_clip(status_font, x + 3., y, status_width, status_color, &line); }
    if form.failed {
        ui.hit(Rect::new(x, footer, status_width, 34.), Action::Swallow, form.status.clone());
    }
    for (offset, label, action, enabled) in [
        (260., "Analyze", Command::Analyze, !busy),
        (178., "Cancel", Command::Close, true),
        (96., "Apply", Command::Apply, form.preview.is_some() && form.analyzed == Some(form.amount)),
    ] {
        button(ui, Rect::new(r.right() - offset, footer, 76., 34.), label, action, action == Command::Apply, enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocal_align_matches_internal_timing_and_undoes_a_batch() {
        let root = std::env::temp_dir().join(format!("lemon-align-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let rate = 8000;
        let make = |label: &str, syllables: &[(f64, f64, f64)]| {
            let path = root.join(format!("{label}.wav"));
            let stereo: Vec<_> = (0..2 * rate).flat_map(|i| {
                let t = f64::from(i) / f64::from(rate);
                let x = syllables.iter().find(|&&(a, b, _)| a < t && t < b).map_or(0., |&(a, b, hz)| {
                    let edge = ((t - a).min(b - t) * 80.).min(1.);
                    (edge * 0.45 * (t * hz * std::f64::consts::TAU).sin()) as f32
                });
                [x, x]
            }).collect();
            os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), rate, &stereo).unwrap();
            path
        };
        let guide = make("Guide", &[(0.28, 0.52, 220.), (0.79, 1.07, 420.), (1.40, 1.63, 650.)]);
        let dub = make("Dub", &[(0.43, 0.67, 245.), (0.95, 1.23, 490.), (1.33, 1.57, 700.)]);
        let mut app = App::new(None);
        app.state.project = crate::app::default_project();
        app.state.tempo = 120.;
        app.state.project.data_folder = Some(os_model::DataFolder { choice: 2, folder: root.to_string_lossy().into_owned() });
        let g = pl::add_audio_clip(&mut app.state, guide.to_str().unwrap(), 0, 24).unwrap();
        let a = pl::add_audio_clip(&mut app.state, dub.to_str().unwrap(), 1, 40).unwrap();
        let b = pl::add_audio_clip(&mut app.state, dub.to_str().unwrap(), 2, 56).unwrap();
        let original = std::fs::read(&dub).unwrap();
        let unpadded = app.state.project.clone();
        let guide_clip = pl::clips(&app.state)[g];
        let extra = u32::from(app.state.project.ppq) / 2;
        let clips = &mut pl::arr_mut(&mut app.state).clips;
        clips[g].length += extra;
        clips[a].source = guide_clip.source;
        app.state.playlist.sel = vec![g, a];
        open(&mut app, g).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.vocal_align.is_some() && std::time::Instant::now() < deadline {
            poll(&mut app);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let Some(Modal { body: Body::VocalAlign(form), .. }) = app.state.dialogs.last() else { panic!() };
        let preview = form.preview.as_ref().unwrap_or_else(|| panic!("{}", form.status));
        assert!(!form.guide_wave.is_empty() && !form.take_waves[0].is_empty());
        assert_eq!(preview.takes[0].warp.points.len(), 2, "identical audio needs no internal stretching");
        let rendered = os_engine::sample::load_shared(Path::new(&preview.takes[0].warp.rendered)).unwrap();
        let source = os_engine::sample::load_shared(&guide).unwrap();
        assert_eq!(&rendered.left[..source.frames()], source.left.as_slice());
        assert!(rendered.left[source.frames()..].iter().all(|&sample| sample == 0.));
        command(&mut app, Command::Close);
        app.state.project = unpadded;
        app.state.playlist.sel = vec![a, g, b];
        assert!(pl::clip_menu(&app.state, g, 0., 0.).items.iter().any(|p| p.cmd == pl::Cmd::VocalAlign(g)));
        crate::workspace_visibility::toggle("visible_playlist-vocal-align");
        assert!(!pl::clip_menu(&app.state, g, 0., 0.).items.iter().any(|p| p.cmd == pl::Cmd::VocalAlign(g)));
        crate::workspace_visibility::toggle("visible_playlist-vocal-align");
        let before = app.state.project.clone();
        let batch = Batch::new(&app.state, g).unwrap();
        let stop = AtomicBool::new(false);
        let natural = render(&batch, &stop, 45).unwrap();
        let mut tight = render(&batch, &stop, 100).unwrap();
        assert_ne!(natural.takes[0].warp.points, tight.takes[0].warp.points);
        drop(natural);
        let results = &tight.takes;
        let points = &results[0].warp.points;
        let to_seconds = |x: f64| x * 60. / batch.tempo;
        let point = points.windows(2).find(|p| p[0].0 <= 0.96 && p[1].0 >= 0.96).unwrap();
        let mapped = point[0].1 + (0.96 - point[0].0) / (point[1].0 - point[0].0) * (point[1].1 - point[0].1);
        assert!((to_seconds(mapped) - 0.80).abs() < 0.09, "{points:?}");
        let audio = os_engine::sample::load_shared(Path::new(&results[0].warp.rendered)).unwrap();
        let energy = |a: f64, b: f64| {
            let range = (a * f64::from(rate)) as usize..(b * f64::from(rate)) as usize;
            audio.left[range.clone()].iter().map(|v| v * v).sum::<f32>() / range.len() as f32
        };
        assert!(energy(0.85, 0.92) > 4. * energy(0.63, 0.70));
        tight.takes[0].review = vec![0.8];
        app.state.dialogs.push(Modal::new(Body::VocalAlign(Form {
            batch: Arc::new(batch), amount: 100, analyzed: Some(100), take: 0, preview: Some(tight),
            status: "Preview ready · Compare Original and Aligned before applying".into(), audition: None,
            guide_wave: Vec::new(), take_waves: Vec::new(), busy: false, failed: false,
        })));
        let mut canvas = os_ui::Canvas::new(1280, 800, 1.).unwrap();
        app.paint(&mut canvas, None);
        assert!(app.hits.iter().any(|hit| hit.action == Action::Dlg(DHit::VocalAlign(Command::Apply))));
        assert!(app.hits.iter().any(|hit| hit.action == Action::Dlg(DHit::VocalAlign(Command::Review(0)))));
        crate::workspace_visibility::toggle("visible_playlist-vocal-align-review");
        app.paint(&mut canvas, None);
        assert!(!app.hits.iter().any(|hit| hit.action == Action::Dlg(DHit::VocalAlign(Command::Review(0)))));
        crate::workspace_visibility::toggle("visible_playlist-vocal-align-review");
        app.paint(&mut canvas, None);
        if let Some(path) = std::env::var_os("LEMON_ALIGN_PREVIEW") {
            canvas.pixmap.save_png(path).unwrap();
        }
        command(&mut app, Command::Amount(45));
        command(&mut app, Command::Apply);
        assert_eq!(app.state.project, before, "a stale preview must never apply");
        command(&mut app, Command::Amount(100));
        command(&mut app, Command::Apply);
        assert!(app.state.dialogs.is_empty());
        let after = app.state.project.clone();
        let clips = pl::clips(&app.state);
        assert_eq!(clips[g], before.arrangement().unwrap().clips[g]);
        assert_eq!(clips[a].position, clips[g].position);
        assert_eq!(clips[b].track, 2);
        assert_eq!(clips[b].length, clips[g].length);
        assert_eq!(std::fs::read(&dub).unwrap(), original);
        let saved = os_flp::project::create(&after).unwrap();
        let restored = os_flp::project::load(&saved).unwrap();
        assert_eq!(restored.audio_warps, after.audio_warps);
        assert_eq!(crate::history::undo(&mut app.state), Some("Vocal Align"));
        assert_eq!(app.state.project, before);
        assert_eq!(crate::history::redo(&mut app.state), Some("Vocal Align"));
        assert_eq!(app.state.project, after);
        for warp in &after.audio_warps { std::fs::remove_file(&warp.rendered).unwrap(); }
        for file in [guide, dub] { std::fs::remove_file(file).unwrap(); }
        std::fs::remove_dir_all(root).unwrap();
    }
}
