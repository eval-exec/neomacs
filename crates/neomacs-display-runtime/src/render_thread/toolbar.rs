//! Toolbar texture residency across all accepted frame presentations.
//!
//! One exact realization key drives both decoding and lookup. Reconciliation
//! retains variants shared by windows and retires obsolete variants through the
//! renderer's image lifetime fence; it never clears unrelated editor images.

use std::collections::{HashMap, HashSet};

use neomacs_display_protocol::{
    DeviceScale, FrameChrome, FrameChromeContent, ImageId, PresentationId, ToolBarIconKey,
};
use neomacs_renderer_wgpu::WgpuRenderer;

use super::RenderApp;

pub(super) trait ToolbarImageStore {
    fn load(&mut self, key: &ToolBarIconKey) -> ImageId;
    fn retire(&mut self, image: ImageId);
}

impl ToolbarImageStore for WgpuRenderer {
    fn load(&mut self, key: &ToolBarIconKey) -> ImageId {
        self.load_toolbar_icon(key)
    }
    fn retire(&mut self, image: ImageId) {
        self.retire_image(image);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ToolbarRealizationState {
    #[default]
    Active,
    /// Wholesale `clear-image-cache` dropped every toolbar texture after this
    /// presentation. Hold chrome icons unrealized until a strictly newer
    /// post-clear presentation is ingested (matching GNU `Fclear_image_cache`,
    /// where chrome reloads on the next redisplay rather than before a
    /// following `image-cache-size` read).
    ClearedUntilAfter(PresentationId),
}

#[derive(Default)]
pub(super) struct ToolbarResources {
    textures: HashMap<ToolBarIconKey, ImageId>,
    realization: ToolbarRealizationState,
}

impl ToolbarResources {
    pub(super) fn textures(&self) -> &HashMap<ToolBarIconKey, ImageId> {
        &self.textures
    }

    /// Forget every realized icon texture after a wholesale `clear-image-cache`
    /// and suppress re-realization until a strictly newer presentation arrives.
    pub(super) fn clear_after_presentation(&mut self, cleared_after: PresentationId) {
        self.textures.clear();
        self.realization = match self.realization {
            ToolbarRealizationState::Active => {
                ToolbarRealizationState::ClearedUntilAfter(cleared_after)
            }
            ToolbarRealizationState::ClearedUntilAfter(existing) => {
                ToolbarRealizationState::ClearedUntilAfter(PresentationId::new(
                    existing.get().max(cleared_after.get()),
                ))
            }
        };
    }

    /// Resume toolbar realization once a post-clear frame presentation arrives.
    pub(super) fn note_frame_published(&mut self, presentation: PresentationId) {
        if let ToolbarRealizationState::ClearedUntilAfter(fence) = self.realization
            && presentation.get() > fence.get()
        {
            self.realization = ToolbarRealizationState::Active;
        }
    }

    /// The old GPU already owns/destroyed these resources; do not retire them
    /// through the replacement device's unrelated image identities. Preserves
    /// `realization` so active frames reload immediately onto the replacement
    /// GPU while a cache-clear hold stays cleared until the next redisplay.
    pub(super) fn forget_after_device_loss(&mut self) {
        self.textures.clear();
    }

    pub(super) fn reconcile(
        &mut self,
        required: HashSet<ToolBarIconKey>,
        store: &mut impl ToolbarImageStore,
    ) {
        if matches!(
            self.realization,
            ToolbarRealizationState::ClearedUntilAfter(_)
        ) {
            return;
        }
        self.textures.retain(|key, image| {
            if required.contains(key) {
                true
            } else {
                store.retire(*image);
                false
            }
        });
        for key in required {
            self.textures
                .entry(key)
                .or_insert_with_key(|key| store.load(key));
        }
    }
}

fn extend_required(
    required: &mut HashSet<ToolBarIconKey>,
    chrome: &FrameChrome,
    scale: DeviceScale,
) {
    for band in chrome.bands() {
        let (items, style) = match band.content() {
            FrameChromeContent::ToolBar(content) => (content.items(), content.icon_style()),
            FrameChromeContent::CompactBar(content) => (content.tool_items(), content.icon_style()),
            FrameChromeContent::MenuBar(_) | FrameChromeContent::DisplayRow(_) => continue,
        };
        required.extend(items.iter().filter_map(|positioned| {
            let item = positioned.item();
            if item.is_separator() {
                return None;
            }
            item.image
                .as_ref()
                .map(|source| style.realize(source.clone(), scale))
        }));
    }
}

impl RenderApp {
    pub(super) fn synchronize_toolbar_resources(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mut required = HashSet::new();
        self.frame_windows.for_each_top_level_window(|window| {
            let scale = DeviceScale::new(window.scale_factor() as f32)
                .expect("native window scale must be positive and finite");
            if let Some(frame) = window.render.compositor.current_frame.as_ref() {
                extend_required(&mut required, &frame.frame_chrome, scale);
            }
            for child in window.render.compositor.child_frames.frames.values() {
                extend_required(&mut required, &child.frame.frame_chrome, scale);
            }
        });
        self.toolbar.reconcile(required, renderer);
    }
}

#[cfg(test)]
#[path = "toolbar/tests/toolbar_test.rs"]
mod tests;
