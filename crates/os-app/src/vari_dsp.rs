// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! VariAudio's signal side, pure std: a pitch track of a monophonic voice (YIN on a decimated copy, then cleaned of
//! octave errors, breaths and consonants), its segments (one per sung note), and a render that changes each segment
//! by TD-PSOLA: pitch (shift, Correct Pitch, Straighten Curve), formants and volume. Timing and length never change.

/// Pitch frames per second of audio (hop of the pitch track); frames are counted in seconds, whatever the rate.
pub const FRAME_RATE: f64 = 172.265625;
const DECIMATED: f64 = 11025.0;
const MIN_HZ: f64 = 65.0;
const MAX_HZ: f64 = 1100.0;
/// YIN threshold: a dip under it counts as the period; frames without one are unvoiced (breaths, consonants).
const YIN_THRESHOLD: f64 = 0.2;

/// One frame of the pitch track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Pitch as a MIDI key (60 = middle C), 0 when unvoiced.
    pub pitch: f32,
    /// Level (RMS of the frame).
    pub rms: f32,
}

impl Frame {
    pub fn voiced(&self) -> bool {
        self.pitch > 0.0
    }
}

/// The frame index of time `secs`.
pub fn frame_of(secs: f64) -> usize {
    (secs * FRAME_RATE).max(0.0) as usize
}

/// The time of frame `i`'s centre.
pub fn secs_of(i: usize) -> f64 {
    i as f64 / FRAME_RATE
}

