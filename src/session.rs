//! One shell session: ConPTY I/O thread, the mirror terminal and the block model.
//!
//! All PTY output is parsed into `Shared::term`, an exact mirror of the ConPTY
//! screen. Our PowerShell prompt clears that screen before every command, so while
//! a command runs the mirror holds only its echo + output. The `Exec` mark records
//! where output starts; the `Done` mark (processed on the I/O thread, between two
//! byte slices) freezes those rows into the block. Finished blocks keep compact
//! styled text, not terminal cells.

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroUsize;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event as TermEvent, EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite};
use alacritty_terminal::vte::ansi::{self, Color};
use polling::{Event as PollEvent, Events, PollMode, Poller};

use crate::shell::{self, Mark, Scanner, Seg};

/// `Mutex::lock` that survives a panic on the other side. A poisoned lock would
/// otherwise turn one crashed pane thread into a crash of the whole window (every tab
/// lost); the terminal state is still usable for drawing and the pane gets closed.
pub trait LockExt<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Marks the pane as exited when the PTY thread ends for any reason, panics included.
struct ExitGuard {
    shared: Arc<Mutex<Shared>>,
    waker: Waker,
}

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.shared.locked().exited = true;
        self.waker.wake();
    }
}

/// Asks the UI for a frame. Panes that aren't on screen (other tab, hidden
/// split) are throttled to 4 frames/s: enough for the activity dot, and a
/// background dev server streaming logs costs almost nothing.
#[derive(Clone)]
pub struct Waker {
    pub ctx: egui::Context,
    pub visible: Arc<std::sync::atomic::AtomicBool>,
}

