//! Portable app directory and small predicates shared with the Python launcher.

use std::path::{Path, PathBuf};

/// Directory next to the running executable (same idea as Python `get_app_dir` when "frozen").
pub fn app_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

pub fn is_uri(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }

    let lower = value.to_ascii_lowercase();
    lower.contains("://") || lower.starts_with("mailto:")
}

pub fn is_steam_uri(value: &str) -> bool {
    value.trim().to_ascii_lowercase().starts_with("steam://")
}

/// Mirrors current Python `is_done()` (assignment gate always open).
/// Change this to `app_dir.join("done.txt").exists()` if you re-enable the file gate.
pub fn is_gate_open(_app_dir: &Path) -> bool {
    true
}
