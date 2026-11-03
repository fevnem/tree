//! The engine: one thread, one tokio runtime, one owner of the persisted state.
//!
//! The UI sends commands over an unbounded channel and reads a snapshot. It
//! never writes task state, so there is no cross-thread mutation of the model.

use crate::{
    error::DlError, io, plan, probe,
    model::{Kind, Segment, Settings, Status, Task, PART_SUFFIX},
    speed::Speedometer,
    store,
    worker::{self, Bucket},
};
use crate::ytdlp;
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub const RUN: u8 = 0;
pub const PAUSE: u8 = 1;
pub const CANCEL: u8 = 2;

const TICK: Duration = Duration::from_millis(250);

pub enum Cmd {
    Add(Vec<String>),
    RefreshTools,
    Pause(u64),
    Resume(u64),
    Remove(u64, bool),
    PauseAll,
    ResumeAll,
    Settings(Settings),
    Finished { id: u64, outcome: Outcome },
    Quit,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Done,
    Paused,
    Failed(String),
    Cancelled,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Live state of one running task, shared by its connections.
pub struct Run {
    pub name: String,
    pub total: u64,
    pub bounds: Vec<(u64, u64)>,
    pub seg_done: Vec<u64>,
    /// Which connection owns each segment.
    pub owner: Vec<usize>,
}

/// Splitting rules, adapted from Vise's adaptive chunker: never leave a victim
/// with less than a piece plus a margin, and keep boundaries aligned so a later
/// split cannot create slivers.
const MIN_PIECE: u64 = 1 << 20;
const STEAL_MARGIN: u64 = 256 << 10;
const ALIGN: u64 = 64 << 10;
const MAX_SEGMENTS: usize = 64;

/// Midpoint of a remaining range, aligned, or `None` when it is too small to
/// split without starving the victim.
fn split_point(offset: u64, end: u64) -> Option<u64> {
    if end == u64::MAX {
        return None;
    }
    let left = (end + 1).saturating_sub(offset);
    if left < 2 * MIN_PIECE + STEAL_MARGIN {
        return None;
    }
    let mid = offset + left / 2;
    let aligned = (mid / ALIGN) * ALIGN;
    let low = offset + MIN_PIECE;
    let high = (end + 1).saturating_sub(MIN_PIECE);
    (low <= high).then(|| aligned.clamp(low, high)).filter(|p| *p > offset && *p <= end)
}

pub struct TaskRun {
    pub id: u64,
    pub url: String,
    pub dir: PathBuf,
    pub client: Client,
    pub kind: Kind,
    pub stop: AtomicU8,
    pub bucket: Arc<Bucket>,
    pub inner: Mutex<Run>,
}

impl TaskRun {
    fn new(t: &Task, client: Client, bucket: Arc<Bucket>) -> TaskRun {
        TaskRun {
            id: t.id,
            url: t.url.clone(),
            dir: PathBuf::from(&t.dir),
            client,
            kind: t.kind,
            stop: AtomicU8::new(RUN),
            bucket,
            inner: Mutex::new(Run {
                name: t.name.clone(),
                total: t.total,
                bounds: t.segments.iter().map(|s| (s.start, s.end)).collect(),
                seg_done: t.segments.iter().map(|s| s.done).collect(),
                owner: (0..t.segments.len()).collect(),
            }),
        }
    }

