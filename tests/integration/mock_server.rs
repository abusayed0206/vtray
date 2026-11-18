use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use vt_tray::api::client::{StaticKeyProvider, VtClient};
use vt_tray::core::cache::Cache;
use vt_tray::core::config::ConfigStore;
use vt_tray::core::quota::QuotaGate;
use vt_tray::worker::WorkerCore;

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl MockResponse {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.as_bytes().to_vec(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        }
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.as_bytes().to_vec(),
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    pub fn start(responses: Vec<MockResponse>) -> Self {
        Self::start_with(|_| responses)
    }

    pub fn start_with<F>(build: F) -> Self
    where
        F: FnOnce(&str) -> Vec<MockResponse>,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{address}");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let queue = Arc::new(Mutex::new(VecDeque::from(build(&base_url))));
        let thread_requests = Arc::clone(&requests);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let response = queue
                            .lock()
                            .ok()
                            .and_then(|mut queue| queue.pop_front())
                            .unwrap_or_else(|| MockResponse::text(500, "no scripted response"));
                        if let Some(request) = read_request(&mut stream)
                            && let Ok(mut requests) = thread_requests.lock()
                        {
                            requests.push(request);
                        }
                        write_response(&mut stream, response);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One isolated test environment: a temp dir holding `config.json` and an on-disk `cache.db`,
/// a scripted mock VirusTotal server, and a client pointed at it. Nothing touches the user's
/// profile, Credential Manager, or the real service.
pub struct TestEnv {
    pub dir: tempfile::TempDir,
    pub server: MockServer,
    pub client: Arc<VtClient>,
    pub cache: Arc<Cache>,
    pub quota: Arc<QuotaGate>,
    pub config: ConfigStore,
}

impl TestEnv {
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self::with(|_| responses)
    }

    /// `build` receives the server's base URL, for responses that point back at it.
    pub fn with<F>(build: F) -> Self
    where
        F: FnOnce(&str) -> Vec<MockResponse>,
    {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start_with(build);
        let keys = Arc::new(StaticKeyProvider::new(Some("test-key".to_string())));
        let client = Arc::new(VtClient::new(keys, server.base_url()));
        let cache = Arc::new(Cache::open(&dir.path().join("cache.db")).unwrap());
        let quota = Arc::new(QuotaGate::new(cache.clone()));
        let config = ConfigStore::new(dir.path().join("config.json"));
        Self {
            dir,
            server,
            client,
            cache,
            quota,
            config,
        }
    }

    pub fn core(&self) -> WorkerCore {
        WorkerCore::new(self.cache.clone(), self.client.clone(), self.quota.clone())
    }

    /// Writes a file inside the environment's temp dir and returns its path.
    pub fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// Opens the same database again, as the app does after a restart.
    pub fn reopen_cache(&self) -> Cache {
        Cache::open(&self.dir.path().join("cache.db")).unwrap()
    }
}

/// A unique `HKCU\Software\VTTrayTest-...` key for registry tests, deleted on drop, so tests
/// never touch the real Run or context-menu keys.
pub struct ScratchKey {
    pub path: String,
}

impl ScratchKey {
    pub fn new(name: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            // No shared parent key: parallel tests deleting a common parent raced with creation.
            path: format!(
                r"Software\VTTrayTest-{name}-{}-{unique}",
                std::process::id()
            ),
        }
    }

    pub fn hkcu() -> winreg::RegKey {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
    }

    /// Path of a key below the scratch root.
    pub fn child(&self, relative: &str) -> String {
        format!(r"{}\{relative}", self.path)
    }

    pub fn exists(&self, relative: &str) -> bool {
        Self::hkcu().open_subkey(self.child(relative)).is_ok()
    }
}

impl Drop for ScratchKey {
    fn drop(&mut self) {
        let _ = Self::hkcu().delete_subkey_all(&self.path);
    }
}

fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
        if buffer.len() > 128 * 1024 {
            return None;
        }
    };
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    let mut body = buffer[body_start..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    Some(RecordedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(stream: &mut TcpStream, response: MockResponse) {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "Response",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        reason,
        response.body.len()
    );
    for (name, value) in response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}
