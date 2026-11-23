use crate::i18n;
use std::cell::Cell;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{self, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use egui::{Align2, Color32, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::config::{self, Config, CursorStyle};
use crate::session::{self, Handle, Listener, Msg, Phase, Row, Shared};
use crate::shell;
use crate::theme::Theme;

/// Metrics shared by every pane for one frame.
pub struct Metrics {
    pub font: FontId,
    pub bold: FontId,
    /// Bottom editor font and its row height.
    pub editor_font: FontId,
    pub editor_row: f32,
    pub cell: Vec2,
    /// Vertical offset that centres glyphs inside the (taller) cell.
    pub dy: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    /// Command blocks + Rayo's editor (PowerShell with integration).
    Blocks,
    /// Plain terminal grid: full-screen apps, or shells without integration.
    Terminal,
}

/// Line editor state for one pane.
#[derive(Default)]
pub struct Editor {
    pub text: String,
    /// Index into history while browsing with the arrows.
    pub hist: Option<usize>,
    pub draft: String,
    pub candidates: Vec<String>,
    /// From the previous frame, used to decide which keys the editor keeps.
    pub cursor: usize,
    pub has_selection: bool,
    pub set_cursor_end: bool,
    /// (cwd, git branch) cache for the context line.
    pub branch: Option<(String, Option<String>)>,
    /// Enter pressed before the shell was ready: run as soon as it is.
    pub queued: bool,
    /// Highlighted entry in the completion list.
    pub cand_sel: usize,
    /// Entries of the open folder / branch chip menu (read when it opens).
    pub nav: Vec<String>,
}

impl Editor {
    pub fn on_first_line(&self) -> bool {
        !self.text.chars().take(self.cursor).any(|c| c == '\n')
    }
    pub fn on_last_line(&self) -> bool {
        !self.text.chars().skip(self.cursor).any(|c| c == '\n')
    }
    pub fn at_end(&self) -> bool {
        self.cursor >= self.text.chars().count()
    }
}

pub enum BlockAction {
    Edit(String),
}

pub struct Pane {
    pub shared: Arc<Mutex<Shared>>,
    io: Handle,
    rx: Receiver<TermEvent>,
    pub title: String,
    pub exited: bool,
    pub activity: bool,
    /// PowerShell integration active (falls back to the plain terminal if the
    /// shell never reports ready).
    pub integrated: bool,
    spawned: Instant,
    size: (usize, usize),
    /// Size waiting to settle before the PTY is resized, and since when.
    pending_size: Option<((usize, usize), Instant)>,
    fitted: bool,
    cell: Vec2,
    origin: Pos2,
    pub editor: Editor,
    /// Drawn this frame (set by the UI); off-screen panes repaint less.
    pub visible: Arc<std::sync::atomic::AtomicBool>,
    /// Text selection in the block view: (block, row, col) anchor and head.
    sel: Cell<Option<(BlockPos, BlockPos)>>,
    selecting: Cell<bool>,
    shell_pid: Option<u32>,
    /// Every process this pane's shell starts (dropped with the pane: kills them).
    job: Option<crate::procs::Job>,
    /// Ctrl+C was sent; when the shell is back, clean up orphaned children.
    reap_after_interrupt: Cell<bool>,
}

type BlockPos = (usize, usize, usize);

impl Pane {
    pub fn spawn(cfg: &Config, ctx: &egui::Context, cwd: Option<String>) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let waker = session::Waker {
            ctx: ctx.clone(),
            visible: visible.clone(),
        };
        let listener = Listener {
            tx,
            waker: waker.clone(),
        };
        let size = (100, 30);
        let term_cfg = term::Config {
            scrolling_history: cfg.scrollback,
            ..Default::default()
        };
        let term = Term::new(term_cfg, &TermSize::new(size.0, size.1), listener);
        let integrated = cfg.command_blocks && shell::is_powershell(&cfg.shell_program);
        let mut args: Vec<String> = cfg
            .shell_args
            .split_whitespace()
            .map(String::from)
            .collect();
        if integrated {
            args.extend(shell::integration_args());
        }
        let wd = cwd.as_deref().unwrap_or(cfg.working_directory.trim());
        let opts = tty::Options {
            shell: Some(tty::Shell::new(cfg.shell_program.clone(), args)),
            working_directory: if wd.is_empty() {
                dirs::home_dir()
            } else {
                Some(wd.into())
            },
            drain_on_exit: false,
            env: Default::default(),
            escape_args: true,
        };
        let ws = WindowSize {
            num_lines: size.1 as u16,
            num_cols: size.0 as u16,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = tty::new(&opts, ws, 0)?;
        let mut sh = Shared::new(term, cfg.max_blocks);
        if !integrated {
            sh.phase = Phase::Ready;
        }
        let shared = Arc::new(Mutex::new(sh));
        let shell_pid = pty.child_watcher().pid().map(|p| p.get());
        let io = session::spawn(pty, shared.clone(), waker)?;
        Ok(Self {
            shared,
            io,
            rx,
            title: shell_name(&cfg.shell_program),
            exited: false,
            activity: false,
            integrated,
            spawned: Instant::now(),
            size,
            pending_size: None,
            fitted: false,
            cell: Vec2::new(8.0, 16.0),
            origin: Pos2::ZERO,
            editor: Editor::default(),
            visible,
            sel: Cell::new(None),
            selecting: Cell::new(false),
            shell_pid,
            job: shell_pid.and_then(crate::procs::Job::for_process),
            reap_after_interrupt: Cell::new(false),
        })
    }

    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        self.io.send(Msg::Input(bytes.into()));
    }

    /// Handle events coming from the terminal.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                TermEvent::Title(t) => {
                    self.title = if t.to_lowercase().ends_with(".exe") {
                        shell_name(&t)
                    } else {
                        t
                    }
                }
                TermEvent::ResetTitle => self.title.clear(),
                TermEvent::PtyWrite(s) => self.write(s),
                TermEvent::ClipboardStore(_, s) => ctx.copy_text(s),
                TermEvent::ClipboardLoad(_, fmt) => {
                    if let Some(s) = clipboard_get() {
                        self.write(fmt(&s));
                    }
                }
                TermEvent::TextAreaSizeRequest(fmt) => self.write(fmt(self.window_size())),
                TermEvent::Wakeup => self.activity = true,
                _ => {}
            }
        }
        let s = self.shared.lock().unwrap();
        self.exited |= s.exited;
        // `pnpm dev` & co: the .cmd wrapper dies on Ctrl+C but node may survive,
        // still writing to this console. The shell is back, so end the leftovers.
        if self.reap_after_interrupt.get() && s.phase != Phase::Running {
            self.reap_after_interrupt.set(false);
            if let (Some(job), Some(pid)) = (&self.job, self.shell_pid) {
                job.kill_except(pid);
            }
        }
        // Integration never came up (blocked profile, odd host…): plain terminal.
        if self.integrated
            && s.phase == Phase::Starting
            && self.spawned.elapsed() > Duration::from_secs(8)
        {
            drop(s);
            self.integrated = false;
            self.shared.lock().unwrap().phase = Phase::Ready;
        }
    }

    pub fn phase(&self) -> Phase {
        self.shared.lock().unwrap().phase
    }

    pub fn view(&self) -> View {
        let s = self.shared.lock().unwrap();
        if !self.integrated || s.term.mode().contains(TermMode::ALT_SCREEN) {
            View::Terminal
        } else {
            View::Blocks
        }
    }

    /// The editor owns the keyboard only while the shell waits for a command.
    pub fn editor_active(&self) -> bool {
        self.view() == View::Blocks && self.phase() != Phase::Running
    }

    pub fn cwd(&self) -> Option<String> {
        self.shared.lock().unwrap().cwd.clone()
    }

    pub fn submit(&mut self, cmd: &str) {
        session::submit(&self.shared, &self.io, cmd);
    }

    pub fn interrupt(&self) {
        let mut s = self.shared.lock().unwrap();
        if let Some(b) = s.blocks.back_mut().filter(|b| b.running()) {
            b.interrupted = true;
        }
        drop(s);
        self.reap_after_interrupt.set(true);
        self.write(vec![0x03]);
    }

    pub fn clear_blocks(&self) {
        self.sel.set(None);
        self.shared.lock().unwrap().blocks.retain(|b| b.running());
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.size.1 as u16,
            num_cols: self.size.0 as u16,
            cell_width: self.cell.x as u16,
            cell_height: self.cell.y as u16,
        }
    }

    /// Size the PTY to the pane (full pane, so full-screen apps get every row).
    /// Returns true while a new size is waiting to settle: repaint again after `RESIZE_SETTLE`.
    pub fn fit(&mut self, rect: Rect, m: &Metrics) -> bool {
        self.cell = m.cell;
        let cols = ((rect.width() / m.cell.x) as usize).max(10);
        let rows = ((rect.height() / m.cell.y) as usize).max(3);
        let want = (cols, rows);
        if want == self.size {
            self.pending_size = None;
            return false;
        }
        // Dragging the window (or moving it to a monitor with another scale) fires dozens of
        // sizes; TUIs like Claude Code redraw on each one and leave garbage. Resize once it settles.
        if self.fitted && !settled(&mut self.pending_size, want, Instant::now()) {
            return true;
        }
        self.fitted = true;
        self.pending_size = None;
        self.size = want;
        self.shared
            .lock()
            .unwrap()
            .term
            .resize(TermSize::new(cols, rows));
        self.io.send(Msg::Resize(self.window_size()));
        false
    }

    pub fn mode(&self) -> TermMode {
        *self.shared.lock().unwrap().term.mode()
    }

    /// Mouse wheel for apps that enabled mouse reporting (opencode, htop, vim with
    /// `mouse=a`): send wheel button events at `pos`. False if the app did not ask.
    pub fn wheel(&self, pos: Pos2, lines: i32) -> bool {
        let mode = self.mode();
        if !mode.intersects(TermMode::MOUSE_MODE) {
            return false;
        }
        let rel = pos - self.origin;
        let col = ((rel.x / self.cell.x).max(0.0) as usize).min(self.size.0.saturating_sub(1)) + 1;
        let row = ((rel.y / self.cell.y).max(0.0) as usize).min(self.size.1.saturating_sub(1)) + 1;
        let seq = wheel_report(lines > 0, col, row, mode.contains(TermMode::SGR_MOUSE));
        self.write(seq.repeat(lines.unsigned_abs() as usize));
        true
    }

    pub fn scroll(&self, lines: i32) {
        self.shared
            .lock()
            .unwrap()
            .term
            .scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_page(&self, up: bool) {
        self.shared.lock().unwrap().term.scroll_display(if up {
            Scroll::PageUp
        } else {
            Scroll::PageDown
        });
    }

    /// Link under the pointer in the terminal view: OSC 8 hyperlink or plain URL.
    /// Returns the screen rect to underline and the target.
    pub fn link_at(&self, pos: Pos2) -> Option<(Rect, String)> {
        let (p, _) = self.point_at(pos);
        let s = self.shared.lock().unwrap();
        let (c0, c1, url) = match s.term.grid()[p].hyperlink() {
            Some(h) => (p.column.0, p.column.0 + 1, h.uri().to_string()),
            None => shell::url_at(
                &session::row_text(&session::row_runs(&s.term, p.line)),
                p.column.0,
            )?,
        };
        let y = self.origin.y + ((pos.y - self.origin.y) / self.cell.y).floor() * self.cell.y;
        let x = |c: usize| self.origin.x + c as f32 * self.cell.x;
        Some((
            Rect::from_min_max(Pos2::new(x(c0), y), Pos2::new(x(c1), y + self.cell.y)),
            url,
        ))
    }

    /// Selected text: block view selection first, then the terminal grid's.
    pub fn copy_selection(&self) -> Option<String> {
        let s = self.shared.lock().unwrap();
        if let Some((a, z)) = self.sel.get() {
            let (a, z) = (a.min(z), a.max(z));
            return Some(selection_text(&s, a, z)).filter(|t| !t.is_empty());
        }
        s.term.selection_to_string().filter(|t| !t.is_empty())
    }

    pub fn clear_selection(&self) {
        self.sel.set(None);
        self.shared.lock().unwrap().term.selection = None;
    }

    pub fn paste(&self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if self.mode().contains(TermMode::BRACKETED_PASTE) {
            self.write(format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")));
        } else {
            self.write(text);
        }
        self.shared
            .lock()
            .unwrap()
            .term
            .scroll_display(Scroll::Bottom);
    }

    fn point_at(&self, pos: Pos2) -> (Point, Side) {
        let rel = pos - self.origin;
        let col = (rel.x / self.cell.x).max(0.0);
        let row = (rel.y / self.cell.y).max(0.0) as usize;
        let side = if col.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        let col = (col as usize).min(self.size.0.saturating_sub(1));
        let row = row.min(self.size.1.saturating_sub(1));
        let offset = self.shared.lock().unwrap().term.grid().display_offset() as i32;
        (Point::new(Line(row as i32 - offset), Column(col)), side)
    }

    pub fn select_start(&self, pos: Pos2, kind: SelectionType) {
        let (p, side) = self.point_at(pos);
        self.shared.lock().unwrap().term.selection = Some(Selection::new(kind, p, side));
    }

    pub fn select_update(&self, pos: Pos2) {
        let (p, side) = self.point_at(pos);
        if let Some(sel) = self.shared.lock().unwrap().term.selection.as_mut() {
            sel.update(p, side);
        }
    }

    /// Plain terminal grid (full-screen apps, shells without integration).
    #[expect(clippy::too_many_arguments, reason = "draw params, a struct would only rename them")]
    pub fn draw_terminal(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        m: &Metrics,
        theme: &Theme,
        cfg: &Config,
        focused: bool,
        blink_on: bool,
    ) {
        self.origin = rect.min;
        let s = self.shared.lock().unwrap();
        let term = &s.term;
        let content = term.renderable_content();
        let colors = content.colors;
        let resolve = |c: Color| -> Color32 {
            let ov = match c {
                Color::Indexed(i) => colors[i as usize],
                Color::Named(n) => colors[n],
                Color::Spec(_) => None,
            };
            ov.map(|rgb| Color32::from_rgb(rgb.r, rgb.g, rgb.b))
                .unwrap_or_else(|| resolve(theme, c))
        };
        let offset = content.display_offset as i32;
        let sel = content.selection;
        let cursor = content.cursor;
        let rows = self.size.1;

        let mut run = String::new();
        let mut run_start = Pos2::ZERO;
        let mut run_color = Color32::TRANSPARENT;
        let flush = |run: &mut String, at: Pos2, color: Color32| {
            if !run.trim().is_empty() {
                painter.text(
                    at + Vec2::new(0.0, m.dy),
                    Align2::LEFT_TOP,
                    run.as_str(),
                    m.font.clone(),
                    color,
                );
            }
            run.clear();
        };
        let mut last_line = i32::MIN;
        for item in content.display_iter {
            let cell = item.cell;
            let line = item.point.line.0 + offset;
            let pos = Pos2::new(
                rect.min.x + item.point.column.0 as f32 * m.cell.x,
                rect.min.y + line as f32 * m.cell.y,
            );
            if line != last_line {
                flush(&mut run, run_start, run_color);
                last_line = line;
            }
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let (fg, bg) = cell_colors(theme, cell.fg, cell.bg, cell.flags, resolve);
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2.0
            } else {
                1.0
            };
            let cell_rect = Rect::from_min_size(pos, Vec2::new(m.cell.x * width, m.cell.y));
            if sel.is_some_and(|s| s.contains(item.point)) {
                painter.rect_filled(cell_rect, 0.0, theme.selection);
            } else if bg != theme.bg {
                painter.rect_filled(cell_rect, 0.0, bg);
            }
            decorations(painter, cell_rect, cell.flags, fg);
            let c = cell.c;
            if c.is_ascii() && width == 1.0 {
                if run.is_empty() || fg != run_color {
                    flush(&mut run, run_start, run_color);
                    run_start = pos;
                    run_color = fg;
                }
                run.push(if c == '\0' { ' ' } else { c });
            } else {
                flush(&mut run, run_start, run_color);
                painter.text(
                    pos + Vec2::new(0.0, m.dy),
                    Align2::LEFT_TOP,
                    c,
                    m.font.clone(),
                    fg,
                );
            }
        }
        flush(&mut run, run_start, run_color);

        let cline = cursor.point.line.0 + offset;
        if cursor.shape != CursorShape::Hidden && cline >= 0 && (cline as usize) < rows {
            let p = Pos2::new(
                rect.min.x + cursor.point.column.0 as f32 * m.cell.x,
                rect.min.y + cline as f32 * m.cell.y,
            );
            let ch = term.grid()[cursor.point].c;
            draw_cursor(
                painter,
                p,
                ch,
                cursor.shape,
                m,
                theme,
                cfg,
                focused,
                blink_on,
            );
        }

        if offset > 0 {
            let total = term.grid().total_lines().saturating_sub(rows).max(1) as f32;
            let h = (rect.height() * rows as f32 / (total + rows as f32)).max(20.0);
            let y = rect.max.y - h - (rect.height() - h) * (offset as f32 / total);
            let bar = Rect::from_min_size(Pos2::new(rect.max.x - 3.0, y), Vec2::new(2.0, h));
            painter.rect_filled(bar, 1.0, theme.muted().gamma_multiply(0.6));
        }
    }

    /// Command blocks, oldest first, in a scroll area that follows new output.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_blocks(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        pane_id: u64,
        m: &Metrics,
        t: &Theme,
        cfg: &Config,
        focused: bool,
        blink_on: bool,
    ) -> Option<BlockAction> {
        const PAD: f32 = 10.0;
        const HEADER: f32 = 18.0;
        let shared = self.shared.clone();
        let s = shared.lock().unwrap();
        let live = s.output_lines(true);
        let rows_of = |b: &session::Block| {
            if b.running() {
                (live.1 - live.0) as usize
            } else {
                b.rows.len()
            }
        };
        let cmd_lines = |b: &session::Block| b.cmd.lines().count().max(1);
        let heights: Vec<f32> = s
            .blocks
            .iter()
            .map(|b| {
                let out = rows_of(b);
                PAD + HEADER
                    + cmd_lines(b) as f32 * m.cell.y
                    + if out > 0 {
                        4.0 + out as f32 * m.cell.y
                    } else {
                        0.0
                    }
                    + PAD
            })
            .collect();
        let total: f32 = heights.iter().sum();
        let pad_x = cfg.padding;
        let meta = FontId::proportional(11.5);
        let mut action = None;

        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical()
            .id_salt(("blocks", pane_id))
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show_viewport(&mut child, |ui, vp| {
                ui.set_min_height(total.max(vp.height()));
                let origin = ui.max_rect().min;
                let width = ui.max_rect().width();
                let visible = vp.translate(origin.to_vec2());
                let painter = ui.painter().clone();
                let now = Instant::now();
                // Blocks sit on the editor and grow upwards, like a chat.
                let mut y = origin.y + (vp.height() - total).max(0.0);
                let pointer = ui.input(|i| i.pointer.hover_pos());
                let ctrl = ui.input(|i| i.modifiers.ctrl);
                let sel = self.sel.get().map(|(a, z)| (a.min(z), a.max(z)));
                let mut hit: Option<BlockPos> = None;
                let (mut drag_started, mut plain_click) = (false, false);
                for (bi, (b, h)) in s.blocks.iter().zip(&heights).enumerate() {
                    let r = Rect::from_min_size(Pos2::new(origin.x, y), Vec2::new(width, *h));
                    y += h;
                    if !r.intersects(visible) {
                        continue;
                    }
                    let failed = b.code.is_some_and(|c| c != 0) && !b.interrupted;
                    if failed {
                        painter.rect_filled(r, 0.0, t.error().gamma_multiply(0.045));
                    }
                    let bar = match (b.running(), failed) {
                        (true, _) => Some(t.accent),
                        (_, true) => Some(t.error()),
                        _ => None,
                    };
                    if let Some(c) = bar {
                        painter.rect_filled(
                            Rect::from_min_size(r.min, Vec2::new(2.0, r.height())),
                            0.0,
                            c.gamma_multiply(0.8),
                        );
                    }
                    painter.hline(r.x_range(), r.max.y - 0.5, Stroke::new(1.0, t.border()));

                    // Header: context on the left, status on the right.
                    let hy = r.min.y + PAD + HEADER / 2.0 - 2.0;
                    // Path in the accent colour, branch in green: easy to scan.
                    let path_rect = painter.text(
                        Pos2::new(r.min.x + pad_x, hy),
                        Align2::LEFT_CENTER,
                        b.cwd.as_deref().map(short_path).unwrap_or_default(),
                        meta.clone(),
                        config::pick(cfg.colors.path, t.path()),
                    );
                    if let Some(br) = &b.branch {
                        painter.text(
                            Pos2::new(path_rect.max.x + 10.0, hy),
                            Align2::LEFT_CENTER,
                            format!("git:({br})"),
                            meta.clone(),
                            config::pick(cfg.colors.branch, t.branch()),
                        );
                    }
                    // Status icons are drawn, not glyphs: no font fallback needed.
                    let (status, color, icon) = if b.running() {
                        (fmt_duration(now - b.start), t.accent, StatusIcon::Dot)
                    } else {
                        let d = fmt_duration(b.end.unwrap_or(now) - b.start);
                        match b.code {
                            _ if b.interrupted => (
                                format!("{}   {d}", i18n::pick("Interrumpido", "Interrupted")),
                                config::pick(cfg.colors.secondary, t.meta),
                                StatusIcon::None,
                            ),
                            Some(0) | None => (d, t.success(), StatusIcon::Check),
                            Some(c) => (format!("{c}   {d}"), t.error(), StatusIcon::Cross),
                        }
                    };
                    let text_rect = painter.text(
                        Pos2::new(r.max.x - pad_x, hy),
                        Align2::RIGHT_CENTER,
                        status,
                        meta.clone(),
                        color,
                    );
                    status_icon(&painter, icon, Pos2::new(text_rect.min.x - 10.0, hy), color);

                    // Command.
                    let mut ly = r.min.y + PAD + HEADER;
                    for line in b.cmd.lines() {
                        painter.text(
                            Pos2::new(r.min.x + pad_x, ly + m.dy),
                            Align2::LEFT_TOP,
                            line,
                            m.bold.clone(),
                            config::pick(cfg.colors.command, t.command),
                        );
                        ly += m.cell.y;
                    }
                    if b.cmd.is_empty() {
                        ly += m.cell.y;
                    }
                    ly += 4.0;

                    // Output: only rows inside the viewport are painted.
                    let out = rows_of(b);
                    let first =
                        (((visible.min.y - ly) / m.cell.y).floor().max(0.0) as usize).min(out);
                    let last =
                        (((visible.max.y - ly) / m.cell.y).ceil().max(0.0) as usize).min(out);
                    let x0 = r.min.x + pad_x;
                    for i in first..last {
                        let pos = Pos2::new(x0, ly + i as f32 * m.cell.y);
                        let row = block_row(&s, b, live.0, i);
                        if let Some((a, z)) =
                            sel.filter(|(a, z)| (a.0, a.1) <= (bi, i) && (bi, i) <= (z.0, z.1))
                        {
                            let c0 = if (bi, i) == (a.0, a.1) { a.2 } else { 0 };
                            let len = session::row_text(&row).chars().count().max(1);
                            let c1 = if (bi, i) == (z.0, z.1) { z.2 + 1 } else { len };
                            if c1 > c0 {
                                let sr = Rect::from_min_size(
                                    Pos2::new(x0 + c0 as f32 * m.cell.x, pos.y),
                                    Vec2::new((c1 - c0) as f32 * m.cell.x, m.cell.y),
                                );
                                painter.rect_filled(sr, 0.0, t.selection);
                            }
                        }
                        paint_row(&painter, &row, pos, m, t);
                    }

                    // Pointer position inside this block, for selection and links.
                    let mut link_clicked = false;
                    if let Some(pp) = pointer.filter(|pp| r.contains(*pp)) {
                        let col = ((pp.x - x0) / m.cell.x).floor().max(0.0) as usize;
                        let in_rows = out > 0 && pp.y >= ly && pp.y < ly + out as f32 * m.cell.y;
                        hit = Some(if out == 0 || pp.y < ly {
                            (bi, 0, 0)
                        } else {
                            (bi, (((pp.y - ly) / m.cell.y) as usize).min(out - 1), col)
                        });
                        if in_rows {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                        }
                        if in_rows && ctrl {
                            let i = ((pp.y - ly) / m.cell.y) as usize;
                            let text = session::row_text(&block_row(&s, b, live.0, i));
                            if let Some((c0, c1, url)) = shell::url_at(&text, col) {
                                let yb = ly + (i + 1) as f32 * m.cell.y - 2.0;
                                painter.hline(
                                    (x0 + c0 as f32 * m.cell.x)..=(x0 + c1 as f32 * m.cell.x),
                                    yb,
                                    Stroke::new(1.0, t.accent),
                                );
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                if ui.input(|i| i.pointer.primary_clicked()) {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                    link_clicked = true;
                                }
                            }
                        }
                    }
                    if b.running() && focused && s.term.mode().contains(TermMode::SHOW_CURSOR) {
                        let c = s.term.grid().cursor.point;
                        let row = c.line.0 - live.0;
                        if row >= 0 && (row as usize) < out {
                            let p = Pos2::new(
                                r.min.x + pad_x + c.column.0 as f32 * m.cell.x,
                                ly + row as f32 * m.cell.y,
                            );
                            let ch = s.term.grid()[c].c;
                            draw_cursor(
                                &painter,
                                p,
                                ch,
                                CursorShape::Block,
                                m,
                                t,
                                cfg,
                                true,
                                blink_on,
                            );
                        }
                    }

                    // Right click: copy / edit again.
                    let resp = ui.interact(
                        r,
                        Id::unique(("block", pane_id, b.start)),
                        Sense::click_and_drag(),
                    );
                    drag_started |= resp.drag_started();
                    plain_click |= resp.clicked() && !link_clicked;
                    resp.context_menu(|ui| {
                        if ui.button(i18n::t("Copiar comando")).clicked() {
                            ui.ctx().copy_text(b.cmd.clone());
                            ui.close();
                        }
                        if !b.running() && ui.button(i18n::t("Copiar salida")).clicked() {
                            ui.ctx().copy_text(b.output_text());
                            ui.close();
                        }
                        if !b.running() && ui.button(i18n::t("Copiar comando y salida")).clicked() {
                            ui.ctx()
                                .copy_text(format!("{}\n{}", b.cmd, b.output_text()));
                            ui.close();
                        }
                        if ui.button(i18n::t("Editar de nuevo")).clicked() {
                            action = Some(BlockAction::Edit(b.cmd.clone()));
                            ui.close();
                        }
                    });
                }

                // Drag to select across rows and blocks; a plain click clears it.
                if drag_started {
                    if let Some(h) = hit {
                        self.sel.set(Some((h, h)));
                        self.selecting.set(true);
                    }
                } else if plain_click {
                    self.sel.set(None);
                }
                if self.selecting.get() {
                    if let (Some(h), Some((a, _))) = (hit, self.sel.get()) {
                        self.sel.set(Some((a, h)));
                    }
                    if !ui.input(|i| i.pointer.primary_down()) {
                        self.selecting.set(false);
                        if cfg.copy_on_select
                            && let Some((a, z)) = self.sel.get() {
                                ui.ctx().copy_text(selection_text(&s, a.min(z), a.max(z)));
                            }
                    }
                }
            });
        action
    }

    /// Translate a key press into bytes for the PTY. Returns false if not handled.
    pub fn key(&self, key: Key, mods: Modifiers) -> bool {
        let app_cursor = self.mode().contains(TermMode::APP_CURSOR);
        let m = 1 + mods.shift as u8 + 2 * mods.alt as u8 + 4 * mods.ctrl as u8;
        let csi = |f: char| -> String {
            if m > 1 {
                format!("\x1b[1;{m}{f}")
            } else if app_cursor {
                format!("\x1bO{f}")
            } else {
                format!("\x1b[{f}")
            }
        };
        let ss3 = |f: char| -> String {
            if m > 1 {
                format!("\x1b[1;{m}{f}")
            } else {
                format!("\x1bO{f}")
            }
        };
        let tilde = |n: u8| -> String {
            if m > 1 {
                format!("\x1b[{n};{m}~")
            } else {
                format!("\x1b[{n}~")
            }
        };
        let s: String = match key {
            Key::Enter => enter_seq(mods).into(),
            Key::Backspace => match (mods.ctrl, mods.alt) {
                (true, _) => "\x08".into(),
                (_, true) => "\x1b\x7f".into(),
                _ => "\x7f".into(),
            },
            Key::Tab => {
                if mods.shift {
                    "\x1b[Z".into()
                } else {
                    "\t".into()
                }
            }
            Key::Escape => "\x1b".into(),
            Key::ArrowUp => csi('A'),
            Key::ArrowDown => csi('B'),
            Key::ArrowRight => csi('C'),
            Key::ArrowLeft => csi('D'),
            Key::Home => csi('H'),
            Key::End => csi('F'),
            Key::Insert => tilde(2),
            Key::Delete => tilde(3),
            Key::PageUp => tilde(5),
            Key::PageDown => tilde(6),
            Key::F1 => ss3('P'),
            Key::F2 => ss3('Q'),
            Key::F3 => ss3('R'),
            Key::F4 => ss3('S'),
            Key::F5 => tilde(15),
            Key::F6 => tilde(17),
            Key::F7 => tilde(18),
            Key::F8 => tilde(19),
            Key::F9 => tilde(20),
            Key::F10 => tilde(21),
            Key::F11 => tilde(23),
            Key::F12 => tilde(24),
            _ if mods.ctrl && !mods.shift => {
                let name = key.name();
                let ch = name.chars().next().unwrap_or(' ');
                let code = match key {
                    Key::Space | Key::Num2 => 0,
                    Key::OpenBracket => 0x1b,
                    Key::Backslash => 0x1c,
                    Key::CloseBracket => 0x1d,
                    Key::Num6 => 0x1e,
                    Key::Minus => 0x1f,
                    _ if name.len() == 1 && ch.is_ascii_alphabetic() => {
                        (ch.to_ascii_lowercase() as u8) & 0x1f
                    }
                    _ => return false,
                };
                let mut v = if mods.alt { vec![0x1b] } else { vec![] };
                v.push(code);
                self.write(v);
                return true;
            }
            // ponytail: no Alt+letter -> ESC prefix; Windows reports AltGr as Alt, so it would
            // break @ # [ ] on Spanish layouts. Add it behind a setting if readline Meta is needed.
            _ => return false,
        };
        self.write(s);
        self.shared
            .lock()
            .unwrap()
            .term
            .scroll_display(Scroll::Bottom);
        true
    }

    pub fn text(&self, s: &str) {
        self.write(s.as_bytes().to_vec());
        self.shared
            .lock()
            .unwrap()
            .term
            .scroll_display(Scroll::Bottom);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cursor(
    p: &egui::Painter,
    pos: Pos2,
    ch: char,
    shape: CursorShape,
    m: &Metrics,
    t: &Theme,
    cfg: &Config,
    focused: bool,
    blink_on: bool,
) {
    let r = Rect::from_min_size(pos, m.cell);
    if !focused {
        p.rect_stroke(r, 0.0, Stroke::new(1.0, t.cursor), egui::StrokeKind::Inside);
        return;
    }
    if !blink_on {
        return;
    }
    // Apps (vim, etc.) may request a shape; otherwise use the user's style.
    let style = match shape {
        CursorShape::Beam => CursorStyle::Bar,
        CursorShape::Underline => CursorStyle::Underline,
        _ => cfg.cursor_style,
    };
    match style {
        CursorStyle::Block => {
            p.rect_filled(r, 0.0, t.cursor);
            if ch != ' ' && ch != '\0' {
                p.text(
                    pos + Vec2::new(0.0, m.dy),
                    Align2::LEFT_TOP,
                    ch,
                    m.font.clone(),
                    t.bg,
                );
            }
        }
        CursorStyle::Bar => {
            p.rect_filled(
                Rect::from_min_size(pos, Vec2::new(2.0, m.cell.y)),
                0.0,
                t.cursor,
            );
        }
        CursorStyle::Underline => {
            p.rect_filled(
                Rect::from_min_size(Pos2::new(pos.x, r.max.y - 2.0), Vec2::new(m.cell.x, 2.0)),
                0.0,
                t.cursor,
            );
        }
    }
}

fn cell_colors(
    t: &Theme,
    fg: Color,
    bg: Color,
    flags: Flags,
    resolve: impl Fn(Color) -> Color32,
) -> (Color32, Color32) {
    let mut f = resolve(fg);
    let mut b = resolve(bg);
    if flags.contains(Flags::BOLD)
        && let Color::Named(n) = fg
            && (n as usize) < 8 {
                f = t.ansi[n as usize + 8];
            }
    if flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut f, &mut b);
    }
    if flags.contains(Flags::DIM) {
        f = f.gamma_multiply(0.66);
    }
    if flags.contains(Flags::HIDDEN) {
        f = b;
    }
    (f, b)
}

