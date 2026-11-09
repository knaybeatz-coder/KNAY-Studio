// SPDX-License-Identifier: GPL-3.0-or-later
//! Native local skin controls with resolution-independent fallbacks.
//! Live values select original sprite frames; no screenshot crops or enlarged JPEGs.
use os_ui::{Canvas, Color, Font, Rect};

fn ink(hex: u32) -> Color { Color::rgb(hex) }

fn step(c: &mut Canvas, r: Rect, red: bool, on: bool) {
    if crate::vintage_skin::draw(c,"SSBtns",usize::from(red)+2*usize::from(on),r) {return;}
    let face=match (red,on) {
        (false,false)=>ink(0x4f5558), (true,false)=>ink(0x5d5051),
        (false,true)=>ink(0xc4cdd2), (true,true)=>ink(0xd7c5c7),
    };
    // Deep socket, rounded shoulder, inset top notch and a softly rolled lower edge.
    let socket=Rect::new(r.x+0.5,r.y+1.,r.w-1.,r.h-1.5);
    c.fill_rrect(socket,3.,ink(0x20292d));
    c.fill_rect_gradient(socket.inset(1.),ink(0x929c9e),ink(0x182226),2.3);
    let shell=Rect::new(r.x+2.,r.y+2.,r.w-4.,r.h-5.);
    c.fill_rect_gradient(shell,face.shade(0.25),face.shade(-0.38),1.8);
    c.fill_rect_gradient(Rect::new(r.x+3.,r.y+8.,r.w-6.,r.h-13.),face.shade(0.02),face.shade(-0.16),1.2);
    c.fill_rrect(Rect::new(r.x+4.,r.y+5.,r.w-8.,3.),0.7,face.shade(-0.55));
    c.fill_rect(Rect::new(r.x+5.,r.y+5.,r.w-10.,1.),face.shade(-0.68));
    c.fill_rect(Rect::new(r.x+4.,r.y+3.,r.w-8.,1.),face.shade(0.38));
    c.fill_rect(Rect::new(r.x+3.,r.y+10.,1.,r.h-17.),face.shade(0.2));
}
pub fn step_gray(c:&mut Canvas,r:Rect) {step(c,r,false,false);}
pub fn step_red(c:&mut Canvas,r:Rect) {step(c,r,true,false);}
pub fn step_gray_on(c:&mut Canvas,r:Rect) {step(c,r,false,true);}
pub fn step_red_on(c:&mut Canvas,r:Rect) {step(c,r,true,true);}

pub fn knob(c:&mut Canvas,r:Rect,value:f32) {
    let frame = (value.clamp(0.0, 1.0) * 60.0).round() as usize;
    let bounds = Rect::new(r.x.round(), r.y.round(), r.w, r.h);
    if crate::vintage_skin::alpha_draw(c, "MixerWheel", frame, bounds) { return; }
    let (cx,cy)=(r.cx(),r.cy());
    let k=(r.w/22.).min(r.h/24.).min(1.);
    c.fill_circle(cx,cy+k,9.5*k,ink(0x263136));
    c.fill_rect_gradient(Rect::new(cx-9.*k,cy-9.*k,18.*k,18.*k),ink(0xaab5b6),ink(0x2a363d),9.*k);
    c.fill_rect_gradient(Rect::new(cx-7.*k,cy-7.*k,14.*k,14.*k),ink(0x74858b),ink(0x27363c),7.*k);
    // Soft off-axis reflection stays distinct from the moving value pointer.
    c.fill_circle(cx-3.*k,cy-3.*k,3.*k,ink(0xc7d5d7).with_alpha(28));
    c.fill_circle(cx-3.*k,cy-3.*k,1.7*k,ink(0xc7d5d7).with_alpha(85));
    let a=(0.75+value.clamp(0.,1.)*1.5)*std::f32::consts::PI;
    c.line(cx+a.cos()*2.*k,cy+a.sin()*2.*k,cx+a.cos()*6.*k,cy+a.sin()*6.*k,1.2*k,ink(0xd4dddd));
}
fn led(c:&mut Canvas,r:Rect,on:bool) {
    if crate::vintage_skin::alpha_native(c, "MixerLamp", usize::from(on), (r.cx()-4.).round(), (r.cy()-4.).round()) {return;}
    let (cx,cy)=(r.cx(),r.cy());
    c.fill_circle(cx,cy,4.5,ink(0x879294));
    c.fill_circle(cx,cy+0.4,3.6,ink(0x253235));
    let light=if on {ink(0x9acc63)} else {ink(0x52614b)};
    c.fill_rect_gradient(Rect::new(cx-2.5,cy-2.5,5.,5.),light,light.shade(-0.4),2.5);
    if on {c.fill_circle(cx-0.7,cy-1.,0.8,ink(0xdbe8b0));}
}
pub fn led_on(c:&mut Canvas,r:Rect) {led(c,r,true);}
pub fn led_off(c:&mut Canvas,r:Rect) {led(c,r,false);}
fn selector(c:&mut Canvas,r:Rect,on:bool) {
    if crate::vintage_skin::draw(c,"SelectBtn",usize::from(on),r) {return;}
    let body=Rect::new(r.cx()-3.5,r.y+2.,7.,r.h-4.);
    c.fill_rrect(body.outset(1.),4.,ink(0x7b898c));
    c.fill_rrect(body,3.5,ink(0x17272b));
    c.fill_rect_gradient(body.inset(1.5),if on {ink(0xc2dda4)} else {ink(0x526169)},
        if on {ink(0x6d9b59)} else {ink(0x1b2a2f)},2.);
}
pub fn selector_on(c:&mut Canvas,r:Rect) {selector(c,r,true);}
pub fn selector_off(c:&mut Canvas,r:Rect) {selector(c,r,false);}

