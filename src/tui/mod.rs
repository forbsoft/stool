mod app;
mod create_backup_view;
mod log_widget;
mod menu_view;
mod restore_backup_view;
mod state;
mod style;
mod uihandler;

use std::sync::{atomic::AtomicBool, Arc, Mutex};

use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uihandler::TuiUiHandler;

use crate::engine::{self, EngineArgs};

use self::app::App;

pub fn run(engine_args: EngineArgs, shutdown: Arc<AtomicBool>) -> Result<(), anyhow::Error> {
    let backup_path = {
        let EngineArgs { name, data_path, .. } = &engine_args;

        let output_path = data_path.join(name);
        output_path.join("backups")
    };

    let app_state = Arc::new(Mutex::new(AppState::default()));
    let ui = TuiUiHandler::new(app_state.clone());

    let engine_control = engine::run(engine_args, shutdown.clone(), ui)?;

    tui_logger::init_logger(tui_logger::LevelFilter::Debug)?;
    tui_logger::set_default_level(tui_logger::LevelFilter::Info);

    tracing_subscriber::registry()
        .with(tui_logger::tracing_subscriber_layer())
        .init();

    let terminal = ratatui::init();
    let result = App::new(app_state, engine_control, backup_path, shutdown).run(terminal);
    ratatui::restore();
    result?;

    Ok(())
}
