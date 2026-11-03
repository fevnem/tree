//! One ranged connection. Writes at absolute file offsets and reports progress
//! into the shared task; never buffers more than one 64 KiB chunk.

use crate::engine::{TaskRun, RUN};
use crate::error::DlError;
use crate::io;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

fn lock_option(m: &Mutex<Option<(String, String)>>) -> Option<(String, String)> {
    m.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A response is only usable when it still carries the validator the probe saw.
/// A missing header on either side means the server never promised stability, so
/// there is nothing to compare.
pub fn validator_conflict(expected: Option<&str>, actual: Option<&str>) -> bool {
    match (expected, actual) {
        (Some(expected), Some(actual)) => expected.trim() != actual.trim(),
        _ => false,
    }
}
use std::time::{Duration, Instant};

const CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ATTEMPTS: u32 = 10;

/// Global byte/second limiter. `0` means unlimited.
pub struct Bucket {
    rate: AtomicU64,
    state: Mutex<(f64, Instant)>,
}

impl Bucket {
    pub fn new(rate: u64) -> Bucket {
        Bucket {
            rate: AtomicU64::new(rate),
            state: Mutex::new((0.0, Instant::now())),
        }
    }

    pub fn set_rate(&self, rate: u64) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    /// Sleep just long enough to keep the average under `rate`.
    pub async fn take(&self, n: u64) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return;
        }
        let wait = {
            let mut g = self.state.lock().unwrap();
            let now = Instant::now();
            let dt = now.duration_since(g.1).as_secs_f64();
            g.1 = now;
            let burst = rate as f64 * 0.25;
            g.0 = (g.0 + dt * rate as f64).clamp(-burst, burst) - n as f64;
            if g.0 < 0.0 {
                -g.0 / rate as f64
            } else {
                0.0
            }
        };
        if wait > 0.001 {
            tokio::time::sleep(Duration::from_secs_f64(wait.min(0.25))).await;
        }
    }
}

enum Step {
    Done,
    Paused,
}

pub async fn run_segment(run: Arc<TaskRun>, worker: usize) -> Result<(), DlError> {
    let file = io::open_part(&run.part())?;

    loop {
        if run.stopped() != RUN {
            return Ok(());
        }
        // Work is claimed, not assigned: a connection that runs dry takes half
        // of the largest remaining range instead of going idle.
        let Some(seg) = run.claim(worker) else {
            return Ok(());
        };
        let mut attempt = 0u32;
        loop {
            if run.stopped() != RUN {
                return Ok(());
            }
            match fetch(&run, seg, &file).await {
                Ok(Step::Done) => {
                    // An open-ended transfer has no end marker: close it so this
                    // connection is not handed the same stream again.
                    if run.end_of(seg) == u64::MAX {
                        run.close(seg);
                    }
                    break; // piece finished, look for more work
                }
                Ok(Step::Paused) => return Ok(()),
                Err(e) => {
                    if run.stopped() != RUN {
                        return Ok(());
                    }
                    attempt += 1;
                    if !e.retryable() || attempt > MAX_ATTEMPTS {
                        return Err(e);
                    }
                    let backoff = (300u64 << attempt.min(4)).min(8_000);
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
            }
        }
    }
}

async fn fetch(run: &Arc<TaskRun>, seg: usize, file: &std::fs::File) -> Result<Step, DlError> {
    let mut off = run.offset(seg);
    let end = run.end_of(seg);
    if off > end {
        return Ok(Step::Done);
    }
    let range = if end == u64::MAX {
        format!("bytes={}-", off)
    } else {
        format!("bytes={}-{}", off, end)
    };
    let mut request = run.client.get(&run.url);
    for (name, value) in &run.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let resp = request
        .header(reqwest::header::RANGE, range)
        .timeout(CHUNK_TIMEOUT.max(Duration::from_secs(1)))
        .send()
        .await?;

    let code = resp.status().as_u16();
    if code == 200 && off > run.bounds(seg).0 {
        // The server silently ignored our range: continuing would corrupt the file.
        return Err(DlError::RangeUnsupported);
    }
    if code == 206 {
        // A server may answer with a different range than the one requested;
        // writing it at our offset would shift the whole file.
        if let Some(cr) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
            match range_start(cr.to_str().unwrap_or("")) {
                Some(from) if from == off => {}
                Some(_) | None => return Err(DlError::RangeUnsupported),
            }
        }
    }
    if let Some((name, expected)) = lock_option(&run.validator) {
        let actual = if name == "etag" {
            resp.headers().get(reqwest::header::ETAG)
        } else {
            resp.headers().get(reqwest::header::LAST_MODIFIED)
        };
        let actual = actual.and_then(|v| v.to_str().ok());
        if validator_conflict(Some(&expected), actual) {
            return Err(DlError::Changed);
        }
    }
    if code == 429 || code == 503 {
        // Back off for as long as the server asks, within reason.
        let wait = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(1)
            .clamp(1, 10);
        tokio::time::sleep(Duration::from_secs(wait)).await;
        return Err(DlError::Http(code));
    }
    if !(200..300).contains(&code) {
        return Err(DlError::Http(code));
    }

    let mut stream = resp.bytes_stream();
    while let Some(item) = stream.next().await {
        if run.stopped() != RUN {
            return Ok(Step::Paused);
        }
        let chunk = item?;
        // The tail may have been handed to another connection while we streamed.
        let end = run.end_of(seg);
        if end != u64::MAX && off > end {
            return Ok(Step::Done);
        }
        let take = if end == u64::MAX {
            chunk.len() as u64
        } else {
            (end - off + 1).min(chunk.len() as u64)
        };
        if take == 0 {
            break;
        }
        io::write_at(file, &chunk[..take as usize], off)?;
        off += take;
        run.add(seg, take);
        if end != u64::MAX && off > end {
            return Ok(Step::Done);
        }
        run.bucket.take(take).await;
    }

    let end = run.end_of(seg);
    if end == u64::MAX || off > end {
        return Ok(Step::Done);
    }
    // Closed before the range was complete: the caller retries the remainder.
    Err(DlError::Network("connection closed early".into()))
}

/// First byte of a `Content-Range: bytes 100-199/2000` header.
fn range_start(header: &str) -> Option<u64> {
    let rest = header.trim().strip_prefix("bytes")?.trim_start();
    rest.split('-').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::range_start;

    #[test]
    fn reads_the_first_byte_of_a_range() {
        assert_eq!(range_start("bytes 1024-2047/8192"), Some(1024));
        assert_eq!(range_start("bytes 0-0/1"), Some(0));
        assert_eq!(range_start("bytes */8192"), None);
        assert_eq!(range_start("garbage"), None);
    }
}
