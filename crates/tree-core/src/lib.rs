//! Tree — download engine.
//!
//! No Win32 here on purpose: everything in this crate compiles and is tested on
//! any platform, which is how the parallel-download logic is verified.

pub mod bridge;
pub mod engine;
pub mod error;
pub mod io;
pub mod model;
pub mod plan;
pub mod probe;
pub mod speed;
pub mod store;
pub mod worker;
pub mod ytdlp;

pub use engine::{Cmd, Engine, Outcome};
pub use error::DlError;
pub use model::{Segment, Settings, Status, Task, PART_SUFFIX};

use std::time::Duration;

/// One client for the whole engine: SChannel TLS on Windows, HTTP/1.1 ranges.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!(
            "Tree/",
            env!("CARGO_PKG_VERSION"),
            " (Windows; +https://github.com/fevnem/tree)"
        ))
        .connect_timeout(Duration::from_secs(20))
        .tcp_keepalive(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("http client")
}

pub use worker::validator_conflict;
