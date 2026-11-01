//! Signed, per-user Windows updates. Network and disk work never runs on the audio thread.
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const VERSION: u64 = 2026103104;
/// KNAY build: automatic updates from the Michael Lane feed are switched off, so a remote
/// release can never replace this customized lemonstudio.exe.
pub const ENABLED: bool = false;
pub const FEED: &str = "https://michaellanemusic.de/lemon-studio/updates/stable.json";
const ROOT: &str = "https://michaellanemusic.de/lemon-studio/updates/";
const PRODUCT: &str = "lemon-studio-michael-lane-windows-x64";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct File { pub name: String, pub sha256: String, pub size: u64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest { pub product: String, pub version: u64, pub label: String, pub notes: String, pub files: Vec<File> }
#[derive(Serialize, Deserialize)]
struct Signed { payload: String, signature: String }
#[derive(Clone, Default)]
struct Status { busy: bool, ready: Option<PathBuf>, message: String, started: bool, last: Option<Instant> }
static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();
fn status() -> &'static Mutex<Status> { STATUS.get_or_init(Default::default) }
fn hex(data: &[u8]) -> String { data.iter().map(|b| format!("{b:02x}")).collect() }
fn unhex(s: &str) -> Result<Vec<u8>> {
    if s.len()%2 != 0 || !s.is_ascii() { return Err("Invalid hexadecimal data".into()); }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i+2],16).map_err(Into::into)).collect()
}
fn public_key() -> Result<VerifyingKey> {
    Ok(VerifyingKey::from_bytes(&unhex(include_str!("update-public-key.hex").trim())?.try_into().map_err(|_| "Invalid public key")?)?)
}
fn allowed(name: &str) -> bool {
    matches!(name, "lemonstudio.exe" | "Lemon Studio Source.zip" | "THIRD-PARTY-NOTICES.txt" | "LICENSE")
}
fn verify_with(bytes: &[u8], key: &VerifyingKey) -> Result<Manifest> {
    verify_newer_than(bytes,key,VERSION)
}
fn verify_newer_than(bytes: &[u8], key: &VerifyingKey, minimum: u64) -> Result<Manifest> {
    if bytes.len() > 65536 { return Err("Update manifest too large".into()); }
    let signed: Signed = serde_json::from_slice(bytes)?;
    key.verify_strict(signed.payload.as_bytes(), &Signature::from_slice(&unhex(&signed.signature)?)?)?;
    let m: Manifest = serde_json::from_str(&signed.payload)?;
    if m.product != PRODUCT || m.version <= minimum || m.label.len()>80 || m.notes.len()>8000 { return Err("Wrong product or non-newer update".into()); }
    if m.files.len()!=4 { return Err("Incomplete update".into()); }
    let mut names = std::collections::HashSet::new();
    for file in &m.files {
        if !allowed(&file.name) || !names.insert(&file.name) || file.size==0 || file.size>512*1024*1024 || unhex(&file.sha256)?.len()!=32 { return Err("Invalid update file".into()); }
    }
    Ok(m)
}
fn verify(bytes: &[u8]) -> Result<Manifest> { verify_with(bytes, &public_key()?) }
fn checked_file(path: &Path, file: &File) -> Result<()> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len()!=file.size { return Err("Update file size/type mismatch".into()); }
    let mut input=fs::File::open(path)?; let mut hash=Sha256::new(); let mut block=[0u8;65536];
    loop { let n=input.read(&mut block)?; if n==0 {break} hash.update(&block[..n]); }
    if hex(&hash.finalize())!=file.sha256.to_ascii_lowercase() { return Err("Update checksum mismatch".into()); }
    Ok(())
}
fn cache() -> Result<PathBuf> {
    let root=std::env::var_os("LOCALAPPDATA").ok_or("No local application folder")?;
    let dir=PathBuf::from(root).join("Lemon Studio").join("Updates"); fs::create_dir_all(&dir)?; Ok(dir)
}
fn get(url: &str, max: u64) -> Result<Vec<u8>> {
    let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).redirects(0).build().get(url).call()?;
    let mut bytes=Vec::new(); response.into_reader().take(max+1).read_to_end(&mut bytes)?;
    if bytes.len() as u64>max {return Err("Download exceeds expected size".into())} Ok(bytes)
}
fn check() -> Result<Option<PathBuf>> {
    let bytes=get(FEED,65536)?;
    // Authenticate even a current/older release, before interpreting its version.
    let signed: Signed=serde_json::from_slice(&bytes)?;
    public_key()?.verify_strict(signed.payload.as_bytes(), &Signature::from_slice(&unhex(&signed.signature)?)?)?;
    let advertised: Manifest=serde_json::from_str(&signed.payload)?;
    if advertised.product!=PRODUCT { return Err("Wrong update product".into()); }
    if advertised.version<=VERSION {return Ok(None)}
    let manifest=verify(&bytes)?;
    let dir=cache()?.join(format!("{}-{}",manifest.version,std::process::id())); fs::create_dir_all(&dir)?;
    for file in &manifest.files {
        status().lock().unwrap().message=format!("Downloading {}",manifest.label);
        let dest=dir.join(&file.name);
        if checked_file(&dest,file).is_ok() {continue}
        let url=format!("{ROOT}{}/{}",manifest.version,file.name.replace(' ',"%20"));
        let data=get(&url,file.size)?;
        let part=dir.join(format!("{}.part",file.name));
        let mut out=fs::File::create(&part)?; out.write_all(&data)?; out.sync_all()?; drop(out);
        checked_file(&part,file)?; fs::rename(&part,&dest)?;
    }
    fs::write(dir.join("signed.json"),bytes)?;
    Ok(Some(dir))
}
pub fn start() { if ENABLED && cfg!(windows) { status().lock().unwrap().started=true; request(); } }
pub fn request() {
    let mut s=status().lock().unwrap();
    if !ENABLED { s.message="Automatic updates are turned off in this build.".into(); return; }
    if !cfg!(windows) { s.message="Automatic updates currently support Windows x64.".into(); return; }
    if s.busy || s.ready.is_some() { return; }
    s.busy=true; s.last=Some(Instant::now()); s.message="Checking for updates…".into(); drop(s);
    std::thread::spawn(|| {
        let result=check(); let mut s=status().lock().unwrap(); s.busy=false;
        match result {
            Ok(Some(dir)) => {s.ready=Some(dir);s.message="Update ready. Click to restart, or it will install when you close Lemon Studio.".into();}
            Ok(None) => s.message="Lemon Studio is up to date.".into(),
            Err(e) => s.message=format!("Update check failed: {e}. Click Update to retry."),
        }
    });
}
pub fn tick() {
    let s=status().lock().unwrap();
    let due=s.started && !s.busy && s.ready.is_none() && s.last.is_some_and(|v|v.elapsed()>Duration::from_secs(21600));
    drop(s); if due {request()}
}
pub fn ready() -> bool { status().lock().unwrap().ready.is_some() }
pub fn display() -> (&'static str,String) {
    let s=status().lock().unwrap();
    ("Update",s.message.clone())
}
pub fn report_result(message: &str) {
    if let Ok(dir)=cache() {let _=fs::write(dir.join("last-update-result.txt"),message);}
}
pub fn take_result() -> Option<String> {
    let file=cache().ok()?.join("last-update-result.txt");
    let message=fs::read_to_string(&file).ok()?;let _=fs::remove_file(file);Some(message)
}

