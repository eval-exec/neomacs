//! Test-only authority fence for core-owned tests of the moved system tests.

/// FENCE (N2 TLS reset): zero the per-thread new-child-command call counter.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_new_child_command_calls_for_test() {
    crate::emacs_core::callproc::reset_new_child_command_calls_for_test();
}

/// Constructor fence for `WaitTimeoutChoice` (peer #816 write arm): every
/// field explicit, no hidden defaults. The wait tests' struct literals switch
/// to it; the type stays crate-private with its test-support widening.
pub fn wait_timeout_choice_for_test(
    duration: std::time::Duration,
    finite_deadline_timeout: bool,
    shortened_by_timer: bool,
) -> crate::emacs_core::wait::WaitTimeoutChoice {
    crate::emacs_core::wait::WaitTimeoutChoice {
        duration,
        finite_deadline_timeout,
        shortened_by_timer,
    }
}

/// Constructor fence for `TlsClientParameters` (peer #816 write arm): every
/// field explicit. The TLS runtime test's literal switches to it.
pub fn tls_client_parameters_for_test(
    hostname: String,
    trust_roots: crate::emacs_core::tls::TlsTrustRoots,
) -> crate::emacs_core::tls::TlsClientParameters {
    crate::emacs_core::tls::TlsClientParameters {
        hostname,
        trust_roots,
    }
}

/// Constructor fence for `TlsPeerStatus` (peer #1005 field remediation):
/// every field explicit; the moved TLS oracle's struct literal switches to
/// it and the seven fields stay private in every arm.
pub fn tls_peer_status_for_test(
    warnings: Vec<&'static str>,
    certificates: Vec<neovm_value::value::Value>,
    key_exchange: Option<String>,
    protocol: Option<String>,
    cipher: Option<String>,
    mac: Option<String>,
    encrypt_then_mac: Option<bool>,
) -> crate::emacs_core::tls::TlsPeerStatus {
    crate::emacs_core::tls::TlsPeerStatus {
        warnings,
        certificates,
        key_exchange,
        protocol,
        cipher,
        mac,
        encrypt_then_mac,
    }
}
