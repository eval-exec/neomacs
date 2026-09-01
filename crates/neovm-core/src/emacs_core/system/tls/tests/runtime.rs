use super::tls::{
    GnutlsCredentialType, TlsBackendError, TlsCloseNotifyResult, TlsPeerStatus, TlsTrustRoots,
    certificate_details_value_pem, der_certificate_to_pem, format_x509_certificate_pem,
    gnutls_available_capabilities, gnutls_close_notify_result_value, gnutls_peer_status_to_value,
    parse_gnutls_boot_parameters, read_rustls_process_output, rustls_root_store,
};
use super::value::Value;
use crate::emacs_core::builtins::gnutls::{
    builtin_gnutls_error_fatalp, builtin_gnutls_error_string,
    builtin_gnutls_peer_status_warning_describe,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error, ServerConfig, ServerConnection,
    SignatureScheme,
};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject};
use std::io::{Cursor, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

const TEST_CERTIFICATE_PEM: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIFWzCCBEOgAwIBAgISAyBIAwu7NBD5CTxX8suDCMgFMA0GCSqGSIb3DQEBCwUA\n",
    "MEoxCzAJBgNVBAYTAlVTMRYwFAYDVQQKEw1MZXQncyBFbmNyeXB0MSMwIQYDVQQD\n",
    "ExpMZXQncyBFbmNyeXB0IEF1dGhvcml0eSBYMzAeFw0xOTA3MTIxMTEyMzBaFw0x\n",
    "OTEwMTAxMTEyMzBaMB0xGzAZBgNVBAMTEmxpc3RzLmZvci1vdXIuaW5mbzCCASIw\n",
    "DQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMVoti34X46DaI2nX24C+aZ2Ofkm\n",
    "hKbidiXiRTon1MLSMGl1oNW9MyRyYYCzP4j6DNKChJnr8ZnVShh2oZD+yHWP9lpn\n",
    "XMGkbsUxejRMU9hnaAB50pXRIDAzavkVFCguFlJ8nKkv/Y1Avlw7tc2aZOd3lOZB\n",
    "Er8gJ8mRDGqqsNU+Z12I6slEstzGMpsq6AewCVw4lMjdWWgugzUrxQTRAsG87on6\n",
    "gOiQH2cMODN3L7Fq4KOLQIjb3/luQhAQhpdKmEGFLin3c+f5or3thCDuwwDtOU1l\n",
    "Zf+8t9S8pZPLrZrIs6H2xjXqCRuUY7iRNbO18Ukc6rlDYhBj9LT+cpmBbHECAwEA\n",
    "AaOCAmYwggJiMA4GA1UdDwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYI\n",
    "KwYBBQUHAwIwDAYDVR0TAQH/BAIwADAdBgNVHQ4EFgQUJj2pvRtl3GloH3He6FX1\n",
    "ds3X0VEwHwYDVR0jBBgwFoAUqEpqYwR93brm0Tm3pkVl7/Oo7KEwbwYIKwYBBQUH\n",
    "AQEEYzBhMC4GCCsGAQUFBzABhiJodHRwOi8vb2NzcC5pbnQteDMubGV0c2VuY3J5\n",
    "cHQub3JnMC8GCCsGAQUFBzAChiNodHRwOi8vY2VydC5pbnQteDMubGV0c2VuY3J5\n",
    "cHQub3JnLzAdBgNVHREEFjAUghJsaXN0cy5mb3Itb3VyLmluZm8wTAYDVR0gBEUw\n",
    "QzAIBgZngQwBAgEwNwYLKwYBBAGC3xMBAQEwKDAmBggrBgEFBQcCARYaaHR0cDov\n",
    "L2Nwcy5sZXRzZW5jcnlwdC5vcmcwggEDBgorBgEEAdZ5AgQCBIH0BIHxAO8AdgAp\n",
    "PFGWVMg5ZbqqUPxYB9S3b79Yeily3KTDDPTlRUf0eAAAAWvmGV7yAAAEAwBHMEUC\n",
    "ICQL2Sm14aCMLxX9a9RbySgyBfichMRdbu6QA2Mbrl4eAiEA1vgJ7snqUWCgoqEE\n",
    "3SEfK3ioMopzWBsPvG6LdCuCMRAAdQBvU3asMfAxGdiZAKRRFf93FRwR2QLBACkG\n",
    "jbIImjfZEwAAAWvmGV9oAAAEAwBGMEQCIExGqw3Lo0nSCyUuTRf92FgGASwWYji5\n",
    "UGnXuYnpJrAvAiBw8AWVag8fzZ4ogAhY9EFRNdLrUcBjStipL888vyuxKzANBgkq\n",
    "hkiG9w0BAQsFAAOCAQEAF8BBLDvSWZg57B6aDtzfUTSGetCYs3k0vJqCJlL+Pz7/\n",
    "UruCSsojQzp5R6jvvgYQ83MaIdwe2mgt+OCQB5v7ylctyBzBmYIw9nPnxEC7HlcJ\n",
    "L2K/k5ZjJFRnv4kV1Si8+TIpEAV0ksf39KGKemG8kGi4GXV1v03zSv0p8aCarpuo\n",
    "SKBJ4qlB0CvmS2MqV4KnzO0O2h0c/ZQ4jg7l53eiN7VPdRMMO1DRw+MaW6I/hEZp\n",
    "+oZQ7hhKXgKUBvF4IGwyrfyIZ8AeWKG4IP98COgyRbz7qtrAVevRKCM0ZC2t04A2\n",
    "Fcix40FKEeiE093Aj3cweMYxNLPgwgQP8Xu3kA5QEw==\n",
    "-----END CERTIFICATE-----\n",
);

