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
    let url = v["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case("rayo.exe")))?
        ["browser_download_url"]
        .as_str()?
        .to_string();
    Some(Release { version, url })
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
    // A truncated/HTML error page would brick the install: require a PE header.
    let valid = ok && std::fs::read(&new).is_ok_and(|b| b.len() > 100_000 && b.starts_with(b"MZ"));
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
}
