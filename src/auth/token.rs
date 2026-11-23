//! Requests to the Spotify token endpoint: exchanging an authorization code for tokens and
//! refreshing an access token.

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

/// Tokens returned by the Spotify token endpoint.
///
/// Tokens are held as [`SecretString`], so `Debug` output redacts them.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    access_token: SecretString,
    expires_in: u64,
    refresh_token: Option<SecretString>,
}

impl TokenResponse {
    /// Return the bearer token for Web API requests.
    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    /// Return the access token's lifetime in seconds.
    pub fn expires_in(&self) -> u64 {
        self.expires_in
    }

    /// Return the refresh token, if Spotify issued one.
    ///
    /// A code exchange should always include one. A refresh includes one only when Spotify rotates
    /// the refresh token, in which case the new token replaces the saved one.
    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }
}

/// Error returned by [`refresh`] when Spotify rejects the refresh token as expired, revoked, or
/// otherwise invalid.
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

/// POSTs `form` to the token endpoint at `token_url` and parses the response.
///
/// A non-success status is returned as a [`TokenRequestError`] for downstream inspection.
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

/// Does the work of [`exchange_code`] against any `token_url`, so tests can use a mock server.
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

/// Does the work of [`refresh`] against any `token_url`, so tests can use a mock server.
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

/// Exchanges the authorization code from the login redirect for tokens.
///
/// `pkce` must be the verifier pair whose challenge was sent in the authorize request.
///
/// # Errors
///
/// Returns an error if the token endpoint cannot be reached, rejects the request, returns
/// unexpected JSON, or omits the refresh token.
pub fn exchange_code(
    http: &Client,
    client_id: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<TokenResponse> {
    exchange_code_at(http, TOKEN_ENDPOINT, client_id, code, pkce)
}

/// Exchanges a refresh token for a new access token.
///
/// The response may include a rotated refresh token; see [`TokenResponse::refresh_token`].
///
/// # Errors
///
/// Returns [`InvalidGrant`], wrapped in an [`anyhow::Error`], if Spotify rejects the refresh
/// token. Returns other errors if the token endpoint cannot be reached, fails, or returns
/// unexpected JSON.
pub fn refresh(
    http: &Client,
    client_id: &str,
    refresh_token: &SecretString,
) -> Result<TokenResponse> {
    refresh_at(http, TOKEN_ENDPOINT, client_id, refresh_token)
}
