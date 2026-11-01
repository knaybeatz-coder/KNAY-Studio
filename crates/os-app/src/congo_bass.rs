// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Congolese bass lines for the chord workspace, pure std: rule-based MIDI (no audio model), so the musical side can
//! be checked on its own and any bass instrument plays it. Three styles: Deep Rumba (sparse, syncopated, rests are
//! part of the groove), Melodic Rumba (the groove plus passing and chord tones) and Seben (driving, repetitive,
//! busier). Lines follow the chords and the key, repeat in 2-bar cells with controlled variation, answer phrase ends
//! (bars 4 and 8) with fills, and anticipate the next chord. Everything sits on a 16th grid; humanizing only moves
//! notes a few ticks and never past their neighbours.
//!
//! These styles are generation categories, not a claim that every Congolese song follows one formula.

/// The bass styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    #[default]
    DeepRumba,
    MelodicRumba,
    Seben,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::DeepRumba, Style::MelodicRumba, Style::Seben];

    pub fn name(self) -> &'static str {
        match self {
            Style::DeepRumba => "Deep Rumba",
            Style::MelodicRumba => "Melodic Rumba",
            Style::Seben => "Seben",
        }
    }
}

/// The controls, each 0 ..= 4 (Low .. High; 2 = the style's own feel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub style: Style,
    pub intensity: u8,
    pub density: u8,
    pub rests: u8,
    pub syncopation: u8,
    pub variation: u8,
    pub fills: u8,
    pub length: u8,
    pub human: u8,
    pub seed: u64,
}

impl Default for Params {
    fn default() -> Params {
        Params { style: Style::DeepRumba, intensity: 2, density: 2, rests: 2, syncopation: 2, variation: 2, fills: 2, length: 2, human: 1, seed: 1 }
    }
}

/// A chord the bass follows: its root and third (pitch classes, C = 0), from `start` for `len` ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordSpan {
    pub root: u8,
    pub minor: bool,
    pub start: u32,
    pub len: u32,
}

/// A generated note: position and length in ticks, MIDI key (60 = middle C), velocity 1..=127.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassNote {
    pub pos: u32,
    pub len: u32,
    pub key: u8,
    pub vel: u8,
}

/// What a step of a cell plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Root,
    Fifth,
    /// The fifth below the root.
    FifthBelow,
    Third,
    Octave,
    /// A scale tone between chord tones (the 6th of the chord, or the 2nd).
    Passing,
    /// A step toward the next chord's root.
    Approach,
    /// A short repeat of the previous note (a ghosted pick).
    Repeat,
}

/// One note of a one-bar cell: 16th step, length in steps, role, accent 0..1, priority (lower = kept longer when the
/// density goes down).
#[derive(Debug, Clone, Copy)]
struct Hit {
    step: u32,
    len: u32,
    role: Role,
    accent: f32,
    keep: u8,
}

const fn h(step: u32, len: u32, role: Role, accent: f32, keep: u8) -> Hit {
    Hit { step, len, role, accent, keep }
}