    pub fn stopped(&self) -> u8 {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn bounds(&self, i: usize) -> (u64, u64) {
        lock(&self.inner).bounds[i]
    }

    pub fn offset(&self, i: usize) -> u64 {
        let g = lock(&self.inner);
        g.bounds[i].0 + g.seg_done[i]
    }

    pub fn end_of(&self, i: usize) -> u64 {
        lock(&self.inner).bounds[i].1
    }

    /// Mark an open-ended segment finished (unknown-length transfers).
    pub fn close(&self, i: usize) {
        let mut g = lock(&self.inner);
        let done = g.bounds[i].0 + g.seg_done[i];
        g.bounds[i].1 = done.saturating_sub(1);
    }

    /// The next segment this connection should work on: its own unfinished
    /// range, or, when it is out of work, half of the largest remaining range.
    pub fn claim(&self, worker: usize) -> Option<usize> {
        let mut g = lock(&self.inner);
        for i in 0..g.bounds.len() {
            if g.owner[i] != worker {
                continue;
            }
            let (start, end) = g.bounds[i];
            if end == u64::MAX {
                return Some(i); // open-ended stream: this connection owns it
            }
            if start + g.seg_done[i] <= end {
                return Some(i);
            }
        }
        if g.bounds.len() >= MAX_SEGMENTS {
            return None;
        }
        let mut victim: Option<(usize, u64)> = None;
        for i in 0..g.bounds.len() {
            if g.owner[i] == worker {
                continue;
            }
            let start = g.bounds[i].0 + g.seg_done[i];
            let end = g.bounds[i].1;
            if split_point(start, end).is_none() {
                continue;
            }
            let left = (end + 1).saturating_sub(start);
            if victim.is_none_or(|(_, l)| left > l) {
                victim = Some((i, left));
            }
        }
        let (victim, _) = victim?;
        let (vstart, vend) = g.bounds[victim];
        let offset = vstart + g.seg_done[victim];
        let split = split_point(offset, vend)?;
        g.bounds[victim].1 = split - 1;
        g.bounds.push((split, vend));
        g.seg_done.push(0);
        g.owner.push(worker);
        Some(g.bounds.len() - 1)
    }

    pub fn add(&self, i: usize, n: u64) {
        let mut g = lock(&self.inner);
        g.seg_done[i] = g.seg_done[i].saturating_add(n);
    }

    pub fn total(&self) -> u64 {
        lock(&self.inner).total
    }

    pub fn done(&self) -> u64 {
        lock(&self.inner).seg_done.iter().sum()
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(lock(&self.inner).name.clone())
    }

    pub fn part(&self) -> PathBuf {
        self.dir
            .join(format!("{}{}", lock(&self.inner).name, PART_SUFFIX))
    }
}

pub struct Engine {
    tasks: Arc<Mutex<Vec<Task>>>,
    settings: Arc<Mutex<Settings>>,
    tx: UnboundedSender<Cmd>,
    ytdlp: Arc<Mutex<Option<PathBuf>>>,
    default_dir: PathBuf,
    join: Option<JoinHandle<()>>,
}

impl Engine {
    /// Load state, then start the engine thread. Tasks left `Running` by a crash
    /// come back as `Queued` and resume from their saved offsets.
    pub fn start(state_path: PathBuf, default_dir: PathBuf) -> Engine {
        let (mut settings, mut tasks) = store::load(&state_path);
        if settings.dir.trim().is_empty() {
            settings.dir = default_dir.to_string_lossy().into_owned();
        }
        for t in &mut tasks {
            if t.status == Status::Running {
                t.status = Status::Queued;
            }
        }

        let tasks = Arc::new(Mutex::new(tasks));
        let settings = Arc::new(Mutex::new(settings));
        let (tx, rx) = unbounded_channel();

        let tools = Arc::new(Mutex::new(resolve_ytdlp(&state_path)));

        let runner = Runner {
            state: state_path,
            tasks: tasks.clone(),
            settings: settings.clone(),
            active: HashMap::new(),
            speeds: HashMap::new(),
            client: crate::http_client(),
            ytdlp: tools.clone(),
            default_dir: default_dir.clone(),
            dirty: true,
        };
        let tx2 = tx.clone();
        let join = std::thread::Builder::new()
            .name("tree-engine".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .thread_name("tree-net")
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(runner.run(rx, tx2));
            })
            .expect("engine thread");

        Engine {
            tasks,
            settings,
            tx,
            ytdlp: tools,
            default_dir,
            join: Some(join),
        }
    }

    pub fn add(&self, url: &str) {
        let _ = self.tx.send(Cmd::Add(vec![url.to_string()]));
    }

    pub fn add_many(&self, urls: &[String]) {
        let _ = self.tx.send(Cmd::Add(urls.to_vec()));
    }

    pub fn pause(&self, id: u64) {
        let _ = self.tx.send(Cmd::Pause(id));
    }

    pub fn resume(&self, id: u64) {
        let _ = self.tx.send(Cmd::Resume(id));
    }

    pub fn remove(&self, id: u64, delete_file: bool) {
        let _ = self.tx.send(Cmd::Remove(id, delete_file));
    }

    pub fn pause_all(&self) {
        let _ = self.tx.send(Cmd::PauseAll);
    }

    pub fn resume_all(&self) {
        let _ = self.tx.send(Cmd::ResumeAll);
    }

