use anyhow::{Context, Result};
use bop_merge::auth;
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
    println!("got access token (expires in {} s)", tokens.expires_in());

    Ok(())
}
