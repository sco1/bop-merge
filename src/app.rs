use anyhow::{Context, Result};
use reqwest::blocking::Client;

use crate::{
    auth,
    spotify::{SpotifyClient, User},
    sync,
};

pub fn login(client_id: &str) -> Result<(SpotifyClient, User)> {
    let http = Client::new();
    let tokens = auth::login(&http, client_id)?;
    let spotify_client = SpotifyClient::new(http, tokens.access_token().clone());
    let user = spotify_client.me()?;
    println!(
        "logged in as {}",
        user.display_name.as_deref().unwrap_or(&user.id),
    );

    Ok((spotify_client, user))
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
