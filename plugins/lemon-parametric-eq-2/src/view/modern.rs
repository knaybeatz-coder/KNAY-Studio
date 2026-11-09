//! Graph-led editor. Window dimensions and interface scale are independent.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub size: (f32, f32),
    pub plot: Rect,
    pub scale: f32,
    pub range: f32,
}
// Bounded per-UI-thread cache: live meters and hover never rebuild static filter curves.
struct Responses { hz: Vec<f64>, total: Vec<f64>, bands: [Vec<f64>; BANDS] }
type ResponseEntry = (Settings, Options, f64, usize, std::rc::Rc<Responses>);
thread_local! { static RESPONSES: std::cell::RefCell<Vec<ResponseEntry>> = const { std::cell::RefCell::new(Vec::new()) }; }
fn responses(s: Settings, options: Options, rate: f64, count: usize) -> std::rc::Rc<Responses> {
    RESPONSES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|e| e.0 == s && e.1 == options && e.2 == rate && e.3 == count) {
            let entry = cache.remove(i); let result = entry.4.clone(); cache.push(entry); return result;
        }
        let design = options.design(rate);
        let hz: Vec<_> = (0..count).map(|j| eq::freq_hz(j as f64 / (count-1) as f64)).collect();
        let bands: [_; BANDS] = std::array::from_fn(|k| { let c=s.bands[k].coefs(design); hz.iter().map(|&f|eq::sections_db(&c,f,design.rate())).collect::<Vec<_>>() });
        let total = (0..count).map(|j| s.main_db + (0..BANDS).filter(|&k|s.audible(k)).map(|k|bands[k][j]).sum::<f64>()).collect();
        let result = std::rc::Rc::new(Responses { hz, total, bands });
        if cache.len() == 8 { cache.remove(0); }
        cache.push((s,options,rate,count,result.clone())); result
    })
}

