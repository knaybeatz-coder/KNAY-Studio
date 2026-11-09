// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Key Match: an audio file dropped on the playlist is checked against the key of the beat already there (the notes
//! of the patterns on the playlist, and the audio clips on it), and the user is asked whether to shift it into that
//! key. The shift is the channel's stretch pitch (event 215 int 25) with a realtime stretch method, so the clip keeps
//! its length and tempo. Relative keys count as the same key (A minor fits C major); the smallest move is taken.
//!
//! Each file's detection is kept for the session (`cache`): drops analyse at once; clip labels ask for the clips of an
//! opened project in a worker thread and repaint once it is done (`take_fresh`). A key set by hand in the clip menu
//! overrides the detection for that file (this session).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use os_model::{Channel, ChannelKind, ClipSource};

use crate::app::{App, State};
use crate::dialog::{spans, Answer, Kind, Msg, Then};
use crate::key_detect::{self as kd, Detected, Key};
use crate::playlist::{self as pl, Cmd, Mark, PItem};

/// What is known of one file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Info {
    pub chroma: [f64; 12],
    pub detected: Option<Detected>,
    /// Set by hand in the clip menu.
    pub manual: Option<Key>,
}

impl Info {
    /// The file's own key: the one set by hand, else a confident detection.
    pub fn key(&self) -> Option<Key> {
        self.manual.or(self.detected.filter(|d| d.confident()).map(|d| d.key))
    }
}

/// Sample path (as the channel stores it) to its analysis; `None` while a worker analyses it.
fn cache() -> &'static Mutex<HashMap<String, Option<Info>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<Info>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

static FRESH: AtomicBool = AtomicBool::new(false);

/// A worker finished an analysis since the last call: the clip labels need a repaint.
pub fn take_fresh() -> bool {
    FRESH.swap(false, Ordering::Relaxed)
}

fn analyse(file: &str) -> Info {
    let (chroma, detected) = match os_engine::sample::load_shared(std::path::Path::new(file)) {
        Ok(s) => {
            let c = kd::chroma(&s.left, &s.right, s.rate);
            (c, kd::key_of(&c))
        }
        Err(_) => ([0.0; 12], None),
    };
    Info { chroma, detected, manual: None }
}

/// The analysis of `path`, made now if it is not known yet (a drop). A manual key already set is kept.
pub fn analyse_now(path: &str) -> Info {
    if let Some(Some(i)) = cache().lock().ok().and_then(|c| c.get(path).copied()) {
        return i;
    }
    let info = analyse(&crate::plugins::resolve_sample(path));
    if let Ok(mut c) = cache().lock() {
        c.insert(path.to_string(), Some(info));
    }
    info
}

/// The analysis of `path` if known; otherwise a worker starts on it and this returns None (no wait while painting).
pub fn info(path: &str) -> Option<Info> {
    let mut c = cache().lock().ok()?;
    match c.get(path) {
        Some(i) => *i,
        None => {
            c.insert(path.to_string(), None);
            drop(c);
            let (key, file) = (path.to_string(), crate::plugins::resolve_sample(path));
            std::thread::spawn(move || {
                let i = analyse(&file);
                if let Ok(mut c) = cache().lock() {
                    let manual = c.get(&key).copied().flatten().and_then(|i| i.manual);
                    c.insert(key, Some(Info { manual, ..i }));
                }
                FRESH.store(true, Ordering::Relaxed);
            });
            None
        }
    }
}

fn set_manual(path: &str, key: Option<Key>) {
    let mut i = analyse_now(path);
    i.manual = key;
    if let Ok(mut c) = cache().lock() {
        c.insert(path.to_string(), Some(i));
    }
}

fn forget(path: &str) {
    if let Ok(mut c) = cache().lock() {
        c.remove(path);
    }
}

/// The audio clip channel behind `src`: its rack row and the channel.
fn audio_channel(s: &State, src: ClipSource) -> Option<(usize, &Channel)> {
    let ClipSource::Channel(ch) = src else { return None };
    let row = s.project.channels.iter().position(|c| c.index == ch)?;
    let c = &s.project.channels[row];
    (c.kind == ChannelKind::AudioClip && c.sample_path.as_deref().is_some_and(|p| !p.is_empty())).then_some((row, c))
}

/// Semitones the channel moves its sample by (its stretch pitch, and its own pitch where that applies).
fn applied(c: &Channel) -> i32 {
    let own = if c.sound.channel_pitch_applies() { f64::from(c.pitch_cents) } else { 0.0 };
    ((c.sound.stretch_cents() + own) / 100.0).round() as i32
}

