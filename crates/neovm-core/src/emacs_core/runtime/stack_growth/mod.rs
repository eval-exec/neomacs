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
