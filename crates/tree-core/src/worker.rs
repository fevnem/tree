//! One ranged connection. Writes at absolute file offsets and reports progress
//! into the shared task; never buffers more than one 64 KiB chunk.

use crate::engine::{TaskRun, RUN};
use crate::error::DlError;
use crate::io;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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

pub async fn run_segment(run: Arc<TaskRun>, idx: usize) -> Result<(), DlError> {
    let (start, end) = run.bounds(idx);
    if start > end {
        return Ok(());
    }
    let mut off = run.offset(idx);
    if off > end {
        return Ok(());
    }
    let file = io::open_part(&run.part())?;
    let mut attempt = 0u32;

    loop {
        if run.stopped() != RUN {
            return Ok(());
        }
        match fetch(&run, idx, &file, &mut off, start, end).await {
            Ok(Step::Done) | Ok(Step::Paused) => return Ok(()),
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

async fn fetch(
    run: &Arc<TaskRun>,
    idx: usize,
    file: &std::fs::File,
    off: &mut u64,
    start: u64,
    end: u64,
) -> Result<Step, DlError> {
    let range = if end == u64::MAX {
        format!("bytes={}-", off)
    } else {
        format!("bytes={}-{}", off, end)
    };
    let resp = run
        .client
        .get(&run.url)
        .header(reqwest::header::RANGE, range)
        .timeout(CHUNK_TIMEOUT.max(Duration::from_secs(1)))
        .send()
        .await?;

    let code = resp.status().as_u16();
    if code == 200 && *off > start {
        // The server silently ignored our range: continuing would corrupt the file.
        return Err(DlError::RangeUnsupported);
    }
    if code == 206 {
        // A server may answer with a different range than the one requested;
        // writing it at our offset would shift the whole file.
        if let Some(cr) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
            match range_start(cr.to_str().unwrap_or("")) {
                Some(from) if from == *off => {}
                Some(_) | None => return Err(DlError::RangeUnsupported),
            }
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
        let take = if end == u64::MAX {
            chunk.len() as u64
        } else {
            (end - *off + 1).min(chunk.len() as u64)
        };
        if take == 0 {
            break;
        }
        io::write_at(file, &chunk[..take as usize], *off)?;
        *off += take;
        run.add(idx, take);
        if end != u64::MAX && *off > end {
            return Ok(Step::Done);
        }
        run.bucket.take(take).await;
    }

    if end == u64::MAX {
        return Ok(Step::Done); // unknown length: end of stream is the end
    }
    // Closed before the range was complete: the outer loop re-requests the rest.
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
