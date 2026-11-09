// SPDX-License-Identifier: GPL-3.0-or-later
//! Cached, directionally lit console materials. Lighting stays fixed when a control turns.
use std::sync::OnceLock;
use std::f32::consts::PI;
const SIZE: usize = 96;

pub(super) fn knob() -> &'static [u8] {
    static MATERIAL: OnceLock<Vec<u8>> = OnceLock::new();
    MATERIAL.get_or_init(|| {
        let mut p=vec![0;SIZE*SIZE*4];
        for y in 0..SIZE {for x in 0..SIZE {
            let nx=(x as f32+0.5-48.)/39.;let ny=(y as f32+0.5-46.)/39.;
            let r=nx.hypot(ny);let a=ny.atan2(nx);
            let shadow=(nx*nx+((ny-0.12)*0.93).powi(2)).sqrt();
            let (v,alpha)=if r>1. {
                (3.,((1.16-shadow)/0.16).clamp(0.,1.).powi(2)*155.)
            }else if r>0.91 {
                // Two long studio-light reflections on a narrow turned aluminium bevel.
                let reflection=((a+2.25).cos().max(0.)).powi(7)*165.
                    +((a-0.70).cos().max(0.)).powi(12)*90.;
                let machining=(r*1100.).sin()*3.;
                (40.+reflection+machining,255.)
            }else if r>0.84 {
                (11.+(a+2.2).cos().max(0.)*38.+(a*56.).cos()*3.,255.)
            }else {
                // Slightly domed black anodized cap, lit from upper left.
                let z=(1.-(r/0.92).powi(2)).max(0.).sqrt();
                let light=(-nx*0.40-ny*0.55+z*0.73).max(0.);
                let softbox=(-((nx+0.34).powi(2)/0.24+(ny+0.45).powi(2)/0.08)).exp();
                let grain=((x*73+y*151)%23) as f32/23.-0.5;
                let turned=(r*360.).sin()*1.5;
                let reflection=(-((nx+ny*0.40+0.10)/0.14).powi(2)).exp()*19.;
                let anisotropic=(a*2.+0.8).cos()*7.;
                (18.+light*28.+softbox*27.+reflection+anisotropic+turned+grain*2.2,255.)
            };
            let i=(y*SIZE+x)*4;
            p[i]=v.clamp(0.,255.) as u8;p[i+1]=(v*1.035).clamp(0.,255.) as u8;
            p[i+2]=(v*1.065).clamp(0.,255.) as u8;p[i+3]=alpha.clamp(0.,255.) as u8;
        }}p
    })
}

pub(super) fn fader() -> &'static [u8] {
    static MATERIAL: OnceLock<Vec<u8>> = OnceLock::new();
    MATERIAL.get_or_init(|| {
        let mut p=vec![0;SIZE*SIZE*4];
        for y in 0..SIZE {for x in 0..SIZE {
            let nx=(x as f32+0.5-48.)/36.;let ny=(y as f32+0.5-45.)/39.;
            let edge=nx.abs().max(ny.abs());
            let (v,alpha)=if edge>1. {
                (2.,((1.13-((nx-0.03).abs().max((ny-0.07).abs())))/0.13).clamp(0.,1.)*170.)
            } else {
                let rim=((1.-edge)/0.07).clamp(0.,1.);
                let broad=0.5+0.5*(ny*PI*2.5-0.3).cos();
                let streak=((-((ny+0.77)/0.06).powi(2)).exp()*55.)
                    +((-((ny-0.13)/0.08).powi(2)).exp()*35.);
                let brush=((y*157+x*13)%31) as f32/31.-0.5;
                let metal=92.+broad*108.+streak-nx*12.+brush*7.;
                let v=if ny.abs()<0.065 {12.}else{metal*rim+25.*(1.-rim)};
                (v,255.)
            };
            let i=(y*SIZE+x)*4;
            p[i]=v.clamp(0.,255.) as u8;p[i+1]=(v*1.018).clamp(0.,255.) as u8;
            p[i+2]=(v*1.035).clamp(0.,255.) as u8;p[i+3]=alpha.clamp(0.,255.) as u8;
        }}p
    })
}

pub(super) const DIM: usize = SIZE;


