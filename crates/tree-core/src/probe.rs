//! Size / resume / filename discovery with a single ranged request.

use crate::{error::DlError, plan};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_TYPE};
use reqwest::{Client, Response};

pub struct Probe {
    pub total: u64,
    pub ranges: bool,
    pub name: Option<String>,
    pub url: String,
}

/// `bytes 0-0/12345` -> `Some(12345)`, `bytes 0-0/*` -> `None`.
pub fn parse_content_range(value: &str) -> Option<u64> {
    let (_, rest) = value.split_once('/')?;
    let rest = rest.trim();
    if rest == "*" {
        None
    } else {
        rest.parse().ok()
    }
}

fn name_from(resp: &Response) -> Option<String> {
    let headers = resp.headers();
    if let Some(cd) = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(n) = plan::name_from_disposition(cd) {
            return Some(n);
        }
    }
    let mut name = plan::name_from_url(resp.url().as_str());
    if !name.contains('.') {
        if let Some(mime) = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
            if let Some(ext) = plan::ext_for_mime(mime) {
                name = format!("{}.{}", name, ext);
            }
        }
    }
    Some(name)
}

pub async fn probe(client: &Client, url: &str) -> Result<Probe, DlError> {
    let resp = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        // A server that accepts the socket and then says nothing must not hang
        // the task forever: the probe is the only request with no progress signal.
        .timeout(std::time::Duration::from_secs(45))
        .send()
        .await?;
    let code = resp.status().as_u16();
    let final_url = resp.url().to_string();

    // Empty file or a server that rejects the probe range.
    if code == 416 {
        let head = client
            .head(url)
            .timeout(std::time::Duration::from_secs(45))
            .send()
            .await?;
        let total = head.content_length().unwrap_or(0);
        return Ok(Probe {
            total,
            ranges: false,
            name: name_from(&head),
            url: head.url().to_string(),
        });
    }

    if code != 200 && code != 206 {
        return Err(DlError::Http(code));
    }

    let total = resp
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range)
        .or_else(|| resp.content_length())
        .unwrap_or(0);
    let ranges = code == 206 && total > 0;
    let name = name_from(&resp);
    // Body (at most one byte, or the start of a stream we do not want) is dropped.
    drop(resp);
    Ok(Probe {
        total,
        ranges,
        name,
        url: final_url,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_content_range;

    #[test]
    fn parses_content_range() {
        assert_eq!(parse_content_range("bytes 0-0/6120328396"), Some(6120328396));
        assert_eq!(parse_content_range("bytes 0-0/*"), None);
        assert_eq!(parse_content_range("garbage"), None);
    }
}
