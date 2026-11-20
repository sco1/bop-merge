use std::collections::HashSet;

use anyhow::{Context, Result};
use bop_merge::{auth, spotify, sync};
use reqwest::blocking::Client;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let client_id = std::env::var("BOPMERGE_CLIENT_ID").context("BOPMERGE_CLIENT_ID not set")?;

    let pkce = auth::Pkce::generate();
    let state = auth::generate_state();
    let listener = auth::bind_callback()?;
    println!("{}", auth::authorize_url(&client_id, &pkce, &state));

    let code = auth::wait_for_callback(&listener, &state)?;
    let http = Client::new();
    let tokens = auth::exchange_code(&http, &client_id, &code, &pkce)?;

    let spotify_client = spotify::SpotifyClient::new(http, tokens.access_token().clone());
    let user = spotify_client.me()?;
    println!(
        "logged in as {}",
        user.display_name.as_deref().unwrap_or(&user.id),
    );

    let sources = sync::select_sources(spotify_client.my_playlists()?, &user.id);
    println!("found {} source playlists", sources.len());

    let mut all_tracks = Vec::with_capacity(sources.len());
    for (_, playlist) in &sources {
        let items = spotify_client
            .playlist_items(&playlist.id)
            .with_context(|| format!("could not fetch tracks from {}", playlist.name))?;

        let tracks = sync::track_uris(items);
        println!(
            "{}: {} tracks ({} skipped)",
            playlist.name,
            tracks.uris.len(),
            tracks.n_skipped,
        );
        all_tracks.push(tracks);
    }

    let total_tracks = all_tracks.iter().map(|t| t.uris.len()).sum::<usize>();
    let unique_tracks: HashSet<_> = all_tracks.iter().flat_map(|t| t.uris.iter()).collect();
    println!(
        "total: {total_tracks} tracks, {} unique",
        unique_tracks.len(),
    );

    Ok(())
}