    pub fn ytdlp_path(&self) -> Option<PathBuf> {
        lock(&self.ytdlp).clone()
    }

    /// Re-scan for yt-dlp after the user installs it from the Tools menu.
    pub fn refresh_tools(&self) {
        let _ = self.tx.send(Cmd::RefreshTools);
    }

    pub fn set_settings(&self, s: Settings) {
        let _ = self.tx.send(Cmd::Settings(s));
    }

    pub fn snapshot(&self) -> Vec<Task> {
        lock(&self.tasks).clone()
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn default_dir(&self) -> &Path {
        &self.default_dir
    }

    /// Ask the engine to checkpoint and flush, then wait for it to stop.
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

struct Runner {
    state: PathBuf,
    tasks: Arc<Mutex<Vec<Task>>>,
    settings: Arc<Mutex<Settings>>,
    active: HashMap<u64, Arc<TaskRun>>,
    speeds: HashMap<u64, Speedometer>,
    client: Client,
    ytdlp: Arc<Mutex<Option<PathBuf>>>,
    default_dir: PathBuf,
    dirty: bool,
}

impl Runner {
    async fn run(mut self, mut rx: UnboundedReceiver<Cmd>, tx: UnboundedSender<Cmd>) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut n: u32 = 0;
        self.start_queued(&tx);

        loop {
            tokio::select! {
                biased;
                msg = rx.recv() => {
                    let Some(cmd) = msg else { break };
                    if matches!(cmd, Cmd::Quit) {
                        // Pause (not cancel) so every connection checkpoints its offset.
                        for run in self.active.values() {
                            run.stop.store(PAUSE, Ordering::Relaxed);
                        }
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        self.save();
                        break;
                    }
                    self.handle(cmd, &tx);
                }
                _ = tick.tick() => {
                    n = n.wrapping_add(1);
                    self.mirror(n.is_multiple_of(4));
                    if n.is_multiple_of(8) && self.dirty {
                        self.save();
                    }
                    self.start_queued(&tx);
                }
            }
        }
    }

    fn handle(&mut self, cmd: Cmd, tx: &UnboundedSender<Cmd>) {
        match cmd {
            Cmd::Add(urls) => self.add(urls),
            Cmd::Pause(id) => {
                match self.active.get(&id) {
                    Some(run) => run.stop.store(PAUSE, Ordering::Relaxed),
                    None => {
                        if let Some(t) = lock(&self.tasks).iter_mut().find(|t| t.id == id) {
                            if t.status == Status::Queued || t.status == Status::Running {
                                t.status = Status::Paused;
                            }
                        }
                    }
                }
                self.dirty = true;
            }
            Cmd::Resume(id) => {
                {
                    let mut v = lock(&self.tasks);
                    if let Some(t) = v.iter_mut().find(|t| t.id == id) {
                        if matches!(t.status, Status::Paused | Status::Failed) {
                            t.status = Status::Queued;
                            t.error = None;
                        }
                    }
                }
                self.dirty = true;
            }
            Cmd::Remove(id, delete) => {
                if let Some(run) = self.active.get(&id) {
                    run.stop.store(CANCEL, Ordering::Relaxed);
                }
                self.active.remove(&id);
                self.speeds.remove(&id);
                let removed = {
                    let mut v = lock(&self.tasks);
                    v.iter()
                        .position(|t| t.id == id)
                        .map(|i| v.remove(i))
                };
                if let Some(t) = removed {
                    io::discard_part(Path::new(&t.dir), &t.name);
                    if delete {
                        let _ = std::fs::remove_file(t.path());
                    }
                }
                self.dirty = true;
            }
            Cmd::PauseAll => {
                for run in self.active.values() {
                    run.stop.store(PAUSE, Ordering::Relaxed);
                }
                for t in lock(&self.tasks).iter_mut() {
                    if matches!(t.status, Status::Queued | Status::Running) {
                        t.status = Status::Paused;
                    }
                }
                self.dirty = true;
            }
            Cmd::ResumeAll => {
                for t in lock(&self.tasks).iter_mut() {
                    if matches!(t.status, Status::Paused | Status::Failed) {
                        t.status = Status::Queued;
                        t.error = None;
                    }
                }
                self.dirty = true;
                self.start_queued(tx);
            }
            Cmd::Settings(s) => {
                let rate = s.speed_limit();
                *lock(&self.settings) = s;
                for run in self.active.values() {
                    run.bucket.set_rate(rate);
                }
                self.dirty = true;
                self.start_queued(tx);
            }
            Cmd::Finished { id, outcome } => {
                self.active.remove(&id);
                self.speeds.remove(&id);
                {
                    let mut v = lock(&self.tasks);
                    if let Some(t) = v.iter_mut().find(|t| t.id == id) {
                        t.speed = 0.0;
                        match outcome {
                            Outcome::Done => {
                                t.status = Status::Done;
                                t.error = None;
                                if t.total > 0 {
                                    t.done = t.total;
                                }
                            }
                            Outcome::Paused => t.status = Status::Paused,
                            Outcome::Failed(message) => {
                                t.status = Status::Failed;
                                t.error = Some(message);
                            }
                            Outcome::Cancelled => {}
                        }
                    }
                }
                self.dirty = true;
                self.start_queued(tx);
            }
            Cmd::RefreshTools => {
                *lock(&self.ytdlp) = resolve_ytdlp(&self.state);
                self.start_queued(tx);
            }
            Cmd::Quit => {}
        }
    }

    fn add(&mut self, urls: Vec<String>) {
        let settings = lock(&self.settings).clone();
        let dir = if settings.dir.trim().is_empty() {
            self.default_dir.to_string_lossy().into_owned()
        } else {
            settings.dir.clone()
        };
        let have_tool = lock(&self.ytdlp).is_some();
        let mut v = lock(&self.tasks);
        let mut next = v.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        for raw in urls {
            let url = raw.trim();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                continue;
            }
            // Never queue the same URL twice while it is still in flight.
            let duplicate = v.iter().any(|t| {
                t.url == url && matches!(t.status, Status::Queued | Status::Running | Status::Paused)
            });
            if duplicate {
                continue;
            }
            v.push(Task {
                id: next,
                url: url.to_string(),
                name: plan::name_from_url(url),
                dir: dir.clone(),
                total: 0,
                done: 0,
                supports_ranges: false,
                status: Status::Queued,
                kind: if have_tool && ytdlp::is_media_url(url) {
                    Kind::YtDlp
                } else {
                    Kind::Direct
                },
                error: None,
                created: now(),
                segments: Vec::new(),
                speed: 0.0,
            });
            next += 1;
        }
        drop(v);
        self.dirty = true;
    }

