// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline factory-preset audition and validation. Never opens an audio device.
use os_host::Library;
use std::{io::Write, path::Path};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let bundle = Path::new(&args[1]);
    let presets = Path::new(&args[2]);
    let output = Path::new(&args[3]);
    std::fs::create_dir_all(output).unwrap();
    let lib = os_clap::Library::load(&bundle.join("Lemon Wave.clap")).unwrap();
    let shared = lib.clone().create("org.lemonstudio.lemon-wave").unwrap();
    let mut p = shared.lock().unwrap();
    assert!(p.activate(48000));
    let infos: Vec<_> = (0..p.param_count())
        .filter_map(|i| p.param_info(i))
        .collect();
    let mut files: Vec<_> = std::fs::read_dir(presets)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "ospreset"))
        .collect();
    files.sort();
    for file in files {
        p.reset();
        let bytes = std::fs::read(&file).unwrap();
        let mut parts = bytes.splitn(3, |b| *b == b'\n');
        let magic = std::str::from_utf8(parts.next().unwrap()).unwrap();
        let header = std::str::from_utf8(parts.next().unwrap()).unwrap();
        let body = parts.next().unwrap();
        if header.contains("body=params") {
            let mut count = 0;
            for line in std::fs::read_to_string(&file)
                .unwrap()
                .lines()
                .skip(2)
                .filter(|s| !s.starts_with('#') && !s.is_empty())
            {
                let (name, value) = line.rsplit_once('=').unwrap();
                let v: f64 = value.trim().parse().unwrap();
                let i = infos
                    .iter()
                    .find(|i| i.name == name.trim())
                    .expect("preset parameter");
                assert!(
                    v >= i.min && v <= i.max,
                    "{}: {} outside {}..{}",
                    name,
                    v,
                    i.min,
                    i.max
                );
                assert!(p.set_param(i.id, (v - i.min) / (i.max - i.min)));
                count += 1;
            }
            assert!(count == 44 || count == lemon_wave::controls::COUNT);
        } else {
            assert!(p.load_state(body));
            p.idle();
        }
        let mut l = [0.; 256];
        let mut r = l;
        for _ in 0..4 {
            p.process(48000, &mut l, &mut r);
        }
        if args.iter().any(|a| a == "--embed-state") {
            let header = header.trim_end_matches('\r').replace("\tbody=params", "");
            let mut bytes = format!("{}\n{}\n", magic.trim_end_matches('\r'), header).into_bytes();
            bytes.extend(p.save_state().unwrap());
            std::fs::write(&file, bytes).unwrap();
        }
        p.reset();
        let name = file.file_stem().unwrap().to_string_lossy();
        let notes = if name.starts_with("Bass") {
            vec![36]
        } else if name.starts_with("Pad") {
            vec![48, 55, 60, 64]
        } else {
            vec![60, 67]
        };
        for &n in &notes {
            p.note_on(0, n, 0.75);
        }
        let mut audio = Vec::<i16>::with_capacity(48000 * 6 * 2);
        let mut peak = 0.0_f32;
        let start = std::time::Instant::now();
        for block in 0..1125 {
            if block == 375 {
                for &n in &notes {
                    p.note_off(0, n);
                }
            }
            p.process(48000, &mut l, &mut r);
            for (&a, &b) in l.iter().zip(&r) {
                assert!(a.is_finite() && b.is_finite());
                peak = peak.max(a.abs()).max(b.abs());
                audio.push((a.clamp(-1., 1.) * 32767.) as i16);
                audio.push((b.clamp(-1., 1.) * 32767.) as i16);
            }
        }
        assert!(peak > 0.005 && peak <= 1.);
        let size = (audio.len() * 2) as u32;
        let mut f = std::fs::File::create(output.join(format!("{name}.wav"))).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + size).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&48000u32.to_le_bytes()).unwrap();
        f.write_all(&192000u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&size.to_le_bytes()).unwrap();
        let bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        f.write_all(&bytes).unwrap();
        println!(
            "{name}: preset loaded, finite signal, peak {peak:.3}, 6s rendered in {:.2}s",
            start.elapsed().as_secs_f32()
        );
    }
}
