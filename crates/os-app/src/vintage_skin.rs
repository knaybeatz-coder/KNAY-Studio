// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional, user-installed FL4 artwork. No third-party artwork is distributed with Lemon.
//! Decode once on the UI thread; drawing never reads files or rebuilds sprite frames.
use std::{collections::HashMap, path::{Path, PathBuf}, sync::OnceLock};
use os_ui::{Canvas, Rect};

struct Sprite { w: usize, h: usize, pixels: Vec<u32> }
struct Alpha {w:usize,h:usize,pixels:Vec<u8>}
struct Skin { sprites: HashMap<String, Vec<Sprite>>, alpha: HashMap<String,Vec<Alpha>> }
static SKIN: OnceLock<Skin> = OnceLock::new();

pub(crate) fn directories() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = std::env::var_os("LEMON_VINTAGE_SKIN") { paths.push(p.into()); }
    if let Some(p) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(p).join("Lemon Studio/Skins/FL4"));
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(p) = std::env::var_os(var) {
            paths.push(PathBuf::from(p).join("FLStudio4/Artwork/Skins/Default"));
        }
    }
    paths
}

impl Sprite {
    fn round_face(self, cx: f32, cy: f32, radius: f32, feather: f32) -> Alpha {
        let pixels = self.pixels.into_iter().enumerate().flat_map(|(i, rgb)| {
            let dx = (i % self.w) as f32 - cx;
            let dy = (i / self.w) as f32 - cy;
            let alpha = ((radius - dx.hypot(dy)) / feather).clamp(0.0, 1.0);
            [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, (alpha * 255.0).round() as u8]
        }).collect();
        Alpha { w: self.w, h: self.h, pixels }
    }

    fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Option<Self> {
        if x.checked_add(w)? > self.w || y.checked_add(h)? > self.h { return None; }
        let mut pixels = Vec::with_capacity(w*h);
        for row in y..y+h { pixels.extend_from_slice(&self.pixels[row*self.w+x..row*self.w+x+w]); }
        Some(Self { w, h, pixels })
    }
}

// FL4 skins use uncompressed indexed (4/8-bit) and RGB Windows BMPs.
// All header/palette/stride reads are checked; malformed optional art falls back to vectors.
fn bmp(b: &[u8]) -> Option<Sprite> {
    let u16at = |p| Some(u16::from_le_bytes(b.get(p..p+2)?.try_into().ok()?));
    let u32at = |p| Some(u32::from_le_bytes(b.get(p..p+4)?.try_into().ok()?));
    if b.get(..2)? != b"BM" || u32at(14)? < 40 || u16at(26)? != 1 || u32at(30)? != 0 { return None; }
    let offset = u32at(10)? as usize;
    let wi = u32at(18)? as i32;
    let hi = u32at(22)? as i32;
    if wi <= 0 || hi == 0 || hi == i32::MIN { return None; }
    let (w,h) = (wi as usize, hi.unsigned_abs() as usize);
    if w > 8192 || h > 8192 || w.checked_mul(h)? > 4_000_000 { return None; }
    let bits = u16at(28)? as usize;
    if ![4,8,24,32].contains(&bits) { return None; }
    let stride = (w*bits).div_ceil(32)*4;
    b.get(offset..offset.checked_add(stride*h)?)?;
    let palette = 14usize.checked_add(u32at(14)? as usize)?;
    if palette > offset { return None; }
    let count = if bits <= 8 { let n=u32at(46)? as usize; if n==0 {1<<bits}else{n} } else {0};
    if count > 256 || palette.checked_add(count*4)? > offset { return None; }
    let mut pixels = Vec::with_capacity(w*h);
    for y in 0..h {
        let row = offset + if hi > 0 { h-1-y } else { y } * stride;
        for x in 0..w {
            let p = match bits {
                4 | 8 => {
                    let v = b[row+x*bits/8];
                    let i = if bits==8 {v} else if x%2==0 {v>>4} else {v&15} as usize;
                    if i >= count { return None; }
                    palette+i*4
                }
                _ => row+x*(bits/8),
            };
            pixels.push(u32::from(b[p+2])<<16 | u32::from(b[p+1])<<8 | u32::from(b[p]));
        }
    }
    Some(Sprite {w,h,pixels})
}

