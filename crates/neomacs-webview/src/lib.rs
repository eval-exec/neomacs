//! Cross-platform embedded web content for Neomacs.

// Without the optional feature, this crate intentionally retains the complete
// public protocol while selecting an unsupported backend. Native adapter-only
// helpers are therefore unreachable in that configuration.
#![cfg_attr(not(feature = "webview"), allow(dead_code))]

mod backend;
mod model;
mod platform;
mod presentation;
mod system;

#[cfg(test)]
#[path = "backend_test.rs"]
mod backend_test;

#[cfg(test)]
#[path = "system_test.rs"]
mod system_test;

pub use model::{
    BrowsingRelationship, ButtonState, DeveloperToolsPolicy, DmaBufFrame, DmaBufPlane,
    DmaBufReadiness, FocusIntent, HistoryAction, HostWindowId, JavaScriptPolicy, NavigationId,
    NavigationTarget, PixelFrame, PointerButton, PolicyDecisionId, ScriptError, ScriptRequest,
    ScriptRequestId, ScriptWorld, StoragePartition, WebContentPoint, WebContentSize,
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