/// Nominal Q of the stored bell octave bandwidth (multi-section/shelf shapes retain their own design).
pub fn q_of(w: f64) -> f64 {
    eq::dynamic::q_of_width(w)
}
pub fn width_of_q(q: f64) -> f64 {
    ((2. * (0.5 / q.clamp(q_of(1.), q_of(0.))).asinh() / std::f64::consts::LN_2 - 0.1) / 4.9)
        .clamp(0., 1.)
}
impl Layout {
    pub fn new(size: (f32, f32), scale: f32) -> Self {
        let (w, h) = if size.0 > 0. && size.1 > 0. {
            size
        } else {
            (W as f32, H as f32)
        };
        let s = if scale > 0. {
            scale.clamp(0.8, 1.5)
        } else {
            1.
        };
        let s = s.min(w / 620.);
        Self {
            size: (w, h),
            plot: Rect::new(
                20. * s,
                44. * s,
                (w - 72. * s).max(100.),
                (h - 100. * s).max(100.),
            ),
            scale: s,
            range: 18.,
        }
    }
    pub fn of(v: &ViewState) -> Self {
        let mut l = Self::new(v.size, v.ui_scale);
        if v.details_open || v.harsh_open {
            l.plot.h = (l.plot.h - 116. * l.scale).max(100.);
        }
        if v.range_db > 0. {
            l.range = v.range_db;
        }
        l
    }
    pub fn x(&self, n: f64) -> f32 {
        self.plot.x + n.clamp(0., 1.) as f32 * self.plot.w
    }
    pub fn n(&self, x: f32) -> f64 {
        f64::from(((x - self.plot.x) / self.plot.w).clamp(0., 1.))
    }
    pub fn y(&self, db: f64) -> f32 {
        self.plot.cy() - db as f32 / self.range * self.plot.h * 0.5
    }
    pub fn db(&self, y: f32) -> f64 {
        f64::from((self.plot.cy() - y) * self.range * 2. / self.plot.h)
    }
    pub fn node(&self, s: &Settings, k: usize) -> (f32, f32) {
        (
            self.x(s.bands[k].freq),
            self.y(if has_level(s.bands[k].kind) {
                s.bands[k].db
            } else {
                0.
            }),
        )
    }
    pub fn panel(&self, n: f64, expanded: bool) -> Rect {
        let s = self
            .scale
            .min((self.plot.w - 8.) / if expanded { 610. } else { 452. });
        let w = if expanded { 610. } else { 452. } * s;
        Rect::new(
            (self.x(n) - w * 0.5).clamp(
                self.plot.x + 2.,
                (self.plot.right() - w - 2.).max(self.plot.x + 2.),
            ),
            self.plot.bottom() - 130. * s,
            w,
            116. * s,
        )
    }
    pub fn panel_for(&self, s: &Settings, v: &ViewState, expanded: bool) -> Rect {
        let mut r = self.panel(s.bands[v.selected.min(6)].freq, expanded);
        r.y = self.plot.bottom() + 24. * self.scale;
        r
    }
    fn band_card(&self, settings: &Settings, k: usize, drawer_open: bool) -> Rect {
        let s = self.scale;
        let (x, y) = self.node(settings, k);
        let y = y.clamp(self.plot.y, self.plot.bottom());
        let (w, h) = (218. * s, 48. * s);
        let left = self.plot.x + 3. * s;
        let right = (self.plot.right() - w - 3. * s).max(left);
        let top = self.plot.y + 32. * s;
        let bottom = if drawer_open { self.plot.bottom() } else { self.size.1 - 30. * s };
        let gap = 15. * s;
        let above = y - h - gap;
        let below = y + gap;
        let center = (x - w / 2.).clamp(left, right);
        if settings.bands[k].db >= 0. && above >= top {
            Rect::new(center, above, w, h)
        } else if below + h <= bottom {
            Rect::new(center, below, w, h)
        } else {
            // A deep cut can sit right on the graph floor. Move sideways to keep its
            // center and the immediately surrounding curve clear.
            let side = if self.plot.right() - x > x - self.plot.x { x + gap } else { x - w - gap };
            Rect::new(side.clamp(left, right), (y - h * 0.5).clamp(top, (bottom - h).max(top)), w, h)
        }
    }
    pub fn ring(&self, s: &Settings, v: &ViewState, expanded: bool) -> (f32, f32, f32) {
        let r = self.panel_for(s, v, expanded);
        let z = r.w / if expanded { 610. } else { 452. };
        (r.x + 229. * z, r.y + 65. * z, 34. * z)
    }
}
pub fn available(s: &Settings, k: usize) -> bool {
    let b = s.bands[k];
    b.kind == 0
        || (b.db.abs() < 1e-5
            && (b.freq - eq::default_freq(k)).abs() < 1e-5
            && (b.width - eq::default_width(b.kind)).abs() < 1e-5
            && b.kind == eq::DEFAULT_KINDS[k])
}
pub fn visible(s: &Settings, v: &ViewState, k: usize) -> bool {
    s.bands[k].kind != 0
        && (!available(s, k)
            || v.created & (1 << k) != 0
            || v.selection & (1 << k) != 0
            || v.active == Some(k)
            || (v.show_dynamics && v.selected == k))
}
fn font(n: f32, s: f32) -> Font {
    Font::new((n * s).round() as u16)
}
fn txt(c: &mut Canvas, r: Rect, n: f32, s: f32, color: Color, text: &str) {
    c.text_center(font(n, s), r, color, text);
}
fn button(c: &mut Canvas, h: &mut Vec<Hit>, r: Rect, t: Target, text: &str, on: bool, s: f32) {
    c.fill_rrect(
        Rect::new(r.x, r.y + s, r.w, r.h),
        4. * s,
        Color::rgb(0x090e10).with_alpha(100),
    );
    c.fill_rect_gradient(
        r,
        Color::rgb(if on { 0xf4e486 } else { 0x3b454a }),
        Color::rgb(if on { 0xc6af37 } else { 0x272f33 }),
        4. * s,
    );
    c.stroke_rrect(
        r.inset(0.5 * s),
        4. * s,
        0.8 * s,
        Color::rgb(if on { 0xfff0a5 } else { 0x8e9da3 }).with_alpha(if on { 125 } else { 45 }),
    );
    c.line(
        r.x + 5. * s,
        r.y + s,
        r.right() - 5. * s,
        r.y + s,
        0.7 * s,
        Color::rgb(0xffffff).with_alpha(if on { 100 } else { 30 }),
    );
    txt(
        c,
        r,
        10.,
        s,
        if on {
            Color::rgb(0x171b1e)
        } else {
            Color::rgb(0xc1ccd2)
        },
        text,
    );
    h.push(Hit { r, t });
}
fn knob(
    c: &mut Canvas,
    h: &mut Vec<Hit>,
    x: f32,
    y: f32,
    r: f32,
    n: f64,
    t: Target,
    label: &str,
    value: &str,
    color: Color,
    s: f32,
) {
    c.fill_circle(
        x,
        y + 2. * s,
        r + 2. * s,
        Color::rgb(0x080d10).with_alpha(185),
    );
    c.fill_rect_gradient(
        Rect::new(
            x - r - 0.8 * s,
            y - r - 0.8 * s,
            2. * r + 1.6 * s,
            2. * r + 1.6 * s,
        ),
        Color::rgb(0x85949a),
        Color::rgb(0x11191e),
        r + s,
    );
    c.fill_rect_gradient(
        Rect::new(x - r, y - r, 2. * r, 2. * r),
        Color::rgb(0x404e55),
        Color::rgb(0x1e272d),
        r,
    );
    c.stroke_circle(
        x,
        y,
        r - 1. * s,
        0.8 * s,
        Color::rgb(0xd4e2e6).with_alpha(45),
    );
    c.arc(
        x,
        y,
        r - 2. * s,
        -PI * 0.91,
        PI * 0.78,
        0.8 * s,
        Color::rgb(0xffffff).with_alpha(50),
    );
    let angle = -PI * 1.25 + PI * 1.5 * n.clamp(0., 1.) as f32;
    c.line(
        x + angle.cos() * r * 0.57,
        y + angle.sin() * r * 0.57,
        x + angle.cos() * (r - 6. * s),
        y + angle.sin() * (r - 6. * s),
        1.5 * s,
        color.with_alpha(220),
    );
    c.arc(
        x,
        y,
        r - 4. * s,
        -PI * 1.25,
        PI * 1.5 * n.clamp(0., 1.) as f32,
        2. * s,
        color,
    );
    txt(
        c,
        Rect::new(x - r - 8. * s, y - r - 19. * s, 2. * r + 16. * s, 15. * s),
        10.,
        s,
        Color::rgb(0xaab7c1),
        value,
    );
    txt(
        c,
        Rect::new(x - r - 10. * s, y + r + 5. * s, 2. * r + 20. * s, 14. * s),
        11.,
        s,
        Color::rgb(0xd5dfe6),
        label,
    );
    h.push(Hit {
        r: Rect::new(
            x - r - 4. * s,
            y - r - 16. * s,
            2. * r + 8. * s,
            2. * r + 38. * s,
        ),
        t,
    });
}
/// Presentation-only, narrow power-domain smoothing plus monotone cubic interpolation.
/// Never feeds the analyzer/detectors or EQ audio. Local extrema cannot overshoot.
fn spectrum_points(values:&[f32], plot:Rect, range:f32)->Vec<(f32,f32)> {
    if values.len()<2 {return Vec::new();}
    let power=|i:usize|10f32.powf(if values[i].is_finite(){values[i].clamp(-150.,60.)*0.1}else{-15.});
    let y:Vec<_>=(0..values.len()).map(|i|{
        let center=power(i);
        let average=0.0625*power(i.saturating_sub(2))+0.25*power(i.saturating_sub(1))+0.375*center+0.25*power((i+1).min(values.len()-1))+0.0625*power((i+2).min(values.len()-1));
        let p=average.max(0.75*center); // Keep isolated resonances within1.25dB of their measured peak.
        let db=10.*p.max(1e-15).log10();
        plot.bottom()-((db+range)/range).clamp(0.,1.)*plot.h
    }).collect();
    let slopes:Vec<_>=(0..y.len()).map(|i|{
        let a=y[i]-y[i.saturating_sub(1)];let b=y[(i+1).min(y.len()-1)]-y[i];
        if a*b>0.{2.*a*b/(a+b)}else{0.}
    }).collect();
    let mut points=Vec::with_capacity(y.len()*2-1);
    for i in 0..y.len()-1 {for step in 0..2{
        let t=step as f32*0.5;let t2=t*t;let t3=t2*t;
        let v=((2.*t3-3.*t2+1.)*y[i]+(t3-2.*t2+t)*slopes[i]+(-2.*t3+3.*t2)*y[i+1]+(t3-t2)*slopes[i+1]).clamp(y[i].min(y[i+1]),y[i].max(y[i+1]));
        points.push((plot.x+(i as f32+t)/(y.len()-1)as f32*plot.w,v));
    }}
    points.push((plot.right(),*y.last().unwrap()));points
}