fn decorations(p: &egui::Painter, r: Rect, flags: Flags, fg: Color32) {
    if flags.intersects(Flags::ALL_UNDERLINES) {
        p.hline(r.x_range(), r.max.y - 1.5, Stroke::new(1.0, fg));
    }
    if flags.contains(Flags::STRIKEOUT) {
        p.hline(r.x_range(), r.center().y, Stroke::new(1.0, fg));
    }
}

#[derive(Clone, Copy)]
enum StatusIcon {
    None,
    Dot,
    Check,
    Cross,
}

fn status_icon(p: &egui::Painter, icon: StatusIcon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4, color);
    match icon {
        StatusIcon::None => {}
        StatusIcon::Dot => {
            p.circle_filled(c, 3.0, color);
        }
        StatusIcon::Check => {
            p.line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.8)], s);
            p.line_segment([c + Vec2::new(-1.0, 2.8), c + Vec2::new(3.8, -3.0)], s);
        }
        StatusIcon::Cross => {
            p.line_segment([c + Vec2::new(-3.0, -3.0), c + Vec2::new(3.0, 3.0)], s);
            p.line_segment([c + Vec2::new(-3.0, 3.0), c + Vec2::new(3.0, -3.0)], s);
        }
    }
}

/// Row `i` of a block's output (live rows come straight from the terminal).
fn block_row(s: &Shared, b: &session::Block, live_start: i32, i: usize) -> Row {
    if b.running() {
        session::row_runs(&s.term, Line(live_start + i as i32))
    } else {
        b.rows.get(i).cloned().unwrap_or_default()
    }
}

