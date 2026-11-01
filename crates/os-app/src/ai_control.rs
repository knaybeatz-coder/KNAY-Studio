// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in loopback MCP endpoint. All application commands execute on the UI thread.
//! No listener, worker, inference or analysis exists while disabled.
use crate::{
    app::{Action, App},
    dialog::Body,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    time::{Duration, Instant},
};
const VERSION: &str = "2025-06-18";
const LIMIT: usize = 262144;
fn random() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    Ok(b.iter().map(|b| format!("{b:02x}")).collect())
}
fn private_write(path: &std::path::Path, content: String) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        if let Some(parent) = path.parent() {
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(content.as_bytes())
}
fn root() -> Result<PathBuf, String> {
    os_paths::user_dir()
        .map(|p| p.join("AI control"))
        .ok_or("User folder unavailable".into())
}
#[derive(Default)]
pub struct UiState {
    pub enabled: bool,
    pub client: String,
    pub pending: String,
    pub remember: bool,
    pub status: String,
    pub address: String,
    pub calls: u64,
}
pub struct Request {
    rpc: Value,
    session: String,
    reply: SyncSender<Response>,
    deadline: Instant,
}
struct Response {
    status: u16,
    body: Value,
    session: Option<String>,
}
impl Response {
    fn result(id: Value, result: Value) -> Self {
        Self {
            status: 200,
            body: json!({"jsonrpc":"2.0","id":id,"result":result}),
            session: None,
        }
    }
    fn error(id: Value, code: i32, msg: &str) -> Self {
        Self {
            status: 200,
            body: json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":msg}}),
            session: None,
        }
    }
}
pub struct Runtime {
    rx: Receiver<Request>,
    stop: Arc<AtomicBool>,
    pending: Option<Request>,
    session: String,
    client: String,
    trusted: String,
    key: String,
    pub controls: Vec<(String, Action)>,
    pub serial: u64,
    pub await_paint: bool,
    pub last: Instant,
    rate_start: Instant,
    rate_count: u32,
    connection: Option<PathBuf>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(p) = &self.connection {
            let _ = std::fs::remove_file(p);
        }
    }
}
fn serve(listener: TcpListener, key: String) -> Result<Runtime, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let off = stop.clone();
    let (tx, rx) = mpsc::sync_channel(8);
    let count = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        while !off.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    if !addr.ip().is_loopback() {
                        continue;
                    }
                    if count.fetch_add(1, Ordering::AcqRel) >= 4 {
                        count.fetch_sub(1, Ordering::Release);
                        let _ = write_http(
                            &mut stream,
                            Response {
                                status: 503,
                                body: json!({"error":"Busy"}),
                                session: None,
                            },
                        );
                        continue;
                    }
                    let (tx, key, off, count) =
                        (tx.clone(), key.clone(), off.clone(), count.clone());
                    std::thread::spawn(move || {
                        let _ = handle(&mut stream, &key, &tx, &off);
                        count.fetch_sub(1, Ordering::Release);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(_) => break,
            }
        }
    });
    Ok(Runtime {
        rx,
        stop,
        pending: None,
        session: String::new(),
        client: String::new(),
        trusted: String::new(),
        key: String::new(),
        controls: vec![],
        serial: 0,
        await_paint: false,
        last: Instant::now(),
        rate_start: Instant::now(),
        rate_count: 0,
        connection: None,
    })
}
fn write_http(s: &mut TcpStream, r: Response) -> std::io::Result<()> {
    let body = if r.status == 202 {
        String::new()
    } else {
        r.body.to_string()
    };
    let session = r
        .session
        .map(|v| format!("Mcp-Session-Id: {v}\r\n"))
        .unwrap_or_default();
    write!(s,"HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n{}\r\n{}",r.status,if r.status<400{"OK"}else{"Error"},body.len(),session,body)
}
fn denied(status: u16) -> Response {
    Response {
        status,
        body: json!({"error":"AI connection refused or unavailable"}),
        session: None,
    }
}
fn secure_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |v, (a, b)| v | (a ^ b)) == 0
}
fn handle(
    s: &mut TcpStream,
    key: &str,
    tx: &SyncSender<Request>,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let end = loop {
        let mut b = [0; 1];
        if s.read(&mut b)? == 0 {
            return Ok(());
        }
        bytes.push(b[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break bytes.len();
        }
        if bytes.len() > 8192 {
            return write_http(s, denied(413));
        }
    };
    let head = match std::str::from_utf8(&bytes[..end]) {
        Ok(h) => h,
        Err(_) => return write_http(s, denied(400)),
    };
    let mut lines = head.split("\r\n");
    let request = lines.next().unwrap_or("");
    let mut headers = std::collections::HashMap::new();
    for l in lines.filter(|l| !l.is_empty()) {
        let Some((k, v)) = l.split_once(':') else {
            return write_http(s, denied(400));
        };
        if headers
            .insert(k.trim().to_ascii_lowercase(), v.trim().to_string())
            .is_some()
        {
            return write_http(s, denied(400));
        }
    }
    let host = headers.get("host").map(String::as_str).unwrap_or("");
    let port = s.local_addr()?.port();
    if stop.load(Ordering::Acquire)
        || headers.contains_key("origin")
        || !(host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}"))
    {
        return write_http(s, denied(403));
    }
    if !secure_eq(
        headers
            .get("authorization")
            .map(String::as_str)
            .unwrap_or(""),
        &format!("Bearer {key}"),
    ) {
        return write_http(s, denied(401));
    }
    if request != "POST /mcp HTTP/1.1" {
        return write_http(s, denied(405));
    }
    if headers.contains_key("transfer-encoding")
        || !headers
            .get("content-type")
            .is_some_and(|s| s.starts_with("application/json"))
    {
        return write_http(s, denied(400));
    }
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|v| !matches!(v.as_str(), "2025-06-18" | "2025-03-26" | "2024-11-05"))
    {
        return write_http(s, denied(400));
    }
    let len = match headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
    {
        Some(n) if n <= LIMIT => n,
        _ => return write_http(s, denied(413)),
    };
    let mut body = vec![0; len];
    s.read_exact(&mut body)?;
    let rpc: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return write_http(s, denied(400)),
    };
    if rpc.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || !rpc.get("method").is_some_and(Value::is_string)
    {
        return write_http(s, denied(400));
    }
    let id = rpc.get("id").cloned().unwrap_or(Value::Null);
    let notification = rpc.get("id").is_none();
    let (reply, rx) = mpsc::sync_channel(1);
    let req = Request {
        rpc,
        session: headers.get("mcp-session-id").cloned().unwrap_or_default(),
        reply,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    if tx.try_send(req).is_err() {
        return write_http(s, denied(503));
    }
    let start = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            return write_http(s, denied(403));
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(mut r) => {
                if notification && r.status == 200 {
                    r.status = 202;
                }
                return write_http(s, r);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return write_http(s, denied(503)),
            Err(_) => {}
        }
        if start.elapsed() > Duration::from_secs(61) {
            return write_http(
                s,
                Response::error(
                    id,
                    -32000,
                    "Approval or command timed out. Reconnect from your assistant.",
                ),
            );
        }
    }
}
pub fn open(app: &mut App) {
    if !app.state.dialogs.iter().any(|m| matches!(m.body, Body::Ai)) {
        app.show_form(Body::Ai);
    }
}
pub fn enable(app: &mut App) {
    if app.ai_runtime.is_some() {
        disable(app);
        return;
    }
    let result = (|| -> Result<Runtime, String> {
        let root = root()?;
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let remembered = std::fs::read_to_string(root.join("remembered.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let trusted = remembered
            .as_ref()
            .and_then(|v| v["client"].as_str())
            .unwrap_or("")
            .to_string();
        let key = match remembered
            .as_ref()
            .and_then(|v| v["key"].as_str())
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            Some(s) => s.to_string(),
            None => random()?,
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let address = format!(
            "http://{}/mcp",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let path = root.join("connection.json");
        let mut runtime = serve(listener, key.clone())?;
        runtime.key = key.clone();
        runtime.trusted = trusted;
        private_write(&path, json!({"url":address,"key":key}).to_string())
            .map_err(|e| e.to_string())?;
        runtime.connection = Some(path);
        app.state.ai.address = address;
        Ok(runtime)
    })();
    match result {
        Ok(r) => {
            app.ai_runtime = Some(r);
            app.state.ai.enabled = true;
            app.state.ai.status =
                "Ready for an assistant. Save the setup file, then connect from your AI app."
                    .into();
        }
        Err(e) => app.state.ai.status = format!("Could not enable AI: {e}"),
    };
    app.invalidate();
}
pub fn disable(app: &mut App) {
    app.state.ai.remember = false;
    app.ai_runtime = None;
    app.state.ai.enabled = false;
    app.state.ai.client.clear();
    app.state.ai.pending.clear();
    app.state.ai.address.clear();
    app.state.ai.status = "AI control is off. All connections have been revoked.".into();
    app.invalidate();
}
pub fn forget(app: &mut App) {
    disable(app);
    if let Ok(r) = root() {
        let _ = std::fs::remove_file(r.join("remembered.json"));
    }
    app.state.ai.status = "Assistant forgotten. New connections will need your approval.".into();
}
pub fn setup(app: &mut App) {
    let result = (|| -> Result<PathBuf, String> {
        if app.ai_runtime.is_none() {
            return Err("Enable AI control first.".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let desktop = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .ok_or("Home folder unavailable")?
            .join("Desktop");
        let dir = desktop.join("Lemon Studio AI Setup");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(if exe.to_string_lossy().contains("development") {
            "Development MCP.json"
        } else {
            "Personal MCP.json"
        });
        let config = json!({"mcpServers":{"lemon-studio":{"command":exe,"args":["--ai-mcp",root()?.join("connection.json")]}}});
        std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap())
            .map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("Read me.txt"),
            include_str!("../assets/ai-setup.txt"),
        )
        .map_err(|e| e.to_string())?;
        Ok(path)
    })();
    app.state.ai.status = match result {
        Ok(p) => format!("Setup saved: {}", p.display()),
        Err(e) => e,
    };
    app.invalidate();
}
fn initialize(req: Request, r: &mut Runtime) {
    let sid = match random() {
        Ok(s) => s,
        Err(e) => {
            let _ = req
                .reply
                .send(Response::error(req.rpc["id"].clone(), -32603, &e));
            return;
        }
    };
    r.session = sid.clone();
    r.client = req.rpc["params"]["clientInfo"]["name"]
        .as_str()
        .unwrap_or("Assistant")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    r.last = Instant::now();
    r.controls.clear();
    let mut reply = Response::result(
        req.rpc["id"].clone(),
        json!({"protocolVersion":VERSION,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"Lemon Studio","version":"1.0"},"instructions":"Control the current Lemon Studio project through semantic tools. Fetch state before editing. UI control IDs expire after a command. Preserve user work, use existing dialogs for file actions. Plugin access is limited to host-exposed parameters. AI approval and permissions always require the human."}),
    );
    reply.session = Some(sid);
    let _ = req.reply.send(reply);
}
pub fn approve(app: &mut App, allow: bool) {
    let Some(r) = app.ai_runtime.as_mut() else {
        return;
    };
    let Some(req) = r.pending.take() else { return };
    app.state.ai.pending.clear();
    if !allow || req.deadline < Instant::now() {
        let _ = req.reply.send(Response::error(
            req.rpc["id"].clone(),
            -32001,
            "Connection not approved",
        ));
        app.state.ai.status = "Connection declined.".into();
        app.invalidate();
        return;
    }
    initialize(req, r);
    app.state.ai.client = r.client.clone();
    app.state.ai.status = "Assistant connected. You can turn AI control off at any time.".into();
    if app.state.ai.remember {
        r.trusted = r.client.clone();
        if let Ok(root) = root() {
            if private_write(
                &root.join("remembered.json"),
                json!({"client":r.trusted,"key":r.key}).to_string(),
            )
            .is_err()
            {
                app.state.ai.status =
                    "Connected for this session; could not remember the assistant.".into();
            }
        }
    }
    if matches!(app.state.dialogs.last().map(|m| &m.body), Some(Body::Ai)) {
        app.state.dialogs.pop();
        app.modal_changed();
    }
    app.invalidate();
}
pub fn tick(app: &mut App) {
    if app.ai_runtime.is_none() {
        return;
    }
    if app
        .ai_runtime
        .as_ref()
        .and_then(|r| r.pending.as_ref())
        .is_some_and(|r| r.deadline < Instant::now())
    {
        approve(app, false);
    }
    if app
        .ai_runtime
        .as_ref()
        .is_some_and(|r| !r.session.is_empty() && r.last.elapsed() > Duration::from_secs(900))
    {
        let r = app.ai_runtime.as_mut().unwrap();
        r.session.clear();
        r.controls.clear();
        app.state.ai.client.clear();
        app.state.ai.status =
            "Assistant session expired after 15 minutes without requests. Reconnect to continue."
                .into();
        app.invalidate();
    }
    for _ in 0..4 {
        let req = match app.ai_runtime.as_ref().unwrap().rx.try_recv() {
            Ok(r) => r,
            Err(_) => break,
        };
        if req.deadline < Instant::now() {
            continue;
        }
        let id = req.rpc["id"].clone();
        let method = req.rpc["method"].as_str().unwrap_or("");
        if method == "initialize" {
            let name = req.rpc["params"]["clientInfo"]["name"]
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect::<String>();
            let r = app.ai_runtime.as_mut().unwrap();
            if name.is_empty() {
                let _ = req
                    .reply
                    .send(Response::error(id, -32602, "clientInfo.name is required"));
                continue;
            }
            if r.pending.is_some() {
                let _ = req.reply.send(Response::error(
                    id,
                    -32000,
                    "Another connection is awaiting approval",
                ));
                continue;
            }
            if name == r.trusted {
                initialize(req, r);
                app.state.ai.client = r.client.clone();
                app.state.ai.status = "Remembered assistant reconnected.".into();
                app.invalidate();
            } else {
                app.state.ai.pending = name;
                r.pending = Some(req);
                open(app);
                app.invalidate();
            }
            continue;
        }
        let r = app.ai_runtime.as_mut().unwrap();
        if r.session.is_empty() || !secure_eq(&req.session, &r.session) {
            let _ = req.reply.send(denied(404));
            continue;
        }
        r.last = Instant::now();
        let result = match method {
            "notifications/initialized" | "notifications/cancelled" => {
                Response::result(id, json!({}))
            }
            "ping" => Response::result(id, json!({})),
            "tools/list" => Response::result(id, json!({"tools":crate::ai_commands::tools()})),
            "tools/call" => {
                if r.rate_start.elapsed() > Duration::from_secs(1) {
                    r.rate_start = Instant::now();
                    r.rate_count = 0;
                }
                r.rate_count += 1;
                if r.rate_count > 20 {
                    let _ = req.reply.send(Response::error(
                        id,
                        -32000,
                        "Too many commands; retry after one second",
                    ));
                    continue;
                }

                let name = req.rpc["params"]["name"].as_str().unwrap_or("");
                let args = &req.rpc["params"]["arguments"];
                let output = crate::ai_commands::call(app, name, args);
                app.state.ai.calls += 1;
                let (is_error, text) = match output {
                    Ok(v) => (false, v.to_string()),
                    Err(e) => (true, e),
                };
                Response::result(
                    id,
                    json!({"isError":is_error,"content":[{"type":"text","text":text}]}),
                )
            }
            _ => Response::error(id, -32601, "Unknown MCP method"),
        };
        let _ = req.reply.send(result);
    }
}
/// MCP stdio adapter: starts no DAW, audio device or model. Credentials stay in the user's local profile.
pub fn bridge(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("Missing Lemon Studio connection file");
        return 2;
    };
    let input = std::io::stdin();
    let mut output = std::io::stdout();
    let mut session = String::new();
    let mut input = input.lock();
    loop {
        let mut line = String::new();
        match (&mut input).take((LIMIT + 1) as u64).read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        if line.len() > LIMIT {
            return 2;
        }
        let rpc: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = rpc.get("id").cloned();
        let result = (|| -> Result<Option<Value>, String> {
            let text = std::fs::read_to_string(path)
                .map_err(|_| "Open Lemon Studio and enable AI control first.".to_string())?;
            let config: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let url = config["url"].as_str().ok_or("Missing endpoint")?;
            if !url
                .strip_prefix("http://127.0.0.1:")
                .and_then(|s| s.strip_suffix("/mcp"))
                .and_then(|s| s.parse::<u16>().ok())
                .is_some_and(|p| p > 0)
            {
                return Err("Only a local Lemon Studio endpoint is allowed".into());
            }
            let mut request = ureq::AgentBuilder::new()
                .redirects(0)
                .timeout(Duration::from_secs(65))
                .build()
                .post(url)
                .set("Content-Type", "application/json")
                .set("Accept", "application/json, text/event-stream")
                .set(
                    "Authorization",
                    &format!(
                        "Bearer {}",
                        config["key"].as_str().ok_or("Missing connection key")?
                    ),
                )
                .set("MCP-Protocol-Version", VERSION);
            if !session.is_empty() {
                request = request.set("Mcp-Session-Id", &session);
            }
            let response=request.send_string(&line).map_err(|_|"Lemon Studio is off, disconnected, or needs approval. Open AI in Lemon Studio, then reconnect.".to_string())?;
            if let Some(s) = response.header("Mcp-Session-Id") {
                session = s.into();
            }
            if response.status() == 202 {
                return Ok(None);
            }
            let text = response.into_string().map_err(|e| e.to_string())?;
            let value = serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?;
            Ok(Some(value))
        })();
        let answer = match result {
            Ok(v) => v,
            Err(e) => id
                .as_ref()
                .map(|id| json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":e}})),
        };
        if id.is_some() {
            if let Some(answer) = answer {
                if writeln!(output, "{answer}")
                    .and_then(|_| output.flush())
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_commands::call;
    fn app() -> App {
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        a.state.dialogs.clear();
        a.state.pattern = 1;
        a
    }
    fn runtime(a: &mut App) -> (String, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let key = random().unwrap();
        a.ai_runtime = Some(serve(listener, key.clone()).unwrap());
        a.state.ai.enabled = true;
        (url, key)
    }
    fn request(
        url: String,
        key: String,
        session: String,
        rpc: Value,
        origin: bool,
    ) -> std::thread::JoinHandle<(u16, Value, String)> {
        std::thread::spawn(move || {
            let mut r = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()
                .post(&url)
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {key}"))
                .set("Mcp-Session-Id", &session);
            if origin {
                r = r.set("Origin", "https://example.com");
            }
            let response = match r.send_string(&rpc.to_string()) {
                Ok(r) => r,
                Err(ureq::Error::Status(_, r)) => r,
                Err(e) => panic!("{e}"),
            };
            let status = response.status();
            let sid = response.header("Mcp-Session-Id").unwrap_or("").to_string();
            let text = response.into_string().unwrap();
            (
                status,
                serde_json::from_str(&text).unwrap_or(Value::Null),
                sid,
            )
        })
    }
    fn wait(a: &mut App, h: std::thread::JoinHandle<(u16, Value, String)>) -> (u16, Value, String) {
        let start = Instant::now();
        while !h.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(6));
            tick(a);
            std::thread::sleep(Duration::from_millis(5));
        }
        h.join().unwrap()
    }
    fn init() -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":VERSION,"clientInfo":{"name":"Test assistant","version":"1"}}})
    }
    #[test]
    fn ai_control_auth_approval_commands_and_revocation() {
        let mut a = app();
        assert!(a.ai_runtime.is_none());
        assert!(!a.state.ai.enabled);
        assert!(call(&mut a, "lemon_state", &json!({})).is_err());
        let (url, key) = runtime(&mut a);
        let h = request(url.clone(), "wrong".into(), "".into(), init(), false);
        assert_eq!(wait(&mut a, h).0, 401);
        let h = request(url.clone(), key.clone(), "".into(), init(), true);
        assert_eq!(wait(&mut a, h).0, 403);
        let h = request(url.clone(), key.clone(), "".into(), init(), false);
        let start = Instant::now();
        while a.state.ai.pending.is_empty() {
            tick(&mut a);
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(a.ai_runtime.as_ref().unwrap().session.is_empty());
        assert!(!h.is_finished());
        assert!(call(&mut a, "lemon_text", &json!({"key":"enter"})).is_err());
        approve(&mut a, false);
        assert!(wait(&mut a, h).1.get("error").is_some());
        a.state.dialogs.clear();
        let h = request(url.clone(), key.clone(), "".into(), init(), false);
        let start = Instant::now();
        while a.state.ai.pending.is_empty() {
            tick(&mut a);
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        approve(&mut a, true);
        let (status, result, sid) = wait(&mut a, h);
        assert_eq!(status, 200);
        assert_eq!(result["result"]["protocolVersion"], VERSION);
        assert_eq!(sid.len(), 64);
        assert!(!a.modal_open());
        let rpc = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lemon_transport","arguments":{"op":"tempo","value":123}}});
        let h = request(url.clone(), key.clone(), "wrong".into(), rpc.clone(), false);
        assert_eq!(wait(&mut a, h).0, 404);
        assert_ne!(a.state.tempo, 123.);
        let h = request(url.clone(), key.clone(), sid.clone(), rpc, false);
        let r = wait(&mut a, h);
        assert_eq!(r.1["result"]["isError"], false);
        assert_eq!(a.state.tempo, 123.);
        let h = request(
            url.clone(),
            key.clone(),
            sid.clone(),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
            false,
        );
        assert_eq!(
            wait(&mut a, h).1["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            16
        );
        open(&mut a);
        assert!(call(
            &mut a,
            "lemon_transport",
            &json!({"op":"tempo","value":140})
        )
        .is_err());
        assert_eq!(a.state.tempo, 123.);
        let stop = a.ai_runtime.as_ref().unwrap().stop.clone();
        disable(&mut a);
        assert!(stop.load(Ordering::Acquire));
        assert!(a.ai_runtime.is_none());
        assert!(call(
            &mut a,
            "lemon_transport",
            &json!({"op":"tempo","value":140})
        )
        .is_err());
        assert_eq!(a.state.tempo, 123.);
    }
    #[test]
    fn ai_control_edit_undo_validation_and_roundtrip() {
        let mut a = app();
        runtime(&mut a);
        let before = a.state.project.clone();
        call(
            &mut a,
            "lemon_notes",
            &json!({"op":"add","row":0,"position":96,"length":48,"key":60,"velocity":100}),
        )
        .unwrap();
        assert_eq!(a.state.project.pattern(1).unwrap().notes.len(), 1);
        assert!(call(
            &mut a,
            "lemon_notes",
            &json!({"op":"add","row":0,"position":96,"length":0,"key":60,"velocity":100})
        )
        .is_err());
        call(&mut a, "lemon_history", &json!({"op":"undo"})).unwrap();
        assert_eq!(a.state.project, before);
        call(&mut a, "lemon_history", &json!({"op":"redo"})).unwrap();
        assert_eq!(a.state.project.pattern(1).unwrap().notes[0].key, 60);
        call(&mut a,"lemon_clips",&json!({"op":"add","source_kind":"pattern","source":1,"track":2,"position":0,"length":192})).unwrap();
        call(
            &mut a,
            "lemon_clips",
            &json!({"op":"move","index":0,"track":3,"position":96}),
        )
        .unwrap();
        assert_eq!(crate::playlist::clips(&a.state)[0].track, 3);
        a.state.mixer.sel.push(2);
        call(
            &mut a,
            "lemon_mixer",
            &json!({"op":"sidechain","track":1,"destination":3}),
        )
        .unwrap();
        assert!(a.state.project.mixer[1].routes.contains(&3));
        assert!(!a.state.project.mixer[2].routes.contains(&3));
        call(
            &mut a,
            "lemon_mixer",
            &json!({"op":"volume","track":1,"value":0.6}),
        )
        .unwrap();
        assert_eq!(a.state.project.mixer[1].volume, Some(0.6));
        assert!(call(
            &mut a,
            "lemon_mixer",
            &json!({"op":"volume","track":1,"value":2.})
        )
        .is_err());
        let bytes = os_flp::project::create(&a.state.project)
            .unwrap()
            .to_bytes()
            .unwrap();
        let loaded = os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
        assert_eq!(loaded.pattern(1).unwrap().notes[0].key, 60);
        assert_eq!(loaded.arrangement().unwrap().clips[0].track, 3);
        assert!((loaded.mixer[1].volume.unwrap() - 0.6).abs() < 0.001);
        disable(&mut a);
    }
    #[test]
    fn ai_control_expired_requests_and_rate_limit() {
        let mut a = app();
        runtime(&mut a);
        let (tx, rx) = mpsc::sync_channel(8);
        a.ai_runtime.as_mut().unwrap().rx = rx;
        a.ai_runtime.as_mut().unwrap().session = "session".into();
        let request = |deadline| {
            let (reply, rx) = mpsc::sync_channel(1);
            (
                Request {
                    rpc: json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lemon_transport","arguments":{"op":"tempo","value":99}}}),
                    session: "session".into(),
                    reply,
                    deadline,
                },
                rx,
            )
        };
        let (req, _) = request(Instant::now() - Duration::from_secs(1));
        tx.send(req).unwrap();
        tick(&mut a);
        assert_ne!(a.state.tempo, 99.);
        a.ai_runtime.as_mut().unwrap().rate_count = 20;
        let (req, rx) = request(Instant::now() + Duration::from_secs(10));
        tx.send(req).unwrap();
        tick(&mut a);
        assert!(rx.recv().unwrap().body.get("error").is_some());
        assert_ne!(a.state.tempo, 99.);
        let (req, _) = request(Instant::now() + Duration::from_secs(10));
        tx.send(req).unwrap();
        disable(&mut a);
        tick(&mut a);
        assert_ne!(a.state.tempo, 99.);
    }
    #[test]
    fn ai_control_toolbar_dialog_and_command_discovery() {
        let mut a = app();
        let mut canvas = os_ui::Canvas::new(1280, 720, 1.).unwrap();
        a.paint(&mut canvas, None);
        assert!(a.hits.iter().any(|h| h.action == Action::Ai));
        assert!(a.ai_runtime.is_none());
        a.act(Action::Ai);
        a.paint(&mut canvas, None);
        assert!(a
            .hits
            .iter()
            .any(|h| h.action == Action::Dlg(crate::dialog::DHit::AiEnable)));
        if let Ok(p) = std::env::var("LEMON_AI_SHOT") {
            canvas.pixmap.save_png(std::path::Path::new(&p)).unwrap();
        }
        runtime(&mut a);
        let controls = call(&mut a, "lemon_controls", &json!({})).unwrap();
        assert!(!controls.to_string().contains("AiEnable"));
        assert!(call(&mut a, "lemon_text", &json!({"key":"escape"})).is_err());
        a.state.dialogs.clear();
        call(&mut a, "lemon_menu", &json!({"kind":"main","index":0})).unwrap();
        a.paint(&mut canvas, None);
        let controls = call(&mut a, "lemon_controls", &json!({})).unwrap();
        assert!(
            controls["controls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["label"].as_str().unwrap().contains("Save")),
            "{controls}"
        );
        call(
            &mut a,
            "lemon_transport",
            &json!({"op":"tempo","value":110}),
        )
        .unwrap();
        assert!(call(
            &mut a,
            "lemon_action",
            &json!({"id":controls["controls"][0]["id"]})
        )
        .is_err());
        disable(&mut a);
    }
}
