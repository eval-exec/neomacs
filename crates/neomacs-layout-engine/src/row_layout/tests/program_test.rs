use super::*;
use crate::display_row::face_state::DisplayRowGlyphMeasurer;
use crate::font::metrics::FontMetricsService;
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::neovm_bridge::ResolvedFace;

fn geometry(width: f32) -> RowProgramGeometry {
    RowProgramGeometry {
        inherited_line_spacing: 0.0,
        character_wrap: false,
        word_wrap: false,
        fringe: None,
        width,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        tabs: DisplayTabPolicy::every(4),
        base_face: FaceId::new(1),
        background: Color::BLACK,
    }
}
fn limits() -> RowProgramLimits {
    RowProgramLimits {
        items: 16,
        text_bytes: 4096,
        glyphs: 1024,
    }
}
fn items() -> Vec<DisplayItem> {
    let text = |id, text: &str| {
        DisplayItem::new(
            SourceSpan::synthetic(id, 0, text.chars().count()),
            RenderFaceRef::FaceId(FaceId::new(id as u32)),
            DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
        )
    };
    let mut raised = text(2, "affinity 好 café á שלום سلام");
    raised.layout.raise = Some(0.25);
    let mut spaced = text(1, "a b\tc");
    spaced.layout.space_width = Some(1.7);
    vec![
        raised,
        spaced,
        DisplayItem::new(
            SourceSpan::synthetic(1, 40, 41),
            RenderFaceRef::FaceId(FaceId::new(1)),
            DisplayItemKind::RowBreak(DisplayRowBreak {
                line_height: DisplayLineHeightPolicy::Default,
                line_spacing: DisplayLineSpacingPolicy::Inherit,
            }),
        ),
    ]
}
fn faces() -> Vec<DisplayRowFace> {
    let base = ResolvedFace::default();
    let mut tall = base.clone();
    tall.font_size = 26.0;
    vec![
        DisplayRowFace::from_resolved(FaceId::new(1), &base),
        DisplayRowFace::from_resolved(FaceId::new(2), &tall),
    ]
}

fn ascii_wrapped_program(text: &str) -> RowProgram {
    let face = FaceId::new(1);
    let faces = vec![DisplayRowFace::from_resolved(
        face,
        &ResolvedFace::default(),
    )];
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 8.0);
    let mut geometry = geometry(32.0);
    geometry.character_wrap = true;
    RowProgram::capture(
        geometry,
        [
            DisplayItem::new(
                SourceSpan::synthetic(1, 0, text.len()),
                RenderFaceRef::FaceId(face),
                DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
            ),
            DisplayItem::new(
                SourceSpan::synthetic(1, text.len(), text.len() + 1),
                RenderFaceRef::FaceId(face),
                DisplayItemKind::RowBreak(DisplayRowBreak {
                    line_height: DisplayLineHeightPolicy::Default,
                    line_spacing: DisplayLineSpacingPolicy::Inherit,
                }),
            ),
        ],
        faces.clone(),
        &mut measurer,
        limits(),
    )
    .unwrap()
}

fn measured_fragment(text: &str, start: usize, complete: bool) -> RowProgram {
    let mut program = ascii_wrapped_program(text);
    for (operation, _) in &mut program.operations {
        let offset = if matches!(operation, Operation::Break { .. }) {
            text.len()
        } else {
            0
        };
        let span = match operation {
            Operation::Text(input) => &mut input.span,
            Operation::Break { span, .. } => span,
            _ => unreachable!(),
        };
        let length = if offset == 0 { text.len() } else { 1 };
        *span = SourceSpan::synthetic(1, start + offset, start + offset + length);
    }
    if !complete {
        program.operations.pop();
    }
    program
}

#[test]
fn fragment_assembly_preserves_visual_rows_and_drops_unfinished_tail() {
    let mut prefix = measured_fragment("abcdef", 0, false);
    let partial = prefix.clone().compute_visual_rows(16, || false).unwrap();
    assert!(
        partial
            .iter()
            .all(|row| row.end_kind == ComputedRowEnd::Continuation)
    );
    assert!(partial.iter().map(|row| row.slots.len()).sum::<usize>() < 6);
    prefix
        .append_measured_fragment(
            measured_fragment("ghijkl", 6, true),
            RowProgramLimits {
                items: 64,
                text_bytes: 16384,
                glyphs: 4096,
            },
        )
        .unwrap();
    let joined = prefix.compute_visual_rows(16, || false).unwrap();
    let whole = ascii_wrapped_program("abcdefghijkl")
        .compute_visual_rows(16, || false)
        .unwrap();
    assert_eq!(joined.len(), whole.len());
    for (joined, whole) in joined.iter().zip(&whole) {
        assert_eq!(joined.source, whole.source);
        assert_eq!(joined.end_kind, whole.end_kind);
        assert_eq!(joined.slots.len(), whole.slots.len());
        // `==`, not the Debug text: each row carries its own appearance
        // id (`RowAppearance`), an identity that row equality ignores.
        assert_eq!(joined.row, whole.row);
    }
}

