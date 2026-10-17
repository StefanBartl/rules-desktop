//! What the tests of the hub share: requests as raw bytes, a session and the event stream.

#![allow(dead_code)]

use std::time::Duration;

use appkit_core::authn::Secret;
use appkit_hub::RunningServer;
use appkit_testkit::wire::{request, send, Reply};

pub async fn get_with(
    server: &RunningServer,
    path: &str,
    auth: Option<&str>,
    extra: &[(&str, &str)],
) -> Reply {
    let host = format!("127.0.0.1:{}", server.addr().port());
    let bearer = auth.map(|t| format!("Bearer {t}"));
    let mut headers = vec![("Host", host.as_str())];
    if let Some(b) = &bearer {
        headers.push(("Authorization", b.as_str()));
    }
    headers.extend_from_slice(extra);
    send(server.addr(), request("GET", path, &headers, b"")).await
}

pub async fn get(server: &RunningServer, path: &str, auth: Option<&str>) -> Reply {
    get_with(server, path, auth, &[]).await
}

pub async fn post(server: &RunningServer, path: &str, auth: Option<&str>, body: &str) -> Reply {
    let host = format!("127.0.0.1:{}", server.addr().port());
    let origin = format!("http://{host}");
    let len = body.len().to_string();
    let bearer = auth.map(|t| format!("Bearer {t}"));
    let mut headers = vec![
        ("Host", host.as_str()),
        ("Origin", origin.as_str()),
        ("Content-Type", "application/json"),
        ("Content-Length", len.as_str()),
    ];
    if let Some(b) = &bearer {
        headers.push(("Authorization", b.as_str()));
    }
    send(
        server.addr(),
        request("POST", path, &headers, body.as_bytes()),
    )
    .await
}

/// Pair with the control secret and exchange the code: the token of a new session.
pub async fn session(server: &RunningServer, control: &Secret) -> String {
    let code = post(server, "/api/control/pair", Some(&control.to_hex()), "{}").await;
    assert_eq!(code.status, 200, "{code:?}");
    let body = format!(r#"{{"code":"{}"}}"#, code.json()["code"].as_str().unwrap());
    let token = post(server, "/auth/exchange", None, &body).await;
    assert_eq!(token.status, 200, "{token:?}");
    token.json()["token"].as_str().unwrap().to_owned()
}

/// An open event stream, read the way a browser's `fetch` reads it.
pub struct Sse {
    stream: tokio::net::TcpStream,
    buf: Vec<u8>,
}

impl Sse {
    /// Open `/api/rules/events`. `Err` carries the status of a refusal.
    pub async fn open(server: &RunningServer, token: Option<&str>) -> Result<Self, u16> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let addr = server.addr();
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut request = format!(
            "GET /api/rules/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAccept: text/event-stream\r\n",
            addr.port()
        );
        if let Some(t) = token {
            request.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }
            let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
                .await
                .expect("a reply")
                .unwrap();
            assert!(n > 0, "closed before a reply");
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        let status: u16 = head.split(' ').nth(1).unwrap().parse().unwrap();
        if status != 200 {
            return Err(status);
        }
        assert!(head.to_lowercase().contains("text/event-stream"), "{head}");
        let rest = buf.split_off(head_end + 4);
        Ok(Self { stream, buf: rest })
    }

    /// The next message of the stream, whole (one chunk is one message), or `None` when none
    /// came within `within` or the stream ended. The end of a chunked body is an empty message.
    pub async fn next(&mut self, within: Duration) -> Option<String> {
        use tokio::io::AsyncReadExt;
        let deadline = tokio::time::Instant::now() + within;
        loop {
            if let Some(message) = self.take_chunk() {
                return Some(message);
            }
            let mut chunk = [0u8; 4096];
            match tokio::time::timeout_at(deadline, self.stream.read(&mut chunk)).await {
                Ok(Ok(n)) if n > 0 => self.buf.extend_from_slice(&chunk[..n]),
                _ => return None,
            }
        }
    }

    fn take_chunk(&mut self) -> Option<String> {
        let line_end = self.buf.windows(2).position(|w| w == b"\r\n")?;
        let size =
            usize::from_str_radix(std::str::from_utf8(&self.buf[..line_end]).ok()?, 16).ok()?;
        let start = line_end + 2;
        if self.buf.len() < start + size + 2 {
            return None;
        }
        let text = String::from_utf8_lossy(&self.buf[start..start + size]).into_owned();
        self.buf.drain(..start + size + 2);
        Some(text)
    }

    /// The data of the next event of this name, skipping everything else; `None` when none came
    /// within `within` or the stream ended.
    pub async fn next_event(&mut self, event: &str, within: Duration) -> Option<serde_json::Value> {
        let deadline = tokio::time::Instant::now() + within;
        let prefix = format!("event: {event}\ndata: ");
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let message = self.next(left).await?;
            if message.is_empty() {
                return None;
            }
            if let Some(data) = message
                .strip_prefix(&prefix)
                .and_then(|m| m.strip_suffix("\n\n"))
            {
                return Some(serde_json::from_str(data).unwrap());
            }
        }
    }

    /// Did the server end the stream within `within`? Every message until then is returned, so a
    /// test can see that none of them was an event.
    pub async fn ended(&mut self, within: Duration) -> (bool, Vec<String>) {
        let deadline = tokio::time::Instant::now() + within;
        let mut seen = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match self.next(left).await {
                Some(message) if message.is_empty() => return (true, seen),
                Some(message) => seen.push(message),
                None => return (false, seen),
            }
        }
    }
}