    fn start_queued(&mut self, tx: &UnboundedSender<Cmd>) {
        let settings = lock(&self.settings).clone();
        let mut slots = settings.simultaneous().saturating_sub(self.active.len());
        if slots == 0 {
            return;
        }
        let mut starting: Vec<Task> = Vec::new();
        {
            let mut v = lock(&self.tasks);
            for t in v.iter_mut() {
                if slots == 0 {
                    break;
                }
                if t.status == Status::Queued {
                    t.status = Status::Running;
                    t.error = None;
                    t.speed = 0.0;
                    starting.push(t.clone());
                    slots -= 1;
                }
            }
        }
        for task in starting {
            let bucket = Arc::new(Bucket::new(settings.speed_limit()));
            let run = Arc::new(TaskRun::new(&task, self.client.clone(), bucket));
            self.active.insert(task.id, run.clone());
            let tasks = self.tasks.clone();
            let tx = tx.clone();
            let st = settings.clone();
            let tools = lock(&self.ytdlp).clone();
            tokio::spawn(async move {
                let outcome = run_task(&run, &st, &tasks, tools).await;
                let _ = tx.send(Cmd::Finished {
                    id: run.id,
                    outcome,
                });
            });
            self.dirty = true;
        }
    }

    fn mirror(&mut self, with_speed: bool) {
        let mut v = lock(&self.tasks);
        for t in v.iter_mut() {
            match self.active.get(&t.id) {
                Some(run) => {
                    let g = lock(&run.inner);
                    let done: u64 = g.seg_done.iter().sum();
                    t.name = g.name.clone();
                    t.total = g.total;
                    t.done = done;
                    if t.segments.len() != g.bounds.len() {
                        t.segments = g
                            .bounds
                            .iter()
                            .zip(g.seg_done.iter())
                            .map(|(&(start, end), &d)| Segment { start, end, done: d })
                            .collect();
                    } else {
                        for ((seg, &(start, end)), &d) in t
                            .segments
                            .iter_mut()
                            .zip(g.bounds.iter())
                            .zip(g.seg_done.iter())
                        {
                            seg.start = start;
                            seg.end = end;
                            seg.done = d;
                        }
                    }
                    if with_speed {
                        let speed = self.speeds.entry(t.id).or_default().sample(done);
                        t.speed = speed;
                    }
                }
                None if !matches!(t.status, Status::Done) => t.speed = 0.0,
                None => {}
            }
        }
    }