fn file_name(c: &Channel) -> String {
    c.name.clone().unwrap_or_else(|| {
        let p = c.sample_path.clone().unwrap_or_default();
        p.rsplit(['/', '\\']).next().unwrap_or(&p).to_string()
    })
}

/// The pitch-class profile of the beat on the playlist: the notes of the patterns placed on it (all patterns when
/// none is placed yet), leaving out channels that play fewer than three pitch classes (drums), plus the audio clips
/// on it whose key is known, as they sound now. Channel `except` (the clip just dropped) is left out.
pub fn beat_chroma(s: &State, except: Option<u16>) -> [f64; 12] {
    let clips = pl::clips(s);
    let mut used: Vec<u16> = clips
        .iter()
        .filter(|c| !c.muted)
        .filter_map(|c| if let ClipSource::Pattern(n) = c.source { Some(n) } else { None })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if used.is_empty() {
        used = s.project.patterns.iter().filter(|p| !p.notes.is_empty()).map(|p| p.number).collect();
    }
    let melodic = |ch: u16| s.project.channels.iter().find(|c| c.index == ch).is_some_and(|c| !matches!(c.kind, ChannelKind::AudioClip | ChannelKind::Automation));
    let mut by_channel: HashMap<u16, Vec<(i32, u32, u8)>> = HashMap::new();
    for n in used.iter().filter_map(|&n| s.project.pattern(n)).flat_map(|p| p.notes.iter()) {
        if !n.muted() && melodic(n.channel) {
            by_channel.entry(n.channel).or_default().push((i32::from(n.key), n.length, n.velocity));
        }
    }
    let notes: Vec<(i32, u32, u8)> = by_channel
        .into_values()
        .filter(|ns| ns.iter().map(|n| n.0.rem_euclid(12)).collect::<HashSet<_>>().len() >= 3)
        .flatten()
        .collect();
    let mut total = kd::normalized(&kd::note_chroma(notes));
    let mut seen = HashSet::new();
    for c in clips.iter().filter(|c| !c.muted) {
        let Some((_, ch)) = audio_channel(s, c.source) else { continue };
        if Some(ch.index) == except || !seen.insert(ch.index) {
            continue;
        }
        // Analysed now if needed: a drop or a menu waits for it, so an audio beat counts at once.
        let i = analyse_now(ch.sample_path.as_deref().unwrap_or_default());
        let own = match (i.manual, i.detected) {
            (Some(k), _) => kd::profile(k),
            (None, Some(d)) if d.confident() => i.chroma,
            _ => continue,
        };
        let now = kd::normalized(&kd::rotate(&own, applied(ch)));
        for p in 0..12 {
            total[p] += now[p];
        }
    }
    total
}

/// The beat's key, when it is clear.
pub fn beat_key(s: &State, except: Option<u16>) -> Option<Detected> {
    kd::key_of(&beat_chroma(s, except)).filter(|d| d.confident())
}

fn signed(n: i32) -> String {
    match n {
        0 => "0".into(),
        n if n > 0 => format!("+{n}"),
        n => format!("-{}", -n),
    }
}

/// An audio clip (playlist index `i`) was just dropped: when the beat has a key and the sample one that does not fit
/// it, the user is asked whether to shift it (`Then::KeyMatch`). Otherwise the status line says why nothing happens.
pub fn after_drop(app: &mut App, i: usize) {
    let s = &app.state;
    let Some(clip) = pl::clips(s).get(i).copied() else { return };
    let Some((_, ch)) = audio_channel(s, clip.source) else { return };
    let (index, name, path) = (ch.index, file_name(ch), ch.sample_path.clone().unwrap_or_default());
    let info = analyse_now(&path);
    let Some(beat) = beat_key(s, Some(index)) else {
        if let Some(k) = info.key() {
            app.state.message = Some(format!("Key match: {name} is in {} (no beat with a clear key to match yet)", k.name()));
        }
        return;
    };
    let Some(sample) = info.key() else {
        app.state.message = Some(format!("Key match: {name} has no clear key (drums or noise?), left as it is"));
        return;
    };
    let shift = kd::shift_to(sample, beat.key);
    if shift == 0 {
        app.state.message = Some(format!("Key match: {name} ({}) already fits the beat ({})", sample.name(), beat.key.name()));
        return;
    }
    let to = sample.transposed(shift).name();
    let text = spans(&[
        "The sample ",
        &name,
        " is in ",
        &sample.name(),
        ".\nThe beat on the playlist is in ",
        &beat.key.name(),
        ".\n\nShift the sample ",
        &format!("{} semitones", signed(shift)),
        " (to ",
        &to,
        ") so it fits the beat?\nIts length and tempo stay the same.",
    ]);
    app.show_msg(Msg::new(Kind::Question, "Key match", text, &[Answer::Yes, Answer::No], Then::KeyMatch(index, shift)).labelled(&["Shift", "Keep"]));
}

