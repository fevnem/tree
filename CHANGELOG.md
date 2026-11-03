# Changelog

## 0.6.0 — 2026-11-03

Driven by the ten research reports (engine, layout, tokens, queues, browser
protocol, yt-dlp, census, packaging, Win32, trackers).

* **Security: the bridge is no longer a download-CSRF hole.** `GET /add?url=…` is
  gone — a cross-origin GET needs no preflight, so any web page could have queued
  downloads. The endpoint is now `POST /v1/downloads` with a `X-Tree-Client`
  header, which forces a CORS preflight that is answered only for extension
  origins. `X-Tree-Client` is required even past the preflight.
* **Layout, measured rather than guessed** (median of nine shipping managers):
  rows 52 → 32 px, progress bar 10 → 4 px pill with the percentage outside,
  status strip 28 → 24 px, sparkline 180×16 → 64×14, detail pane 116 → 128 px with
  20×9 per-connection cells and 8 px gaps.
* **Palette** now uses the researched dark tokens: `#0D0F12` background,
  `#14171C` rows, `#E6E9EF` text, `#9AA4B2` secondary, `#4C8DFF` accent.
* **yt-dlp**: explicit `-f bv*+ba/b`, `--merge-output-format mp4` and
  `--ffmpeg-location`; a missing output file is now an error even when yt-dlp
  exits 0 and prints the path (the missing-ffmpeg case), instead of a silent
  "done" with fragmented files on disk.
* **One source of truth for the version**: `scripts/version.sh` rewrites the
  `.rc` and installer from `Cargo.toml`, refuses a mismatched release tag and
  requires a matching CHANGELOG section — this class of drift already shipped a
  wrong archive once.
* Tests: 4 new bridge tests (JSON reading, escapes, origin gating).

## 0.5.1 — 2026-11-03

* **Authenticated downloads.** The browser bridge now accepts `cookie`, `referer`
  and `ua` parameters and applies them to the probe and every segment request, so
  links behind a session work. Verified against a server that returns 403 without
  the cookie: anonymous request fails honestly, the same URL with the cookie
  downloads to 100 %.
* Requests from the extension are applied per link, so a page with several hosts
  keeps each host's own cookies.

## 0.5.0 — 2026-11-03

Researched 120 download-manager repositories (616 swept) and closed the gaps the
field considers basic.

* **Adaptive segmentation.** Connections claim work instead of being handed a
  fixed slice; a connection that runs dry steals half of the largest remaining
  range, with Vise-style margins (1 MiB minimum piece, 256 KiB margin, 64 KiB
  alignment, 64 segment cap). Verified on a deliberately lopsided link: a 300 MiB
  transfer spread from 8 to **19 segments** and the finished file still hashed
  byte-identical to the source.
