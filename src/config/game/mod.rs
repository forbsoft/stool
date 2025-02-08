use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameSaveDir {
    pub path: PathBuf,
    pub include: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameSaveFile {
    pub path: PathBuf,
    pub staging_subdirectory: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AutoBackup {
    pub enabled: bool,
    pub min_interval: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameConfig {
    pub grace_time: u64,
    pub copy_latest_to_path: Option<PathBuf>,

    pub auto_backup: AutoBackup,

    #[serde(default)]
    pub save_dirs: BTreeMap<String, GameSaveDir>,
    #[serde(default)]
    #[serde(rename = "save-file")]
    pub save_files: Vec<GameSaveFile>,
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
