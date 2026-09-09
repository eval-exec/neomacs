//! Drawing parameters are immutable resources, never renderer-global GPU state.

mod parameters;
pub(super) use parameters::{DrawParameterCache, DrawParameters};

use super::{WgpuRenderer, target::RenderTarget};

/// Scoped drawing destination. Dropping it requires no global-state restoration.
pub struct DrawContext<'r, 't> {
    pub(super) renderer: &'r mut WgpuRenderer,
    pub(super) target: RenderTarget<'t>,
    pub(super) parameters: DrawParameters,
}

impl WgpuRenderer {
    /// Copy an already-validated composition into the native content viewport.
    /// No scaling, alpha blending, or native-window calls occur here.
    pub fn place_native_content(
        &mut self,
        placement: super::NativeContentPlacement<'_>,
        background: neomacs_display_protocol::Color,
    ) {
        let draw = self.begin_draw(placement.target);
        draw.renderer.paint_blit(
            draw.target,
            &draw.parameters,
            placement.source.bind_group(),
            super::paint::BlitPlacement::NativeContent(background),
        );
    }

    pub fn begin_draw<'r, 't>(&'r mut self, target: RenderTarget<'t>) -> DrawContext<'r, 't> {
        assert_eq!(
            target.view.texture().format(),
            self.surface_format,
            "draw target must match the renderer's pipeline format"
        );
        let size = target.surface.logical_size();
        let parameters = self.parameters([size.width(), size.height()], 0.0);
        DrawContext {
            renderer: self,
            target,
            parameters,
        }
    }
}

impl DrawContext<'_, '_> {
    pub fn paint_menu(
        &mut self,
        menu: &neomacs_display_protocol::menu::MenuPanelPaint<'_>,
        atlas: &mut crate::WgpuGlyphAtlas,
    ) {
        self.renderer
            .paint_menu(self.target, &self.parameters, menu, atlas);
    }

    pub fn blit_retained(&mut self, source: &wgpu::BindGroup) {
        self.renderer.paint_blit(
            self.target,
            &self.parameters,
            source,
            super::paint::BlitPlacement::Retained,
        );
    }
}