#[derive(Debug)]
struct AcceptAnyServerCertificate;

impl ServerCertVerifier for AcceptAnyServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

#[test]
fn nonblocking_tls_reads_drain_buffered_plaintext_before_waiting_for_ciphertext() {
    let certificate = CertificateDer::from_pem_slice(include_bytes!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/test/lisp/net/network-stream-resources/cert.pem"
    )))
    .expect("test certificate");
    let private_key = PrivateKeyDer::from_pem_slice(include_bytes!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/test/lisp/net/network-stream-resources/key.pem"
    )))
    .expect("test private key");
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], private_key)
        .expect("server certificate and key");
    let client_config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCertificate))
        .with_no_client_auth();
    let server_name = ServerName::try_from("localhost").expect("valid server name");
    let mut connection =
        ClientConnection::new(Arc::new(client_config), server_name).expect("TLS client connection");
    let mut server_connection =
        ServerConnection::new(Arc::new(server_config)).expect("TLS server connection");

    for _ in 0..10 {
        let mut client_wire = Vec::new();
        connection
            .write_tls(&mut client_wire)
            .expect("write client handshake records");
        if !client_wire.is_empty() {
            server_connection
                .read_tls(&mut Cursor::new(client_wire))
                .expect("read client handshake records");
            server_connection
                .process_new_packets()
                .expect("process client handshake records");
        }

        let mut server_wire = Vec::new();
        server_connection
            .write_tls(&mut server_wire)
            .expect("write server handshake records");
        if !server_wire.is_empty() {
            connection
                .read_tls(&mut Cursor::new(server_wire))
                .expect("read server handshake records");
            connection
                .process_new_packets()
                .expect("process server handshake records");
        }
        if !connection.is_handshaking() && !server_connection.is_handshaking() {
            break;
        }
    }
    assert!(
        !connection.is_handshaking(),
        "client handshake should finish"
    );
    assert!(
        !server_connection.is_handshaking(),
        "server handshake should finish"
    );
    // Flush post-handshake records (for example TLS 1.3 tickets and the
    // client's Finished) before constructing the application-data scenario.
    for _ in 0..4 {
        let mut client_wire = Vec::new();
        connection
            .write_tls(&mut client_wire)
            .expect("flush client post-handshake records");
        if !client_wire.is_empty() {
            server_connection
                .read_tls(&mut Cursor::new(client_wire))
                .expect("read client post-handshake records");
            server_connection
                .process_new_packets()
                .expect("process client post-handshake records");
        }

        let mut server_wire = Vec::new();
        server_connection
            .write_tls(&mut server_wire)
            .expect("flush server post-handshake records");
        if !server_wire.is_empty() {
            connection
                .read_tls(&mut Cursor::new(server_wire))
                .expect("read server post-handshake records");
            connection
                .process_new_packets()
                .expect("process server post-handshake records");
        }
        if !connection.wants_write() && !server_connection.wants_write() {
            break;
        }
    }

    let expected = vec![b'x'; 8_000];
    server_connection
        .writer()
        .write_all(&expected)
        .expect("buffer TLS test plaintext");
    let mut application_records = Vec::new();
    server_connection
        .write_tls(&mut application_records)
        .expect("write TLS application records");
    let application_records_len = application_records.len() as u64;
    let mut application_records = Cursor::new(application_records);
    let mut plaintext_bytes = 0;
    while application_records.position() < application_records_len {
        connection
            .read_tls(&mut application_records)
            .expect("read TLS application records");
        plaintext_bytes = connection
            .process_new_packets()
            .expect("decrypt TLS application records")
            .plaintext_bytes_to_read();
        if plaintext_bytes == expected.len() {
            break;
        }
    }
    assert_eq!(plaintext_bytes, expected.len());

    // Give the helper a live nonblocking socket with no bytes ready. The only
    // readable bytes are the plaintext already buffered inside rustls.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind empty test socket");
    let mut socket = TcpStream::connect(listener.local_addr().expect("listener address"))
        .expect("connect empty test socket");
    let (_peer, _) = listener.accept().expect("accept empty test socket");
    socket
        .set_nonblocking(true)
        .expect("set TLS test client nonblocking");
    // Make an outstanding TLS write hit WouldBlock. `rustls::Stream::read'
    // flushes such writes before exposing already-decrypted plaintext.
    let filler = vec![0; 65_536];
    loop {
        match socket.write(&filler) {
            Ok(0) => panic!("test socket stopped accepting filler bytes"),
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(err) => panic!("fill test socket send buffer: {err}"),
        }
    }
    connection
        .writer()
        .write_all(b"pending TLS transport write")
        .expect("queue pending TLS write");
    assert!(connection.wants_write(), "TLS write should remain queued");

    let mut actual = Vec::new();
    let mut chunk = vec![0; 65_536];
    while actual.len() < expected.len() {
        let read = read_rustls_process_output(&mut connection, &mut socket, &mut chunk)
            .expect("buffered plaintext must not report WouldBlock");
        assert!(read > 0, "the complete TLS payload should remain readable");
        actual.extend_from_slice(&chunk[..read]);
    }

    assert_eq!(actual.len(), expected.len());
    assert!(actual.iter().all(|byte| *byte == b'x'));
}

