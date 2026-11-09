// SPDX-License-Identifier: GPL-3.0-or-later
//! Full mixer console finish. Painters only: control geometry, hit targets and audio laws stay in mixer.rs.
use crate::{app::Ui, meters};
use os_ui::{Color, Font, Rect};
use std::f32::consts::PI;
const INK: Color = Color::rgb(0xe1ddd1);
const DARK: Color = Color::rgb(0x111315);

pub(super) fn panel(ui: &mut Ui, r: Rect, base: Color, selected: bool) {
    if !ui.c.visible(r.outset(6.)) {return;}
    ui.c.fill_rect(r, Color::rgb(0x101214));
    let face=r.inset(1.5);
    let _=base;
    super::material::surface(ui,face,super::material::Finish::Graphite);
    ui.c.stroke_rrect(face,3.,0.7,Color::rgb(0x77828a).with_alpha(110));
    if selected {ui.c.stroke_rrect(face,3.,1.,Color::rgb(0xe4c74f));}
}

pub(super) fn well(ui: &mut Ui, r: Rect) {
    if !ui.c.visible(r.outset(6.)) {return;}
    super::material::surface(ui,r,super::material::Finish::Graphite);
    ui.c.fill_rrect(r,3.,Color::rgb(0x080a0c).with_alpha(40));
    ui.c.stroke_rrect(r,3.,0.7,Color::rgb(0x111315));
}

pub(super) fn plate(ui: &mut Ui, r: Rect) {
    if !ui.c.visible(r.outset(6.)) {return;}
    ui.c.fill_rect_gradient(r.inset(0.5),Color::rgb(0x303438),Color::rgb(0x222528),2.);
    ui.c.line(r.x+2.,r.y+0.5,r.right()-2.,r.y+0.5,0.6,Color::rgb(0x697177).with_alpha(90));
    ui.c.line(r.x+2.,r.bottom(),r.right()-2.,r.bottom(),0.8,DARK);
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
    ui.c.fill_rrect(Rect::new(r.x,r.y+1.,r.w,r.h),2.5,Color::rgb(0x090a0b));
    ui.c.fill_rrect(r,2.5,Color::rgb(0x111315));
    let face=r.inset(1.);
    let base=if on {col}else{Color::rgb(if hover {0x44494e}else{0x34393d})};
    ui.c.fill_rect_gradient(face,base.mix(Color::rgb(0xffffff),if on {0.25}else{0.10}),base.mix(DARK,if on {0.08}else{0.25}),2.);
    ui.c.line(face.x+2.,face.y+0.5,face.right()-2.,face.y+0.5,0.7,INK.with_alpha(if hover {140}else{65}));
    if on {Color::rgb(0x171b1e)}else{Color::rgb(0xdce0e2)}
}

