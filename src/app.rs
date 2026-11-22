#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;

use crate::{
    auth::{self, TokenStore},
    spotify::{SpotifyClient, User},
    sync,
};

fn print_login(user: &User) {
    println!(
        "logged in as {}",
        user.display_name.as_deref().unwrap_or(&user.id),
    );
}

/// Log in through the browser and save the refresh token for later `sync` runs.
pub fn login(client_id: &str, store: &dyn TokenStore) -> Result<()> {
    let http = Client::new();
    let tokens = auth::login(&http, client_id)?;
    let refresh_token = tokens
        .refresh_token()
        .context("token response had no refresh token")?;
    store.save(refresh_token)?;

    let user = SpotifyClient::new(http, tokens.access_token().clone()).me()?;
    print_login(&user);
    println!("login saved");

    Ok(())
}

/// Connect using the saved refresh token.
pub fn connect(client_id: &str, store: &dyn TokenStore) -> Result<(SpotifyClient, User)> {
    let Some(refresh_token) = store.load()? else {
        bail!("not logged in; run `bop-merge login`");
    };

    let http = Client::new();
    let tokens = match auth::refresh(&http, client_id, &refresh_token) {
        Ok(tokens) => tokens,
        Err(e) if e.downcast_ref::<auth::InvalidGrant>().is_some() => {
            if let Err(clear_err) = store.clear() {
                eprintln!("warning: {clear_err:#}");
            }
            return Err(e.context("saved login expired or was revoked; run `bop-merge login`"));
        }
        Err(e) => return Err(e),
    };

    // Save a rotated refresh token
    if let Some(new_refresh_token) = tokens.refresh_token() {
        store.save(new_refresh_token)?;
    }

    let client = SpotifyClient::new(http, tokens.access_token().clone());
    let user = client.me()?;
    print_login(&user);

    Ok((client, user))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    pub dry_run: bool,
    pub force: bool,
}

pub fn run_sync(
    client: &SpotifyClient,
    user: &User,
    target_id: &str,
    opts: SyncOptions,
) -> Result<()> {
    let (target_playlist, rest) =
        sync::take_target_playlist(client.my_playlists()?, target_id, &user.id)?;
    println!("found target playlist '{}'", target_playlist.name);

    let existing_merged_items = client
        .playlist_items(&target_playlist.id)
        .with_context(|| format!("could not fetch tracks from '{}'", target_playlist.name))?;
    let existing_merged_tracks = sync::track_uris(existing_merged_items);

    let sources = sync::select_sources(rest, &user.id);
    println!("found {} source playlists", sources.len());

    let mut all_tracks = Vec::with_capacity(sources.len());
    for (_, playlist) in &sources {
        let items = client
            .playlist_items(&playlist.id)
            .with_context(|| format!("could not fetch tracks from '{}'", playlist.name))?;

        let tracks = sync::track_uris(items);
        println!(
            "{}: {} tracks ({} skipped)",
            playlist.name,
            tracks.uris.len(),
            tracks.n_skipped,
        );
        all_tracks.push(tracks);
    }

    let plan = sync::plan(&all_tracks, &existing_merged_tracks);
    if plan.add.is_empty() && plan.remove.is_empty() {
        println!("already in sync");
        return Ok(());
    }
    println!(
        "sync plan: {} to add, {} to remove",
        plan.add.len(),
        plan.remove.len()
    );

    if opts.dry_run {
        if let Err(e) = sync::check_plan(&plan, opts.force) {
            eprintln!("warning: {e:#}");
        }
        return Ok(());
    }

    sync::check_plan(&plan, opts.force)?;
    client
        .remove_items(&target_playlist.id, &plan.remove)
        .with_context(|| format!("could not remove tracks from '{}'", target_playlist.name))?;
    println!("removed {} tracks", plan.remove.len());
    client
        .add_items(&target_playlist.id, &plan.add)
        .with_context(|| format!("could not add tracks to '{}'", target_playlist.name))?;
    println!("added {} tracks", plan.add.len());

    Ok(())
}