#[test]
fn backend_errors_render_boundary_messages() {
    assert_eq!(
        TlsBackendError::InvalidHostname("bad host".to_owned()).to_string(),
        "Invalid hostname for TLS: bad host"
    );
    assert_eq!(
        TlsBackendError::Connect("bad cert".to_owned()).to_string(),
        "TLS handshake failed: bad cert"
    );
    assert_eq!(
        TlsBackendError::TrustFile {
            path: PathBuf::from("roots.pem"),
            reason: "no PEM certificates found".to_owned(),
        }
        .to_string(),
        "Invalid TLS trust file roots.pem: no PEM certificates found"
    );
    assert_eq!(
        TlsBackendError::UnexpectedEof.to_string(),
        "TLS handshake: unexpected EOF"
    );
}

#[test]
fn rustls_backend_advertises_conservative_gnutls_compatibility() {
    assert_eq!(
        gnutls_available_capabilities(),
        &["ciphers", "macs", "digests", "gnutls3", "gnutls"]
    );
}

#[test]
fn gnutls_boot_parameters_parse_x509_hostname() {
    let parameters = parse_gnutls_boot_parameters(
        Value::symbol("gnutls-x509pki"),
        Value::list(vec![
            Value::keyword(":hostname"),
            Value::string("example.org"),
        ]),
    )
    .expect("valid parameters");
    assert_eq!(parameters.credential_type, GnutlsCredentialType::X509Pki);
    assert_eq!(parameters.client.hostname, "example.org");
    assert_eq!(parameters.client.trust_roots, TlsTrustRoots::Default);
}

#[test]
fn gnutls_boot_parameters_preserve_trust_files_as_additional_roots() {
    let parameters = parse_gnutls_boot_parameters(
        Value::symbol("gnutls-x509pki"),
        Value::list(vec![
            Value::keyword(":hostname"),
            Value::string("example.org"),
            Value::keyword(":trustfiles"),
            Value::list(vec![
                Value::string("/first.pem"),
                Value::string("/second.pem"),
            ]),
        ]),
    )
    .expect("valid trust files");

    assert_eq!(
        parameters.client.trust_roots,
        TlsTrustRoots::DefaultPlusFiles(vec![
            PathBuf::from("/first.pem"),
            PathBuf::from("/second.pem"),
        ])
    );
}

