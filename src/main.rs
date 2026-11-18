use anyhow::{Context, Result};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let client_id = std::env::var("BOPMERGE_CLIENT_ID").context("BOPMERGE_CLIENT_ID not set")?;

    let pkce = bop_merge::auth::Pkce::generate();
    let state = bop_merge::auth::generate_state();
    println!(
        "{}",
        bop_merge::auth::authorize_url(&client_id, &pkce, &state)
    );

    Ok(())
}
