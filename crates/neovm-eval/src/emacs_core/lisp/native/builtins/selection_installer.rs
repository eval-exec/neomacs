//! Hot clipboard installer. A small `Copy` selector; the owned arms
//! themselves are not `Copy`. The selector is derived from the owner's
//! installed [`SelectionPhase`](crate::emacs_core::eval::activation::SelectionPhase),
//! so the phase and the lowering cannot disagree.

/// Which lowering the five clipboard/PRIMARY stubs use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionInstaller {
    /// Unhosted or default-mode owner: the serial direct call.
    InlineSerialMain,
    /// Hosted main owner: the local owner turn calls the direct body.
    OwnedLocalMain,
    /// Worker boundary (C4b): the caller keeps arity, conversion and the Lisp
    /// result, and only a typed selection request crosses to the host owner.
    OwnedRoutedWorker,
}
