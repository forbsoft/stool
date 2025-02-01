use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Context;
use tracing::{error, info};

use crate::{
    engine::{self, make_backup_filename, BackupRequest},
    ui::FancyUiHandler,
};

pub fn interactive(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    let output_path = data_path.join(name);
    let backup_path = output_path.join("backups");

    let ui = FancyUiHandler::new();

    // Cancellation boolean.
    let cancel = Arc::new(AtomicBool::new(false));

    // Set break (Ctrl-C) handler.
    ctrlc::set_handler({
        let cancel = cancel.clone();

        move || {
            info!("Cancellation requested by user.");
            cancel.store(true, Ordering::SeqCst);
        }
    })
    .unwrap_or_else(|err| error!("Error setting Ctrl-C handler: {}", err));

    let (engine_join_handle, backup_tx) = engine::run(name, game_config_path, data_path, cancel.clone(), ui)?;

    // Interactive prompt

    let create_manual_backup = || -> Result<(), anyhow::Error> {
        let description: String = dialoguer::Input::new()
            .with_prompt("Backup description")
            .default("Manual".into())
            .interact_text()?;

        let archive_name = make_backup_filename(&description);

        backup_tx.send(BackupRequest::CreateBackup { archive_name })?;

        Ok(())
    };

    let restore_backup = || -> Result<(), anyhow::Error> {
        let backup_files = fs::read_dir(&backup_path)?;
        let mut backup_files: Vec<_> = backup_files
            .filter_map(Result::ok)
            .filter_map(|e| {
                let path = e.path();

                if !path.is_file() || !matches!(path.extension(), Some(ext) if ext == "7z") {
                    return None;
                }

                let metadata = path.metadata().unwrap();
                let modified = metadata.modified().unwrap();

                Some((path, modified))
            })
            .collect();

        backup_files.sort_by_key(|(_, v)| *v);
        backup_files.reverse();
        backup_files.truncate(20);

        let backup_items: Vec<_> = backup_files
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy())
            .chain(["...".into()])
            .collect();

        let Some(selected_ix) = dialoguer::Select::new()
            .with_prompt("Backup to restore")
            .items(&backup_items)
            .interact_opt()?
        else {
            return Ok(());
        };

        let archive_name: String = if selected_ix < backup_items.len() - 1 {
            backup_items
                .get(selected_ix)
                .context("Getting selected backup item by index")?
                .clone()
                .into_owned()
        } else {
            dialoguer::Input::new()
                .with_prompt("Name of archive to restore")
                .allow_empty(true)
                .interact_text()?
        };

        if archive_name.is_empty() {
            return Ok(());
        }

        backup_tx.send(BackupRequest::RestoreBackup { archive_name })?;

        Ok(())
    };

    loop {
        eprintln!();

        let Ok(choice) = dialoguer::Select::new()
            .default(0)
            .item("Create backup") // 0
            .item("Restore backup") // 0
            .item("Exit") // 1
            .interact_opt()
        else {
            break;
        };

        dialoguer::console::Term::stderr().clear_screen()?;

        let Some(choice) = choice else {
            continue;
        };

        match choice {
            0 => create_manual_backup()?,
            1 => restore_backup()?,
            2 => break,
            _ => {}
        }
    }

    cancel.store(true, Ordering::SeqCst);

    drop(backup_tx);

    // Wait for engine to shut down gracefully
    engine_join_handle.join().unwrap();

    Ok(())
}