/// Channel `index` (an audio clip channel) plays its sample shifted by `semitones` from the original (0 = original),
/// one undo step. A sample that only resampled gets the realtime stretcher, so its length stays.
pub fn apply(app: &mut App, index: u16, semitones: i32) -> Option<String> {
    let s = &mut app.state;
    let row = s.project.channels.iter().position(|c| c.index == index)?;
    pl::snapshot_as(s, "Key match");
    let c = &mut s.project.channels[row];
    c.sound.stretch[1] = (semitones * 100) as u32;
    if semitones != 0 && c.sound.stretch_mode() == os_model::stretch_mode::RESAMPLE {
        c.sound.stretch[3] = os_model::stretch_mode::STRETCH_PRO as u32;
    }
    let name = file_name(c);
    let sound = c.sound;
    app.plugins.set_sampler_stretch(row, &sound);
    app.state.piano.dirty = true;
    app.invalidate();
    Some(if semitones == 0 { format!("Key match: {name} back to its original key") } else { format!("Key match: {name} shifted {} semitones", signed(semitones)) })
}

/// The label after an audio clip's name: its key, and the move when it is shifted ("Am", "Am -> Em (-5)").
pub fn clip_label(s: &State, src: ClipSource) -> Option<String> {
    let (_, ch) = audio_channel(s, src)?;
    let key = info(ch.sample_path.as_deref()?)?.key()?;
    let n = applied(ch);
    Some(if n == 0 { key.short() } else { format!("{} -> {} ({})", key.short(), key.transposed(n).short(), signed(n)) })
}

/// The clip menu's Key commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KCmd {
    /// Shift to the beat's key now.
    Match,
    /// Back to the original key.
    Reset,
    /// Analyse the file again (and forget a key set by hand).
    Redetect,
    /// The file's own key, set by hand.
    SetKey(Key),
    /// A shift chosen by hand.
    Shift(i32),
}

/// The Key submenu of audio clip `i`'s menu.
pub fn menu(s: &State, i: usize) -> Option<PItem> {
    let clip = pl::clips(s).get(i)?;
    let (_, ch) = audio_channel(s, clip.source)?;
    let info = info(ch.sample_path.as_deref()?);
    let key = info.and_then(|i| i.key());
    let n = applied(ch);
    let mut items = vec![PItem::head(match key {
        Some(k) if n != 0 => format!("Key: {} -> {}", k.name(), k.transposed(n).name()),
        Some(k) => format!("Key: {}", k.name()),
        None if info.is_none() => "Key: analysing...".into(),
        None => "Key: not clear".into(),
    })];
    let beat = beat_key(s, Some(ch.index));
    let matched = PItem::new(
        match beat {
            Some(b) => format!("Match key to beat ({})", b.key.name()),
            None => "Match key to beat (no clear beat key)".into(),
        },
        Cmd::KeyMatch(i, KCmd::Match),
    );
    items.push(if beat.is_some() && key.is_some() { matched } else { matched.off() });
    let reset = PItem::new("Back to original key", Cmd::KeyMatch(i, KCmd::Reset));
    items.push(if n != 0 { reset } else { reset.off() });
    items.push(PItem::sub(
        "Shift by",
        (-6..=6).filter(|&d| d != 0).map(|d| PItem::new(format!("{} semitones", signed(d)), Cmd::KeyMatch(i, KCmd::Shift(d))).mark(Mark::Radio(n == d))).collect(),
    ));
    items.push(PItem::sep());
    items.push(PItem::new("Detect key again", Cmd::KeyMatch(i, KCmd::Redetect)));
    let set = |minor: bool| -> Vec<PItem> { Key::ALL.iter().filter(|k| k.minor == minor).map(|&k| PItem::new(k.name(), Cmd::KeyMatch(i, KCmd::SetKey(k))).mark(Mark::Radio(key == Some(k)))).collect() };
    items.push(PItem::sub("Sample's key (major)", set(false)));
    items.push(PItem::sub("Sample's key (minor)", set(true)));
    Some(PItem::sub("Key match", items))
}

