use super::*;

/// The native-arithmetic intrinsics the JIT bakes (`arith_op_inlines`) are
/// exactly the `CannotReenter` class, and none of them is missing from it.
#[test]
fn inlined_intrinsics_cannot_reenter() {
    for name in ["logand", "logior", "lognot", "logxor", "mod"] {
        assert_eq!(fast_path_reentry(name), FastPathReentry::CannotReenter);
    }
}

/// Everything that can run Lisp or wait stays `MayReenter`: the catalog's
/// thread_select family, the thread primitives (contended `mutex-lock`
/// included — GNU `lisp_mutex_lock` releases on contention), the read
/// family, and ordinary subrs. `ash` stays on the self-arming shim but is
/// still an original call boundary, so it must not be classified
/// call-free either.
#[test]
fn wait_and_lisp_families_may_reenter() {
    for name in [
        "accept-process-output",
        "cond-var-wait",
        "mutex-lock",
        "process-send-string",
        "read-char",
        "read-event",
        "read-key-sequence",
        "sit-for",
        "sleep-for",
        "thread-join",
        "thread-yield",
        "ash",
        "car",
        "gethash",
    ] {
        assert_eq!(
            fast_path_reentry(name),
            FastPathReentry::MayReenter,
            "{name}"
        );
    }
}

/// Unknown names never manufacture a call-free classification.
#[test]
fn unknown_names_default_may_reenter() {
    assert_eq!(
        fast_path_reentry("some-user-function"),
        FastPathReentry::MayReenter
    );
    assert_eq!(fast_path_reentry(""), FastPathReentry::MayReenter);
}

/// The table stays searchable: ascending order is a compile-time fact of the
/// array, so pin it against a future edit breaking the binary search.
#[test]
fn intrinsic_table_stays_sorted() {
    let mut sorted = INLINED_NATIVE_OPS;
    sorted.sort_unstable();
    assert_eq!(sorted, INLINED_NATIVE_OPS);
}
