use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

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
    VerifyCheckSum { path: PathBuf, crc32: u32 },
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

impl SyncDir {
    pub fn new(path: &Path) -> Result<Self, anyhow::Error> {
        let path = path.canonicalize()?;
        let mut dirs: HashSet<PathBuf> = HashSet::new();
        let mut files: HashSet<PathBuf> = HashSet::new();

        let entries = walkdir::WalkDir::new(&path).into_iter().filter_map(Result::ok);

        for entry in entries {
            let is_file = entry.file_type().is_file();
            let rel_path = entry.into_path().strip_prefix(&path)?.to_path_buf();

            if !is_file {
                dirs.insert(rel_path);
                continue;
            }

            files.insert(rel_path);
        }

        Ok(Self { path, dirs, files })
    }

    pub fn sync_from(&self, other: &Self) -> Result<SyncJob, anyhow::Error> {
        let src = other;
        let dst = self;

        let src_path = src.path.clone();
        let dst_path = dst.path.clone();

        let item_count = src.dirs.len() + src.files.len();
        let mut ops: Vec<SyncOp> = Vec::with_capacity(item_count);
        let mut post_ops: Vec<SyncOp> = Vec::with_capacity(item_count);

        // Create dirs not in destination
        let dirs_not_in_dst = src.dirs.difference(&self.dirs);
        ops.extend(dirs_not_in_dst.map(|p| SyncOp::CreateDir { path: p.clone() }));

        // Copy files not in destination
        let files_not_in_dst = src.files.difference(&self.files);
        //ops.extend(files_not_in_dst.map(|p| SyncOp::Copy { path: p.clone() }));
        for p in files_not_in_dst {
            let src_file_path = src_path.join(p);

            let src_hash = hash_crc32(&src_file_path)?;

            ops.push(SyncOp::Copy { path: p.clone() });
            post_ops.push(SyncOp::VerifyCheckSum {
                path: p.clone(),
                crc32: src_hash,
            });
        }

        // Copy files that differ
        let files_in_both = src.files.intersection(&dst.files);
        'copy_different: for p in files_in_both.into_iter() {
            let src_file_path = src_path.join(p);
            let dst_file_path = dst_path.join(p);

            'diff: {
                let dst_metadata = dst_file_path.metadata()?;
                let src_metadata = src_file_path.metadata()?;

                if src_metadata.len() != dst_metadata.len() {
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

            let src_hash = hash_crc32(&src_file_path)?;

            ops.push(SyncOp::Copy { path: p.clone() });
            post_ops.push(SyncOp::VerifyCheckSum {
                path: p.clone(),
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

        Ok(SyncJob {
            src_path,
            dst_path,
            ops,
        })
    }
}

impl SyncJob {
    pub fn execute(self) -> Result<(), SyncJobError> {
        let src_path = self.src_path;
        let dst_path = self.dst_path;

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

                    let res = fs::copy(&src_file_path, &dst_file_path);
                    match res {
                        Ok(_) => {}
                        Err(err) => match err.kind() {
                            ErrorKind::NotFound => return Err(SyncJobError::FileNotFound { path }),
                            _ => return Err(SyncJobError::Anyhow(err.into())),
                        },
                    }

                    filetime::set_file_mtime(&dst_file_path, src_modified)
                        .map_err(|e| SyncJobError::Anyhow(e.into()))?;
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
                SyncOp::VerifyCheckSum { path, crc32 } => {
                    let dst_file_path = dst_path.join(&path);

                    let dst_hash = hash_crc32(&dst_file_path)?;

                    if dst_hash != crc32 {
                        return Err(SyncJobError::ChecksumMismatch);
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn sync(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    // Create destination directory if it does not exist
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    let mut attempt = 0;

    loop {
        let src = SyncDir::new(src)?;
        let dst = SyncDir::new(dst)?;
        let job = dst.sync_from(&src)?;

        let res = job.execute();
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
