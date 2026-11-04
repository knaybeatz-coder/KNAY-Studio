//! MPE MIDI decoding shared by native plugin hosts. Member identities never use
//! pitch as their key, so unisons remain independently addressable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    On {
        channel: u8,
        key: u8,
        id: i32,
        velocity: f32,
    },
    Off {
        channel: u8,
        key: u8,
        id: i32,
        velocity: f32,
    },
    Expression {
        id: i32,
        lane: u8,
        value: f32,
    },
}
#[derive(Debug, Clone, Copy)]
struct Member {
    key: u8,
    id: i32,
    up: bool,
    bend: f32,
    pressure: f32,
    slide: f32,
}
impl Default for Member {
    fn default() -> Self {
        Self {
            key: 0,
            id: -1,
            up: false,
            bend: 0.,
            pressure: 0.,
            slide: 0.5,
        }
    }
}
#[derive(Clone)]
pub struct Decoder {
    lower: u8,
    upper: u8,
    members: [Member; 16],
    rpn: [[u8; 2]; 16],
    ranges: [f32; 16],
    pedal: [bool; 16],
    next: i32,
}
impl Default for Decoder {
    fn default() -> Self {
        Self {
            lower: 0,
            upper: 0,
            members: [Member::default(); 16],
            rpn: [[127; 2]; 16],
            ranges: [48.; 16],
            pedal: [false; 16],
            next: 0x40000000,
        }
    }
}
impl Decoder {
    pub fn bend_range(&self, ch: u8) -> f32 {
        self.ranges[usize::from(ch & 15)].max(1.)
    }
    pub fn active(&self) -> bool {
        self.lower > 0 || self.upper > 0
    }
    pub fn manager(&self, ch: u8) -> Option<u8> {
        if ch > 0 && ch <= self.lower {
            Some(0)
        } else if ch < 15 && ch >= 15 - self.upper {
            Some(15)
        } else {
            None
        }
    }
    pub fn config(&self, ch: u8) -> Option<os_model::mpe::Config> {
        let manager = self.manager(ch)?;
        Some(os_model::mpe::Config {
            enabled: true,
            upper: manager == 15,
            members: if manager == 15 {
                self.upper
            } else {
                self.lower
            },
            bend: self.bend_range(ch).round().clamp(1., 96.) as u8,
            ..Default::default()
        })
    }
    pub fn reset(&mut self) {
        self.members = [Member::default(); 16];
        self.pedal = [false; 16];
    }
    fn expr(&self, ch: usize, lane: u8) -> Event {
        let m = self.members[ch];
        let mgr = self.manager(ch as u8).unwrap_or(ch as u8) as usize;
        let g = self.members[mgr];
        let value = match lane {
            0 => m.bend + g.bend,
            1 => (m.pressure + g.pressure).clamp(0., 1.),
            _ => (m.slide + g.slide - 0.5).clamp(0., 1.),
        };
        Event::Expression {
            id: m.id,
            lane,
            value,
        }
    }
    /// Returns None when ordinary MIDI should keep its existing delivery path.
    pub fn receive(&mut self, b: [u8; 3]) -> Option<Vec<Event>> {
        let ch = (b[0] & 15) as usize;
        let (kind, a, v) = (b[0] & 0xf0, b[1] & 127, b[2] & 127);
        let mut out = vec![];
        if kind == 0xb0 {
            match a {
                101 => {
                    self.rpn[ch][0] = v;
                    return self.active().then_some(out);
                }
                100 => {
                    self.rpn[ch][1] = v;
                    return self.active().then_some(out);
                }
                6 if self.rpn[ch] == [0, 6] && (ch == 0 || ch == 15) => {
                    let previous: Vec<_> = (0..16).map(|c| self.manager(c)).collect();
                    let count = v.min(15);
                    if ch == 0 {
                        self.lower = count;
                        self.upper = self.upper.min(14u8.saturating_sub(count));
                    } else {
                        self.upper = count;
                        self.lower = self.lower.min(14u8.saturating_sub(count));
                    }
                    for c in 0..16 {
                        if previous[c] != self.manager(c as u8) {
                            let m = self.members[c];
                            if m.id >= 0 {
                                out.push(Event::Off {
                                    channel: c as u8,
                                    key: m.key,
                                    id: m.id,
                                    velocity: 0.5,
                                });
                            }
                            self.members[c] = Member::default();
                        }
                    }
                    self.ranges[0] = 2.;
                    self.ranges[15] = 2.;
                    return Some(out);
                }
                6 if self.rpn[ch] == [0, 0] => {
                    let range = f32::from(v.min(96));
                    if let Some(manager) = self.manager(ch as u8) {
                        for c in 0..16 {
                            if self.manager(c as u8) == Some(manager) {
                                self.ranges[c] = range;
                            }
                        }
                    } else {
                        self.ranges[ch] = range;
                    }
                    return self.active().then_some(out);
                }
                _ => {}
            }
        }
        let mgr = self.manager(ch as u8);
        let manager = (ch == 0 && self.lower > 0) || (ch == 15 && self.upper > 0);
        if mgr.is_none() && !manager {
            return None;
        }
        let off = |m: Member, c: usize, vel: u8| Event::Off {
            channel: c as u8,
            key: m.key,
            id: m.id,
            velocity: f32::from(vel) / 127.,
        };
        match kind {
            0x90 if v > 0 && mgr.is_some() => {
                let old = self.members[ch];
                if old.id >= 0 {
                    out.push(off(old, ch, 64));
                }
                self.next = self.next.wrapping_add(1) & 0x7fffffff;
                let m = &mut self.members[ch];
                m.id = self.next;
                m.key = a;
                m.up = false;
                out.push(Event::On {
                    channel: ch as u8,
                    key: a,
                    id: m.id,
                    velocity: f32::from(v) / 127.,
                });
                for lane in 0..3 {
                    out.push(self.expr(ch, lane));
                }
            }
            0x80 | 0x90 if mgr.is_some() => {
                let m = &mut self.members[ch];
                if m.id >= 0 && m.key == a {
                    if self.pedal[mgr.unwrap() as usize] {
                        m.up = true;
                    } else {
                        out.push(off(*m, ch, v));
                        m.id = -1;
                    }
                }
            }
            0xa0 if mgr.is_some() && self.members[ch].key != a => {}
            0xe0 | 0xd0 | 0xa0 => {
                let lane = if kind == 0xe0 { 0 } else { 1 };
                let m = &mut self.members[ch];
                if kind == 0xe0 {
                    let raw = i32::from(a) | (i32::from(v) << 7);
                    m.bend = (raw - 8192) as f32 / if raw >= 8192 { 8191. } else { 8192. }
                        * self.ranges[ch];
                } else {
                    m.pressure = f32::from(if kind == 0xa0 { v } else { a }) / 127.;
                }
                if manager {
                    for c in 0..16 {
                        if self.manager(c as u8) == Some(ch as u8) && self.members[c].id >= 0 {
                            out.push(self.expr(c, lane));
                        }
                    }
                } else if m.id >= 0 {
                    out.push(self.expr(ch, lane));
                }
            }
            0xb0 if a == 74 => {
                self.members[ch].slide = f32::from(v) / 127.;
                for c in 0..16 {
                    if self.members[c].id >= 0
                        && (c == ch || manager && self.manager(c as u8) == Some(ch as u8))
                    {
                        out.push(self.expr(c, 2));
                    }
                }
            }
            0xb0 if a == 64 && manager => {
                self.pedal[ch] = v >= 64;
                if v < 64 {
                    for c in 0..16 {
                        if self.manager(c as u8) == Some(ch as u8)
                            && self.members[c].up
                            && self.members[c].id >= 0
                        {
                            out.push(off(self.members[c], c, 64));
                            self.members[c].id = -1;
                        }
                    }
                }
            }
            0xb0 if a == 120 || a == 123 => {
                for c in 0..16 {
                    if self.members[c].id >= 0
                        && (c == ch || manager && self.manager(c as u8) == Some(ch as u8))
                    {
                        out.push(off(self.members[c], c, 64));
                        self.members[c].id = -1;
                    }
                }
            }
            _ => return None,
        }
        Some(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpe_decoder_unisons_sustain_ranges() {
        let mut d = Decoder::default();
        for b in os_model::mpe::Config::default().setup() {
            d.receive(b);
        }
        let a = d.receive([0x91, 60, 100]).unwrap();
        let b = d.receive([0x92, 60, 100]).unwrap();
        let Event::On { id: x, .. } = a[0] else {
            panic!()
        };
        let Event::On { id: y, .. } = b[0] else {
            panic!()
        };
        assert_ne!(x, y);
        assert_eq!(
            d.receive([0xe1, 127, 127]).unwrap(),
            vec![Event::Expression {
                id: x,
                lane: 0,
                value: 48.
            }]
        );
        d.receive([0xb0, 64, 127]);
        assert!(d.receive([0x81, 60, 64]).unwrap().is_empty());
        assert!(matches!(d.receive([0xb0,64,0]).unwrap()[0],Event::Off{id,..}if id==x));
        assert!(matches!(d.receive([0xd2,100,0]).unwrap()[0],Event::Expression{id,..}if id==y));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scheduled {
    pub start: u64,
    pub end: u64,
    pub key: u8,
    pub velocity: u8,
    pub release: u8,
    pub expression: os_model::mpe::Expression,
    pub fine: f32,
}
#[derive(Clone)]
struct Voice {
    note: usize,
    channel: u8,
    last: [i32; 3],
}
#[derive(Clone)]
pub struct Playback {
    /// Restart notes spanning a seek only when the transport requests note chasing.
    pub chase_notes: bool,
    pub(crate) resume_tick: Option<u64>,
    pub config: os_model::mpe::Config,
    pub pitch_offset: f32,
    pub notes: Vec<Scheduled>,
    voices: Vec<Voice>,
    live: [bool; 16],
    live_up: [bool; 16],
    pedal: bool,
    last: Option<u64>,
    setup: bool,
    pending: Vec<[u8; 3]>,
}
impl Playback {
    pub fn new(config: os_model::mpe::Config, mut notes: Vec<Scheduled>) -> Self {
        notes.sort_by_key(|n| n.start);
        Self {
            chase_notes: true,
            resume_tick: None,
            config: config.valid(),
            pitch_offset: 0.,
            notes,
            voices: vec![],
            live: [false; 16],
            live_up: [false; 16],
            pedal: false,
            last: None,
            setup: false,
            pending: vec![],
        }
    }
    pub fn with_chase(mut self, on: bool) -> Self { self.chase_notes = on; self }
    pub fn adopt(&mut self, old: &mut Self) {
        if self.config != old.config {
            self.pending.extend(old.shutdown());
            return;
        }
        self.live = std::mem::replace(&mut old.live, [false; 16]);
        self.live_up = old.live_up;
        self.pedal = old.pedal;
        self.last = old.last;
        self.resume_tick = old.resume_tick.take();
        self.setup = old.setup;
        for voice in old.voices.drain(..) {
            let oldnote = &old.notes[voice.note];
            let matches = |i: usize, n: &Scheduled| {
                n.start == oldnote.start
                    && n.key == oldnote.key
                    && !self.voices.iter().any(|v| v.note == i)
            };
            let note = self
                .notes
                .get(voice.note)
                .filter(|n| matches(voice.note, n))
                .map(|_| voice.note)
                .or_else(|| {
                    self.notes
                        .iter()
                        .enumerate()
                        .find(|(i, n)| matches(*i, n))
                        .map(|(i, _)| i)
                });
            if let Some(note) = note {
                self.voices.push(Voice {
                    note,
                    last: [i32::MIN; 3],
                    ..voice
                });
            } else {
                self.pending.push([0x80 | voice.channel, oldnote.key, 64]);
            }
        }
    }
    pub fn shutdown(&mut self) -> Vec<[u8; 3]> {
        let mut out = self.offs();
        let b = 0xb0 | self.config.manager();
        out.extend([
            [b, 101, 0],
            [b, 100, 6],
            [b, 6, 0],
            [b, 101, 127],
            [b, 100, 127],
        ]);
        self.released();
        out
    }
    pub fn offs(&self) -> Vec<[u8; 3]> {
        let mut v: Vec<_> = self
            .voices
            .iter()
            .map(|v| {
                [
                    0x80 | v.channel,
                    self.notes[v.note].key,
                    self.notes[v.note].release,
                ]
            })
            .collect();
        for c in 0..16 {
            if self.live[c] {
                v.push([0xb0 | c as u8, 120, 0]);
            }
        }
        v
    }
    pub fn released(&mut self) {
        self.resume_tick = if self.voices.is_empty() { None } else { self.last.map(|t| t.saturating_add(1)) };
        self.voices.clear();
        self.live = [false; 16];
        self.live_up = [false; 16];
        self.last = None;
        self.pedal = false;
        self.setup = false;
    }
    pub fn stop(&mut self) -> Vec<[u8; 3]> {
        let mut out = vec![];
        for v in self.voices.drain(..) {
            out.push([
                0x80 | v.channel,
                self.notes[v.note].key,
                self.notes[v.note].release,
            ]);
        }
        out
    }
    pub fn live_message(&mut self, b: [u8; 3]) -> Vec<[u8; 3]> {
        let c = b[0] & 15;
        let mut out = vec![];
        if !self.setup {
            out.extend(self.config.setup());
            self.setup = true;
        }
        if self.config.member(c) {
            if b[0] & 0xf0 == 0x90 && b[2] > 0 {
                if let Some(i) = self.voices.iter().position(|v| v.channel == c) {
                    let v = self.voices.remove(i);
                    out.push([0x80 | c, self.notes[v.note].key, self.notes[v.note].release]);
                }
                self.live[c as usize] = true;
                self.live_up[c as usize] = false;
            } else if b[0] & 0xf0 == 0x80 || b[0] & 0xf0 == 0x90 && b[2] == 0 {
                self.live_up[c as usize] = true;
                if !self.pedal {
                    self.live[c as usize] = false;
                }
            }
        }
        if b[0] == (0xb0 | self.config.manager()) {
            if b[1] == 64 {
                self.pedal = b[2] >= 64;
                if !self.pedal {
                    for c in 0..16 {
                        if self.live_up[c] {
                            self.live[c] = false;
                        }
                    }
                    self.live_up = [false; 16];
                }
            }
            if b[1] == 120 || b[1] == 123 {
                self.live = [false; 16];
                self.live_up = [false; 16];
                out.extend(self.stop());
            }
        }
        out.push(b);
        out
    }
    pub fn tick(&mut self, tick: u64, on: bool) -> Vec<[u8; 3]> {
        let mut out = std::mem::take(&mut self.pending);
        if !self.setup {
            out.extend(self.config.setup());
            self.setup = true;
        }
        if !on {
            out.extend(self.stop());
            self.last = Some(tick);
            return out;
        }
        if self.last.is_some_and(|last| tick != last + 1) {
            out.extend(self.stop());
        }
        let mut i = 0;
        while i < self.voices.len() {
            let v = &self.voices[i];
            if tick >= self.notes[v.note].end {
                let v = self.voices.remove(i);
                out.push([
                    0x80 | v.channel,
                    self.notes[v.note].key,
                    self.notes[v.note].release,
                ]);
            } else {
                i += 1;
            }
        }
        let resume = self.resume_tick.take() == Some(tick);
        let seeking = self.last.is_none() || self.last.is_some_and(|last| tick != last + 1);
        let first = if seeking {
            0
        } else {
            self.notes.partition_point(|n| n.start < tick)
        };
        let last = self.notes.partition_point(|n| n.start <= tick);
        for i in first..last {
            let n = &self.notes[i];
            if n.start > tick
                || n.end <= tick
                || (n.start < tick && (!seeking || !(self.chase_notes || resume)))
                || self.voices.iter().any(|v| v.note == i)
            {
                continue;
            }
            let members = self.config.channels();
            let channel = members
                .iter()
                .copied()
                .find(|&c| !self.live[c as usize] && !self.voices.iter().any(|v| v.channel == c))
                .or_else(|| {
                    let v = self.voices.first()?;
                    Some(v.channel)
                });
            let Some(c) = channel else {
                continue;
            };
            if let Some(at) = self.voices.iter().position(|v| v.channel == c) {
                let v = self.voices.remove(at);
                out.push([0x80 | c, self.notes[v.note].key, self.notes[v.note].release]);
            }
            out.extend([
                [0xe0 | c, 0, 64],
                [0xd0 | c, 0, 0],
                [0xb0 | c, 74, 64],
                [0x90 | c, n.key, n.velocity.max(1).min(127)],
            ]);
            self.voices.push(Voice {
                note: i,
                channel: c,
                last: [i32::MIN; 3],
            });
        }
        for v in &mut self.voices {
            let n = &self.notes[v.note];
            let at = (tick - n.start) as f32 / (n.end - n.start).max(1) as f32;
            let pitch =
                (n.expression.value(0, at) * self.config.sensitivity + n.fine + self.pitch_offset)
                    / f32::from(self.config.bend);
            let raw = (8192. + pitch.clamp(-1., 1.) * if pitch >= 0. { 8191. } else { 8192. })
                .round() as i32;
            let values = [
                raw,
                (n.expression
                    .value(1, at)
                    .clamp(0., 1.)
                    .powf(self.config.pressure)
                    * 127.)
                    .round() as i32,
                (n.expression
                    .value(2, at)
                    .clamp(0., 1.)
                    .powf(self.config.slide)
                    * 127.)
                    .round() as i32,
            ];
            for lane in 0..3 {
                if v.last[lane] == values[lane] {
                    continue;
                }
                v.last[lane] = values[lane];
                let value = values[lane];
                out.push(match lane {
                    0 => [
                        0xe0 | v.channel,
                        (value & 127) as u8,
                        ((value >> 7) & 127) as u8,
                    ],
                    1 => [0xd0 | v.channel, value as u8, 0],
                    _ => [0xb0 | v.channel, 74, value as u8],
                });
            }
        }
        self.last = Some(tick);
        out
    }
}
#[cfg(test)]
mod playback_tests {
    use super::*;
    #[test]
    fn mpe_playback_channels_seek_overlap_and_muting() {
        let mut expr = os_model::mpe::Expression::default();
        expr.set(0, 0., 12., false);
        let note = Scheduled {
            start: 0,
            end: 96,
            key: 60,
            velocity: 100,
            release: 64,
            expression: expr,
            fine: 0.,
        };
        let mut p = Playback::new(
            os_model::mpe::Config::default(),
            vec![
                note.clone(),
                Scheduled {
                    expression: Default::default(),
                    ..note
                },
            ],
        );
        let mut d = Decoder::default();
        let events: Vec<_> = p
            .tick(0, true)
            .into_iter()
            .flat_map(|b| d.receive(b).unwrap_or_default())
            .collect();
        let ids: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::On { id, .. } = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(events.iter().any(|e|matches!(e,Event::Expression{id,lane:0,value}if *id==ids[0]&&(*value-12.).abs()<0.01)));
        assert!(p.tick(1, true).is_empty());
        assert_eq!(
            p.tick(96, true)
                .iter()
                .filter(|b| b[0] & 0xf0 == 0x80)
                .count(),
            2
        );
        assert_eq!(
            p.tick(48, true)
                .iter()
                .filter(|b| b[0] & 0xf0 == 0x90)
                .count(),
            2
        );
        assert_eq!(
            p.tick(49, false)
                .iter()
                .filter(|b| b[0] & 0xf0 == 0x80)
                .count(),
            2
        );
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn mpe_zone_changes_release_old_notes_and_disable_cleanly() {
        let mut d = Decoder::default();
        for b in os_model::mpe::Config::default().setup() {
            d.receive(b);
        }
        d.receive([0x9f, 60, 100]);
        let mut events = vec![];
        for b in (os_model::mpe::Config {
            members: 7,
            ..Default::default()
        })
        .setup()
        {
            events.extend(d.receive(b).unwrap_or_default());
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Off { channel: 15, .. }))
        );
        let mut p = Playback::new(os_model::mpe::Config::default(), vec![]);
        for b in p.shutdown() {
            d.receive(b);
        }
        assert!(!d.active());
        assert!(d.receive([0xe0, 0, 80]).is_none());
        let upper = os_model::mpe::Config {
            upper: true,
            members: 7,
            ..Default::default()
        };
        for b in upper.setup() {
            d.receive(b);
        }
        assert!(matches!(
            d.receive([0x98, 64, 100]).unwrap()[0],
            Event::On { channel: 8, .. }
        ));
        assert!(d.receive([0x97, 64, 100]).is_none());
    }
}