impl Waker {
    pub fn wake(&self) {
        if self.visible.load(std::sync::atomic::Ordering::Relaxed) {
            self.ctx.request_repaint();
        } else {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

#[derive(Clone)]
pub struct Listener {
    pub tx: mpsc::Sender<TermEvent>,
    pub waker: Waker,
}

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        let _ = self.tx.send(event);
        self.waker.wake();
    }
}

/// A styled stretch of one row. Non-ASCII chars get their own run so they land in
/// their column.
#[derive(Clone, Debug)]
pub struct Run {
    pub col: u16,
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

pub type Row = Vec<Run>;

const STYLE_FLAGS: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::WIDE_CHAR);

pub fn row_runs<T>(term: &Term<T>, line: Line) -> Row {
    let grid = term.grid();
    let row = &grid[line];
    let mut runs: Row = vec![];
    for c in 0..grid.columns() {
        let cell = &row[Column(c)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let ch = if cell.c == '\0' { ' ' } else { cell.c };
        let flags = cell.flags & STYLE_FLAGS;
        if let Some(last) = runs.last_mut() {
            let contiguous = last.col as usize + last.text.chars().count() == c;
            if ch.is_ascii()
                && last.text.is_ascii()
                && contiguous
                && last.fg == cell.fg
                && last.bg == cell.bg
                && last.flags == flags
            {
                last.text.push(ch);
                continue;
            }
        }
        runs.push(Run {
            col: c as u16,
            text: ch.to_string(),
            fg: cell.fg,
            bg: cell.bg,
            flags,
        });
    }
    // Drop trailing blank space with the default background.
    while let Some(last) = runs.last_mut() {
        let default_bg = last.bg == Color::Named(ansi::NamedColor::Background)
            && !last.flags.contains(Flags::INVERSE);
        let trimmed = last.text.trim_end().len();
        if !default_bg || trimmed == last.text.len() {
            break;
        }
        if trimmed == 0 {
            runs.pop();
        } else {
            last.text.truncate(trimmed);
            break;
        }
    }
    runs
}

/// Cheap emptiness test (no allocation): any visible glyph or coloured background.
pub fn row_has_content<T>(term: &Term<T>, line: Line) -> bool {
    let default_bg = Color::Named(ansi::NamedColor::Background);
    term.grid()[line].into_iter().any(|c| {
        (c.c != ' ' && c.c != '\0') || c.bg != default_bg || c.flags.contains(Flags::INVERSE)
    })
}

pub fn row_text(row: &Row) -> String {
    let mut s = String::new();
    for r in row {
        while s.chars().count() < r.col as usize {
            s.push(' ');
        }
        s.push_str(&r.text);
    }
    s
}

pub struct Block {
    pub cmd: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub start: Instant,
    pub end: Option<Instant>,
    pub code: Option<i32>,
    pub interrupted: bool,
    /// Frozen output, filled when the command finishes.
    pub rows: Vec<Row>,
    /// Absolute line (from the top of the mirror's history) where output starts.
    out_start: Option<usize>,
}

impl Block {
    pub fn running(&self) -> bool {
        self.end.is_none()
    }

    pub fn output_text(&self) -> String {
        self.rows
            .iter()
            .map(row_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    /// Shell starting, integration not confirmed yet.
    Starting,
    /// Waiting for a command: the editor is active.
    Ready,
    Running,
    /// Command finished, prompt not printed yet.
    Waiting,
}

pub struct Shared {
    pub term: Term<Listener>,
    pub blocks: VecDeque<Block>,
    pub phase: Phase,
    pub cwd: Option<String>,
    pub max_blocks: usize,
    /// Most frozen output rows kept across all blocks.
    // ponytail: counts rows, not bytes (~100 B a row → ~20 MB); bytes if it matters.
    row_budget: usize,
    pub exited: bool,
    /// Exit code of the shell once it ended (None while running or unknown).
    pub exit_code: Option<i32>,
}

impl Shared {
    pub fn new(term: Term<Listener>, max_blocks: usize) -> Self {
        Self {
            term,
            blocks: VecDeque::new(),
            phase: Phase::Starting,
            cwd: None,
            max_blocks,
            row_budget: 200_000,
            exited: false,
            exit_code: None,
        }
    }

    fn on_mark(&mut self, mark: Mark) {
        match mark {
            Mark::Cwd(c) => self.cwd = Some(c),
            Mark::Ready => {
                if self.phase != Phase::Running {
                    self.phase = Phase::Ready;
                }
            }
            Mark::Exec => {
                let abs = self.cursor_abs();
                if let Some(b) = self
                    .blocks
                    .back_mut()
                    .filter(|b| b.running() && b.out_start.is_none())
                {
                    b.out_start = Some(abs);
                }
            }
            Mark::Done(code) => {
                if self.blocks.back().is_some_and(|b| b.running()) {
                    let (start, end) = self.output_lines(false);
                    let rows: Vec<Row> = (start..end)
                        .map(|l| row_runs(&self.term, Line(l)))
                        .collect();
                    let b = self.blocks.back_mut().unwrap();
                    b.rows = rows;
                    b.code = Some(code);
                    b.end = Some(Instant::now());
                    self.phase = Phase::Waiting;
                    // Oldest blocks go first, by count and by total frozen rows (1000 blocks
                    // of a 10k-line build log each would otherwise sit in RAM).
                    let mut total: usize = self.blocks.iter().map(|b| b.rows.len()).sum();
                    while self.blocks.len() > self.max_blocks
                        || (self.blocks.len() > 1 && total > self.row_budget)
                    {
                        total -= self.blocks.pop_front().map_or(0, |b| b.rows.len());
                    }
                }
            }
        }
    }

    fn cursor_abs(&self) -> usize {
        let g = self.term.grid();
        (g.history_size() as i32 + g.cursor.point.line.0).max(0) as usize
    }

    /// Mirror lines [start, end) holding the running block's output.
    /// `with_cursor` keeps the cursor row even when empty (live view).
    pub fn output_lines(&self, with_cursor: bool) -> (i32, i32) {
        let g = self.term.grid();
        let h = g.history_size() as i32;
        let screen = g.screen_lines() as i32;
        let Some(start) = self.blocks.back().and_then(|b| b.out_start) else {
            return (0, 0);
        };
        let start = (start as i32 - h).max(-h);
        let cursor = g.cursor.point.line.0;
        let mut last = start - 1;
        for l in start..screen {
            if row_has_content(&self.term, Line(l)) {
                last = l;
            }
        }
        if with_cursor {
            last = last.max(cursor);
        }
        (start, (last + 1).max(start))
    }

    pub fn running(&self) -> bool {
        self.phase == Phase::Running
    }
}

pub enum Msg {
    Input(Vec<u8>),
    Resize(WindowSize),
    Shutdown,
}

pub struct Handle {
    tx: mpsc::Sender<Msg>,
    poll: Arc<Poller>,
}

impl Handle {
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
        let _ = self.poll.notify();
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.send(Msg::Shutdown);
    }
}

/// Run the PTY on its own thread. Mirrors alacritty's event loop, but parses
/// between marks so block boundaries are exact.
pub fn spawn(mut pty: tty::Pty, shared: Arc<Mutex<Shared>>, waker: Waker) -> io::Result<Handle> {
    let poll = Arc::new(Poller::new()?);
    let (tx, rx) = mpsc::channel::<Msg>();
    let p = poll.clone();
    std::thread::Builder::new()
        .name("rayo-pty".into())
        .spawn(move || {
            let _guard = ExitGuard {
                shared: shared.clone(),
                waker: waker.clone(),
            };
            let mut interest = PollEvent::readable(0);
            // SAFETY: the pty outlives its registration; we deregister before returning.
            if unsafe { pty.register(&p, interest, PollMode::Level) }.is_err() {
                return;
            }
            let mut events = Events::with_capacity(NonZeroUsize::new(64).unwrap());
            let mut parser = ansi::Processor::<ansi::StdSyncHandler>::new();
            let mut scanner = Scanner::default();
            let mut pending: VecDeque<u8> = VecDeque::new();
            let mut buf = vec![0u8; 65536];

            'outer: loop {
                let timeout = parser
                    .sync_timeout()
                    .sync_timeout()
                    .map(|t| t.saturating_duration_since(Instant::now()));
                events.clear();
                match p.wait(&mut events, timeout) {
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                    Ok(_) => {}
                }
                if events.is_empty() && timeout.is_some() {
                    parser.stop_sync(&mut shared.locked().term);
                    waker.wake();
                }

                loop {
                    match rx.try_recv() {
                        Ok(Msg::Input(b)) => pending.extend(b),
                        Ok(Msg::Resize(ws)) => pty.on_resize(ws),
                        Ok(Msg::Shutdown) | Err(TryRecvError::Disconnected) => break 'outer,
                        Err(TryRecvError::Empty) => break,
                    }
                }

                for ev in events.iter() {
                    match ev.key {
                        tty::PTY_CHILD_EVENT_TOKEN => {
                            if let Some(tty::ChildEvent::Exited(status)) = pty.next_child_event() {
                                // Last words of the shell (an error, `exit` output) are still
                                // in the pipe: show them before the pane goes.
                                let _ = read_available(&mut pty, &mut buf, |b| {
                                    feed_bytes(&shared, &mut parser, &mut scanner, b)
                                });
                                shared.locked().exit_code = status.and_then(|s| s.code());
                                break 'outer;
                            }
                        }
                        tty::PTY_READ_WRITE_TOKEN if ev.readable => {
                            let ok = read_available(&mut pty, &mut buf, |b| {
                                feed_bytes(&shared, &mut parser, &mut scanner, b);
                                waker.wake();
                            });
                            if !ok {
                                break 'outer;
                            }
                        }
                        _ => {}
                    }
                }

                while !pending.is_empty() {
                    let (a, _) = pending.as_slices();
                    match pty.writer().write(a) {
                        Ok(n) => {
                            pending.drain(..n);
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => break 'outer,
                    }
                }
                let need_write = !pending.is_empty();
                if need_write != interest.writable {
                    interest.writable = need_write;
                    let _ = pty.reregister(&p, interest, PollMode::Level);
                }
            }
            let _ = pty.deregister(&p);
        })?;
    Ok(Handle { tx, poll })
}

/// Read until the pipe is empty, handing each chunk to `f`. False on a read error.
fn read_available(pty: &mut tty::Pty, buf: &mut [u8], mut f: impl FnMut(&[u8])) -> bool {
    loop {
        match pty.reader().read(buf) {
            Ok(0) => return true,
            Ok(n) => f(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                return true;
            }
            Err(_) => return false,
        }
    }
}

fn feed_bytes(
    shared: &Mutex<Shared>,
    parser: &mut ansi::Processor<ansi::StdSyncHandler>,
    scanner: &mut Scanner,
    bytes: &[u8],
) {
    let mut s = shared.locked();
    for seg in scanner.feed(bytes) {
        match seg {
            Seg::Bytes(b) => parser.advance(&mut s.term, &b),
            Seg::Mark(m) => s.on_mark(m),
        }
    }
}

/// Start a block for `cmd` and send it to PowerShell.
pub fn submit(shared: &Mutex<Shared>, io: &Handle, cmd: &str) {
    let mut s = shared.locked();
    let cwd = s.cwd.clone();
    let branch = cwd.as_deref().and_then(shell::git_branch);
    s.blocks.push_back(Block {
        cmd: cmd.to_string(),
        cwd,
        branch,
        start: Instant::now(),
        end: None,
        code: None,
        interrupted: false,
        rows: vec![],
        out_start: None,
    });
    s.phase = Phase::Running;
    drop(s);
    io.send(Msg::Input(shell::wrap_command(cmd).into_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::test::TermSize;

    fn shared() -> Shared {
        let (tx, _rx) = mpsc::channel();
        let waker = Waker {
            ctx: egui::Context::default(),
            visible: Default::default(),
        };
        let l = Listener { tx, waker };
        Shared::new(Term::new(Default::default(), &TermSize::new(40, 10), l), 2)
    }

    fn feed(s: &mut Shared, p: &mut ansi::Processor, sc: &mut Scanner, bytes: &[u8]) {
        for seg in sc.feed(bytes) {
            match seg {
                Seg::Bytes(b) => p.advance(&mut s.term, &b),
                Seg::Mark(m) => s.on_mark(m),
            }
        }
    }

    #[test]
    fn block_lifecycle_captures_only_output() {
        let mut s = shared();
        let (mut p, mut sc) = (ansi::Processor::new(), Scanner::default());
        feed(
            &mut s,
            &mut p,
            &mut sc,
            &shell::with_nonce(b"\x1b]133;D;0\x07\x1b[2J\x1b[H\x1b]9;9;\"C:\\w\"\x07\x1b]133;B\x07"),
        );
        assert_eq!(s.phase, Phase::Ready);
        assert!(s.blocks.is_empty(), "first prompt must not create a block");

        let push = |s: &mut Shared, cmd: &str| {
            s.blocks.push_back(Block {
                cmd: cmd.into(),
                cwd: None,
                branch: None,
                start: Instant::now(),
                end: None,
                code: None,
                interrupted: false,
                rows: vec![],
                out_start: None,
            });
            s.phase = Phase::Running;
        };
        push(&mut s, "ls");
        // Echo of the wrapped line, then real output in red, then the prompt.
        feed(&mut s, &mut p, &mut sc, &shell::with_nonce(b"$global:LASTEXITCODE=0;...\r\n\x1b]133;C\x07one\r\n\x1b[31mtwo\x1b[0m\r\n\x1b]133;D;3\x07\x1b[2J\x1b[H\x1b]133;B\x07"));
        let b = &s.blocks[0];
        assert_eq!(b.code, Some(3));
        assert_eq!(b.output_text(), "one\ntwo");
        assert_eq!(b.rows[1][0].fg, Color::Named(ansi::NamedColor::Red));
        assert_eq!(s.phase, Phase::Ready);

        // Block limit.
        for c in ["a", "b"] {
            push(&mut s, c);
            feed(
                &mut s,
                &mut p,
                &mut sc,
                &shell::with_nonce(b"x\r\n\x1b]133;C\x07\x1b]133;D;0\x07\x1b]133;B\x07"),
            );
        }
        assert_eq!(s.blocks.len(), 2);
        assert_eq!(s.blocks[0].cmd, "a");
        assert!(s.blocks[0].rows.is_empty());

        // Row budget: a big block pushes older ones out, but the newest always stays.
        s.row_budget = 3;
        push(&mut s, "big");
        feed(
            &mut s,
            &mut p,
            &mut sc,
            &shell::with_nonce(b"\x1b]133;C\x071\r\n2\r\n3\r\n4\r\n\x1b]133;D;0\x07\x1b]133;B\x07"),
        );
        assert_eq!(s.blocks.len(), 1);
        assert_eq!(s.blocks[0].output_text(), "1\n2\n3\n4");
    }

    /// End to end over a real ConPTY: PowerShell with the integration script reports
    /// ready, runs a submitted command and the block gets its output and exit code.
    #[test]
    fn powershell_block_over_conpty() {
        let wait = |shared: &Mutex<Shared>, what: &str, ok: &dyn Fn(&Shared) -> bool| {
            let t0 = Instant::now();
            while !ok(&shared.locked()) {
                assert!(t0.elapsed().as_secs() < 30, "timed out waiting for {what}");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        };
        let (tx, _rx) = mpsc::channel();
        let waker = Waker {
            ctx: egui::Context::default(),
            visible: Default::default(),
        };
        let listener = Listener {
            tx,
            waker: waker.clone(),
        };
        let term = Term::new(Default::default(), &TermSize::new(100, 30), listener);
        let mut args = vec!["-NoLogo".to_string(), "-NoProfile".to_string()];
        args.extend(shell::integration_args());
        let opts = tty::Options {
            shell: Some(tty::Shell::new("powershell.exe".into(), args)),
            working_directory: Some(std::env::temp_dir()),
            drain_on_exit: false,
            env: Default::default(),
            escape_args: true,
        };
        let ws = WindowSize {
            num_lines: 30,
            num_cols: 100,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = tty::new(&opts, ws, 0).unwrap();
        let shared = Arc::new(Mutex::new(Shared::new(term, 10)));
        let io = spawn(pty, shared.clone(), waker).unwrap();

        wait(&shared, "ready", &|s| s.phase == Phase::Ready);
        assert!(s_cwd(&shared).is_some(), "integration reports the cwd");
        submit(&shared, &io, "Write-Output 'hola rayo'; cmd /c exit 3");
        wait(&shared, "block end", &|s| s.blocks.back().is_some_and(|b| !b.running()));
        let s = shared.locked();
        let b = s.blocks.back().unwrap();
        assert_eq!(b.code, Some(3));
        assert!(b.output_text().contains("hola rayo"), "output: {:?}", b.output_text());
        assert!(!b.output_text().contains("LASTEXITCODE"), "wrapper echo leaked into the block");
    }

    /// A shell that exits right away: its last output is drained and its code kept,
    /// so the UI can show why it closed.
    #[test]
    fn shell_exit_keeps_last_output_and_code() {
        let (tx, _rx) = mpsc::channel();
        let waker = Waker {
            ctx: egui::Context::default(),
            visible: Default::default(),
        };
        let listener = Listener {
            tx,
            waker: waker.clone(),
        };
        let term = Term::new(Default::default(), &TermSize::new(80, 10), listener);
        let opts = tty::Options {
            shell: Some(tty::Shell::new(
                "cmd.exe".into(),
                vec!["/c".into(), "echo adios&& exit 7".into()],
            )),
            working_directory: None,
            drain_on_exit: false,
            env: Default::default(),
            escape_args: false,
        };
        let ws = WindowSize {
            num_lines: 10,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = tty::new(&opts, ws, 0).unwrap();
        let shared = Arc::new(Mutex::new(Shared::new(term, 10)));
        let _io = spawn(pty, shared.clone(), waker).unwrap();
        let t0 = Instant::now();
        while !shared.locked().exited {
            assert!(t0.elapsed().as_secs() < 15, "shell never reported exit");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = shared.locked();
        assert_eq!(s.exit_code, Some(7));
        let screen: String = (0..10).map(|l| row_text(&row_runs(&s.term, Line(l)))).collect();
        assert!(screen.contains("adios"), "screen: {screen:?}");
    }

    fn s_cwd(shared: &Mutex<Shared>) -> Option<String> {
        shared.locked().cwd.clone()
    }
}