pub fn paint(c: &mut Canvas, look: &Look, v: &ViewState) -> Vec<Hit> {
    let mut v = v.clone();
    v.size = c.size();
    let l = Layout::of(&v);
    let (w, h) = l.size;
    let s = l.scale;
    let mut hits = vec![];
    let settings = &look.s;
    let cfg = eq::dynamic::Config::load(look.extra, false);
    for k in 0..7 {
        if cfg.bands[k][0] != 0. || cfg.spectral & (1 << k) != 0 {
            v.created |= 1 << k;
        }
    }
    c.begin_frame(None);
    c.fill_rect_gradient(
        Rect::new(0., 0., w, h),
        Color::rgb(0x252a2f),
        Color::rgb(0x171a1e),
        0.,
    );
    c.fill_rect(Rect::new(0., 0., w, 34. * s), Color::rgb(0x252a2d));
    lemon::wordmark(
        c,
        13. * s,
        5. * s,
        true,
        if w < 950. * s { "EQ" } else { "PARAMETRIC EQ" },
        font(17., s),
        font(8., s).bold(),
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 350. * s, 6. * s, 44. * s, 22. * s),
        Target::Undo,
        "Undo",
        false,
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 300. * s, 6. * s, 44. * s, 22. * s),
        Target::Redo,
        "Redo",
        false,
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 244. * s, 6. * s, 47. * s, 22. * s),
        Target::Store,
        "A > B",
        false,
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 189. * s, 6. * s, 47. * s, 22. * s),
        Target::Flip,
        if v.compare_b { "B / A" } else { "A / B" },
        v.compare_b,
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 134. * s, 6. * s, 55. * s, 22. * s),
        Target::Options,
        "Options",
        false,
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(w - 71. * s, 6. * s, 54. * s, 22. * s),
        Target::UiScale,
        &format!(
            "{}%",
            (if v.ui_scale > 0. { v.ui_scale } else { 1. } * 100.).round()
        ),
        false,
        s,
    );
    button(
        c, &mut hits, Rect::new(w - 449. * s, 6. * s, 89. * s, 22. * s),
        Target::Details, "Band controls", v.details_open, s,
    );
    c.fill_rrect(l.plot.outset(1. * s), 5. * s, Color::rgb(0x070d10));
    c.fill_rect_gradient(l.plot, Color::rgb(0x253137), Color::rgb(0x141b1f), 4. * s);
    c.push_clip(l.plot.inset(1. * s));
    c.fill_polygon(
        &[
            (l.plot.x, l.plot.y),
            (l.plot.right(), l.plot.y),
            (l.plot.right(), l.plot.y + l.plot.h * 0.1),
            (l.plot.x, l.plot.y + l.plot.h * 0.32),
        ],
        Color::rgb(0xc7e0e7).with_alpha(5),
    );
    c.pop_clip();
    c.stroke_rrect(
        l.plot.inset(0.5 * s),
        4. * s,
        0.8 * s,
        Color::rgb(0x9cafb4).with_alpha(38),
    );
    hits.push(Hit {
        r: l.plot,
        t: Target::Graph,
    });
    for hz in [
        20., 30., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000.,
    ] {
        let x = l.x(eq::freq_pos(hz));
        c.line(
            x,
            l.plot.y,
            x,
            l.plot.bottom(),
            0.7,
            Color::rgb(if hz == 100. || hz == 1000. || hz == 10000. {
                0x353b43
            } else {
                0x292f36
            }),
        );
        let label = if hz >= 1000. {
            format!("{}k", hz / 1000.)
        } else {
            format!("{hz:.0}")
        };
        txt(
            c,
            Rect::new(x - 20. * s, l.plot.bottom() + 3. * s, 40. * s, 14. * s),
            9.,
            s,
            Color::rgb(0x9daab4),
            &label,
        );
    }
    for frac in [-1., -0.5, 0., 0.5, 1.] {
        let db = f64::from(frac * l.range);
        let y = l.y(db);
        c.line(
            l.plot.x,
            y,
            l.plot.right(),
            y,
            if frac == 0. { 1.2 } else { 0.6 },
            Color::rgb(if frac == 0. { 0x52545b } else { 0x30363d }),
        );
        txt(
            c,
            Rect::new(l.plot.right() + 3. * s, y - 7. * s, 32. * s, 14. * s),
            9.,
            s,
            Color::rgb(0xdac66f),
            &format!("{db:+.0}"),
        );
    }
    let display = Rect::new(
        l.plot.right() - 52. * s,
        l.plot.y + 5. * s,
        46. * s,
        20. * s,
    );
    button(
        c,
        &mut hits,
        display,
        Target::DisplayRange,
        &format!("{} dB", l.range),
        false,
        s,
    );
    let harsh = Rect::new(l.plot.x + 8. * s, l.plot.y + 7. * s, 94. * s, 22. * s);
    button(
        c,
        &mut hits,
        harsh,
        Target::HarshPanel,
        "De-harsh",
        eq::dynamic::Config::load(look.extra, false).harsh[0] > 0.,
        s,
    );
    c.push_clip(l.plot);
    if flag(look.extra, params::F_HEATMAP) {
        if let Some(a) = look.spectrum.filter(|a| !a.is_empty()) {
            let range =
                [60., 90., 120.][(params::opt(look.extra, Opt::Range) - 1).clamp(0, 2) as usize];
            let (top, height, alpha) = match params::opt(look.extra, Opt::HeatmapPosition) {
                0 => (l.plot.y, 12. * s, 220.),
                1 => (l.plot.bottom() - 12. * s, 12. * s, 220.),
                _ => (l.plot.y, l.plot.h, 35.),
            };
            for (i, db) in a.iter().enumerate() {
                let v = (1. + db / range).clamp(0., 1.);
                let color = Color::rgb(if v < 0.3 {
                    0x31518a
                } else if v < 0.65 {
                    0x559e91
                } else {
                    0xe4bd58
                });
                c.fill_cell(
                    Rect::new(
                        l.plot.x + i as f32 / a.len() as f32 * l.plot.w,
                        top,
                        l.plot.w / a.len() as f32 + 0.5,
                        height,
                    ),
                    color.with_alpha((alpha * v) as u8),
                );
            }
        }
    }
    if let Some(a) = look.spectrum.filter(|a| a.len()>1) {
        let range=[60.,90.,120.][(params::opt(look.extra,Opt::Range)-1).clamp(0,2)as usize];
        let points=spectrum_points(a,l.plot,range);
        if flag(look.extra,params::F_HISTOGRAM){
            let mut fill=points.clone();fill.push((l.plot.right(),l.plot.bottom()));fill.push((l.plot.x,l.plot.bottom()));
            c.fill_polygon(&fill,Color::rgb(0x9cb9c0).with_alpha(25));
        }
        c.polyline(&points,1.15*s,Color::rgb(0xb7ced1).with_alpha(185));
    }
    let cfg = eq::dynamic::Config::load(look.extra, false);
    let count = (l.plot.w as usize).clamp(300, 1800);
    let cached = responses(*settings, Options::from_image(look.extra), look.rate.max(22050.), count);
    let live: [_; BANDS] = std::array::from_fn(|k| eq::dynamic::Response::new(look.rate, settings.bands[k], f64::from(v.reduction[k])));
    for k in 0..BANDS {
        if !visible(settings, &v, k) {
            continue;
        }
        let selected = v.selection & (1 << k) != 0 || (v.show_dynamics && v.selected == k);
        if selected || v.active == Some(k) {
            let envelope_response = eq::dynamic::Response::new(look.rate, settings.bands[k], f64::from(cfg.bands[k][0]));
            let mut points = Vec::with_capacity(count + 3);
            for j in 0..count {
                let n = j as f64 / (count - 1) as f64;
                points.push((l.x(n), l.y(cached.bands[k][j])));
            }
            let mut fill = points.clone();
            fill.push((l.plot.right(), l.y(0.)));
            fill.push((l.plot.x, l.y(0.)));
            c.fill_polygon(
                &fill,
                band_color(k).with_alpha(if selected { 36 } else { 18 }),
            );
            c.polyline(&points, 1. * s, band_color(k).with_alpha(150));
            if cfg.bands[k][0] != 0. {
                let mut envelope = Vec::with_capacity(count + 3);
                for j in 0..count {
                    let n = j as f64 / (count - 1) as f64;
                    let hz = cached.hz[j];
                    let db = cached.bands[k][j] + envelope_response.db(hz);
                    envelope.push((l.x(n), l.y(db)));
                }
                let mut fill = envelope.clone();
                fill.extend(points.iter().rev());
                c.fill_polygon(&fill, band_color(k).with_alpha(35));
                c.polyline(&envelope, 0.8 * s, band_color(k).with_alpha(130));
            }
        }
    }
    let mut points = Vec::with_capacity(count);
    for j in 0..count {
        let n = j as f64 / (count - 1) as f64;
        let hz = cached.hz[j];
        let mut db = cached.total[j];
        for k in 0..BANDS {
            if settings.audible(k) && settings.bands[k].kind != 0 && cfg.spectral & (1 << k) == 0 {
                db += live[k].db(hz);
            }
        }
        if !v.spectral.is_empty() {
            let f = n * (v.spectral.len() - 1) as f64;
            let i = f.floor() as usize;
            let t = (f - i as f64) as f32;
            db += f64::from(
                v.spectral[i] * (1. - t) + v.spectral[(i + 1).min(v.spectral.len() - 1)] * t,
            );
        }
        points.push((l.x(n), l.y(db)));
    }
    c.polyline(&points, 4.8 * s, Color::rgb(0xf3d967).with_alpha(20));
    c.polyline(&points, 1.8 * s, Color::rgb(0xf5dc73));
    for k in 0..BANDS {
        if !visible(settings, &v, k) {
            continue;
        }
        let (x, y) = l.node(settings, k);
        let col = band_color(k);
        let selected = v.selection & (1 << k) != 0 || (v.show_dynamics && v.selected == k);
        let radius = if selected { 6. } else { 4.5 } * s;
        c.fill_circle(x, y, radius + 2. * s, Color::rgb(0x171c21));
        c.fill_circle(
            x,
            y,
            radius,
            if settings.audible(k) {
                col
            } else {
                Color::rgb(0x59616b)
            },
        );
        if selected {
            c.stroke_circle(x, y, radius + 3. * s, 1.2 * s, col.with_alpha(170));
        }
        hits.push(Hit {
            r: Rect::new(x - 10. * s, y - 10. * s, 20. * s, 20. * s),
            t: Target::Token(k),
        });
        if cfg.bands[k][0] != 0. {
            let ry = l.y(settings.bands[k].db + f64::from(cfg.bands[k][0]));
            kit::triangle(
                c,
                x,
                ry,
                4. * s,
                if cfg.bands[k][0] < 0. {
                    kit::Dir::Up
                } else {
                    kit::Dir::Down
                },
                col,
            );
            hits.push(Hit {
                r: Rect::new(x - 8. * s, ry - 7. * s, 16. * s, 14. * s),
                t: Target::Dynamic(k * 4),
            });
        }
    }
    if let Some(r) = v.marquee {
        c.fill_rect(r, Color::rgb(0xf0d465).with_alpha(22));
        c.stroke_rrect(r, 0., 1., Color::rgb(0xf0d465));
    }
    c.pop_clip();
    if v.harsh_open || (v.show_dynamics && v.details_open) {
        panel(c, look, &v, &l, &mut hits);
    }
    if v.show_dynamics && !v.harsh_open && !v.hide_band_card {
        let k = v.selected.min(BANDS - 1);
        if settings.bands[k].kind != 0 {
            band_card(c, &mut hits, &v, &l, settings, k);
        }
    }
    // Familiar Lemon footer, kept slim so the graph remains the main editing surface.
    let fy = h - 26. * s;
    button(
        c,
        &mut hits,
        Rect::new(15. * s, fy, 53. * s, 20. * s),
        Target::Lin,
        "LIN",
        flag(look.extra, params::F_LIN),
        s,
    );
    button(
        c,
        &mut hits,
        Rect::new(76. * s, fy, 47. * s, 20. * s),
        Target::Hq,
        "HQ",
        byte(look.extra, params::HQ_BYTE) & 1 != 0,
        s,
    );
    for i in 0..3 {
        button(
            c,
            &mut hits,
            Rect::new(145. * s + i as f32 * 53. * s, fy, 47. * s, 20. * s),
            Target::Monitor(i),
            ["Off", "Pre", "Post"][i as usize],
            byte(look.extra, params::MONITOR_BYTE) == i,
            s,
        );
    }
    button(c, &mut hits, Rect::new(310.*s, fy, 78.*s, 20.*s), Target::GainMatch, "Level match", v.gain_match, s);
    let speed=crate::analyzer::SPEEDS[params::opt(look.extra,Opt::TimeSmoothing).clamp(0,4) as usize];
    button(c, &mut hits, Rect::new(400.*s, fy, 92.*s, 20.*s), Target::Analyzer, &format!("Speed: {speed}"), false, s);
    button(
        c,
        &mut hits,
        Rect::new(w - 121. * s, fy, 99. * s, 20. * s),
        Target::Fader(MAIN_FADER),
        &format!("Out {:+.1} dB", settings.main_db),
        false,
        s,
    );
    for d in [2., 6., 10.] {
        c.line(w - d, h - 2., w - 2., h - d, 1., Color::rgb(0xa1adb5));
    }
    hits
}
fn card_control(c: &mut Canvas, hits: &mut Vec<Hit>, v: &ViewState, r: Rect, target: Target, on: bool) -> Color {
    let hovered = v.mouse.is_some_and(|(x, y)| r.contains(x, y));
    if on || hovered {
        c.fill_rrect(r, 4., Color::rgb(if on { 0xefd065 } else { 0x38444e }));
    }
    hits.push(Hit { r, t: target });
    Color::rgb(if on { 0x222323 } else { 0xd0d9e0 })
}

