pub mod ui;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::Sender,
        Arc, Mutex, Weak,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::Context;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use num_enum::{IntoPrimitive, TryFromPrimitive};
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

#[derive(Clone, Copy, IntoPrimitive, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum EngineState {
    Starting = 0,
    Running = 1,
    ShuttingDown = 2,
    ShutDown = 3,
}

#[derive(Clone)]
pub struct EngineArgs {
    pub name: String,
    pub game_config_path: PathBuf,
    pub data_path: PathBuf,
}

/// Represents a running instance of an S-Tool engine.
pub struct Engine {
    args: EngineArgs,
    control: EngineControl,
    join_handle: JoinHandle<()>,
}

/// Exposes various functions to allow limited
/// interactions with a running S-Tool engine.
#[derive(Clone)]
pub struct EngineControl {
    shutdown: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    autobackup: Arc<AtomicBool>,
    backup_tx: Weak<Sender<BackupRequest>>,
}

#[derive(Clone)]
struct InternalGameSaveDir {
    pub name: String,
    pub path: PathBuf,
    pub include_globset: Option<globset::GlobSet>,
    pub ignore_globset: Option<globset::GlobSet>,
}

impl Engine {
    pub fn args(&self) -> &EngineArgs {
        &self.args
    }

    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }

    pub fn has_shut_down(&self) -> bool {
        self.join_handle.is_finished()
    }

    /// Wait for engine thread to finish
    pub fn join(self) {
        self.join_handle.join().unwrap();
    }
}

impl EngineControl {
    /// Request shutdown of engine
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn state(&self) -> EngineState {
        self.state
            .load(Ordering::Acquire)
            .try_into()
            .expect("Unknown engine state")
    }

    pub fn get_autobackup(&self) -> bool {
        self.autobackup.load(Ordering::Relaxed)
    }

    pub fn set_autobackup(&self, val: bool) {
        self.autobackup.store(val, Ordering::Relaxed);
    }

    /// Request a backup operation
    pub fn send(&self, req: BackupRequest) -> Result<(), anyhow::Error> {
        let Some(backup_tx) = self.backup_tx.upgrade() else {
            return Ok(());
        };

        backup_tx.send(req)?;

        Ok(())
    }
}

