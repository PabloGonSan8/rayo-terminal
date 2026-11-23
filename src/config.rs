use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}

/// Per-element text colours (None = derived from the theme).
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct TextColors {
    pub command: Option<[u8; 3]>,
    pub typed: Option<[u8; 3]>,
    pub path: Option<[u8; 3]>,
    pub branch: Option<[u8; 3]>,
    pub secondary: Option<[u8; 3]>,
}

/// Custom colour or the theme's default.
pub fn pick(custom: Option<[u8; 3]>, default: egui::Color32) -> egui::Color32 {
    custom
        .map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
        .unwrap_or(default)
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Config {
    /// UI language: "auto" (Windows language), "es" or "en".
    pub language: String,
    pub theme: String,
    /// Custom text colour over any theme (None = the theme's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<[u8; 3]>,
    /// Switch between `day_theme` and `night_theme` by local time.
    pub auto_theme: bool,
    pub day_theme: String,
    pub night_theme: String,
    pub day_start: u32,
    pub night_start: u32,
    pub font: String,
    pub font_size: f32,
    pub font_weight: f32,
    pub line_height: f32,
    pub padding: f32,
    pub opacity: f32,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub dim_inactive_panes: bool,
    pub command_blocks: bool,
    /// Show the "Ejecutando…" bar while a command runs (off: the process gets the whole pane).
    pub show_running_bar: bool,
    /// Bottom editor: font size and minimum number of lines.
    pub editor_font_size: f32,
    pub editor_lines: u32,
    /// Reopen the last tabs, splits and folders on start.
    pub restore_session: bool,
    pub shell_program: String,
    pub shell_args: String,
    pub working_directory: String,
    pub scrollback: usize,
    pub max_blocks: usize,
    pub copy_on_select: bool,
    pub right_click_paste: bool,
    /// Ask before closing a pane / tab / the window while a command runs.
    pub confirm_close: bool,
    /// Flash the taskbar when a command of 10 s or more ends while Rayo isn't focused.
    pub notify_done: bool,
    pub colors: TextColors,
    /// Shortcut bindings: action id -> "Ctrl+Shift+D" (empty = unassigned).
    pub keys: std::collections::BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            theme: "Rayo".into(),
            text_color: None,
            auto_theme: false,
            day_theme: "Claro".into(),
            night_theme: "Rayo".into(),
            day_start: 8,
            night_start: 20,
            font: "Cascadia Code".into(),
            font_size: 13.5,
            font_weight: 400.0,
            line_height: 1.35,
            padding: 14.0,
            opacity: 1.0,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            dim_inactive_panes: true,
            command_blocks: true,
            show_running_bar: false,
            editor_font_size: 14.5,
            editor_lines: 1,
            restore_session: true,
            shell_program: default_shell().0.into(),
            shell_args: default_shell().1.into(),
            working_directory: String::new(),
            scrollback: 10_000,
            max_blocks: 1000,
            copy_on_select: false,
            right_click_paste: true,
            confirm_close: true,
            notify_done: true,
            colors: TextColors::default(),
            keys: crate::keys::defaults(),
        }
    }
}

fn default_shell() -> (&'static str, &'static str) {
    if which("pwsh.exe") {
        ("pwsh.exe", "-NoLogo")
    } else {
        ("powershell.exe", "-NoLogo")
    }
}

pub fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

impl Config {
    /// Theme to use at `hour` (0-23), honouring the day/night schedule.
    pub fn theme_at(&self, hour: u32) -> &str {
        if !self.auto_theme {
            return &self.theme;
        }
        let (d, n) = (self.day_start, self.night_start);
        let day = if d <= n {
            (d..n).contains(&hour)
        } else {
            hour >= d || hour < n
        };
        if day {
            &self.day_theme
        } else {
            &self.night_theme
        }
    }

    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rayo")
            .join("config.toml")
    }

    /// Settings from disk. A file that doesn't parse is copied to `config.invalid.toml`
    /// (the next save would otherwise overwrite the user's hand edits) and the error is
    /// returned so the UI can say so.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match toml::from_str::<Config>(&text) {
            Ok(mut c) => {
                crate::keys::migrate(&mut c.keys);
                // Themes get renamed; fall back instead of showing a stale name.
                if !crate::theme::THEMES.iter().any(|t| t.name == c.theme) {
                    c.theme = Config::default().theme;
                }
                (c, None)
            }
            Err(e) => {
                let backup = path.with_file_name("config.invalid.toml");
                let _ = std::fs::copy(&path, &backup);
                let line = e.message().lines().next().unwrap_or_default().to_string();
                (Self::default(), Some(format!("{line} → {}", backup.display())))
            }
        }
    }

    pub fn save(&self) {
        if let Ok(s) = toml::to_string_pretty(self) {
            write_atomic(&Self::path(), &s);
        }
    }
}

/// Write then rename: a crash or full disk mid-write never leaves a half file.
fn write_atomic(path: &std::path::Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

/// Window layout persisted between runs: tabs, splits and each pane's folder.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Session {
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct SavedTab {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Focused pane, as an index into the tab's leaves (left to right).
    #[serde(default)]
    pub focus: usize,
    pub root: SavedNode,
}

/// A pane (`cwd`) or a split of two nodes.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct SavedNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<Box<SavedSplit>>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct SavedSplit {
    pub down: bool,
    pub ratio: f32,
    pub a: SavedNode,
    pub b: SavedNode,
}

impl Session {
    fn path() -> PathBuf {
        Config::path().with_file_name("session.toml")
    }

    pub fn load() -> Option<Self> {
        toml::from_str(&std::fs::read_to_string(Self::path()).ok()?).ok()
    }

