use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tracing::{error, info};

use crate::engine::EngineArgs;

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

    crate::tui::run(engine_args, shutdown)?;

    Ok(())
}
