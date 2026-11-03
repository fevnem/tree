//! Protocol for the browser bridge: what a request means and who may send it.
//!
//! Lives in the core crate so the rules are unit-testable on any platform; the
//! socket handling stays in the Windows crate.

/// One link from the browser, with the headers that make it work: a download
/// behind a session cookie fails without them.
#[derive(Clone, Debug)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// `{"url": "...", "referer": "...", "cookie": "..."}` — three flat strings, so a
/// small reader avoids a JSON dependency in the UI crate.
pub fn parse_download(body: &str) -> Option<Request> {
    let url = json_string(body, "url")?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let mut headers = Vec::new();
    if let Some(cookie) = json_string(body, "cookie").filter(|v| !v.is_empty()) {
        headers.push(("Cookie".to_string(), cookie));
    }
    if let Some(referer) = json_string(body, "referer").filter(|v| !v.is_empty()) {
        headers.push(("Referer".to_string(), referer));
    }
    if let Some(agent) = json_string(body, "user_agent").filter(|v| !v.is_empty()) {
        headers.push(("User-Agent".to_string(), agent));
    }
    Some(Request { url, headers })
}

/// One string value out of a flat JSON object, honouring the escapes a cookie or
/// URL can actually contain.
pub fn json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if first != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    let mut pending: Option<(u32, u8)> = None; // collecting \uXXXX
    for (index, ch) in chars {
        if let Some((code, count)) = pending.as_mut() {
            if let Some(digit) = ch.to_digit(16) {
                *code = (*code << 4) | digit;
                *count += 1;
            }
            if *count == 4 {
                if let Some(c) = char::from_u32(*code) {
                    out.push(c);
                }
                pending = None;
            }
            continue;
        }
        if escaped {
            match ch {
                'u' => {
                    pending = Some((0, 0));
                    escaped = false;
                    continue;
                }
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            other => out.push(other),
        }
        if index > 8192 {
            break;
        }
    }
    None
}

pub fn origin_allowed(origin: &str) -> bool {
    origin.starts_with("chrome-extension://")
        || origin.starts_with("moz-extension://")
        || origin.starts_with("safari-web-extension://")
}

#[cfg(test)]
mod tests {
    use super::{json_string, origin_allowed, parse_download};

    #[test]
    fn reads_the_three_fields() {
        let body = r#"{"url":"https://example.com/a.zip","cookie":"sid=abc; t=1","referer":"https://example.com/"}"#;
        let request = parse_download(body).expect("parsed");
        assert_eq!(request.url, "https://example.com/a.zip");
        assert_eq!(request.headers[0].0, "Cookie");
        assert_eq!(request.headers[0].1, "sid=abc; t=1");
        assert_eq!(request.headers[1].0, "Referer");
    }

    #[test]
    fn rejects_non_http_urls_and_missing_keys() {
        assert!(parse_download(r#"{"url":"file:///etc/passwd"}"#).is_none());
        assert!(parse_download(r#"{"cookie":"x"}"#).is_none());
        assert!(json_string(r#"{"url":"a"}"#, "referer").is_none());
    }

    #[test]
    fn handles_escapes_in_a_cookie() {
        assert_eq!(
            json_string(r#"{"cookie":"a=\"b\"; c=d"}"#, "cookie").as_deref(),
            Some("a=\"b\"; c=d")
        );
    }

    #[test]
    fn only_extension_origins_preflight() {
        assert!(origin_allowed("chrome-extension://abcdef"));
        assert!(origin_allowed("moz-extension://abc"));
        assert!(!origin_allowed("https://evil.example"));
        assert!(!origin_allowed(""));
    }
}
