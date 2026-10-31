// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Musical key detection for Key Match: a pitch-class profile (chroma) of audio or of notes, matched against the
//! Krumhansl-Kessler key profiles. Pure std, no allocation on any audio thread (it runs on drops and in a worker).

/// Pitch-class names, C = 0.
pub const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Krumhansl-Kessler probe-tone profiles, tonic first.
const MAJOR: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

/// A key: tonic pitch class (C = 0) and mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub root: u8,
    pub minor: bool,
}

impl Key {
    pub const ALL: [Key; 24] = {
        let mut all = [Key { root: 0, minor: false }; 24];
        let mut i = 0;
        while i < 24 {
            all[i] = Key { root: (i % 12) as u8, minor: i >= 12 };
            i += 1;
        }
        all
    };

    /// "G major", "A minor".
    pub fn name(self) -> String {
        format!("{} {}", NAMES[usize::from(self.root % 12)], if self.minor { "minor" } else { "major" })
    }

    /// "G", "Am": the clip label's short form.
    pub fn short(self) -> String {
        format!("{}{}", NAMES[usize::from(self.root % 12)], if self.minor { "m" } else { "" })
    }

    /// The tonic of the major key with the same notes (A minor -> C).
    pub fn relative_major_root(self) -> u8 {
        if self.minor { (self.root + 3) % 12 } else { self.root % 12 }
    }

    /// This key moved by `semitones`.
    pub fn transposed(self, semitones: i32) -> Key {
        Key { root: (i32::from(self.root) + semitones).rem_euclid(12) as u8, minor: self.minor }
    }
}

/// The smallest move, in semitones (-6..=5), that puts `from` on the notes of `to`. Relative keys need none: A minor
/// on a C major beat stays, an A minor sample on a G major beat goes down 5 (to E minor, G major's relative minor).
pub fn shift_to(from: Key, to: Key) -> i32 {
    let d = (i32::from(to.relative_major_root()) - i32::from(from.relative_major_root())).rem_euclid(12);
    if d > 5 { d - 12 } else { d }
}

/// A detection: the key, how well the best profile matched (Pearson r, -1..1) and how far ahead of the runner-up
/// that is not the same scale (its relative key).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detected {
    pub key: Key,
    pub score: f64,
    pub margin: f64,
}

impl Detected {
    /// Clear enough to act on: drum loops, noise and one-shots without a pitch fall below this.
    pub fn confident(&self) -> bool {
        self.score >= 0.55 && self.margin >= 0.04
    }
}

fn pearson(a: &[f64; 12], b: &[f64; 12], rot: usize) -> f64 {
    let ma = a.iter().sum::<f64>() / 12.0;
    let mb = b.iter().sum::<f64>() / 12.0;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        let x = a[(i + rot) % 12] - ma;
        let y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 1e-12 || db <= 1e-12 { 0.0 } else { num / (da * db).sqrt() }
}

/// The key of a pitch-class profile (index 0 = C). None when the profile is empty or flat.
pub fn key_of(chroma: &[f64; 12]) -> Option<Detected> {
    let total: f64 = chroma.iter().sum();
    if !(total > 1e-9) {
        return None;
    }
    let mut scores: Vec<(Key, f64)> = Key::ALL
        .iter()
        .map(|&k| (k, pearson(chroma, if k.minor { &MINOR } else { &MAJOR }, usize::from(k.root))))
        .collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, score) = scores[0];
    let runner = scores.iter().skip(1).find(|(k, _)| k.relative_major_root() != best.relative_major_root()).map_or(-1.0, |s| s.1);
    Some(Detected { key: best, score, margin: score - runner })
}

/// Key `k`'s profile as a pitch-class profile (index 0 = C): what a sample set by hand to `k` adds to a beat.
pub fn profile(k: Key) -> [f64; 12] {
    let base = if k.minor { &MINOR } else { &MAJOR };
    std::array::from_fn(|p| base[(p as i32 - i32::from(k.root)).rem_euclid(12) as usize])
}

/// In-place radix-2 FFT of `re` / `im` (length a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Seconds of audio looked at (from the start); enough for a loop or a song's opening.
pub const MAX_SECS: f64 = 90.0;
const TARGET_RATE: f64 = 11025.0;
const FRAME: usize = 4096;
const HOP: usize = 2048;
/// Pitches counted: A1 (55 Hz) to about B6.
const LOW_HZ: f64 = 55.0;
const HIGH_HZ: f64 = 2000.0;

