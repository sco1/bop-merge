use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json;
use url::Url;

use crate::spotify::User;

const API_BASE: &str = "https://api.spotify.com/v1";

#[derive(Deserialize)]
struct ErrorObject {
    msg: String,
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

    fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let r = self
            .http
            .get(url)
            .bearer_auth(self.access_token.expose_secret())
            .send()
            .context("could not reach Spotify API")?;

        let status = r.status();
        if !status.is_success() {
            let body = r.text().unwrap_or_default();
            match serde_json::from_str::<ApiError>(&body) {
                Ok(e) => bail!("Spotify API error ({status}): {}", e.error.msg),
                Err(_) => bail!("Spotify API error ({status}): {body}"),
            }
        }

        r.json().context("unexpected response from Spotify API")
    }

    pub fn me(&self) -> Result<User> {
        self.get_json(self.endpoint(&["me"]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::serve_once;

    const PROFILE_JSON: &str = r#"{"account_id":"aB3dE5fG7h","display_name":"Test User","external_urls":{"spotify":"https://open.spotify.com/user/test-user"},"href":"https://api.spotify.com/v1/users/test-user","id":"test-user","images":[],"type":"user","uri":"spotify:user:test-user"}"#;

    /// A client whose API base is `{base}/v1`, mirroring the real `API_BASE` path.
    fn client_for(base: &str) -> SpotifyClient {
        SpotifyClient::with_base_url(
            Client::new(),
            SecretString::from("test-token".to_owned()),
            Url::parse(&format!("{base}/v1")).unwrap(),
        )
    }

    #[test]
    fn endpoint_appends_segments() {
        let client = client_for("http://127.0.0.1:1");
        assert_eq!(
            client.endpoint(&["me"]).as_str(),
            "http://127.0.0.1:1/v1/me"
        );
        assert_eq!(
            client.endpoint(&["playlists", "abc", "items"]).as_str(),
            "http://127.0.0.1:1/v1/playlists/abc/items"
        );
    }

    #[test]
    fn endpoint_tolerates_trailing_slash_on_base() {
        let client = client_for("http://127.0.0.1:1");
        let client = SpotifyClient {
            base_url: Url::parse("http://127.0.0.1:1/v1/").unwrap(),
            ..client
        };
        assert_eq!(
            client.endpoint(&["me"]).as_str(),
            "http://127.0.0.1:1/v1/me"
        );
    }

    #[test]
    fn endpoint_encodes_slashes_in_segments() {
        let client = client_for("http://127.0.0.1:1");
        assert_eq!(
            client.endpoint(&["playlists", "a/../b"]).as_str(),
            "http://127.0.0.1:1/v1/playlists/a%2F..%2Fb"
        );
    }

    #[test]
    fn me_parses_user() {
        let (base, server) = serve_once("200 OK", PROFILE_JSON);
        let user = client_for(&base).me().unwrap();
        server.join().unwrap();

        assert_eq!(user.id, "test-user");
        assert_eq!(user.display_name.as_deref(), Some("Test User"));
    }

    #[test]
    fn me_null_display_name() {
        let (base, server) = serve_once("200 OK", r#"{"id":"test-user","display_name":null}"#);
        let user = client_for(&base).me().unwrap();
        server.join().unwrap();

        assert_eq!(user.id, "test-user");
        assert_eq!(user.display_name, None);
    }

    #[test]
    fn me_sends_bearer_and_path() {
        let (base, server) = serve_once("200 OK", PROFILE_JSON);
        client_for(&base).me().unwrap();
        let request = server.join().unwrap();

        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/v1/me");
        assert_eq!(request.header("authorization"), Some("Bearer test-token"));
    }

    #[test]
    fn me_error_shows_message() {
        let (base, server) = serve_once(
            "401 Unauthorized",
            r#"{"error":{"status":401,"message":"The access token expired"}}"#,
        );
        let err = client_for(&base).me().unwrap_err();
        server.join().unwrap();

        let msg = format!("{err:#}");
        assert!(msg.contains("401"), "{msg}");
        assert!(msg.contains("The access token expired"), "{msg}");
        // The message should be extracted from ErrorObject, not the raw JSON body.
        assert!(!msg.contains(r#""error""#), "{msg}");
    }

    #[test]
    fn me_error_non_json_body() {
        let (base, server) = serve_once(
            "403 Forbidden",
            "User not registered in the Developer Dashboard",
        );
        let err = client_for(&base).me().unwrap_err();
        server.join().unwrap();

        let msg = format!("{err:#}");
        assert!(msg.contains("403"), "{msg}");
        assert!(msg.contains("User not registered"), "{msg}");
    }

    #[test]
    fn me_unexpected_shape() {
        let (base, server) = serve_once("200 OK", r#"{"display_name":"Test User"}"#);
        let err = client_for(&base).me().unwrap_err();
        server.join().unwrap();

        let msg = format!("{err:#}");
        assert!(msg.contains("unexpected response"), "{msg}");
        assert!(msg.contains("id"), "{msg}");
    }
}
