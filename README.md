# Rayo ⚡

[![CI](https://github.com/PabloGonSan8/rayo-terminal/actions/workflows/ci.yml/badge.svg)](https://github.com/PabloGonSan8/rayo-terminal/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/PabloGonSan8/rayo-terminal)](https://github.com/PabloGonSan8/rayo-terminal/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)

**English** · [Español](README.es.md)

A native Windows terminal written in Rust, built for PowerShell and for running AI CLIs:
Warp-style command blocks without an account, a Ctrl+C that really cleans up after itself,
and a single `.exe`.

No AI built in, no login, no telemetry. If you run agents like Claude Code, Codex or Gemini CLI,
Rayo is a good host for them (Shift+Enter, pasted images, an agents panel), but it never needs one.

- VT engine: `alacritty_terminal` (ConPTY on Windows)
- UI: `egui` / `eframe` (OpenGL, glow)
- Language: Settings → Appearance → Language (Automatic follows the Windows display language).

> Platforms: **Windows 10 (1809+) / 11** only. Linux and macOS are not supported yet.

## Installation

### Option A — download (no tools needed)

In PowerShell (no admin). Installs to `%LOCALAPPDATA%\Programs\Rayo`, adds `rayo` to PATH, a Start menu shortcut, **Open in Rayo** on folder right click, and an entry in Settings → Apps:

```
irm https://raw.githubusercontent.com/PabloGonSan8/rayo-terminal/main/install.ps1 | iex
```

Or by hand:

1. Go to [Releases](https://github.com/PabloGonSan8/rayo-terminal/releases/latest) and download `rayo.exe`.
2. Put it in any folder (e.g. `%LOCALAPPDATA%\Programs\Rayo`) and run it.
3. Rayo tells you when a new version is available. The in-app updater only installs a release whose SHA-256 matches the `rayo.exe.sha256` published with it.

To check a download yourself: `Get-FileHash rayo.exe -Algorithm SHA256` must match `rayo.exe.sha256`
from the same release, and `gh attestation verify rayo.exe -R PabloGonSan8/rayo-terminal` checks it
was built by this repo's release workflow. The exe isn't code-signed yet, so SmartScreen may warn: *More info → Run anyway*.

### Option B — build from source

Requirements: [Rust](https://rustup.rs) (stable) and the Visual Studio *Build Tools* (C++ workload). Then:

```
git clone https://github.com/PabloGonSan8/rayo-terminal
cd rayo-terminal
powershell -ExecutionPolicy Bypass -File install.ps1
```

This builds Rayo, copies `rayo.exe` to `%LOCALAPPDATA%\Programs\Rayo` and adds a "Rayo" Start menu shortcut (no admin needed).
To pin it: Start menu → Rayo → right click → **Pin to taskbar**. To update: `git pull` and run the script again (even while Rayo is open), then restart Rayo.

Run without installing: `cargo run --release` (the exe ends up in `target\release\rayo.exe`).

### Uninstall

Settings → Apps → Rayo → Uninstall (removes the exe, PATH entry, right click menu and shortcut). Settings and history stay in `%APPDATA%\rayo`; delete it if you don't want them.

## Command line

```
rayo                 # opens Rayo (restores the session)
rayo C:\project      # new window in that folder
rayo -d C:\project   # same, Windows Terminal style
```

## Development

- `cargo test` runs the tests.
- CI (`.github/workflows/ci.yml`) runs clippy, the tests and a release build on Windows for every PR and every push to `main`.
- Enable the hook that blocks `git push` when tests fail (once per clone):
  ```
  git config core.hooksPath .githooks
  ```
- To stop GitHub from merging into `main` without green tests: Settings → Branches → rule for `main` → *Require status checks* → `test (windows-latest)`.

## Command blocks (PowerShell / pwsh)

- Built-in editor pinned at the bottom: Enter runs, Shift+Enter inserts a new line, ↑/↓ history (saved in `%APPDATA%\rayo\history.txt`), Tab completes paths, → accepts the grey suggestion.
- Every command is a block: directory, git branch, command, output, status (✓ / ✗ code / Interrupted) and duration.
- While a command runs, keys go to the process (Ctrl+C interrupts). Full-screen apps (vim, less, fzf…) switch to a full terminal view and go back to blocks on exit.
- Right click a block: copy command / output, or edit it again. `cls` or Ctrl+L clear the blocks.
- cmd, WSL and Git Bash work as a classic terminal.

How it works: Rayo starts PowerShell with its own prompt that emits OSC 133 marks (end + exit code, ready) and every command is sent wrapped to mark where its output starts. A custom I/O loop over ConPTY captures each block exactly when it ends.

## Settings

Gear icon at the top right or `Ctrl + ,`. It has a search box: type "font", "cursor", "shell"…
Everything is saved automatically to `%APPDATA%\rayo\config.toml` ("Open file" button to edit it by hand).

## Shortcuts

| Action | Shortcut |
|---|---|
| Settings | `Ctrl + ,` |
| New tab | `Ctrl + Shift + T` |
| Close pane | `Ctrl + Shift + W` |
| Split right / down | `Ctrl + Shift + D` / `Ctrl + Shift + E` |
| Maximize pane | `Ctrl + Shift + Enter` |
| Move focus | `Ctrl + Alt + Arrows` |
| Switch tab | `Ctrl + Tab`, `Ctrl + 1…9` |
| Copy / paste | `Ctrl + C` (with selection) / `Ctrl + V`, right click |
| Font zoom | `Ctrl + =` / `Ctrl + -` / `Ctrl + 0` |
| Search history (editor) | `Ctrl + R` |
| Find in block output | `Ctrl + F` |

Drag the dividers between panes to resize them. Shortcuts are editable in Settings → Shortcuts.

## Features

- **Editable shortcuts**: click a shortcut and press the new combination (Del = unassigned, Esc = cancel).
- **Time-based theme**: Settings → Appearance (day/night theme and switch times).
- **Links**: Ctrl + click opens URLs (and OSC 8 links).
- **Real Ctrl+C**: each pane runs in a Windows Job Object; interrupting kills processes the shell leaves orphaned (e.g. `node` after `pnpm dev`) and closing a pane closes everything it started.
- **Light at rest**: OpenGL rendering, LTO; 0 % CPU when idle. RAM: the Rayo process with one PowerShell tab measured between ~75 and ~140 MB working set (`Get-Process rayo`, release build, Windows 11; it varies with window and GPU state).
- **Safe closing**: closing a pane, tab or the window while a command runs asks first (Settings → Terminal → Confirm on close).
- **Private history**: a command that starts with a space isn't saved to the history (like bash's `ignorespace`).
- **Notify when done**: the taskbar flashes when a command of 10 s or more ends while Rayo is in the background.
- **Crash log**: if Rayo ever crashes, the details go to `%APPDATA%\rayo\crash.log`. Attach it to the bug report.
- **Session**: tabs, splits and each pane's folder come back when you reopen Rayo.

## Contributing

PRs welcome: read [CONTRIBUTING.md](CONTRIBUTING.md). Every PR needs green CI and the maintainer's approval.

## License

[MIT](LICENSE)
