use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameSavePathType {
    Directory,
    File,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameSavePath {
    #[serde(rename = "type")]
    #[serde(default = "default_gamesavepath_type")]
    pub type_: GameSavePathType,
    pub path: PathBuf,
    pub include: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameConfig {
    pub save_paths: HashMap<String, GameSavePath>,
    pub backup_interval: u64,
    pub grace_time: u64,
    pub copy_latest_to_path: Option<PathBuf>,
}

impl GameConfig {
    pub fn from_file(path: &Path) -> Result<Self, anyhow::Error> {
        use std::io::Read;

        let mut file = fs::File::open(path).context("Error opening config file")?;

        let mut toml_str = String::new();
        file.read_to_string(&mut toml_str)
            .context("Error reading config file")?;

        Self::from_str(&toml_str)
    }

    pub fn write(&self, path: &Path) -> Result<(), anyhow::Error> {
        let toml_str = toml::to_string_pretty(self)?;

        // Write to file.
        let mut file = fs::File::create(path).context("Error creating game config file")?;
        file.write_all(toml_str.as_bytes())
            .context("Error writing to config file")?;

        Ok(())
    }
}

impl FromStr for GameConfig {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let config: Self = toml::from_str(s).context("Error parsing config")?;

        Ok(config)
    }
}

fn default_gamesavepath_type() -> GameSavePathType {
    GameSavePathType::Directory
}
