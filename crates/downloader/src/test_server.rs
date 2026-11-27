//! Minimal HTTP/1.1 file server for download tests.
//!
//! Serves one in-memory body at every path except `/missing` (404), with
//! optional `Range` / `If-Range` support, and can be scripted to drop or
//! stall the connection after a given number of body bytes so that resume
//! logic can be exercised deterministically.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What to do with the body of the next response.
#[derive(Debug, Clone, Copy)]
pub enum Fault {
    /// Close the connection after this many body bytes.
    DropAfter(usize),
    /// Send this many body bytes, then hang without closing.
    StallAfter(usize),
}

#[derive(Debug, Clone)]
pub struct LoggedRequest {
    pub range: Option<String>,
    pub if_range: Option<String>,
}

pub struct ServerState {
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub support_range: bool,
    /// Faults applied to the next responses, one per response.
    pub faults: VecDeque<Fault>,
    pub requests: Vec<LoggedRequest>,
}

pub struct TestServer {
    pub base: String,
    pub state: Arc<Mutex<ServerState>>,
}

impl TestServer {
    pub async fn start(body: Vec<u8>, etag: Option<&str>, support_range: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let state = Arc::new(Mutex::new(ServerState {
            body,
            etag: etag.map(ToString::to_string),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
            support_range,
            faults: VecDeque::new(),
            requests: Vec::new(),
        }));

        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else { break };
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let _ = handle(socket, shared).await;
                });
            }
        });

        Self { base: format!("http://{addr}"), state }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub fn push_fault(&self, fault: Fault) {
        self.state.lock().unwrap().faults.push_back(fault);
    }

    pub fn requests(&self) -> Vec<LoggedRequest> {
        self.state.lock().unwrap().requests.clone()
    }
}

async fn handle(mut socket: TcpStream, state: Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = socket.read(&mut buf).await?;
        if read == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..read]);
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let mut lines = head.split("\r\n");
    let path = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let mut range = None;
    let mut if_range = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "range" => range = Some(value.trim().to_string()),
                "if-range" => if_range = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }

    let (status_line, headers, payload, fault) = {
        let mut s = state.lock().unwrap();
        s.requests.push(LoggedRequest { range: range.clone(), if_range: if_range.clone() });
        let fault = s.faults.pop_front();

        if path == "/missing" {
            ("404 Not Found", Vec::new(), b"missing".to_vec(), None)
        } else {
            let len = s.body.len() as u64;
            let mut headers = vec!["Accept-Ranges: bytes".to_string()];
            if let Some(tag) = &s.etag {
                headers.push(format!("ETag: {tag}"));
            }
            if let Some(modified) = &s.last_modified {
                headers.push(format!("Last-Modified: {modified}"));
            }

            let validator_ok = match &if_range {
                None => true,
                Some(value) => s.etag.as_deref() == Some(value) || s.last_modified.as_deref() == Some(value),
            };
            let start = range
                .as_deref()
                .filter(|_| s.support_range && validator_ok)
                .and_then(|value| value.strip_prefix("bytes="))
                .and_then(|value| value.strip_suffix('-'))
                .and_then(|value| value.parse::<u64>().ok());

            match start {
                Some(start) if start >= len => {
                    headers.push(format!("Content-Range: bytes */{len}"));
                    ("416 Range Not Satisfiable", headers, Vec::new(), None)
                }
                Some(start) => {
                    headers.push(format!("Content-Range: bytes {}-{}/{}", start, len - 1, len));
                    ("206 Partial Content", headers, s.body[start as usize..].to_vec(), fault)
                }
                None => ("200 OK", headers, s.body.clone(), fault),
            }
        }
    };

    let mut response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n",
        payload.len()
    );
    for header in headers {
        response.push_str(&header);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    socket.write_all(response.as_bytes()).await?;

    match fault {
        None => socket.write_all(&payload).await?,
        Some(Fault::DropAfter(bytes)) => {
            socket.write_all(&payload[..bytes.min(payload.len())]).await?;
            socket.flush().await?;
            // Dropping the socket closes the connection mid-body.
            return Ok(());
        }
        Some(Fault::StallAfter(bytes)) => {
            socket.write_all(&payload[..bytes.min(payload.len())]).await?;
            socket.flush().await?;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
    socket.flush().await?;
    let _ = socket.shutdown().await;
    Ok(())
}
