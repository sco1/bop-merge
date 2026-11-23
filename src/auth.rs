//! Spotify authorization using the OAuth 2.0 authorization code flow with PKCE.

mod callback;
mod pkce;
mod store;
mod token;

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;

use anyhow::Result;
use rand::distr::{Alphanumeric, SampleString};
use reqwest::blocking::Client;
use url::Url;

use crate::auth::{
    callback::{bind_callback, wait_for_callback},
    pkce::Pkce,
    token::exchange_code,
};
pub use crate::auth::{
    store::{KeychainStore, TokenStore},
    token::{InvalidGrant, TokenResponse, refresh},
};

const AUTHORIZE_ENDPOINT: &str = "https://accounts.spotify.com/authorize";
/// Loopback redirect URI that receives the authorization code. It must match the redirect URI
/// registered for the app in the Spotify developer dashboard.
const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
/// Scopes needed to read the user's playlists and modify the target playlist.
const SCOPES: &[&str] = &[
    "playlist-read-private",
    "playlist-modify-public",
    "playlist-modify-private",
];

/// Return a random string of `len` ASCII alphanumeric characters.
fn rand_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), len)
}

/// Generate the OAuth `state` value that ties the redirect to this login attempt.
fn generate_state() -> String {
    rand_string(32)
}

/// Build the Spotify authorize URL that the user will open to approve access.
fn authorize_url(client_id: &str, pkce: &Pkce, state: &str) -> Url {
    let scope = SCOPES.join(" ");
    Url::parse_with_params(
        AUTHORIZE_ENDPOINT,
        &[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("code_challenge_method", "S256"),
            ("code_challenge", pkce.challenge()),
            ("state", state),
            ("scope", &scope),
        ],
    )
    .expect("AUTHORIZE_ENDPOINT is a valid URL")
}

/// Run the interactive PKCE login and return the resulting tokens.
///
/// The authorization flow prints the Spotify authorization URL to the terminal, then waits for the
/// browser redirect on the configured loopback redirect URI. The response `state` is checked and
/// then the authorization code is exchanged for tokens.
///
/// Blocks until a redirect to the callback path arrives; there is no timeout.
///
/// On success, the response always includes a refresh token.
///
/// # Errors
///
/// Returns an error if the local callback port cannot be bound, the user denies access, the
/// redirect fails validation, or the code exchange fails.
pub fn login(http: &Client, client_id: &str) -> Result<TokenResponse> {
    let pkce = Pkce::generate();
    let state = generate_state();
    let listener = bind_callback()?;
    println!("open this URL to log in:");
    println!("{}", authorize_url(client_id, &pkce, &state));

    let code = wait_for_callback(&listener, &state)?;
    exchange_code(http, client_id, &code, &pkce)
}
