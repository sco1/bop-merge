use std::time::Instant;

use reqwest::header::HeaderValue;

use super::*;
use crate::test_support::{FakeServer, Reply, serve_once};

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

const RATE_LIMITED_JSON: &str = r#"{"error":{"status":429,"message":"API rate limit exceeded"}}"#;

fn rate_limited(retry_after: &str) -> Reply {
    Reply::new("429 Too Many Requests", RATE_LIMITED_JSON).header("Retry-After", retry_after)
}

fn playlist_json(id: &str, name: &str) -> String {
    format!(r#"{{"id":"{id}","name":"{name}","owner":{{"id":"test-user"}}}}"#)
}

// --- pure helpers ---

#[test]
fn backoff_secs_doubles() {
    let secs: Vec<u64> = (0..5).map(backoff_secs).collect();
    assert_eq!(secs, [1, 2, 4, 8, 16]);
}

#[test]
fn backoff_secs_saturates() {
    assert_eq!(backoff_secs(u32::MAX), u64::MAX);
}

#[test]
fn retry_after_parses_seconds() {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("3"));
    assert_eq!(retry_after(&headers), Some(3));
}

#[test]
fn retry_after_missing_or_unparseable() {
    assert_eq!(retry_after(&HeaderMap::new()), None);
    for value in ["soon", "-1", "1.5", "Wed, 21 Oct 2026 07:28:00 GMT"] {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        assert_eq!(retry_after(&headers), None, "{value}");
    }
}

// --- 429 retry ---

#[test]
fn retries_429_then_succeeds() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let handle = server.serve(vec![rate_limited("0"), Reply::new("200 OK", PROFILE_JSON)]);

    let user = client.me().unwrap();
    let requests = handle.join().unwrap();

    assert_eq!(user.id, "test-user");
    assert_eq!(requests.len(), 2);
}

#[test]
fn gives_up_after_max_retries() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let replies = (0..=MAX_RETRIES).map(|_| rate_limited("0")).collect();
    let handle = server.serve(replies);

    let err = client.me().unwrap_err();
    let requests = handle.join().unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("429"), "{msg}");
    assert!(msg.contains("API rate limit exceeded"), "{msg}");
    assert_eq!(requests.len(), MAX_RETRIES as usize + 1);
}

#[test]
fn gives_up_at_once_on_long_retry_after() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let handle = server.serve(vec![rate_limited("3600")]);

    let started = Instant::now();
    let err = client.me().unwrap_err();
    let elapsed = started.elapsed();
    let requests = handle.join().unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("429"), "{msg}");
    assert!(msg.contains("wait 3600s"), "{msg}");
    assert_eq!(requests.len(), 1);
    assert!(elapsed < Duration::from_secs(2), "slept for {elapsed:?}");
}

#[test]
fn does_not_retry_other_errors() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let handle = server.serve(vec![Reply::new(
        "401 Unauthorized",
        r#"{"error":{"status":401,"message":"The access token expired"}}"#,
    )]);

    let err = client.me().unwrap_err();
    let requests = handle.join().unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("401"), "{msg}");
    assert!(!msg.contains("wait"), "{msg}");
    assert_eq!(requests.len(), 1);
}

// --- paging ---

#[test]
fn my_playlists_follows_next() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let page1 = format!(
        r#"{{"items":[{}],"next":"{}/v1/me/playlists?offset=1&limit=1"}}"#,
        playlist_json("a", "2023 Bops"),
        server.base()
    );
    let page2 = format!(
        r#"{{"items":[{}],"next":null}}"#,
        playlist_json("b", "2024 Bops")
    );
    let handle = server.serve(vec![
        Reply::new("200 OK", page1),
        Reply::new("200 OK", page2),
    ]);

    let playlists = client.my_playlists().unwrap();
    let requests = handle.join().unwrap();

    let ids: Vec<&str> = playlists.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, "/v1/me/playlists?limit=50");
    assert_eq!(requests[1].path, "/v1/me/playlists?offset=1&limit=1");
}

#[test]
fn my_playlists_single_page() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let page = format!(
        r#"{{"items":[{}],"next":null}}"#,
        playlist_json("a", "2023 Bops")
    );
    let handle = server.serve(vec![Reply::new("200 OK", page)]);

    let playlists = client.my_playlists().unwrap();
    let requests = handle.join().unwrap();

    assert_eq!(playlists.len(), 1);
    assert_eq!(requests.len(), 1);
}

#[test]
fn my_playlists_rejects_next_on_another_host() {
    let server = FakeServer::bind();
    let client = client_for(server.base());
    let page = format!(
        r#"{{"items":[{}],"next":"http://example.invalid/v1/me/playlists?offset=1"}}"#,
        playlist_json("a", "2023 Bops")
    );
    let handle = server.serve(vec![Reply::new("200 OK", page)]);

    let err = client.my_playlists().unwrap_err();
    let requests = handle.join().unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("example.invalid"), "{msg}");
    assert_eq!(requests.len(), 1);
}
