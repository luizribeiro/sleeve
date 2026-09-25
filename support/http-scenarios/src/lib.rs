//! Reusable HTTP policy scenarios for native and JavaScript hosts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Selects the behavior exercised by the shared plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum Scenario {
    /// Reads a secret note before attempting an allowed fetch.
    ReadThenFetch,
    /// Fetches an allowed origin before reading a secret note.
    FetchThenRead,
    /// Reads a secret note while a request body writer remains open.
    ReadWithOpenBody,
    /// Sends and closes a body before reading a secret note.
    CloseBodyThenRead,
    /// Sends a request body larger than the configured limit.
    BodyOverLimit,
    /// Fetches an origin absent from the allowlist.
    FetchBlockedOrigin,
    /// Fetches the allowed origin with an uppercase host spelling.
    FetchNormalizedOrigin,
    /// Sends an invalid request, then reads a secret note.
    InvalidRequestThenRead,
}

/// Identifies the rule expected to decide a scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// Opening a public writable channel at secret is refused.
    ChannelOpenVeto,
    /// An open writable channel prevents a secret-label raise.
    OpenChannelRaise,
    /// The normalized origin must be allowlisted.
    OriginAllowlist,
    /// The configured request-body byte limit is enforced.
    BodyLimit,
    /// Request validation rejects an incomplete origin and cleans up channels.
    OriginValidation,
    /// No refusal applies to the operation ordering.
    Allowed,
}

/// Chooses which authority spelling the host passes to the plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Authority {
    /// The exact allowlisted authority.
    Allowed,
    /// An authority absent from the allowlist.
    Blocked,
    /// The allowed authority with its hostname uppercased.
    UppercaseAllowed,
}

/// Expected result of a scenario invocation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Expected {
    /// The plugin returns this value normally.
    Returned(String),
    /// Policy denial traps the plugin export.
    Trapped,
}

/// One reusable plugin scenario and its expected decision.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
pub struct Case {
    /// Human-readable scenario identifier.
    pub name: String,
    /// Numeric scenario accepted by the plugin export.
    pub input: u8,
    /// Authority spelling to pass.
    pub authority: Authority,
    /// Request body size in bytes.
    pub body_size: u32,
    /// Expected plugin outcome.
    pub expected: Expected,
    /// Policy or validation rule that decides the case.
    pub rule: Rule,
}

/// Reads the scenarios exercised under every IFC sleeve variant and host.
///
/// # Errors
///
/// Returns an error if the embedded shared scenario data is invalid.
pub fn cases() -> Result<Vec<Case>, serde_json::Error> {
    serde_json::from_str(include_str!("../scenarios.json"))
}

/// A loopback HTTP server used by tests and examples.
pub struct LocalServer {
    authority: String,
    stopping: Arc<AtomicBool>,
    errors: Receiver<String>,
    thread: Option<JoinHandle<()>>,
}

impl LocalServer {
    /// Starts a server on an ephemeral loopback port.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the listener cannot be created or configured.
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = Arc::clone(&stopping);
        let (error_sender, errors) = mpsc::channel();
        let thread = thread::spawn(move || serve(&listener, &thread_stopping, &error_sender));
        Ok(Self {
            authority: format!("localhost:{port}"),
            stopping,
            errors,
            thread: Some(thread),
        })
    }

    /// Returns the allowlisted origin, including its explicit port.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("http://{}", self.authority)
    }

    /// Returns the authority accepted by the local server.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// Returns the allowed authority with an uppercase hostname.
    #[must_use]
    pub fn uppercase_authority(&self) -> String {
        self.authority.replace("localhost", "LOCALHOST")
    }

    /// Returns a reachable authority whose normalized origin is not allowlisted.
    #[must_use]
    pub fn blocked_authority(&self) -> String {
        self.authority.replace("localhost", "127.0.0.1")
    }

    /// Reports an error raised while serving a connection.
    ///
    /// # Errors
    ///
    /// Returns the first server error that has not already been reported.
    pub fn check(&self) -> io::Result<()> {
        match self.errors.try_recv() {
            Ok(error) => Err(io::Error::other(error)),
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "loopback server stopped unexpectedly",
            )),
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _joined = thread.join();
        }
    }
}

fn serve(listener: &TcpListener, stopping: &AtomicBool, errors: &Sender<String>) {
    let mut connections = Vec::new();
    while !stopping.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = stream.set_nonblocking(false) {
                    report(errors, "configure accepted connection", &error);
                    continue;
                }
                let connection_errors = errors.clone();
                connections.push(thread::spawn(move || {
                    if let Err(error) = respond(stream) {
                        report(&connection_errors, "serve connection", &error);
                    }
                }));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => {
                report(errors, "accept connection", &error);
                return;
            }
        }
        reap_finished(&mut connections, errors);
    }
    for connection in connections {
        if connection.join().is_err() {
            report_message(errors, "connection worker panicked");
        }
    }
}

fn reap_finished(connections: &mut Vec<JoinHandle<()>>, errors: &Sender<String>) {
    while let Some(index) = connections.iter().position(JoinHandle::is_finished) {
        let connection = connections.swap_remove(index);
        if connection.join().is_err() {
            report_message(errors, "connection worker panicked");
        }
    }
}

fn report(errors: &Sender<String>, operation: &str, error: &io::Error) {
    report_message(
        errors,
        &format!("loopback server failed to {operation}: {error}"),
    );
}

fn report_message(errors: &Sender<String>, message: &str) {
    let _reported = errors.send(message.to_owned());
}

fn respond(mut stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = &request[..header_end];
    let body_length = content_length(headers);
    let chunked = is_chunked(headers);
    while request.len() < header_end + body_length
        || (chunked && !chunked_body_complete(&request[header_end..]))
    {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
    stream.flush()?;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_chunked(headers: &[u8]) -> bool {
    let Ok(headers) = str::from_utf8(headers) else {
        return false;
    };
    headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value
                    .split(',')
                    .any(|coding| coding.trim().eq_ignore_ascii_case("chunked"))
        })
    })
}

fn chunked_body_complete(body: &[u8]) -> bool {
    body.windows(5).any(|bytes| bytes == b"0\r\n\r\n")
}

fn content_length(headers: &[u8]) -> usize {
    let Ok(headers) = str::from_utf8(headers) else {
        return 0;
    };
    headers.lines().find_map(parse_content_length).unwrap_or(0)
}

fn parse_content_length(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    name.eq_ignore_ascii_case("content-length")
        .then(|| value.trim().parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_complete_chunked_bodies() {
        assert!(is_chunked(
            b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
        ));
        assert!(chunked_body_complete(b"4\r\ndata\r\n0\r\n\r\n"));
        assert!(!chunked_body_complete(b"4\r\ndata\r\n"));
    }

    #[test]
    fn accepts_a_body_that_arrives_after_its_headers() {
        let server = LocalServer::start().unwrap();
        let mut stream = TcpStream::connect(server.authority()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 4\r\n\r\n")
            .unwrap();
        thread::sleep(Duration::from_millis(50));
        stream.write_all(b"data").unwrap();
        let mut response = [0_u8; 64];
        let read = stream.read(&mut response).unwrap();
        server.check().unwrap();
        assert!(response[..read].starts_with(b"HTTP/1.1 200 OK"));
    }
}
