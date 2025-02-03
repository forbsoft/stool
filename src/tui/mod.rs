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
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uihandler::TuiUiHandler;

use crate::engine;

use self::app::App;

pub fn run(
    name: &str,
    game_config_path: &Path,
    data_path: &Path,
    shutdown: Arc<AtomicBool>,
) -> Result<(), anyhow::Error> {
    let output_path = data_path.join(name);
    let backup_path = output_path.join("backups");

    let app_state = Arc::new(Mutex::new(AppState::default()));
    let ui = TuiUiHandler::new(app_state.clone());

    let (engine_join_handle, backup_tx) = engine::run(name, game_config_path, data_path, shutdown.clone(), ui)?;

    tui_logger::init_logger(tui_logger::LevelFilter::Debug)?;
    tui_logger::set_default_level(tui_logger::LevelFilter::Info);

    tracing_subscriber::registry()
        .with(tui_logger::tracing_subscriber_layer())
        .init();

    let terminal = ratatui::init();
    let result = App::new(app_state, backup_tx, backup_path, shutdown.clone(), engine_join_handle).run(terminal);
    ratatui::restore();
    result?;

    Ok(())
}
