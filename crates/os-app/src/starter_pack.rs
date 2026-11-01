// SPDX-License-Identifier: GPL-3.0-or-later
//! One opt-in, curated CC0 starter library. No network or worker until Download is clicked.
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Mutex, OnceLock}, time::Duration};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use os_ui::{Font, Rect};
use crate::app::{Action, App, Ui};

const NAME: &str = "Lemon Studio Starter Pack";
const CREDITS: &str = include_str!("../assets/starter-pack/Credits.txt");
const MANIFEST: &str = include_str!("../assets/starter-pack/catalog.json");
#[derive(Debug, Deserialize)]
struct Sample { path: String, url: String, size: u64, sha256: String }
fn catalog() -> &'static [Sample] {
    static DATA: OnceLock<Vec<Sample>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(MANIFEST).expect("validated starter catalog"))
}
#[derive(Default, Clone)]
struct Status { busy: bool, cancel: bool, changed: bool, message: String, ready: Option<PathBuf>, done: u64 }
fn status() -> &'static Mutex<Status> { static S: OnceLock<Mutex<Status>> = OnceLock::new(); S.get_or_init(Default::default) }
pub fn busy() -> bool { status().lock().unwrap().busy }
fn total() -> u64 { catalog().iter().map(|s|s.size).sum() }
fn report(message: String, done: u64) { let mut s=status().lock().unwrap(); s.message=message; s.done=done; s.changed=true; }
fn cancelled() -> Result<(), String> { if status().lock().unwrap().cancel {Err("Download cancelled. Click Download to continue.".into())} else {Ok(())} }
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_path(path: &str) -> bool {
    !path.is_empty() && !path.contains('\\') && !path.contains(':') && path.split('/').all(|p| !p.is_empty() && p!="." && p!=".." && !p.ends_with('.') && !p.ends_with(' ') && !p.chars().any(char::is_control)) && path.ends_with(".wav")
}
fn validate(s: &Sample) -> Result<(), String> {
    if !valid_path(&s.path) || s.size<44 || s.size>32*1024*1024 || s.sha256.len()!=64 || !s.sha256.bytes().all(|c|c.is_ascii_hexdigit()) {return Err("Invalid starter sample entry".into())}
    if !s.url.starts_with("https://raw.githubusercontent.com/stargatedaw/stargate-sample-pack/dbfd6ec52d4ed53b60bdbea5fc6adf295127c027/") && !s.url.starts_with("https://raw.githubusercontent.com/sgossner/VCSL/c1ea7bcc3c7309650ab0da9d15c9cd1fbc4a4c7e/") {return Err("Unrecognized sample source".into())}
    Ok(())
}
fn verified(path: &Path, s: &Sample) -> bool {
    // At most one bounded sample is read at a time, on the download thread.
    fs::symlink_metadata(path).is_ok_and(|m|m.is_file() && !m.file_type().is_symlink() && m.len()==s.size)
        && fs::read(path).is_ok_and(|b|hash(&b)==s.sha256)
}
fn copy_checked(mut reader: impl Read, mut out: impl Write, s: &Sample, mut progress: impl FnMut(u64)->Result<(),String>) -> Result<(),String> {
    let mut digest=Sha256::new(); let mut count=0u64; let mut buf=[0u8;65536];
    loop {
        progress(count)?;
        let n=reader.read(&mut buf).map_err(|e|e.to_string())?; if n==0 {break}
        count+=n as u64; if count>s.size {return Err("Sample download exceeded its expected size".into())}
        out.write_all(&buf[..n]).map_err(|e|e.to_string())?; digest.update(&buf[..n]);
    }
    if count!=s.size || format!("{:x}",digest.finalize())!=s.sha256 {return Err("Sample verification failed. Please retry.".into())}
    Ok(())
}
fn plain_dir(path: &Path) -> Result<(),String> {
    fs::create_dir_all(path).map_err(|e|e.to_string())?;
    let m=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !m.is_dir() || m.file_type().is_symlink() {return Err("Sample destination must be a normal folder".into())} Ok(())
}
fn unique() -> String { format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()) }
fn fetch(agent: &ureq::Agent, s: &Sample, cache: &Path, base: u64) -> Result<PathBuf,String> {
    validate(s)?; cancelled()?;
    let file=cache.join(format!("{}.wav",s.sha256));
    if verified(&file,s) {return Ok(file)}
    // Independent temp names permit two app processes to download without sharing unfinished writes.
    let temp=cache.join(format!("{}.part",unique()));
    let result=(|| {
        let response=agent.get(&s.url).set("User-Agent","LemonStudio-StarterPack/1").call().map_err(|_|"Could not download samples. Check your connection and click Download to retry.".to_string())?;
        if response.status()!=200 {return Err("The sample server is unavailable. Please retry later.".into())}
        let mut out=fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
        let mut last=u64::MAX;
        copy_checked(response.into_reader(),&mut out,s,|n| {
            cancelled()?; let done=base+n; let percent=done*100/total();
            if percent!=last {report(format!("Downloading {percent}%"),done);last=percent;} Ok(())
        })?;
        out.sync_all().map_err(|e|e.to_string())?; drop(out); cancelled()?;
        if file.exists() {
            if verified(&file,s) {return Ok(file.clone())}
            // Only the checksum-named cache file is replaceable; never touch a user's library file.
            let m=fs::symlink_metadata(&file).map_err(|e|e.to_string())?;
            if !m.is_file() || m.file_type().is_symlink() {return Err("Invalid sample cache entry".into())}
            fs::remove_file(&file).map_err(|e|e.to_string())?;
        }
        fs::rename(&temp,&file).map_err(|e|e.to_string())?; Ok(file.clone())
    })();
    let _=fs::remove_file(&temp); result
}
fn installed(dest: &Path) -> bool {
    fs::symlink_metadata(dest).is_ok_and(|m|m.is_dir() && !m.file_type().is_symlink())
        && fs::read_to_string(dest.join("Lemon pack version.txt")).is_ok_and(|s|s==hash(MANIFEST.as_bytes()))
        && catalog().iter().all(|s|dest.join(&s.path).is_file())
}
fn install(root: &Path) -> Result<PathBuf,String> {
    let samples=root.join("Samples"); plain_dir(&samples)?;
    let dest=samples.join(NAME);
    if dest.exists() {
        if installed(&dest) {return Ok(dest)}
        return Err("A Starter Pack folder already exists. Keep or rename it before downloading again; your files were not changed.".into())
    }
    let cache=root.join("Sample Downloads").join("Starter Pack v1"); plain_dir(&cache)?;
    let stage=samples.join(format!(".lemon-starter-{}",unique()));
    fs::create_dir(&stage).map_err(|e|e.to_string())?;
    let result=(|| {
        let agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(15)).build();
        let mut done=0;
        for sample in catalog() {
            cancelled()?;
            let source=fetch(&agent,sample,&cache,done)?;
            let target=stage.join(&sample.path);
            fs::create_dir_all(target.parent().ok_or("Invalid sample path")?).map_err(|e|e.to_string())?;
            let mut output=fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e|e.to_string())?;
            // Verify the copy too, including a concurrently modified cache file.
            copy_checked(fs::File::open(source).map_err(|e|e.to_string())?,&mut output,sample,|_|cancelled())?;
            done+=sample.size; report(format!("Downloading {}%",done*100/total()),done);
        }
        cancelled()?; report("Adding sounds to your library...".into(),total());
        fs::write(stage.join("Credits and License.txt"),CREDITS).map_err(|e|e.to_string())?;
        fs::write(stage.join("Lemon pack version.txt"),hash(MANIFEST.as_bytes())).map_err(|e|e.to_string())?;
        if dest.exists() { if installed(&dest) {return Ok(dest.clone())} return Err("Destination appeared during download. Existing files were preserved.".into()) }
        fs::rename(&stage,&dest).map_err(|e|e.to_string())?;
        Ok(dest.clone())
    })();
    // This exact, newly-created private staging directory is never exposed as a library.
    if stage.exists() {let _=fs::remove_dir_all(&stage);}
    result
}
fn start() {
    let mut s=status().lock().unwrap(); if s.busy {s.cancel=true;s.changed=true;s.message="Cancelling...".into();return}
    let Some(root)=os_paths::user_dir() else {s.message="Could not find your sample storage folder.".into();s.changed=true;return};
    *s=Status {busy:true,changed:true,message:"Connecting...".into(),..Default::default()}; drop(s);
    std::thread::spawn(move || {
        let result=install(&root); let mut s=status().lock().unwrap(); s.busy=false; s.changed=true;
        match result { Ok(path)=>{s.ready=Some(path);s.message=format!("{} sounds ready to use",catalog().len());}, Err(e)=>s.message=e }
    });
}
pub fn act(app: &mut App, cmd: u8) {
    match cmd {
        0=>app.search_folder_pick(app.state.browser_folders.len()),
        1=>if !cfg!(test) {start()},
        2=>{let mut s=status().lock().unwrap();s.cancel=true;s.changed=true;s.message="Cancelling...".into();},
        _=>{}
    }
    app.invalidate();
}
pub fn tick(app: &mut App) {
    let mut s=status().lock().unwrap(); if !std::mem::take(&mut s.changed) {return}
    let ready=s.ready.take(); let message=s.message.clone(); drop(s);
    if let Some(path)=ready {
        app.state.browser_dirs.borrow_mut().clear(); *app.state.browser_index.borrow_mut()=Default::default();
        // Extra root only when the user's chosen library is elsewhere; avoid duplicate trees.
        if path.parent()!=Some(crate::views::browser::samples_root().as_path()) {app.add_browser_folder(path.clone());}
        app.state.browser_tab=0;app.state.browser_search.clear();app.state.browser_scroll=0.;
        let root=crate::views::browser::samples_root();
        let key=if path.parent()==Some(root.as_path()) {format!("{}/{}",crate::views::browser::disk_key(&root),NAME)} else {crate::views::browser::disk_key(&path)};
        app.state.browser_open.insert(key);
    }
    app.state.message=Some(format!("Starter Pack: {message}"));app.invalidate();
}
fn button(ui:&mut Ui,r:Rect,label:&str,sub:&str,cmd:u8) {
    let hov=ui.hit(r,Action::StarterPack(cmd),format!("{label} {sub}"));
    ui.c.fill_rrect(r,5.,if hov {ui.t.browser_row_hi}else{ui.t.sunken});
    ui.c.stroke_rrect(r,5.,1.,ui.t.browser_tab_outline);
    ui.c.text_clip(Font::new(13),r.x+8.,r.y+8.,r.w-16.,ui.t.text,label);
    if !sub.is_empty() {ui.c.text_clip(Font::new(12),r.x+8.,r.y+27.,r.w-16.,ui.t.text_dim,sub);}
}
fn wrapped(ui:&mut Ui,text:&str,r:Rect) {
    let mut line=String::new();let mut y=r.y;let f=Font::new(12);
    for word in text.split_whitespace() {
        let next=if line.is_empty(){word.to_string()}else{format!("{line} {word}")};
        if !line.is_empty() && ui.c.text_width(f,&next)>r.w {
            if y+15.>r.bottom(){return} ui.c.text_clip(f,r.x,y,r.w,ui.t.text_dim,&line);y+=15.;line=word.into();
        } else {line=next;}
    }
    if y+15.<=r.bottom(){ui.c.text_clip(f,r.x,y,r.w,ui.t.text_dim,&line);}
}
pub fn paint_empty(ui:&mut Ui,r:Rect) {
    let x=r.x+8.;let w=(r.w-16.).max(0.);let y=r.y+16.;
    ui.c.text_clip(Font::new(15),x,y,w,ui.t.text,"Your sample library");
    button(ui,Rect::new(x,y+32.,w,36.),"Add my sample folder","",0);
    let drop=Rect::new(x,y+80.,w,53.);
    ui.c.stroke_rrect(drop,5.,1.,ui.t.browser_tab_outline);
    wrapped(ui,"Drag samples or folders here",Rect::new(x+10.,y+90.,w-20.,40.));
    let s=status().lock().unwrap().clone();
    if s.busy {button(ui,Rect::new(x,y+146.,w,36.),"Cancel download","",2);}
    else {button(ui,Rect::new(x,y+146.,w,52.),"Download","Lemon Studio Starter Pack",1);}
    wrapped(ui,&format!("{} sounds / {:.0} MB / Free CC0",catalog().len(),total() as f64/1048576.),Rect::new(x,y+210.,w,35.));
    wrapped(ui,"Drums, loops, keys, mallets, vocals and effects.",Rect::new(x,y+248.,w,48.));
    if !s.message.is_empty() {
        let rr=Rect::new(x,y+302.,w,95.);ui.hit(rr,Action::Swallow,s.message.clone());wrapped(ui,&s.message,rr);
        if s.busy {ui.c.fill_rect(Rect::new(x,y+289.,w,3.),ui.t.sunken);ui.c.fill_rect(Rect::new(x,y+289.,w*(s.done as f32/total() as f32).clamp(0.,1.),3.),ui.t.browser_search);}
    }
}
pub fn paint_status(ui:&mut Ui,r:Rect) {
    let s=status().lock().unwrap().clone(); if !s.busy {return}
    let panel=Rect::new(r.x+4.,(r.bottom()-94.).max(r.y),r.w-8.,94.);
    ui.c.fill_rrect(panel,4.,ui.t.browser_footer);ui.hit(panel,Action::Swallow,s.message.clone());
    wrapped(ui,&format!("Starter Pack: {}",s.message),Rect::new(panel.x+6.,panel.y+6.,panel.w-12.,40.));
    button(ui,Rect::new(panel.x+6.,panel.y+51.,panel.w-12.,35.),"Cancel download","",2);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_pack_browser_and_optional_download_audit() {
        let root=std::env::temp_dir().join(format!("lemon-starter-flow-{}",unique()));fs::create_dir_all(&root).unwrap();
        let library=root.join("Samples");fs::create_dir(&library).unwrap();
        crate::prefs::set_text("browser_samples_root",&library.to_string_lossy());
        let mut app=App::new(None);app.state.show_browser=true;app.state.browser_tab=0;app.state.browser_folders.clear();
        app.state.browser_search.clear();app.state.browser_auto_hidden=false;
        let mut canvas=os_ui::Canvas::new(1280,720,1.).unwrap();app.paint(&mut canvas,None);
        for cmd in [0,1] {assert!(app.hits.iter().any(|h|h.action==Action::StarterPack(cmd)),"empty browser control {cmd}");}
        if let Ok(path)=std::env::var("LEMON_STARTER_SHOT"){canvas.pixmap.save_png(Path::new(&path)).unwrap();}
        assert!(!busy());
        app.state.browser_search="not found".into();app.paint(&mut canvas,None);
        assert!(!app.hits.iter().any(|h|h.action==Action::StarterPack(1)),"search results are not onboarding");
        app.state.browser_search.clear();app.state.browser_tab=3;app.paint(&mut canvas,None);
        assert!(!app.hits.iter().any(|h|h.action==Action::StarterPack(1)),"favorites are not onboarding");
        app.state.browser_tab=0;app.paint(&mut canvas,None);crate::dialog::fake::live(true);act(&mut app,0);assert!(!app.state.dialogs.is_empty());app.state.dialogs.clear();crate::dialog::fake::live(false);
        if let Ok(audit)=std::env::var("LEMON_STARTER_AUDIT") {
            let cache=root.join("Sample Downloads").join("Starter Pack v1");fs::create_dir_all(&cache).unwrap();
            for (i,sample) in catalog().iter().enumerate() {
                let file=Path::new(&audit).join(&sample.path);assert!(verified(&file,sample),"{}",sample.path);
                let sound=os_engine::sample::load(&file).unwrap_or_else(|e|panic!("{}: {e}",sample.path));
                assert!(sound.frames()>0 && sound.rate>=22050);
                assert!(sound.left.iter().chain(&sound.right).all(|v|v.is_finite()));
                assert!(sound.left.iter().chain(&sound.right).any(|v|v.abs()>0.00001),"silent {}",sample.path);
                // One real HTTPS request exercises the production downloader; the other audited files exercise resume.
                if i!=0 {fs::copy(&file,cache.join(format!("{}.wav",sample.sha256))).unwrap();}
            }
            let dest=install(&root).unwrap();assert!(installed(&dest));
            for sample in catalog() {assert!(verified(&dest.join(&sample.path),sample));}
            {let mut state=status().lock().unwrap();state.ready=Some(dest.clone());state.changed=true;state.message="Ready".into();}
            tick(&mut app);let rows=crate::views::browser::rows(&app.state);
            assert!(rows.iter().any(|r|r.label=="Drums"),"installed tree expanded");
            assert!(app.state.browser_folders.is_empty(),"default root is not added twice");
            let before=app.state.project.channels.len();app.mouse=(30.,220.);
            app.drop_file(Path::new(&audit).join(&catalog()[0].path));
            assert_eq!(app.state.project.channels.len(),before);
            assert!(crate::views::browser::rows(&app.state).iter().any(|r|matches!(r.item,crate::views::browser::Item::Sample(_))));
            assert_eq!(install(&root).unwrap(),dest,"repeat does not overwrite the pack");
        }
        *status().lock().unwrap()=Status::default();
        crate::prefs::set_text("browser_samples_root","");fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn starter_pack_integrity_and_safe_install() {
        let mut paths=std::collections::HashSet::new();
        assert_eq!(catalog().len(),263);
        for s in catalog() {validate(s).unwrap();assert!(paths.insert(s.path.to_lowercase()));}
        assert!(!valid_path("../outside.wav"));assert!(!valid_path("C:/outside.wav"));assert!(!valid_path("safe/../../outside.wav"));assert!(!valid_path("safe\\outside.wav"));
        let bytes=b"synthetic fixture";
        let sample=Sample{path:"Drums/test.wav".into(),url:catalog()[0].url.clone(),size:bytes.len() as u64,sha256:hash(bytes)};
        let mut out=Vec::new();copy_checked(&bytes[..],&mut out,&sample,|_|Ok(())).unwrap();assert_eq!(out,bytes);
        assert!(copy_checked(&b"wrong bytes"[..],Vec::new(),&sample,|_|Ok(())).is_err());
        assert!(copy_checked(&bytes[..],Vec::new(),&sample,|_|Err("cancelled".into())).is_err());
        let root=std::env::temp_dir().join(format!("lemon-starter-test-{}",unique()));
        let dest=root.join("Samples").join(NAME);fs::create_dir_all(&dest).unwrap();fs::write(dest.join("personal.wav"),b"preserve me").unwrap();
        assert!(install(&root).unwrap_err().contains("already exists"));assert_eq!(fs::read(dest.join("personal.wav")).unwrap(),b"preserve me");
        fs::remove_dir_all(root).unwrap();
    }
}
