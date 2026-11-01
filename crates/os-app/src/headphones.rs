// SPDX-License-Identifier: GPL-3.0-or-later
//! AutoEq correction lives only at the physical output, never in Engine/export/project state.
use crate::{
    app::App,
    dialog::{Body, Field},
    settings_file,
};
#[cfg(not(test))]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::{io::Read, sync::mpsc, time::Duration};

pub const CATALOG: &str = include_str!("../assets/headphones/catalog.tsv");
const REV: &str = include_str!("../assets/headphones/REVISION");
pub fn catalog() -> &'static Vec<(&'static str, &'static str)> {
    static C: std::sync::OnceLock<Vec<(&str, &str)>> = std::sync::OnceLock::new();
    C.get_or_init(|| CATALOG.lines().filter_map(|l| l.split_once('\t')).collect())
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Band {
    kind: u8,
    hz: f64,
    db: f64,
    q: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    bands: [Band; 12],
    count: usize,
    preamp: f64,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            bands: [Band::default(); 12],
            count: 0,
            preamp: 0.,
        }
    }
}
pub fn parse(text: &str) -> Result<Profile, String> {
    let mut p = Profile::default();
    let mut pre = false;
    for l in text.lines() {
        let w: Vec<_> = l.split_whitespace().collect();
        if w.first() == Some(&"Preamp:") {
            p.preamp = w
                .get(1)
                .and_then(|s| s.parse().ok())
                .ok_or("Invalid preamp")?;
            pre = true;
        } else if w.first() == Some(&"Filter") {
            if w.len() != 12
                || w[2] != "ON"
                || w[4] != "Fc"
                || w[6] != "Hz"
                || w[7] != "Gain"
                || w[9] != "dB"
                || w[10] != "Q"
            {
                return Err("Unsupported headphone filter".into());
            }
        }
    }
    // The AutoEq text has 12 whitespace-delimited fields in each filter line.
    p.count = 0;
    for l in text.lines().filter(|l| l.starts_with("Filter")) {
        let w: Vec<_> = l.split_whitespace().collect();
        let kind = match w[3] {
            "PK" => 0,
            "LSC" => 1,
            "HSC" => 2,
            _ => return Err("Unsupported filter type".into()),
        };
        let num = |i: usize| -> Result<f64, String> {
            w[i].parse().map_err(|_| "Invalid filter value".into())
        };
        let b = Band {
            kind,
            hz: num(5)?,
            db: num(8)?,
            q: num(11)?,
        };
        if !b.hz.is_finite()
            || !(10. ..=24000.).contains(&b.hz)
            || !b.db.is_finite()
            || b.db.abs() > 30.
            || !b.q.is_finite()
            || !(0.05..=30.).contains(&b.q)
            || p.count == 12
        {
            return Err("Unsafe headphone profile".into());
        }
        p.bands[p.count] = b;
        p.count += 1;
    }
    if !pre || !p.preamp.is_finite() || !(-60. ..=0.).contains(&p.preamp) || p.count == 0 {
        return Err("Incomplete headphone profile".into());
    }
    Ok(p)
}
#[derive(Default)]
pub struct Form {
    pub search: Field,
    pub results: Vec<usize>,
    pub scroll: usize,
    pub selected: usize,
}
impl Form {
    pub fn new() -> Self {
        let mut f = Self::default();
        f.filter();
        f
    }
    pub fn filter(&mut self) {
        let q = self.search.text.to_lowercase();
        self.results = catalog()
            .iter()
            .enumerate()
            .filter(|(_, (n, _))| q.split_whitespace().all(|w| n.to_lowercase().contains(w)))
            .map(|(i, _)| i)
            .collect();
        self.scroll = 0;
        self.selected = 0;
    }
    pub fn scroll(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.results.len().saturating_sub(10));
    }
}
pub struct UiState {
    pub name: String,
    pub on: bool,
    pub status: String,
    profile: Option<Profile>,
    text: String,
}
impl Default for UiState {
    fn default() -> Self {
        let name = settings_file::get("headphones_model").unwrap_or_default();
        let text = settings_file::get("headphones_profile")
            .and_then(|s| serde_json::from_str::<String>(&s).ok())
            .unwrap_or_default();
        let profile = parse(&text).ok();
        let on =
            profile.is_some() && settings_file::get("headphones_on").as_deref() == Some("true");
        publish(profile.unwrap_or_default(), on);
        Self {
            name,
            on,
            status: String::new(),
            profile,
            text,
        }
    }
}
impl UiState {
    fn save(&self) {
        settings_file::write_all(&[
            ("headphones_model".into(), self.name.clone()),
            ("headphones_on".into(), self.on.to_string()),
            (
                "headphones_profile".into(),
                serde_json::to_string(&self.text).unwrap(),
            ),
        ]);
    }
}
pub struct Job {
    rx: mpsc::Receiver<Result<(usize, String), String>>,
}
pub fn open(app: &mut App) {
    app.show_form(Body::Headphones(Form::new()));
}
pub fn toggle(app: &mut App) {
    if app.state.headphones.profile.is_none() {
        if app
            .state
            .dialogs
            .iter()
            .any(|m| matches!(m.body, Body::Headphones(_)))
        {
            app.state.headphones.status = "Choose a headphone model from the list first.".into();
            app.invalidate();
        } else {
            open(app);
        }
        return;
    }
    let s = &mut app.state.headphones;
    s.on = !s.on;
    publish(s.profile.unwrap(), s.on);
    s.save();
    app.invalidate();
}
pub fn select(app: &mut App, index: usize) {
    if app.headphone_job.is_some() || index >= catalog().len() {
        return;
    }
    if let Some(m) = app.state.dialogs.last_mut() {
        if let Body::Headphones(f) = &mut m.body {
            if let Some(n) = f.results.iter().position(|&i| i == index) {
                f.selected = n;
            }
        }
    }
    let name = catalog()[index].0;
    app.state.headphones.status = format!("Loading {name}...");
    let (tx, rx) = mpsc::channel();
    app.headphone_job = Some(Job { rx });
    std::thread::spawn(move || {
        let result = load(index).map(|s| (index, s));
        let _ = tx.send(result);
    });
    app.invalidate();
}
fn load(index: usize) -> Result<String, String> {
    let (_, path) = catalog()[index];
    let cache = os_paths::user_dir().map(|p| {
        p.join("Headphone profiles")
            .join(REV.trim())
            .join(format!("{index}.txt"))
    });
    if let Some(p) = cache.as_ref() {
        if let Ok(s) = std::fs::read_to_string(p) {
            if parse(&s).is_ok() {
                return Ok(s);
            }
        }
    }
    let text = fetch(path)?;
    if let Some(p) = cache {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, &text);
    }
    Ok(text)
}
fn fetch(path: &str) -> Result<String, String> {
    let file = path.rsplit('/').next().ok_or("Missing profile path")?;
    let url=format!("https://raw.githubusercontent.com/jaakkopasanen/AutoEq/{}/results/{path}/{file}%20ParametricEQ.txt",REV.trim());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let response=agent.get(&url).call().map_err(|_|"Could not download this profile. Check your connection and try again. Your current correction is unchanged.".to_string())?;
    let mut text = String::new();
    response
        .into_reader()
        .take(16385)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 16384 {
        return Err("Profile is too large".into());
    }
    parse(&text)?;
    Ok(text)
}
pub fn tick(app: &mut App) {
    let Some(job) = app.headphone_job.as_ref() else {
        return;
    };
    let result = match job.rx.try_recv() {
        Ok(v) => v,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("Profile download stopped. Please try again.".into()),
    };
    app.headphone_job = None;
    let s = &mut app.state.headphones;
    match result {
        Ok((i, text)) => match parse(&text) {
            Ok(p) => {
                s.name = catalog()[i].0.into();
                s.on = true;
                s.profile = Some(p);
                s.text = text;
                s.status = format!("{} active | Headroom {:.1} dB", s.name, p.preamp);
                publish(p, true);
                s.save();
            }
            Err(e) => s.status = e,
        },
        Err(e) => s.status = e,
    }
    app.invalidate();
}
#[derive(Clone, Copy, Default)]
struct Config {
    profile: Profile,
    on: bool,
}
#[cfg(not(test))]
static CONFIG: Mutex<Config> = Mutex::new(Config {
    profile: Profile {
        bands: [Band {
            kind: 0,
            hz: 0.,
            db: 0.,
            q: 0.,
        }; 12],
        count: 0,
        preamp: 0.,
    },
    on: false,
});
#[cfg(not(test))]
static GENERATION: AtomicU64 = AtomicU64::new(1);
#[cfg(test)]
thread_local! {static TEST_CONFIG:std::cell::Cell<(u64,Config)>=std::cell::Cell::new((1,Config::default()));}
fn publish(profile: Profile, on: bool) {
    #[cfg(test)]
    TEST_CONFIG.with(|c| c.set((c.get().0 + 1, Config { profile, on })));
    #[cfg(not(test))]
    if let Ok(mut c) = CONFIG.lock() {
        *c = Config { profile, on };
        GENERATION.fetch_add(1, Ordering::Release);
    }
}
fn config_changed(previous: u64) -> Option<(u64, Config)> {
    #[cfg(test)]
    return TEST_CONFIG.with(|c| {
        let (g, v) = c.get();
        (g != previous).then_some((g, v))
    });
    #[cfg(not(test))]
    {
        let g = GENERATION.load(Ordering::Acquire);
        if g == previous {
            return None;
        }
        CONFIG.try_lock().ok().map(|c| (g, *c))
    }
}
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [[f64; 2]; 2],
}
impl Biquad {
    fn new(b: Band, rate: u32) -> Self {
        let a = 10f64.powf(b.db / 40.);
        let w = std::f64::consts::TAU * b.hz.min(rate as f64 * 0.45) / rate.max(1) as f64;
        let c = w.cos();
        let alpha = w.sin() / (2. * b.q);
        let t = 2. * a.sqrt() * alpha;
        let (bb, aa) = match b.kind {
            1 => (
                [
                    a * ((a + 1.) - (a - 1.) * c + t),
                    2. * a * ((a - 1.) - (a + 1.) * c),
                    a * ((a + 1.) - (a - 1.) * c - t),
                ],
                [
                    (a + 1.) + (a - 1.) * c + t,
                    -2. * ((a - 1.) + (a + 1.) * c),
                    (a + 1.) + (a - 1.) * c - t,
                ],
            ),
            2 => (
                [
                    a * ((a + 1.) + (a - 1.) * c + t),
                    -2. * a * ((a - 1.) + (a + 1.) * c),
                    a * ((a + 1.) + (a - 1.) * c - t),
                ],
                [
                    (a + 1.) - (a - 1.) * c + t,
                    2. * ((a - 1.) - (a + 1.) * c),
                    (a + 1.) - (a - 1.) * c - t,
                ],
            ),
            _ => (
                [1. + alpha * a, -2. * c, 1. - alpha * a],
                [1. + alpha / a, -2. * c, 1. - alpha / a],
            ),
        };
        Self {
            b: bb.map(|v| v / aa[0]),
            a: [aa[1] / aa[0], aa[2] / aa[0]],
            z: [[0.; 2]; 2],
        }
    }
    fn sample(&mut self, x: f64, ch: usize) -> f64 {
        let y = self.b[0] * x + self.z[ch][0];
        self.z[ch][0] = self.b[1] * x - self.a[0] * y + self.z[ch][1];
        self.z[ch][1] = self.b[2] * x - self.a[1] * y;
        y
    }
}
#[derive(Clone, Copy)]
struct Chain {
    filters: [Biquad; 12],
    count: usize,
    gain: f64,
}
impl Default for Chain {
    fn default() -> Self {
        Self {
            filters: [Biquad::default(); 12],
            count: 0,
            gain: 1.,
        }
    }
}
impl Chain {
    fn new(c: Config, rate: u32) -> Self {
        if !c.on {
            return Self::default();
        }
        let mut s = Self {
            gain: 10f64.powf(c.profile.preamp / 20.),
            ..Self::default()
        };
        s.count = c.profile.count;
        for (f, b) in s.filters.iter_mut().zip(c.profile.bands).take(s.count) {
            *f = Biquad::new(b, rate);
        }
        s
    }
    fn sample(&mut self, mut v: f64, ch: usize) -> f64 {
        v *= self.gain;
        for f in &mut self.filters[..self.count] {
            v = f.sample(v, ch);
        }
        v
    }
}
/// Fixed storage, no allocation or blocking lock in the device callback. Crossfades old/new chains.
pub struct Processor {
    rate: u32,
    generation: u64,
    current: Chain,
    old: Chain,
    fade: usize,
    total: usize,
}
impl Default for Processor {
    fn default() -> Self {
        Self {
            rate: 48000,
            generation: 0,
            current: Chain::default(),
            old: Chain::default(),
            fade: 0,
            total: 960,
        }
    }
}
impl Processor {
    pub fn set_rate(&mut self, rate: u32) {
        self.rate = rate.max(1);
        self.total = (self.rate / 50).max(1) as usize;
        self.generation = 0;
    }
    pub fn process(&mut self, out: &mut [f32]) {
        if let Some((g, c)) = config_changed(self.generation) {
            self.old = self.current;
            self.current = Chain::new(c, self.rate);
            self.fade = self.total;
            self.generation = g;
        }
        if self.fade == 0 && self.current.count == 0 {
            return;
        }
        for pair in out.chunks_exact_mut(2) {
            let wet = 1. - self.fade as f64 / self.total as f64;
            for (ch, v) in pair.iter_mut().enumerate() {
                let x = *v as f64;
                let y = self.current.sample(x, ch);
                *v = if self.fade > 0 {
                    (self.old.sample(x, ch) * (1. - wet) + y * wet) as f32
                } else {
                    y as f32
                };
            }
            self.fade = self.fade.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headphone_profile_download() {
        if std::env::var_os("LEMON_HEADPHONE_NETWORK").is_none() {
            return;
        }
        let (_, path) = catalog()
            .iter()
            .find(|(n, _)| *n == "Sennheiser HD 650")
            .unwrap();
        let text = fetch(path).unwrap();
        assert_eq!(parse(&text).unwrap().count, 10);
    }
    #[test]
    fn headphone_profiles_audio_and_bypass() {
        assert_eq!(catalog().len(), 6033);
        let p = parse(include_str!("../assets/headphones/test-hd650.txt")).unwrap();
        assert_eq!(p.count, 10);
        assert_eq!(p.preamp, -6.1);
        for bad in [
            "Preamp: NaN dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 1",
            "Preamp: 10 dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 1",
            "Preamp: -6 dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 0",
            "<html>oops</html>",
        ] {
            assert!(parse(bad).is_err());
        }
        // Known -6 dB preamp and +6 dB peaking filter: unity at its centre, attenuated away from it.
        let p = parse("Preamp: -6 dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 2").unwrap();
        for rate in [44100, 48000, 96000, 192000] {
            for (hz, want) in [(1000., 1.), (50., 0.501)] {
                let mut c = Chain::new(
                    Config {
                        profile: p,
                        on: true,
                    },
                    rate,
                );
                let mut input = 0.;
                let mut output = 0.;
                for n in 0..rate / 2 {
                    let x = (std::f64::consts::TAU * hz * n as f64 / rate as f64).sin() * 0.2;
                    let y = c.sample(x, 0);
                    assert!(y.is_finite());
                    if n > rate / 4 {
                        input += x * x;
                        output += y * y;
                    }
                }
                assert!(
                    ((output / input).sqrt() - want).abs() < 0.005,
                    "rate {rate}, freq {hz}"
                );
            }
            // Verify the shelf types reach their expected low/high-frequency gains too.
            for (kind, hz, want) in [(1, 20., 2.), (1, 18000., 1.), (2, 20., 1.), (2, 18000., 2.)] {
                let mut b = Biquad::new(
                    Band {
                        kind,
                        hz: 1000.,
                        db: 6.0206,
                        q: 0.707,
                    },
                    rate,
                );
                let mut input = 0.;
                let mut output = 0.;
                for n in 0..rate {
                    let x = (std::f64::consts::TAU * hz * n as f64 / rate as f64).sin() * 0.1;
                    let y = b.sample(x, 0);
                    if n > rate / 2 {
                        input += x * x;
                        output += y * y;
                    }
                }
                assert!(((output / input).sqrt() - want).abs() < 0.01);
            }
        }
        let path =
            std::env::temp_dir().join(format!("lemon-headphone-export-{}.wav", std::process::id()));
        let audio: Vec<_> = (0..22050)
            .flat_map(|i| {
                let v = (i as f32 * 0.073).sin() * 0.15;
                [v, v * 0.7]
            })
            .collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), 44100, &audio)
            .unwrap();
        let mut project = crate::app::test_project();
        let ch = &mut project.channels[0];
        ch.kind = os_model::ChannelKind::AudioClip;
        ch.sample_path = Some(path.to_string_lossy().into_owned());
        ch.mixer_track = 1;
        ch.sound.sampler_flags = Some(os_model::ChannelSound::AUDIO_CLIP_FLAGS);
        project
            .arrangements
            .push(os_flp::project::default_arrangement());
        project.arrangements[0].clips.push(os_model::Clip::new(
            0,
            96,
            0,
            os_model::ClipSource::Channel(0),
        ));
        let render = || {
            let mut lib = os_engine::SampleLibrary::new(None);
            let mut engine =
                os_engine::Engine::new(&project, &mut lib, 44100, os_engine::PlayMode::Song);
            engine.set_span(0, 96);
            engine.render_all(96)
        };
        publish(p, false);
        let before = render();
        assert!(before.iter().any(|v| v.abs() > 0.001));
        publish(p, true);
        assert_eq!(
            before,
            render(),
            "offline export never receives headphone correction"
        );
        let _ = std::fs::remove_file(path);
        let mut dsp = Processor::default();
        dsp.set_rate(48000);
        publish(p, true);
        let mut b = vec![0.1; 9600];
        dsp.process(&mut b);
        assert!(b.iter().all(|v| v.is_finite()));
        assert!((b[9598] - 0.0501187).abs() < 0.0001);
        publish(p, false);
        let mut dry = vec![0.1; 9600];
        dsp.process(&mut dry);
        assert_eq!(dry[9598], 0.1);
        assert!(
            (dry[0] - b[9598]).abs() < 0.0001,
            "bypass starts without a step"
        );
        let raw = vec![0.2, -0.1, 0.3, -0.2];
        let mut got = raw.clone();
        dsp.process(&mut got);
        assert_eq!(got, raw, "bypass is bit exact after crossfade");
        let mut search = Form::new();
        search.search.set("sennheiser 650");
        search.filter();
        assert!(search
            .results
            .iter()
            .any(|&i| catalog()[i].0 == "Sennheiser HD 650"));
        search.search.set("not a real headphone zzzz");
        search.filter();
        assert!(search.results.is_empty());
    }
}
