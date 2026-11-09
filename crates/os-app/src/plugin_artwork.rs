// SPDX-License-Identifier: GPL-3.0-or-later
//! Cached real editor previews; never open a hosted plugin to paint the picker.
use std::{collections::HashMap,sync::{Arc,OnceLock},path::{Path,PathBuf},io::Read};
use crate::{app::{App,Ui},plugin_db::Entry};
use os_ui::{Canvas,Rect};
pub type Image=(usize,usize,Vec<u32>);
pub type Images=HashMap<String,Arc<Image>>;
fn key(name:&str)->String{crate::plugin_menu::normalized(name)}
fn native()->&'static Images{
    static DATA:OnceLock<Images>=OnceLock::new();
    DATA.get_or_init(||[
        (crate::master_assistant::NAME,include_bytes!("../assets/plugin-picker/master-assistant.png") as &[u8]),
        ("Lemon Wave",include_bytes!("../assets/plugin-picker/lemon-wave.png") as &[u8]),
        ("Lemon Chop", include_bytes!("../assets/plugin-picker/chop.png") as &[u8]),
        ("Lemon Sampler", include_bytes!("../assets/plugin-picker/sampler.png") as &[u8]),
        ("Lemon Trio", include_bytes!("../assets/plugin-picker/trio.png") as &[u8]),
        ("Citrus", include_bytes!("../assets/plugin-picker/citrus_default.png") as &[u8]),
        ("Lemon Parametric EQ", include_bytes!("../assets/plugin-picker/eq.png") as &[u8]),
        ("Lemon Limiter", include_bytes!("../assets/plugin-picker/limiter_0.png") as &[u8]),
        ("Lemon Reverb", include_bytes!("../assets/plugin-picker/reverb_default.png") as &[u8]),
        ("Lemon Delay", include_bytes!("../assets/plugin-picker/delay_default.png") as &[u8]),
        ("Lemon Balance", include_bytes!("../assets/plugin-picker/balance_default.png") as &[u8]),
        ("Lemon Chorus", include_bytes!("../assets/plugin-picker/chorus_default.png") as &[u8]),
        ("Lemon Fast Distortion", include_bytes!("../assets/plugin-picker/fast_distortion_default.png") as &[u8]),
        ("Lemon Soft Clipper", include_bytes!("../assets/plugin-picker/soft_clipper_default.png") as &[u8]),
        ("Lemon Stereo Enhancer", include_bytes!("../assets/plugin-picker/stereo_enhancer_default.png") as &[u8]),
        ("Lemon Stereo Shaper", include_bytes!("../assets/plugin-picker/stereo_shaper_default.png") as &[u8]),
        ("Lemon Transient Processor", include_bytes!("../assets/plugin-picker/transient_processor_default.png") as &[u8]),
        ("Press", include_bytes!("../assets/plugin-picker/press_default.png") as &[u8]),
        ("Pulp Filter", include_bytes!("../assets/plugin-picker/pulp_default.png") as &[u8]),
        ("Tesla", include_bytes!("../assets/plugin-picker/tesla_empty.png") as &[u8]),
    ].into_iter().filter_map(|(name,bytes)|Canvas::decode_png(bytes).map(|p|(key(name),Arc::new(p)))).collect())
}
pub(crate) fn custom_path(uid:&str)->Option<PathBuf>{
    use sha2::{Sha256,Digest};
    os_paths::user_file("Plugin pictures").map(|p|p.join(format!("{:x}.png",Sha256::digest(uid.as_bytes()))))
}
pub(crate) fn read(path:&Path)->Option<Image>{
    let mut bytes=Vec::new();std::fs::File::open(path).ok()?.take(8*1024*1024+1).read_to_end(&mut bytes).ok()?;
    if bytes.len()>8*1024*1024{return None;}
    crate::wallpaper::decode(&bytes)
}
pub fn load(entries:impl Iterator<Item=Entry>)->Images{
    entries.filter_map(|e|{
        let custom=if cfg!(test){None}else{custom_path(&e.uid).and_then(|p|read(&p)).map(Arc::new)};
        let picture=custom.or_else(||native().get(&key(&e.name)).cloned()).or_else(||crate::plugin_downloads::picker_picture(&e.name));
        picture.map(|p|(e.uid,p))
    }).collect()
}
pub fn draw(ui:&mut Ui,r:Rect,p:&Image){
    let ratio=(r.w/p.0 as f32).min(r.h/p.1 as f32);
    let (w,h)=(p.0 as f32*ratio,p.1 as f32*ratio);
    ui.c.image(Rect::new(r.cx()-w/2.,r.cy()-h/2.,w,h),p.0,p.1,&p.2);
}
pub fn choose(app:&mut App,uid:String){
    crate::plugin_picker::close(app);
    let dialog=crate::filedlg::FileDlg::new(crate::filedlg::Mode::Open,"Choose plugin picture","plugin-picture",crate::dialog::Then::PluginPicture(uid)).filter("Picture",&["png","jpg","jpeg"]);
    app.file_dialog(dialog);
}
pub fn chosen(app:&mut App,uid:String,path:Option<PathBuf>){
    if let Some(path)=path{
        let result=custom_path(&uid).ok_or_else(||"Cannot find profile folder".to_string()).and_then(|dest|save_picture(&path,&dest));
        if let Err(e)=result{app.state.message=Some(e);}
    }
    let target=app.state.plugin_picker.target;crate::plugin_picker::open(app,target);
}