fn band_card(c: &mut Canvas, hits: &mut Vec<Hit>, v: &ViewState, l: &Layout, settings: &Settings, k: usize) {
    let r = l.band_card(settings, k, v.details_open);
    let s = l.scale;
    let band = settings.bands[k];
    let color = band_color(k);
    let face = Color::rgb(0x21282f);
    let cell = |x: f32, y: f32, w: f32| Rect::new(r.x + x * s, r.y + y * s, w * s, 19. * s);
    let (x, y) = l.node(settings, k);
    if x > r.x + 10. * s && x < r.right() - 10. * s {
        let edge = if y < r.y { r.y } else { r.bottom() };
        let tip = if y < r.y { edge - 5. * s } else { edge + 5. * s };
        c.fill_polygon(&[(x - 6. * s, edge), (x + 6. * s, edge), (x, tip)], color.with_alpha(165));
    }
    c.fill_rrect(r.outset(2. * s), 8. * s, Color::rgb(0x080c10).with_alpha(125));
    c.fill_rect_gradient(r, Color::rgb(0x303941), face, 6. * s);
    c.stroke_rrect(r.inset(0.5 * s), 6. * s, s, color.with_alpha(150));
    hits.push(Hit { r, t: Target::BandCard });

    let power = cell(5., 4., 22.);
    let on = settings.audible(k);
    let ink = card_control(c, hits, v, power, Target::Switch(k), on);
    c.arc(power.cx(), power.cy() + s, 4.5 * s, -PI / 3., 5. * PI / 3., 1.2 * s, ink);
    c.line(power.cx(), power.cy() - 6. * s, power.cx(), power.cy() - s, 1.3 * s, ink);

    let freq = cell(30., 4., 105.);
    let ink = card_control(c, hits, v, freq, Target::Freq(k), false);
    txt(c, freq, 11., s, ink, &params::hz_text(band.hz()));

    let phones = cell(164., 4., 24.);
    let ink = card_control(c, hits, v, phones, Target::Audition(k), v.audition == Some(k));
    let (cx, cy) = (phones.cx(), phones.cy());
    c.arc(cx, cy, 5. * s, PI, PI, 1.4 * s, ink);
    for x in [cx - 6. * s, cx + 3. * s] {
        c.fill_rrect(Rect::new(x, cy - s, 3. * s, 6. * s), s, ink);
    }

    let delete = cell(191., 4., 22.);
    let ink = card_control(c, hits, v, delete, Target::DeleteBand, false);
    let (cx, cy) = (delete.cx(), delete.cy());
    c.line(cx - 3.5 * s, cy - 3.5 * s, cx + 3.5 * s, cy + 3.5 * s, 1.3 * s, ink);
    c.line(cx + 3.5 * s, cy - 3.5 * s, cx - 3.5 * s, cy + 3.5 * s, 1.3 * s, ink);

    let shape = cell(5., 25., 86.);
    let ink = card_control(c, hits, v, shape, Target::BandMenu(k), false);
    shape_icon(c, shape.x + 4. * s, shape.cy(), s, band.kind, color);
    let name = if band.kind == eq::PEAKING { "Bell" } else { params::TYPE_NAMES[band.kind.clamp(0, 7) as usize] };
    txt(c, Rect::new(shape.x + 24. * s, shape.y, shape.w - 28. * s, shape.h), 9., s, ink, name);
    let gain = cell(92., 25., 61.);
    if has_level(band.kind) {
        let ink = card_control(c, hits, v, gain, Target::Fader(k), false);
        txt(c, gain, 10., s, ink, &params::db_text(band.db));
    }
    let q = cell(154., 25., 59.);
    let ink = card_control(c, hits, v, q, Target::Q(k), false);
    txt(c, q, 9., s, ink, &format!("Q {:.2}", q_of(band.width)));
}

