use super::*;
use crate::display_source::{DisplayItemSource, DisplaySourceContext};
use neomacs_display_protocol::types::{FaceId, VideoId};
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos};

fn buffer_span(buffer_id: BufferId, start_char: usize, end_char: usize) -> SourceSpan {
    SourceSpan::new(
        DisplaySourcePosition::buffer(
            buffer_id,
            CharPos0::new(start_char),
            EmacsBytePos::new(start_char),
        ),
        DisplaySourcePosition::buffer(
            buffer_id,
            CharPos0::new(end_char),
            EmacsBytePos::new(end_char),
        ),
    )
}

#[test]
fn display_item_text_run_keeps_source_span_and_face_ref() {
    let span = buffer_span(BufferId(7), 3, 6);
    let item = DisplayItem::new(
        span.clone(),
        RenderFaceRef::FaceId(FaceId::new(12)),
        DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
    );

    assert_eq!(item.span, span);
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(12)));
    assert_eq!(
        item.kind,
        DisplayItemKind::TextRun(DisplayTextRun::new("abc"))
    );
}

#[test]
fn display_item_stretch_uses_typed_lengths() {
    let item = DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::Inherit,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(32.0)),
            height: Some(DisplayLength::Pixels(14.0)),
            ascent: Some(DisplayLength::Pixels(10.0)),
        }),
    );

    assert_eq!(
        item.kind,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(32.0)),
            height: Some(DisplayLength::Pixels(14.0)),
            ascent: Some(DisplayLength::Pixels(10.0)),
        })
    );
}

#[test]
fn display_item_inline_media_slots_are_source_neutral() {
    let span = SourceSpan::lisp_string(2, 0, 1, 0, 1);

    let image = DisplayItem::new(
        span.clone(),
        RenderFaceRef::Inherit,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::image(DisplayImageItem {
            image_id: 42,
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            width: 64.0,
            height: 32.0,
            ascent: 32.0,
            horizontal_margin: 0.0,
            vertical_margin: 0.0,
            opaque_background: None,
        })),
    );
    let video = DisplayItem::new(
        span.clone(),
        RenderFaceRef::Inherit,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::video(DisplayVideoItem {
            video_id: VideoId::new(43),
            width: 80.0,
            height: 45.0,
            opacity: 0.75,
        })),
    );
    let xwidget = DisplayItem::new(
        span,
        RenderFaceRef::Inherit,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
            xwidget_id: neomacs_display_protocol::XwidgetId::new(44),
            webview_id: neomacs_display_protocol::WebViewId::new(440),
            width: 96.0,
            height: 54.0,
        })),
    );

    assert_eq!(
        image.kind,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::image(DisplayImageItem {
            image_id: 42,
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            width: 64.0,
            height: 32.0,
            ascent: 32.0,
            horizontal_margin: 0.0,
            vertical_margin: 0.0,
            opaque_background: None,
        }))
    );
    assert_eq!(
        video.kind,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::video(DisplayVideoItem {
            video_id: VideoId::new(43),
            width: 80.0,
            height: 45.0,
            opacity: 0.75,
        }))
    );
    assert_eq!(
        xwidget.kind,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
            xwidget_id: neomacs_display_protocol::XwidgetId::new(44),
            webview_id: neomacs_display_protocol::WebViewId::new(440),
            width: 96.0,
            height: 54.0,
        }))
    );
}

#[test]
fn display_item_row_break_is_a_typed_item() {
    let source_pos = DisplaySourcePosition::lisp_string(5, 4, 4);
    let span = SourceSpan::new(source_pos.clone(), source_pos);

    let row_break = DisplayItem::new(
        span,
        RenderFaceRef::Inherit,
        DisplayItemKind::RowBreak(DisplayRowBreak::explicit_newline()),
    );

    assert!(matches!(
        row_break.kind,
        DisplayItemKind::RowBreak(DisplayRowBreak {
            reason: DisplayRowBreakReason::ExplicitNewline,
            ..
        })
    ));
}

struct StaticItemSource {
    items: std::vec::IntoIter<DisplayItem>,
}

impl DisplayItemSource for StaticItemSource {
    fn next_item(&mut self, _context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.items.next()
    }
}

