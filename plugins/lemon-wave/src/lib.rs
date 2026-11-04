// SPDX-License-Identifier: GPL-3.0-or-later
//! Lemon Wave: original two-oscillator wavetable instrument, stage one.
#![allow(semicolon_in_expressions_from_non_local_macros)]
pub mod design;
pub mod assets;
pub mod controls;
pub mod dsp;
mod effects;
pub mod fxrack;
pub mod instrument_view;
pub mod modulation;
mod modulation_editor;
pub mod reference_envelope;
pub mod reference_filter;
pub mod reference_oscillator;
mod reference_voice;
pub mod table_editor;
pub mod table_tools;
mod wavetable_editor;
pub mod wavetable;
pub mod sample;
pub mod included_library;
mod sound_browser;
use lemon_small_fx::Shared;
#[cfg(test)]
mod import_tests;
use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::{Arc, atomic::Ordering};

pub struct LemonWave {
    params: Arc<assets::WaveParams>,
    shared: Arc<Shared>,
    engine: dsp::Engine,
}
impl Default for LemonWave {
    fn default() -> Self {
        let params = Arc::new(assets::WaveParams::default());
        let shared = Arc::new(Shared {
            extension: Some(Arc::new(table_editor::TableEditor::new(params.clone()))),
            ..Shared::default()
        });
        Self {
            params,
            shared,
            engine: dsp::Engine::new(44100.),
        }
    }
}
impl Plugin for LemonWave {
    const NAME: &'static str = "Lemon Wave";
    const VENDOR: &'static str = "Lemon Studio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;
    type SysExMessage = ();
    type BackgroundTask = ();
    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }
    fn filter_state(state: &mut PluginState) {
        // NIH-plug retains current values for missing parameters. Explicitly
        // initialize appended controls so an old patch cannot inherit Reference
        // mode from whichever patch was loaded immediately before it.
        for setting in &controls::SETTINGS[44..] {
            state
                .params
                .entry(setting.id.to_string())
                .or_insert(nih_plug::wrapper::state::ParamValue::I32(setting.start));
        }
        if let Some(raw) = state.fields.get_mut("state") {
            let hex = raw.trim_matches('"');
            if hex.starts_with("01000000") && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                let old_count = hex.len().saturating_sub(8) / 8;
                if (old_count == 44 || old_count == 50 || old_count == 62 || old_count == 72 || old_count == 109 || old_count == 128)
                    && hex.len() == 8 + old_count * 8
                {
                    let suffix: String = controls::SETTINGS[old_count..]
                        .iter()
                        .flat_map(|s| s.start.to_le_bytes())
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    *raw = format!("\"{hex}{suffix}\"");
                }
            }
        }
    }
    fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(lemon_small_fx::editor::FxEditor::new(
            controls::SPEC,
            self.params.base.clone(),
            self.shared.clone(),
        )))
    }
    fn initialize(
        &mut self,
        _: &AudioIOLayout,
        c: &BufferConfig,
        _: &mut impl InitContext<Self>,
    ) -> bool {
        if !c.sample_rate.is_finite() || c.sample_rate < 8000. || c.sample_rate > 384000. {
            return false;
        }
        self.engine = dsp::Engine::new(c.sample_rate);
        self.engine
            .set(std::array::from_fn(|i| self.params.base.v(i) as f32));
        self.engine.custom = self.params.tables.read().unwrap().clone();
        self.engine.reset();
        self.shared
            .rate
            .store(c.sample_rate as u32, Ordering::Relaxed);
        true
    }
    fn reset(&mut self) {
        self.engine.reset();
    }
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _: &mut AuxiliaryBuffers,
        ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.engine
            .set(std::array::from_fn(|i| self.params.base.v(i) as f32));
        // Never block the audio callback behind an editor import. The writer retains
        // retired assets so replacing this Arc cannot free a large table here.
        if let Ok(tables) = self.params.tables.try_read() {
            self.engine.custom = tables.clone();
        }
        if let Ok(curves) = self.params.curves.try_read() {
            self.engine.curves = *curves;
        }
        let tempo = ctx.transport().tempo.unwrap_or(120.) as f32;
        self.engine.tempo = if tempo.is_finite() {
            tempo.clamp(20., 400.)
        } else {
            120.
        };
        let len = buffer.samples();
        let out = buffer.as_slice();
        for ch in out.iter_mut() {
            ch.fill(0.);
        }
        if out.len() < 2 {
            return ProcessStatus::Normal;
        }
        let (l, r) = out.split_at_mut(1);
        let mut at = 0;
        while let Some(ev) = ctx.next_event() {
            let t = (ev.timing() as usize).min(len).max(at);
            if t > at {
                self.engine.render(&mut l[0][at..t], &mut r[0][at..t]);
                at = t;
            }
            match ev {
                NoteEvent::NoteOn {
                    note,
                    velocity,
                    channel,
                    voice_id,
                    ..
                } => self.engine.on(note, velocity, channel, voice_id),
                NoteEvent::NoteOff {
                    note,
                    channel,
                    voice_id,
                    ..
                } => self.engine.off(note, channel, voice_id),
                NoteEvent::Choke {
                    note,
                    channel,
                    voice_id,
                    ..
                } => self.engine.choke(note, channel, voice_id),
                NoteEvent::PolyTuning {
                    note,
                    channel,
                    voice_id,
                    tuning,
                    ..
                } => self.engine.tune(note, channel, voice_id, tuning),
                NoteEvent::MidiPitchBend { channel, value, .. } => self.engine.bend(channel, value),
                NoteEvent::MidiCC {
                    channel,
                    cc: 64,
                    value,
                    ..
                } => self.engine.sustain(channel, value >= 0.5),
                NoteEvent::MidiCC {
                    channel, cc: 120, ..
                } => self.engine.all_off(channel, true),
                NoteEvent::MidiCC {
                    channel, cc: 123, ..
                } => self.engine.all_off(channel, false),
                _ => {}
            }
        }
        if at < len {
            self.engine.render(&mut l[0][at..len], &mut r[0][at..len]);
        }
        self.params.telemetry.publish(self.engine.live);
        for (note, channel, voice_id) in self.engine.take_ended() {
            ctx.send_event(NoteEvent::VoiceTerminated {
                timing: (len as u32).saturating_sub(1),
                note,
                channel,
                voice_id,
            });
        }
        if self.engine.active() {
            ProcessStatus::KeepAlive
        } else {
            ProcessStatus::Normal
        }
    }
}
impl ClapPlugin for LemonWave {
    const CLAP_ID: &'static str = "org.lemonstudio.lemon-wave";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Polyphonic wavetable synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}
impl Vst3Plugin for LemonWave {
    const VST3_CLASS_ID: [u8; 16] = *b"LemonWaveSynth01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}
#[cfg(not(feature = "linked"))]
nih_export_clap!(LemonWave);
#[cfg(not(feature = "linked"))]
nih_export_vst3!(LemonWave);
os_plugkit::linked_clap_entry!(LemonWave);

#[cfg(test)]
mod tests {
    use super::*;
    use lemon_small_fx::params::FxParams;
    #[test]
    fn factory_presets_extend_old_state_and_preserve_parameter_defaults() {
        for index in 0..14 {
            let state = table_editor::TableEditor::factory_state(index);
            assert_eq!(state.params.len(), controls::COUNT);
            for setting in &controls::SETTINGS[if index < 5 {
                50
            } else if index < 8 {
                62
            } else if index < 11 {
                72
            } else {
                controls::COUNT
            }..]
            {
                assert!(
                    matches!(state.params.get(setting.id),Some(nih_plug::wrapper::state::ParamValue::I32(v)) if *v==setting.start)
                );
            }
            let raw = state.fields.get("state").unwrap().trim_matches('"');
            assert_eq!(raw.len(), (4 + controls::COUNT * 4) * 2);
        }
    }
    #[test]
    fn parameter_identity_state_and_editor_layout() {
        let params = FxParams::new(controls::SPEC);
        let map = params.param_map();
        assert_eq!(map.len(), controls::COUNT);
        let mut ids = std::collections::HashSet::new();
        for (id, _, _) in &map {
            assert!(ids.insert(id));
        }
        let values: Vec<i32> = controls::SETTINGS.iter().map(|s| s.range.1).collect();
        let bytes = lemon_small_fx::params::to_fl(controls::SPEC, &values, &params.raw_copy());
        let restored = FxParams::new(controls::SPEC);
        restored.set_raw(bytes.clone());
        let fields = restored.serialize_fields();
        params.deserialize_fields(&fields);
        assert_eq!(params.raw_copy(), bytes);
        for (i, v) in values.iter().enumerate() {
            assert_eq!(
                lemon_small_fx::params::setting(controls::SPEC, &bytes, i),
                *v
            );
        }
        struct Model;
        impl lemon_small_fx::view::Model for Model {
            fn plain(&self, id: &str) -> f32 {
                controls::SETTINGS[controls::SPEC.index(id).unwrap()].start as f32
            }
            fn norm(&self, id: &str) -> f32 {
                let s = &controls::SETTINGS[controls::SPEC.index(id).unwrap()];
                (s.start - s.range.0) as f32 / (s.range.1 - s.range.0) as f32
            }
            fn text(&self, id: &str) -> String {
                let i = controls::SPEC.index(id).unwrap();
                controls::text(i, controls::SETTINGS[i].start)
            }
        }
        for scale in [1., 1.5] {
            let mut c = os_ui::Canvas::new(1180, 820, scale).unwrap();
            let params = assets::WaveParams::default();
            let mut hits = instrument_view::paint(
                &mut c,
                &Model,
                &Default::default(),
                "",
                &params,
                [5; 2],
                None,
            );
            params.env_tab.store(1, Ordering::Relaxed);
            hits.extend(instrument_view::paint(&mut c,&Model,&Default::default(),"",&params,[5;2],None));
            params.page.store(1, Ordering::Relaxed);
            hits.extend(instrument_view::paint(
                &mut c,
                &Model,
                &Default::default(),
                "",
                &params,
                [5; 2],
                None,
            ));
            // Bank/source selection is now handled by the unified asset menu,
            // rather than generic parameter-selector hit targets.
            for (k,setting) in controls::SETTINGS.into_iter().enumerate() {
                if (38..44).contains(&k) || k>=109 {continue;} // Dynamic rack controls are checked in rack interaction tests.
                if ["a_bank", "b_bank", "a_source", "b_source"].contains(&setting.id) {
                    assert!(!hits.iter().any(|h| h.t.id() == setting.id));
                    continue;
                }
                assert!(
                    hits.iter().any(|h| h.t.id() == setting.id),
                    "{}",
                    setting.id
                );
            }
            for h in &hits {
                assert!(h.r.x >= 0. && h.r.y >= 0. && h.r.right() <= 1180. && h.r.bottom() <= 820.);
            }
            if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
                std::fs::create_dir_all(&dir).unwrap();
                c.pixmap
                    .save_png(format!("{dir}/lemon-wave-{scale}.png"))
                    .unwrap();
            }
        }
    }
}
