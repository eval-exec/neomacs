use super::{
    EvalResult, Value, ValueKind, expect_args, expect_args_range, expect_lisp_string, signal,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::tls::format_x509_certificate_pem;
use crate::emacs_core::value::{VecLikeType, list_to_vec};
use aes::Aes256;
use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::NoPadding};
use hmac::{KeyInit, Mac, SimpleHmac};
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

pub(crate) fn builtin_gnutls_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-available-p", &args, 0)?;
    Ok(Value::list(
        crate::emacs_core::tls::gnutls_available_capabilities()
            .iter()
            .map(|capability| Value::symbol(*capability))
            .collect(),
    ))
}

pub(crate) fn builtin_gnutls_ciphers(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-ciphers", &args, 0)?;
    Ok(Value::list(
        CIPHER_ALGORITHMS
            .iter()
            .map(|algorithm| algorithm.cipher_plist())
            .collect(),
    ))
}

pub(crate) fn builtin_gnutls_digests(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-digests", &args, 0)?;
    Ok(Value::list(
        DIGEST_ALGORITHMS
            .iter()
            .map(|algorithm| algorithm.digest_plist())
            .collect(),
    ))
}

pub(crate) fn builtin_gnutls_macs(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-macs", &args, 0)?;
    Ok(Value::list(
        MAC_ALGORITHMS
            .iter()
            .map(|algorithm| algorithm.mac_plist())
            .collect(),
    ))
}

