#![windows_subsystem = "windows"]

mod agents;
mod chrome;
mod config;
mod i18n;
mod icon;
mod keys;
mod pane;
mod procs;
mod session;
mod shell;
mod theme;
mod update;

use std::collections::HashMap;
use std::time::Duration;

use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use egui::emath::GuiRounding as _;
use egui::{
    Align, Align2, Color32, CursorIcon, Event, FontFamily, FontId, Id, Key, Layout, Modifiers,
    Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2,
};

use chrome::{BAR_H, Icon};
use config::{Config, CursorStyle};
use keys::{Action, Combo};
use pane::{BlockAction, Editor, Metrics, Pane, View};
use session::{LockExt as _, Phase};
use theme::Theme;

/// Panics are invisible in a GUI-subsystem exe: append them to `%APPDATA%\rayo\crash.log`
/// so bug reports can include what happened.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let path = Config::path().with_file_name("crash.log");
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let thread = std::thread::current();
        let bt = std::backtrace::Backtrace::force_capture();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(
                f,
                "--- unix {secs} · Rayo v{} · thread {}\n{info}\n{bt}",
                env!("CARGO_PKG_VERSION"),
                thread.name().unwrap_or("?"),
            );
        }
        default(info);
    }));
}

fn main() -> eframe::Result {
    install_panic_hook();
    alacritty_terminal::tty::setup_env();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rayo")
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([480.0, 280.0])
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: icon::rgba(64),
                width: 64,
                height: 64,
            }))
            .with_decorations(false)
            .with_transparent(true),
        centered: true,
        // OpenGL: measured ~3-5x less memory than wgpu with the same frame times.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Rayo",
        opts,
        Box::new(|cc| {
            chrome::round_corners(cc);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

// ---------------------------------------------------------------- layout tree

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Right,
    Down,
}

enum Node {
    Leaf(u64),
    Split {
        dir: Dir,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

impl Node {
    fn split(&mut self, target: u64, new: u64, dir: Dir) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split {
                    dir,
                    ratio: 0.5,
                    a: Box::new(Node::Leaf(target)),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(target, new, dir) || b.split(target, new, dir),
        }
    }

    /// Remove a leaf, collapsing its parent. Returns None when the tree becomes empty.
    fn remove(self, target: u64) -> Option<Node> {
        match self {
            Node::Leaf(id) if id == target => None,
            Node::Leaf(_) => Some(self),
            Node::Split { dir, ratio, a, b } => match (a.remove(target), b.remove(target)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(n), None) | (None, Some(n)) => Some(n),
                (None, None) => None,
            },
        }
    }

    fn leaves(&self, out: &mut Vec<u64>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { a, b, .. } => {
                a.leaves(out);
                b.leaves(out);
            }
        }
    }

    /// Number of panes laid out along `dir` (nested splits in the other direction count as one).
    fn weight(&self, dir: Dir) -> f32 {
        match self {
            Node::Split { dir: d, a, b, .. } if *d == dir => a.weight(dir) + b.weight(dir),
            _ => 1.0,
        }
    }

    /// Give every pane in a row/column the same share.
    fn equalize(&mut self) {
        if let Node::Split { dir, ratio, a, b } = self {
            let (wa, wb) = (a.weight(*dir), b.weight(*dir));
            *ratio = wa / (wa + wb);
            a.equalize();
            b.equalize();
        }
    }

    /// Rects of both halves and the divider's grab area.
    fn halves(dir: Dir, ratio: f32, rect: Rect) -> (Rect, Rect, Rect) {
        match dir {
            Dir::Right => {
                let x = (rect.min.x + rect.width() * ratio).round() + 0.5;
                (
                    Rect::from_min_max(rect.min, Pos2::new(x - 0.5, rect.max.y)),
                    Rect::from_min_max(Pos2::new(x + 0.5, rect.min.y), rect.max),
                    Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::new(9.0, rect.height())),
                )
            }
            Dir::Down => {
                let y = (rect.min.y + rect.height() * ratio).round() + 0.5;
                (
                    Rect::from_min_max(rect.min, Pos2::new(rect.max.x, y - 0.5)),
                    Rect::from_min_max(Pos2::new(rect.min.x, y + 0.5), rect.max),
                    Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(rect.width(), 9.0)),
                )
            }
        }
    }

    /// Compute pane rects.
    fn layout(&self, rect: Rect, out: &mut Vec<(u64, Rect)>) {
        match self {
            Node::Leaf(pid) => out.push((*pid, rect)),
            Node::Split { dir, ratio, a, b } => {
                let (ra, rb, _) = Self::halves(*dir, *ratio, rect);
                a.layout(ra, out);
                b.layout(rb, out);
            }
        }
    }

    /// Draggable dividers: hairlines with a wider invisible grab area. Called after the
    /// panes so they sit on top (in egui the last widget registered wins the pointer).
    fn dividers(&mut self, ui: &mut Ui, rect: Rect, id: Id, t: &Theme) {
        let Node::Split { dir, ratio, a, b } = self else { return };
        let (ra, rb, grab) = Self::halves(*dir, *ratio, rect);
        a.dividers(ui, ra, id.with(0), t);
        b.dividers(ui, rb, id.with(1), t);
        let resp = ui.interact(grab, id, Sense::drag());
        let hot = ui
            .ctx()
            .animate_bool_with_time(id, resp.hovered() || resp.dragged(), 0.1);
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(if *dir == Dir::Right {
                CursorIcon::ResizeHorizontal
            } else {
                CursorIcon::ResizeVertical
            });
        }
        if resp.double_clicked() {
            *ratio = 0.5;
        }
        if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
            *ratio = match dir {
                Dir::Right => (p.x - rect.min.x) / rect.width(),
                Dir::Down => (p.y - rect.min.y) / rect.height(),
            }
            .clamp(0.08, 0.92);
        }
        let color = t.border().lerp_to_gamma(t.accent.gamma_multiply(0.7), hot);
        let w = 1.0 + hot;
        match dir {
            Dir::Right => ui.painter().vline(grab.center().x, rect.y_range(), Stroke::new(w, color)),
            Dir::Down => ui.painter().hline(rect.x_range(), grab.center().y, Stroke::new(w, color)),
        };
    }
}

struct Tab {
    root: Node,
    focus: u64,
    zoom: bool,
    name: Option<String>,
}

// ---------------------------------------------------------------- settings UI

#[derive(Clone, Copy, PartialEq)]
enum Cat {
    Appearance,
    Terminal,
    Shell,
    Keys,
}

const CATS: &[(Cat, &str)] = &[
    (Cat::Appearance, "Apariencia"),
    (Cat::Terminal, "Terminal"),
    (Cat::Shell, "Shell"),
    (Cat::Keys, "Atajos"),
];

/// Keys that are part of how the editor / tabs work and can't be rebound.
const FIXED_KEYS: &[(&str, &str)] = &[
    ("Ejecutar comando", "Enter"),
    ("Nueva línea en el editor", "Shift Enter"),
    ("Historial de comandos", "↑ ↓"),
    ("Autocompletar ruta", "Tab"),
    ("Aceptar sugerencia", "→"),
    ("Interrumpir proceso / borrar línea", "Ctrl C"),
    ("Copiar · Pegar", "Ctrl C · Ctrl V · clic derecho"),
    ("Ir a pestaña N", "Ctrl 1…9"),
    ("Renombrar pestaña", "Doble clic en la pestaña"),
    ("Cerrar pestaña", "Clic central en la pestaña"),
];

fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

/// Renders setting rows, hiding the ones that don't match the search query.
struct Rows<'t> {
    query: String,
    heading: &'static str,
    heading_shown: bool,
    t: &'t Theme,
}

impl Rows<'_> {
    fn row(&mut self, ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
        if !self.query.is_empty()
            && !fold(&format!("{label} {help} {}", self.heading)).contains(&self.query)
        {
            return;
        }
        if !self.query.is_empty() && !self.heading_shown {
            self.heading_shown = true;
            ui.add_space(4.0);
            ui.label(
                RichText::new(self.heading.to_uppercase())
                    .size(10.5)
                    .color(self.t.faint()),
            );
            ui.add_space(2.0);
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(13.0).color(self.t.fg));
                if !help.is_empty() {
                    ui.label(RichText::new(help).size(11.0).color(self.t.muted()));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), add);
        });
        ui.add_space(4.0);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(
            r.x_range(),
            r.min.y,
            Stroke::new(1.0, self.t.border().gamma_multiply(0.6)),
        );
        ui.add_space(8.0);
    }
}

// ---------------------------------------------------------------- app

struct App {
    cfg: Config,
    saved: Config,
    loaded_font: (String, f32),
    applied_theme: &'static str,
    saved_session: config::Session,
    last_session_save: f64,
    /// Settings changed at this time and not written yet (see `ui`).
    unsaved_since: Option<f64>,
    panes: HashMap<u64, Pane>,
    next_id: u64,
    tabs: Vec<Tab>,
    active: usize,
    rects: Vec<(u64, Rect)>,
    /// Captured key events; `true` = captured while the line editor was active.
    pending: Vec<(Event, bool)>,
    history: Vec<String>,
    settings_open: bool,
    settings_cat: Cat,
    settings_query: String,
    renaming: Option<(usize, String)>,
    /// Shortcut being rebound in settings (action id).
    capturing: Option<&'static str>,
    key_msg: Option<String>,
    scroll_acc: f32,
    error: Option<String>,
    title: String,
    /// Agents popover open, anchored at this point.
    agents_at: Option<Pos2>,
    usage: std::sync::Arc<std::sync::Mutex<Option<agents::Usage>>>,
    update: std::sync::Arc<std::sync::Mutex<update::State>>,
    /// Started on a folder from the command line: don't overwrite the saved session.
    ephemeral: bool,
    /// Close waiting for confirmation because a command is still running.
    confirm: Option<CloseReq>,
    /// Window close already confirmed: let it through.
    close_ok: bool,
}

/// What a close button / shortcut asked to close. Tabs are named by one of their
/// panes so the request survives tabs closing or moving before it is confirmed.
#[derive(Clone, Copy)]
enum CloseReq {
    Pane(u64),
    Tab(u64),
    Window,
}

