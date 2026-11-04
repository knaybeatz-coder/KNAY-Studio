//! Piano Roll MPE editing and controller performance.
use crate::{
    app::{Action, App, State, Ui},
    history::{self, Snap},
    piano,
    playlist::{Cmd, Mark, PItem},
};
use os_model::mpe::{Config, Expression};
use os_ui::{Font, Rect};
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    View,
    Enable(bool),
    Upper(bool),
    Members(u8),
    Bend(u8),
    Sensitivity(f32),
    Pressure(f32),
    Slide(f32),
    Smooth,
    Range,
    Clear,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Command(Command),
    Curve(u8, Rect),
}
#[derive(Default)]
pub struct UiState {
    pub open: bool,
    pub support: String,
    pub smooth: bool,
    pub range: u8,
    pub drag: Option<(u8, Rect, u32)>,
    decoders: std::collections::BTreeMap<(String, u16), os_engine::mpe::Decoder>,
    held: Vec<Held>,
    serial: u64,
}
struct Held {
    config: Config,
    dev: String,
    id: i32,
    input: u8,
    key: u8,
    output: u8,
    row: usize,
    src: crate::step_edit::Src,
}
fn selected(s: &State) -> Option<usize> {
    let sel = s.piano.selection(s.pattern, piano::channel(s));
    (sel.len() == 1).then(|| sel[0])
}
fn config(s: &State) -> Config {
    piano::channel(s).map_or(Config::default(), |id| s.project.mpe.config(id))
}
pub fn menu(s: &State) -> PItem {
    let c = config(s);
    let item = |name, cmd| PItem::new(name, Cmd::Mpe(cmd));
    PItem::sub(
        "MPE / Note expression",
        vec![
            item("Show expression lanes".into(), Command::View).mark(Mark::Check(s.mpe.open)),
            item(
                "Enable MPE for this instrument".into(),
                Command::Enable(!c.enabled),
            )
            .mark(Mark::Check(c.enabled)),
            PItem::sub(
                "Zone",
                vec![
                    item("Lower (manager channel 1)".into(), Command::Upper(false))
                        .mark(Mark::Radio(!c.upper)),
                    item("Upper (manager channel 16)".into(), Command::Upper(true))
                        .mark(Mark::Radio(c.upper)),
                ],
            ),
            PItem::sub(
                "Member channels",
                [1, 2, 4, 7, 10, 14, 15]
                    .into_iter()
                    .map(|n| {
                        item(n.to_string(), Command::Members(n)).mark(Mark::Radio(c.members == n))
                    })
                    .collect(),
            ),
            PItem::sub(
                "Pitch bend range",
                [2, 12, 24, 48, 96]
                    .into_iter()
                    .map(|n| {
                        item(format!("+/- {n} semitones"), Command::Bend(n))
                            .mark(Mark::Radio(c.bend == n))
                    })
                    .collect(),
            ),
            PItem::sub(
                "Pitch sensitivity",
                [0.25, 0.5, 1., 1.5, 2.]
                    .into_iter()
                    .map(|v| {
                        item(format!("{v}x"), Command::Sensitivity(v))
                            .mark(Mark::Radio(c.sensitivity == v))
                    })
                    .collect(),
            ),
            PItem::sub(
                "Pressure response",
                [0.5, 1., 2.]
                    .into_iter()
                    .map(|v| {
                        item(format!("Curve {v}"), Command::Pressure(v))
                            .mark(Mark::Radio(c.pressure == v))
                    })
                    .collect(),
            ),
            PItem::sub(
                "Slide response",
                [0.5, 1., 2.]
                    .into_iter()
                    .map(|v| {
                        item(format!("Curve {v}"), Command::Slide(v))
                            .mark(Mark::Radio(c.slide == v))
                    })
                    .collect(),
            ),
            PItem::head("Enable MPE in the instrument too; support depends on its patch."),
        ],
    )
}
pub fn command(app: &mut App, cmd: Command) {
    let support = app
        .plugins
        .hosted
        .get(&app.state.selected_channel)
        .and_then(|h| h.plugin.lock().ok().map(|p| p.mpe_support()));
    app.state.mpe.support = match support {
        Some(2) => "Native expression delivery; enable expressive mappings in the instrument.",
        Some(1) => "MIDI MPE delivery; enable MPE mode in the instrument.",
        _ => "This instrument does not advertise complete MPE support.",
    }
    .into();
    match cmd {
        Command::View => app.state.mpe.open = !app.state.mpe.open,
        Command::Smooth => app.state.mpe.smooth = !app.state.mpe.smooth,
        Command::Range => {
            app.state.mpe.range = match app.state.mpe.range {
                2 => 12,
                12 => 48,
                48 => 96,
                _ => 2,
            }
        }
        Command::Clear => {
            let Some(i) = selected(&app.state) else {
                return;
            };
            let snap = Snap::pattern(&app.state, app.state.pattern);
            history::record(&mut app.state, "Clear note expression", snap);
            piano::pattern_mut(&mut app.state).notes[i].expression = 0;
            app.state.piano.dirty = true;
        }
        _ => {
            let Some(id) = piano::channel(&app.state) else {
                return;
            };
            let mut c = config(&app.state);
            match cmd {
                Command::Enable(v) => c.enabled = v,
                Command::Upper(v) => c.upper = v,
                Command::Members(v) => c.members = v,
                Command::Bend(v) => c.bend = v,
                Command::Sensitivity(v) => c.sensitivity = v,
                Command::Pressure(v) => c.pressure = v,
                Command::Slide(v) => c.slide = v,
                _ => {}
            }
            let old = app.state.project.mpe.channels.clone();
            history::record(&mut app.state, "MPE configuration", Snap::MpeConfig(old));
            app.mpe_release_all();
            app.state.project.mpe.set_config(id, c);
            app.state.mpe.decoders.clear();
            app.state.piano.dirty = true;
            app.state.mpe.open = true;
        }
    }
    if app.state.mpe.open && app.state.piano.split.panel_collapsed() {
        app.state.piano.split = Default::default();
    }
    app.state.message = Some(app.state.mpe.support.clone());
    app.invalidate();
}
pub fn paint(ui: &mut Ui, s: &State, r: Rect, notes: &[os_model::Note], sel: &[usize]) {
    let th = ui.t;
    ui.c.fill_rect(r, th.sunken);
    ui.hit(r, Action::Swallow, s.mpe.support.clone());
    if r.w < 320. || r.h < 100. {
        ui.c.text_clip(
            Font::new(11),
            r.x + 5.,
            r.y + 6.,
            r.w - 10.,
            th.text_dim,
            "Expand the event panel to edit MPE",
        );
        return;
    }
    let button = |ui: &mut Ui, r: Rect, label: &str, c| {
        ui.hit(r, Action::Mpe(Hit::Command(c)), label.to_owned());
        ui.c.fill_rrect(r, 3., th.button_hi);
        ui.c.text_clip(Font::new(10), r.x + 4., r.y + 4., r.w - 8., th.text, label);
    };
    let Some((index, note)) = sel
        .first()
        .filter(|_| sel.len() == 1)
        .and_then(|&i| notes.get(i).map(|n| (i, n)))
    else {
        ui.c.text_clip(
            Font::new(11),
            r.x + 6.,
            r.y + 7.,
            r.w - 12.,
            th.text,
            "MPE: select one note to edit its expression",
        );
        button(
            ui,
            Rect::new(r.right() - 58., r.bottom() - 25., 52., 20.),
            "Close",
            Command::View,
        );
        return;
    };
    let _ = index;
    let range = if s.mpe.range == 0 { 48 } else { s.mpe.range };
    let header = format!(
        "{} @ {}  |  {}",
        crate::views::piano::key_name(note.key as i32),
        note.position,
        if config(s).enabled {
            "MPE"
        } else {
            "MPE disabled"
        }
    );
    ui.c.text_clip(
        Font::new(11),
        r.x + 6.,
        r.y + 6.,
        (r.w - 240.).max(20.),
        th.text,
        &header,
    );
    button(
        ui,
        Rect::new(r.right() - 230., r.y + 3., 65., 20.),
        if s.mpe.smooth { "Linear" } else { "Stepped" },
        Command::Smooth,
    );
    button(
        ui,
        Rect::new(r.right() - 160., r.y + 3., 55., 20.),
        &format!("+/-{range}"),
        Command::Range,
    );
    button(
        ui,
        Rect::new(r.right() - 100., r.y + 3., 43., 20.),
        "Clear",
        Command::Clear,
    );
    button(
        ui,
        Rect::new(r.right() - 52., r.y + 3., 46., 20.),
        "Close",
        Command::View,
    );
    ui.c.text_clip(
        Font::new(10),
        r.x + 62.,
        r.bottom() - 15.,
        r.w - 70.,
        th.text_dim,
        &format!("Note start  —  {} ticks  —  note end", note.length),
    );
    let expression = s
        .project
        .mpe
        .get(note.expression)
        .cloned()
        .unwrap_or_default();
    let h = (r.h - 48.) / 3.;
    for lane in 0..3 {
        let y = r.y + 28. + lane as f32 * h;
        let area = Rect::new(r.x + 62., y, r.w - 70., h - 5.);
        ui.c.text_clip(
            Font::new(10),
            r.x + 5.,
            y + 5.,
            54.,
            th.text_dim,
            ["Pitch", "Pressure", "Slide"][lane],
        );
        ui.c.fill_rect(area, th.pr_row);
        ui.hit(
            area,
            Action::Mpe(Hit::Curve(lane as u8, area)),
            "Draw expression; right-click removes the nearest point",
        );
        let to_y = |v: f32| {
            area.bottom()
                - if lane == 0 {
                    (v / f32::from(range) * 0.5 + 0.5).clamp(0., 1.)
                } else {
                    v.clamp(0., 1.)
                } * area.h
        };
        let mut last = (area.x, to_y(expression.value(lane, 0.)));
        for x in 1..=(area.w as usize).min(4096) {
            let at = x as f32 / area.w;
            let next = (area.x + x as f32, to_y(expression.value(lane, at)));
            ui.c.line(last.0, last.1, next.0, next.1, 1., th.accent);
            last = next;
        }
        for p in &expression.lanes[lane] {
            let x = area.x + p.at * area.w;
            let y = to_y(p.value);
            ui.c.fill_rect(Rect::new(x - 2., y - 2., 4., 4.), th.text);
        }
    }
}
pub fn draw(app: &mut App, lane: u8, r: Rect, id: u32, remove: bool) {
    let (x, y) = app.mouse;
    let at = ((x - r.x) / r.w).clamp(0., 1.);
    let value = (1. - (y - r.y) / r.h).clamp(0., 1.);
    let range = if app.state.mpe.range == 0 {
        48
    } else {
        app.state.mpe.range
    };
    let value = if lane == 0 {
        (value * 2. - 1.) * f32::from(range)
    } else {
        value
    };
    let smooth = app.state.mpe.smooth;
    if let Some(e) = id
        .checked_sub(1)
        .and_then(|i| app.state.project.mpe.expressions.get_mut(i as usize))
    {
        if remove {
            e.remove(lane as usize, at);
        } else {
            e.set(lane as usize, (at * 512.).round() / 512., value, smooth);
        }
    }
    app.state.piano.dirty = true;
    app.invalidate();
}
pub fn begin(app: &mut App, lane: u8, r: Rect, remove: bool) {
    let Some(i) = selected(&app.state) else {
        return;
    };
    let note = piano::notes(&app.state)[i];
    let e = app
        .state
        .project
        .mpe
        .get(note.expression)
        .cloned()
        .unwrap_or_default();
    let snap = Snap::pattern(&app.state, app.state.pattern);
    history::record(&mut app.state, "Edit note expression", snap);
    let id = app.state.project.mpe.add(e);
    piano::pattern_mut(&mut app.state).notes[i].expression = id;
    if !remove {
        app.state.mpe.drag = Some((lane, r, id));
    }
    draw(app, lane, r, id, remove);
}
impl App {
    pub(crate) fn mpe_release_all(&mut self) {
        for h in std::mem::take(&mut self.state.mpe.held) {
            self.transport.midi(h.row, [0x80 | h.output, h.key, 64], 0.);
            self.record_up_with(h.src, Some(64));
        }
        self.state.mpe.decoders.clear();
    }
    pub(crate) fn mpe_tick(&mut self) {
        if self.state.mpe.held.iter().any(|h| {
            self.state
                .project
                .channels
                .get(h.row)
                .is_none_or(|c| self.state.project.mpe.config(c.index) != h.config)
        }) {
            self.mpe_release_all();
        }
    }
    pub(crate) fn mpe_input(&mut self, dev: &str, msg: crate::midi_in::Msg) -> bool {
        use crate::midi_in::Msg as M;
        let manager = matches!(
            msg,
            M::Cc { ch: 0 | 15, .. } | M::Bend { ch: 0 | 15, .. } | M::Pressure { ch: 0 | 15, .. }
        );
        let mut rows = vec![self.state.selected_channel];
        if manager {
            rows.extend(
                self.state
                    .mpe
                    .held
                    .iter()
                    .filter(|h| h.dev == dev)
                    .map(|h| h.row),
            );
            rows.sort_unstable();
            rows.dedup();
        }
        let mut used = false;
        for row in rows {
            used |= self.mpe_input_row(dev, msg, row);
        }
        used
    }
    fn mpe_input_row(&mut self, dev: &str, msg: crate::midi_in::Msg, selected_row: usize) -> bool {
        use crate::midi_in::Msg as M;
        let b = match msg {
            M::On { ch, key, vel } => [0x90 | ch, key, vel],
            M::Off { ch, key, vel } => [0x80 | ch, key, vel],
            M::Bend { ch, v } => [0xe0 | ch, (v & 127) as u8, (v >> 7) as u8],
            M::Cc { ch, n, v } => [0xb0 | ch, n, v],
            M::Pressure { ch, v } => [0xd0 | ch, v, 0],
            M::Poly { ch, key, v } => [0xa0 | ch, key, v],
            _ => return false,
        };
        let input = b[0] & 15;
        let row = self
            .state
            .mpe
            .held
            .iter()
            .find(|h| h.dev == dev && h.input == input)
            .map_or(selected_row, |h| h.row);
        let Some(channel) = self.state.project.channels.get(row).map(|c| c.index) else {
            return false;
        };
        let cfg = self.state.project.mpe.config(channel);
        if !cfg.enabled {
            return false;
        }
        let decoder = self
            .state
            .mpe
            .decoders
            .entry((dev.to_owned(), channel))
            .or_insert_with(|| {
                let mut d = os_engine::mpe::Decoder::default();
                for b in cfg.setup() {
                    d.receive(b);
                }
                d
            });
        let Some(events) = decoder.receive(b) else {
            if let Some(bytes) = msg.bytes() {
                self.sync_live();
                let mut bytes = bytes;
                bytes[0] = (bytes[0] & 0xf0) | cfg.manager();
                self.transport.midi(row, bytes, 0.);
            }
            return true;
        };
        self.sync_live();
        for e in events {
            use os_engine::mpe::Event as E;
            match e {
                E::On {
                    channel: input,
                    key,
                    id,
                    velocity,
                } => {
                    let mut output = cfg.channels().into_iter().find(|c| {
                        !self
                            .state
                            .mpe
                            .held
                            .iter()
                            .any(|h| h.row == row && h.output == *c)
                    });
                    if output.is_none() {
                        if let Some(i) = self.state.mpe.held.iter().position(|h| h.row == row) {
                            let h = self.state.mpe.held.remove(i);
                            self.transport.midi(row, [0x80 | h.output, h.key, 64], 0.);
                            self.record_up_with(h.src, Some(64));
                            output = Some(h.output);
                        }
                    }
                    let Some(output) = output else {
                        continue;
                    };
                    self.state.mpe.serial = self.state.mpe.serial.wrapping_add(1);
                    let src = crate::step_edit::Src::Mpe(self.state.mpe.serial);
                    let expression = self
                        .state
                        .rec
                        .armed
                        .then(|| self.state.project.mpe.add(Expression::default()));
                    self.record_down_with(
                        src,
                        u16::from(key),
                        (velocity * 127.).round() as u8,
                        crate::record::Extra {
                            channel: Some(channel),
                            expression,
                            ..Default::default()
                        },
                    );
                    self.transport.midi(
                        row,
                        [0x90 | output, key, (velocity * 127.).round().max(1.) as u8],
                        0.,
                    );
                    self.state.mpe.held.push(Held {
                        config: cfg,
                        dev: dev.into(),
                        id,
                        input,
                        key,
                        output,
                        row,
                        src,
                    });
                }
                E::Off { id, velocity, .. } => {
                    if let Some(i) = self
                        .state
                        .mpe
                        .held
                        .iter()
                        .position(|h| h.dev == dev && h.id == id && h.row == row)
                    {
                        let h = self.state.mpe.held.remove(i);
                        self.transport.midi(
                            h.row,
                            [0x80 | h.output, h.key, (velocity * 127.).round() as u8],
                            0.,
                        );
                        self.record_up_with(h.src, Some((velocity * 127.).round() as u8));
                    }
                }
                E::Expression { id, lane, value } => {
                    if let Some(h) = self
                        .state
                        .mpe
                        .held
                        .iter()
                        .find(|h| h.dev == dev && h.id == id && h.row == row)
                    {
                        let (src, output) = (h.src, h.output);
                        self.record_mpe_point(src, lane, value);
                        let raw = match lane {
                            0 => {
                                let p =
                                    (value * cfg.sensitivity / f32::from(cfg.bend)).clamp(-1., 1.);
                                let v = (8192. + p * if p >= 0. { 8191. } else { 8192. }).round()
                                    as u16;
                                [0xe0 | output, (v & 127) as u8, (v >> 7) as u8]
                            }
                            1 => [
                                0xd0 | output,
                                (value.clamp(0., 1.).powf(cfg.pressure) * 127.).round() as u8,
                                0,
                            ],
                            _ => [
                                0xb0 | output,
                                74,
                                (value.clamp(0., 1.).powf(cfg.slide) * 127.).round() as u8,
                            ],
                        };
                        self.transport.midi(row, raw, 0.);
                    }
                }
            }
        }
        self.invalidate();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        a.state.selected_channel = 0;
        a.state.pattern = 1;
        a.state.project.mpe.set_config(
            0,
            Config {
                enabled: true,
                ..Default::default()
            },
        );
        a
    }
    #[test]
    fn mpe_edit_copy_undo_native_recall() {
        let mut a = app();
        let mut n = os_model::Note {
            length: 96,
            ..os_model::Note::step(0, 0)
        };
        let mut e = Expression::default();
        e.set(0, 0., 0., true);
        e.set(0, 1., 12., true);
        n.expression = a.state.project.mpe.add(e);
        a.state.project.patterns[0].notes = vec![n, os_model::Note { position: 96, ..n }];
        a.state.piano.select(1, 0, vec![1]);
        let r = Rect::new(100., 100., 200., 100.);
        a.mouse = (200., 125.);
        begin(&mut a, 0, r, false);
        a.state.mpe.drag = None;
        let notes = piano::notes(&a.state);
        assert_ne!(notes[0].expression, notes[1].expression);
        assert_eq!(
            a.state
                .project
                .mpe
                .get(notes[0].expression)
                .unwrap()
                .value(0, 0.5),
            6.
        );
        assert_eq!(
            a.state
                .project
                .mpe
                .get(notes[1].expression)
                .unwrap()
                .value(0, 0.5),
            24.
        );
        history::undo(&mut a.state);
        assert_eq!(
            piano::notes(&a.state),
            &[n, os_model::Note { position: 96, ..n }]
        );
        history::redo(&mut a.state);
        let f = os_flp::project::create(&a.state.project).unwrap();
        let loaded =
            os_flp::project::load(&os_flp::FlpFile::parse(&f.to_bytes().unwrap()).unwrap())
                .unwrap();
        assert_eq!(loaded.mpe.config(0), a.state.project.mpe.config(0));
        for (old, new) in a.state.project.patterns[0]
            .notes
            .iter()
            .zip(&loaded.patterns[0].notes)
        {
            assert_eq!(
                a.state.project.mpe.get(old.expression),
                loaded.mpe.get(new.expression)
            );
        }
        a.state.project.reorder_channels(&[1, 0, 2, 3, 4]);
        assert!(a.state.project.mpe.config(1).enabled);
        assert!(!a.state.project.mpe.config(0).enabled);
    }
    #[test]
    fn mpe_record_unisons_and_expression_without_channel_flattening() {
        use crate::midi_in::Msg as M;
        let mut a = app();
        crate::settings_file::set_flag("rec_notes", true);
        crate::settings_file::set_flag("rec_q_start", false);
        crate::settings_file::set_flag("rec_q_end", false);
        a.state.rec.armed = true;
        a.transport.pretend_at(0., false);
        a.record_tick();
        assert!(a.mpe_input(
            "Controller",
            M::On {
                ch: 1,
                key: 60,
                vel: 100
            }
        ));
        assert!(a.mpe_input(
            "Controller",
            M::On {
                ch: 2,
                key: 60,
                vel: 90
            }
        ));
        a.transport.pretend_at(48., false);
        a.mpe_input("Controller", M::Bend { ch: 1, v: 12288 });
        a.mpe_input("Controller", M::Pressure { ch: 2, v: 100 });
        a.mpe_input(
            "Controller",
            M::Cc {
                ch: 2,
                n: 74,
                v: 110,
            },
        );
        a.transport.pretend_at(96., false);
        a.mpe_input(
            "Controller",
            M::Off {
                ch: 1,
                key: 60,
                vel: 64,
            },
        );
        a.mpe_input(
            "Controller",
            M::Off {
                ch: 2,
                key: 60,
                vel: 64,
            },
        );
        let notes = piano::notes(&a.state);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].key, notes[1].key);
        assert_ne!(notes[0].expression, notes[1].expression);
        let x = a.state.project.mpe.get(notes[0].expression).unwrap();
        let y = a.state.project.mpe.get(notes[1].expression).unwrap();
        assert!(x.value(0, 0.75) > 23.9);
        assert_eq!(y.value(0, 0.75), 0.);
        assert!(y.value(1, 0.75) > 0.7);
        assert_eq!(x.value(1, 0.75), 0.);
        assert!(a.state.project.patterns[0].controls.is_empty());
        a.end_take();
        history::undo(&mut a.state);
        assert!(piano::notes(&a.state).is_empty());
    }
    #[test]
    fn mpe_midi_roundtrip_preserves_polyphonic_dimensions() {
        let mut a = app();
        let mut e = Expression::default();
        e.set(0, 0., 0., true);
        e.set(0, 1., 24., true);
        e.set(1, 0., 0., true);
        e.set(1, 1., 1., true);
        e.set(2, 0., 0.2, false);
        e.set(2, 0.5, 0.8, false);
        let id = a.state.project.mpe.add(e);
        a.state.project.patterns[0].notes = vec![
            os_model::Note {
                length: 96,
                expression: id,
                ..os_model::Note::step(0, 0)
            },
            os_model::Note {
                length: 96,
                ..os_model::Note::step(0, 0)
            },
        ];
        let f = crate::midi_file::pattern_file(&a.state.project, 1, 0, 120.);
        let parsed = os_smf::read(&os_smf::write(&f)).unwrap();
        let imported =
            crate::midi_file::convert(&parsed, &crate::midi_file::Options::default(), 96);
        let part = imported.parts.iter().find(|p| p.mpe.is_some()).unwrap();
        assert_eq!(part.notes.len(), 2);
        let e = &part.expressions[part.notes[0].expression as usize - 1];
        assert!((e.value(0, 0.5) - 12.).abs() < 0.03);
        assert!((e.value(1, 0.5) - 0.5).abs() < 0.01);
        assert!((e.value(2, 0.75) - 0.8).abs() < 0.01);
        assert!(part.controls.is_empty());
    }
    #[test]
    fn mpe_split_curves_panel_and_malformed_extension() {
        let mut a = app();
        let mut e = Expression::default();
        e.set(0, 0., 0., true);
        e.set(0, 1., 24., false);
        let id = a.state.project.mpe.add(e);
        a.state.project.patterns[0].notes = vec![os_model::Note {
            length: 96,
            expression: id,
            ..os_model::Note::step(0, 0)
        }];
        assert_eq!(piano::slice(&mut a.state, 48), 1);
        let n = piano::notes(&a.state);
        assert_eq!(
            a.state
                .project
                .mpe
                .get(n[0].expression)
                .unwrap()
                .value(0, 0.5),
            6.
        );
        assert_eq!(
            a.state
                .project
                .mpe
                .get(n[1].expression)
                .unwrap()
                .value(0, 0.5),
            18.
        );
        history::undo(&mut a.state);
        assert_eq!(piano::notes(&a.state).len(), 1);
        a.state.piano.select(1, 0, vec![0]);
        a.state.mpe.open = true;
        a.state.raise(crate::app::Win::PianoRoll);
        let mut canvas = os_ui::Canvas::new(1280, 900, 1.).unwrap();
        a.paint(&mut canvas, None);
        let hits: Vec<_> = a
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::Mpe(_)))
            .collect();
        assert!(hits.len() >= 7);
        for h in hits {
            assert!(
                h.rect.w > 0.
                    && h.rect.h > 0.
                    && h.rect.x >= 0.
                    && h.rect.right() <= 1280.
                    && h.rect.y >= 0.
                    && h.rect.bottom() <= 900.
            );
        }
        let mut f = os_flp::project::create(&a.state.project).unwrap();
        for c in &mut f.chunks {
            if c.tag == os_flp::Tag(*b"LSME") {
                if let os_flp::ChunkBody::Raw(d) = &mut c.body {
                    d.truncate(8);
                }
            }
        }
        let loaded = os_flp::project::load(&f).unwrap();
        assert!(!loaded.mpe.config(0).enabled);
        assert_eq!(loaded.patterns[0].notes[0].expression, 0);
    }
}

pub fn chop(s: &mut State, size: u32) -> bool {
    let mut added = vec![];
    let changed = crate::piano_tools::apply_as(
        s,
        |s, notes| {
            let mut out = vec![];
            for original in notes {
                for mut n in crate::piano_tools::chop_from_start(vec![original], size) {
                    if let Some(e) = s.project.mpe.get(original.expression) {
                        let length = original.length.max(1) as f32;
                        added.push(e.window(
                            (n.position - original.position) as f32 / length,
                            (n.position + n.length - original.position) as f32 / length,
                        ));
                        n.expression = (s.project.mpe.expressions.len() + added.len()) as u32;
                    }
                    out.push(n);
                }
            }
            out
        },
        None,
    );
    s.project.mpe.expressions.extend(added);
    changed
}
