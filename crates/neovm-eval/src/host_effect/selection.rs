//! Typed native payloads for the five selection primitives.
//!
//! A selection call is evaluated by its calling Lisp thread: arity,
//! argument conversion and the Lisp result are decided there, exactly as GNU
//! runs the call on the thread that made it. Only these owned values cross to
//! the owner that holds the display host; no `Value`, root or evaluator
//! borrow crosses. The reply is a value or a host error; a transport failure
//! is not represented here.

/// One selection host request, already converted from Lisp on the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRequest {
    /// `neomacs-clipboard-set`; `None` clears the clipboard.
    ClipboardSet(Option<String>),
    /// `neomacs-clipboard-get`.
    ClipboardGet,
    /// `neomacs-primary-selection-set`; `None` clears the selection.
    PrimarySet(Option<String>),
    /// `neomacs-primary-selection-get`.
    PrimaryGet,
    /// `neomacs-primary-selection-owner`.
    PrimaryOwner,
}

/// The host's answer, converted back to Lisp by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionReply {
    /// Both set primitives return nil.
    Set,
    /// `neomacs-clipboard-get`: nil when the host has no text.
    Clipboard(Option<String>),
    /// `neomacs-primary-selection-get`: nil when the host has no text.
    Primary(Option<String>),
    /// `neomacs-primary-selection-owner`: one of four symbols.
    Owner(SelectionOwnerReply),
    /// The host refused; the caller signals `(error MESSAGE)` exactly as the
    /// direct body does.
    HostError(String),
}

/// The four owner symbols `neomacs-primary-selection-owner` can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionOwnerReply {
    ThisProcess,
    OtherProcess,
    None,
    Unknown,
}
