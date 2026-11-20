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

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread::{self, JoinHandle},
    };

    use secrecy::ExposeSecret;

    use super::*;

    const SUCCESS_JSON: &str = r#"{"access_token":"access-123","token_type":"Bearer","scope":"playlist-modify-public","expires_in":3600,"refresh_token":"refresh-456"}"#;

    /// Serve one canned response on a random local port.
    ///
    /// Returns the URL to post to and a handle that yields the request body the client sent.
    fn serve_once(status: &'static str, body: &'static str) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/token", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request_body = read_body(&mut stream);
            let len = body.len();
            write!(
                stream,
                "HTTP/1.1 {status}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {len}\r\n\
                 Connection: close\r\n\r\n{body}"
            )
            .unwrap();
            request_body
        });
        (url, handle)
    }

    /// Read a whole request (headers plus `Content-Length` bytes) and return its body.
    ///
    /// Draining the body before replying keeps the client from seeing a connection reset.
    fn read_body(stream: &mut impl Read) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "client closed before sending a full request");
            buf.extend_from_slice(&chunk[..n]);

            let mut headers = [httparse::EMPTY_HEADER; 32];
            let mut req = httparse::Request::new(&mut headers);
            if let httparse::Status::Complete(head_len) = req.parse(&buf).unwrap() {
                let content_len = req
                    .headers
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                    .map(|h| std::str::from_utf8(h.value).unwrap().parse().unwrap())
                    .unwrap_or(0);
                let end = head_len + content_len;
                if buf.len() >= end {
                    return String::from_utf8(buf[head_len..end].to_vec()).unwrap();
                }
            }
        }
    }

    fn exchange(url: &str) -> Result<TokenResponse> {
        exchange_code_at(
            &Client::new(),
            url,
            "test-client",
            "test-code",
            &Pkce::generate(),
        )
    }

    #[test]
    fn exchange_success() {
        let (url, server) = serve_once("200 OK", SUCCESS_JSON);
        let tokens = exchange(&url).unwrap();
        server.join().unwrap();

        assert_eq!(tokens.access_token().expose_secret(), "access-123");
        assert_eq!(tokens.expires_in(), 3600);
        assert_eq!(
            tokens.refresh_token().map(|t| t.expose_secret()),
            Some("refresh-456")
        );
    }

    #[test]
    fn exchange_sends_form() {
        let (url, server) = serve_once("200 OK", SUCCESS_JSON);
        let pkce = Pkce::generate();
        exchange_code_at(&Client::new(), &url, "test-client", "test-code", &pkce).unwrap();
        let body = server.join().unwrap();

        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        let expected = [
            ("grant_type", "authorization_code"),
            ("code", "test-code"),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", "test-client"),
            ("code_verifier", pkce.verifier()),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        assert_eq!(pairs, expected);
    }

    #[test]
    fn exchange_missing_refresh_token() {
        let (url, server) = serve_once(
            "200 OK",
            r#"{"access_token":"access-123","token_type":"Bearer","expires_in":3600}"#,
        );
        let err = exchange(&url).unwrap_err();
        server.join().unwrap();

        assert!(format!("{err:#}").contains("no refresh token"), "{err:#}");
    }

    #[test]
    fn exchange_error_shows_body() {
        let (url, server) = serve_once(
            "400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"Invalid authorization code"}"#,
        );
        let err = exchange(&url).unwrap_err();
        server.join().unwrap();

        let msg = format!("{err:#}");
        assert!(msg.contains("400"), "{msg}");
        assert!(msg.contains("invalid_grant"), "{msg}");
    }

    #[test]
    fn exchange_bad_json() {
        let (url, server) = serve_once("200 OK", "not json");
        let err = exchange(&url).unwrap_err();
        server.join().unwrap();

        assert!(
            format!("{err:#}").contains("not the expected JSON"),
            "{err:#}"
        );
    }

    #[test]
    fn exchange_unreachable() {
        // Bind then drop to get a local port with nothing listening on it.
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let err = exchange(&format!("http://{addr}/api/token")).unwrap_err();

        assert!(format!("{err:#}").contains("could not reach"), "{err:#}");
    }

    #[test]
    fn debug_redacts_tokens() {
        let (url, server) = serve_once("200 OK", SUCCESS_JSON);
        let tokens = exchange(&url).unwrap();
        server.join().unwrap();

        let debug = format!("{tokens:?}");
        assert!(!debug.contains("access-123"), "{debug}");
        assert!(!debug.contains("refresh-456"), "{debug}");
        assert!(debug.contains("3600"), "{debug}");
    }
}
