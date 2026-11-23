# Launch posts (English)

Drafts for announcing Rayo. Post them **after** Phase 2 of the [roadmap](../ROADMAP.md): signed exe
(or at least the SHA-256 release), WinGet, and a GIF in the README. One channel per day, answer every
comment for the first hours, and never post numbers you haven't measured.

**Checklist before any post**

- [ ] New release out with `rayo.exe.sha256` (and ideally signed).
- [ ] README GIF: a block run, Ctrl+C killing an orphaned `node` after `pnpm dev`, a split with Claude Code.
- [ ] Website matches the app (no features the app doesn't have, no "macOS & Linux soon" promise).
- [ ] Repo topics set, Discussions on, 3–5 `good first issue` issues open.
- [ ] RAM/CPU figures re-measured on the release build, with the method stated.

---

## Show HN

**Title:** Show HN: Rayo – a native Windows terminal with command blocks, in Rust

**Text:**

I've been building Rayo, a terminal for Windows 10/11 written in Rust (egui + alacritty_terminal over ConPTY).

What it does differently:

- Command blocks for PowerShell: every command is a block with its folder, git branch, output, exit code and duration. No account, no login, no telemetry.
- Ctrl+C that actually cleans up. On Windows, Ctrl+C often kills `pnpm.cmd` while the real `node` child keeps running and writing to your console. Each pane in Rayo runs inside a Job Object, so after an interrupt the orphans are terminated, and closing a pane ends everything it started.
- It works well as a host for AI CLIs (Claude Code, Codex, Gemini CLI): Shift+Enter for new lines, pasted images become file paths, and a small panel lists the agents running in your panes. There's no AI inside Rayo itself.
- One `.exe`, per-user install, Explorer "Open in Rayo", session restore.

Limitations, honestly: Windows only; blocks need PowerShell for now (cmd/WSL/Git Bash run as a classic terminal); Rayo replaces PSReadLine with its own editor, so PowerShell's own completion isn't there yet.

Install: `winget install …` / or download `rayo.exe` from Releases (SHA-256 published with each release).

I'd love feedback on the shell integration approach (OSC 133 marks from a replaced `prompt` + wrapped commands) and on what would make you switch from Windows Terminal.

---

## r/rust

**Title:** Rayo: a Windows terminal in Rust with Warp-style command blocks (egui + alacritty_terminal + ConPTY)

**Body:**

Rayo is an MIT-licensed terminal for Windows built on `alacritty_terminal` for the VT engine and `egui`/`eframe` (glow) for the UI.

Some implementation details people here might find interesting:

- A custom I/O loop over ConPTY that splits the byte stream at our own OSC 133 marks, so the boundaries of each command block are exact (the scanner handles marks split across reads).
- Windows Job Objects per pane (`windows-sys`) to find and kill orphaned children after Ctrl+C.
- Finished blocks are frozen into compact styled runs instead of keeping terminal cells.
- Off-screen panes are throttled to a few frames per second; the cursor blink wakes exactly at the next edge, so an idle window draws nothing.
- No HTTP crate: updates use Windows' bundled `curl.exe` and `certutil` for the SHA-256 check.

Repo: https://github.com/PabloGonSan8/rayo-terminal — issues labelled `good first issue` are a good place to start. Feedback on the architecture is very welcome.

---

## r/PowerShell / r/Windows / r/commandline

**Title:** I made a free Windows terminal where every PowerShell command is a block (folder, branch, exit code, duration)

**Body:**

Rayo is a free, open-source terminal for Windows 10/11. Each PowerShell command becomes a block you can copy, re-run or edit; failed commands are marked with their exit code. Tabs, splits, 6 themes, path completion, session restore, "Open in Rayo" in Explorer.

The feature I'm proudest of: Ctrl+C that also kills the processes `npm`/`pnpm` leave behind, so your dev server doesn't keep printing after you stopped it.

No account, no telemetry, no built-in AI. Install with one line (`irm …/install.ps1 | iex`, checksum verified) or grab the single exe from Releases.

What's missing that would make it your daily terminal?

---

## DEV Community (article outline)

**Title:** Fixing Ctrl+C on Windows: how my terminal kills orphaned `node` processes with Job Objects

1. The bug everyone has seen: `pnpm dev`, Ctrl+C, prompt is back… and the server keeps logging.
2. Why: the `.cmd` wrapper dies, `node` survives with a dead parent PID, so a parent/child walk misses it.
3. Job Objects: every child joins the shell's job automatically; `JobObjectBasicProcessIdList` lists them, `KILL_ON_JOB_CLOSE` ends them when the pane closes.
4. Don't kill the console host (`conhost` / `OpenConsole`).
5. Code walk-through (`src/procs.rs`, ~150 lines) and the test that reproduces the orphan.
6. Where it lives: Rayo, link, call for contributors.

---

## X / Bluesky thread

1/ I built Rayo, a native Windows terminal in Rust. Every PowerShell command is a block: folder, git branch, output, exit code, duration. No account, no telemetry. 🧵 [GIF]
2/ Ctrl+C on Windows is broken for dev servers: the wrapper dies, `node` keeps running. Rayo puts each pane in a Job Object and cleans up the orphans. [GIF]
3/ It's a comfy home for AI CLIs (Claude Code, Codex…): Shift+Enter, paste images as paths, a panel with the agents running in your panes. No AI inside Rayo.
4/ One .exe, MIT, Windows 10/11. Feedback and first PRs welcome → github.com/PabloGonSan8/rayo-terminal

---

## Other places

- **This Week in Rust**: submit the repo or the DEV article to the "Project/Tooling Updates" section.
- **Rust users forum / Discord `#showcase`**: short version of the r/rust post.
- **awesome lists**: `awesome-rust` (Applications → Terminal), `awesome-terminals`. Only once the project has a few releases.
