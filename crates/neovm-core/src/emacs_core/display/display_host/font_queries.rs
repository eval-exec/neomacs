//! Font queries independent of window, popup, or media ownership.
use super::{AvailableFontFamilyName, DisplayHost, FontResolveRequest, FrameFontRequest};
use crate::emacs_core::eval::{
    FontEntityMetricsRequest, FontSpecResolveRequest, ResolvedFontEntityMetrics, ResolvedFontMatch,
    ResolvedFontSpecMatch, ResolvedFrameFont,
};
use crate::window::FrameId;

pub(crate) fn font_queries_for_hosts<'a>(
    display: &'a mut Option<Box<dyn DisplayHost>>,
    fonts: &'a mut Option<Box<dyn FontQueryHost>>,
) -> Option<&'a mut dyn FontQueryHost> {
    if let Some(host) = display.as_mut() {
        Some(host)
    } else if let Some(host) = fonts.as_mut() {
        Some(host.as_mut())
    } else {
        None
    }
}

/// Synchronous, evaluator-thread font capabilities. No window ownership is implied.
pub trait FontQueryHost {
    fn list_font_families(
        &mut self,
        _frame_id: FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String>;
    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String>;

    fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String>;

    fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String>;

    fn probe_font_px_metrics(
        &mut self,
        file: &str,
        face_index: u32,
        pixel_size: u32,
        wght: Option<f32>,
    ) -> Result<Option<crate::emacs_core::eval::FontPxProbeResult>, String>;

    fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String>;

    fn font_otf_capability(
        &mut self,
        file: &str,
        face_index: u32,
    ) -> Result<Option<crate::emacs_core::eval::FontOtfCapability>, String>;
}

// Preserve native DisplayHost implementations while allowing direct surfaces
// to install only font capabilities. Native font selection still has one owner.
impl FontQueryHost for Box<dyn DisplayHost> {
    fn list_font_families(
        &mut self,
        _frame_id: FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String> {
        self.as_mut().list_font_families(_frame_id)
    }
    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        self.as_mut().resolve_font_for_char(request)
    }

    fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        self.as_mut().resolve_frame_font(frame_id, request)
    }

    fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        self.as_mut().resolve_font_for_spec(request)
    }

    fn probe_font_px_metrics(
        &mut self,
        file: &str,
        face_index: u32,
        pixel_size: u32,
        wght: Option<f32>,
    ) -> Result<Option<crate::emacs_core::eval::FontPxProbeResult>, String> {
        self.as_mut()
            .probe_font_px_metrics(file, face_index, pixel_size, wght)
    }

    fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String> {
        self.as_mut().probe_font_entity_metrics(request)
    }

    fn font_otf_capability(
        &mut self,
        file: &str,
        face_index: u32,
    ) -> Result<Option<crate::emacs_core::eval::FontOtfCapability>, String> {
        self.as_mut().font_otf_capability(file, face_index)
    }
}
