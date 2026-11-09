// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Lemon Parametric EQ: Lemon Studio's 7-band parametric EQ as a VST3 / CLAP plugin (nih-plug). The sound is the
//! original's in every mode (`os_engine::effects::eq::Equalizer`); the plugin reports LIN's and HQ Enhanced's delay.
//!
// spec: plugins/lemon-parametric-eq-2.md, plugins/fruity-parametric-eq-2.md, plugins/parametric-eq-2.md,
// plugins/plugin-formats.md
#![allow(semicolon_in_expressions_from_non_local_macros)]

pub mod analyzer;
mod gain_match;
pub mod editor;
pub mod params;
pub mod view;
pub mod visibility;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use os_engine::effects::eq::{Options, PARAMS, routed::{self,RoutedEq as Equalizer}};

use params::EqParams;

/// Samples kept for the analyzer.
pub const SCOPE_LEN: usize = 1 << 15;

/// What the audio thread hands the editor: the monitored signal for the analyzer.
pub struct Shared {
    pub scope: Mutex<Scope>,
    pub rate: AtomicU32,
    pub dynamics:[AtomicU32;8],
    pub spectral:[AtomicU32;256],
    pub trigger:[AtomicU32;7],
    pub threshold:[AtomicU32;7],
    pub solo:AtomicI32,
    pub gain_match:AtomicBool,
    pub repaint_pending:AtomicBool,
    pub editor_w:AtomicU32,
    pub editor_h:AtomicU32,
    /// Set while an editor is open (the audio thread feeds the analyzer only then).
    pub editor_open: AtomicBool,
}

pub struct Scope {
    pub buf: Vec<f32>,
    pub pos: usize,
    /// Samples written in total.
    pub written: u64,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            scope: Mutex::new(Scope { buf: vec![0.0; SCOPE_LEN], pos: 0, written: 0 }),
            rate: AtomicU32::new(44100),
            editor_w:AtomicU32::new(view::W),
            editor_h:AtomicU32::new(view::H),
            solo:AtomicI32::new(-1),
            gain_match:AtomicBool::new(false),
            repaint_pending:AtomicBool::new(false),
            spectral:std::array::from_fn(|_|AtomicU32::new(0f32.to_bits())),
            trigger:std::array::from_fn(|_|AtomicU32::new((-120f32).to_bits())),
            threshold:std::array::from_fn(|_|AtomicU32::new((-30f32).to_bits())),
            dynamics:std::array::from_fn(|_|AtomicU32::new(0f32.to_bits())),
            editor_open: AtomicBool::new(false),
        }
    }
}

impl Scope {
    /// The newest `n` samples, oldest first.
    pub fn latest(&self, n: usize) -> Vec<f32> {
        let n = n.min(self.buf.len());
        (0..n).map(|i| self.buf[(self.pos + self.buf.len() - n + i) % self.buf.len()]).collect()
    }
}

static EXPORTING: AtomicBool = AtomicBool::new(false);

/// Suppress monitoring during Lemon Studio exports, including normal-quality renders.
pub fn set_exporting(exporting: bool) {
    EXPORTING.store(exporting, Ordering::Relaxed);
}

pub struct LemonEq {
    params: Arc<EqParams>,
    shared: Arc<Shared>,
    sr: f64,
    offline: bool,
    /// The original's processing (every mode), and the parameters it was last given as stored values.
    eq: Equalizer,
    gain_match:gain_match::GainMatch,
    audition:os_engine::effects::eq::dynamic::Audition,
    applied: [i32; PARAMS],
    /// The options read from the state image, and the delay last reported to the host.
    opts: Options,
    latency: u32,
    /// Monitor (0 off, 1 input, 2 output) and input (1 Mid, 2 Side, 3 Left, 4 Right), read with the options.
    monitor: (u8, i32),
    extra_seen: u64,
    feed: Vec<f32>,
}

