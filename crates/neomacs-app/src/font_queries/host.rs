use super::*;
use neovm_core::emacs_core::display_host::{AvailableFontFamilyName, FontQueryHost};

impl FontQueryHost for FontQueryService {
    fn list_font_families(
        &mut self,
        _frame_id: FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String> {
        Ok(FontQueryService::list_font_families(self))
    }
    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        self.resolve_font_for_char(request)
    }

    fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        self.resolve_frame_font(frame_id, request)
    }

    fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        self.resolve_font_for_spec(request)
    }

    fn probe_font_px_metrics(
        &mut self,
        file: &str,
        face_index: u32,
        pixel_size: u32,
        wght: Option<f32>,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontPxProbeResult>, String> {
        self.probe_font_px_metrics(file, face_index, pixel_size, wght)
    }

    fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String> {
        self.probe_font_entity_metrics(request)
    }

    fn font_otf_capability(
        &mut self,
        file: &str,
        face_index: u32,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontOtfCapability>, String> {
        self.font_otf_capability(file, face_index)
    }
}
