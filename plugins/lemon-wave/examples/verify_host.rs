// SPDX-License-Identifier: GPL-3.0-or-later
//! Headless verification through Lemon Studio's actual CLAP/VST3 hosts.
use os_host::Library;
use std::{path::Path, sync::Arc};
fn verify(lib: Arc<dyn Library>) {
    let desc = lib
        .plugins()
        .iter()
        .find(|d| d.name == "Lemon Wave")
        .expect("instrument descriptor")
        .clone();
    assert!(desc.is_instrument());
    let shared = lib.create(&desc.id).expect("create");
    let mut p = shared.lock().unwrap();
    assert!(p.has_editor());
    assert!(p.activate(48000));
    let infos: Vec<_> = (0..p.param_count())
        .filter_map(|i| p.param_info(i))
        .collect();
    for spec in lemon_wave::controls::SETTINGS {
        assert!(infos.iter().any(|i| i.name == spec.name), "{}", spec.name);
    }
    let gain = infos.iter().find(|i| i.name == "Output").unwrap().id;
    let mut flush_l = [0.; 128];
    let mut flush_r = flush_l;
    assert!(p.set_param(gain, 0.37));
    p.process(48000, &mut flush_l, &mut flush_r);
    let state = p.save_state().expect("save state");
    assert!(p.set_param(gain, 0.9));
    p.process(48000, &mut flush_l, &mut flush_r);
    assert!(p.load_state(&state));
    p.idle();
    assert!(
        (p.param_value(gain).unwrap() - 0.37).abs() < 0.002,
        "restored {}",
        p.param_value(gain).unwrap()
    );
    p.reset();
    p.note_on(37, 69, 0.8);
    let mut l = [0.; 512];
    let mut r = l;
    p.process(48000, &mut l, &mut r);
    assert!(l[..37].iter().all(|x| x.abs() < 1e-9), "early MIDI event");
    assert!(l[100..].iter().any(|x| x.abs() > 0.001), "no sound");
    let release = infos.iter().find(|i| i.name == "Release").unwrap().id;
    assert!(p.set_param(release, 0.));
    p.note_off(0, 69);
    for _ in 0..32 {
        p.process(48000, &mut l, &mut r);
    }
    assert!(l.iter().chain(&r).all(|x| x.abs() < 1e-6));
    p.reset();
    p.process(48000, &mut l, &mut r);
    assert!(l.iter().chain(&r).all(|x| *x == 0.));
    // Reference path and new settings must survive native host state recall.
    let find = |name: &str| infos.iter().find(|i| i.name == name).unwrap().id;
    let source = find("A Source");
    let mode = find("A Warp mode");
    let amount = find("A Warp amount");
    assert!(p.set_param(source, 1.));
    assert!(p.set_param(mode, 0.9));
    assert!(p.set_param(amount, 0.65));
    for (name, value) in [
        ("Envelope Engine", 1.),
        ("Amp Hold", 0.3),
        ("Amp Attack Curve", 0.7),
        ("Amp Decay Curve", 0.65),
        ("Amp Release Curve", 0.7),
        ("Env 2 Hold", 0.2),
        ("Env 2 Decay Curve", 0.8),
        ("Filter Model", 1.),
        ("Filter", 1. / 3.),
        ("LFO 1 Shape Engine", 1.),
        ("LFO 2 Sync", 6. / 8.),
        ("LFO 2 Division", 1.),
        ("LFO 3 Trigger", 1.),
        ("LFO 4 Trigger", 0.5),
        ("Mod 3 Source", 5. / 12.),
        ("Mod 3 Target", 6. / 20.),
        ("Mod 3 Amount", 0.8),
        ("Mod 4 Source", 8. / 12.),
        ("Mod 4 Target", 1. / 20.),
        ("Mod 4 Amount", 0.65),
        ("Macro 1", 0.6),
        ("A Phase", 0.25),
        ("A Blend", 0.5),
        ("Mod 1 Source", 0.75),
        ("Mod 1 Target", 2. / 7.),
        ("Mod 1 Amount", 0.8),
    ] {
        assert!(p.set_param(find(name), value));
    }
    p.process(48000, &mut l, &mut r);
    let reference_state = p.save_state().unwrap();
    p.reset();
    p.note_on(0, 60, 0.8);
    let (mut reference_l, mut reference_r) = (vec![0.; 8192], vec![0.; 8192]);
    p.process(48000, &mut reference_l, &mut reference_r);
    assert!(
        reference_l.iter().all(|x| x.is_finite()) && reference_l.iter().any(|x| x.abs() > 0.001)
    );
    assert!(p.set_param(source, 0.));
    p.process(48000, &mut l, &mut r);
    assert!(p.load_state(&reference_state));
    p.idle();
    p.reset();
    p.note_on(0, 60, 0.8);
    let (mut recalled_l, mut recalled_r) = (vec![0.; 8192], vec![0.; 8192]);
    p.process(48000, &mut recalled_l, &mut recalled_r);
    assert_eq!(reference_l, recalled_l);
    assert_eq!(reference_r, recalled_r);
    p.note_off(0, 60);
    for _ in 0..32 {
        p.process(48000, &mut l, &mut r);
    }
    assert!(l.iter().chain(&r).all(|x| x.abs() < 1e-6));
    assert!(p.set_param(source, 0.));
    p.process(48000, &mut l, &mut r);
    p.reset();
    // Exercise embedded table data through both real host state containers.
    let baseline = p.save_state().unwrap();
    let mut json = serde_json::json!({"version":"0.0.1","params":{},"fields":{}});
    let source = lemon_wave::wavetable::Source {
        name: "Host recall test".into(),
        samples: (0..1024)
            .map(|i| (std::f32::consts::TAU * i as f32 / 1024.).sin() * 0.7)
            .collect(),
    };
    let mut sources = lemon_wave::assets::Sources {
        samples: [
            None,
            Some(lemon_wave::sample::Sample {
                name: "Synthetic recorded note".into(),
                rate: 48000,
                root: 60.,
                samples: (0..48000)
                    .map(|i| (std::f32::consts::TAU * 261.6256 * i as f32 / 48000.).sin() * 0.4)
                    .collect(),
            }),
        ],
        oscillators: [Some(source), None],
        noise: None,
    };
    if !lemon_wave::included_library::PACK.is_empty() {
        let sample = lemon_wave::included_library::load(0).unwrap();
        let mut edited = lemon_wave::table_tools::extract(&sample, (0.1, 0.5), 16).unwrap();
        lemon_wave::table_tools::harmonic(&mut edited.samples[..1024], 3, 0.2);
        sources.oscillators[0] = Some(edited);
        sources.samples[1] = Some(sample);
    }
    json["fields"][lemon_wave::assets::FIELD] =
        serde_json::Value::String(serde_json::to_string(&sources).unwrap());
    let mut curves = lemon_wave::modulation::Curves::default();
    curves.draw(1, (0., 1.), (1., -1.));
    json["fields"][lemon_wave::modulation::FIELD] = serde_json::Value::String(curves.encode());
    for (id, value) in [
        ("b_level", 500),
        ("mod3_source", 5),
        ("mod3_target", 7),
        ("mod3_amount", 900),
        ("lfo2_sync", 5),
    ] {
        json["params"][id] = serde_json::json!({"i32":value});
    }
    let mut imported = serde_json::to_vec(&json).unwrap();
    if desc.id == "org.lemonstudio.lemon-wave" {
        let mut clap = (imported.len() as u64).to_le_bytes().to_vec();
        clap.extend(imported);
        imported = clap;
    }
    // VST3 accepts a bare component chunk too.
    assert!(p.load_state(&imported));
    p.idle();
    p.reset();
    p.note_on(0, 60, 0.8);
    let mut a = vec![0.; 4096];
    let mut b = a.clone();
    p.process(48000, &mut a, &mut b);
    assert!(a.iter().all(|x| x.is_finite()) && a.iter().any(|x| x.abs() > 0.001));
    let saved = p.save_state().unwrap();
    assert!(p.load_state(&baseline));
    p.idle();
    p.reset();
    p.note_on(0, 60, 0.8);
    let mut builtin = vec![0.; 4096];
    p.process(48000, &mut builtin, &mut b);
    assert!(a.iter().zip(&builtin).any(|(a, b)| (a - b).abs() > 0.001));
    assert!(p.load_state(&saved));
    p.idle();
    p.reset();
    p.note_on(0, 60, 0.8);
    let mut restored = vec![0.; 4096];
    p.process(48000, &mut restored, &mut b);
    assert!(
        a.iter().zip(&restored).all(|(a, b)| (a - b).abs() < 1e-6),
        "embedded audio recall differs"
    );
    // Stress the actual release plugin with eight notes, two seven-voice sources.
    for (name, value) in [
        ("A Source", 1.),
        ("B Source", 1.),
        ("A Unison", 1.),
        ("B Unison", 1.),
        ("B Level", 0.4),
    ] {
        assert!(p.set_param(find(name), value));
    }
    p.process(48000, &mut l, &mut r);
    p.reset();
    for note in [48, 52, 55, 59, 60, 64, 67, 71] {
        p.note_on(0, note, 0.6);
    }
    let start = std::time::Instant::now();
    for _ in 0..188 {
        p.process(48000, &mut l, &mut r);
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.));
    }
    println!(
        "{}: {:.3}s offline audio / 8 notes x 14 oscillators rendered in {:.3}s",
        desc.uid(),
        188. * 512. / 48000.,
        start.elapsed().as_secs_f32()
    );
    println!(
        "{}: descriptor, 109 controls / reference-source audio recall, state restore, MIDI offset, release/reset, embedded-table audio recall and built-in restore PASS",
        desc.uid()
    );
}
fn main() {
    let dir = std::env::args().nth(1).expect("bundle directory");
    verify(os_clap::Library::load(&Path::new(&dir).join("Lemon Wave.clap")).expect("CLAP load"));
    verify(os_vst3::Module::load(&Path::new(&dir).join("Lemon Wave.vst3")).expect("VST3 load"));
}
