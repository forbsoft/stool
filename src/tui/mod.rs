mod app;
mod create_backup_view;
mod log_widget;
mod menu_view;
mod restore_backup_view;
mod state;
mod style;
mod uihandler;

use std::sync::{atomic::AtomicBool, Arc, Mutex};

pub use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
pub use uihandler::TuiUiHandler;

use crate::engine::{Engine, EngineArgs};

use self::app::App;

pub fn run(engine: Engine, app_state: Arc<Mutex<AppState>>, shutdown: Arc<AtomicBool>) -> Result<(), anyhow::Error> {
    let backup_path = {
        let EngineArgs { name, data_path, .. } = engine.args();

        let output_path = data_path.join(name);
        output_path.join("backups")
    };

    tui_logger::init_logger(tui_logger::LevelFilter::Debug)?;
    tui_logger::set_default_level(tui_logger::LevelFilter::Info);

    tracing_subscriber::registry()
        .with(tui_logger::tracing_subscriber_layer())
        .init();

    let terminal = ratatui::init();
    let result = App::new(app_state, engine, backup_path, shutdown).run(terminal);
    ratatui::restore();
    result?;

    Ok(())
}
