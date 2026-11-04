// SPDX-License-Identifier: GPL-3.0-or-later
//! Wavetable editing operations and user banks; all work stays off the audio thread.
use crate::wavetable::{MAX_FRAMES, SIZE, Source, fft};
use std::path::{Path, PathBuf};
pub fn normalize(frame: &mut [f32]) {
    let mean = frame.iter().sum::<f32>() / frame.len() as f32;
    for x in frame.iter_mut() {
        *x -= mean;
    }
    let peak = frame.iter().fold(0.0_f32, |a, x| a.max(x.abs()));
    if peak > 1e-8 {
        for x in frame {
            *x /= peak;
        }
    }
}
pub fn draw(frame: &mut [f32], from: (f32, f32), to: (f32, f32)) {
    let index = |p: f32| (p.clamp(0., 1.) * (SIZE - 1) as f32).round() as usize;
    let (a, b) = (index(from.0), index(to.0));
    for i in a.min(b)..=a.max(b) {
        let t = if a == b {
            1.
        } else {
            (i as f32 - a as f32) / (b as f32 - a as f32)
        };
        frame[i] = (from.1 + (to.1 - from.1) * t).clamp(-1., 1.);
    }
}
pub fn spectrum(frame: &[f32]) -> (Vec<f64>, Vec<f64>) {
    let mut re: Vec<_> = frame.iter().map(|x| *x as f64).collect();
    let mut im = vec![0.; SIZE];
    fft(&mut re, &mut im, false);
    (re, im)
}
pub fn amplitudes(frame: &[f32]) -> Vec<f32> {
    let (r, i) = spectrum(frame);
    (1..=64)
        .map(|h| (2. * r[h].hypot(i[h]) / SIZE as f64) as f32)
        .collect()
}
pub fn harmonic(frame: &mut [f32], h: usize, amplitude: f32) {
    let h = h.clamp(1, 64);
    let (mut r, mut i) = spectrum(frame);
    let phase = if r[h].hypot(i[h]) < 1e-8 {
        -std::f64::consts::FRAC_PI_2
    } else {
        i[h].atan2(r[h])
    };
    let a = amplitude.clamp(0., 1.) as f64 * SIZE as f64 / 2.;
    r[h] = a * phase.cos();
    i[h] = a * phase.sin();
    r[SIZE - h] = r[h];
    i[SIZE - h] = -i[h];
    fft(&mut r, &mut i, true);
    let peak = r.iter().fold(SIZE as f64, |a, x| a.max(x.abs()));
    for (n, x) in frame.iter_mut().enumerate() {
        *x = (r[n] / peak) as f32;
    }
}
pub fn smooth(frame: &mut [f32]) {
    let old = frame.to_vec();
    for i in 0..SIZE {
        frame[i] = (old[(i + SIZE - 1) % SIZE] + 2. * old[i] + old[(i + 1) % SIZE]) * 0.25;
    }
}
pub fn extract(
    sample: &crate::sample::Sample,
    selection: (f32, f32),
    count: usize,
) -> Result<Source, String> {
    sample.validate()?;
    let start = (selection.0.min(selection.1).clamp(0., 1.) * sample.samples.len() as f32) as usize;
    let end = (selection.0.max(selection.1).clamp(0., 1.) * sample.samples.len() as f32) as usize;
    let hz = 440. * 2.0_f64.powf((sample.root as f64 - 69.) / 12.);
    let period = sample.rate as f64 / hz;
    if period < 4. || (end.saturating_sub(start) as f64) < period * 1.5 {
        return Err(
            "Select at least two periods of the recording; check its Root pitch on Synth.".into(),
        );
    }
    let count = count.clamp(1, MAX_FRAMES);
    let mut samples = Vec::with_capacity(count * SIZE);
    for f in 0..count {
        let desired = start as f64 + (end - start) as f64 * f as f64 / count as f64;
        let lo = (desired - period * 0.4).max(start as f64) as usize;
        let hi =
            (desired + period * 0.4).min((end as f64 - period - 1.).max(start as f64)) as usize;
        let crossing = (lo..hi)
            .filter(|i| sample.samples[*i] <= 0. && sample.samples[*i + 1] > 0.)
            .min_by_key(|i| ((*i as f64 - desired).abs() * 100.) as usize);
        let begin = crossing
            .map(|i| i as f64)
            .unwrap_or(desired.min(end as f64 - period - 1.));
        let mut frame: Vec<_> = (0..SIZE)
            .map(|i| {
                let p = begin + period * i as f64 / SIZE as f64;
                let k = p.max(0.) as usize;
                let t = p.fract() as f32;
                let a = sample.samples[k.min(sample.samples.len() - 1)];
                let b = sample.samples[(k + 1).min(sample.samples.len() - 1)];
                a + (b - a) * t
            })
            .collect();
        let mean = frame.iter().sum::<f32>() / SIZE as f32;
        for x in &mut frame {
            *x -= mean;
        }
        samples.extend(frame);
    }
    let peak = samples.iter().fold(1.0_f32, |a, x| a.max(x.abs()));
    for x in &mut samples {
        *x /= peak;
    }
    let name: String = format!("{} - table", sample.name)
        .chars()
        .take(100)
        .collect();
    let source = Source { name, samples };
    source.validate()?;
    Ok(source)
}
pub fn root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Lemon Studio/Lemon Wave/User Wavetables")
}
pub fn suggested_name(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .take(64)
        .collect();
    let s = s.trim().trim_end_matches('.').trim();
    if safe_name(s).is_ok() {
        s.to_string()
    } else {
        "New Table".into()
    }
}
fn safe_name(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty()
        || s.chars().count() > 64
        || s.ends_with('.')
        || s.chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || s == "."
        || s == ".."
    {
        return Err("Use a name of 1-64 characters without file-path symbols.".into());
    }
    let upper = s.split('.').next().unwrap().to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || ["COM", "LPT"].iter().any(|p| {
            upper
                .strip_prefix(p)
                .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        })
    {
        return Err("Choose another name.".into());
    }
    Ok(s.to_string())
}
pub fn save_to(root: &Path, bank: &str, name: &str, source: &Source) -> Result<PathBuf, String> {
    use std::io::Write;
    source.validate()?;
    let bank = safe_name(bank)?;
    let name = safe_name(name)?;
    let dir = root.join(bank);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut source = source.clone();
    source.name = name.clone();
    let bytes = serde_json::to_vec(&source).map_err(|e| e.to_string())?;
    for n in 1..10000 {
        let file = dir.join(if n == 1 {
            format!("{name}.lwtable")
        } else {
            format!("{name} ({n}).lwtable")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(&bytes).and_then(|_| f.sync_all()) {
                    drop(f);
                    let _ = std::fs::remove_file(&file);
                    return Err(e.to_string());
                }
                return Ok(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many tables with this name.".into())
}
pub fn load(path: &Path) -> Result<Source, String> {
    use std::io::Read;
    let mut b = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut b)
        .map_err(|e| e.to_string())?;
    if b.len() > 8 * 1024 * 1024 {
        return Err("Table file exceeds 8 MB".into());
    }
    let source: Source = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
    source.validate()?;
    Ok(source)
}
pub fn list() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(banks) = std::fs::read_dir(root()) {
        for bank in banks.flatten().take(256) {
            if bank.path().is_dir() {
                if let Ok(files) = std::fs::read_dir(bank.path()) {
                    for file in files.flatten().take(512) {
                        let p = file.path();
                        if p.extension().is_some_and(|s| s == "lwtable") {
                            out.push((
                                format!(
                                    "{} / {}",
                                    bank.file_name().to_string_lossy(),
                                    p.file_stem().unwrap_or_default().to_string_lossy()
                                ),
                                p,
                            ));
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drawing_harmonics_and_sample_cycles() {
        let mut frame = vec![0.; SIZE];
        draw(&mut frame, (0., -1.), (1., 1.));
        assert_eq!(frame[0], -1.);
        assert_eq!(frame[SIZE - 1], 1.);
        harmonic(&mut frame, 1, 0.);
        let a = amplitudes(&frame);
        assert!(a[0] < 0.0001);
        let mut sine = vec![0.; SIZE];
        harmonic(&mut sine, 3, 0.7);
        let a = amplitudes(&sine);
        assert!((a[2] - 0.7).abs() < 0.0001);
        assert!(a[0] < 0.0001);
        smooth(&mut frame);
        normalize(&mut frame);
        assert!(frame.iter().all(|x| x.is_finite() && x.abs() <= 1.));
        let sample = crate::sample::Sample {
            name: "Test A4".into(),
            rate: 48000,
            root: 69.,
            samples: (0..48000)
                .map(|i| 0.6 * (std::f32::consts::TAU * 440. * i as f32 / 48000.).sin())
                .collect(),
        };
        let source = extract(&sample, (0.2, 0.6), 16).unwrap();
        assert_eq!(source.frames(), 16);
        for f in source.samples.chunks_exact(SIZE) {
            let a = amplitudes(f);
            assert!((a[0] - 0.6).abs() < 0.005);
            assert!(a[1..].iter().all(|x| *x < 0.005));
        }
        assert!(extract(&sample, (0., 0.00001), 16).is_err());
    }
    #[test]
    fn bank_round_trip_duplicates_and_path_validation() {
        let root = std::env::temp_dir().join(format!(
            "wave-bank-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let s = crate::sound_browser::basic(2);
        let a = save_to(&root, "Shapes", "Square", &s).unwrap();
        let b = save_to(&root, "Shapes", "Square", &s).unwrap();
        assert_ne!(a, b);
        assert_eq!(load(&a).unwrap().samples, s.samples);
        assert_eq!(load(&b).unwrap().samples, s.samples);
        for bank in ["..", "../escape", "CON", "bad/name"] {
            assert!(save_to(&root, bank, "Sound", &s).is_err());
        }
        std::fs::remove_file(a).unwrap();
        std::fs::remove_file(b).unwrap();
        std::fs::remove_dir(root.join("Shapes")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
