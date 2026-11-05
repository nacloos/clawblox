use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const PROXY_THREAD_STACK_SIZE: usize = 256 * 1024;

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const ALLOWED_EXACT_PATHS: &[&str] = &[
    "/observe",
    "/observe/image",
    "/input",
    "/chat",
    "/chat/messages",
    "/api.md",
    "/skill.md",
    "/renderer/manifest",
];

const ALLOWED_PREFIXES: &[&str] = &["/renderer-files/", "/assets/", "/static/"];

/// the endpoints a world's API.md documents.
const API_ONLY_PATHS: &[&str] = &["/observe", "/input", "/chat", "/chat/messages", "/api.md"];

/// Which paths the proxy passes. The default is upstream's list. `api_only` narrows it
/// to the documented API, with `/observe/image` only when `allow_image` (a sighted run).
/// Set from `CLAWBLOX_PROXY_API_ONLY=1` and `CLAWBLOX_PROXY_ALLOW_IMAGE=1`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PathPolicy {
    pub api_only: bool,
    pub allow_image: bool,
}

impl PathPolicy {
    pub fn from_env() -> Self {
        let flag = |name: &str| std::env::var(name).map(|v| v == "1").unwrap_or(false);
        Self {
            api_only: flag("CLAWBLOX_PROXY_API_ONLY"),
            allow_image: flag("CLAWBLOX_PROXY_ALLOW_IMAGE"),
        }
    }
}

fn path_policy() -> PathPolicy {
    static POLICY: std::sync::OnceLock<PathPolicy> = std::sync::OnceLock::new();
    *POLICY.get_or_init(PathPolicy::from_env)
}
const DENIED_PREFIXES: &[&str] = &["/api/", "/replay/", "/sessions/"];

#[derive(Clone, Debug)]
pub struct WorldCapabilityProxyConfig {
    pub target_base_url: String,
    pub public_host: String,
    pub session_token_file: PathBuf,
    pub session_header: String,
}

