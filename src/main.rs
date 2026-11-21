#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

use anyhow::{Context, Result};
use bop_merge::app;
use clap::{Parser, Subcommand};

const CLIENT_ID_ENV: &str = "BOPMERGE_CLIENT_ID";
const TARGET_PLAYLIST_ID_ENV: &str = "BOPMERGE_TARGET_PLAYLIST_ID";

/// Mirror every Spotify playlist, named as "YYYY Bops", into a single merged playlist.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, PartialEq, Subcommand)]
enum Commands {
    /// Login to Spotify & retrieve an access token.
    Login,
    /// Mirror the source playlists into the target playlist.
    Sync {
        /// Show the planned sync without applying.
        #[arg(long)]
        dry_run: bool,
        /// Execute sync even if the safety guard would refuse.
        #[arg(long)]
        force: bool,
    },
}

fn fetch_env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} not set"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    dotenvy::dotenv().ok();

    let client_id = fetch_env_var(CLIENT_ID_ENV)?;

    // Temporarily login here until token storage is implemented
    let (spotify_client, user) = app::login(&client_id)?;

    match cli.command {
        Commands::Login => {
            println!("(token storage not yet implemented)");
            Ok(())
        }
        Commands::Sync { dry_run, force } => {
            let target_id = fetch_env_var(TARGET_PLAYLIST_ID_ENV)?;
            app::run_sync(
                &spotify_client,
                &user,
                &target_id,
                app::SyncOptions { dry_run, force },
            )
        }
    }
}
