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

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_HEADERS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

const SUCCESS_BODY: &str = "<!doctype html>\
    <html><head><meta charset=\"utf-8\"><title>bop-merge</title></head>\
    <body><p>Login complete. You can close this tab and return to the terminal.</p></body></html>";

const FAILURE_BODY: &str = "<!doctype html>\
    <html><head><meta charset=\"utf-8\"><title>bop-merge</title></head>\
    <body><p>Login failed. Return to the terminal for details.</p></body></html>";

#[derive(Debug)]
enum CallbackError {
    Denied(String),
    StateMismatch,
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

enum Outcome {
    NotFound,
    Code(String),
    Failed(CallbackError),
}

pub fn bind_callback() -> Result<TcpListener> {
    let url = Url::parse(REDIRECT_URI)?;
    let host = url.host_str().context("REDIRECT_URI has no host")?;
    let port = url
        .port_or_known_default()
        .context("REDIRECT_URI has no port")?;

    TcpListener::bind((host, port)).with_context(|| format!("could not listen on {host}:{port}"))
}

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
