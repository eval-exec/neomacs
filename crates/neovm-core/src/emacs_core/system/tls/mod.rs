//! TLS transport and GNU-compatible TLS facade support.
//!
//! Rustls is the default transport backend, but it is deliberately kept behind
//! this module so process management and Elisp builtins do not depend on
//! rustls-specific types.

use super::builtins::{EvalResult, signal};
use super::error::Flow;
use super::value::Value;
use super::value::list_to_vec;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use base64::Engine;
use rustls_pki_types::{CertificateDer, pem::PemObject};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use x509_parser::prelude::{FromDer, X509Certificate, parse_x509_pem};

pub(crate) fn gnutls_available_capabilities() -> &'static [&'static str] {
    &["ciphers", "macs", "digests", "gnutls3", "gnutls"]
}

pub(crate) fn builtin_neomacs_tls_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-tls-available-p", &args, 0)?;
    Ok(Value::T)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GnutlsCredentialType {
    X509Pki,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GnutlsBootParameters {
    pub(crate) credential_type: GnutlsCredentialType,
    pub(crate) client: TlsClientParameters,
}

/// Certificate roots a TLS client must use for peer verification.
///
/// GNU always loads system roots before adding every `:trustfiles` entry.  The
/// two variants keep the additive nature explicit: a Lisp trust file augments
/// the defaults instead of replacing them or disabling verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TlsTrustRoots {
    Default,
    DefaultPlusFiles(Vec<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TlsClientParameters {
    pub(crate) hostname: String,
    pub(crate) trust_roots: TlsTrustRoots,
}

impl TlsClientParameters {
    pub(crate) fn default_roots(hostname: String) -> Self {
        Self {
            hostname,
            trust_roots: TlsTrustRoots::Default,
        }
    }
}

fn plist_first(items: &[Value], key: &str) -> Option<Value> {
    items
        .chunks_exact(2)
        .find(|pair| pair[0].as_symbol_name() == Some(key))
        .map(|pair| pair[1])
}

fn parse_trust_files(items: &[Value]) -> Result<Vec<PathBuf>, Flow> {
    let mut tail = plist_first(items, ":trustfiles").unwrap_or(Value::NIL);
    let mut files = Vec::new();
    while tail.is_cons() {
        let trust_file = tail.cons_car();
        let Some(filename) = trust_file.as_lisp_string() else {
            return Err(signal("error", vec![Value::string("Invalid trustfile")]));
        };
        files.push(crate::emacs_core::fileio::lisp_file_name_to_path_buf(
            filename,
        ));
        tail = tail.cons_cdr();
    }
    Ok(files)
}

pub(crate) fn parse_gnutls_boot_parameters(
    credential_type: Value,
    proplist: Value,
) -> Result<GnutlsBootParameters, Flow> {
    let credential_type = match credential_type.as_symbol_name() {
        Some("gnutls-x509pki") => GnutlsCredentialType::X509Pki,
        Some("gnutls-anon") => {
            return Err(signal(
                "error",
                vec![Value::string(
                    "GnuTLS anonymous credentials are not available",
                )],
            ));
        }
        Some(_) => {
            return Err(signal(
                "error",
                vec![Value::string("Invalid GnuTLS credential type")],
            ));
        }
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), credential_type],
            ));
        }
    };

    let Some(items) = list_to_vec(&proplist) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), proplist],
        ));
    };

    let Some(hostname) = plist_first(&items, ":hostname").and_then(|v| {
        v.as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    }) else {
        return Err(signal(
            "error",
            vec![Value::string(
                "gnutls-boot: invalid :hostname parameter (not a string)",
            )],
        ));
    };
    let trust_files = parse_trust_files(&items)?;
    let trust_roots = if trust_files.is_empty() {
        TlsTrustRoots::Default
    } else {
        TlsTrustRoots::DefaultPlusFiles(trust_files)
    };

    Ok(GnutlsBootParameters {
        credential_type,
        client: TlsClientParameters {
            hostname,
            trust_roots,
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TlsPeerStatus {
    pub(crate) warnings: Vec<&'static str>,
    pub(crate) certificates: Vec<Value>,
    pub(crate) key_exchange: Option<String>,
    pub(crate) protocol: Option<String>,
    pub(crate) cipher: Option<String>,
    pub(crate) mac: Option<String>,
    pub(crate) encrypt_then_mac: Option<bool>,
}

impl TlsPeerStatus {
    fn new() -> Self {
        Self {
            warnings: Vec::new(),
            certificates: Vec::new(),
            key_exchange: None,
            protocol: None,
            cipher: None,
            mac: None,
            encrypt_then_mac: None,
        }
    }
}

pub(crate) enum TlsCloseNotifyResult {
    Success,
    Again,
    Interrupted,
}

pub(crate) fn gnutls_close_notify_result_value(result: TlsCloseNotifyResult) -> Value {
    match result {
        TlsCloseNotifyResult::Success => Value::T,
        TlsCloseNotifyResult::Again => Value::symbol("gnutls-e-again"),
        TlsCloseNotifyResult::Interrupted => Value::symbol("gnutls-e-interrupted"),
    }
}

pub(crate) fn gnutls_peer_status_to_value(status: &TlsPeerStatus) -> Value {
    let mut entries = Vec::new();

    if !status.warnings.is_empty() {
        entries.push(Value::keyword(":warnings"));
        entries.push(Value::list(
            status.warnings.iter().map(Value::keyword).collect(),
        ));
    }

    if !status.certificates.is_empty() {
        let certificates = Value::list(status.certificates.to_vec());
        entries.push(Value::keyword(":certificates"));
        entries.push(certificates);
        entries.push(Value::keyword(":certificate"));
        entries.push(status.certificates[0]);
    }

    if let Some(key_exchange) = &status.key_exchange {
        entries.push(Value::keyword(":key-exchange"));
        entries.push(Value::string(key_exchange.clone()));
    }

    if let Some(protocol) = &status.protocol {
        entries.push(Value::keyword(":protocol"));
        entries.push(Value::string(protocol.clone()));
    }

    if let Some(cipher) = &status.cipher {
        entries.push(Value::keyword(":cipher"));
        entries.push(Value::string(cipher.clone()));
    }

    if let Some(mac) = &status.mac {
        entries.push(Value::keyword(":mac"));
        entries.push(Value::string(mac.clone()));
    }

    if let Some(encrypt_then_mac) = status.encrypt_then_mac {
        entries.push(Value::keyword(":encrypt-then-mac"));
        entries.push(if encrypt_then_mac {
            Value::T
        } else {
            Value::NIL
        });
    }

    Value::list(entries)
}

#[derive(Debug)]
pub(crate) struct CertificateFormatError {
    message: String,
}

impl CertificateFormatError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CertificateFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub(crate) fn format_x509_certificate_pem(
    pem_bytes: &[u8],
) -> Result<String, CertificateFormatError> {
    with_x509_certificate_pem(pem_bytes, format_x509_certificate)
}

pub(crate) fn certificate_details_value_pem(pem: &str) -> Result<Value, CertificateFormatError> {
    with_x509_certificate_pem(pem.as_bytes(), |cert| {
        let validity = cert.validity();
        let mut entries = vec![
            Value::keyword(":version"),
            Value::fixnum(i64::from(cert.version().0 + 1)),
            Value::keyword(":serial-number"),
            Value::string(cert.raw_serial_as_string()),
            Value::keyword(":issuer"),
            Value::string(cert.issuer().to_string()),
            Value::keyword(":valid-from"),
            Value::string(asn1_date_string(&validity.not_before)),
            Value::keyword(":valid-to"),
            Value::string(asn1_date_string(&validity.not_after)),
            Value::keyword(":subject"),
            Value::string(cert.subject().to_string()),
            Value::keyword(":public-key-algorithm"),
            Value::string(public_key_algorithm_name(cert)),
            Value::keyword(":signature-algorithm"),
            Value::string(signature_algorithm_name(cert)),
            Value::keyword(":pem"),
            Value::string(pem.to_owned()),
        ];
        entries.shrink_to_fit();
        Value::list(entries)
    })
}

fn with_x509_certificate_pem<R>(
    pem_bytes: &[u8],
    f: impl FnOnce(&X509Certificate<'_>) -> R,
) -> Result<R, CertificateFormatError> {
    let (_, pem) = parse_x509_pem(pem_bytes)
        .map_err(|err| CertificateFormatError::new(format!("cannot import X.509 PEM: {err}")))?;
    if pem.label != "CERTIFICATE" {
        return Err(CertificateFormatError::new(format!(
            "expected CERTIFICATE PEM block, got {}",
            pem.label
        )));
    }

    let (remaining, cert) = X509Certificate::from_der(&pem.contents)
        .map_err(|err| CertificateFormatError::new(format!("cannot parse X.509 DER: {err}")))?;
    if !remaining.is_empty() {
        return Err(CertificateFormatError::new(
            "trailing data after X.509 certificate",
        ));
    }

    Ok(f(&cert))
}

fn format_x509_certificate(cert: &X509Certificate<'_>) -> String {
    let validity = cert.validity();
    let mut out = String::new();
    out.push_str("X.509 Certificate\n");
    out.push_str(&format!("Version: {}\n", cert.version().0 + 1));
    out.push_str(&format!("Serial Number: {}\n", cert.raw_serial_as_string()));
    out.push_str(&format!("Issuer: {}\n", cert.issuer()));
    out.push_str(&format!("Subject: {}\n", cert.subject()));
    out.push_str(&format!("Not Before: {}\n", validity.not_before));
    out.push_str(&format!("Not After: {}\n", validity.not_after));
    out.push_str(&format!(
        "Public Key Algorithm: {}\n",
        cert.public_key().algorithm.algorithm
    ));
    out.push_str(&format!(
        "Signature Algorithm: {}\n",
        cert.signature_algorithm.algorithm
    ));
    if !cert.extensions().is_empty() {
        out.push_str("Extensions:\n");
        for extension in cert.extensions() {
            out.push_str(&format!(
                "  {}{}\n",
                extension.oid,
                if extension.critical {
                    " (critical)"
                } else {
                    ""
                }
            ));
        }
    }
    out
}

fn asn1_date_string(time: &x509_parser::time::ASN1Time) -> String {
    let datetime = time.to_datetime();
    format!(
        "{:04}-{:02}-{:02}",
        datetime.year(),
        u8::from(datetime.month()),
        datetime.day()
    )
}

fn public_key_algorithm_name(cert: &X509Certificate<'_>) -> String {
    match cert
        .public_key()
        .algorithm
        .algorithm
        .to_id_string()
        .as_str()
    {
        "1.2.840.113549.1.1.1" => "RSA".to_owned(),
        "1.2.840.10045.2.1" => "EC/ECDSA".to_owned(),
        other => other.to_owned(),
    }
}

fn signature_algorithm_name(cert: &X509Certificate<'_>) -> String {
    match cert.signature_algorithm.algorithm.to_id_string().as_str() {
        "1.2.840.113549.1.1.5" => "RSA-SHA1".to_owned(),
        "1.2.840.113549.1.1.11" => "RSA-SHA256".to_owned(),
        "1.2.840.113549.1.1.12" => "RSA-SHA384".to_owned(),
        "1.2.840.113549.1.1.13" => "RSA-SHA512".to_owned(),
        "1.2.840.10045.4.3.2" => "ECDSA-SHA256".to_owned(),
        "1.2.840.10045.4.3.3" => "ECDSA-SHA384".to_owned(),
        "1.2.840.10045.4.3.4" => "ECDSA-SHA512".to_owned(),
        other => other.to_owned(),
    }
}

type RustlsClientStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

pub(crate) struct RustlsTlsStream {
    inner: RustlsClientStream,
    peer_certificates_pem: Vec<String>,
}

/// Backend-neutral TLS stream owned by a Neomacs process.
pub(crate) enum TlsStream {
    Rustls(RustlsTlsStream),
}

impl TlsStream {
    fn rustls(inner: RustlsClientStream, peer_certificates_pem: Vec<String>) -> Self {
        Self::Rustls(RustlsTlsStream {
            inner,
            peer_certificates_pem,
        })
    }

    pub(crate) fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        match self {
            Self::Rustls(stream) => stream.inner.sock.set_nonblocking(nonblocking),
        }
    }

    pub(crate) fn tcp_stream(&self) -> &TcpStream {
        match self {
            Self::Rustls(stream) => &stream.inner.sock,
        }
    }

    pub(crate) fn write_process_input_once(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.set_nonblocking(true)?;
        match self {
            Self::Rustls(stream) => match stream.inner.write(bytes) {
                Ok(n) => {
                    match stream.inner.flush() {
                        Ok(()) => {}
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(err) => return Err(err),
                    }
                    Ok(n)
                }
                Err(err) => Err(err),
            },
        }
    }

    pub(crate) fn read_process_output(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Rustls(stream) => {
                read_rustls_process_output(&mut stream.inner.conn, &mut stream.inner.sock, buf)
            }
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn peer_certificates_pem(&self) -> &[String] {
        match self {
            Self::Rustls(stream) => &stream.peer_certificates_pem,
        }
    }

    pub(crate) fn peer_status(&self) -> TlsPeerStatus {
        match self {
            Self::Rustls(stream) => rustls_peer_status(stream),
        }
    }

    pub(crate) fn send_close_notify(
        &mut self,
        _wait_for_peer: bool,
    ) -> std::io::Result<TlsCloseNotifyResult> {
        match self {
            Self::Rustls(stream) => {
                stream.inner.conn.send_close_notify();
                rustls_complete_io_result(stream)
            }
        }
    }
}

pub(crate) fn read_rustls_process_output(
    connection: &mut rustls::ClientConnection,
    socket: &mut TcpStream,
    buf: &mut [u8],
) -> std::io::Result<usize> {
    if buf.is_empty() {
        return Ok(0);
    }

    loop {
        // Rustls may already have decrypted application data from a previous
        // socket read. Drain it before asking the nonblocking socket for
        // another TLS record: `rustls::Stream::read' flushes pending writes
        // first and can return WouldBlock with plaintext still available.
        match connection.reader().read(buf) {
            Ok(read) => return Ok(read),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }

        match connection.complete_io(socket) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
}

fn rustls_complete_io_result(
    stream: &mut RustlsTlsStream,
) -> std::io::Result<TlsCloseNotifyResult> {
    match stream.inner.conn.complete_io(&mut stream.inner.sock) {
        Ok(_) => Ok(TlsCloseNotifyResult::Success),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(TlsCloseNotifyResult::Again),
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
            Ok(TlsCloseNotifyResult::Interrupted)
        }
        Err(err) => Err(err),
    }
}

fn rustls_peer_status(stream: &RustlsTlsStream) -> TlsPeerStatus {
    let mut status = TlsPeerStatus::new();
    status.certificates = stream
        .peer_certificates_pem
        .iter()
        .map(|cert| certificate_details_value_pem(cert).unwrap_or_else(|_| Value::string(cert)))
        .collect();
    status.protocol = stream
        .inner
        .conn
        .protocol_version()
        .map(rustls_protocol_name);
    if let Some(suite) = stream.inner.conn.negotiated_cipher_suite() {
        let cipher_suite = suite.suite();
        status.key_exchange = Some(rustls_key_exchange_name(cipher_suite, stream));
        status.cipher = Some(rustls_cipher_name(cipher_suite));
        status.mac = Some(rustls_mac_name(cipher_suite));
        status.encrypt_then_mac = Some(false);
    }
    status
}

fn rustls_protocol_name(version: rustls::ProtocolVersion) -> String {
    match version {
        rustls::ProtocolVersion::SSLv2 => "SSL2.0".to_owned(),
        rustls::ProtocolVersion::SSLv3 => "SSL3.0".to_owned(),
        rustls::ProtocolVersion::TLSv1_0 => "TLS1.0".to_owned(),
        rustls::ProtocolVersion::TLSv1_1 => "TLS1.1".to_owned(),
        rustls::ProtocolVersion::TLSv1_2 => "TLS1.2".to_owned(),
        rustls::ProtocolVersion::TLSv1_3 => "TLS1.3".to_owned(),
        other => format!("{other:?}"),
    }
}

fn rustls_cipher_name(cipher_suite: rustls::CipherSuite) -> String {
    match cipher_suite {
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => "AES-128-GCM".to_owned(),
        rustls::CipherSuite::TLS13_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => "AES-256-GCM".to_owned(),
        rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 => {
            "CHACHA20-POLY1305".to_owned()
        }
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256 => "AES-128-CBC".to_owned(),
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => "AES-256-CBC".to_owned(),
        other => format!("{other:?}"),
    }
}

fn rustls_key_exchange_name(cipher_suite: rustls::CipherSuite, stream: &RustlsTlsStream) -> String {
    match cipher_suite {
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384 => "ECDHE-ECDSA".to_owned(),
        rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => "ECDHE-RSA".to_owned(),
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS13_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256 => {
            let auth = stream
                .peer_certificates_pem
                .first()
                .and_then(|pem| certificate_public_key_exchange_name(pem).ok())
                .unwrap_or_else(|| "UNKNOWN".to_owned());
            format!("ECDHE-{auth}")
        }
        other => format!("{other:?}"),
    }
}

fn certificate_public_key_exchange_name(pem: &str) -> Result<String, CertificateFormatError> {
    with_x509_certificate_pem(pem.as_bytes(), |cert| {
        match public_key_algorithm_name(cert).as_str() {
            "RSA" => "RSA".to_owned(),
            "EC/ECDSA" => "ECDSA".to_owned(),
            other => other.to_owned(),
        }
    })
}

fn rustls_mac_name(cipher_suite: rustls::CipherSuite) -> String {
    match cipher_suite {
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS13_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 => "AEAD".to_owned(),
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => "SHA1".to_owned(),
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256 => "SHA256".to_owned(),
        rustls::CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384
        | rustls::CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 => "SHA384".to_owned(),
        other => format!("{other:?}"),
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

/// Error produced by a TLS backend before conversion to GNU-shaped Lisp errors.
#[derive(Debug)]
pub(crate) enum TlsBackendError {
    InvalidHostname(String),
    TrustFile { path: PathBuf, reason: String },
    Connect(String),
    UnexpectedEof,
    Io(std::io::Error),
}

impl std::fmt::Display for TlsBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHostname(host) => write!(f, "Invalid hostname for TLS: {host}"),
            Self::TrustFile { path, reason } => {
                write!(f, "Invalid TLS trust file {}: {reason}", path.display())
            }
            Self::Connect(err) => write!(f, "TLS handshake failed: {err}"),
            Self::UnexpectedEof => f.write_str("TLS handshake: unexpected EOF"),
            Self::Io(err) => write!(f, "TLS handshake: {err}"),
        }
    }
}

/// TLS transport backend boundary.
///
/// The process layer owns backend-neutral `TlsStream` values, while each
/// backend handles its own handshake, certificate roots, and error conversion.
pub(crate) trait TlsClientBackend {
    fn connect_client(
        tcp_stream: TcpStream,
        parameters: &TlsClientParameters,
    ) -> Result<TlsStream, TlsBackendError>;
}

/// Rustls-backed TLS transport implementation.
pub(crate) struct RustlsBackend;

pub(crate) fn rustls_root_store(
    trust_roots: &TlsTrustRoots,
) -> Result<rustls::RootCertStore, TlsBackendError> {
    let mut root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let TlsTrustRoots::DefaultPlusFiles(files) = trust_roots else {
        return Ok(root_store);
    };

    for path in files {
        let pem = std::fs::read(path).map_err(|error| TlsBackendError::TrustFile {
            path: path.clone(),
            reason: error.to_string(),
        })?;
        let certificates = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| TlsBackendError::TrustFile {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        if certificates.is_empty() {
            return Err(TlsBackendError::TrustFile {
                path: path.clone(),
                reason: "no PEM certificates found".to_owned(),
            });
        }
        for certificate in certificates {
            root_store
                .add(certificate)
                .map_err(|error| TlsBackendError::TrustFile {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;
        }
    }
    Ok(root_store)
}

impl TlsClientBackend for RustlsBackend {
    fn connect_client(
        tcp_stream: TcpStream,
        parameters: &TlsClientParameters,
    ) -> Result<TlsStream, TlsBackendError> {
        let root_store = rustls_root_store(&parameters.trust_roots)?;
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let server_name: rustls_pki_types::ServerName<'_> = parameters
            .hostname
            .to_owned()
            .try_into()
            .map_err(|_| TlsBackendError::InvalidHostname(parameters.hostname.clone()))?;

        let mut tls_conn = rustls::ClientConnection::new(Arc::new(config), server_name)
            .map_err(|err| TlsBackendError::Connect(err.to_string()))?;

        tcp_stream
            .set_nonblocking(false)
            .map_err(TlsBackendError::Io)?;
        let mut tcp_stream = tcp_stream;
        rustls_complete_client_handshake(&mut tls_conn, &mut tcp_stream)?;
        let tls_stream = rustls::StreamOwned::new(tls_conn, tcp_stream);

        let peer_certificates_pem = tls_stream
            .conn
            .peer_certificates()
            .map(|certs| {
                certs
                    .iter()
                    .map(|cert| der_certificate_to_pem(cert.as_ref()))
                    .collect()
            })
            .unwrap_or_default();
        let stream = TlsStream::rustls(tls_stream, peer_certificates_pem);
        stream.set_nonblocking(true).ok();
        Ok(stream)
    }
}

fn rustls_complete_client_handshake(
    tls_conn: &mut rustls::ClientConnection,
    tcp_stream: &mut TcpStream,
) -> Result<(), TlsBackendError> {
    while tls_conn.is_handshaking() {
        match tls_conn.complete_io(tcp_stream) {
            Ok((0, 0)) if tls_conn.is_handshaking() => {
                return Err(TlsBackendError::UnexpectedEof);
            }
            Ok(_) => {}
            Err(ref err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(ref err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(TlsBackendError::UnexpectedEof);
            }
            Err(err) => return Err(TlsBackendError::Io(err)),
        }
    }
    Ok(())
}

pub(crate) fn der_certificate_to_pem(der: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 output is ASCII"));
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}
