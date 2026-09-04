//! Android surface presentation and acknowledgement ownership.

use neomacs_app::frontend_event::FrontendFrameId;
use neomacs_app::session::{
    ActiveFrontendPresentation, FrontendInputDisconnected, PendingFrontendFrame,
};
use neomacs_display_protocol::{FrameGlyphBuffer, GeometrySize, LogicalPixels};
use neomacs_layout_engine::bootstrap_frame::PortableBootstrapFrameBuilder;
use neomacs_wgpu_runtime::{
    PresentationOutcome, SurfaceCursorVisibility, SurfaceFramePresentError, SurfaceFrameRenderer,
    SurfaceScaleError,
};

pub(super) struct PresentedFrontend {
    renderer: SurfaceFrameRenderer,
    bootstrap: PortableBootstrapFrameBuilder,
    frame: Option<FrameGlyphBuffer>,
    active: Option<ActiveFrontendPresentation>,
}

impl PresentedFrontend {
    pub(super) fn new(renderer: SurfaceFrameRenderer) -> Self {
        let mut this = Self {
            renderer,
            bootstrap: PortableBootstrapFrameBuilder::new(),
            frame: None,
            active: None,
        };
        this.refresh_bootstrap_frame();
        this
    }

    pub(super) fn logical_size(
        &self,
    ) -> Result<Option<GeometrySize<LogicalPixels>>, neomacs_display_protocol::GeometryError> {
        self.renderer.logical_size()
    }

    pub(super) fn resize_physical(&mut self, width: u32, height: u32) {
        self.renderer.resize_physical(width, height);
        if self.active.is_none() {
            self.refresh_bootstrap_frame();
        }
    }

    pub(super) fn set_scale_factor(&mut self, scale: f64) -> Result<(), SurfaceScaleError> {
        self.renderer.set_scale_factor(scale)
    }

    pub(super) fn install(
        &mut self,
        pending: PendingFrontendFrame,
    ) -> Result<FrontendFrameId, FrontendInputDisconnected> {
        let target = pending.target();
        let frame = pending.materialize();
        let previous_frame = self.frame.replace(frame);
        let active = match pending.activate() {
            Ok(active) => active,
            Err(error) => {
                self.frame = previous_frame;
                return Err(error);
            }
        };
        self.active = Some(active);
        Ok(target)
    }

    pub(super) fn present(
        &mut self,
    ) -> Result<Option<PresentationOutcome>, SurfaceFramePresentError> {
        let cursor_visibility = if self.active.is_some() {
            SurfaceCursorVisibility::Visible
        } else {
            SurfaceCursorVisibility::Hidden
        };
        self.frame
            .as_ref()
            .map(|frame| self.renderer.present_frame(frame, cursor_visibility))
            .transpose()
    }

    fn refresh_bootstrap_frame(&mut self) {
        let size = self
            .renderer
            .logical_size()
            .unwrap_or_else(|error| panic!("invalid Android surface geometry: {error}"));
        self.frame = size.map(|size| {
            self.bootstrap
                .build(size)
                .unwrap_or_else(|error| panic!("failed to build Android initial frame: {error}"))
        });
    }
}

impl Drop for PresentedFrontend {
    fn drop(&mut self) {
        // Retire the visible evaluator revision before GPU/window resources
        // disappear. The guard's Drop sends the typed retirement event.
        self.active.take();
    }
}
