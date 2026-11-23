//! PowerShell integration, OSC mark scanning and editor helpers.
//!
//! ConPTY does not tell us when a command starts or ends, so PowerShell is started
//! with a small script (see `PS_INTEGRATION`) that:
//! - replaces `prompt` with one that prints `OSC 133;D;<exit code>`, clears the
//!   screen, reports the cwd (`OSC 9;9`) and prints `OSC 133;B` (ready). No visible
//!   prompt text: Rayo draws its own editor.
//! - removes PSReadLine: Rayo owns line editing, so input is plain cooked-mode.
//!
//! Each command is sent wrapped (`wrap_command`): it resets `$LASTEXITCODE`,
//! remembers `$Error[0]`, prints `OSC 133;C` (output starts here) and dot-sources
//! the user's command decoded from Base64 (so quotes / multi-line text survive).

use std::path::{Path, PathBuf};

use base64::Engine;

#[derive(Debug, PartialEq, Clone)]
pub enum Mark {
    /// Shell is idle and waiting for a command.
    Ready,
    /// Output of the submitted command starts at the cursor.
    Exec,
    /// Command finished with this exit code.
    Done(i32),
    Cwd(String),
}

#[derive(Debug, PartialEq)]
pub enum Seg {
    Bytes(Vec<u8>),
    Mark(Mark),
}

/// Splits PTY output into plain bytes and our marks, across read boundaries.
#[derive(Default)]
pub struct Scanner {
    pending: Vec<u8>,
}

impl Scanner {
    pub fn feed(&mut self, data: &[u8]) -> Vec<Seg> {
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut out = vec![];
        let mut plain: Vec<u8> = vec![];
        let mut i = 0;
        while i < buf.len() {
            let Some(k) = find(&buf[i..], b"\x1b]") else {
                // A trailing ESC may start an OSC split across reads.
                let end = if buf.last() == Some(&0x1b) {
                    buf.len() - 1
                } else {
                    buf.len()
                };
                plain.extend_from_slice(&buf[i..end]);
                self.pending.extend_from_slice(&buf[end..]);
                break;
            };
            let s = i + k;
            plain.extend_from_slice(&buf[i..s]);
            match find_terminator(&buf[s + 2..]) {
                None if buf.len() - s < 8192 => {
                    self.pending.extend_from_slice(&buf[s..]);
                    break;
                }
                None => {
                    plain.extend_from_slice(&buf[s..]);
                    break;
                }
                Some((body_len, term_len)) => {
                    let end = s + 2 + body_len + term_len;
                    match parse_mark(&buf[s + 2..s + 2 + body_len]) {
                        Some(m) => {
                            if !plain.is_empty() {
                                out.push(Seg::Bytes(std::mem::take(&mut plain)));
                            }
                            out.push(Seg::Mark(m));
                        }
                        None => plain.extend_from_slice(&buf[s..end]),
                    }
                    i = end;
                }
            }
        }
        if !plain.is_empty() {
            out.push(Seg::Bytes(plain));
        }
        out
    }
}

