//! A minimal local HTTP server that receives the OAuth redirect after the user approves access.

#[cfg(test)]
#[path = "callback_tests.rs"]
mod tests;

use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use httparse::Status;
use url::Url;

use crate::auth::REDIRECT_URI;

/// Largest request head (request line plus headers) accepted before the connection is dropped.
const MAX_REQUEST_BYTES: usize = 8 * 1024;
/// Most headers parsed in one request.
const MAX_HEADERS: usize = 32;
/// Read and write timeout for each connection, so a stalled client cannot block the login.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Page shown in the browser after a successful login.
const SUCCESS_BODY: &str = "<!doctype html>\
    <html><head><meta charset=\"utf-8\"><title>bop-merge</title></head>\
    <body><p>Login complete. You can close this tab and return to the terminal.</p></body></html>";

/// Page shown in the browser after a failed login.
const FAILURE_BODY: &str = "<!doctype html>\
    <html><head><meta charset=\"utf-8\"><title>bop-merge</title></head>\
    <body><p>Login failed. Return to the terminal for details.</p></body></html>";

/// Reasons a redirect to the callback path is rejected.
#[derive(Debug)]
enum CallbackError {
    /// Spotify redirected with an `error` parameter, such as `access_denied`.
    Denied(String),
    /// The `state` parameter is missing or does not match the login attempt.
    StateMismatch,
    /// The redirect has a valid `state` but no `code`.
    MissingCode,
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "authorization denied by Spotify: {reason}"),
            Self::StateMismatch => write!(f, "redirect state is missing or does not match login"),
            Self::MissingCode => write!(f, "redirect missing authorization code"),
        }
    }
}
impl std::error::Error for CallbackError {}

/// The result of examining the request to the callback server.
enum Outcome {
    /// The request is not for the callback path (for example, `/favicon.ico`); keep waiting.
    NotFound,
    /// A valid redirect carrying this authorization code.
    Code(String),
    /// A redirect to the callback path that ends the login with an error.
    Failed(CallbackError),
}

/// Bind a TCP listener on the host and port of the redirect URI.
///
/// # Errors
///
/// Returns an error if the address cannot be bound, for example because another process (or
/// another login) is already listening on the port.
pub fn bind_callback() -> Result<TcpListener> {
    let url = Url::parse(REDIRECT_URI)?;
    let host = url.host_str().context("REDIRECT_URI has no host")?;
    let port = url
        .port_or_known_default()
        .context("REDIRECT_URI has no port")?;

    TcpListener::bind((host, port)).with_context(|| format!("could not listen on {host}:{port}"))
}

/// Read one HTTP request head from `stream` and returns its request target (path and query).
///
/// Any request body is ignored.
///
/// # Errors
///
/// Returns an error if the request head exceeds [`MAX_REQUEST_BYTES`], is malformed, or the
/// connection closes or times out before the head is complete.
fn read_request(stream: &mut impl Read) -> Result<String> {
    let mut buf = [0u8; MAX_REQUEST_BYTES];
    let mut filled = 0;

    loop {
        if filled == buf.len() {
            bail!("request larger than {MAX_REQUEST_BYTES} bytes");
        }

        let n = stream.read(&mut buf[filled..])?;
        if n == 0 {
            bail!("connection closed before request headers were complete");
        }
        filled += n;

        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut r = httparse::Request::new(&mut headers);

        match r.parse(&buf[..filled])? {
            Status::Partial => {}
            Status::Complete(_) => {
                let path = r.path.context("request has no path")?;
                return Ok(path.to_owned());
            }
        }
    }
}

/// Classify the request target `path` against the redirect URI and `expected_state`.
///
/// An `error` parameter takes precedence over everything else. Otherwise, `state` must match before
/// `code` is accepted.
fn parse_callback(path: &str, expected_state: &str) -> Outcome {
    let url_base = Url::parse(REDIRECT_URI).expect("REDIRECT_URI is a valid URL");
    let Ok(url) = url_base.join(path) else {
        return Outcome::NotFound;
    };

    if url.path() != url_base.path() {
        return Outcome::NotFound;
    }

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;

    for (key, val) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(val.into_owned()),
            "state" => state = Some(val.into_owned()),
            "error" => error = Some(val.into_owned()),
            _ => {}
        }
    }

    if let Some(e) = error {
        return Outcome::Failed(CallbackError::Denied(e));
    }
    if state.as_deref() != Some(expected_state) {
        return Outcome::Failed(CallbackError::StateMismatch);
    }

    match code {
        Some(c) => Outcome::Code(c),
        None => Outcome::Failed(CallbackError::MissingCode),
    }
}

/// Write a minimal HTTP/1.1 response with an HTML `body` and `Connection: close`.
fn write_response(stream: &mut impl Write, status: &str, body: &str) -> std::io::Result<()> {
    let len = body.len();
    write!(
        stream,
        "HTTP/1.1 {status}\r\n\
        Content-Type: text/html; charset=utf-8\r\n\
        Content-Length: {len}\r\n\
        Connection: close\r\n\r\n{body}"
    )
}

/// Serve `listener` until a redirect to the callback path arrives, then returns its authorization
/// code.
///
/// Requests for other paths get a 404 and are ignored. Connections that fail or send malformed
/// requests are logged to stderr and skipped. The browser is shown a success or failure page.
///
/// Blocks until a redirect to the callback path is received; there is no timeout.
///
/// # Errors
///
/// Returns a `CallbackError` if the redirect reports a denial, has a missing or mismatched `state`,
/// or lacks a code.
pub fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("could not accept connection: {e}");
                continue;
            }
        };

        if let Err(e) = stream.set_read_timeout(Some(IO_TIMEOUT)) {
            eprintln!("could not set read timeout: {e}");
            continue;
        }
        if let Err(e) = stream.set_write_timeout(Some(IO_TIMEOUT)) {
            eprintln!("could not set write timeout: {e}");
            continue;
        }

        let path = match read_request(&mut stream) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("ignoring connection: {e:#}");
                continue;
            }
        };

        let (status, body, result) = match parse_callback(&path, expected_state) {
            Outcome::NotFound => ("404 Not Found", "", None),
            Outcome::Code(code) => ("200 OK", SUCCESS_BODY, Some(Ok(code))),
            Outcome::Failed(reason) => ("400 Bad Request", FAILURE_BODY, Some(Err(reason.into()))),
        };
        if let Err(e) = write_response(&mut stream, status, body) {
            eprintln!("could not send response: {e}");
        }
        if let Some(result) = result {
            return result;
        }
    }
}