/// Cells per style: two bars (A, B) that alternate, and an alternative for each used by Variation.
fn cells(style: Style) -> [[&'static [Hit]; 2]; 2] {
    use Role::*;
    match style {
        // Short strong notes, space on 2 and around 3, the "and" of 2 and the anticipation on 4-and.
        Style::DeepRumba => [
            [
                const { &[h(0, 3, Root, 1.0, 0), h(6, 2, Fifth, 0.75, 1), h(10, 2, Octave, 0.6, 2), h(14, 2, Approach, 0.7, 1)] },
                const { &[h(0, 3, Root, 1.0, 0), h(3, 1, Repeat, 0.35, 3), h(6, 3, FifthBelow, 0.8, 1), h(12, 2, Root, 0.65, 2), h(15, 1, Approach, 0.6, 2)] },
            ],
            [
                const { &[h(0, 2, Root, 1.0, 0), h(4, 2, Fifth, 0.6, 2), h(6, 2, Octave, 0.75, 1), h(14, 2, Approach, 0.7, 1)] },
                const { &[h(0, 4, Root, 1.0, 0), h(7, 2, Fifth, 0.75, 1), h(10, 1, Repeat, 0.35, 3), h(12, 2, Third, 0.6, 2), h(14, 2, Approach, 0.7, 1)] },
            ],
        ],
        // The groove with chord and passing tones walking between.
        Style::MelodicRumba => [
            [
                const { &[h(0, 2, Root, 1.0, 0), h(3, 1, Third, 0.5, 2), h(6, 2, Fifth, 0.8, 1), h(10, 2, Passing, 0.6, 2), h(12, 2, Fifth, 0.6, 1), h(14, 2, Approach, 0.7, 1)] },
                const { &[h(0, 3, Root, 1.0, 0), h(4, 2, Octave, 0.55, 2), h(6, 2, Fifth, 0.8, 1), h(9, 1, Third, 0.5, 3), h(10, 2, Passing, 0.6, 2), h(14, 2, Approach, 0.7, 1)] },
            ],
            [
                const { &[h(0, 2, Root, 1.0, 0), h(2, 2, Third, 0.55, 2), h(6, 2, Fifth, 0.8, 1), h(8, 2, Octave, 0.6, 2), h(12, 2, Passing, 0.6, 2), h(14, 2, Approach, 0.7, 1)] },
                const { &[h(0, 3, Root, 1.0, 0), h(6, 2, Third, 0.75, 1), h(8, 2, Fifth, 0.6, 2), h(11, 1, Passing, 0.5, 3), h(12, 2, Octave, 0.6, 1), h(15, 1, Approach, 0.6, 2)] },
            ],
        ],
        // Eighth-note drive with 16th pickups, the fifth and octave bouncing.
        Style::Seben => [
            [
                const { &[h(0, 2, Root, 1.0, 0), h(2, 1, Repeat, 0.5, 2), h(3, 1, Root, 0.6, 3), h(4, 2, Fifth, 0.8, 1), h(6, 2, Octave, 0.7, 1), h(8, 2, Root, 0.85, 0), h(10, 1, Third, 0.55, 2), h(11, 1, Fifth, 0.6, 3), h(12, 2, Octave, 0.75, 1), h(14, 2, Approach, 0.75, 1)] },
                const { &[h(0, 2, Root, 1.0, 0), h(2, 2, Fifth, 0.6, 2), h(4, 1, Octave, 0.75, 1), h(5, 1, Fifth, 0.5, 3), h(6, 2, Root, 0.7, 1), h(8, 2, Fifth, 0.85, 0), h(10, 2, Octave, 0.6, 2), h(12, 1, Third, 0.6, 2), h(13, 1, Fifth, 0.5, 3), h(14, 2, Approach, 0.75, 1)] },
            ],
            [
                const { &[h(0, 2, Root, 1.0, 0), h(2, 2, Octave, 0.6, 2), h(4, 2, Fifth, 0.8, 1), h(6, 1, Root, 0.6, 2), h(7, 1, Repeat, 0.45, 3), h(8, 2, Octave, 0.85, 0), h(10, 2, Fifth, 0.6, 1), h(12, 2, Root, 0.7, 1), h(14, 2, Approach, 0.75, 1)] },
                const { &[h(0, 2, Root, 1.0, 0), h(3, 1, Repeat, 0.5, 2), h(4, 2, Octave, 0.8, 1), h(6, 2, Fifth, 0.6, 2), h(8, 2, Root, 0.85, 0), h(10, 1, Third, 0.55, 2), h(11, 1, Fifth, 0.5, 3), h(12, 2, Octave, 0.7, 1), h(14, 1, Fifth, 0.6, 2), h(15, 1, Approach, 0.7, 1)] },
            ],
        ],
    }
}

/// A small deterministic generator (xorshift), so a seed always gives the same line.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// 0.0 ..< 1.0
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Bass register: roots between A1 (33) and G#2 (44).
fn root_key(pc: u8) -> i32 {
    let k = 36 + i32::from(pc % 12);
    if k > 44 { k - 12 } else { k }
}

/// The scale tone `steps` scale steps from `key` (scale: pitch classes; empty = the chord's major or minor scale).
fn scale_step(key: i32, steps: i32, scale: &[u8]) -> i32 {
    let mut k = key;
    let dir = steps.signum();
    for _ in 0..steps.abs() {
        k += dir;
        for _ in 0..12 {
            if scale.is_empty() || scale.contains(&(k.rem_euclid(12) as u8)) {
                break;
            }
            k += dir;
        }
    }
    k
}

/// Generates a bass line over `chords` (in order, not overlapping) with `scale` (the key's pitch classes; empty =
/// chromatic) at `ppq` ticks per quarter, 4/4.
pub fn generate(chords: &[ChordSpan], scale: &[u8], ppq: u32, p: Params) -> Vec<BassNote> {
    let Some(first) = chords.first() else { return Vec::new() };
    let end = chords.iter().map(|c| c.start + c.len).max().unwrap_or(0);
    let step = (ppq / 4).max(1);
    let bar = step * 16;
    let mut rng = Rng(p.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5DEE_CE66);
    let level = |v: u8| f32::from(v.min(4)) / 4.0; // 0..1, 0.5 = the style's own
    let chord_at = |t: u32| chords.iter().rev().find(|c| c.start <= t).copied().unwrap_or(*first);
    let next_chord = |t: u32| chords.iter().find(|c| c.start > t).copied().unwrap_or(*first);
    let cells = cells(p.style);
    // The style's own density, moved by Density and Rest Amount: notes whose priority is above `keep` go.
    let keep_max = (2.0 + (level(p.density) - 0.5) * 4.0 - (level(p.rests) - 0.5) * 3.0).round().clamp(0.0, 3.0) as u8;
    let mut out: Vec<BassNote> = Vec::new();
    let mut prev_key = root_key(first.root);
    let bars = end.div_ceil(bar);
    for b in 0..bars {
        let bar_start = first.start + b * bar;
        let phrase_end = (b + 1) % 4 == 0;
        // Variation: the cell's alternative, more often at higher settings, never on the first bar of a phrase.
        let alt = b % 4 != 0 && rng.unit() < 0.15 + 0.6 * level(p.variation);
        let cell = cells[usize::from(alt)][(b % 2) as usize];
        // A fill closes phrases (bar 4; bar 8 more surely), Fill Frequency deciding how often.
        let fill = phrase_end && rng.unit() < level(p.fills) * if (b + 1) % 8 == 0 { 1.2 } else { 0.7 };
        for hit in cell {
            if fill && hit.step >= 12 {
                continue;
            }
            // Density and rests: the downbeat always stays; others by their priority, then a little chance.
            if hit.step != 0 && (hit.keep > keep_max || rng.unit() < (level(p.rests) - 0.5).max(0.0) * 0.3 * f32::from(hit.keep)) {
                continue;
            }
            let mut st = hit.step;
            // Syncopation: notes on beats 2 and 4 pushed to the 16th before at higher settings, or held on the beat lower.
            if (st == 4 || st == 12) && rng.unit() < (level(p.syncopation) - 0.4).max(0.0) {
                st -= 1;
            }
            let pos = bar_start + st * step;
            if pos >= end {
                continue;
            }
            let chord = chord_at(pos);
            let root = root_key(chord.root);
            let third = root + if chord.minor { 3 } else { 4 };
            let key = match hit.role {
                Role::Root => root,
                Role::Fifth => root + 7,
                Role::FifthBelow => root - 5,
                Role::Third => third,
                Role::Octave => root + 12,
                Role::Passing => {
                    if rng.unit() < 0.5 { scale_step(root + 7, 1, scale) } else { scale_step(root, 1, scale) }
                }
                Role::Repeat => prev_key,
                Role::Approach => {
                    let next = if pos + 2 * step >= chord.start + chord.len || (st >= 14) { next_chord(pos) } else { chord };
                    let target = root_key(next.root);
                    if next.root == chord.root && rng.unit() < 0.5 {
                        root + 7
                    } else if target > prev_key {
                        scale_step(target, -1, scale)
                    } else {
                        scale_step(target, 1, scale)
                    }
                }
            };
            // Note Length: staccato (0.45 of the cell's) .. held (1.2), never into the next note.
            let len_k = 0.45 + 0.75 * level(p.length) * if p.style == Style::Seben { 0.85 } else { 1.0 };
            let len = ((hit.len * step) as f32 * len_k).round().max((step / 2) as f32) as u32;
            let accent = hit.accent * (0.75 + 0.5 * level(p.intensity));
            let vel = (40.0 + 80.0 * accent.clamp(0.0, 1.05)).round().clamp(1.0, 127.0) as u8;
            out.push(BassNote { pos, len, key: key.clamp(24, 60) as u8, vel });
            prev_key = key;
        }
        if fill {
            // Four 16ths walking to the next bar's root, scale-wise, from where the line is.
            let next = chord_at(bar_start + bar);
            let target = root_key(next.root);
            let dir = if target >= prev_key { 1 } else { -1 };
            let mut k = scale_step(target, -dir * 4, scale);
            for i in 0..4u32 {
                let pos = bar_start + (12 + i) * step;
                if pos >= end {
                    break;
                }
                k = if i == 0 { k } else { scale_step(k, dir, scale) };
                let vel = (60.0 + 50.0 * level(p.intensity) + 6.0 * i as f32).min(127.0) as u8;
                out.push(BassNote { pos, len: (step * 9 / 10).max(1), key: k.clamp(24, 60) as u8, vel });
                prev_key = k;
            }
        }
    }
    // One note per position (a syncopated note can land on a neighbour's step): the first stays.
    out.sort_by_key(|n| n.pos);
    out.dedup_by_key(|n| n.pos);
    // Humanize: velocity and a few ticks of timing, never onto or past a neighbour; downbeats stay put.
    if p.human > 0 {
        let amount = f32::from(p.human.min(4));
        let shift = (step / 12).max(1) as f32 * amount / 2.0;
        for i in 0..out.len() {
            let dv = ((rng.unit() - 0.5) * 12.0 * amount) as i32;
            out[i].vel = (i32::from(out[i].vel) + dv).clamp(1, 127) as u8;
            if (out[i].pos - first.start.min(out[i].pos)) % bar != 0 {
                let dt = ((rng.unit() - 0.5) * 2.0 * shift).round() as i32;
                let lo = if i > 0 { out[i - 1].pos + 2 } else { 0 };
                let hi = out.get(i + 1).map_or(u32::MAX, |n| n.pos.saturating_sub(2));
                let pos = (out[i].pos as i32 + dt).max(0) as u32;
                out[i].pos = pos.clamp(lo.min(hi), hi.max(lo));
            }
        }
    }
    // No overlaps: each note ends before the next starts.
    for i in 0..out.len().saturating_sub(1) {
        let gap = out[i + 1].pos.saturating_sub(out[i].pos);
        out[i].len = out[i].len.min(gap.saturating_sub(1).max(1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPQ: u32 = 96;

    /// I - IV - V - IV in C, a bar each, 8 bars.
    fn rumba_progression() -> Vec<ChordSpan> {
        let bar = PPQ * 4;
        [0u8, 5, 7, 5, 0, 5, 7, 5].iter().enumerate().map(|(i, &r)| ChordSpan { root: r, minor: false, start: i as u32 * bar, len: bar }).collect()
    }

    fn c_major() -> Vec<u8> {
        vec![0, 2, 4, 5, 7, 9, 11]
    }

    fn line(style: Style) -> Vec<BassNote> {
        generate(&rumba_progression(), &c_major(), PPQ, Params { style, human: 0, ..Params::default() })
    }

    #[test]
    fn follows_the_chords_and_the_key() {
        for style in Style::ALL {
            let l = line(style);
            assert!(!l.is_empty());
            for n in &l {
                let chord = rumba_progression().into_iter().rev().find(|c| c.start <= n.pos).unwrap();
                if n.pos == chord.start {
                    assert_eq!(u32::from(n.key) % 12, u32::from(chord.root), "{style:?}: the downbeat plays the root");
                }
                assert!(c_major().contains(&(n.key % 12)), "{style:?}: {} is in the key", n.key);
                assert!((24..=60).contains(&n.key), "bass register");
            }
        }
    }

    #[test]
    fn sits_on_the_grid_at_the_tempo() {
        // Without humanize every note starts on a 16th of the given timebase, inside the progression.
        let step = PPQ / 4;
        for style in Style::ALL {
            for n in line(style) {
                assert_eq!(n.pos % step, 0);
                assert!(n.pos < 8 * 4 * PPQ);
            }
        }
    }

    fn busy(l: &[BassNote]) -> f32 {
        l.len() as f32 / 8.0
    }

    #[test]
    fn deep_rumba_rests_and_seben_drives() {
        let deep = line(Style::DeepRumba);
        let seben = line(Style::Seben);
        // Deep Rumba leaves space: at least a beat of silence in most bars.
        let bar = 4 * PPQ;
        let mut with_space = 0;
        for b in 0..8 {
            let notes: Vec<&BassNote> = deep.iter().filter(|n| n.pos / bar == b).collect();
            let sounding: u32 = notes.iter().map(|n| n.len).sum();
            if bar - sounding.min(bar) >= PPQ {
                with_space += 1;
            }
        }
        assert!(with_space >= 6, "rests in {with_space} bars");
        assert!(busy(&seben) > busy(&deep) * 1.6, "seben {} vs deep {}", busy(&seben), busy(&deep));
        // Rest Amount and Density move it.
        let sparse = generate(&rumba_progression(), &c_major(), PPQ, Params { rests: 4, density: 0, human: 0, ..Params::default() });
        let dense = generate(&rumba_progression(), &c_major(), PPQ, Params { rests: 0, density: 4, human: 0, ..Params::default() });
        assert!(dense.len() > sparse.len() + 8, "{} vs {}", dense.len(), sparse.len());
    }

    #[test]
    fn varies_without_losing_the_groove() {
        let a = generate(&rumba_progression(), &c_major(), PPQ, Params { variation: 4, human: 0, seed: 3, ..Params::default() });
        let bar = 4 * PPQ;
        let shape = |b: u32| a.iter().filter(|n| n.pos / bar == b).map(|n| n.pos % bar).collect::<Vec<_>>();
        let shapes: Vec<Vec<u32>> = (0..8).map(shape).collect();
        assert!(shapes.iter().skip(1).any(|s| s != &shapes[0]), "some bars vary");
        assert!(shapes.iter().all(|s| s.first() == Some(&0)), "every bar keeps its downbeat");
        let calm = generate(&rumba_progression(), &c_major(), PPQ, Params { variation: 0, fills: 0, human: 0, seed: 3, ..Params::default() });
        let calm_shapes: Vec<Vec<u32>> = (0..8).map(|b| calm.iter().filter(|n| n.pos / bar == b).map(|n| n.pos % bar).collect()).collect();
        assert_eq!(calm_shapes[0], calm_shapes[2], "low variation repeats the cell");
    }

    #[test]
    fn notes_have_bass_lengths_and_never_overlap() {
        for style in Style::ALL {
            let l = generate(&rumba_progression(), &c_major(), PPQ, Params { style, human: 3, ..Params::default() });
            for w in l.windows(2) {
                assert!(w[0].pos + w[0].len <= w[1].pos, "{style:?}: no overlap");
                assert!(w[0].pos < w[1].pos);
            }
            for n in &l {
                assert!(n.len >= PPQ / 8 && n.len <= PPQ * 2, "{style:?}: length {}", n.len);
            }
        }
    }

    #[test]
    fn same_seed_same_line_new_seed_new_line() {
        let p = Params { variation: 3, ..Params::default() };
        assert_eq!(generate(&rumba_progression(), &c_major(), PPQ, p), generate(&rumba_progression(), &c_major(), PPQ, p));
        assert_ne!(generate(&rumba_progression(), &c_major(), PPQ, p), generate(&rumba_progression(), &c_major(), PPQ, Params { seed: 9, ..p }));
    }

    #[test]
    fn minor_keys_and_fills() {
        // i - iv - V in A minor with fills: notes stay in A harmonic-ish minor (A natural minor + G#).
        let bar = PPQ * 4;
        let chords: Vec<ChordSpan> = [(9u8, true), (2, true), (4, false), (9, true)].iter().cycle().take(8).enumerate().map(|(i, &(r, m))| ChordSpan { root: r, minor: m, start: i as u32 * bar, len: bar }).collect();
        let scale = vec![9, 11, 0, 2, 4, 5, 7, 8];
        let l = generate(&chords, &scale, PPQ, Params { fills: 4, human: 0, ..Params::default() });
        assert!(l.iter().all(|n| scale.contains(&(n.key % 12))));
        // A fill: four 16ths in the last beat of bar 4.
        let last_beat: Vec<&BassNote> = l.iter().filter(|n| n.pos >= 3 * bar + 3 * PPQ && n.pos < 4 * bar).collect();
        assert_eq!(last_beat.len(), 4, "{last_beat:?}");
    }
}