    fn save(&mut self) {
        let settings = lock(&self.settings).clone();
        let tasks = lock(&self.tasks).clone();
        let _ = store::save(&self.state, &settings, &tasks);
        self.dirty = false;
    }
}

async fn run_task(
    run: &Arc<TaskRun>,
    settings: &Settings,
    tasks: &Arc<Mutex<Vec<Task>>>,
    tool: Option<PathBuf>,
) -> Outcome {
    // Media sites need yt-dlp's extractors; when the binary is missing we fall
    // back to a plain ranged download of whatever the URL returns.
    if run.kind == Kind::YtDlp {
        if let Some(exe) = tool {
            return run_ytdlp(run, tasks, exe).await;
        }
    }
    let needs_probe = lock(&run.inner).bounds.is_empty();

    if needs_probe {
        let probed = match probe::probe(&run.client, &run.url).await {
            Ok(p) => p,
            Err(e) => return Outcome::Failed(e.user_message()),
        };
        if run.stopped() != RUN {
            return stopped(run);
        }
        let desired = probed
            .name
            .clone()
            .unwrap_or_else(|| plan::name_from_url(&run.url));
        let name = plan::unique_name(&run.dir, &desired);
        let count = plan::segment_count(probed.total, settings.conns(), probed.ranges);
        let segments = plan::split_segments(probed.total, count);
        {
            let mut g = lock(&run.inner);
            g.name = name.clone();
            g.total = probed.total;
            g.bounds = segments.iter().map(|s| (s.start, s.end)).collect();
            g.seg_done = vec![0; segments.len()];
            g.owner = (0..segments.len()).collect();
        }
        {
            let mut v = lock(tasks);
            if let Some(t) = v.iter_mut().find(|t| t.id == run.id) {
                t.name = name;
                t.total = probed.total;
                t.supports_ranges = probed.ranges;
                t.segments = segments;
            }
        }
        if let Err(e) = io::create_part(&run.part(), probed.total) {
            return Outcome::Failed(DlError::from(e).user_message());
        }
    }

    if run.stopped() != RUN {
        return stopped(run);
    }

    let count = lock(&run.inner).bounds.len();
    let mut handles = Vec::with_capacity(count);
    for i in 0..count {
        handles.push(tokio::spawn(worker::run_segment(run.clone(), i)));
    }

    let mut failure: Option<DlError> = None;
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if failure.is_none() {
                    failure = Some(e);
                }
                // stop the sibling connections at their next checkpoint
                run.stop.store(PAUSE, Ordering::Relaxed);
            }
            Err(_join) => {}
        }
    }

    if let Some(e) = failure {
        return Outcome::Failed(e.user_message());
    }
    if run.stopped() != RUN {
        return stopped(run);
    }

    let total = run.total();
    let done = run.done();

    if total == 0 {
        if done == 0 {
            return Outcome::Failed("Server sent no data".into());
        }
        // Unknown length: the received size becomes the final size.
        lock(&run.inner).total = done;
        {
            let mut v = lock(tasks);
            if let Some(t) = v.iter_mut().find(|t| t.id == run.id) {
                t.total = done;
            }
        }
        return match io::finalize(&run.part(), &run.path(), 0) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(DlError::from(e).user_message()),
        };
    }

    if done >= total {
        match io::finalize(&run.part(), &run.path(), total) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(DlError::from(e).user_message()),
        }
    } else {
        Outcome::Failed(DlError::Incomplete.user_message())
    }
}

