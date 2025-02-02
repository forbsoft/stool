use tracing::{error, info};

use crate::{engine::ui::StoolUiHandler, internal::sync::SyncUiHandler};

#[derive(Default)]
pub struct TuiUiHandler {
    backup_name: Option<String>,
}

impl TuiUiHandler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StoolUiHandler for TuiUiHandler {
    fn clear(self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn begin_backup(&mut self, name: &str) {
        info!("Creating backup {name}...");

        self.backup_name = Some(name.into());
    }

    fn end_backup(&mut self, success: bool) {
        let name = self.backup_name.take().unwrap_or_default();

        if success {
            info!("Backup created: {name}");
        } else {
            error!("Backup failed: {name}");
        }
    }

    fn begin_staging(&mut self, count: usize) {}

    fn begin_stage(&mut self, name: &str) {}

    fn end_stage(&mut self) {}

    fn end_staging(&mut self) {}

    fn begin_compress(&mut self) {}

    fn end_compress(&mut self) {}

    fn begin_restore(&mut self, name: &str) {
        info!("Restoring backup {name}...");

        self.backup_name = Some(name.into());
    }

    fn end_restore(&mut self, success: bool) {
        let name = self.backup_name.take().unwrap_or_default();

        if success {
            info!("Backup restored: {name}");
        } else {
            error!("Restore backup failed: {name}");
        }
    }

    fn begin_extract(&mut self) {}

    fn end_extract(&mut self) {}

    fn begin_restore_sp(&mut self, name: &str) {}

    fn end_restore_sp(&mut self) {}
}

impl SyncUiHandler for TuiUiHandler {
    fn begin_scan(&mut self) {}

    fn end_scan(&mut self) {}

    fn begin_prepare(&mut self) {}

    fn end_prepare(&mut self) {}

    fn begin_sync(&mut self, op_count: usize) {}

    fn sync_progress(&mut self) {}

    fn end_sync(&mut self) {}

    fn begin_file(&mut self, prefix: &str, filename: &str, size: u64) {}

    fn file_progress(&mut self, bytes: u64) {}

    fn end_file(&mut self) {}
}
