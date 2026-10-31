//! Native piano-roll harmony workspace. Drafts do not modify notes until Apply.
use crate::{
    app::{Action, App, State, Ui},
    chords, piano,
    playlist::{Cmd, Mark, PItem, Popup},
};
use os_model::Note;
use os_ui::{Color, Font, Rect};

pub const ROOTS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const PRESETS: [(&str, &[u8]); 8] = [
    ("Pop / uplifting", &[0, 4, 5, 3]),
    ("Emotional", &[5, 3, 0, 4]),
    ("House / circular", &[0, 5, 3, 4]),
    ("Jazz turnaround", &[1, 4, 0, 5]),
    ("Classic cadence", &[0, 3, 4, 0]),
    ("Descending", &[0, 6, 5, 4]),
    ("Dreamy", &[0, 2, 3, 1]),
    ("Minor journey", &[0, 5, 2, 6]),
];
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chord {
    pub root: u8,
    pub kind: usize,
    pub degree: u8,
    pub inversion: i8,
    pub beats: u8,
    pub bass: i8,
    pub main: bool,
    pub locked: bool,
    pub voicing: u8,
    pub extension: Option<u8>,
}
impl Default for Chord {
    fn default() -> Self {
        Self {
            root: 0,
            kind: 0,
            degree: 0,
            inversion: 0,
            beats: 4,
            bass: -2,
            main: true,
            locked: false,
            voicing: 0,
            extension: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Harmony {
    pub open: bool,
    pub complex: bool,
    pub tab: u8,
    pub root: u8,
    pub scale: usize,
    pub edit_snap: bool,
    pub midi_snap: bool,
    pub typing_snap: bool,
    pub shade: bool,
    pub stamp: Option<Chord>,
    pub slots: Vec<Chord>,
    pub selected: usize,
    pub octave: i32,
    pub extensions: u8,
    pub leading: bool,
    pub rhythm: u8,
    pub human: u8,
    pub strum: u8,
    pub existing: u8,
    pub adventure: u8,
    pub seed: u64,
    pub start: u32,
    pub undo: Vec<Vec<Chord>>,
    pub redo: Vec<Vec<Chord>>,
    pub copied: Option<Chord>,
    pub saved: Vec<Chord>,
    pub roman: bool,
    pub input: Option<String>,
    pub error: Option<String>,
}
impl Default for Harmony {
    fn default() -> Self {
        Self {
            open: false,
            complex: crate::settings_file::flag("harmony_complex", false),
            tab: 0,
            root: 0,
            scale: 67,
            edit_snap: false,
            midi_snap: false,
            typing_snap: false,
            shade: false,
            stamp: None,
            slots: vec![Chord::default(); 4],
            selected: 0,
            octave: 4,
            extensions: 0,
            leading: true,
            rhythm: 0,
            human: 0,
            strum: 0,
            existing: 0,
            adventure: 0,
            seed: 1,
            start: 0,
            undo: vec![],
            redo: vec![],
            copied: None,
            saved: vec![],
            roman: false,
            input: None,
            error: None,
        }
    }
}
impl Harmony {
    pub fn allows(&self, key: i32) -> bool {
        chords::classes(self.scale, self.root)[key.rem_euclid(12) as usize]
    }
    pub fn snap(&self, key: i32, max: i32) -> u16 {
        let k = key.clamp(0, max);
        (0..=max)
            .filter(|n| self.allows(*n))
            .min_by_key(|n| ((n - k).abs(), *n))
            .unwrap_or(k) as u16
    }
    pub fn scale_notes(&self) -> Vec<i32> {
        (0..12)
            .filter(|i| self.allows(i + i32::from(self.root)))
            .collect()
    }
    pub fn root_of(&self, c: &Chord) -> i32 {
        if c.kind == 0 {
            let ns = self.scale_notes();
            i32::from(self.root) + ns[c.degree as usize % ns.len()]
        } else {
            i32::from(c.root)
        }
    }
    pub fn keys(&self, c: &Chord) -> Vec<u16> {
        let root = self.octave * 12 + self.root_of(c);
        let mut keys = vec![];
        if c.main {
            if c.kind == 0 {
                let ns = self.scale_notes();
                let count = 3 + usize::from(c.extension.unwrap_or(self.extensions));
                let d = c.degree as usize % ns.len();
                for j in 0..count {
                    let i = d + j * 2;
                    keys.push(
                        self.octave * 12
                            + i32::from(self.root)
                            + ns[i % ns.len()]
                            + 12 * (i / ns.len()) as i32,
                    );
                }
            } else {
                keys.push(root);
                for &iv in chords::entry(c.kind).unwrap().intervals {
                    keys.push(keys.last().unwrap() + i32::from(iv));
                }
            }
            for _ in 0..c.inversion.unsigned_abs() {
                keys.sort();
                if c.inversion > 0 {
                    let n = keys.remove(0);
                    keys.push(n + 12);
                } else {
                    let n = keys.pop().unwrap();
                    keys.push(n - 12);
                }
            }
            keys.sort();
            match c.voicing {
                1 => {
                    for i in (1..keys.len()).step_by(2) {
                        keys[i] += 12;
                    }
                }
                2 => {
                    if let Some(n) = keys.first().copied() {
                        keys.push(n + 12);
                    }
                }
                3 => {
                    let more: Vec<_> = keys.iter().map(|n| n + 12).collect();
                    keys.extend(more);
                }
                _ => {}
            }
        }
        if c.bass != -1 {
            keys.push(if c.bass == -2 {
                root - 12
            } else {
                (self.octave - 1) * 12 + i32::from(c.bass)
            });
        }
        keys.retain(|n| (0..=127).contains(n));
        keys.sort();
        keys.dedup();
        keys.into_iter().map(|n| n as u16).collect()
    }
    pub fn name(&self, c: &Chord) -> String {
        let root = self.root_of(c).rem_euclid(12);
        if c.kind != 0 {
            return format!("{} {}", ROOTS[root as usize], chords::label(c.kind));
        }
        let ns = self.scale_notes();
        let d = c.degree as usize % ns.len();
        let interval = |j: usize| ns[(d + j) % ns.len()] + 12 * ((d + j) / ns.len()) as i32 - ns[d];
        let minor = interval(2) == 3;
        let dim = interval(4) == 6;
        let prefix = if self.roman {
            let roman = [
                "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
            ][d];
            if minor {
                roman.to_lowercase()
            } else {
                roman.to_owned()
            }
        } else {
            ROOTS[root as usize].to_owned()
        };
        if c.extension.unwrap_or(self.extensions) == 0 {
            return format!(
                "{prefix}{}",
                if dim {
                    "dim"
                } else if minor && !self.roman {
                    "m"
                } else if interval(4) == 8 {
                    "aug"
                } else {
                    ""
                }
            );
        }
        if ns.len() != 7 {
            return format!(
                "{prefix} ({} tones)",
                3 + c.extension.unwrap_or(self.extensions)
            );
        }
        let extension = ["", "7", "9", "11", "13"][c.extension.unwrap_or(self.extensions) as usize];
        let quality = if dim && interval(6) == 9 {
            "dim"
        } else if minor && interval(6) == 11 {
            "mMaj"
        } else if minor && !self.roman {
            "m"
        } else if !minor && interval(6) == 11 {
            "maj"
        } else {
            ""
        };
        format!(
            "{prefix}{quality}{extension}{}",
            if dim && interval(6) != 9 { "b5" } else { "" }
        )
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.slots.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn generate(&mut self, pattern: usize) {
        self.checkpoint();
        let seq = PRESETS[pattern % PRESETS.len()].1;
        let n = self.scale_notes().len();
        for (i, c) in self.slots.iter_mut().enumerate() {
            if !c.locked {
                c.kind = 0;
                c.degree = seq[i % seq.len()] % n as u8;
                c.inversion = 0;
                c.extension = Some(self.extensions.max(self.adventure));
            }
        }
    }
    pub fn notes(&self, channel: u16, ppq: u16) -> Vec<Note> {
        let mut out = vec![];
        let mut at = self.start;
        let mut prev: Vec<u16> = vec![];
        for (slot, c) in self.slots.iter().enumerate() {
            let duration = u32::from(c.beats) * u32::from(ppq);
            let mut keys = self.keys(c);
            if self.leading && c.inversion == 0 && !c.locked && !prev.is_empty() {
                keys = voice_lead(&keys, &prev, c.bass != -1);
            }
            let step = match self.rhythm {
                1 => u32::from(ppq),
                2 => u32::from(ppq) / 2,
                _ => duration,
            }
            .max(1);
            for offset in (0..duration).step_by(step as usize) {
                for (j, &key) in keys.iter().enumerate() {
                    let delay = (u32::from(self.strum) * j as u32 * u32::from(ppq) / 96)
                        .min(step.saturating_sub(1));
                    let length = if self.rhythm == 0 { step } else { step * 3 / 4 };
                    let variation = if self.human == 0 {
                        0
                    } else {
                        ((slot * 13 + j * 7 + offset as usize) % 17) as i32 - 8
                    };
                    let velocity = (100 + variation * i32::from(self.human)).clamp(1, 127) as u8;
                    out.push(Note {
                        key,
                        length: length.saturating_sub(delay).max(1),
                        velocity,
                        ..Note::step(channel, at + offset + delay)
                    });
                }
            }
            prev = keys;
            at += duration;
        }
        out
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HCmd {
    Mode(bool),
    Tab(u8),
    None,
    Open,
    Close,
    Menu(u8),
    Root(u8),
    Scale(usize),
    EditSnap,
    MidiSnap,
    TypingSnap,
    Shade,
    SnapExisting,
    Stamp,
    ClearStamp,
    Select(usize),
    Generate,
    Analyze,
    Preset(usize),
    Kind(usize),
    Degree(u8),
    ChordRoot(u8),
    Invert(i8),
    Beats(u8),
    Bass(i8),
    Main,
    Lock,
    Voicing(u8),
    Add,
    Delete,
    Swap(i8),
    Copy,
    Paste,
    Passing,
    Undo,
    Redo,
    Octave(i32),
    Extensions(u8),
    Leading,
    Rhythm(u8),
    Human(u8),
    Strum(u8),
    Existing(u8),
    Adventure(u8),
    Names,
    Preview(bool),
    Stop,
    Apply,
    Save,
    Load,
    Text,
    Regenerate,
}
fn item(label: impl Into<String>, cmd: HCmd) -> PItem {
    PItem::new(label.into(), Cmd::Harmony(cmd))
}
pub fn menu(s: &State, which: u8, x: f32, y: f32) -> Popup {
    let h = &s.piano.harmony;
    let c = &h.slots[h.selected.min(h.slots.len() - 1)];
    let items = match which {
        0 => ROOTS
            .iter()
            .enumerate()
            .map(|(i, n)| item(*n, HCmd::Root(i as u8)).mark(Mark::Check(i == h.root as usize)))
            .collect(),
        1 => (67..=88)
            .map(|i| item(chords::label(i), HCmd::Scale(i)).mark(Mark::Check(i == h.scale)))
            .collect(),
        2 => {
            let mut v = vec![
                item("Single notes (stamp off)", HCmd::ClearStamp),
                item("Diatonic chord from chosen key", HCmd::Kind(0)),
            ];
            for (name, a, b) in [
                ("Triads / suspended", 1, 9),
                ("Sixths", 10, 14),
                ("Sevenths", 15, 39),
                ("Ninths", 40, 54),
                ("Elevenths / thirteenths", 55, 66),
            ] {
                v.push(PItem::sub(
                    name,
                    (a..=b)
                        .map(|i| item(chords::entry(i).unwrap().name, HCmd::Kind(i)))
                        .collect(),
                ));
            }
            v
        }
        3 => PRESETS
            .iter()
            .enumerate()
            .map(|(i, (name, _))| item(*name, HCmd::Preset(i)))
            .collect(),
        4 => {
            let mut v = vec![
                item("Off", HCmd::Bass(-1)),
                item("Follow chord root", HCmd::Bass(-2)),
            ];
            v.extend(
                ROOTS
                    .iter()
                    .enumerate()
                    .map(|(i, n)| item(*n, HCmd::Bass(i as i8))),
            );
            v
        }
        5 => ROOTS
            .iter()
            .enumerate()
            .map(|(i, n)| item(*n, HCmd::ChordRoot(i as u8)))
            .collect(),
        6 => (0..h.scale_notes().len())
            .map(|i| item(format!("Degree {}", i + 1), HCmd::Degree(i as u8)))
            .collect(),
        7 => [1, 2, 3, 4, 6, 8, 12, 16]
            .into_iter()
            .map(|n| item(format!("{n} beats"), HCmd::Beats(n)))
            .collect(),
        8 => ["Close", "Open", "Octave", "Stacked"]
            .iter()
            .enumerate()
            .map(|(i, n)| item(*n, HCmd::Voicing(i as u8)).mark(Mark::Check(c.voicing == i as u8)))
            .collect(),
        9 => [
            "Keep existing notes",
            "Replace notes in this range",
            "Replace clashing pitches",
            "Snap existing notes to chords",
        ]
        .iter()
        .enumerate()
        .map(|(i, n)| item(*n, HCmd::Existing(i as u8)))
        .collect(),
        11 => vec![
            item("Undo chord edit", HCmd::Undo),
            item("Redo chord edit", HCmd::Redo),
            item("Save this draft", HCmd::Save),
            item("Load saved draft", HCmd::Load),
            item("Type a progression...", HCmd::Text),
            item("Show Roman numerals", HCmd::Names),
        ],
        10 => vec![
            item("Open chord workspace", HCmd::Open),
            item(format!("Key: {}", ROOTS[h.root as usize]), HCmd::Menu(0)),
            item(format!("Scale: {}", chords::label(h.scale)), HCmd::Menu(1)),
            item("Snap drawn / moved notes", HCmd::EditSnap).mark(Mark::Check(h.edit_snap)),
            item("Snap MIDI keyboard", HCmd::MidiSnap).mark(Mark::Check(h.midi_snap)),
            item("Snap typing keyboard", HCmd::TypingSnap).mark(Mark::Check(h.typing_snap)),
            item("Highlight scale", HCmd::Shade).mark(Mark::Check(h.shade)),
            item("Snap selected notes now", HCmd::SnapExisting),
        ],
        _ => vec![],
    };
    Popup {
        x,
        y,
        items,
        sub: None,
        scroll: [0; 2],
    }
}
pub fn run(app: &mut App, cmd: HCmd) {
    use HCmd::*;
    if let Menu(which) = cmd {
        app.state.playlist.popup = Some(menu(&app.state, which, app.mouse.0, app.mouse.1));
        app.invalidate();
        return;
    }
    if matches!(cmd, Preview(_) | Apply | SnapExisting) {
        if cmd == SnapExisting {
            piano::snapshot(&mut app.state);
            let h = app.state.piano.harmony.clone();
            let idx = piano::targets(&app.state);
            for i in idx {
                if let Some(n) = piano::pattern_mut(&mut app.state).notes.get_mut(i) {
                    n.key = h.snap(i32::from(n.key), 131);
                }
            }
            piano::finish_edit(&mut app.state);
        } else {
            let Some(ch) = piano::channel(&app.state) else {
                return;
            };
            let mut h = app.state.piano.harmony.clone();
            if cmd == Preview(true) {
                h.slots = vec![h.slots[h.selected].clone()];
            }
            let notes = h.notes(ch, app.state.project.ppq);
            if matches!(cmd, Preview(_)) {
                let mut p = crate::rack::play_project(&app.state);
                let mut notes = notes;
                for n in &mut notes {
                    n.position -= h.start;
                }
                p.patterns = vec![os_model::Pattern {
                    number: 1,
                    notes,
                    ..Default::default()
                }];
                p.arrangements.clear();
                app.sync_live();
                app.transport.audition(p);
            } else {
                piano::snapshot_as(&mut app.state, "Generate chord progression");
                let start = h.start;
                let end = notes
                    .iter()
                    .map(|n| n.position + n.length)
                    .max()
                    .unwrap_or(start);
                let p = piano::pattern_mut(&mut app.state);
                if h.existing == 1 {
                    p.notes.retain(|n| {
                        n.channel != ch || n.position >= end || n.position + n.length <= start
                    });
                }
                if h.existing == 2 {
                    p.notes.retain(|n| {
                        n.channel != ch
                            || !notes.iter().any(|g| {
                                g.key == n.key
                                    && n.position < g.position + g.length
                                    && g.position < n.position + n.length
                            })
                    });
                }
                if h.existing == 3 {
                    for n in p
                        .notes
                        .iter_mut()
                        .filter(|n| n.channel == ch && n.position >= start && n.position < end)
                    {
                        if let Some(k) = notes
                            .iter()
                            .filter(|g| {
                                g.position <= n.position && n.position < g.position + g.length
                            })
                            .map(|g| g.key)
                            .min_by_key(|k| (*k as i32 - n.key as i32).abs())
                        {
                            n.key = k;
                        }
                    }
                }
                p.notes.extend(notes);
                piano::finish_edit(&mut app.state);
                app.state.piano.harmony.open = false;
                app.transport.stop_preview();
            }
        }
        app.invalidate();
        return;
    }
    if cmd == Analyze {
        let h = &app.state.piano.harmony;
        let ch = piano::channel(&app.state);
        let ns = piano::notes(&app.state).to_vec();
        let ppq = u32::from(app.state.project.ppq);
        let mut at = h.start;
        let mut draft = h.clone();
        draft.checkpoint();
        for i in 0..draft.slots.len() {
            let len = u32::from(draft.slots[i].beats) * ppq;
            if !draft.slots[i].locked {
                let best = (0..draft.scale_notes().len())
                    .max_by_key(|d| {
                        let mut c = draft.slots[i].clone();
                        c.kind = 0;
                        c.degree = *d as u8;
                        let keys = draft.keys(&c);
                        ns.iter()
                            .filter(|n| {
                                Some(n.channel) == ch
                                    && n.position < at + len
                                    && n.position + n.length > at
                            })
                            .map(|n| {
                                if keys.iter().any(|k| k % 12 == n.key % 12) {
                                    3
                                } else {
                                    -1
                                }
                            })
                            .sum::<i32>()
                    })
                    .unwrap_or(0);
                draft.slots[i].kind = 0;
                draft.slots[i].degree = best as u8;
            }
            at += len;
        }
        app.state.piano.harmony = draft;
        app.invalidate();
        return;
    }
    let start = app
        .state
        .piano
        .range
        .map(|r| r.0)
        .unwrap_or(app.state.piano.marker);
    let h = &mut app.state.piano.harmony;
    let i = h.selected.min(h.slots.len() - 1);
    if matches!(
        cmd,
        Kind(_)
            | Degree(_)
            | ChordRoot(_)
            | Invert(_)
            | Beats(_)
            | Bass(_)
            | Main
            | Lock
            | Voicing(_)
            | Add
            | Delete
            | Swap(_)
            | Paste
            | Passing
    ) {
        h.checkpoint();
    }
    match cmd {
        Mode(on) => {
            h.complex = on;
            crate::settings_file::set_flag("harmony_complex", on);
        }
        Tab(n) => h.tab = n.min(2),
        Open => {
            if h.undo.is_empty() && h.slots.iter().all(|c| *c == Chord::default()) {
                h.generate(0);
            }
            h.open = true;
            h.start = start;
        }
        Close => {
            h.open = false;
            app.transport.stop_preview();
        }
        Root(r) => {
            h.root = r;
            h.shade = true;
        }
        Scale(s) => {
            h.scale = s;
            h.shade = true;
        }
        EditSnap => {
            h.edit_snap = !h.edit_snap;
            h.shade = true;
        }
        MidiSnap => h.midi_snap = !h.midi_snap,
        TypingSnap => h.typing_snap = !h.typing_snap,
        Shade => h.shade = !h.shade,
        Stamp => {
            h.stamp = Some(h.slots[i].clone());
            h.open = false;
            app.state.piano.tool = piano::Tool::Draw;
        }
        ClearStamp => h.stamp = Option::None,
        Text => {
            h.input = Some(String::new());
            h.error = Option::None;
        }
        Stop => app.transport.stop_preview(),
        Regenerate => {
            h.checkpoint();
            if !h.slots[i].locked {
                h.slots[i].degree = (h.slots[i].degree + 1) % h.scale_notes().len() as u8;
                h.slots[i].kind = 0;
            }
        }
        Select(n) => h.selected = n.min(h.slots.len() - 1),
        Generate => {
            h.seed = h.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let p = (h.seed >> 32) as usize;
            h.generate(p);
        }
        Preset(p) => h.generate(p),
        Kind(k) => {
            h.slots[i].kind = k;
            if !h.open {
                h.stamp = Some(h.slots[i].clone());
                app.state.piano.tool = piano::Tool::Draw;
            }
        }
        Degree(d) => {
            h.slots[i].degree = d;
            h.slots[i].kind = 0;
        }
        ChordRoot(r) => {
            h.slots[i].root = r;
            if h.slots[i].kind == 0 {
                h.slots[i].kind = 1;
            }
        }
        Invert(d) => h.slots[i].inversion = (h.slots[i].inversion + d).clamp(-4, 4),
        Beats(b) => h.slots[i].beats = b,
        Bass(b) => h.slots[i].bass = b,
        Main => h.slots[i].main = !h.slots[i].main,
        Lock => h.slots[i].locked = !h.slots[i].locked,
        Voicing(v) => h.slots[i].voicing = v,
        Add => {
            if h.slots.len() < 32 {
                h.slots.insert(i + 1, Chord::default());
                h.selected = i + 1;
            }
        }
        Delete => {
            if h.slots.len() > 1 {
                h.slots.remove(i);
                h.selected = i.min(h.slots.len() - 1);
            }
        }
        Swap(d) => {
            let j = (i as i32 + i32::from(d)).clamp(0, h.slots.len() as i32 - 1) as usize;
            h.slots.swap(i, j);
            h.selected = j;
        }
        Copy => h.copied = Some(h.slots[i].clone()),
        Paste => {
            if let Some(c) = h.copied.clone() {
                h.slots[i] = c;
            }
        }
        Passing => {
            if h.slots.len() < 32 && h.slots[i].beats >= 2 {
                let mut c = h.slots[i].clone();
                c.beats /= 2;
                h.slots[i].beats -= c.beats;
                c.degree = (c.degree + 4) % h.scale_notes().len() as u8;
                c.kind = 0;
                c.locked = false;
                h.slots.insert(i, c);
            }
        }
        Undo => {
            if let Some(prev) = h.undo.pop() {
                h.redo.push(h.slots.clone());
                h.slots = prev;
                h.selected = h.selected.min(h.slots.len() - 1);
            }
        }
        Redo => {
            if let Some(next) = h.redo.pop() {
                h.undo.push(h.slots.clone());
                h.slots = next;
                h.selected = h.selected.min(h.slots.len() - 1);
            }
        }
        Octave(d) => h.octave = (h.octave + d).clamp(2, 7),
        Extensions(n) => {
            h.extensions = n;
            for c in &mut h.slots {
                c.extension = Option::None;
            }
        }
        Leading => h.leading = !h.leading,
        Rhythm(n) => h.rhythm = n,
        Human(n) => h.human = n,
        Strum(n) => h.strum = n,
        Existing(n) => h.existing = n,
        Adventure(n) => h.adventure = n,
        Names => h.roman = !h.roman,
        Save => {
            h.saved = h.slots.clone();
            if let Ok(json) = serde_json::to_string(&h.saved) {
                crate::settings_file::write("harmony_saved_progression", &json);
            }
        }
        Load => {
            let saved = crate::settings_file::read(|s| {
                s.get("harmony_saved_progression")
                    .and_then(|v| serde_json::from_str::<Vec<Chord>>(v).ok())
            });
            if let Some(saved) = saved.filter(|v| {
                !v.is_empty()
                    && v.len() <= 32
                    && v.iter().all(|c| {
                        c.extension.is_none_or(|n| n <= 4)
                            && c.kind < 67
                            && c.root < 12
                            && c.degree < 12
                            && c.beats > 0
                            && c.beats <= 16
                            && (-2..12).contains(&c.bass)
                            && c.voicing < 4
                            && (-4..=4).contains(&c.inversion)
                    })
            }) {
                h.checkpoint();
                h.slots = saved;
                h.selected = 0;
            }
        }
        _ => {}
    }
    let h = &app.state.piano.harmony;
    let config = (
        h.root,
        h.scale as u8,
        (h.edit_snap as u8)
            | ((h.midi_snap as u8) << 1)
            | ((h.typing_snap as u8) << 2)
            | ((h.shade as u8) << 3),
    );
    if app.state.project.harmony != Some(config) {
        app.state.project.harmony = Some(config);
        app.state.piano.dirty = true;
    }
    app.invalidate();
}
fn button(ui: &mut Ui, r: Rect, label: &str, cmd: HCmd, on: bool) {
    let hover = ui.hit(r, Action::Harmony(cmd), label.to_owned());
    ui.c.fill_rrect(
        r,
        4.,
        if on {
            Color::rgb(0x665b2f)
        } else if hover {
            ui.t.button_hi
        } else {
            Color::rgb(0x353b40)
        },
    );
    ui.c.stroke_rrect(r, 4., 1., Color::rgb(0x59636b));
    ui.c.text_center(Font::new(13), r, ui.t.text, label);
}
pub fn paint(ui: &mut Ui, s: &State, r: Rect) {
    let area = Rect::new(r.x + 8., r.y + 26., r.w - 16., r.h - 34.);
    let k = (area.w / 920.).min(area.h / 650.).min(1.).max(0.1);
    ui.scaled(ui.c.scale * k, |ui, _| {
        panel(ui, s, Rect::new(area.x / k, area.y / k, 920., 650.))
    });
}
fn label(ui: &mut Ui, x: f32, y: f32, text: &str) {
    ui.c.text(Font::new(12), x, y, Color::rgb(0x9daeb8), text);
}
fn section(ui: &mut Ui, r: Rect, title: &str, subtitle: &str) {
    ui.c.fill_rrect(r, 9., Color::rgb(0x272f35));
    ui.c.stroke_rrect(r, 9., 1., Color::rgb(0x3c4850));
    ui.c.text(
        Font::new(17),
        r.x + 16.,
        r.y + 12.,
        Color::rgb(0xe5ebee),
        title,
    );
    label(ui, r.x + 16., r.y + 36., subtitle);
}
fn field(ui: &mut Ui, r: Rect, title: &str, value: &str, cmd: HCmd, on: bool) {
    label(ui, r.x, r.y, title);
    button(
        ui,
        Rect::new(r.x, r.y + 19., r.w, r.h - 19.),
        value,
        cmd,
        on,
    );
}
fn keyboard(ui: &mut Ui, r: Rect, keys: &[u16]) {
    let first = keys.first().copied().unwrap_or(48) / 12 * 12;
    let whites = [0, 2, 4, 5, 7, 9, 11];
    let w = r.w / 14.;
    for i in 0..14 {
        let pitch = first + i / 7 * 12 + whites[(i % 7) as usize];
        let on = keys.contains(&pitch);
        let b = Rect::new(r.x + f32::from(i) * w, r.y, w - 1., r.h);
        ui.c.fill_rrect(
            b,
            2.,
            if on {
                Color::rgb(0xd7c080)
            } else {
                Color::rgb(0xc7ced0)
            },
        );
    }
    for octave in 0..2 {
        for (pc, after) in [(1, 0), (3, 1), (6, 3), (8, 4), (10, 5)] {
            let pitch = first + octave * 12 + pc;
            let b = Rect::new(
                r.x + (f32::from(octave * 7 + after) + 0.68) * w,
                r.y,
                w * 0.62,
                r.h * 0.62,
            );
            ui.c.fill_rrect(
                b,
                2.,
                if keys.contains(&pitch) {
                    Color::rgb(0xa68d4c)
                } else {
                    Color::rgb(0x10181d)
                },
            );
        }
    }
}
fn panel(ui: &mut Ui, s: &State, r: Rect) {
    use HCmd::*;
    let h = &s.piano.harmony;
    let c = &h.slots[h.selected];
    let x = r.x;
    let y = r.y;
    ui.hit(r, Action::Harmony(None), "Chord workspace");
    ui.c.fill_rrect(r, 12., Color::rgb(0x1c242a));
    ui.c.text(
        Font::new(23),
        x + 20.,
        y + 16.,
        Color::rgb(0xe8d59e),
        "Chord workspace",
    );
    label(
        ui,
        x + 20.,
        y + 44.,
        "Choose a key. Shape a progression. Hear it, then add it.",
    );
    button(
        ui,
        Rect::new(x + 575., y + 20., 96., 32.),
        "Simple",
        Mode(false),
        !h.complex,
    );
    button(
        ui,
        Rect::new(x + 673., y + 20., 96., 32.),
        "Complex",
        Mode(true),
        h.complex,
    );
    button(
        ui,
        Rect::new(x + 794., y + 20., 106., 32.),
        "Close",
        Close,
        false,
    );
    let key_r = Rect::new(x + 16., y + 78., 220., 262.);
    section(
        ui,
        key_r,
        "1   Key & scale",
        "The musical home for your chords",
    );
    field(
        ui,
        Rect::new(x + 32., y + 141., 188., 49.),
        "KEY",
        ROOTS[h.root as usize],
        Menu(0),
        false,
    );
    field(
        ui,
        Rect::new(x + 32., y + 202., 188., 49.),
        "SCALE",
        chords::label(h.scale),
        Menu(1),
        false,
    );
    button(
        ui,
        Rect::new(x + 32., y + 271., 188., 34.),
        if h.midi_snap {
            "Keyboard in key: ON"
        } else {
            "Keyboard in key: OFF"
        },
        MidiSnap,
        h.midi_snap,
    );
    label(ui, x + 32., y + 313., "Maps incoming MIDI to this scale");
    let pr = Rect::new(x + 250., y + 78., 654., 262.);
    section(
        ui,
        pr,
        "2   Your progression",
        "Click a chord to edit it below",
    );
    button(
        ui,
        Rect::new(x + 808., y + 92., 80., 27.),
        "Draft...",
        Menu(11),
        false,
    );
    let page = h.selected / 8;
    for j in 0..8 {
        let i = page * 8 + j;
        if i >= h.slots.len() {
            break;
        }
        let c = &h.slots[i];
        let b = Rect::new(
            x + 266. + (j % 4) as f32 * 157.,
            y + 142. + (j / 4) as f32 * 63.,
            148.,
            55.,
        );
        let hover = ui.hit(b, Action::Harmony(Select(i)), "Select this chord");
        ui.c.fill_rrect(
            b,
            6.,
            if i == h.selected {
                Color::rgb(0x554c32)
            } else if hover {
                Color::rgb(0x404b54)
            } else {
                Color::rgb(0x303b43)
            },
        );
        ui.c.stroke_rrect(
            b,
            6.,
            if i == h.selected { 2. } else { 1. },
            if i == h.selected {
                Color::rgb(0xd7bc70)
            } else {
                Color::rgb(0x4b5963)
            },
        );
        ui.c.text_clip(
            Font::new(20),
            b.x + 12.,
            b.y + 5.,
            126.,
            ui.t.text,
            &h.name(c),
        );
        label(
            ui,
            b.x + 12.,
            b.y + 33.,
            &format!(
                "{} beats{}",
                c.beats,
                if c.locked { "  /  locked" } else { "" }
            ),
        );
    }
    button(
        ui,
        Rect::new(x + 266., y + 279., 111., 32.),
        "New idea",
        Generate,
        false,
    );
    button(
        ui,
        Rect::new(x + 385., y + 279., 125., 32.),
        "Choose preset...",
        Menu(3),
        false,
    );
    button(
        ui,
        Rect::new(x + 518., y + 279., 96., 32.),
        "+ Chord",
        Add,
        false,
    );
    button(
        ui,
        Rect::new(x + 622., y + 279., 88., 32.),
        "Remove",
        Delete,
        false,
    );
    button(
        ui,
        Rect::new(x + 764., y + 279., 55., 32.),
        "<",
        Select(h.selected.saturating_sub(8)),
        false,
    );
    button(
        ui,
        Rect::new(x + 827., y + 279., 55., 32.),
        ">",
        Select((h.selected + 8).min(h.slots.len() - 1)),
        false,
    );
    label(
        ui,
        x + 266.,
        y + 316.,
        &format!(
            "{} chords  /  {} beats total",
            h.slots.len(),
            h.slots.iter().map(|c| u32::from(c.beats)).sum::<u32>()
        ),
    );
    let detail = Rect::new(x + 16., y + 354., 888., 196.);
    section(ui, detail, "3   Selected chord", "");
    ui.c.text_clip(
        Font::new(28),
        x + 32.,
        y + 400.,
        195.,
        ui.t.text,
        &h.name(c),
    );
    keyboard(ui, Rect::new(x + 32., y + 441., 190., 54.), &h.keys(c));
    button(
        ui,
        Rect::new(x + 32., y + 506., 190., 29.),
        "Hear this chord",
        Preview(true),
        false,
    );
    if !h.complex {
        label(
            ui,
            x + 254.,
            y + 375.,
            "Change the sound without needing music theory",
        );
        field(
            ui,
            Rect::new(x + 254., y + 405., 235., 51.),
            "CHORD TYPE",
            if c.kind == 0 {
                "Follow the scale"
            } else {
                chords::label(c.kind)
            },
            Menu(2),
            false,
        );
        field(
            ui,
            Rect::new(x + 507., y + 405., 160., 51.),
            "LENGTH",
            &format!("{} beats", c.beats),
            Menu(7),
            false,
        );
        button(
            ui,
            Rect::new(x + 684., y + 424., 199., 32.),
            if c.locked {
                "Keep this chord: ON"
            } else {
                "Keep this chord: OFF"
            },
            Lock,
            c.locked,
        );
        button(
            ui,
            Rect::new(x + 254., y + 484., 160., 35.),
            "Try an alternative",
            Regenerate,
            false,
        );
        button(
            ui,
            Rect::new(x + 430., y + 484., 144., 35.),
            "Move earlier",
            Swap(-1),
            false,
        );
        button(
            ui,
            Rect::new(x + 590., y + 484., 144., 35.),
            "Move later",
            Swap(1),
            false,
        );
        button(
            ui,
            Rect::new(x + 750., y + 484., 133., 35.),
            "Draw this chord",
            Stamp,
            false,
        );
    } else {
        for (i, name) in ["Chord detail", "Performance", "Input & notes"]
            .iter()
            .enumerate()
        {
            button(
                ui,
                Rect::new(x + 254. + i as f32 * 212., y + 366., 204., 27.),
                name,
                Tab(i as u8),
                h.tab == i as u8,
            );
        }
        let f = |ui: &mut Ui, col: usize, row: usize, title: &str, value: &str, cmd, on| {
            field(
                ui,
                Rect::new(
                    x + 254. + col as f32 * 159.,
                    y + 404. + row as f32 * 69.,
                    150.,
                    51.,
                ),
                title,
                value,
                cmd,
                on,
            )
        };
        match h.tab {
            0 => {
                f(
                    ui,
                    0,
                    0,
                    "TYPE",
                    if c.kind == 0 {
                        "Diatonic"
                    } else {
                        chords::label(c.kind)
                    },
                    Menu(2),
                    false,
                );
                f(
                    ui,
                    1,
                    0,
                    "ROOT / DEGREE",
                    &format!(
                        "{} / degree {}",
                        ROOTS[h.root_of(c).rem_euclid(12) as usize],
                        c.degree + 1
                    ),
                    if c.kind == 0 { Menu(6) } else { Menu(5) },
                    false,
                );
                f(
                    ui,
                    2,
                    0,
                    "LENGTH",
                    &format!("{} beats", c.beats),
                    Menu(7),
                    false,
                );
                f(
                    ui,
                    3,
                    0,
                    "VOICING",
                    ["Close", "Open", "Octave", "Stacked"][c.voicing as usize],
                    Menu(8),
                    false,
                );
                f(
                    ui,
                    0,
                    1,
                    "INVERSION",
                    &format!("{:+}  /  next", c.inversion),
                    Invert(if c.inversion >= 3 { -6 } else { 1 }),
                    false,
                );
                f(
                    ui,
                    1,
                    1,
                    "BASS",
                    if c.bass == -1 {
                        "Off"
                    } else if c.bass == -2 {
                        "Chord root"
                    } else {
                        ROOTS[c.bass as usize]
                    },
                    Menu(4),
                    false,
                );
                f(
                    ui,
                    2,
                    1,
                    "MAIN NOTES",
                    if c.main { "On" } else { "Off" },
                    Main,
                    c.main,
                );
                f(
                    ui,
                    3,
                    1,
                    "LOCK CHORD",
                    if c.locked { "Locked" } else { "Unlocked" },
                    Lock,
                    c.locked,
                );
            }
            1 => {
                f(
                    ui,
                    0,
                    0,
                    "EXTENSIONS",
                    &format!("{} extra tones", c.extension.unwrap_or(h.extensions)),
                    Extensions((c.extension.unwrap_or(h.extensions) + 1) % 5),
                    false,
                );
                f(
                    ui,
                    1,
                    0,
                    "RHYTHM",
                    ["Sustain", "Quarter notes", "Eighth notes"][h.rhythm as usize],
                    Rhythm((h.rhythm + 1) % 3),
                    false,
                );
                f(
                    ui,
                    2,
                    0,
                    "HUMANIZE",
                    ["Off", "Subtle", "Medium", "Strong"][h.human as usize],
                    Human((h.human + 1) % 4),
                    false,
                );
                f(
                    ui,
                    3,
                    0,
                    "STRUM",
                    &format!("{}", h.strum),
                    Strum((h.strum + 3) % 15),
                    false,
                );
                f(
                    ui,
                    0,
                    1,
                    "OCTAVE",
                    &format!("{} / next", h.octave),
                    Octave(if h.octave == 7 { -5 } else { 1 }),
                    false,
                );
                f(
                    ui,
                    1,
                    1,
                    "SMOOTH VOICING",
                    if h.leading { "On" } else { "Off" },
                    Leading,
                    h.leading,
                );
                f(
                    ui,
                    2,
                    1,
                    "GENERATION",
                    ["Simple", "7ths", "9ths", "11ths", "13ths"][h.adventure as usize],
                    Adventure((h.adventure + 1) % 5),
                    false,
                );
                f(ui, 3, 1, "MATCH MELODY", "Analyze notes", Analyze, false);
            }
            _ => {
                f(
                    ui,
                    0,
                    0,
                    "DRAWING IN KEY",
                    if h.edit_snap { "On" } else { "Off" },
                    EditSnap,
                    h.edit_snap,
                );
                f(
                    ui,
                    1,
                    0,
                    "TYPING IN KEY",
                    if h.typing_snap { "On" } else { "Off" },
                    TypingSnap,
                    h.typing_snap,
                );
                f(
                    ui,
                    2,
                    0,
                    "SCALE SHADING",
                    if h.shade { "On" } else { "Off" },
                    Shade,
                    h.shade,
                );
                f(
                    ui,
                    3,
                    0,
                    "FIX EXISTING NOTES",
                    "Snap to scale",
                    SnapExisting,
                    false,
                );
                f(
                    ui,
                    0,
                    1,
                    "WHEN INSERTING",
                    [
                        "Keep notes",
                        "Replace range",
                        "Replace clashes",
                        "Snap to chords",
                    ][h.existing as usize],
                    Menu(9),
                    false,
                );
                f(ui, 1, 1, "PASSING CHORD", "Add before", Passing, false);
                f(ui, 2, 1, "CHORD CLIPBOARD", "Copy chord", Copy, false);
                f(ui, 3, 1, "CHORD CLIPBOARD", "Paste chord", Paste, false);
            }
        }
    }
    let footer = Rect::new(x + 16., y + 564., 888., 70.);
    ui.c.fill_rrect(footer, 8., Color::rgb(0x121b21));
    button(
        ui,
        Rect::new(x + 32., y + 580., 190., 37.),
        "Play progression",
        Preview(false),
        true,
    );
    button(
        ui,
        Rect::new(x + 232., y + 580., 75., 37.),
        "Stop",
        Stop,
        false,
    );
    let channel = s
        .project
        .channels
        .get(s.selected_channel)
        .and_then(|c| c.name.as_deref())
        .unwrap_or("Selected instrument");
    label(ui, x + 326., y + 577., "PREVIEW INSTRUMENT");
    ui.c.text_clip(Font::new(15), x + 326., y + 598., 305., ui.t.text, channel);
    button(
        ui,
        Rect::new(x + 658., y + 580., 226., 37.),
        "Insert into piano roll",
        Apply,
        true,
    );
    if let Some(text) = &h.input {
        ui.hit(r, Action::Harmony(None), "Enter accepts; Escape cancels");
        let box_r = Rect::new(x + 50., y + 200., 820., 200.);
        section(
            ui,
            box_r,
            "Type your progression",
            "Enter to use it. Escape to cancel.",
        );
        ui.c.text_clip(
            Font::new(18),
            x + 70.,
            y + 280.,
            780.,
            ui.t.text,
            &format!("{text}|"),
        );
        ui.c.text_clip(
            Font::new(13),
            x + 70.,
            y + 350.,
            780.,
            ui.t.text_dim,
            h.error.as_deref().unwrap_or(
                "Try Cmaj7 Am7 Dm7 G7, or I vi IV V. Use Am/E:8 for a bass note and length.",
            ),
        );
    }
}

pub fn stamp(s: &mut State, ch: u16, tick: u32, key: i32) {
    let mut h = s.piano.harmony.clone();
    let Some(mut c) = h.stamp.clone() else { return };
    let key = if h.edit_snap {
        i32::from(h.snap(key, 127))
    } else {
        key.clamp(0, 127)
    };
    h.octave = key / 12;
    c.root = (key % 12) as u8;
    if c.kind == 0 {
        let ns = h.scale_notes();
        c.degree = ns
            .iter()
            .enumerate()
            .min_by_key(|(_, n)| {
                ((**n + i32::from(h.root)).rem_euclid(12) - key.rem_euclid(12)).abs()
            })
            .map(|(i, _)| i as u8)
            .unwrap_or(0);
    }
    let keys = h.keys(&c);
    let len = if s.piano.last_len > 0 {
        s.piano.last_len
    } else {
        u32::from(s.project.ppq)
    };
    piano::snapshot_as(s, "Draw chord");
    let mut indices = vec![];
    for key in keys {
        let p = piano::pattern_mut(s);
        indices.push(p.notes.len());
        p.notes.push(Note {
            key,
            length: len,
            ..Note::step(ch, tick)
        });
    }
    s.piano.select(s.pattern, ch, indices);
    s.piano.dirty = true;
}
pub fn parse(text: &str, h: &Harmony) -> Result<Vec<Chord>, String> {
    let mut out: Vec<Chord> = vec![];
    for word in text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '|' || c == '-')
        .filter(|s| !s.is_empty())
    {
        if out.len() >= 32 {
            return Err("Use at most 32 chords".into());
        }
        if word == "." {
            out.push(
                out.last()
                    .cloned()
                    .ok_or("Repeat needs a preceding chord")?,
            );
            continue;
        }
        let (word, beats) = word
            .split_once(':')
            .map_or((word, 4), |(w, b)| (w, b.parse::<u8>().unwrap_or(0)));
        if beats == 0 || beats > 16 {
            return Err("Length must be 1 to 16 beats, e.g. Am:4".into());
        }
        let mut c = Chord {
            beats,
            ..Default::default()
        };
        let roman = ["I", "II", "III", "IV", "V", "VI", "VII"]
            .iter()
            .position(|r| word.eq_ignore_ascii_case(r));
        if let Some(i) = roman {
            c.degree = i as u8 % h.scale_notes().len() as u8;
        } else {
            let (name, bass) = word
                .split_once('/')
                .map_or((word, None), |(a, b)| (a, Some(b)));
            let parse_root = |n: &str| -> Option<(u8, usize)> {
                let root = match n.as_bytes().first()?.to_ascii_uppercase() {
                    b'C' => 0i32,
                    b'D' => 2,
                    b'E' => 4,
                    b'F' => 5,
                    b'G' => 7,
                    b'A' => 9,
                    b'B' => 11,
                    _ => return None,
                };
                let (offset, len) = match n.as_bytes().get(1) {
                    Some(b'#') => (1, 2),
                    Some(b'b') => (-1, 2),
                    _ => (0, 1),
                };
                Some(((root + offset).rem_euclid(12) as u8, len))
            };
            let (root, len) = parse_root(name).ok_or_else(|| format!("Unknown chord: {word}"))?;
            c.root = root;
            let suffix = &name[len..];
            c.kind = match suffix {
                "" | "maj" => 1,
                "m" | "min" => 5,
                "dim" => 6,
                "aug" => 7,
                "sus2" => 2,
                "sus4" => 3,
                "maj7" | "M7" => 27,
                "m7" => 32,
                "7" => 15,
                "9" => 40,
                "maj9" => 47,
                "m9" => 51,
                "add9" => 42,
                "madd9" => 52,
                "6" => 10,
                "m6" => 13,
                "dim7" => 9,
                "m7b5" => 33,
                "11" => 55,
                "m11" => 58,
                "13" => 60,
                "maj13" => 64,
                "m13" => 65,
                _ => chords::ENTRIES[..66]
                    .iter()
                    .position(|e| e.short.eq_ignore_ascii_case(suffix))
                    .map(|i| i + 1)
                    .ok_or_else(|| format!("Unknown chord quality: {suffix}"))?,
            };
            if let Some(b) = bass {
                let (root, len) = parse_root(b).ok_or("Invalid slash bass")?;
                if len != b.len() {
                    return Err("Invalid slash bass".into());
                }
                c.bass = root as i8;
            }
        }
        out.push(c);
    }
    if out.is_empty() {
        Err("Enter chords such as Cmaj7 Am7 Dm7 G7 or I vi IV V".into())
    } else {
        Ok(out)
    }
}
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, _alt: bool) {
    use HCmd::*;
    use winit::keyboard::{Key, NamedKey};
    if app.state.piano.harmony.input.is_some() {
        match key {
            Key::Named(NamedKey::Escape) => app.state.piano.harmony.input = Option::None,
            Key::Named(NamedKey::Backspace) => {
                app.state.piano.harmony.input.as_mut().unwrap().pop();
            }
            Key::Named(NamedKey::Enter) => {
                let h = &mut app.state.piano.harmony;
                match parse(h.input.as_ref().unwrap(), h) {
                    Ok(slots) => {
                        h.checkpoint();
                        h.slots = slots;
                        h.selected = 0;
                        h.input = Option::None;
                        h.error = Option::None;
                    }
                    Err(e) => h.error = Some(e),
                }
            }
            Key::Character(c) if !ctrl => {
                let text = app.state.piano.harmony.input.as_mut().unwrap();
                if text.len() < 512 {
                    text.push_str(c);
                }
            }
            Key::Named(NamedKey::Space) => {
                app.state.piano.harmony.input.as_mut().unwrap().push(' ')
            }
            _ => {}
        }
        app.invalidate();
        return;
    }
    let cmd = match key {
        Key::Named(NamedKey::Escape) => Close,
        Key::Named(NamedKey::Enter) => Apply,
        Key::Named(NamedKey::Space) => Stop,
        Key::Character(c) if ctrl && c.eq_ignore_ascii_case("z") => Undo,
        Key::Character(c) if ctrl && c.eq_ignore_ascii_case("y") => Redo,
        _ => None,
    };
    run(app, cmd);
}

pub fn restore(s: &mut State) {
    if let Some((root, scale, flags)) = s.project.harmony {
        let h = &mut s.piano.harmony;
        h.root = root;
        h.scale = usize::from(scale);
        h.edit_snap = flags & 1 != 0;
        h.midi_snap = flags & 2 != 0;
        h.typing_snap = flags & 4 != 0;
        h.shade = flags & 8 != 0;
    }
}
fn voice_lead(keys: &[u16], prev: &[u16], bass: bool) -> Vec<u16> {
    if keys.is_empty() || prev.is_empty() {
        return keys.to_vec();
    }
    let main = if bass { &keys[1..] } else { keys };
    if main.is_empty() {
        return keys.to_vec();
    }
    let target = if bass && prev.len() > 1 {
        &prev[1..]
    } else {
        prev
    };
    let mut best = main.to_vec();
    let mut cost = i32::MAX;
    for inv in 0..main.len() {
        for oct in [-12, 0, 12] {
            let mut v: Vec<i32> = main
                .iter()
                .enumerate()
                .map(|(i, k)| i32::from(*k) + oct + if i < inv { 12 } else { 0 })
                .collect();
            v.sort();
            if v.iter().any(|k| !(0..=127).contains(k)) {
                continue;
            }
            let score = v
                .iter()
                .enumerate()
                .map(|(i, k)| (k - i32::from(target[i * target.len() / v.len()])).abs())
                .sum::<i32>();
            if score < cost {
                cost = score;
                best = v.into_iter().map(|k| k as u16).collect();
            }
        }
    }
    if bass {
        best.push(keys[0]);
    }
    best.sort();
    best.dedup();
    best
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn harmony_generation_scale_stamp_and_undo() {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        let ch = piano::channel(&app.state).unwrap();
        run(&mut app, HCmd::Root(2));
        run(&mut app, HCmd::Scale(73));
        run(&mut app, HCmd::EditSnap);
        run(&mut app, HCmd::MidiSnap);
        let h = &app.state.piano.harmony;
        for key in 0..128 {
            assert!(h.allows(i32::from(h.snap(key, 127))));
        }
        let parsed = parse("Dm7 Gm7 A7 Dm/F:8", h).unwrap();
        assert_eq!(parsed[3].bass, 5);
        assert_eq!(parsed[3].beats, 8);
        assert!(parse("nonsense", h).is_err());
        app.state.piano.harmony.slots = parsed;
        app.state.piano.harmony.slots[0].locked = true;
        let first = app.state.piano.harmony.slots[0].clone();
        run(&mut app, HCmd::Generate);
        assert_eq!(app.state.piano.harmony.slots[0], first);
        run(&mut app, HCmd::Undo);
        assert_eq!(app.state.piano.harmony.slots[1].kind, 32);
        let before = piano::notes(&app.state).to_vec();
        run(&mut app, HCmd::Apply);
        assert!(piano::notes(&app.state).len() > before.len());
        crate::history::undo(&mut app.state);
        assert_eq!(piano::notes(&app.state), before);
        app.state.piano.harmony.stamp = Some(Chord {
            kind: 1,
            bass: -1,
            ..Default::default()
        });
        stamp(&mut app.state, ch, 0, 60);
        let keys: Vec<_> = piano::notes(&app.state)
            .iter()
            .rev()
            .take(3)
            .map(|n| n.key)
            .collect();
        assert_eq!(keys, vec![67, 64, 60]);
        crate::history::undo(&mut app.state);
        assert_eq!(piano::notes(&app.state), before);
        let file = os_flp::project::create(&app.state.project).unwrap();
        let back = os_flp::project::load(&file).unwrap();
        assert_eq!(back.harmony, app.state.project.harmony);
        run(&mut app, HCmd::Save);
        app.state.piano.harmony.slots = vec![Chord::default()];
        run(&mut app, HCmd::Load);
        assert_eq!(app.state.piano.harmony.slots.len(), 4);
    }
    #[test]
    fn harmony_all_chords_scales_and_note_bounds() {
        for scale in 67..=88 {
            let h = Harmony {
                scale,
                ..Default::default()
            };
            for root in 0..12 {
                for k in 0..128 {
                    let h = Harmony { root, ..h.clone() };
                    assert!(h.allows(i32::from(h.snap(k, 127))));
                }
            }
        }
        for kind in 0..67 {
            for inversion in -4..=4 {
                let h = Harmony::default();
                let c = Chord {
                    kind,
                    inversion,
                    ..Default::default()
                };
                let keys = h.keys(&c);
                assert!(!keys.is_empty());
                assert!(keys.windows(2).all(|k| k[0] < k[1]));
            }
        }
        let mut locked = Harmony::default();
        locked.slots[0].locked = true;
        let keys = locked.keys(&locked.slots[0]);
        locked.adventure = 4;
        locked.generate(3);
        assert_eq!(locked.keys(&locked.slots[0]), keys);
        assert!(locked.keys(&locked.slots[1]).len() > keys.len());
        let h = Harmony::default();
        let ns = h.notes(0, 96);
        assert!(ns.iter().all(|n| n.length > 0 && n.key <= 127));
        assert_eq!(
            voice_lead(&[36, 48, 52, 55], &[36, 48, 52, 55], true),
            vec![36, 48, 52, 55]
        );
    }
}
