use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    #[default]
    Release,
    Debug,
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

    pub fn load_or_create(app_dir: &Path) -> (Self, Option<String>) {
        let path = Self::path(app_dir);
        if let Ok(text) = fs::read_to_string(&path) {
            match serde_json::from_str::<Settings>(&text) {
                Ok(settings) => return (settings, None),
                Err(error) => {
                    let warning = backup_invalid_settings(&path, &error.to_string());
                    let settings = Settings::default();
                    let save_warning = settings.save(app_dir).err().map(|save_error| {
                        format!("Не удалось записать новый settings.json: {save_error}")
                    });
                    return (settings, combine_warnings(warning, save_warning));
                }
            }
        }

        let settings = Settings::default();
        let warning = settings
            .save(app_dir)
            .err()
            .map(|error| format!("Не удалось создать settings.json: {error}"));
        (settings, warning)
    }

    pub fn save(&self, app_dir: &Path) -> io::Result<()> {
        let path = Self::path(app_dir);
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn backup_invalid_settings(path: &Path, reason: &str) -> Option<String> {
    let backup_path = path.with_extension("json.bak");
    match fs::copy(path, &backup_path) {
        Ok(_) => Some(format!(
            "settings.json поврежден и сохранен в {}: {reason}",
            backup_path.display()
        )),
        Err(backup_error) => Some(format!(
            "settings.json поврежден ({reason}), и резервную копию создать не удалось: {backup_error}"
        )),
    }
}

fn combine_warnings(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}\n{second}")),
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (None, None) => None,
    }
}
