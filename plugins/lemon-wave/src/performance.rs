// SPDX-License-Identifier: GPL-3.0-or-later
//! Input-note ownership and sample-clock scheduling for the performance keyboard.
use crate::{
    controls::*,
    dsp::{Engine, Patch},
};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed};

const NONE: u8 = 255;
const ROOT_LIMIT: usize = 64;
const SCALES: [[i16; 7]; 5] = [
    [0, 2, 4, 5, 7, 9, 11],
    [0, 2, 3, 5, 7, 8, 10],
    [0, 2, 3, 5, 7, 9, 10],
    [0, 2, 4, 5, 7, 9, 10],
    [0, 2, 3, 5, 7, 8, 11],
];
const SHAPES: [&[i16]; 15] = [
    &[0, 4, 7],
    &[0, 3, 7],
    &[0, 3, 6],
    &[0, 4, 8],
    &[0, 2, 7],
    &[0, 5, 7],
    &[0, 4, 7, 11],
    &[0, 3, 7, 10],
    &[0, 4, 7, 10],
    &[0, 3, 6, 10],
    &[0, 4, 7, 14],
    &[0, 3, 7, 14],
    &[0, 4, 7, 11, 14],
    &[0, 3, 7, 10, 14],
    &[0, 7],
];
pub fn labels(key: usize) -> Option<&'static [&'static str]> {
    Some(match key {
        CHORD_KEY => &[
            "C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B",
        ],
        CHORD_SCALE => &[
            "Major",
            "Natural minor",
            "Dorian",
            "Mixolydian",
            "Harmonic minor",
        ],
        CHORD_SHAPE => &[
            "In-key triad",
            "In-key seventh",
            "Major",
            "Minor",
            "Diminished",
            "Augmented",
            "Sus 2",
            "Sus 4",
            "Major 7",
            "Minor 7",
            "Dominant 7",
            "Half-diminished 7",
            "Add 9",
            "Minor add 9",
            "Major 9",
            "Minor 9",
            "Power fifth",
        ],
        CHORD_INV => &["Root position", "1st inversion", "2nd inversion"],
        ARP_ORDER => &["Up", "Down", "Up / Down", "As played"],
        ARP_RATE => &["1/4", "1/8", "1/16", "1/32", "1/8 triplet", "1/16 triplet"],
        ARP_OCT => &["1 octave", "2 octaves", "3 octaves"],
        ARP_GATE => &["25%", "50%", "75%", "95%"],
        _ => return None,
    })
}

