use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::config::game::{GameConfig, GameSaveDir, GameSaveFile};

pub fn new(game_config_path: &Path) -> Result<(), anyhow::Error> {
    let name: String = dialoguer::Input::new().with_prompt("Name").interact_text()?;
    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    if file_path.exists() {
        return Err(anyhow::anyhow!("Game config '{name}' already exists"));
    }

    let mut save_dirs: BTreeMap<String, GameSaveDir> = BTreeMap::new();
    let mut save_files: Vec<GameSaveFile> = Vec::new();

    loop {
        let path: String = dialoguer::Input::new()
            .with_prompt("Save path (blank to proceed without adding)")
            .allow_empty(true)
            .interact_text()?;

        if path.is_empty() {
            if save_dirs.is_empty() && save_files.is_empty() {
                eprintln!("At least one save directory or file is required.");
                continue;
            }

            break;
        }

        let path: PathBuf = path.into();

        if path.is_file() {
            save_files.push(GameSaveFile {
                path,
                staging_subdirectory: None,
            });
        } else {
            let name: String = dialoguer::Input::new().with_prompt("Name").interact_text()?;

            save_dirs.insert(
                name,
                GameSaveDir {
                    path,
                    include: Default::default(),
                    ignore: Default::default(),
                },
            );
        }
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
        save_dirs,
        save_files,
        backup_interval,
        grace_time,
        copy_latest_to_path,
    };

    fs::create_dir_all(game_config_path)?;
    game_config.write(&file_path)?;

    Ok(())
}