/// Start folder from the command line: `rayo <dir>`, `rayo -d <dir>` or
/// `rayo --starting-directory <dir>` (Windows Terminal's spelling).
fn start_dir(mut args: impl Iterator<Item = String>) -> Option<String> {
    let a = args.next()?;
    match a.as_str() {
        "-d" | "--starting-directory" | "--startingDirectory" => args.next(),
        _ if a.starts_with('-') => None,
        // Explorer passes "C:\" for a drive root; a trailing quote can sneak in from "%V\".
        _ => Some(a.trim_end_matches('"').to_string()),
    }
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let (cfg, cfg_error) = Config::load();
        i18n::set(&cfg.language);
        // Scan PATH for command highlighting now, not on the first keystroke.
        std::thread::spawn(|| shell::is_command(""));
        let mut app = Self {
            saved: cfg.clone(),
            cfg,
            loaded_font: (String::new(), 0.0),
            applied_theme: "",
            saved_session: config::Session::default(),
            last_session_save: 0.0,
            unsaved_since: None,
            panes: HashMap::new(),
            next_id: 1,
            tabs: vec![],
            active: 0,
            rects: vec![],
            pending: vec![],
            history: config::load_history(),
            settings_open: false,
            settings_cat: Cat::Appearance,
            settings_query: String::new(),
            renaming: None,
            capturing: None,
            key_msg: None,
            scroll_acc: 0.0,
            error: cfg_error.map(|e| {
                format!(
                    "{}: {e}",
                    i18n::pick(
                        "config.toml tiene un error, se usan los valores por defecto. Copia guardada",
                        "config.toml has an error, using defaults. Copy saved"
                    )
                )
            }),
            title: String::new(),
            agents_at: None,
            usage: Default::default(),
            update: Default::default(),
            ephemeral: false,
            confirm: None,
            close_ok: false,
        };
        app.apply_style(ctx);
        update::cleanup();
        // ponytail: checked once per launch; add a periodic check if Rayo stays open for days.
        let (upd, c) = (app.update.clone(), ctx.clone());
        std::thread::spawn(move || {
            if let Some(r) = update::check() {
                *upd.locked() = update::State::Available(r);
                c.request_repaint();
            }
        });
        // `rayo <dir>` (Explorer's "Open in Rayo", PATH): one tab there, and this window
        // doesn't replace the saved session.
        let start = start_dir(std::env::args().skip(1)).filter(|d| std::path::Path::new(d).is_dir());
        app.ephemeral = start.is_some();
        let restored = start.is_none()
            && app.cfg.restore_session
            && config::Session::load().is_some_and(|s| app.restore(ctx, s));
        if !restored {
            app.new_tab_in(ctx, start);
        }
        app
    }

    /// Active theme (follows the day/night schedule when enabled).
    /// Current layout, for restoring next time.
    fn snapshot(&self) -> config::Session {
        fn node(n: &Node, panes: &HashMap<u64, Pane>) -> config::SavedNode {
            match n {
                Node::Leaf(id) => config::SavedNode {
                    cwd: panes.get(id).and_then(|p| p.cwd()),
                    split: None,
                },
                Node::Split { dir, ratio, a, b } => config::SavedNode {
                    cwd: None,
                    split: Some(Box::new(config::SavedSplit {
                        down: *dir == Dir::Down,
                        ratio: *ratio,
                        a: node(a, panes),
                        b: node(b, panes),
                    })),
                },
            }
        }
        let tabs = self
            .tabs
            .iter()
            .map(|t| {
                let mut v = vec![];
                t.root.leaves(&mut v);
                config::SavedTab {
                    name: t.name.clone(),
                    focus: v.iter().position(|id| *id == t.focus).unwrap_or(0),
                    root: node(&t.root, &self.panes),
                }
            })
            .collect();
        config::Session {
            active: self.active,
            tabs,
        }
    }

    /// Rebuild tabs and splits, starting each pane in its saved folder.
    fn restore(&mut self, ctx: &egui::Context, s: config::Session) -> bool {
        fn build(app: &mut App, ctx: &egui::Context, n: &config::SavedNode) -> Option<Node> {
            match &n.split {
                Some(sp) => match (build(app, ctx, &sp.a), build(app, ctx, &sp.b)) {
                    (Some(a), Some(b)) => Some(Node::Split {
                        dir: if sp.down { Dir::Down } else { Dir::Right },
                        ratio: sp.ratio.clamp(0.08, 0.92),
                        a: Box::new(a),
                        b: Box::new(b),
                    }),
                    (Some(x), None) | (None, Some(x)) => Some(x),
                    (None, None) => None,
                },
                // A folder that no longer exists falls back to the default one.
                None => {
                    let cwd = n.cwd.clone().filter(|c| std::path::Path::new(c).is_dir());
                    app.spawn(ctx, cwd).map(Node::Leaf)
                }
            }
        }
        for t in &s.tabs {
            if let Some(root) = build(self, ctx, &t.root) {
                let mut v = vec![];
                root.leaves(&mut v);
                let focus = v.get(t.focus).copied().unwrap_or(v[0]);
                self.tabs.push(Tab {
                    root,
                    focus,
                    zoom: false,
                    name: t.name.clone(),
                });
            }
        }
        self.active = s.active.min(self.tabs.len().saturating_sub(1));
        self.saved_session = s;
        !self.tabs.is_empty()
    }

    /// Persist the layout when it changed (checked every few seconds, and on exit).
    fn save_session(&mut self) {
        if self.ephemeral {
            return;
        }
        let snap = self.snapshot();
        // Closing the last tab quits: keep the previous layout for next time.
        if !snap.tabs.is_empty() && snap != self.saved_session {
            snap.save();
            self.saved_session = snap;
        }
    }

    fn theme(&self) -> &'static Theme {
        let base = theme::by_name(self.cfg.theme_at(config::local_hour()));
        match self.cfg.text_color {
            Some(rgb) => theme::with_fg(base, rgb),
            None => base,
        }
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        let font_key = (self.cfg.font.clone(), self.cfg.font_weight);
        if self.loaded_font != font_key {
            let mut defs = egui::FontDefinitions::default();
            let wght = |w: f32| egui::FontTweak {
                coords: egui::epaint::text::VariationCoords::new([(b"wght", w)]),
                ..Default::default()
            };
            if let Some(bytes) =
                config::font_path(&self.cfg.font).and_then(|p| std::fs::read(p).ok())
            {
                // Same file twice: regular and bold instances of the variable font.
                let bold = (self.cfg.font_weight + 250.0).min(800.0);
                defs.font_data.insert(
                    "term-bold".into(),
                    egui::FontData::from_owned(bytes.clone())
                        .tweak(wght(bold))
                        .into(),
                );
                defs.font_data.insert(
                    "term".into(),
                    egui::FontData::from_owned(bytes)
                        .tweak(wght(self.cfg.font_weight))
                        .into(),
                );
                defs.families
                    .entry(FontFamily::Monospace)
                    .or_default()
                    .insert(0, "term".into());
            }
            let mut bold_family = defs.families[&FontFamily::Monospace].clone();
            if defs.font_data.contains_key("term-bold") {
                bold_family[0] = "term-bold".into();
            }
            defs.families
                .insert(FontFamily::Name("bold".into()), bold_family);
            // Native Windows UI font for the chrome.
            for file in ["SegUIVar.ttf", "segoeui.ttf"] {
                if let Ok(bytes) = std::fs::read(config::fonts_dir().join(file)) {
                    defs.font_data.insert(
                        "ui".into(),
                        egui::FontData::from_owned(bytes).tweak(wght(400.0)).into(),
                    );
                    defs.families
                        .entry(FontFamily::Proportional)
                        .or_default()
                        .insert(0, "ui".into());
                    break;
                }
            }
            // Fallback for symbols the text fonts lack (✓ ⬆ ⏺ ⎿ ✻, monochrome emoji): without
            // it they render as boxes, in the chrome and in TUIs like Claude Code.
            // ponytail: Segoe UI Symbol only (2.4 MB); the colour emoji font is 12 MB of RAM.
            if let Ok(bytes) = std::fs::read(config::fonts_dir().join("seguisym.ttf")) {
                defs.font_data
                    .insert("symbols".into(), egui::FontData::from_owned(bytes).into());
                for fam in defs.families.values_mut() {
                    fam.push("symbols".into());
                }
            }
            ctx.set_fonts(defs);
            self.loaded_font = font_key;
        }

        let t = self.theme();
        self.applied_theme = t.name;
        let mut style = (*ctx.global_style()).clone();
        let mut v = if t.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        if t.light {
            // Plain linear coverage makes thin strokes look washed out on a light background.
            v.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Gamma(0.8);
        }
        v.panel_fill = t.bg;
        v.window_fill = t.bg2();
        v.extreme_bg_color = t.bg;
        v.faint_bg_color = t.bg2();
        v.code_bg_color = t.bg3();
        v.window_stroke = Stroke::new(1.0, t.border());
        v.window_corner_radius = 12.into();
        v.menu_corner_radius = 8.into();
        v.window_shadow = egui::Shadow {
            offset: [0, 10],
            blur: 32,
            spread: 0,
            color: Color32::from_black_alpha(150),
        };
        v.popup_shadow = egui::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(120),
        };
        v.selection.bg_fill = t.accent.gamma_multiply(0.35);
        v.text_cursor.blink = false;
        v.selection.stroke = Stroke::new(1.0, t.accent);
        v.hyperlink_color = t.accent;
        v.override_text_color = None;
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, t.border());
        w.noninteractive.fg_stroke = Stroke::new(1.0, t.fg);
        for (ws, fill, stroke) in [
            (&mut w.inactive, t.bg3(), t.border()),
            (
                &mut w.hovered,
                t.bg3().lerp_to_gamma(t.fg, 0.06),
                t.border().lerp_to_gamma(t.fg, 0.15),
            ),
            (
                &mut w.active,
                t.bg3().lerp_to_gamma(t.fg, 0.1),
                t.accent.gamma_multiply(0.6),
            ),
            (&mut w.open, t.bg3(), t.border()),
        ] {
            ws.bg_fill = fill;
            ws.weak_bg_fill = fill;
            ws.bg_stroke = Stroke::new(1.0, stroke);
            ws.fg_stroke = Stroke::new(1.0, t.fg);
            ws.corner_radius = 6.into();
            ws.expansion = 0.0;
        }
        // Keep a hairline so checkboxes / sliders stay visible on light themes.
        w.inactive.bg_stroke = Stroke::new(1.0, t.border());
        w.inactive.fg_stroke = Stroke::new(1.0, t.fg.lerp_to_gamma(t.bg, 0.15));
        style.visuals = v;
        style.spacing.button_padding = Vec2::new(10.0, 4.0);
        style.spacing.interact_size.y = 26.0;
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.slider_width = 140.0;
        use egui::TextStyle::*;
        for (ts, size) in [
            (Body, 13.0),
            (Button, 13.0),
            (Small, 11.0),
            (Heading, 15.0),
            (Monospace, 12.5),
        ] {
            if let Some(f) = style.text_styles.get_mut(&ts) {
                f.size = size;
            }
        }
        ctx.set_global_style(style);
    }

    fn spawn(&mut self, ctx: &egui::Context, cwd: Option<String>) -> Option<u64> {
        let pane = Pane::spawn(&self.cfg, ctx, cwd.clone()).or_else(|e| {
            self.error = Some(format!(
                "{} «{}»: {e}. {}",
                i18n::pick("No se pudo iniciar", "Couldn't start"),
                self.cfg.shell_program,
                i18n::pick("Se usa cmd.exe.", "Using cmd.exe instead."),
            ));
            let fallback = Config {
                shell_program: "cmd.exe".into(),
                shell_args: String::new(),
                ..self.cfg.clone()
            };
            Pane::spawn(&fallback, ctx, cwd)
        });
        match pane {
            Ok(p) => {
                let id = self.next_id;
                self.next_id += 1;
                self.panes.insert(id, p);
                Some(id)
            }
            Err(e) => {
                self.error = Some(format!("{}: {e}", i18n::pick("No se pudo crear la terminal", "Couldn't create the terminal")));
                None
            }
        }
    }

    /// Working directory of the focused pane (reported by shell integration).
    fn focused_cwd(&self) -> Option<String> {
        self.focused().and_then(|p| p.cwd())
    }

    fn new_tab(&mut self, ctx: &egui::Context) {
        let cwd = self.focused_cwd();
        self.new_tab_in(ctx, cwd);
    }

    fn new_tab_in(&mut self, ctx: &egui::Context, cwd: Option<String>) {
        if let Some(id) = self.spawn(ctx, cwd) {
            self.tabs.push(Tab {
                root: Node::Leaf(id),
                focus: id,
                zoom: false,
                name: None,
            });
            self.active = self.tabs.len() - 1;
        }
    }

    fn split(&mut self, ctx: &egui::Context, dir: Dir) {
        if self.tabs.is_empty() {
            return;
        }
        let cwd = self.focused_cwd();
        if let Some(id) = self.spawn(ctx, cwd) {
            let tab = &mut self.tabs[self.active];
            tab.root.split(tab.focus, id, dir);
            tab.root.equalize();
            tab.focus = id;
            tab.zoom = false;
        }
    }

    fn close_pane(&mut self, id: u64) {
        self.panes.remove(&id);
        let Some(ti) = self.tabs.iter().position(|t| {
            let mut v = vec![];
            t.root.leaves(&mut v);
            v.contains(&id)
        }) else {
            return;
        };
        let tab = &mut self.tabs[ti];
        let root = std::mem::replace(&mut tab.root, Node::Leaf(0));
        match root.remove(id) {
            Some(mut r) => {
                r.equalize();
                tab.root = r;
                if tab.focus == id {
                    let mut v = vec![];
                    tab.root.leaves(&mut v);
                    tab.focus = v[0];
                }
                tab.zoom = false;
            }
            None => self.close_tab(ti),
        }
    }

    /// Index of the tab holding pane `id`.
    fn tab_of(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| {
            let mut v = vec![];
            t.root.leaves(&mut v);
            v.contains(&id)
        })
    }

    /// Panes a close request would end.
    fn close_targets(&self, req: CloseReq) -> Vec<u64> {
        match req {
            CloseReq::Pane(id) => vec![id],
            CloseReq::Tab(id) => self
                .tab_of(id)
                .map(|ti| {
                    let mut v = vec![];
                    self.tabs[ti].root.leaves(&mut v);
                    v
                })
                .unwrap_or_default(),
            CloseReq::Window => self.panes.keys().copied().collect(),
        }
    }

    /// How many of `ids` are running a command.
    // ponytail: only panes with shell integration know a command is running; cmd/WSL/Git Bash
    // panes close without asking. Upgrade: count job processes besides the shell.
    fn busy(&self, ids: &[u64]) -> usize {
        ids.iter()
            .filter(|id| self.panes.get(id).is_some_and(|p| p.busy()))
            .count()
    }

    /// Close now, or ask first when it would kill a running command.
    fn request_close(&mut self, req: CloseReq) {
        if self.cfg.confirm_close && self.busy(&self.close_targets(req)) > 0 {
            self.confirm = Some(req);
        } else {
            self.do_close(req, None);
        }
    }

    fn do_close(&mut self, req: CloseReq, ctx: Option<&egui::Context>) {
        match req {
            CloseReq::Pane(id) => self.close_pane(id),
            CloseReq::Tab(id) => {
                if let Some(ti) = self.tab_of(id) {
                    self.close_tab(ti);
                }
            }
            CloseReq::Window => {
                self.close_ok = true;
                if let Some(ctx) = ctx {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(req) = self.confirm else { return };
        let n = self.busy(&self.close_targets(req));
        if n == 0 {
            // The command ended while the dialog was open: nothing to lose.
            self.confirm = None;
            self.do_close(req, Some(ctx));
            return;
        }
        let t = self.theme();
        let (mut yes, mut no) = (false, false);
        egui::Window::new("confirm-close")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Foreground)
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                ui.label(
                    RichText::new(i18n::t("¿Cerrar con comandos en ejecución?"))
                        .size(14.0)
                        .strong()
                        .color(t.fg),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{n} {}",
                        i18n::pick(
                            "comando(s) siguen en marcha y se terminarán.",
                            "command(s) are still running and will be ended."
                        )
                    ))
                    .size(12.0)
                    .color(t.muted()),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let close = egui::Button::new(RichText::new(i18n::t("Cerrar")).color(t.bg))
                        .fill(t.error());
                    yes = ui.add(close).clicked();
                    no = ui.button(i18n::t("Cancelar")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            no = true;
        }
        if yes {
            self.confirm = None;
            self.do_close(req, Some(ctx));
        } else if no {
            self.confirm = None;
        }
    }

    fn close_tab(&mut self, ti: usize) {
        let tab = self.tabs.remove(ti);
        let mut v = vec![];
        tab.root.leaves(&mut v);
        for id in v {
            self.panes.remove(&id);
        }
        if self.active >= ti && self.active > 0 {
            self.active -= 1;
        }
    }

    fn focused(&self) -> Option<&Pane> {
        self.tabs
            .get(self.active)
            .and_then(|t| self.panes.get(&t.focus))
    }

    fn move_focus(&mut self, dx: f32, dy: f32) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(cur) = self
            .rects
            .iter()
            .find(|(id, _)| *id == tab.focus)
            .map(|r| r.1)
        else {
            return;
        };
        let c = cur.center();
        let best = self
            .rects
            .iter()
            .filter(|(id, _)| *id != tab.focus)
            .filter_map(|(id, r)| {
                let d = r.center() - c;
                let along = d.x * dx + d.y * dy;
                (along > 0.0).then(|| (*id, along + (d.x * dy).abs() + (d.y * dx).abs()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = best {
            tab.focus = id;
        }
    }

    fn copy(&self, ctx: &egui::Context) -> bool {
        match self.focused().and_then(|p| p.copy_selection()) {
            Some(s) => {
                ctx.copy_text(s);
                self.focused().unwrap().clear_selection();
                true
            }
            None => false,
        }
    }

    fn paste(&self, text: Option<String>) {
        if let (Some(p), Some(t)) = (self.focused(), text.or_else(pane::clipboard_get)) {
            p.paste(&t);
        }
    }

    /// App shortcuts first; then the editor or the running process gets the key.
    fn handle_input(&mut self, ctx: &egui::Context) {
        for (ev, editor) in std::mem::take(&mut self.pending) {
            if editor {
                self.editor_event(ctx, ev);
                continue;
            }
            match ev {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers: m,
                    ..
                } => {
                    if !self.shortcut(ctx, key, m)
                        && let Some(p) = self.focused() {
                            p.key(key, m);
                        }
                }
                Event::Text(t) => {
                    if let Some(p) = self.focused() {
                        p.text(&t);
                    }
                }
                Event::Paste(t) => self.paste(Some(t)),
                Event::Copy => {
                    if !self.copy(ctx)
                        && let Some(p) = self.focused() {
                            p.interrupt();
                        }
                }
                Event::Cut => {
                    if let Some(p) = self.focused() {
                        p.write(vec![0x18]);
                    }
                }
                _ => {}
            }
        }
    }

    /// Keys the line editor handles itself (the rest of typing goes through egui).
    fn editor_event(&mut self, ctx: &egui::Context, ev: Event) {
        let Some(id) = self.tabs.get(self.active).map(|t| t.focus) else {
            return;
        };
        match ev {
            // Ctrl+C without selection: discard the line, like a shell.
            Event::Copy => {
                // Selected block text wins; otherwise discard the line like a shell.
                if self.copy(ctx) {
                    return;
                }
                if let Some(p) = self.panes.get_mut(&id) {
                    p.editor = Editor {
                        branch: p.editor.branch.take(),
                        ..Default::default()
                    };
                }
            }
            Event::Key {
                key, modifiers: m, ..
            } => {
                let plain = !m.ctrl && !m.alt && !m.shift;
                match keys::lookup(&self.cfg.keys, key, m) {
                    Some(Action::Paste) => {
                        if let (Some(p), Some(t)) = (self.panes.get_mut(&id), pane::clipboard_get())
                        {
                            // At the cursor, like Ctrl+V, not at the end.
                            let e = &mut p.editor;
                            let at = e.text.char_indices().nth(e.cursor).map_or(e.text.len(), |(b, _)| b);
                            e.text.insert_str(at, &t);
                            e.cursor_to = Some(e.cursor + t.chars().count());
                            e.set_cursor_end = true;
                        }
                        return;
                    }
                    Some(_) => {
                        self.shortcut(ctx, key, m);
                        return;
                    }
                    None => {}
                }
                // Completion list open: arrows move, Tab/Enter accept, Esc closes.
                if let Some(e) = self
                    .panes
                    .get_mut(&id)
                    .map(|p| &mut p.editor)
                    .filter(|e| !e.candidates.is_empty())
                {
                    let n = e.candidates.len();
                    match key {
                        Key::ArrowUp if plain => {
                            e.cand_sel = (e.cand_sel + n - 1) % n;
                            return;
                        }
                        Key::ArrowDown if plain => {
                            e.cand_sel = (e.cand_sel + 1) % n;
                            return;
                        }
                        Key::Enter | Key::Tab if !m.ctrl && !m.alt => {
                            e.text = shell::apply_completion(
                                &e.text,
                                &e.candidates[e.cand_sel.min(n - 1)],
                            );
                            e.candidates.clear();
                            e.set_cursor_end = true;
                            return;
                        }
                        Key::Escape => {
                            e.candidates.clear();
                            return;
                        }
                        _ => {}
                    }
                }
                match key {
                    Key::Enter if plain => self.submit(id),
                    Key::Tab if !m.ctrl => {
                        if let Some(p) = self.panes.get_mut(&id) {
                            let cwd = p.cwd().unwrap_or_default();
                            let (text, cands) = shell::complete_path(&p.editor.text, &cwd);
                            p.editor.text = text;
                            p.editor.candidates = cands;
                            p.editor.cand_sel = 0;
                            p.editor.set_cursor_end = true;
                        }
                    }
                    Key::ArrowUp | Key::ArrowDown if plain => {
                        let (Some(p), h) = (self.panes.get_mut(&id), &self.history) else {
                            return;
                        };
                        let e = &mut p.editor;
                        if h.is_empty() {
                            return;
                        }
                        let next = match (key, e.hist) {
                            (Key::ArrowUp, None) => {
                                e.draft = e.text.clone();
                                Some(h.len() - 1)
                            }
                            (Key::ArrowUp, Some(i)) => Some(i.saturating_sub(1)),
                            (Key::ArrowDown, Some(i)) if i + 1 < h.len() => Some(i + 1),
                            (Key::ArrowDown, Some(_)) => None,
                            _ => return,
                        };
                        e.hist = next;
                        e.text = next
                            .map(|i| h[i].clone())
                            .unwrap_or_else(|| e.draft.clone());
                        e.set_cursor_end = true;
                    }
                    Key::ArrowRight if plain => {
                        let (Some(p), h) = (self.panes.get_mut(&id), &self.history) else {
                            return;
                        };
                        if let Some(rest) = suggestion(h, &p.editor.text) {
                            p.editor.text.push_str(rest);
                            p.editor.set_cursor_end = true;
                        }
                    }
                    _ => {
                        self.shortcut(ctx, key, m);
                    }
                }
            }
            _ => {}
        }
    }

    fn submit(&mut self, id: u64) {
        let Some(p) = self.panes.get_mut(&id) else {
            return;
        };
        match p.phase() {
            Phase::Ready => {}
            Phase::Starting | Phase::Waiting => {
                p.editor.queued = true;
                return;
            }
            Phase::Running => return,
        }
        let cmd = p.editor.text.trim_end().to_string();
        if cmd.trim().is_empty() {
            return;
        }
        p.editor = Editor {
            branch: p.editor.branch.take(),
            ..Default::default()
        };
        // A leading space keeps a command out of the history (bash's `ignorespace`),
        // for one-off lines with tokens or passwords.
        if !cmd.starts_with(' ') && self.history.last() != Some(&cmd) {
            self.history.push(cmd.clone());
            config::append_history(&cmd);
        }
        if matches!(
            cmd.trim().to_lowercase().as_str(),
            "cls" | "clear" | "clear-host"
        ) {
            p.clear_blocks();
            return;
        }
        p.submit(&cmd);
    }

    /// Run the action bound to this key (see `keys.rs`), or Ctrl+1…9 tab jumps.
    fn shortcut(&mut self, ctx: &egui::Context, key: Key, m: Modifiers) -> bool {
        let Some(action) = keys::lookup(&self.cfg.keys, key, m) else {
            let n = match key {
                Key::Num1 => 1,
                Key::Num2 => 2,
                Key::Num3 => 3,
                Key::Num4 => 4,
                Key::Num5 => 5,
                Key::Num6 => 6,
                Key::Num7 => 7,
                Key::Num8 => 8,
                Key::Num9 => 9,
                _ => return false,
            };
            if !(m.ctrl && !m.shift && !m.alt) {
                return false;
            }
            if n <= self.tabs.len() {
                self.active = n - 1;
            }
            return true;
        };
        let n = self.tabs.len().max(1);
        match action {
            Action::Settings => self.settings_open = true,
            Action::NewTab => self.new_tab(ctx),
            Action::ClosePane => {
                if let Some(t) = self.tabs.get(self.active) {
                    self.request_close(CloseReq::Pane(t.focus));
                }
            }
            Action::SplitRight => self.split(ctx, Dir::Right),
            Action::SplitDown => self.split(ctx, Dir::Down),
            Action::Zoom => {
                if let Some(t) = self.tabs.get_mut(self.active) {
                    t.zoom = !t.zoom;
                }
            }
            Action::NextTab => self.active = (self.active + 1) % n,
            Action::PrevTab => self.active = (self.active + n - 1) % n,
            Action::FocusLeft => self.move_focus(-1.0, 0.0),
            Action::FocusRight => self.move_focus(1.0, 0.0),
            Action::FocusUp => self.move_focus(0.0, -1.0),
            Action::FocusDown => self.move_focus(0.0, 1.0),
            Action::Copy => {
                self.copy(ctx);
            }
            Action::Paste => self.paste(None),
            Action::FontBigger => self.cfg.font_size = (self.cfg.font_size + 1.0).min(40.0),
            Action::FontSmaller => self.cfg.font_size = (self.cfg.font_size - 1.0).max(6.0),
            Action::FontReset => self.cfg.font_size = Config::default().font_size,
            Action::ClearBlocks => {
                if let Some(p) = self.focused() {
                    p.clear_blocks();
                }
            }
            Action::PageUp | Action::PageDown => {
                if let Some(p) = self.focused() {
                    p.scroll_page(action == Action::PageUp);
                }
            }
        }
        true
    }

    fn tab_label(&self, tab: &Tab) -> String {
        if let Some(n) = &tab.name {
            return n.clone();
        }
        let Some(p) = self.panes.get(&tab.focus) else {
            return "terminal".into();
        };
        // With shell integration the folder name is the most useful label.
        let cwd = p.shared.locked().cwd.clone();
        cwd.and_then(|c| {
            let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
            if home.as_deref() == Some(c.trim_end_matches('\\')) {
                return Some("~".to_string());
            }
            std::path::Path::new(&c)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
        .unwrap_or_else(|| "terminal".into())
    }

    /// Single compact bar: tabs, new tab, drag space, actions, window controls.
    fn title_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme();
        let full = ui.max_rect();
        ui.painter().hline(
            full.x_range(),
            full.max.y - 0.5,
            Stroke::new(1.0, t.border().gamma_multiply(0.7)),
        );
        let upd_label = match &*self.update.locked() {
            update::State::Available(r) => Some(format!("⬆ {} v{}", i18n::pick("Actualizar a", "Update to"), r.version)),
            update::State::Installing(v) => Some(format!("{} v{v}…", i18n::pick("Descargando", "Downloading"))),
            _ => None,
        };
        let upd_w = if upd_label.is_some() { 160.0 } else { 0.0 };
        let right_w = 3.0 * 46.0 + 4.0 * 30.0 + 3.0 * 2.0 + 14.0 + upd_w;
        let left = Rect::from_min_max(
            full.min + Vec2::new(8.0, 0.0),
            Pos2::new(full.max.x - right_w, full.max.y),
        );
        let right = Rect::from_min_max(Pos2::new(left.max.x, full.min.y), full.max);

        let mut close = None;
        let mut new_tab = false;
        let mut drag_from = left.min.x;
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(left)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let n = self.tabs.len().max(1) as f32;
                let max_w = ((left.width() - 40.0) / n).clamp(72.0, 260.0);
                for i in 0..self.tabs.len() {
                    let label = self.tab_label(&self.tabs[i]);
                    if let Some((ri, buf)) = self.renaming.as_mut().filter(|(ri, _)| *ri == i) {
                        let r = ui.add(
                            egui::TextEdit::singleline(buf)
                                .desired_width(140.0)
                                .font(FontId::proportional(12.5))
                                .margin(Vec2::new(8.0, 4.0)),
                        );
                        r.request_focus();
                        if r.lost_focus() || ui.input(|inp| inp.key_pressed(Key::Enter)) {
                            let name = buf.trim().to_string();
                            self.tabs[*ri].name = (!name.is_empty()).then_some(name);
                            self.renaming = None;
                        }
                        continue;
                    }
                    let selected = i == self.active;
                    let font = FontId::proportional(12.5);
                    let w = max_w; // wide, Chrome-like tabs that shrink as more open
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::click_and_drag());
                    let hover = ui
                        .ctx()
                        .animate_bool_with_time(resp.id, resp.hovered(), 0.08);
                    let p = ui.painter();
                    let fill = if selected {
                        t.bg3()
                    } else {
                        t.bg2().gamma_multiply(hover)
                    };
                    p.rect_filled(rect, 7.0, fill);
                    if selected {
                        // Accent underline marks the active tab.
                        let u = Rect::from_min_max(
                            Pos2::new(rect.min.x + 10.0, rect.max.y - 2.0),
                            Pos2::new(rect.max.x - 10.0, rect.max.y),
                        );
                        p.rect_filled(u, 1.0, t.accent);
                    }

                    // Activity: pulsing accent dot while a command runs, faint dot for unseen output.
                    let mut leaves = vec![];
                    self.tabs[i].root.leaves(&mut leaves);
                    let running = leaves.iter().any(|id| {
                        self.panes
                            .get(id)
                            .is_some_and(|p| p.shared.locked().running())
                    });
                    let unseen = !selected
                        && leaves
                            .iter()
                            .any(|id| self.panes.get(id).is_some_and(|p| p.activity));
                    let dot = Pos2::new(rect.min.x + 12.0, rect.center().y);
                    if running {
                        // Static dot: an animation here would mean frames forever while
                        // a dev server runs.
                        p.circle_filled(dot, 3.0, t.accent);
                    } else if unseen {
                        p.circle_filled(dot, 2.5, t.muted());
                    }

                    let text_color = if selected {
                        t.fg
                    } else {
                        t.muted().lerp_to_gamma(t.fg, hover * 0.6)
                    };
                    let text_rect = Rect::from_min_max(
                        Pos2::new(rect.min.x + 22.0, rect.min.y),
                        Pos2::new(rect.max.x - 24.0, rect.max.y),
                    );
                    let shown = elide(ui, &label, &font, text_rect.width());
                    p.with_clip_rect(text_rect).text(
                        Pos2::new(text_rect.min.x, rect.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        font,
                        text_color,
                    );

                    // Close button, visible on hover / active tab.
                    let cr = Rect::from_center_size(
                        Pos2::new(rect.max.x - 13.0, rect.center().y),
                        Vec2::splat(18.0),
                    );
                    let cresp = ui.interact(cr, resp.id.with("x"), Sense::click());
                    let vis = if selected { 1.0 } else { hover };
                    if vis > 0.0 {
                        let ch = ui
                            .ctx()
                            .animate_bool_with_time(cresp.id, cresp.hovered(), 0.08);
                        if ch > 0.0 {
                            ui.painter()
                                .rect_filled(cr, 5.0, t.border().gamma_multiply(ch));
                        }
                        chrome::draw_icon(
                            ui.painter(),
                            Icon::SmallClose,
                            cr.center(),
                            t.muted().lerp_to_gamma(t.fg, ch).gamma_multiply(vis),
                        );
                    }
                    // Dragging a tab moves the window (the bar is mostly tabs).
                    if resp.drag_started() {
                        chrome::start_window_drag(ui.ctx());
                    }
                    if cresp.clicked() || resp.middle_clicked() {
                        close = Some(i);
                    } else if resp.double_clicked() {
                        self.renaming = Some((i, label.clone()));
                    } else if resp.clicked() {
                        self.active = i;
                    }
                    resp.on_hover_text(&label);
                }
                ui.add_space(2.0);
                if chrome::icon_button(
                    ui,
                    Icon::Plus,
                    i18n::t("Nueva pestaña  (Ctrl Shift T)"),
                    Vec2::new(28.0, 26.0),
                    t,
                )
                .clicked()
                {
                    new_tab = true;
                }
                drag_from = ui.cursor().min.x + 4.0;
            },
        );
        chrome::drag_area(
            ui,
            Rect::from_min_max(
                Pos2::new(drag_from, full.min.y),
                Pos2::new(left.max.x, full.max.y),
            ),
        );

        ui.scope_builder(
            UiBuilder::new()
                .max_rect(right)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                chrome::window_controls(ui, t);
                ui.add_space(10.0);
                let s = Vec2::new(30.0, 26.0);
                if chrome::icon_button(ui, Icon::Gear, i18n::t("Ajustes  (Ctrl ,)"), s, t).clicked() {
                    self.settings_open = !self.settings_open;
                }
                let b = chrome::icon_button(ui, Icon::Agents, i18n::t("Agentes de IA"), s, t);
                if !self.agents().is_empty() {
                    ui.painter()
                        .circle_filled(b.rect.right_top() + Vec2::new(-7.0, 7.0), 3.0, t.accent);
                }
                if b.clicked() {
                    self.agents_at = self.agents_at.is_none().then_some(b.rect.right_bottom());
                    if self.agents_at.is_some() {
                        self.refresh_usage(ctx);
                    }
                }
                if chrome::icon_button(
                    ui,
                    Icon::SplitDown,
                    i18n::t("Dividir hacia abajo  (Ctrl Shift E)"),
                    s,
                    t,
                )
                .clicked()
                {
                    self.split(ctx, Dir::Down);
                }
                if chrome::icon_button(
                    ui,
                    Icon::SplitRight,
                    i18n::t("Dividir a la derecha  (Ctrl Shift D)"),
                    s,
                    t,
                )
                .clicked()
                {
                    self.split(ctx, Dir::Right);
                }
                if let Some(label) = upd_label {
                    ui.add_space(6.0);
                    let b = ui
                        .add(
                            egui::Button::new(RichText::new(label).size(12.0).color(t.bg))
                                .fill(t.accent)
                                .corner_radius(6),
                        )
                        .on_hover_text(i18n::t("Instala la nueva versión y reinicia Rayo (las pestañas se restauran)"));
                    if b.clicked() {
                        self.start_update(ctx);
                    }
                }
            },
        );

        if let Some(i) = close {
            self.request_close(CloseReq::Tab(self.tabs[i].focus));
        }
        if new_tab {
            self.new_tab(ctx);
        }
    }

    fn terminals(&mut self, ui: &mut Ui) {
        let theme = self.theme();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let rect = ui.max_rect();
        ui.painter()
            .rect_filled(rect, 0.0, theme.bg.gamma_multiply(self.cfg.opacity));

        let mut rects = vec![];
        if tab.zoom {
            rects.push((tab.focus, rect));
        } else {
            tab.root.layout(rect, &mut rects);
        }
        let focus = tab.focus;
        let multi = rects.len() > 1;

        let font = FontId::monospace(self.cfg.font_size);
        let (gw, rh) = ui
            .ctx()
            .fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
        // Snap to physical pixels, not points: at 125 % / 150 % scaling (or after moving the
        // window to a monitor with another scale) point-rounded cells land between pixels,
        // so glyphs blur and rows/columns drift apart.
        let ppp = ui.ctx().pixels_per_point();
        let px = |v: f32| (v * ppp).round().max(1.0) / ppp;
        let cell_h = px(rh * self.cfg.line_height);
        let editor_font = FontId::monospace(self.cfg.editor_font_size);
        let editor_row = ui.ctx().fonts_mut(|f| f.row_height(&editor_font));
        let m = Metrics {
            editor_font,
            editor_row,
            bold: FontId::new(self.cfg.font_size, FontFamily::Name("bold".into())),
            font,
            cell: Vec2::new(px(gw), cell_h),
            dy: px((cell_h - rh) / 2.0).max(0.0),
        };

        let time = ui.input(|i| i.time);
        // Blink only in a focused window: an idle Rayo in the background repaints nothing.
        let window_focused = ui.input(|i| i.focused);
        let blink_on = !self.cfg.cursor_blink || !window_focused || (time * 1.6) as i64 % 2 == 0;
        if self.cfg.cursor_blink && window_focused {
            // Wake exactly at the next blink edge instead of polling.
            let next = ((time * 1.6).floor() + 1.0) / 1.6 - time;
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(next.max(0.05)));
        }

        let pad = Vec2::new(self.cfg.padding, (self.cfg.padding * 0.6).round());
        // Only panes drawn this frame repaint at full speed on output.
        for p in self.panes.values() {
            p.visible.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        for (id, _) in &rects {
            if let Some(p) = self.panes.get(id) {
                p.visible.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let mut new_focus = None;
        let mut close = None;
        for (id, r) in &rects {
            let Some(pane) = self.panes.get_mut(id) else {
                continue;
            };
            pane.activity = false;
            let is_focus = *id == focus;
            if ui.rect_contains_pointer(*r) && ui.input(|i| i.pointer.any_pressed()) {
                new_focus = Some(*id);
            }
            let inner = Rect::from_min_max(r.min + pad, r.max - pad).round_to_pixels(ppp);
            // The PTY spans the whole pane so full-screen apps get every row.
            if pane.fit(inner, &m) {
                ui.ctx().request_repaint_after(pane::RESIZE_SETTLE);
            }
            match pane.view() {
                View::Blocks => {
                    let lines = pane.editor.text.split('\n').count().clamp(1, 10) as f32;
                    // While a command runs the bar is optional: hidden, the process
                    // gets the whole pane.
                    let hide_bar = pane.phase() == Phase::Running && !self.cfg.show_running_bar;
                    let lines = lines.max(self.cfg.editor_lines as f32);
                    let ed_h = if hide_bar {
                        0.0
                    } else {
                        44.0 + lines * m.editor_row + 10.0
                    };
                    let blocks_rect = Rect::from_min_max(r.min, Pos2::new(r.max.x, r.max.y - ed_h));
                    let ed_rect = Rect::from_min_max(Pos2::new(r.min.x, r.max.y - ed_h), r.max);
                    if let Some(BlockAction::Edit(c)) = pane.draw_blocks(
                        ui,
                        blocks_rect,
                        *id,
                        &m,
                        theme,
                        &self.cfg,
                        is_focus,
                        blink_on,
                    ) {
                        pane.editor.text = c;
                        pane.editor.set_cursor_end = true;
                    }
                    let take_focus = is_focus && !self.settings_open && self.renaming.is_none();
                    if !hide_bar {
                        editor_ui(
                            ui,
                            ed_rect,
                            *id,
                            pane,
                            &self.history,
                            &self.cfg,
                            theme,
                            &m,
                            take_focus,
                        );
                    }
                }
                View::Terminal => {
                    let painter = ui.painter_at(*r);
                    pane.draw_terminal(&painter, inner, &m, theme, &self.cfg, is_focus, blink_on);
                    let resp = ui.interact(*r, Id::unique(("pane", id)), Sense::click_and_drag());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::Text);
                    }
                    // Ctrl+click opens links.
                    if ui.input(|i| i.modifiers.ctrl)
                        && let Some((lr, url)) = resp.hover_pos().and_then(|hp| pane.link_at(hp)) {
                            painter.hline(
                                lr.x_range(),
                                lr.max.y - 2.0,
                                Stroke::new(1.0, theme.accent),
                            );
                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            if resp.clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                        }
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if resp.drag_started() {
                            pane.select_start(pos, SelectionType::Simple);
                        } else if resp.dragged() {
                            pane.select_update(pos);
                        }
                        if resp.triple_clicked() {
                            pane.select_start(pos, SelectionType::Lines);
                            pane.select_update(pos);
                        } else if resp.double_clicked() {
                            pane.select_start(pos, SelectionType::Semantic);
                            pane.select_update(pos);
                        } else if resp.clicked() {
                            pane.clear_selection();
                        }
                    }
                    if resp.drag_stopped() && self.cfg.copy_on_select
                        && let Some(s) = pane.copy_selection() {
                            ui.ctx().copy_text(s);
                        }
                    if resp.secondary_clicked() {
                        if let Some(s) = pane.copy_selection() {
                            ui.ctx().copy_text(s);
                            pane.clear_selection();
                        } else if self.cfg.right_click_paste
                            && let Some(t) = pane::clipboard_get() {
                                pane.paste(&t);
                            }
                    }
                    if resp.hovered() {
                        let dy = ui.input(|i| i.smooth_scroll_delta.y);
                        if dy != 0.0 {
                            self.scroll_acc += dy / m.cell.y;
                            let lines = self.scroll_acc.trunc() as i32;
                            self.scroll_acc -= lines as f32;
                            let hover = ui.input(|i| i.pointer.hover_pos());
                            if lines != 0 && !hover.is_some_and(|p| pane.wheel(p, lines)) {
                                if pane.mode().contains(TermMode::ALT_SCREEN) {
                                    // Alternate scroll: full-screen apps (less, vim) get arrow keys.
                                    let k = if lines > 0 {
                                        Key::ArrowUp
                                    } else {
                                        Key::ArrowDown
                                    };
                                    for _ in 0..lines.abs() {
                                        pane.key(k, Modifiers::NONE);
                                    }
                                } else {
                                    pane.scroll(lines);
                                }
                            }
                        }
                    }
                }
            }
            let painter = ui.painter_at(*r);
            if multi {
                if is_focus {
                    painter.hline(
                        r.x_range(),
                        r.min.y + 0.5,
                        Stroke::new(1.0, theme.accent.gamma_multiply(0.55)),
                    );
                } else if self.cfg.dim_inactive_panes {
                    painter.rect_filled(*r, 0.0, theme.bg.gamma_multiply(0.42));
                }
            }

            // Close button on hover when the tab has several panes.
            let hovered_pane = ui.rect_contains_pointer(*r);
            if multi {
                let cr = Rect::from_center_size(
                    Pos2::new(r.max.x - 16.0, r.min.y + 14.0),
                    Vec2::splat(20.0),
                );
                let cresp = ui.interact(cr, Id::unique(("pane-x", id)), Sense::click());
                let vis = ui
                    .ctx()
                    .animate_bool_with_time(cresp.id.with("v"), hovered_pane, 0.12);
                if vis > 0.0 {
                    let h = ui
                        .ctx()
                        .animate_bool_with_time(cresp.id, cresp.hovered(), 0.08);
                    ui.painter().rect_filled(
                        cr,
                        6.0,
                        theme
                            .bg3()
                            .lerp_to_gamma(theme.border(), h)
                            .gamma_multiply(vis),
                    );
                    chrome::draw_icon(
                        ui.painter(),
                        Icon::SmallClose,
                        cr.center(),
                        theme.muted().lerp_to_gamma(theme.fg, h).gamma_multiply(vis),
                    );
                }
                if cresp
                    .on_hover_text(i18n::t("Cerrar panel  (Ctrl Shift W)"))
                    .clicked()
                {
                    close = Some(*id);
                }
            }
        }
        if !tab.zoom {
            tab.root.dividers(ui, rect, Id::unique(("split", self.active)), theme);
        }
        if let Some(f) = new_focus {
            tab.focus = f;
        }
        self.rects = rects;
        if let Some(id) = close {
            self.request_close(CloseReq::Pane(id));
        }
    }

    /// While a shortcut button waits for input, the next key press becomes its binding.
    fn capture_shortcut(&mut self, ctx: &egui::Context) {
        let Some(id) = self.capturing else { return };
        let mut pressed = None;
        ctx.input_mut(|i| {
            for e in &i.events {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                {
                    pressed = Some((*key, *modifiers));
                }
            }
            // Nothing else (search box, Esc-to-close) sees these keys.
            i.events
                .retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_)));
        });
        let Some((key, m)) = pressed else { return };
        self.capturing = None;
        let plain = !m.ctrl && !m.shift && !m.alt;
        if key == Key::Escape && plain {
            return;
        }
        if matches!(key, Key::Delete | Key::Backspace) && plain {
            self.cfg.keys.insert(id.to_string(), String::new());
            self.key_msg = None;
            return;
        }
        let combo = Combo::new(key, m);
        if combo.reserved() {
            self.key_msg = Some(
                i18n::t("Ctrl C, Ctrl V y Ctrl X están reservados para copiar, pegar y cortar.").into(),
            );
            return;
        }
        self.key_msg = None;
        // One combo, one action: free it from whoever had it.
        for (_, other, label, default) in keys::ACTIONS {
            let v = self
                .cfg
                .keys
                .get(*other)
                .cloned()
                .unwrap_or_else(|| default.to_string());
            if *other != id && Combo::parse(&v).is_some_and(|c| c.matches(key, m)) {
                self.cfg.keys.insert(other.to_string(), String::new());
                self.key_msg = Some(format!(
                    "{} {} «{}»{}",
                    combo.label(),
                    i18n::pick("se quitó de", "was removed from"),
                    i18n::t(label),
                    i18n::pick(", que queda sin asignar.", ", now unassigned.")
                ));
            }
        }
        self.cfg.keys.insert(id.to_string(), combo.to_config());
    }

    fn settings(&mut self, ctx: &egui::Context) {
        self.capture_shortcut(ctx);
        let t = self.theme();
        let size =
            (ctx.content_rect().size() - Vec2::new(80.0, 120.0)).min(Vec2::new(720.0, 500.0));
        let body_h = (size.y - 110.0).max(100.0);
        let mut close = false;
        egui::Window::new("ajustes")
            .title_bar(false)
            .collapsible(false)
            .fixed_size(size)
            .anchor(Align2::CENTER_CENTER, [0.0, 10.0])
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(i18n::t("Ajustes")).size(15.0).color(t.fg).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if chrome::icon_button(
                            ui,
                            Icon::SmallClose,
                            i18n::t("Cerrar  (Esc)"),
                            Vec2::new(24.0, 24.0),
                            t,
                        )
                        .clicked()
                        {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.settings_query)
                        .hint_text(RichText::new(i18n::t("Buscar ajustes…")).color(t.faint()))
                        .margin(Vec2::new(10.0, 7.0))
                        .desired_width(f32::INFINITY),
                );
                if ui.memory(|m| m.focused().is_none()) {
                    search.request_focus();
                }
                ui.add_space(10.0);
                let query = fold(self.settings_query.trim());
                ui.horizontal_top(|ui| {
                    if query.is_empty() {
                        ui.vertical(|ui| {
                            ui.set_width(140.0);
                            for (cat, name) in CATS {
                                let sel = self.settings_cat == *cat;
                                let (r, resp) =
                                    ui.allocate_exact_size(Vec2::new(140.0, 28.0), Sense::click());
                                let h =
                                    ui.ctx()
                                        .animate_bool_with_time(resp.id, resp.hovered(), 0.08);
                                if sel || h > 0.0 {
                                    ui.painter().rect_filled(
                                        r,
                                        6.0,
                                        if sel {
                                            t.bg3()
                                        } else {
                                            t.bg3().gamma_multiply(h * 0.6)
                                        },
                                    );
                                }
                                ui.painter().text(
                                    r.left_center() + Vec2::new(10.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    i18n::t(name),
                                    FontId::proportional(13.0),
                                    if sel { t.fg } else { t.muted() },
                                );
                                if resp.clicked() {
                                    self.settings_cat = *cat;
                                }
                            }
                        });
                        ui.add_space(8.0);
                    }
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink(false)
                            .max_height(body_h)
                            .show(ui, |ui| {
                                ui.add_space(2.0);
                                let mut any = false;
                                for (cat, name) in CATS {
                                    if query.is_empty() && *cat != self.settings_cat {
                                        continue;
                                    }
                                    let mut rows = Rows {
                                        query: query.clone(),
                                        heading: i18n::t(name),
                                        heading_shown: false,
                                        t,
                                    };
                                    self.settings_page(ui, *cat, &mut rows);
                                    any |= rows.heading_shown;
                                }
                                if !query.is_empty() && !any {
                                    ui.label(
                                        RichText::new(i18n::t("Ningún ajuste coincide")).color(t.muted()),
                                    );
                                }
                            });
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let path = Config::path();
                    ui.label(
                        RichText::new(i18n::t("Se guarda automáticamente"))
                            .size(11.0)
                            .color(t.faint()),
                    )
                    .on_hover_text(path.display().to_string());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(RichText::new(i18n::t("Restablecer")).size(12.0)).clicked() {
                            self.cfg = Config::default();
                        }
                        if ui
                            .button(RichText::new(i18n::t("Abrir config.toml")).size(12.0))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            self.cfg.save();
                            let _ = std::process::Command::new("notepad.exe").arg(&path).spawn();
                        }
                    });
                });
            });
        if close || (self.capturing.is_none() && ctx.input(|i| i.key_pressed(Key::Escape))) {
            self.settings_open = false;
            self.settings_query.clear();
        }
    }

    fn settings_page(&mut self, ui: &mut Ui, cat: Cat, rows: &mut Rows) {
        let current = rows.t;
        let cfg = &mut self.cfg;
        match cat {
            Cat::Appearance => {
                let theme_combo = |ui: &mut Ui, id: &str, value: &mut String| {
                    egui::ComboBox::from_id_salt(id)
                        .selected_text(value.as_str())
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for t in theme::THEMES {
                                ui.selectable_value(value, t.name.to_string(), t.name);
                            }
                        });
                };
                rows.row(ui, i18n::t("Idioma"), "Language", |ui| {
                    for (v, name) in [("auto", i18n::t("Automático")), ("es", "Español"), ("en", "English")] {
                        if ui.selectable_value(&mut cfg.language, v.to_string(), name).changed() {
                            i18n::set(v);
                        }
                    }
                });
                if !cfg.auto_theme {
                    rows.row(ui, i18n::t("Tema"), i18n::t("Colores del terminal y de la interfaz"), |ui| {
                        theme_combo(ui, "theme", &mut cfg.theme)
                    });
                }
                // Colours of each kind of text, over any theme.
                let base = theme::by_name(&cfg.theme);
                rows.row(
                    ui,
                    i18n::t("Texto de salida"),
                    i18n::t("Lo que escriben los programas"),
                    |ui| color_option(ui, &mut cfg.text_color, base.fg),
                );
                rows.row(ui, i18n::t("Comandos"), i18n::t("El comando de cada bloque"), |ui| {
                    color_option(ui, &mut cfg.colors.command, current.command)
                });
                rows.row(ui, i18n::t("Texto que escribes"), i18n::t("El editor de abajo"), |ui| {
                    color_option(ui, &mut cfg.colors.typed, current.command)
                });
                rows.row(ui, i18n::t("Ruta (carpeta)"), i18n::t("Chip de la carpeta actual"), |ui| {
                    color_option(ui, &mut cfg.colors.path, current.path())
                });
                rows.row(ui, i18n::t("Rama git"), i18n::t("Chip de la rama"), |ui| {
                    color_option(ui, &mut cfg.colors.branch, current.branch())
                });
                rows.row(
                    ui,
                    i18n::t("Texto secundario"),
                    i18n::t("«Ejecutando», «Interrumpido» y ayudas"),
                    |ui| color_option(ui, &mut cfg.colors.secondary, current.meta),
                );
                rows.row(
                    ui,
                    i18n::t("Letra del editor"),
                    i18n::t("Tamaño del texto que escribes abajo"),
                    |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.editor_font_size, 10.0..=28.0).step_by(0.5),
                        );
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Altura del editor"),
                    i18n::t("Líneas visibles como mínimo"),
                    |ui| {
                        ui.add(egui::Slider::new(&mut cfg.editor_lines, 1..=6));
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Tema según la hora"),
                    i18n::t("Usa un tema de día y otro de noche automáticamente"),
                    |ui| {
                        ui.checkbox(&mut cfg.auto_theme, "");
                    },
                );
                if cfg.auto_theme {
                    rows.row(ui, i18n::t("Tema de día"), "", |ui| {
                        theme_combo(ui, "day_theme", &mut cfg.day_theme)
                    });
                    rows.row(ui, i18n::t("Tema de noche"), "", |ui| {
                        theme_combo(ui, "night_theme", &mut cfg.night_theme)
                    });
                    rows.row(ui, i18n::t("Empieza el día"), i18n::t("Hora local"), |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.day_start, 0..=23)
                                .custom_formatter(|h, _| format!("{h:02}:00")),
                        );
                    });
                    rows.row(ui, i18n::t("Empieza la noche"), i18n::t("Hora local"), |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.night_start, 0..=23)
                                .custom_formatter(|h, _| format!("{h:02}:00")),
                        );
                    });
                }
                if rows.query.is_empty() {
                    ui.label(
                        RichText::new(i18n::t("Paleta ANSI del tema: los 16 colores que usan los programas para colorear su salida (errores, git, ls…). Pasa el ratón para ver cada uno."))
                            .size(11.0)
                            .color(current.muted()),
                    );
                    let (r, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
                    ui.painter().rect_filled(r, 6.0, current.bg);
                    ui.painter().rect_stroke(
                        r,
                        6.0,
                        Stroke::new(1.0, current.border()),
                        egui::StrokeKind::Inside,
                    );
                    let w = (r.width() - 12.0) / 16.0;
                    for (i, c) in current.ansi.iter().enumerate() {
                        let cr = Rect::from_min_size(
                            Pos2::new(r.min.x + 6.0 + i as f32 * w, r.min.y + 7.0),
                            Vec2::new(w - 3.0, 8.0),
                        );
                        ui.painter().rect_filled(cr, 2.0, *c);
                        const NAMES: [&str; 8] = [
                            "Negro", "Rojo", "Verde", "Amarillo", "Azul", "Magenta", "Cian",
                            "Blanco",
                        ];
                        let name = format!(
                            "{}{} · ANSI {i}",
                            i18n::t(NAMES[i % 8]),
                            if i >= 8 { i18n::t(" brillante") } else { "" }
                        );
                        ui.interact(cr.expand(2.0), Id::unique(("ansi", i)), Sense::hover())
                            .on_hover_text(name);
                    }
                    ui.add_space(14.0);
                }
                rows.row(ui, i18n::t("Fuente"), i18n::t("Fuentes monoespaciadas instaladas"), |ui| {
                    egui::ComboBox::from_id_salt("font")
                        .selected_text(i18n::t(&cfg.font))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for f in config::available_fonts() {
                                ui.selectable_value(&mut cfg.font, f.to_string(), i18n::t(f));
                            }
                        });
                });
                rows.row(
                    ui,
                    i18n::t("Tamaño de fuente"),
                    i18n::t("También con Ctrl = / Ctrl -"),
                    |ui| {
                        ui.add(egui::Slider::new(&mut cfg.font_size, 8.0..=32.0).step_by(0.5));
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Peso de fuente"),
                    i18n::t("Grosor del trazo (fuentes variables como Cascadia)"),
                    |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.font_weight, 200.0..=700.0).step_by(50.0),
                        );
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Altura de línea"),
                    i18n::t("Espacio vertical entre líneas"),
                    |ui| {
                        ui.add(egui::Slider::new(&mut cfg.line_height, 1.0..=2.0).step_by(0.05));
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Margen interior"),
                    i18n::t("Espacio entre el borde del panel y el texto"),
                    |ui| {
                        ui.add(egui::Slider::new(&mut cfg.padding, 0.0..=40.0).step_by(1.0));
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Opacidad del fondo"),
                    i18n::t("Transparencia de la ventana"),
                    |ui| {
                        ui.add(egui::Slider::new(&mut cfg.opacity, 0.5..=1.0).step_by(0.05));
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Atenuar paneles inactivos"),
                    i18n::t("Oscurece los paneles divididos sin foco"),
                    |ui| {
                        ui.checkbox(&mut cfg.dim_inactive_panes, "");
                    },
                );
            }
            Cat::Terminal => {
                rows.row(
                    ui,
                    i18n::t("Bloques de comandos"),
                    i18n::t("Historial de bloques y editor propio (PowerShell). Terminales nuevas"),
                    |ui| {
                        ui.checkbox(&mut cfg.command_blocks, "");
                    },
                );
                rows.row(ui, i18n::t("Barra «Ejecutando»"), i18n::t("Barra inferior mientras corre un comando. Desactivada: el proceso ocupa todo el panel"), |ui| {
                    ui.checkbox(&mut cfg.show_running_bar, "");
                });
                rows.row(
                    ui,
                    i18n::t("Restaurar sesión al abrir"),
                    i18n::t("Vuelve a abrir las pestañas, divisiones y carpetas de la última vez"),
                    |ui| {
                        ui.checkbox(&mut cfg.restore_session, "");
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Bloques en historial"),
                    i18n::t("Máximo de bloques guardados por panel"),
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut cfg.max_blocks)
                                .range(10..=10_000)
                                .speed(10),
                        );
                    },
                );
                rows.row(ui, i18n::t("Estilo del cursor"), "", |ui| {
                    ui.selectable_value(&mut cfg.cursor_style, CursorStyle::Underline, i18n::t("Subrayado"));
                    ui.selectable_value(&mut cfg.cursor_style, CursorStyle::Bar, i18n::t("Barra"));
                    ui.selectable_value(&mut cfg.cursor_style, CursorStyle::Block, i18n::t("Bloque"));
                });
                rows.row(ui, i18n::t("Cursor parpadeante"), "", |ui| {
                    ui.checkbox(&mut cfg.cursor_blink, "");
                });
                rows.row(
                    ui,
                    i18n::t("Líneas de historial"),
                    i18n::t("Scrollback. Se aplica a terminales nuevas"),
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut cfg.scrollback)
                                .range(100..=200_000)
                                .speed(100),
                        );
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Copiar al seleccionar"),
                    i18n::t("Copia al soltar el ratón"),
                    |ui| {
                        ui.checkbox(&mut cfg.copy_on_select, "");
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Confirmar al cerrar"),
                    i18n::t("Pregunta antes de cerrar si hay un comando en marcha"),
                    |ui| {
                        ui.checkbox(&mut cfg.confirm_close, "");
                    },
                );
                rows.row(
                    ui,
                    i18n::t("Clic derecho pega"),
                    i18n::t("Con selección, el clic derecho copia"),
                    |ui| {
                        ui.checkbox(&mut cfg.right_click_paste, "");
                    },
                );
            }
            Cat::Shell => {
                rows.row(
                    ui,
                    i18n::t("Shell predeterminado"),
                    i18n::t("Para pestañas y paneles nuevos"),
                    |ui| {
                        let presets = config::shell_presets();
                        let current = presets
                            .iter()
                            .find(|p| p.1 == cfg.shell_program && p.2 == cfg.shell_args)
                            .map(|p| p.0)
                            .unwrap_or(i18n::t("Personalizado"));
                        egui::ComboBox::from_id_salt("shell")
                            .selected_text(current)
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for (name, prog, args) in presets {
                                    if ui.selectable_label(current == name, i18n::t(name)).clicked() {
                                        cfg.shell_program = prog;
                                        cfg.shell_args = args.into();
                                    }
                                }
                            });
                    },
                );
                rows.row(ui, i18n::t("Programa"), i18n::t("Ruta o nombre del ejecutable"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut cfg.shell_program).desired_width(240.0));
                });
                rows.row(ui, i18n::t("Argumentos"), i18n::t("Separados por espacios"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut cfg.shell_args).desired_width(240.0));
                });
                rows.row(
                    ui,
                    i18n::t("Directorio inicial"),
                    i18n::t("Vacío = carpeta de usuario"),
                    |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut cfg.working_directory)
                                .desired_width(240.0)
                                .hint_text("C:\\Users\\…"),
                        );
                    },
                );
            }
            Cat::Keys => {
                if rows.query.is_empty() {
                    ui.label(
                        RichText::new(i18n::t("Haz clic en un atajo y pulsa la nueva combinación. Supr lo deja sin asignar, Esc cancela."))
                            .size(11.5)
                            .color(current.muted()),
                    );
                    if let Some(msg) = &self.key_msg {
                        ui.label(RichText::new(msg).size(11.5).color(current.accent));
                    }
                    ui.add_space(8.0);
                }
                let capturing = &mut self.capturing;
                for (_, id, label, default) in keys::ACTIONS {
                    let value = cfg
                        .keys
                        .get(*id)
                        .cloned()
                        .unwrap_or_else(|| default.to_string());
                    rows.row(ui, i18n::t(label), "", |ui| {
                        let active = *capturing == Some(*id);
                        let text = if active {
                            i18n::t("Pulsa una combinación…").to_string()
                        } else if value.is_empty() {
                            i18n::t("Sin asignar").to_string()
                        } else {
                            Combo::parse(&value)
                                .map(Combo::label)
                                .unwrap_or_else(|| value.clone())
                        };
                        let color = if active {
                            current.accent
                        } else if value.is_empty() {
                            current.faint()
                        } else {
                            current.fg
                        };
                        let btn = egui::Button::new(
                            RichText::new(text).monospace().size(12.0).color(color),
                        )
                        .min_size(Vec2::new(170.0, 24.0))
                        .stroke(if active {
                            Stroke::new(1.0, current.accent)
                        } else {
                            Stroke::NONE
                        });
                        if ui.add(btn).on_hover_text(i18n::t("Cambiar atajo")).clicked() {
                            *capturing = Some(id);
                        }
                        if value != *default
                            && chrome::icon_button(
                                ui,
                                Icon::SmallClose,
                                &format!(
                                    "{} ({})",
                                    i18n::t("Restablecer"),
                                    Combo::parse(default).map(Combo::label).unwrap_or_default()
                                ),
                                Vec2::new(22.0, 22.0),
                                current,
                            )
                            .clicked()
                        {
                            cfg.keys.insert(id.to_string(), default.to_string());
                        }
                    });
                }
                for (action, keys) in FIXED_KEYS {
                    rows.row(ui, i18n::t(action), i18n::t("Fijo"), |ui| {
                        ui.label(
                            RichText::new(i18n::t(keys))
                                .monospace()
                                .size(12.0)
                                .color(current.muted()),
                        );
                    });
                }
                if rows.query.is_empty()
                    && ui
                        .button(RichText::new(i18n::t("Restablecer todos los atajos")).size(12.0))
                        .clicked()
                {
                    cfg.keys = keys::defaults();
                    self.key_msg = None;
                }
            }
        }
    }

    fn start_update(&self, ctx: &egui::Context) {
        let mut st = self.update.locked();
        let update::State::Available(r) = &*st else { return };
        let r = r.clone();
        *st = update::State::Installing(r.version.clone());
        let (upd, c) = (self.update.clone(), ctx.clone());
        std::thread::spawn(move || {
            let next = match update::install(&r) {
                Ok(exe) => update::State::Installed(exe),
                Err(_) => update::State::Available(r), // retry from the button
            };
            *upd.locked() = next;
            c.request_repaint();
        });
    }

    /// New exe in place: save tabs, launch it, quit.
    fn finish_update(&mut self, ctx: &egui::Context) {
        let exe = match &*self.update.locked() {
            update::State::Installed(exe) => exe.clone(),
            _ => return,
        };
        *self.update.locked() = update::State::Idle;
        self.save_session();
        match std::process::Command::new(&exe).spawn() {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.error = Some(format!("{}: {e}", i18n::pick("Actualizado, pero no se pudo reiniciar", "Updated, but couldn't restart"))),
        }
    }

    /// AI agents in any pane: (tab, pane, agent, what it runs, seconds running).
    fn agents(&self) -> Vec<(usize, u64, &'static str, String, Option<u64>)> {
        let mut out = vec![];
        for (i, tab) in self.tabs.iter().enumerate() {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for id in leaves {
                let Some(p) = self.panes.get(&id) else { continue };
                let s = p.shared.locked();
                let block = s.blocks.back().filter(|b| b.running());
                let found = block
                    .and_then(|b| agents::detect(&b.cmd).map(|a| (a, b.cmd.clone(), b.start)))
                    .map(|(a, cmd, start)| (a, cmd, Some(start.elapsed().as_secs())))
                    .or_else(|| agents::detect(&p.title).map(|a| (a, p.title.clone(), None)));
                if let Some((a, what, secs)) = found {
                    out.push((i, id, a, what, secs));
                }
            }
        }
        out
    }

    fn refresh_usage(&self, ctx: &egui::Context) {
        use std::sync::atomic::{AtomicBool, Ordering};
        // One scan at a time: clicking the button repeatedly must not pile up threads.
        static SCANNING: AtomicBool = AtomicBool::new(false);
        if SCANNING.swap(true, Ordering::AcqRel) {
            return;
        }
        let (usage, ctx) = (self.usage.clone(), ctx.clone());
        std::thread::spawn(move || {
            *usage.locked() = Some(agents::claude_usage());
            SCANNING.store(false, Ordering::Release);
            ctx.request_repaint();
        });
    }

    fn agents_popover(&mut self, ctx: &egui::Context) {
        let Some(at) = self.agents_at else { return };
        let t = self.theme();
        let list = self.agents();
        let tab_names: Vec<String> = self.tabs.iter().map(|tb| self.tab_label(tb)).collect();
        let usage = self.usage.locked().clone();
        let mut go = None;
        let area = egui::Area::new(Id::unique("agents"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(at + Vec2::new(0.0, 4.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(t.bg2())
                    .stroke(Stroke::new(1.0, t.border()))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(300.0);
                        ui.label(RichText::new(i18n::t("Agentes activos")).size(13.0).strong().color(t.fg));
                        ui.add_space(4.0);
                        if list.is_empty() {
                            ui.label(RichText::new(i18n::t("Ninguno en ejecución")).size(12.0).color(t.muted()));
                        }
                        for (tab, id, name, what, secs) in &list {
                            let time = secs
                                .map(|s| format!("  ·  {}m {:02}s", s / 60, s % 60))
                                .unwrap_or_default();
                            let text = format!("{name}  ·  {}{time}\n{what}", tab_names[*tab]);
                            if ui
                                .add(egui::Button::new(RichText::new(text).size(12.0).color(t.fg)).frame(false))
                                .on_hover_text(i18n::t("Ir al panel"))
                                .clicked()
                            {
                                go = Some((*tab, *id));
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new(i18n::t("Uso de Claude Code")).size(13.0).strong().color(t.fg));
                        ui.label(
                            RichText::new(i18n::t("Aproximado · solo sesiones de este equipo"))
                                .size(11.0)
                                .color(t.muted()),
                        );
                        ui.add_space(6.0);
                        let heading = |ui: &mut Ui, s: &str| {
                            ui.label(RichText::new(s).size(12.0).strong().color(t.fg));
                        };
                        let line = |ui: &mut Ui, s: String| {
                            ui.label(RichText::new(s).size(12.0).color(t.muted()));
                        };
                        match usage {
                            None => line(ui, i18n::t("Calculando…").into()),
                            Some(u) => {
                                heading(ui, i18n::t("Sesión actual (5 h)"));
                                line(
                                    ui,
                                    format!(
                                        "{} tokens · {} {}",
                                        agents::short(u.last_5h.total()),
                                        u.messages_5h,
                                        i18n::pick("respuestas", "responses"),
                                    ),
                                );
                                if let Some(d) = u.resets_in(std::time::SystemTime::now()) {
                                    let m = d.as_secs() / 60;
                                    line(ui, format!("{} {}h {:02}m", i18n::pick("Se reinicia en", "Resets in"), m / 60, m % 60));
                                }
                                ui.add_space(6.0);
                                heading(ui, i18n::t("Últimos 7 días"));
                                line(ui, format!("{} tokens", agents::short(u.last_7d.total())));
                                if !u.by_model.is_empty() {
                                    ui.add_space(6.0);
                                    heading(ui, i18n::t("Por modelo (7 días)"));
                                    let total = u.by_model.iter().map(|m| m.1).sum::<u64>().max(1) as f32;
                                    for (model, n) in &u.by_model {
                                        let frac = *n as f32 / total;
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [110.0, 14.0],
                                                egui::Label::new(RichText::new(model).size(11.5).color(t.fg)).truncate(),
                                            );
                                            let (r, _) = ui.allocate_exact_size(Vec2::new(120.0, 8.0), Sense::hover());
                                            ui.painter().rect_filled(r, 3.0, t.bg3());
                                            let mut f = r;
                                            f.set_width(r.width() * frac);
                                            ui.painter().rect_filled(f, 3.0, t.accent);
                                            ui.label(RichText::new(format!("{:.0}%", frac * 100.0)).size(11.5).color(t.muted()));
                                        });
                                    }
                                }
                            }
                        }
                    });
            });
        if let Some((tab, id)) = go {
            self.active = tab;
            self.tabs[tab].focus = id;
            self.agents_at = None;
        }
        // Close on Esc or a click outside (the top bar toggles it itself).
        let outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| p.y > BAR_H && !area.response.rect.contains(p))
        });
        if outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.agents_at = None;
        }
    }

    fn error_toast(&mut self, ctx: &egui::Context) {
        let Some(err) = self.error.clone() else {
            return;
        };
        let t = self.theme();
        egui::Area::new(Id::unique("toast"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(t.bg2())
                    .stroke(Stroke::new(1.0, t.error().gamma_multiply(0.6)))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(err).size(12.0).color(t.fg));
                            if chrome::icon_button(
                                ui,
                                Icon::SmallClose,
                                "",
                                Vec2::new(20.0, 20.0),
                                t,
                            )
                            .clicked()
                            {
                                self.error = None;
                            }
                        });
                    });
            });
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.settings_open || self.renaming.is_some() || self.confirm.is_some() {
            return;
        }
        // Images and dropped files become pasted paths: AI CLIs (Claude Code,
        // Codex…) attach an image when its file path is pasted.
        for e in raw.events.iter_mut() {
            if let Event::PasteImage(img) = e
                && let Some(path) = pane::save_png(img) {
                    *e = Event::Paste(path);
                }
        }
        let dropped: Vec<String> = std::mem::take(&mut raw.dropped_files)
            .into_iter()
            .map(|f| pane::quote_path(&f.path().to_string_lossy()))
            .collect();
        if !dropped.is_empty() {
            raw.events.push(Event::Paste(dropped.join(" ")));
        }
        let editor = self.focused().filter(|p| p.editor_active()).map(|p| {
            let e = &p.editor;
            let sugg = e.at_end() && suggestion(&self.history, &e.text).is_some();
            (
                e.on_first_line(),
                e.on_last_line(),
                sugg,
                e.has_selection,
                !e.candidates.is_empty(),
            )
        });
        let Some((first, last, sugg, has_sel, popup)) = editor else {
            // A process owns the keyboard: steal everything before egui sees it
            // (otherwise Tab/arrows would move UI focus).
            raw.events.retain(|e| {
                let take = matches!(
                    e,
                    Event::Key { pressed: true, .. }
                        | Event::Text(_)
                        | Event::Paste(_)
                        | Event::Copy
                        | Event::Cut
                );
                if take {
                    self.pending.push((e.clone(), false));
                }
                !take
            });
            return;
        };
        // Line editor: egui's TextEdit does the typing; we take Enter, Tab,
        // history arrows and shortcuts.
        raw.events.retain(|e| {
            let (push, steal) = match e {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers: m,
                    ..
                } => {
                    let plain = !m.ctrl && !m.alt && !m.shift;
                    let steal = match key {
                        Key::Enter => !(m.shift && !m.ctrl && !m.alt),
                        Key::Tab => true,
                        Key::Escape => plain && popup,
                        Key::ArrowUp => plain && (first || popup),
                        Key::ArrowDown => plain && (last || popup),
                        Key::ArrowRight => plain && sugg,
                        _ => m.alt,
                    } || keys::lookup(&self.cfg.keys, *key, *m).is_some();
                    (steal || m.ctrl || m.alt, steal)
                }
                Event::Copy => (!has_sel, !has_sel),
                _ => (false, false),
            };
            if push {
                self.pending.push((e.clone(), true));
            }
            !steal
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
        if self.unsaved_since.is_some() {
            self.cfg.save();
        }
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);

        let mut early = None;
        let exited: Vec<u64> = self
            .panes
            .iter_mut()
            .filter_map(|(id, p)| {
                p.poll(&ctx);
                if !p.exited || p.kept {
                    return None;
                }
                // A shell that dies right away (bad program / arguments, broken profile)
                // stays open as a plain terminal so its error can be read.
                if p.just_started() {
                    p.kept = true;
                    p.integrated = false;
                    early = Some(p.exit_code());
                    return None;
                }
                Some(*id)
            })
            .collect();
        if let Some(code) = early {
            self.error = Some(format!(
                "{} «{}» {} ({}). {}",
                i18n::pick("El shell", "The shell"),
                self.cfg.shell_program,
                i18n::pick("se cerró al iniciar", "exited on start"),
                code.map_or("?".into(), |c| c.to_string()),
                i18n::pick(
                    "Revisa Ajustes → Shell. Cierra el panel con Ctrl+Shift+W.",
                    "Check Settings → Shell. Close the pane with Ctrl+Shift+W."
                ),
            ));
        }
        for id in exited {
            self.close_pane(id);
        }
        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);

        if self.theme().name != self.applied_theme {
            self.apply_style(&ctx);
        }
        // Changes apply at once but reach the disk once they settle: dragging a slider
        // would otherwise rewrite config.toml every frame.
        let now = ctx.input(|i| i.time);
        if self.cfg != self.saved {
            self.saved = self.cfg.clone();
            self.apply_style(&ctx);
            self.unsaved_since = Some(now);
            ctx.request_repaint_after(Duration::from_millis(600));
        }
        if self.unsaved_since.is_some_and(|t| now - t > 0.5) {
            self.cfg.save();
            self.unsaved_since = None;
        }

        let queued: Vec<u64> = self
            .panes
            .iter()
            .filter(|(_, p)| p.editor.queued && p.phase() == Phase::Ready)
            .map(|(id, _)| *id)
            .collect();
        for id in queued {
            self.submit(id);
        }
        if now - self.last_session_save > 3.0 {
            self.last_session_save = now;
            self.save_session();
        }
        // Running durations tick once per second, and only while you're looking:
        // an unfocused Rayo with `pnpm dev` running draws nothing unless output arrives.
        let window_focused = ctx.input(|i| i.focused);
        if window_focused
            && self
                .panes
                .values()
                .any(|p| p.shared.locked().running())
        {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        let title = self
            .focused()
            .map(|p| format!("{} — Rayo", p.title))
            .unwrap_or_else(|| "Rayo".into());
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        let on_edge = chrome::edge_resize(&ctx);
        let theme = self.theme();
        let bg = theme.bg.gamma_multiply(self.cfg.opacity);
        egui::Panel::top("bar")
            .exact_size(BAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(bg))
            .show(ui, |ui| self.title_bar(ui, &ctx));
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ui, |ui| {
                if on_edge {
                    ui.disable();
                }
                self.terminals(ui)
            });
        if self.settings_open {
            // Scrim behind the settings sheet.
            let screen = ctx.content_rect();
            egui::Area::new(Id::unique("scrim"))
                .fixed_pos(screen.min)
                .order(egui::Order::Middle)
                .show(&ctx, |ui| {
                    ui.painter()
                        .rect_filled(screen, 0.0, Color32::from_black_alpha(110));
                    if ui.allocate_rect(screen, Sense::click()).clicked() {
                        self.settings_open = false;
                    }
                });
            self.settings(&ctx);
        }
        self.finish_update(&ctx);
        self.agents_popover(&ctx);
        self.error_toast(&ctx);
        // Window close (button, Alt+F4, taskbar): ask first if something is running.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.close_ok
            && self.cfg.confirm_close
            && self.busy(&self.close_targets(CloseReq::Window)) > 0
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(CloseReq::Window);
        }
        self.confirm_dialog(&ctx);
    }
}

