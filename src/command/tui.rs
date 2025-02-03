use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tracing::{error, info};

pub fn tui(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    // Shutdown signal
    let shutdown = Arc::new(AtomicBool::new(false));

    // Set break (Ctrl-C) handler.
    ctrlc::set_handler({
        let shutdown = shutdown.clone();

        move || {
            info!("Shutdown requested by user.");
            shutdown.store(true, Ordering::SeqCst);
        }
    })
    .unwrap_or_else(|err| error!("Error setting Ctrl-C handler: {}", err));

    let autobackup = Arc::new(AtomicBool::new(true));

    crate::tui::run(name, game_config_path, data_path, autobackup, shutdown)?;

    Ok(())
}
