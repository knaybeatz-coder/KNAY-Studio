// SPDX-License-Identifier: GPL-3.0-or-later
//! Full mixer console finish. Painters only: control geometry, hit targets and audio laws stay in mixer.rs.
use crate::{app::Ui, meters};
use os_ui::{Color, Font, Rect};
use std::f32::consts::PI;
const INK: Color = Color::rgb(0xe1ddd1);
const DARK: Color = Color::rgb(0x111315);

pub(super) fn panel(ui: &mut Ui, r: Rect, base: Color, selected: bool) {
    ui.c.fill_rect(r, DARK);
    let face = r.inset(2.);
    ui.c.fill_rect_gradient(
        face,
        base.mix(Color::rgb(0x858781), 0.15),
        base.mix(DARK, 0.2),
        2.,
    );
    // Rolled edges and restrained horizontal grain, anchored to the panel rather than the screen.
    ui.c.line(
        face.x,
        face.y,
        face.x,
        face.bottom(),
        1.,
        Color::rgb(0xa1a39b).with_alpha(65),
    );
    ui.c.line(
        face.right(),
        face.y,
        face.right(),
        face.bottom(),
        2.,
        DARK.with_alpha(180),
    );
    for i in 1..(face.h / 9.) as usize {
        let y = face.y + i as f32 * 9.;
        ui.c.line(
            face.x + 2.,
            y,
            face.right() - 2.,
            y,
            0.5,
            Color::rgb(0xffffff).with_alpha(if i % 3 == 0 { 5 } else { 2 }),
        );
    }
    if selected {
        ui.c.fill_rect(Rect::new(r.x + 1., r.y, 1.5, r.h), Color::rgb(0xd4bc77));
    }
}
pub(super) fn well(ui: &mut Ui, r: Rect) {
    ui.c.fill_rrect(r, 4., Color::rgb(0x111213));
    ui.c.fill_rect_gradient(r.inset(1.5), Color::rgb(0x242726), Color::rgb(0x343735), 3.);
    ui.c.line(
        r.x + 3.,
        r.y + 1.,
        r.right() - 3.,
        r.y + 1.,
        1.,
        Color::rgb(0x050607),
    );
    ui.c.line(
        r.x + 3.,
        r.bottom() - 1.,
        r.right() - 3.,
        r.bottom() - 1.,
        1.,
        Color::rgb(0x72766d).with_alpha(70),
    );
}
pub(super) fn plate(ui: &mut Ui, r: Rect) {
    ui.c.fill_rect_gradient(r, Color::rgb(0x383c3a), Color::rgb(0x292d2b), 2.);
    ui.c.line(
        r.x + 2.,
        r.y,
        r.right() - 2.,
        r.y,
        0.8,
        Color::rgb(0x858a80).with_alpha(48),
    );
    ui.c.line(
        r.x + 2.,
        r.bottom(),
        r.right() - 2.,
        r.bottom(),
        1.,
        DARK.with_alpha(180),
    );
}
pub(super) fn screw(ui: &mut Ui, x: f32, y: f32) {
    ui.c.fill_circle(x, y + 0.7, 2.7, Color::rgb(0x080909));
    ui.c.fill_circle(x, y, 2.1, Color::rgb(0x717570));
    ui.c.arc(x, y, 1.8, PI, PI, 0.7, Color::rgb(0xb3b5ac));
    ui.c.line(x - 1., y + 0.6, x + 1., y - 0.6, 1., Color::rgb(0x1a1c1b));
}
pub(super) fn lamp(ui: &mut Ui, x: f32, y: f32, on: bool, col: Color, r: f32) {
    ui.c.fill_circle(x, y + 0.6, r + 1.5, DARK);
    ui.c.stroke_circle(x, y, r + 0.6, 1., Color::rgb(0x686c64));
    if on {
        ui.c.fill_circle(x, y, r + 2., col.with_alpha(26));
    }
    ui.c.fill_circle(x, y, r, if on { col } else { col.mix(DARK, 0.79) });
    ui.c.fill_circle(
        x - r * 0.25,
        y - r * 0.3,
        r * 0.32,
        if on {
            Color::rgb(0xfff5d5)
        } else {
            Color::rgb(0x929489).with_alpha(70)
        },
    );
}
pub(super) fn button(ui: &mut Ui, r: Rect, on: bool, hover: bool, col: Color) -> Color {
    ui.c.fill_rrect(Rect::new(r.x, r.y + 1., r.w, r.h), 3., Color::rgb(0x070808));
    ui.c.fill_rrect(r, 3., Color::rgb(0x101211));
    let face = r.inset(1.5);
    let base = if on {
        col.mix(Color::rgb(0x3b4036), 0.42)
    } else {
        Color::rgb(0x454a46)
    };
    let base = if hover { base.mix(INK, 0.12) } else { base };
    ui.c.fill_rect_gradient(
        face,
        base.mix(INK, if on { 0.15 } else { 0.23 }),
        base.mix(DARK, 0.25),
        2.,
    );
    ui.c.line(
        face.x + 1.,
        face.y + 0.5,
        face.right() - 1.,
        face.y + 0.5,
        0.7,
        INK.with_alpha(100),
    );
    ui.c.line(
        face.x + 1.,
        face.bottom() - 0.6,
        face.right() - 1.,
        face.bottom() - 0.6,
        1.,
        DARK.with_alpha(170),
    );
    if on {
        ui.c.fill_rrect(
            Rect::new(face.x + 3., face.bottom() - 2., (face.w - 6.).max(1.), 1.),
            0.5,
            col.with_alpha(180),
        );
    }
    if on {
        Color::rgb(0xfff2c6)
    } else {
        INK
    }
}
/// Backlit console switch: the symbol lives inside a recessed glass lens.
pub(super) fn signal_switch(ui: &mut Ui, r: Rect, on: bool, hover: bool, swap: bool) {
    let tint = Color::rgb(if swap { 0x79b9c8 } else { 0xd6b776 });
    button(ui, r, false, hover, tint);
    let lens = r.inset(3.);
    ui.c.fill_rrect(lens.outset(0.6), 1.8, Color::rgb(0x090c0b));
    ui.c.fill_rect_gradient(lens, tint.mix(DARK, if on {0.55} else {0.88}),
        tint.mix(DARK, if on {0.22} else {0.75}), 1.4);
    ui.c.push_clip(lens);
    let x=r.cx(); let y=r.cy();
    let ink=tint.mix(INK, if on {0.34} else if hover {0.12} else {0.0})
        .mix(DARK, if on {0.0} else {0.38});
    if swap {
        let mut g=os_ui::Glyph::new();
        g.polygon(&[(x-1.,y-2.8),(x-4.,y),(x-1.,y+2.8)]);
        g.polygon(&[(x+1.,y-2.8),(x+4.,y),(x+1.,y+2.8)]);
        ui.c.glyph(ink,&g);
    } else {
        // Familiar polarity symbol, engraved into the amber lens.
        ui.c.stroke_circle(x,y,2.7,1.,ink);
        ui.c.line(x-3.2,y+3.2,x+3.2,y-3.2,1.,ink);
    }
    ui.c.line(lens.x+1.,lens.y+0.6,lens.right()-1.,lens.y+0.6,0.7,
        tint.with_alpha(if on {105} else {40}));
    ui.c.pop_clip();
}
pub(super) fn knob(ui: &mut Ui, x: f32, y: f32, r: f32, value: f32, cap: Color, hover: bool) {
    let r = r.max(1.);
    ui.c.fill_circle(
        x + 0.6,
        y + 1.4,
        r + 1.8,
        Color::rgb(0x070809).with_alpha(210),
    );
    ui.c.fill_circle(x, y, r + 1.3, Color::rgb(0x141615));
    ui.c.stroke_circle(x, y, r + 1., 0.8, Color::rgb(0x767d71).with_alpha(110));
    if r >= 6. {
        for i in 0..14 {
            let a = i as f32 / 14. * 2. * PI;
            ui.c.line(
                x + a.cos() * (r - 0.2),
                y + a.sin() * (r - 0.2),
                x + a.cos() * (r - 1.6),
                y + a.sin() * (r - 1.6),
                0.8,
                if i < 7 {
                    Color::rgb(0x4e534c)
                } else {
                    Color::rgb(0x252825)
                },
            );
        }
    }
    let face = r * 0.79;
    let cap = cap.mix(Color::rgb(0x696d60), 0.25);
    let top = cap.mix(INK, if hover { 0.43 } else { 0.28 });
    ui.c.fill_rect_gradient(
        Rect::new(x - face, y - face, face * 2., face * 2.),
        top,
        cap.mix(DARK, 0.34),
        face,
    );
    ui.c.arc(x, y, face - 0.5, PI, PI, 0.8, INK.with_alpha(90));
    let a = (-225. + 270. * value.clamp(0., 1.)) * PI / 180.;
    ui.c.line(
        x + a.cos() * face * 0.24,
        y + a.sin() * face * 0.24,
        x + a.cos() * face * 0.87,
        y + a.sin() * face * 0.87,
        (r * 0.16).clamp(0.65, 1.8),
        Color::rgb(0x111510),
    );
    ui.c.line(
        x + a.cos() * face * 0.3 - 0.3,
        y + a.sin() * face * 0.3 - 0.3,
        x + a.cos() * face * 0.82 - 0.3,
        y + a.sin() * face * 0.82 - 0.3,
        (r * 0.09).clamp(0.5, 1.),
        Color::rgb(0xfff6d8),
    );
}
pub(super) fn rocker(ui: &mut Ui, r: Rect, on: bool, hover: bool) {
    ui.c.fill_rrect(r.outset(1.), 2., DARK);
    ui.c.fill_rect_gradient(r, Color::rgb(0x92978c), Color::rgb(0x474d45), 2.);
    let face = r.inset(2.);
    let h = face.h / 2.;
    for i in 0..2 {
        let raised = (i == 1) == on;
        ui.c.fill_rect_gradient(
            Rect::new(face.x, face.y + i as f32 * h, face.w, h),
            Color::rgb(if raised { 0xe9e7d9 } else { 0x767c71 }),
            Color::rgb(if raised { 0xa6ada0 } else { 0x4a5147 }),
            1.,
        );
    }
    ui.c.line(face.x, face.cy(), face.right(), face.cy(), 1., DARK);
    if hover {
        ui.c.stroke_rrect(r, 2., 0.8, INK.with_alpha(160));
    }
    lamp(
        ui,
        face.cx(),
        if on {
            face.y + 3.5
        } else {
            face.bottom() - 3.5
        },
        on,
        Color::rgb(0xb8d88e),
        1.1,
    );
}
pub(super) fn fader(ui: &mut Ui, x: f32, y: f32, selected: bool, hover: bool) {
    let r = Rect::new(x - 9., y - 13., 18., 26.);
    ui.c.fill_rrect(
        Rect::new(r.x + 1., r.y + 3., r.w + 1., r.h),
        2.,
        Color::rgb(0x090b0b).with_alpha(220),
    );
    ui.c.fill_rrect(r, 2., Color::rgb(0x191e1d));
    ui.c.fill_rect_gradient(
        r.inset(1.),
        Color::rgb(if hover { 0xf0f0df } else { 0xd4d6c9 }),
        Color::rgb(0x68736b),
        1.5,
    );
    ui.c.fill_rect_gradient(
        Rect::new(r.x + 2., r.y + 4., r.w - 4., r.h - 8.),
        Color::rgb(0xc2c8ba),
        Color::rgb(0x99a391),
        1.,
    );
    for dy in [-7., -4., 4., 7.] {
        ui.c.line(
            r.x + 3.,
            y + dy,
            r.right() - 3.,
            y + dy,
            0.7,
            Color::rgb(0x687266),
        );
        ui.c.line(
            r.x + 3.,
            y + dy + 0.8,
            r.right() - 3.,
            y + dy + 0.8,
            0.6,
            Color::rgb(0xe0e3d5),
        );
    }
    ui.c.fill_rect(
        Rect::new(r.x + 1.5, y - 1., r.w - 3., 2.),
        Color::rgb(0x202922),
    );
    if selected {
        ui.c.fill_rect(
            Rect::new(r.x + 2., r.bottom() - 3., r.w - 4., 1.),
            Color::rgb(0xe5c777),
        );
    }
    ui.c.line(
        r.x + 1.,
        r.y + 1.,
        r.right() - 1.,
        r.y + 1.,
        1.,
        Color::rgb(0xf3f0df),
    );
}
pub(super) fn vu(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, selected: bool) {
    ui.c.fill_rrect(
        Rect::new(r.x - 2., r.y - 1., r.w + 4., r.h + 4.),
        5.,
        Color::rgb(0x060807),
    );
    ui.c.fill_rect_gradient(r.outset(1.), Color::rgb(0x6d7067), Color::rgb(0x222720), 4.);
    ui.c.fill_rrect(r, 3., Color::rgb(0x0f120f));
    let face = r.inset(2.);
    ui.c.push_clip(face);
    ui.c.fill_rect_gradient(face, Color::rgb(0xbaa979), Color::rgb(0xf1e4b7), 2.);
    ui.c.fill_rect_gradient(
        Rect::new(face.x, face.y + 3., face.w, face.h - 3.),
        Color::rgb(0xecdba7),
        Color::rgb(0xdcc995),
        1.,
    );
    let ink = Color::rgb(0x302f25);
    let red = Color::rgb(0x9b3428);
    let cx = face.cx();
    let cy = face.bottom() - 10.;
    let radius = (face.w * 0.43).min(face.h * 0.72);
    let point = |f: f32, rr: f32| {
        let a = (-150. + 120. * f) * PI / 180.;
        (cx + rr * a.cos(), cy + rr * a.sin())
    };
    let zero = meters::vu_fraction(0.);
    ui.c.arc(
        cx,
        cy,
        radius,
        -150. * PI / 180.,
        120. * PI / 180.,
        0.8,
        ink,
    );
    ui.c.arc(
        cx,
        cy,
        radius,
        (-150. + 120. * zero) * PI / 180.,
        120. * (1. - zero) * PI / 180.,
        2.,
        red,
    );
    for db in [-20., -10., -7., -5., -3., -2., -1., 0., 1., 2., 3.] {
        let f = meters::vu_fraction(db);
        let major = matches!(db, -20. | -5. | 0. | 3.);
        let (x0, y0) = point(f, radius - if major { 5. } else { 3. });
        let (x1, y1) = point(f, radius);
        ui.c.line(x0, y0, x1, y1, 0.8, if db > 0. { red } else { ink });
        if major && face.h > 42. {
            let (x, y) = point(f, radius + 6.);
            let label = if db > 0. {
                format!("+{db:0.0}")
            } else {
                format!("{db:0.0}")
            };
            ui.c.text_center(
                Font::new(8),
                Rect::new((x - 8.).clamp(face.x + 1., face.right() - 17.), y - 5., 16., 10.),
                if db > 0. { red } else { ink },
                &label,
            );
        }
    }
    ui.c.text_center(
        Font::new(10).bold(),
        Rect::new(cx - 14., cy - 21., 28., 12.),
        ink,
        "VU",
    );
    if face.h > 49. {
        ui.c.text_center(
            Font::new(5),
            Rect::new(cx - 23., face.bottom() - 6., 46., 6.),
            ink,
            "0 VU = -18 dBFS",
        );
    }
    let level = (levels[0] + levels[1]) * 0.5;
    let (x, y) = point(meters::vu_fraction(meters::vu_db(level)), radius - 1.5);
    ui.c.line(
        cx + 1.,
        cy + 1.,
        x + 1.,
        y + 1.,
        1.4,
        Color::rgb(0x6a5e42).with_alpha(100),
    );
    ui.c.line(cx, cy, x, y, 1.1, Color::rgb(0x25261e));
    ui.c.fill_circle(cx, cy, 3., Color::rgb(0x272c23));
    ui.c.fill_circle(cx - 0.4, cy - 0.5, 1.3, Color::rgb(0xa3a48d));
    ui.c.fill_polygon(
        &[
            (face.x, face.y),
            (face.right(), face.y),
            (face.right(), face.y + face.h * 0.12),
            (face.x, face.y + face.h * 0.36),
        ],
        Color::rgb(0xffffff).with_alpha(28),
    );
    ui.c.line(
        face.x,
        face.y + 1.,
        face.right(),
        face.y + 1.,
        2.,
        Color::rgb(0x51492c).with_alpha(90),
    );
    ui.c.pop_clip();
    ui.c.stroke_rrect(
        r,
        3.,
        0.7,
        Color::rgb(0xe9dcb4).with_alpha(if selected { 100 } else { 45 }),
    );
    lamp(
        ui,
        r.right() - 6.,
        r.bottom() - 6.,
        clipped,
        Color::rgb(0xf46335),
        1.7,
    );
}

/// Raised brushed-metal divider, with a narrow highlight, shadow and end fasteners.
pub(super) fn separator(ui:&mut Ui,r:Rect){
    ui.c.fill_rect(r,Color::rgb(0x101313));
    ui.c.fill_rect_gradient(Rect::new(r.x+1.,r.y,r.w-2.,r.h),Color::rgb(0x92988f),Color::rgb(0x555d58),1.);
    ui.c.line(r.x+1.,r.y,r.x+1.,r.bottom(),1.,Color::rgb(0xc0c5ba));
    ui.c.line(r.x+3.,r.y,r.x+3.,r.bottom(),1.,Color::rgb(0x858d81));
    ui.c.line(r.right()-1.,r.y,r.right()-1.,r.bottom(),1.,Color::rgb(0x2b322e));
    for y in [r.y+8.,r.bottom()-8.] {ui.c.fill_circle(r.cx(),y,1.8,Color::rgb(0x29312b));ui.c.line(r.cx()-0.8,y+0.5,r.cx()+0.8,y-0.5,0.6,Color::rgb(0xaeb7a8));}
}