fn hz_to_key(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

fn key_to_hz(key: f64) -> f64 {
    440.0 * 2f64.powf((key - 69.0) / 12.0)
}

/// Mono mix of a stereo pair.
pub fn mono(left: &[f32], right: &[f32]) -> Vec<f32> {
    left.iter().enumerate().map(|(i, l)| 0.5 * (l + right.get(i).copied().unwrap_or(*l))).collect()
}

/// `x` at `rate` down to about 11 kHz: averaged groups (a gentle low-pass), so the pitch search is cheap.
fn decimate(x: &[f32], rate: u32) -> (Vec<f64>, f64) {
    let step = ((f64::from(rate) / DECIMATED).round() as usize).max(1);
    let out = x.chunks(step).map(|c| c.iter().map(|v| f64::from(*v)).sum::<f64>() / c.len() as f64).collect();
    (out, f64::from(rate) / step as f64)
}

/// The pitch track of mono audio `x` at `rate` Hz: one frame per `1 / FRAME_RATE` seconds.
pub fn track(x: &[f32], rate: u32) -> Vec<Frame> {
    if rate == 0 || x.is_empty() {
        return Vec::new();
    }
    let (d, sr) = decimate(x, rate);
    let tau_min = (sr / MAX_HZ).floor().max(2.0) as usize;
    let tau_max = (sr / MIN_HZ).ceil() as usize;
    let w = tau_max + 8; // integration window: a little over the longest period
    let secs = x.len() as f64 / f64::from(rate);
    let n = (secs * FRAME_RATE).ceil() as usize;
    let mut diff = vec![0.0f64; tau_max + 2];
    let mut out = Vec::with_capacity(n);
    for f in 0..n {
        let centre = (secs_of(f) * sr) as usize;
        let start = centre.saturating_sub(w / 2);
        let rms = rms_at(x, rate, f);
        if start + w + tau_max >= d.len() {
            out.push(Frame { pitch: 0.0, rms });
            continue;
        }
        let seg = &d[start..start + w + tau_max];
        let energy: f64 = seg[..w].iter().map(|v| v * v).sum::<f64>() / w as f64;
        if energy < 1e-7 {
            out.push(Frame { pitch: 0.0, rms });
            continue;
        }
        // Cumulative mean normalized difference (YIN steps 2-3).
        let mut running = 0.0;
        diff[0] = 1.0;
        for tau in 1..=tau_max {
            let mut s = 0.0;
            for j in 0..w {
                let e = seg[j] - seg[j + tau];
                s += e * e;
            }
            running += s;
            diff[tau] = if running > 0.0 { s * tau as f64 / running } else { 1.0 };
        }
        // First dip under the threshold (step 4); none: unvoiced.
        let mut best = None;
        let mut tau = tau_min;
        while tau < tau_max {
            if diff[tau] < YIN_THRESHOLD {
                while tau + 1 < tau_max && diff[tau + 1] < diff[tau] {
                    tau += 1;
                }
                best = Some(tau);
                break;
            }
            tau += 1;
        }
        let pitch = best.map_or(0.0, |t| {
            // Parabolic interpolation around the dip (step 5).
            let (a, b, c) = (diff[t - 1], diff[t], diff[t + 1]);
            let den = a - 2.0 * b + c;
            let shift = if den.abs() > 1e-12 { 0.5 * (a - c) / den } else { 0.0 };
            let hz = sr / (t as f64 + shift.clamp(-1.0, 1.0));
            if (MIN_HZ..=MAX_HZ).contains(&hz) { hz_to_key(hz) as f32 } else { 0.0 }
        });
        out.push(Frame { pitch, rms });
    }
    clean(&mut out);
    out
}

fn rms_at(x: &[f32], rate: u32, f: usize) -> f32 {
    let half = (f64::from(rate) / FRAME_RATE) as usize;
    let c = (secs_of(f) * f64::from(rate)) as usize;
    let (a, b) = (c.saturating_sub(half), (c + half).min(x.len()));
    if b <= a {
        return 0.0;
    }
    (x[a..b].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// Cleans a raw track: quiet frames are unvoiced; each frame is compared with the median of the voiced frames around
/// it (about 120 ms): an octave off is moved back, anything else more than 2.5 semitones off is unvoiced (a breath, a
/// consonant, a detection error). Then a 5-frame median smooths the curve, and voiced islands too short to be sung
/// (under about 45 ms) go.
fn clean(t: &mut [Frame]) {
    let loud = t.iter().map(|f| f.rms).fold(0.0f32, f32::max);
    for f in t.iter_mut() {
        if f.rms < loud * 0.03 {
            f.pitch = 0.0;
        }
    }
    const HALF: usize = 10;
    let raw: Vec<f32> = t.iter().map(|f| f.pitch).collect();
    for i in 0..t.len() {
        if raw[i] <= 0.0 {
            continue;
        }
        let mut near: Vec<f32> = raw[i.saturating_sub(HALF)..(i + HALF + 1).min(raw.len())].iter().copied().filter(|p| *p > 0.0).collect();
        if near.len() < 5 {
            continue;
        }
        let m = median(&mut near);
        let d = raw[i] - m;
        if (d.abs() - 12.0).abs() < 1.5 {
            t[i].pitch = raw[i] - 12.0 * d.signum();
        } else if d.abs() > 2.5 {
            t[i].pitch = 0.0;
        }
    }
    let fixed: Vec<f32> = t.iter().map(|f| f.pitch).collect();
    for i in 0..t.len() {
        if fixed[i] <= 0.0 {
            continue;
        }
        let mut near: Vec<f32> = fixed[i.saturating_sub(2)..(i + 3).min(fixed.len())].iter().copied().filter(|p| *p > 0.0).collect();
        t[i].pitch = median(&mut near);
    }
    // Single unvoiced frames inside a voiced run are filled in.
    for i in 1..t.len().saturating_sub(1) {
        let (a, b, c) = (t[i - 1].pitch, t[i].pitch, t[i + 1].pitch);
        if b == 0.0 && a > 0.0 && c > 0.0 && (a - c).abs() < 1.0 {
            t[i].pitch = 0.5 * (a + c);
        }
    }
    let mut i = 0;
    while i < t.len() {
        if t[i].voiced() {
            let start = i;
            while i < t.len() && t[i].voiced() {
                i += 1;
            }
            if i - start < 8 {
                for f in &mut t[start..i] {
                    f.pitch = 0.0;
                }
            }
        } else {
            i += 1;
        }
    }
}

/// A sung note: frames `start..end` of the track, and its centre pitch (the median of its frames).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub pitch: f32,
}

/// The segments of a pitch track: each voiced run, cut where the pitch settles on another note (its median over the
/// next ~140 ms moves more than three quarters of a semitone from the current note's). Runs split by a short gap
/// (under ~35 ms) on the same note are one note.
pub fn segments(t: &[Frame]) -> Vec<Segment> {
    // About 140 ms ahead: longer than half a vibrato cycle, so vibrato does not cut a note.
    const LOOK: usize = 24;
    const MIN: usize = 14;
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        if !t[i].voiced() {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < t.len() && t[i].voiced() {
            i += 1;
        }
        let run = run_start..i;
        let mut cuts = vec![run.start];
        let mut k = run.start + MIN;
        while k + LOOK <= run.end {
            let last = *cuts.last().expect("a cut");
            let here = median(&mut t[last..k].iter().map(|f| f.pitch).collect::<Vec<_>>());
            let next = median(&mut t[k..k + LOOK].iter().map(|f| f.pitch).collect::<Vec<_>>());
            if (next - here).abs() > 0.75 && k - last >= MIN && run.end - k >= MIN {
                // Cut at the frame where the pitch crosses half way.
                let mid = 0.5 * (here + next);
                let at = (k.saturating_sub(LOOK / 2)..(k + LOOK / 2).min(run.end))
                    .find(|&j| (t[j].pitch - here).abs() > (mid - here).abs())
                    .unwrap_or(k);
                cuts.push(at.max(last + MIN / 2));
                k = at + MIN;
            } else {
                k += 1;
            }
        }
        cuts.push(run.end);
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if b <= a {
                continue;
            }
            let pitch = median(&mut t[a..b].iter().map(|f| f.pitch).collect::<Vec<_>>());
            match out.last_mut() {
                // A short gap on the same note: the same segment.
                Some(prev) if a - prev.end <= 6 && (prev.pitch - pitch).abs() < 0.6 => {
                    prev.end = b;
                    prev.pitch = median(&mut t[prev.start..b].iter().filter(|f| f.voiced()).map(|f| f.pitch).collect::<Vec<_>>());
                }
                _ => out.push(Segment { start: a, end: b, pitch }),
            }
        }
    }
    out
}

/// What the user did to a segment (the Cubase VariAudio parameters).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    /// Semitones up (cents as hundredths), by dragging.
    pub shift: f32,
    /// Correct Pitch, 0..1: how far the segment's centre moves to the nearest target note (`Edit::target`).
    pub correct: f32,
    /// Straighten Curve, 0..1: 0 keeps the pitch curve, 1 makes it a straight line at the segment's centre.
    pub straighten: f32,
    /// Shift Formant in semitones (-12..12): the voice's colour, without its pitch.
    pub formant: f32,
    /// Volume in dB.
    pub gain_db: f32,
}