/// Starts a separate copy of this trusted executable; it waits for our process to exit normally.
pub fn on_exit(restart: bool, project: Option<&Path>) -> Result<()> {
    let Some(stage)=status().lock().unwrap().ready.clone() else {return Ok(())};
    let exe=std::env::current_exe()?; let helper=stage.join("lemon-update-helper.exe");
    fs::copy(&exe,&helper)?;
    // The helper uses the same runtime DLLs as this build. Only copy beside it, never download/execute arbitrary helpers.
    for entry in fs::read_dir(exe.parent().ok_or("No install directory")?)? {
        let entry=entry?;
        if entry.path().extension().is_some_and(|s|s.eq_ignore_ascii_case("dll")) {fs::copy(entry.path(),stage.join(entry.file_name()))?;}
    }
    let mut cmd=std::process::Command::new(helper);
    cmd.arg("--apply-update").arg(std::process::id().to_string()).arg(&exe).arg(&stage).arg(if restart{"restart"}else{"close"});
    if let Some(path)=project {cmd.arg(path);}
    #[cfg(windows)] {use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
    let ready=stage.join("helper-ready"); let _=fs::remove_file(&ready);
    cmd.spawn()?;
    let until=Instant::now()+Duration::from_secs(5);
    while !ready.exists() && Instant::now()<until {std::thread::sleep(Duration::from_millis(20));}
    if !ready.exists(){return Err("Updater did not start; installation deferred".into())}
    Ok(())
}

