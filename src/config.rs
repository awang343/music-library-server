use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub library_path: PathBuf,
    pub db_path: PathBuf,
    pub bind: String,
    /// If set, all /api/* routes require `Authorization: Bearer <token>`.
    /// If unset, the API is open — fine for localhost, never for non-loopback.
    #[serde(default)]
    pub auth_token: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw).context("parsing config")?;
        Ok(cfg)
    }
}
