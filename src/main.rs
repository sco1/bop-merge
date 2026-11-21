use anyhow::{Context, Result};
use bop_merge::{auth, spotify, sync};
use reqwest::blocking::Client;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let client_id = std::env::var("BOPMERGE_CLIENT_ID").context("BOPMERGE_CLIENT_ID not set")?;
    let target_id = std::env::var("BOPMERGE_TARGET_PLAYLIST_ID")
        .context("BOPMERGE_TARGET_PLAYLIST_ID not set")?;

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

    let (target_playlist, rest) =
        sync::take_target_playlist(spotify_client.my_playlists()?, &target_id, &user.id)?;
    println!("found target playlist '{}'", target_playlist.name);

    let existing_merged_items = spotify_client
        .playlist_items(&target_playlist.id)
        .with_context(|| format!("could not fetch tracks from '{}'", target_playlist.name))?;
    let existing_merged_tracks = sync::track_uris(existing_merged_items);

    let sources = sync::select_sources(rest, &user.id);
    println!("found {} source playlists", sources.len());

    let mut all_tracks = Vec::with_capacity(sources.len());
    for (_, playlist) in &sources {
        let items = spotify_client
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
    println!(
        "sync plan: {} to add, {} to remove",
        plan.add.len(),
        plan.remove.len()
    );
    if let Err(e) = sync::check_plan(&plan, false) {
        eprintln!("warning: {e:#}");
    }

    Ok(())
}
