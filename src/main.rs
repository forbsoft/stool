mod command;
mod config;
mod engine;
mod internal;
mod ui;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Debug, Parser)]
#[clap(name = "stool", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"))]
struct Opt {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
enum Command {
    #[clap(about = "Run stool interactively")]
    Interactive {
        #[clap(help = "Game name")]
        name: String,
    },
    #[clap(about = "Create a new game config")]
    New,
    #[clap(about = "Run game via stool")]
    RunGame {
        #[clap(help = "Game name")]
        name: String,

        #[clap(help = "Game command")]
        game_command: Vec<String>,
    },
    #[clap(about = "Run stool in TUI mode")]
    Tui {
        #[clap(help = "Game name")]
        name: String,
    },
}

fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    // Initialize logging
    initialize_logging();

    let config_path = self::config::main::get_default_config_path().context("Getting default config path")?;
    let game_config_path = config_path.join("games");

    let config = self::config::main::MainConfig::load_or_write_default_from_location(&config_path)?;

    match opt.command {
        Command::Interactive { name } => command::interactive(&name, &game_config_path, &config.data_path),
        Command::New => command::new(&game_config_path),
        Command::RunGame { name, game_command } => {
            command::rungame(&name, &game_config_path, &config.data_path, game_command)
        }
        Command::Tui { name } => command::tui(&name, &game_config_path, &config.data_path),
    }?;

    Ok(())
}

fn initialize_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Setting default tracing subscriber failed!");
}
