use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_update_check")]
    pub update_check: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            update_check: true,
        }
    }
}

fn default_update_check() -> bool {
    true
}

impl AppConfig {
    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_optional(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match Self::load_from(path) {
            Ok(config) => config,
            Err(_) => Self::default(),
        }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from("zerocode.toml")
    }
}
