//! One error type, and the one sentence the user sees for each case.

use std::fmt;

#[derive(Debug, Clone)]
pub enum DlError {
    Network(String),
    Http(u16),
    NoSpace,
    Permission(String),
    Disk(String),
    RangeUnsupported,
    Cancelled,
    Incomplete,
}

impl DlError {
    pub fn retryable(&self) -> bool {
        match self {
            DlError::Network(_) => true,
            DlError::Http(code) => *code == 408 || *code == 425 || *code == 429 || *code >= 500,
            _ => false,
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            DlError::Network(s) => format!("Network error — {}", s),
            DlError::Http(401) => "Server needs a login (401)".into(),
            DlError::Http(403) => "Server refused the request (403)".into(),
            DlError::Http(404) => "File not found on the server (404)".into(),
            DlError::Http(410) => "File was removed from the server (410)".into(),
            DlError::Http(429) => "Server is rate-limiting (429) — try again later".into(),
            DlError::Http(code) if *code >= 500 => format!("Server error ({}) — try again later", code),
            DlError::Http(code) => format!("Server answered {} — nothing to download", code),
            DlError::NoSpace => "Disk is full".into(),
            DlError::Permission(s) => format!("Access denied — {}", s),
            DlError::Disk(s) => format!("Disk error — {}", s),
            DlError::RangeUnsupported => "Server stopped supporting resume for this file".into(),
            DlError::Cancelled => "Cancelled".into(),
            DlError::Incomplete => "Download stopped before the end — press Resume".into(),
        }
    }
}

impl fmt::Display for DlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.user_message())
    }
}

impl std::error::Error for DlError {}

impl From<std::io::Error> for DlError {
    fn from(e: std::io::Error) -> Self {
        match e.raw_os_error() {
            Some(28) | Some(112) => DlError::NoSpace,
            _ => match e.kind() {
                std::io::ErrorKind::PermissionDenied => DlError::Permission(e.to_string()),
                std::io::ErrorKind::NotFound => DlError::Disk("folder missing".into()),
                _ => DlError::Disk(e.to_string()),
            },
        }
    }
}

impl From<reqwest::Error> for DlError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            return DlError::Network("timed out".into());
        }
        if e.is_connect() {
            return DlError::Network("cannot connect".into());
        }
        if e.is_body() || e.is_decode() {
            return DlError::Network("connection dropped".into());
        }
        if e.is_builder() || e.is_redirect() {
            return DlError::Network("bad request".into());
        }
        DlError::Network(e.without_url().to_string())
    }
}