impl Default for Edit {
    fn default() -> Edit {
        Edit { shift: 0.0, correct: 0.0, straighten: 0.0, formant: 0.0, gain_db: 0.0 }
    }
}

impl Edit {
    pub fn is_none(&self) -> bool {
        self.shift.abs() < 1e-4 && self.correct.abs() < 1e-4 && self.straighten.abs() < 1e-4 && self.formant.abs() < 1e-4 && self.gain_db.abs() < 1e-3
    }
}

/// The nearest note to `pitch` in `scale` (pitch classes, C = 0); every semitone when the scale is empty.
pub fn nearest(pitch: f32, scale: &[u8]) -> f32 {
    if scale.is_empty() {
        return pitch.round();
    }
    let base = pitch.floor() as i32;
    (base - 6..=base + 7).filter(|k| scale.contains(&(k.rem_euclid(12) as u8))).map(|k| k as f32).min_by(|a, b| (a - pitch).abs().total_cmp(&(b - pitch).abs())).unwrap_or(pitch.round())
}

/// Segment `s`'s centre once edited: its pitch, the drag's shift, then Correct Pitch toward the nearest note of
/// `scale`.
pub fn centre(s: &Segment, e: &Edit, scale: &[u8]) -> f32 {
    let moved = s.pitch + e.shift;
    moved + (nearest(moved, scale) - moved) * e.correct.clamp(0.0, 1.0)
}

/// Frame pitch `orig` (in segment `s`) once edited: its distance from the segment's pitch kept less Straighten Curve,
/// around the edited centre.
pub fn edited_pitch(orig: f32, s: &Segment, e: &Edit, scale: &[u8]) -> f32 {
    centre(s, e, scale) + (orig - s.pitch) * (1.0 - e.straighten.clamp(0.0, 1.0))
}

