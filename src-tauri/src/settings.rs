//! App settings persisted as JSON (`data_root/settings.json`).
//! Library working DB lives next to the music library, not here.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Absolute path of the library root (music + axmusic.db live here).
    pub library_root: Option<String>,
    /// 0.0 ..= 1.0
    pub volume: f32,
    /// Scrape wizard default: embed cover on apply
    pub scrape_write_cover: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            library_root: None,
            volume: 0.8,
            scrape_write_cover: true,
        }
    }
}

pub fn settings_path() -> PathBuf {
    crate::paths::data_root().join("settings.json")
}

pub fn load() -> AppSettings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save(settings: &AppSettings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, text).with_context(|| format!("写设置失败 {}", path.display()))?;
    Ok(())
}