/// Runs a Key menu command for audio clip `i`.
pub fn run(app: &mut App, i: usize, cmd: KCmd) -> Option<String> {
    let s = &app.state;
    let clip = *pl::clips(s).get(i)?;
    let (_, ch) = audio_channel(s, clip.source)?;
    let (index, path, name) = (ch.index, ch.sample_path.clone()?, file_name(ch));
    match cmd {
        KCmd::Match => {
            let Some(key) = analyse_now(&path).key() else { return Some(format!("Key match: {name} has no clear key; set it in Key match > Sample's key")) };
            let Some(beat) = beat_key(s, Some(index)) else { return Some("Key match: the beat has no clear key yet".into()) };
            apply(app, index, kd::shift_to(key, beat.key))
        }
        KCmd::Reset => apply(app, index, 0),
        KCmd::Shift(n) => apply(app, index, n),
        KCmd::Redetect => {
            forget(&path);
            let i = analyse_now(&path);
            app.invalidate();
            Some(match i.detected {
                Some(d) if d.confident() => format!("Key match: {name} is in {}", d.key.name()),
                Some(d) => format!("Key match: {name} might be in {}, but it is not clear", d.key.name()),
                None => format!("Key match: no pitch found in {name}"),
            })
        }
        KCmd::SetKey(k) => {
            set_manual(&path, Some(k));
            app.invalidate();
            Some(format!("Key match: {name} is in {}", k.name()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialog::{Body, Reply};
    use os_model::Note;

    fn note(position: u32, channel: u16, key: u16) -> Note {
        Note { expression: 0, drive: 0, position, length: 96, channel, key, fine_pitch: 120, release: 64, pan: 64, velocity: 100, mod_x: 128, mod_y: 128, flags: 0x4000, group: 0, unk14: 0, repeat: 0 }
    }

    fn hz(midi: i32) -> f64 {
        440.0 * 2f64.powf(f64::from(midi - 69) / 12.0)
    }

    /// The owner's example: a G major beat on the playlist, an A minor sample dropped from the browser. Key match asks
    /// first; Shift moves it down 5 semitones (to E minor, G major's relative minor) without changing its length.
    #[test]
    fn dropped_sample_is_offered_the_beats_key() {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.playlist.reset();
        // The Pluck channel (index 4) plays G major; the Kick (index 0) one pitch only, which does not count.
        let melody = [55, 59, 62, 67, 69, 71, 72, 74, 76, 78, 79, 62, 67, 71, 74, 55];
        let mut notes: Vec<Note> = melody.iter().enumerate().map(|(i, &k)| note(i as u32 * 96, 4, k)).collect();
        notes.extend((0..16u32).map(|i| note(i * 96, 0, 60)));
        notes.sort_by_key(|n| n.position);
        app.state.project.patterns[0].notes = notes;
        app.state.playlist.source = Some(ClipSource::Pattern(1));
        pl::place(&mut app.state, 0, 0);
        let beat = beat_key(&app.state, None).expect("the beat has a key");
        assert_eq!(beat.key.relative_major_root(), 7, "{beat:?}");

        // An A minor sample: i - iv - V - i.
        let path = std::env::temp_dir().join(format!("knay-key-match-{}.wav", std::process::id()));
        let rate = 44100u32;
        let mut audio = Vec::new();
        for chord in [[57, 60, 64], [62, 65, 69], [64, 68, 71], [57, 60, 64]] {
            for i in 0..rate * 3 / 2 {
                let t = f64::from(i) / f64::from(rate);
                let v = (chord.iter().map(|&m| (2.0 * std::f64::consts::PI * hz(m) * t).sin()).sum::<f64>() / 6.0) as f32;
                audio.push(v);
                audio.push(v);
            }
        }
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(), rate, &audio).unwrap();
        let i = pl::add_audio_clip(&mut app.state, &path.to_string_lossy(), 1, 0).expect("audio clip");
        after_drop(&mut app, i);
        let (index, shift) = match app.state.dialogs.last().map(|d| &d.body) {
            Some(Body::Msg(m)) => match m.then {
                Then::KeyMatch(index, shift) => (index, shift),
                ref other => panic!("{other:?}"),
            },
            _ => panic!("Key match asks first: {:?}", app.state.message),
        };
        assert_eq!(shift, -5);
        app.state.dialogs.pop();
        app.dialog_done(Then::KeyMatch(index, shift), Reply::Button(Answer::Yes));
        let ch = app.state.project.channels.iter().find(|c| c.index == index).unwrap();
        assert_eq!(ch.sound.stretch_cents(), -500.0);
        assert!(os_model::stretch_mode::realtime(ch.sound.stretch_mode()), "length kept");
        let label = clip_label(&app.state, pl::clips(&app.state)[i].source).expect("a label");
        assert!(label.ends_with("(-5)"), "{label}");
        // Back to the original key from the clip menu.
        assert!(menu(&app.state, i).is_some());
        run(&mut app, i, KCmd::Reset);
        assert_eq!(app.state.project.channels.iter().find(|c| c.index == index).unwrap().sound.stretch_cents(), 0.0);
        let _ = std::fs::remove_file(&path);
    }
}