#[test]
fn fragment_assembly_rejects_gaps_geometry_changes_and_budget_overflow() {
    let maximum = RowProgramLimits {
        items: 64,
        text_bytes: 16384,
        glyphs: 4096,
    };
    let prefix = measured_fragment("abcd", 0, false);
    assert!(matches!(
        prefix
            .clone()
            .append_measured_fragment(measured_fragment("efgh", 5, true), maximum),
        Err(RowProgramError::Unsupported)
    ));
    let mut next = measured_fragment("efgh", 4, true);
    next.geometry.width += 1.0;
    assert!(matches!(
        prefix.clone().append_measured_fragment(next, maximum),
        Err(RowProgramError::Unsupported)
    ));
    assert!(matches!(
        prefix
            .clone()
            .append_measured_fragment(measured_fragment("efgh", 4, true), limits()),
        Err(RowProgramError::Budget)
    ));
    let mut next = measured_fragment("efgh", 4, true);
    next.measurements.char_width += 1.0;
    assert!(matches!(
        prefix.clone().append_measured_fragment(next, maximum),
        Err(RowProgramError::Unsupported)
    ));
    let mut next = measured_fragment("efgh", 4, true);
    next.faces[0].font_size += 1.0;
    assert!(matches!(
        prefix.clone().append_measured_fragment(next, maximum),
        Err(RowProgramError::Cancelled)
    ));
    assert!(matches!(
        prefix.compute_visual_rows(16, || true),
        Err(RowProgramError::Cancelled)
    ));
}

#[test]
fn wrapped_program_bounds_visual_rows_and_total_glyphs() {
    let program = ascii_wrapped_program("abcdefghijkl");
    let rows = program.clone().compute_visual_rows(16, || false).unwrap();
    assert!(rows.len() > 1);
    assert!(
        rows[..rows.len() - 1]
            .iter()
            .all(|row| row.end_kind == ComputedRowEnd::Continuation)
    );
    assert_eq!(rows.last().unwrap().end_kind, ComputedRowEnd::Newline);
    assert_eq!(rows.iter().map(|row| row.slots.len()).sum::<usize>(), 12);
    let prefix = program.clone().compute_visual_rows(1, || false).unwrap();
    assert_eq!(prefix.len(), 1);
    assert_eq!(prefix[0].end_kind, ComputedRowEnd::Continuation);
    assert!(matches!(
        program.clone().compute(|| false),
        Err(RowProgramError::Overflow)
    ));
    assert!(matches!(
        program.clone().compute_visual_rows(0, || false),
        Err(RowProgramError::Budget)
    ));
    let mut small = program;
    small.limits.glyphs = 5;
    assert!(matches!(
        small.compute_visual_rows(16, || false),
        Err(RowProgramError::Budget)
    ));
}

#[test]
fn wrapped_program_supports_tab_after_continuation_and_rejects_cancellation() {
    let rows = ascii_wrapped_program("abcdefghijkl\tz")
        .compute_visual_rows(16, || false)
        .unwrap();
    assert_eq!(rows.last().unwrap().end_kind, ComputedRowEnd::Newline);
    let calls = std::cell::Cell::new(0);
    assert!(matches!(
        ascii_wrapped_program("abcdefghijkl").compute_visual_rows(16, || {
            calls.set(calls.get() + 1);
            calls.get() > 1
        }),
        Err(RowProgramError::Cancelled)
    ));
}

