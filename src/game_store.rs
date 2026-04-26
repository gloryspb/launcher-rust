//! Persistence and game list operations (Python `GameManager`).

use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::core::is_steam_uri;

const ICON_EXTENSIONS: &[&str] = &[".png", ".gif", ".ppm", ".pgm", ".ico"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub icon: String,
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

pub struct GameStore {
    pub config_path: PathBuf,
    pub icons_dir: PathBuf,
    pub games: Vec<Game>,
}

#[allow(dead_code)] // public surface for future UI / tests
impl GameStore {
    pub fn new(app_dir: impl AsRef<Path>) -> Self {
        let app_dir = app_dir.as_ref();
        let config_path = app_dir.join("games.json");
        let icons_dir = app_dir.join("icons");
        let _ = fs::create_dir_all(&icons_dir);

        let mut s = Self {
            config_path,
            icons_dir,
            games: Vec::new(),
        };
        s.load();
        s
    }

    fn load(&mut self) {
        if !self.config_path.exists() {
            self.games.clear();
            return;
        }
        match fs::read_to_string(&self.config_path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(list) => self.games = list,
                Err(_) => self.games.clear(),
            },
            Err(_) => self.games.clear(),
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
        if !ICON_EXTENSIONS.iter().any(|allowed| *allowed == format!(".{ext}")) {
            return Ok(String::new());
        }

        if ext == "ico" {
            let target = self.icons_dir.join(format!("{game_id}.png"));
            let img = image::open(&source).map_err(|e| GameStoreError::Image(e.to_string()))?;
            let rgba = img.to_rgba8();
            let mut out = Vec::new();
            PngEncoder::new(&mut out)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| GameStoreError::Image(e.to_string()))?;
            fs::write(&target, out)?;
            return Ok(target.to_string_lossy().to_string());
        }

        let target = self.icons_dir.join(format!("{game_id}.{ext}"));
        fs::copy(&source, &target)?;
        Ok(target.to_string_lossy().to_string())
    }

    pub fn add(
        &mut self,
        name: impl Into<String>,
        path: impl Into<String>,
        icon_source: impl Into<String>,
    ) -> Result<(), GameStoreError> {
        let path = path.into().trim().to_string();
        if path.is_empty() {
            return Err(GameStoreError::EmptyPath);
        }

        if is_steam_uri(&path) {
            let game_id = Uuid::new_v4().simple().to_string();
            let icon = self.copy_icon(&icon_source.into(), &game_id)?;
            self.games.push(Game {
                id: game_id,
                name: name.into(),
                path,
                icon,
            });
            self.save()?;
            return Ok(());
        }

        let exe = PathBuf::from(&path);
        if !exe.exists() {
            return Err(GameStoreError::FileNotFound(path));
        }

        let game_id = Uuid::new_v4().simple().to_string();
        let icon = self.copy_icon(&icon_source.into(), &game_id)?;
        self.games.push(Game {
            id: game_id,
            name: name.into(),
            path: exe.to_string_lossy().to_string(),
            icon,
        });
        self.save()?;
        Ok(())
    }

    pub fn remove(&mut self, game_id: &str) -> Result<(), GameStoreError> {
        let pos = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let game = self.games.remove(pos);
        let icon = game.icon.trim();
        if !icon.is_empty() {
            let p = PathBuf::from(icon);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
        self.save()?;
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
        if is_steam_uri(path) {
            open_uri(path)?;
            return Ok(());
        }
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| GameStoreError::Io(e))?;
        Ok(())
    }

    pub fn update_icon(&mut self, game_id: &str, icon_source: &str) -> Result<(), GameStoreError> {
        let idx = self
            .games
            .iter()
            .position(|g| g.id == game_id)
            .ok_or(GameStoreError::NotFound)?;
        let old = self.games[idx].icon.trim().to_string();
        if !old.is_empty() {
            let p = PathBuf::from(&old);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
        let new_icon = self.copy_icon(icon_source, game_id)?;
        self.games[idx].icon = new_icon;
        self.save()?;
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
        self.games[idx].name = name.into();
        let old = self.games[idx].icon.trim().to_string();
        if !old.is_empty() {
            let p = PathBuf::from(&old);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
        let new_icon = self.copy_icon(icon_source, game_id)?;
        self.games[idx].icon = new_icon;
        self.save()?;
        Ok(())
    }

    pub fn reorder(&mut self, ordered_ids: &[String]) -> Result<(), GameStoreError> {
        let order_index: std::collections::HashMap<&str, usize> = ordered_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let fallback = ordered_ids.len();
        self.games.sort_by_key(|g| {
            *order_index
                .get(g.id.as_str())
                .unwrap_or(&fallback)
        });
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

    pub fn path_exists_for_display(path: &str) -> bool {
        let p = path.trim();
        if p.is_empty() {
            return false;
        }
        if is_steam_uri(p) {
            return true;
        }
        Path::new(p).exists()
    }
}

fn open_uri(uri: &str) -> Result<(), GameStoreError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", uri])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(GameStoreError::Io)?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        Err(GameStoreError::UriLaunchUnsupported)
    }
}
