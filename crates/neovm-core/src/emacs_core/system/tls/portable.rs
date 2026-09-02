//! TLS boundary for hosts without native stream sockets.

use crate::emacs_core::error::EvalResult;
use crate::emacs_core::value::Value;

pub(crate) fn gnutls_available_capabilities() -> &'static [&'static str] {
    &[]
}

pub(crate) fn builtin_neomacs_tls_available_p(_args: Vec<Value>) -> EvalResult {
    Ok(Value::NIL)
}

#[derive(Debug)]
pub(crate) struct CertificateFormatError {
    message: &'static str,
}

impl std::fmt::Display for CertificateFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

pub(crate) fn format_x509_certificate_pem(
    _pem_bytes: &[u8],
) -> Result<String, CertificateFormatError> {
    Err(CertificateFormatError {
        message: "X.509 formatting requires the native TLS capability",
    })
}