fn parse_mark(body: &[u8]) -> Option<Mark> {
    let body = std::str::from_utf8(body).ok()?;
    if let Some(cwd) = body.strip_prefix("9;9;") {
        return Some(Mark::Cwd(cwd.trim_matches('"').to_string()));
    }
    let mark = body.strip_prefix("133;")?;
    let (kind, arg) = mark.split_once(';').unwrap_or((mark, ""));
    match kind {
        "B" => Some(Mark::Ready),
        "C" => Some(Mark::Exec),
        "D" => Some(Mark::Done(
            arg.split(';')
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(0),
        )),
        _ => None,
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// (body length, terminator length) for BEL or ESC \ terminated strings.
fn find_terminator(b: &[u8]) -> Option<(usize, usize)> {
    for j in 0..b.len() {
        match b[j] {
            0x07 => return Some((j, 1)),
            0x1b if j + 1 == b.len() => return None,
            0x1b if b[j + 1] == b'\\' => return Some((j, 2)),
            // Aborted OSC: pass through up to the new escape.
            0x1b => return Some((j, 0)),
            _ => {}
        }
    }
    None
}

const PS_INTEGRATION: &str = r#"
Remove-Module PSReadLine -ErrorAction SilentlyContinue
$global:__rayo_e0 = $null
function global:prompt {
  $c = 0
  if ($global:LASTEXITCODE) { $c = $global:LASTEXITCODE }
  elseif ($global:Error.Count -gt 0 -and -not [object]::ReferenceEquals($global:Error[0], $global:__rayo_e0)) { $c = 1 }
  $global:__rayo_e0 = $global:Error[0]
  $e = [char]27; $b = [char]7
  "$e]133;D;$c$b$e[2J$e[3J$e[H$e]9;9;`"$($executionContext.SessionState.Path.CurrentLocation.ProviderPath)`"$b$e]133;B$b"
}
"#;

pub fn is_powershell(program: &str) -> bool {
    let stem = Path::new(program)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase());
    matches!(stem.as_deref(), Some("powershell" | "pwsh"))
}

/// Extra arguments that load the integration into PowerShell.
pub fn integration_args() -> Vec<String> {
    vec![
        "-NoExit".into(),
        "-EncodedCommand".into(),
        b64_utf16(PS_INTEGRATION),
    ]
}

fn b64_utf16(s: &str) -> String {
    let utf16: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// The single input line sent to PowerShell for a user command.
pub fn wrap_command(cmd: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(cmd.as_bytes());
    format!(
        "$global:LASTEXITCODE=0;$global:__rayo_e0=$global:Error[0];[Console]::Write([char]27+']133;C'+[char]7);. ([scriptblock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{b64}'))))\r"
    )
}

/// The `.git` directory of the repo containing `cwd`.
fn git_dir(cwd: &str) -> Option<PathBuf> {
    let mut dir = Some(Path::new(cwd));
    while let Some(d) = dir {
        let git = d.join(".git");
        if git.is_dir() {
            return Some(git);
        }
        if git.is_file() {
            // Worktrees / submodules: ".git" file pointing at the real dir.
            let s = std::fs::read_to_string(&git).ok()?;
            let p = PathBuf::from(s.trim().strip_prefix("gitdir:")?.trim());
            return Some(if p.is_absolute() { p } else { d.join(p) });
        }
        dir = d.parent();
    }
    None
}

/// Current git branch for `cwd`, read straight from `.git/HEAD`.
pub fn git_branch(cwd: &str) -> Option<String> {
    let s = std::fs::read_to_string(git_dir(cwd)?.join("HEAD")).ok()?;
    Some(match s.trim().strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_string(),
        None => s.trim().chars().take(7).collect(),
    })
}

/// Local branches of the repo containing `cwd`, sorted. Read from `refs/heads`
/// and `packed-refs` (no `git` process: no console flash, works offline).
pub fn git_branches(cwd: &str) -> Vec<String> {
    let Some(git) = git_dir(cwd) else { return vec![] };
    // Linked worktrees keep refs in the main repo ("commondir").
    let common = std::fs::read_to_string(git.join("commondir"))
        .map(|s| git.join(s.trim()))
        .unwrap_or(git);
    let mut out = vec![];
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = format!("{prefix}{}", e.file_name().to_string_lossy());
            if e.path().is_dir() {
                walk(&e.path(), &format!("{name}/"), out);
            } else {
                out.push(name);
            }
        }
    }
    walk(&common.join("refs").join("heads"), "", &mut out);
    let packed = std::fs::read_to_string(common.join("packed-refs")).unwrap_or_default();
    out.extend(
        packed
            .lines()
            .filter_map(|l| l.split_once(' ')?.1.strip_prefix("refs/heads/"))
            .map(String::from),
    );
    out.sort_by_key(|b| b.to_lowercase());
    out.dedup();
    out
}

/// Subfolders of `cwd`, visible ones first, each group sorted case-insensitively.
pub fn subdirs(cwd: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(cwd)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort_by_key(|n| (n.starts_with('.'), n.to_lowercase()));
    v
}

/// PowerShell single-quoted literal: `it's` → `'it''s'`.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// (start of last word, length of its directory part) — the word completion edits.
fn last_word(text: &str) -> (usize, usize) {
    let start = text
        .rfind([' ', '\t', '\n', '"', '\''])
        .map(|i| i + 1)
        .unwrap_or(0);
    let split = text[start..].rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    (start, split)
}

/// The file-name prefix being completed (what candidates are matched against).
pub fn completion_prefix(text: &str) -> &str {
    let (start, split) = last_word(text);
    &text[start + split..]
}

/// Replace the last word's file name with `candidate` (dirs end in `\`).
pub fn apply_completion(text: &str, candidate: &str) -> String {
    let (start, split) = last_word(text);
    let full = format!("{}{candidate}", &text[start..start + split]);
    let quoted = text[..start].ends_with(['"', '\'']);
    let word = if full.contains(' ') && !quoted {
        format!("'{full}'")
    } else {
        full
    };
    format!("{}{word}", &text[..start])
}

/// Files and folders matching the last word, relative to `cwd`. Folders end in `\`.
pub fn path_candidates(text: &str, cwd: &str) -> Vec<String> {
    let (start, split) = last_word(text);
    let dir_part = &text[start..start + split];
    let prefix = completion_prefix(text).to_lowercase();
    let base = if let Some(rest) = dir_part.strip_prefix('~') {
        dirs::home_dir()
            .unwrap_or_default()
            .join(rest.trim_start_matches(['\\', '/']))
    } else if Path::new(dir_part).is_absolute() || dir_part.starts_with(['\\', '/']) {
        PathBuf::from(dir_part)
    } else {
        Path::new(cwd).join(dir_part)
    };
    let Ok(rd) = std::fs::read_dir(base) else {
        return vec![];
    };
    let mut matches: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let dir = e.file_type().is_ok_and(|t| t.is_dir());
            name.to_lowercase()
                .starts_with(&prefix)
                .then(|| if dir { format!("{name}\\") } else { name })
        })
        .collect();
    // Folders first, like Warp, then alphabetical.
    matches.sort_by_key(|n| (!n.ends_with('\\'), n.to_lowercase()));
    matches.truncate(200);
    matches
}

/// Tab completion of the last word. Returns the new text and, when ambiguous,
/// the candidates (the text is extended to their common prefix).
pub fn complete_path(text: &str, cwd: &str) -> (String, Vec<String>) {
    // Word already names a folder (`..`, `src`, `D:`): just add the separator, like Warp.
    let (start, _) = last_word(text);
    let word = &text[start..];
    if !word.is_empty() && !word.ends_with(['\\', '/']) {
        let path = match word.strip_prefix('~') {
            Some(rest) => dirs::home_dir()
                .unwrap_or_default()
                .join(rest.trim_start_matches(['\\', '/'])),
            None => Path::new(cwd).join(word), // join keeps absolute paths as they are
        };
        if path.is_dir() {
            return (format!("{text}\\"), vec![]);
        }
    }
    let matches = path_candidates(text, cwd);
    match matches.len() {
        0 => (text.to_string(), vec![]),
        1 => (apply_completion(text, &matches[0]), vec![]),
        _ => {
            let prefix_len = completion_prefix(text).len();
            let first = matches[0].trim_end_matches('\\');
            let common = (prefix_len..=first.len())
                .rev()
                .find(|&n| {
                    first.is_char_boundary(n)
                        && matches
                            .iter()
                            .all(|m| m.to_lowercase().starts_with(&first[..n].to_lowercase()))
                })
                .unwrap_or(prefix_len);
            // Keep what the user typed if nothing longer is shared.
            let new = if common > prefix_len {
                apply_completion(text, &first[..common])
            } else {
                text.to_string()
            };
            (new, matches)
        }
    }
}

/// URL covering column `col` of a terminal row: (start col, end col exclusive, url).
pub fn url_at(text: &str, col: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let lower: String = text.to_lowercase();
    let lower: Vec<char> = lower.chars().collect();
    if lower.len() != chars.len() {
        return None;
    }
    let mut i = 0;
    while i < chars.len() {
        let rest: String = lower[i..].iter().take(8).collect();
        let is_start = ["https://", "http://", "file://", "www."]
            .iter()
            .any(|p| rest.starts_with(p))
            && (i == 0 || !chars[i - 1].is_alphanumeric());
        if !is_start {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < chars.len() && !chars[end].is_whitespace() && !"<>\"'`|{}^".contains(chars[end])
        {
            end += 1;
        }
        // Trailing punctuation belongs to the sentence, not the link.
        while end > i && ".,;:!?)]".contains(chars[end - 1]) {
            let c = chars[end - 1];
            let opener = if c == ')' { '(' } else { '[' };
            if matches!(c, ')' | ']') && chars[i..end].contains(&opener) {
                break;
            }
            end -= 1;
        }
        if (i..end).contains(&col) {
            let url: String = chars[i..end].iter().collect();
            let url = if url.to_lowercase().starts_with("www.") {
                format!("https://{url}")
            } else {
                url
            };
            return Some((i, end, url));
        }
        i = end.max(i + 1);
    }
    None
}

/// PowerShell aliases / built-ins that aren't files on PATH.
const BUILTINS: &[&str] = &[
    "cd", "ls", "dir", "cls", "clear", "echo", "cat", "type", "pwd", "mkdir", "md", "rm", "del",
    "rmdir", "rd", "cp", "copy", "mv", "move", "ren", "ni", "gci", "gc", "sc", "sl", "set", "where",
    "exit", "history", "kill", "ps", "start", "sleep", "tee", "sort", "select", "foreach", "if",
    "for", "while", "function", "return", "iex", "irm", "iwr", "curl", "wget",
];

/// Lowercase names (no extension) of everything runnable on PATH, built once.
fn path_commands() -> &'static std::collections::HashSet<String> {
    static SET: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        let exts = ["exe", "cmd", "bat", "com", "ps1"];
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let ext = p.extension()?.to_str()?.to_lowercase();
                exts.contains(&ext.as_str())
                    .then(|| p.file_stem()?.to_str().map(str::to_lowercase))?
            })
            .collect()
    })
}

