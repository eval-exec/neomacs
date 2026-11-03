//! Static Subr entry synchronization lock; no Lisp state or new TLS.
//!
//! Threading: the upper registration owner decides whether a writer takes
//! this lock and passes that decision down; this module owns only the lock.
//! The RwLock protects the scalar replacement against readers in other
//! mutators; no Lisp call or GC occurs while held. Readers try the read side
//! and skip selected proof reads while a writer holds it. Non-JIT builds
//! compile the original writer body without this module.

static STATIC_SUBR_ENTRY_SYNC: std::sync::RwLock<()> = std::sync::RwLock::new(());

#[inline]
#[doc(hidden)]
pub fn with_static_subr_entry_read<R>(read: impl FnOnce() -> R) -> Option<R> {
    let _guard = STATIC_SUBR_ENTRY_SYNC.try_read().ok()?;
    Some(read())
}

/// Rare registration replacement. Poison conservatively disables selected
/// proof readers while writers still complete metadata replacement.
#[cold]
#[inline(never)]
#[doc(hidden)]
pub fn with_static_subr_entry_write<R>(write: impl FnOnce() -> R) -> R {
    let _guard = STATIC_SUBR_ENTRY_SYNC
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    write()
}