pub(crate) fn builtin_gnutls_errorp(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-errorp", &args, 1)?;
    if args[0] == Value::T || args[0].is_symbol_named("gnutls-e-again") {
        Ok(Value::NIL)
    } else {
        Ok(Value::T)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gnutls_error_string(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-error-string", &args, 1)?;
    gnutls_error_string_impl(None, args[0])
}

pub(crate) fn builtin_gnutls_error_string_with_ctx(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-error-string", &args, 1)?;
    gnutls_error_string_impl(Some(eval), args[0])
}

fn gnutls_error_string_impl(eval: Option<&Context>, error: Value) -> EvalResult {
    let message = match gnutls_error_code(eval, error) {
        GnutlsErrorCode::SuccessSentinel => "Not an error",
        GnutlsErrorCode::Code(code) => gnutls_error_code_string(code),
        GnutlsErrorCode::SymbolWithoutCode => "Symbol has no numeric gnutls-code property",
        GnutlsErrorCode::InvalidObject => "Not an error symbol or code",
    };
    Ok(Value::string(message))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gnutls_error_fatalp(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-error-fatalp", &args, 1)?;
    gnutls_error_fatalp_impl(None, args[0])
}

pub(crate) fn builtin_gnutls_error_fatalp_with_ctx(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-error-fatalp", &args, 1)?;
    gnutls_error_fatalp_impl(Some(eval), args[0])
}

fn gnutls_error_fatalp_impl(eval: Option<&Context>, error: Value) -> EvalResult {
    match gnutls_error_code(eval, error) {
        GnutlsErrorCode::SuccessSentinel | GnutlsErrorCode::Code(0) => Ok(Value::NIL),
        GnutlsErrorCode::Code(-28 | -52) => Ok(Value::NIL),
        GnutlsErrorCode::Code(code) if code < 0 => Ok(Value::T),
        GnutlsErrorCode::Code(_) => Ok(Value::NIL),
        GnutlsErrorCode::SymbolWithoutCode => Err(signal(
            "error",
            vec![Value::string("Symbol has no numeric gnutls-code property")],
        )),
        GnutlsErrorCode::InvalidObject => Err(signal(
            "error",
            vec![Value::string("Not an error symbol or code")],
        )),
    }
}

enum GnutlsErrorCode {
    SuccessSentinel,
    Code(i64),
    SymbolWithoutCode,
    InvalidObject,
}

fn gnutls_error_code(eval: Option<&Context>, value: Value) -> GnutlsErrorCode {
    if value == Value::T {
        return GnutlsErrorCode::SuccessSentinel;
    }
    match value.kind() {
        ValueKind::Fixnum(code) => GnutlsErrorCode::Code(code),
        ValueKind::Nil => GnutlsErrorCode::SymbolWithoutCode,
        ValueKind::Symbol(_) => gnutls_symbol_error_code(eval, value),
        _ => GnutlsErrorCode::InvalidObject,
    }
}

fn gnutls_symbol_error_code(eval: Option<&Context>, value: Value) -> GnutlsErrorCode {
    if let Some(eval) = eval
        && let Some(symbol) = value.as_symbol_id()
        && let Some(code) = eval
            .obarray()
            .get_property_id(symbol, intern("gnutls-code"))
    {
        return match code.kind() {
            ValueKind::Fixnum(code) => GnutlsErrorCode::Code(code),
            ValueKind::Float | ValueKind::Veclike(VecLikeType::Bignum) => {
                GnutlsErrorCode::InvalidObject
            }
            _ => GnutlsErrorCode::SymbolWithoutCode,
        };
    }

    match value.as_symbol_name() {
        Some("gnutls-e-again") => GnutlsErrorCode::Code(-28),
        Some("gnutls-e-interrupted") => GnutlsErrorCode::Code(-52),
        Some("gnutls-e-invalid-session") => GnutlsErrorCode::Code(-10),
        Some("gnutls-e-not-ready-for-handshake") => GnutlsErrorCode::Code(-65500),
        _ => GnutlsErrorCode::SymbolWithoutCode,
    }
}

fn gnutls_error_code_string(code: i64) -> &'static str {
    match code {
        0 => "Success.",
        -28 => "Resource temporarily unavailable, try again.",
        -52 => "Function was interrupted.",
        -10 => "The specified session has been invalidated for some reason.",
        _ => "(unknown error code)",
    }
}

pub(crate) fn builtin_gnutls_peer_status_warning_describe(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-peer-status-warning-describe", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let Some(symbol) = args[0].as_symbol_name() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    let Some(description) = gnutls_peer_status_warning_description(symbol) else {
        return Ok(Value::NIL);
    };
    Ok(Value::string(description))
}

fn gnutls_peer_status_warning_description(symbol: &str) -> Option<&'static str> {
    match symbol {
        ":invalid" => Some("certificate could not be verified"),
        ":revoked" => Some("certificate was revoked (CRL)"),
        ":self-signed" => Some("certificate signer was not found (self-signed)"),
        ":unknown-ca" => {
            Some("the certificate was signed by an unknown and therefore untrusted authority")
        }
        ":not-ca" => Some("certificate signer is not a CA"),
        ":insecure" => Some("certificate was signed with an insecure algorithm"),
        ":not-activated" => Some("certificate is not yet activated"),
        ":expired" => Some("certificate has expired"),
        ":no-host-match" => Some("certificate host does not match hostname"),
        ":signature-failure" => Some("certificate signature could not be verified"),
        ":revocation-data-superseded" => {
            Some("certificate revocation data are old and have been superseded")
        }
        ":revocation-data-issued-in-future" => {
            Some("certificate revocation data have a future issue date")
        }
        ":signer-constraints-failure" => Some("certificate signer constraints were violated"),
        ":purpose-mismatch" => Some("certificate does not match the intended purpose"),
        ":missing-ocsp-status" => Some(
            "certificate requires the server to send a OCSP certificate status, but no status was received",
        ),
        ":invalid-ocsp-status" => Some("the received OCSP certificate status is invalid"),
        _ => None,
    }
}

pub(crate) fn builtin_gnutls_format_certificate(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-format-certificate", &args, 1)?;
    let cert = expect_lisp_string(&args[0])?;
    let formatted = format_x509_certificate_pem(cert.as_bytes()).map_err(|err| {
        signal(
            "error",
            vec![Value::string(format!(
                "gnutls-format-certificate error: {err}"
            ))],
        )
    })?;
    Ok(Value::string(formatted))
}

pub(crate) fn builtin_gnutls_hash_digest(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-hash-digest", &args, 2)?;
    let algorithm = gnutls_digest_algorithm(&args[0])?;
    let input = gnutls_crypto_input_bytes(&args[1], "digest input")?;
    Ok(unibyte_value(&algorithm.digest(&input)))
}

pub(crate) fn builtin_gnutls_hash_mac(args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-hash-mac", &args, 3)?;
    let algorithm = gnutls_mac_algorithm(&args[0])?;
    let key = gnutls_crypto_input_bytes(&args[1], "MAC key")?;
    let input = gnutls_crypto_input_bytes(&args[2], "MAC input")?;
    Ok(unibyte_value(&algorithm.hmac(&key, &input)?))
}

