// SPDX-License-Identifier: GPL-3.0-or-later
//! Lemon Chop: original slicing sampler using Lemon's audio and editor infrastructure.
#![allow(semicolon_in_expressions_from_non_local_macros)]
pub mod audio;
pub mod controller;
pub mod cues;
pub mod dsp;
pub mod editor;
pub mod export;
pub mod params;
pub mod view;
use nih_plug::prelude::*;
use params::*;
use std::{
    num::NonZeroU32,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};
pub type Separator = fn(&str, &AtomicBool) -> Result<[String; 4], String>;
static SEPARATOR: OnceLock<Separator> = OnceLock::new();
pub fn set_separator(f: Separator) {
    let _ = SEPARATOR.set(f);
}
pub struct Shared {
    pub editor_open: AtomicBool,
    pub preview_cursor: AtomicU64,
    pub preview_active: AtomicBool,
    pub seek_preview: AtomicBool,
    pub played_pad: AtomicU32,
    pub captures: crossbeam_queue::ArrayQueue<(usize, f64, Arc<Document>, u64)>,
    pub capture_cursor: AtomicU64,
    pub snap: AtomicBool,
    pub audio: Mutex<Option<Arc<audio::Audio>>>,
    pub status: Mutex<String>,
    pub notes: crossbeam_queue::ArrayQueue<(u8, f32, bool)>,
    pub sounding: AtomicU32,
    pub position: AtomicU64,
    pub tempo: AtomicU64,
    pub cancel: Arc<AtomicBool>,
    pub busy: AtomicBool,
    pub analysis_version: AtomicU64,
    pub reload: AtomicU64,
    pub stop: AtomicBool,
    pub analysing: AtomicBool,
    pub cancel_analysis: AtomicBool,
    pub waveform: Mutex<Vec<(f32, f32)>>,
    pub metronome: AtomicBool,
    pub controller: crossbeam_queue::ArrayQueue<controller::Action>,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            editor_open: AtomicBool::new(false),
            preview_cursor: AtomicU64::new(0),
            preview_active: AtomicBool::new(false),
            seek_preview: AtomicBool::new(false),
            played_pad: AtomicU32::new(u32::MAX),
            captures: crossbeam_queue::ArrayQueue::new(128),
            capture_cursor: AtomicU64::new(0),
            snap: AtomicBool::new(true),
            audio: Mutex::new(None),
            status: Mutex::new("Load audio to start chopping".into()),
            notes: crossbeam_queue::ArrayQueue::new(256),
            sounding: AtomicU32::new(0),
            position: AtomicU64::new(0),
            tempo: AtomicU64::new(120f64.to_bits()),
            cancel: Arc::new(AtomicBool::new(false)),
            busy: AtomicBool::new(false),
            analysis_version: AtomicU64::new(0),
            reload: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            analysing: AtomicBool::new(false),
            cancel_analysis: AtomicBool::new(false),
            waveform: Mutex::new(Vec::new()),
            metronome: AtomicBool::new(false),
            controller: crossbeam_queue::ArrayQueue::new(128),
        }
    }
}
impl Shared {
    fn persist_captures(&self, p: &ChopParams) {
        while let Some((n, time, original, generation)) = self.captures.pop() {
            let current = p.document();
            if current.path == original.path
                && self.reload.load(Ordering::Acquire) == generation
                && !current.cues[n].enabled
                && p.pending_capture(n)
            {
                let a = self.audio.lock().unwrap().clone();
                if let Some(a) = a.filter(|a| a.path == current.path && time < a.duration()) {
                    let mut d = (*current).clone();
                    d.cues[n] = Cue {
                        start: time,
                        end: a.duration(),
                        enabled: true,
                        name: format!("Chop {:02}", n + 1),
                        ..Default::default()
                    };
                    d.analysis_complete = true;
                    p.set_doc(d);
                }
            }
        }
    }
    pub fn message(&self, s: impl Into<String>) {
        *self.status.lock().unwrap() = s.into();
    }
    pub fn separate(self: &Arc<Self>, p: Arc<ChopParams>) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(f) = SEPARATOR.get().copied() else {
            self.message("Stem separation is available inside Lemon Studio");
            self.busy.store(false, Ordering::Release);
            return;
        };
        self.cancel.store(false, Ordering::Release);
        self.message("Separating vocals, drums, bass and instruments...");
        let shared = self.clone();
        let old = p.document();
        std::thread::spawn(move || {
            match f(&old.path, &shared.cancel) {
                Ok(stems) => {
                    if p.document().path == old.path && !shared.cancel.load(Ordering::Acquire) {
                        let mut d = (*p.document()).clone();
                        d.stems = stems;
                        p.set_doc(d);
                        shared.message("Loading separated stems...");
                    }
                }
                Err(e) => shared.message(e),
            }
            shared.busy.store(false, Ordering::Release);
        });
    }
}
pub struct LemonChop {
    pub params: Arc<ChopParams>,
    pub shared: Arc<Shared>,
    voices: Vec<dsp::Voice>,
    doc: Arc<Document>,
    audio: Option<Arc<audio::Audio>>,
    values: Vec<f32>,
    next: usize,
    last_click: i64,
    click_age: usize,
    sample_rate: f32,
    pending_cues: [Option<f64>; 32],
}
impl Default for LemonChop {
    fn default() -> Self {
        let params = Arc::new(ChopParams::default());
        let shared = Arc::new(Shared::default());
        let weak = Arc::downgrade(&shared);
        let wp = Arc::downgrade(&params);
        std::thread::spawn(move || {
            let mut loaded = (
                String::new(),
                [String::new(), String::new(), String::new(), String::new()],
                0,
            );
            let mut wave_gains = [f32::NAN; 4];
            loop {
                std::thread::sleep(std::time::Duration::from_millis(80));
                let (Some(sh), Some(p)) = (weak.upgrade(), wp.upgrade()) else {
                    break;
                };
                sh.persist_captures(&p);
                let d = p.document();
                let requested = (
                    d.path.clone(),
                    d.stems.clone(),
                    sh.reload.load(Ordering::Acquire),
                );
                if requested == loaded {
                    let gains = std::array::from_fn(|i| p.v(12 + i));
                    if gains != wave_gains {
                        let a = sh.audio.lock().unwrap().clone();
                        if let Some(a) = a.filter(|a| a.path == d.path) {
                            if let Some(peaks) =
                                a.mixed_peaks(gains, &|| p.document().path != d.path)
                            {
                                *sh.waveform.lock().unwrap() = peaks;
                                wave_gains = gains;
                            }
                        }
                    }
                    continue;
                }
                loaded = requested.clone();
                if d.path.is_empty() {
                    *sh.audio.lock().unwrap() = None;
                    sh.waveform.lock().unwrap().clear();
                    sh.message("Load audio to start chopping");
                    continue;
                }
                sh.cancel_analysis.store(false, Ordering::Release);
                sh.analysing.store(true, Ordering::Release);
                sh.message("Reading and analysing audio...");
                let cancelled = || {
                    sh.cancel_analysis.load(Ordering::Acquire)
                        || p.document().path != requested.0
                        || p.document().stems != requested.1
                        || sh.reload.load(Ordering::Acquire) != requested.2
                };
                match audio::Audio::load_cancellable(&d.path, &d.stems, &cancelled) {
                    Ok(a) => {
                        let now = p.document();
                        if now.path == requested.0 && now.stems == requested.1 && !cancelled() {
                            *sh.waveform.lock().unwrap() = a.peaks.clone();
                            wave_gains = [f32::NAN; 4];
                            *sh.audio.lock().unwrap() = Some(Arc::new(a));
                            sh.analysis_version.fetch_add(1, Ordering::Release);
                            sh.message("Ready — click pads to play; drag cue edges to trim");
                        }
                    }
                    Err(e) => {
                        if p.document().path == requested.0
                            && p.document().stems == requested.1
                            && sh.reload.load(Ordering::Acquire) == requested.2
                        {
                            *sh.audio.lock().unwrap() = None;
                            sh.waveform.lock().unwrap().clear();
                            sh.message(e);
                        }
                    }
                }
                sh.analysing.store(false, Ordering::Release);
            }
        });
        Self {
            doc: params.document(),
            values: vec![0.; GLOBAL + 32 * STRIDE],
            params,
            shared,
            voices: Vec::new(),
            audio: None,
            next: 0,
            last_click: i64::MIN,
            click_age: usize::MAX,
            sample_rate: 48000.,
            pending_cues: [None; 32],
        }
    }
}
impl Drop for LemonChop {
    fn drop(&mut self) {
        self.shared.cancel_analysis.store(true, Ordering::Release);
        self.shared.cancel.store(true, Ordering::Release);
    }
}
impl LemonChop {
    fn note(&mut self, mut note: u8, mut channel: u8, velocity: f32, on: bool) {
        if self.doc.slab {
            if let Some(action) = controller::button(channel, note) {
                if on && velocity > 0. {
                    let _ = self.shared.controller.push(action);
                }
                return;
            }
            if let Some(mapped) = controller::pad_note(channel, note) {
                note = mapped;
                channel = 0;
            } else if channel == 7 || channel == 8 || channel == 6 {
                return;
            }
        }
        if !on || velocity == 0. {
            for v in &mut self.voices {
                v.off(note, channel);
            }
            return;
        }
        if note == 127 && channel == u8::MAX {
            if let Some(audio) = self.audio.as_ref().filter(|a| a.path == self.doc.path) {
                for v in &mut self.voices {
                    v.stop();
                }
                let mut values = [0.; GLOBAL + 32 * STRIDE];
                values.copy_from_slice(&self.values);
                values[HOLD] = 0.;
                values[KEYBOARD] = 0.;
                for k in 0..STRIDE {
                    values[pad(0, k)] = self.params.defs[pad(0, k)].default;
                }
                self.voices[0].start(
                    audio.clone(),
                    &self.doc,
                    &values,
                    0,
                    127,
                    0,
                    1.,
                    f64::from_bits(self.shared.tempo.load(Ordering::Relaxed)),
                    Some(f64::from_bits(
                        self.shared.preview_cursor.load(Ordering::Relaxed),
                    )),
                );
            }
            return;
        }
        let n = if self.values[KEYBOARD] > 0.5 {
            self.doc.selected
        } else {
            let Some(i) = note.checked_sub(36).filter(|i| *i < 32) else {
                return;
            };
            i as usize
        };
        let Some(audio) = &self.audio else { return };
        if audio.path != self.doc.path {
            return;
        }
        self.shared.played_pad.store(n as u32, Ordering::Release);
        let capture = if !self.doc.cues[n].enabled && self.values[KEYBOARD] < 0.5 {
            let mut time = self.pending_cues[n].unwrap_or_else(|| {
                self.voices.iter().find(|v| v.active).map_or_else(
                    || f64::from_bits(self.shared.capture_cursor.load(Ordering::Relaxed)),
                    |v| v.position(),
                )
            });
            if self.shared.snap.load(Ordering::Relaxed) {
                let bpm = self.values[SOURCE_BPM] as f64;
                time = self
                    .doc
                    .time_at((self.doc.beat_at(time, bpm) * 8.).round() / 8., bpm);
            }
            time = time.clamp(0., audio.duration());
            if time >= audio.duration() {
                return;
            }
            if self.pending_cues[n].is_none() {
                self.params
                    .capture_cue(n, time, audio.duration(), &self.doc.path);
                if self
                    .shared
                    .captures
                    .push((
                        n,
                        time,
                        self.doc.clone(),
                        self.shared.reload.load(Ordering::Acquire),
                    ))
                    .is_err()
                {
                    self.params.clear_capture(n);
                    return;
                }
                self.pending_cues[n] = Some(time);
            }
            Some(time)
        } else {
            None
        };
        if self.values[POLY] < 0.5 && self.values[KEYBOARD] > 0.5 && self.values[GLIDE] > 0. {
            if let Some(v) = self.voices.iter_mut().find(|v| v.active) {
                if self.doc.glide_trigger {
                    let old_note = v.note;
                    v.start(
                        audio.clone(),
                        &self.doc,
                        &self.values,
                        n,
                        old_note,
                        channel,
                        velocity,
                        f64::from_bits(self.shared.tempo.load(Ordering::Relaxed)),
                        None,
                    );
                }
                v.channel = channel;
                v.glide_to(
                    note,
                    self.values[GLIDE],
                    std::array::from_fn(|i| self.values[12 + i]),
                );
                return;
            }
        }
        if self.values[POLY] < 0.5 {
            for v in &mut self.voices {
                v.stop();
            }
        }
        let i = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or(self.next % 32);
        self.next = (i + 1) % 32;
        self.voices[i].start(
            audio.clone(),
            &self.doc,
            &self.values,
            n,
            note,
            channel,
            velocity,
            f64::from_bits(self.shared.tempo.load(Ordering::Relaxed)),
            capture,
        );
    }
}
impl Plugin for LemonChop {
    const NAME: &'static str = "Lemon Chop";
    const VENDOR: &'static str = "Lemon Studio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_output_ports: &[NonZeroU32::new(2).unwrap(); 15],
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;
    type SysExMessage = ();
    type BackgroundTask = ();
    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }
    fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(editor::ChopEditor::new(
            self.params.clone(),
            self.shared.clone(),
        )))
    }
    fn initialize(
        &mut self,
        _: &AudioIOLayout,
        c: &BufferConfig,
        _: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = c.sample_rate;
        self.voices = (0..32)
            .map(|_| dsp::Voice::new(c.sample_rate as u32))
            .collect();
        let d = self.params.document();
        if !d.path.is_empty() {
            match audio::Audio::load(&d.path, &d.stems) {
                Ok(a) => {
                    let a = Arc::new(a);
                    *self.shared.audio.lock().unwrap() = Some(a.clone());
                    self.audio = Some(a);
                }
                Err(e) => self.shared.message(e),
            }
        }
        true
    }
    fn reset(&mut self) {
        self.shared.preview_active.store(false, Ordering::Relaxed);
        self.shared.seek_preview.store(false, Ordering::Relaxed);
        self.last_click = i64::MIN;
        self.click_age = usize::MAX;
        for v in &mut self.voices {
            v.stop();
        }
        while self.shared.notes.pop().is_some() {}
        self.shared.sounding.store(0, Ordering::Relaxed);
    }
    fn process(
        &mut self,
        b: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if let Ok(d) = self.params.doc.try_read() {
            if !Arc::ptr_eq(&d, &self.doc) {
                self.pending_cues.fill(None);
            }
            if d.path != self.doc.path {
                for v in &mut self.voices {
                    v.stop();
                }
            }
            self.doc = d.clone();
        }
        if let Ok(a) = self.shared.audio.try_lock() {
            self.audio = a.clone();
        }
        for (i, v) in self.values.iter_mut().enumerate() {
            *v = self.params.v(i);
        }
        if let Some(bpm) = ctx.transport().tempo {
            self.shared.tempo.store(bpm.to_bits(), Ordering::Relaxed);
        }
        if self.shared.stop.swap(false, Ordering::Relaxed) {
            self.reset();
        }
        if self.shared.seek_preview.swap(false, Ordering::AcqRel)
            && self.shared.preview_active.load(Ordering::Relaxed)
        {
            self.note(127, u8::MAX, 1., true);
        }
        let tempo = f64::from_bits(self.shared.tempo.load(Ordering::Relaxed));
        for v in &mut self.voices {
            v.update(&self.values, tempo);
        }
        while let Some((n, v, on)) = self.shared.notes.pop() {
            self.note(n, if n == 127 { u8::MAX } else { 0 }, v, on);
        }
        for o in aux.outputs.iter_mut() {
            for channel in o.as_slice() {
                channel.fill(0.);
            }
        }
        let gains = std::array::from_fn(|i| self.values[12 + i]);
        let master = 10f32.powf(self.values[MASTER] / 20.);
        let mut event = ctx.next_event();
        let out = b.as_slice();
        if out.len() < 2 {
            return ProcessStatus::Normal;
        }
        for frame in 0..out[0].len() {
            while event.as_ref().is_some_and(|e| e.timing() as usize <= frame) {
                match event.take().unwrap() {
                    NoteEvent::NoteOn {
                        note,
                        channel,
                        velocity,
                        ..
                    } => self.note(note, channel, velocity, true),
                    NoteEvent::NoteOff { note, channel, .. } => self.note(note, channel, 0., false),
                    NoteEvent::MidiCC {
                        channel, cc, value, ..
                    } if self.doc.slab => {
                        if let Some(a) = controller::cc(channel, cc, (value * 127.).round() as u8) {
                            let _ = self.shared.controller.push(a);
                        }
                    }
                    _ => {}
                }
                event = ctx.next_event();
            }
            let mut sum = [0f32; 2];
            for v in &mut self.voices {
                let sample = v.tick(gains);
                if v.output > 0 && v.output <= aux.outputs.len() {
                    let dst = aux.outputs[v.output - 1].as_slice();
                    if dst.len() >= 2 && frame < dst[0].len() {
                        dst[0][frame] += sample[0] * master;
                        dst[1][frame] += sample[1] * master;
                    }
                } else {
                    sum[0] += sample[0];
                    sum[1] += sample[1];
                }
            }
            if self.shared.metronome.load(Ordering::Relaxed) {
                if let Some(v) = self.voices.iter().find(|v| v.active && v.preview) {
                    let beat = self
                        .doc
                        .beat_at(v.position(), self.values[SOURCE_BPM] as f64)
                        .floor() as i64;
                    if beat != self.last_click {
                        self.last_click = beat;
                        self.click_age = 0;
                    }
                } else {
                    self.click_age = usize::MAX;
                    self.last_click = i64::MIN;
                }
                if self.click_age < (self.sample_rate * 0.025) as usize {
                    let t = self.click_age as f32 / self.sample_rate;
                    let hz = if self.last_click.rem_euclid(4) == 0 {
                        1600.
                    } else {
                        1000.
                    };
                    let click = (std::f32::consts::TAU * hz * t).sin() * (-t * 160.).exp() * 0.15;
                    sum[0] += click;
                    sum[1] += click;
                    self.click_age += 1;
                }
            } else {
                self.last_click = i64::MIN;
            }
            out[0][frame] = sum[0] * master;
            out[1][frame] = sum[1] * master;
        }
        let mut sounding = 0u32;
        for v in &self.voices {
            if v.active {
                sounding |= 1 << v.pad;
                self.shared
                    .position
                    .store(v.position().to_bits(), Ordering::Relaxed);
            }
        }
        self.shared.sounding.store(sounding, Ordering::Relaxed);
        self.shared.preview_active.store(
            self.voices.iter().any(|v| v.active && v.preview),
            Ordering::Relaxed,
        );
        ProcessStatus::KeepAlive
    }
}
impl ClapPlugin for LemonChop {
    const CLAP_ID: &'static str = "org.lemonstudio.lemon-chop";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("32-pad sample chopping and stem instrument");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}
impl Vst3Plugin for LemonChop {
    const VST3_CLASS_ID: [u8; 16] = *b"LemonStudioChop1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Sampler];
}
#[cfg(not(feature = "linked"))]
nih_export_clap!(LemonChop);
#[cfg(not(feature = "linked"))]
nih_export_vst3!(LemonChop);
os_plugkit::linked_clap_entry!(LemonChop);

#[cfg(test)]
mod polish_tests {
    use super::*;
    #[test]
    fn midi_empty_pad_captures_immediately_and_persists_off_audio_thread() {
        let mut chop = LemonChop::default();
        let a = source();
        let d = Document {
            path: a.path.clone(),
            analysis_complete: true,
            ..Default::default()
        };
        chop.params.set_doc(d);
        chop.doc = chop.params.document();
        chop.audio = Some(a.clone());
        *chop.shared.audio.lock().unwrap() = Some(a);
        chop.values = chop.params.defs.iter().map(|d| d.default).collect();
        chop.voices = (0..32).map(|_| dsp::Voice::new(8000)).collect();
        chop.shared
            .capture_cursor
            .store(0.27f64.to_bits(), Ordering::Relaxed);
        chop.note(39, 0, 1., true);
        assert!(chop.voices[0].active);
        assert!((chop.voices[0].position() - 0.25).abs() < 1e-9);
        assert!(!chop.params.document().cues[3].enabled);
        let fields = chop.params.serialize_fields();
        let saved: Document = serde_json::from_str(&fields["chop-document"]).unwrap();
        assert!(
            saved.cues[3].enabled,
            "saving immediately after MIDI capture retains its cue"
        );
        let peak = (0..100)
            .map(|_| chop.voices[0].tick([1.; 4])[0].abs())
            .fold(0f32, f32::max);
        assert!(peak > 0.3);
        chop.note(39, 0, 0., false);
        for _ in 0..100 {
            chop.voices[0].tick([1.; 4]);
        }
        assert!(!chop.voices[0].active);
        chop.shared.persist_captures(&chop.params);
        assert_eq!(chop.params.document().cues[3].start, 0.25);
        assert!(chop.params.document().cues[3].enabled);
        let mut ui = editor::Logic::new(chop.params.clone(), chop.shared.clone(), None);
        ui.poll();
        assert_eq!(chop.params.document().selected, 3);
        ui.mode = 1;
        ui.button("Autoset", Default::default());
        let end = chop.params.document().cues[0].end;
        ui.button("Size -", Default::default());
        assert!(chop.params.document().cues[0].end < end);
        ui.button("Grid", Default::default());
        ui.cursor = 1.;
        ui.button("Grid marker", Default::default());
        assert_eq!(chop.params.document().marker_beats, vec![0., 2.]);
        ui.button("Slip >", Default::default());
        assert_eq!(chop.params.document().markers[0], 0.01);
        ui.button("Grid cancel", Default::default());
        assert!(chop.params.document().markers.is_empty());
    }
    #[test]
    fn trigger_envelope_export_and_old_project_sound_are_preserved() {
        let a = source();
        let mut d = Document::default();
        d.cues[0] = Cue {
            start: 0.,
            end: 1.,
            enabled: true,
            ..Default::default()
        };
        let p = ChopParams::default();
        let mut values: Vec<_> = p.defs.iter().map(|d| d.default).collect();
        values[HOLD] = 0.;
        values[pad(0, ATTACK)] = 0.1;
        values[pad(0, RELEASE)] = 0.2;
        let mut out = std::io::Cursor::new(Vec::new());
        let frames = export::render(&mut out, a.clone(), &d, &values, 120., &|| false).unwrap();
        assert!(
            (frames as i64 - 2401).abs() < 3,
            "attack plus release duration: {frames}"
        );
        d.trigger_decay = false;
        let frames = export::render(
            &mut std::io::Cursor::new(Vec::new()),
            a.clone(),
            &d,
            &values,
            120.,
            &|| false,
        )
        .unwrap();
        assert!((frames as i64 - 8001).abs() < 3);
        d.trigger_decay = true;
        values[HOLD] = 1.;
        let frames = export::render(
            &mut std::io::Cursor::new(Vec::new()),
            a,
            &d,
            &values,
            120.,
            &|| false,
        )
        .unwrap();
        assert!(
            (frames as i64 - 8001).abs() < 3,
            "Hold export must not become a Trigger decay"
        );
    }
    #[test]
    fn failed_source_retries_same_path_and_analyzes_right_channel() {
        let path = std::env::temp_dir().join(format!(
            "chop-retry-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let chop = LemonChop::default();
        let mut ui = editor::Logic::new(chop.params.clone(), chop.shared.clone(), None);
        ui.load(path.to_string_lossy().into_owned());
        let wait = |ready: &dyn Fn() -> bool| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !ready() {
                assert!(
                    std::time::Instant::now() < end,
                    "background loading timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        wait(&|| {
            !chop.shared.analysing.load(Ordering::Acquire)
                && chop.shared.status.lock().unwrap().as_str() != "Load audio to start chopping"
        });
        assert!(chop.shared.audio.lock().unwrap().is_none());
        let mut bytes = Vec::new();
        os_engine::wav::write_f32_stereo_header(&mut bytes, 8000, 8000).unwrap();
        for i in 0..8000 {
            bytes.extend(0f32.to_le_bytes());
            bytes.extend(
                ((std::f32::consts::TAU * 220. * i as f32 / 8000.).sin() * 0.5).to_le_bytes(),
            );
        }
        std::fs::write(&path, bytes).unwrap();
        ui.load(path.to_string_lossy().into_owned());
        wait(&|| chop.shared.analysis_version.load(Ordering::Acquire) > 0);
        ui.poll();
        let a = chop.shared.audio.lock().unwrap().clone().unwrap();
        assert!(a.peaks.iter().any(|p| p.0 > 0.4));
        assert_ne!(a.key, "Unknown");
        assert!(chop.params.document().cues.iter().any(|c| c.enabled));
        ui.button("All", editor::Mods::default());
        ui.button("Delete", editor::Mods::default());
        chop.shared.analysis_version.fetch_add(1, Ordering::Release);
        ui.poll();
        assert!(
            !chop.params.document().cues.iter().any(|c| c.enabled),
            "stem reload must not recreate deleted pads"
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn source_preview_ignores_pad_processing_and_stale_audio() {
        let mut chop = LemonChop::default();
        let a = source();
        let mut d = Document {
            path: a.path.clone(),
            ..Default::default()
        };
        chop.doc = Arc::new(d.clone());
        chop.audio = Some(a);
        chop.voices = (0..32).map(|_| dsp::Voice::new(8000)).collect();
        chop.values = chop.params.defs.iter().map(|d| d.default).collect();
        chop.values[pad(0, OUTPUT)] = 7.;
        chop.values[pad(0, LEVEL)] = -12.;
        chop.values[pad(0, FILTER)] = -1.;
        chop.note(127, u8::MAX, 1., true);
        assert_eq!(chop.voices[0].output, 0);
        let peak = (0..1000)
            .map(|_| chop.voices[0].tick([1.; 4])[0].abs())
            .fold(0f32, f32::max);
        assert!(peak > 0.35);
        chop.voices[0].stop();
        d.path = "different.wav".into();
        chop.doc = Arc::new(d);
        chop.note(127, u8::MAX, 1., true);
        assert!(!chop.voices[0].active);
        let mut d = (*chop.doc).clone();
        d.path = source().path.clone();
        d.cues[0] = Cue {
            start: 0.,
            end: 1.,
            enabled: true,
            ..Default::default()
        };
        chop.doc = Arc::new(d);
        chop.values[KEYBOARD] = 1.;
        chop.note(127, 0, 1., true);
        assert!(chop.voices[0].active);
        assert!(!chop.voices[0].preview);
        assert_eq!(chop.voices[0].output, 7);
    }
    fn source() -> Arc<audio::Audio> {
        let rate = 8000;
        let left: Vec<f32> = (0..16000)
            .map(|i| (std::f32::consts::TAU * 220. * i as f32 / rate as f32).sin() * 0.4)
            .collect();
        let sample = Arc::new(os_engine::Sample {
            rate,
            left: left.clone(),
            right: left,
        });
        Arc::new(audio::Audio {
            path: "synthetic".into(),
            source: sample,
            stems: Default::default(),
            peaks: vec![],
            onsets: vec![0.],
            bpm: 120.,
            key: "A".into(),
        })
    }
    #[test]
    fn processed_export_stem_waveform_and_cancellation() {
        let a = source();
        let mut d = Document::default();
        d.cues[0] = Cue {
            start: 0.,
            end: 1.,
            enabled: true,
            ..Default::default()
        };
        let p = ChopParams::default();
        let mut v: Vec<_> = p.defs.iter().map(|d| d.default).collect();
        v[pad(0, SPEED)] = 0.5;
        v[pad(0, PITCH)] = 12.;
        v[pad(0, LEVEL)] = -6.;
        let mut out = std::io::Cursor::new(Vec::new());
        let frames = export::render(&mut out, a.clone(), &d, &v, 120., &|| false).unwrap();
        assert!((frames as i64 - 16000).abs() < 5);
        let bytes = out.into_inner();
        let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let x: Vec<f32> = bytes[data..]
            .chunks_exact(8)
            .map(|b| f32::from_le_bytes(b[..4].try_into().unwrap()))
            .collect();
        let cross = x[4000..12000]
            .windows(2)
            .filter(|w| w[0] < 0. && w[1] >= 0.)
            .count();
        assert!((cross as i32 - 440).abs() < 12);
        assert!(x.iter().all(|v| v.is_finite() && v.abs() < 0.4));
        assert!(
            export::render(
                &mut std::io::Cursor::new(Vec::new()),
                a.clone(),
                &d,
                &v,
                120.,
                &|| true
            )
            .unwrap_err()
            .contains("cancelled")
        );
        let mut mixed = audio::Audio {
            path: "".into(),
            source: a.source.clone(),
            stems: std::array::from_fn(|_| Some(a.source.clone())),
            peaks: vec![],
            onsets: vec![],
            bpm: 120.,
            key: "".into(),
        };
        assert!(
            mixed
                .mixed_peaks([0.; 4], &|| false)
                .unwrap()
                .iter()
                .all(|p| p.0 == 0.)
        );
        assert!(
            mixed
                .mixed_peaks([1., 0., 0., 0.], &|| false)
                .unwrap()
                .iter()
                .any(|p| p.0 > 0.3)
        );
        mixed.stems[1] = Some(Arc::new(os_engine::Sample {
            rate: 8000,
            left: a.source.left.iter().map(|v| -v).collect(),
            right: a.source.right.iter().map(|v| -v).collect(),
        }));
        assert!(
            mixed
                .mixed_peaks([1., 1., 0., 0.], &|| false)
                .unwrap()
                .iter()
                .all(|p| p.0 < 1e-6)
        );
    }
    #[test]
    fn tempo_map_grid_cancel_tap_eject_and_glide_modes() {
        let mut d = Document {
            markers: vec![0., 2., 5.],
            ..Default::default()
        };
        for t in [-1., 0., 1., 3., 5., 7.] {
            assert!((d.time_at(d.beat_at(t, 120.), 120.) - t).abs() < 1e-9);
        }
        assert_eq!(d.bpm_at(3., 120.), 80.);
        let mut slices = d.clone();
        slices.autoset(1, 10., 120., 0., 1., &[], 1);
        assert_eq!(slices.cues[4].start, 2.);
        assert_eq!(slices.cues[4].end, 2.75);
        assert_eq!(slices.cues[8].start, 5.);
        let p = Arc::new(ChopParams::default());
        let sh = Arc::new(Shared::default());
        p.set_doc(d.clone());
        let mut ui = editor::Logic::new(p.clone(), sh.clone(), None);
        let m = editor::Mods::default();
        ui.button("Grid", m);
        ui.cursor = 1.;
        ui.button("Grid start", m);
        ui.button("Grid cancel", m);
        assert_eq!(p.document().markers, d.markers);
        ui.tap(10.);
        ui.tap(10.5);
        ui.tap(11.);
        assert!((p.v(SOURCE_BPM) - 120.).abs() < 0.01);
        d.path = "synthetic".into();
        d.markers.clear();
        d.cues[0] = Cue {
            start: 0.,
            end: 2.,
            enabled: true,
            ..Default::default()
        };
        p.set_doc(d.clone());
        let mut chop = LemonChop::default();
        chop.audio = Some(source());
        chop.doc = Arc::new(d);
        chop.values = p.defs.iter().map(|d| d.default).collect();
        chop.values[KEYBOARD] = 1.;
        chop.values[GLIDE] = 0.1;
        chop.voices = (0..32).map(|_| dsp::Voice::new(8000)).collect();
        chop.note(60, 0, 1., true);
        for _ in 0..2000 {
            chop.voices[0].tick([1.; 4]);
        }
        let before = chop.voices[0].position();
        chop.note(64, 0, 1., true);
        assert!((chop.voices[0].position() - before).abs() < 0.01);
        let mut doc = (*chop.doc).clone();
        doc.glide_trigger = true;
        chop.doc = Arc::new(doc);
        chop.note(67, 0, 1., true);
        assert!(chop.voices[0].position() < before / 2.);
        ui.button("Eject", m);
        assert!(p.document().path.is_empty());
        assert!(sh.stop.load(Ordering::Relaxed));
    }
}
