use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub database: DatabaseConfig,
    pub notion: NotionConfig,
}

impl Config {
    pub fn open(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).context("Failed to read file")?;
        let config = toml::from_str(&text).context("Failed to parse config")?;
        Ok(config)
    }
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct DatabaseConfig {
    pub host: String,
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct NotionConfig {
    pub token_v2: String,
    pub file_token: String,
    pub page_id: String,
    pub user_agent: Option<String>,
}
