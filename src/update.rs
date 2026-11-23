//! Self-update from GitHub Releases. Uses Windows' bundled `curl.exe` (no HTTP crate).
//!
//! A release is a tag `vX.Y.Z` with a `rayo.exe` asset (see .github/workflows/release.yml).
//! Install: download next to the running exe, rename the running exe to `.old`
//! (Windows allows renaming a running exe, not overwriting it), move the new one in.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub const REPO: &str = "PabloGonSan8/rayo-terminal";
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    Available(Release),
    Installing(String),
    Installed(PathBuf),
}

#[derive(Clone)]
pub struct Release {
    pub version: String,
    url: String,
    /// `rayo.exe.sha256` asset published next to the exe by the release workflow.
    sha_url: String,
}

/// Newer release than this build, if any. Blocking: call off the UI thread.
pub fn check() -> Option<Release> {
    let out = Command::new("curl.exe")
        .args(["-fsL", "-m", "15", "-H", "User-Agent: rayo"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .creation_flags(NO_WINDOW)
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let version = v["tag_name"].as_str()?.trim_start_matches('v').to_string();
    if !newer(&version, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let assets = v["assets"].as_array()?;
    let asset = |name: &str| -> Option<String> {
        let a = assets
            .iter()
            .find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
        Some(a["browser_download_url"].as_str()?.to_string())
    };
    // No checksum, no update: an exe we can't verify is never installed.
    Some(Release { version, url: asset("rayo.exe")?, sha_url: asset("rayo.exe.sha256")? })
}

/// Download and swap the exe in place. The caller restarts.
pub fn install(r: &Release) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = exe.with_extension("exe.new");
    let old = exe.with_extension("exe.old");
    let ok = Command::new("curl.exe")
        .args(["-fsL", "-m", "300", "-o"])
        .arg(&new)
        .arg(&r.url)
        .creation_flags(NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success());
    // A truncated/HTML error page would brick the install: require a PE header, and
    // the SHA-256 published with the release (catches corrupt or swapped downloads).
    let valid = ok
        && std::fs::read(&new).is_ok_and(|b| b.len() > 100_000 && b.starts_with(b"MZ"))
        && expected_sha(&r.sha_url).is_some_and(|want| file_sha(&new).is_some_and(|got| got == want));
    if !valid {
        let _ = std::fs::remove_file(&new);
        return Err(crate::i18n::t("No se pudo descargar la actualización").into());
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe); // put the working one back
        return Err(e.to_string());
    }
    Ok(exe)
}

/// Remove the previous exe left by an update (it was still running then).
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
    }
}

/// SHA-256 from the release's `.sha256` asset (`<hex>  rayo.exe`).
fn expected_sha(url: &str) -> Option<String> {
    let out = Command::new("curl.exe")
        .args(["-fsL", "-m", "30", url])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    parse_sha256(&String::from_utf8_lossy(&out.stdout))
}

/// SHA-256 of a file with Windows' bundled `certutil` (no hashing crate).
fn file_sha(path: &std::path::Path) -> Option<String> {
    let out = Command::new("certutil.exe")
        .arg("-hashfile")
        .arg(path)
        .arg("SHA256")
        .creation_flags(NO_WINDOW)
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    parse_sha256(&String::from_utf8_lossy(&out.stdout))
}

/// First SHA-256 (64 hex digits, lowercased) in `text`: a `sha256sum` line, or certutil's
/// output (whose hash line older Windows versions split into space-separated bytes).
fn parse_sha256(text: &str) -> Option<String> {
    let is_hash = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    text.lines().find_map(|line| {
        let joined: String = line.split_whitespace().collect();
        line.split_whitespace()
            .find(|w| is_hash(w))
            .map(str::to_string)
            .or_else(|| is_hash(&joined).then_some(joined))
            .map(|h| h.to_ascii_lowercase())
    })
}

/// "0.10.0" > "0.9.3"; non-numeric parts count as 0.
fn newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_order() {
        assert!(super::newer("0.10.0", "0.9.3"));
        assert!(super::newer("1.0.0", "0.99.99"));
        assert!(!super::newer("0.1.0", "0.1.0"));
        assert!(!super::newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn sha256_from_sum_file_and_certutil() {
        let h = "ab".repeat(32);
        assert_eq!(super::parse_sha256(&format!("{}  rayo.exe
", h.to_uppercase())), Some(h.clone()));
        let spaced: Vec<&str> = (0..32).map(|_| "ab").collect();
        let certutil = format!("SHA256 hash of x:
{}
CertUtil: done.
", spaced.join(" "));
        assert_eq!(super::parse_sha256(&certutil), Some(h));
        assert_eq!(super::parse_sha256("404: Not Found"), None);
    }

    #[test]
    fn certutil_hash_matches_known_value() {
        // sha256("abc"), checks the real certutil call and its output parsing.
        let p = std::env::temp_dir().join(format!("rayo-sha-{}", std::process::id()));
        std::fs::write(&p, "abc").unwrap();
        let got = super::file_sha(&p);
        std::fs::remove_file(&p).unwrap();
        assert_eq!(got.as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    }
}
