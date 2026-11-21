use clap::{CommandFactory, error::ErrorKind};

use super::*;

/// Parse `args` as if typed after the binary name.
fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
    Cli::try_parse_from(std::iter::once("bop-merge").chain(args.iter().copied())).map(|c| c.command)
}

fn parse_err(args: &[&str]) -> ErrorKind {
    parse(args).unwrap_err().kind()
}

#[test]
fn cli_definition_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn login_parses() {
    assert_eq!(parse(&["login"]).unwrap(), Commands::Login);
}

#[test]
fn sync_flags_default_off() {
    assert_eq!(
        parse(&["sync"]).unwrap(),
        Commands::Sync {
            dry_run: false,
            force: false,
        }
    );
}

#[test]
fn sync_dry_run() {
    assert_eq!(
        parse(&["sync", "--dry-run"]).unwrap(),
        Commands::Sync {
            dry_run: true,
            force: false,
        }
    );
}

#[test]
fn sync_force() {
    assert_eq!(
        parse(&["sync", "--force"]).unwrap(),
        Commands::Sync {
            dry_run: false,
            force: true,
        }
    );
}

#[test]
fn sync_both_flags_in_any_order() {
    let expected = Commands::Sync {
        dry_run: true,
        force: true,
    };
    assert_eq!(parse(&["sync", "--dry-run", "--force"]).unwrap(), expected);
    assert_eq!(parse(&["sync", "--force", "--dry-run"]).unwrap(), expected);
}

#[test]
fn missing_subcommand_shows_help() {
    assert_eq!(
        parse_err(&[]),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
}

#[test]
fn unknown_subcommand_is_rejected() {
    assert_eq!(parse_err(&["merge"]), ErrorKind::InvalidSubcommand);
}

#[test]
fn unknown_flag_is_rejected() {
    assert_eq!(parse_err(&["sync", "--bogus"]), ErrorKind::UnknownArgument);
}

#[test]
fn sync_flags_are_not_accepted_by_login() {
    assert_eq!(
        parse_err(&["login", "--dry-run"]),
        ErrorKind::UnknownArgument
    );
}

#[test]
fn flags_must_follow_the_subcommand() {
    // `--dry-run` belongs to `sync`, not to the top-level command.
    assert_eq!(
        parse_err(&["--dry-run", "sync"]),
        ErrorKind::UnknownArgument
    );
}