/// Plain text between two block positions (inclusive), one line per row.
fn selection_text(s: &Shared, a: BlockPos, z: BlockPos) -> String {
    let live = s.output_lines(true);
    let mut lines = vec![];
    for bi in a.0..=z.0 {
        let Some(b) = s.blocks.get(bi) else { break };
        let n = if b.running() {
            (live.1 - live.0) as usize
        } else {
            b.rows.len()
        };
        if n == 0 {
            continue;
        }
        let r0 = if bi == a.0 { a.1 } else { 0 };
        let r1 = if bi == z.0 { z.1.min(n - 1) } else { n - 1 };
        for i in r0..=r1 {
            let chars: Vec<char> = session::row_text(&block_row(s, b, live.0, i))
                .chars()
                .collect();
            let c0 = if (bi, i) == (a.0, a.1) { a.2 } else { 0 }.min(chars.len());
            let c1 = if (bi, i) == (z.0, z.1) {
                z.2 + 1
            } else {
                chars.len()
            }
            .min(chars.len())
            .max(c0);
            lines.push(
                chars[c0..c1]
                    .iter()
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
            );
        }
    }
    lines.join("\n")
}

/// Paint one frozen/live row of styled runs.
fn paint_row(p: &egui::Painter, row: &Row, pos: Pos2, m: &Metrics, t: &Theme) {
    for run in row {
        let (fg, bg) = cell_colors(t, run.fg, run.bg, run.flags, |c| resolve(t, c));
        let x = pos.x + run.col as f32 * m.cell.x;
        let cells = if run.flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            run.text.chars().count()
        };
        let r = Rect::from_min_size(
            Pos2::new(x, pos.y),
            Vec2::new(cells as f32 * m.cell.x, m.cell.y),
        );
        if bg != t.bg {
            p.rect_filled(r, 0.0, bg);
        }
        decorations(p, r, run.flags, fg);
        if !run.text.trim().is_empty() {
            let font = if run.flags.contains(Flags::BOLD) {
                &m.bold
            } else {
                &m.font
            };
            p.text(
                Pos2::new(x, pos.y + m.dy),
                Align2::LEFT_TOP,
                &run.text,
                font.clone(),
                fg,
            );
        }
    }
}

