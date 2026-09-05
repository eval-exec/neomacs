//! Process-related bootstrap variables (GNU `process.c` / `gnutls.c` DEFVARs).
//!
//! One table is shared by both process backends. GNU only DEFVARs most of
//! these inside `#ifdef subprocesses`, but Lisp that runs on every host
//! (`comint`, `tramp-sh`, `nsm`, `gnutls.el`) reads them unconditionally.
//! Portable builds therefore keep them bound with GNU's default values; the
//! process primitives themselves are what signal capability absence.

use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.set_symbol_value("process-connection-type", Value::T);
    obarray.make_special("process-connection-type");
    // GNU `process.c` `syms_of_process` DEFVAR_LISPs
    // `process-adaptive-read-buffering` (default nil); it controls the
    // short-read delay heuristic in `read_process_output` and is set per
    // process at `start-process'/`make-process' time
    // (`p->adaptive_read_buffering`).  It must be *bound* (to nil) so that
    // `(boundp 'process-adaptive-read-buffering)` is t and reading the
    // variable does not signal `void-variable`; e.g. `tramp-sh.el` binds it
    // with `(let ((process-adaptive-read-buffering nil)) ...)`.  Without this
    // DEFVAR, code that reads the variable before calling a (non-existent)
    // helper sees `void-variable` instead of reaching the real error.
    obarray.set_symbol_value("process-adaptive-read-buffering", Value::NIL);
    obarray.make_special("process-adaptive-read-buffering");
    obarray.set_symbol_value(
        "interrupt-process-functions",
        Value::list(vec![Value::symbol("internal-default-interrupt-process")]),
    );
    obarray.make_special("interrupt-process-functions");
    obarray.set_symbol_value(
        "signal-process-functions",
        Value::list(vec![Value::symbol("internal-default-signal-process")]),
    );
    obarray.make_special("signal-process-functions");
    obarray.set_symbol_value("internal--daemon-sockname", Value::NIL);
    obarray.make_special("internal--daemon-sockname");
    obarray.define_int_variable("read-process-output-max", 65536);
    obarray.define_int_variable("process-error-pause-time", 1);
    // GNU `gnutls.c` provides this via `DEFVAR_INT ("gnutls-log-level",
    // global_gnutls_log_level)` (default 0).  `gnutls.el` only forward-declares
    // it (`(defvar gnutls-log-level)  ; gnutls.c`), so without the C-side
    // definition it is void and `gnutls-negotiate` errors on
    // `:loglevel ,gnutls-log-level` before it ever reaches the (working,
    // TLS-capable) `gnutls-boot` -- breaking every package download and
    // thus `use-package`.  See https://github.com/eval-exec/neomacs/issues/121.
    obarray.define_int_variable("gnutls-log-level", 0);
    // GNU `gnutls.c` always DEFVAR_LISPs `libgnutls-version`; when Emacs is
    // built without libgnutls, the documented value is -1.  Neomacs exposes a
    // `gnutls-boot` compatibility API over Rust TLS rather than linking
    // libgnutls, so keep the variable bound without pretending to have a
    // libgnutls version.  `nsm.el` reads this during HTTPS package refresh.
    obarray.set_symbol_value("libgnutls-version", Value::fixnum(-1));
    obarray.make_special("libgnutls-version");
    for (symbol, code) in [
        ("gnutls-e-interrupted", -52),
        ("gnutls-e-again", -28),
        ("gnutls-e-invalid-session", -10),
        ("gnutls-e-not-ready-for-handshake", -65500),
    ] {
        obarray
            .put_property(symbol, "gnutls-code", Value::fixnum(code))
            .expect("bootstrap gnutls-code plist should be well formed");
    }
}
