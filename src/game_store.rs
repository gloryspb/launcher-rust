//! Persistence and game list operations (Python `GameManager`).

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::core::is_uri;
use crate::drop_resolve;

const ICON_EXTENSIONS: &[&str] = &[".png", ".gif", ".ppm", ".pgm", ".ico"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub launch_args: String,
}

#[derive(Debug, Error)]
pub enum GameStoreError {
    #[error("empty game path")]
    EmptyPath,
    #[error("game file not found: {0}")]
    FileNotFound(String),
    #[error("game not found")]
    NotFound,
    #[error("empty launch path")]
    EmptyLaunchPath,
    #[error("shortcut error: {0}")]
    Shortcut(String),
    #[error("unable to open uri: {0}")]
    UriLaunchFailed(String),
    #[cfg(not(windows))]
    #[error("uri launch is only implemented on Windows")]
    UriLaunchUnsupported,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("image: {0}")]
    Image(String),
}

struct GameDraft {
    name: String,
    path: String,
    icon_source: String,
    working_dir: String,
    launch_args: String,
}

struct PreparedIconChange {
    next_icon: String,
    new_icon_to_cleanup: Option<String>,
    old_icon_to_delete: Option<String>,
}

pub struct GameStore {
    pub config_path: PathBuf,
    pub icons_dir: PathBuf,
    pub games: Vec<Game>,
    startup_warning: Option<String>,
}

#[allow(dead_code)] // public surface for future UI / tests
impl GameStore {
    pub fn new(app_dir: impl AsRef<Path>) -> Self {
        let app_dir = app_dir.as_ref();
        let config_path = app_dir.join("games.json");
        let icons_dir = app_dir.join("icons");

        let mut warnings = Vec::new();
        if let Err(error) = fs::create_dir_all(&icons_dir) {
            warnings.push(format!("Не удалось создать каталог иконок: {error}"));
        }

        let mut store = Self {
            config_path,
            icons_dir,
            games: Vec::new(),
            startup_warning: None,
        };

        if let Some(warning) = store.load() {
            warnings.push(warning);
        }

        if !warnings.is_empty() {
            store.startup_warning = Some(warnings.join("\n\n"));
        }

        store
    }

    pub fn take_startup_warning(&mut self) -> Option<String> {
        self.startup_warning.take()
    }

