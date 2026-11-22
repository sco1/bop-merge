mod callback;
mod pkce;
mod token;

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;

use anyhow::Result;
use rand::distr::{Alphanumeric, SampleString};
use reqwest::blocking::Client;
use url::Url;

pub use crate::auth::token::TokenResponse;
use crate::auth::{
    callback::{bind_callback, wait_for_callback},
    pkce::Pkce,
    token::exchange_code,
};

const AUTHORIZE_ENDPOINT: &str = "https://accounts.spotify.com/authorize";
const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &[&str] = &[
    "playlist-read-private",
    "playlist-modify-public",
    "playlist-modify-private",
];

fn rand_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), len)
}

fn generate_state() -> String {
    rand_string(32)
}

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

/// Run the interactive PKCE login: print the authorize URL, wait for the browser callback on
/// [`REDIRECT_URI`], and exchange the code for tokens.
pub fn login(http: &Client, client_id: &str) -> Result<TokenResponse> {
    let pkce = Pkce::generate();
    let state = generate_state();
    let listener = bind_callback()?;
    println!("open this URL to log in:");
    println!("{}", authorize_url(client_id, &pkce, &state));

    let code = wait_for_callback(&listener, &state)?;
    exchange_code(http, client_id, &code, &pkce)
}
