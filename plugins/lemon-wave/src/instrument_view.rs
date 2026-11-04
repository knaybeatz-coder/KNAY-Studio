// SPDX-License-Identifier: GPL-3.0-or-later
//! Dedicated Lemon Wave instrument view. Uses the same hit targets and host gestures
//! as the shared editor; graphs are parameter-derived, never decorative fake meters.
use crate::{assets::WaveParams, controls::*, reference_oscillator::Warp};
use lemon_small_fx::view::{Hit, Model, Target, ViewState};
use os_ui::{Canvas, Color, Font, Rect};
fn font(size: u16) -> Font {
    Font::light(crate::design::font_size(size + 1), 0).natural()
}
fn theme_bg() -> Color { crate::design::color(0x101319) }
fn theme_panel() -> Color { crate::design::color(0x202a33) }
fn theme_line() -> Color { crate::design::color(0x303a49) }
fn theme_ink() -> Color { crate::design::color(0xe8edf3) }
fn theme_dim() -> Color { crate::design::color(0x8d9baa) }
fn theme_a() -> Color { crate::design::color(0xe3df8b) }
fn theme_b() -> Color { crate::design::color(0x73dacc) }
fn theme_mod() -> Color { crate::design::color(0xbda4fb) }
pub const PRESET_PREV: Rect = Rect::new(414., 18., 30., 32.);
pub const PRESET_NEXT: Rect = Rect::new(724., 18., 30., 32.);
pub const PRESET_INIT: Rect = Rect::new(768., 18., 70., 32.);
pub const PRESET_NAMES: [&str; 14] = [
    "Init",
    "Bend Bass",
    "Mirror Keys",
    "Quantize Motion",
    "Wide Lead",
    "Dual Sweep",
    "Wide Sky",
    "Moving Edge",
    "Velvet Pluck",
    "Held Bloom",
    "Resonant Band",
    "Glass Tide",
    "Clockwork",
    "Four Corners",
];
pub fn edit_table_button(o: usize) -> Rect {
    Rect::new(295. + 410. * o as f32, 84., 105., 23.)
}
pub fn table_buttons(o: usize) -> [Rect; 3] {
    let x = 28. + o as f32 * 410.;
    [
        Rect::new(x, 418., 105., 20.),
        Rect::new(x + 111., 418., 140., 20.),
        Rect::new(x + 257., 418., 117., 20.),
    ]
}
pub fn raised(c: &mut Canvas, r: Rect, radius: f32, base: Color) {
    let radius = radius * crate::design::radius();
    let bevel = crate::design::bevel().min(0.035);
    c.fill_rrect(
        Rect::new(r.x + 1., r.y + crate::design::shadow().min(1.5), r.w, r.h),
        radius,
        crate::design::color(0x080c10).with_alpha(100),
    );
    c.fill_rect_gradient(r, base.shade(bevel), base.shade(-bevel), radius);
    c.stroke_rrect(r.inset(0.5), radius, 1., base.shade(0.10));
    c.polyline(
        &[(r.x + radius, r.y + 1.5), (r.right() - radius, r.y + 1.5)],
        1.,
        crate::design::color(0xc4d4dd).with_alpha(15),
    );
}
pub fn dropdown(c: &mut Canvas, r: Rect, label: &str, color: Color, hot: bool) {
    raised(c, r, 4., crate::design::color(if hot { 0x354450 } else { 0x24313b }));
    c.text_clip(font(10), r.x + 9., r.y + 7., r.w - 29., color, label);
    let x = r.right() - 13.;
    let y = r.cy();
    c.polyline(
        &[(x - 3., y - 1.5), (x, y + 1.5), (x + 3., y - 1.5)],
        1.5,
        color,
    );
}
struct Ui<'a> {
    live: crate::modulation::Live,
    c: &'a mut Canvas,
    m: &'a dyn Model,
    st: &'a ViewState,
    hits: Vec<Hit>,
}
impl Ui<'_> {
    fn label(&mut self, x: f32, y: f32, s: &str, size: u32, color: Color) {
        self.c.text(
            if size >= 12 {
                font(size as _).bold()
            } else {
                font(size as _)
            },
            x,
            y,
            color,
            s,
        );
    }
    fn panel(&mut self, r: Rect, title: &str, accent: Color, subtitle: &str) {
        let r = r.inset(-3.);
        raised(self.c, r, 8., theme_panel());
        self.c.fill_rrect(Rect::new(r.x+1.,r.y+1.,r.w-2.,32.),6.,crate::design::color(0x25313b));
        self.c.polyline(&[(r.x+10.,r.y+33.),(r.right()-10.,r.y+33.)],0.7,crate::design::color(0x35424c).with_alpha(100));
        self.c
            .fill_rrect(Rect::new(r.x + 12., r.y + 13., 3., 15.), 1.5, accent);
        self.label(r.x + 24., r.y + 12., title, 12, theme_ink());
        if !subtitle.is_empty() {
            self.label(r.x + 24., r.y + 31., subtitle, 10, theme_dim());
        }
    }
    fn hit(&mut self, r: Rect, t: Target) {
        self.hits.push(Hit { r, t });
    }
    fn hover(&self, r: Rect) -> bool {
        self.st.mouse.is_some_and(|(x, y)| r.contains(x, y))
    }
    fn knob(&mut self, k: usize, x: f32, y: f32, caption: &str, color: Color) {
        let id=SETTINGS[k].id;
        let (x,y)=crate::design::knob(id,x,y);
        let n=self.m.norm(id).clamp(0.,1.);
        let r=Rect::new(x-24.,y-25.,48.,65.);
        let hot=self.hover(r)||self.st.pressed.as_ref().is_some_and(|t|t.id()==id);
        let arc=|lo:f32,hi:f32,rad:f32|->Vec<(f32,f32)>{
            (0..=40).map(|i|{let a=(135.+270.*(lo+(hi-lo)*i as f32/40.)).to_radians();(x+rad*a.cos(),y+rad*a.sin())}).collect()
        };
        self.c.polyline(&arc(0.,1.,24.),2.,theme_line());
        let zero=if SETTINGS[k].range.0<0 {0.5} else {0.};
        self.c.polyline(&arc(zero,n,24.),1.8,color);
        if let Some(target)=mod_target(k) {
            let depth:f32=ROUTES.iter().filter(|slot|self.m.plain(SETTINGS[**slot].id)>0.&&self.m.plain(SETTINGS[*slot+1].id) as usize==target)
                .map(|slot|self.m.plain(SETTINGS[slot+2].id).abs()/1000.).sum();
            if depth>0. {
                self.c.polyline(&arc((n-depth).max(0.),(n+depth).min(1.),26.),1.,theme_mod());
                if self.live.active {
                    let offset=self.live.amounts[target];
                    let v=(if target==7 {n*(1.+offset)} else if target==6 {n+offset*0.5} else {n+offset}).clamp(0.,1.);
                    self.c.polyline(&arc(n.min(v),n.max(v),26.),2.,theme_mod());
                }
            }
        }
        self.c.fill_circle(x+1.,y+2.,22.,crate::design::color(0x060a0e));
        self.c.fill_rect_gradient(Rect::new(x-22.,y-22.,44.,44.),crate::design::color(0x586a76),crate::design::color(0x24323d),22.);
        self.c.stroke_circle(x,y,21.5,0.8,crate::design::color(0xa1afb7).with_alpha(65));
        // The entire center is a shallow dish in the knob, not a value insert.
        self.c.fill_rect_gradient(Rect::new(x-18.,y-18.,36.,36.),
            crate::design::color(if hot {0x263c49} else {0x1b2b36}),
            crate::design::color(if hot {0x4b606c} else {0x3c505c}),18.);
        self.c.polyline(&arc(0.16,0.5,18.),0.8,crate::design::color(0x12212c).with_alpha(150));
        self.c.polyline(&arc(0.66,1.,18.),0.7,crate::design::color(0x80929b).with_alpha(45));
        let a=(135.+270.*n).to_radians();
        self.c.polyline(&[(x+18.*a.cos(),y+18.*a.sin()),(x+21.*a.cos(),y+21.*a.sin())],2.,theme_ink());
        let well=Rect::new(x-15.,y-6.,30.,12.);
        let text=self.m.text(id).replace(" voices","").replace(" cents","ct").replace(" deg","\u{00b0}");
        let mut value=text.replace(".0%","%").replace(".00 "," ").replace(' ',"");
        if let Some(hz)=value.strip_suffix("Hz").and_then(|v|v.parse::<f32>().ok()) {
            value=if hz>=1000. {format!("{:.1}kHz",hz/1000.).replace(".0k","k")} else {format!("{hz}Hz")};
        }
        let mut size=9;
        while size>6 && self.c.text_width(font(size),&value)>well.w-4. {size-=1;}
        self.c.text_center(font(size),well,theme_ink(),&value);
        self.c.text_center(font(10),Rect::new(x-30.,y+28.,60.,12.),theme_ink(),caption);
        self.hit(r,Target::Knob(id.into()));
    }
    fn level_fader(&mut self,k:usize,x:f32,top:f32,bottom:f32,color:Color) {
        let id=SETTINGS[k].id;
        let n=self.m.norm(id).clamp(0.,1.);
        let y=bottom-n*(bottom-top);
        for i in 0..=4 {
            let yy=top+(bottom-top)*i as f32/4.;
            self.c.fill_rect(Rect::new(x-22.,yy,7.,1.),theme_line());
            self.c.fill_rect(Rect::new(x+15.,yy,7.,1.),theme_line());
        }
        self.c.fill_rrect(Rect::new(x-4.,top-5.,8.,bottom-top+10.),4.,crate::design::color(0x070c10));
        self.c.fill_rrect(Rect::new(x-1.,y,2.,(bottom-y).max(1.)),1.,color.with_alpha(145));
        let cap=Rect::new(x-13.,y-7.,26.,14.);
        self.c.fill_rect_gradient(cap,crate::design::color(0xb1bdc4),crate::design::color(0x52636e),3.);
        self.c.stroke_rrect(cap,3.,1.,crate::design::color(0x17212a));
        self.c.fill_rect(Rect::new(x-9.,y,18.,1.),crate::design::color(0x18232a));
        self.c.text_center(font(9),Rect::new(x-29.,bottom+13.,58.,12.),theme_ink(),"Level");
        self.c.text_center(font(9),Rect::new(x-29.,bottom+26.,58.,12.),color,&self.m.text(id));
        self.hit(Rect::new(x-25.,top-8.,50.,bottom-top+16.),Target::Fader(id.into(),bottom-top));
    }
    fn select(&mut self, k: usize, r: Rect, color: Color) {
        let id = SETTINGS[k].id;
        let r = crate::design::selector(id, r);
        dropdown(self.c, r, &self.m.text(id), color, self.hover(r));
        self.hit(
            r,
            Target::Choice(id.into(), self.m.plain(id).round() as i32),
        );
    }
    fn grid(&mut self, r: Rect) {
        let r = Rect::new(r.x-6.,r.y,r.w+12.,r.h);
        self.c.fill_rrect(r, 5., crate::design::color(0x50616b));
        self.c
            .fill_rect_gradient(r.inset(1.), crate::design::color(0x080e14), crate::design::color(0x15232c), 4.);
        self.c
            .stroke_rrect(r.inset(1.5), 4., 1., crate::design::color(0x020609));
        for i in 1..8 {
            let x = r.x + r.w * i as f32 / 8.;
            self.c
                .fill_rect(Rect::new(x, r.y + 4., 1., r.h - 8.), crate::design::color(0x202a36));
        }
        for i in 1..4 {
            let y = r.y + r.h * i as f32 / 4.;
            self.c
                .fill_rect(Rect::new(r.x + 4., y, r.w - 8., 1.), crate::design::color(0x202a36));
        }
    }
    fn env(&mut self, base: usize, r: Rect, color: Color) {
        self.grid(r);
        let reference = self.m.plain("env_engine") > 0.5;
        let key = if base == ATTACK { AMP_HOLD } else { ENV_HOLD };
        let a = seconds(self.m.plain(SETTINGS[base].id));
        let d = seconds(self.m.plain(SETTINGS[base + 1].id));
        let mut sustain = self.m.plain(SETTINGS[base + 2].id) / 1000.;
        if reference {
            sustain *= sustain;
        }
        let release = seconds(self.m.plain(SETTINGS[base + 3].id));
        let hold = if reference {
            hold_seconds(self.m.plain(SETTINGS[key].id))
        } else {
            0.
        };
        let gate = (a + d + release).max(0.1) * 0.3;
        let total = a + hold + d + gate + release;
        let shape = |i: usize| {
            if reference {
                self.m.plain(SETTINGS[key + 1 + i].id) as f64 / 1000.
            } else {
                0.5
            }
        };
        let mut pts = Vec::new();
        for (start, length, from, to, curve) in [
            (0., a, 0., 1., shape(0)),
            (a, hold, 1., 1., 0.5),
            (a + hold, d, 1., sustain, shape(1)),
            (a + hold + d, gate, sustain, sustain, 0.5),
            (a + hold + d + gate, release, sustain, 0., shape(2)),
        ] {
            for i in 0..=40 {
                let t = i as f64 / 40.;
                let v = to + (from - to) * crate::reference_envelope::falling(curve, t);
                pts.push((
                    r.x + 8. + (r.w - 16.) * (start + length * t as f32) / total,
                    r.bottom() - 9. - (r.h - 18.) * v,
                ));
            }
        }
        self.c.polyline(&pts, 1.8, color);
        let i = if base == ATTACK { 0 } else { 1 };
        let stage = self.live.env_stage[i];
        let elapsed = self.live.env_elapsed[i];
        if self.live.active && stage != 0 {
            let time = match stage {
                1 => elapsed.min(a),
                5 => a + elapsed.min(hold),
                2 => a + hold + elapsed.min(d),
                3 => a + hold + d + gate * 0.5,
                4 => a + hold + d + gate + elapsed.min(release),
                _ => 0.,
            };
            let x = r.x + 8. + (r.w - 16.) * time / total;
            self.c
                .polyline(&[(x, r.y + 4.), (x, r.bottom() - 4.)], 1., theme_ink());
        }
    }
}
pub fn paint(
    c: &mut Canvas,
    m: &dyn Model,
    st: &ViewState,
    hint: &str,
    params: &WaveParams,
    cycles: [usize; 2],
    preset: Option<usize>,
) -> Vec<Hit> {
    let mut u = Ui {
        c,
        m,
        st,
        hits: Vec::new(),
        live: params.telemetry.read(),
    };
    u.c.begin_frame(None);
    u.c.clear(theme_bg());
    u.c.fill_rect_gradient(
        Rect::new(0., 0., 1180., 66.),
        crate::design::color(0x3a4851),
        crate::design::color(0x17212a),
        0.,
    );
    u.c.fill_rect(Rect::new(0., 65., 1180., 1.), crate::design::color(0x6b796f));
    for y in (68..820).step_by(4) {
        u.c.fill_rect(
            Rect::new(0., y as f32, 1180., 1.),
            crate::design::color(0x8da19f).with_alpha(5),
        );
    }
    u.label(23., 17., "LEMON", 12, theme_a());
    u.label(23., 32., "WAVE", 23, theme_ink());
    let page = params.page.load(std::sync::atomic::Ordering::Relaxed);
    for (i, label) in ["SYNTH", "MOD", "TABLE EDIT"].iter().enumerate() {
        let r = page_button(i);
        raised(
            u.c,
            r,
            4.,
            if page == i {
                crate::design::color(0x4c5943)
            } else {
                crate::design::color(0x27343f)
            },
        );
        u.c.text_center(font(11), r, if page == i { theme_a() } else { theme_dim() }, label);
    }

    if page != 2 {
        let title = preset
            .map(|i| PRESET_NAMES[i])
            .unwrap_or("Choose a factory preset");
        for (r, t) in [
            (PRESET_PREV, "‹"),
            (PRESET_NEXT, "›"),
            (PRESET_INIT, "Init"),
        ] {
            raised(
                u.c,
                r,
                5.,
                crate::design::color(if u.hover(r) { 0x43525c } else { 0x2e3b46 }),
            );
            u.c.text_center(font(13), r, theme_ink(), t);
        }
        dropdown(u.c, Rect::new(450., 18., 268., 32.), title, theme_ink(), false);
        u.select(ENV_ENGINE, Rect::new(850., 14., 210., 27.), theme_mod());
        u.label(878., 45., "ENVELOPES 1 + 2 / NEXT NOTE", 9, theme_dim());
        u.label(1080., 27., "LEMON STUDIO", 9, theme_dim());
    } else {
        u.label(
            440.,
            28.,
            "WAVETABLE WORKSHOP  /  APPLY THE DRAFT TO HEAR YOUR EDITS",
            12,
            theme_dim(),
        );
    }
    if page == 1 {
        paint_modulation(&mut u, params);
        return u.hits;
    }
    if page == 2 {
        return u.hits;
    }
    let tables = params.tables.read().unwrap();
    for o in 0..2 {
        let x = 16. + 410. * o as f32;
        let color = if o == 0 { theme_a() } else { theme_b() };
        let base = o * 8;
        let source = if o == 0 { A_SOURCE } else { B_SOURCE };
        let phase = if o == 0 { A_PHASE } else { B_PHASE };
        u.panel(
            Rect::new(x, 80., 398., 362.),
            if o == 0 {
                "OSCILLATOR A"
            } else {
                "OSCILLATOR B"
            },
            color,
            "",
        );
        let edit = edit_table_button(o);
        raised(u.c, edit, 3., crate::design::color(0x344237));
        u.c.text_center(font(9), edit, color, "EDIT TABLE");
        let reference = m.plain(SETTINGS[source].id) > 0.5;
        let table = &tables.oscillators[o];
        let sample = &tables.samples[o];
        let label = if reference {
            "Reference / Saw".to_string()
        } else if let Some(s) = sample {
            s.source
                .name
                .rsplit(" / ")
                .next()
                .unwrap_or(&s.source.name)
                .to_string()
        } else if let Some(t) = table {
            t.source.name.clone()
        } else {
            ["Basic Morph", "Hollow", "Glass", "Vowel"][(m.plain(SETTINGS[base].id) as usize).min(3)].to_string()
        };
        let browse = crate::sound_browser::button(o);
        dropdown(u.c, browse, &label, theme_ink(), u.hover(browse));
        let r = Rect::new(x + 12., 118., 374., 106.);
        u.grid(r);
        let position = m.plain(SETTINGS[base + 1].id) / 1000.;
        let bank = m.plain(SETTINGS[base].id) as usize;
        let mode = m.plain(SETTINGS[source + 1].id) as usize;
        let amount = (m.plain(SETTINGS[source + 2].id) / 1000.
            + m.plain(SETTINGS[base + 7].id) / 200.)
            .clamp(0., 1.);
        let warp = Warp::from_reference_id(WARP_IDS[mode.min(10)]).unwrap();
        let points: Vec<_> = (0..360)
            .map(|i| {
                let p = i as f32 / 359.;
                let y = if let Some(s) = sample.as_ref().filter(|_| !reference) {
                    s.source.samples[((p * (s.source.samples.len() - 1) as f32) as usize)
                        .min(s.source.samples.len() - 1)]
                } else if reference {
                    let (q, g) = warp.apply((p as f64).min(1. - f64::EPSILON), amount);
                    (((q + 0.5).fract() * 2. - 1.) as f32) * g
                } else {
                    let pivot = 0.5 + m.plain(SETTINGS[base + 7].id) / 200.;
                    let q = if p < pivot {
                        p / pivot * 0.5
                    } else {
                        0.5 + (p - pivot) / (1. - pivot) * 0.5
                    };
                    table
                        .as_ref()
                        .map(|t| t.sample(position, q.fract(), 20., 48000.))
                        .unwrap_or_else(|| crate::dsp::preview(bank, position, q.fract()))
                };
                (r.x + 7. + i as f32, r.y + 51. - y.clamp(-1., 1.) * 30.)
            })
            .collect();
        // Adjacent frame shapes behind the result make Position's morph readable.
        if !reference && sample.is_none() {
            let frames = table.as_ref().map(|t| t.source.frames()).unwrap_or(5);
            if frames > 1 {
                let frame = position * (frames - 1) as f32;
                let lo = frame.floor() / (frames - 1) as f32;
                let hi = (frame.ceil() / (frames - 1) as f32).min(1.);
                for (pos, alpha) in [(lo, 65), (hi, 105)] {
                    let trace: Vec<_> = (0..360)
                        .map(|i| {
                            let phase = i as f32 / 359.;
                            let y = table
                                .as_ref()
                                .map(|t| t.sample(pos, phase, 20., 48000.))
                                .unwrap_or_else(|| crate::dsp::preview(bank, pos, phase));
                            (r.x + 7. + i as f32, r.y + 51. - y.clamp(-1., 1.) * 30.)
                        })
                        .collect();
                    u.c.polyline(&trace, 1., color.with_alpha(alpha));
                }
            }
        }
        u.c.polyline(&points, 4., color.with_alpha(25));
        u.c.polyline(&points, 1.5, color);
        let name = if let Some(s) = sample.as_ref().filter(|_| !reference) {
            format!("SAMPLE: {} | Root {:.1}", s.source.name, s.source.root)
        } else if reference {
            "INIT SAW  /  WARP PREVIEW".into()
        } else {
            table
                .as_ref()
                .map(|t| format!("{}  ·  {} frames", t.source.name, t.source.frames()))
                .unwrap_or_else(|| {
                    format!("WAVETABLE: {}", crate::controls::text(base, bank as i32))
                })
        };
        u.c.text_clip(font(9), r.x + 7., r.y + 4., r.w - 14., theme_dim(), &name);
        let count = (m.plain(SETTINGS[base + 4].id) as usize).clamp(1, 7);
        let blend = (m.plain(SETTINGS[phase + 2].id) / 1000.
            + if u.live.active {
                u.live.amounts[19 + o]
            } else {
                0.
            })
        .clamp(0., 1.);
        u.c.fill_rect(
            Rect::new(r.x + 3., r.bottom() - 21., r.w - 6., 18.),
            crate::design::color(0x0a141c),
        );
        u.c.text(
            font(8),
            r.x + 7.,
            r.bottom() - 16.,
            theme_dim(),
            if count > 2 {
                "UNISON BLEND"
            } else {
                "UNISON VOICES"
            },
        );
        let weights: Vec<_> = (0..count)
            .map(|i| {
                let w = crate::dsp::blend_weight(count, i, blend);
                if reference {
                    w
                } else {
                    w / crate::dsp::blend_weight(count, i, 0.75)
                }
            })
            .collect();
        let peak = weights.iter().copied().fold(1.0_f32, f32::max);
        for (i, w) in weights.iter().enumerate() {
            let x = r.x + 114. + i as f32 * 25.;
            let h = (*w / peak * 13.).max(1.);
            let central = if count % 2 == 1 {
                i == count / 2
            } else {
                i == count / 2 || i + 1 == count / 2
            };
            u.c.fill_rect_gradient(
                Rect::new(x, r.bottom() - 5. - h, 15., h),
                if central { theme_ink() } else { color },
                color.with_alpha(75),
                2.,
            );
        }
        u.c.text(
            font(8),
            r.right() - 61.,
            r.bottom() - 16.,
            theme_dim(),
            &format!("{} VOICES", count),
        );

        // Keep sound construction on the left and the oscillator's output strip on the right.
        for panel in [Rect::new(x+10.,232.,302.,83.),Rect::new(x+10.,320.,302.,95.),Rect::new(x+320.,232.,68.,179.)] {
            u.c.fill_rrect(panel,5.,crate::design::color(0x1b2730));
            u.c.stroke_rrect(panel,5.,0.8,crate::design::color(0x3b4b56));
        }
        u.label(x+20.,239.,"PITCH & VOICES",8,theme_dim());
        for (j,(k,caption)) in [(base+1,"Position"),(base+3,"Tune"),(base+4,"Unison"),(base+5,"Detune"),(phase+2,"Blend")].into_iter().enumerate() {
            u.knob(k,x+43.+57.*j as f32,274.,caption,color);
        }
        u.label(x+20.,330.,"WAVE SHAPING",8,theme_dim());
        if let Some(s)=sample.as_ref().filter(|_|!reference) {
            u.c.text_center(font(8),Rect::new(x+151.,324.,152.,21.),theme_dim(),&format!("ONE SHOT / ROOT {:.1}",s.source.root));
        } else {
            let width = u.c.text_width(font(10), "Quantize").ceil() + 30.;
            let menu = Rect::new(x+161.-width/2.,324.,width,16.);
            let id = SETTINGS[source+1].id;
            // Recessed field: keep the bottom above the knob's outer modulation ring.
            u.c.fill_rrect(Rect::new(menu.x,menu.y+1.,menu.w,menu.h),3.,crate::design::color(0x52636e));
            u.c.fill_rect_gradient(menu,crate::design::color(0x0b141b),crate::design::color(0x1c2a34),3.);
            u.c.stroke_rrect(menu,3.,0.8,crate::design::color(0x111c24));
            u.c.text_clip(font(10),menu.x+8.,menu.y+3.,menu.w-27.,color,&m.text(id));
            let arrow=menu.right()-11.;
            u.c.polyline(&[(arrow-3.,menu.cy()-1.),(arrow,menu.cy()+2.),(arrow+3.,menu.cy()-1.)],1.,color);
            u.hit(menu,Target::Choice(id.into(),m.plain(id).round() as i32));
        }
        for (j,(k,caption)) in [(source+2,"Warp"),(base+7,"Bend"),(phase,"Phase"),(phase+1,"Random")].into_iter().enumerate() {
            u.knob(k,x+43.+76.*j as f32,372.,caption,color);
        }
        u.knob(base+6,x+354.,260.,"Pan",color);
        u.level_fader(base+2,x+354.,315.,371.,color);
        if sample.is_some() && !reference {
            for (i, label) in ["Root -12", "Root -1", "Root +1", "Root +12"]
                .iter()
                .enumerate()
            {
                let r = crate::sound_browser::root_button(o, i);
                raised(u.c, r, 3., crate::design::color(0x2b3843));
                u.c.text_center(font(9), r, theme_dim(), label);
            }
        } else {
            let labels = [
                "Import table".to_string(),
                format!("Cycle {}", crate::wavetable::CYCLES[cycles[o]]),
                "Reset table".into(),
            ];
            for (r, label) in table_buttons(o).into_iter().zip(labels) {
                raised(u.c, r, 3., crate::design::color(0x2b3843));
                u.c.text_center(font(9), r, theme_dim(), &label);
            }
        }
    }
    drop(tables);
    u.panel(Rect::new(836., 80., 328., 362.), "FILTER / MIX", theme_a(), "");
    u.select(FILTER, Rect::new(850., 112., 145., 25.), theme_a());
    u.select(FILTER_MODEL, Rect::new(1005., 112., 145., 25.), theme_a());
    let fr = Rect::new(850., 146., 300., 78.);
    u.grid(fr);
    let fc = cutoff(m.plain("cutoff"));
    let mode = m.plain("filter") as usize;
    let q = 1. / (2. - 1.9 * m.norm("resonance"));
    let curve: Vec<_> = (0..286)
        .map(|i| {
            let f = 20. * 1000_f32.powf(i as f32 / 285.);
            let w = f / fc;
            let denom = ((1. - w * w).powi(2) + (w / q).powi(2)).sqrt().max(1e-6);
            let mut mag = match mode {
                1 => 1. / denom,
                2 => w / denom,
                3 => w * w / denom,
                _ => 1.,
            };
            if m.plain("filter_model") > 1.5 {
                mag *= mag;
            }
            (
                fr.x + 7. + i as f32,
                fr.y + 17. + (-20. * mag.max(1e-4).log10() / 48.).clamp(-0.2, 1.) * 52.,
            )
        })
        .collect();
    u.c.polyline(&curve, 1.5, theme_a());
    u.label(858., 151., "RESPONSE GUIDE", 9, theme_dim());
    for (j, (k, cap)) in [
        (CUTOFF, "Cutoff"),
        (RESONANCE, "Resonance"),
        (DRIVE, "Drive"),
    ]
    .into_iter()
    .enumerate()
    {
        u.knob(k, 892. + 108. * j as f32, 280., cap, theme_a());
    }
    for (j, (k, cap)) in [
        (SUB, "Sub"),
        (NOISE, "Noise"),
        (SPREAD, "Width"),
        (GAIN, "Output"),
    ]
    .into_iter()
    .enumerate()
    {
        u.knob(k, 875. + 82. * j as f32, 356., cap, theme_a());
    }
    let env=params.env_tab.load(std::sync::atomic::Ordering::Relaxed).min(1);
    let lfo=params.lfo_tab.load(std::sync::atomic::Ordering::Relaxed).min(3);
    let base=if env==0 {ATTACK} else {ENV_ATTACK};
    let ec=if env==0 {theme_a()} else {theme_b()};
    u.panel(Rect::new(16.,454.,398.,334.),"",ec,"");
    u.panel(Rect::new(426.,454.,398.,334.),"",theme_mod(),"");
    for i in 0..2 {
        let r=envelope_tab(i);
        raised(u.c,r,4.,crate::design::color(if env==i {0x36454c} else {0x202b34}));
        u.c.text_center(font(10),r,if env==i {ec} else {theme_dim()},["ENV 1 / AMP","ENV 2"][i]);
    }
    u.select(ENV_ENGINE,Rect::new(254.,459.,110.,22.),ec);
    for i in 0..4 {
        let r=lfo_tab(i);
        raised(u.c,r,4.,crate::design::color(if lfo==i {0x3c354e} else {0x202b34}));
        u.c.text_center(font(10),r,if lfo==i {theme_mod()} else {theme_dim()},["LFO 1","LFO 2","LFO 3","LFO 4"][i]);
    }
    u.env(base,synth_env_graph(),ec);
    for (_,_,px,py) in envelope_nodes(env,|k|m.plain(SETTINGS[k].id)) {
        u.c.fill_circle(px,py,4.,theme_bg());u.c.stroke_circle(px,py,3.5,1.5,ec);
    }
    let hold=if env==0 {AMP_HOLD} else {ENV_HOLD};
    for (j,(k,caption)) in [(base,"Attack"),(hold,"Hold"),(base+1,"Decay"),(base+2,"Sustain"),(base+3,"Release")].into_iter().enumerate() {
        u.knob(k,55.+78.*j as f32,650.,caption,ec);
    }
    for (j,caption) in ["Attack curve","Decay curve","Release curve"].iter().enumerate() {
        u.knob(hold+1+j,68.+108.*j as f32,734.,caption,ec);
    }
    if env==1 {u.knob(ENV_AMOUNT,370.,734.,"To filter",ec);}
    let c=LFO_CONTROLS[lfo];
    let r=synth_lfo_graph();
    u.grid(r);
    let curves=*params.curves.read().unwrap();
    let points:Vec<_>=(0..=256).map(|j| {
        let phase=j as f32/256.;
        let v=if lfo==0 && m.plain("lfo_draw")<0.5 {
            match m.plain("lfo_shape") as usize {1=>1.-4.*(phase-0.5).abs(),2=>phase*2.-1.,3=>if phase<0.5 {1.} else {-1.},_=>(phase*std::f32::consts::TAU).sin()}
        } else {curves.sample(lfo,phase)};
        (r.x+phase*r.w,r.cy()-v*(r.h/2.-2.))
    }).collect();
    u.c.polyline(&points,1.6,theme_mod());
    if u.live.active {let x=r.x+r.w*u.live.phases[lfo];u.c.polyline(&[(x,r.y),(x,r.bottom())],1.,theme_ink());}
    for (j,label) in ["Sine","Triangle","Ramp","Square","Undo","Copy","Paste"].iter().enumerate() {
        let r=synth_shape_button(j);raised(u.c,r,3.,crate::design::color(0x293741));u.c.text_center(font(9),r,theme_ink(),label);
    }
    u.knob(c[0],472.,674.,"Free Hz",theme_mod());
    u.select(c[1],Rect::new(520.,648.,132.,24.),theme_mod());
    u.select(c[2],Rect::new(668.,648.,144.,24.),theme_mod());
    u.select(c[3],Rect::new(520.,683.,132.,24.),theme_mod());
    if lfo==0 {
        u.select(LFO_DRAW,Rect::new(668.,683.,144.,24.),theme_mod());
        u.select(LFO_SHAPE,Rect::new(438.,746.,104.,24.),theme_mod());
        u.knob(LFO_AMOUNT,590.,739.,"Direct depth",theme_mod());
        u.select(LFO_TARGET,Rect::new(636.,746.,176.,24.),theme_mod());
    } else {u.label(445.,746.,"Draw in the graph, then drag its handle to a control.",9,theme_dim());}
    paint_fx_rack(&mut u,params);
    for (_, r, label) in source_buttons(0,params) { paint_source_handle(u.c,r,label,u.hover(r)); }
    let status = params.status.lock().unwrap();
    let footer = if hint.is_empty() {
        status.as_str()
    } else {
        hint
    };
    let drag_hint=source_buttons(0,params).into_iter().find(|(_,r,_)|u.hover(*r)).map(|(_,_,label)|format!("Click and hold the handle to drag {label} onto a control. Edit routing depth on MOD."));
    u.c.text_clip(font(10), 20., 800., 1140., theme_dim(), drag_hint.as_deref().unwrap_or(footer));
    u.hits
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Patch(crate::dsp::Patch);
    impl Model for Patch {
        fn plain(&self, id: &str) -> f32 {
            self.0[SPEC.index(id).unwrap()]
        }
        fn norm(&self, id: &str) -> f32 {
            let s = &SETTINGS[SPEC.index(id).unwrap()];
            (self.plain(id) - s.range.0 as f32) / (s.range.1 - s.range.0) as f32
        }
        fn text(&self, id: &str) -> String {
            crate::controls::text(SPEC.index(id).unwrap(), self.plain(id) as i32)
        }
    }
    #[test]
    fn modular_rack_scrolled_controls_stay_inside_the_panel_and_clear_each_other() {
        let mut p=crate::dsp::defaults();p[RACK_MODE]=1.;
        for (slot,kind) in [4,5,1,2,3,6,7,8,9,10,11,12,13,4,5,1].into_iter().enumerate(){let b=crate::fxrack::base(slot);p[b]=kind as f32;p[b+6..b+12].copy_from_slice(&crate::fxrack::initial(kind));}
        let m=Patch(p);let params=WaveParams::default();
        for scale in [1.,1.5]{for scroll in (0..=fx_max_scroll(16)).step_by(46) {
            params.fx_scroll.store(scroll,std::sync::atomic::Ordering::Relaxed);
            let mut c=Canvas::new((1180.*scale) as u32,(820.*scale) as u32,scale).unwrap();
            let hits=paint(&mut c,&m,&Default::default(),"",&params,[5;2],None);
            let mut rects:Vec<_>=hits.iter().filter(|h|SPEC.index(h.t.id()).is_some_and(|k|k>=RACK_START)).map(|h|h.r).collect();
            for position in 0..16{for action in 0..6 {let r=fx_button(fx_card(position,scroll),action).intersect(&FX_AREA);if r.w>0.&&r.h>0.{rects.push(r);}}}
            for (i,a) in rects.iter().enumerate(){assert!(a.x>=FX_AREA.x&&a.y>=FX_AREA.y&&a.right()<=FX_AREA.right()+0.01&&a.bottom()<=FX_AREA.bottom()+0.01);
                for b in &rects[i+1..]{assert!(a.right().min(b.right())<=a.x.max(b.x)+0.01||a.bottom().min(b.bottom())<=a.y.max(b.y)+0.01,"overlap {a:?} {b:?}");}}
        }}
    }
    #[test]
    fn reference_view_has_accessible_nonoverlapping_targets_at_both_scales() {
        let mut p = crate::dsp::defaults();
        p[ENV_ENGINE] = 1.;
        p[AMP_HOLD] = 300.;
        p[AMP_ATK_CURVE] = 700.;
        p[AMP_DEC_CURVE] = 650.;
        p[ENV_DEC_CURVE] = 750.;
        p[FILTER_MODEL] = 2.;
        p[A_SOURCE] = 1.;
        p[A_MODE] = 9.;
        p[A_AMOUNT] = 650.;
        p[A_UNISON] = 5.;
        p[A_RANDOM] = 350.;
        p[B_SOURCE] = 1.;
        p[B_LEVEL] = 400.;
        p[B_MODE] = 4.;
        p[B_AMOUNT] = 400.;
        p[CUTOFF] = 700.;
        p[FILTER] = 1.;
        p[MOD1_SOURCE] = 1.;
        p[MOD1_TARGET] = 2.;
        p[MOD1_AMOUNT] = 350.;
        p[MOD2_SOURCE] = 2.;
        p[MOD2_TARGET] = 1.;
        p[MOD2_AMOUNT] = 280.;
        let model = Patch(p);
        let params = WaveParams::default();
        let mut cases=Vec::new();
        for scale in [1.,1.5] {for env in 0..2 {for lfo in 0..4 {cases.push((0,scale,env,lfo));}} cases.push((1,scale,0,0));}
        for (page, scale, env, lfo) in cases {
            params.env_tab.store(env,std::sync::atomic::Ordering::Relaxed);
            params.lfo_tab.store(lfo,std::sync::atomic::Ordering::Relaxed);
            params
                .page
                .store(page, std::sync::atomic::Ordering::Relaxed);
            let mut c = Canvas::new((1180. * scale) as u32, (820. * scale) as u32, scale).unwrap();
            let hits = paint(
                &mut c,
                &model,
                &Default::default(),
                "Drag to edit · Ctrl for fine control · Right-click for automation",
                &params,
                [5; 2],
                None,
            );
            let mut all: Vec<_> = hits.iter().map(|h| h.r).collect();
            all.extend([page_button(0), page_button(1)]);
            all.extend(source_buttons(page,&params).into_iter().map(|(_, r, _)| r));
            if page==0 {
                all.extend((0..2).map(envelope_tab));all.extend((0..4).map(lfo_tab));
                all.extend((0..7).map(synth_shape_button));all.push(synth_lfo_graph());all.push(synth_env_graph());
            }
            if page == 1 {
                for i in 0..4 {
                    all.push(lfo_graph(i));
                    for j in 0..5 {
                        all.push(shape_button(i, j));
                    }
                }
                for i in 0..8 {
                    all.push(clear_route(i));
                }
            }
            for (i, a) in all.iter().enumerate() {
                assert!(a.x >= 0. && a.y >= 0. && a.right() <= 1180. && a.bottom() <= 820.);
                for b in &all[i + 1..] {
                    let w = a.right().min(b.right()) - a.x.max(b.x);
                    let h = a.bottom().min(b.bottom()) - a.y.max(b.y);
                    assert!(w <= 0.01 || h <= 0.01, "overlap page{page}: {a:?} {b:?}");
                }
            }
            for (i, a) in hits.iter().enumerate() {
                for b in &hits[i + 1..] {
                    let w = a.r.right().min(b.r.right()) - a.r.x.max(b.r.x);
                    let h = a.r.bottom().min(b.r.bottom()) - a.r.y.max(b.r.y);
                    assert!(
                        w <= 0.01 || h <= 0.01,
                        "overlap: {} / {}",
                        a.t.id(),
                        b.t.id()
                    );
                }
            }
            if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
                std::fs::create_dir_all(&dir).unwrap();
                c.pixmap
                    .save_png(format!("{dir}/modulation-page{page}-{scale}.png"))
                    .unwrap();
            }
        }
    }
}

