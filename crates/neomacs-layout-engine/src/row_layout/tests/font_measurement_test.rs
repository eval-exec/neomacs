use super::*;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::types::FaceId;

fn face() -> DisplayRowFace {
    DisplayRowFace::from_resolved(FaceId::new(1), &ResolvedFace::default())
}

#[test]
fn cancellation_does_not_initialize_native_fonts() {
    let mut source = FontMetricsService::new();
    let faces = [face()];
    let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
    let mut worker = WorkerFontMeasurements::default();
    assert!(matches!(
        worker.prepare(&snapshot, &faces, &|| true),
        Err(RowProgramError::Cancelled)
    ));
    assert!(worker.service.is_none());
}

#[test]
fn changed_font_metrics_reject_worker_measurement() {
    let mut source = FontMetricsService::new();
    let faces = [face()];
    let mut snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
    snapshot.receipts[0].ascent += 1.0;
    let mut worker = WorkerFontMeasurements::default();
    assert!(matches!(
        worker.prepare(&snapshot, &faces, &|| false),
        Err(RowProgramError::Cancelled)
    ));
}

#[test]
fn font_receipts_are_bounded_before_submission() {
    let mut source = FontMetricsService::new();
    let faces = vec![face(); MAX_CACHED_FACES];
    assert!(matches!(
        FontMeasurementSnapshot::capture(&faces, &mut source, &[]),
        Err(RowProgramError::Budget)
    ));
}

#[test]
fn worker_matches_captured_fonts_after_scale_change() {
    let mut source = FontMetricsService::new();
    let faces = [face()];
    let mut worker = WorkerFontMeasurements::default();
    for scale in [1.0, 1.5, 2.0, 1.0] {
        source.set_device_scale(DeviceScale::new(scale).unwrap());
        let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
        let service = worker.prepare(&snapshot, &faces, &|| false).unwrap();
        assert_eq!(service.device_scale().get(), scale);
    }
}
#[test]
fn repeated_worker_unicode_queries_keep_native_font_caches_warm() {
    use crate::display_item::*;
    let mut source = FontMetricsService::new();
    let faces = [face()];
    let items = [DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(faces[0].face_id),
        DisplayItemKind::TextRun(DisplayTextRun::independent("中")),
    )];
    let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &items).unwrap();
    let mut worker = WorkerFontMeasurements::default();
    for _ in 0..MAX_CACHED_CHARACTER_QUERIES + 1 {
        let service = worker.prepare(&snapshot, &faces, &|| false).unwrap();
        let face = &faces[0];
        assert!(
            service
                .resolved_font_for_char(
                    '中',
                    &face.font_family,
                    face.font_weight,
                    face.italic,
                    face.font_size
                )
                .is_some()
        );
        assert!(!service.worker_font_policy_missing());
    }
    assert_eq!(
        worker.cache_rebuilds, 1,
        "repeated use is not working-set growth"
    );
}

#[test]
fn worker_font_face_budget_counts_distinct_selections() {
    let mut source = FontMetricsService::new();
    let mut worker = WorkerFontMeasurements::default();
    for index in 0..MAX_CACHED_FACES {
        let mut current = face();
        current.font_size = 10.0 + index as f32;
        let faces = [current];
        let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
        worker.prepare(&snapshot, &faces, &|| false).unwrap();
    }
    for size in [10.0, 10.0 + MAX_CACHED_FACES as f32] {
        let mut current = face();
        current.font_size = size;
        let faces = [current];
        let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
        worker.prepare(&snapshot, &faces, &|| false).unwrap();
        assert_eq!(worker.cache_rebuilds, if size == 10.0 { 1 } else { 2 });
    }
}
