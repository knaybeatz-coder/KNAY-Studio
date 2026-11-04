// SPDX-License-Identifier: GPL-3.0-or-later
//! Release-native engine, carried inside the executable for backwards-compatible updates.
use std::{fs, io::{Cursor, Read}, path::{Path, PathBuf}, sync::OnceLock};
use sha2::{Digest, Sha256};
use serde::Deserialize;
const BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/master-engine.zip"));
#[derive(Deserialize)]
struct Manifest { os: String, arch: String, files: Vec<Entry> }
#[derive(Deserialize)]
struct Entry { path: String, sha256: String, size: u64 }
pub fn is_bundled() -> bool { !BYTES.is_empty() }
pub fn bundle_path() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let root = os_paths::user_dir()?.join("Builtin Plugins");
        match unpack(BYTES, &root) {
            Ok(p) => Some(p),
            Err(e) => { eprintln!("Master Assistant embedded engine: {e}"); None }
        }
    }).clone()
}
fn safe_path(name: &str) -> Result<&Path, String> {
    let p = Path::new(name);
    if name.contains('\\') || name.contains(':') || !name.starts_with("Lemon Master Assistant.vst3/")
        || !p.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err("Invalid engine path".into());
    }
    Ok(p)
}
fn verified(root: &Path, entries: &[Entry]) -> bool {
    entries.iter().all(|e| {
        let Ok(p) = safe_path(&e.path) else { return false };
        let path = root.join(p);
        if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() == e.size) { return false }
        fs::read(path).is_ok_and(|b| format!("{:x}", Sha256::digest(&b)) == e.sha256)
    })
}
fn unpack(bytes: &[u8], root: &Path) -> Result<PathBuf, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let manifest: Manifest = {
        let f = zip.by_name("engine.json").map_err(|e| e.to_string())?;
        serde_json::from_reader(f.take(1024 * 1024)).map_err(|e| e.to_string())?
    };
    if manifest.os != std::env::consts::OS || manifest.arch != std::env::consts::ARCH || manifest.files.is_empty() {
        return Err("Engine platform does not match this application".into());
    }
    if zip.len() != manifest.files.len() + 1 { return Err("Unexpected engine files".into()) }
    for e in &manifest.files { safe_path(&e.path)?; }
    let hash = format!("{:x}", Sha256::digest(bytes));
    let dest = root.join(&hash);
    if verified(&dest, &manifest.files) { return Ok(dest.join(super::FILE)) }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    // Each process writes its own staging directory. A complete validated directory is renamed atomically.
    let stage = root.join(format!("{hash}-{}.tmp", std::process::id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    for e in &manifest.files {
        if e.size > 256 * 1024 * 1024 { return Err("Engine file exceeds size limit".into()) }
        let mut f = zip.by_name(&e.path).map_err(|e| e.to_string())?;
        if f.size() != e.size || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Invalid engine entry".into());
        }
        let mut data = Vec::new();
        (&mut f).take(e.size + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
        if data.len() as u64 != e.size || format!("{:x}", Sha256::digest(&data)) != e.sha256 {
            return Err("Engine checksum mismatch".into());
        }
        let path = stage.join(safe_path(&e.path)?);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&path, data).map_err(|e| e.to_string())?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(f.unix_mode().unwrap_or(0o644) & 0o777)).map_err(|e| e.to_string())?;
        }
    }
    if let Err(e) = fs::rename(&stage, &dest) {
        if !verified(&dest, &manifest.files) { return Err(e.to_string()) }
    }
    Ok(dest.join(super::FILE))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_paths_reject_escape() {
        for p in ["../outside", "/absolute", "Lemon Master Assistant.vst3/../outside", "Lemon Master Assistant.vst3/C:evil"] {
            assert!(safe_path(p).is_err());
        }
        assert!(safe_path("Lemon Master Assistant.vst3/Contents/x86_64-win/Lemon Master Assistant.vst3").is_ok());
    }
    #[test]
    fn embedded_release_can_extract_and_reuse() {
        if BYTES.is_empty() { return; }
        let root = std::env::temp_dir().join(format!("lemon-engine-test-{}", std::process::id()));
        let first = unpack(BYTES, &root).unwrap();
        assert!(first.is_dir());
        assert_eq!(unpack(BYTES, &root).unwrap(), first);
        fs::remove_dir_all(root).unwrap();
    }
}
