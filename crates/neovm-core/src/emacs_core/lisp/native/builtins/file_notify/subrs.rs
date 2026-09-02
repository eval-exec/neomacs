//! Native Lisp declarations owned by the file-notification subsystem.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    SubrSpec::new(
        "inotify-add-watch",
        NativeFn::ContextVec(inotify_add_watch),
        SubrArity::new(3, Some(3)),
    ),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    SubrSpec::new(
        "inotify-rm-watch",
        NativeFn::ContextVec(|_ctx, args| inotify_rm_watch(args)),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    SubrSpec::new(
        "inotify-valid-p",
        NativeFn::ContextVec(|_ctx, args| inotify_valid_p(args)),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(target_os = "macos")]
    SubrSpec::new(
        "kqueue-add-watch",
        NativeFn::ContextVec(kqueue_add_watch),
        SubrArity::new(3, Some(3)),
    ),
    #[cfg(target_os = "macos")]
    SubrSpec::new(
        "kqueue-rm-watch",
        NativeFn::ContextVec(|_ctx, args| kqueue_rm_watch(args)),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(target_os = "macos")]
    SubrSpec::new(
        "kqueue-valid-p",
        NativeFn::ContextVec(|_ctx, args| kqueue_valid_p(args)),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(target_os = "windows")]
    SubrSpec::new(
        "w32notify-add-watch",
        NativeFn::ContextVec(w32notify_add_watch),
        SubrArity::new(3, Some(3)),
    ),
    #[cfg(target_os = "windows")]
    SubrSpec::new(
        "w32notify-rm-watch",
        NativeFn::ContextVec(|_ctx, args| w32notify_rm_watch(args)),
        SubrArity::new(1, Some(1)),
    ),
    #[cfg(target_os = "windows")]
    SubrSpec::new(
        "w32notify-valid-p",
        NativeFn::ContextVec(|_ctx, args| w32notify_valid_p(args)),
        SubrArity::new(1, Some(1)),
    ),
}