pub struct Keyboard {
    pub octave: AtomicU8,
    pub dragging: AtomicBool,
    gesture: AtomicU32,
    pub open: AtomicBool,
    pub lights: [AtomicU64; 2],
}
impl Default for Keyboard {
    fn default() -> Self {
        Self {
            octave: AtomicU8::new(2),
            dragging: AtomicBool::new(false),
            gesture: AtomicU32::new(0xffff),
            open: Default::default(),
            lights: Default::default(),
        }
    }
}
impl Keyboard {
    pub fn note(&self) -> u8 {
        self.gesture.load(Relaxed) as u8
    }
    pub fn set_note(&self, note: u8) {
        if note == NONE {
            self.gesture.fetch_or(255, Relaxed);
            return;
        }
        let _ = self.gesture.fetch_update(Relaxed, Relaxed, |old| {
            if old as u8 == note {
                return None;
            }
            let sequence = (old >> 16).wrapping_add(1);
            Some((sequence << 16) | ((note as u32) << 8) | note as u32)
        });
    }
    pub fn cancel(&self) {
        self.dragging.store(false, Relaxed);
        self.gesture.store(0xffff, Relaxed);
    }
    pub fn release(&self) -> bool {
        self.set_note(NONE);
        self.dragging.swap(false, Relaxed)
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Harmony {
    on: bool,
    key: i16,
    scale: usize,
    shape: usize,
    inversion: usize,
}
impl Harmony {
    fn tones(self, note: u8) -> ([u8; 5], usize) {
        let mut notes = [0; 5];
        if !self.on {
            notes[0] = note;
            return (notes, 1);
        }
        let scale = SCALES[self.scale];
        let relative = note as i16 - self.key;
        let degree = scale
            .iter()
            .rposition(|&step| step <= relative.rem_euclid(12))
            .unwrap();
        let octave = self.key + 12 * relative.div_euclid(12);
        let mut pitches = [0i16; 5];
        let count = if self.shape < 2 {
            let count = 3 + self.shape;
            for (i, pitch) in pitches[..count].iter_mut().enumerate() {
                let d = degree + 2 * i;
                *pitch = octave + scale[d % 7] + 12 * (d / 7) as i16;
            }
            count
        } else {
            let offsets = SHAPES[self.shape - 2];
            for (pitch, interval) in pitches.iter_mut().zip(offsets) {
                *pitch = octave + scale[degree] + interval;
            }
            offsets.len()
        };
        for _ in 0..self.inversion.min(count - 1) {
            pitches[..count].rotate_left(1);
            while pitches[count - 1] <= pitches[count - 2] {
                pitches[count - 1] += 12;
            }
        }
        // Keep the whole voicing inside MIDI's limits at either end of the keyboard.
        while pitches[count - 1] > 127 {
            for pitch in &mut pitches[..count] {
                *pitch -= 12;
            }
        }
        while pitches[0] < 0 {
            for pitch in &mut pitches[..count] {
                *pitch += 12;
            }
        }
        for (out, pitch) in notes.iter_mut().zip(pitches) {
            *out = pitch as u8;
        }
        (notes, count)
    }
}

#[derive(Clone, Copy)]
struct Root {
    note: u8,
    channel: u8,
    id: Option<i32>,
    mouse: bool,
    velocity: f32,
    tuning: f32,
    owner: u64,
    down: bool,
    sustained: bool,
}
impl Root {
    fn held(&self) -> bool {
        self.down || self.sustained
    }
}
#[derive(Clone, Copy)]
struct Tone {
    note: u8,
    root: Root,
}

pub struct Player {
    roots: Vec<Root>,
    pool: Vec<Tone>,
    ended: Vec<(u8, u8, Option<i32>)>,
    next_owner: u64,
    pedal: [bool; 16],
    harmony: Harmony,
    arp: bool,
    order: usize,
    octaves: usize,
    beats: f64,
    gate: f64,
    phase: f64,
    step: usize,
    sounding: Option<u64>,
    mouse_note: u8,
    mouse_sequence: u16,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            roots: Vec::with_capacity(ROOT_LIMIT),
            pool: Vec::with_capacity(ROOT_LIMIT * 15),
            ended: Vec::with_capacity(256),
            next_owner: 1,
            pedal: [false; 16],
            harmony: Default::default(),
            arp: false,
            order: 0,
            octaves: 1,
            beats: 0.5,
            gate: 0.75,
            phase: 1.,
            step: 0,
            sounding: None,
            mouse_note: NONE,
            mouse_sequence: 0,
        }
    }
}
impl Player {
    pub fn reset(&mut self) {
        self.roots.clear();
        self.ended.clear();
        self.pedal.fill(false);
        self.phase = 1.;
        self.step = 0;
        self.sounding = None;
        self.mouse_note = NONE;
        self.mouse_sequence = 0;
    }
    fn play_chord(&self, engine: &mut Engine, root: Root) {
        let (notes, count) = self.harmony.tones(root.note);
        for &note in &notes[..count] {
            engine.start_voice(note, root.velocity, root.channel, None, root.owner);
        }
        engine.tune_owner(root.owner, root.tuning);
    }
    pub fn configure(&mut self, engine: &mut Engine, p: &Patch) {
        let choice = |k: usize| p[k].clamp(0., SETTINGS[k].range.1 as f32) as usize;
        let harmony = Harmony {
            on: p[CHORD_ON] > 0.5,
            key: choice(CHORD_KEY) as i16,
            scale: choice(CHORD_SCALE),
            shape: choice(CHORD_SHAPE),
            inversion: choice(CHORD_INV),
        };
        let arp = p[ARP_ON] > 0.5;
        let restart =
            arp != self.arp || (harmony != self.harmony && (harmony.on || self.harmony.on));
        self.harmony = harmony;
        self.arp = arp;
        self.order = choice(ARP_ORDER);
        self.octaves = choice(ARP_OCT) + 1;
        self.beats = [1., 0.5, 0.25, 0.125, 1. / 3., 1. / 6.][choice(ARP_RATE)];
        self.gate = [0.25, 0.5, 0.75, 0.95][choice(ARP_GATE)];
        if restart {
            self.phase = 1.;
            self.step = 0;
            self.sounding = None;
            for root in self.roots.iter().filter(|r| r.held()) {
                engine.release_owner(root.owner, false);
                if !arp {
                    self.play_chord(engine, *root);
                }
            }
        }
    }
    fn finish(&mut self, root: Root) {
        if !root.mouse && self.ended.len() < self.ended.capacity() {
            self.ended.push((root.note, root.channel, root.id));
        }
    }
    pub fn on(
        &mut self,
        engine: &mut Engine,
        note: u8,
        vel: f32,
        ch: u8,
        id: Option<i32>,
        mouse: bool,
    ) {
        if note > 127 || ch >= 16 {
            return;
        }
        if vel <= 0. {
            self.off(engine, note, ch, id, mouse, false);
            return;
        }
        if self.roots.len() == ROOT_LIMIT {
            let index = self.roots.iter().position(|r| !r.held()).unwrap_or(0);
            let old = self.roots.remove(index);
            engine.release_owner(old.owner, true);
            self.finish(old);
        }
        if !self.roots.iter().any(Root::held) {
            self.phase = 1.;
            self.step = 0;
        }
        let root = Root {
            note,
            channel: ch,
            id,
            mouse,
            velocity: vel,
            tuning: 0.,
            owner: self.next_owner,
            down: true,
            sustained: false,
        };
        self.next_owner = self.next_owner.wrapping_add(1).max(1);
        self.roots.push(root);
        if !self.arp {
            self.play_chord(engine, root);
        }
    }
    pub fn off(
        &mut self,
        engine: &mut Engine,
        note: u8,
        ch: u8,
        id: Option<i32>,
        mouse: bool,
        hard: bool,
    ) {
        for root in &mut self.roots {
            if root.channel == ch
                && root.note == note
                && root.mouse == mouse
                && id.is_none_or(|i| root.id == Some(i))
            {
                root.down = false;
                root.sustained = !hard && !mouse && self.pedal[ch as usize];
                if !root.sustained {
                    engine.release_owner(root.owner, hard);
                }
            }
        }
    }
    pub fn sustain(&mut self, engine: &mut Engine, ch: u8, on: bool) {
        if ch >= 16 {
            return;
        }
        self.pedal[ch as usize] = on;
        if !on {
            for root in &mut self.roots {
                if root.channel == ch && root.sustained {
                    root.sustained = false;
                    if !root.down {
                        engine.release_owner(root.owner, false);
                    }
                }
            }
        }
    }
    pub fn tune(&mut self, engine: &mut Engine, note: u8, ch: u8, id: Option<i32>, amount: f32) {
        if !amount.is_finite() {
            return;
        }
        for root in &mut self.roots {
            if !root.mouse
                && root.channel == ch
                && root.note == note
                && id.is_none_or(|i| root.id == Some(i))
            {
                root.tuning = amount;
                engine.tune_owner(root.owner, amount);
            }
        }
    }
    pub fn all_off(&mut self, engine: &mut Engine, ch: u8, hard: bool) {
        if ch >= 16 {
            return;
        }
        self.pedal[ch as usize] = false;
        for root in &mut self.roots {
            if root.channel == ch {
                root.down = false;
                root.sustained = false;
                engine.release_owner(root.owner, hard);
            }
        }
    }
    pub fn mouse(&mut self, engine: &mut Engine, keyboard: &Keyboard) {
        let gesture = keyboard.gesture.load(Relaxed);
        let sequence = (gesture >> 16) as u16;
        let pressed = sequence != self.mouse_sequence;
        self.mouse_sequence = sequence;
        let mut note = gesture as u8;
        // A click can start and end between callbacks. Sound it for one block.
        if pressed && note == NONE {
            note = (gesture >> 8) as u8;
        }
        if self.mouse_note == note && !pressed {
            return;
        }
        if self.mouse_note != NONE {
            self.off(engine, self.mouse_note, 0, None, true, false);
        }
        self.mouse_note = note;
        if note != NONE {
            self.on(engine, note, 0.8, 0, None, true);
        }
    }
    fn trigger(&mut self, engine: &mut Engine) {
        self.pool.clear();
        for root in self.roots.iter().filter(|r| r.held()) {
            let (notes, count) = self.harmony.tones(root.note);
            for octave in 0..self.octaves {
                for &pitch in &notes[..count] {
                    let note = pitch as usize + 12 * octave;
                    if note <= 127 {
                        self.pool.push(Tone {
                            note: note as u8,
                            root: *root,
                        });
                    }
                }
            }
        }
        if self.order != 3 {
            self.pool
                .sort_unstable_by_key(|t| (t.note, t.root.channel, t.root.owner));
            self.pool.dedup_by_key(|t| (t.note, t.root.channel));
        }
        let n = self.pool.len();
        if n == 0 {
            return;
        }
        let index = match self.order {
            1 => n - 1 - self.step % n,
            2 if n > 1 => {
                let cycle = 2 * n - 2;
                let i = self.step % cycle;
                i.min(cycle - i)
            }
            _ => self.step % n,
        };
        let tone = self.pool[index];
        self.step = self.step.wrapping_add(1);
        engine.start_voice(
            tone.note,
            tone.root.velocity,
            tone.root.channel,
            None,
            tone.root.owner,
        );
        engine.tune_owner(tone.root.owner, tone.root.tuning);
        self.sounding = Some(tone.root.owner);
    }
    pub fn render(&mut self, engine: &mut Engine, left: &mut [f32], right: &mut [f32]) {
        if !self.arp || !self.roots.iter().any(Root::held) {
            engine.render(left, right);
            return;
        }
        let period = (engine.rate as f64 * 60. / engine.tempo as f64 * self.beats).max(1.);
        let mut at = 0;
        while at < left.len() {
            if self.phase >= 1. - 1e-10 {
                if let Some(owner) = self.sounding.take() {
                    engine.release_owner(owner, false);
                }
                self.phase = (self.phase - 1.).max(0.);
                self.trigger(engine);
            }
            if self.phase >= self.gate - 1e-10 {
                if let Some(owner) = self.sounding.take() {
                    engine.release_owner(owner, false);
                }
            }
            let boundary = if self.sounding.is_some() {
                self.gate
            } else {
                1.
            };
            let frames = ((boundary - self.phase) * period - 1e-8).ceil().max(1.) as usize;
            let end = (at + frames).min(left.len());
            engine.render(&mut left[at..end], &mut right[at..end]);
            self.phase += (end - at) as f64 / period;
            at = end;
        }
    }
    pub fn reap(&mut self, engine: &Engine, mut send: impl FnMut(u8, u8, Option<i32>)) {
        self.roots.retain(|root| {
            let finished = (!root.held() || !self.arp) && !engine.owner_alive(root.owner);
            if finished && !root.mouse {
                send(root.note, root.channel, root.id);
            }
            !finished
        });
        for (n, c, id) in self.ended.drain(..) {
            send(n, c, id);
        }
    }
    pub fn playing(&self) -> bool {
        self.arp && self.roots.iter().any(Root::held)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn notes(h: Harmony, n: u8) -> Vec<u8> {
        let (a, l) = h.tones(n);
        a[..l].to_vec()
    }
    #[test]
    fn voicings_follow_key_and_remain_playable_at_midi_boundaries() {
        let h = Harmony {
            on: true,
            ..Default::default()
        };
        assert_eq!(notes(h, 60), [60, 64, 67]);
        assert_eq!(notes(h, 62), [62, 65, 69]);
        assert_eq!(
            notes(
                Harmony {
                    key: 2,
                    shape: 1,
                    ..h
                },
                62
            ),
            [62, 66, 69, 73]
        );
        assert_eq!(
            notes(
                Harmony {
                    shape: 1,
                    scale: 1,
                    ..h
                },
                60
            ),
            [60, 63, 67, 70]
        );
        assert_eq!(notes(Harmony { inversion: 1, ..h }, 60), [64, 67, 72]);
        for shape in 0..17 {
            for key in 0..12 {
                for inversion in 0..3 {
                    for n in [0, 1, 126, 127] {
                        let chord = notes(
                            Harmony {
                                shape,
                                key,
                                inversion,
                                ..h
                            },
                            n,
                        );
                        assert!(
                            chord.iter().all(|&v| v <= 127)
                                && chord.windows(2).all(|v| v[0] < v[1])
                        );
                    }
                }
            }
        }
    }
    fn lit(e: &Engine) -> Vec<u8> {
        let bits = e.lit_keys();
        (0..128u8)
            .filter(|&n| bits[n as usize / 64] & (1u64 << (n % 64)) != 0)
            .collect()
    }
    fn advance(player: &mut Player, e: &mut Engine, frames: usize) {
        let mut l = vec![0.; frames];
        let mut r = l.clone();
        player.render(e, &mut l, &mut r);
        assert!(l.iter().chain(&r).all(|v| v.is_finite()));
    }
    #[test]
    fn generated_notes_release_by_trigger_even_with_shared_chord_tones_and_sustain() {
        let mut e = Engine::new(8000.);
        let mut p = Player::default();
        let mut patch = crate::dsp::defaults();
        patch[CHORD_ON] = 1.;
        p.configure(&mut e, &patch);
        p.on(&mut e, 60, 1., 0, Some(7), false);
        p.on(&mut e, 64, 1., 0, Some(8), false);
        assert_eq!(lit(&e), [60, 64, 67, 71]);
        p.off(&mut e, 60, 0, Some(7), false, false);
        assert_eq!(lit(&e), [64, 67, 71]);
        p.sustain(&mut e, 0, true);
        p.off(&mut e, 64, 0, Some(8), false, false);
        assert_eq!(lit(&e), [64, 67, 71]);
        patch[CHORD_KEY] = 2.;
        p.configure(&mut e, &patch);
        assert_eq!(lit(&e), [64, 67, 71]);
        p.sustain(&mut e, 0, false);
        assert!(lit(&e).is_empty());
        advance(&mut p, &mut e, 32000);
        let mut ended = Vec::new();
        p.reap(&e, |n, _, id| ended.push((n, id)));
        assert_eq!(ended, [(60, Some(7)), (64, Some(8))]);
    }
    #[test]
    fn arp_uses_sample_clock_across_blocks_and_handles_gate_and_tempo() {
        let mut e = Engine::new(8000.);
        e.tempo = 120.;
        let mut p = Player::default();
        let mut patch = crate::dsp::defaults();
        patch[CHORD_ON] = 1.;
        patch[ARP_ON] = 1.;
        patch[ARP_RATE] = 2.;
        patch[ARP_GATE] = 1.;
        p.configure(&mut e, &patch);
        p.on(&mut e, 60, 1., 0, None, false);
        advance(&mut p, &mut e, 127);
        assert_eq!(lit(&e), [60]);
        advance(&mut p, &mut e, 373);
        advance(&mut p, &mut e, 1);
        assert!(lit(&e).is_empty());
        advance(&mut p, &mut e, 500);
        assert_eq!(lit(&e), [64]);
        e.tempo = 60.;
        advance(&mut p, &mut e, 1999);
        assert_eq!(lit(&e), [67]);
        p.all_off(&mut e, 0, true);
        advance(&mut p, &mut e, 4000);
        assert!(lit(&e).is_empty() && !p.playing());
    }
}
