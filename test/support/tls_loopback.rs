//! Real, certificate-verifying TLS peers for unit and GNU oracle tests.
//! The fixture private key is public test data and has no deployment use.
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

pub fn resources() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/lisp/net/tls-loopback-resources")
}

/// Upper bound on any single wait inside a peer.  It exists only so a client
/// that hangs cannot wedge the suite; it is a watchdog, never a pacing
/// assumption, and is far above the worst stall seen on a saturated machine.
const WATCHDOG: Duration = Duration::from_secs(10);

/// Accept one connection on a listener that `spawn` bound and put into
/// listening state before returning its port.  The listener stays open for as
/// long as the caller waits, so a client that connects late (a loaded machine
/// can delay the Lisp side by seconds) completes in the backlog instead of
/// being refused.  Waiting ends only when the owner cancels via `stopped`
/// (message or dropped sender), or on an accept error.
fn accept_until_stopped(
    listener: &TcpListener,
    stopped: &std::sync::mpsc::Receiver<()>,
) -> Result<Option<TcpStream>, std::io::Error> {
    loop {
        match stopped.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(None),
        }
        match listener.accept() {
            Ok((socket, _)) => {
                // Darwin inherits O_NONBLOCK from the listener; these workers
                // require blocking I/O with timeouts.
                socket.set_nonblocking(false)?;
                return Ok(Some(socket));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(error),
        }
    }
}

pub struct TlsPeer {
    pub port: u16,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl TlsPeer {
    pub fn spawn(connections: usize, delay: Duration, echo: bool) -> Self {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from_pem_file(resources().join("server.pem")).unwrap()],
                PrivateKeyDer::from_pem_file(resources().join("server-key.pem")).unwrap(),
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stopped) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let config = Arc::new(config);
            for _ in 0..connections {
                // GNU and Neomacs oracle processes bootstrap separately;
                // cancellation keeps failed tests from waiting on accept.
                let socket = match accept_until_stopped(&listener, &stopped) {
                    Ok(Some(socket)) => socket,
                    Ok(None) | Err(_) => return,
                };
                socket.set_read_timeout(Some(WATCHDOG)).unwrap();
                socket.set_write_timeout(Some(WATCHDOG)).unwrap();
                thread::sleep(delay);
                let mut stream = rustls::StreamOwned::new(
                    rustls::ServerConnection::new(config.clone()).unwrap(),
                    socket,
                );
                if echo {
                    let mut input = [0; 128];
                    if let Ok(count) = stream.read(&mut input) {
                        let _ = stream.write_all(&input[..count]);
                    }
                } else {
                    let _ = stream.write_all(b"verified loopback\n");
                }
                let _ = stream.flush();
            }
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for TlsPeer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            // Cleanup must not replace an assertion failure or panic again
            // while the test is already unwinding.
            let _ = worker.join();
        }
    }
}

/// A fixed-length encrypted echo whose lifetime is controlled by its caller.
/// Unlike the one-read peer, this fixture verifies the complete application
/// message and keeps the transport open until the Lisp client is cleaned up.
/// All socket/TLS state belongs to the worker; cancellation crosses threads
/// only through the channel. Failed clients cannot leave an unbounded join.
pub struct FramedEchoPeer {
    pub port: u16,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}

impl FramedEchoPeer {
    pub fn spawn(expected: &'static [u8], delay: Duration) -> Self {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from_pem_file(resources().join("server.pem")).unwrap()],
                PrivateKeyDer::from_pem_file(resources().join("server-key.pem")).unwrap(),
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stopped) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let socket = accept_until_stopped(&listener, &stopped)
                .map_err(|error| format!("accept framed TLS client: {error}"))?
                .ok_or_else(|| "TLS client cancelled before connecting".to_owned())?;
            socket
                .set_read_timeout(Some(WATCHDOG))
                .map_err(|error| format!("set TLS read timeout: {error}"))?;
            socket
                .set_write_timeout(Some(WATCHDOG))
                .map_err(|error| format!("set TLS write timeout: {error}"))?;
            thread::sleep(delay);
            let connection = rustls::ServerConnection::new(Arc::new(config))
                .map_err(|error| format!("create framed TLS server: {error}"))?;
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut received = vec![0; expected.len()];
            stream
                .read_exact(&mut received)
                .map_err(|error| format!("read framed TLS message: {error}"))?;
            if received != expected {
                return Err(format!("unexpected framed TLS message: {received:?}"));
            }
            stream
                .write_all(&received)
                .and_then(|()| stream.flush())
                .map_err(|error| format!("echo framed TLS message: {error}"))?;
            stopped
                .recv_timeout(WATCHDOG)
                .map_err(|error| format!("TLS client did not finish after echo: {error}"))?;
            Ok(())
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }

    /// Cancel the controlled lifetime and report protocol/transport failures.
    pub fn finish(mut self) -> Result<(), String> {
        let _ = self.stop.send(());
        self.worker
            .take()
            .ok_or_else(|| "TLS worker already joined".to_owned())?
            .join()
            .map_err(|_| "framed TLS worker panicked".to_owned())?
    }
}

impl Drop for FramedEchoPeer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            // Explicit finish reports failures; cleanup must not panic while
            // another test assertion is unwinding.
            let _ = worker.join();
        }
    }
}

pub struct StalledPeer {
    pub port: u16,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl StalledPeer {
    pub fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stopped) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let Ok(Some(socket)) = accept_until_stopped(&listener, &stopped) else {
                return;
            };
            // Hold the stalled connection until the test finishes; the
            // watchdog bounds the defective blocking path without adding a
            // timeout to the production TLS implementation.
            let _ = stopped.recv_timeout(WATCHDOG);
            drop(socket);
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for StalledPeer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            // Cleanup must not replace an assertion failure or panic again
            // while the test is already unwinding.
            let _ = worker.join();
        }
    }
}
