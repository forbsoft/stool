use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::config::game::GameConfig;

pub fn new(game_config_path: &Path) -> Result<(), anyhow::Error> {
    let name: String = dialoguer::Input::new().with_prompt("Name").interact_text()?;
    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    if file_path.exists() {
        return Err(anyhow::anyhow!("Game config '{name}' already exists"));
    }

    let save_path: PathBuf = dialoguer::Input::<String>::new()
        .with_prompt("Save path")
        .interact_text()?
        .into();

    let backup_interval: u64 = dialoguer::Input::new()
        .with_prompt("Backup interval (seconds)")
        .interact_text()?;

    let game_config = GameConfig {
        save_path,
        backup_interval,
    };

    fs::create_dir_all(game_config_path)?;
    game_config.write(&file_path)?;

    Ok(())
}