pub fn run(args: EngineArgs, shutdown: Arc<AtomicBool>, mut ui: impl StoolUiHandler) -> Result<Engine, anyhow::Error> {
    let EngineArgs {
        name,
        game_config_path,
        data_path,
    } = &args;

    let file_name = format!("{name}.toml");
    let file_path = game_config_path.join(&file_name);

    // Read game config
    let gcfg = crate::config::game::GameConfig::from_file(&file_path)?;

    let output_path = data_path.join(name);

    fs::create_dir_all(&output_path)?;

    let pid_lock = PidLock::acquire(output_path.join("stool.pid")).context("Acquiring PID-lock")?;

    let staging_path = output_path.join("staging");
    let backup_path = output_path.join("backups");

    if staging_path.exists() {
        fs::remove_dir_all(&staging_path)?;
    }

    let state = Arc::new(AtomicU8::new(EngineState::Starting as u8));

    let last_backup_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let last_change_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let latest_backup_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

    let backup_or_restore_ongoing = Arc::new(AtomicBool::new(false));

    let autobackup = Arc::new(AtomicBool::new(gcfg.auto_backup.enabled));
    let (backup_tx, backup_rx) = std::sync::mpsc::channel::<BackupRequest>();

    let save_dirs: Vec<InternalGameSaveDir> = gcfg
        .save_dirs
        .iter()
        .map(|(name, gsp)| {
            let name = name.clone();
            let path = gsp.path.clone();
            let include_globset = gsp.include.as_ref().map(|v| filter::build_globset(v).unwrap());
            let ignore_globset = gsp.ignore.as_ref().map(|v| filter::build_globset(v).unwrap());

            InternalGameSaveDir {
                name,
                path,
                include_globset,
                ignore_globset,
            }
        })
        .collect();

    // Backup thread
    // Ensures that multiple backups cannot run simultaneously
    let backup_join_handle = {
        let save_dirs = save_dirs.clone();
        let save_files = gcfg.save_files.clone();

        let staging_path = staging_path.to_owned();
        let backup_path = backup_path.to_owned();

        let grace_time = Duration::from_secs(gcfg.grace_time);

        let backup_or_restore_ongoing = backup_or_restore_ongoing.clone();
        let last_backup_at = last_backup_at.clone();
        let last_change_at = last_change_at.clone();
        let latest_backup_path = latest_backup_path.clone();

        std::thread::spawn(move || {
            for backup_request in &backup_rx {
                // Pause autobackup while executing a request
                backup_or_restore_ongoing.store(true, Ordering::Release);

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

                            ui.begin_staging(save_dirs.len() + save_files.len());

                            for gsp in save_dirs.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                ui.begin_stage(name);

                                'stage: {
                                    let staging_gsp_path = staging_path.join(name);

                                    // If source path is missing, remove the existing staging directory for this save path
                                    if !path.exists() {
                                        warn!("Save dir does not exist [{name}]: {}", path.display());

                                        fs::remove_dir_all(&staging_gsp_path)?;
                                        break 'stage;
                                    }

                                    // Sync to staging directory
                                    sync::sync_dir(
                                        path,
                                        &staging_gsp_path,
                                        gsp.include_globset.as_ref(),
                                        gsp.ignore_globset.as_ref(),
                                        false,
                                        &mut ui,
                                    )?;
                                }

                                ui.end_stage();
                            }

                            for gsf in save_files.iter() {
                                let path = &gsf.path;
                                let dir_path = path
                                    .parent()
                                    .context("Couldn't get parent directory of game save file")?;
                                let rel_path = path.strip_prefix(dir_path)?;

                                ui.begin_stage(&rel_path.to_string_lossy());

                                'stage: {
                                    let staging_dir_path = if let Some(staging_subdir) = &gsf.staging_subdirectory {
                                        &staging_path.join(staging_subdir)
                                    } else {
                                        &staging_path
                                    };

                                    let staging_file_path = staging_dir_path.join(rel_path);

                                    // If source path is missing, remove the existing staging directory for this save path
                                    if !path.exists() {
                                        warn!("Save file does not exist [{}]: {}", rel_path.display(), path.display());

                                        fs::remove_file(&staging_file_path)?;
                                        break 'stage;
                                    }

                                    // Sync to staging directory
                                    fs::create_dir_all(staging_dir_path)?;
                                    sync::sync_file(path, staging_dir_path, &mut ui)?;
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

                            for gsp in save_dirs.iter() {
                                let name = &gsp.name;
                                let path = &gsp.path;

                                ui.begin_restore_sp(name);

                                'restore: {
                                    let src_path = staging_path.join(name);

                                    if !src_path.exists() {
                                        warn!("Directory does not exist in backup [{name}]: {}", src_path.display());
                                        break 'restore;
                                    }

                                    // Sync to save directory
                                    sync::sync_dir(
                                        &src_path,
                                        path,
                                        gsp.include_globset.as_ref(),
                                        gsp.ignore_globset.as_ref(),
                                        true,
                                        &mut ui,
                                    )?;
                                }

                                ui.end_restore_sp();
                            }

                            for gsf in save_files.iter() {
                                let path = &gsf.path;
                                let dir_path = path
                                    .parent()
                                    .context("Couldn't get parent directory of game save file")?;
                                let rel_path = path.strip_prefix(dir_path)?;

                                ui.begin_restore_sp(&rel_path.to_string_lossy());

                                'restore: {
                                    let staging_dir_path = if let Some(staging_subdir) = &gsf.staging_subdirectory {
                                        &staging_path.join(staging_subdir)
                                    } else {
                                        &staging_path
                                    };

                                    let staging_file_path = staging_dir_path.join(rel_path);

                                    if !staging_file_path.exists() {
                                        warn!(
                                            "File does not exist in backup [{}]: {}",
                                            rel_path.display(),
                                            staging_file_path.display()
                                        );
                                        break 'restore;
                                    }

                                    // Sync to save directory
                                    fs::create_dir_all(dir_path)?;
                                    sync::sync_file(&staging_file_path, dir_path, &mut ui)?;
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
                backup_or_restore_ongoing.store(false, Ordering::Release);
            }

            ui.clear().unwrap();
        })
    };

    // Auto-backup thread
    let autobackup_join_handle = {
        let shutdown = shutdown.clone();
        let autobackup = autobackup.clone();

        let min_interval = Duration::from_secs(gcfg.auto_backup.min_interval);

        let backup_or_restore_ongoing = backup_or_restore_ongoing.clone();
        let last_backup_at = last_backup_at.clone();
        let last_change_at = last_change_at.clone();

        let backup_tx = backup_tx.clone();

        let mut last_autobackup_at: Option<Instant> = None;

        std::thread::spawn(move || loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            std::thread::sleep(Duration::from_secs(1));

            if !autobackup.load(Ordering::Acquire) || backup_or_restore_ongoing.load(Ordering::Acquire) {
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
                    if now < (last_backup_at + min_interval) {
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
        let last_change_at = last_change_at.clone();
        let save_files: Vec<_> = gcfg.save_files.iter().map(|gsf| gsf.path.clone()).collect();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        // Watch save directories
        for gsp in save_dirs.iter() {
            watcher.watch(&gsp.path, RecursiveMode::Recursive)?;
        }

        // Watch save files
        for gsf_path in save_files.iter() {
            watcher.watch(gsf_path, RecursiveMode::NonRecursive)?;
        }

        let save_dirs: Vec<_> = save_dirs
            .into_iter()
            .filter_map(|gsp| {
                if gsp.include_globset.is_none() && gsp.ignore_globset.is_none() {
                    return None;
                }

                Some((gsp.path, gsp.include_globset, gsp.ignore_globset))
            })
            .collect();

        let join_handle = std::thread::spawn(move || {
            'watch_event: for result in &rx {
                match result {
                    Ok(event) => {
                        if event.kind.is_access() {
                            continue;
                        }

                        'ignore: {
                            for path in event.paths.iter() {
                                if save_files.contains(path) {
                                    break 'ignore;
                                }
                            }

                            if save_dirs.is_empty() {
                                break 'ignore;
                            }

                            for (save_dir_path, include_globset, ignore_globset) in save_dirs.iter() {
                                for path in event.paths.iter() {
                                    let Ok(rel_path) = path.strip_prefix(save_dir_path) else {
                                        continue;
                                    };

                                    if let Some(include_globset) = include_globset {
                                        if !include_globset.is_match(rel_path) {
                                            continue;
                                        }
                                    }

                                    if let Some(ignore_globset) = ignore_globset {
                                        if ignore_globset.is_match(rel_path) {
                                            continue;
                                        }
                                    }

                                    break 'ignore;
                                }
                            }

                            continue 'watch_event;
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

    let backup_tx = Arc::new(backup_tx);
    let weak_backup_tx = Arc::downgrade(&backup_tx);

    let engine_join_handle = {
        let shutdown = shutdown.clone();
        let state = state.clone();

        std::thread::spawn(move || {
            let _pid_lock = pid_lock;

            // Set engine state to Running
            state.store(EngineState::Running as u8, Ordering::Release);

            while !shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(SLEEP_DURATION);
            }

            info!("Shutting down...");

            // Set engine state to ShuttingDown
            state.store(EngineState::ShuttingDown as u8, Ordering::Release);

            'exit_backup: {
                // If a backup or restore is ongoing, do not request an exit backup.
                if backup_or_restore_ongoing.load(Ordering::Acquire) {
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

                info!("Creating exit backup...");

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

            // Try to delete staging directory
            if staging_path.exists() {
                fs::remove_dir_all(&staging_path).ok();
            }

            // Set engine state to ShutDown
            state.store(EngineState::ShutDown as u8, Ordering::Release);
        })
    };

    let control = EngineControl {
        shutdown,
        state,
        autobackup,
        backup_tx: weak_backup_tx,
    };

    Ok(Engine {
        args,
        control,
        join_handle: engine_join_handle,
    })
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
