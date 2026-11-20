//! Test-only helpers shared across modules.

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
};

/// The request the fake server received from the client.
pub struct ReceivedRequest {
    pub method: String,
    pub path: String,
    headers: Vec<(String, String)>,
    pub body: String,
}

impl ReceivedRequest {
    /// Value of the first header named `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Serve one canned JSON response on a random local port.
///
/// Returns the server's base URL (`http://127.0.0.1:PORT`, no trailing slash) and a handle that
/// yields the request the client sent. Join the handle to surface any panic in the server thread.
pub fn serve_once(
    status: &'static str,
    body: &'static str,
) -> (String, JoinHandle<ReceivedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let len = body.len();
        write!(
            stream,
            "HTTP/1.1 {status}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {len}\r\n\
             Connection: close\r\n\r\n{body}"
        )
        .unwrap();
        request
    });
    (base, handle)
}

/// Read a whole request (headers plus `Content-Length` bytes).
///
/// Draining the body before replying keeps the client from seeing a connection reset.
fn read_request(stream: &mut impl Read) -> ReceivedRequest {
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
                return ReceivedRequest {
                    method: req.method.unwrap().to_owned(),
                    path: req.path.unwrap().to_owned(),
                    headers: req
                        .headers
                        .iter()
                        .map(|h| {
                            (
                                h.name.to_owned(),
                                String::from_utf8_lossy(h.value).into_owned(),
                            )
                        })
                        .collect(),
                    body: String::from_utf8(buf[head_len..end].to_vec()).unwrap(),
                };
            }
        }
    }
}