/// Warp-style completion list floating above the editor. Returns a clicked entry.
fn completion_popup(
    ui: &Ui,
    id: u64,
    e: &Editor,
    bottom_left: Pos2,
    t: &Theme,
    m: &Metrics,
) -> Option<String> {
    if e.candidates.is_empty() {
        return None;
    }
    let prefix_len = shell::completion_prefix(&e.text).chars().count();
    let row_h = 26.0;
    let mut chosen = None;
    egui::Area::new(Id::unique(("completion", id)))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(bottom_left)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(t.bg2())
                .stroke(Stroke::new(1.0, t.border()))
                .corner_radius(8)
                .inner_margin(4)
                .shadow(egui::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                })
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(row_h * 8.0)
                        .show(ui, |ui| {
                            for (i, cand) in e.candidates.iter().enumerate() {
                                let dir = cand.ends_with('\\');
                                let name = cand.trim_end_matches('\\');
                                let (r, resp) =
                                    ui.allocate_exact_size(Vec2::new(340.0, row_h), Sense::click());
                                let sel = i == e.cand_sel;
                                if sel {
                                    ui.painter().rect_filled(r, 5.0, t.bg3());
                                    resp.scroll_to_me(None);
                                } else if resp.hovered() {
                                    ui.painter()
                                        .rect_filled(r, 5.0, t.bg3().gamma_multiply(0.5));
                                }
                                let p = ui.painter();
                                // Folder / file glyph, drawn.
                                let c = Pos2::new(r.min.x + 16.0, r.center().y);
                                let ic = t.muted();
                                if dir {
                                    p.rect_stroke(
                                        Rect::from_center_size(
                                            c + Vec2::new(0.0, 1.0),
                                            Vec2::new(12.0, 9.0),
                                        ),
                                        1.5,
                                        Stroke::new(1.1, ic),
                                        egui::StrokeKind::Middle,
                                    );
                                    p.hline(c.x - 6.0..=c.x - 1.0, c.y - 4.5, Stroke::new(1.1, ic));
                                } else {
                                    p.rect_stroke(
                                        Rect::from_center_size(c, Vec2::new(9.0, 12.0)),
                                        1.5,
                                        Stroke::new(1.1, ic),
                                        egui::StrokeKind::Middle,
                                    );
                                }
                                // Typed prefix bold, rest regular, then the kind.
                                let split = name
                                    .char_indices()
                                    .nth(prefix_len)
                                    .map(|(b, _)| b)
                                    .unwrap_or(name.len());
                                let mut job = egui::text::LayoutJob::default();
                                job.append(
                                    &name[..split],
                                    0.0,
                                    egui::TextFormat::simple(m.bold.clone(), t.fg),
                                );
                                job.append(
                                    &name[split..],
                                    0.0,
                                    egui::TextFormat::simple(
                                        m.font.clone(),
                                        t.fg.gamma_multiply(0.85),
                                    ),
                                );
                                let g = p.layout_job(job);
                                let ty = r.center().y - g.size().y / 2.0;
                                let kind_x = r.min.x + 32.0 + g.size().x + 10.0;
                                p.galley(Pos2::new(r.min.x + 32.0, ty), g, t.fg);
                                p.text(
                                    Pos2::new(kind_x, r.center().y),
                                    Align2::LEFT_CENTER,
                                    if dir { i18n::t("Carpeta") } else { i18n::t("Archivo") },
                                    FontId::proportional(11.5),
                                    t.muted(),
                                );
                                if resp.clicked() {
                                    chosen = Some(cand.clone());
                                }
                            }
                        });
                });
        });
    chosen
}

