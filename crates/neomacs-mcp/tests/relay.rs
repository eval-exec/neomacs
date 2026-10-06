//! Process-level tests of the `neomacs-mcp` relay against a fixture
//! Unix listener.  The fixture is transport-only; it does not speak MCP.
#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT_MS: &str = "300";

struct Fixture {
    dir: tempfile::TempDir,
    socket: PathBuf,
    listener: UnixListener,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("mcp-{name}-"))
            .tempdir()
            .unwrap();
        let socket = dir.path().join("mcp");
        let listener = UnixListener::bind(&socket).unwrap();
        Self {
            dir,
            socket,
            listener,
        }
    }

    fn accept(&self) -> UnixStream {
        let (stream, _) = self.listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
    }
}

fn relay(socket: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_neomacs-mcp"))
        .arg("--socket")
        .arg(socket)
        .args(["--timeout-ms", TIMEOUT_MS])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait(child: &mut Child, limit: Duration) -> ExitStatus {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("relay did not exit within {limit:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_exact(reader: &mut impl Read, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes).unwrap();
    bytes
}

fn stderr(child: &mut Child) -> String {
    let mut text = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn copies_bytes_both_ways_without_framing() {
    let fixture = Fixture::new("duplex");
    let mut child = relay(&fixture.socket);
    let mut peer = fixture.accept();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // A UTF-8 character split across writes and two messages in one write.
    let message = "{\"text\":\"Ελλάδα\"}\n{\"id\":2}\n".as_bytes();
    stdin.write_all(&message[..12]).unwrap();
    stdin.flush().unwrap();
    thread::sleep(Duration::from_millis(20));
    stdin.write_all(&message[12..]).unwrap();
    stdin.flush().unwrap();
    assert_eq!(read_exact(&mut peer, message.len()), message);

    // Large binary payloads arrive unchanged in both directions.
    let large: Vec<u8> = (0..300_000u32).map(|n| (n % 251) as u8).collect();
    let writer = {
        let large = large.clone();
        thread::spawn(move || {
            stdin.write_all(&large).unwrap();
            stdin
        })
    };
    assert_eq!(read_exact(&mut peer, large.len()), large);
    let stdin = writer.join().unwrap();
    let echo = thread::spawn({
        let mut peer = peer.try_clone().unwrap();
        let large = large.clone();
        move || peer.write_all(&large).unwrap()
    });
    assert_eq!(read_exact(&mut stdout, large.len()), large);
    echo.join().unwrap();

    drop(stdin);
    drop(peer);
    assert!(wait(&mut child, Duration::from_secs(5)).success());
}

#[test]
fn stdin_eof_half_closes_and_delivers_late_response() {
    let fixture = Fixture::new("eof");
    let mut child = relay(&fixture.socket);
    let mut peer = fixture.accept();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"request\n").unwrap();
    drop(stdin);

    let mut received = Vec::new();
    peer.read_to_end(&mut received).unwrap();
    assert_eq!(received, b"request\n");
    peer.write_all(b"response\n").unwrap();
    drop(peer);

    let mut stdout = child.stdout.take().unwrap();
    let mut output = Vec::new();
    stdout.read_to_end(&mut output).unwrap();
    assert_eq!(output, b"response\n");
    assert!(wait(&mut child, Duration::from_secs(5)).success());
    // The relay never removes or replaces the editor's socket.
    assert!(fixture.socket.exists());
}

#[test]
fn stdin_eof_without_peer_close_times_out() {
    let fixture = Fixture::new("drain");
    let mut child = relay(&fixture.socket);
    let _peer = fixture.accept();
    drop(child.stdin.take());
    let status = wait(&mut child, Duration::from_secs(5));
    assert_eq!(status.code(), Some(1));
    assert!(stderr(&mut child).contains("stdin EOF drain deadline expired"));
}

#[test]
fn peer_close_exits_while_stdin_is_open() {
    let fixture = Fixture::new("peer-close");
    let mut child = relay(&fixture.socket);
    drop(fixture.accept());
    let _stdin = child.stdin.take();
    assert!(wait(&mut child, Duration::from_secs(5)).success());
}

#[test]
fn unread_socket_write_is_bounded() {
    let fixture = Fixture::new("unread");
    let mut child = relay(&fixture.socket);
    let _peer = fixture.accept();
    let mut stdin = child.stdin.take().unwrap();
    // The peer never reads, so the relay's socket write blocks.
    let writer = thread::spawn(move || {
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..256 {
            if stdin.write_all(&chunk).is_err() {
                break;
            }
        }
    });
    let status = wait(&mut child, Duration::from_secs(10));
    assert_eq!(status.code(), Some(1));
    assert!(stderr(&mut child).contains("upstream write deadline expired"));
    writer.join().unwrap();
}

#[test]
fn missing_socket_fails_without_fallback() {
    let fixture = Fixture::new("missing");
    let mut child = relay(&fixture.dir.path().join("absent"));
    let status = wait(&mut child, Duration::from_secs(5));
    assert_eq!(status.code(), Some(1));
    assert!(stderr(&mut child).contains("connect"));
}

#[test]
fn rejects_invalid_arguments() {
    for args in [&[][..], &["--socket", "relative"][..]] {
        let output = Command::new(env!("CARGO_BIN_EXE_neomacs-mcp"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage: neomacs-mcp"));
    }
}
