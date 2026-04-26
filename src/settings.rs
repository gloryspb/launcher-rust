use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Release,
    Debug,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Release
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub mode: AppMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: AppMode::Release,
        }
    }
}

impl Settings {
    pub fn path(app_dir: &Path) -> PathBuf {
        app_dir.join("settings.json")
    }

    pub fn load_or_create(app_dir: &Path) -> Self {
        let path = Self::path(app_dir);
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }

        let s = Settings::default();
        let _ = s.save(app_dir);
        s
    }

    pub fn save(&self, app_dir: &Path) -> std::io::Result<()> {
        let path = Self::path(app_dir);
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).unwrap_or_else(|_| b"{\"mode\":\"release\"}".to_vec());
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