pub(crate) fn builtin_gnutls_symmetric_decrypt(args: Vec<Value>) -> EvalResult {
    expect_args_range("gnutls-symmetric-decrypt", &args, 4, 5)?;
    gnutls_symmetric_cipher(args, GnutlsCipherOperation::Decrypt)
}

pub(crate) fn builtin_gnutls_symmetric_encrypt(args: Vec<Value>) -> EvalResult {
    expect_args_range("gnutls-symmetric-encrypt", &args, 4, 5)?;
    gnutls_symmetric_cipher(args, GnutlsCipherOperation::Encrypt)
}

#[derive(Clone, Copy)]
struct GnutlsDigestAlgorithm {
    name: &'static str,
    id: i64,
    length: usize,
}

const DIGEST_ALGORITHMS: &[GnutlsDigestAlgorithm] = &[
    GnutlsDigestAlgorithm {
        name: "SHA1",
        id: 1,
        length: 20,
    },
    GnutlsDigestAlgorithm {
        name: "SHA224",
        id: 2,
        length: 28,
    },
    GnutlsDigestAlgorithm {
        name: "SHA256",
        id: 3,
        length: 32,
    },
    GnutlsDigestAlgorithm {
        name: "SHA384",
        id: 4,
        length: 48,
    },
    GnutlsDigestAlgorithm {
        name: "SHA512",
        id: 5,
        length: 64,
    },
];

const MAC_ALGORITHMS: &[GnutlsDigestAlgorithm] = DIGEST_ALGORITHMS;

#[derive(Clone, Copy)]
struct GnutlsCipherAlgorithm {
    name: &'static str,
    id: i64,
    block_size: usize,
    key_size: usize,
    iv_size: usize,
}

const CIPHER_ALGORITHMS: &[GnutlsCipherAlgorithm] = &[GnutlsCipherAlgorithm {
    name: "AES-256-CBC",
    id: 5,
    block_size: 16,
    key_size: 32,
    iv_size: 16,
}];

impl GnutlsDigestAlgorithm {
    fn digest_plist(self) -> Value {
        Value::list(vec![
            Value::symbol(self.name),
            Value::keyword(":digest-algorithm-id"),
            Value::fixnum(self.id),
            Value::keyword(":type"),
            Value::symbol("gnutls-digest-algorithm"),
            Value::keyword(":digest-algorithm-length"),
            Value::fixnum(self.length as i64),
        ])
    }

    fn mac_plist(self) -> Value {
        Value::list(vec![
            Value::symbol(self.name),
            Value::keyword(":mac-algorithm-id"),
            Value::fixnum(self.id),
            Value::keyword(":type"),
            Value::symbol("gnutls-mac-algorithm"),
            Value::keyword(":mac-algorithm-length"),
            Value::fixnum(self.length as i64),
            Value::keyword(":mac-algorithm-keysize"),
            Value::fixnum(self.length as i64),
            Value::keyword(":mac-algorithm-noncesize"),
            Value::fixnum(0),
        ])
    }

    fn digest(self, input: &[u8]) -> Vec<u8> {
        match self.name {
            "SHA1" => digest_with::<Sha1>(input),
            "SHA224" => digest_with::<Sha224>(input),
            "SHA256" => digest_with::<Sha256>(input),
            "SHA384" => digest_with::<Sha384>(input),
            "SHA512" => digest_with::<Sha512>(input),
            _ => unreachable!("unknown GnuTLS digest algorithm"),
        }
    }

    fn hmac(self, key: &[u8], input: &[u8]) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
        match self.name {
            "SHA1" => hmac_with::<Sha1>(key, input),
            "SHA224" => hmac_with::<Sha224>(key, input),
            "SHA256" => hmac_with::<Sha256>(key, input),
            "SHA384" => hmac_with::<Sha384>(key, input),
            "SHA512" => hmac_with::<Sha512>(key, input),
            _ => unreachable!("unknown GnuTLS MAC algorithm"),
        }
    }
}

