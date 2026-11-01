// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! VariAudio's signal side, pure std: a pitch track of a monophonic voice (YIN on a decimated copy), its segments
//! (one per sung note), and a render that moves each segment's pitch by TD-PSOLA. PSOLA keeps the formants of the
//! voice, so a shifted segment still sounds like the same singer; it changes pitch only, never timing.

/// Pitch frames per second of audio (hop of the pitch track).
pub const FRAME_RATE: f64 = 172.265625; // 44100 / 256, kept for any rate: frames are counted in seconds
const DECIMATED: f64 = 11025.0;
const MIN_HZ: f64 = 65.0;
const MAX_HZ: f64 = 1100.0;
/// YIN threshold: below it a dip counts as the period.
const YIN_THRESHOLD: f64 = 0.15;

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
        if start + w + tau_max >= d.len() {
            out.push(Frame { pitch: 0.0, rms: rms_at(x, rate, f) });
            continue;
        }
        let seg = &d[start..start + w + tau_max];
        let energy: f64 = seg[..w].iter().map(|v| v * v).sum::<f64>() / w as f64;
        let rms = rms_at(x, rate, f);
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
        // First dip under the threshold (step 4), else the global minimum when it is clear enough.
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
        let best = best.or_else(|| {
            let (t, v) = (tau_min..tau_max).map(|t| (t, diff[t])).min_by(|a, b| a.1.total_cmp(&b.1))?;
            (v < 0.3).then_some(t)
        });
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

