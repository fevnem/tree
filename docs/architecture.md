# Tree — architecture

Two crates, one rule: `tree-core` knows nothing about Windows, `tree-win` knows
nothing about HTTP.

```
crates/tree-core        engine — compiles and is tested on Linux
crates/tree-win         native shell — the only unsafe code in the project
```

## Threads

```
UI thread (Win32 message loop)
  WM_TIMER 250 ms  →  engine.snapshot()  →  diff rows  →  repaint
  WM_COMMAND       →  engine.cmd(...)    →  unbounded channel
  WM_DROPFILES     →  add(urls)
  WM_COPYDATA      →  add(url) from a second instance
  WM_SETTINGCHANGE →  retheme

engine thread (owns the tokio runtime, 4 worker threads)
  select! { cmd = rx.recv() => ... , _ = tick(250 ms) => mirror + persist }
  └─ per task: one supervisor task
       ├─ probe (first start only)
       ├─ N segment workers (one per connection)
       └─ finalize: truncate → sync → rename .tree-part
```

Six threads total at idle: one UI, four network, one engine loop. Nothing else.

## Invariants

1. The engine is the only writer of `state.json` and the only place a task's
   status changes. The UI sends commands and reads snapshots.
2. The UI takes a **clone** of the task list per tick; it never holds the model
   lock while painting.
3. One ranged connection = one tokio task. No OS thread per connection.
4. The UI thread never blocks on network or disk; the engine never calls Win32.
5. `write_at` is a positioned write, so N connections write one file with no
   shared cursor and no lock in the write path.

## Status machine

```
add → Queued ──slot free──► Running ──all bytes──► Done
        │                     │  │
        │ pause               │  └──retries exhausted──► Failed
        ▼                     ▼                            │
      Paused ◄──── pause ─────┘                            │ resume
        │                                                  │
        └──────────── resume ──────────────────────────────┘

any → Removed   (+ optional delete of part file / final file)
```

`Settings::simultaneous` (default 3) tasks run at once; the rest wait as
`Queued` in creation order.

## Resume math

Segments are persisted as `{start, end, done}`. A worker always writes at
`start + done`, so:

* pause = stop between chunks, offsets stay exact;
* crash = lose at most the last 250 ms of counters (persisted every 2 s);
* resume = `Range: bytes=(start+done)-end`, no re-verification, no restart.

Finishing is `set_len(total)` → `sync_all()` → `rename(tree-part → final)`,
so a half-downloaded file can never look complete.

## Probing

One `GET` with `Range: bytes=0-0`:

| Observation | Consequence |
|---|---|
| `206` + `Content-Range: bytes 0-0/N` | size = N, ranges supported |
| `200` + `Content-Length` | single stream, size known |
| `416` | empty file or refused range → `HEAD` fallback |
| no `Content-Length`, no ranges | single stream, size discovered at EOF |

If a server later answers `200` to a non-zero range request, the worker fails the
segment with `RangeUnsupported` instead of writing garbage at the wrong offset.

## Why these limits

* **8 connections** default, 16 ceiling. Beyond ~8, per-connection overhead and
  server politeness dominate on real links; a 1 MiB floor and one connection per
  2 MiB keep small files from spawning pointless sockets.
* **64 KiB chunks**: one `Bytes` buffer per connection, so RSS during an
  eight-way download stays flat instead of tracking file size.
* **Preallocation** via `set_len` fails *before* the transfer on a full disk,
  which is the only place a "disk full" error is cheap. If preallocation is
  refused (exFAT, network share), writes grow the file in 8 MiB steps.