/// Per frame: (pitch ratio, formant ratio, gain) the render applies; 1 outside edited segments. Eased over a few
/// frames at segment edges so a change does not click in.
pub fn controls(t: &[Frame], segs: &[Segment], edits: &[Edit], scale: &[u8]) -> Vec<(f64, f64, f64)> {
    let mut r = vec![(0.0f64, 0.0f64, 0.0f64); t.len()]; // logs
    for (s, e) in segs.iter().zip(edits) {
        if e.is_none() {
            continue;
        }
        let c = centre(s, e, scale);
        for f in s.start..s.end.min(t.len()) {
            let p = t[f].pitch;
            let semis = if p > 0.0 { edited_pitch(p, s, e, scale) - p } else { c - s.pitch };
            r[f] = (f64::from(semis) / 12.0 * std::f64::consts::LN_2, f64::from(e.formant) / 12.0 * std::f64::consts::LN_2, f64::from(e.gain_db) / 20.0 * std::f64::consts::LN_10);
        }
    }
    let k = 3usize;
    (0..r.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(k), (i + k + 1).min(r.len()));
            let n = (b - a) as f64;
            let sum = r[a..b].iter().fold((0.0, 0.0, 0.0), |s, v| (s.0 + v.0, s.1 + v.1, s.2 + v.2));
            ((sum.0 / n).exp(), (sum.1 / n).exp(), (sum.2 / n).exp())
        })
        .collect()
}

/// Analysis pitch marks of `x` at `rate`: one per period in voiced frames (on the waveform's peaks, so grains line
/// up), every 5 ms elsewhere.
fn marks(x: &[f32], rate: u32, t: &[Frame]) -> Vec<(usize, usize)> {
    let sr = f64::from(rate);
    let mut out = Vec::new();
    let mut pos = 0usize;
    let unvoiced = (sr / 200.0) as usize;
    while pos < x.len() {
        let f = frame_of(pos as f64 / sr).min(t.len().saturating_sub(1));
        let period = t.get(f).filter(|fr| fr.voiced()).map_or(unvoiced, |fr| (sr / key_to_hz(f64::from(fr.pitch))).round() as usize).max(16);
        let voiced = t.get(f).is_some_and(|fr| fr.voiced());
        let at = if voiced {
            // The largest sample within a quarter period of where the mark is expected.
            let (a, b) = (pos.saturating_sub(period / 4), (pos + period / 4).min(x.len()));
            (a..b).max_by(|&i, &j| x[i].total_cmp(&x[j])).unwrap_or(pos)
        } else {
            pos
        };
        let at = at.max(out.last().map_or(0, |m: &(usize, usize)| m.0 + period / 2));
        if at >= x.len() {
            break;
        }
        out.push((at, period));
        pos = at + period;
    }
    out
}

fn sample_at(x: &[f32], pos: f64) -> f32 {
    if pos < 0.0 {
        return 0.0;
    }
    let i = pos.floor() as usize;
    let f = (pos - i as f64) as f32;
    match (x.get(i), x.get(i + 1)) {
        (Some(a), Some(b)) => a + (b - a) * f,
        (Some(a), None) => *a,
        _ => 0.0,
    }
}

