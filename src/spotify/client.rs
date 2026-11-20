#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

use std::{thread, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{HeaderMap, RETRY_AFTER},
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, de::DeserializeOwned};
use url::Url;

use crate::spotify::{Playlist, PlaylistItem, User, models::Page};

const API_BASE: &str = "https://api.spotify.com/v1";
const MAX_PAGES: usize = 100;
const MAX_RETRIES: u32 = 5;
const MAX_WAIT_SECS: u64 = 60;

fn backoff_secs(attempt: u32) -> u64 {
    2u64.saturating_pow(attempt)
}

fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers.get(RETRY_AFTER)?.to_str().ok()?.parse().ok()
}

#[derive(Deserialize)]
struct ErrorObject {
    message: String,
}

#[derive(Deserialize)]
struct ApiError {
    error: ErrorObject,
}

pub struct SpotifyClient {
    http: Client,
    access_token: SecretString,
    base_url: Url,
}

impl SpotifyClient {
    pub fn new(http: Client, access_token: SecretString) -> Self {
        Self::with_base_url(
            http,
            access_token,
            Url::parse(API_BASE).expect("API_BASE is a valid URL"),
        )
    }

    fn with_base_url(http: Client, access_token: SecretString, base_url: Url) -> Self {
        Self {
            http,
            access_token,
            base_url,
        }
    }

    fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("API base URL must be http(s)")
            .pop_if_empty()
            .extend(segments);

        url
    }

    fn send_with_retry(&self, url: &Url) -> Result<Response> {
        let mut attempt: u32 = 0;
        loop {
            let r = self
                .http
                .get(url.clone())
                .bearer_auth(self.access_token.expose_secret())
                .send()
                .context("could not reach Spotify API")?;

            if r.status() != StatusCode::TOO_MANY_REQUESTS || attempt == MAX_RETRIES {
                return Ok(r);
            }

            let wait = retry_after(r.headers()).unwrap_or_else(|| backoff_secs(attempt));
            if wait > MAX_WAIT_SECS {
                return Ok(r);
            }
            eprintln!(
                "rate limited; retrying in {wait}s ({}/{MAX_RETRIES})",
                attempt + 1
            );
            thread::sleep(Duration::from_secs(wait));
            attempt += 1;
        }
    }

    fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let r = self.send_with_retry(&url)?;
        let status = r.status();
        if !status.is_success() {
            let hint = if status == StatusCode::TOO_MANY_REQUESTS {
                retry_after(r.headers())
                    .map(|s| format!(" (Spotify asks to wait {s}s)"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let body = r.text().unwrap_or_default();

            match serde_json::from_str::<ApiError>(&body) {
                Ok(e) => bail!("Spotify API error ({status}): {}{hint}", e.error.message),
                Err(_) => bail!("Spotify API error ({status}): {body}{hint}"),
            }
        }

        r.json().context("unexpected response from Spotify API")
    }

    pub fn me(&self) -> Result<User> {
        self.get_json(self.endpoint(&["me"]))
    }

    fn get_all<T: DeserializeOwned>(&self, first: Url) -> Result<Vec<T>> {
        let mut url = first;
        let mut all: Vec<T> = Vec::new();

        for _ in 0..MAX_PAGES {
            let page: Page<T> = self.get_json(url)?;
            all.extend(page.items);

            match page.next {
                Some(next) => {
                    let next = Url::parse(&next).context("Spotify returned an invalid next URL")?;
                    if next.origin() != self.base_url.origin() {
                        bail!("next URL {next} is not on {}", self.base_url)
                    }
                    url = next;
                }
                None => return Ok(all),
            }
        }

        bail!("pagination exceeded {MAX_PAGES} pages")
    }

    pub fn my_playlists(&self) -> Result<Vec<Playlist>> {
        let mut url = self.endpoint(&["me", "playlists"]);
        url.query_pairs_mut().append_pair("limit", "50");

        self.get_all(url)
    }

    pub fn playlist_items(&self, id: &str) -> Result<Vec<PlaylistItem>> {
        let mut url = self.endpoint(&["playlists", id, "items"]);
        url.query_pairs_mut()
            .append_pair("limit", "50")
            .append_pair("fields", "items(is_local,item(type,uri)),next");

        self.get_all(url)
    }
}
