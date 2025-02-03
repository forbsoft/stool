use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

pub fn tui(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    // Shutdown signal
    let shutdown = Arc::new(AtomicBool::new(false));

    crate::tui::run(name, game_config_path, data_path, shutdown)?;

    Ok(())
}