#[derive(Debug)]
pub struct WorldCapabilityProxy {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WorldCapabilityProxy {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WorldCapabilityProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_world_capability_proxy(
    config: WorldCapabilityProxyConfig,
) -> Result<WorldCapabilityProxy, String> {
    let target = parse_http_url(&config.target_base_url)
        .ok_or_else(|| "target_base_url must be an http URL with a host".to_string())?;
    if target.scheme != "http" {
        return Err("target_base_url must use http".to_string());
    }

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("failed to bind world capability proxy: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to configure world capability proxy: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("failed to read world capability proxy port: {err}"))?
        .port();

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let public_host = config.public_host.to_ascii_lowercase();
    let session_header = config.session_header.clone();
    let session_token_file = config.session_token_file.clone();
    let thread = thread::Builder::new()
        .name("clawblox-world-capability-proxy".to_string())
        .stack_size(PROXY_THREAD_STACK_SIZE)
        .spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let mut error_stream = stream.try_clone().ok();
                        let target = target.clone();
                        let public_host = public_host.clone();
                        let session_header = session_header.clone();
                        let session_token_file = session_token_file.clone();
                        if let Err(err) = thread::Builder::new()
                            .name("clawblox-world-capability-client".to_string())
                            .stack_size(PROXY_THREAD_STACK_SIZE)
                            .spawn(move || {
                                let _ = handle_client(
                                    stream,
                                    &target,
                                    &public_host,
                                    &session_header,
                                    &session_token_file,
                                );
                            })
                        {
                            if let Some(stream) = &mut error_stream {
                                let _ = write_json_response(
                                    stream,
                                    503,
                                    &format!(
                                        "World capability proxy could not accept client: {err}"
                                    ),
                                    false,
                                );
                            }
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|err| format!("failed to start world capability proxy thread: {err}"))?;

    Ok(WorldCapabilityProxy {
        port,
        stop,
        thread: Some(thread),
    })
}

fn handle_client(
    mut stream: TcpStream,
    target: &ParsedUrl,
    public_host: &str,
    session_header: &str,
    session_token_file: &PathBuf,
) -> Result<(), String> {
    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(err) => {
            let _ = write_json_response(&mut stream, 400, &err, false);
            return Err(err);
        }
    };
    if request.method == "CONNECT" {
        let _ = write_json_response(&mut stream, 403, "CONNECT is not allowed", false);
        return Ok(());
    }
    if !matches!(request.method.as_str(), "GET" | "POST" | "HEAD") {
        let _ = write_json_response(&mut stream, 405, "Method is not allowed", false);
        return Ok(());
    }

    let Some(path_with_query) = parse_proxy_target(&request, public_host) else {
        let _ = write_json_response(
            &mut stream,
            403,
            "Request host is not this agent's world capability",
            request.method == "HEAD",
        );
        return Ok(());
    };
    if !validate_path_with(&path_policy(), &path_with_query) {
        let _ = write_json_response(
            &mut stream,
            403,
            "Path is not in the world API capability",
            request.method == "HEAD",
        );
        return Ok(());
    }
    let expected = std::fs::read_to_string(session_token_file)
        .unwrap_or_default()
        .trim()
        .to_string();
    if expected.is_empty() {
        let _ = write_json_response(
            &mut stream,
            503,
            "World session token is not ready",
            request.method == "HEAD",
        );
        return Ok(());
    }
    let provided = request
        .headers
        .get(&session_header.to_ascii_lowercase())
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    if provided != expected {
        let _ = write_json_response(
            &mut stream,
            401,
            "Missing or invalid world session token",
            request.method == "HEAD",
        );
        return Ok(());
    }

    forward_request(stream, request, target, &path_with_query)
}

fn forward_request(
    mut client: TcpStream,
    request: HttpRequest,
    target: &ParsedUrl,
    path_with_query: &str,
) -> Result<(), String> {
    let upstream_path = format!("{}{}", target.path_prefix, path_with_query);
    let mut upstream = TcpStream::connect((target.host.as_str(), target.port))
        .map_err(|err| format!("World upstream request failed: {err}"))?;
    upstream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|err| err.to_string())?;

