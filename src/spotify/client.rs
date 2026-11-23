#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

use std::{thread, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder, Response},
    header::{HeaderMap, RETRY_AFTER},
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

use crate::spotify::{Playlist, PlaylistItem, User, models::Page};

const API_BASE: &str = "https://api.spotify.com/v1";
/// Maximum number of pages allowed to be fetched by `get_all`.
const MAX_PAGES: usize = 100;
/// Maximum number of items Spotify accepts in one add or remove request.
const MAX_ITEMS_PER_REQUEST: usize = 100;
/// Maximum allowed retries of a rate-limited (429) request.
const MAX_RETRIES: u32 = 5;
/// Longest wait honored before a retry. If Spotify asks for a longer wait, the request fails
/// instead.
const MAX_WAIT_SECS: u64 = 60;

/// Return the fallback delay before retry `attempt` (counting from 0) when the response has no
/// `Retry-After` header: 1, 2, 4, … seconds.
fn backoff_secs(attempt: u32) -> u64 {
    2u64.saturating_pow(attempt)
}

/// Parse a `Retry-After` header given in whole seconds.
///
/// Returns `None` if the header is absent or uses another format, such as an HTTP date.
fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers.get(RETRY_AFTER)?.to_str().ok()?.parse().ok()
}

/// Pass through a successful response, or turn an error response into a descriptive error.
///
/// The error uses Spotify's `message` when the body is a standard error object, and mentions
/// the requested wait for a 429.
fn check_status(r: Response) -> Result<Response> {
    let status = r.status();
    if status.is_success() {
        return Ok(r);
    }

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

/// The `error` object inside an [`ApiError`].
#[derive(Deserialize)]
struct ErrorObject {
    message: String,
}

/// Spotify's standard error response body: `{"error": {"status": ..., "message": ...}}`.
#[derive(Deserialize)]
struct ApiError {
    error: ErrorObject,
}

/// One entry in a [`RemoveItemsBody`].
#[derive(Serialize)]
struct ItemUri<'a> {
    uri: &'a str,
}

/// JSON body of a request that adds items to a playlist.
#[derive(Serialize)]
struct AddItemsBody<'a> {
    uris: &'a [String],
}

/// JSON body of a request that removes items from a playlist.
#[derive(Serialize)]
struct RemoveItemsBody<'a> {
    items: Vec<ItemUri<'a>>,
}

/// A blocking Spotify Web API client that authenticates with a bearer access token.
///
/// Rate-limited (429) requests are retried a limited number of times, honoring `Retry-After`
/// when Spotify sends it. The client does not refresh its access token, so it is intended to last
/// for a single command run.
pub struct SpotifyClient {
    http: Client,
    access_token: SecretString,
    base_url: Url,
}

impl SpotifyClient {
    /// Create a client for the Spotify Web API that sends requests through `http` and
    /// authenticates with `access_token`.
    pub fn new(http: Client, access_token: SecretString) -> Self {
        Self::with_base_url(
            http,
            access_token,
            Url::parse(API_BASE).expect("API_BASE is a valid URL"),
        )
    }

    /// Create a client that targets `base_url` instead of the real API, so tests can use a
    /// mock server.
    fn with_base_url(http: Client, access_token: SecretString, base_url: Url) -> Self {
        Self {
            http,
            access_token,
            base_url,
        }
    }

    /// Build a URL by appending path `segments` to the base URL.
    ///
    /// Each segment is percent-encoded, including `/`, so an ID cannot change the path.
    fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("API base URL must be http(s)")
            .pop_if_empty()
            .extend(segments);

