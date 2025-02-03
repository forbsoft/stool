pub mod ui;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use time::{format_description::BorrowedFormatItem, macros::format_description, OffsetDateTime};
use tracing::{error, info, warn};
use ui::StoolUiHandler;

use crate::internal::{filter, pid::PidLock, sync};

pub const ARCHIVE_DATE_FORMAT: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]-[minute]-[second]");

const SLEEP_DURATION: Duration = Duration::from_secs(1);

pub enum BackupRequest {
    CreateBackup { archive_name: String },
    RestoreBackup { archive_name: String },
}

struct InternalGameSavePath {
    pub name: String,
    pub path: PathBuf,
    pub ignore_globset: Option<globset::GlobSet>,
}

pub fn run(
    name: &str,
    game_config_path: &Path,
    data_path: &Path,
    autobackup: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    mut ui: impl StoolUiHandler,
) -> Result<(std::thread::JoinHandle<()>, Sender<BackupRequest>), anyhow::Error> {
    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    // Read game config
    let gcfg = crate::config::game::GameConfig::from_file(&file_path)?;

    let output_path = data_path.join(name);

    fs::create_dir_all(&output_path)?;

    let pid_lock = PidLock::acquire(output_path.join("stool.pid")).context("Acquiring PID-lock")?;

    let staging_path = output_path.join("staging");
    let backup_path = output_path.join("backups");

    let last_backup_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let last_change_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let latest_backup_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

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
        let latest_backup_path = latest_backup_path.clone();

        let empty_globset = globset::GlobSet::empty();

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

                            ui.begin_backup(&archive_name);

                            let now = Instant::now();

                            // Update last_backup_at
                            {
                                let mut last_backup_at = last_backup_at.lock().unwrap();
                                *last_backup_at = Some(now);
                            }

                            let archive_path = backup_path.join(&archive_name);

                            ui.begin_staging(save_paths.len());

                            for gsp in save_paths.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                ui.begin_stage(name);

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
                                    sync::sync(path, &staging_gsp_path, ignore_globset, &mut ui)?;
                                }

                                ui.end_stage();
                            }

                            ui.end_staging();

                            ui.begin_compress();

                            // Create backup archive
                            create_archive(&staging_path, &archive_path)?;

                            ui.end_compress();

                            ui.end_backup(true);

                            // Store path to latest backup archive
                            let mut latest_backup_path = latest_backup_path.lock().unwrap();
                            *latest_backup_path = Some(archive_path);
                        }
                        BackupRequest::RestoreBackup { archive_name } => {
                            let archive_path = backup_path.join(&archive_name);

                            if !archive_path.exists() {
                                error!("Archive does not exist: {}", archive_path.display());
                                return Ok(());
                            }

                            ui.begin_restore(&archive_name);

                            // Remove staging directory if it exists
                            if staging_path.exists() {
                                fs::remove_dir_all(&staging_path)?;
                            }

                            // Create new empty staging directory
                            fs::create_dir_all(&staging_path)?;

                            ui.begin_extract();

                            // Unpack archive to be restored into staging directory
                            unpack_archive(&archive_path, &staging_path)?;

                            ui.end_extract();

                            // Restore save paths from staging directory

                            for gsp in save_paths.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                ui.begin_restore_sp(name);

                                'restore: {
                                    let src_path = staging_path.join(name);

                                    if !src_path.exists() {
                                        warn!("Path does not exist [{name}]: {}", src_path.display());
                                        break 'restore;
                                    }

                                    // Update staging directory
                                    sync::sync(&src_path, path, &empty_globset, &mut ui)?;
                                }

                                ui.end_restore_sp();
                            }

                            ui.end_restore(true);

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

            ui.clear().unwrap();
        })
    };

    // Auto-backup thread
    let autobackup_join_handle = {
        let shutdown = shutdown.clone();

        let backup_interval = Duration::from_secs(gcfg.backup_interval);

        let pause_autobackup = pause_autobackup.clone();
        let last_backup_at = last_backup_at.clone();
        let last_change_at = last_change_at.clone();

        let backup_tx = backup_tx.clone();

        let mut last_autobackup_at: Option<Instant> = None;

        std::thread::spawn(move || loop {
            if shutdown.load(Ordering::SeqCst) {
                break;
            }

            std::thread::sleep(Duration::from_secs(1));

            if !autobackup.load(Ordering::SeqCst) || pause_autobackup.load(Ordering::SeqCst) {
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

            let archive_name = make_backup_filename("Auto");
            backup_tx.send(BackupRequest::CreateBackup { archive_name }).unwrap();
        })
    };

    // Watch save directory for changes
    let (watcher_join_handle, watcher) = {
        let shutdown = shutdown.clone();

        let last_change_at = last_change_at.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        for (_, gsp) in gcfg.save_paths.iter() {
            watcher.watch(&gsp.path, RecursiveMode::Recursive)?;
        }

        let join_handle = std::thread::spawn(move || {
            for result in &rx {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }

                match result {
                    Ok(event) => {
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

    let engine_join_handle = {
        let backup_tx = backup_tx.clone();

        std::thread::spawn(move || {
            let _pid_lock = pid_lock;

            while !shutdown.load(Ordering::SeqCst) {
                std::thread::sleep(SLEEP_DURATION);
            }

            info!("Shutting down...");

            'exit_backup: {
                // If autobackup is paused, do not request an exit backup,
                // as that means a backup or restore is in progress.
                if pause_autobackup.load(Ordering::SeqCst) {
                    break 'exit_backup;
                }

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

                let archive_name = make_backup_filename("Exit");

                backup_tx.send(BackupRequest::CreateBackup { archive_name }).unwrap();
            }

            drop(watcher);
            drop(backup_tx);

            // Wait for threads to complete
            watcher_join_handle.join().unwrap();
            autobackup_join_handle.join().unwrap();
            backup_join_handle.join().unwrap();

            // If a copy_latest_to_path is set, and a backup was created this session,
            // copy the latest backup to the specified path.
            'copy_latest: {
                if let Some(copy_latest_to_path) = gcfg.copy_latest_to_path {
                    let latest_backup_path = latest_backup_path.lock().unwrap();
                    if let Some(latest_backup_path) = latest_backup_path.as_ref() {
                        let Some(filename) = latest_backup_path.file_name() else {
                            break 'copy_latest;
                        };

                        fs::copy(latest_backup_path, copy_latest_to_path.join(filename)).unwrap();
                    }
                }
            }
        })
    };

    Ok((engine_join_handle, backup_tx))
}

pub fn make_backup_filename(description: &str) -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());

    format!("{} {description}.7z", now.format(ARCHIVE_DATE_FORMAT).unwrap())
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