impl Skin {
    fn load(dir: &Path) -> Self {
        let mut skin = Self { sprites: HashMap::new(), alpha:HashMap::new() };
        for (name,fw,fh) in [
            ("SSBtns",16,28),("SetBtn",64,28),("SelectBtn",17,28),
            ("MuteBtn",8,8),("Wheel_18",18,18),("Wheel_19",19,19),("Wheel_27",27,27),
            ("SSFXBtns",25,22),("SSTop",409,22),("SSBottom",313,22),("SSPRView",129,28),
            ("TransportBtns",28,22),("DiskBtns",18,15),("SCBtns",29,25),("MinClose",14,15),
            ("TimeDigits_24",18,24),("TimeDigits_16",12,16),("TempoDigits_9",6,9),
            ("ControlToolBar",376,80),("TransportToolBar",197,40),("RecordToolBar",202,40),
            ("CPUToolBar",107,40),("MonitorToolBar",145,40),("PeakMeter",162,11),
            ("PatternToolBar",113,40),("PatPad",48,30),("Digits_9",6,9),("TimeToolBar",183,40),("ShortcutToolBar",204,40),
            ("EETools",18,18),("MenuItems",284,20),("FXPeakMeter",10,19),("TimeArrow",15,9),("SBItem",16,13),
        ] {
            let path=dir.join(format!("{name}.bmp"));
            let Some(art)=std::fs::metadata(&path).ok().filter(|m|m.len()<=16_000_000)
                .and_then(|_|std::fs::read(&path).ok()).and_then(|b|bmp(&b)) else {continue};
            if art.w%fw!=0 || art.h!=fh {continue;}
            let frames=(0..art.w/fw).filter_map(|i|art.crop(i*fw,0,fw,fh)).collect();
            if name == "PatPad" {
                let keys = (0..18).filter_map(|i| {
                    let key = i % 9;
                    art.crop((i / 9) * 48 + key % 3 * 16, (2 - key / 3) * 10, 16, 10)
                }).collect();
                skin.sprites.insert("PatternKey".into(), keys);
            }
            if name == "MinClose" {
                let faces = (0..art.w / fw).filter_map(|i| art.crop(i * fw, 2, fw - 1, 11)).collect();
                skin.sprites.insert("WindowButtonFaces".into(), faces);
            }
            if name == "Digits_9" {
                // Keep the lit segments and let the strip color show between them.
                let digits = (0..art.w / fw).filter_map(|i| art.crop(i * fw, 0, fw, fh))
                    .map(|cell| {
                        let pixels = cell.pixels.into_iter().flat_map(|rgb| {
                            let green = (rgb >> 8) as u8;
                            [0xf4, 0xce, 0x94, green.saturating_sub(47).saturating_mul(2)]
                        }).collect();
                        Alpha { w: fw, h: fh, pixels }
                    }).collect();
                skin.alpha.insert("MixerDigits".into(), digits);
            }
            if name == "Wheel_18" {
                // Remove the dithered backing that surrounds the original face.
                let wheels = (0..art.w / 18).filter_map(|i| art.crop(i * 18, 0, 18, 18))
                    .map(|cell| cell.round_face(8.0, 8.0, 8.9, 1.4)).collect();
                skin.alpha.insert("MixerWheel".into(), wheels);
            }
            if name == "MuteBtn" {
                let lamps = (0..art.w / 8).filter_map(|i| art.crop(i * 8, 0, 8, 8))
                    .map(|cell| cell.round_face(3.5, 3.5, 4.6, 1.0)).collect();
                skin.alpha.insert("MixerLamp".into(), lamps);
            }
            if name == "FXPeakMeter" {
                // The atlas pairs a dim well with a bright bar, not left/right channels.
                if let Some(idle) = art.crop(0, 0, 4, 19) {
                    skin.sprites.insert("FXMeterIdle".into(), vec![idle]);
                }
                if let Some(lit) = art.crop(5, 0, 4, 19) {
                    skin.sprites.insert("FXMeterColumn".into(), vec![lit]);
                }
            }
            if name == "TimeArrow" {
                let pixels = art.pixels.iter().flat_map(|&rgb| [
                    (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8,
                    if rgb == 0x00ff00 { 0 } else { 255 },
                ]).collect();
                skin.alpha.insert("TimeArrow".into(), vec![Alpha { w: 15, h: 9, pixels }]);
            }
            if name == "PeakMeter" {
                if let Some(lit) = art.crop(54, 0, 108, 11) {
                    skin.sprites.insert("PeakLit".into(), vec![lit]);
                }
            }
            if name == "RecordToolBar" {
                for (key, x, y, w, h) in [
                    ("RecordSwitches", 0, 0, 127, 40),
                    ("RecordRail", 126, 0, 1, 40),
                    ("RecordSocket", 9, 5, 14, 14),
                ] {
                    if let Some(part) = art.crop(x, y, w, h) {
                        skin.sprites.insert(key.into(), vec![part]);
                    }
                }
                for (key, x, y, w, h) in [
                    ("RecordMetronome", 24, 19, 22, 18),
                    ("RecordWait", 64, 19, 20, 18),
                    ("RecordCountdown", 64, 4, 21, 15),
                ] {
                    if let Some(glyph) = art.crop(x, y, w, h) {
                        skin.sprites.insert(key.into(), vec![glyph]);
                    }
                }
            }
            if name == "ControlToolBar" {
                if let Some(well) = art.crop(78, 6, 171, 15) {
                    skin.sprites.insert("CaptionGlass".into(), vec![well]);
                }
            }
            if name == "TransportBtns" {
                let leds = [28 + 17, 17].into_iter()
                    .filter_map(|x| art.crop(x, 0, 9, 9)).collect();
                skin.sprites.insert("RecordLed".into(), leds);
            }
            if name=="SSTop" {
                for (key,x,w) in [("RackLeft",0,90),("RackMiddle",100,1),("RackSwing",255,75),("RackEnd",382,27)] {
                    if let Some(part)=art.crop(x,0,w,22) {skin.sprites.insert(key.into(),vec![part]);}
                }
            }
            if name=="SSBottom" {
                for (key,x,w) in [("FooterLeft",0,133),("FooterMiddle",150,1),("FooterRight",311,2)] {
                    if let Some(part)=art.crop(x,0,w,22) {skin.sprites.insert(key.into(),vec![part]);}
                }
            }
            if name=="EETools" {
                let mut glyphs=Vec::new();
                for frame in 0..art.w/18 {
                    let part=art.crop(frame*18+2,2,14,14).unwrap();
                    let bg=part.pixels[0];let mut pixels=Vec::with_capacity(14*14*4);
                    for v in part.pixels {pixels.extend_from_slice(&[(v>>16) as u8,(v>>8) as u8,v as u8,if v==bg {0}else{255}]);}
                    glyphs.push(Alpha{w:14,h:14,pixels});
                }
                skin.alpha.insert("Tools".into(),glyphs);
            }
            if name=="MenuItems" {
                // Glyph bounds measured from the original menu strip. Recombine its alphabet
                // for Lemon-only labels without stretching letters or substituting a font.
                let cells=[(7,4),(12,2),(15,4),(20,4),(37,4),(42,5),(48,2),(51,4),
                    (70,5),(76,5),(82,5),(88,5),(94,5),(100,4),(105,4),(110,5),
                    (129,5),(135,2),(138,4),(143,8),(165,5),(171,5),(177,4),(182,2),
                    (185,5),(191,5),(197,5),(216,4),(221,5),(227,5),(233,4),(238,5),
                    (256,5),(262,4),(267,4),(272,5)];
                for (ch,(x,w)) in "FILEEDITCHANNELSVIEWOPTIONSTOOLSHELP".chars().zip(cells) {
                    if let Some(part)=art.crop(x,0,w+1,20) {skin.sprites.entry(ch.to_string()).or_insert(vec![part]);}
                }
                for (i,(x,w)) in [(0,31),(31,31),(62,60),(122,36),(158,51),(209,41),(250,34)].into_iter().enumerate() {
                    if let Some(part)=art.crop(x,0,w,20) {skin.sprites.insert(format!("Menu{i}"),vec![part]);}
                }
            }
            if name=="MenuItems" {
                // R is the only letter needed by Lemon's menu titles that FL4's strip lacks.
                // Build it from the native P bowl and a matching two-pixel diagonal leg.
                if let Some(p)=skin.sprites.get("P").and_then(|v|v.first()) {
                    let mut r=p.crop(0,0,p.w,p.h).unwrap();
                    for y in 10..17 {let x=2+(y-10)/3;for dx in 0..2 {
                        if x+dx<r.w {r.pixels[y*r.w+x+dx]=p.pixels[y*p.w];}
                    }}
                    skin.sprites.insert("R".into(),vec![r]);
                }
            }
            skin.sprites.insert(name.into(),frames);
            if name == "ShortcutToolBar" {
                let mut backing = art;
                if let Some(buttons) = skin.sprites.get("SCBtns").filter(|frames| frames.len() >= 10) {
                    for (slot, face) in buttons.iter().step_by(2).take(5).enumerate() {
                        for (row, pixels) in face.pixels.chunks_exact(29).enumerate() {
                            let x = (row + 6) * backing.w + 51 + slot * 29;
                            backing.pixels[x..x + 29].copy_from_slice(pixels);
                        }
                    }
                }
                if let Some(bank) = backing.crop(47, 0, 154, 40) {
                    skin.sprites.insert("ShortcutBank".into(), vec![bank]);
                }
                skin.sprites.insert("ShortcutToolBar".into(), vec![backing]);
            }
        }
        for (name,fw) in [("VSlider",18usize),("HSlider",21),("HSlider_Grey",20)] {
            let Some(b)=std::fs::read(dir.join(format!("{name}.tga"))).ok().filter(|b|b.len()<100_000) else {continue};
            if b.len()<18 || b[1]!=0 || b[2]!=2 || b[16]!=32 {continue;}
            let w=u16::from_le_bytes([b[12],b[13]]) as usize;
            let h=u16::from_le_bytes([b[14],b[15]]) as usize;
            let off=18+usize::from(b[0]);
            if w==0 || h==0 || w%fw!=0 || w>512 || h>512 || b.len()<off+w*h*4 {continue;}
            let frames=(0..w/fw).map(|frame| {
                let mut pixels=Vec::with_capacity(fw*h*4);
                for y in 0..h {for x in frame*fw..(frame+1)*fw {
                    let sy=if b[17]&32!=0 {y}else{h-1-y};let i=off+(sy*w+x)*4;
                    pixels.extend_from_slice(&[b[i+2],b[i+1],b[i],b[i+3]]);
                }}
                Alpha {w:fw,h,pixels}
            }).collect();
            skin.alpha.insert(name.into(),frames);
        }
        skin
    }
}

fn skin() -> &'static Skin {
    SKIN.get_or_init(|| directories().iter().map(|p|Skin::load(p))
        .find(|s|s.sprites.contains_key("SSBtns") && s.sprites.contains_key("SetBtn"))
        .unwrap_or(Skin {sprites:HashMap::new(),alpha:HashMap::new()}))
}
pub fn available() -> bool {skin().sprites.contains_key("SSBtns")}
pub fn has_frame(name: &str, frame: usize) -> bool {
    skin().sprites.get(name).is_some_and(|frames| frame < frames.len())
}

