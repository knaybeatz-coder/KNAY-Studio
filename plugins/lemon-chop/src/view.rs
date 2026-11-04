// SPDX-License-Identifier: GPL-3.0-or-later
//! Lemon Pith surfaces, knobs and typography; original waveform/pad layout.
use crate::{
    editor::{Logic, Target},
    params::*,
};
use os_theme::design::lemon as brand;
use os_ui::{
    Canvas, Color, Font, Rect,
    kit::lemon::{self, plug},
};
use std::sync::atomic::Ordering;
pub const W: u32 = 1040;
pub const H: u32 = 860;
fn color(n: usize) -> Color {
    [
        0xd6b85c, 0x87b68a, 0x6fabb9, 0xb99ad0, 0xcf8a79, 0x88a9ca, 0xb8be73, 0xc58da9,
    ]
    .map(Color::rgb)[n % 8]
}
fn cue_color(d: &Document, n: usize) -> Color {
    d.cues[n]
        .color
        .map_or_else(|| color(n), |v| Color::rgb(v & 0xffffff))
}
impl Logic {
    fn button_view(&mut self, c: &mut Canvas, r: Rect, label: &str, id: &'static str, on: bool) {
        let ink = lemon::plug_button(c, r, 5., self.visual_state(&Target::Button(id), on));
        c.text_center(Font::new(13), r, ink, label);
        self.hits.push((r, Target::Button(id)));
    }
    fn param_view(&mut self, c: &mut Canvas, r: Rect, k: usize, label: &str) {
        let v = self.control_value(k);
        let toggle = self.params.defs[k].step == 1. && self.params.defs[k].max == 1.;
        let ink = lemon::plug_button(
            c,
            r,
            5.,
            self.visual_state(&Target::Param(k), toggle && v > 0.5),
        );
        let text = if toggle {
            label.to_string()
        } else if k >= GLOBAL && (k - GLOBAL) % STRIDE == OUTPUT {
            if v < 0.5 {
                "Output: Main".into()
            } else {
                format!("Mixer: +{}", v as usize)
            }
        } else if k == GLIDE {
            if v <= 0. {
                "Glide: Off".into()
            } else {
                format!("Glide: {:.0} ms", v * 1000.)
            }
        } else if (12..16).contains(&k) {
            format!("{label}  {:.0}%", v * 100.)
        } else if k == DETUNE {
            format!("{v:+.0} cents")
        } else if k == MASTER {
            format!("{v:+.1} dB")
        } else if k == KEY {
            format!("Key {v:+.0} st")
        } else {
            format!("{label}  {v:.2}")
        };
        c.text_center(Font::new(13), r, ink, &text);
        self.hits.push((r, Target::Param(k)));
    }
    pub fn paint(&mut self, c: &mut Canvas, t: &os_theme::Palette) {
        self.poll();
        self.hits.clear();
        c.begin_frame(None);
        c.clear(plug::BG);
        c.fill_rect_gradient(Rect::new(0., 0., W as f32, 48.), plug::PANEL, plug::BG, 0.);
        c.fill_rrect(Rect::new(16., 15., 4., 24.), 2., brand::MAIN);
        c.text(Font::new(24).bold(), 30., 11., brand::TEXT, "LEMON");
        c.text(Font::new(24).bold(), 119., 11., brand::MAIN, "CHOP");
        c.text(Font::new(11), 201., 21., plug::DIM, "SAMPLING INSTRUMENT");
        let d = self.params.document();
        let missing = !d.path.is_empty()
            && self.shared.audio.lock().unwrap().is_none()
            && !self.shared.analysing.load(Ordering::Relaxed);
        let working = self.shared.analysing.load(Ordering::Relaxed)
            || self.shared.busy.load(Ordering::Relaxed);
        self.button_view(
            c,
            Rect::new(474., 12., 86., 30.),
            if working { "Cancel" } else { "Eject" },
            if working { "Cancel" } else { "Eject" },
            working,
        );
        self.button_view(
            c,
            Rect::new(566., 12., 102., 30.),
            "Import cues",
            "Import cues",
            false,
        );
        self.button_view(
            c,
            Rect::new(674., 12., 64., 30.),
            if missing { "Locate" } else { "Load" },
            if missing { "Locate" } else { "Load" },
            false,
        );
        self.button_view(c, Rect::new(744., 12., 64., 30.), "Undo", "Undo", false);
        self.button_view(c, Rect::new(814., 12., 64., 30.), "Redo", "Redo", false);
        self.button_view(c, Rect::new(884., 12., 64., 30.), "Export", "Export", false);
        self.button_view(c, Rect::new(954., 12., 70., 30.), "Stop", "Stop", false);
        let a = self
            .shared
            .audio
            .lock()
            .unwrap()
            .clone()
            .filter(|a| a.path == d.path);
        lemon::plug_panel(c, Rect::new(16., 52., 1008., 62.));
        let file = std::path::Path::new(&d.path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Drop an audio file here, or choose Load".into());
        c.text_clip(Font::new(17), 28., 61., 355., brand::TEXT, &file);
        let description = a
            .as_ref()
            .map(|a| {
                format!(
                    "{:.2} seconds   •   {} Hz   •   {} (estimated)",
                    a.duration(),
                    a.source.rate,
                    a.key
                )
            })
            .unwrap_or_else(|| {
                "32 pads • MIDI notes 36–67 • keyboard mode rooted at C4 (60)".into()
            });
        c.text_clip(Font::new(12), 28., 86., 355., plug::DIM, &description);
        self.param_view(c, Rect::new(398., 61., 116., 34.), SOURCE_BPM, "BPM");
        self.param_view(c, Rect::new(520., 61., 70., 34.), SYNC, "Sync");
        self.param_view(c, Rect::new(596., 61., 116., 34.), TARGET_BPM, "Tempo");
        self.param_view(c, Rect::new(718., 61., 98., 34.), KEY, "Key");
        self.param_view(c, Rect::new(822., 61., 98., 34.), DETUNE, "Cents");
        self.param_view(c, Rect::new(926., 61., 86., 34.), LINK, "Link");
        for (i, (label, id, on)) in [
            ("Snap", "Snap", self.snap),
            ("Freeze", "Freeze", self.frozen),
            ("Grid", "Grid", self.grid),
            ("−", "Zoom -", false),
            ("+", "Zoom +", false),
            ("Set start", "Set start", false),
            ("Set end", "Set end", false),
            ("Preview", "Preview", false),
        ]
        .into_iter()
        .enumerate()
        {
            self.button_view(
                c,
                Rect::new(16. + i as f32 * 80., 122., 74., 25.),
                label,
                id,
                on,
            );
        }
        if !self.grid {
            let pos = if self.shared.sounding.load(Ordering::Relaxed) != 0 {
                f64::from_bits(self.shared.position.load(Ordering::Relaxed))
            } else {
                self.cursor
            };
            c.text_center(
                Font::new(13),
                Rect::new(680., 122., 344., 25.),
                brand::TEXT,
                &format!(
                    "{:02}:{:05.2}    /    {:.1} s     •     {:.1}× zoom",
                    (pos / 60.) as usize,
                    pos % 60.,
                    a.as_ref().map_or(0., |a| a.duration()),
                    self.zoom
                ),
            );
        }
        if self.grid {
            for (i, (label, id)) in [
                ("Grid start", "Grid start"),
                ("Set anchor", "Grid marker"),
                ("Remove", "Remove marker"),
                ("BPM /2", "BPM /2"),
                ("BPM x2", "BPM x2"),
            ]
            .into_iter()
            .enumerate()
            {
                self.button_view(
                    c,
                    Rect::new(660. + i as f32 * 73., 122., 68., 25.),
                    label,
                    id,
                    false,
                );
            }
        }
        if self.grid {
            for (i, (label, id)) in [
                ("Tap", "Tap"),
                ("Click", "Click"),
                ("Slip <", "Slip <"),
                ("Slip >", "Slip >"),
                ("Adjust <", "Adjust <"),
                ("Adjust >", "Adjust >"),
                ("Save", "Grid save"),
                ("Cancel", "Grid cancel"),
                ("Clear", "Clear grid"),
            ]
            .into_iter()
            .enumerate()
            {
                self.button_view(
                    c,
                    Rect::new(16. + i as f32 * 88., 153., 82., 24.),
                    label,
                    id,
                    false,
                );
            }
        }
        let wave = Rect::new(16., 184., 1008., 174.);
        let overview = Rect::new(16., 366., 1008., 36.);
        lemon::plug_well(c, wave, 6.);
        lemon::plug_well(c, overview, 5.);
        if !self.grid {
            c.text(Font::new(11), 18., 158., plug::DIM, "SOURCE WAVEFORM");
            c.text_clip(
                Font::new(11),
                620.,
                158.,
                400.,
                plug::DIM,
                "Scroll to zoom  •  drag cue handles to trim",
            );
        }
        c.line(
            wave.x + 1.,
            wave.cy(),
            wave.x + wave.w - 1.,
            wave.cy(),
            1.,
            plug::TICK.with_alpha(70),
        );
        self.hits.push((wave, Target::Wave));
        self.hits.push((overview, Target::Overview));
        if let Some(a) = &a {
            let duration = a.duration();
            let span = duration / self.zoom;
            self.offset = self.offset.min((duration - span).max(0.));
            if !self.frozen && self.shared.sounding.load(Ordering::Relaxed) != 0 {
                let pos = f64::from_bits(self.shared.position.load(Ordering::Relaxed));
                if pos < self.offset || pos > self.offset + span {
                    self.offset = (pos - span * 0.25).clamp(0., (duration - span).max(0.));
                }
            }
            let peaks = self.shared.waveform.lock().unwrap().clone();
            let peaks = if peaks.is_empty() { &a.peaks } else { &peaks };
            for (r, offset, width) in [(wave, self.offset, span), (overview, 0., duration)] {
                for x in 0..1006 {
                    let f = (offset + x as f64 / 1006. * width) / duration;
                    let j = (f * 4096.) as usize;
                    if let Some(&(amp, high)) = peaks.get(j) {
                        let col = brand::MAIN.mix(Color::rgb(0x6daebd), high);
                        let h = (amp * r.h * 0.43).min(r.h * 0.46);
                        c.line(
                            r.x + 1. + x as f32,
                            r.cy() - h,
                            r.x + 1. + x as f32,
                            r.cy() + h,
                            1.,
                            col,
                        );
                    }
                }
                for (i, cue) in d.cues.iter().enumerate().filter(|(_, c)| c.enabled) {
                    let x = r.x + ((cue.start - offset) / width) as f32 * r.w;
                    if x >= r.x && x <= r.x + r.w {
                        let col = cue_color(&d, i);
                        c.line(x, r.y + 2., x, r.y + r.h - 2., 1., col.with_alpha(150));
                        if r.h > 100. && x + 32. < r.x + r.w {
                            let next = d
                                .cues
                                .iter()
                                .filter(|c| c.enabled && c.start > cue.start)
                                .map(|c| c.start)
                                .fold(offset + width, f64::min);
                            if (next - cue.start) / width * r.w as f64 > 36. {
                                c.fill_rrect(
                                    Rect::new(x + 10., r.y + 4., 22., 16.),
                                    3.,
                                    plug::WELL.mix(col, 0.3),
                                );
                                c.text_center(
                                    Font::new(10).bold(),
                                    Rect::new(x + 10., r.y + 4., 22., 16.),
                                    col,
                                    &format!("{:02}", i + 1),
                                );
                            }
                        }
                    }
                }
            }
            let bpm = self.params.v(SOURCE_BPM) as f64;
            let first = d.beat_at(self.offset, bpm).ceil() as i64;
            for b in first..first + 2048 {
                let time = d.time_at(b as f64, bpm);
                if time > self.offset + span {
                    break;
                }
                let x = wave.x + ((time - self.offset) / span) as f32 * wave.w;
                if x >= wave.x && x <= wave.x + wave.w {
                    c.line(
                        x,
                        wave.y,
                        x,
                        wave.y + if b % 4 == 0 { wave.h } else { 12. },
                        1.,
                        plug::TICK.with_alpha(140),
                    );
                    if self.grid {
                        self.hits.push((
                            Rect::new((x - 4.).max(wave.x), wave.y + 22., 8., wave.h - 50.),
                            Target::Beat(b as f64),
                        ));
                    }
                }
            }
            if self.grid {
                for (i, &time) in d.markers.iter().enumerate() {
                    let x = wave.x + ((time - self.offset) / span) as f32 * wave.w;
                    if (wave.x..wave.x + wave.w).contains(&x) {
                        c.fill_rrect(
                            Rect::new(x - 6., wave.y + wave.h - 20., 12., 20.),
                            2.,
                            brand::MAIN,
                        );
                        c.text(
                            Font::new(11),
                            x + 8.,
                            wave.y + wave.h - 19.,
                            brand::TEXT,
                            &format!("Beat {:.0}", d.marker_beat(i) + 1.),
                        );
                        self.hits.push((
                            Rect::new(x - 7., wave.y + wave.h - 24., 14., 24.),
                            Target::Marker(i),
                        ));
                    }
                }
            }
            let cue = &d.cues[d.selected];
            if cue.enabled {
                let x1 = wave.x + ((cue.start - self.offset) / span) as f32 * wave.w;
                let x2 = wave.x + ((cue.end - self.offset) / span) as f32 * wave.w;
                let l = x1.clamp(wave.x, wave.x + wave.w);
                let r = x2.clamp(wave.x, wave.x + wave.w);
                if r > l {
                    c.fill_rect(
                        Rect::new(l, wave.y + 1., r - l, wave.h - 2.),
                        cue_color(&d, d.selected).with_alpha(35),
                    );
                }
                for (x, target) in [(x1, Target::Start), (x2, Target::End)] {
                    if (wave.x..=wave.x + wave.w).contains(&x) {
                        c.line(x, wave.y, x, wave.y + wave.h, 2., cue_color(&d, d.selected));
                        c.fill_rrect(
                            Rect::new(
                                (x - 5.).clamp(wave.x, wave.x + wave.w - 10.),
                                wave.y,
                                10.,
                                18.,
                            ),
                            2.,
                            cue_color(&d, d.selected),
                        );
                        self.hits.push((
                            Rect::new(x - 6., wave.y, 12., if self.grid { 20. } else { wave.h }),
                            target,
                        ));
                    }
                }
            }
            let pos = if self.shared.sounding.load(Ordering::Relaxed) != 0 {
                f64::from_bits(self.shared.position.load(Ordering::Relaxed))
            } else {
                self.cursor
            };
            let x = wave.x + ((pos - self.offset) / span) as f32 * wave.w;
            if x >= wave.x && x <= wave.x + wave.w {
                c.line(
                    x,
                    wave.y,
                    x,
                    wave.y + wave.h,
                    4.,
                    brand::TEXT.with_alpha(25),
                );
                c.line(x, wave.y, x, wave.y + wave.h, 1., brand::TEXT);
            }
            let x = overview.x + (self.offset / duration) as f32 * overview.w;
            let width = (span / duration) as f32 * overview.w;
            c.stroke_rrect(
                Rect::new(x, overview.y, width, overview.h),
                3.,
                1.,
                brand::TEXT,
            );
        } else {
            c.text_center(
                Font::new(21),
                wave,
                brand::TEXT,
                "Drop a sample. Find your next idea.",
            );
        }
        if a.is_none() {
            c.text_center(
                Font::new(12),
                Rect::new(16., 294., 1008., 24.),
                plug::DIM,
                "Drag audio from your browser or Explorer, or choose Load above",
            );
        }
        for (i, label) in ["Vocals", "Drums", "Bass", "Instruments"]
            .into_iter()
            .enumerate()
        {
            self.param_view(
                c,
                Rect::new(16. + i as f32 * 119., 414., 113., 27.),
                12 + i,
                label,
            );
        }
        let busy = self.shared.busy.load(Ordering::Relaxed);
        self.button_view(
            c,
            Rect::new(498., 414., 124., 27.),
            if busy { "Cancel job" } else { "Separate stems" },
            if busy { "Cancel" } else { "Separate" },
            busy,
        );
        self.param_view(
            c,
            Rect::new(636., 414., 70., 27.),
            HOLD,
            if self.params.v(HOLD) > 0.5 {
                "Hold"
            } else {
                "Trigger"
            },
        );
        self.param_view(
            c,
            Rect::new(712., 414., 70., 27.),
            POLY,
            if self.params.v(POLY) > 0.5 {
                "Poly"
            } else {
                "Mono"
            },
        );
        self.param_view(c, Rect::new(788., 414., 90., 27.), KEYBOARD, "Keyboard");
        self.param_view(c, Rect::new(884., 414., 80., 27.), VELOCITY, "Velocity");
        lemon::plug_panel(c, Rect::new(16., 456., 1008., 120.));
        let n = d.selected;
        let count = self.selection.count_ones();
        let scope = if count == 32 {
            "ALL PADS".into()
        } else if count > 1 {
            format!("{count} PADS SELECTED")
        } else {
            format!(
                "PAD {:02}  /  {}",
                n + 1,
                if d.cues[n].enabled {
                    d.cues[n].name.as_str()
                } else {
                    "EMPTY"
                }
            )
        };
        let accent = if count > 1 {
            brand::TEXT
        } else {
            cue_color(&d, n)
        };
        c.fill_circle(30., 470., 3., accent);
        c.text_clip(Font::new(11).bold(), 40., 462., 690., accent, &scope);
        c.text(Font::new(10), 765., 462., plug::DIM, "PLAYBACK & OUTPUT");
        for (i, (field, label)) in [
            (LEVEL, "LEVEL"),
            (FILTER, "FILTER"),
            (ATTACK, "ATTACK"),
            (RELEASE, "RELEASE"),
            (PITCH, "PITCH"),
            (SPEED, "SPEED"),
        ]
        .into_iter()
        .enumerate()
        {
            let k = pad(n, field);
            let cx = 62. + i as f32 * 122.;
            let v = self.control_value(k);
            let def = &self.params.defs[k];
            let norm = (v - def.min) / (def.max - def.min);
            let state = self.visual_state(&Target::Param(k), false);
            lemon::dial(
                c,
                cx,
                518.,
                25.,
                3.,
                norm,
                def.min < 0. && def.min == -def.max,
                accent,
                state,
            );
            c.text_center(
                Font::new(11),
                Rect::new(cx - 49., 482., 98., 18.),
                plug::DIM,
                label,
            );
            let text = match field {
                ATTACK | RELEASE => format!("{:.0} ms", v * 1000.),
                SPEED => format!("{v:.2}×"),
                PITCH => format!("{v:+.0} st"),
                LEVEL => format!("{v:+.1} dB"),
                FILTER if v.abs() < 0.01 => "Off".into(),
                FILTER => format!(
                    "{} {:.0}%",
                    if v < 0. { "LP" } else { "HP" },
                    v.abs() * 100.
                ),
                _ => format!("{v:.2}"),
            };
            c.text_center(
                Font::new(12),
                Rect::new(cx - 49., 548., 98., 20.),
                brand::TEXT,
                &text,
            );
            self.hits
                .push((Rect::new(cx - 45., 480., 90., 90.), Target::Param(k)));
        }
        self.param_view(
            c,
            Rect::new(760., 488., 84., 28.),
            pad(n, REVERSE),
            "Reverse",
        );
        self.button_view(
            c,
            Rect::new(852., 488., 78., 28.),
            "Favorite",
            "Favorite",
            d.cues[n].favorite,
        );
        self.param_view(c, Rect::new(938., 488., 74., 28.), MASTER, "dB");
        self.param_view(
            c,
            Rect::new(760., 536., 126., 26.),
            pad(n, OUTPUT),
            "Mixer offset",
        );
        self.param_view(c, Rect::new(892., 536., 120., 26.), GLIDE, "Glide");
        self.button_view(
            c,
            Rect::new(594., 590., 128., 28.),
            if d.glide_trigger {
                "Glide: Trigger"
            } else {
                "Glide: Sustain"
            },
            "Glide mode",
            false,
        );
        self.button_view(
            c,
            Rect::new(728., 590., 110., 28.),
            "Controller",
            "SLAB",
            d.slab,
        );
        let mode = ["Find transients", "Beat slicer", "Random", "Key shift pads"][self.mode];
        self.button_view(c, Rect::new(16., 590., 150., 28.), mode, "Mode", false);
        self.button_view(
            c,
            Rect::new(172., 590., 84., 28.),
            "Autoset",
            "Autoset",
            true,
        );
        self.button_view(c, Rect::new(264., 590., 42., 28.), "−", "Size -", false);
        c.text_center(
            Font::new(12),
            Rect::new(309., 590., 78., 28.),
            brand::TEXT,
            &format!("{} beats", self.beats),
        );
        self.button_view(c, Rect::new(390., 590., 42., 28.), "+", "Size +", false);
        self.button_view(
            c,
            Rect::new(440., 590., 70., 28.),
            "Region <",
            "Region <",
            false,
        );
        self.button_view(
            c,
            Rect::new(516., 590., 70., 28.),
            "Region >",
            "Region >",
            false,
        );
        self.button_view(
            c,
            Rect::new(846., 590., 84., 28.),
            "All pads",
            "All",
            self.selection == u32::MAX,
        );
        self.button_view(
            c,
            Rect::new(938., 590., 86., 28.),
            "Delete",
            "Delete",
            false,
        );
        let sounding = self.shared.sounding.load(Ordering::Relaxed);
        for i in 0..32 {
            let cue = &d.cues[i];
            let r = Rect::new(
                16. + (i % 8) as f32 * 127.,
                632. + (i / 8) as f32 * 50.,
                119.,
                44.,
            );
            let col = cue_color(&d, i);
            let selected = self.selection & (1 << i) != 0;
            let playing = sounding & (1 << i) != 0;
            let state = self.visual_state(&Target::Pad(i), selected);
            let face = if playing {
                col
            } else if selected {
                plug::FIELD.mix(col, 0.22)
            } else if state.hover {
                plug::FIELD.mix(col, 0.13)
            } else {
                plug::FIELD
            };
            c.fill_rrect(Rect::new(r.x, r.y + 2., r.w, r.h), 7., plug::WELL);
            c.fill_rect_gradient(
                r,
                face.mix(brand::TEXT, if playing { 0.15 } else { 0.04 }),
                face,
                7.,
            );
            c.stroke_rrect(
                r.inset(0.5),
                6.5,
                if selected || playing { 1.5 } else { 1. },
                if selected || playing || state.hover {
                    col
                } else {
                    plug::PANEL_LINE
                },
            );
            if cue.enabled {
                c.fill_rrect(Rect::new(r.x + 10., r.y + 7., 22., 3.), 1.5, col);
            }
            let ink = if playing {
                plug::WELL
            } else if cue.enabled {
                brand::TEXT
            } else {
                plug::DIM
            };
            c.text(
                Font::new(17).bold(),
                r.x + 9.,
                r.y + 15.,
                ink,
                &format!("{:02}", i + 1),
            );
            c.text_clip(
                Font::new(12).bold(),
                r.x + 37.,
                r.y + 7.,
                75.,
                ink,
                if cue.enabled { &cue.name } else { "Empty" },
            );
            c.text(
                Font::new(10),
                r.x + 37.,
                r.y + 26.,
                if playing { plug::WELL } else { plug::DIM },
                &format!(
                    "{}{}",
                    note_name(36 + i),
                    if cue.favorite { "  ★" } else { "" }
                ),
            );
            if self.pad_drop_target() == Some(i) {
                c.stroke_rrect(r.inset(3.), 4., 2., brand::TEXT);
            }
            self.hits.push((r, Target::Pad(i)));
        }
        c.line(16., 834., 1024., 834., 1., plug::PANEL_LINE);
        c.text_clip(
            Font::new(11),
            16.,
            841.,
            1008.,
            plug::DIM,
            &self.interaction_hint(),
        );
        let _ = t;
    }
}

fn note_name(note: usize) -> String {
    format!(
        "{}{}",
        [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"
        ][note % 12],
        note as i32 / 12 - 1
    )
}
