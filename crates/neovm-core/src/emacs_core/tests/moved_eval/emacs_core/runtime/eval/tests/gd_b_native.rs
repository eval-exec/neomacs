use super::*;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
/// Owns JIT test overrides on the test thread that installed them.
/// The thread-local extent ends on this same thread when the guard drops.
#[cfg(feature = "jit")]
#[must_use = "the test policy lasts until its guard drops"]
#[derive(Debug)]
pub(super) struct RawCallTestPolicy {
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(RawCallTestPolicy: Send, Sync);

#[cfg(feature = "jit")]
impl RawCallTestPolicy {
    #[doc(hidden)]
    pub fn enter() -> Self {
        neovm_eval::test_support::authority::force_inline_for_test(Some(false));
        neovm_eval::test_support::authority::force_profit_defer_for_test(Some(1));
        Self {
            _thread: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "jit")]
impl Drop for RawCallTestPolicy {
    fn drop(&mut self) {
        neovm_eval::test_support::authority::force_inline_for_test(None);
        neovm_eval::test_support::authority::force_profit_defer_for_test(None);
    }
}

#[cfg(feature = "jit")]
pub(super) fn assert_native_frames_warmed(ctx: &Context, names: &[&str]) {
    if !crate::emacs_core::jit::jit_runtime_enabled() {
        return;
    }
    for name in names {
        let function = ctx
            .obarray()
            .symbol_function_id(neovm_value::intern::intern(name))
            .expect("defined test function");
        let bytecode = function.get_bytecode_data().expect("byte-compiled");
        let id = jit_compiled_id_or_assign_for_test(&bytecode);
        assert!(
            crate::emacs_core::jit::cache::is_compiled_for_test(id),
            "{name} must have a native leaf before signalling"
        );
    }
}
