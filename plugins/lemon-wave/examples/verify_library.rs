// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline local-library validation, with no audio device or source asset copying.
use lemon_wave::{
    assets::{Sources, WaveParams},
    dsp::Engine,
    sample::Sample,
};
use nih_plug::prelude::Params;
#[derive(serde::Deserialize)]
struct Entry {
    path: std::path::PathBuf,
    root: f32,
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let entries: Vec<Entry> = serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let mut count = 0;
    for entry in &entries {
        let sample = Sample::import(&entry.path, entry.root).unwrap();
        let p = WaveParams::default();
        p.replace(Sources {
            samples: [Some(sample), None],
            ..Default::default()
        })
        .unwrap();
        let mut e = Engine::new(48000.);
        e.custom = p.tables.read().unwrap().clone();
        e.on(60, 0.8, 0, None);
        let mut l = vec![0.; 16384];
        let mut r = l.clone();
        e.render(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
        if l.iter().any(|x| x.abs() > 0.00001) {
            count += 1;
        }
        if count % 40 == 1 {
            let state = p.serialize_fields();
            let q = WaveParams::default();
            q.deserialize_fields(&state);
            assert_eq!(
                p.sources().samples[0].as_ref().unwrap().samples,
                q.sources().samples[0].as_ref().unwrap().samples
            );
        }
    }
    assert_eq!(count, entries.len());
    println!(
        "{} local recordings decoded, rendered finite/non-silent, sampled embedded-state checks passed",
        count
    );
}