pub fn draw(c: &mut Canvas, name: &str, frame: usize, r: Rect) -> bool {
    let Some(s)=skin().sprites.get(name).and_then(|v|v.get(frame)) else {return false};
    c.image(r,s.w,s.h,&s.pixels);
    true
}
pub fn native(c: &mut Canvas, name: &str, frame: usize, x:f32, y:f32) -> bool {
    let Some(s)=skin().sprites.get(name).and_then(|v|v.get(frame)) else {return false};
    c.image(Rect::new(x,y,s.w as f32,s.h as f32),s.w,s.h,&s.pixels);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vintage_native_artwork_matches_installed_pixels() {
        let source=directories().into_iter().find(|p|p.join("SSBtns.bmp").is_file());
        let Some(source)=source else {return};
        let loaded=Skin::load(&source);
        for (name,fw,fh) in [("SSBtns",16,28),("SetBtn",64,28),("SelectBtn",17,28),("Wheel_18",18,18),("SCBtns",29,25)] {
            let art=bmp(&std::fs::read(source.join(format!("{name}.bmp"))).unwrap()).unwrap();
            let frames=loaded.sprites.get(name).unwrap();
            for (i,sprite) in frames.iter().enumerate() {
                let original=art.crop(i*fw,0,fw,fh).unwrap();
                let mut canvas=Canvas::new(fw as u32,fh as u32,1.).unwrap();
                canvas.image(Rect::new(0.,0.,fw as f32,fh as f32),sprite.w,sprite.h,&sprite.pixels);
                for (actual,expected) in canvas.pixmap.data().chunks_exact(4).zip(original.pixels) {
                    assert_eq!(actual,[(expected>>16) as u8,(expected>>8) as u8,expected as u8,255],"{name} frame {i}");
                }
            }
        }
        for name in ["MixerWheel", "MixerLamp"] {
            if let Some(frames) = loaded.alpha.get(name) {
                for frame in frames {
                    assert_eq!(frame.pixels[3], 0, "{name} corner is transparent");
                    assert_eq!(frame.pixels[(frame.h / 2 * frame.w + frame.w / 2) * 4 + 3], 255);
                }
            }
        }
        assert_eq!(loaded.alpha["VSlider"].len(),2);
        assert!(loaded.sprites.contains_key("R"));
    }
    #[test]
    fn vintage_artwork_rejects_invalid_bmp_and_copies_native_pixels() {
        for n in 0..100 {assert!(bmp(&vec![0;n]).is_none());}
        // 2x1 24-bit bitmap, padded row. Test without depending on a local FL installation.
        let mut b=vec![0;62]; b[..2].copy_from_slice(b"BM");
        for (p,v) in [(10,54u32),(14,40),(18,2),(22,1)] {b[p..p+4].copy_from_slice(&v.to_le_bytes());}
        b[26]=1;b[28]=24;b[54..60].copy_from_slice(&[3,2,1,6,5,4]);
        let art=bmp(&b).unwrap();assert_eq!(art.pixels,[0x010203,0x040506]);
        let mut c=Canvas::new(2,1,1.).unwrap();c.image(Rect::new(0.,0.,2.,1.),art.w,art.h,&art.pixels);
        assert_eq!(c.pixmap.data(),[1,2,3,255,4,5,6,255]);
        b[54..].fill(255);b[28]=4;assert!(bmp(&b).is_none());
        b[28]=24;b[18..22].copy_from_slice(&u32::MAX.to_le_bytes());assert!(bmp(&b).is_none());
    }
}

