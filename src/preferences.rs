//! Small, version-tolerant desktop preferences; no workbook contents are saved.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub output: PathBuf,
    pub color: String,
    pub keep_format: bool,
    pub comments: bool,
    pub recent: Vec<PathBuf>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            output: crate::cli::default_output(),
            color: "#FFF0BD".into(),
            keep_format: true,
            comments: true,
            recent: Vec::new(),
        }
    }
}
impl Preferences {
    pub fn load(path: &Path) -> Self {
        let mut result: Self = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if crate::compare::parse_color(&result.color).is_err() {
            result.color = "#FFF0BD".into();
        }
        result.recent.truncate(8);
        result
    }
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(8);
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(temp.as_file_mut(), self)?;
        temp.as_file_mut().sync_all()?;
        temp.persist(path)?;
        Ok(())
    }
}
pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("DIFF_EXCEL_CONFIG_DIR") {
        return PathBuf::from(path).join("preferences.json");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
    };
    base.join("Diff Excel/preferences.json")
}
