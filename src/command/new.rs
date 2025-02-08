use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::config::game::{GameConfig, GameSavePath, GameSavePathType};

pub fn new(game_config_path: &Path) -> Result<(), anyhow::Error> {
    let name: String = dialoguer::Input::new().with_prompt("Name").interact_text()?;
    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    if file_path.exists() {
        return Err(anyhow::anyhow!("Game config '{name}' already exists"));
    }

    let mut save_paths: HashMap<String, GameSavePath> = HashMap::new();

    loop {
        let name: String = dialoguer::Input::new()
            .with_prompt("Add save path name (blank to proceed without adding)")
            .allow_empty(true)
            .interact_text()?;

        if name.is_empty() {
            if save_paths.is_empty() {
                eprintln!("At least one save path is required.");
                continue;
            }

            break;
        }

        let path: PathBuf = dialoguer::Input::<String>::new()
            .with_prompt("Path")
            .interact_text()?
            .into();

        let type_ = if path.is_file() {
            GameSavePathType::File
        } else {
            GameSavePathType::Directory
        };

        save_paths.insert(
            name,
            GameSavePath {
                type_,
                path,
                ignore: Default::default(),
            },
        );
    }

    let backup_interval: u64 = dialoguer::Input::new()
        .with_prompt("Backup interval (seconds)")
        .default(600)
        .interact_text()?;

    let grace_time: u64 = dialoguer::Input::new()
        .with_prompt("Grace time (seconds)")
        .default(10)
        .interact_text()?;

    let copy_latest_to_path: String = dialoguer::Input::new()
        .with_prompt("Copy latest backup to path (blank for none)")
        .allow_empty(true)
        .interact_text()?;

    let copy_latest_to_path: Option<PathBuf> = if !copy_latest_to_path.is_empty() {
        Some(copy_latest_to_path.into())
    } else {
        None
    };

    let game_config = GameConfig {
        save_paths,
        backup_interval,
        grace_time,
        copy_latest_to_path,
    };

    fs::create_dir_all(game_config_path)?;
    game_config.write(&file_path)?;

    Ok(())
}