#[test]
fn gnutls_boot_parameters_reject_non_string_trust_file_entries() {
    let error = parse_gnutls_boot_parameters(
        Value::symbol("gnutls-x509pki"),
        Value::list(vec![
            Value::keyword(":hostname"),
            Value::string("example.org"),
            Value::keyword(":trustfiles"),
            Value::list(vec![Value::fixnum(1)]),
        ]),
    )
    .unwrap_err();

    match error {
        crate::emacs_core::error::Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            assert_eq!(signal.data, vec![Value::string("Invalid trustfile")]);
        }
        other => panic!("expected error, got {other:?}"),
    }
}

#[test]
fn rustls_root_store_adds_certificates_from_explicit_trust_files() {
    let certificate = PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("test/lisp/net/network-stream-resources/cert.pem");
    let default_roots = rustls_root_store(&TlsTrustRoots::Default).expect("default roots");
    let augmented_roots = rustls_root_store(&TlsTrustRoots::DefaultPlusFiles(vec![certificate]))
        .expect("explicit PEM root");

    assert_eq!(augmented_roots.len(), default_roots.len() + 1);
}

#[test]
fn gnutls_boot_parameters_validate_gnu_argument_shape() {
    let type_error = parse_gnutls_boot_parameters(Value::fixnum(1), Value::NIL).unwrap_err();
    match type_error {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let list_error =
        parse_gnutls_boot_parameters(Value::symbol("gnutls-x509pki"), Value::fixnum(1))
            .unwrap_err();
    match list_error {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let hostname_error =
        parse_gnutls_boot_parameters(Value::symbol("gnutls-x509pki"), Value::NIL).unwrap_err();
    match hostname_error {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "gnutls-boot: invalid :hostname parameter (not a string)"
                )]
            );
        }
        other => panic!("expected error, got {other:?}"),
    }
}

#[test]
fn format_x509_certificate_rejects_invalid_pem() {
    assert!(format_x509_certificate_pem(b"x").is_err());
}

#[test]
fn format_x509_certificate_extracts_parsed_fields() {
    let formatted =
        format_x509_certificate_pem(TEST_CERTIFICATE_PEM.as_bytes()).expect("valid cert");
    assert!(formatted.contains("X.509 Certificate"));
    assert!(formatted.contains("Subject: CN=lists.for-our.info"));
    assert!(formatted.contains("Issuer: C=US, O=Let's Encrypt, CN=Let's Encrypt Authority X3"));
    assert!(formatted.contains("Signature Algorithm: 1.2.840.113549.1.1.11"));
}

#[test]
fn der_certificates_are_formatted_as_pem_blocks() {
    assert_eq!(
        der_certificate_to_pem(&[1, 2, 3]),
        "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n"
    );
}

#[test]
fn certificate_details_value_uses_gnu_peer_status_plist_shape() {
    let details = certificate_details_value_pem(TEST_CERTIFICATE_PEM).expect("valid cert");
    let items = crate::emacs_core::value::list_to_vec(&details).expect("plist");
    assert_eq!(items[0], Value::keyword(":version"));
    assert_eq!(items[1], Value::fixnum(3));
    assert_eq!(items[2], Value::keyword(":serial-number"));
    assert_eq!(items[4], Value::keyword(":issuer"));
    assert_eq!(
        items[5],
        Value::string("C=US, O=Let's Encrypt, CN=Let's Encrypt Authority X3")
    );
    assert_eq!(items[6], Value::keyword(":valid-from"));
    assert_eq!(items[7], Value::string("2019-07-12"));
    assert_eq!(items[8], Value::keyword(":valid-to"));
    assert_eq!(items[9], Value::string("2019-10-10"));
    assert_eq!(items[10], Value::keyword(":subject"));
    assert_eq!(items[11], Value::string("CN=lists.for-our.info"));
    assert!(items.contains(&Value::keyword(":pem")));
}

#[test]
fn gnutls_close_notify_results_use_gnu_error_symbols() {
    assert_eq!(
        gnutls_close_notify_result_value(TlsCloseNotifyResult::Success),
        Value::T
    );
    assert_eq!(
        gnutls_close_notify_result_value(TlsCloseNotifyResult::Again),
        Value::symbol("gnutls-e-again")
    );
    assert_eq!(
        gnutls_close_notify_result_value(TlsCloseNotifyResult::Interrupted),
        Value::symbol("gnutls-e-interrupted")
    );
}

