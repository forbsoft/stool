use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Context;
use tracing::{error, info};

use crate::engine::EngineArgs;

pub fn rungame(engine_args: EngineArgs, game_command: Vec<String>) -> Result<(), anyhow::Error> {
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

    let game_join_handle = {
        let shutdown = shutdown.clone();

        std::thread::spawn(move || -> Result<(), anyhow::Error> {
            let (program, args) = game_command.split_first().context("Couldn't split game command")?;

            // Run game
            let result = std::process::Command::new(program).args(args).status();

            shutdown.store(true, Ordering::SeqCst);

            result?;
            Ok(())
        })
    };

    crate::tui::run(engine_args, shutdown)?;

    game_join_handle.join().unwrap()?;

    Ok(())
}