/// The pitch-class profile of stereo audio at `rate` Hz (index 0 = C). Each frame's profile is normalized, so loud
/// and quiet parts count alike; near-silent frames are skipped.
pub fn chroma(left: &[f32], right: &[f32], rate: u32) -> [f64; 12] {
    let mut out = [0.0; 12];
    if rate == 0 || left.is_empty() {
        return out;
    }
    // Mono, decimated by averaging (a crude low-pass, plenty for pitches under 2 kHz).
    let step = ((f64::from(rate) / TARGET_RATE).round() as usize).max(1);
    let sr = f64::from(rate) / step as f64;
    let frames = left.len().min((MAX_SECS * f64::from(rate)) as usize);
    let mono: Vec<f64> = (0..frames / step)
        .map(|i| {
            let a = i * step;
            let (mut sum, mut n) = (0.0, 0.0);
            for j in a..(a + step).min(frames) {
                sum += f64::from(left[j]) + f64::from(*right.get(j).unwrap_or(&left[j]));
                n += 2.0;
            }
            sum / n
        })
        .collect();
    if mono.len() < FRAME {
        return chroma_short(&mono, sr);
    }
    let window: Vec<f64> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / FRAME as f64).cos()).collect();
    let bins = bin_classes(sr, FRAME);
    let (mut re, mut im) = (vec![0.0; FRAME], vec![0.0; FRAME]);
    let mut start = 0;
    while start + FRAME <= mono.len() {
        let block = &mono[start..start + FRAME];
        let rms = (block.iter().map(|v| v * v).sum::<f64>() / FRAME as f64).sqrt();
        if rms > 1e-4 {
            for i in 0..FRAME {
                re[i] = block[i] * window[i];
                im[i] = 0.0;
            }
            fft(&mut re, &mut im);
            add_frame(&mut out, &re, &im, &bins);
        }
        start += HOP;
    }
    out
}

/// A clip shorter than one frame: zero-padded into a single frame.
fn chroma_short(mono: &[f64], sr: f64) -> [f64; 12] {
    let mut out = [0.0; 12];
    if mono.len() < 256 {
        return out;
    }
    let bins = bin_classes(sr, FRAME);
    let (mut re, mut im) = (vec![0.0; FRAME], vec![0.0; FRAME]);
    let n = mono.len();
    for i in 0..n {
        re[i] = mono[i] * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos());
    }
    fft(&mut re, &mut im);
    add_frame(&mut out, &re, &im, &bins);
    out
}

/// Each FFT bin's pitch class (None outside the counted range).
fn bin_classes(sr: f64, n: usize) -> Vec<Option<usize>> {
    (0..n / 2)
        .map(|b| {
            let f = b as f64 * sr / n as f64;
            (LOW_HZ..=HIGH_HZ).contains(&f).then(|| {
                let midi = 69.0 + 12.0 * (f / 440.0).log2();
                (midi.round() as i64).rem_euclid(12) as usize
            })
        })
        .collect()
}

fn add_frame(out: &mut [f64; 12], re: &[f64], im: &[f64], bins: &[Option<usize>]) {
    let mut frame = [0.0; 12];
    for (b, pc) in bins.iter().enumerate() {
        if let Some(pc) = pc {
            // Compressed magnitude: a loud bass note does not drown the rest.
            frame[*pc] += (re[b] * re[b] + im[b] * im[b]).sqrt().sqrt();
        }
    }
    let sum: f64 = frame.iter().sum();
    if sum > 1e-9 {
        for i in 0..12 {
            out[i] += frame[i] / sum;
        }
    }
}

/// The pitch-class profile of notes: (MIDI key, length in ticks, velocity 0..=128). Each counts by its length and
/// loudness.
pub fn note_chroma(notes: impl IntoIterator<Item = (i32, u32, u8)>) -> [f64; 12] {
    let mut out = [0.0; 12];
    for (key, len, vel) in notes {
        let w = f64::from(len.max(24)) * (f64::from(vel.max(1)) / 100.0);
        out[key.rem_euclid(12) as usize] += w;
    }
    out
}

/// `chroma` moved up by `semitones` (what a pitch shift does to it).
pub fn rotate(chroma: &[f64; 12], semitones: i32) -> [f64; 12] {
    let mut out = [0.0; 12];
    for (i, v) in chroma.iter().enumerate() {
        out[(i as i32 + semitones).rem_euclid(12) as usize] = *v;
    }
    out
}

