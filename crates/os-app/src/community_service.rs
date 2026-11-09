use base64::Engine;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const BASE: &str = "https://lemon-studio-exchange-api.lemon-community-8808aa.workers.dev";
const LIMIT: usize = 5 * 1024 * 1024;
fn transport() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .user_agent("LemonStudio-Community/1")
            .redirects(0)
            .timeout(Duration::from_secs(40))
            .build()
    })
}
fn response(r: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, String> {
    match r {
        Ok(r) => Ok(r),
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            Err(serde_json::from_str::<String>(&body)
                .unwrap_or_else(|_| format!("Community service returned {code}.")))
        }
        Err(e) => Err(format!("Community connection failed: {e}")),
    }
}
fn ticket() -> Result<String, String> {
    static SESSION: Mutex<Option<(Instant, String)>> = Mutex::new(None);
    let mut guard = SESSION
        .lock()
        .map_err(|_| "Community session unavailable")?;
    if let Some((at, token)) = guard.as_ref() {
        if at.elapsed() < Duration::from_secs(23 * 3600) {
            return Ok(token.clone());
        }
    }
    let body = response(transport().post(&format!("{BASE}/desktop/session")).call())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let token = value["ticket"]
        .as_str()
        .ok_or("Invalid community session")?
        .to_owned();
    *guard = Some((Instant::now(), token.clone()));
    Ok(token)
}
fn api(path: &str, body: Option<&Value>, member: &str) -> Result<ureq::Response, String> {
    let req = transport()
        .request(
            if body.is_some() { "POST" } else { "GET" },
            &format!("{BASE}{path}"),
        )
        .set("X-Lemon-Desktop", &ticket()?)
        .set("X-Member", member);
    response(if let Some(v) = body {
        req.set("Content-Type", "application/json")
            .send_string(&v.to_string())
    } else {
        req.call()
    })
}
fn sample_path(root: &Path, item: &Value) -> Result<PathBuf, String> {
    let id = item["id"].as_str().ok_or("Missing sample ID")?;
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid sample ID".into());
    }
    let name: String = item["title"]
        .as_str()
        .unwrap_or("Sample")
        .chars()
        .take(60)
        .map(|c| {
            if c.is_ascii_alphanumeric() || " -_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(root.join(format!("{name} - {id}.wav")))
}
fn canonical_wave(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return false;
    }
    let mut i = 12;
    while let Some(header) = bytes.get(i..i + 8) {
        let n = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let Some(chunk) = bytes.get(i + 8..i + 8 + n) else {
            return false;
        };
        if &header[..4] == b"fmt " && n >= 16 {
            let mut tag = u16::from_le_bytes([chunk[0], chunk[1]]);
            if tag == 65534 && n >= 40 {
                tag = u16::from_le_bytes([chunk[24], chunk[25]]);
            }
            let bits = u16::from_le_bytes([chunk[14], chunk[15]]);
            return (tag == 1 && matches!(bits, 8 | 16 | 24 | 32))
                || (tag == 3 && matches!(bits, 32 | 64));
        }
        i += 8 + n + n % 2;
    }
    false
}
fn upload_audio(path: &Path) -> Result<Vec<u8>, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > LIMIT as u64 {
        return Err("Community samples must be 5 MB or smaller.".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if canonical_wave(&bytes) {
        return Ok(bytes);
    }
    let sample = os_engine::sample::load(path).map_err(|e| e.to_string())?;
    if sample.frames() * 8 + 44 > LIMIT {
        return Err("Decoded audio exceeds 5 MB. Choose a shorter excerpt.".into());
    }
    let mut wav = Vec::with_capacity(sample.frames() * 8 + 44);
    os_engine::wav::write_f32_stereo_header(&mut wav, sample.rate, sample.frames() as u64)
        .map_err(|e| e.to_string())?;
    for (l, r) in sample.left.iter().zip(&sample.right) {
        wav.extend(l.to_le_bytes());
        wav.extend(r.to_le_bytes());
    }
    Ok(wav)
}
pub fn run(action: &str, mut data: Value, root: PathBuf, member: String) -> Result<Value, String> {
    let root = root.join("Sample Exchange");
    if action == "fetch" {
        let item = &data["item"];
        let path = sample_path(&root, item)?;
        if !path.is_file() {
            let mut bytes = Vec::new();
            api(
                &format!("/wav/{}", item["id"].as_str().unwrap()),
                None,
                &member,
            )?
            .into_reader()
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
            if bytes.len() > LIMIT || !canonical_wave(&bytes) {
                return Err("Invalid community WAV.".into());
            }
            fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let tmp = path.with_extension(format!(
                "{:x}.tmp",
                getrandom::u64().map_err(|e| e.to_string())?
            ));
            let result = fs::write(&tmp, &bytes).and_then(|_| fs::rename(&tmp, &path));
            let _ = fs::remove_file(&tmp);
            if !path.is_file() {
                result.map_err(|e| e.to_string())?;
            }
        }
        return Ok(json!({"path":path}));
    }
    if action == "upload" {
        let path = data
            .as_object_mut()
            .and_then(|o| o.remove("path"))
            .ok_or("Choose audio first")?;
        data["wav"] = json!(
            base64::engine::general_purpose::STANDARD.encode(upload_audio(Path::new(
                path.as_str().ok_or("Invalid file")?
            ))?)
        );
    }
    let body = (!matches!(action, "catalog" | "me")).then_some(&data);
    let result = api(&format!("/{action}"), body, &member)?
        .into_string()
        .map_err(|e| e.to_string())?;
    let mut reply: Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    match action {
        "catalog" => {
            for item in reply.as_array_mut().ok_or("Invalid catalog")? {
                item["path"] = json!(sample_path(&root, item)?);
            }
            Ok(json!({"items":reply}))
        }
        "login" | "register" => Ok(json!({"token":reply["token"],"message":"Signed in."})),
        "me" => Ok(reply),
        _ => Ok(
            json!({"message":match action{"upload"=>"Sample uploaded.","vote"=>"Vote saved.","logout"=>"Signed out.",_=>"Preference saved."}}),
        ),
    }
}
