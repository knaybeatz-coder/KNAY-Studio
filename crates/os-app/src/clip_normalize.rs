// SPDX-License-Identifier: GPL-3.0-or-later
//! Measure individual Playlist clips before the mixer, then apply one undoable gain edit.
use crate::{app::App, playlist as pl};
use os_model::{Arrangement, Channel, Clip, ClipSource, Project};
use std::sync::{Arc, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Peak,
    Rms,
    Lufs,
}
impl Mode {
    pub const ALL: [Self; 3] = [Self::Peak, Self::Rms, Self::Lufs];
    pub fn label(self) -> &'static str {
        match self {
            Self::Peak => "Sample peak (dBFS)...",
            Self::Rms => "RMS (dBFS)...",
            Self::Lufs => "Integrated loudness (LUFS)...",
        }
    }
    pub fn default_db(self) -> i32 {
        match self {
            Self::Peak => -1,
            Self::Rms => -18,
            Self::Lufs => -14,
        }
    }
}

struct Input {
    index: usize,
    clip: Clip,
    channel: Channel,
    path: String,
}
struct Batch {
    inputs: Vec<Input>,
    tempo: f64,
    ppq: u16,
    arrangement: Option<u16>,
}
pub struct Job {
    batch: Arc<Batch>,
    rx: mpsc::Receiver<Result<Vec<Option<f32>>, String>>,
}

pub(crate) fn analysis_engine(
    original: Clip,
    channel: &Channel,
    path: &str,
    tempo: f64,
    ppq: u16,
    library: &mut os_engine::SampleLibrary,
) -> Result<(os_engine::Engine, u32), String> {
    let sample = library
        .get(&path)
        .ok_or_else(|| format!("Cannot read {}", path))?;
    let rate = sample.rate;
    let mut clip = original;
    clip.source = ClipSource::Channel(0);
    clip.position = 0;
    clip.track = 0;
    clip.muted = false;
    clip.set_gain(1.);
    let mut ch = channel.clone();
    ch.index = 0;
    ch.mixer_track = 0;
    ch.enabled = true;
    ch.volume = 0.78125;
    ch.pan = 0.;
    ch.sample_path = Some(path.into());
    let p = Project {
        ppq: ppq,
        tempo: Some(tempo),
        channels: vec![ch],
        arrangements: vec![Arrangement {
            number: 1,
            clips: vec![clip],
            ..Default::default()
        }],
        ..Default::default()
    };
    Ok((
        os_engine::Engine::new(&p, library, rate, os_engine::PlayMode::Song),
        rate,
    ))
}

fn measure(
    input: &Input,
    batch: &Batch,
    mode: Mode,
    library: &mut os_engine::SampleLibrary,
) -> Result<Option<f64>, String> {
    let (mut engine, rate) = analysis_engine(
        input.clip,
        &input.channel,
        &input.path,
        batch.tempo,
        batch.ppq,
        library,
    )?;
    let mut peak = 0.0_f64;
    let mut energy = 0.0;
    let mut count = 0_u64;
    let mut meter = if mode == Mode::Lufs {
        Some(ebur128::EbuR128::new(2, rate, ebur128::Mode::I).map_err(|e| e.to_string())?)
    } else {
        None
    };
    while let Some((audio, tick, _, _, _)) = engine.next_tick() {
        if tick >= f64::from(input.clip.length) {
            break;
        }
        match mode {
            Mode::Peak => {
                for &v in audio {
                    peak = peak.max(f64::from(v).abs());
                }
            }
            Mode::Rms => {
                energy += audio.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>();
                count += audio.len() as u64;
            }
            Mode::Lufs => meter
                .as_mut()
                .unwrap()
                .add_frames_f32(audio)
                .map_err(|e| e.to_string())?,
        }
    }
    let db = match mode {
        Mode::Peak => 20. * peak.log10(),
        Mode::Rms => 10. * (energy / count.max(1) as f64).log10(),
        Mode::Lufs => meter
            .unwrap()
            .loudness_global()
            .map_err(|e| e.to_string())?,
    };
    Ok(db.is_finite().then_some(db))
}