    let mut out = format!(
        "{} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
        request.method,
        upstream_path,
        target.host,
        target.port,
        request.body.len()
    );
    for (name, value) in &request.headers {
        if HOP_BY_HOP_HEADERS.contains(&name.as_str()) || name == "host" || name == "content-length"
        {
            continue;
        }
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    upstream
        .write_all(out.as_bytes())
        .and_then(|_| upstream.write_all(&request.body))
        .map_err(|err| format!("World upstream request failed: {err}"))?;

    let mut response = Vec::new();
    upstream
        .read_to_end(&mut response)
        .map_err(|err| format!("World upstream response failed: {err}"))?;
    write_upstream_response(&mut client, &response, request.method == "HEAD")
        .map_err(|err| format!("World proxy response failed: {err}"))?;
    Ok(())
}

fn write_upstream_response(
    client: &mut TcpStream,
    response: &[u8],
    head: bool,
) -> Result<(), String> {
    let header_end =
        find_header_end(response).ok_or_else(|| "upstream response headers missing".to_string())?;
    let head_text = String::from_utf8_lossy(&response[..header_end]).to_string();
    let mut lines = head_text.split("\r\n");
    let status_line = lines
        .next()
        .filter(|line| line.starts_with("HTTP/"))
        .ok_or_else(|| "upstream response status line missing".to_string())?;
    let mut headers = Vec::new();
    let mut chunked = false;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let lower = name.trim().to_ascii_lowercase();
        if lower == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
        if HOP_BY_HOP_HEADERS.contains(&lower.as_str()) || lower == "content-length" {
            continue;
        }
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut body = response[header_end + 4..].to_vec();
    if chunked {
        body = decode_chunked_body(&body)?;
    }

    let mut out = format!("{status_line}\r\n");
    for (name, value) in headers {
        out.push_str(&name);
        out.push_str(": ");
        out.push_str(&value);
        out.push_str("\r\n");
    }
    out.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    client
        .write_all(out.as_bytes())
        .map_err(|err| err.to_string())?;
    if !head {
        client.write_all(&body).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    let mut pos = 0;
    loop {
        let line_end = body[pos..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| pos + offset)
            .ok_or_else(|| "chunked upstream response is truncated".to_string())?;
        let size_line = String::from_utf8_lossy(&body[pos..line_end]);
        let size_text = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "chunked upstream response has an invalid chunk size".to_string())?;
        pos = line_end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        let chunk_end = pos
            .checked_add(size)
            .ok_or_else(|| "chunked upstream response is too large".to_string())?;
        if chunk_end + 2 > body.len() || &body[chunk_end..chunk_end + 2] != b"\r\n" {
            return Err("chunked upstream response is truncated".to_string());
        }
        decoded.extend_from_slice(&body[pos..chunk_end]);
        pos = chunk_end + 2;
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| err.to_string())?;
    let mut data = Vec::new();
    let mut buf = [0_u8; 4096];
    let header_end = loop {
        let n = stream
            .read(&mut buf)
            .map_err(|err| format!("failed to read request: {err}"))?;
        if n == 0 {
            return Err("client closed request before headers completed".to_string());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = find_header_end(&data) {
            break pos;
        }
        if data.len() > 64 * 1024 {
            return Err("request headers are too large".to_string());
        }
    };

    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "request line missing".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "request method missing".to_string())?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| "request target missing".to_string())?
        .to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    let mut body = data[body_start..].to_vec();
    while body.len() < content_length {
        let n = stream
            .read(&mut buf)
            .map_err(|err| format!("failed to read request body: {err}"))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        target,
        headers,
        body,
    })
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_proxy_target(request: &HttpRequest, public_host: &str) -> Option<String> {
    if let Some(parsed) = parse_http_url(&request.target) {
        if parsed.scheme != "http" || parsed.host.to_ascii_lowercase() != public_host {
            return None;
        }
        if parsed.port != 80 {
            return None;
        }
        return Some(parsed.path_with_query);
    }
    let host = request
        .headers
        .get("host")
        .and_then(|value| value.split(':').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if host != public_host {
        return None;
    }
    Some(if request.target.is_empty() {
        "/".to_string()
    } else {
        request.target.clone()
    })
}

#[cfg(test)]
fn validate_path(path_with_query: &str) -> bool {
    validate_path_with(&PathPolicy::default(), path_with_query)
}

fn validate_path_with(policy: &PathPolicy, path_with_query: &str) -> bool {
    let path = path_with_query.split('?').next().unwrap_or("/");
    if DENIED_PREFIXES
        .iter()
        .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(prefix))
    {
        return false;
    }
    if policy.api_only {
        return API_ONLY_PATHS.contains(&path) || (policy.allow_image && path == "/observe/image");
    }
    ALLOWED_EXACT_PATHS.contains(&path)
        || ALLOWED_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
}

fn write_json_response(
    stream: &mut TcpStream,
    status: u16,
    message: &str,
    head: bool,
) -> std::io::Result<()> {
    let reason = match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let body = format!(
        "{{\"error\":{}}}\n",
        serde_json::to_string(message).unwrap_or_else(|_| "\"error\"".to_string())
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    if !head {
        stream.write_all(body.as_bytes())?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct ParsedUrl {
    scheme: String,
    host: String,
    port: u16,
    path_prefix: String,
    path_with_query: String,
}

fn parse_http_url(value: &str) -> Option<ParsedUrl> {
    let (scheme, rest) = value.split_once("://")?;
    let (authority, path_query) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().ok()?),
        None => (
            authority.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    if host.is_empty() {
        return None;
    }
    let path_prefix = path_query
        .split('?')
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    Some(ParsedUrl {
        scheme: scheme.to_string(),
        host,
        port,
        path_prefix: path_prefix.to_string(),
        path_with_query: path_query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn rejects_non_world_hosts() {
        let request = HttpRequest {
            method: "GET".to_string(),
            target: "http://localhost/observe".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        };

        assert_eq!(parse_proxy_target(&request, "world"), None);
    }

    #[test]
    fn api_only_policy_allows_exactly_the_documented_endpoints() {
        // for new runs, only API.md's endpoints,
        // and the image endpoint only when the run has vision.
        let blind = PathPolicy { api_only: true, allow_image: false };
        for ok in ["/observe", "/input", "/chat", "/chat/messages?after=1", "/api.md"] {
            assert!(validate_path_with(&blind, ok), "{ok} should be allowed");
        }
        for no in [
            "/observe/image",
            "/renderer/manifest",
            "/renderer-files/index.js",
            "/assets/player.glb",
            "/static/x",
            "/skill.md",
            "/api/admin",
        ] {
            assert!(!validate_path_with(&blind, no), "{no} should be refused");
        }
        let sighted = PathPolicy { api_only: true, allow_image: true };
        assert!(validate_path_with(&sighted, "/observe/image?width=320"));
        assert!(!validate_path_with(&sighted, "/assets/player.glb"));
        // The default (upstream) policy is unchanged.
        assert!(validate_path_with(&PathPolicy::default(), "/assets/player.glb"));
        assert!(validate_path_with(&PathPolicy::default(), "/observe/image"));
    }

    #[test]
    fn validates_allowed_and_denied_paths() {
        assert!(validate_path("/observe"));
        assert!(validate_path("/observe/image?width=320&height=180"));
        assert!(validate_path("/assets/foo.png"));
        assert!(!validate_path("/api/admin"));
        assert!(!validate_path("/sessions/secret"));
        assert!(!validate_path("/unknown"));
    }

    #[test]
    fn filters_upstream_response_like_capability_proxy() {
        let response = b"HTTP/1.1 201 Created\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n3\r\nhey\r\n0\r\n\r\n";
        let (mut client, mut server) = connected_pair();

        write_upstream_response(&mut server, response, false).unwrap();
        drop(server);
        let mut forwarded = String::new();
        client.read_to_string(&mut forwarded).unwrap();

        assert!(forwarded.starts_with("HTTP/1.1 201 Created\r\n"));
        assert!(forwarded.contains("Content-Type: text/plain\r\n"));
        assert!(forwarded.contains("Content-Length: 3\r\n"));
        assert!(forwarded.ends_with("\r\n\r\nhey"));
        assert!(!forwarded.to_ascii_lowercase().contains("transfer-encoding"));
    }

    #[test]
    fn forwards_only_with_valid_session() {
        let (target_port_tx, target_port_rx) = mpsc::channel();
        let (request_tx, request_rx) = mpsc::channel();
        let target_thread = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            target_port_tx
                .send(listener.local_addr().unwrap().port())
                .unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 2048];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            request_tx.send(request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nok\n")
                .unwrap();
        });
        let target_port = target_port_rx.recv().unwrap();
        let token_file = std::env::temp_dir().join(format!(
            "clawblox-proxy-token-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&token_file, "session-token\n").unwrap();
        let mut proxy = start_world_capability_proxy(WorldCapabilityProxyConfig {
            target_base_url: format!("http://127.0.0.1:{target_port}"),
            public_host: "world".to_string(),
            session_token_file: token_file.clone(),
            session_header: "X-Session".to_string(),
        })
        .unwrap();

        let mut client = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
        client
            .write_all(
                b"GET http://world/observe HTTP/1.1\r\nHost: world\r\nX-Session: session-token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();

        proxy.stop();
        let _ = std::fs::remove_file(token_file);
        target_thread.join().unwrap();
        assert!(response.contains("200 OK"));
        assert!(request_rx.recv().unwrap().contains("GET /observe HTTP/1.1"));
    }

    fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }
}