pub fn short_path(p: &str) -> String {
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    match p.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => p.to_string(),
    }
}

pub fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub fn quote_path(p: &str) -> String {
    if p.contains(' ') { format!("\"{p}\"") } else { p.to_string() }
}

/// Clipboard image → `%TEMP%\rayo\paste-<ms>.png`; returns the (quoted) path.
/// ponytail: temp files never cleaned up; OS temp cleanup handles it.
pub fn save_png(img: &egui::ColorImage) -> Option<String> {
    let dir = std::env::temp_dir().join("rayo");
    std::fs::create_dir_all(&dir).ok()?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = dir.join(format!("paste-{ms}.png"));
    let file = std::io::BufWriter::new(std::fs::File::create(&path).ok()?);
    let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    enc.write_header().ok()?.write_image_data(&rgba).ok()?;
    Some(quote_path(&path.to_string_lossy()))
}

fn shell_name(program: &str) -> String {
    std::path::Path::new(program)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

fn resolve(t: &Theme, c: Color) -> Color32 {
    match c {
        Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => indexed(t, i),
        Color::Named(n) => named(t, n),
    }
}

fn named(t: &Theme, n: NamedColor) -> Color32 {
    let i = n as usize;
    match n {
        _ if i < 16 => t.ansi[i],
        NamedColor::Foreground | NamedColor::BrightForeground => t.fg,
        NamedColor::DimForeground => t.fg.gamma_multiply(0.66),
        NamedColor::Background => t.bg,
        NamedColor::Cursor => t.cursor,
        _ => t.ansi[(i - NamedColor::DimBlack as usize) % 8].gamma_multiply(0.66),
    }
}

fn indexed(t: &Theme, i: u8) -> Color32 {
    match i {
        0..=15 => t.ansi[i as usize],
        16..=231 => {
            let i = i - 16;
            let v = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
            Color32::from_rgb(v(i / 36), v((i / 6) % 6), v(i % 6))
        }
        _ => Color32::from_gray(8 + (i - 232) * 10),
    }
}

pub fn fmt_duration(d: Duration) -> String {
    let ms = d.as_millis();
    match ms {
        0..1000 => format!("{ms} ms"),
        1000..60_000 => format!("{:.1} s", d.as_secs_f32()),
        _ => format!("{}m {:02}s", ms / 60_000, (ms / 1000) % 60),
    }
}

/// One wheel step as an xterm mouse report (buttons 64 up / 65 down), 1-based cell.
fn wheel_report(up: bool, col: usize, row: usize, sgr: bool) -> String {
    let b = if up { 64 } else { 65 };
    if sgr {
        format!("\x1b[<{b};{col};{row}M")
    } else {
        // Legacy X10 encoding: one byte each, offset by 32, capped at 223.
        let enc = |v: usize| char::from((v.min(223) + 32) as u8);
        format!("\x1b[M{}{}{}", enc(b), enc(col), enc(row))
    }
}

pub const RESIZE_SETTLE: Duration = Duration::from_millis(120);

/// Whether `want` has been the requested size for `RESIZE_SETTLE`; tracks it in `pending`.
fn settled(pending: &mut Option<((usize, usize), Instant)>, want: (usize, usize), now: Instant) -> bool {
    match *pending {
        Some((s, since)) if s == want => now - since >= RESIZE_SETTLE,
        _ => {
            *pending = Some((want, now));
            false
        }
    }
}

/// Shift+Enter sends ESC CR like Alt+Enter: what Claude Code (and most TUIs) read as
/// "new line" — the same thing its /terminal-setup makes other terminals send.
fn enter_seq(mods: Modifiers) -> &'static str {
    if mods.alt || mods.shift { "\x1b\r" } else { "\r" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_should_wait_until_size_is_stable() {
        let t0 = Instant::now();
        let mut p = None;
        assert!(!settled(&mut p, (80, 24), t0));
        assert!(!settled(&mut p, (81, 24), t0 + RESIZE_SETTLE)); // changed again: restart
        assert!(!settled(&mut p, (81, 24), t0 + RESIZE_SETTLE + Duration::from_millis(50)));
        assert!(settled(&mut p, (81, 24), t0 + RESIZE_SETTLE * 2));
    }

    #[test]
    fn enter_sequences() {
        assert_eq!(enter_seq(Modifiers::NONE), "\r");
        assert_eq!(enter_seq(Modifiers::SHIFT), "\x1b\r");
        assert_eq!(enter_seq(Modifiers::ALT), "\x1b\r");
    }

    #[test]
    fn wheel_reports() {
        assert_eq!(wheel_report(true, 3, 7, true), "\x1b[<64;3;7M");
        assert_eq!(wheel_report(false, 1, 1, true), "\x1b[<65;1;1M");
        assert_eq!(wheel_report(true, 1, 2, false), "\x1b[M`!\"");
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(Duration::from_millis(340)), "340 ms");
        assert_eq!(fmt_duration(Duration::from_millis(1250)), "1.2 s");
        assert_eq!(fmt_duration(Duration::from_secs(125)), "2m 05s");
    }

    #[test]
    fn color_cube() {
        let t = &crate::theme::THEMES[0];
        assert_eq!(indexed(t, 16), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed(t, 231), Color32::from_rgb(255, 255, 255));
        assert_eq!(indexed(t, 196), Color32::from_rgb(255, 0, 0));
        assert_eq!(indexed(t, 255), Color32::from_gray(238));
    }

    #[test]
    fn editor_line_position() {
        let mut e = Editor {
            text: "ab\ncd".into(),
            cursor: 1,
            ..Default::default()
        };
        assert!(e.on_first_line() && !e.on_last_line());
        e.cursor = 4;
        assert!(!e.on_first_line() && e.on_last_line() && !e.at_end());
    }
}
