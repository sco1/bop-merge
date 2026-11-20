//! Test-only helpers shared across modules.

use std::{
    io::{ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// How long the fake server waits for each expected connection before giving up.
///
/// Keeps a test from hanging if the client makes fewer requests than expected.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(5);

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

/// One canned HTTP response, always sent with `Content-Type: application/json`.
pub struct Reply {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: String,
}

impl Reply {
    pub fn new(status: &'static str, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// Add an extra response header, e.g. `Retry-After`.
    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

/// A fake HTTP server bound to a random local port.
///
/// Binding before serving lets a test build reply bodies that point back at the server
/// (for example a paging `next` URL).
pub struct FakeServer {
    listener: TcpListener,
    base: String,
}

impl FakeServer {
    pub fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        Self { listener, base }
    }

    /// Base URL (`http://127.0.0.1:PORT`, no trailing slash).
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Answer one connection per reply, in order.
    ///
    /// The handle yields every request received. If the client makes fewer requests than
    /// there are replies, the server stops after [`ACCEPT_TIMEOUT`] and returns what it got,
    /// so the test can assert on the count instead of hanging.
    pub fn serve(self, replies: Vec<Reply>) -> JoinHandle<Vec<ReceivedRequest>> {
        thread::spawn(move || serve_replies(&self.listener, replies))
    }
}

/// Serve one canned JSON response on a random local port.
///
/// Returns the server's base URL and a handle that yields the request the client sent.
/// Join the handle to surface any panic in the server thread.
pub fn serve_once(
    status: &'static str,
    body: &'static str,
) -> (String, JoinHandle<ReceivedRequest>) {
    let server = FakeServer::bind();
    let base = server.base().to_owned();
    let handle = thread::spawn(move || {
        serve_replies(&server.listener, vec![Reply::new(status, body)])
            .pop()
            .expect("client made no request")
    });
    (base, handle)
}

fn serve_replies(listener: &TcpListener, replies: Vec<Reply>) -> Vec<ReceivedRequest> {
    listener.set_nonblocking(true).unwrap();
    let mut received = Vec::new();
    for reply in replies {
        let Some(mut stream) = accept_before_deadline(listener) else {
            break;
        };
        received.push(read_request(&mut stream));
        write_reply(&mut stream, &reply);
    }
    received
}

fn accept_before_deadline(listener: &TcpListener) -> Option<TcpStream> {
    let deadline = Instant::now() + ACCEPT_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Some platforms let the accepted socket inherit non-blocking mode.
                stream.set_nonblocking(false).unwrap();
                return Some(stream);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("accept failed: {e}"),
        }
    }
}

fn write_reply(stream: &mut impl Write, reply: &Reply) {
    let mut head = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(reply.body.as_bytes()).unwrap();
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