impl GnutlsCipherAlgorithm {
    fn cipher_plist(self) -> Value {
        Value::list(vec![
            Value::symbol(self.name),
            Value::keyword(":cipher-id"),
            Value::fixnum(self.id),
            Value::keyword(":type"),
            Value::symbol("gnutls-symmetric-cipher"),
            Value::keyword(":cipher-aead-capable"),
            Value::NIL,
            Value::keyword(":cipher-tagsize"),
            Value::fixnum(0),
            Value::keyword(":cipher-blocksize"),
            Value::fixnum(self.block_size as i64),
            Value::keyword(":cipher-keysize"),
            Value::fixnum(self.key_size as i64),
            Value::keyword(":cipher-ivsize"),
            Value::fixnum(self.iv_size as i64),
        ])
    }
}

enum GnutlsCipherOperation {
    Encrypt,
    Decrypt,
}

fn gnutls_symmetric_cipher(
    args: Vec<Value>,
    operation: GnutlsCipherOperation,
) -> Result<Value, crate::emacs_core::error::Flow> {
    let algorithm = gnutls_cipher_algorithm(&args[0])?;
    if args.len() == 5 && !args[4].is_nil() {
        return Err(signal(
            "error",
            vec![Value::string("GnuTLS cipher does not support AEAD data")],
        ));
    }

    let key = gnutls_crypto_input_bytes(&args[1], "cipher key")?;
    if key.len() != algorithm.key_size {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "GnuTLS cipher key length must be {} bytes",
                algorithm.key_size
            ))],
        ));
    }

    let iv = gnutls_cipher_iv_bytes(&args[2], algorithm.iv_size)?;
    let input = gnutls_crypto_input_bytes(&args[3], "cipher input")?;
    if input.len() % algorithm.block_size != 0 {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "GnuTLS cipher input length must be a multiple of {} bytes",
                algorithm.block_size
            ))],
        ));
    }

    let output = match operation {
        GnutlsCipherOperation::Encrypt => encrypt_aes_256_cbc(&key, &iv, &input)?,
        GnutlsCipherOperation::Decrypt => decrypt_aes_256_cbc(&key, &iv, &input)?,
    };
    Ok(Value::list(vec![
        unibyte_value(&output),
        unibyte_value(&iv),
    ]))
}

fn encrypt_aes_256_cbc(
    key: &[u8],
    iv: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    Ok(cbc::Encryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| gnutls_cipher_extraction_error("cipher parameters"))?
        .encrypt_padded_vec::<NoPadding>(input))
}

fn decrypt_aes_256_cbc(
    key: &[u8],
    iv: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    cbc::Decryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| gnutls_cipher_extraction_error("cipher parameters"))?
        .decrypt_padded_vec::<NoPadding>(input)
        .map_err(|_| {
            signal(
                "error",
                vec![Value::string("GnuTLS cipher decryption failed")],
            )
        })
}

fn digest_with<D>(input: &[u8]) -> Vec<u8>
where
    D: Digest + Default,
{
    let mut digest = D::new();
    digest.update(input);
    digest.finalize().to_vec()
}

