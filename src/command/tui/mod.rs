mod app;
mod create_backup_view;
mod status_view;

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
    ui::TuiUiHandler,
};

use self::app::App;

pub fn tui(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    let output_path = data_path.join(name);
    let backup_path = output_path.join("backups");

    let ui = TuiUiHandler::new();

    // Cancellation boolean.
    let cancel = Arc::new(AtomicBool::new(false));

    //let (engine_join_handle, backup_tx) = engine::run(name, game_config_path, data_path, cancel.clone(), ui)?;

    // TUI START

    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    result.unwrap();

    // TUI END

    cancel.store(true, Ordering::SeqCst);

    //drop(backup_tx);

    // Wait for engine to shut down gracefully
    //engine_join_handle.join().unwrap();

    Ok(())
}
