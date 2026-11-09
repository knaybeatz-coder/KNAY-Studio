// SPDX-License-Identifier: GPL-3.0-or-later
//! A host can accept a waveform release over its Browser. Snapshot parameters
//! at release; decoding, processing and writing run off the audio/UI threads.
use super::{SamplerParams, Shared, block, dsp, fx};
use os_engine::Sample;
use std::{
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub struct Snapshot {
    pub editor_parent: usize,
    pub path: PathBuf,
    raw: Option<Arc<Sample>>,
    source: fx::Source,
    key: super::params::FxKey,
    voice: dsp::Block,
    rate: u32,
}

type Receiver = fn(Snapshot);
static HOST: OnceLock<(fn() -> bool, Receiver)> = OnceLock::new();
pub fn connect(accepts: fn() -> bool, receive: Receiver) {
    let _ = HOST.set((accepts, receive));
}

pub(crate) fn released(shared: &Shared, params: &SamplerParams) {
    let Some((accepts, receive)) = HOST.get() else {
        return;
    };
    if !accepts() {
        return;
    }
    let path = shared.loaded_path();
    if path.is_empty() {
        return;
    }
    let rate = shared.rate().max(1);
    let tempo = shared.tempo();
    receive(Snapshot {
        editor_parent: shared
            .editor_parent
            .load(std::sync::atomic::Ordering::Relaxed),
        path: path.into(),
        raw: shared.raw.lock().unwrap().clone(),
        source: *shared.source.lock().unwrap(),
        key: params.fx_key(tempo, rate),
        voice: block(params, tempo, 0.0),
        rate,
    });
}

impl Snapshot {
    pub fn add_channel_pitch(&mut self, cents: f64) {
        if !self.voice.realtime {
            self.voice.cents += cents;
        }
    }
    pub fn write(mut self, out: &mut (impl Write + Seek)) -> Result<(), String> {
        let raw = match self.raw.take() {
            Some(raw) => raw,
            None => os_engine::sample::load_shared(&self.path).map_err(|e| e.to_string())?,
        };
        let built = fx::build(&raw, &self.source, &self.key);
        let mut engine = dsp::Engine::new(self.rate as f32);
        engine.file_frames = raw.frames() as f64 * built.per_file_frame;
        engine.sample = Some(Arc::new(built.sample));
        // A reusable file contains one pass of the edited sample.
        self.voice.loop_on = false;
        engine.note_on(60, 1.0, &self.voice);
        if !engine.has_voices() {
            return Err("The sampler has no playable audio at C5".into());
        }
        let mut frames = 0u64;
        os_engine::wav::write_f32_stereo_header(out, self.rate, 0).map_err(|e| e.to_string())?;
        while engine.has_voices() {
            let (mut l, mut r) = ([0f32; 512], [0f32; 512]);
            let n = engine.render_frames(&mut l, &mut r, &self.voice);
            frames += n as u64;
            if frames > (u32::MAX as u64 - 64) / 8 {
                return Err("Edited sample exceeds the WAV size limit".into());
            }
            let mut bytes = [0u8; 4096];
            for i in 0..n {
                bytes[i * 8..i * 8 + 4].copy_from_slice(&l[i].to_le_bytes());
                bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&r[i].to_le_bytes());
            }
            out.write_all(&bytes[..n * 8]).map_err(|e| e.to_string())?;
        }
        out.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        os_engine::wav::write_f32_stereo_header(out, self.rate, frames)
            .map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_wav_bakes_trim_reverse_and_pitch() {
        let side: Vec<_> = (0..4096)
            .map(|i| if i < 2048 { 0.2 } else { -0.4 })
            .collect();
        let raw = Arc::new(Sample {
            rate: 44100,
            left: side.clone(),
            right: side,
        });
        let render = |reverse: bool, cents: f64| {
            let mut key = super::super::params::FxKey::neutral();
            key.fx.start_bits = 0.25f64.to_bits();
            key.fx.length_bits = (2.0f64 / 3.0).to_bits();
            if reverse {
                key.fx.flags |= os_model::SampleFx::REVERSE;
            }
            let mut voice = dsp::Block::default();
            voice.cents = cents;
            let mut wav = std::io::Cursor::new(Vec::new());
            Snapshot {
                editor_parent: 0,
                path: "unused.wav".into(),
                raw: Some(raw.clone()),
                source: Default::default(),
                key,
                voice,
                rate: 44100,
            }
            .write(&mut wav)
            .unwrap();
            let bytes = wav.into_inner();
            assert_eq!(
                u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
                bytes.len() - 44
            );
            bytes[44..]
                .chunks_exact(8)
                .map(|frame| f32::from_le_bytes(frame[..4].try_into().unwrap()))
                .collect::<Vec<_>>()
        };
        let forward = render(false, 0.);
        let reverse = render(true, 0.);
        let octave = render(false, 1200.);
        // The one-shot preserves the sampler voice's declick tail. The polarity
        // transition lies halfway through the 2048 selected source frames.
        let crossing = |samples: &[f32]| samples.iter().position(|v| *v < -0.05).unwrap();
        assert!((crossing(&forward) as isize - 1024).abs() < 12);
        assert!(forward[128] > 0. && reverse[128] < 0.);
        assert!((crossing(&octave) as isize * 2 - crossing(&forward) as isize).abs() < 12);
        assert!(octave.len() < forward.len());
    }
}
