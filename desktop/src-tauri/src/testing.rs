//! What the shell's tests share: a loopback HTTP server that answers what a
//! test scripted and keeps every request it was asked, for the modules
//! that speak to the node over a raw socket (`terminal`, `logging`,
//! `sidecar`). Test code only; the meter leaves it out.

#[cfg(test)]
pub(crate) mod canned {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// One request as the server read it.
    #[derive(Clone, Debug)]
    pub(crate) struct Request {
        pub method: String,
        pub path: String,
        pub head: String,
        pub body: String,
    }

    impl Request {
        /// A header's value, by name, case-insensitively.
        pub(crate) fn header(&self, name: &str) -> Option<String> {
            self.head.lines().skip(1).find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_string())
            })
        }
    }

    /// A server on a loopback port and what it has been asked so far.
    pub(crate) struct Server {
        pub port: u16,
        requests: Arc<Mutex<Vec<Request>>>,
    }

    impl Server {
        pub(crate) fn base(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        pub(crate) fn requests(&self) -> Vec<Request> {
            self.requests.lock().unwrap().clone()
        }

        /// The requests once at least `n` have landed, within ten seconds.
        pub(crate) fn wait_for(&self, n: usize) -> Vec<Request> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let seen = self.requests();
                if seen.len() >= n {
                    return seen;
                }
                assert!(
                    Instant::now() < deadline,
                    "only {} of {n} requests arrived: {seen:?}",
                    seen.len()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    /// A server on a free port: every request is answered with the status
    /// and body `answer` says, sent whole with its length, and closed.
    pub(crate) fn serve(answer: impl Fn(&Request) -> (u16, String) + Send + 'static) -> Server {
        serve_at(0, answer)
    }

    /// [`serve`] on a named port — the one a supervisor chose for its node.
    pub(crate) fn serve_at(
        port: u16,
        answer: impl Fn(&Request) -> (u16, String) + Send + 'static,
    ) -> Server {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    break;
                };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let (status, body) = answer(&request);
                seen.lock().unwrap().push(request);
                let reply = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    reason(status),
                    body.len()
                );
                if stream
                    .write_all(reply.as_bytes())
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    // The asker went before reading its answer: nothing to say.
                }
            }
        });
        Server { port, requests }
    }

    fn reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            _ => "Internal Server Error",
        }
    }

    /// The whole request off the socket: the head, then as many body
    /// bytes as its content length says.
    fn read_request(stream: &mut TcpStream) -> Option<Request> {
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break at;
            }
            let n = stream.read(&mut chunk).ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        let mut words = head.lines().next().unwrap_or_default().split_whitespace();
        let method = words.next().unwrap_or_default().to_string();
        let path = words.next().unwrap_or_default().to_string();
        let length: usize = head
            .lines()
            .skip(1)
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or(0);
        let mut body = buf[head_end + 4..].to_vec();
        while body.len() < length {
            let n = stream.read(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        Some(Request {
            method,
            path,
            head,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }
}