#[test]
fn display_item_source_trait_exposes_items() {
    let expected = DisplayItem::new(
        SourceSpan::synthetic(9, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(1)),
        DisplayItemKind::TextRun(DisplayTextRun::new("x")),
    );
    let mut source = StaticItemSource {
        items: vec![expected.clone()].into_iter(),
    };
    let mut context = DisplaySourceContext::empty();

    assert_eq!(source.next_item(&mut context), Some(expected));
    assert_eq!(source.next_item(&mut context), None);
}

#[test]
fn display_source_mapped_text_remainder_preserves_face_run_alignment() {
    use neovm_core::face::LispFaceId;

    let remainder = DisplaySourceMappedText::new("abc")
        .with_lisp_face_runs(vec![
            DisplaySourceMappedFaceRun::new(1, LispFaceId::glyph_override(7)),
            DisplaySourceMappedFaceRun::new(2, LispFaceId::glyph_override(8)),
        ])
        .into_remainder_after(1)
        .expect("one-character remainder");

    assert_eq!(remainder.text.as_ref(), "bc");
    assert_eq!(
        remainder.lisp_face_runs(),
        &[DisplaySourceMappedFaceRun::new(
            2,
            LispFaceId::glyph_override(8)
        )]
    );
}

#[test]
fn glyphless_method_routes_cf_format_control_to_zero_width() {
    let m = |c: char| glyphless_method_for_char(c, GlyphlessJoinerPolicy::ClassifyAsGlyphless);
    // GNU `format-control` group (Cf, except SHY): the interlinear annotation
    // marks U+FFF9..U+FFFB were drawn as a `.notdef` box; they must render
    // invisible like ZWSP/ZWJ (glyphless-char-display-control default).
    assert_eq!(m('\u{fff9}'), Some(GlyphlessMethod::ZeroWidth)); // IAA
    assert_eq!(m('\u{fffa}'), Some(GlyphlessMethod::ZeroWidth)); // IAS
    assert_eq!(m('\u{fffb}'), Some(GlyphlessMethod::ZeroWidth)); // IAT
    assert_eq!(m('\u{2060}'), Some(GlyphlessMethod::ZeroWidth)); // WORD JOINER (Cf)
    assert_eq!(m('\u{200d}'), Some(GlyphlessMethod::ZeroWidth)); // ZWJ (fast-path)
    // SHY (U+00AD) is Cf but GNU excludes it (it has a visible glyph).
    assert_eq!(m('\u{00ad}'), None);
    // Hot-path printables stay non-glyphless.
    assert_eq!(m('a'), None);
    assert_eq!(m('中'), None);
    // Object Replacement keeps its EmptyBox handling; the non-printable
    // noncharacters (U+FDD0.., U+FFFE/U+FFFF) are octal-escaped upstream of this
    // fn, so they are intentionally not routed here.
    assert_eq!(m('\u{fffc}'), Some(GlyphlessMethod::EmptyBox));
}

#[test]
fn glyphless_method_keeps_non_ignorable_format_controls_visible() {
    let m = |c: char| glyphless_method_for_char(c, GlyphlessJoinerPolicy::ClassifyAsGlyphless);
    // `Cf` chars that are NOT `Default_Ignorable_Code_Point` must render, not
    // hide -- GNU emits them (font glyph or composed cluster). Regression: a
    // blanket "all `Cf` -> ZeroWidth" dropped U+180E from etc/HELLO's Mongolian
    // line, diverging from GNU on the TTY.
    assert_eq!(m('\u{180e}'), None); // MONGOLIAN VOWEL SEPARATOR (removed from DI in 6.3)
    assert_eq!(m('\u{0600}'), None); // ARABIC NUMBER SIGN (prepended concatenation mark)
    assert_eq!(m('\u{06dd}'), None); // ARABIC END OF AYAH
    assert_eq!(m('\u{070f}'), None); // SYRIAC ABBREVIATION MARK
    assert_eq!(m('\u{08e2}'), None); // ARABIC DISPUTED END OF AYAH
    assert_eq!(m('\u{110bd}'), None); // KAITHI NUMBER SIGN
    assert_eq!(m('\u{13430}'), None); // EGYPTIAN HIEROGLYPH VERTICAL JOINER
    // Other `Cf` chars that ARE Default_Ignorable still hide: e.g. U+061C
    // ARABIC LETTER MARK (a bidi control), so the narrowing is exact.
    assert_eq!(m('\u{061c}'), Some(GlyphlessMethod::ZeroWidth));
}

