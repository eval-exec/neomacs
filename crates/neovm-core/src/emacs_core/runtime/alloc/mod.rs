//! Bootstrap-facing subset of GNU Emacs's `alloc.c`.
//!
//! GNU exposes several GC / memory-management variables from C before Lisp
//! startup runs.  Keep those defaults here so Lisp like `jit-lock.el` can rely
//! on the same low-level variables during runtime and bootstrap.

use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// Register bootstrap variables owned by the allocation / GC subsystem.
pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.define_int_variable("gc-cons-threshold", 800_000);
    obarray.set_symbol_value("gc-cons-percentage", Value::make_float(0.1));
    obarray.make_special("gc-cons-percentage");
    obarray.set_symbol_value("post-gc-hook", Value::NIL);
    obarray.make_special("post-gc-hook");
    obarray.set_symbol_value(
        "memory-signal-data",
        Value::list(vec![
            Value::symbol("error"),
            Value::string(
                "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
            ),
        ]),
    );
    obarray.make_special("memory-signal-data");
    obarray.set_symbol_value("memory-full", Value::NIL);
    obarray.make_special("memory-full");
    obarray.set_symbol_value("gc-elapsed", Value::make_float(0.0));
    obarray.make_special("gc-elapsed");
    obarray.define_int_variable("gcs-done", 0);
    obarray.define_int_variable("pure-bytes-used", 0);
    // `src/alloc.c:7448' DEFVAR_INT, no initializer -- the C global starts at 0
    // and `allocate_string' counts up from there.  Neomacs does not track it
    // yet, like its five siblings in `eval.rs' (`cons-cells-consed' and
    // friends), so it reads 0 where GNU reads whatever it has allocated.
    obarray.define_int_variable("strings-consed", 0);
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