pub(crate) fn save_picture(path:&Path,dest:&Path)->Result<(),String>{
            let (w,h,p)=read(&path).ok_or("Choose a PNG or JPEG picture smaller than 8 MB")?;
            let ratio=(480./w as f32).min(300./h as f32).min(1.);
            let (w2,h2)=((w as f32*ratio).round().max(1.) as u32,(h as f32*ratio).round().max(1.) as u32);
            let mut c=Canvas::new(w2,h2,1.)?;
            c.begin_frame(None);c.image(Rect::new(0.,0.,w2 as f32,h2 as f32),w,h,&p);
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e|e.to_string())?;
            let pending=dest.with_extension("pending.png");c.pixmap.save_png(&pending).map_err(|e|e.to_string())?;
            std::fs::rename(pending,dest).map_err(|e|e.to_string())?;Ok(())

}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn plugin_picture_is_bounded_persistent_and_replaceable(){
  let root=std::env::temp_dir().join(format!("lemon-plugin-picture-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
  let source=root.join("source.png");std::fs::write(&source,include_bytes!("../assets/plugin-picker/citrus_default.png")).unwrap();
  let dest=root.join("saved.png");save_picture(&source,&dest).unwrap();let picture=read(&dest).unwrap();assert!(picture.0<=480&&picture.1<=300);
  let first=std::fs::read(&dest).unwrap();std::fs::write(&source,b"not a picture").unwrap();assert!(save_picture(&source,&dest).is_err());assert_eq!(first,std::fs::read(&dest).unwrap());
  std::fs::write(&source,include_bytes!("../assets/plugin-picker/balance_default.png")).unwrap();save_picture(&source,&dest).unwrap();assert_ne!(first,std::fs::read(&dest).unwrap());
  assert_ne!(custom_path("vst3:a"),custom_path("vst3:b"));
  std::fs::remove_file(source).unwrap();std::fs::remove_file(dest).unwrap();std::fs::remove_dir(root).unwrap();
 }
}

pub(crate) fn custom_picture(uid:&str)->Option<Arc<Image>>{if cfg!(test){return None;}custom_path(uid).and_then(|p|read(&p)).map(Arc::new)}
pub(crate) fn has_picture(e:&Entry)->bool{custom_picture(&e.uid).is_some() || native().contains_key(&key(&e.name)) || crate::plugin_downloads::picker_picture(&e.name).is_some()}