/// Whether `word` runs something: a PATH program, a shell built-in or a Verb-Noun cmdlet.
pub fn is_command(word: &str) -> bool {
    let w = word.to_lowercase();
    let w = w
        .strip_suffix(".exe")
        .or_else(|| w.strip_suffix(".cmd"))
        .or_else(|| w.strip_suffix(".ps1"))
        .unwrap_or(&w);
    BUILTINS.contains(&w)
        || is_cmdlet(w)
        || path_commands().contains(w)
}

/// "get-childitem", "set-location"…: letters, one dash, letters.
fn is_cmdlet(w: &str) -> bool {
    w.split_once('-').is_some_and(|(v, n)| {
        !v.is_empty() && !n.is_empty() && (v.chars().chain(n.chars())).all(|c| c.is_ascii_alphabetic())
    })
}

/// Byte ranges of the command word of each command in `text` (start, after `|`, `;`, `&`,
/// a newline or a bracket) that `known` accepts. Quoted text is skipped.
pub fn command_spans(text: &str, known: impl Fn(&str) -> bool) -> Vec<std::ops::Range<usize>> {
    let mut out = vec![];
    let mut expect = true;
    let mut word: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut close = |word: &mut Option<usize>, end: usize, expect: &mut bool| {
        if let Some(s) = word.take() {
            if known(&text[s..end]) {
                out.push(s..end);
            }
            *expect = false;
        }
    };
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                close(&mut word, i, &mut expect);
                quote = Some(c);
                expect = false;
            }
            '|' | ';' | '&' | '\n' | '(' | '{' => {
                close(&mut word, i, &mut expect);
                expect = true;
            }
            c if c.is_whitespace() => close(&mut word, i, &mut expect),
            _ if word.is_none() && expect => word = Some(i),
            _ => {}
        }
    }
    close(&mut word, text.len(), &mut expect);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_spans_should_mark_first_word_of_each_command() {
        let known = |w: &str| ["claude", "git", "ls"].contains(&w);
        let t = "claude --resume | git log; ls \"git\"";
        let words: Vec<_> = command_spans(t, known).into_iter().map(|r| &t[r]).collect();
        assert_eq!(words, ["claude", "git", "ls"]);
    }

    #[test]
    fn command_spans_should_skip_unknown_and_arguments() {
        let known = |w: &str| w == "git";
        assert!(command_spans("foo git status", known).is_empty());
    }

    #[test]
    fn is_command_should_accept_builtins_and_cmdlets() {
        assert!(is_command("cd") && is_command("Get-ChildItem") && !is_command("--resume"));
    }

    #[test]
    fn branches_and_subdirs() {
        let root = std::env::temp_dir().join(format!("rayo-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let heads = root.join(".git/refs/heads/feat");
        std::fs::create_dir_all(&heads).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("Docs")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(root.join(".git/refs/heads/main"), "x").unwrap();
        std::fs::write(heads.join("login"), "x").unwrap();
        std::fs::write(
            root.join(".git/packed-refs"),
            "# pack-refs\nabc refs/heads/old\nabc refs/heads/main\nabc refs/tags/v1\n",
        )
        .unwrap();
        let cwd = root.join("src");
        let cwd = cwd.to_str().unwrap();
        assert_eq!(git_branch(cwd).as_deref(), Some("main"));
        assert_eq!(git_branches(cwd), ["feat/login", "main", "old"]);
        assert_eq!(subdirs(root.to_str().unwrap()), ["Docs", "src", ".git"]);
        assert_eq!(ps_quote("it's"), "'it''s'");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn completion_edits_last_word() {
        assert_eq!(completion_prefix("cd src\\pa"), "pa");
        assert_eq!(
            apply_completion("cd pdf", "pdfmusiccraft-web\\"),
            "cd pdfmusiccraft-web\\"
        );
        assert_eq!(
            apply_completion("cd D:\\dev\\pd", "pdfcraft\\"),
            "cd D:\\dev\\pdfcraft\\"
        );
        assert_eq!(
            apply_completion("cat my", "my file.txt"),
            "cat 'my file.txt'"
        );
        assert_eq!(
            apply_completion("cat \"my", "my file.txt"),
            "cat \"my file.txt"
        );
    }

    #[test]
    fn finds_urls() {
        let t = "see https://github.com/warpdotdev/warp. and (www.rust-lang.org) end";
        assert_eq!(
            url_at(t, 10).unwrap().2,
            "https://github.com/warpdotdev/warp"
        );
        assert_eq!(url_at(t, 1), None);
        assert_eq!(url_at(t, 45).unwrap().2, "https://www.rust-lang.org");
        assert_eq!(
            url_at("x https://en.wikipedia.org/wiki/Rust_(lenguaje)", 5)
                .unwrap()
                .2,
            "https://en.wikipedia.org/wiki/Rust_(lenguaje)"
        );
        let (s, e, _) = url_at("http://a.b", 0).unwrap();
        assert_eq!((s, e), (0, 10));
    }

    #[test]
    fn splits_marks_across_reads() {
        let mut s = Scanner::default();
        let mut segs = s.feed(b"out\x1b]13");
        segs.extend(s.feed(b"3;D;7\x07\x1b[2J\x1b]9;9;\"C:\\x\"\x07\x1b]133;B\x1b"));
        segs.extend(s.feed(b"\\\x1b]0;title\x07"));
        assert_eq!(
            segs,
            vec![
                Seg::Bytes(b"out".to_vec()),
                Seg::Mark(Mark::Done(7)),
                Seg::Bytes(b"\x1b[2J".to_vec()),
                Seg::Mark(Mark::Cwd("C:\\x".into())),
                Seg::Mark(Mark::Ready),
                Seg::Bytes(b"\x1b]0;title\x07".to_vec()),
            ]
        );
    }

    #[test]
    fn wrapped_command_roundtrips_base64() {
        let w = wrap_command("echo 'a b'\nls");
        let b64 = w.split('\'').nth(3).unwrap();
        let dec = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(dec, b"echo 'a b'\nls");
        assert!(w.ends_with('\r') && !w[..w.len() - 1].contains(['\r', '\n']));
    }

    #[test]
    fn path_completion() {
        let dir = std::env::temp_dir().join("rayo_complete_test");
        let _ = std::fs::create_dir_all(dir.join("src dir"));
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("Cargo.lock"), "").unwrap();
        let cwd = dir.to_str().unwrap();
        assert_eq!(
            complete_path("cat car", cwd),
            (
                "cat Cargo.".into(),
                vec!["Cargo.lock".into(), "Cargo.toml".into()]
            )
        );
        assert_eq!(complete_path("cd src", cwd).0, "cd 'src dir\\'");
        assert_eq!(complete_path("ls zz", cwd).0, "ls zz");
        // Already a folder: Tab only adds the separator.
        assert_eq!(complete_path("cd ..", cwd), ("cd ..\\".into(), vec![]));
        assert_eq!(complete_path("cd .", cwd).0, "cd .\\");
        assert_eq!(complete_path("cd ~", cwd).0, "cd ~\\");
        let _ = std::fs::create_dir_all(dir.join("build"));
        let _ = std::fs::create_dir_all(dir.join("build-old"));
        assert_eq!(complete_path("cd build", cwd).0, "cd build\\");
        // A file name is not a folder: normal completion.
        assert_eq!(complete_path("cat Cargo.toml", cwd).0, "cat Cargo.toml");
    }

    #[test]
    fn powershell_detection() {
        assert!(is_powershell("pwsh.exe"));
        assert!(is_powershell(
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
        ));
        assert!(!is_powershell("cmd.exe"));
    }
}