pub fn name(c:&mut Canvas,r:Rect,color:Option<Color>,hover:bool) {
    if crate::vintage_skin::draw(c,"SetBtn",usize::from(hover),r) {
        if let Some(tint)=color {c.fill_rect(r.inset(3.),tint.with_alpha(64));}
        return;
    }
    // The installed FL4 SetBtn plate is plain gray with a rolled top edge.
    let face=color.map_or(ink(0x808080),|color|color.mix(ink(0x808080),0.25));
    let face=if hover {face.shade(0.07)} else {face};
    c.fill_rrect(Rect::new(r.x,r.y+1.,r.w,r.h-1.),2.,ink(0x151b20));
    c.fill_rect_gradient(r.inset(1.),ink(0x979fa3),ink(0x192126),1.4);
    let plate=Rect::new(r.x+3.,r.y+3.,r.w-6.,r.h-7.);
    c.fill_rect_gradient(plate,face.shade(0.22),face.shade(-0.09),0.8);
    c.fill_rect(Rect::new(plate.x,plate.y,plate.w,1.),face.shade(0.28));
    c.fill_rect(Rect::new(plate.x,plate.bottom()-1.,plate.w,1.),face.shade(-0.30));
}

pub fn header_middle(c:&mut Canvas,r:Rect) {
    if crate::vintage_skin::draw(c,"RackMiddle",0,r) {return;}
    c.fill_rect_gradient(r,ink(0x454e53),ink(0x3e474c),0.);
    c.fill_rect(Rect::new(r.x,r.y,r.w,1.),ink(0x29343a));
    c.fill_rect(Rect::new(r.x,r.y+1.,r.w,1.),ink(0x89979c));
    c.fill_rect(Rect::new(r.x,r.bottom()-3.,r.w,1.),ink(0x748287));
    c.fill_rect(Rect::new(r.x,r.bottom()-2.,r.w,2.),ink(0x202c32));
}
pub fn header_left(c:&mut Canvas,r:Rect) {
    let (x,y)=(r.x,r.y);
    let points=[(x+5.,y+2.),(r.right()-3.,y+2.),(r.right()-32.,r.bottom()-3.),(x+5.,r.bottom()-3.)];
    c.fill_polygon(&points,ink(0x728086));
    c.polyline(&[(x+6.,y+3.),(r.right()-5.,y+3.)],1.,ink(0xa0acb0));
    c.line(r.right()-3.,y+2.,r.right()-32.,r.bottom()-3.,1.3,ink(0x1b2c33));
    // Options: paired horizontal arrows.
    c.line(x+13.,y+16.,x+29.,y+16.,1.3,ink(0xc5d0d0));
    c.polyline(&[(x+17.,y+12.),(x+13.,y+16.),(x+17.,y+20.)],1.3,ink(0xc5d0d0));
    c.polyline(&[(x+25.,y+12.),(x+29.,y+16.),(x+25.,y+20.)],1.3,ink(0xc5d0d0));
    let length=Rect::new(x+34.,y+3.,24.,24.);
    c.fill_rect(length,ink(0x20363b));
    c.fill_rect_gradient(length.inset(2.),ink(0x91b0b0),ink(0x6d9192),0.);
    c.fill_rect(Rect::new(x+39.,y+14.,5.,2.),ink(0x29494b));
    c.fill_rect(Rect::new(x+48.,y+14.,5.,2.),ink(0x29494b));
    c.fill_polygon(&[(x+74.,y+13.),(x+78.,y+13.),(x+83.,y+9.),(x+83.,y+23.),(x+78.,y+19.),(x+74.,y+19.)],ink(0xc9d1ce));
    c.polyline(&[(x+86.,y+12.),(x+88.,y+15.),(x+88.,y+18.),(x+86.,y+21.)],1.5,ink(0xc9d1ce));
    c.fill_rrect(Rect::new(x+94.,y+8.,14.,16.),2.,ink(0x253034));
    c.fill_rect_gradient(Rect::new(x+97.,y+11.,8.,10.),ink(0x967d65),ink(0x463c35),1.);
}
pub fn header_swing(c:&mut Canvas,r:Rect,value:f32) {
    c.text(Font::vintage(9),r.x+1.,r.y+16.,ink(0xaebdbd),"SWING");
    for i in 0..12 {
        let x=r.x+37.+i as f32*4.;
        c.fill_rect(Rect::new(x,r.y+5.,1.,if i%3==0 {10.} else {7.}),ink(0x293b42));
        c.fill_rect(Rect::new(x+1.,r.y+5.,1.,if i%3==0 {10.} else {7.}),ink(0x6b7b80));
    }
    c.fill_rect(Rect::new(r.x+35.,r.y+20.,54.,3.),ink(0x1d2d34));
    c.fill_rect(Rect::new(r.x+35.,r.y+23.,54.,1.),ink(0x7b8a8d));
    let handle=Rect::new(r.x+35.+value.clamp(0.,1.)*46.,r.y+15.,8.,10.);
    c.fill_rect_gradient(handle,ink(0x7f8b8f),ink(0x29383e),1.);
    c.fill_rect(Rect::new(handle.x+1.,handle.y+4.,6.,2.),ink(0xa97771));
}
fn display(c:&mut Canvas,r:Rect) {
    c.fill_rect(r.inset(1.),ink(0x26383b));
    c.fill_rect(Rect::new(r.x+2.,r.y+2.,r.w-4.,1.),ink(0x9bafad));
    c.fill_rect_gradient(Rect::new(r.x+3.,r.y+8.,r.w-6.,r.h-11.),ink(0x1c353a),ink(0x2f4c50),0.);
    c.fill_rect(Rect::new(r.x+3.,r.y+6.,r.w-6.,1.),ink(0x607b7c));
    c.fill_rect(Rect::new(r.x+3.,r.bottom()-3.,r.w-6.,1.),ink(0x6a8c87));
}
pub fn header_graph(c:&mut Canvas,r:Rect) {
    display(c,r);
    for (i,h) in [13.,7.,16.,11.,18.].into_iter().enumerate() {
        c.fill_rect(Rect::new(r.x+7.+i as f32*4.,r.bottom()-6.-h,2.,h),ink(0xb7d3c8));
    }
}
pub fn header_roll(c:&mut Canvas,r:Rect) {
    display(c,r);
    for i in 0..5 {
        let x=r.x+6.+i as f32*4.;
        c.fill_rect(Rect::new(x,r.y+12.,3.,13.),ink(0xb7d3c8));
        if i!=2 {c.fill_rect(Rect::new(x+2.,r.y+12.,2.,7.),ink(0x233a3e));}
    }
}
pub fn header_end(_c:&mut Canvas,_r:Rect) {}
pub fn frame_left(c:&mut Canvas,r:Rect) {
    for (dx,w,color) in [(0.,1.,0x202b30),(1.,2.,0x8c999d),(3.,2.,0x35444c),(5.,1.,0x9aa6aa),(6.,2.,0x56656c)] {
        c.fill_rect(Rect::new(r.x+dx,r.y,w,r.h),ink(color));
    }
}
pub fn frame_right(c:&mut Canvas,r:Rect) {
    for (dx,w,color) in [(0.,1.,0x87969c),(1.,3.,0x4a5961),(4.,2.,0x243139)] {
        c.fill_rect(Rect::new(r.x+dx,r.y,w,r.h),ink(color));
    }
}