impl Batch {
    fn new(s: &crate::app::State, anchor: usize) -> Self {
        let selection = if s.playlist.sel.contains(&anchor) {
            s.playlist.sel.clone()
        } else {
            vec![anchor]
        };
        let inputs = selection
            .into_iter()
            .filter_map(|index| {
                let clip = *pl::clips(s).get(index)?;
                let ClipSource::Channel(id) = clip.source else {
                    return None;
                };
                let channel = s.project.channel(id)?.clone();
                if channel.kind != os_model::ChannelKind::AudioClip {
                    return None;
                }
                let path =
                    crate::plugins::resolve_sample(channel.sample_path.as_deref().unwrap_or(""));
                Some(Input {
                    index,
                    clip,
                    channel,
                    path,
                })
            })
            .collect();
        Self {
            inputs,
            tempo: s.tempo,
            ppq: s.project.ppq,
            arrangement: s.project.current_arrangement,
        }
    }
    fn current(&self, s: &crate::app::State) -> bool {
        self.tempo == s.tempo
            && self.ppq == s.project.ppq
            && self.arrangement == s.project.current_arrangement
            && self.inputs.iter().all(|input| {
                pl::clips(s).get(input.index) == Some(&input.clip)
                    && s.project.channel(input.channel.index) == Some(&input.channel)
            })
    }
}

pub fn tick(app: &mut App) {
    if let Some((index, mode, target)) = app.state.playlist.normalize_request.take() {
        if app.clip_normalization.is_some() {
            app.state.message = Some("Clip normalization is already running".into());
            return;
        }
        let batch = Arc::new(Batch::new(&app.state, index));
        if batch.inputs.is_empty() {
            app.state.message = Some("Select an audio clip to normalize".into());
            return;
        }
        let work = batch.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut lib = os_engine::SampleLibrary::new(None);
            let result = work
                .inputs
                .iter()
                .map(|input| {
                    let Some(db) = measure(input, &work, mode, &mut lib)? else {
                        return Ok(None);
                    };
                    let gain = target - db;
                    if !(-60.0..=24.0).contains(&gain) {
                        return Err(format!(
                            "Target exceeds the clip gain range for {}",
                            input.path
                        ));
                    }
                    Ok(Some(10_f32.powf(gain as f32 / 20.0)))
                })
                .collect();
            let _ = tx.send(result);
            os_host::wake();
        });
        app.state.message = Some(format!("Measuring {} audio clips...", batch.inputs.len()));
        app.clip_normalization = Some(Job { batch, rx });
        app.invalidate();
    }
    let Some(job) = app.clip_normalization.as_ref() else {
        return;
    };
    let result = match job.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("Normalization worker stopped".into()),
    };
    let job = app.clip_normalization.take().unwrap();
    app.state.message = Some(match result {
        Err(error) => error,
        Ok(_) if !job.batch.current(&app.state) => {
            "Clips changed during measurement. Run Normalize again.".into()
        }
        Ok(gains) => {
            let changed = gains.iter().filter(|v| v.is_some()).count();
            if changed > 0 {
                pl::snapshot_as(&mut app.state, "Normalize audio clips");
                let clips = &mut pl::arr_mut(&mut app.state).clips;
                for (input, gain) in job.batch.inputs.iter().zip(gains) {
                    if let Some(g) = gain {
                        clips[input.index].set_gain(g);
                    }
                }
                app.state.piano.dirty = true;
            }
            format!(
                "Normalized {changed} audio clips; {} below measurement threshold",
                job.batch.inputs.len() - changed
            )
        }
    });
    app.invalidate();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_selected_clips_peak_rms_lufs_and_undo() {
        let file = std::env::temp_dir().join(format!("lemon-normalize-{}.wav", std::process::id()));
        let audio: Vec<f32> = (0..96000)
            .flat_map(|i| {
                let v = (i as f32 * std::f32::consts::TAU * 1000. / 48000.).sin() * 0.25;
                [v, v]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&file).unwrap(), 48000, &audio)
            .unwrap();
        let mut a = App::new(None);
        a.state.project = crate::app::default_project();
        let path = file.to_str().unwrap();
        let i = pl::add_audio_clip(&mut a.state, path, 0, 0).unwrap();
        let mut peer = pl::clips(&a.state)[i];
        peer.track = 1;
        peer.set_gain(0.5);
        pl::arr_mut(&mut a.state).clips.push(peer);
        let mut outside = peer;
        outside.track = 2;
        pl::arr_mut(&mut a.state).clips.push(outside);
        for mode in Mode::ALL {
            a.state.playlist.sel = vec![i, i + 1];
            pl::run(&mut a, pl::Cmd::Normalize(i, mode));
            if let Some(crate::dialog::Modal {
                body: crate::dialog::Body::Normalize(f),
                ..
            }) = a.state.dialogs.last_mut()
            {
                f.target.set("-18");
            } else {
                panic!("normalization dialog");
            }
            command(&mut a, Command::Apply);
            tick(&mut a);
            let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while a.clip_normalization.is_some() && std::time::Instant::now() < end {
                std::thread::sleep(std::time::Duration::from_millis(5));
                tick(&mut a);
            }
            assert!(
                a.state
                    .message
                    .as_deref()
                    .is_some_and(|m| m.contains("Normalized 2")),
                "{:?}",
                a.state.message
            );
            let gain = pl::clips(&a.state)[i].gain();
            assert_eq!(pl::clips(&a.state)[i + 1].gain(), gain);
            let expected = match mode {
                Mode::Peak => -12.0412,
                Mode::Rms => -15.0515,
                Mode::Lufs => -12.04,
            };
            assert!(
                (20. * gain.log10() + expected + 18.).abs() < 0.2,
                "{mode:?}: {gain}"
            );
            assert_eq!(pl::clips(&a.state)[i + 2], outside);
            crate::history::undo(&mut a.state).unwrap();
            assert_eq!(pl::clips(&a.state)[i].gain(), 1.);
            assert_eq!(pl::clips(&a.state)[i + 1].gain(), 0.5);
        }
        a.state.playlist.sel = vec![i, i + 1];
        let original = pl::clips(&a.state).to_vec();
        open(&mut a, i, Mode::Peak).unwrap();
        let mut canvas = os_ui::Canvas::new(1280, 900, 1.).unwrap();
        a.paint(&mut canvas, None);
        if let Some(dir) = std::env::var_os("OS_SHOT_DIR") {
            canvas
                .pixmap
                .save_png(std::path::PathBuf::from(dir).join("normalize-dialog.png"))
                .unwrap();
        }
        command(&mut a, Command::Cancel);
        assert_eq!(pl::clips(&a.state), original);
        open(&mut a, i, Mode::Peak).unwrap();
        command(&mut a, Command::Reset);
        assert_eq!(pl::clips(&a.state)[i].gain(), 1.);
        assert_eq!(pl::clips(&a.state)[i + 1].gain(), 1.);
        assert_eq!(pl::clips(&a.state)[i + 2], outside);
        crate::history::undo(&mut a.state).unwrap();
        assert_eq!(pl::clips(&a.state), original);
        std::fs::remove_file(file).unwrap();
    }
}

