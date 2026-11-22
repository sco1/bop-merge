#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;

use std::fmt;

use anyhow::{Context, Result, bail};
use reqwest::{StatusCode, blocking::Client};
use secrecy::{ExposeSecret, SecretString};
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

#[derive(Debug)]
pub struct InvalidGrant;

impl fmt::Display for InvalidGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the refresh token is expired, revoked, or invalid")
    }
}

impl std::error::Error for InvalidGrant {}

/// A non-2xx reply from the token endpoint.
#[derive(Debug)]
struct TokenRequestError {
    status: StatusCode,
    body: String,
}

impl TokenRequestError {
    /// True for the OAuth error `{"error": "invalid_grant", ...}` on a 400.
    fn is_invalid_grant(&self) -> bool {
        #[derive(Deserialize)]
        struct OAuthError {
            error: String,
        }

        self.status == StatusCode::BAD_REQUEST
            && serde_json::from_str::<OAuthError>(&self.body)
                .is_ok_and(|e| e.error == "invalid_grant")
    }
}

impl fmt::Display for TokenRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "token request failed ({}): {}", self.status, self.body)
    }
}

impl std::error::Error for TokenRequestError {}

fn request_tokens(http: &Client, token_url: &str, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let r = http
        .post(token_url)
        .form(form)
        .send()
        .context("could not reach Spotify token endpoint")?;

    let status = r.status();
    if !status.is_success() {
        let body = r.text().unwrap_or_default();
        return Err(TokenRequestError { status, body }.into());
    }

    r.json::<TokenResponse>()
        .context("token response was not the expected JSON")
}

fn exchange_code_at(
    http: &Client,
    token_url: &str,
    client_id: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<TokenResponse> {
    let tokens = request_tokens(
        http,
        token_url,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", client_id),
            ("code_verifier", pkce.verifier()),
        ],
    )?;

    if tokens.refresh_token().is_none() {
        bail!("token response had no refresh token")
    }

    Ok(tokens)
}

fn refresh_at(
    http: &Client,
    token_url: &str,
    client_id: &str,
    refresh_token: &SecretString,
) -> Result<TokenResponse> {
    request_tokens(
        http,
        token_url,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", client_id),
        ],
    )
    .map_err(|e| match e.downcast_ref::<TokenRequestError>() {
        Some(t) if t.is_invalid_grant() => InvalidGrant.into(),
        _ => e,
    })
}

pub fn exchange_code(
    http: &Client,
    client_id: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<TokenResponse> {
    exchange_code_at(http, TOKEN_ENDPOINT, client_id, code, pkce)
}

/// Exchange a refresh token for a new access token (and possibly a new refresh token).
///
/// Fails with [`InvalidGrant`] if Spotify rejects the refresh token.
pub fn refresh(
    http: &Client,
    client_id: &str,
    refresh_token: &SecretString,
) -> Result<TokenResponse> {
    refresh_at(http, TOKEN_ENDPOINT, client_id, refresh_token)
}
