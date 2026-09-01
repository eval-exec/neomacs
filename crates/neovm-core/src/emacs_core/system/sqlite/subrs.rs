//! Native Lisp declarations owned by GNU `src/sqlite.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

// Keep declarations in GNU `syms_of_sqlite` order.  Item-level feature gates
// leave only GNU's two capability probes when the backend is omitted.
crate::emacs_core::subr::define_subrs! {
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-open",
        NativeFn::ContextVec(open),
        SubrArity::new(0, Some(3)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-close",
        NativeFn::NoContextVec(close),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-execute",
        NativeFn::ContextVec(execute),
        SubrArity::new(2, Some(3)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-select",
        NativeFn::ContextVec(select),
        SubrArity::new(2, Some(4)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-execute-batch",
        NativeFn::ContextVec(execute_batch),
        SubrArity::new(2, Some(2)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-transaction",
        NativeFn::NoContextVec(transaction),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-commit",
        NativeFn::NoContextVec(commit),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-rollback",
        NativeFn::NoContextVec(rollback),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-pragma",
        NativeFn::NoContextVec(pragma),
        SubrArity::new(2, Some(2)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-load-extension",
        NativeFn::ContextVec(load_extension),
        SubrArity::new(2, Some(2)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-next",
        NativeFn::NoContextVec(next),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-columns",
        NativeFn::NoContextVec(columns),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-more-p",
        NativeFn::NoContextVec(more_p),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-finalize",
        NativeFn::NoContextVec(finalize),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(feature = "sqlite")]
    SubrSpec::new(
        "sqlite-version",
        NativeFn::NoContextVec(version),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "sqlitep",
        NativeFn::NoContextVec(is_sqlite_object),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "sqlite-available-p",
        NativeFn::NoContextVec(available_p),
        SubrArity::new(0, Some(0)),
    ),
}
