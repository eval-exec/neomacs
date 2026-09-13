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

/// What this host does when Lisp recursion outruns the stack.
///
/// GNU lets Lisp raise `max-lisp-eval-depth` without an upper bound, and it
/// can afford to: overrunning the C stack there is an OS-reported fault, and
/// this port additionally grows the stack through [`maybe_grow`] above.
/// Neither holds in a browser Worker. The shadow stack is a fixed window of
/// linear memory (`-z stack-size`, placed first so it grows down toward
/// address 0), `maybe_grow` is a no-op there by construction, and the overrun
/// is a wasm **trap** -- which is not a Rust panic: it bypasses the panic
/// hook, kills the instance, and takes every unsaved buffer with it.
///
/// So on wasm one `(setq max-lisp-eval-depth 100000)` is the difference
/// between a signalled Lisp error and an unrecoverable one, and the host has
/// to keep a ceiling that Lisp cannot lift. Everywhere else Lisp keeps GNU's
/// rule unchanged.
// One variant is unreachable per target by construction -- that is the point
// of `CURRENT` below -- so the unconstructed one is expected, not dead.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LispDepthLimit {
    /// An overrun is reported and the stack grows; Lisp owns the limit.
    HostRecovers,
    /// An overrun traps unrecoverably; Lisp's limit is capped at `ceiling`.
    HostTraps { ceiling: usize },
}

impl LispDepthLimit {
    /// GNU raises a requested limit below this before it signals, so a handler
    /// has room to run (`src/eval.c:2587-2588`). Applies on every host.
    pub(crate) const GNU_FLOOR: i64 = 100;

    /// Policy compiled into the current editor executable.
    ///
    /// The browser ceiling is MEASURED, not chosen. Against the 4 MiB worker
    /// shadow stack, driven in Chrome 149 with `browser_stack_smoke.py`, the
    /// deepest `max-lisp-eval-depth` that still signals
    /// `excessive-lisp-nesting` rather than killing the Worker is:
    ///
    /// | call style  | signals cleanly at |
    /// |-------------|--------------------|
    /// | `direct`    | >= 12800           |
    /// | `protected` | >= 12800           |
    /// | `macro`     | 1600 (dies at 3200)|
    ///
    /// The worst style sets the ceiling, so 1600 it is -- which is also
    /// exactly GNU's default, so no configuration that works in stock GNU
    /// loses anything here; only *raising* the limit is refused.
    ///
    /// `macro` is the outlier because each level re-enters macro expansion,
    /// which the continuation driver does not own and which therefore still
    /// recurses on the machine stack. Note what kills it: a matching probe of
    /// nested source forms died at the same depth with `RangeError: Maximum
    /// call stack size exceeded` -- V8's **engine** stack (Blink pins a
    /// Worker isolate to `kWorkerMaxStackSize = 500 * 1024`), not the shadow
    /// stack. So raising `-z stack-size` past 4 MiB would not move this
    /// number: nothing a linker flag controls is the binding constraint any
    /// more. Migrating macro expansion onto the driver would.
    pub(crate) const CURRENT: Self = std::cfg_select! {
        target_family = "wasm" => { Self::HostTraps { ceiling: 1600 } }
        _ => { Self::HostRecovers }
    };

    /// Clamp one Lisp-supplied `max-lisp-eval-depth` to what the host survives.
    ///
    /// This is deliberately silent. It runs on the `setq` write path and on
    /// every depth-limit read, so it cannot signal or message without turning
    /// a variable assignment into a control-flow event GNU does not have.
    #[inline]
    pub(crate) const fn clamp(self, requested: i64) -> usize {
        let requested = if requested < Self::GNU_FLOOR {
            Self::GNU_FLOOR
        } else {
            requested
        };
        match self {
            Self::HostRecovers => requested as usize,
            Self::HostTraps { ceiling } => {
                if requested as usize > ceiling {
                    ceiling
                } else {
                    requested as usize
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
