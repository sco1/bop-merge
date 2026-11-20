#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use secrecy::SecretString;
use serde::Deserialize;

use crate::auth::{REDIRECT_URI, pkce::Pkce};

const TOKEN_ENDPOINT: &str = "https://accounts.spotify.com/api/token";

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    access_token: SecretString,
    expires_in: u64,
    refresh_token: Option<SecretString>,
}

impl TokenResponse {
    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }
    pub fn expires_in(&self) -> u64 {
        self.expires_in
    }
    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }
}

fn exchange_code_at(
    http: &Client,
    token_url: &str,
    client_id: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<TokenResponse> {
    let r = http
        .post(token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", client_id),
            ("code_verifier", pkce.verifier()),
        ])
        .send()
        .context("could not reach Spotify token endpoint")?;

    let status = r.status();
    if !status.is_success() {
        let body = r.text().unwrap_or_default();
        bail!("token request failed ({status}): {body}")
    }

    let tokens = r
        .json::<TokenResponse>()
        .context("token response was not the expected JSON")?;

    if tokens.refresh_token().is_none() {
        bail!("token response had no refresh token")
    }

    Ok(tokens)
}

pub fn exchange_code(
    http: &Client,
    client_id: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<TokenResponse> {
    exchange_code_at(http, TOKEN_ENDPOINT, client_id, code, pkce)
}
