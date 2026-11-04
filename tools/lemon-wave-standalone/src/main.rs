// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use nih_plug::prelude::*;
use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    midi_input: Option<String>,
    output_device: Option<String>,
    sample_rate: Option<u32>,
    period_size: Option<u32>,
    dpi_scale: Option<f32>,
}
fn ports() -> Vec<String> {
    midir::MidiInput::new("Lemon Wave device list")
        .map(|m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default()
}
fn message(text: &str) {
    rfd::MessageDialog::new()
        .set_title("Lemon Wave")
        .set_description(text)
        .show();
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--build-info") {
        if let Some(path) = args.get(2) {
            std::fs::write(path,"Lemon Wave standalone; shared synth and Wavetable Workshop; MIDI keyboard input; Windows WASAPI output; no audio input; no on-screen keyboard\n").expect("write build info");
        }
        return;
    }
    if args.get(1).is_some_and(|a| a == "--self-test") {
        let mut engine = lemon_wave::dsp::Engine::new(48000.);
        engine.on(60, 0.7, 0, None);
        let mut left = [0f32; 4096];
        let mut right = [0f32; 4096];
        engine.render(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|v| v.is_finite()));
        assert!(left.iter().any(|v| v.abs() > 1e-5));
        engine.off(60, 0, None);
        engine.all_off(0, true);
        engine.reset();
        engine.render(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|v| v.abs() < 1e-6));
        let mut patch=lemon_wave::dsp::defaults();
        patch[lemon_wave::controls::RACK_MODE]=1.;
        for (slot,kind) in [2,4,5].into_iter().enumerate(){let b=lemon_wave::fxrack::base(slot);patch[b]=kind as f32;patch[b+6..b+12].copy_from_slice(&lemon_wave::fxrack::initial(kind));}
        engine.set(patch);engine.reset();for note in 48..56{engine.on(note,0.5,0,None);}
        let began=std::time::Instant::now();
        for _ in 0..12 {engine.render(&mut left,&mut right);assert!(left.iter().chain(right.iter()).all(|v|v.is_finite()));}
        let rack_ms=began.elapsed().as_millis();assert!(left.iter().any(|v|v.abs()>1e-5));engine.reset();engine.render(&mut left,&mut right);assert!(left.iter().chain(right.iter()).all(|v|v.abs()<1e-6));
        let included = lemon_wave::included_library::entries().len();
        if included > 0 {
            lemon_wave::included_library::load(0)
                .unwrap()
                .validate()
                .unwrap();
        }
        if let Some(path) = args.get(2) {
            std::fs::write(path,format!("PASS: offline synth note-on, finite non-silent stereo audio, reset clears notes and effect tails; {included} included recordings; first recording decoded. Modular compressor/delay/reverb chain: 8 synthetic notes, 1.024 seconds rendered in {rack_ms} ms; reset cleared the rack. No audio or MIDI device opened.\n")).expect("write self-test");
        }
        return;
    }
    if args.get(1).is_some_and(|a| a == "--list-audio") {
        use cpal::traits::{HostTrait,DeviceTrait};
        let host=cpal::default_host();
        let default=host.default_output_device().and_then(|d|d.name().ok());
        let outputs:Vec<_>=host.output_devices().map(|ds|ds.filter_map(|d|d.name().ok()).collect()).unwrap_or_default();
        if let Some(path)=args.get(2) {
            std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"default":default,"outputs":outputs})).unwrap()).expect("write output device list");
        }
        return;
    }
    if args.get(1).is_some_and(|a| a == "--list-midi") {
        if let Some(path) = args.get(2) {
            std::fs::write(path, ports().join("\n")).expect("write MIDI list");
        }
        return;
    }
    let folder = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .expect("app folder");
    let settings = folder.join("Settings.json");
    let settings: Settings = match std::fs::read(&settings) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(e) => {
                message(&format!("Please correct Settings.json beside the app: {e}"));
                return;
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            message(&format!("Cannot read Settings.json: {e}"));
            return;
        }
    };
    let mut args = args;
    if args.len() == 1 {
        let available = ports();
        let midi = settings.midi_input.or_else(|| available.first().cloned());
        let Some(midi) = midi else {
            message(
                "Connect and switch on your MIDI keyboard, then open Lemon Wave again. No MIDI input device was found.",
            );
            return;
        };
        if !available.contains(&midi) {
            message(&format!(
                "The MIDI input '{midi}' is unavailable. Connect it, or change midi_input in Settings.json. Available inputs:\n{}",
                available.join("\n")
            ));
            return;
        }
        args.extend([
            "--backend".into(),
            "wasapi".into(),
            "--midi-input".into(),
            midi,
        ]);
        if let Some(d) = settings.output_device {
            args.extend(["--output-device".into(), d]);
        }
        for (name, value) in [
            ("--sample-rate", settings.sample_rate.map(|v| v.to_string())),
            ("--period-size", settings.period_size.map(|v| v.to_string())),
            ("--dpi-scale", settings.dpi_scale.map(|v| v.to_string())),
        ] {
            if let Some(value) = value {
                args.extend([name.into(), value]);
            }
        }
    }
    if !nih_export_standalone_with_args::<lemon_wave::LemonWave, _>(args) {
        message(
            "Lemon Wave could not open the selected audio or MIDI device. Close any application using the keyboard or audio output exclusively, check the Windows default output, then try again. See Read Me.txt beside this app for device options.",
        );
        std::process::exit(1);
    }
}