#[test]
fn gnutls_peer_status_plist_matches_gnu_certificate_shape() {
    let status = TlsPeerStatus {
        warnings: vec![":unknown-ca"],
        certificates: vec![
            certificate_details_value_pem(TEST_CERTIFICATE_PEM).expect("valid cert"),
        ],
        key_exchange: Some("ECDHE-RSA".to_owned()),
        protocol: Some("TLS1.3".to_owned()),
        cipher: Some("AES-256-GCM".to_owned()),
        mac: Some("AEAD".to_owned()),
        encrypt_then_mac: Some(false),
    };

    let plist = gnutls_peer_status_to_value(&status);
    let items = crate::emacs_core::value::list_to_vec(&plist).expect("plist");
    assert_eq!(items[0], Value::keyword(":warnings"));
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&items[1]).expect("warnings"),
        vec![Value::keyword(":unknown-ca")]
    );
    assert_eq!(items[2], Value::keyword(":certificates"));
    assert_eq!(items[4], Value::keyword(":certificate"));
    assert_eq!(items[6], Value::keyword(":key-exchange"));
    assert_eq!(items[7], Value::string("ECDHE-RSA"));
    assert_eq!(items[8], Value::keyword(":protocol"));
    assert_eq!(items[9], Value::string("TLS1.3"));
    assert_eq!(items[10], Value::keyword(":cipher"));
    assert_eq!(items[11], Value::string("AES-256-GCM"));
    assert_eq!(items[12], Value::keyword(":mac"));
    assert_eq!(items[13], Value::string("AEAD"));
    assert_eq!(items[14], Value::keyword(":encrypt-then-mac"));
    assert_eq!(items[15], Value::NIL);
    let certificate = crate::emacs_core::value::list_to_vec(&items[5]).expect("certificate plist");
    assert!(certificate.contains(&Value::keyword(":subject")));
    assert!(certificate.contains(&Value::string("CN=lists.for-our.info")));
}

#[test]
fn gnutls_peer_status_warning_descriptions_match_gnu() {
    assert_eq!(
        builtin_gnutls_peer_status_warning_describe(vec![Value::keyword(":unknown-ca")])
            .expect("description"),
        Value::string("the certificate was signed by an unknown and therefore untrusted authority")
    );
    assert_eq!(
        builtin_gnutls_peer_status_warning_describe(vec![Value::keyword(":expired")])
            .expect("description"),
        Value::string("certificate has expired")
    );
    assert_eq!(
        builtin_gnutls_peer_status_warning_describe(vec![Value::keyword(":not-a-warning")])
            .expect("unknown warning"),
        Value::NIL
    );
}

#[test]
fn gnutls_error_helpers_match_gnu_type_and_known_code_rules() {
    assert_eq!(
        builtin_gnutls_error_string(vec![Value::string("x")]).expect("string"),
        Value::string("Not an error symbol or code")
    );
    assert_eq!(
        builtin_gnutls_error_string(vec![Value::symbol("no-such")]).expect("string"),
        Value::string("Symbol has no numeric gnutls-code property")
    );
    assert_eq!(
        builtin_gnutls_error_string(vec![Value::symbol("gnutls-e-invalid-session")])
            .expect("string"),
        Value::string("The specified session has been invalidated for some reason.")
    );

    assert_eq!(
        builtin_gnutls_error_fatalp(vec![Value::fixnum(1)]).expect("fatalp"),
        Value::NIL
    );
    assert_eq!(
        builtin_gnutls_error_fatalp(vec![Value::fixnum(-1)]).expect("fatalp"),
        Value::T
    );
    assert_eq!(
        builtin_gnutls_error_fatalp(vec![Value::symbol("gnutls-e-again")]).expect("fatalp"),
        Value::NIL
    );
    assert_eq!(
        builtin_gnutls_error_fatalp(vec![Value::symbol("gnutls-e-invalid-session")])
            .expect("fatalp"),
        Value::T
    );

    let invalid_object = builtin_gnutls_error_fatalp(vec![Value::string("x")]).unwrap_err();
    match invalid_object {
        crate::emacs_core::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Not an error symbol or code")]);
        }
        other => panic!("expected error, got {other:?}"),
    }
}
