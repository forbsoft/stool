use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::Context;
use tracing::{error, info};

use crate::{
    engine::{self, EngineArgs, EngineState},
    tui::{AppState, TuiUiHandler},
};

const WAIT_SLEEP_DURATION: Duration = Duration::from_secs(1);

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

    let app_state = Arc::new(Mutex::new(AppState::default()));
    let ui = TuiUiHandler::new(app_state.clone());

    let engine = engine::run(engine_args, shutdown.clone(), ui)?;
    let engine_control = engine.control();

    // Wait for engine to start up
    while engine_control.state() != EngineState::Running {
        std::thread::sleep(WAIT_SLEEP_DURATION);
    }

    // Run game
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

    // Run TUI
    crate::tui::run(engine, app_state, shutdown)?;

    // Wait for run game thread to finish
    game_join_handle.join().unwrap()?;

    Ok(())
}