/// `chroma` scaled to sum 1 (zeros stay zeros), so sources of different sizes count alike when added.
pub fn normalized(chroma: &[f64; 12]) -> [f64; 12] {
    let sum: f64 = chroma.iter().sum();
    if sum > 1e-12 { chroma.map(|v| v / sum) } else { [0.0; 12] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_mix(freqs: &[f64], secs: f64, rate: u32) -> Vec<f32> {
        let n = (secs * f64::from(rate)) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                (freqs.iter().map(|f| (2.0 * std::f64::consts::PI * f * t).sin()).sum::<f64>() / freqs.len() as f64 * 0.5) as f32
            })
            .collect()
    }

    fn hz(midi: i32) -> f64 {
        440.0 * 2f64.powf(f64::from(midi - 69) / 12.0)
    }

    /// I-IV-V-I chords with the scale's melody notes: a clear key.
    fn progression(root: i32, minor: bool, rate: u32) -> Vec<f32> {
        let third = if minor { 3 } else { 4 };
        let chords: [[i32; 3]; 4] = if minor {
            [[0, 3, 7], [5, 8, 12], [7, 11, 14], [0, 3, 7]]
        } else {
            [[0, third, 7], [5, 9, 12], [7, 11, 14], [0, third, 7]]
        };
        let mut out = Vec::new();
        for c in chords {
            let fs: Vec<f64> = c.iter().map(|d| hz(48 + root + d)).chain([hz(36 + root + c[0])]).collect();
            out.extend(tone_mix(&fs, 1.5, rate));
        }
        out
    }

    #[test]
    fn shift_examples() {
        let k = |r: u8, m: bool| Key { root: r, minor: m };
        // A minor sample, G major beat: down 5 to E minor.
        assert_eq!(shift_to(k(9, true), k(7, false)), -5);
        assert_eq!(k(9, true).transposed(-5), k(4, true));
        // Relative keys stay.
        assert_eq!(shift_to(k(9, true), k(0, false)), 0);
        assert_eq!(shift_to(k(0, false), k(9, true)), 0);
        // Same mode: root distance, the short way round.
        assert_eq!(shift_to(k(0, false), k(2, false)), 2);
        assert_eq!(shift_to(k(0, false), k(10, false)), -2);
        assert_eq!(shift_to(k(0, false), k(6, false)), -6);
        for a in Key::ALL {
            for b in Key::ALL {
                let s = shift_to(a, b);
                assert!((-6..=5).contains(&s));
                assert_eq!(a.transposed(s).relative_major_root(), b.relative_major_root());
            }
        }
    }

    #[test]
    fn detects_audio_keys() {
        let rate = 44100;
        for (root, minor) in [(7, false), (9, true), (2, false), (4, true), (0, false), (10, true)] {
            let a = progression(root, minor, rate);
            let d = key_of(&chroma(&a, &a, rate)).expect("a key");
            assert_eq!(d.key.relative_major_root(), Key { root: root as u8, minor }.relative_major_root(), "{root} {minor}: {:?}", d);
            assert!(d.score > 0.5, "{d:?}");
        }
    }

    #[test]
    fn detects_note_keys() {
        // G major scale notes, G and D heavier.
        let notes = [(67, 4), (69, 1), (71, 2), (72, 1), (74, 3), (76, 1), (78, 1), (55, 4), (62, 2)]
            .map(|(k, w)| (k, w * 96, 100));
        let d = key_of(&note_chroma(notes)).unwrap();
        assert_eq!(d.key, Key { root: 7, minor: false }, "{d:?}");
        assert!(d.confident());
    }

    #[test]
    fn silence_and_noise_are_not_confident() {
        assert!(key_of(&chroma(&[0.0; 50000], &[0.0; 50000], 44100)).is_none());
        let mut seed = 1u64;
        let noise: Vec<f32> = (0..200_000)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((seed >> 33) as f32 / (1u64 << 31) as f32) - 0.5
            })
            .collect();
        assert!(!key_of(&chroma(&noise, &noise, 44100)).is_some_and(|d| d.confident()));
    }

    #[test]
    fn profiles_detect_their_own_key() {
        for k in Key::ALL {
            assert_eq!(key_of(&profile(k)).unwrap().key, k);
        }
    }

    #[test]
    fn rotation_matches_transposition() {
        let rate = 22050;
        let a = progression(9, true, rate);
        let c = chroma(&a, &a, rate);
        let d = key_of(&rotate(&c, -5)).unwrap();
        assert_eq!(d.key.relative_major_root(), Key { root: 4, minor: true }.relative_major_root());
    }
}
