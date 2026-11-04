// SPDX-License-Identifier: GPL-3.0-or-later
//! One design definition for offline preview, development app and final builds.
//! All lookups happen on the UI thread, never in audio processing.
use os_ui::{Color, Rect};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Design {
    pub colors: BTreeMap<String, String>,
    pub radius_scale: f32,
    pub bevel: f32,
    pub shadow_depth: f32,
    pub font_offset: i16,
    pub knobs: BTreeMap<String, [f32; 2]>,
    pub selectors: BTreeMap<String, [f32; 4]>,
}
impl Default for Design {
    fn default() -> Self {
        Self {colors:Default::default(),radius_scale:1.,bevel:0.1,shadow_depth:3.,font_offset:0,knobs:Default::default(),selectors:Default::default()}
    }
}
fn hex(s:&str)->Result<u32,String> {
    let s=s.trim_start_matches('#');
    if s.len()!=6 {return Err("Colors must contain six hex digits".into())}
    u32::from_str_radix(s,16).map_err(|_|"Invalid hex color".into())
}
impl Design {
    pub fn parse(s:&str)->Result<Self,String> {
        if s.len()>65536 {return Err("Design file is too large".into())}
        let v:Self=serde_json::from_str(s).map_err(|e|e.to_string())?;
        for (a,b) in &v.colors {hex(a)?;hex(b)?;}
        if !v.radius_scale.is_finite() || !(0.0..=2.5).contains(&v.radius_scale) || !v.bevel.is_finite() || !(0.0..=0.4).contains(&v.bevel) || !v.shadow_depth.is_finite() || !(0.0..=12.).contains(&v.shadow_depth) || !(-3..=4).contains(&v.font_offset) {return Err("Design value outside supported range".into())}
        for (id,p) in &v.knobs {
            if !crate::controls::SETTINGS.iter().any(|s|s.id==id) || p.iter().any(|x|!x.is_finite() || x.abs()>300.) {return Err(format!("Invalid knob offset: {id}"))}
        }
        for (id,r) in &v.selectors {
            if !crate::controls::SETTINGS.iter().any(|s|s.id==id) || r.iter().any(|x|!x.is_finite()) || r[0]<0. || r[1]<0. || r[2]<20. || r[3]<16. || r[0]+r[2]>1180. || r[1]+r[3]>820. {return Err(format!("Invalid selector rectangle: {id}"))}
        }
        Ok(v)
    }
}
struct Active { design: Design, colors:BTreeMap<u32,u32> }
impl Active {
    fn new(design:Design)->Self {
        let colors=design.colors.iter().map(|(a,b)|(hex(a).unwrap(),hex(b).unwrap())).collect();
        Self{design,colors}
    }
}
thread_local! {static ACTIVE: RefCell<Active> = RefCell::new(Active::new(Design::parse(include_str!("../design/live.json")).expect("valid embedded Wave design")));}
pub fn apply(design:Design) { ACTIVE.with(|d|*d.borrow_mut()=Active::new(design)); }
pub fn color(original:u32)->Color { ACTIVE.with(|d|Color::rgb(*d.borrow().colors.get(&original).unwrap_or(&original))) }
pub fn radius()->f32 {ACTIVE.with(|d|d.borrow().design.radius_scale)}
pub fn bevel()->f32 {ACTIVE.with(|d|d.borrow().design.bevel)}
pub fn shadow()->f32 {ACTIVE.with(|d|d.borrow().design.shadow_depth)}
pub fn font_size(size:u16)->u16 {ACTIVE.with(|d|(size as i32+d.borrow().design.font_offset as i32).clamp(7,48) as u16)}
pub fn knob(id:&str,x:f32,y:f32)->(f32,f32) {ACTIVE.with(|d|{let d=d.borrow();let a=d.design.knobs.get(id).copied().unwrap_or([0.,0.]);(x+a[0],y+a[1])})}
pub fn selector(id:&str,r:Rect)->Rect {ACTIVE.with(|d|d.borrow().design.selectors.get(id).map(|a|Rect::new(a[0],a[1],a[2],a[3])).unwrap_or(r))}
/// Development-only file refresh, throttled and last-good on incomplete edits.
/// Release builds never inspect an environment variable or external design file.
pub fn refresh() {
    #[cfg(feature="live-design")]
    {
        use std::time::{Instant,Duration};
        thread_local! {static WATCH:RefCell<(Option<Instant>,String)>=RefCell::new((None,String::new()));}
        WATCH.with(|watch|{
            let mut watch=watch.borrow_mut();
            if watch.0.is_some_and(|t|t.elapsed()<Duration::from_millis(300)){return}
            watch.0=Some(Instant::now());
            let Ok(path)=std::env::var("LEMON_WAVE_DESIGN") else {return};
            let Ok(text)=std::fs::read_to_string(path) else {return};
            if watch.1==text{return}
            if let Ok(design)=Design::parse(&text){apply(design);watch.1=text;}
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn design_defaults_validation_and_control_hit_offsets() {
        let original=color(0x25303a);
        let d=Design::parse(r##"{"colors":{"25303a":"#112233"},"knobs":{"cutoff":[12,-8]}}"##).unwrap();
        apply(d);assert_eq!(color(0x25303a).r,0x11);assert_eq!(knob("cutoff",100.,100.),(112.,92.));
        assert!(Design::parse(r#"{"bevel":5}"#).is_err());
        assert!(Design::parse(r##"{"colors":{"broken":"#112233"}}"##).is_err());
        apply(Design::default());assert_eq!(color(0x25303a).r,original.r);
    }
}
