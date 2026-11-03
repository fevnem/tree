//! A loopback-only HTTP endpoint so a browser extension can add downloads.
//!
//! Two rules came out of studying real managers:
//!
//! * **A web page must not be able to queue downloads.** `GET /add?url=…` is a
//!   simple request, so any page could fire it cross-origin (download CSRF). The
//!   endpoint therefore takes a **POST with a custom header**, which forces a CORS
//!   preflight; the preflight is only answered for extension origins, so a page
//!   never gets to send the request at all.
//! * **Authenticated links need their headers.** Cookies and the referer travel in
//!   the JSON body and are applied to the probe and every segment.

use std::io::{Read, Write};
pub use tree_core::bridge::Request;
use tree_core::bridge::{origin_allowed, parse_download};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};

pub const PORT: u16 = 17817;

/// One link from the browser, with the headers that make it work: a download
/// behind a session cookie fails without them.
/// Start the listener. Returns `None` if the port is taken (another Tree, or an
/// unrelated program) — the app still works, only the extension bridge is off.
pub fn start(port: u16) -> Option<Receiver<Vec<Request>>> {
    let listener = TcpListener::bind(("127.0.0.1", port)).ok()?;
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("tree-bridge".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if let Some(requests) = handle(stream) {
                    if tx.send(requests).is_err() {
                        break;
                    }
                }
            }
        })
        .ok()?;
    Some(rx)
}

fn handle(mut stream: TcpStream) -> Option<Vec<Request>> {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut buf = [0u8; 16384];
    let n = stream.read(&mut buf).unwrap_or(0);
    if n == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&buf[..n]).to_string();
    let (head, body) = match text.split_once("\r\n\r\n") {
        Some((h, b)) => (h, b),
        None => (text.as_str(), ""),
    };
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let mut origin = String::new();
    let mut has_client_header = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => origin = value.trim().to_string(),
            "x-tree-client" => has_client_header = true,
            _ => {}
        }
    }

    // Preflight: only extension pages may talk to this endpoint at all.
    if method == "OPTIONS" {
        if origin_allowed(&origin) {
            respond_preflight(&mut stream, &origin);
        } else {
            respond(&mut stream, 403, r#"{"ok":false}"#, None);
        }
        return None;
    }

    if method == "GET" && path.starts_with("/ping") {
        respond(&mut stream, 200, r#"{"ok":true,"app":"tree","api":2}"#, None);
        return None;
    }

    if method == "POST" && path.starts_with("/v1/downloads") {
        if !has_client_header {
            respond(
                &mut stream,
                400,
                r#"{"ok":false,"error":"missing X-Tree-Client"}"#,
                None,
            );
            return None;
        }
        if let Some(request) = parse_download(body) {
            respond(&mut stream, 200, r#"{"ok":true}"#, None);
            return Some(vec![request]);
        }
        respond(
            &mut stream,
            400,
            r#"{"ok":false,"error":"needs a http(s) url"}"#,
            None,
        );
        return None;
    }

    respond(&mut stream, 404, r#"{"ok":false}"#, None);
    None
}


/// `{"url": "...", "referer": "...", "cookie": "..."}` — three flat strings, so a
/// small reader avoids a JSON dependency in the UI crate.
fn respond_preflight(stream: &mut TcpStream, origin: &str) {
    let head = format!(
        "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {}\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\n\
         Access-Control-Allow-Headers: content-type, x-tree-client\r\n\
         Access-Control-Max-Age: 600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        origin
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
}

fn respond(stream: &mut TcpStream, status: u16, body: &str, origin: Option<&str>) {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "OK",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        reason,
        body.len()
    );
    if let Some(origin) = origin.filter(|o| origin_allowed(o)) {
        head.push_str(&format!("Access-Control-Allow-Origin: {}\r\n", origin));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}