pub const FX_AREA:Rect=Rect::new(844.,492.,309.,276.);
pub const FX_ADD:Rect=Rect::new(1058.,459.,94.,23.);
pub const FX_CARD_HEIGHT:f32=232.;
pub fn fx_card(position:usize,scroll:usize)->Rect {Rect::new(845.,493.+position as f32*FX_CARD_HEIGHT-scroll as f32,303.,224.)}
pub fn fx_button(card:Rect,action:usize)->Rect {Rect::new(card.x+140.+action as f32*26.,card.y+6.,22.,20.)}
pub fn fx_menu_button(kind:usize)->Rect {Rect::new(850.+((kind-1)%2) as f32*151.,499.+((kind-1)/2) as f32*36.,143.,29.)}
pub fn fx_max_scroll(count:usize)->usize {(count as f32*FX_CARD_HEIGHT-FX_AREA.h).max(0.) as usize}
struct FxModel(crate::dsp::Patch);
impl Model for FxModel {
 fn plain(&self,id:&str)->f32 {self.0[SPEC.index(id).unwrap()]}
 fn norm(&self,id:&str)->f32 {let k=SPEC.index(id).unwrap();let s=&SETTINGS[k];(self.0[k]-s.range.0 as f32)/(s.range.1-s.range.0) as f32}
 fn text(&self,id:&str)->String {let k=SPEC.index(id).unwrap();if k>=RACK_START {let b=crate::fxrack::base((k-RACK_START)/RACK_STRIDE);let kind=self.0[b] as usize;let col=k-b;
  if col==5{return crate::fxrack::modes(kind)[(self.0[k] as usize).min(crate::fxrack::modes(kind).len()-1)].into();}
  if col>=6{return crate::fxrack::value(kind,col-6,self.0[k]);}
 }crate::controls::text(k,self.0[k] as i32)}
}
fn paint_fx_rack(u:&mut Ui,params:&WaveParams) {
 use std::sync::atomic::Ordering;
 u.panel(Rect::new(836.,454.,328.,334.),"EFFECTS",theme_b(),"");
 raised(u.c,FX_ADD,4.,crate::design::color(0x34463f));u.c.text_center(font(10),FX_ADD,theme_b(),if params.fx_add.load(Ordering::Relaxed){"CLOSE"}else{"+ ADD FX"});
 if params.fx_add.load(Ordering::Relaxed) {
  for kind in 1..crate::fxrack::NAMES.len(){let r=fx_menu_button(kind);raised(u.c,r,4.,crate::design::color(if u.hover(r){0x40534e}else{0x293741}));u.c.text_center(font(10),r,theme_ink(),crate::fxrack::NAMES[kind]);}
  u.label(850.,773.,"Choose a module. Repeated effects are welcome.",8,theme_dim());return;
 }
 let p=crate::fxrack::effective(std::array::from_fn(|k|u.m.plain(SETTINGS[k].id)));let slots=crate::fxrack::order(&p);
 let scroll=params.fx_scroll.load(Ordering::Relaxed).min(fx_max_scroll(slots.len()));params.fx_scroll.store(scroll,Ordering::Relaxed);
 if slots.is_empty(){u.c.text_center(font(13),Rect::new(854.,559.,285.,28.),theme_ink(),"Build your effects chain");u.c.text_center(font(10),Rect::new(854.,594.,285.,28.),theme_dim(),"Add modules above, then arrange them here.");}
 let model=FxModel(p);let mut v=Ui{c:u.c,m:&model,st:u.st,hits:Vec::new(),live:u.live};v.c.push_clip(FX_AREA);
 for (position,slot) in slots.iter().enumerate(){let b=crate::fxrack::base(*slot);let kind=model.0[b] as usize;let r=fx_card(position,scroll);if r.bottom()<FX_AREA.y||r.y>FX_AREA.bottom(){continue;}
  raised(v.c,r,5.,crate::design::color(0x1a2731));
  v.c.text_clip(font(10).bold(),r.x+9.,r.y+11.,124.,theme_ink(),&format!("{}  {}",position+1,match kind {10=>"Hyper / Dim",11=>"Freq Shifter",_=>crate::fxrack::NAMES[kind]}));
  for (a,label) in ["ON","A","B","↑","↓","×"].iter().enumerate(){let button=fx_button(r,a);let on=if a<3 {model.0[b+2+a]>0.5}else{false};raised(v.c,button,3.,crate::design::color(if on{0x3e534c}else{0x293741}));v.c.text_center(font(9),button,if on{theme_b()}else{theme_dim()},if a==0&&!on{"OFF"}else{label});}
  v.select(b+5,Rect::new(r.x+10.,r.y+34.,if kind==4{140.}else{282.},24.),theme_b());
  if kind==4 {v.select(b+10,Rect::new(r.x+158.,r.y+34.,134.,24.),theme_b());}
  let labels=crate::fxrack::captions(kind);
  for i in 0..6{if kind==4&&i==4 {v.c.text_center(font(9),Rect::new(r.x+112.,r.y+164.,80.,37.),theme_dim(),"Beat sync above");continue;}
   v.knob(b+6+i,r.x+50.+(i%3) as f32*100.,r.y+94.+(i/3) as f32*82.,labels[i],theme_b());}
 }
 v.c.pop_clip();
 for mut h in v.hits {h.r=h.r.intersect(&FX_AREA);if h.r.w>0.&&h.r.h>0.{u.hits.push(h);}}
 if fx_max_scroll(slots.len())>0 {let total=slots.len() as f32*FX_CARD_HEIGHT;let h=(FX_AREA.h*FX_AREA.h/total).max(20.);let y=FX_AREA.y+(FX_AREA.h-h)*scroll as f32/fx_max_scroll(slots.len()) as f32;u.c.fill_rrect(Rect::new(1157.,FX_AREA.y,4.,FX_AREA.h),2.,theme_line());u.c.fill_rrect(Rect::new(1157.,y,4.,h),2.,theme_b());}
 u.label(850.,775.,&format!("{} / 16 modules  ·  Scroll here  ·  ↑↓ reorder",slots.len()),8,theme_dim());
}

