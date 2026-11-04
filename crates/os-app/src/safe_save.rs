//! Transactional project writes: failures before replacement leave the original untouched.
use std::{fs,io::{self,Write},path::Path,sync::atomic::{AtomicU64,Ordering}};
static NEXT:AtomicU64=AtomicU64::new(0);
pub fn write(path:&Path, bytes:&[u8])->io::Result<()> { replace(path,|f|f.write_all(bytes)) }
fn replace(path:&Path, fill:impl FnOnce(&mut fs::File)->io::Result<()>)->io::Result<()> {
 let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
 let temp=parent.join(format!(".lemon-save-{}-{}.tmp",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
 let result=(||{let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;fill(&mut f)?;f.sync_all()?;drop(f);fs::rename(&temp,path)})();
 if result.is_err(){let _=fs::remove_file(&temp);}result
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn audit_atomic_save_preserves_original_on_failure(){let d=std::env::temp_dir().join(format!("audit-safe-save-{}",std::process::id()));fs::create_dir_all(&d).unwrap();let p=d.join("song.lmn");fs::write(&p,b"old").unwrap();assert!(replace(&p,|f|{f.write_all(b"partial")?;Err(io::Error::other("disk full"))}).is_err());assert_eq!(fs::read(&p).unwrap(),b"old");write(&p,b"new").unwrap();assert_eq!(fs::read(&p).unwrap(),b"new");assert_eq!(fs::read_dir(&d).unwrap().count(),1);fs::remove_dir_all(d).unwrap();}
}
