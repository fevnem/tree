//! Integration test: the real thing against a real server.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{Duration, Instant};
use tree_core::{Engine, Settings, Status};

const URL: &str = "https://proof.ovh.net/files/10Mb.dat";
const TIMEOUT: Duration = Duration::from_secs(240);

fn hash(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read downloaded file");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

/// Reference download: one plain GET, no ranges. Proves the segmented file is
/// byte-identical to what a browser would save.
fn reference_sha() -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let bytes = rt.block_on(async {
        let resp = reqwest::get(URL).await.unwrap();
        resp.bytes().await.unwrap()
    });
    format!("{:x}", Sha256::digest(&bytes))
}

fn wait_for<F: Fn(&[tree_core::Task]) -> bool>(engine: &Engine, what: &str, f: F) -> Vec<tree_core::Task> {
    let started = Instant::now();
    loop {
        let tasks = engine.snapshot();
        if f(&tasks) {
            return tasks;
        }
        if started.elapsed() > TIMEOUT {
            panic!("timed out waiting for {what}: {tasks:?}");
        }
        std::thread::sleep(Duration::from_millis(120));
    }
}

#[test]
fn eight_connections_match_a_plain_download() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.dir = dir.path().to_string_lossy().into_owned();
    settings.conns = 8;
    settings.simultaneous = 2;

    let mut engine = Engine::start(dir.path().join("state.json"), dir.path().to_path_buf());
    engine.set_settings(settings);
    engine.add(URL);

    let tasks = wait_for(&engine, "completion", |t| {
        t.first().map(|x| x.status == Status::Done).unwrap_or(false)
    });
    let task = &tasks[0];

    assert_eq!(task.name, "10Mb.dat", "name from the URL");
    assert_eq!(task.total, 10_485_760, "size from Content-Range");
    assert!(task.supports_ranges, "server supports ranges");
    assert!(task.segments.len() > 1, "used several connections");
    assert_eq!(task.done, task.total);
    let saved = dir.path().join(&task.name);
    assert_eq!(
        std::fs::metadata(&saved).unwrap().len(),
        task.total,
        "file is the exact size"
    );

    assert_eq!(hash(&saved), reference_sha(), "content is byte-identical");

    // Paused state must not be persisted as Running, and the part file is gone.
    engine.shutdown();
    assert!(!dir.path().join("10Mb.dat.tree-part").exists());
}

#[test]
fn pause_then_resume_keeps_every_byte() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.dir = dir.path().to_string_lossy().into_owned();
    settings.conns = 8;
    settings.simultaneous = 1;

    let mut engine = Engine::start(dir.path().join("state.json"), dir.path().to_path_buf());
    engine.set_settings(settings);
    engine.add(URL);

    // let it get going, then pause
    wait_for(&engine, "some progress", |t| {
        t.first().map(|x| x.done > 1_000_000).unwrap_or(false)
    });
    let id = engine.snapshot()[0].id;
    engine.pause(id);
    let paused = wait_for(&engine, "pause", |t| {
        t.first().map(|x| x.status == Status::Paused).unwrap_or(false)
    });
    let after_pause = paused[0].done;
    assert!(after_pause > 0);
    assert!(after_pause < paused[0].total, "paused before the end");

    engine.resume(id);
    let done = wait_for(&engine, "resume to finish", |t| {
        t.first().map(|x| x.status == Status::Done).unwrap_or(false)
    });
    assert_eq!(done[0].done, done[0].total);

    let saved = dir.path().join(&done[0].name);
    assert_eq!(hash(&saved), reference_sha(), "resume did not corrupt the file");
    engine.shutdown();
}

#[test]
fn rejects_non_http_input_and_deduplicates() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::start(dir.path().join("state.json"), dir.path().to_path_buf());
    engine.add("ftp://example.com/x");
    engine.add("not a url");
    engine.add_many(&[URL.to_string(), URL.to_string()]);
    std::thread::sleep(Duration::from_millis(400));
    let tasks = engine.snapshot();
    assert_eq!(tasks.len(), 1, "only the http URL, once: {tasks:?}");
    assert_eq!(tasks[0].url, URL);
    engine.pause_all();
    engine.shutdown();
}
