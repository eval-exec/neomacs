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
    pending: Option<(FrameGlyphBuffer, ActiveFrontendPresentation)>,
    input_ready: bool,
}

/// CPU scene ownership survives Activity surface loss; no GPU or window handle.
pub(super) struct RetainedPresentation(FrameGlyphBuffer, ActiveFrontendPresentation);

impl RetainedPresentation {
    pub(super) fn from_pending(
        pending: PendingFrontendFrame,
    ) -> Result<Self, FrontendInputDisconnected> {
        let frame = pending.materialize();
        Ok(Self(frame, pending.activate()?))
    }
}

impl PresentedFrontend {
    pub(super) fn set_preedit(&mut self, text: String) {
        self.renderer.set_preedit(text);
    }
    pub(super) fn restore(&mut self, retained: RetainedPresentation) {
        self.pending = Some((retained.0, retained.1));
    }

    pub(super) fn retain(&mut self) -> Option<RetainedPresentation> {
        self.input_ready = false;
        if let Some((frame, active)) = self.pending.take() {
            return Some(RetainedPresentation(frame, active));
        }
        self.active.take().and_then(|active| {
            self.frame
                .take()
                .map(|frame| RetainedPresentation(frame, active))
        })
    }
    pub(super) fn new(renderer: SurfaceFrameRenderer) -> Self {
        let mut this = Self {
            renderer,
            bootstrap: PortableBootstrapFrameBuilder::new(),
            frame: None,
            active: None,
            pending: None,
            input_ready: false,
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
        self.input_ready = false;
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
        let active = pending.activate()?;
        self.pending = Some((frame, active));
        Ok(target)
    }

    pub(super) fn present(
        &mut self,
    ) -> Result<Option<PresentationOutcome>, SurfaceFramePresentError> {
        let cursor_visibility = if self.active.is_some() || self.pending.is_some() {
            SurfaceCursorVisibility::Visible
        } else {
            SurfaceCursorVisibility::Hidden
        };
        let outcome = self
            .pending
            .as_ref()
            .map(|(frame, _)| frame)
            .or(self.frame.as_ref())
            .map(|frame| self.renderer.present_frame(frame, cursor_visibility))
            .transpose()?;
        if outcome == Some(PresentationOutcome::Presented) {
            if let Some((frame, active)) = self.pending.take() {
                self.frame = Some(frame);
                self.active = Some(active);
            }
            self.input_ready = self.active.is_some();
        }
        Ok(outcome)
    }

    pub(super) fn pointer_input(
        &self,
        x: f32,
        y: f32,
        action: neomacs_display_protocol::PointerAction,
    ) -> Option<neomacs_display_protocol::PositionedPointerInput> {
        use neomacs_display_protocol::present_mapping::DeviceSurfacePoint;
        use neomacs_display_protocol::{
            PointerPosition, PointerTarget, PositionedPointerInput, PresentedHitQuery,
        };
        if !self.input_ready {
            return None;
        }
        let frame = self.frame.as_ref()?;
        let mapping = self.renderer.mapping_for_frame(frame).ok()??;
        let point = mapping.frame_from_device(DeviceSurfacePoint::from_px(x, y).ok()?)?;
        let projected =
            neomacs_display_protocol::interaction_projection::InteractionProjection::settled(
                frame.presentation_id,
            )
            .map(
                neomacs_display_protocol::GeometryPoint::<
                    neomacs_display_protocol::geometry::RootSurfaceSpace,
                    LogicalPixels,
                >::from_px(point.x(), point.y())
                .ok()?,
            )?;
        let hit = frame
            .resolve_presented_hit(PresentedHitQuery::new(projected))
            .ok()?
            .and_then(|hit| hit.semantic());
        Some(PositionedPointerInput {
            position: PointerPosition {
                x: point.x(),
                y: point.y(),
                target_frame_id: frame.frame_placement.frame().get(),
            },
            target: PointerTarget::Presented {
                presentation: frame.presentation_id.get(),
                hit,
            },
            action,
        })
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

    pub(super) fn animation_interval(&self) -> Option<std::time::Duration> {
        if !self.input_ready {
            return None;
        }
        self.renderer
            .animation_interval(self.frame.as_ref()?, SurfaceCursorVisibility::Visible)
    }
}

impl Drop for PresentedFrontend {
    fn drop(&mut self) {
        // Retire the visible evaluator revision before GPU/window resources
        // disappear. The guard's Drop sends the typed retirement event.
        self.active.take();
    }
}
