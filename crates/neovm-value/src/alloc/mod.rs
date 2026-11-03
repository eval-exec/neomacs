//! Bootstrap-facing subset of GNU Emacs's `alloc.c`.
//!
//! GNU exposes several GC / memory-management variables from C before Lisp
//! startup runs.  Keep those defaults here so Lisp like `jit-lock.el` can rely
//! on the same low-level variables during runtime and bootstrap.

use crate::symbol::Obarray;
use crate::value::Value;

use crate::error::{Flow, LispCondition, signal, signal_with_data};

fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
    value.as_fixnum().filter(|n| *n >= 0).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *value],
        )
    })
}

mod sealed {
    pub trait Sealed {}
}

static_assertions::assert_impl_all!(HashTableHint: Send, Sync);

/// One byte of room for the NUL every owned Lisp string payload appends, so
/// turning the reserved text into a string never reallocates it.
fn lisp_text_payload_capacity(len: usize) -> usize {
    len + 1
}

fn reserved_lisp_text(len: usize) -> Result<String, Flow> {
    let mut text = String::new();
    text.try_reserve_exact(lisp_text_payload_capacity(len))
        .map_err(|_| memory_exhausted())?;
    Ok(text)
}

static_assertions::assert_impl_all!(RecordLen: Send, Sync);
static_assertions::assert_impl_all!(ObarrayBits: Send, Sync);
static_assertions::assert_impl_all!(HashTableSize: Send, Sync);
static_assertions::assert_impl_all!(CharTableExtras: Send, Sync);
static_assertions::assert_impl_all!(BufferByteLen: Send, Sync);
static_assertions::assert_impl_all!(RepeatCount: Send, Sync);
static_assertions::assert_impl_all!(StringByteLen: Send, Sync);

/// Register bootstrap variables owned by the allocation / GC subsystem.
pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.define_int_variable("gc-cons-threshold", 800_000);
    obarray.seed_value("gc-cons-percentage", Value::make_float(0.1));
    obarray.make_special("gc-cons-percentage");
    obarray.seed_value("post-gc-hook", Value::NIL);
    obarray.make_special("post-gc-hook");
    obarray.seed_value(
        "memory-signal-data",
        Value::list(vec![
            Value::symbol("error"),
            Value::string(
                "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
            ),
        ]),
    );
    obarray.make_special("memory-signal-data");
    obarray.seed_value("memory-full", Value::NIL);
    obarray.make_special("memory-full");
    obarray.seed_value("gc-elapsed", Value::make_float(0.0));
    obarray.make_special("gc-elapsed");
    obarray.define_int_variable("gcs-done", 0);
    obarray.define_int_variable("pure-bytes-used", 0);
    // `src/alloc.c:7448' DEFVAR_INT, no initializer -- the C global starts at 0
    // and `allocate_string' counts up from there.  Neomacs does not track it
    // yet, like its five siblings in `eval.rs' (`cons-cells-consed' and
    // friends), so it reads 0 where GNU reads whatever it has allocated.
    obarray.define_int_variable("strings-consed", 0);
}

// Definitions are private here; the old core paths retain their visibility.
mod data;
#[doc(hidden)]
pub use data::AllocLen;
#[doc(hidden)]
pub use data::AllocationFailure;
#[doc(hidden)]
pub use data::BufferByteLen;
#[doc(hidden)]
pub use data::CharTableExtras;
#[doc(hidden)]
pub use data::HashTableHint;
#[doc(hidden)]
pub use data::HashTableSize;
#[doc(hidden)]
pub use data::ObarrayBits;
#[doc(hidden)]
pub use data::RecordLen;
#[doc(hidden)]
pub use data::RepeatCount;
#[doc(hidden)]
pub use data::RepeatCountError;
#[doc(hidden)]
pub use data::StringByteLen;
#[doc(hidden)]
pub use data::buffer_overflow;
#[doc(hidden)]
pub use data::memory_exhausted;
#[doc(hidden)]
pub use data::reserved_values;

#[cfg(test)]
#[path = "tests/validated_lengths.rs"]
mod validated_lengths;
