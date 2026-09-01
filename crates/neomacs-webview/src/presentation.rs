use std::collections::HashMap;

use neomacs_display_protocol::{DeviceScale, DisplayWindowId, RootSurfaceRect, WebViewId};

use crate::{HostWindowId, WebViewOccurrenceId, WebViewSceneRevision};

/// Offset of the visible viewport's origin inside the full browser content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WebContentOffset {
    x: f32,
    y: f32,
}

impl WebContentOffset {
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
}

/// One already-clipped WebView occurrence in a sealed display presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedWebViewPlacement {
    view: WebViewId,
    occurrence: WebViewOccurrenceId,
    owner: DisplayWindowId,
    content_rect: RootSurfaceRect,
    visible_rect: RootSurfaceRect,
    content_offset: WebContentOffset,
    device_scale: DeviceScale,
}

impl ResolvedWebViewPlacement {
    pub fn new(
        view: WebViewId,
        occurrence: WebViewOccurrenceId,
        owner: DisplayWindowId,
        content_rect: RootSurfaceRect,
        visible_rect: RootSurfaceRect,
        device_scale: DeviceScale,
    ) -> Result<Self, WebViewPlacementError> {
        if content_rect.width() <= 0.0 || content_rect.height() <= 0.0 {
            return Err(WebViewPlacementError::EmptyContent);
        }
        if visible_rect.width() <= 0.0 || visible_rect.height() <= 0.0 {
            return Err(WebViewPlacementError::EmptyVisibleRegion);
        }

        let offset_x = visible_rect.x() - content_rect.x();
        let offset_y = visible_rect.y() - content_rect.y();
        let visible_right = visible_rect.x() + visible_rect.width();
        let visible_bottom = visible_rect.y() + visible_rect.height();
        let content_right = content_rect.x() + content_rect.width();
        let content_bottom = content_rect.y() + content_rect.height();
        if offset_x < 0.0
            || offset_y < 0.0
            || visible_right > content_right
            || visible_bottom > content_bottom
        {
            return Err(WebViewPlacementError::VisibleRegionOutsideContent);
        }

        Ok(Self {
            view,
            occurrence,
            owner,
            content_rect,
            visible_rect,
            content_offset: WebContentOffset {
                x: offset_x,
                y: offset_y,
            },
            device_scale,
        })
    }

    #[must_use]
    pub const fn view(&self) -> WebViewId {
        self.view
    }

    #[must_use]
    pub const fn occurrence(&self) -> WebViewOccurrenceId {
        self.occurrence
    }

    #[must_use]
    pub const fn owner(&self) -> DisplayWindowId {
        self.owner
    }

    #[must_use]
    pub const fn content_rect(&self) -> RootSurfaceRect {
        self.content_rect
    }

    #[must_use]
    pub const fn visible_rect(&self) -> RootSurfaceRect {
        self.visible_rect
    }

    #[must_use]
    pub const fn content_offset(&self) -> WebContentOffset {
        self.content_offset
    }

    #[must_use]
    pub const fn device_scale(&self) -> DeviceScale {
        self.device_scale
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebViewPlacementError {
    #[error("webview content rectangle is empty")]
    EmptyContent,
    #[error("webview visible rectangle is empty")]
    EmptyVisibleRegion,
    #[error("webview visible rectangle lies outside its content rectangle")]
    VisibleRegionOutsideContent,
}

/// Complete WebView state for one host's active presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedWebViewScene {
    host: HostWindowId,
    revision: WebViewSceneRevision,
    placements: Box<[ResolvedWebViewPlacement]>,
}

impl ResolvedWebViewScene {
    pub fn try_new(
        host: HostWindowId,
        revision: WebViewSceneRevision,
        placements: Vec<ResolvedWebViewPlacement>,
    ) -> Result<Self, WebViewSceneError> {
        let mut seen = HashMap::with_capacity(placements.len());
        for placement in &placements {
            if let Some(first) = seen.insert(placement.view(), placement.occurrence()) {
                return Err(WebViewSceneError::DuplicateView {
                    view: placement.view(),
                    first,
                    duplicate: placement.occurrence(),
                });
            }
        }

        Ok(Self {
            host,
            revision,
            placements: placements.into_boxed_slice(),
        })
    }

    #[must_use]
    pub const fn host(&self) -> HostWindowId {
        self.host
    }

    #[must_use]
    pub const fn revision(&self) -> WebViewSceneRevision {
        self.revision
    }

    #[must_use]
    pub fn placements(&self) -> &[ResolvedWebViewPlacement] {
        &self.placements
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebViewSceneError {
    #[error("webview {view} appears twice in one presentation ({first:?} and {duplicate:?})")]
    DuplicateView {
        view: WebViewId,
        first: WebViewOccurrenceId,
        duplicate: WebViewOccurrenceId,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebViewPresentationEffects {
    changed: bool,
}

impl WebViewPresentationEffects {
    pub(crate) const fn new(changed: bool) -> Self {
        Self { changed }
    }

    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebViewPresentationError {
    #[error(
        "host {host:?} already presents {current:?}; cannot install stale presentation {received:?}"
    )]
    Stale {
        host: HostWindowId,
        current: WebViewSceneRevision,
        received: WebViewSceneRevision,
    },
    #[error("presentation {presentation:?} for host {host:?} conflicts with its installed scene")]
    ConflictingRevision {
        host: HostWindowId,
        presentation: WebViewSceneRevision,
    },
    #[error("presentation references unknown webview {0}")]
    UnknownView(WebViewId),
    #[error("webview {view} is already attached to host {current:?}, not {requested:?}")]
    AttachedToAnotherHost {
        view: WebViewId,
        current: HostWindowId,
        requested: HostWindowId,
    },
    #[error("webview {view} presentation failed on host {host:?}: {error}")]
    Backend {
        host: HostWindowId,
        view: WebViewId,
        error: String,
    },
}
