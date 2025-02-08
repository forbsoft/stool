use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use filetime::FileTime;
use tracing::error;

use crate::internal::hash::hash_crc32;

#[derive(Debug)]
pub struct SyncDir {
    path: PathBuf,

    dirs: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
}

#[derive(Debug)]
enum SyncOp {
    Copy { path: PathBuf },
    CreateDir { path: PathBuf },
    Delete { path: PathBuf },
    RemoveDir { path: PathBuf },
    VerifyCheckSum { path: PathBuf, size: u64, crc32: u32 },
}

#[derive(Debug)]
pub struct SyncJob {
    src_path: PathBuf,
    dst_path: PathBuf,

    ops: Vec<SyncOp>,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncJobError {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    #[error("File not found in source: {path}")]
    FileNotFound { path: PathBuf },
    #[error("Error reading from source: {path}")]
    ReadError { path: PathBuf },
}

pub trait SyncUiHandler {
    fn begin_scan(&mut self);
    fn end_scan(&mut self);

    fn begin_prepare(&mut self);
    fn end_prepare(&mut self);

    fn begin_sync(&mut self, op_count: usize);
    fn sync_progress(&mut self);
    fn end_sync(&mut self);

    fn begin_file(&mut self, prefix: &str, filename: &str, size: u64);
    fn file_progress(&mut self, bytes: u64);
    fn end_file(&mut self);
}

impl SyncDir {
    pub fn new(
        path: &Path,
        ignore_globset: &globset::GlobSet,
        ui: &mut dyn SyncUiHandler,
    ) -> Result<Self, anyhow::Error> {
        let path = path.canonicalize()?;
        let mut dirs: HashSet<PathBuf> = HashSet::new();
        let mut files: HashSet<PathBuf> = HashSet::new();

        ui.begin_scan();

        let entries = walkdir::WalkDir::new(&path).into_iter().filter_map(Result::ok);

        for entry in entries {
            let is_file = entry.file_type().is_file();
            let rel_path = entry.into_path().strip_prefix(&path)?.to_path_buf();

            if ignore_globset.is_match(&rel_path) {
                continue;
            }

            if !is_file {
                dirs.insert(rel_path);
                continue;
            }

            files.insert(rel_path);
        }

        ui.end_scan();

        Ok(Self { path, dirs, files })
    }

    pub fn sync_from(&self, other: &Self, ui: &mut dyn SyncUiHandler) -> Result<SyncJob, anyhow::Error> {
        let src = other;
        let dst = self;

        let src_path = src.path.clone();
        let dst_path = dst.path.clone();

        ui.begin_prepare();

        let item_count = src.dirs.len() + src.files.len();
        let mut ops: Vec<SyncOp> = Vec::with_capacity(item_count);
        let mut post_ops: Vec<SyncOp> = Vec::with_capacity(item_count);

        // Create dirs not in destination
        let dirs_not_in_dst = src.dirs.difference(&self.dirs);
        ops.extend(dirs_not_in_dst.map(|p| SyncOp::CreateDir { path: p.clone() }));

        // Copy files not in destination
        let files_not_in_dst = src.files.difference(&self.files);
        for p in files_not_in_dst {
            let src_file_path = src_path.join(p);

            let src_metadata = src_file_path.metadata()?;
            let size = src_metadata.len();

            ui.begin_file("Checksum", &p.to_string_lossy(), size);

            let src_hash = hash_crc32(&src_file_path, |bytes| ui.file_progress(bytes as u64))?;

            ui.end_file();

            ops.push(SyncOp::Copy { path: p.clone() });
            post_ops.push(SyncOp::VerifyCheckSum {
                path: p.clone(),
                size,
                crc32: src_hash,
            });
        }

        // Copy files that differ
        let files_in_both = src.files.intersection(&dst.files);
        'copy_different: for p in files_in_both.into_iter() {
            let src_file_path = src_path.join(p);
            let dst_file_path = dst_path.join(p);

            let src_size;

            'diff: {
                let dst_metadata = dst_file_path.metadata()?;
                let src_metadata = src_file_path.metadata()?;

                src_size = src_metadata.len();
                let dst_size = dst_metadata.len();

                if src_size != dst_size {
                    break 'diff;
                }

                let src_modified = FileTime::from_last_modification_time(&src_metadata);
                let dst_modified = FileTime::from_last_modification_time(&dst_metadata);

                if src_modified != dst_modified {
                    break 'diff;
                }

                // No differences found, skip to next file
                continue 'copy_different;
            }

            ui.begin_file("Checksum", &p.to_string_lossy(), src_size);

            let src_hash = hash_crc32(&src_file_path, |bytes| ui.file_progress(bytes as u64))?;

            ui.end_file();

            ops.push(SyncOp::Copy { path: p.clone() });
            post_ops.push(SyncOp::VerifyCheckSum {
                path: p.clone(),
                size: src_size,
                crc32: src_hash,
            });
        }

        // Delete files not in source
        let files_not_in_src = dst.files.difference(&src.files);
        ops.extend(files_not_in_src.map(|p| SyncOp::Delete { path: p.clone() }));

        // Delete dirs not in source
        let mut dirs_not_in_src: Vec<_> = dst.dirs.difference(&src.dirs).collect();
        dirs_not_in_src.sort_unstable_by_key(|p| std::cmp::Reverse(p.components().count()));

        ops.extend(
            dirs_not_in_src
                .into_iter()
                .map(|p| SyncOp::RemoveDir { path: p.clone() }),
        );

        // Add post-ops to the end
        ops.extend(post_ops);

        ui.end_prepare();

        Ok(SyncJob {
            src_path,
            dst_path,
            ops,
        })
    }
}

