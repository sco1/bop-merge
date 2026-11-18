use std::{io::Read, net::TcpListener};

use anyhow::{Context, Result};
use url::Url;

use crate::auth::REDIRECT_URI;

enum CallbackError {
    Denied(String),
    StateMismatch,
    MissingCode,
}

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
    unimplemented!()
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

pub fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<String> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use std::{io::Write, net::TcpStream, thread, time::Duration};

    use super::*;

    const STATE: &str = "expected-state";

    #[test]
    fn parse_success_returns_code() {
        let outcome = parse_callback("/callback?code=abc123&state=expected-state", STATE);
        assert!(matches!(outcome, Outcome::Code(ref c) if c == "abc123"));
    }

    #[test]
    fn parse_decodes_percent_encoded_code() {
        // query_pairs() should decode %2F to '/'
        let outcome = parse_callback("/callback?code=a%2Fb&state=expected-state", STATE);
        assert!(matches!(outcome, Outcome::Code(ref c) if c == "a/b"));
    }

    #[test]
    fn parse_state_mismatch_fails() {
        let outcome = parse_callback("/callback?code=abc123&state=wrong", STATE);
        assert!(matches!(
            outcome,
            Outcome::Failed(CallbackError::StateMismatch)
        ));
    }

    #[test]
    fn parse_missing_state_fails() {
        let outcome = parse_callback("/callback?code=abc123", STATE);
        assert!(matches!(
            outcome,
            Outcome::Failed(CallbackError::StateMismatch)
        ));
    }

    #[test]
    fn parse_access_denied_fails() {
        let outcome = parse_callback("/callback?error=access_denied&state=expected-state", STATE);
        assert!(matches!(
            outcome,
            Outcome::Failed(CallbackError::Denied(ref e)) if e == "access_denied"
        ));
    }

    #[test]
    fn parse_error_is_checked_before_state() {
        // Check order is error -> state -> code, so an error wins even with a bad state.
        let outcome = parse_callback("/callback?error=access_denied&state=wrong", STATE);
        assert!(matches!(outcome, Outcome::Failed(CallbackError::Denied(_))));
    }

    #[test]
    fn parse_missing_code_fails() {
        let outcome = parse_callback("/callback?state=expected-state", STATE);
        assert!(matches!(
            outcome,
            Outcome::Failed(CallbackError::MissingCode)
        ));
    }

    #[test]
    fn parse_other_paths_are_not_found() {
        for path in [
            "/favicon.ico",
            "/",
            "/callback/extra?code=abc123&state=expected-state",
        ] {
            assert!(
                matches!(parse_callback(path, STATE), Outcome::NotFound),
                "expected NotFound for {path}"
            );
        }
    }

    // ---- read_request: any `impl Read`, no sockets ----

    /// Returns at most one byte per `read` call, like a slow network connection.
    struct OneByteReader<'a>(&'a [u8]);

    impl Read for OneByteReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match (self.0.split_first(), buf.first_mut()) {
                (Some((byte, rest)), Some(slot)) => {
                    *slot = *byte;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    const SIMPLE_REQUEST: &[u8] =
        b"GET /callback?code=abc123&state=expected-state HTTP/1.1\r\nHost: 127.0.0.1:8888\r\n\r\n";

    #[test]
    fn read_whole_request_in_one_read() {
        let mut stream: &[u8] = SIMPLE_REQUEST;
        let path = read_request(&mut stream).unwrap();
        assert_eq!(path, "/callback?code=abc123&state=expected-state");
    }

    #[test]
    fn read_request_split_across_many_reads() {
        let mut stream = OneByteReader(SIMPLE_REQUEST);
        let path = read_request(&mut stream).unwrap();
        assert_eq!(path, "/callback?code=abc123&state=expected-state");
    }

    #[test]
    fn read_browser_like_request_with_many_headers() {
        // Real browsers send 10-20 headers; the headers array must hold them.
        let mut raw = String::from("GET /favicon.ico HTTP/1.1\r\nHost: 127.0.0.1:8888\r\n");
        for i in 0..20 {
            raw.push_str(&format!("X-Header-{i}: value\r\n"));
        }
        raw.push_str("\r\n");
        let mut stream = raw.as_bytes();
        assert_eq!(read_request(&mut stream).unwrap(), "/favicon.ico");
    }

    #[test]
    fn read_oversized_request_fails() {
        // Well past the ~8 KiB cap, with no end of headers in sight.
        let raw = format!(
            "GET /callback HTTP/1.1\r\nX-Pad: {}\r\n\r\n",
            "a".repeat(16 * 1024)
        );
        let mut stream = raw.as_bytes();
        assert!(read_request(&mut stream).is_err());
    }

    #[test]
    fn read_early_eof_fails() {
        // Connection closes before the blank line that ends the headers.
        let mut stream: &[u8] = b"GET /callback HTTP/1.1\r\nHost: 127.0.0.1:8888\r\n";
        assert!(read_request(&mut stream).is_err());
    }

    #[test]
    fn read_malformed_request_fails() {
        let mut stream: &[u8] = b"NOT AN HTTP REQUEST\r\n\r\n";
        assert!(read_request(&mut stream).is_err());
    }

    // ---- wait_for_callback: real sockets on an OS-assigned port, not 8888 ----

    /// Sends one raw request and returns the full response text.
    fn send(addr: std::net::SocketAddr, raw: &str) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn get(path: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
    }

    #[test]
    fn wait_skips_other_paths_then_returns_code() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || {
            let not_found = send(addr, &get("/favicon.ico"));
            let ok = send(addr, &get("/callback?code=abc123&state=expected-state"));
            (not_found, ok)
        });

        let code = wait_for_callback(&listener, STATE).unwrap();
        let (not_found, ok) = client.join().unwrap();

        assert_eq!(code, "abc123");
        assert!(not_found.starts_with("HTTP/1.1 404"), "got: {not_found}");
        assert!(ok.starts_with("HTTP/1.1 200"), "got: {ok}");
    }

    #[test]
    fn wait_returns_error_and_400_on_bad_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || send(addr, &get("/callback?code=abc123&state=wrong")));

        let result = wait_for_callback(&listener, STATE);
        let response = client.join().unwrap();

        assert!(result.is_err());
        assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    }

    #[test]
    fn wait_survives_a_malformed_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || {
            // Garbage then hang up: the loop should log it and keep going.
            let mut junk = TcpStream::connect(addr).unwrap();
            junk.write_all(b"NOT AN HTTP REQUEST\r\n\r\n").unwrap();
            drop(junk);
            send(addr, &get("/callback?code=abc123&state=expected-state"))
        });

        let code = wait_for_callback(&listener, STATE).unwrap();
        client.join().unwrap();
        assert_eq!(code, "abc123");
    }

    #[test]
    #[ignore = "slow: waits out the per-stream read timeout"]
    fn wait_survives_an_idle_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || {
            // Connect and send nothing, like a browser preconnect.
            let _idle = TcpStream::connect(addr).unwrap();
            send(addr, &get("/callback?code=abc123&state=expected-state"))
        });

        let code = wait_for_callback(&listener, STATE).unwrap();
        client.join().unwrap();
        assert_eq!(code, "abc123");
    }
}