    pub fn save(&self) {
        if let Ok(s) = toml::to_string_pretty(self) {
            write_atomic(&Self::path(), &s);
        }
    }
}

fn history_path() -> PathBuf {
    Config::path().with_file_name("history.txt")
}

const HISTORY_MAX: usize = 5000;

/// Command history shared by every pane (newest last). Newlines are stored as U+2028.
/// The file is append-only while running, so it is trimmed back here once it doubles.
pub fn load_history() -> Vec<String> {
    let s = std::fs::read_to_string(history_path()).unwrap_or_default();
    let lines: Vec<&str> = s.lines().collect();
    let keep = &lines[lines.len().saturating_sub(HISTORY_MAX)..];
    if lines.len() > 2 * HISTORY_MAX {
        write_atomic(&history_path(), &(keep.join("\n") + "\n"));
    }
    keep.iter().map(|l| l.replace('\u{2028}', "\n")).collect()
}

pub fn append_history(cmd: &str) {
    use std::io::Write;
    if let Some(dir) = history_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path())
    {
        let _ = writeln!(f, "{}", cmd.replace('\n', "\u{2028}"));
    }
}

pub fn local_hour() -> u32 {
    // SAFETY: SYSTEMTIME is plain data; all-zero is valid.
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: GetLocalTime only writes into the struct we pass.
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
    t.wHour as u32
}

/// Shell presets shown in settings. Only those found on the system are listed.
pub fn shell_presets() -> Vec<(&'static str, String, &'static str)> {
    let mut v = vec![];
    if which("pwsh.exe") {
        v.push(("PowerShell 7", "pwsh.exe".to_string(), "-NoLogo"));
    }
    v.push((
        "Windows PowerShell",
        "powershell.exe".to_string(),
        "-NoLogo",
    ));
    v.push(("Símbolo del sistema (CMD)", "cmd.exe".to_string(), ""));
    if which("wsl.exe") {
        v.push(("WSL", "wsl.exe".to_string(), ""));
    }
    // Machine-wide and per-user ("Only for me") installs of Git for Windows.
    let git_bash = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .flat_map(|r| [r.join(r"Git\bin\bash.exe"), r.join(r"Programs\Git\bin\bash.exe")])
        .find(|p| p.is_file());
    if let Some(p) = git_bash {
        v.push(("Git Bash", p.to_string_lossy().into_owned(), "--login -i"));
    }
    v
}

/// Monospace fonts we know how to find in the Windows font folders.
const FONT_FILES: &[(&str, &str)] = &[
    ("Cascadia Code", "CascadiaCode.ttf"),
    ("Cascadia Mono", "CascadiaMono.ttf"),
    ("Consolas", "consola.ttf"),
    ("JetBrains Mono", "JetBrainsMono-Regular.ttf"),
    ("Fira Code", "FiraCode-Regular.ttf"),
    ("Hack", "Hack-Regular.ttf"),
    ("Source Code Pro", "SourceCodePro-Regular.ttf"),
    ("Lucida Console", "lucon.ttf"),
    ("Courier New", "cour.ttf"),
];

/// `%WINDIR%\Fonts` (Windows isn't always on C:).
pub fn fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

pub fn font_path(name: &str) -> Option<PathBuf> {
    let file = FONT_FILES.iter().find(|(n, _)| *n == name)?.1;
    let mut dirs = vec![fonts_dir()];
    if let Some(local) = dirs::data_local_dir() {
        dirs.push(local.join(r"Microsoft\Windows\Fonts"));
    }
    dirs.into_iter().map(|d| d.join(file)).find(|p| p.is_file())
}

pub fn available_fonts() -> Vec<&'static str> {
    let mut v: Vec<_> = FONT_FILES
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| font_path(n).is_some())
        .collect();
    v.push("Integrada");
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_roundtrip_and_partial_file() {
        let c = Config::default();
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(c, back);
        // Missing keys fall back to defaults so old config files keep working.
        let partial: Config = toml::from_str("font_size = 20.0").unwrap();
        assert_eq!(partial.font_size, 20.0);
        assert_eq!(partial.scrollback, 10_000);
    }

    #[test]
    fn session_roundtrip() {
        let leaf = |c: &str| SavedNode {
            cwd: Some(c.into()),
            split: None,
        };
        let s = Session {
            active: 1,
            tabs: vec![
                SavedTab {
                    name: None,
                    focus: 0,
                    root: leaf("C:\\a"),
                },
                SavedTab {
                    name: Some("web".into()),
                    focus: 1,
                    root: SavedNode {
                        cwd: None,
                        split: Some(Box::new(SavedSplit {
                            down: false,
                            ratio: 0.4,
                            a: leaf("D:\\x"),
                            b: leaf("D:\\y"),
                        })),
                    },
                },
            ],
        };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Session>(&text).unwrap(), s);
    }

    #[test]
    fn theme_schedule() {
        let mut c = Config {
            auto_theme: true,
            day_theme: "D".into(),
            night_theme: "N".into(),
            ..Config::default()
        };
        (c.day_start, c.night_start) = (8, 20);
        assert_eq!(
            (c.theme_at(7), c.theme_at(8), c.theme_at(19), c.theme_at(20)),
            ("N", "D", "D", "N")
        );
        // Night shift: day from 22 to 6.
        (c.day_start, c.night_start) = (22, 6);
        assert_eq!(
            (c.theme_at(23), c.theme_at(3), c.theme_at(12)),
            ("D", "D", "N")
        );
        c.auto_theme = false;
        assert_eq!(c.theme_at(3), c.theme);
    }
}
