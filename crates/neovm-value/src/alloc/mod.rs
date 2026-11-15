//! Bootstrap-facing subset of GNU Emacs's `alloc.c`.
//!
//! GNU exposes several GC / memory-management variables from C before Lisp
//! startup runs.  Keep those defaults here so Lisp like `jit-lock.el` can rely
//! on the same low-level variables during runtime and bootstrap.

use crate::symbol::Obarray;
use crate::tagged::gc::MemoryUseCountSlot;
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
    // `src/alloc.c:7429-7449' DEFVAR_INTs, no initializers -- each C global
    // starts at 0 and the allocator counts up from there.
    for (name, counter) in ALLOCATION_COUNTER_VARIABLES {
        obarray.define_counter_variable(name, counter);
    }
}

/// GNU's allocation-counter variables (`src/alloc.c:7429-7449`), each with
/// the `memory-use-counts` field that reports the same C global
/// (`Fmemory_use_counts`, `src/alloc.c:7190-7196`).
///
/// One table for the declaration above and for the portable-dump loader,
/// which rebuilds these descriptors by name: an image cannot carry which
/// counter a descriptor reads.
#[doc(hidden)]
pub const ALLOCATION_COUNTER_VARIABLES: [(&str, MemoryUseCountSlot); 7] = [
    ("cons-cells-consed", MemoryUseCountSlot::ConsCells),
    ("floats-consed", MemoryUseCountSlot::Floats),
    ("vector-cells-consed", MemoryUseCountSlot::VectorCells),
    ("symbols-consed", MemoryUseCountSlot::Symbols),
    ("string-chars-consed", MemoryUseCountSlot::StringChars),
    ("intervals-consed", MemoryUseCountSlot::Intervals),
    ("strings-consed", MemoryUseCountSlot::Strings),
];

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