pub fn page_button(page: usize) -> Rect {
    Rect::new(
        130. + page as f32 * 88.,
        18.,
        if page == 2 { 100. } else { 80. },
        32.,
    )
}
pub fn lfo_panel(i: usize) -> Rect {
    Rect::new(
        16. + (i % 2) as f32 * 580.,
        80. + (i / 2) as f32 * 198.,
        568.,
        186.,
    )
}
pub fn lfo_graph(i: usize) -> Rect {
    let p = lfo_panel(i);
    Rect::new(p.x + 12., p.y + 49., 330., 100.)
}
pub fn envelope_nodes(env:usize,value:impl Fn(usize)->f32)->Vec<(usize,Option<usize>,f32,f32)> {
    let base=if env==0 {ATTACK} else {ENV_ATTACK};
    let hold_key=if env==0 {AMP_HOLD} else {ENV_HOLD};
    let curved=value(ENV_ENGINE)>0.5;
    let a=seconds(value(base));let d=seconds(value(base+1));let release=seconds(value(base+3));
    let h=if curved {hold_seconds(value(hold_key))} else {0.};
    let raw=value(base+2)/1000.;let sustain=if curved {raw*raw} else {raw};
    let gate=(a+d+release).max(0.1)*0.3;let total=a+h+d+gate+release;
    let r=synth_env_graph();
    let point=|k,second,time,v|(k,second,r.x+8.+(r.w-16.)*time/total,r.bottom()-9.-(r.h-18.)*v);
    let mut nodes=vec![point(base,None,a,1.),point(base+1,Some(base+2),a+h+d,sustain),point(base+3,None,a+h+d+gate+release*0.5,sustain*crate::reference_envelope::falling(if curved {value(hold_key+3) as f64/1000.} else {0.5},0.5))];
    if curved && (r.w-16.)*h/total>12. {nodes.push(point(hold_key,None,a+h,1.));}
    nodes
}
pub fn envelope_tab(i:usize)->Rect {Rect::new(28.+i as f32*112.,459.,104.,22.)}
pub fn lfo_tab(i:usize)->Rect {Rect::new(438.+i as f32*80.,459.,72.,22.)}
pub fn synth_env_graph()->Rect {Rect::new(28.,496.,374.,112.)}
pub fn synth_lfo_graph()->Rect {Rect::new(438.,496.,374.,112.)}
pub fn synth_shape_button(i:usize)->Rect {Rect::new(438.+i as f32*54.,615.,50.,21.)}
pub fn source_buttons(page: usize, params: &WaveParams) -> Vec<(usize, Rect, &'static str)> {
    if page == 0 {
        let e=params.env_tab.load(std::sync::atomic::Ordering::Relaxed).min(1);
        let l=params.lfo_tab.load(std::sync::atomic::Ordering::Relaxed).min(3);
        return vec![([12,2][e],Rect::new(374.,459.,28.,22.),["ENV 1","ENV 2"][e]),
            ([1,5,6,7][l],Rect::new(784.,459.,28.,22.),["LFO 1","LFO 2","LFO 3","LFO 4"][l])];
    }
    let mut v:Vec<_>=[1,5,6,7].into_iter().enumerate().map(|(i,s)| {
        let p=lfo_panel(i);
        (s,Rect::new(p.right()-40.,p.y+7.,28.,22.),["LFO 1","LFO 2","LFO 3","LFO 4"][i])
    }).collect();
    for i in 0..4 {
        v.push((8+i,Rect::new(916.+(i%2) as f32*148.,506.+(i/2) as f32*125.,28.,22.),["Macro 1","Macro 2","Macro 3","Macro 4"][i]));
    }
    v
}
fn paint_source_handle(c:&mut Canvas,r:Rect,_label:&str,hot:bool) {
    let color=if hot {theme_ink()} else {theme_mod()};
    if hot { c.fill_rrect(r,4.,crate::design::color(0x35434e)); }
    for row in 0..3 { for col in 0..2 {
        c.fill_circle(r.x+7.+col as f32*4.,r.y+6.+row as f32*4.,1.,color.with_alpha(180));
    }}
    c.polyline(&[(r.x+17.,r.y+15.),(r.x+23.,r.y+7.),(r.x+18.,r.y+7.)],1.3,color);
    c.polyline(&[(r.x+23.,r.y+7.),(r.x+23.,r.y+12.)],1.3,color);
}
pub fn shape_button(i: usize, shape: usize) -> Rect {
    let p = lfo_panel(i);
    Rect::new(p.x + 12. + shape as f32 * 66., p.y + 157., 62., 21.)
}
pub fn clear_route(i: usize) -> Rect {
    Rect::new(790., 510. + i as f32 * 32., 22., 24.)
}
fn paint_modulation(u: &mut Ui, params: &WaveParams) {
    let curves = *params.curves.read().unwrap();
    for i in 0..4 {
        let p = lfo_panel(i);
        u.panel(p, ["LFO 1", "LFO 2", "LFO 3", "LFO 4"][i], theme_mod(), "");
        let r = lfo_graph(i);
        u.grid(r);
        let pts: Vec<_> = (0..=128)
            .map(|j| {
                let phase = j as f32 / 128.;
                let y = if i == 0 && u.m.plain("lfo_draw") < 0.5 {
                    match u.m.plain("lfo_shape") as usize {
                        1 => 1. - 4. * (phase - 0.5).abs(),
                        2 => phase * 2. - 1.,
                        3 => {
                            if phase < 0.5 {
                                1.
                            } else {
                                -1.
                            }
                        }
                        _ => (phase * std::f32::consts::TAU).sin(),
                    }
                } else {
                    curves.sample(i, phase)
                };
                (r.x + phase * r.w, r.y + (1. - y) * r.h * 0.5)
            })
            .collect();
        u.c.polyline(&pts, 1.6, theme_mod());
        if u.live.active {
            let x = r.x + r.w * u.live.phases[i];
            u.c.polyline(&[(x, r.y), (x, r.bottom())], 1., theme_ink());
        }
        let c = LFO_CONTROLS[i];
        u.knob(c[0], p.x + 384., p.y + 82., "Free Hz", theme_mod());
        for (j, k) in c[1..].iter().enumerate() {
            u.select(
                *k,
                Rect::new(p.x + 430., p.y + 49. + j as f32 * 32., 126., 25.),
                theme_mod(),
            );
        }
        for (j, label) in ["Sine", "Triangle", "Ramp", "Square", "Undo"]
            .iter()
            .enumerate()
        {
            let r = shape_button(i, j);
            raised(u.c, r, 3., crate::design::color(0x293741));
            u.c.text_center(font(9), r, theme_dim(), label);
        }
        if i == 0 {
            u.select(LFO_DRAW, Rect::new(p.x + 350., p.y + 157., 206., 21.), theme_mod());
        } else {
            u.label(p.x + 356., p.y + 161., "DRAW IN THE GRAPH", 9, theme_dim());
        }
    }
    u.panel(
        Rect::new(16., 476., 808., 312.),
        "MODULATION MATRIX",
        theme_mod(),
        "",
    );
    for (i, slot) in ROUTES.into_iter().enumerate() {
        let y = 510. + i as f32 * 32.;
        u.label(28., y + 5., &format!("{}", i + 1), 10, theme_dim());
        u.select(slot, Rect::new(48., y, 178., 24.), theme_mod());
        u.select(slot + 1, Rect::new(238., y, 278., 24.), theme_ink());
        let r = Rect::new(528., y, 250., 24.);
        raised(u.c, r, 4., crate::design::color(0x293741));
        let v = u.m.plain(SETTINGS[slot + 2].id) / 1000.;
        u.c.text_center(font(11), r, theme_mod(), &format!("Depth {:+.1}%", v * 100.));
        u.c.fill_rect(
            Rect::new(
                r.x + 125_f32.min(125. + v * 114.),
                y + 20.,
                (v * 114.).abs().max(1.),
                2.,
            ),
            theme_mod(),
        );
        u.hit(r, Target::Knob(SETTINGS[slot + 2].id.into()));
        let r = clear_route(i);
        raised(u.c, r, 3., crate::design::color(0x293741));
        u.c.text_center(font(12), r, theme_dim(), "×");
    }
    u.panel(Rect::new(836., 476., 328., 312.), "MACROS", theme_a(), "");
    for i in 0..4 {
        u.knob(
            MACRO1 + i,
            930. + (i % 2) as f32 * 148.,
            560. + (i / 2) as f32 * 125.,
            ["Macro 1", "Macro 2", "Macro 3", "Macro 4"][i],
            theme_a(),
        );
    }
    for (_, r, label) in source_buttons(1,params) { paint_source_handle(u.c,r,label,u.hover(r)); }
    u.label(20.,800.,"Draw a shape • Drag a source onto a synth knob (hover SYNTH while dragging) • Drag Depth to adjust",10,theme_dim());
}