fn shape_icon(c: &mut Canvas, x: f32, y: f32, scale: f32, kind: i32, ink: Color) {
    let points = match kind {
        1 => [-2., -2., -2., -1., 1., 3., 4.],
        2 => [4., 3., 0., -4., 0., 3., 4.],
        3 => [4., 3., 1., -1., -2., -2., -2.],
        4 => [-2., -2., -1., 4., -1., -2., -2.],
        5 => [-4., -4., -3., -1., 0., 0., 0.],
        6 => [2., 2., 0., -4., 0., 2., 2.],
        7 => [0., 0., 0., -1., -3., -4., -4.],
        _ => [0.; 7],
    };
    let line: [(f32, f32); 7] = std::array::from_fn(|i| (x + i as f32 * 2.6 * scale, y + points[i] * scale));
    c.polyline(&line, 1.4 * scale, ink);
}

fn panel(c: &mut Canvas, look: &Look, v: &ViewState, l: &Layout, hits: &mut Vec<Hit>) {
    let k = v.selected.min(6);
    let cfg = eq::dynamic::Config::load(look.extra, false);
    let expanded = if v.harsh_open {
        false
    } else {
        !cfg.auto[k] && cfg.bands[k][0] != 0.
    };
    let r = l.panel_for(&look.s, v, expanded);
    let s = r.w / if expanded { 610. } else { 452. };
    c.fill_rrect(
        Rect::new(r.x - 3. * s, r.y + 3. * s, r.w + 6. * s, r.h + 4. * s),
        12. * s,
        Color::rgb(0x050b0e).with_alpha(150),
    );
    c.fill_rect_gradient(
        r,
        Color::rgb(0x303c43).with_alpha(242),
        Color::rgb(0x161f25).with_alpha(247),
        9. * s,
    );
    let color = if v.harsh_open {
        Color::rgb(0xe6d267)
    } else {
        band_color(k)
    };
    c.stroke_rrect(r, 9. * s, 1. * s, color.with_alpha(105));
    c.stroke_rrect(
        r.inset(1.5 * s),
        8. * s,
        0.7 * s,
        Color::rgb(0xe0eef2).with_alpha(35),
    );
    c.push_clip(r.inset(3. * s));
    c.fill_polygon(
        &[
            (r.x, r.y),
            (r.right(), r.y),
            (r.right(), r.y + 17. * s),
            (r.x, r.y + 32. * s),
        ],
        Color::rgb(0xd7eff7).with_alpha(9),
    );
    c.pop_clip();
    c.line(
        r.x + 12. * s,
        r.y + 1.5 * s,
        r.right() - 12. * s,
        r.y + 1.5 * s,
        0.8 * s,
        Color::rgb(0xe4eff1).with_alpha(65),
    );
    if v.harsh_open {
        txt(
            c,
            Rect::new(r.x + 14. * s, r.y + 6. * s, 200. * s, 15. * s),
            11.,
            s,
            color,
            "SPECTRAL DE-HARSH",
        );
        button(
            c,
            hits,
            Rect::new(r.x + 218. * s, r.y + 2. * s, 60. * s, 16. * s),
            Target::Dynamic(39),
            "AUTO",
            cfg.harsh_auto,
            s,
        );
        knob(
            c,
            hits,
            r.x + 65. * s,
            r.y + 61. * s,
            23. * s,
            f64::from(cfg.harsh[0] / 12.),
            Target::Dynamic(28),
            "AMOUNT",
            &format!("{:.1} dB", cfg.harsh[0]),
            color,
            s,
        );
        knob(
            c,
            hits,
            r.x + 165. * s,
            r.y + 61. * s,
            23. * s,
            f64::from((cfg.harsh[1] + 72.) / 72.),
            Target::Dynamic(29),
            "THRESHOLD",
            &format!("{:.0} dBFS", cfg.harsh[1]),
            color,
            s,
        );
        knob(
            c,
            hits,
            r.x + 266. * s,
            r.y + 61. * s,
            23. * s,
            f64::from(cfg.density),
            Target::Dynamic(37),
            "DENSITY",
            &format!("{:.0}%", cfg.density * 100.),
            color,
            s,
        );
        button(
            c,
            hits,
            Rect::new(r.right() - 80. * s, r.y + 39. * s, 67. * s, 24. * s),
            Target::Dynamic(38),
            "3dB TILT",
            cfg.tilt,
            s,
        );
        txt(
            c,
            Rect::new(r.right() - 101. * s, r.y + 75. * s, 90. * s, 17. * s),
            10.,
            s,
            color,
            &format!("{:.1} dB", v.reduction[7]),
        );
        return;
    }
    button(
        c,
        hits,
        Rect::new(r.x + 8. * s, r.y + 7. * s, 25. * s, 20. * s),
        Target::Switch(k),
        "On",
        look.s.audible(k),
        s,
    );
    button(
        c,
        hits,
        Rect::new(r.x + 9. * s, r.y + 39. * s, 86. * s, 25. * s),
        Target::BandMenu(k),
        params::TYPE_NAMES[look.s.bands[k].kind.clamp(0, 7) as usize],
        false,
        s,
    );
    button(
        c,
        hits,
        Rect::new(r.x + 9. * s, r.y + 72. * s, 86. * s, 20. * s),
        Target::OrderMark(k),
        params::ORDER_NAMES[(look.s.bands[k].order + 3).clamp(0, 6) as usize],
        false,
        s,
    );
    let b = look.s.bands[k];
    let gainx = r.x + 229. * s;
    let cy = r.y + 65. * s;
    knob(
        c,
        hits,
        r.x + 140. * s,
        cy,
        23. * s,
        b.freq,
        Target::Freq(k),
        "FREQ",
        &params::hz_text(b.hz()),
        color,
        s,
    );
    knob(
        c,
        hits,
        gainx,
        cy,
        28. * s,
        (b.db + 18.) / 36.,
        Target::Fader(k),
        "GAIN",
        &params::db_text(b.db),
        color,
        s,
    );
    let ring = 34. * s;
    let range = cfg.bands[k][0] / 18.;
    let dynamic_on = range.abs() > 0.001;
    c.stroke_circle(
        gainx,
        cy,
        ring,
        if dynamic_on { 1.6 * s } else { 0.8 * s },
        if dynamic_on {
            Color::rgb(0xf0d768).with_alpha(175)
        } else {
            Color::rgb(0x59606a).with_alpha(85)
        },
    );
    if dynamic_on {
        c.stroke_circle(
            gainx,
            cy,
            ring + 1.5 * s,
            3. * s,
            Color::rgb(0xf0d768).with_alpha(20),
        );
    }
    if range.abs() > 0.001 {
        c.arc(
            gainx,
            cy,
            ring,
            -PI * 0.5 + range.min(0.) * PI,
            range.abs() * PI,
            4. * s,
            Color::rgb(0xf5da65),
        );
        let actual = v.reduction[k] / 18.;
        c.arc(
            gainx,
            cy,
            ring - 4. * s,
            -PI * 0.5 + actual.min(0.) * PI,
            actual.abs() * PI,
            2. * s,
            Color::rgb(0xf0d768),
        );
    }
    button(
        c,
        hits,
        Rect::new(gainx - 110. * s, r.y - 5. * s, 76. * s, 18. * s),
        Target::DynamicToggle,
        "Dynamic",
        dynamic_on,
        s,
    );
    button(
        c,
        hits,
        Rect::new(gainx - 28. * s, r.y - 5. * s, 56. * s, 18. * s),
        Target::SpectralBand,
        "Spectral",
        dynamic_on && cfg.spectral & (1 << k) != 0,
        s,
    );
    button(
        c,
        hits,
        Rect::new(gainx + 40. * s, r.y - 4. * s, 26. * s, 19. * s),
        Target::ExpandDynamics,
        if expanded { "<<" } else { ">>" },
        expanded,
        s,
    );
    let qx = r.x + if expanded { 455. } else { 315. } * s;
    knob(
        c,
        hits,
        qx,
        cy,
        23. * s,
        b.width,
        Target::Q(k),
        "Q",
        &format!("{:.2}", q_of(b.width)),
        color,
        s,
    );
    if dynamic_on {
        txt(c, Rect::new(r.x+9.*s, r.y+97.*s,86.*s,13.*s),9.,s,color,&format!("GR {:+.1} dB",v.reduction[k]));
    }
    if expanded {
        let tx = r.x + 300. * s;
        let tr = Rect::new(tx, r.y + 23. * s, 15. * s, 76. * s);
        c.fill_rrect(tr, 3. * s, Color::rgb(0x131a21));
        let value = (cfg.bands[k][1] + 72.) / 72.;
        c.fill_rect(
            Rect::new(
                tr.x + 3. * s,
                tr.bottom() - (v.trigger[k] + 72.).clamp(0., 72.) / 72. * tr.h,
                9. * s,
                (v.trigger[k] + 72.).clamp(0., 72.) / 72. * tr.h,
            ),
            color.with_alpha(100),
        );
        c.line(
            tr.x - 3. * s,
            tr.bottom() - value * tr.h,
            tr.right() + 3. * s,
            tr.bottom() - value * tr.h,
            2. * s,
            color,
        );
        txt(
            c,
            Rect::new(tx - 19. * s, r.y + 100. * s, 53. * s, 13. * s),
            9.,
            s,
            color,
            &format!("{:.0}dB", cfg.bands[k][1]),
        );
        hits.push(Hit {
            r: Rect::new(tx - 10. * s, tr.y - 4. * s, 35. * s, 96. * s),
            t: Target::Dynamic(k * 4 + 1),
        });
        button(
            c,
            hits,
            Rect::new(tx + 2. * s, r.y + 3. * s, 36. * s, 16. * s),
            Target::AutoDynamics,
            "Auto",
            cfg.auto[k],
            s,
        );
        knob(
            c,
            hits,
            r.x + 350. * s,
            r.y + 72. * s,
            14. * s,
            f64::from(cfg.bands[k][2] / 200.),
            Target::Dynamic(k * 4 + 2),
            "Attack",
            &format!("{:.0}ms", cfg.bands[k][2]),
            color,
            s,
        );
        knob(
            c,
            hits,
            r.x + 395. * s,
            r.y + 72. * s,
            14. * s,
            f64::from(cfg.bands[k][3] / 1500.),
            Target::Dynamic(k * 4 + 3),
            "Release",
            &format!("{:.0}ms", cfg.bands[k][3]),
            color,
            s,
        );
    }
    let end = r.right() - 94. * s;
    button(
        c,
        hits,
        Rect::new(end, r.y + 41. * s, 84. * s, 22. * s),
        Target::BandMenu(k),
        eq::routed::NAMES[eq::routed::routes(look.extra, false)[k] as usize],
        false,
        s,
    );
    button(
        c,
        hits,
        Rect::new(r.right() - 92. * s, r.y + 7. * s, 22. * s, 18. * s),
        Target::PrevBand,
        "<",
        false,
        s,
    );
    txt(
        c,
        Rect::new(r.right() - 68. * s, r.y + 7. * s, 24. * s, 18. * s),
        10.,
        s,
        color,
        &(k + 1).to_string(),
    );
    button(
        c,
        hits,
        Rect::new(r.right() - 43. * s, r.y + 7. * s, 22. * s, 18. * s),
        Target::NextBand,
        ">",
        false,
        s,
    );
    button(
        c,
        hits,
        Rect::new(r.right() - 19. * s, r.y + 7. * s, 15. * s, 18. * s),
        Target::DeleteBand,
        "x",
        false,
        s,
    );
    button(
        c,
        hits,
        Rect::new(end + 17. * s, r.y + 78. * s, 50. * s, 21. * s),
        Target::Solo(k),
        "Listen",
        v.pressed == Some(Target::Solo(k)),
        s,
    );
}

