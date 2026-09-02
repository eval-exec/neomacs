//! Native-stack growth policy for recursive evaluator boundaries.

std::cfg_select! {
    target_family = "wasm" => {
        /// Browser WebAssembly uses its engine-managed stack, so there is no
        /// native segmented-stack facility to invoke.
        #[inline]
        pub(crate) fn maybe_grow<R>(
            red_zone: usize,
            stack_size: usize,
            callback: impl FnOnce() -> R,
        ) -> R {
            let _ = (red_zone, stack_size);
            callback()
        }
    }
    _ => {
        /// Run `callback`, growing the native stack around recursive evaluator
        /// boundaries when the remaining stack enters the red zone.
        #[inline]
        pub(crate) fn maybe_grow<R>(
            red_zone: usize,
            stack_size: usize,
            callback: impl FnOnce() -> R,
        ) -> R {
            stacker::maybe_grow(red_zone, stack_size, callback)
        }
    }
}

/// The remaining native stack, when the host exposes segmented-stack bounds.
#[inline]
pub(crate) fn remaining_stack() -> Option<usize> {
    #[cfg(target_family = "wasm")]
    { None }
    #[cfg(not(target_family = "wasm"))]
    { stacker::remaining_stack() }
}

/// Run a callback on a fresh native segment where the host supports it.
#[inline]
pub(crate) fn grow<R>(stack_size: usize, callback: impl FnOnce() -> R) -> R {
    #[cfg(target_family = "wasm")]
    { let _ = stack_size; callback() }
    #[cfg(not(target_family = "wasm"))]
    { stacker::grow(stack_size, callback) }
}