/// Renders `channels` (each the same length, at `rate`) with each segment's edit: pitch and formants move by
/// TD-PSOLA on pitch marks taken from `mono_ref`, volume by the segment's gain; timing and length stay. `scale` is
/// the Correct Pitch target (pitch classes; empty = every semitone). With no edit the output equals the input.
pub fn render(channels: &[&[f32]], mono_ref: &[f32], rate: u32, t: &[Frame], segs: &[Segment], edits: &[Edit], scale: &[u8]) -> Vec<Vec<f32>> {
    let c = controls(t, segs, edits, scale);
    let idle = |v: &(f64, f64, f64)| (v.0 - 1.0).abs() < 1e-6 && (v.1 - 1.0).abs() < 1e-6 && (v.2 - 1.0).abs() < 1e-6;
    if c.iter().all(idle) {
        return channels.iter().map(|c| c.to_vec()).collect();
    }
    let sr = f64::from(rate);
    let ms = marks(mono_ref, rate, t);
    let n = mono_ref.len();
    let at = |pos: usize| c.get(frame_of(pos as f64 / sr)).copied().unwrap_or((1.0, 1.0, 1.0));
    // Synthesis marks: from each position, the next mark comes a (local period / ratio) later; each takes the grain
    // of the analysis mark nearest to it in time.
    let mut synth: Vec<(usize, usize, f64)> = Vec::new(); // (synthesis position, analysis mark index, formant ratio)
    let mut pos = ms.first().map_or(0, |m| m.0) as f64;
    let mut mi = 0usize;
    while (pos as usize) < n && !ms.is_empty() {
        while mi + 1 < ms.len() && (ms[mi + 1].0 as f64) <= pos {
            mi += 1;
        }
        let near = if mi + 1 < ms.len() && (ms[mi + 1].0 as f64 - pos) < (pos - ms[mi].0 as f64) { mi + 1 } else { mi };
        let (ratio, formant, _) = at(pos as usize);
        synth.push((pos as usize, near, formant));
        let period = ms[near].1 as f64;
        pos += (period / ratio).max(8.0);
    }
    channels
        .iter()
        .map(|x| {
            let mut out = vec![0.0f32; n];
            let mut norm = vec![0.0f32; n];
            for &(sp, ai, formant) in &synth {
                let (am, period) = ms[ai];
                // A grain two periods long, read `formant` times faster than it plays (formants up for > 1).
                let half = period as isize;
                for k in -half..half {
                    let dst = sp as isize + k;
                    if dst < 0 || dst as usize >= n {
                        continue;
                    }
                    let src_off = k as f64 * formant;
                    if src_off.abs() >= half as f64 {
                        continue;
                    }
                    let w = 0.5 + 0.5 * (std::f64::consts::PI * src_off / half as f64).cos();
                    out[dst as usize] += sample_at(x, am as f64 + src_off) * w as f32;
                    norm[dst as usize] += w as f32;
                }
            }
            // Where nothing changes the original is kept exactly; elsewhere the window sum is divided out.
            (0..n)
                .map(|i| {
                    let v = at(i);
                    if idle(&v) {
                        x[i]
                    } else if (v.0 - 1.0).abs() < 1e-6 && (v.1 - 1.0).abs() < 1e-6 {
                        x[i] * v.2 as f32
                    } else if norm[i] > 1e-3 {
                        out[i] / norm[i] * v.2 as f32
                    } else {
                        x[i] * v.2 as f32
                    }
                })
                .collect()
        })
        .collect()
}

/// Segment `i` alone, edited, with a little room either side: what a click on it plays. Returns the stereo audio.
pub fn render_segment(left: &[f32], right: &[f32], rate: u32, t: &[Frame], segs: &[Segment], edits: &[Edit], i: usize, scale: &[u8]) -> (Vec<f32>, Vec<f32>) {
    let Some(s) = segs.get(i) else { return (Vec::new(), Vec::new()) };
    let pad = 4usize;
    let (f0, f1) = (s.start.saturating_sub(pad), (s.end + pad).min(t.len()));
    let sr = f64::from(rate);
    let (a, b) = (((secs_of(f0) * sr) as usize).min(left.len()), ((secs_of(f1) * sr) as usize).min(left.len()));
    if b <= a {
        return (Vec::new(), Vec::new());
    }
    let (l, r) = (&left[a..b], &right[a.min(right.len())..b.min(right.len())]);
    let sub_t = &t[f0..f1];
    let sub_s = [Segment { start: s.start - f0, end: s.end - f0, pitch: s.pitch }];
    let e = [edits.get(i).copied().unwrap_or_default()];
    let m = mono(l, r);
    let mut out = render(&[l, if r.len() == l.len() { r } else { l }], &m, rate, sub_t, &sub_s, &e, scale);
    // Short fades so the cut-out does not click.
    let fade = (rate as usize / 200).min(out[0].len() / 2);
    for ch in &mut out {
        let len = ch.len();
        for k in 0..fade {
            let g = k as f32 / fade as f32;
            ch[k] *= g;
            ch[len - 1 - k] *= g;
        }
    }
    let r = out.pop().unwrap_or_default();
    let l = out.pop().unwrap_or_default();
    (l, r)
}

