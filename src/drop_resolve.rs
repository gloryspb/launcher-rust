//! Resolve dropped files or typed paths into (display name, launch target, optional icon path).

use std::path::{Path, PathBuf};

use lnk::ShellLink;

use crate::core::{is_steam_uri, is_uri};

#[derive(Debug, Clone)]
pub struct ResolvedDrop {
    pub name: String,
    pub target_path: String,
    pub icon_source: String,
    pub working_dir: String,
    pub launch_args: String,
}

pub fn resolve_drop_path(dropped: &Path) -> Result<ResolvedDrop, String> {
    let path_str = dropped.to_string_lossy().to_string();
    let lower = path_str.to_lowercase();

    if is_uri(&path_str) {
        let tail = path_str.rsplit('/').next().unwrap_or("link");
        let prefix = if is_steam_uri(&path_str) {
            "Steam"
        } else {
            "Link"
        };
        return Ok(ResolvedDrop {
            name: format!("{prefix} {tail}"),
            target_path: path_str,
            icon_source: String::new(),
            working_dir: String::new(),
            launch_args: String::new(),
        });
    }

    if lower.ends_with(".lnk") {
        return resolve_lnk(dropped);
    }
    if lower.ends_with(".url") {
        return resolve_url_shortcut(dropped);
    }
    if lower.ends_with(".exe") {
        let name = dropped
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Game")
            .to_string();
        return Ok(ResolvedDrop {
            name,
            target_path: path_str,
            icon_source: String::new(),
            working_dir: String::new(),
            launch_args: String::new(),
        });
    }

    Err("unsupported file type".into())
}

fn resolve_lnk(path: &Path) -> Result<ResolvedDrop, String> {
    let link = ShellLink::open(path, lnk::encoding::WINDOWS_1252).map_err(|e| e.to_string())?;
    let target = link
        .link_target()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "shortcut has no resolved target".to_string())?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Shortcut")
        .to_string();

    let icon_source = link
        .string_data()
        .icon_location()
        .as_ref()
        .map(|s| {
            s.split(',')
                .next()
                .unwrap_or(s)
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_default();
    let working_dir = link
        .string_data()
        .working_dir()
        .as_ref()
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let launch_args = link
        .string_data()
        .command_line_arguments()
        .as_ref()
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    Ok(ResolvedDrop {
        name,
        target_path: target,
        icon_source,
        working_dir,
        launch_args,
    })
}

fn resolve_url_shortcut(path: &Path) -> Result<ResolvedDrop, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut url = String::new();
    let mut icon_source = String::new();

    for line in raw.lines() {
        let line_strip = line.trim();
        let lower = line_strip.to_ascii_lowercase();
        if lower.starts_with("url=") {
            url = line
                .split_once('=')
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_default();
        } else if lower.starts_with("iconfile=") {
            icon_source = line
                .split_once('=')
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
                .unwrap_or_default();
        }
    }

    if url.is_empty() {
        return Err(format!("no URL= in {}", path.display()));
    }

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Link")
        .to_string();

    Ok(ResolvedDrop {
        name,
        target_path: url,
        icon_source,
        working_dir: String::new(),
        launch_args: String::new(),
    })
}

pub fn normalize_icon_path_for_preview(icon_path: &str, config_parent: &Path) -> PathBuf {
    let p = PathBuf::from(icon_path.trim());
    if p.is_absolute() {
        p
    } else {
        config_parent.join(p)
    }
}
