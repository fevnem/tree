//! A loopback-only HTTP endpoint so a browser extension can add downloads.
//!
//! Deliberately tiny and boring: bind 127.0.0.1, answer `GET /add?url=…`, ignore
//! everything else, never touch the network. Requests are handed to the UI thread
//! through a channel, so no Win32 call ever happens off the UI thread.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};

pub const PORT: u16 = 17817;

/// Start the listener. Returns `None` if the port is taken (another Tree, or an
/// unrelated program) — the app still works, only the extension bridge is off.
pub fn start(port: u16) -> Option<Receiver<Vec<String>>> {
    let listener = TcpListener::bind(("127.0.0.1", port)).ok()?;
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("tree-bridge".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if let Some(urls) = handle(stream) {
                    if tx.send(urls).is_err() {
                        break;
                    }
                }
            }
        })
        .ok()?;
    Some(rx)
}

fn handle(mut stream: TcpStream) -> Option<Vec<String>> {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    if n == 0 {
        return None;
    }
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    let mut lines = request.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    if method == "OPTIONS" {
        respond(&mut stream, 204, "", true);
        return None;
    }

    let body = if target.starts_with("/add?url=") {
        let raw = target.trim_start_matches("/add?url=");
        let url = percent_decode(raw.split('&').next().unwrap_or(raw));
        if url.starts_with("http://") || url.starts_with("https://") {
            respond(&mut stream, 200, r#"{"ok":true}"#, true);
            return Some(vec![url]);
        }
        r#"{"ok":false,"error":"not an http(s) url"}"#
    } else if target.starts_with("/ping") {
        r#"{"ok":true,"app":"tree"}"#
    } else {
        respond(&mut stream, 404, r#"{"ok":false}"#, true);
        return None;
    };
    respond(&mut stream, 200, body, true);
    None
}

fn respond(stream: &mut TcpStream, status: u16, body: &str, cors: bool) {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "OK",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        reason,
        body.len()
    );
    if cors {
        head.push_str("Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET,POST,OPTIONS\r\nAccess-Control-Allow-Headers: *\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// Only the escapes a browser actually produces in a URL.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(h), Some(l)) => {
                let hex = |c: u8| match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                };
                if let (Some(hi), Some(lo)) = (hex(*h), hex(*l)) {
                    out.push(hi * 16 + lo);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            (b'+', _, _) => {
                out.push(b' ');
                i += 1;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_browser_escapes() {
        assert_eq!(
            percent_decode("https%3A%2F%2Fexample.com%2Fa%20b.zip"),
            "https://example.com/a b.zip"
        );
        assert_eq!(percent_decode("https://x/y?q=1"), "https://x/y?q=1");
        assert_eq!(percent_decode("a+b"), "a b");
    }
}
