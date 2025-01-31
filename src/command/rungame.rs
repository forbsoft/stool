use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Context;

use crate::{engine, ui::FancyUiHandler};

pub fn rungame(
    name: &str,
    game_config_path: &Path,
    data_path: &Path,
    game_command: Vec<String>,
) -> Result<(), anyhow::Error> {
    let ui = FancyUiHandler::new();

    // Cancellation boolean.
    let cancel = Arc::new(AtomicBool::new(false));

    let (engine_join_handle, backup_tx) = engine::run(name, game_config_path, data_path, cancel.clone(), ui)?;

    let (program, args) = game_command.split_first().context("Couldn't split game command")?;

    // Run game
    std::process::Command::new(program).args(args).status()?;

    cancel.store(true, Ordering::SeqCst);
    drop(backup_tx);

    // Wait for engine to shut down gracefully
    engine_join_handle.join().unwrap();

    Ok(())
}
