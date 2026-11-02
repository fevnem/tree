# Tree — the interface

Stock Win32 controls only: menu bar, `BUTTON`, `SysListView32` (report view),
`EDIT`, `msctls_progress32`, `msctls_statusbar32`. Nothing is owner-drawn, so
Tree inherits the system theme, high-contrast mode and DPI behaviour instead of
imitating them.

## Main window

```
┌ Tree — Download Manager ────────────────────────────────────── ─ □ ✕ ┐
│ File   Downloads   View   Help                                        │
├───────────────────────────────────────────────────────────────────────┤
│ [ Add ] [ Pause ] [ Resume ] [ Remove ] [ Folder ]     [ Filter…    ] │
├───────────────────────────────────────────────────────────────────────┤
│ Name                 │ Size    │ Progress │ Speed     │ Status  │ ETA │
│ ubuntu-24.04.2.iso   │ 5.7 GB  │   63 %   │ 11.2 MB/s │ Downl…  │ 3:10│
│ report.pdf           │ 2.4 MB  │  100 %   │ —         │ Completed│  — │
│ dataset.tar.zst      │ 812 MB  │    0 %   │ —         │ Queued  │  — │
├───────────────────────────────────────────────────────────────────────┤
│ [███████████████░░░░░░░░░░░░░░] 63 %   ubuntu.iso · 11.2 MB/s · 3:10 │
├───────────────────────────────────────────────────────────────────────┤
│ 3 active · 12.4 MB/s · 2 queued · 1 done  │ selected item  │ 58 GB free│
└───────────────────────────────────────────────────────────────────────┘
```

* Status bar part 1 = queue summary, part 2 = selection detail (or the error
  sentence when a task failed), part 3 = free space on the target drive.
* The progress bar follows the selection; with nothing selected it shows the
  aggregate of the whole queue.
* Buttons enable themselves from the selection: Pause needs something running,
  Resume needs something paused or failed.

## Add dialog (modal)

```
┌ Add download ───────────────────────────────── □ ✕ ┐
│ URL (one per line):                                │
│ [ https://…                                      ] │
│ Save to:                                           │
│ [ C:\Users\R\Downloads             ] [ Browse… ]   │
│                                    [ Add ] [Cancel]│
└────────────────────────────────────────────────────┘
```

* Owner window is disabled: it is a real modal, with Tab order, Enter on the
  default button and Esc to cancel (`IsDialogMessageW`).
* Newline-separated links are added in one go.
* The clipboard link is pre-filled and pre-selected: Ctrl+N, Enter.
* Browse opens the system folder picker.

## Ways to add a download

| Gesture | Result |
|---|---|
| `Ctrl+N` | add dialog |
| `Ctrl+V` in the window | add the URL on the clipboard |
| drag a link from a browser | added |
| drop a `.txt` / `.url` file | every link inside is added |
| copy a link anywhere (watch enabled) | queued quietly, never steals focus |
| `tree.exe <url>` / second launch | handed to the running window via `WM_COPYDATA` |

## Keys

| Key | Action |
|---|---|
| `Ctrl+N` | add |
| `Ctrl+V` | paste-add |
| `Ctrl+F` | focus the filter |
| `Space` | pause / resume the selection |
| `Del` | remove from the list (file kept) |
| `Enter` | open the selected file |
| `F5` | retry / resume |
| `Alt+F4` | exit (or minimize to tray) |

## Menus

* **File** — Add URL…, Open download folder, Exit
* **Downloads** — Open file, Copy URL, Pause, Resume, Pause all, Resume all,
  Remove, Remove and delete file
* **View** — Connections per file (1/2/4/8/16), Speed limit
  (Unlimited/1/5/10/25 MB/s), Theme (System/Light/Dark), Watch clipboard,
  Minimize to tray
* **Help** — About

Check marks reflect the stored settings; the menu is rebuilt on every change so
it cannot drift from the model.

## Shell integration

* **Taskbar progress** — aggregate percentage on the taskbar button; yellow when
  paused, red when a task failed.
* **Tray icon** — tooltip shows the aggregate percentage; right-click gives
  Pause all / Resume all / Open / Exit. Closing the window minimizes to tray
  (toggleable).
* **Dark mode** — follows the system, live-switches on `WM_SETTINGCHANGE`
  (immersive dark title bar + `DarkMode_Explorer` for the list), and degrades to
  the light theme on Windows versions without the attribute.
* **Portable mode** — drop a `tree.ini` next to `tree.exe` and state stays in
  that folder instead of `%APPDATA%\Tree`.
