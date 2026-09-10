//! Native-stack growth policy for recursive evaluator boundaries.

// Unoptimized dispatch frames are substantially larger than release frames
// (the Android ARM64 bytecode loop alone reserves about 160 KiB). A release
// probe interval can exhaust an entire segment before the next debug probe.
std::cfg_select! {
    debug_assertions => {
        pub(crate) const RED_ZONE: usize = 1024 * 1024;
        pub(crate) const SEGMENT: usize = 4 * 1024 * 1024;
        pub(crate) const PROBE_INTERVAL: usize = 1;
    }
    _ => {
        pub(crate) const RED_ZONE: usize = 128 * 1024;
        pub(crate) const SEGMENT: usize = 2 * 1024 * 1024;
        pub(crate) const PROBE_INTERVAL: usize = 16;
    }
}

#[inline]
pub(crate) fn should_probe(depth: usize) -> bool {
    depth >= PROBE_INTERVAL && depth.is_multiple_of(PROBE_INTERVAL)
}

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
