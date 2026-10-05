use super::*;

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
    pub(super) fn enter() -> Self {
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
        Self {
            _thread: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "jit")]
impl Drop for RawCallTestPolicy {
    fn drop(&mut self) {
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        crate::emacs_core::jit::force_profit_defer_for_test(None);
    }
}

#[cfg(feature = "jit")]
pub(super) fn assert_native_frames_warmed(ctx: &Context, names: &[&str]) {
    if !crate::emacs_core::jit::jit_runtime_enabled() {
        return;
    }
    for name in names {
        let function = ctx
            .obarray
            .symbol_function_id(crate::emacs_core::intern::intern(name))
            .expect("defined test function");
        let bytecode = function.get_bytecode_data().expect("byte-compiled");
        let id = bytecode.jit_runtime().compiled_id_or_assign();
        assert!(
            crate::emacs_core::jit::cache::is_compiled_for_test(id),
            "{name} must have a native leaf before signalling"
        );
    }
}
