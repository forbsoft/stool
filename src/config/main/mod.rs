use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context;
use serde_derive::{Deserialize, Serialize};
use tracing::error;

pub const CONFIG_DIR_NAME: &str = "stool";
pub const CONFIG_FILENAME: &str = "config.toml";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MainConfig {
    pub data_path: PathBuf,
}

impl MainConfig {
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

    fn path_from_location(path: &Path) -> Result<PathBuf, anyhow::Error> {
        Ok(path.join(CONFIG_FILENAME))
    }

    /// Load configuration from default location,
    /// creating it if it is missing.
    pub fn load_or_write_default_from_location(config_location: &Path) -> Result<Self, anyhow::Error> {
        let config_file_path = Self::path_from_location(config_location)?;

        if config_file_path.exists() {
            Ok(Self::from_file(&config_file_path)?)
        } else {
            let data_path = dirs::data_local_dir()
                .context("Get local data directory")?
                .join(CONFIG_DIR_NAME);

            let config = MainConfig { data_path };

            // Create parent directory if needed
            fs::create_dir_all(config_location)?;

            // Write config file
            config.write(&config_file_path)?;

            Ok(config)
        }
    }
}

impl FromStr for MainConfig {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let config: Self = toml::from_str(s).context("Error parsing config")?;

        Ok(config)
    }
}

pub fn get_default_config_path() -> Option<PathBuf> {
    let config_path = dirs::config_dir().map(|p| p.join(CONFIG_DIR_NAME));

    if config_path.is_none() {
        error!("Could not get configuration path!");
    }

    config_path
}
