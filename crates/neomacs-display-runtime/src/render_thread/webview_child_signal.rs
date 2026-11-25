//! The embedder's SIGCHLD re-claim for the WebView stack.

/// The embedder's re-claim of SIGCHLD, which the render thread runs right
/// after it creates the WebView system.
///
/// GNU calls `catch_child_signal` again after WebKit/GLib touch the
/// disposition (src/xwidget.c:382, :3128, :4281, :4303); the WPE stack is
/// Neo's corresponding claim point. The handler and its latches belong to the
/// evaluator's process layer, so the runtime never installs a SIGCHLD handler
/// of its own: the embedder passes its re-claim into the render loop
/// (`neomacs` passes `neovm_core::emacs_core::reclaim_child_signal`).
#[derive(Clone, Copy)]
pub struct WebViewChildSignal(fn() -> bool);

impl WebViewChildSignal {
    /// `reclaim` re-installs the embedder's SIGCHLD handler, chaining whatever
    /// handler the WebView stack installed; it reports whether it did.
    pub const fn new(reclaim: fn() -> bool) -> Self {
        Self(reclaim)
    }

    pub(super) fn reclaim(self) -> bool {
        (self.0)()
    }
}