/// Newest history entry extending `text` (single-line input only).
fn suggestion<'a>(history: &'a [String], text: &str) -> Option<&'a str> {
    if text.is_empty() || text.contains('\n') {
        return None;
    }
    history
        .iter()
        .rev()
        .find(|h| h.len() > text.len() && h.starts_with(text))
        .map(|h| &h[text.len()..])
}

/// Bottom input area: context line + Rayo's own line editor, or a running indicator.
#[allow(clippy::too_many_arguments)]
fn editor_ui(
    ui: &mut Ui,
    rect: Rect,
    id: u64,
    pane: &mut Pane,
    history: &[String],
    cfg: &Config,
    t: &Theme,
    m: &Metrics,
    take_focus: bool,
) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, t.bg2());
    p.hline(
        rect.x_range(),
        rect.min.y + 0.5,
        Stroke::new(1.0, t.border()),
    );
    let pad = cfg.padding;
    let meta = FontId::proportional(11.5);
    let phase = pane.phase();
    let top = rect.min.y + 20.0;

    {
        let cwd = pane.cwd().unwrap_or_default();
        let branch = match &pane.editor.branch {
            Some((c, b)) if *c == cwd => b.clone(),
            _ => {
                let b = shell::git_branch(&cwd);
                pane.editor.branch = Some((cwd.clone(), b.clone()));
                b
            }
        };
        // Warp-style chips: folder + path, branch icon + branch.
        let chip_font = FontId::new(
            (cfg.editor_font_size - 2.0).max(10.0),
            FontFamily::Name("bold".into()),
        );
        let mut x = rect.min.x + pad;
        // Clicking a chip opens a menu: subfolders to `cd` into, or branches to switch to.
        let clickable = pane.integrated && phase == Phase::Ready;
        let mut run = None;
        if !cwd.is_empty() {
            let c = config::pick(cfg.colors.path, t.path());
            let r = chrome::chip(
                &p,
                Pos2::new(x, top),
                chrome::ChipIcon::Folder,
                &pane::short_path(&cwd),
                chip_font.clone(),
                c,
                t,
            );
            x = r.max.x + 8.0;
            if clickable {
                let resp = ui
                    .interact(r, Id::unique(("cwd-chip", id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    pane.editor.nav = shell::subdirs(&cwd);
                }
                egui::Popup::menu(&resp).show(|ui| {
                    // Room on the left for a drawn folder icon (emoji glyphs show as boxes).
                    let folder_button = |ui: &mut Ui, name: &str| {
                        let r = ui.button(format!("       {name}"));
                        let color = ui.visuals().widgets.style(&r).text_color();
                        chrome::draw_folder(ui.painter(), r.rect.left_center() + Vec2::new(14.0, 0.0), color);
                        r.clicked()
                    };
                    if std::path::Path::new(&cwd).parent().is_some() && folder_button(ui, "..") {
                        run = Some("cd ..".to_string());
                    }
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for d in &pane.editor.nav {
                            if folder_button(ui, d) {
                                run = Some(format!("cd -LiteralPath {}", shell::ps_quote(d)));
                            }
                        }
                    });
                    if pane.editor.nav.is_empty() {
                        ui.weak(i18n::t("Sin subcarpetas"));
                    }
                });
            }
        }
        if let Some(b) = branch {
            let c = config::pick(cfg.colors.branch, t.branch());
            let r = chrome::chip(
                &p,
                Pos2::new(x, top),
                chrome::ChipIcon::Branch,
                &b,
                chip_font,
                c,
                t,
            );
            if clickable {
                let resp = ui
                    .interact(r, Id::unique(("branch-chip", id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    pane.editor.nav = shell::git_branches(&cwd);
                }
                egui::Popup::menu(&resp).show(|ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for br in &pane.editor.nav {
                            let cur = *br == b;
                            let label = if cur { format!("✓  {br}") } else { format!("     {br}") };
                            if ui.add_enabled(!cur, egui::Button::new(label)).clicked() {
                                run = Some(format!("git switch {}", shell::ps_quote(br)));
                            }
                        }
                    });
                });
            }
        }
        if let Some(cmd) = run {
            pane.submit(&cmd);
        }
    }

    let input = Rect::from_min_max(
        Pos2::new(rect.min.x + pad, rect.min.y + 42.0),
        Pos2::new(rect.max.x - pad, rect.max.y - 6.0),
    );
    if phase == Phase::Running {
        let y = input.min.y + m.editor_row / 2.0;
        p.circle_filled(Pos2::new(input.min.x + 4.0, y), 3.5, t.accent);
        let cmd = pane
            .shared
            .lock()
            .unwrap()
            .blocks
            .back()
            .map(|b| b.cmd.lines().next().unwrap_or("").to_string())
            .unwrap_or_default();
        let hint = p.text(
            Pos2::new(input.max.x, y),
            Align2::RIGHT_CENTER,
            i18n::t("Ctrl+C interrumpe · el teclado va al proceso"),
            meta,
            t.faint(),
        );
        // Long commands are clipped before the hint instead of overlapping it.
        let room = Rect::from_min_max(
            Pos2::new(input.min.x, rect.min.y),
            Pos2::new(hint.min.x - 16.0, rect.max.y),
        );
        p.with_clip_rect(room).text(
            Pos2::new(input.min.x + 16.0, y),
            Align2::LEFT_CENTER,
            format!("{}   {cmd}", i18n::pick("Ejecutando", "Running")),
            FontId::proportional(12.5),
            config::pick(cfg.colors.secondary, t.meta),
        );
        return;
    }

    let te_id = Id::unique(("editor", id));
    let hint = if phase == Phase::Starting {
        i18n::t("Iniciando PowerShell…")
    } else {
        i18n::t("Escribe un comando...")
    };
    let mut child = ui.new_child(UiBuilder::new().max_rect(input));
    // Known commands (claude, git, Get-ChildItem…) in the accent colour, the rest as typed.
    let typed = config::pick(cfg.colors.typed, t.command);
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let text = buf.as_str();
        let fmt = |c| egui::TextFormat::simple(m.editor_font.clone(), c);
        let mut job = egui::text::LayoutJob::default();
        let mut pos = 0;
        for r in shell::command_spans(text, shell::is_command) {
            job.append(&text[pos..r.start], 0.0, fmt(typed));
            job.append(&text[r.clone()], 0.0, fmt(t.accent));
            pos = r.end;
        }
        job.append(&text[pos..], 0.0, fmt(typed));
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let out = egui::TextEdit::multiline(&mut pane.editor.text)
        .id(te_id)
        .font(m.editor_font.clone())
        .frame(egui::Frame::NONE)
        .margin(Vec2::ZERO)
        .desired_width(f32::INFINITY)
        .desired_rows(1)
        .lock_focus(true)
        .layouter(&mut layouter)
        .hint_text(
            RichText::new(hint)
                .color(t.faint())
                .font(m.editor_font.clone()),
        )
        .show(&mut child);
    if take_focus && !out.response.has_focus() {
        out.response.request_focus();
    }
    let cwd = pane.cwd().unwrap_or_default();
    let e = &mut pane.editor;
    if out.response.changed() {
        e.hist = None;
        // Typing with the list open filters it live.
        if !e.candidates.is_empty() {
            e.candidates = shell::path_candidates(&e.text, &cwd);
            e.cand_sel = 0;
        }
    }
    if let Some(chosen) = completion_popup(
        ui,
        id,
        e,
        Pos2::new(input.min.x - 6.0, rect.min.y - 6.0),
        t,
        m,
    ) {
        e.text = shell::apply_completion(&e.text, &chosen);
        e.candidates.clear();
        e.set_cursor_end = true;
    }
    if e.set_cursor_end {
        let n = e.cursor_to.take().unwrap_or_else(|| e.text.chars().count());
        let mut st = out.state.clone();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(n),
        )));
        st.store(ui.ctx(), te_id);
        e.cursor = n;
        e.has_selection = false;
        e.set_cursor_end = false;
    } else if let Some(cr) = out.cursor_range {
        e.cursor = cr.primary.index.into();
        e.has_selection = cr.primary.index != cr.secondary.index;
    }
    if e.at_end() && out.response.has_focus()
        && let Some(rest) = suggestion(history, &e.text) {
            let pos = out.galley_pos + Vec2::new(out.galley.rect.width(), 0.0);
            p.text(
                pos,
                Align2::LEFT_TOP,
                rest,
                m.editor_font.clone(),
                t.faint(),
            );
        }
}

