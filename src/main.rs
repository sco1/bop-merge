use anyhow::{Context, Result};
use bop_merge::{auth, spotify};
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
        "logged in as {} ({})",
        user.display_name.as_deref().unwrap_or(&user.id),
        user.id
    );

    Ok(())
}