        url
    }

    /// Send the request produced by `build`, retrying when Spotify responds 429 Too Many Requests.
    ///
    /// Before each retry, waits for the `Retry-After` duration, or for an exponential backoff if
    /// the header is absent, for up to [`MAX_RETRIES`] retries. When the retries run out, or the
    /// requested wait exceeds [`MAX_WAIT_SECS`], returns the last response unchecked (possibly
    /// still a 429) for the caller to pass to [`check_status`].
    ///
    /// # Errors
    ///
    /// Returns an error only if a request cannot be sent at all.
    fn send_with_retry(&self, build: impl Fn() -> RequestBuilder) -> Result<Response> {
        let mut attempt: u32 = 0;
        loop {
            // Must build per attempt since RequestBuilder is consumed when sent
            let r = build()
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

    /// Send a GET request to `url` and deserialize a successful JSON response.
    fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let r = check_status(self.send_with_retry(|| self.http.get(url.clone()))?)?;
        r.json().context("unexpected response from Spotify API")
    }

    /// Fetche the current user's profile (`GET /me`).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response is not a valid profile.
    pub fn me(&self) -> Result<User> {
        self.get_json(self.endpoint(&["me"]))
    }

    /// Fetche every page of a paginated endpoint, starting at `first` and following `next` links.
    ///
    /// Rejects a `next` URL on a different origin from the base URL, so the access token is never
    /// sent anywhere else. Gives up after [`MAX_PAGES`] pages.
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

    /// Fetch every playlist the current user owns or follows (`GET /me/playlists`).
    ///
    /// # Errors
    ///
    /// Returns an error if any page request fails, a response is malformed, or pagination exceeds
    /// the page limit.
    pub fn my_playlists(&self) -> Result<Vec<Playlist>> {
        let mut url = self.endpoint(&["me", "playlists"]);
        url.query_pairs_mut().append_pair("limit", "50");

        self.get_all(url)
    }

    /// Fetch every item in the playlist with ID `id` (`GET /playlists/{id}/items`).
    ///
    /// Requests only the fields that [`PlaylistItem`] models.
    ///
    /// # Errors
    ///
    /// Returns an error if any page request fails, a response is malformed, or pagination exceeds
    /// the page limit.
    pub fn playlist_items(&self, id: &str) -> Result<Vec<PlaylistItem>> {
        let mut url = self.endpoint(&["playlists", id, "items"]);
        url.query_pairs_mut()
            .append_pair("limit", "50")
            .append_pair("fields", "items(is_local,item(type,uri)),next");

        self.get_all(url)
    }

    /// Append `uris`, in order, to the end of the playlist with ID `playlist_id`
    /// (`POST /playlists/{id}/items`).
    ///
    /// Sends the URIs in batches of up to 100, the most Spotify accepts per request.
    ///
    /// # Errors
    ///
    /// Returns an error if any batch fails. Batches sent before the failure stay applied.
    pub fn add_items(&self, playlist_id: &str, uris: &[String]) -> Result<()> {
        let url = self.endpoint(&["playlists", playlist_id, "items"]);
        for chunk in uris.chunks(MAX_ITEMS_PER_REQUEST) {
            let body = AddItemsBody { uris: chunk };
            check_status(self.send_with_retry(|| self.http.post(url.clone()).json(&body))?)?;
        }

        Ok(())
    }

    /// Remove the items with the given `uris` from the playlist with ID `playlist_id`
    /// (`DELETE /playlists/{id}/items`).
    ///
    /// Sends the URIs in batches of up to 100, the most Spotify accepts per request.
    ///
    /// # Errors
    ///
    /// Returns an error if any batch fails. Batches sent before the failure stay applied.
    pub fn remove_items(&self, playlist_id: &str, uris: &[String]) -> Result<()> {
        let url = self.endpoint(&["playlists", playlist_id, "items"]);
        for chunk in uris.chunks(MAX_ITEMS_PER_REQUEST) {
            let body = RemoveItemsBody {
                items: chunk.iter().map(|uri| ItemUri { uri }).collect(),
            };
            check_status(self.send_with_retry(|| self.http.delete(url.clone()).json(&body))?)?;
        }

        Ok(())
    }
}