impl Default for LemonEq {
    fn default() -> Self {
        let params = Arc::new(EqParams::default());
        let opts = Options::from_image(&params::default_state());
        let applied = params.settings(opts.mask).params();
        LemonEq {
            params,
            shared: Arc::new(Shared::default()),
            sr: 44100.0,
            offline: false,
            gain_match:gain_match::GainMatch::default(),
            audition:os_engine::effects::eq::dynamic::Audition::new(44100.),
            eq: Equalizer::new(44100.0, &applied, opts),
            applied,
            opts,
            latency: 0,
            monitor: (2, 1),
            extra_seen: u64::MAX,
            feed: Vec::new(),
        }
    }
}

impl LemonEq {
    /// Options from the state image (never blocks: keeps the last values while the editor writes).
    fn read_extra(&mut self) {
        let v = self.params.extra_version.load(Ordering::Acquire);
        if v == self.extra_seen {
            return;
        }
        if let Ok(b) = self.params.extra.try_read() {
            self.opts = Options::from_image(&b);
            self.eq.set_routes(routed::routes(&b,false));
            self.monitor = (params::byte(&b, params::MONITOR_BYTE), params::opt(&b, params::Opt::MonitorInput));
            self.extra_seen = v;
        }
    }

    /// The parameters and options as they stand, handed to the processing: a changed value counts as an automation
    /// value (HQ Enhanced ramps continuous band coefficients; other modes retain their original timing).
    fn apply(&mut self) {
        self.read_extra();
        self.eq.set_options(self.opts);
        let mut dyns=os_engine::effects::eq::dynamic::Config::default();for i in 0..47{dyns.set(i,self.params.live.value(&self.params.dynamic[i]));}self.eq.set_dynamic(dyns);
        let p = self.params.settings(self.opts.mask).params();
        for (i, (&new, old)) in p.iter().zip(self.applied.iter_mut()).enumerate() {
            if new != *old {
                *old = new;
                self.eq.set_param(i, new);
            }
        }
    }

    fn process_eq(&mut self, left: &mut [f32], right: &mut [f32], solo: Option<usize>) {
        self.eq.process(left, right);
        // Isolate the monitoring range without changing the EQ's routing, enable mask or delay.
        if let Some(k) = solo {
            let band = self.params.settings(self.opts.mask).bands[k];
            self.audition.process(left, right, band);
        }
    }

    fn feed_scope(&mut self, l: &[f32], r: &[f32]) {
        self.feed.clear();
        let input = self.monitor.1;
        self.feed.extend(l.iter().zip(r).map(|(&l, &r)| match input {
            2 => (l - r) * 0.5,
            3 => l,
            4 => r,
            _ => (l + r) * 0.5,
        }));
        if let Ok(mut s) = self.shared.scope.try_lock() {
            let n = s.buf.len();
            for &x in &self.feed {
                let p = s.pos;
                s.buf[p] = x;
                s.pos = (p + 1) % n;
            }
            s.written += self.feed.len() as u64;
        }
    }
}