    fn load(&mut self) -> Option<String> {
        if !self.config_path.exists() {
            self.games.clear();
            return None;
        }

        match fs::read_to_string(&self.config_path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(list) => {
                    self.games = list;
                    None
                }
                Err(error) => {
                    self.games.clear();
                    self.backup_invalid_config(&error.to_string())
                }
            },
            Err(error) => {
                self.games.clear();
                self.backup_invalid_config(&error.to_string())
            }
        }
    }

    fn backup_invalid_config(&self, reason: &str) -> Option<String> {
        let backup_path = self.config_path.with_extension("json.bak");
        match fs::copy(&self.config_path, &backup_path) {
            Ok(_) => Some(format!(
                "games.json поврежден и сохранен в {}: {reason}",
                backup_path.display()
            )),
            Err(backup_error) => Some(format!(
                "games.json поврежден ({reason}), и резервную копию создать не удалось: {backup_error}"
            )),
        }
    }

    fn save(&self) -> Result<(), GameStoreError> {
        let tmp = self.config_path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.games)?;
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &self.config_path)?;
        Ok(())
    }

    fn copy_icon(&self, icon_source: &str, game_id: &str) -> Result<String, GameStoreError> {
        let source = PathBuf::from(icon_source.trim());
        if source.as_os_str().is_empty() || !source.exists() {
            return Ok(String::new());
        }

        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !ICON_EXTENSIONS
            .iter()
            .any(|allowed| *allowed == format!(".{ext}"))
        {
            return Ok(String::new());
        }

        let unique_suffix = Uuid::new_v4().simple().to_string();
        if ext == "ico" {
            let target = self
                .icons_dir
                .join(format!("{game_id}-{unique_suffix}.png"));
            let img = image::open(&source).map_err(|e| GameStoreError::Image(e.to_string()))?;
            let rgba = img.to_rgba8();
            let mut out = Vec::new();
            PngEncoder::new(&mut out)
                .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ColorType::Rgba8)
                .map_err(|e| GameStoreError::Image(e.to_string()))?;
            fs::write(&target, out)?;
            return Ok(target.to_string_lossy().to_string());
        }

        let target = self
            .icons_dir
            .join(format!("{game_id}-{unique_suffix}.{ext}"));
        fs::copy(&source, &target)?;
        Ok(target.to_string_lossy().to_string())
    }

    fn remove_icon_file(&self, icon_path: &str) {
        let trimmed = icon_path.trim();
        if trimmed.is_empty() {
            return;
        }

        let path = PathBuf::from(trimmed);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    fn prepare_icon_change(
        &self,
        current_icon: &str,
        icon_source: &str,
        game_id: &str,
    ) -> Result<PreparedIconChange, GameStoreError> {
        let current_icon = current_icon.trim();
        let icon_source = icon_source.trim();

        if icon_source.is_empty() {
            return Ok(PreparedIconChange {
                next_icon: String::new(),
                new_icon_to_cleanup: None,
                old_icon_to_delete: (!current_icon.is_empty()).then(|| current_icon.to_string()),
            });
        }

        if icon_source == current_icon {
            return Ok(PreparedIconChange {
                next_icon: current_icon.to_string(),
                new_icon_to_cleanup: None,
                old_icon_to_delete: None,
            });
        }

        let next_icon = self.copy_icon(icon_source, game_id)?;
        Ok(PreparedIconChange {
            next_icon: next_icon.clone(),
            new_icon_to_cleanup: Some(next_icon),
            old_icon_to_delete: (!current_icon.is_empty()).then(|| current_icon.to_string()),
        })
    }

    fn finalize_icon_change(&self, change: &PreparedIconChange) {
        if let Some(old_icon) = &change.old_icon_to_delete {
            self.remove_icon_file(old_icon);
        }
    }

    fn rollback_icon_change(&self, change: &PreparedIconChange) {
        if let Some(new_icon) = &change.new_icon_to_cleanup {
            self.remove_icon_file(new_icon);
        }
    }

    fn resolve_game_input(
        &self,
        name: String,
        path: String,
        icon_source: String,
    ) -> Result<GameDraft, GameStoreError> {
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(GameStoreError::EmptyPath);
        }

        if is_uri(&path) {
            return Ok(GameDraft {
                name: choose_name(&name, "Link"),
                path,
                icon_source,
                working_dir: String::new(),
                launch_args: String::new(),
            });
        }

        let raw_path = PathBuf::from(&path);
        let lower = raw_path.to_string_lossy().to_ascii_lowercase();

        if lower.ends_with(".lnk") || lower.ends_with(".url") {
            let resolved =
                drop_resolve::resolve_drop_path(&raw_path).map_err(GameStoreError::Shortcut)?;
            if !is_uri(&resolved.target_path) && !Path::new(&resolved.target_path).exists() {
                return Err(GameStoreError::FileNotFound(resolved.target_path));
            }

            let resolved_icon = if icon_source.trim().is_empty() {
                resolved.icon_source
            } else {
                icon_source
            };

            return Ok(GameDraft {
                name: choose_name(&name, &resolved.name),
                path: resolved.target_path,
                icon_source: resolved_icon,
                working_dir: resolved.working_dir,
                launch_args: resolved.launch_args,
            });
        }

        if !raw_path.exists() {
            return Err(GameStoreError::FileNotFound(path));
        }

        let inferred_name = raw_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Game");

        Ok(GameDraft {
            name: choose_name(&name, inferred_name),
            path: raw_path.to_string_lossy().to_string(),
            icon_source,
            working_dir: raw_path
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
                .unwrap_or_default(),
            launch_args: String::new(),
        })
    }

    pub fn add(
        &mut self,
        name: impl Into<String>,
        path: impl Into<String>,
        icon_source: impl Into<String>,
    ) -> Result<(), GameStoreError> {
        let draft = self.resolve_game_input(name.into(), path.into(), icon_source.into())?;
        let game_id = Uuid::new_v4().simple().to_string();
        let icon = self.copy_icon(&draft.icon_source, &game_id)?;

        self.games.push(Game {
            id: game_id,
            name: draft.name,
            path: draft.path,
            icon: icon.clone(),
            working_dir: draft.working_dir,
            launch_args: draft.launch_args,
        });

        if let Err(error) = self.save() {
            let _ = self.games.pop();
            self.remove_icon_file(&icon);
            return Err(error);
        }

        Ok(())
    }

    pub fn remove(&mut self, game_id: &str) -> Result<(), GameStoreError> {
        let pos = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let game = self.games.remove(pos);
        self.save()?;
        self.remove_icon_file(&game.icon);
        Ok(())
    }

    pub fn get(&self, game_id: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.id == game_id)
    }

    pub fn launch(&self, game_id: &str) -> Result<(), GameStoreError> {
        let game = self.get(game_id).ok_or(GameStoreError::NotFound)?;
        let path = game.path.trim();
        if path.is_empty() {
            return Err(GameStoreError::EmptyLaunchPath);
        }
        if is_uri(path) {
            open_uri(path)?;
            return Ok(());
        }
        if !Path::new(path).exists() {
            return Err(GameStoreError::FileNotFound(path.to_string()));
        }

        let mut command = std::process::Command::new(path);
        if let Some(working_dir) = resolve_working_dir(game, path) {
            command.current_dir(working_dir);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            if !game.launch_args.trim().is_empty() {
                command.raw_arg(game.launch_args.trim());
            }
        }

        #[cfg(not(windows))]
        {
            if !game.launch_args.trim().is_empty() {
                command.args(game.launch_args.split_whitespace());
            }
        }

        command.spawn().map_err(GameStoreError::Io)?;
        Ok(())
    }

    pub fn update_icon(&mut self, game_id: &str, icon_source: &str) -> Result<(), GameStoreError> {
        let idx = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let old_icon = self.games[idx].icon.clone();
        let change = self.prepare_icon_change(&old_icon, icon_source, game_id)?;
        self.games[idx].icon = change.next_icon.clone();

        if let Err(error) = self.save() {
            self.games[idx].icon = old_icon;
            self.rollback_icon_change(&change);
            return Err(error);
        }

        self.finalize_icon_change(&change);
        Ok(())
    }

    pub fn update_game_meta(
        &mut self,
        game_id: &str,
        name: impl Into<String>,
        icon_source: &str,
    ) -> Result<(), GameStoreError> {
        let idx = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let old_name = self.games[idx].name.clone();
        let old_icon = self.games[idx].icon.clone();
        let change = self.prepare_icon_change(&old_icon, icon_source, game_id)?;
        self.games[idx].name = name.into();
        self.games[idx].icon = change.next_icon.clone();

        if let Err(error) = self.save() {
            self.games[idx].name = old_name;
            self.games[idx].icon = old_icon;
            self.rollback_icon_change(&change);
            return Err(error);
        }

        self.finalize_icon_change(&change);
        Ok(())
    }

    pub fn reorder(&mut self, ordered_ids: &[String]) -> Result<(), GameStoreError> {
        let order_index: std::collections::HashMap<&str, usize> = ordered_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let fallback = ordered_ids.len();
        self.games
            .sort_by_key(|g| *order_index.get(g.id.as_str()).unwrap_or(&fallback));
        self.save()?;
        Ok(())
    }

    pub fn move_game(&mut self, game_id: &str, delta: isize) -> Result<(), GameStoreError> {
        let idx = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let new_idx = idx as isize + delta;
        if new_idx < 0 || new_idx >= self.games.len() as isize {
            return Ok(());
        }
        let new_idx = new_idx as usize;
        let item = self.games.remove(idx);
        self.games.insert(new_idx, item);
        self.save()?;
        Ok(())
    }

    pub fn move_game_to(&mut self, game_id: &str, new_index: usize) -> Result<(), GameStoreError> {
        let idx = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        if idx == new_index || self.games.is_empty() {
            return Ok(());
        }

        let max_index = self.games.len().saturating_sub(1);
        let new_index = new_index.min(max_index);
        let item = self.games.remove(idx);
        self.games.insert(new_index, item);
        self.save()?;
        Ok(())
    }

    pub fn path_exists_for_display(path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() {
            return false;
        }
        if is_uri(path) {
            return true;
        }
        Path::new(path).exists()
    }
}

fn choose_name(name: &str, fallback: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_working_dir(game: &Game, path: &str) -> Option<PathBuf> {
    let working_dir = game.working_dir.trim();
    if !working_dir.is_empty() {
        let working_dir = PathBuf::from(working_dir);
        if working_dir.exists() {
            return Some(working_dir);
        }
    }

    Path::new(path).parent().map(Path::to_path_buf)
}

fn open_uri(uri: &str) -> Result<(), GameStoreError> {
    #[cfg(windows)]
    {
        use std::iter;
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::UI::Shell::ShellExecuteW;

        const SW_SHOWNORMAL: i32 = 1;

        let operation: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let target: Vec<u16> = OsStr::new(uri).encode_wide().chain(iter::once(0)).collect();

        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };

        if (result as usize) <= 32 {
            return Err(GameStoreError::UriLaunchFailed(uri.to_string()));
        }

        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        Err(GameStoreError::UriLaunchUnsupported)
    }
}
