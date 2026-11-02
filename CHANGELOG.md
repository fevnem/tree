# Changelog

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
