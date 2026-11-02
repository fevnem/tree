# Changelog

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
