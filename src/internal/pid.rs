use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sysinfo::{Pid, ProcessRefreshKind};
use tracing::{debug, error};

pub struct PidLock {
    path: PathBuf,
}

impl PidLock {
    pub fn acquire(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref().to_path_buf();

        debug!("Trying to acquire PID lock at {}", path.display());

        if path.exists() {
            debug!("PID file found at {:?}", path);
            if let Ok(mut file) = fs::File::open(&path) {
                let mut pid = String::new();

                // Try to read content of PID-lock file into a string.
                if let Err(err) = file.read_to_string(&mut pid) {
                    error!("Could not read PID-lock file: {}", err.to_string());
                    return None;
                }

                if let Ok(pid) = pid.parse::<Pid>() {
                    debug!("File contains PID {}.", pid);
                    if process_exists(pid) {
                        // Process already exists, cannot get lock.
                        debug!("Process with PID {} exists, cannot get lock.", pid);
                        return None;
                    }
                }
            }
        }

        // Try to create a PID file...
        if let Ok(mut file) = fs::File::create(&path) {
            // Write our PID to the newly created file.
            if let Err(err) = file.write(format!("{}", std::process::id()).as_bytes()) {
                error!("Could not write PID-lock file: {}", err.to_string());
                return None;
            }
        } else {
            error!("Could not create PID-lock file!");
            return None;
        };

        Some(Self { path })
    }
}

impl Drop for PidLock {
    fn drop(&mut self) {
        debug!("Dropping PID-lock at {}", self.path.display());
        fs::remove_file(&self.path).expect("Could not remove PID-lock file!");
    }
}

fn process_exists(pid: Pid) -> bool {
    use sysinfo::{RefreshKind, System};

    let sys = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()));

    sys.process(pid).is_some()
}
