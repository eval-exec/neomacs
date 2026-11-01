//! Preserve the manager's native scalar loads and its reserved Context slot.
//!
//! The legacy shape is never instantiated. It describes the previous Rust
//! layout without retaining obsolete runtime maps. The new manager uses these
//! constants to reserve its cold storage around the one real identity issuer.

use super::Value;
use crate::heap_types::threading::{CondVarId, MutexId, ThreadId, ThreadIds};
use std::collections::HashMap;
use std::mem::{MaybeUninit, align_of, offset_of, size_of};

/// The previous declaration order; deliberately retains Rust representation.
/// Map values affect neither the map's storage size nor its alignment. Their
/// old owned state types are unnecessary for this uninstantiated layout input.
#[allow(dead_code)]
struct LegacyShape {
    threads: HashMap<u64, Value>,
    next_id: u64,
    current_thread: u64,
    thread_handles: HashMap<u64, Value>,
    mutexes: HashMap<u64, Value>,
    next_mutex_id: u64,
    mutex_handles: HashMap<u64, Value>,
    condition_vars: HashMap<u64, Value>,
    next_cv_id: u64,
    condition_var_handles: HashMap<u64, Value>,
    last_error: Option<Value>,
}

pub(super) const OLD_SIZE: usize = size_of::<LegacyShape>();
pub(super) const OLD_ALIGN: usize = align_of::<LegacyShape>();
pub(super) const OLD_NEXT: usize = offset_of!(LegacyShape, next_id);
pub(super) const OLD_CURRENT: usize = offset_of!(LegacyShape, current_thread);
pub(super) const OLD_LAST_ERROR: usize = offset_of!(LegacyShape, last_error);

/// The new manager's meaningful prefix: live registrations, the native
/// transport mailbox, and the global last error. No obsolete map is retained.
pub(super) const PREFIX_SIZE: usize =
    2 * size_of::<HashMap<u64, Value>>() + size_of::<Option<Value>>();

/// Keep the issuer's sole next-thread word at the old direct-load address.
pub(super) const ISSUER_OFFSET: usize = OLD_NEXT - ThreadIds::NEXT_THREAD_OFFSET;
pub(super) const BEFORE_ISSUER_BYTES: usize = ISSUER_OFFSET - PREFIX_SIZE;
pub(super) const TAIL_BYTES: usize = OLD_SIZE - OLD_CURRENT - size_of::<u64>();

const BEFORE_ISSUER_WORDS: usize = BEFORE_ISSUER_BYTES / size_of::<usize>();
const TAIL_WORDS: usize = TAIL_BYTES / size_of::<usize>();

/// Uninitialized native reservation; contains no Lisp words or owned maps.
#[repr(C)]
pub(super) struct BeforeIssuerReservation {
    _words: [MaybeUninit<usize>; BEFORE_ISSUER_WORDS],
}

impl BeforeIssuerReservation {
    pub(super) const fn new() -> Self {
        Self {
            _words: [MaybeUninit::uninit(); BEFORE_ISSUER_WORDS],
        }
    }
}

/// The remainder of the original slot after the native current-thread word.
#[repr(C)]
pub(super) struct TailReservation {
    _words: [MaybeUninit<usize>; TAIL_WORDS],
}

impl TailReservation {
    pub(super) const fn new() -> Self {
        Self {
            _words: [MaybeUninit::uninit(); TAIL_WORDS],
        }
    }
}

const _: () = {
    assert!(size_of::<HashMap<ThreadId, Value>>() == size_of::<HashMap<u64, Value>>());
    assert!(align_of::<HashMap<ThreadId, Value>>() == align_of::<HashMap<u64, Value>>());
    assert!(size_of::<HashMap<MutexId, Value>>() == size_of::<HashMap<u64, Value>>());
    assert!(align_of::<HashMap<MutexId, Value>>() == align_of::<HashMap<u64, Value>>());
    assert!(size_of::<HashMap<CondVarId, Value>>() == size_of::<HashMap<u64, Value>>());
    assert!(align_of::<HashMap<CondVarId, Value>>() == align_of::<HashMap<u64, Value>>());
    assert!(size_of::<HashMap<u64, Vec<Value>>>() == size_of::<HashMap<u64, Value>>());
    assert!(align_of::<HashMap<u64, Vec<Value>>>() == align_of::<HashMap<u64, Value>>());
    assert!(BEFORE_ISSUER_BYTES % size_of::<usize>() == 0);
    assert!(TAIL_BYTES % size_of::<usize>() == 0);
    assert!(size_of::<BeforeIssuerReservation>() == BEFORE_ISSUER_BYTES);
    assert!(size_of::<TailReservation>() == TAIL_BYTES);
    assert!(ISSUER_OFFSET % align_of::<ThreadIds>() == 0);
    assert!(ISSUER_OFFSET + size_of::<ThreadIds>() == OLD_CURRENT);
    assert!(OLD_ALIGN == align_of::<usize>());
};

// The existing Context ABI test fixes threads at 144 and command_loop at 480.
// Hot scalar offsets are computed from LegacyShape, not guessed numerically.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const _: () = {
    assert!(OLD_SIZE == 336);
    assert!(OLD_ALIGN == 8);
};
