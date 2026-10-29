use super::*;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::types::FaceId;

fn face() -> DisplayRowFace {
    DisplayRowFace::from_resolved(FaceId::new(1), &ResolvedFace::default())
}

#[test]
fn cancellation_does_not_initialize_native_fonts() {
    let mut source = crate::font::metrics::test_catalog::metrics_service();
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
fn worker_retains_the_pinned_catalog_and_rebuilds_for_another_owner() {
    use crate::display_item::*;
    use crate::font::metrics::test_catalog::{TestFontCatalog, TestFontCatalogSource};
    let files = [
        neomacs_test_fonts::noto_sans_mono_variable(),
        neomacs_test_fonts::noto_sans_arabic_variable(),
    ];
    let faces = [face()];
    let items = [DisplayItem::new(
        SourceSpan::synthetic(1, 0, 4),
        RenderFaceRef::FaceId(faces[0].face_id),
        DisplayItemKind::TextRun(DisplayTextRun::independent("سلام")),
    )];
    let mut worker = WorkerFontMeasurements::default();
    for expected_rebuilds in 1..=2 {
        let catalog = TestFontCatalog::from_files(&files).expect("pinned measurement catalog");
        let mut source = catalog.metrics_service();
        let snapshot = FontMeasurementSnapshot::capture(&faces, &mut source, &items).unwrap();
        let service = worker.prepare(&snapshot, &faces, &|| false).unwrap();
        assert_eq!(
            service.test_catalog(),
            TestFontCatalogSource::Fixture(catalog)
        );
        let selected = service
            .resolved_font_for_char(
                'ل',
                &faces[0].font_family,
                faces[0].font_weight,
                faces[0].italic,
                faces[0].font_size.max(1.0),
            )
            .expect("Arabic fixture selected in the worker");
        assert_eq!(
            std::path::Path::new(selected.identity.file_path.as_ref().unwrap()),
            files[1]
        );
        assert!(!service.worker_font_policy_missing());
        assert_eq!(worker.cache_rebuilds, expected_rebuilds);
    }
}

#[test]
fn changed_font_metrics_reject_worker_measurement() {
    let mut source = crate::font::metrics::test_catalog::metrics_service();
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
    let mut source = crate::font::metrics::test_catalog::metrics_service();
    let faces = vec![face(); MAX_CACHED_FACES];
    assert!(matches!(
        FontMeasurementSnapshot::capture(&faces, &mut source, &[]),
        Err(RowProgramError::Budget)
    ));
}

#[test]
fn worker_matches_captured_fonts_after_scale_change() {
    let mut source = crate::font::metrics::test_catalog::metrics_service();
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
    let mut source = crate::font::metrics::test_catalog::metrics_service();
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
    let mut source = crate::font::metrics::test_catalog::metrics_service();
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

#[test]
fn joined_font_receipts_authorize_both_fragment_character_policies() {
    use crate::display_item::*;
    use crate::font::metrics::test_catalog::TestFontCatalog;
    let catalog = TestFontCatalog::from_files(&[
        neomacs_test_fonts::noto_sans_mono_variable(),
        neomacs_test_fonts::noto_sans_arabic_variable(),
        neomacs_test_fonts::noto_sans_hebrew_variable(),
    ])
    .expect("pinned fragment fonts");
    let mut source = catalog.metrics_service();
    let faces = [face()];
    let items = |text: &str| {
        [DisplayItem::new(
            SourceSpan::synthetic(1, 0, text.chars().count()),
            RenderFaceRef::FaceId(faces[0].face_id),
            DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
        )]
    };
    let arabic = FontMeasurementSnapshot::capture(&faces, &mut source, &items("س")).unwrap();
    let hebrew = FontMeasurementSnapshot::capture(&faces, &mut source, &items("ש")).unwrap();
    let ascii = FontMeasurementSnapshot::capture(&faces, &mut source, &[]).unwrap();
    assert_eq!(
        arabic.policy.merged(&ascii.policy, 0),
        Err(crate::font::resolver::FontPolicyMergeError::Budget)
    );
    let joined = arabic.merged(&hebrew).expect("same capture authority");
    assert_eq!(joined.receipts.len(), 1);
    assert_eq!(joined.policy.query_count(), 2);
    let mut worker = WorkerFontMeasurements::default();
    let fonts = worker.prepare(&joined, &faces, &|| false).unwrap();
    for ch in ['س', 'ש'] {
        assert!(
            fonts
                .resolved_font_for_char(
                    ch,
                    &faces[0].font_family,
                    faces[0].font_weight,
                    faces[0].italic,
                    faces[0].font_size.max(1.0),
                )
                .is_some()
        );
    }
    assert!(!fonts.worker_font_policy_missing());
    let mut changed = hebrew.clone();
    changed.receipts[0].ascent += 1.0;
    assert_eq!(arabic.merged(&changed), Err(RowProgramError::Cancelled));
    let mut changed = hebrew;
    changed.revision.scale_bits = 2.0f32.to_bits();
    assert_eq!(arabic.merged(&changed), Err(RowProgramError::Cancelled));
}

#[test]
fn native_font_authority_spends_only_its_owner_reservation() {
    use crate::font::metrics::test_catalog::TestFontCatalog;
    let catalog = TestFontCatalog::from_files(&[neomacs_test_fonts::noto_sans_mono_variable()])
        .expect("pinned font receipt catalog");
    let mut source = catalog.metrics_service();
    let original: Vec<_> = (1..=20)
        .map(|id| DisplayRowFace::from_resolved(FaceId::new(id), &ResolvedFace::default()))
        .collect();
    assert!(matches!(
        FontMeasurementSnapshot::capture(&original, &mut source, &[]),
        Err(RowProgramError::Budget)
    ));
    let mut owner = FontSnapshotOwner::default();
    let geometry = crate::row_layout::program::RowProgramGeometry {
        inherited_line_spacing: 0.0,
        character_wrap: false,
        word_wrap: false,
        word_wrap_policy: Default::default(),
        fringe: None,
        width: 200.0,
        text_origin: crate::display_row::geometry::DisplayRowTextAreaOrigin::row_local(),
        metrics: crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
            8.0, 16.0, 12.0,
        ),
        tabs: crate::display_row::builder::DisplayTabPolicy::every(4),
        base_face: FaceId::new(1),
        background: neomacs_display_protocol::types::Color::BLACK,
    };
    assert!(matches!(
        owner.capture_program(
            geometry,
            Vec::new(),
            original.clone(),
            &mut source,
            crate::row_layout::program::RowProgramLimits {
                items: 0,
                text_bytes: 4096,
                glyphs: 256
            }
        ),
        Err(RowProgramError::Budget)
    ));
    assert!(
        owner.reserved.is_empty(),
        "a rejected row cannot spend its candidate's font budget"
    );
    let first = owner
        .capture(&original, &mut source, &[])
        .expect("whole-owner native authority");
    assert!(first.bytes() > MAX_ROW_FONT_BYTES);
    assert!(first.bytes() <= super::super::worker::MAX_FONT_BYTES);
    let mut worker = WorkerFontMeasurements::default();
    worker
        .prepare(&first, &original, &|| false)
        .expect("all primary font receipts validated");
    assert!(matches!(
        FontMeasurementSnapshot::capture_with_budget(
            &original,
            &mut source,
            &[],
            FontSnapshotBudget::bounded(first.bytes() - 1).unwrap(),
        ),
        Err(RowProgramError::Budget)
    ));
    assert!(matches!(
        FontSnapshotBudget::bounded(super::super::worker::MAX_FONT_BYTES + 1),
        Err(RowProgramError::Budget)
    ));
    let mut exhausted = false;
    let mut changed = original.clone();
    let attempts = super::super::worker::MAX_FONT_BYTES / first.bytes() + 2;
    for _ in 0..attempts {
        for face in &mut changed {
            face.font_size += 1.0;
        }
        let admitted = owner.reserved.len();
        match owner.capture(&changed, &mut source, &[]) {
            Ok(_) => {}
            Err(RowProgramError::Budget) => {
                assert_eq!(
                    owner.reserved.len(),
                    admitted,
                    "refusal cannot publish another owner"
                );
                exhausted = true;
                break;
            }
            Err(error) => panic!("unexpected font admission refusal: {error:?}"),
        }
    }
    assert!(
        exhausted,
        "the unchanged mailbox ceiling must reject distinct new receipts"
    );
    assert!(
        owner
            .reserved
            .iter()
            .map(|snapshot| snapshot.bytes())
            .sum::<usize>()
            <= super::super::worker::MAX_FONT_BYTES
    );
    let admitted = owner.reserved.len();
    let reused = owner
        .capture(&original, &mut source, &[])
        .expect("already reserved authority reuses its charge");
    assert!(std::sync::Arc::ptr_eq(&first, &reused));
    assert_eq!(owner.reserved.len(), admitted);
}