impl SyncJob {
    pub fn execute(self, ui: &mut dyn SyncUiHandler) -> Result<(), SyncJobError> {
        let src_path = self.src_path;
        let dst_path = self.dst_path;

        ui.begin_sync(self.ops.len());

        for op in self.ops {
            match op {
                SyncOp::Copy { path } => {
                    let src_file_path = src_path.join(&path);
                    let dst_file_path = dst_path.join(&path);

                    let Ok(src_metadata) = src_file_path.metadata() else {
                        error!("Could not get metadata for source file: {}", src_file_path.display());
                        return Err(SyncJobError::ReadError { path });
                    };

                    let src_modified = FileTime::from_last_modification_time(&src_metadata);

                    let size = src_metadata.len();
                    ui.begin_file("Copy", &path.to_string_lossy(), size);

                    let res = fs::copy(&src_file_path, &dst_file_path);
                    match res {
                        Ok(_) => {}
                        Err(err) => match err.kind() {
                            ErrorKind::NotFound => return Err(SyncJobError::FileNotFound { path }),
                            _ => return Err(SyncJobError::Anyhow(err.into())),
                        },
                    }

                    ui.file_progress(size);

                    filetime::set_file_mtime(&dst_file_path, src_modified)
                        .map_err(|e| SyncJobError::Anyhow(e.into()))?;

                    ui.end_file();
                }
                SyncOp::CreateDir { path } => {
                    fs::create_dir_all(dst_path.join(path)).map_err(|e| SyncJobError::Anyhow(e.into()))?;
                }
                SyncOp::Delete { path } => {
                    fs::remove_file(dst_path.join(path)).map_err(|e| SyncJobError::Anyhow(e.into()))?;
                }
                SyncOp::RemoveDir { path } => {
                    fs::remove_dir(dst_path.join(path)).map_err(|e| SyncJobError::Anyhow(e.into()))?;
                }
                SyncOp::VerifyCheckSum { path, size, crc32 } => {
                    let dst_file_path = dst_path.join(&path);

                    ui.begin_file("Verify", &path.to_string_lossy(), size);

                    let dst_hash = hash_crc32(&dst_file_path, |bytes| ui.file_progress(bytes as u64))?;

                    ui.end_file();

                    if dst_hash != crc32 {
                        return Err(SyncJobError::ChecksumMismatch);
                    }
                }
            }

            ui.sync_progress();
        }

        ui.end_sync();

        Ok(())
    }
}

pub fn sync_dir(
    src: &Path,
    dst: &Path,
    ignore_globset: &globset::GlobSet,
    ui: &mut dyn SyncUiHandler,
) -> Result<(), anyhow::Error> {
    // Create destination directory if it does not exist
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    let mut attempt = 0;

    loop {
        let src = SyncDir::new(src, ignore_globset, ui)?;
        let dst = SyncDir::new(dst, &globset::GlobSet::empty(), ui)?;
        let job = dst.sync_from(&src, ui)?;

        let res = job.execute(ui);
        match res {
            Ok(_) => {}
            Err(err) => {
                attempt += 1;

                if attempt > 3 {
                    return Err(err.into());
                }

                match err {
                    SyncJobError::ChecksumMismatch => error!("Checksum mismatch, re-running sync job..."),
                    SyncJobError::FileNotFound { path } => error!("File not found in source: {}", path.display()),
                    SyncJobError::ReadError { path } => error!("Error reading source file: {}", path.display()),
                    _ => Err(err)?,
                }

                continue;
            }
        };

        break;
    }

    Ok(())
}

pub fn sync_file(src_file_path: &Path, dst: &Path, ui: &mut dyn SyncUiHandler) -> Result<(), anyhow::Error> {
    let src_dir_path = src_file_path
        .parent()
        .context("Error getting parent directory of source file")?;
    let rel_file_path = src_file_path.strip_prefix(src_dir_path)?;
    let dst_file_path = dst.join(rel_file_path);

    // Create destination directory if it does not exist
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    let mut attempt = 0;

    loop {
        let src_metadata = src_file_path.metadata()?;
        let src_size = src_metadata.len();

        ui.begin_file("Checksum", &rel_file_path.to_string_lossy(), src_size);

        let src_hash = hash_crc32(src_file_path, |bytes| ui.file_progress(bytes as u64))?;

        ui.end_file();

        if dst_file_path.exists() {
            'diff: {
                let dst_metadata = dst_file_path.metadata()?;
                let dst_size = dst_metadata.len();

                if src_size != dst_size {
                    break 'diff;
                }

                let src_modified = FileTime::from_last_modification_time(&src_metadata);
                let dst_modified = FileTime::from_last_modification_time(&dst_metadata);

                if src_modified != dst_modified {
                    break 'diff;
                }

                // No differences found
                return Ok(());
            }
        }

        let job = SyncJob {
            ops: vec![
                SyncOp::Copy {
                    path: rel_file_path.to_path_buf(),
                },
                SyncOp::VerifyCheckSum {
                    path: rel_file_path.to_path_buf(),
                    size: src_size,
                    crc32: src_hash,
                },
            ],
            src_path: src_dir_path.to_path_buf(),
            dst_path: dst.to_path_buf(),
        };

        let res = job.execute(ui);
        match res {
            Ok(_) => {}
            Err(err) => {
                attempt += 1;

                if attempt > 3 {
                    return Err(err.into());
                }

                match err {
                    SyncJobError::ChecksumMismatch => error!("Checksum mismatch, re-running sync job..."),
                    SyncJobError::FileNotFound { path } => error!("File not found in source: {}", path.display()),
                    SyncJobError::ReadError { path } => error!("Error reading source file: {}", path.display()),
                    _ => Err(err)?,
                }

                continue;
            }
        };

        break;
    }

    Ok(())
}