fn stopped(run: &TaskRun) -> Outcome {
    match run.stopped() {
        CANCEL => Outcome::Cancelled,
        _ => Outcome::Paused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_big_ranges_only() {
        assert!(split_point(0, 100).is_none());
        // 2 MiB cannot yield two pieces of 1 MiB + margin each.
        assert!(split_point(0, 2 << 20).is_none());
        assert!(split_point(0, 4 << 20).is_some());
        let p = split_point(0, 40 << 20).expect("40 MiB splits");
        assert_eq!(p % ALIGN, 0, "boundaries are aligned");
        assert!(p >= MIN_PIECE && p <= (40 << 20) + 1 - MIN_PIECE);
    }

    #[test]
    fn never_splits_an_open_ended_stream() {
        assert!(split_point(0, u64::MAX).is_none());
    }

    fn sample_task() -> Task {
        Task {
            id: 1,
            url: "https://example.com/a.bin".into(),
            name: "a.bin".into(),
            dir: ".".into(),
            total: 100,
            done: 40,
            supports_ranges: true,
            status: Status::Queued,
            kind: Kind::Direct,
            error: None,
            created: 0,
            segments: vec![
                Segment {
                    start: 0,
                    end: 49,
                    done: 40,
                },
                Segment {
                    start: 50,
                    end: 99,
                    done: 0,
                },
            ],
            speed: 0.0,
        }
    }

    #[test]
    fn resumed_offsets_come_from_the_model() {
        let run = TaskRun::new(&sample_task(), crate::http_client(), Arc::new(Bucket::new(0)));
        assert_eq!(run.offset(0), 40);
        assert_eq!(run.offset(1), 50);
        assert_eq!(run.done(), 40);
        assert_eq!(run.bounds(1), (50, 99));
        assert!(run.part().to_string_lossy().ends_with("a.bin.tree-part"));
    }
}


/// Where yt-dlp lives: beside the exe (portable), in our data folder, or on PATH.
fn resolve_ytdlp(state_path: &Path) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let data_dir = state_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    ytdlp::find(&exe_dir, &data_dir)
}

/// yt-dlp is a subprocess: a watcher thread copies its counters into the shared
/// run state so the table, tray and taskbar keep updating; pause kills it, and
/// `--continue` makes the restart a resume.
async fn run_ytdlp(run: &Arc<TaskRun>, tasks: &Arc<Mutex<Vec<Task>>>, exe: PathBuf) -> Outcome {
    let url = run.url.clone();
    let dir = run.dir.clone();

    {
        // yt-dlp knows the real title, which is far more useful than the URL slug
        // the task was created with.
        let current = lock(&run.inner).name.clone();
        let guessed = tokio::task::spawn_blocking({
            let exe = exe.clone();
            let url = url.clone();
            move || ytdlp::query_name(&exe, &url)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or(current);
        let mut g = lock(&run.inner);
        g.name = guessed;
        g.bounds = vec![(0, u64::MAX)];
        g.seg_done = vec![0];
    }

    let ctl = Arc::new(ytdlp::Ctl::default());
    let finished = Arc::new(AtomicBool::new(false));
    let watcher = {
        let run = run.clone();
        let ctl = ctl.clone();
        let finished = finished.clone();
        std::thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                if run.stopped() != RUN {
                    ctl.stop.store(PAUSE, Ordering::Relaxed);
                }
                let done = ctl.done.load(Ordering::Relaxed);
                let total = ctl.total.load(Ordering::Relaxed);
                {
                    let mut g = lock(&run.inner);
                    if total > 0 {
                        g.total = total;
                    }
                    if g.seg_done.is_empty() {
                        g.bounds = vec![(0, u64::MAX)];
                        g.seg_done = vec![0];
                    }
                    g.seg_done[0] = done;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        })
    };

    let result = tokio::task::spawn_blocking({
        let ctl = ctl.clone();
        move || ytdlp::download(&exe, &url, &dir, &ctl)
    })
    .await;

    finished.store(true, Ordering::Relaxed);
    let _ = watcher.join();

    match result {
        Ok(Ok(path)) => {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| lock(&run.inner).name.clone());
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            {
                let mut g = lock(&run.inner);
                g.name = name.clone();
                g.total = size;
                g.bounds = vec![(0, size.max(1) - 1)];
                g.seg_done = vec![size];
            }
            {
                let mut v = lock(tasks);
                if let Some(t) = v.iter_mut().find(|t| t.id == run.id) {
                    t.name = name;
                    t.total = size;
                    t.done = size;
                }
            }
            Outcome::Done
        }
        Ok(Err(DlError::Cancelled)) => stopped(run),
        Ok(Err(e)) => Outcome::Failed(e.user_message()),
        Err(_) => Outcome::Failed("yt-dlp stopped unexpectedly".into()),
    }
}
