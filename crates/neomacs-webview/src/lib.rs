//! Cross-platform embedded web content for Neomacs.

// Without the optional feature, this crate intentionally retains the complete
// public protocol while selecting an unsupported backend. Native adapter-only
// helpers are therefore unreachable in that configuration.
#![cfg_attr(not(feature = "webview"), allow(dead_code))]

mod backend;
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
#[cfg_attr(target_os = "windows", allow(dead_code))]
mod load_state;
mod model;
mod platform;
mod presentation;
mod system;

#[cfg(test)]
#[path = "tests/backend_test.rs"]
mod backend_test;

#[cfg(test)]
#[path = "tests/system_test.rs"]
mod system_test;

pub use model::{
    BrowsingRelationship, ButtonState, DeveloperToolsPolicy, DmaBufFrame, DmaBufPlane,
    DmaBufReadiness, FocusIntent, HistoryAction, HostWindowId, JavaScriptPolicy, LoadPhase,
    NavigationId, NavigationTarget, PixelFrame, PointerButton, PolicyDecisionId, ScriptError,
    ScriptRequest, ScriptRequestId, ScriptWorld, StoragePartition, WebContentPoint, WebContentSize,
    WebContentSizeError, WebProcessFailure, WebProfileId, WebValue, WebViewCommand,
    WebViewCommandError, WebViewCreate, WebViewEvent, WebViewFrame, WebViewFrameTransport,
    WebViewGeneration, WebViewHost, WebViewInitError, WebViewInput, WebViewInputTarget,
    WebViewModifiers, WebViewOccurrenceId, WebViewPolicy, WebViewSceneRevision, WebViewScrollDelta,
    WebViewState, WebViewSystemConfig, WebViewWake,
};
pub use neomacs_display_protocol::WebViewId;
pub use presentation::{
    ResolvedWebViewPlacement, ResolvedWebViewScene, WebContentOffset, WebViewPlacementError,
    WebViewPresentationEffects, WebViewPresentationError, WebViewSceneError,
};
pub use system::WebViewSystem;

/// Configure platform WebView environment variables before any threads exist.
///
/// On Linux with the `webview` feature enabled, resolves the WPE DRM render
/// node and exports `WPE_DRM_DEVICE` / `WPE_DRM_RENDER_NODE` so WebKit child
/// processes inherit the chosen node without mutating the environment after
/// worker threads start. On other platforms or without `webview`, this is a
/// no-op.
///
/// # Safety
/// The caller must invoke this before any other thread exists in the process
/// (`std::env::set_var` precondition).
pub unsafe fn configure_before_threads() {
    // SAFETY: caller guarantees this process is single-threaded.
    unsafe { platform::configure_before_threads() };
}