/// Pitch classes of a major or minor scale on `root`.
pub fn scale_notes(root: u8, minor: bool) -> Vec<u8> {
    let steps: [u8; 7] = if minor { [0, 2, 3, 5, 7, 8, 10] } else { [0, 2, 4, 5, 7, 9, 11] };
    steps.iter().map(|s| (root + s) % 12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44100;

    /// A voice-like tone: a fundamental and a few harmonics, at `key` for `secs` (with a little vibrato when asked).
    fn tone(key: f64, secs: f64, vibrato: f64) -> Vec<f32> {
        let n = (secs * f64::from(RATE)) as usize;
        let mut phase = 0.0f64;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let hz = key_to_hz(key + vibrato * (2.0 * std::f64::consts::PI * 5.5 * t).sin());
                phase += 2.0 * std::f64::consts::PI * hz / f64::from(RATE);
                let v = phase.sin() + 0.5 * (2.0 * phase).sin() + 0.3 * (3.0 * phase).sin() + 0.15 * (4.0 * phase).sin();
                (v * 0.3) as f32
            })
            .collect()
    }

    fn noise(secs: f64, level: f32, seed: &mut u64) -> Vec<f32> {
        (0..(secs * f64::from(RATE)) as usize)
            .map(|_| {
                *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (((*seed >> 33) as f32 / (1u64 << 31) as f32) - 0.5) * level
            })
            .collect()
    }

    fn voiced_median(t: &[Frame], a: f64, b: f64) -> f32 {
        let mut v: Vec<f32> = t[frame_of(a)..frame_of(b).min(t.len())].iter().filter(|f| f.voiced()).map(|f| f.pitch).collect();
        assert!(!v.is_empty(), "voiced frames between {a} and {b}");
        median(&mut v)
    }

    #[test]
    fn tracks_a_sung_pitch() {
        for key in [48.0, 57.0, 64.0, 72.0] {
            let x = tone(key, 1.0, 0.0);
            let t = track(&x, RATE);
            let p = voiced_median(&t, 0.2, 0.8);
            assert!((f64::from(p) - key).abs() < 0.15, "{key}: {p}");
        }
    }

    #[test]
    fn segments_follow_the_notes_and_skip_silence() {
        let mut x = tone(60.0, 0.6, 0.1);
        x.extend(tone(64.0, 0.6, 0.1));
        x.extend(vec![0.0; (0.3 * f64::from(RATE)) as usize]);
        x.extend(tone(67.0, 0.6, 0.1));
        let t = track(&x, RATE);
        let s = segments(&t);
        let keys: Vec<i32> = s.iter().map(|s| s.pitch.round() as i32).collect();
        assert_eq!(keys, vec![60, 64, 67], "{s:?}");
        assert!(s[2].start > frame_of(1.4), "the gap is not a segment: {s:?}");
    }

    /// Breaths and consonants (noise) between notes make no segments and no spikes in the curve.
    #[test]
    fn noise_makes_no_segments_or_spikes() {
        let mut seed = 7u64;
        let mut x = noise(0.15, 0.4, &mut seed);
        x.extend(tone(62.0, 0.7, 0.2));
        x.extend(noise(0.1, 0.6, &mut seed));
        x.extend(tone(65.0, 0.7, 0.2));
        x.extend(noise(0.2, 0.3, &mut seed));
        let t = track(&x, RATE);
        let s = segments(&t);
        let keys: Vec<i32> = s.iter().map(|s| s.pitch.round() as i32).collect();
        assert_eq!(keys, vec![62, 65], "{s:?}");
        for sg in &s {
            for f in &t[sg.start..sg.end] {
                assert!(!f.voiced() || (f.pitch - sg.pitch).abs() < 1.5, "spike {} in {sg:?}", f.pitch);
            }
        }
    }

    #[test]
    fn no_edit_is_the_original() {
        let x = tone(62.0, 0.8, 0.2);
        let t = track(&x, RATE);
        let s = segments(&t);
        let e = vec![Edit::default(); s.len()];
        assert_eq!(render(&[&x], &x, RATE, &t, &s, &e, &[])[0], x);
    }

    fn level(v: &[f32]) -> f32 {
        (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn a_shift_moves_the_pitch_and_keeps_the_length() {
        let x = tone(57.0, 1.2, 0.0);
        let t = track(&x, RATE);
        let s = segments(&t);
        assert_eq!(s.len(), 1, "{s:?}");
        for shift in [2.0f32, -3.0, 5.0] {
            let e = vec![Edit { shift, ..Edit::default() }];
            let y = &render(&[&x], &x, RATE, &t, &s, &e, &[])[0];
            assert_eq!(y.len(), x.len());
            let p = voiced_median(&track(y, RATE), 0.3, 0.9);
            assert!((p - (57.0 + shift)).abs() < 0.25, "shift {shift}: {p}");
            let (a, b) = (level(&x), level(y));
            assert!(b > a * 0.6 && b < a * 1.5, "level {a} -> {b}");
        }
    }

    #[test]
    fn straighten_flattens_vibrato() {
        let x = tone(60.0, 1.5, 0.6);
        let t = track(&x, RATE);
        let s = segments(&t);
        assert_eq!(s.len(), 1, "{s:?}");
        let spread = |t: &[Frame]| {
            let v: Vec<f32> = t[frame_of(0.3)..frame_of(1.2)].iter().filter(|f| f.voiced()).map(|f| f.pitch).collect();
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        let before = spread(&t);
        let y = &render(&[&x], &x, RATE, &t, &s, &[Edit { straighten: 1.0, ..Edit::default() }], &[])[0];
        let after = spread(&track(y, RATE));
        assert!(before > 0.9, "vibrato is there: {before}");
        assert!(after < before * 0.5, "{before} -> {after}");
    }

    #[test]
    fn correct_pitch_goes_to_the_scale() {
        // Sung 30 cents flat of E (64): Correct Pitch 100% puts it on E; in C minor (no E) on E flat or F.
        let x = tone(63.7, 1.0, 0.0);
        let t = track(&x, RATE);
        let s = segments(&t);
        assert_eq!(s.len(), 1, "{s:?}");
        let e = [Edit { correct: 1.0, ..Edit::default() }];
        assert!((centre(&s[0], &e[0], &[]) - 64.0).abs() < 1e-4);
        let y = &render(&[&x], &x, RATE, &t, &s, &e, &[])[0];
        assert!((voiced_median(&track(y, RATE), 0.2, 0.8) - 64.0).abs() < 0.15);
        let minor = scale_notes(0, true);
        assert_eq!(centre(&s[0], &e[0], &minor), 63.0);
        assert_eq!(nearest(64.4, &scale_notes(0, false)), 64.0);
        assert_eq!(nearest(66.0, &scale_notes(0, false)), 65.0); // F# not in C major: F (G is as near; the lower wins)
    }

    #[test]
    fn volume_and_formant_keep_the_pitch() {
        let x = tone(60.0, 1.0, 0.0);
        let t = track(&x, RATE);
        let s = segments(&t);
        let y = &render(&[&x], &x, RATE, &t, &s, &[Edit { gain_db: -6.0, ..Edit::default() }], &[])[0];
        let mid = |v: &[f32]| level(&v[RATE as usize / 4..RATE as usize * 3 / 4]);
        assert!((mid(y) / mid(&x) - 0.501).abs() < 0.03, "{}", mid(y) / mid(&x));
        let z = &render(&[&x], &x, RATE, &t, &s, &[Edit { formant: 4.0, ..Edit::default() }], &[])[0];
        assert!((voiced_median(&track(z, RATE), 0.2, 0.8) - 60.0).abs() < 0.3, "formant leaves the pitch");
        assert!(z.iter().zip(&x).any(|(a, b)| (a - b).abs() > 1e-3), "but changes the sound");
    }

    #[test]
    fn a_segment_renders_alone_for_audition() {
        let mut x = tone(60.0, 0.6, 0.0);
        x.extend(tone(64.0, 0.6, 0.0));
        let t = track(&x, RATE);
        let s = segments(&t);
        assert_eq!(s.len(), 2);
        let e = vec![Edit::default(), Edit { shift: 3.0, ..Edit::default() }];
        let (l, r) = render_segment(&x, &x, RATE, &t, &s, &e, 1, &[]);
        assert_eq!(l.len(), r.len());
        assert!(l.len() > RATE as usize / 2 && l.len() < RATE as usize * 3 / 4, "{}", l.len());
        assert!((voiced_median(&track(&l, RATE), 0.1, 0.45) - 67.0).abs() < 0.3);
    }
}