fn hmac_with<D>(key: &[u8], input: &[u8]) -> Result<Vec<u8>, crate::emacs_core::error::Flow>
where
    D: Digest + Default + Clone + hmac::digest::block_api::BlockSizeUser,
{
    let mut mac = SimpleHmac::<D>::new_from_slice(key).map_err(|_| {
        signal(
            "error",
            vec![Value::string("GnuTLS MAC key extraction failed")],
        )
    })?;
    Mac::update(&mut mac, input);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn gnutls_digest_algorithm(
    value: &Value,
) -> Result<GnutlsDigestAlgorithm, crate::emacs_core::error::Flow> {
    gnutls_algorithm(
        value,
        DIGEST_ALGORITHMS,
        ":digest-algorithm-id",
        "digest-method",
    )
}

fn gnutls_mac_algorithm(
    value: &Value,
) -> Result<GnutlsDigestAlgorithm, crate::emacs_core::error::Flow> {
    gnutls_algorithm(value, MAC_ALGORITHMS, ":mac-algorithm-id", "MAC-method")
}

fn gnutls_cipher_algorithm(
    value: &Value,
) -> Result<GnutlsCipherAlgorithm, crate::emacs_core::error::Flow> {
    if let Some(name) = value.as_symbol_name().or_else(|| value.as_utf8_str()) {
        if let Some(algorithm) = CIPHER_ALGORITHMS
            .iter()
            .find(|algorithm| algorithm.name == name)
        {
            return Ok(*algorithm);
        }
        return Err(invalid_gnutls_algorithm("cipher", *value));
    }

    if let ValueKind::Fixnum(id) = value.kind() {
        if let Some(algorithm) = CIPHER_ALGORITHMS
            .iter()
            .find(|algorithm| algorithm.id == id)
        {
            return Ok(*algorithm);
        }
        return Err(invalid_gnutls_algorithm("cipher", *value));
    }

    if let Some(items) = list_to_vec(value)
        && let Some(id) = plist_fixnum(&items, ":cipher-id")
        && let Some(algorithm) = CIPHER_ALGORITHMS
            .iter()
            .find(|algorithm| algorithm.id == id)
    {
        return Ok(*algorithm);
    }

    Err(invalid_gnutls_algorithm("cipher", *value))
}

fn gnutls_algorithm(
    value: &Value,
    algorithms: &[GnutlsDigestAlgorithm],
    id_key: &str,
    description: &str,
) -> Result<GnutlsDigestAlgorithm, crate::emacs_core::error::Flow> {
    if let Some(name) = value.as_symbol_name().or_else(|| value.as_utf8_str()) {
        if let Some(algorithm) = algorithms.iter().find(|algorithm| algorithm.name == name) {
            return Ok(*algorithm);
        }
        return Err(invalid_gnutls_algorithm(description, *value));
    }

    if let ValueKind::Fixnum(id) = value.kind() {
        if let Some(algorithm) = algorithms.iter().find(|algorithm| algorithm.id == id) {
            return Ok(*algorithm);
        }
        return Err(invalid_gnutls_algorithm(description, *value));
    }

    if let Some(items) = list_to_vec(value)
        && let Some(id) = plist_fixnum(&items, id_key)
        && let Some(algorithm) = algorithms.iter().find(|algorithm| algorithm.id == id)
    {
        return Ok(*algorithm);
    }

    Err(invalid_gnutls_algorithm(description, *value))
}

fn plist_fixnum(items: &[Value], key: &str) -> Option<i64> {
    items.windows(2).find_map(|pair| {
        if pair[0].is_symbol_named(key)
            && let ValueKind::Fixnum(id) = pair[1].kind()
        {
            return Some(id);
        }
        None
    })
}

fn invalid_gnutls_algorithm(description: &str, value: Value) -> crate::emacs_core::error::Flow {
    signal(
        "error",
        vec![
            Value::string(format!("GnuTLS {description} is invalid or not found")),
            value,
        ],
    )
}

fn gnutls_crypto_input_bytes(
    value: &Value,
    description: &str,
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    if let Some(string) = value.as_lisp_string() {
        return Ok(string.as_bytes().to_vec());
    }

    let Some(items) = list_to_vec(value) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), *value],
        ));
    };

    let mut bytes = Vec::new();
    for item in items {
        let string = expect_lisp_string(&item).map_err(|_| {
            signal(
                "error",
                vec![Value::string(format!(
                    "GnuTLS {description} extraction failed"
                ))],
            )
        })?;
        bytes.extend_from_slice(string.as_bytes());
    }
    Ok(bytes)
}

fn gnutls_cipher_iv_bytes(
    value: &Value,
    expected_size: usize,
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    if let Some(items) = list_to_vec(value)
        && items.len() == 2
        && items[0].as_symbol_name() == Some("iv-auto")
    {
        let ValueKind::Fixnum(size) = items[1].kind() else {
            return Err(gnutls_cipher_extraction_error("cipher IV"));
        };
        if size < 0 || size as usize != expected_size {
            return Err(gnutls_cipher_extraction_error("cipher IV"));
        }
        let mut iv = vec![0; expected_size];
        getrandom::fill(&mut iv).map_err(|err| {
            signal(
                "error",
                vec![Value::string(format!(
                    "GnuTLS cipher IV generation failed: {err}"
                ))],
            )
        })?;
        return Ok(iv);
    }

    let iv = gnutls_crypto_input_bytes(value, "cipher IV")?;
    if iv.len() != expected_size {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "GnuTLS cipher IV length must be {expected_size} bytes"
            ))],
        ));
    }
    Ok(iv)
}

fn gnutls_cipher_extraction_error(description: &str) -> crate::emacs_core::error::Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "GnuTLS {description} extraction failed"
        ))],
    )
}

fn unibyte_value(bytes: &[u8]) -> Value {
    Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes.to_vec()))
}