/// The original menu alphabet, also used to construct ADD and PATTERNS.
pub fn lettering_width(text:&str)->f32 {
    text.as_bytes().iter().map(|b| {
        let a=[b.to_ascii_uppercase()];
        std::str::from_utf8(&a).ok().and_then(|k|skin().sprites.get(k)).map_or(5.,|v|v[0].w as f32)
    }).sum()
}
pub fn lettering(c:&mut Canvas,text:&str,mut x:f32,y:f32) {
    for b in text.bytes() {
        let a=[b.to_ascii_uppercase()];
        if let Ok(k)=std::str::from_utf8(&a) {
            if let Some(s)=skin().sprites.get(k).and_then(|v|v.first()) {
                c.image(Rect::new(x,y,s.w as f32,s.h as f32),s.w,s.h,&s.pixels);x+=s.w as f32;continue;
            }
        }
        x+=5.;
    }
}

pub fn alpha_draw(c: &mut Canvas, name: &str, frame: usize, r: Rect) -> bool {
    let Some(sprite) = skin().alpha.get(name).and_then(|frames| frames.get(frame)) else { return false; };
    c.image_rgba(r, sprite.w, sprite.h, &sprite.pixels);
    true
}

pub fn alpha_native(c:&mut Canvas,name:&str,frame:usize,x:f32,y:f32)->bool {
    let Some(s)=skin().alpha.get(name).and_then(|v|v.get(frame)) else {return false};
    c.image_rgba(Rect::new(x,y,s.w as f32,s.h as f32),s.w,s.h,&s.pixels);true
}

/// Use the original event-editor symbols wherever their meaning matches Lemon's tools.
/// Lemon-only symbols continue through the coordinated vector pack.
pub fn icon(c:&mut Canvas,icon:crate::icons::Icon,r:Rect)->bool {
    use crate::icons::Icon;
    let frame=match icon {
        Icon::Delete=>2,Icon::Mute=>4,Icon::Slide=>6,Icon::Magnet=>10,
        Icon::PianoRoll|Icon::TypingKeyboard=>18,Icon::Wave=>20,Icon::ArrowUp=>22,
        Icon::Undo|Icon::Back=>24,Icon::Brush=>26,Icon::ArrowLeft=>28,Icon::ArrowRight=>30,
        Icon::Select=>32,Icon::ChannelRack=>38,Icon::Slice=>40,_=>return false,
    };
    let Some(s)=skin().alpha.get("Tools").and_then(|v|v.get(frame)) else {return false};
    c.image_rgba(r,s.w,s.h,&s.pixels);true
}