/// "Personalizado" toggle + colour picker; shows the theme's colour when off.
fn color_option(ui: &mut Ui, value: &mut Option<[u8; 3]>, default: Color32) {
    let mut custom = value.is_some();
    if let Some(rgb) = value.as_mut() {
        ui.color_edit_button_srgb(rgb);
    } else {
        let (r, _) = ui.allocate_exact_size(Vec2::new(28.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 4.0, default);
    }
    if ui.checkbox(&mut custom, i18n::t("Personalizado")).changed() {
        *value = custom.then(|| [default.r(), default.g(), default.b()]);
    }
}

/// Shorten text with an ellipsis to fit `max` points.
fn elide(ui: &Ui, s: &str, font: &FontId, max: f32) -> String {
    let width = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if width(s) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut n = chars.len();
    while n > 1 && width(&format!("{}…", chars[..n].iter().collect::<String>())) > max {
        n -= 1;
    }
    format!("{}…", chars[..n].iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_dir_should_read_positional_and_flag_forms() {
        let a = |v: &[&str]| start_dir(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&[r"C:\dev"]), Some(r"C:\dev".into()));
        assert_eq!(a(&["-d", r"D:\x"]), Some(r"D:\x".into()));
        assert_eq!(a(&["C:\\\""]), Some("C:\\".into()));
        assert_eq!(a(&["--help"]), None);
        assert_eq!(a(&[]), None);
    }

    #[test]
    fn split_remove_and_equalize() {
        let mut root = Node::Leaf(1);
        assert!(root.split(1, 2, Dir::Right));
        assert!(root.split(2, 3, Dir::Right));
        root.equalize();
        // Three panes in a row: first split gives 1/3 to pane 1.
        let Node::Split { ratio, .. } = &root else {
            panic!()
        };
        assert!((ratio - 1.0 / 3.0).abs() < 1e-6);
        let mut v = vec![];
        root.leaves(&mut v);
        assert_eq!(v, [1, 2, 3]);
        let root = root.remove(2).unwrap();
        let mut v = vec![];
        root.leaves(&mut v);
        assert_eq!(v, [1, 3]);
        let root = root.remove(1).unwrap();
        assert!(matches!(root, Node::Leaf(3)));
        assert!(root.remove(3).is_none());
    }

    #[test]
    fn search_folds_accents() {
        assert!(fold("Líneas de historial").contains(&fold("lineas")));
    }
}