* **File types.** The sidebar gained a `FILE TYPES` section — All types, Video,
  Music, Documents, Programs, Compressed, Other — with coloured dots and live
  counts, filtering the table (XDM's model).
* **Rows** are 52 px with pill-shaped status badges and 40 px type badges.
* **Quiet hours (scheduler).** View ▸ Quiet hours, window editable in a dialog
  (`02:00`/`0800`/`7` all parse). Transfers pause inside the window and the tasks
  this feature paused resume after it — verified pausing at 6 s and finishing
  after the window closed. A manual pause is never undone.
* **Dialogs follow the palette**: owner-drawn buttons, themed labels and edits, so
  the Add and Quiet-hours windows are no longer white boxes on a dark app.
* **yt-dlp**: a complete transfer whose only complaint is `Unable to rename file`
  (target already claimed) is now reported as done instead of failed.

## 0.4.0 — 2026-11-03

The window is rebuilt around what the established managers do, after reading the
source of Vise, DownloadCraft and gosh-dl and the docs of aria2, Motrix and XDM.

* **Per-row progress bars** with percentage, speed and ETA in the table, plus a
  file-type badge per row.
* **Sidebar** with All / Downloading / Queued / Completed / Failed / Paused and
  live counts; clicking filters the table.
* **Detail pane** showing the file, its URL, the error if any, and a **per-
  connection grid** — one cell per segment, so a multi-connection transfer is
  visible instead of implied.
* **Status strip** with total throughput, a 60-second **speed sparkline**, and
  the queue summary.
* **Labelled toolbar** (Add, Pause, Resume, Remove, Folder, Open) with hover and
  disabled states driven by the selection.
* **Selection follows the transfer** until you pick a row yourself.
* Empty state with a call to action instead of a blank table.
* Dark and light palettes rebuilt around the same painting code, so nothing is
  half-themed (header, rows, chrome and the area below the last row).
* Fixed: pure-black band below the last row, wrong COLORREF channel order in
  status colours, `WM_MOUSELEAVE` silently defined as a catch-all match arm that
  swallowed every later message, stale selection after a rebuild.

## 0.3.2 — 2026-11-02

* **Fixed: no tray icon.** `load_icon` passed a dangling pointer as the resource
  name, so both window and tray icons silently fell back to the generic system
  icon. It now uses `MAKEINTRESOURCE(1)`; the tray shows Tree's own icon.
* Tray registration logs its result under `TREE_DEBUG=1` (`tray: add ok=… icon=…`).

## 0.3.1 — 2026-11-02

* **Engine:** respect `Retry-After` on 429/503 instead of hammering a busy server.
* **Engine:** verify the `Content-Range` start of every 206 response matches the
  requested offset; a server answering with a different range now aborts the
  segment instead of writing it at the wrong place.
* Unit tests for the range header parser.

## 0.3.0 — 2026-11-02

* **New:** loopback browser bridge on `127.0.0.1:17817` (`/add?url=…`, `/ping`),
  used by the Tree Downloader extension for Chrome/Edge/Brave and Firefox.
  Binds to loopback only; if the port is taken the app still runs.

## 0.2.0 — 2026-11-02

* **Fixed:** the Add dialog's buttons did nothing — its window procedure had no
  `WM_COMMAND` handler.
* **Fixed:** a second launch opened a second window, tray icon and engine. One
  instance only; later launches hand their links to the running window.
* **Fixed:** a leftover window from a killed process could block startup.
* **Fixed:** column widths grew on every launch at 125 %/150 % DPI.
* **Fixed:** the default sort order was reverse-alphabetical.
* **Fixed:** closing the app could leave it invisible with no tray icon
  (close-to-tray now defaults off, and only hides when the icon exists).
* **New:** ten themes with accent colours; dark palettes repaint header, rows,
  buttons, status bar and progress bar (custom-draw where Windows refuses to
  theme stock controls).
* **New:** yt-dlp integration for video/audio links, including a
  Tools ▸ Download yt-dlp action.
* **New:** window size/position remembered; Name column fills the empty space;
  progress bar hides when there is nothing to show; newly added rows are
  selected; status bar counts paused and failed items.
* **New:** startup breadcrumbs behind `TREE_DEBUG=1`.

## 0.1.0 — 2026-11-02

First release.

* Native Win32 shell: menu bar, toolbar, `SysListView32` report view, progress
  bar, three-part status bar, real modal add dialog, system folder picker.
* Parallel ranged downloads: 8 connections per file by default, up to 16,
  positioned writes into a preallocated part file.
* Byte-exact resume from checkpointed segment offsets, including after a crash.
* Queue with a configurable number of simultaneous downloads.
* Duplicate-safe naming (`file (2).ext`), Content-Disposition and RFC 5987
  filename parsing, reserved-name sanitising.
* Drag-and-drop, clipboard watch, `tree.exe <url>`, single-instance URL handoff.
* Taskbar progress, tray icon, dark title bar following the system theme,
  per-monitor DPI, persisted column widths and sort order.
* Portable mode via `tree.ini`.
* Cross-built from Linux with cargo-xwin; NSIS installer and portable zip.
