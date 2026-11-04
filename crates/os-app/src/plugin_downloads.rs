// SPDX-License-Identifier: GPL-3.0-or-later
//! Curated, checksum-pinned third-party plugin downloads. No background installs or silent elevation.
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Mutex, OnceLock}, time::Duration};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use os_ui::{Rect, Font};
use crate::{app::{Action, App, State, Ui}, icons::{self, Icon}, views::browser::{Row, Item}};

pub const TAB: usize = 5;
pub const ROW_H: f32 = 46.;
#[derive(Clone, Debug, Deserialize)]
pub struct Download {
    pub platform: String, pub url: String, pub filename: String, pub version: String,
    pub note: String, pub sha256: String, pub size: u64,
    #[serde(default)] pub install: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Plugin {
    pub id: String, pub name: String, pub category: String, pub description: String,
    pub source: String, pub license: String, pub license_url: String, pub downloads: Vec<Download>,
    pub note: String,
    pub info: String,
    #[serde(default)] pub freeware: bool,
}
pub fn catalog() -> &'static [Plugin] {
    static DATA: OnceLock<Vec<Plugin>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/plugin-catalog/catalog.json")).expect("validated plugin catalog"))
}
fn platform() -> &'static str {
    if cfg!(all(windows,target_arch="x86_64")) {"windows-x86_64"}
    else if cfg!(all(target_os="macos",target_arch="aarch64")) {"macos-aarch64"}
    else if cfg!(all(target_os="macos",target_arch="x86_64")) {"macos-x86_64"}
    else if cfg!(all(target_os="linux",target_arch="x86_64")) {"linux-x86_64"}
    else {"unsupported"}
}
fn for_platform<'a>(p: &'a Plugin, platform: &str) -> Option<&'a Download> {
    p.downloads.iter().find(|d|d.platform==platform)
        .or_else(|| platform.starts_with("macos-").then(|| p.downloads.iter().find(|d|d.platform=="macos-universal")).flatten())
}
fn download(p: &Plugin) -> Option<&Download> { for_platform(p, platform()) }
pub fn row_index(key: &str) -> Option<usize> { key.strip_prefix("5:plugin/")?.parse().ok().filter(|i:&usize|*i<catalog().len()) }
pub fn rows(s: &State) -> Vec<Row> {
    let q=s.browser_search.to_lowercase();let mut out=Vec::new();
    let mut categories=Vec::new();
    for p in catalog() {if !categories.contains(&p.category.as_str()) {categories.push(p.category.as_str());}}
    for category in categories {
        let matches: Vec<_>=catalog().iter().enumerate().filter(|(_,p)| p.category==category &&
            (q.is_empty() || format!("{} {} {} {} {}",p.name,p.category,p.description,p.info,p.license).to_lowercase().contains(&q))).collect();
        if matches.is_empty() {continue}
        let key=format!("5:category/{category}");let open=!q.is_empty() || s.browser_open.contains(&key);
        out.push(Row{key,depth:0,label:format!("{category} ({})",matches.len()),icon:Icon::Folder,cat:255,open:Some(open),bright:true,item:Item::None,path:None});
        if open {for (i,p) in matches {out.push(Row{key:format!("5:plugin/{i}"),depth:1,label:p.name.clone(),icon:Icon::Plug,cat:1,open:None,bright:true,item:Item::None,path:None});}}
    }
    out
}
pub fn detail_height(s: &State) -> f32 { ((s.screen.1-crate::views::browser::list_top(s)-28.)*0.55).clamp(155.,280.) }
fn selected(s:&State)->Option<usize> { s.browser_sel.as_deref().and_then(row_index) }
include!("plugin_thumbnails.rs");
fn thumbnail(ui:&mut Ui,i:usize,r:Rect) {
    if let Some((w,h,pixels))=thumbnails().get(i).and_then(|p|p.as_ref()) {ui.c.image(r,*w,*h,pixels);}
}
fn hover_allowed(s:&State)->bool {
    s.show_browser && s.browser_tab==TAB && !s.browser_auto_hidden && s.native_view.is_none()
        && s.dialogs.is_empty() && s.menu_open.is_none() && !s.settings.open && s.playlist.popup.is_none()
}
fn hover_tick(app:&mut App) {
    let index=if hover_allowed(&app.state) && app.drag.is_none() {
        match app.hover.as_ref() {
            Some(Action::BrowserRow(ri))=>rows(&app.state).get(*ri).and_then(|r|row_index(&r.key)),
            Some(Action::PluginDownload(i,0)) if *i<catalog().len()=>Some(*i),
            _=>None,
        }
    }else{None};
    let previous=app.state.catalog_tooltip;
    if app.state.catalog_hover.map(|(i,_)|i)!=index {
        app.state.catalog_hover=index.map(|i|(i,std::time::Instant::now()));
        app.state.catalog_tooltip=None;
    }
    if let Some((i,since))=app.state.catalog_hover {
        if since.elapsed()>=Duration::from_millis(450){app.state.catalog_tooltip=Some(i);}
    }
    if previous!=app.state.catalog_tooltip {app.invalidate();}
}
fn text_height(ui:&mut Ui,text:&str,width:f32)->f32 {
    let mut line=String::new();let mut lines=1;
    for word in text.split_whitespace() {
        let next=if line.is_empty(){word.into()}else{format!("{line} {word}")};
        if !line.is_empty() && ui.c.text_width(Font::new(12),&next)>width {lines+=1;line=word.into();}else{line=next;}
    }
    lines as f32*15.
}
pub fn paint_tooltip(ui:&mut Ui,s:&State) {
    if !hover_allowed(s){return}
    let Some(i)=s.catalog_tooltip else{return};let Some(p)=catalog().get(i)else{return};
    let Some(ri)=rows(s).iter().position(|r|row_index(&r.key)==Some(i))else{return};
    let top=crate::views::browser::list_top(s);
    let row_y=top+ri as f32*ROW_H-s.browser_scroll;
    if row_y+ROW_H<=top || row_y>=s.screen.1-28.-detail_height(s){return}
    let w=360_f32.min(s.screen.0-16.);let body_w=w-24.;
    let ih=text_height(ui,&p.info,body_w);let nh=text_height(ui,&p.note,body_w);
    let h=94.+ih+nh;
    let x=(crate::views::browser_w(s)+8.).min((s.screen.0-w-8.).max(8.));
    let y=row_y.clamp(8.,(s.screen.1-h-8.).max(8.));let r=Rect::new(x,y,w,h);
    // Register as an overlay so native plugin editors are clipped underneath it.
    // The card has no controls and cannot start a download.
    ui.hit(r,Action::Swallow,format!("{}: {}",p.name,p.info));
    ui.c.fill_rrect(r,6.,ui.t.browser_footer);
    ui.c.stroke_rrect(r,6.,1.,ui.t.browser_search);
    thumbnail(ui,i,Rect::new(x+12.,y+12.,80.,50.));
    ui.c.text_clip(Font::new(14),x+102.,y+12.,w-114.,ui.t.text,&p.name);
    ui.c.text_clip(Font::new(12),x+102.,y+32.,w-114.,ui.t.text_dim,&p.category);
    ui.c.text_clip(Font::new(11),x+102.,y+48.,w-114.,ui.t.browser_search,if p.freeware{"Freeware"}else{"Free / open source"});
    wrapped(ui,&p.info,Rect::new(x+12.,y+74.,body_w,ih),ui.t.text);
    wrapped(ui,&p.note,Rect::new(x+12.,y+82.+ih,body_w,nh),ui.t.text_dim);
}
fn button(ui:&mut Ui,r:Rect,label:&str,action:Action,tip:&str,enabled:bool) {
    let hov=if enabled {ui.hit(r,action,tip.to_string())} else {ui.hit(r,Action::Swallow,tip.to_string());false};
    ui.c.fill_rrect(r,3.,if hov {ui.t.browser_row_hi} else {ui.t.sunken});
    ui.c.text_clip(Font::new(13),r.x+5.,r.y+3.,r.w-10.,if enabled {ui.t.text}else{ui.t.text_faint},label);
}
fn wrapped(ui:&mut Ui,text:&str,r:Rect,color:os_ui::Color) {
    let f=Font::new(12);let mut y=r.y;let mut line=String::new();
    for word in text.split_whitespace() {
        let next=if line.is_empty(){word.to_string()}else{format!("{line} {word}")};
        if !line.is_empty() && ui.c.text_width(f,&next)>r.w {
            if y+15.>r.bottom(){return} ui.c.text_clip(f,r.x,y,r.w,color,&line);y+=15.;line=word.into();
        } else {line=next;}
    }
    if y+15.<=r.bottom(){ui.c.text_clip(f,r.x,y,r.w,color,&line);}
}
pub fn paint(ui:&mut Ui,s:&State,r:Rect) {
    let dh=detail_height(s).min((r.h-46.).max(0.));let list=Rect::new(r.x,r.y,r.w,(r.h-dh).max(0.));
    ui.c.fill_rect(r,ui.t.browser_list);ui.hit(r,Action::BrowserList,"Plugin Downloads");
    let rows=rows(s);let scroll=s.browser_scroll.min((rows.len() as f32*ROW_H-list.h).max(0.));
    ui.c.push_clip(list);
    for (ri,row) in rows.iter().enumerate() {
        let y=list.y+ri as f32*ROW_H-scroll;
        if y+ROW_H<=list.y || y>=list.bottom(){continue}
        let rr=Rect::new(r.x+4.,y,r.w-8.,ROW_H-2.);
        let hov=ui.hit(rr.intersect(&list),Action::BrowserRow(ri),row_index(&row.key).map_or_else(||row.label.clone(),|i|format!("{}: {}",catalog()[i].name,catalog()[i].info)));
        if hov || s.browser_sel.as_deref()==Some(&row.key){ui.c.fill_rrect(rr,3.,ui.t.browser_row_hi);}
        if let Some(i)=row_index(&row.key) {
            let p=&catalog()[i];thumbnail(ui,i,Rect::new(rr.x+3.,y+6.,48.,30.));
            let x=rr.x+57.;let w=(rr.w-83.).max(0.);
            ui.c.text_clip(Font::new(14),x,y+4.,w,ui.t.text,&p.name);
            ui.c.text_clip(Font::new(11),x,y+24.,w,ui.t.text_dim,&format!("Free / {}",p.license));
            let dl=Rect::new(rr.right()-25.,y+10.,23.,25.);
            ui.hit(dl.intersect(&list),Action::PluginDownload(i,0),format!("Download {}. {}",p.name,p.note));
            icons::draw(ui.c,Icon::Download,dl,if download(p).is_some(){ui.t.browser_search}else{ui.t.text_faint});
        } else {
            icons::draw(ui.c,if row.open==Some(true){Icon::FolderOpen}else{Icon::Folder},Rect::new(rr.x+4.,y+12.,18.,18.),ui.t.browser_folder);
            ui.c.text_clip(Font::new(14),rr.x+27.,y+12.,rr.w-30.,ui.t.text,&row.label);
        }
    }
    if rows.is_empty(){ui.c.text(Font::new(13),r.x+9.,r.y+10.,ui.t.text_dim,"No matching plugins");}
    ui.c.pop_clip();
    let panel=Rect::new(r.x,list.bottom(),r.w,dh);ui.c.fill_rect(panel,ui.t.browser_footer);ui.hit(panel,Action::Swallow,"");ui.c.push_clip(panel);
    let x=panel.x+8.;let w=panel.w-16.;let bottom=panel.bottom();let i=selected(s).unwrap_or(0);let p=&catalog()[i];
    let st=status().lock().unwrap().clone();let same=st.index==Some(i);
    if selected(s).is_some() {
        ui.c.text_clip(Font::new(15),x,panel.y+5.,w,ui.t.text,&p.name);
        let top=panel.y+27.;
        if dh>=250. {
            thumbnail(ui,i,Rect::new(x,top,96.,60.));
            wrapped(ui,&format!("Free\n{}\n{}",p.license,download(p).map_or("No native build",|d|d.version.as_str())),Rect::new(x+102.,top,w-102.,60.),ui.t.text_dim);
            wrapped(ui,&p.description,Rect::new(x,top+64.,w,(bottom-135.-top-64.).max(0.)),ui.t.text);
        } else {wrapped(ui,&p.description,Rect::new(x,top,w,(bottom-135.-top).max(0.)),ui.t.text);}
    } else {
        ui.c.text_clip(Font::new(15),x,panel.y+5.,w,ui.t.text,"75 free plugins");
        wrapped(ui,"Choose a category, then a plugin. Hover for information. Portable plugins install automatically. Other downloads open their installer or archive.",Rect::new(x,panel.y+29.,w,(dh-165.).max(0.)),ui.t.text_dim);
    }
    let tip=download(p).map_or("No compatible native download for this system".to_string(),|d|format!("{} / {:.1} MB. {} {} Finish installation, then Scan Plugins.",d.version,d.size as f64/1048576.,d.note,p.note));
    let label=if st.busy&&same {"Downloading..."}else if download(p).is_some_and(|d|!d.install.is_empty()) {"Download + Install"}else{"Download + Open"};
    button(ui,Rect::new(x,bottom-130.,w,24.),label,Action::PluginDownload(i,0),&tip,selected(s).is_some()&&download(p).is_some()&&!st.busy);
    let half=(w-5.)/2.;
    button(ui,Rect::new(x,bottom-102.,half,21.),if p.freeware {"Website"} else {"Source"},Action::PluginDownload(i,1),&p.source,selected(s).is_some());
    button(ui,Rect::new(x+half+5.,bottom-102.,half,21.),"License",Action::PluginDownload(i,5),&p.license_url,selected(s).is_some());
    let message=if st.busy {format!("{}: {}",st.index.and_then(|i|catalog().get(i)).map_or("Download",|p|p.name.as_str()),st.message)}else if same && !st.message.is_empty(){st.message.clone()}else if selected(s).is_some(){tip}else{"Installer/archive opens after download. Nothing downloads until you click.".into()};
    ui.hit(Rect::new(x,bottom-77.,w,43.),Action::Swallow,message.clone());
    wrapped(ui,&message,Rect::new(x,bottom-77.,w,43.),ui.t.text_dim);
    button(ui,Rect::new(x,bottom-29.,half,23.),"Scan Plugins",Action::PluginDownload(i,3),"Scan after completing the plugin installation",true);
    button(ui,Rect::new(x+half+5.,bottom-29.,half,23.),if st.busy{"Cancel"}else{"Downloads"},Action::PluginDownload(i,if st.busy{4}else{2}),"Cancel this download, or open downloaded packages",true);
    ui.c.pop_clip();
}
#[derive(Clone,Default)]
struct Status { busy:bool, cancel:bool, changed:bool, index:Option<usize>, message:String, ready:Option<PathBuf>, installed:Option<PathBuf> }
static STATUS: OnceLock<Mutex<Status>>=OnceLock::new();
fn status()->&'static Mutex<Status>{STATUS.get_or_init(Default::default)}
fn cache()->Result<PathBuf,String>{
    let dir=os_paths::user_dir().ok_or("Cannot find Lemon Studio's user folder")?.join("Plugin Downloads");
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;Ok(dir)
}
fn report(message:String){let mut s=status().lock().unwrap();s.message=message;s.changed=true;}
fn cancelled()->bool {status().lock().unwrap().cancel}
fn allowed_url(url:&str)->bool {
    let Some(rest)=url.strip_prefix("https://")else{return false};
    let host=rest.split('/').next().unwrap_or("").to_ascii_lowercase();
    ["github.com","release-assets.githubusercontent.com","objects.githubusercontent.com","www.airwindows.com","airwindows.com","socalabs.com","www.socalabs.com","www.dropbox.com","dl.dropboxusercontent.com","www.tokyodawn.net","www.voxengo.com","cdn.youlean.co","xferrecords.com","klanghelm.com","dl.u-he.com","tal-software.com","0911754ed6a6961486bafb46cc7496df.r2.cloudflarestorage.com"].contains(&host.as_str())
        || host=="klanghelmfree.s3.eu-central-1.amazonaws.com" || host=="uhe-dl.b-cdn.net"
        || host.ends_with(".githubusercontent.com") || host.ends_with(".dropboxusercontent.com")
}
fn safe_name(name:&str)->bool{!name.is_empty() && name.len()<180 && name.chars().all(|c|c.is_ascii_alphanumeric()||"._-".contains(c)) && !name.starts_with('.')}
fn validate(d:&Download)->Result<(),String>{
    if !allowed_url(&d.url)||!safe_name(&d.filename)||d.size==0||d.size>2*1024*1024*1024||d.sha256.len()!=64||!d.sha256.bytes().all(|c|c.is_ascii_hexdigit()) {return Err("This catalog entry needs a download update. Open Source to visit the maker.".into())}
    if d.install.iter().any(|r|!member_allowed(r,&d.install)||!r.ends_with(".vst3")&&!r.ends_with(".dll")&&!r.ends_with(".clap")) {return Err("Invalid portable plugin paths".into())}
    if ![".exe",".msi",".zip",".dmg",".pkg",".deb",".tar.gz",".tar.xz"].iter().any(|e|d.filename.ends_with(e)){return Err("Unsupported package type".into())} Ok(())
}
fn hex(bytes:&[u8])->String{bytes.iter().map(|b|format!("{b:02x}")).collect()}
fn verified(path:&Path,d:&Download)->bool {
    let Ok(meta)=fs::symlink_metadata(path)else{return false};if !meta.is_file()||meta.file_type().is_symlink()||meta.len()!=d.size{return false}
    let Ok(mut file)=fs::File::open(path)else{return false};let mut hash=Sha256::new();let mut buf=[0u8;65536];
    loop{match file.read(&mut buf){Ok(0)=>break,Ok(n)=>hash.update(&buf[..n]),Err(_)=>return false}}
    hex(&hash.finalize())==d.sha256.to_lowercase()
}
fn response(url:&str)->Result<ureq::Response,String>{
    let agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(Duration::from_secs(20)).timeout_read(Duration::from_secs(20)).build();
    let mut url=url.to_string();
    for _ in 0..8 {
        if cancelled(){return Err("Download cancelled".into())}if !allowed_url(&url){return Err("Download redirected outside the official download services".into())}
        let r=agent.get(&url).set("User-Agent","LemonStudio-PluginDownloads/1.0").set("Accept","*/*").call().map_err(|e|e.to_string())?;
        if (300..400).contains(&r.status()) {
            let loc=r.header("Location").ok_or("Redirect without a destination")?;
            url=if loc.starts_with("https://"){loc.into()}else if loc.starts_with('/')&&!loc.starts_with("//"){format!("https://{}{}",url.strip_prefix("https://").unwrap().split('/').next().unwrap(),loc)}else{return Err("Unsupported download redirect".into())};
        }else if r.status()==200{return Ok(r)}else{return Err(format!("Download server returned {}",r.status()))}
    }Err("Too many download redirects".into())
}
fn copy_verified(mut reader:impl Read,mut output:impl Write,d:&Download,mut progress:impl FnMut(u64)->Result<(),String>)->Result<(),String>{
    let mut hash=Sha256::new();let mut count=0;let mut buf=[0u8;65536];
    loop {progress(count)?;let n=reader.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}count+=n as u64;
        if count>d.size{return Err("Download size changed. Please update the catalog.".into())}
        output.write_all(&buf[..n]).map_err(|e|e.to_string())?;hash.update(&buf[..n]);}
    if count!=d.size||hex(&hash.finalize())!=d.sha256.to_lowercase(){return Err("Download verification failed. Nothing was opened. Please update the catalog or visit Source.".into())}Ok(())
}
fn fetch(d:&Download,root:&Path)->Result<PathBuf,String>{
    validate(d)?;let dir=root.join(&d.sha256[..16]);fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let path=dir.join(&d.filename);if verified(&path,d){return Ok(path)}
    if path.exists(){return Err("An existing downloaded file failed verification. Remove it from Downloads and retry.".into())}
    let part=dir.join(format!("{}-{}.part",d.filename,std::process::id()));
    let run=||->Result<(),String>{
        let r=response(&d.url)?;
        if r.header("Content-Type").is_some_and(|t|t.contains("text/html")){return Err("Server returned a web page instead of an installer".into())}
        let mut out=fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(|e|e.to_string())?;
        let mut last=101;
        copy_verified(r.into_reader(),&mut out,d,|count|{
            if cancelled(){return Err("Download cancelled".into())}
            let percent=count.saturating_mul(100)/d.size;
            if percent!=last{report(format!("Downloading: {percent}% ({:.1} / {:.1} MB)",count as f64/1048576.,d.size as f64/1048576.));last=percent;}Ok(())
        })?;
        out.sync_all().map_err(|e|e.to_string())?;drop(out);
        if cancelled(){return Err("Download cancelled".into())}fs::rename(&part,&path).map_err(|e|e.to_string())?;Ok(())
    };
    let result=run();if result.is_err(){let _=fs::remove_file(&part);}result.map(|_|path)
}
fn mark_download(path:&Path,url:&str)->Result<(),String>{
    #[cfg(windows)] {
        use std::ffi::OsString;
        let mut ads:OsString=path.as_os_str().to_os_string();ads.push(":Zone.Identifier");
        fs::write(PathBuf::from(ads),format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={url}\r\n")).map_err(|e|format!("Could not mark downloaded package: {e}"))?;
    }
    #[cfg(target_os="macos")] {
        let time=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs();
        let result=std::process::Command::new("/usr/bin/xattr").args(["-w","com.apple.quarantine",&format!("0083;{time:x};LemonStudio;")]).arg(path).status().map_err(|e|e.to_string())?;
        if !result.success(){return Err("Could not mark downloaded package for macOS verification".into())}
    }
    let _=(path,url);Ok(())
}
// Explicit catalog member roots only: never unpack arbitrary paths or execute archive contents.
fn member_allowed(name:&str,roots:&[String])->bool {
    !name.contains('\\') && !name.split('/').any(|s|s==".." || s.contains(':')) && !name.starts_with('/') &&
        roots.iter().any(|r|name==r || name.starts_with(&format!("{r}/")))
}
fn install_portable(archive:&Path,d:&Download,id:&str,root:&Path)->Result<PathBuf,String>{
    if d.install.is_empty()||!d.filename.ends_with(".zip")||d.platform!="windows-x86_64" {return Err("This package needs its own installer".into())}
    let parent=root.join("Installed").join(id);fs::create_dir_all(&parent).map_err(|e|e.to_string())?;
    let dest=parent.join(&d.sha256[..16]);
    let stage=parent.join(format!(".partial-{}",std::process::id()));fs::create_dir(&stage).map_err(|e|e.to_string())?;
    let result=(||->Result<(),String>{
        let mut zip=zip::ZipArchive::new(fs::File::open(archive).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let mut found=std::collections::HashSet::new();let mut written=std::collections::HashSet::new();let mut paths=Vec::new();let mut total=0u64;
        if zip.len()>20000{return Err("Archive contains too many files".into())}
        for n in 0..zip.len(){
            if cancelled(){return Err("Download cancelled".into())}
            let file=zip.by_index(n).map_err(|e|e.to_string())?;let name=file.name().map_err(|e|e.to_string())?.into_owned();
            if !member_allowed(&name,&d.install){continue}
            if file.unix_mode().is_some_and(|m|m&0o170000==0o120000){return Err("Plugin archive contains a symbolic link".into())}
            let member=d.install.iter().find(|r|name==**r || name.starts_with(&format!("{r}/"))).unwrap();
            let base=member.rsplit('/').next().ok_or("Invalid plugin member")?;
            let relative=format!("{base}{}",&name[member.len()..]);
            if relative.split('/').any(|s|s.ends_with('.')||s.ends_with(' ')){return Err("Invalid Windows plugin path".into())}
            let path=stage.join(&relative);if file.is_dir(){fs::create_dir_all(&path).map_err(|e|e.to_string())?;continue}
            if !written.insert(relative.to_lowercase()){return Err("Duplicate archive member".into())}
            paths.push(relative);total=total.checked_add(file.size()).ok_or("Archive is too large")?;if total>512*1024*1024{return Err("Extracted plugin is too large".into())}
            fs::create_dir_all(path.parent().ok_or("Invalid member directory")?).map_err(|e|e.to_string())?;
            let mut out=fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|e.to_string())?;
            let declared=file.size();let copied=std::io::copy(&mut file.take(declared+1),&mut out).map_err(|e|e.to_string())?;
            if copied!=declared{return Err("Plugin member size mismatch".into())}drop(out);
            if path.extension().is_some_and(|e|e=="dll"||e=="vst3"||e=="clap") && os_vst2::pe_machine(&path)!=Some("x86_64") {return Err("Plugin binary does not match Windows 64-bit".into())}
            found.insert(member.clone());
        }
        if d.install.iter().any(|r|!found.contains(r)){return Err("The archive does not contain the expected plugin".into())}
        if cancelled(){return Err("Download cancelled".into())}
        fs::write(stage.join("Lemon download.txt"),format!("Downloaded from {}\nArchive SHA256: {}\nOriginal package retained in Plugin Downloads.\n",d.url,d.sha256)).map_err(|e|e.to_string())?;
        if dest.exists(){
            if fs::symlink_metadata(&dest).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Installed plugin folder is a link".into())}
            for relative in &paths {
                let existing=dest.join(relative);let expected=stage.join(relative);
                let meta=fs::symlink_metadata(&existing).map_err(|_|"Installed plugin is incomplete. Use Downloads to inspect it before retrying.")?;
                if !meta.is_file()||meta.file_type().is_symlink()||fs::read(&existing).map_err(|e|e.to_string())?!=fs::read(&expected).map_err(|e|e.to_string())? {return Err("Installed plugin differs from the download; it was preserved.".into())}
            }
            fs::remove_dir_all(&stage).map_err(|e|e.to_string())?;
        }else{fs::rename(&stage,&dest).map_err(|e|e.to_string())?;}Ok(())
    })();
    if result.is_err(){let _=fs::remove_dir_all(&stage);}result.map(|_|dest)
}
fn start(i:usize){
    let Some(p)=catalog().get(i)else{return};let mut s=status().lock().unwrap();
    if s.busy || s.ready.is_some() || s.installed.is_some(){return}let Some(d)=download(p).cloned()else{s.index=Some(i);s.message="No compatible native download. See Source for the maker's options.".into();s.changed=true;return};
    *s=Status{busy:true,index:Some(i),changed:true,message:"Connecting to the official download...".into(),..Default::default()};drop(s);
    std::thread::spawn(move||{
        let result=cache().and_then(|root| {
            let path=fetch(&d,&root)?;
            if !d.install.is_empty(){report("Installing portable plugin...".into());install_portable(&path,&d,&catalog()[i].id,&root).map(|p|(p,true))}
            else {mark_download(&path,&d.url)?;Ok((path,false))}
        });let mut s=status().lock().unwrap();s.busy=false;s.changed=true;
        if s.cancel && !matches!(&result,Ok((_,true))) {s.message="Download cancelled. Nothing opened.".into();return}
        match result{Ok((path,true))=>{s.installed=Some(path);s.message="Portable plugin installed. Scanning...".into()},Ok((path,false))=>{s.ready=Some(path);s.message="Download verified. Opening...".into()},Err(e)=>s.message=format!("{e} Click Download to retry.")}
    });
}
pub fn act(app:&mut App,i:usize,cmd:u8){
    match cmd{
        0=>{app.state.browser_sel=Some(format!("5:plugin/{i}"));if !cfg!(test){start(i)}},
        1|5=>if let Some(p)=catalog().get(i){if let Err(e)=crate::donate::open_url(if cmd==1{&p.source}else{&p.license_url}){app.state.message=Some(e)}},
        2=>match cache().and_then(|p|crate::donate::open_url(&p.to_string_lossy())){Ok(())=>{},Err(e)=>app.state.message=Some(e)},
        3=>app.start_scan(crate::plugin_db::Mode::Changed),
        4=>{let mut s=status().lock().unwrap();s.cancel=true;s.ready=None;s.message="Cancelling download...".into();s.changed=true;},
        _=>{}
    }app.invalidate();
}
pub fn tick(app:&mut App){
    hover_tick(app);
    let mut s=status().lock().unwrap();if !s.changed{return}s.changed=false;
    let ready=if s.cancel{None}else{s.ready.take()};let installed=s.installed.take();drop(s);
    if let Some(path)=installed {
        // Disable only older catalog-owned scan folders for this same plugin; keep their files for rollback.
        if let Some(parent)=path.parent(){
            for folder in &mut app.state.plugdb.prefs.folders {let previous=Path::new(&folder.path);if previous==path {folder.on=true;}else if previous.parent()==Some(parent){folder.on=false;}}
        }
        app.state.plugdb.prefs.add_folder(&path);app.prefs_changed();app.start_scan(crate::plugin_db::Mode::Changed);
        report("Installed. Plugin scan started; it will appear in the instrument/effect menu when scanning finishes.".into());
    }
    if let Some(path)=ready {
        let message=match crate::donate::open_url(&path.to_string_lossy()){
            Ok(())=>"Package opened. Complete installation, then click Scan Plugins. For archives, extract the plugin files into your plugin folder, or add their extracted folder in Settings > Plugins.".to_string(),
            Err(e)=>format!("Downloaded, but could not open: {e}. Use Downloads to open the saved package.")};
        report(message.clone());app.state.message=Some(message);
    }app.invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_and_download_integrity(){
        let mut cats=std::collections::HashMap::new();let mut ids=std::collections::HashSet::new();assert_eq!(catalog().len(),75);
        for p in catalog(){assert!(ids.insert(&p.id));*cats.entry(&p.category).or_insert(0)+=1;
            assert!(p.source.starts_with("https://"));assert!(p.info.len()>50);assert!(!p.license.is_empty());
            assert!(for_platform(p,"windows-x86_64").is_some(),"{}",p.name);
            for d in &p.downloads{validate(d).unwrap_or_else(|e|panic!("{} {}: {e}",p.name,d.platform));}
        }assert_eq!(cats.len(),10);assert!(cats.values().all(|n|*n>=5));assert_eq!(catalog().iter().filter(|p|p.freeware).count(),25);
        assert!(!member_allowed("bundle/../bad.dll",&["bundle".into()]));
        assert!(!member_allowed("C:/bad.dll",&["C:".into()]));
        assert!(!member_allowed("plugin.dll.exe",&["plugin.dll".into()]));
        assert!(member_allowed("good.vst3/Contents/x86_64-win/good.vst3",&["good.vst3".into()]));
        assert!(!allowed_url("http://github.com/file"));assert!(!allowed_url("https://github.com.evil.test/file"));assert!(!safe_name("../bad.exe"));
        let mut d=catalog()[0].downloads[0].clone();let bytes=b"test package";d.size=bytes.len() as u64;d.sha256=hex(&Sha256::digest(bytes));
        let mut out=Vec::new();copy_verified(bytes.as_slice(),&mut out,&d,|_|Ok(())).unwrap();assert_eq!(out,bytes);
        assert!(copy_verified(b"wrong".as_slice(),Vec::new(),&d,|_|Ok(())).is_err());
        assert!(copy_verified(bytes.as_slice(),Vec::new(),&d,|_|Err("cancelled".into())).is_err());
        assert!(copy_verified(b"test package extra".as_slice(),Vec::new(),&d,|_|Ok(())).is_err());
        let oxygen=catalog().iter().find(|p|p.name=="oXygen").unwrap();assert!(for_platform(oxygen,"macos-aarch64").is_none());
        assert_eq!(thumbnails().len(),75);assert!(thumbnails().iter().all(Option::is_some));
    }
    #[test]
    fn catalog_browser_search_and_layout(){
        let mut app=App::new(None);app.state.show_browser=true;app.browser_act(&Action::BrowserTab(TAB));
        assert_eq!(app.state.browser_tab,TAB);assert_eq!(rows(&app.state).len(),10);
        app.browser_act(&Action::BrowserRow(0));assert_eq!(rows(&app.state).len(),20);
        app.browser_act(&Action::BrowserRow(1));assert!(app.drag.is_none());assert_eq!(selected(&app.state),Some(0));
        app.state.browser_search="oxygen".into();assert_eq!(rows(&app.state).len(),2);assert_eq!(rows(&app.state)[1].label,"oXygen");
        app.state.browser_search.clear();
        for (width,height) in [(1280,900),(1000,600)]{
            app.state.screen=(width as f32,height as f32);let mut c=os_ui::Canvas::new(width,height,1.).unwrap();app.paint(&mut c,None);
            let actions:Vec<_>=app.hits.iter().filter(|h|matches!(h.action,Action::PluginDownload(_,0|3))).collect();assert!(actions.len()>=2);
            assert!(actions.iter().all(|h|h.rect.bottom()<=height as f32));
            let r=actions.iter().find(|h|h.action==Action::PluginDownload(1,0)).map(|h|h.rect);
            if let Some(r)=r {app.mouse_move(r.cx(),r.cy());app.mouse_down();app.mouse_up();assert_eq!(selected(&app.state),Some(1));assert!(app.drag.is_none());app.paint(&mut c,None);}
            app.browser_wheel(-20.);assert!(app.state.browser_scroll<=crate::views::browser::max_scroll(&app.state));
            app.state.browser_scroll=0.;
            if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(PathBuf::from(dir).join(format!("plugin-downloads-{height}.png"))).unwrap();}
        }
    }
    #[test]
    fn hover_information_is_delayed_and_never_downloads(){
        let mut app=App::new(None);app.state.show_browser=true;app.browser_act(&Action::BrowserTab(TAB));
        app.state.browser_search="nova".into();
        let mut c=os_ui::Canvas::new(1000,600,1.).unwrap();app.state.screen=(1000.,600.);app.paint(&mut c,None);
        let r=app.hits.iter().find(|h|h.action==Action::BrowserRow(1)).unwrap().rect;
        app.mouse_move(r.x+60.,r.cy());hover_tick(&mut app);
        let i=catalog().iter().position(|p|p.id=="nova").unwrap();
        assert_eq!(app.state.catalog_hover.map(|v|v.0),Some(i));assert_eq!(app.state.catalog_tooltip,None);
        app.state.catalog_hover=Some((i,std::time::Instant::now()-Duration::from_millis(500)));
        hover_tick(&mut app);assert_eq!(app.state.catalog_tooltip,Some(i));assert!(selected(&app.state).is_none());
        app.paint(&mut c,None);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(PathBuf::from(dir).join("plugin-hover-600.png")).unwrap();}
        app.browser_wheel(-1.);assert_eq!(app.state.catalog_tooltip,None);
        app.state.catalog_hover=Some((i,std::time::Instant::now()-Duration::from_secs(1)));
        app.mouse_move(900.,550.);hover_tick(&mut app);assert_eq!(app.state.catalog_hover,None);
    }
    #[test]
    #[ignore="Explicit official freeware download check, no installer executes"]
    fn freeware_download_roundtrip(){
        let dir=std::env::temp_dir().join(format!("lemon-freeware-check-{}",std::process::id()));
        for id in ["nova","span","ivgi","ott","protoverb","youlean","chorus"] {
            let p=catalog().iter().find(|p|p.id==id).unwrap();let d=for_platform(p,"windows-x86_64").unwrap();
            let path=fetch(d,&dir).unwrap_or_else(|e|panic!("{id}: {e}"));assert!(verified(&path,d));
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore="Explicit package integration test using already downloaded official archives; no plugin executes"]
    fn portable_packages_install_in_scratch(){
        let packages=PathBuf::from(std::env::var_os("LEMON_CATALOG_CACHE").expect("set package cache"));
        let root=std::env::temp_dir().join(format!("lemon-portable-check-{}",std::process::id()));
        let mut count=0;
        for p in catalog(){for d in &p.downloads{if d.install.is_empty(){continue}
            let source=packages.join(format!("{}-{}",&hex(&Sha256::digest(d.url.as_bytes()))[..16],d.filename));
            assert!(verified(&source,d),"{} package",p.name);
            let dest=install_portable(&source,d,&p.id,&root).unwrap_or_else(|e|panic!("{}: {e}",p.name));
            for member in &d.install{assert!(dest.join(member.rsplit('/').next().unwrap()).exists());}
            assert_eq!(install_portable(&source,d,&p.id,&root).unwrap(),dest,"repeat must not overwrite");count+=1;
        }}assert_eq!(count,18);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore="Explicit real download check; does not launch anything or use the real profile"]
    fn official_download_roundtrip(){
        let dir=std::env::temp_dir().join(format!("lemon-catalog-check-{}",std::process::id()));
        for name in ["Airwindows TapeDelay2","oXygen","SocaLabs Compressor"]{
            let p=catalog().iter().find(|p|p.name==name).unwrap();let d=for_platform(p,"windows-x86_64").unwrap();
            let path=fetch(d,&dir).unwrap_or_else(|e|panic!("{name}: {e}"));assert!(verified(&path,d));assert_eq!(fetch(d,&dir).unwrap(),path);
        }fs::remove_dir_all(dir).unwrap();
    }
}
