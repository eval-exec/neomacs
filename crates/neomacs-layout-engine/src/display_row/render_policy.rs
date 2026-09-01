use crate::display_item::DisplayItem;
use crate::display_row::builder::DisplayRowItemMeasurement;
use crate::font::metrics::FontMetricsService;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::types::FaceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayRowRenderClipBehavior {
    PreserveRemainderAndStop,
    Continue,
}

pub(crate) trait DisplayRowRenderPolicy {
    fn stop_before_item(
        &mut self,
        _item: &DisplayItem,
        _face_id: FaceId,
        _face: &ResolvedFace,
    ) -> bool {
        false
    }

    fn measurement_for(
        &mut self,
        _item: &DisplayItem,
        _face_id: FaceId,
        _font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        DisplayRowItemMeasurement::Default
    }

    fn clipped_behavior(&mut self, _item: &DisplayItem) -> DisplayRowRenderClipBehavior {
        DisplayRowRenderClipBehavior::PreserveRemainderAndStop
    }
}

pub(crate) struct NaturalDisplayRowRenderPolicy;

impl DisplayRowRenderPolicy for NaturalDisplayRowRenderPolicy {}