/// Fine brushed graphite with a broad reflected studio light and darker machined edges.
/// Generated once, then filtered by the existing canvas at the current DPI.
pub(super) fn panel() -> &'static [u32] {
    static MATERIAL:OnceLock<Vec<u32>>=OnceLock::new();
    MATERIAL.get_or_init(||(0..128*512).map(|i| {
        let x=(i%128) as f32/127.;let y=(i/128) as f32/511.;
        let grain=((i/128*73)%31) as f32/31.-0.5;
        let fleck=((i*173+37)%43) as f32/43.-0.5;
        let light=(-((x-0.26)/0.42).powi(2)).exp()*13.;
        let bevel=if x<0.025 {20.*(1.-x/0.025)}else if x>0.975 {-12.*((x-0.975)/0.025)}else{0.};
        let v=24.+light-y*4.+grain*2.5+fleck*1.5+bevel;
        let r=v.clamp(0.,255.) as u32;let g=(v*1.045).clamp(0.,255.) as u32;let b=(v*1.08).clamp(0.,255.) as u32;
        (r<<16)|(g<<8)|b
    }).collect())
}

pub(super) fn vu_face() -> &'static [u32] {
    static MATERIAL:OnceLock<Vec<u32>>=OnceLock::new();
    MATERIAL.get_or_init(||(0..128*96).map(|i| {
        let x=(i%128) as f32/127.;let y=(i/128) as f32/95.;
        // A broad reflection across satin silver with fine horizontal machining lines.
        let highlight = (-((x + 0.24*y - 0.54)/0.30).powi(2)).exp();
        let edge = (-y*40.).exp()*25. + (-(1.-y)*50.).exp()*12.;
        let grain = ((i/128*31)%17) as f32/17. - 0.5;
        let light = 179. + 41.*highlight - edge + grain*5.;
        let r = light as u32;
        let g = (light + 4.) as u32;
        let b = (light + 8.) as u32;
        (r<<16)|(g<<8)|b
    }).collect())
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Finish { Graphite, Silver, CreamGlass }

/// Warm printed stock behind a convex lens. The bowed highlight hugs the upper
/// left rim, leaving the live needle and calibrated scale clear in the center.
fn cream_glass() -> &'static [u32] {
    static FACE: OnceLock<Vec<u32>> = OnceLock::new();
    FACE.get_or_init(|| (0..128*96).map(|i| {
        let x=(i%128) as f32/127.;
        let y=(i/128) as f32/95.;
        let rim=x.min(1.-x).min(y.min(1.-y));
        let shade = 0.91 + 0.10*(1.-y) - 0.27*(-rim*38.).exp()
            - 0.18*(-y*17.).exp() - 0.055*(2.*x-1.).powi(2);
        let bent = y - (0.030 + 0.23*(1.-x).powi(4));
        let glint = (-(bent/0.055).powi(2)).exp() * (-((x-0.20)/0.28).powi(2)).exp();
        let soft = (-((x-0.24)/0.34).powi(2)-((y-0.16)/0.12).powi(2)).exp();
        let glass = (glint*0.78 + soft*0.34).min(0.91);
        let ch=|paper:f32| ((paper*shade)*(1.-glass)+255.*glass).clamp(0.,255.) as u32;
        (ch(247.)<<16)|(ch(220.)<<8)|ch(165.)
    }).collect())
}

/// Cache the surface at device resolution so a repaint only copies pixels.
/// The strip height changes on resize, never on the audio thread.
pub(super) fn surface(ui:&mut crate::app::Ui,r:os_ui::Rect,finish:Finish) {
    use std::sync::{Arc,Mutex};
    type Entry=((usize,usize,Finish),Arc<Vec<u32>>);
    static CACHE:OnceLock<Mutex<Vec<Entry>>>=OnceLock::new();
    let k=ui.c.scale;
    let x=(r.x*k).round();let y=(r.y*k).round();
    let w=((r.right()*k).round()-x).max(1.) as usize;
    let h=((r.bottom()*k).round()-y).max(1.) as usize;
    let key=(w,h,finish);
    let mut cache=CACHE.get_or_init(Default::default).lock().unwrap();
    let pixels=if let Some((_,p))=cache.iter().find(|(size,_)|*size==key) {p.clone()}else{
        let (height,source)=match finish {
            Finish::Graphite => (512,panel()),
            Finish::Silver => (96,vu_face()),
            Finish::CreamGlass => (96,cream_glass()),
        };
        let p=Arc::new(os_ui::resample(128,height,source,w,h));
        if cache.len()>=16 {cache.remove(0);}
        cache.push((key,p.clone()));p
    };
    drop(cache);
    ui.c.image(os_ui::Rect::new(x/k,y/k,w as f32/k,h as f32/k),w,h,&pixels);
}
