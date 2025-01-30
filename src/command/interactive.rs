use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use indicatif::ProgressBar;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use time::{format_description::BorrowedFormatItem, macros::format_description, OffsetDateTime};
use tracing::{debug, error, info, warn};

use crate::internal::{filter, sync};

const ARCHIVE_DATE_FORMAT: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]-[minute]-[second]");

enum BackupRequest {
    CreateBackup { archive_name: String },
    RestoreBackup { archive_name: String },
}

pub struct InternalGameSavePath {
    pub name: String,
    pub path: PathBuf,
    pub ignore_globset: Option<globset::GlobSet>,
}

pub fn interactive(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    // Read game config
    let gcfg = crate::config::game::GameConfig::from_file(&file_path)?;

    let output_path = data_path.join(name);

    let staging_path = output_path.join("staging");
    let backup_path = output_path.join("backups");

    let last_backup_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let last_change_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let mp = indicatif::MultiProgress::new();

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

    let pause_autobackup = Arc::new(AtomicBool::new(false));

    let (backup_tx, backup_rx) = std::sync::mpsc::channel::<BackupRequest>();

    // Backup thread
    // Ensures that multiple backups cannot run simultaneously
    let backup_join_handle = {
        let save_paths: Vec<InternalGameSavePath> = gcfg
            .save_paths
            .iter()
            .map(|(name, gsp)| {
                let name = name.clone();
                let path = gsp.path.clone();
                let ignore_globset = gsp.ignore.as_ref().map(|v| filter::build_globset(v).unwrap());

                InternalGameSavePath {
                    name,
                    path,
                    ignore_globset,
                }
            })
            .collect();

        let staging_path = staging_path.to_owned();
        let backup_path = backup_path.to_owned();

        let grace_time = Duration::from_secs(gcfg.grace_time);

        let pause_autobackup = pause_autobackup.clone();
        let last_backup_at = last_backup_at.clone();
        let last_change_at = last_change_at.clone();

        let empty_globset = globset::GlobSet::empty();

        let mp = mp.clone();

        std::thread::spawn(move || {
            for backup_request in &backup_rx {
                // Pause autobackup while executing a request
                pause_autobackup.store(true, Ordering::SeqCst);

                let res: Result<(), anyhow::Error> = (|| {
                    match backup_request {
                        BackupRequest::CreateBackup { archive_name } => {
                            // Wait for grace time to elapse.
                            // The purpose of this is to avoid creating backup while files are still
                            // in the middle of being updated. How long grace time is needed
                            // would depend on the game and how long it takes to finish writing its changes,
                            // but in general at least some grace time should always be used.
                            // Any change detected will reset grace time.
                            // Only when grace time has elapsed with no new changes detected in the meantime
                            // should the backup proceed.
                            loop {
                                let grace_time_left = 'gtl: {
                                    let now = Instant::now();

                                    let mut last_change_at = last_change_at.lock().unwrap();

                                    if let Some(time_since_last_change) = last_change_at.map(|lca| now - lca) {
                                        if time_since_last_change < grace_time {
                                            break 'gtl grace_time - time_since_last_change;
                                        }
                                    }

                                    *last_change_at = None;

                                    Duration::ZERO
                                };

                                if grace_time_left.is_zero() {
                                    break;
                                }

                                std::thread::sleep(grace_time_left);
                            }

                            info!("Creating backup: {archive_name}");

                            let now = Instant::now();

                            // Update last_backup_at
                            {
                                let mut last_backup_at = last_backup_at.lock().unwrap();
                                *last_backup_at = Some(now);
                            }

                            let archive_path = backup_path.join(&archive_name);

                            let pb =
                                mp.add(ProgressBar::new(save_paths.len() as u64).with_message("Preparing to stage"));

                            for gsp in save_paths.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                pb.set_message(format!("Staging: {name}"));

                                'stage: {
                                    let staging_gsp_path = staging_path.join(name);

                                    // If source path is missing, remove the existing staging directory for this save path
                                    if !path.exists() {
                                        warn!("Path does not exist [{name}]: {}", path.display());

                                        fs::remove_dir_all(&staging_gsp_path)?;
                                        break 'stage;
                                    }

                                    let ignore_globset = gsp.ignore_globset.as_ref().unwrap_or(&empty_globset);

                                    // Update staging directory
                                    sync::sync(path, &staging_gsp_path, ignore_globset)?;
                                }

                                pb.inc(1);
                            }

                            pb.finish_and_clear();

                            let pb = mp.add(ProgressBar::new_spinner().with_message("Compressing archive"));

                            // Create backup archive
                            create_archive(&staging_path, &archive_path)?;

                            pb.finish_and_clear();

                            info!("Backup created: {archive_name}");
                        }
                        BackupRequest::RestoreBackup { archive_name } => {
                            let archive_path = backup_path.join(&archive_name);

                            if !archive_path.exists() {
                                error!("Archive does not exist: {}", archive_path.display());
                                return Ok(());
                            }

                            info!("Restoring backup from archive: {archive_name}");

                            // Remove staging directory if it exists
                            if staging_path.exists() {
                                fs::remove_dir_all(&staging_path)?;
                            }

                            // Create new empty staging directory
                            fs::create_dir_all(&staging_path)?;

                            let pb = mp.add(ProgressBar::new_spinner().with_message("Unpacking archive"));

                            // Unpack archive to be restored into staging directory
                            unpack_archive(&archive_path, &staging_path)?;

                            pb.finish_and_clear();

                            // Restore save paths from staging directory

                            let pb =
                                mp.add(ProgressBar::new(save_paths.len() as u64).with_message("Preparing to restore"));

                            for gsp in save_paths.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                pb.set_message(format!("Restoring: {name}"));

                                'restore: {
                                    let src_path = staging_path.join(name);

                                    if !src_path.exists() {
                                        warn!("Path does not exist [{name}]: {}", src_path.display());
                                        break 'restore;
                                    }

                                    // Update staging directory
                                    sync::sync(&src_path, path, &empty_globset)?;
                                }

                                pb.inc(1);
                            }

                            pb.finish_and_clear();

                            info!("Backup restored: {archive_name}");

                            let now = Instant::now();

                            // Clear change tracker, to avoid restore triggering automatic backup
                            let mut last_change_at = last_change_at.lock().unwrap();
                            *last_change_at = None;

                            // Set last backup timestamp to now, to prevent autobackup immediately after restore
                            let mut last_backup_at = last_backup_at.lock().unwrap();
                            *last_backup_at = Some(now);
                        }
                    }

                    Ok(())
                })();

                if let Err(err) = res {
                    error!("{err}");
                }

                // Resume autobackup after request is completed
                pause_autobackup.store(false, Ordering::SeqCst);
            }
        })
    };

    // Auto-backup thread
    let autobackup_join_handle = {
        let cancel = cancel.clone();

        let backup_interval = Duration::from_secs(gcfg.backup_interval);

        let pause_autobackup = pause_autobackup.clone();
        let last_backup_at = last_backup_at.clone();
        let last_change_at = last_change_at.clone();

        let backup_tx = backup_tx.clone();

        let mut last_autobackup_at: Option<Instant> = None;

        std::thread::spawn(move || loop {
            if cancel.load(Ordering::SeqCst) {
                break;
            }

            std::thread::sleep(Duration::from_secs(1));

            if pause_autobackup.load(Ordering::SeqCst) {
                continue;
            }

            let now = Instant::now();

            {
                let last_backup_at = last_backup_at.lock().unwrap();

                // If no backup has been created since the last autobackup was created,
                // do not request another. It's likely still waiting for grace time.
                if let Some(last_autobackup_at) = last_autobackup_at {
                    let Some(last_backup_at) = *last_backup_at else {
                        continue;
                    };

                    if last_backup_at < last_autobackup_at {
                        continue;
                    }
                }

                let last_change_at = last_change_at.lock().unwrap();

                let Some(last_change_at) = *last_change_at else {
                    continue;
                };

                if let Some(last_autobackup_at) = last_autobackup_at {
                    if last_autobackup_at >= last_change_at {
                        continue;
                    }
                }

                if let Some(last_backup_at) = *last_backup_at {
                    if now < (last_backup_at + backup_interval) {
                        continue;
                    }
                }
            }

            last_autobackup_at = Some(now);

            info!("Creating auto-backup");

            let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
            let archive_name = format!("Auto {}.7z", now.format(ARCHIVE_DATE_FORMAT).unwrap());

            backup_tx.send(BackupRequest::CreateBackup { archive_name }).unwrap();
        })
    };

    // Watch save directory for changes
    let (watcher_join_handle, watcher) = {
        let cancel = cancel.clone();

        let last_change_at = last_change_at.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        for (_, gsp) in gcfg.save_paths.iter() {
            watcher.watch(&gsp.path, RecursiveMode::Recursive)?;
        }

        let join_handle = std::thread::spawn(move || {
            for result in &rx {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }

                match result {
                    Ok(event) => {
                        debug!("Event {event:?}");

                        if event.kind.is_access() {
                            continue;
                        }

                        let mut last_change_at = last_change_at.lock().unwrap();
                        *last_change_at = Some(Instant::now())
                    }
                    Err(error) => error!("Error {error:?}"),
                }
            }
        });

        (join_handle, watcher)
    };

    // Interactive prompt

    let create_manual_backup = || -> Result<(), anyhow::Error> {
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let default_name = format!("Manual {}", now.format(ARCHIVE_DATE_FORMAT).unwrap());

        let name: String = dialoguer::Input::new()
            .with_prompt("Backup name")
            .default(default_name)
            .interact_text()?;

        let archive_name = format!("{name}.7z");

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

    info!("Shutting down...");

    'exit_backup: {
        let last_backup_at = last_backup_at.lock().unwrap();
        let last_change_at = last_change_at.lock().unwrap();

        let Some(last_change_at) = *last_change_at else {
            break 'exit_backup;
        };

        if let Some(last_backup_at) = *last_backup_at {
            if last_backup_at > last_change_at {
                break 'exit_backup;
            }
        }

        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let archive_name = format!("Exit {}.7z", now.format(ARCHIVE_DATE_FORMAT).unwrap());

        backup_tx.send(BackupRequest::CreateBackup { archive_name })?;
    }

    // Signal cancellation
    cancel.store(true, Ordering::SeqCst);

    drop(watcher);
    drop(backup_tx);

    // Wait for threads to complete
    watcher_join_handle.join().unwrap();
    autobackup_join_handle.join().unwrap();
    backup_join_handle.join().unwrap();

    mp.clear()?;

    Ok(())
}

fn create_archive(src: &Path, archive_path: &Path) -> Result<(), anyhow::Error> {
    std::process::Command::new("7z")
        .current_dir(src)
        .args(["a", "-mx9"])
        .arg(archive_path)
        .arg(".")
        .stdout(Stdio::null())
        .status()?;

    Ok(())
}

fn unpack_archive(archive_path: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    std::process::Command::new("7z")
        .current_dir(dst)
        .arg("x")
        .arg(archive_path)
        .stdout(Stdio::null())
        .status()?;

    Ok(())
}