impl Plugin for LemonEq {
    const NAME: &'static str = "Lemon Parametric EQ";
    const VENDOR: &'static str = "Lemon Studio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(editor::EqEditor::new(self.params.clone(), self.shared.clone())))
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        self.sr = f64::from(config.sample_rate);
        self.offline = config.process_mode == ProcessMode::Offline;
        self.gain_match.reset();
        self.audition=os_engine::effects::eq::dynamic::Audition::new(self.sr);
        self.shared.rate.store(config.sample_rate as u32, Ordering::Relaxed);
        self.extra_seen = u64::MAX;
        self.read_extra();
        self.applied = self.params.settings(self.opts.mask).params();
        self.eq = Equalizer::new(self.sr, &self.applied, self.opts);
        self.eq.set_routes(routed::routes(&self.params.extra_copy(),false));
        // Restore dynamics before reporting latency. CLAP reinitializes after a live delay
        // change; reporting the static-only delay here causes an endless restart/reset loop.
        self.apply();
        self.latency = self.eq.latency() as u32;
        ctx.set_latency_samples(self.latency);
        self.feed = Vec::with_capacity(config.max_buffer_size as usize);
        true
    }

    /// A new instance's state, with FL's silent buffers before a render already run (LIN starts at full level).
    fn reset(&mut self) {
        self.gain_match.reset();
        self.eq.reset();
        self.eq.preroll();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let solo = self.shared.solo.load(Ordering::Relaxed);
        let solo = (!self.offline && !EXPORTING.load(Ordering::Relaxed) && (0..7).contains(&solo))
            .then_some(solo as usize);
        self.apply();
        // The delay is looked at every block and reported when it has changed (LIN on or off, its size, HQ Enhanced).
        let latency = self.eq.latency() as u32;
        if latency != self.latency {
            self.latency = latency;
            ctx.set_latency_samples(latency);
        }
        let len = buffer.samples();
        let out = buffer.as_slice();
        if out.len() < 2 {
            return ProcessStatus::Normal;
        }
        let watch = self.shared.editor_open.load(Ordering::Relaxed) && self.monitor.0 != 0;
        if watch && self.monitor.0 == 1 {
            self.feed_scope(&out[0][..len], &out[1][..len]);
        }
        let matching=self.shared.gain_match.load(Ordering::Relaxed) && self.shared.editor_open.load(Ordering::Relaxed);
        let input_energy=if matching {gain_match::GainMatch::energy(&out[0][..len],&out[1][..len])}else{0.};
        let (left, right) = out.split_at_mut(1);
        self.process_eq(&mut left[0][..len], &mut right[0][..len], solo);
        self.gain_match.process(input_energy,&mut left[0][..len],&mut right[0][..len],self.sr,matching && solo.is_none());
        if self.shared.editor_open.load(Ordering::Relaxed){
        for (meter,v) in self.shared.dynamics.iter().zip(self.eq.dynamic_meters()){meter.store(v.to_bits(),Ordering::Relaxed);}
        for (meter,v) in self.shared.spectral.iter().zip(self.eq.spectral_response()){meter.store(v.to_bits(),Ordering::Relaxed);}
        let (trigger,threshold)=self.eq.detector_meters();for i in 0..7{self.shared.trigger[i].store(trigger[i].to_bits(),Ordering::Relaxed);self.shared.threshold[i].store(threshold[i].to_bits(),Ordering::Relaxed);}}

        if watch && self.monitor.0 == 2 {
            self.feed_scope(&out[0][..len], &out[1][..len]);
        }
        ProcessStatus::Normal
    }
}

// CLAP id and VST3 class id keep the spelling of the plugin's earlier "Open" name: hosts find saved sessions'
// plugins by them (spec: plugins/plugin-formats.md, identity).
impl ClapPlugin for LemonEq {
    const CLAP_ID: &'static str = "org.openstudio.open-parametric-eq-2";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Lemon Studio's 7-band parametric EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Equalizer, ClapFeature::Stereo];
}

impl Vst3Plugin for LemonEq {
    const VST3_CLASS_ID: [u8; 16] = *b"OpenStudioParEQ2";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

// Built as a plugin module: the C entry points hosts look for. Linked into Lemon Studio (`linked` feature): only the
// symbol-free table, so several Lemon plugins fit in one program (spec: plugins/plugin-formats.md).
#[cfg(not(feature = "linked"))]
nih_export_clap!(LemonEq);
#[cfg(not(feature = "linked"))]
nih_export_vst3!(LemonEq);
os_plugkit::linked_clap_entry!(LemonEq);

#[cfg(test)]mod dynamic_tests {
 use super::*;
 #[test]fn live_automated_dynamic_parameters_reach_audio(){
  let mut plugin=LemonEq::default();let mut vals=os_plugkit::vals::Vals::new(&*plugin.params,plugin.params.live.clone(),None);
  vals.edit_plain(&params::freq_id(3),os_engine::effects::eq::freq_pos(1000.) as f32);
  vals.edit_plain(&params::dynamic_id(12),-12.);vals.edit_plain(&params::dynamic_id(13),-30.);
  plugin.apply();let input:Vec<_>=(0..44100).map(|i|0.5*(std::f64::consts::TAU*1000.*i as f64/44100.).sin() as f32).collect();let mut l=input.clone();let mut r=input.clone();plugin.eq.process(&mut l,&mut r);
  assert!(plugin.eq.dynamic_meters()[3]< -10.);let ratio=(l[22050..].iter().map(|v|v*v).sum::<f32>()/input[22050..].iter().map(|v|v*v).sum::<f32>()).sqrt();assert!(ratio<0.4);
 }
}