// ---- Increment 2i rung 1: replacement-source vocabulary. The pipeline
// expresses "N string glyphs standing for M covered buffer chars" as a
// `SourceMappedText` whose span is the covered buffer range and whose separate
// `glyph_string_start` retains GNU's string object/index coordinate. The
// builder therefore stamps indices 0..N while registering the exact covered
// buffer range once on the row's typed string-source occurrence. These pins
// prove the item producer surface: `item_from_replacement_string_item` is the
// seam where the replacement session preserves both coordinate systems.

#[test]
fn replacement_source_converts_string_text_run_to_covered_provenance_run() {
    // The session's string items carry LispString spans (string indices); the
    // replacement source rewrites them to ONE covered buffer span [B, E) with
    // kind SourceMappedText, preserving face, layout, and pointer appearance.
    let covered = BufferDisplayReplacementSource::spanning(
        BufferId(7),
        CharPos0::new(2),
        EmacsBytePos::new(3),
        CharPos0::new(4),
        EmacsBytePos::new(5),
    );
    let string_item = DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(11, 0, 0),
            DisplaySourcePosition::lisp_string(11, 3, 3),
        ),
        RenderFaceRef::FaceId(FaceId::new(12)),
        DisplayItemKind::TextRun(DisplayTextRun::new("STR")),
    )
    .with_layout(DisplayItemLayout {
        raise: Some(0.5),
        ..DisplayItemLayout::default()
    });

    let item = covered.item_from_replacement_string_item(string_item);

    assert_eq!(
        item.kind,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "STR",
            DisplaySourcePosition::lisp_string(11, 0, 0),
        )),
        "a replacement run keeps its string start separate from buffer coverage"
    );
    assert_eq!(
        item.span,
        SourceSpan::new(
            DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(2), EmacsBytePos::new(3)),
            DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(4), EmacsBytePos::new(5)),
        ),
        "the run's span is the COVERED buffer range, not the string's indices"
    );
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(12)));
    assert_eq!(item.layout.raise, Some(0.5));
}

#[test]
fn replacement_source_keeps_non_text_kinds_with_covered_span() {
    // Non-text items from inside a display string (e.g. a nested stretch) keep
    // their kind; only the span is rewritten to the covered range.
    let covered = BufferDisplayReplacementSource::spanning(
        BufferId(3),
        CharPos0::new(5),
        EmacsBytePos::new(6),
        CharPos0::new(6),
        EmacsBytePos::new(7),
    );
    let stretch = DisplayItem::new(
        SourceSpan::synthetic(9, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(4)),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(24.0)),
            height: None,
            ascent: None,
        }),
    );

    let item = covered.item_from_replacement_string_item(stretch);

    assert!(matches!(item.kind, DisplayItemKind::Stretch(_)));
    assert_eq!(
        item.span.buffer_end_charpos(),
        Some(CharPos0::new(6)),
        "the covered buffer span applies to every item kind the string yields"
    );
}

/// `produce_xwidget_glyph` crops the glyph's advance (`it->pixel_width -=
/// crop`, src/xdisp.c:32579, emacs-31.0.90) and nothing else: the widget's
/// own size still drives the native view, which
/// `x_draw_xwidget_glyph_string` clips (src/xwidget.c:2841-2847).
#[test]
fn cropping_an_xwidgets_advance_keeps_its_content_extent() {
    let xwidget = DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(1),
        webview_id: neomacs_display_protocol::WebViewId::new(1),
        width: 600.0,
        height: 40.0,
    });
    let content = |media: DisplayMediaReplacement| match media.kind {
        DisplayMediaReplacementKind::Xwidget { content, .. } => content,
        other => panic!("still an xwidget, got {other:?}"),
    };
    assert_eq!(xwidget.width, 600.0);
    assert_eq!(content(xwidget).width_px(), 600.0);
    assert_eq!(content(xwidget).height_px(), 40.0);

    let cropped = xwidget.xwidget_advance_cropped_to(304.0);
    assert_eq!(cropped.width, 304.0, "the layout advance is cropped");
    assert_eq!(cropped.height, 40.0);
    assert_eq!(
        content(cropped).width_px(),
        600.0,
        "the widget's own width is not"
    );

    // Widening is not cropping; a non-positive width is not either.
    assert_eq!(xwidget.xwidget_advance_cropped_to(900.0).width, 600.0);
    assert_eq!(xwidget.xwidget_advance_cropped_to(0.0).width, 600.0);
    assert_eq!(xwidget.xwidget_advance_cropped_to(f32::NAN).width, 600.0);
}