/// Drops octave jumps of one or two frames and voiced islands too short to sing, and quiet frames.
fn clean(t: &mut [Frame]) {
    let loud = t.iter().map(|f| f.rms).fold(0.0f32, f32::max);
    for f in t.iter_mut() {
        if f.rms < loud * 0.02 {
            f.pitch = 0.0;
        }
    }
    for i in 1..t.len().saturating_sub(1) {
        let (a, b, c) = (t[i - 1].pitch, t[i].pitch, t[i + 1].pitch);
        if a > 0.0 && c > 0.0 && (a - c).abs() < 1.0 {
            if b == 0.0 || (b - a).abs() > 5.0 {
                t[i].pitch = 0.5 * (a + c);
            }
        }
    }
    let mut i = 0;
    while i < t.len() {
        if t[i].voiced() {
            let start = i;
            while i < t.len() && t[i].voiced() {
                i += 1;
            }
            if i - start < 6 {
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

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// The segments of a pitch track: each voiced run, cut where the pitch settles on another note (its median over the
/// next ~140 ms moves more than three quarters of a semitone from the current note's).
pub fn segments(t: &[Frame]) -> Vec<Segment> {
    // About 140 ms ahead: longer than half a vibrato cycle, so vibrato does not cut a note.
    const LOOK: usize = 24;
    const MIN: usize = 10;
    let mut out = Vec::new();
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
            if b > a {
                let pitch = median(&mut t[a..b].iter().map(|f| f.pitch).collect::<Vec<_>>());
                out.push(Segment { start: a, end: b, pitch });
            }
        }
    }
    out
}

/// What the user did to a segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    /// Semitones up (cents as hundredths).
    pub shift: f32,
    /// 0 keeps the pitch curve, 1 makes it a straight line at the segment's centre.
    pub straighten: f32,
}

impl Default for Edit {
    fn default() -> Edit {
        Edit { shift: 0.0, straighten: 0.0 }
    }
}

impl Edit {
    pub fn is_none(&self) -> bool {
        self.shift.abs() < 1e-4 && self.straighten.abs() < 1e-4
    }
}

/// Frame `f`'s pitch once `edit` is applied to segment `s`.
pub fn edited_pitch(orig: f32, s: &Segment, edit: &Edit) -> f32 {
    s.pitch + (orig - s.pitch) * (1.0 - edit.straighten.clamp(0.0, 1.0)) + edit.shift
}

/// Per frame: the pitch ratio the render applies (1 outside edited segments), eased over a few frames at segment
/// edges so a shift does not click in.
pub fn ratios(t: &[Frame], segs: &[Segment], edits: &[Edit]) -> Vec<f64> {
    let mut r = vec![1.0f64; t.len()];
    for (s, e) in segs.iter().zip(edits) {
        if e.is_none() {
            continue;
        }
        for f in s.start..s.end.min(t.len()) {
            let p = t[f].pitch;
            if p > 0.0 {
                r[f] = 2f64.powf(f64::from(edited_pitch(p, s, e) - p) / 12.0);
            } else {
                r[f] = 2f64.powf(f64::from(e.shift) / 12.0);
            }
        }
    }
    // Ease: a short moving average over the log ratio.
    let lg: Vec<f64> = r.iter().map(|v| v.ln()).collect();
    let k = 3usize;
    (0..r.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(k), (i + k + 1).min(r.len()));
            (lg[a..b].iter().sum::<f64>() / (b - a) as f64).exp()
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

/// Renders `channels` (each the same length, at `rate`) with each segment's edit: pitch moves by TD-PSOLA on pitch
/// marks taken from `mono_ref`; timing and length stay. With no edit the output equals the input.
pub fn render(channels: &[&[f32]], mono_ref: &[f32], rate: u32, t: &[Frame], segs: &[Segment], edits: &[Edit]) -> Vec<Vec<f32>> {
    let r = ratios(t, segs, edits);
    if r.iter().all(|v| (v - 1.0).abs() < 1e-6) {
        return channels.iter().map(|c| c.to_vec()).collect();
    }
    let sr = f64::from(rate);
    let ms = marks(mono_ref, rate, t);
    let n = mono_ref.len();
    let ratio_at = |pos: usize| r.get(frame_of(pos as f64 / sr)).copied().unwrap_or(1.0);
    // Synthesis marks: from each position, the next mark comes a (local period / ratio) later; each takes the grain
    // of the analysis mark nearest to it in time.
    let mut synth: Vec<(usize, usize)> = Vec::new(); // (synthesis position, analysis mark index)
    let mut pos = ms.first().map_or(0, |m| m.0) as f64;
    let mut mi = 0usize;
    while (pos as usize) < n && !ms.is_empty() {
        while mi + 1 < ms.len() && (ms[mi + 1].0 as f64) <= pos {
            mi += 1;
        }
        let near = if mi + 1 < ms.len() && (ms[mi + 1].0 as f64 - pos) < (pos - ms[mi].0 as f64) { mi + 1 } else { mi };
        synth.push((pos as usize, near));
        let period = ms[near].1 as f64;
        pos += (period / ratio_at(pos as usize)).max(8.0);
    }
    channels
        .iter()
        .map(|x| {
            let mut out = vec![0.0f32; n];
            let mut norm = vec![0.0f32; n];
            for &(sp, ai) in &synth {
                let (am, period) = ms[ai];
                let half = period as isize;
                for k in -half..half {
                    let src = am as isize + k;
                    let dst = sp as isize + k;
                    if src < 0 || dst < 0 || src as usize >= n || dst as usize >= n {
                        continue;
                    }
                    let w = 0.5 + 0.5 * (std::f32::consts::PI * k as f32 / half as f32).cos();
                    out[dst as usize] += x[src as usize] * w;
                    norm[dst as usize] += w;
                }
            }
            // Where nothing changes the original is kept exactly; elsewhere the window sum is divided out.
            out.iter()
                .zip(&norm)
                .enumerate()
                .map(|(i, (v, w))| {
                    let ratio = ratio_at(i);
                    if (ratio - 1.0).abs() < 1e-4 {
                        x[i]
                    } else if *w > 1e-3 {
                        v / w
                    } else {
                        x[i]
                    }
                })
                .collect()
        })
        .collect()
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

    #[test]
    fn no_edit_is_the_original() {
        let x = tone(62.0, 0.8, 0.2);
        let t = track(&x, RATE);
        let s = segments(&t);
        let e = vec![Edit::default(); s.len()];
        assert_eq!(render(&[&x], &x, RATE, &t, &s, &e)[0], x);
    }

    #[test]
    fn a_shift_moves_the_pitch_and_keeps_the_length() {
        let x = tone(57.0, 1.2, 0.0);
        let t = track(&x, RATE);
        let s = segments(&t);
        assert_eq!(s.len(), 1, "{s:?}");
        for shift in [2.0f32, -3.0, 5.0] {
            let e = vec![Edit { shift, straighten: 0.0 }];
            let y = &render(&[&x], &x, RATE, &t, &s, &e)[0];
            assert_eq!(y.len(), x.len());
            let p = voiced_median(&track(y, RATE), 0.3, 0.9);
            assert!((p - (57.0 + shift)).abs() < 0.25, "shift {shift}: {p}");
            // Still about as loud.
            let level = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
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
        let y = &render(&[&x], &x, RATE, &t, &s, &[Edit { shift: 0.0, straighten: 1.0 }])[0];
        let after = spread(&track(y, RATE));
        assert!(before > 0.9, "vibrato is there: {before}");
        assert!(after < before * 0.5, "{before} -> {after}");
    }
}
