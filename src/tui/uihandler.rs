use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::info;

use crate::{engine::ui::StoolUiHandler, internal::sync::SyncUiHandler};

use super::state::{Action, ActionKind, AppState, Progress};

pub struct TuiUiHandler {
    state: Arc<Mutex<AppState>>,

    backup_estimate: Option<Duration>,
    restore_estimate: Option<Duration>,
}

impl TuiUiHandler {
    pub fn new(state: Arc<Mutex<AppState>>) -> Self {
        Self {
            state,
            backup_estimate: None,
            restore_estimate: None,
        }
    }
}

impl StoolUiHandler for TuiUiHandler {
    fn clear(self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn begin_backup(&mut self, name: &str) {
        let now = Instant::now();

        let name = name.to_owned();
        let mut action = Action::new(ActionKind::CreateBackup { name });

        action.progress = self
            .backup_estimate
            .map(|est| Progress::Estimate {
                start: now,
                end: now + est,
            })
            .unwrap_or_default();

        let mut state = self.state.lock().unwrap();
        state.current_action = Some(action);
    }

    fn end_backup(&mut self, success: bool) {
        let now = Instant::now();

        let mut state = self.state.lock().unwrap();

        let Some(action) = state.current_action.take() else {
            return;
        };

        self.backup_estimate = Some(now - action.started_at);

        let msg = if success {
            action.kind.describe_complete()
        } else {
            action.kind.describe_error()
        };

        info!("{}", msg);
    }

    fn begin_staging(&mut self, _count: usize) {}

    fn begin_stage(&mut self, _name: &str) {}

    fn end_stage(&mut self) {}

    fn end_staging(&mut self) {}

    fn begin_compress(&mut self) {}

    fn end_compress(&mut self) {}

    fn begin_restore(&mut self, name: &str) {
        let now = Instant::now();

        let name = name.to_owned();
        let mut action = Action::new(ActionKind::RestoreBackup { name });

        action.progress = self
            .restore_estimate
            .map(|est| Progress::Estimate {
                start: now,
                end: now + est,
            })
            .unwrap_or_default();

        let mut state = self.state.lock().unwrap();
        state.current_action = Some(action);
    }

    fn end_restore(&mut self, success: bool) {
        let now = Instant::now();

        let mut state = self.state.lock().unwrap();

        let Some(action) = state.current_action.take() else {
            return;
        };

        self.restore_estimate = Some(now - action.started_at);

        let msg = if success {
            action.kind.describe_complete()
        } else {
            action.kind.describe_error()
        };

        info!("{}", msg);
    }

    fn begin_extract(&mut self) {}

    fn end_extract(&mut self) {}

    fn begin_restore_sp(&mut self, _name: &str) {}

    fn end_restore_sp(&mut self) {}
}

impl SyncUiHandler for TuiUiHandler {
    fn begin_scan(&mut self) {}

    fn end_scan(&mut self) {}

    fn begin_prepare(&mut self) {}

    fn end_prepare(&mut self) {}

    fn begin_sync(&mut self, _op_count: usize) {}

    fn sync_progress(&mut self) {}

    fn end_sync(&mut self) {}

    fn begin_file(&mut self, _prefix: &str, _filename: &str, _size: u64) {}

    fn file_progress(&mut self, _bytes: u64) {}

    fn end_file(&mut self) {}
}
