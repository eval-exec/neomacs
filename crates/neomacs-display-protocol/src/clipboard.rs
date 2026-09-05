//! Clipboard and selection ownership vocabulary shared across runtime layers.

/// What the native selection backend knows about the current owner.
///
/// GNU Emacs keeps ownership distinct from selection contents: X asks the
/// display server, NS compares pasteboard change counts, and w32 owns its
/// process-local PRIMARY while a value is present.  Keeping every state
/// explicit prevents an unobservable native owner from being mistaken for
/// this process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SelectionOwner {
    /// The current Neomacs process owns the selection.
    ThisProcess,
    /// A different client owns the selection.
    OtherProcess,
    /// The selection currently has no owner.
    None,
    /// The backend cannot determine ownership.
    Unknown,
}