/// Backlit console switch: the symbol lives inside a recessed glass lens.
pub(super) fn signal_switch(ui: &mut Ui, r: Rect, on: bool, hover: bool, swap: bool) {
    if !ui.c.visible(r.outset(6.)) {return;}
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
    if !ui.c.visible(Rect::new(x-r-5.,y-r-5.,2.*r+10.,2.*r+10.)) {return;}
    let q=r*48./39.;
    ui.c.image_rgba(Rect::new(x-q,y-q+2.*r/39.,q*2.,q*2.),super::material::DIM,super::material::DIM,super::material::knob());
    // Restrained colour coding sits outside the silver rim, with a clear gap.
    ui.c.arc(x,y,r+2.1,-225.*PI/180.,270.*PI/180.,if hover {1.2}else{0.8},cap.mix(DARK,if hover {0.0}else{0.12}));
    let a=(-225.+270.*value.clamp(0.,1.))*PI/180.;
    ui.c.line(x+a.cos()*r*0.25,y+a.sin()*r*0.25,x+a.cos()*r*0.74,y+a.sin()*r*0.74,(r*0.11).clamp(0.7,1.4),Color::rgb(0xedf1ed));
}
pub(super) fn rocker(ui: &mut Ui, r: Rect, on: bool, hover: bool) {
    if !ui.c.visible(r.outset(6.)) {return;}
    ui.c.fill_rrect(r.outset(1.), 2., DARK);
    ui.c.fill_rect_gradient(r, Color::rgb(0x666e74), Color::rgb(0x292f34), 2.);
    let face = r.inset(2.);
    let h = face.h / 2.;
    for i in 0..2 {
        let raised = (i == 1) == on;
        ui.c.fill_rect_gradient(
            Rect::new(face.x, face.y + i as f32 * h, face.w, h),
            Color::rgb(if raised { 0xe1e6e9 } else { 0x68727a }),
            Color::rgb(if raised { 0x9aa6ae } else { 0x394249 }),
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
pub(super) fn fader(ui: &mut Ui, x: f32, y: f32, _selected: bool, hover: bool) {
    let r=Rect::new(x-14.5,y-17.,29.,37.);
    ui.c.image_rgba(r,super::material::DIM,super::material::DIM,super::material::fader());
    if hover {ui.c.line(x-9.,y-14.,x+9.,y-14.,0.7,Color::rgb(0xf8fafb));}
}
pub(super) fn vu(ui: &mut Ui, r: Rect, levels: [f32; 2], clipped: bool, selected: bool) {
    if !ui.c.visible(r.outset(6.)) {return;}
    ui.c.fill_rrect(
        Rect::new(r.x - 2., r.y - 1., r.w + 4., r.h + 4.),
        5.,
        Color::rgb(0x060807),
    );
    ui.c.fill_rect_gradient(r.outset(1.5), Color::rgb(0x71767a), Color::rgb(0x151719), 4.);
    ui.c.line(r.x+4.,r.y-0.6,r.right()-4.,r.y-0.6,0.7,Color::rgb(0xd5dce0).with_alpha(155));
    ui.c.fill_rrect(r, 3., Color::rgb(0x171b1f));
    let face = r.inset(2.);
    ui.c.push_clip(face);
    super::material::surface(ui,face,super::material::Finish::Silver);
    let ink = Color::rgb(0x202428);
    let red = Color::rgb(0x9b3428);
    let (cx,cy,x,y) = meter_scale(ui,face,levels,ink,red);
    ui.c.line(
        cx + 1.,
        cy + 1.,
        x + 1.,
        y + 1.,
        1.4,
        Color::rgb(0x444b51).with_alpha(100),
    );
    ui.c.line(cx, cy, x, y, 1.1, Color::rgb(0x25282c));
    ui.c.fill_circle(cx, cy, 3., Color::rgb(0x262a2f));
    ui.c.fill_circle(cx - 0.4, cy - 0.5, 1.3, Color::rgb(0xafb8bf));
    ui.c.fill_polygon(
        &[
            (face.x, face.y),
            (face.right(), face.y),
            (face.right(), face.y + face.h * 0.12),
            (face.x, face.y + face.h * 0.36),
        ],
        Color::rgb(0xffffff).with_alpha(40),
    );
    ui.c.line(
        face.x,
        face.y + 1.,
        face.right(),
        face.y + 1.,
        2.,
        Color::rgb(0x56616b).with_alpha(90),
    );
    ui.c.pop_clip();
    ui.c.stroke_rrect(
        r,
        3.,
        0.7,
        Color::rgb(0xd2dae1).with_alpha(if selected { 100 } else { 45 }),
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

pub(super) fn meter_scale(ui:&mut Ui, face:Rect, levels:[f32;2], ink:Color, red:Color)->(f32,f32,f32,f32){
    let cx = face.cx();
    let cy = face.bottom() - 5.;
    let labels = face.h >= 33. && face.w >= 47.;
    let radius = (face.w * 0.42).min(cy - face.y - if labels {12.} else {3.});
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
        if major && labels {
            draw_scale_number(ui, face, cx, cy, radius, x1, db, if db > 0. {red} else {ink});
        }
    }
    ui.c.text_center(
        Font::light(11,0).bold(),
        Rect::new(cx - 14., cy - 21., 28., 12.),
        ink,
        "VU",
    );
    if face.h > 80. {
        ui.c.text_center(
            Font::new(5),
            Rect::new(cx - 23., face.bottom() - 6., 46., 6.),
            ink,
            "0 VU = -18 dBFS",
        );
    }
    let level = (levels[0] + levels[1]) * 0.5;
    let (x, y) = point(meters::vu_fraction(meters::vu_db(level)), radius - 1.5);
    (cx,cy,x,y)
}

/// Set each number above the nearest part of the curve under its full text width.
/// Near the ends a centered label alone would let its inner corner cross the arc.
pub(super) fn draw_scale_number(ui: &mut Ui, face: Rect, cx: f32, cy: f32, radius: f32,
                                tick_x: f32, db: f32, ink: Color) {
    let label = match db as i32 {
        -20 => "-20", -5 => "-5", 0 => "0", 3 => "+3", _ => return,
    };
    let font = Font::light(if face.w < 55. {7} else {8}, 0);
    let width = ui.c.text_width(font, label).ceil();
    let height = ui.c.line_height(font).ceil();
    let offset = match db as i32 { -20 => -4., -5 => -4., 3 => 4., _ => 0. };
    let left = (tick_x + offset - width / 2.).clamp(face.x+1., face.right()-width-1.);
    let nearest = cx.clamp(left, left+width) - cx;
    let arc_y = cy - (radius * radius - nearest * nearest).max(0.).sqrt();
    let gap = if db == -5. {2.} else {3.};
    let top = (arc_y - gap - height).floor().max(face.y+1.);
    ui.c.text(font, left.round(), top, ink, label);
}