/// Commit only known app files. Originals stay in a versioned rollback directory; user data is never part of the transaction.
fn install(stage: &Path, target: &Path, manifest: &Manifest) -> Result<()> {
    if target.file_name().and_then(|v|v.to_str())!=Some("lemonstudio.exe") {return Err("Unexpected application filename".into())}
    let root=target.parent().ok_or("No installation directory")?;
    let root=fs::canonicalize(root)?;
    for file in &manifest.files {
        checked_file(&stage.join(&file.name),file)?;
        let dest=root.join(&file.name);
        if dest.exists() { let meta=fs::symlink_metadata(&dest)?;if !meta.is_file() || meta.file_type().is_symlink(){return Err("Unsafe installation destination".into())} }
    }
    let backup=root.join(format!(".update-backup-{}-{}",manifest.version,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    // Never overwrite a prior rollback directory or follow a junction.
    fs::create_dir(&backup)?;
    let mut moved: Vec<(PathBuf,PathBuf,bool)>=Vec::new();
    let result: Result<()>=(|| {
        // Executable last. If it is locked by another session, roll all preceding changes back.
        let mut files=manifest.files.clone(); files.sort_by_key(|f|f.name=="lemonstudio.exe");
        for file in files {
            let dest=root.join(&file.name); let old=backup.join(&file.name); let existed=dest.exists();
            let temp=backup.join(format!("{}.new",file.name));
            fs::copy(stage.join(&file.name),&temp)?; checked_file(&temp,&file)?;
            if existed {fs::copy(&dest,&old)?;}
            moved.push((dest.clone(),old,existed));
            fs::rename(&temp,&dest)?;
        }
        Ok(())
    })();
    if let Err(error)=result {
        let mut recovery_errors=Vec::new();
        for (dest,old,existed) in moved.iter().rev() {
            if *existed {if let Err(e)=fs::rename(old,dest){recovery_errors.push(e.to_string());}}
            else if dest.exists() {if let Err(e)=fs::remove_file(dest){recovery_errors.push(e.to_string());}}
        }
        return Err(format!("Update failed: {error}; rollback errors: {recovery_errors:?}; backups: {}",backup.display()).into());
    }
    Ok(())
}
#[cfg(windows)]
fn wait_parent(pid: u32, stage: &Path) -> Result<()> {
    use windows_sys::Win32::{Foundation::{CloseHandle,WAIT_OBJECT_0},System::Threading::{OpenProcess,WaitForSingleObject,PROCESS_SYNCHRONIZE}};
    unsafe {let handle=OpenProcess(PROCESS_SYNCHRONIZE,0,pid); if handle.is_null(){return Err("Cannot identify the closing process".into())}
        fs::write(stage.join("helper-ready"),b"ready")?;
        let result=WaitForSingleObject(handle,120000);CloseHandle(handle);if result!=WAIT_OBJECT_0{return Err("Lemon Studio is still running; update deferred".into())}}
    Ok(())
}
pub fn helper(args: &[String]) -> i32 {
    let result: Result<()>=(|| {
        if args.len()<4 {return Err("Missing updater arguments".into())}
        let stage=PathBuf::from(&args[2]); let target=PathBuf::from(&args[1]);
        let manifest=verify(&fs::read(stage.join("signed.json"))?)?;
        #[cfg(windows)] wait_parent(args[0].parse()?, &stage)?;
        #[cfg(not(windows))] return Err("Windows updater only".into());
        install(&stage,&target,&manifest)?;
        fs::write(stage.join("result.txt"),format!("Installed {}",manifest.label))?;
        report_result(&format!("Lemon Studio updated to {}.",manifest.label));
        if args[3]=="restart" {let mut cmd=std::process::Command::new(&target);if let Some(p)=args.get(4){cmd.arg(p);}cmd.current_dir(target.parent().ok_or("No parent")?).spawn()?;}
        Ok(())
    })();
    if let Err(e)=result {report_result(&format!("Automatic update was not completed: {e}"));if let Some(dir)=args.get(2){let _=fs::write(Path::new(dir).join("result.txt"),format!("{e}"));} return 1} 0
}

/// Read-only release diagnostics, before any window, project or audio device is opened.
pub fn diagnostic(args: &[String]) -> i32 {
    let result: Result<String>=(|| {
        match args.first().map(String::as_str) {
            Some("--update-version") => Ok(VERSION.to_string()),
            Some("--check-update-feed") => Ok(format!("{}",if check()?.is_some(){"Update downloaded and verified"}else{"Signed feed verified; up to date"})),
            Some("--verify-update") => {
                let path=Path::new(args.get(1).ok_or("Manifest path required")?);
                let m=verify_newer_than(&fs::read(path)?,&public_key()?,0)?;
                for file in &m.files {checked_file(&path.parent().ok_or("No manifest folder")?.join(&file.name),file)?;}
                Ok(format!("Verified {} ({}), all {} files",m.label,m.version,m.files.len()))
            },
            _ => Err("Unknown diagnostic".into())
        }
    })();
    match result {Ok(s)=>{println!("{s}");0},Err(e)=>{eprintln!("{e}");1}}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer,SigningKey};
    /// KNAY build: no Update or Donate button, and nothing checks the feed.
    #[test]
    fn updater_is_off_and_its_buttons_are_gone() {
        use crate::app::{App,Action};
        assert!(!ENABLED);
        let mut app=App::new(None);
        start();
        assert!(!status().lock().unwrap().busy && !status().lock().unwrap().started);
        let mut canvas=os_ui::Canvas::new(1600,900,1.0).unwrap();app.paint(&mut canvas,None);
        assert!(!app.hits.iter().any(|h|h.action==Action::Updates || h.action==Action::Donate));
        app.act(Action::Updates);
        app.act(Action::Donate);
        assert!(!app.want_exit && !app.restart_update && app.state.dialogs.is_empty());
    }
    #[test]
    fn updater_signature_scope_and_transaction() {
        let key=SigningKey::from_bytes(&[19;32]);
        let root=std::env::temp_dir().join(format!("lemon-update-test-{}",std::process::id()));
        let stage=root.join("stage");let app=root.join("app"); fs::create_dir_all(&stage).unwrap();fs::create_dir_all(&app).unwrap();
        let files:Vec<_>=["lemonstudio.exe","Lemon Studio Source.zip","THIRD-PARTY-NOTICES.txt","LICENSE"].iter().map(|n| {
            fs::write(stage.join(n),b"new payload").unwrap();fs::write(app.join(n),b"old payload").unwrap();
            File{name:n.to_string(),size:11,sha256:hex(&Sha256::digest(b"new payload"))}
        }).collect();
        let mut m=Manifest{product:PRODUCT.into(),version:VERSION+1,label:"Test".into(),notes:"".into(),files};
        let signed=|m:&Manifest| {let payload=serde_json::to_string(m).unwrap();let signature=hex(&key.sign(payload.as_bytes()).to_bytes());serde_json::to_vec(&Signed{payload,signature}).unwrap()};
        let bytes=signed(&m);assert!(verify_with(&bytes,&key.verifying_key()).is_ok());
        assert!(verify_with(&bytes,&SigningKey::from_bytes(&[20;32]).verifying_key()).is_err());
        m.files[0].name="../escape.exe".into();assert!(verify_with(&signed(&m),&key.verifying_key()).is_err());m.files[0].name="lemonstudio.exe".into();
        m.product="another-product".into();assert!(verify_with(&signed(&m),&key.verifying_key()).is_err());m.product=PRODUCT.into();
        m.version=VERSION;assert!(verify_with(&signed(&m),&key.verifying_key()).is_err());m.version=VERSION+1;
        fs::write(app.join("song.flp"),b"keep my project").unwrap();
        fs::write(stage.join("LICENSE"),b"bad").unwrap();assert!(install(&stage,&app.join("lemonstudio.exe"),&m).is_err());assert_eq!(fs::read(app.join("lemonstudio.exe")).unwrap(),b"old payload");
        fs::write(stage.join("LICENSE"),b"new payload").unwrap();
        #[cfg(windows)] {
            use std::os::windows::fs::OpenOptionsExt;
            let locked=fs::OpenOptions::new().read(true).share_mode(0).open(app.join("lemonstudio.exe")).unwrap();
            assert!(install(&stage,&app.join("lemonstudio.exe"),&m).is_err());
            drop(locked);
            for file in &m.files {assert_eq!(fs::read(app.join(&file.name)).unwrap(),b"old payload", "failed update must roll back every file");}
        }
        install(&stage,&app.join("lemonstudio.exe"),&m).unwrap();
        assert_eq!(fs::read(app.join("lemonstudio.exe")).unwrap(),b"new payload");
        assert!(fs::read_dir(&app).unwrap().filter_map(std::result::Result::ok).any(|e|e.file_name().to_string_lossy().starts_with(".update-backup-") && fs::read(e.path().join("lemonstudio.exe")).is_ok_and(|b|b==b"old payload")));
        assert_eq!(fs::read(app.join("song.flp")).unwrap(),b"keep my project");
        fs::remove_dir_all(root).unwrap();
    }
}