pub struct Form {
    batch: Batch,
    mode: Mode,
    target: crate::dialog::Field,
    message: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Target,
    Apply,
    Reset,
    Cancel,
}

pub fn open(app: &mut App, index: usize, mode: Mode) -> Result<(), String> {
    if app.clip_normalization.is_some() {
        return Err("Clip normalization is already running".into());
    }
    let batch = Batch::new(&app.state, index);
    if batch.inputs.is_empty() {
        return Err("Select an audio clip to normalize".into());
    }
    let mut target = crate::dialog::Field::default();
    target.set(&mode.default_db().to_string());
    target.select_all();
    app.state
        .dialogs
        .push(crate::dialog::Modal::new(crate::dialog::Body::Normalize(
            Form {
                batch,
                mode,
                target,
                message: String::new(),
            },
        )));
    app.invalidate();
    Ok(())
}
pub fn command(app: &mut App, c: Command) {
    use crate::dialog::{Body, Modal};
    if c == Command::Cancel {
        app.state.dialogs.pop();
        app.invalidate();
        return;
    }
    let Some(Modal {
        body: Body::Normalize(f),
        ..
    }) = app.state.dialogs.last_mut()
    else {
        return;
    };
    if c == Command::Target {
        f.target.select_all();
        app.invalidate();
        return;
    }
    if c == Command::Apply
        && !f
            .target
            .text
            .trim()
            .parse::<f64>()
            .is_ok_and(|db| db.is_finite() && (-96.0..=0.).contains(&db))
    {
        f.message = "Enter a target between -96 and 0.".into();
        app.invalidate();
        return;
    }
    let Some(Modal {
        body: Body::Normalize(f),
        ..
    }) = app.state.dialogs.pop()
    else {
        return;
    };
    if !f.batch.current(&app.state) {
        app.state.message = Some("Clips changed. Reopen Normalize.".into());
        app.invalidate();
        return;
    }
    if c == Command::Reset {
        pl::snapshot_as(&mut app.state, "Reset clip gain");
        let clips = &mut pl::arr_mut(&mut app.state).clips;
        for input in f.batch.inputs {
            clips[input.index].set_gain(1.);
        }
        app.state.piano.dirty = true;
        app.state.message = Some("Selected clip gains reset to 0 dB".into());
    } else {
        let indices: Vec<_> = f.batch.inputs.iter().map(|v| v.index).collect();
        app.state.playlist.sel = indices.clone();
        app.state.playlist.normalize_request =
            Some((indices[0], f.mode, f.target.text.trim().parse().unwrap()));
    }
    app.invalidate();
}
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(NamedKey::Escape) => command(app, Command::Cancel),
        Key::Named(NamedKey::Enter) => command(app, Command::Apply),
        _ => {
            if let Some(crate::dialog::Modal {
                body: crate::dialog::Body::Normalize(f),
                ..
            }) = app.state.dialogs.last_mut()
            {
                if f.target.len() < 32 || !matches!(key, Key::Character(_)) {
                    f.target.key(key, ctrl, alt);
                }
            }
            app.invalidate();
        }
    }
}
pub fn paint(ui: &mut crate::app::Ui, r: os_ui::Rect, f: &Form) {
    use crate::{app::Action, dialog::DHit};
    use os_ui::{Color, Font, Rect};
    let yellow = Color::rgb(0xe8cf62);
    ui.c.text(
        Font::new(13),
        r.x + 20.,
        r.y + 44.,
        ui.t.text,
        &format!("{} selected audio clips", f.batch.inputs.len()),
    );
    ui.c.text(
        Font::new(13),
        r.x + 20.,
        r.y + 75.,
        ui.t.text,
        f.mode.label().trim_end_matches("..."),
    );
    ui.c.text(Font::new(13), r.x + 20., r.y + 116., ui.t.text, "Target");
    let field = Rect::new(r.x + 150., r.y + 105., r.w - 170., 34.);
    ui.c.fill_rrect(field, 4., ui.t.sunken);
    ui.c.stroke_rrect(field, 4., 1., yellow);
    if f.target.all {
        ui.c.fill_rrect(field.inset(4.), 2., ui.t.button_hi);
    }
    ui.c.text(
        Font::new(14),
        field.x + 10.,
        field.y + 9.,
        yellow,
        &f.target.text,
    );
    let caret = field.x + 10. + ui.c.text_width(Font::new(14), f.target.before_caret());
    ui.c.line(caret, field.y + 7., caret, field.bottom() - 7., 1., yellow);
    ui.hit(
        field,
        Action::Dlg(DHit::Normalize(Command::Target)),
        "Enter a normalization target",
    );
    ui.c.text_clip(
        Font::new(11),
        r.x + 20.,
        r.y + 155.,
        r.w - 40.,
        ui.t.text_dim,
        if f.message.is_empty() {
            "Reset restores selected clip gains to 0 dB."
        } else {
            &f.message
        },
    );
    let width = (r.w - 56.) / 3.;
    for (i, (label, c)) in [
        ("Reset", Command::Reset),
        ("Cancel", Command::Cancel),
        ("Apply", Command::Apply),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            r.x + 20. + i as f32 * (width + 8.),
            r.bottom() - 52.,
            width,
            32.,
        );
        let on = c == Command::Apply;
        ui.c.fill_rrect(rect, 4., if on { yellow } else { ui.t.button });
        ui.c.text_center(
            Font::new(13),
            rect,
            if on { Color::rgb(0x171914) } else { ui.t.text },
            label,
        );
        ui.hit(rect, Action::Dlg(DHit::Normalize(c)), label);
    }
}
