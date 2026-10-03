//! Selected static Subr entry synchronization; no Lisp state or new TLS.
//!
//! Threading: configuration is process immutable. Every production writer and
//! selected reader therefore agrees before metadata can be inspected. The
//! RwLock protects the scalar replacement against readers in other mutators;
//! no Lisp call or GC occurs while held. Test builds synchronize every writer
//! because per-thread compiler overrides may select Sink on another thread.
//! Non-JIT builds compile the original writer body without this module.

static STATIC_SUBR_ENTRY_SYNC: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Immutable process selection, read only on the rare registration writer.
/// The compiler's once-per-process selectors also govern every emitted reader;
/// this condition is decided before the first write during Context setup.
#[cfg(not(test))]
#[inline]
pub(super) fn writer_sync_selected() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        use crate::emacs_core::jit::compile::{OptMode, jit_opt_mode, jit_opt_passes};
        jit_opt_mode() == OptMode::Opt && jit_opt_passes().sink
    })
}

#[inline]
pub(crate) fn with_static_subr_entry_read<R>(read: impl FnOnce() -> R) -> Option<R> {
    let _guard = STATIC_SUBR_ENTRY_SYNC.try_read().ok()?;
    Some(read())
}

/// Rare registration replacement. Poison conservatively disables selected
/// proof readers while writers still complete metadata replacement.
#[cold]
#[inline(never)]
pub(crate) fn with_static_subr_entry_write<R>(write: impl FnOnce() -> R) -> R {
    let _guard = STATIC_SUBR_ENTRY_SYNC
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    write()
}
