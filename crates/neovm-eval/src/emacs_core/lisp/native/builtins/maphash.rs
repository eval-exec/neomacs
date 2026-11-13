//! `maphash` — the evaluator-facing hash-table walker.
//!
//! The hash-table core (`runtime/hashtab`, G0) owns the table representation
//! and the slot-iteration primitives; this upper module owns the callback
//! selection (generic call vs the JIT bytecode entry) and the evaluator
//! dispatch. It lives here so the G0 hash-table module keeps no upward
//! reference to `eval`.

use super::*;
use crate::emacs_core::eval::{CheckedNativeCallback, Context, native_callback_cache_enabled};
use neovm_value::hashtab::{maphash_entry_at_slot, maphash_slot_len};

/// A callback selected for one synchronous maphash activation. GNU resolves
/// symbol callbacks anew for every slot; only a direct bytecode identity can
/// use the specialized entry. This value and its roots belong to one mutator,
/// and no Lisp state is cached across activations or shared with other threads.
pub(crate) enum MaphashCallee {
    Generic(Value),
    #[cfg(feature = "jit")]
    ByteCode(Value),
    #[cfg(feature = "jit")]
    UnobservedByteCode(Value),
}

impl MaphashCallee {
    #[doc(hidden)]
    pub fn resolve(function: Value) -> Self {
        #[cfg(feature = "jit")]
        if *MAPHASH_BYTECODE && function.is_bytecode() {
            return if neovm_value::tagged::collection_reads::reads_need_observation() {
                Self::ByteCode(function)
            } else {
                Self::UnobservedByteCode(function)
            };
        }
        Self::Generic(function)
    }

    #[inline]
    #[doc(hidden)]
    pub fn call(&self, eval: &mut Context, key: Value, value: Value) -> EvalResult {
        match *self {
            Self::Generic(function) => eval.apply2(function, key, value),
            #[cfg(feature = "jit")]
            Self::ByteCode(function) => eval.apply2_bytecode(function, key, value),
            #[cfg(feature = "jit")]
            Self::UnobservedByteCode(function) => {
                eval.apply2_bytecode_unobserved(function, key, value)
            }
        }
    }
}

/// Immutable process configuration holds no Lisp state. Concurrent mutators
/// may read it; each maphash activation owns its callback and roots.
#[cfg(feature = "jit")]
static MAPHASH_BYTECODE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    !std::env::var("NEOVM_MAPHASH_BYTECODE").is_ok_and(|value| value == "off")
});

/// (maphash FUNCTION TABLE) — call FUNCTION with each (KEY VALUE) pair.
#[doc(hidden)]
pub fn builtin_maphash(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("maphash", &args, 2)?;
    if !args[1].is_hash_table() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[1]],
        ));
    }

    let roots = eval.save_specpdl_roots();
    for arg in &args {
        eval.push_specpdl_root(*arg);
    }
    let func = args[0];
    let table = args[1];
    let callee = MaphashCallee::resolve(func);
    // Native proof stays within this mutator's activation and owns no roots.
    let native = if native_callback_cache_enabled() {
        let epoch = eval.obarray().function_epoch();
        eval.resolve_mapped_subr_callee(func).and_then(|(subr, _)| {
            CheckedNativeCallback::resolve(subr, 2).map(|proof| (subr, epoch, proof))
        })
    } else {
        None
    };
    let result = (|| -> EvalResult {
        let mut slot = 0_usize;
        loop {
            let Some((key, val)) = maphash_entry_at_slot(table, slot) else {
                if slot >= maphash_slot_len(table) {
                    break;
                }
                slot += 1;
                continue;
            };
            eval.push_specpdl_root(key);
            eval.push_specpdl_root(val);
            match native {
                Some((subr, epoch, proof)) => {
                    eval.apply2_checked_subr(func, subr, epoch, proof, key, val)?;
                }
                None => {
                    callee.call(eval, key, val)?;
                }
            }
            slot += 1;
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}
