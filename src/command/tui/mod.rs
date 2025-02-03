mod app;
mod create_backup_view;
mod log_widget;
mod menu_view;
mod restore_backup_view;
mod state;
mod style;
mod uihandler;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uihandler::TuiUiHandler;

use crate::engine;

use self::app::App;

pub fn tui(name: &str, game_config_path: &Path, data_path: &Path) -> Result<(), anyhow::Error> {
    let output_path = data_path.join(name);
    let backup_path = output_path.join("backups");

    let app_state = Arc::new(Mutex::new(AppState::default()));
    let ui = TuiUiHandler::new(app_state.clone());

    // Cancellation boolean.
    let cancel = Arc::new(AtomicBool::new(false));

    let (engine_join_handle, backup_tx) = engine::run(name, game_config_path, data_path, cancel.clone(), ui)?;

    // TUI START

    tui_logger::init_logger(tui_logger::LevelFilter::Debug)?;
    tui_logger::set_default_level(tui_logger::LevelFilter::Info);

    tracing_subscriber::registry()
        .with(tui_logger::tracing_subscriber_layer())
        .init();

    let terminal = ratatui::init();
    let result = App::new(app_state, backup_tx.clone(), backup_path).run(terminal);
    ratatui::restore();
    result?;

    // TUI END

    cancel.store(true, Ordering::SeqCst);

    drop(backup_tx);

    // Wait for engine to shut down gracefully
    engine_join_handle.join().unwrap();

    Ok(())
}
