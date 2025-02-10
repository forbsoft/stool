use std::{
    env,
    process::Stdio,
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

const STOOL_PASSTHROUGH_PREFIX: &str = "STOOL_PASSTHROUGH_";
const WAIT_SLEEP_DURATION: Duration = Duration::from_secs(1);

pub fn rungame(engine_args: EngineArgs, game_command: Vec<String>) -> Result<(), anyhow::Error> {
    // Shutdown signal
    let shutdown = Arc::new(AtomicBool::new(false));

    // Set break (Ctrl-C) handler.
    ctrlc::set_handler({
        let shutdown = shutdown.clone();

        move || {
            info!("Shutdown requested by user.");
            shutdown.store(true, Ordering::Release);
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

        let mut env_vars: Vec<_> = env::vars_os().collect();

        // Process pass-through variables
        let passthrough_env_vars: Vec<_> = env_vars
            .iter()
            .filter_map(|(k, v)| {
                k.to_str()
                    .and_then(|k| k.strip_prefix(STOOL_PASSTHROUGH_PREFIX))
                    .map(|k| (k.to_owned(), v.to_owned()))
            })
            .collect();

        // Remove pass-through prefixed variables from the main
        // environment variables
        env_vars.retain(|(k, _)| {
            k.to_str()
                .map(|k| !k.starts_with(STOOL_PASSTHROUGH_PREFIX))
                .unwrap_or(true)
        });

        std::thread::spawn(move || -> Result<(), anyhow::Error> {
            let (program, args) = game_command.split_first().context("Couldn't split game command")?;

            // Run game
            let result = std::process::Command::new(program)
                .args(args)
                .env_clear()
                .envs(env_vars)
                .envs(passthrough_env_vars)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            shutdown.store(true, Ordering::Release);

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
