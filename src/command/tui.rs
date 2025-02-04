use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use tracing::{error, info};

use crate::{
    engine::{self, EngineArgs},
    tui::{AppState, TuiUiHandler},
};

pub fn tui(engine_args: EngineArgs) -> Result<(), anyhow::Error> {
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

    crate::tui::run(engine, app_state, shutdown)?;

    Ok(())
}