#[cfg(test)]mod spectral_integration_tests{
 use super::*;
 #[test]fn spectral_automation_recall_and_native_render_agree(){
  let mut plugin=LemonEq::default();let mut editor=editor::Logic::new(plugin.params.clone(),plugin.shared.clone(),None);
  editor.vals.edit_plain(&params::freq_id(3),os_engine::effects::eq::freq_pos(4100.)as f32);editor.vals.edit_plain(&params::dynamic_id(12),-9.);editor.vals.edit_plain(&params::dynamic_id(13),-50.);editor.vals.edit_plain(&params::dynamic_id(43),1.);editor.vals.edit_plain(&params::dynamic_id(37),0.7);
  let image=params::to_fl(&editor.shown(),&editor.shown_extra());let recalled=params::fl_from_plugin_state(&params::plugin_state_from_fl(&image)).unwrap();assert_eq!(recalled,image);
  plugin.apply();let mut native=routed::RoutedEq::from_state(&recalled,44100.).unwrap();assert_eq!(plugin.eq.latency(),native.latency());assert_eq!(plugin.eq.latency(),1024);
  let input:Vec<_>=(0..44100).map(|i|0.4*(std::f64::consts::TAU*4100.*i as f64/44100.).sin()as f32).collect();let mut l=input.clone();let mut r=input.clone();let mut a=input.clone();let mut b=input.clone();plugin.eq.process(&mut l,&mut r);native.process(&mut a,&mut b);assert_eq!(l,a);assert_eq!(r,b);assert!(plugin.eq.dynamic_meters()[3]< -3.);assert!(l[22050..].iter().map(|v|v*v).sum::<f32>()<input[22050..].iter().map(|v|v*v).sum::<f32>()*0.7);
 }
}


#[cfg(test)]
mod audition_tests {
    use super::*;

    #[test]
    fn band_card_audition_isolates_frequency_without_changing_the_eq_state() {
        let mut eq = LemonEq::default();
        let k = 3;
        let center = eq.params.settings(eq.opts.mask).bands[k].hz();
        let distant = (center * 12.).min(16_000.);
        let rate = eq.sr;
        let tone = |hz: f64, i: usize| (std::f64::consts::TAU * hz * i as f64 / rate).sin();
        let input: Vec<_> = (0..8192).map(|i| ((tone(center, i) + tone(distant, i)) * 0.25) as f32).collect();
        let (mut left, mut right) = (input.clone(), input.clone());
        let latency = eq.eq.latency();
        let saved = eq.params.extra_copy();
        eq.process_eq(&mut left, &mut right, Some(k));
        let at = |hz| (4096..8192).map(|i| f64::from(left[i]) * tone(hz, i)).sum::<f64>().abs();
        assert!(at(center) > 10. * at(distant));
        assert_eq!(eq.params.extra_copy(), saved);
        assert_eq!(eq.eq.latency(), latency);
        left.clone_from(&input);
        right.clone_from(&input);
        eq.process_eq(&mut left, &mut right, None);
        let mut normal = LemonEq::default();
        let (mut dry_l, mut dry_r) = (input.clone(), input.clone());
        normal.process_eq(&mut dry_l, &mut dry_r, None);
        dry_l.clone_from(&input);
        dry_r.clone_from(&input);
        normal.process_eq(&mut dry_l, &mut dry_r, None);
        assert!(left == dry_l && right == dry_r, "normal output changed after audition");
    }
}
