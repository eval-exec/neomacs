//! TLS stream state owned by the evaluator layer (stage-4 W2/E6c-A2
//! TYPE-DOWN).
//!
//! Peer ruling 16:2xZ: the process state unit extends down to the TLS stream
//! cluster so the future `neovm-eval` crate can name
//! `Process::live_io.tls_stream`.  This module holds the state definitions
//! and their state-only impls (construction, socket/certificate accessors).
//! The handshake and stream IO stay in `emacs_core::system::tls` as the
//! `TlsStreamIo` extension trait, implemented for this type in the upper
//! crate (orphan-legal: the trait is core-local).
//!
//! `std::io::Read`/`Write` are the one orphan-forced exception: a foreign
//! trait cannot be implemented for this type in the upper crate after the
//! split, so these thin rustls delegations move with the type.  They name
//! only rustls/std types (no system-module dependency).
//!
//! This unit adds `rustls` as an eval-only dependency, copied verbatim from
//! core's workspace entry (no features added).

use std::io::{Read, Write};
use std::net::TcpStream;

pub type RustlsClientStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

pub struct RustlsTlsStream {
    pub inner: RustlsClientStream,
    pub peer_certificates_pem: Vec<String>,
}

/// Backend-neutral TLS stream owned by a Neomacs process.
pub enum TlsStream {
    Rustls(RustlsTlsStream),
}

impl TlsStream {
    pub fn rustls(inner: RustlsClientStream, peer_certificates_pem: Vec<String>) -> Self {
        Self::Rustls(RustlsTlsStream {
            inner,
            peer_certificates_pem,
        })
    }

    pub fn tcp_stream(&self) -> &TcpStream {
        match self {
            Self::Rustls(stream) => &stream.inner.sock,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn peer_certificates_pem(&self) -> &[String] {
        match self {
            Self::Rustls(stream) => &stream.peer_certificates_pem,
        }
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Rustls(stream) => stream.inner.read(buf),
        }
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Rustls(stream) => stream.inner.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Rustls(stream) => stream.inner.flush(),
        }
    }
}
