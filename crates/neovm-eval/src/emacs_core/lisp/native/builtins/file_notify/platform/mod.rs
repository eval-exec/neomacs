//! Compile-time-selected native file-notification adapters.
//!
//! GNU Emacs builds exactly one local file-notification implementation into an
//! executable.  Keeping the same property here makes a request for one OS
//! impossible to pass to another OS adapter.

#[cfg(target_os = "linux")]
pub(super) mod linux;

// The pure snapshot-diff model is also compiled by Linux unit tests.  The
// native kqueue implementation inside the module remains macOS-only.
#[cfg(any(target_os = "macos", all(test, target_os = "linux")))]
pub(super) mod macos;

#[cfg(any(target_os = "windows", all(test, target_os = "linux")))]
pub(super) mod windows;

// Keep the common evaluator lifecycle available on targets with no native
// file-notification subrs. Linux tests compile this adapter as an architecture
// check without weakening the compile-time desktop backend selection.
#[cfg(any(
    not(any(target_os = "linux", target_os = "macos", target_os = "windows")),
    all(test, target_os = "linux")
))]
mod unsupported;

std::cfg_select! {
    target_os = "linux" => {
        pub(super) type Backend = linux::InotifyBackend;
    }
    target_os = "macos" => {
        pub(super) type Backend = macos::KqueueBackend;
    }
    target_os = "windows" => {
        pub(super) type Backend = windows::W32NotifyBackend;
    }
    _ => {
        pub(super) type Backend = unsupported::UnsupportedBackend;
    }
}

/// The three primitives this build's backend registers, in GNU's DEFUN order,
/// or `None` where GNU has no local file-notification backend.
///
/// GNU compiles exactly one backend file, and that file's `DEFUN`s and its
/// `Fprovide` share one `#ifdef`: src/inotify.c:370,490,511 with :589,
/// src/kqueue.c:338,475,505 with :545, and src/w32notify.c:515,640,681 with
/// :739.  A build therefore cannot answer another platform's watch
/// primitive, and tests ask this table instead of naming one OS's set.
#[cfg_attr(not(test), allow(dead_code))] // test-facing inventory today
pub(crate) const BACKEND_SUBRS: Option<[&'static str; 3]> = std::cfg_select! {
    target_os = "linux" => Some([
        "inotify-add-watch",
        "inotify-rm-watch",
        "inotify-valid-p",
    ]),
    target_os = "macos" => Some([
        "kqueue-add-watch",
        "kqueue-rm-watch",
        "kqueue-valid-p",
    ]),
    target_os = "windows" => Some([
        "w32notify-add-watch",
        "w32notify-rm-watch",
        "w32notify-valid-p",
    ]),
    _ => None,
};

/// How the compiled-in backend answers `rm-watch`, from GNU's implementation
/// of each one.
///
/// GNU's three backends disagree on purpose: inotify checks only the
/// descriptor's shape and returns t whether or not the watch was still in its
/// list (src/inotify.c:490-506); kqueue looks the descriptor up and signals
/// `file-notify-error` "Not a watch descriptor" when it is gone
/// (src/kqueue.c:475-490); w32notify returns nil on success and signals
/// "Invalid watch descriptor" when the watch is absent
/// (src/w32notify.c:640-668).  A test of the watch lifecycle must ask which
/// backend it is running, not assume inotify's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoveWatchSemantics {
    /// inotify: rm-watch returns t for any well-shaped descriptor.
    Inotify,
    /// kqueue: a removed descriptor makes the next rm-watch signal.
    Kqueue,
    /// w32notify: success is nil; an absent watch signals.
    W32Notify,
}

/// The compiled backend's `rm-watch` semantics, or `None` with no backend.
#[cfg_attr(not(test), allow(dead_code))] // test-facing inventory today
pub(crate) const REMOVE_WATCH_SEMANTICS: Option<RemoveWatchSemantics> = std::cfg_select! {
    target_os = "linux" => Some(RemoveWatchSemantics::Inotify),
    target_os = "macos" => Some(RemoveWatchSemantics::Kqueue),
    target_os = "windows" => Some(RemoveWatchSemantics::W32Notify),
    _ => None,
};
