//! Real, certificate-verifying TLS peers for unit and GNU oracle tests.
//! The fixture private key is public test data and has no deployment use.
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

pub fn resources() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/lisp/net/tls-loopback-resources")
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
                // cancellation below keeps failed tests from waiting this long.
                let deadline = Instant::now() + Duration::from_secs(30);
                let socket = loop {
                    if stopped.try_recv().is_ok() {
                        return;
                    }
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => return,
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
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
            worker.join().unwrap();
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
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match listener.accept() {
                    Ok((socket, _)) => {
                        // Bound the defective blocking path without adding a
                        // timeout to the production TLS implementation.
                        let _ = stopped.recv_timeout(Duration::from_secs(1));
                        drop(socket);
                        return;
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => return,
                }
            }
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
            worker.join().unwrap();
        }
    }
}