#[test]
fn complete_row_program_uses_exact_captured_fonts_on_worker() {
    fn require_send<T: Send + 'static>() {}
    require_send::<RowProgram>();
    require_send::<ComputedRow>();
    let faces = faces();
    let mut fonts = FontMetricsService::new();
    let mut measurer = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        Some(&mut fonts),
        8.0,
        GlyphAdvanceQuantization::PreserveLogicalPixels,
        DisplayRowMeasurementMode::ConcreteFont,
    );
    let geometry = geometry(2000.0);
    let inputs = items();
    let job = RowProgram::capture(
        geometry.clone(),
        inputs.clone(),
        faces.clone(),
        &mut measurer,
        limits(),
    )
    .unwrap();
    // Independent synchronous writer call: no capture or replay adapter.
    let layout = geometry.layout();
    let mut expected = new_display_row(&layout);
    let mut position = DisplayRowPosition::new(0.0, 0);
    let mut slots = Vec::new();
    for input in inputs {
        // Match the buffer source's active-face installation before
        // handing the item to the independent synchronous glyph writer.
        let face = faces
            .iter()
            .find(|face| face.face_id == render_face_ref_id(input.face, geometry.base_face))
            .unwrap();
        let minimum = measurer.face_vertical_metrics_px(face.face_id).unwrap_or(
            DisplayRowVerticalMetrics::new(face.metrics.line_height_px(), face.metrics.ascent_px()),
        );
        let mut item_layout = layout.clone();
        item_layout.height_px = expected.height_px.max(minimum.height_px());
        item_layout.ascent_px = expected.ascent_px.max(minimum.ascent_px());
        let newline = match input.kind {
            DisplayItemKind::RowBreak(value) => Some(value),
            _ => None,
        };
        let progress = DisplayRowProgressWriter::with_glyph_measurer(
            &item_layout,
            &mut expected,
            &mut measurer,
            position,
            geometry.width,
        )
        .push_item(input);
        assert_ne!(progress.status(), DisplayRowAppendStatus::Clipped);
        position = progress.end();
        slots.extend(progress.slots().iter().cloned());
        if let Some(newline) = newline {
            DisplayRowLineEndFinalizer::new(
                newline,
                FaceId::new(1),
                geometry.width - position.x_px(),
                geometry.metrics,
                geometry.background,
                DisplayRowMeasurementMode::ConcreteFont,
                Default::default(),
                Default::default(),
            )
            .finalize(&mut expected, &faces);
        }
    }
    crate::glyph_row_writer::normalize_external_row(&mut expected);
    // Dropping the real font service before spawning prevents accidental
    // dependence on a live catalog or a fresh default-font worker.
    drop(measurer);
    drop(fonts);
    let actual = std::thread::spawn(move || job.compute(|| false))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(actual.row, expected);
    assert_eq!(actual.slots, slots);
}

#[test]
fn incomplete_overbudget_cancelled_and_clipped_rows_are_never_admitted() {
    let faces = faces();
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 8.0);
    assert!(matches!(
        RowProgram::capture(
            geometry(2000.0),
            items().into_iter().take(1),
            faces.clone(),
            &mut measurer,
            limits()
        ),
        Err(RowProgramError::Incomplete)
    ));
    for budget in [
        RowProgramLimits {
            items: 1,
            ..limits()
        },
        RowProgramLimits {
            text_bytes: 1,
            ..limits()
        },
        RowProgramLimits {
            glyphs: 1,
            ..limits()
        },
    ] {
        assert!(matches!(
            RowProgram::capture(
                geometry(2000.0),
                items(),
                faces.clone(),
                &mut measurer,
                budget
            ),
            Err(RowProgramError::Budget)
        ));
    }
    let job = RowProgram::capture(
        geometry(2000.0),
        items(),
        faces.clone(),
        &mut measurer,
        limits(),
    )
    .unwrap();
    assert!(matches!(
        job.compute(|| true),
        Err(RowProgramError::Cancelled)
    ));
    let job = RowProgram::capture(
        geometry(8.0),
        items(),
        faces.clone(),
        &mut measurer,
        limits(),
    )
    .unwrap();
    assert!(matches!(
        job.compute(|| false),
        Err(RowProgramError::Overflow)
    ));
}

#[test]
fn named_line_spacing_never_crosses_worker_boundary() {
    let _eval = neovm_core::emacs_core::Context::new();
    let faces = faces();
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 8.0);
    let mut input = items();
    if let DisplayItemKind::RowBreak(value) = &mut input.last_mut().unwrap().kind {
        value.line_spacing = DisplayLineSpacingPolicy::Scale {
            factor: 1.5,
            reference: DisplayLineSpacingReference::Named(neovm_core::emacs_core::Value::symbol(
                "default",
            )),
        };
    }
    assert!(matches!(
        RowProgram::capture(
            geometry(2000.0),
            input,
            faces.clone(),
            &mut measurer,
            limits()
        ),
        Err(RowProgramError::Unsupported)
    ));
}
