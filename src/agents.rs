//! AI agents running in panes, and Claude Code token usage from its local logs.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Agent name if `s` (a command line or window title) looks like one.
pub fn detect(s: &str) -> Option<&'static str> {
    let s = s.to_lowercase();
    // Title set by Claude Code ("✳ Claude Code"), or the command typed.
    if s.contains("claude") {
        return Some("Claude Code");
    }
    let first = s.split_whitespace().next()?;
    let first = first.rsplit(['\\', '/']).next()?.trim_end_matches(".exe").trim_end_matches(".cmd");
    Some(match first {
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "aider" => "Aider",
        "opencode" => "OpenCode",
        "copilot" => "Copilot CLI",
        "cursor-agent" => "Cursor Agent",
        "amp" => "Amp",
        "goose" => "Goose",
        _ => return None,
    })
}

#[derive(Default, Clone, Copy)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

#[derive(Default, Clone)]
pub struct Usage {
    pub last_5h: Tokens,
    pub last_7d: Tokens,
    pub messages_5h: usize,
    /// First message of the current 5 h window (UTC "YYYY-MM-DDTHH:MM:SS").
    pub session_start: Option<String>,
    /// Tokens per model over 7 days, biggest first.
    pub by_model: Vec<(String, u64)>,
}

impl Usage {
    /// Time until the 5 h window ends.
    /// ponytail: window = earliest message in the last 5 h; Anthropic's exact window isn't in the logs.
    pub fn resets_in(&self, now: SystemTime) -> Option<Duration> {
        let start = parse_iso(self.session_start.as_deref()?)?;
        let now = now.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
        u64::try_from(start + 5 * 3600 - now).ok().map(Duration::from_secs)
    }
}

/// Sum Claude Code usage over the last 5 h / 7 days. Slow (reads logs): call off the UI thread.
/// Local logs only: plan limits (%) aren't there, so none are shown.
pub fn claude_usage() -> Usage {
    let mut u = Usage::default();
    let Some(root) = dirs::home_dir().map(|h| h.join(".claude").join("projects")) else {
        return u;
    };
    let now = SystemTime::now();
    let week = now - Duration::from_secs(7 * 86400);
    let (cut_5h, cut_7d) = (iso(now - Duration::from_secs(5 * 3600)), iso(week));
    let mut seen = std::collections::HashSet::new();
    let mut models = std::collections::HashMap::<String, u64>::new();
    let files = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten());
    for f in files {
        let fresh = f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t >= week);
        if !fresh || f.path().extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(f.path()) else { continue };
        for line in text.lines().filter(|l| l.contains("\"usage\"")) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let ts = v["timestamp"].as_str().unwrap_or_default();
            if ts < cut_7d.as_str() {
                continue;
            }
            // Streamed messages repeat across lines; count each message id once.
            if let Some(id) = v["message"]["id"].as_str()
                && !seen.insert(id.to_string()) {
                    continue;
                }
            let us = &v["message"]["usage"];
            let n = |k: &str| us[k].as_u64().unwrap_or(0);
            let t = Tokens {
                input: n("input_tokens"),
                output: n("output_tokens"),
                cache_read: n("cache_read_input_tokens"),
                cache_write: n("cache_creation_input_tokens"),
            };
            add(&mut u.last_7d, t);
            let model = v["message"]["model"].as_str().unwrap_or("?");
            if model != "<synthetic>" {
                *models.entry(model.trim_start_matches("claude-").to_string()).or_default() += t.total();
            }
            if ts >= cut_5h.as_str() {
                add(&mut u.last_5h, t);
                u.messages_5h += 1;
                if u.session_start.as_deref().is_none_or(|s| ts < s) {
                    u.session_start = Some(ts.to_string());
                }
            }
        }
    }
    u.by_model = models.into_iter().filter(|m| m.1 > 0).collect();
    u.by_model.sort_by_key(|m| std::cmp::Reverse(m.1));
    u
}

/// "YYYY-MM-DDTHH:MM:SS…" (UTC) → Unix seconds. Inverse of `iso`.
fn parse_iso(s: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let secs = n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    // Days-from-civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + secs)
}

fn add(a: &mut Tokens, b: Tokens) {
    a.input += b.input;
    a.output += b.output;
    a.cache_read += b.cache_read;
    a.cache_write += b.cache_write;
}

/// UTC "YYYY-MM-DDTHH:MM:SS" — compares lexicographically with log timestamps.
fn iso(t: SystemTime) -> String {
    let s = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// 1234567 → "1.2M".
pub fn short(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_and_detect() {
        assert_eq!(iso(UNIX_EPOCH + Duration::from_secs(1_763_571_159)), "2025-11-19T16:52:39");
        assert_eq!(iso(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00");
        assert_eq!(detect("claude --resume"), Some("Claude Code"));
        assert_eq!(detect("✳ Claude Code"), Some("Claude Code"));
        assert_eq!(detect("C:\\bin\\codex.exe exec"), Some("Codex"));
        assert_eq!(detect("cargo build"), None);
        assert_eq!(short(1_500), "1.5k");
    }

    #[test]
    fn parse_iso_should_invert_iso() {
        for s in [0, 951_782_400, 1_763_571_159] {
            assert_eq!(parse_iso(&iso(UNIX_EPOCH + Duration::from_secs(s))), Some(s as i64));
        }
    }

    #[test]
    fn resets_in_should_count_from_window_start() {
        let u = Usage { session_start: Some(iso(UNIX_EPOCH + Duration::from_secs(1_000_000))), ..Default::default() };
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000 + 3600);
        assert_eq!(u.resets_in(now), Some(Duration::from_secs(4 * 3600)));
    }
}