#[cfg(test)] mod response_tests {
    use super::*;
    #[test] fn cached_graph_agrees_with_original_curve_after_edits_and_mode_changes() {
        let mut s=Settings::default();s.bands[3].db=9.;s.bands[1].kind=eq::HIGH_PASS;
        for rate in [44100.,96000.] { for opts in [Options::default(),Options{hq:1,flags:2,..Options::default()},Options{hq:3,..Options::default()}] {
            for mask in [0x7f,0x08,0x01] {s.mask=mask;for width in [0.2,0.8] {s.bands[3].width=width;
                let c=responses(s,opts,rate,500);let reference=s.curve(opts.design(rate));
                for j in 0..500 { assert!((c.total[j]-reference.db(c.hz[j])).abs()<1e-10); }
                assert!(std::rc::Rc::ptr_eq(&c,&responses(s,opts,rate,500)));
            }}
        }}
    }
}

#[cfg(test)]mod spectrum_polish_tests {
 use super::*;
 #[test]fn smooth_outline_stays_bounded_and_keeps_narrow_peaks(){
  let r=Rect::new(20.,44.,928.,520.);let mut values=vec![-90.;500];values[250]=-12.;
  let p=spectrum_points(&values,r,90.);assert_eq!(p.len(),999);
  for pair in p.windows(2){assert!(pair[0].0<pair[1].0);}
  assert!(p.iter().all(|&(x,y)|x.is_finite()&&y.is_finite()&&x>=r.x&&x<=r.right()&&y>=r.y&&y<=r.bottom()));
  let top=p.iter().map(|p|p.1).fold(f32::INFINITY,f32::min);let db=(r.bottom()-top)/r.h*90.-90.;assert!(db < -12.&&db > -13.3,"peak{db}");
  let flat=spectrum_points(&vec![-30.;500],r,90.);assert!(flat.iter().all(|p|(p.1-flat[0].1).abs()<0.001));
  assert!(spectrum_points(&[],r,90.).is_empty());assert!(spectrum_points(&[f32::NAN,0.,f32::INFINITY],r,90.).iter().all(|p|p.1.is_finite()));
 }
}
