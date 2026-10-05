//! Copy bytes between stdio and one explicitly named Unix socket.
//!
//! The relay does not parse MCP, retry, reconnect, or start an editor.
//! Blocking stdio workers are deliberately not joined: this executable owns
//! the process, and the bounded supervisor exits it after closing its socket.
//! Process exit is the cancellation boundary for an inherited blocked pipe.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

const CHUNK: usize = 16 * 1024;
const USAGE: &str = "usage: neomacs-mcp --socket ABSOLUTE-PATH [--timeout-ms 1..60000]";

struct Config {
    socket: PathBuf,
    timeout: Duration,
}

impl Config {
    fn parse(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, String> {
        let mut args = args;
        let mut socket = None;
        let mut timeout = None;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--socket") if socket.is_none() => {
                    socket = Some(PathBuf::from(args.next().ok_or(USAGE)?));
                }
                Some("--timeout-ms") if timeout.is_none() => {
                    let value = args.next().ok_or(USAGE)?;
                    let millis = value
                        .to_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|n| (1..=60_000).contains(n))
                        .ok_or(USAGE)?;
                    timeout = Some(Duration::from_millis(millis));
                }
                _ => return Err(USAGE.into()),
            }
        }
        let socket = socket.filter(|s| s.is_absolute()).ok_or(USAGE)?;
        Ok(Self {
            socket,
            timeout: timeout.unwrap_or(Duration::from_secs(2)),
        })
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Upstream,
    Stdout,
}

enum Event {
    Writing(Direction),
    Wrote(Direction),
    InputEof,
    PeerEof,
    Error(String),
}

// Two fixed-size buffers plus four small control events. No payload queues.
fn copy_chunks(
    mut read: impl Read,
    mut write: impl Write,
    direction: Direction,
    events: &SyncSender<Event>,
) -> io::Result<()> {
    let mut bytes = [0; CHUNK];
    loop {
        let size = match read.read(&mut bytes) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if size == 0 {
            return Ok(());
        }
        events
            .send(Event::Writing(direction))
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        write.write_all(&bytes[..size])?;
        write.flush()?;
        events
            .send(Event::Wrote(direction))
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
    }
}

struct Connection(UnixStream);
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

fn supervise(events: Receiver<Event>, timeout: Duration) -> Result<(), String> {
    let mut upstream = None;
    let mut stdout = None;
    let mut drain = None;
    loop {
        let now = Instant::now();
        for (deadline, label) in [
            (upstream, "upstream write"),
            (stdout, "stdout write"),
            (drain, "stdin EOF drain"),
        ] {
            if deadline.is_some_and(|d| now >= d) {
                return Err(format!(
                    "{label} deadline expired; connection retired (delivery may be incomplete)"
                ));
            }
        }
        let wait = [upstream, stdout, drain]
            .into_iter()
            .flatten()
            .min()
            .map(|d| d.saturating_duration_since(now))
            .unwrap_or(Duration::from_secs(60));
        match events.recv_timeout(wait) {
            Ok(Event::Writing(d)) => {
                let deadline = Some(Instant::now() + timeout);
                match d {
                    Direction::Upstream => upstream = deadline,
                    Direction::Stdout => stdout = deadline,
                }
            }
            Ok(Event::Wrote(d)) => match d {
                Direction::Upstream => upstream = None,
                Direction::Stdout => stdout = None,
            },
            Ok(Event::InputEof) => drain = Some(Instant::now() + timeout),
            Ok(Event::PeerEof) => return Ok(()),
            Ok(Event::Error(e)) => return Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("transport workers disconnected".into());
            }
        }
    }
}

fn relay(config: Config) -> Result<(), String> {
    // Even connect can block on a full Unix listener backlog. Bound it without
    // FFI or changing inherited descriptors' flags (which affect their owners).
    let (connected_tx, connected_rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("mcp-connect".into())
        .spawn(move || {
            let _ = connected_tx.send(UnixStream::connect(config.socket));
        })
        .map_err(|e| format!("connect worker: {e}"))?;
    let stream = connected_rx
        .recv_timeout(config.timeout)
        .map_err(|e| format!("connect deadline/worker: {e}"))?
        .map_err(|e| format!("connect: {e}"))?;
    let connection = Connection(stream);
    let inbound = connection
        .0
        .try_clone()
        .map_err(|e| format!("clone inbound socket: {e}"))?;
    let outbound = connection
        .0
        .try_clone()
        .map_err(|e| format!("clone outbound socket: {e}"))?;
    let (tx, rx) = mpsc::sync_channel(4);
    let input_tx = tx.clone();
    thread::Builder::new()
        .name("mcp-input".into())
        .spawn(move || {
            match copy_chunks(io::stdin().lock(), &inbound, Direction::Upstream, &input_tx) {
                Ok(()) => {
                    let _ = input_tx.send(Event::InputEof);
                    if let Err(e) = inbound.shutdown(Shutdown::Write) {
                        let _ = input_tx.send(Event::Error(format!("stdin EOF half-close: {e}")));
                    }
                }
                Err(e) => {
                    let _ = input_tx.send(Event::Error(format!("stdin to socket: {e}")));
                }
            }
        })
        .map_err(|e| format!("stdin worker: {e}"))?;
    thread::Builder::new()
        .name("mcp-output".into())
        .spawn(move || {
            let event = match copy_chunks(&outbound, io::stdout().lock(), Direction::Stdout, &tx) {
                Ok(()) => Event::PeerEof,
                Err(e) => Event::Error(format!("socket to stdout: {e}")),
            };
            let _ = tx.send(event);
        })
        .map_err(|e| format!("stdout worker: {e}"))?;
    supervise(rx, config.timeout)
}

pub(crate) fn main() {
    let result = Config::parse(std::env::args_os().skip(1)).and_then(relay);
    let code = if let Err(error) = result {
        // Diagnostics must not hang retirement when stderr is itself a full pipe.
        // Best effort: give the small message 20ms, then terminate every worker.
        let (tx, rx) = mpsc::sync_channel(1);
        if thread::Builder::new()
            .name("mcp-diagnostic".into())
            .spawn(move || {
                let _ = writeln!(io::stderr().lock(), "neomacs-mcp: {error}");
                let _ = tx.send(());
            })
            .is_ok()
        {
            let _ = rx.recv_timeout(Duration::from_millis(20));
        }
        1
    } else {
        0
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Config, String> {
        Config::parse(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn requires_one_absolute_socket() {
        let config = parse(&["--socket", "/tmp/mcp"]).unwrap();
        assert_eq!(config.socket, PathBuf::from("/tmp/mcp"));
        assert_eq!(config.timeout, Duration::from_secs(2));
        for args in [
            &[][..],
            &["--socket"][..],
            &["--socket", "relative"][..],
            &["--socket", "/a", "--socket", "/b"][..],
            &["/tmp/mcp"][..],
            &["--socket", "/tmp/mcp", "--extra"][..],
        ] {
            assert_eq!(parse(args).err().as_deref(), Some(USAGE), "{args:?}");
        }
    }

    #[test]
    fn bounds_timeout() {
        let config = parse(&["--timeout-ms", "250", "--socket", "/tmp/mcp"]).unwrap();
        assert_eq!(config.timeout, Duration::from_millis(250));
        for value in ["0", "60001", "-1", "x"] {
            assert!(parse(&["--socket", "/tmp/mcp", "--timeout-ms", value]).is_err());
        }
    }
}
