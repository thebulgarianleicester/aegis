use crate::{api, AppState};
use anyhow::{bail, Context, Result};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    thread,
    time::Duration,
};

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json<T: serde::Serialize>(value: &T) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8".to_owned(),
            body: serde_json::to_vec(value).unwrap_or_else(|error| {
                format!("{{\"error\":\"serialization failed: {error}\"}}").into_bytes()
            }),
        }
    }

    pub fn error(status: u16, message: impl std::fmt::Display) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8".to_owned(),
            body: serde_json::to_vec(&serde_json::json!({ "error": message.to_string() }))
                .unwrap_or_default(),
        }
    }
}

pub fn serve(listener: TcpListener, state: AppState, web_root: Option<PathBuf>) -> Result<()> {
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("aegis-daemon: accept failed: {error}");
                continue;
            }
        };
        let state = state.clone();
        let web_root = web_root.clone();
        thread::spawn(move || {
            if let Err(error) = handle(stream, state, web_root) {
                eprintln!("aegis-daemon: connection ended: {error:#}");
            }
        });
    }
    Ok(())
}

fn handle(stream: TcpStream, state: AppState, web_root: Option<PathBuf>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let request = read_request(&mut reader)?;
    if request.path == "/api/v1/stream"
        && request
            .headers
            .get("upgrade")
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        let key = request
            .headers
            .get("sec-websocket-key")
            .context("missing Sec-WebSocket-Key")?;
        let receiver = state.subscribe()?;
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            crate::websocket::accept_key(key)
        )?;
        stream.flush()?;
        stream.set_read_timeout(None)?;
        for message in receiver {
            if write_text_frame(&mut stream, message.as_bytes()).is_err() {
                break;
            }
        }
        return Ok(());
    }
    let response = if request.path.starts_with("/api/") {
        api::route(&state, &request)
    } else {
        static_response(&request, web_root.as_deref())
    };
    let mut stream = reader.into_inner();
    write_response(&mut stream, response)?;
    Ok(())
}

fn read_request(reader: &mut impl BufRead) -> Result<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        bail!("client closed before request");
    }
    if line.len() > 8192 {
        bail!("request line too long");
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().context("missing HTTP method")?.to_owned();
    let target = parts.next().context("missing HTTP target")?;
    let path = target.split('?').next().unwrap_or(target).to_owned();
    let mut headers = HashMap::new();
    for _ in 0..64 {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            bail!("truncated HTTP headers");
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .context("invalid Content-Length")?
        .unwrap_or(0);
    if length > 65_536 {
        bail!("request body exceeds 64 KiB");
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn write_response(stream: &mut TcpStream, response: Response) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        426 => "Upgrade Required",
        500 => "Internal Server Error",
        _ => "Response",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()?;
    Ok(())
}

fn write_text_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    stream.write_all(&[0x81])?;
    match payload.len() {
        length @ 0..=125 => stream.write_all(&[length as u8])?,
        length @ 126..=65_535 => {
            stream.write_all(&[126])?;
            stream.write_all(&(length as u16).to_be_bytes())?;
        }
        length => {
            stream.write_all(&[127])?;
            stream.write_all(&(length as u64).to_be_bytes())?;
        }
    }
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn static_response(request: &Request, web_root: Option<&Path>) -> Response {
    if request.method != "GET" {
        return Response::error(405, "static content supports GET only");
    }
    let Some(root) = web_root else {
        return Response::error(
            404,
            "web root not configured; use the Vite development server",
        );
    };
    let relative = request.path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if Path::new(relative)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Response::error(400, "invalid static path");
    }
    let mut path = root.join(relative);
    if !path.is_file() {
        path = root.join("index.html");
    }
    match fs::read(&path) {
        Ok(body) => Response {
            status: 200,
            content_type: content_type(&path).to_owned(),
            body,
        },
        Err(_) => Response::error(404, "console not built; run npm run build in web/"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}
