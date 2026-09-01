use super::*;
use crate::buffer_source::consumption::{BufferSourceConsumedItem, BufferSourceConsumptionState};
use crate::buffer_source::text_source::{BufferTextCursorItem, BufferTextSourceCursor};
use crate::display_item::{
    BufferDisplayReplacementSource, DisplayGlyphless, DisplayImageItem, DisplayItem,
    DisplayItemKind, DisplayLength, DisplayMediaReplacement, DisplayRowBreakReason,
    DisplaySourceId, DisplaySourceMappedFaceRun, DisplaySourceMappedText, DisplaySourcePosition,
    DisplayStretch, DisplayStretchWidth, DisplayTextRun, GlyphlessMethod, RenderFaceRef,
    SourceSpan,
};
use crate::display_property::DisplayReplacementProperty;
use crate::display_source::DisplaySourceTextPosition;
use crate::neovm_bridge::{LayoutBufferSnapshot, LayoutBufferView};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::emacs_core::{Context, Value};

fn collect_items(source: &mut impl DisplayItemSource) -> Vec<DisplayItem> {
    let mut context = DisplaySourceContext::empty();
    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }
    items
}

fn item_texts(items: &[DisplayItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match &item.kind {
            DisplayItemKind::TextRun(run) => Some(run.text.to_string()),
            DisplayItemKind::SourceMappedText(text) => Some(text.text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn display_source_step_item_splits_text_run_at_buffer_charpos() {
    let buffer_id = BufferId(7);
    let item = DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::buffer(buffer_id, CharPos0::new(5), EmacsBytePos::new(110)),
            DisplaySourcePosition::buffer(buffer_id, CharPos0::new(8), EmacsBytePos::new(115)),
        ),
        RenderFaceRef::FaceId(FaceId::new(3)),
        DisplayItemKind::TextRun(DisplayTextRun::new("éβx")),
    );
    let source_item = DisplaySourceItem::new_for_test(item, 10, 5, Some('é'));
    let step_item = DisplaySourceStepItem::new(source_item, 100).expect("step item");

    let (prefix, suffix) = step_item
        .split_text_run_at_charpos(7, 100)
        .expect("split text run");
    let (_prefix_step, prefix_item) = prefix.into_test_render_parts().expect("prefix parts");
    let (_suffix_step, suffix_item) = suffix.into_test_render_parts().expect("suffix parts");

    let DisplayItemKind::TextRun(prefix_run) = &prefix_item.kind else {
        panic!("expected prefix text run");
    };
    let DisplayItemKind::TextRun(suffix_run) = &suffix_item.kind else {
        panic!("expected suffix text run");
    };
    assert_eq!(&*prefix_run.text, "éβ");
    assert_eq!(&*suffix_run.text, "x");
    assert_eq!(
        prefix_item.span.end,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(7), EmacsBytePos::new(114))
    );
    assert_eq!(
        suffix_item.span.start,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(7), EmacsBytePos::new(114))
    );
}

fn snapshot_with_text(text: &str) -> (BufferId, LayoutBufferSnapshot, CharPos0) {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert(text);
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    (buffer_id, snapshot, end)
}

fn expected_source_coords(text: &str) -> Vec<(char, usize, i64)> {
    let mut byte_offset = 0usize;
    let mut charpos = 0i64;
    text.chars()
        .map(|ch| {
            let source = (ch, byte_offset, charpos);
            byte_offset += ch.len_utf8();
            charpos += 1;
            source
        })
        .collect()
}

#[test]
fn text_source_char_classification_matches_display_items() {
    assert_eq!(
        classify_text_source_char('\n'),
        TextSourceCharClassification::RowBreak
    );
    assert_eq!(
        classify_text_source_char('\u{7f}'),
        TextSourceCharClassification::ControlChar { ch: '\u{7f}' }
    );
    assert_eq!(
        classify_text_source_char('\u{feff}'),
        TextSourceCharClassification::Glyphless {
            ch: '\u{feff}',
            method: GlyphlessMethod::ZeroWidth,
        }
    );
    assert_eq!(
        classify_text_source_char('\t'),
        TextSourceCharClassification::Text
    );
    assert_eq!(
        classify_text_source_char('x'),
        TextSourceCharClassification::Text
    );
}

#[test]
fn buffer_display_replacement_source_builds_items_without_appending() {
    let source =
        BufferDisplayReplacementSource::new(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12));

    let stretch_item = source.display_item(
        FaceId::new(42),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(16.0)),
            height: Some(DisplayLength::Pixels(9.0)),
            ascent: Some(DisplayLength::Pixels(7.0)),
        }),
    );
    assert_eq!(stretch_item.face, RenderFaceRef::FaceId(FaceId::new(42)));
    assert!(matches!(
        stretch_item.kind,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(16.0)),
            height: Some(DisplayLength::Pixels(9.0)),
            ascent: Some(DisplayLength::Pixels(7.0)),
        })
    ));

    let text_item = source.display_item(
        FaceId::new(43),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("fallback")),
    );
    assert_eq!(text_item.face, RenderFaceRef::FaceId(FaceId::new(43)));
    assert!(matches!(
        text_item.kind,
        DisplayItemKind::SourceMappedText(text) if text.text.as_ref() == "fallback"
    ));
}

#[test]
fn buffer_display_replacement_source_can_span_covered_buffer_text() {
    let source = BufferDisplayReplacementSource::spanning(
        BufferId(7),
        CharPos0::new(3),
        EmacsBytePos::new(12),
        CharPos0::new(5),
        EmacsBytePos::new(18),
    );

    let text_item = source.display_item(
        FaceId::new(43),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("fallback")),
    );

    assert_eq!(
        text_item.span.start,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12))
    );
    assert_eq!(
        text_item.span.end,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(5), EmacsBytePos::new(18))
    );
}

#[test]
fn buffer_text_item_source_single_char_maps_one_buffer_character() {
    let source = BufferTextItemSource::single_char(
        BufferId(7),
        CharPos0::new(3),
        EmacsBytePos::new(12),
        EmacsBytePos::new(16),
    );

    let item = source.item(
        RenderFaceRef::FaceId(FaceId::new(42)),
        DisplayItemKind::TextRun(DisplayTextRun::new("x")),
    );

    assert_eq!(
        item.span.start,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12))
    );
    assert_eq!(
        item.span.end,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(4), EmacsBytePos::new(16))
    );
}

#[test]
fn buffer_display_replacement_string_source_maps_text_to_buffer_slot() {
    let _eval = Context::new();
    let replacement_source =
        BufferDisplayReplacementSource::new(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12));
    let string_source = LispStringSourceCursor::new(
        1,
        Value::string("fallback"),
        RenderFaceRef::FaceId(FaceId::new(42)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut source =
        BufferDisplayReplacementStringSource::new(replacement_source, string_source, None);
    let mut context = DisplaySourceContext::empty();

    let item = source.next_item(&mut context).expect("replacement item");

    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(42)));
    assert_eq!(
        item.span.start,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12))
    );
    assert_eq!(
        item.span.end,
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(4), EmacsBytePos::new(12))
    );
    assert!(matches!(
        item.kind,
        DisplayItemKind::SourceMappedText(text) if text.text.as_ref() == "fallback"
    ));
    assert!(source.next_item(&mut context).is_none());
}

#[test]
fn buffer_display_replacement_string_inherits_buffer_mouse_face() {
    let _eval = Context::new();
    let replacement_source =
        BufferDisplayReplacementSource::new(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12));
    let pointer = crate::display_item::DisplayPointerAppearance::new(
        crate::display_item::DisplayPointerSourceRange::ending_at(
            DisplaySourcePosition::buffer(BufferId(7), CharPos0::ZERO, EmacsBytePos::new(0)),
            4,
        ),
        RenderFaceRef::FaceId(FaceId::new(11)),
    );
    let mut source = BufferDisplayReplacementStringRequest::new(
        1,
        Value::string("replacement"),
        replacement_source,
    )
    .with_pointer_appearance(Some(pointer))
    .into_source(FaceId::new(42))
    .unwrap();

    let item = source
        .next_item(&mut DisplaySourceContext::empty())
        .unwrap();

    assert_eq!(
        item.pointer_appearance().map(|pointer| pointer.face()),
        Some(RenderFaceRef::FaceId(FaceId::new(11)))
    );
    assert!(matches!(
        item.span.start,
        DisplaySourcePosition::Buffer { .. }
    ));
}

#[test]
fn buffer_display_replacement_string_source_ignores_display_properties_inside_replacement_string() {
    let _eval = Context::new();
    let replacement_source =
        BufferDisplayReplacementSource::new(BufferId(7), CharPos0::new(3), EmacsBytePos::new(12));
    let value = Value::string_with_text_properties(
        "Y",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("display"), Value::string("Z")]),
        }],
    );
    let mut source = BufferDisplayReplacementStringRequest::new(1, value, replacement_source)
        .into_source(FaceId::new(42))
        .expect("replacement string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["Y"]);
}

#[test]
fn lisp_string_source_cursor_emits_text_runs_with_source_spans() {
    let _eval = Context::new();
    let value = Value::string("abc");
    let mut source = LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["abc"]);
    assert_eq!(items[0].face, RenderFaceRef::FaceId(FaceId::new(3)));
    assert_eq!(
        items[0].span.start,
        DisplaySourcePosition::lisp_string(1, 0, 0)
    );
    assert_eq!(
        items[0].span.end,
        DisplaySourcePosition::lisp_string(1, 3, 3)
    );
}

struct BoxedFaceResolver;

impl DisplayItemFaceResolver for BoxedFaceResolver {
    fn resolve_face_ref(&mut self, base: RenderFaceRef, _face_value: Value) -> RenderFaceRef {
        base
    }

    fn resolve_face_sources(
        &mut self,
        base: RenderFaceRef,
        _sources: &OrderedFaceSources,
    ) -> RenderFaceRef {
        base
    }

    fn resolve_lisp_face_ref(
        &mut self,
        _base: RenderFaceRef,
        lisp_face_id: neovm_core::face::LispFaceId,
    ) -> RenderFaceRef {
        RenderFaceRef::FaceId(FaceId::new(
            u32::try_from(lisp_face_id.get()).expect("test face id fits protocol domain"),
        ))
    }

    fn face_has_box(&self, face: RenderFaceRef) -> bool {
        face == RenderFaceRef::FaceId(FaceId::new(3))
    }
}

#[test]
fn standalone_boxed_string_publishes_source_terminals_on_its_items() {
    use neomacs_display_protocol::face::BoxVerticalEdges;

    let _eval = Context::new();
    let mut source = LispStringSourceCursor::new(
        1,
        Value::string("x\n"),
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut resolver = BoxedFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].box_vertical_edges, BoxVerticalEdges::Left);
    assert!(matches!(items[1].kind, DisplayItemKind::RowBreak(_)));
    assert_eq!(items[1].box_vertical_edges, BoxVerticalEdges::Right);
}

#[test]
fn nested_boxed_string_uses_distinct_underlying_faces_at_each_boundary() {
    use neomacs_display_protocol::face::BoxVerticalEdges;

    let _eval = Context::new();
    let boxed = RenderFaceRef::FaceId(FaceId::new(3));
    let mut frame = LispStringSourceFrame::new_with_occurrence(
        9,
        Value::string("x\n"),
        boxed,
        None,
        NestedDisplayPolicy::ModifiersOnly,
        DisplayPointerOccurrence::Source,
        DisplayStringBoxBoundaries::known(true, false),
    )
    .expect("nested string frame");
    let mut resolver = BoxedFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let LispStringAction::Emit(text) = frame.next_action(&mut context) else {
        panic!("text item");
    };
    let LispStringAction::Emit(newline) = frame.next_action(&mut context) else {
        panic!("newline item");
    };

    assert_eq!(text.box_vertical_edges, BoxVerticalEdges::Neither);
    assert!(matches!(newline.kind, DisplayItemKind::RowBreak(_)));
    assert_eq!(newline.box_vertical_edges, BoxVerticalEdges::Right);
}

struct SymbolFaceResolver;

impl DisplayItemFaceResolver for SymbolFaceResolver {
    fn face_has_box(&self, _face: RenderFaceRef) -> bool {
        false
    }

    fn resolve_face_ref(&mut self, base: RenderFaceRef, face_value: Value) -> RenderFaceRef {
        match face_value.as_symbol_name() {
            Some("bold") => RenderFaceRef::FaceId(FaceId::new(7)),
            Some("font-lock-string-face") => RenderFaceRef::FaceId(FaceId::new(9)),
            _ => base,
        }
    }

    fn resolve_face_sources(
        &mut self,
        mut base: RenderFaceRef,
        sources: &OrderedFaceSources,
    ) -> RenderFaceRef {
        for value in sources.values() {
            base = self.resolve_face_ref(base, value);
        }
        base
    }

    fn resolve_lisp_face_ref(
        &mut self,
        _base: RenderFaceRef,
        lisp_face_id: neovm_core::face::LispFaceId,
    ) -> RenderFaceRef {
        RenderFaceRef::FaceId(FaceId::new(
            lisp_face_id
                .get()
                .try_into()
                .expect("test face id fits u32"),
        ))
    }

    fn resolve_pointer_face_ref(
        &mut self,
        base: RenderFaceRef,
        face_value: Value,
    ) -> Option<RenderFaceRef> {
        match face_value.as_symbol_name() {
            Some("highlight") => Some(RenderFaceRef::FaceId(FaceId::new(11))),
            Some("low-mouse") => Some(RenderFaceRef::FaceId(FaceId::new(12))),
            Some("high-mouse") => Some(RenderFaceRef::FaceId(FaceId::new(13))),
            _ => self
                .resolve_face_ref(base, face_value)
                .ne(&base)
                .then(|| self.resolve_face_ref(base, face_value)),
        }
    }
}

#[test]
fn display_item_segment_source_keeps_multibyte_face_runs_aligned() {
    let base = RenderFaceRef::FaceId(FaceId::new(3));
    let mapped = DisplaySourceMappedText::from_string_run(
        "é中x",
        DisplaySourcePosition::lisp_string(4, 10, 20),
    )
    .with_lisp_face_runs(vec![
        DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(7)),
        DisplaySourceMappedFaceRun::new(1, None),
        DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(9)),
    ]);
    let item = DisplayItem::new(
        SourceSpan::synthetic(5, 0, 1),
        base,
        DisplayItemKind::SourceMappedText(mapped),
    );
    let mut source = DisplayItemSegmentSource::new(item);
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let mut segments = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        segments.push(item);
    }

    assert_eq!(item_texts(&segments), ["é", "中", "x"]);
    assert_eq!(
        segments.iter().map(|item| item.face).collect::<Vec<_>>(),
        [
            RenderFaceRef::FaceId(FaceId::new(7)),
            base,
            RenderFaceRef::FaceId(FaceId::new(9)),
        ]
    );
    assert_eq!(
        segments
            .iter()
            .map(|item| match &item.kind {
                DisplayItemKind::SourceMappedText(mapped) => {
                    mapped.glyph_string_start.clone().expect("string origin")
                }
                other => panic!("unexpected segment kind: {other:?}"),
            })
            .collect::<Vec<_>>(),
        [
            DisplaySourcePosition::lisp_string(4, 10, 20),
            DisplaySourcePosition::lisp_string(4, 11, 22),
            DisplaySourcePosition::lisp_string(4, 12, 25),
        ]
    );
    assert!(
        segments
            .iter()
            .all(|item| item.span == SourceSpan::synthetic(5, 0, 1))
    );
}

#[test]
fn display_table_face_run_terminals_compare_with_the_replaced_source_face() {
    use neomacs_display_protocol::face::BoxVerticalEdges;

    let mapped = |base, mapped_face| {
        DisplayItem::new(
            SourceSpan::synthetic(5, 0, 1),
            base,
            DisplayItemKind::SourceMappedText(
                DisplaySourceMappedText::from_string_run(
                    "x",
                    DisplaySourcePosition::lisp_string(4, 10, 20),
                )
                .with_lisp_face_runs(vec![DisplaySourceMappedFaceRun::new(
                    1,
                    neovm_core::face::LispFaceId::new(mapped_face),
                )]),
            ),
        )
    };
    let boxed = RenderFaceRef::FaceId(FaceId::new(3));
    let unboxed = RenderFaceRef::FaceId(FaceId::new(4));
    let mut resolver = BoxedFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let output_box = DisplayItemSegmentSource::new(mapped(unboxed, 3))
        .next_item(&mut context)
        .expect("mapped boxed run");
    assert_eq!(output_box.box_vertical_edges, BoxVerticalEdges::Both);

    let output_plain = DisplayItemSegmentSource::new(mapped(boxed, 4))
        .next_item(&mut context)
        .expect("mapped unboxed run");
    assert_eq!(output_plain.box_vertical_edges, BoxVerticalEdges::Neither);
}

struct ResolvedDisplayPropertyResolver {
    seen_face: Option<RenderFaceRef>,
}

impl DisplayItemFaceResolver for ResolvedDisplayPropertyResolver {
    fn face_has_box(&self, _face: RenderFaceRef) -> bool {
        false
    }

    fn resolve_face_ref(&mut self, base: RenderFaceRef, face_value: Value) -> RenderFaceRef {
        match face_value.as_symbol_name() {
            Some("bold") => RenderFaceRef::FaceId(FaceId::new(7)),
            _ => base,
        }
    }

    fn resolve_face_sources(
        &mut self,
        mut base: RenderFaceRef,
        sources: &OrderedFaceSources,
    ) -> RenderFaceRef {
        for value in sources.values() {
            base = self.resolve_face_ref(base, value);
        }
        base
    }

    fn resolve_display_media_replacement(
        &mut self,
        display_prop: Value,
        _image_slice: Option<crate::display_spec::DisplayImageSliceSpec>,
        face: RenderFaceRef,
    ) -> Option<DisplayMediaReplacement> {
        self.seen_face = Some(face);
        if display_prop.cons_car().is_symbol_named("image") {
            Some(DisplayMediaReplacement::image(DisplayImageItem {
                image_id: 42,
                source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
                width: 64.0,
                height: 32.0,
                ascent: 32.0,
                horizontal_margin: 0.0,
                vertical_margin: 0.0,
                opaque_background: None,
            }))
        } else {
            None
        }
    }
}

#[test]
fn display_property_source_action_classifies_strings_typed_items_and_resolver_fallback() {
    let _eval = Context::new();
    let base_face = RenderFaceRef::FaceId(FaceId::new(7));
    let mut resolver = ResolvedDisplayPropertyResolver { seen_face: None };

    {
        let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

        let display_string = DisplayPropertySourcePlan::new(Value::string("displayed"));
        match display_string.source_action(
            &mut context,
            DisplayPropertySourceFaces::Buffer {
                effective: base_face,
            },
        ) {
            DisplayPropertySourceAction::PushReplacement { value, base_face } => {
                assert_eq!(
                    value.as_runtime_string_owned().as_deref(),
                    Some("displayed")
                );
                assert_eq!(base_face, RenderFaceRef::FaceId(FaceId::new(7)));
            }
            action => panic!("expected replacement string action, got {action:?}"),
        }

        let space_spec = Value::list(vec![
            Value::symbol("space"),
            Value::keyword(":width"),
            Value::fixnum(2),
        ]);
        let space_plan = DisplayPropertySourcePlan::new(space_spec);
        match space_plan.source_action(
            &mut context,
            DisplayPropertySourceFaces::Buffer {
                effective: base_face,
            },
        ) {
            DisplayPropertySourceAction::Emit {
                kind:
                    DisplayItemKind::Stretch(DisplayStretch {
                        width: DisplayStretchWidth::Length(DisplayLength::Em(2.0)),
                        height: None,
                        ascent: None,
                    }),
                layout,
            } => assert_eq!(layout, DisplayItemLayout::default()),
            action => panic!("expected typed space action, got {action:?}"),
        }

        let image_plan = DisplayPropertySourcePlan::new(Value::list(vec![Value::symbol("image")]));
        match image_plan.source_action(
            &mut context,
            DisplayPropertySourceFaces::Buffer {
                effective: base_face,
            },
        ) {
            DisplayPropertySourceAction::Emit {
                kind:
                    DisplayItemKind::MediaReplacement(DisplayMediaReplacement {
                        width: 64.0,
                        height: 32.0,
                        ..
                    }),
                layout,
            } => assert_eq!(layout, DisplayItemLayout::default()),
            action => panic!("expected resolved image action, got {action:?}"),
        }
    }

    assert_eq!(resolver.seen_face, Some(base_face));
}

#[test]
fn display_property_string_base_face_is_explicit_for_buffer_and_string_sources() {
    let _eval = Context::new();
    let effective = RenderFaceRef::FaceId(FaceId::new(7));
    let underlying = RenderFaceRef::FaceId(FaceId::new(3));
    let plan = DisplayPropertySourcePlan::new(Value::string("displayed"));
    let mut context = DisplaySourceContext::empty();

    let buffer_action = plan.source_action(
        &mut context,
        DisplayPropertySourceFaces::Buffer { effective },
    );
    assert!(matches!(
        buffer_action,
        DisplayPropertySourceAction::PushReplacement {
            base_face,
            ..
        } if base_face == effective
    ));

    let string_action = plan.source_action(
        &mut context,
        DisplayPropertySourceFaces::LispString {
            effective,
            underlying,
        },
    );
    assert!(matches!(
        string_action,
        DisplayPropertySourceAction::PushReplacement {
            base_face,
            ..
        } if base_face == underlying
    ));
}

#[test]
fn display_property_source_replacement_resolves_direct_media_item() {
    let _eval = Context::new();
    let media = DisplayMediaReplacement::xwidget(crate::display_item::DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(21),
        webview_id: neomacs_display_protocol::WebViewId::new(210),
        width: 30.0,
        height: 12.0,
    });
    let mut context = DisplaySourceContext::empty();
    let classification = crate::display_property::DisplayPropertyClassification::new_for_test(
        Some(crate::display_property::DisplayReplacementProperty::Media(
            crate::display_property::DisplayMediaReplacementProperty::Xwidget(media),
        )),
        Value::NIL,
        Default::default(),
    );

    let replacement = DisplayPropertySourceReplacement::resolve(
        &mut context,
        Value::NIL,
        &classification,
        RenderFaceRef::FaceId(FaceId::new(7)),
    );

    let DisplayPropertySourceReplacement::Item(DisplayItemKind::MediaReplacement(resolved)) =
        replacement
    else {
        panic!("expected direct media replacement item");
    };
    assert_eq!(resolved, media);
}

#[test]
fn display_property_source_action_builds_cursor_actions() {
    let span = SourceSpan::synthetic(3, 0, 1);
    let face = RenderFaceRef::FaceId(FaceId::new(7));
    let layout = DisplayItemLayout {
        raise: Some(0.25),
        height: None,
        space_width: None,
        break_after_row: false,
    };

    let emit = DisplayPropertySourceAction::Emit {
        kind: DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("x")),
        layout,
    }
    .into_cursor_action(span.clone(), face);

    let DisplayPropertySourceCursorAction::Emit(item) = emit else {
        panic!("expected emitted cursor item");
    };
    assert_eq!(item.span, span);
    assert_eq!(item.face, face);
    assert_eq!(item.layout, layout);

    let fallthrough = DisplayPropertySourceAction::Ignore { layout }.into_cursor_action(span, face);
    assert_eq!(
        fallthrough,
        DisplayPropertySourceCursorAction::FallThrough { layout }
    );
}

#[test]
fn lisp_string_source_cursor_resolves_face_property() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "abc",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        2,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }

    assert_eq!(item_texts(&items), ["a", "b", "c"]);
    assert_eq!(items[0].face, RenderFaceRef::FaceId(FaceId::new(3)));
    assert_eq!(items[1].face, RenderFaceRef::FaceId(FaceId::new(7)));
    assert_eq!(items[2].face, RenderFaceRef::FaceId(FaceId::new(3)));
}

#[test]
fn lisp_string_source_cursor_renders_explicit_composition_property_replacement() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "* H",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("composition"),
                Value::list(vec![
                    Value::fixnum(0),
                    Value::fixnum(1),
                    Value::vector(vec![Value::fixnum('◉' as i64)]),
                ]),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        5,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["◉", " H"]);
}

#[test]
fn lisp_string_source_cursor_resolves_display_property_through_context() {
    let _eval = Context::new();
    let display_spec = Value::list(vec![Value::symbol("image")]);
    let value = Value::string_with_text_properties(
        "x",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::symbol("bold"),
                Value::symbol("display"),
                display_spec,
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        4,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut resolver = ResolvedDisplayPropertyResolver { seen_face: None };

    let item = {
        let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
        let item = source.next_item(&mut context).expect("display item");
        assert!(source.next_item(&mut context).is_none());
        item
    };

    assert_eq!(
        resolver.seen_face,
        Some(RenderFaceRef::FaceId(FaceId::new(7)))
    );
    assert!(matches!(
        item.kind,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement {
            width: 64.0,
            height: 32.0,
            ..
        })
    ));
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(7)));
}

#[test]
fn lisp_string_source_cursor_uses_font_lock_face_when_face_is_absent() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "xy",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("font-lock-face"),
                Value::symbol("font-lock-string-face"),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        3,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let first = source.next_item(&mut context).expect("first item");
    let second = source.next_item(&mut context).expect("second item");

    assert_eq!(
        first.kind,
        DisplayItemKind::TextRun(DisplayTextRun::new("x"))
    );
    assert_eq!(first.face, RenderFaceRef::FaceId(FaceId::new(9)));
    assert_eq!(
        second.kind,
        DisplayItemKind::TextRun(DisplayTextRun::new("y"))
    );
    assert_eq!(second.face, RenderFaceRef::FaceId(FaceId::new(3)));
}

#[test]
fn lisp_string_source_cursor_parses_display_space_width_as_stretch() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "a b",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword(":width"),
                    Value::fixnum(3),
                ]),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        4,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "b"]);
    assert_eq!(
        items[1].kind,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Em(3.0)),
            height: None,
            ascent: None,
        })
    );
}

#[test]
fn lisp_string_source_cursor_parses_display_space_align_to_as_typed_expression() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        " ",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword(":align-to"),
                    Value::list(vec![
                        Value::symbol("-"),
                        Value::symbol("right"),
                        Value::fixnum(2),
                    ]),
                ]),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        5,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let item = source
        .next_item(&mut DisplaySourceContext::empty())
        .expect("item");

    assert_eq!(
        item.kind,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::AlignTo(Value::list(vec![
                Value::symbol("-"),
                Value::symbol("right"),
                Value::fixnum(2),
            ])),
            height: None,
            ascent: None,
        })
    );
}

#[test]
fn lisp_string_source_cursor_emits_explicit_newline_row_breaks() {
    let _eval = Context::new();
    let value = Value::string("a\nb");
    let mut source = LispStringSourceCursor::new(
        6,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "b"]);
    assert!(matches!(items[1].kind, DisplayItemKind::RowBreak(_)));
}

#[test]
fn lisp_string_source_cursor_emits_control_and_glyphless_items() {
    let _eval = Context::new();
    // U+FFFC (OBJECT REPLACEMENT, So) is printable-but-glyphless so it stays a
    // glyphless item; a non-printable char like U+FFF0 instead escapes to
    // `\`+octal (see the escape-glyph-octal tests).
    let value = Value::string("a\u{0001}\u{fffc}b");
    let mut source = LispStringSourceCursor::new(
        7,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "b"]);
    assert_eq!(
        items[1].kind,
        DisplayItemKind::ControlChar { ch: '\u{0001}' }
    );
    assert_eq!(
        items[2].kind,
        DisplayItemKind::Glyphless(DisplayGlyphless {
            ch: '\u{fffc}',
            method: GlyphlessMethod::EmptyBox,
        })
    );
}

/// Non-printable chars -- the C1 controls (U+0080..U+009F), the unassigned
/// specials (U+FFF0..U+FFF8), and the noncharacters (U+FFFE/U+FFFF, U+FDD0..) --
/// classify as GNU's escape-glyph `\`+octal escape, NOT a glyphless hex-code
/// box (checked BEFORE the glyphless methods, GNU `!CHAR_PRINTABLE_P`).
#[test]
fn classify_non_printable_c1_and_specials_as_escape_octal() {
    for (ch, octal) in [
        ('\u{0080}', "\\200"),
        ('\u{009f}', "\\237"),
        ('\u{fff0}', "\\177760"),
        ('\u{ffff}', "\\177777"),
        ('\u{fdd0}', "\\176720"),
    ] {
        assert_eq!(
            classify_text_source_char(ch),
            TextSourceCharClassification::EscapeOctal { ch },
            "{ch:?} must classify as escape-octal"
        );
        assert_eq!(escape_glyph_octal_text(ch), octal, "octal text for {ch:?}");
    }
    // A printable-but-glyphless char (U+FFFC, So) is NOT escaped -- it stays a
    // glyphless item so the two behaviors don't collide.
    assert!(matches!(
        classify_text_source_char('\u{fffc}'),
        TextSourceCharClassification::Glyphless { .. }
    ));
}

#[test]
fn lisp_string_source_cursor_pushes_display_string_replacement_source() {
    let _eval = Context::new();
    let replacement = Value::string("YZ");
    let value = Value::string_with_text_properties(
        "axb",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![Value::symbol("display"), replacement]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        7,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "YZ", "b"]);
    let DisplaySourcePosition::LispString { source_id, .. } = items[1].span.start else {
        panic!("replacement text should come from a Lisp string source");
    };
    assert_ne!(
        source_id,
        DisplaySourceId::new(7),
        "replacement string should be emitted from a nested source frame, not flattened into the parent span"
    );
}

#[test]
fn lisp_string_source_cursor_ignores_display_properties_inside_display_string_replacement() {
    let _eval = Context::new();
    let replacement = Value::string_with_text_properties(
        "Y",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("display"), Value::string("Z")]),
        }],
    );
    let value = Value::string_with_text_properties(
        "x",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("display"), replacement]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        7,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["Y"]);
}

#[test]
fn display_sources_parse_xwidget_display_specs_as_typed_items() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let xwidget = Value::make_xwidget(
        Value::symbol("webkit"),
        Value::string("Title"),
        Value::make_buffer(buffer_id),
        96,
        54,
        1234,
        neomacs_display_protocol::WebViewId::new(5678),
    );
    let display_spec = Value::list(vec![
        Value::symbol("xwidget"),
        Value::keyword("xwidget"),
        xwidget,
    ]);

    let mut lisp_source = LispStringSourceCursor::new(
        8,
        Value::string_with_text_properties(
            "x",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("display"), display_spec]),
            }],
        ),
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let lisp_items = collect_items(&mut lisp_source);

    assert!(matches!(
        lisp_items[0].kind,
        DisplayItemKind::MediaReplacement(DisplayMediaReplacement {
            width: 96.0,
            height: 54.0,
            ..
        })
    ));

    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("x");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            display_spec,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut buffer_source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let buffer_items = collect_items(&mut buffer_source);

    assert_eq!(buffer_items[0].kind, lisp_items[0].kind);
}

#[test]
fn buffer_text_source_cursor_emits_text_runs_with_buffer_spans() {
    let (buffer_id, snapshot, end) = snapshot_with_text("ab中");
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["ab中"]);
    assert_eq!(items[0].face, RenderFaceRef::FaceId(FaceId::new(3)));
    assert_eq!(
        items[0].span.start,
        DisplaySourcePosition::buffer(
            buffer_id,
            CharPos0::new(0),
            neovm_core::buffer::EmacsBytePos::new(0)
        )
    );
    assert_eq!(
        items[0].span.end,
        DisplaySourcePosition::buffer(
            buffer_id,
            CharPos0::new(3),
            snapshot.layout_char_pos_to_emacs_byte_pos(CharPos0::new(3))
        )
    );
}

/// Build a snapshot whose first buffer char carries an `indent-bars-display`
/// property = `(space :width 2)`, optionally with `char-property-alias-alist`
/// mapping `display` → `indent-bars-display`. Uses the obarray-capturing
/// snapshot so the alias-alist default is visible to the walk. Every Lisp value
/// is built inside the live `Context` so they share its heap.
fn snapshot_with_aliased_indent_bars_display(
    configure_alias: bool,
) -> (BufferId, LayoutBufferSnapshot, CharPos0) {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    if configure_alias {
        let alias_alist = Value::list(vec![Value::list(vec![
            Value::symbol("display"),
            Value::symbol("indent-bars-display"),
        ])]);
        eval.obarray_mut()
            .set_symbol_value("char-property-alias-alist", alias_alist);
    }
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("ab");
        let display_spec = Value::list(vec![
            Value::symbol("space"),
            Value::keyword(":width"),
            Value::fixnum(2),
        ]);
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("indent-bars-display"),
            display_spec,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.obarray());
    (buffer_id, snapshot, end)
}

#[test]
fn buffer_display_property_resolves_through_char_property_alias_alist() {
    // indent-bars' character mode records its bar glyphs under an ALIAS name
    // (`indent-bars-display`) declared in `char-property-alias-alist`. GNU's
    // `lookup_char_property` resolves the alias, so the display replacement must
    // apply exactly as if the property were the literal `display`.
    let (buffer_id, snapshot, end) = snapshot_with_aliased_indent_bars_display(true);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert!(
        matches!(
            items[0].kind,
            DisplayItemKind::Stretch(DisplayStretch {
                width: DisplayStretchWidth::Length(DisplayLength::Em(2.0)),
                ..
            })
        ),
        "aliased display property must produce the space stretch, got {:?}",
        items[0].kind
    );
    assert_eq!(item_texts(&items), ["b"]);
}

#[test]
fn buffer_display_property_ignores_aliased_key_without_alias_alist() {
    // The SAME `indent-bars-display` property with NO alias configured must be
    // inert: the covered char renders as plain buffer text, never a display
    // replacement. (The property boundary may still split the plain run.)
    let (buffer_id, snapshot, end) = snapshot_with_aliased_indent_bars_display(false);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items).concat(), "ab");
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.kind, DisplayItemKind::Stretch(_))),
        "unaliased key must not produce a display replacement, got {items:?}"
    );
}

#[test]
fn buffer_mouse_face_is_resolved_over_the_effective_display_face() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("abc");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert!(items[0].pointer_appearance().is_none());
    let pointer = items[1]
        .pointer_appearance()
        .expect("mouse-face run must retain its alternate paint");
    assert_eq!(items[1].face, RenderFaceRef::FaceId(FaceId::new(7)));
    assert_eq!(pointer.face(), RenderFaceRef::FaceId(FaceId::new(11)));
    assert_eq!(pointer.source().buffer_id(), Some(buffer_id));
    assert_eq!(pointer.source().end_char_index(), 2);
    assert!(items[2].pointer_appearance().is_none());
}

#[test]
fn typed_display_replacement_keeps_underlying_buffer_mouse_face() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("x");
        let range = EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(1));
        buffer.text_props_put_property_in_emacs_byte_range(
            range,
            Value::symbol("display"),
            Value::list(vec![Value::symbol("image")]),
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            range,
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source
        .next_cursor_item(
            &mut context,
            crate::buffer_source::text_source::BufferTextDisplayReplacementMode::TypedReplacementItem,
        )
        .unwrap();
    let BufferTextCursorItem::DisplayPropertyReplacement(replacement) = item else {
        panic!("expected typed replacement");
    };

    assert_eq!(
        replacement
            .descriptor()
            .pointer_appearance()
            .map(|pointer| pointer.face()),
        Some(RenderFaceRef::FaceId(FaceId::new(11)))
    );
}

#[test]
fn overlay_mouse_face_uses_the_highest_priority_effective_property() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("x");
        for (serial, priority, face) in [(1, 1, "low-mouse"), (2, 20, "high-mouse")] {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial,
                plist: Value::NIL,
                buffer: Some(buffer_id),
                start: 0,
                end: 1,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("priority"), Value::fixnum(priority))
                .unwrap();
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("mouse-face"), Value::symbol(face))
                .unwrap();
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source.next_item(&mut context).unwrap();

    assert_eq!(
        item.pointer_appearance().map(|pointer| pointer.face()),
        Some(RenderFaceRef::FaceId(FaceId::new(13)))
    );
}

#[test]
fn overlay_mouse_face_resolves_through_its_category() {
    let mut eval = Context::new();
    eval.eval_str("(put 'mouse-face-category 'mouse-face 'highlight)")
        .expect("define category mouse-face");
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("xy");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 1,
            plist: Value::list(vec![
                Value::symbol("category"),
                Value::symbol("mouse-face-category"),
            ]),
            buffer: Some(buffer_id),
            start: 0,
            end: 2,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.obarray());
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source.next_item(&mut context).expect("display item");

    assert_eq!(
        item.pointer_appearance().map(|pointer| pointer.face()),
        Some(RenderFaceRef::FaceId(FaceId::new(11)))
    );
}

#[test]
fn text_mouse_face_category_is_confined_to_its_effective_property_run() {
    let mut eval = Context::new();
    eval.eval_str("(put 'text-mouse-face-category 'mouse-face 'highlight)")
        .expect("define category mouse-face");
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(3)),
            Value::symbol("category"),
            Value::symbol("text-mouse-face-category"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.obarray());
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert_eq!(items.len(), 3);
    assert_eq!(
        items
            .iter()
            .map(|item| item.pointer_appearance().is_some())
            .collect::<Vec<_>>(),
        vec![false, true, false]
    );
}

#[test]
fn text_mouse_face_alias_is_confined_to_its_effective_property_run() {
    let mut eval = Context::new();
    let alias_alist = Value::list(vec![Value::list(vec![
        Value::symbol("mouse-face"),
        Value::symbol("alternate-mouse-face"),
    ])]);
    eval.obarray_mut()
        .set_symbol_value("char-property-alias-alist", alias_alist);
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(3)),
            Value::symbol("alternate-mouse-face"),
            Value::symbol("highlight"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.obarray());
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert_eq!(items.len(), 3);
    assert_eq!(
        items
            .iter()
            .map(|item| item.pointer_appearance().is_some())
            .collect::<Vec<_>>(),
        vec![false, true, false]
    );
}

#[test]
fn overlay_mouse_face_interrupts_and_then_reveals_text_mouse_face() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(4)),
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 1,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("high-mouse"),
            ]),
            buffer: Some(buffer_id),
            start: 1,
            end: 3,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert_eq!(items.len(), 3);
    assert_eq!(
        items
            .iter()
            .map(|item| item.pointer_appearance().map(|pointer| pointer.face()))
            .collect::<Vec<_>>(),
        vec![
            Some(RenderFaceRef::FaceId(FaceId::new(11))),
            Some(RenderFaceRef::FaceId(FaceId::new(13))),
            Some(RenderFaceRef::FaceId(FaceId::new(11))),
        ]
    );
    assert_eq!(
        items
            .iter()
            .map(|item| {
                let source = item.pointer_appearance().unwrap().source();
                (source.start_char_index(), source.end_char_index())
            })
            .collect::<Vec<_>>(),
        vec![(0, 1), (1, 3), (3, 4)]
    );
}

#[test]
fn text_mouse_face_started_before_the_cursor_keeps_its_maximal_source_range() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(4)),
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::new(2),
        CharPos0::new(3),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source.next_item(&mut context).expect("display item");
    let pointer_source = item.pointer_appearance().expect("mouse-face").source();

    assert_eq!(
        (
            pointer_source.start_char_index(),
            pointer_source.end_char_index()
        ),
        (0, 4)
    );
}

#[test]
fn overlay_mouse_face_started_before_the_cursor_keeps_its_maximal_source_range() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 1,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("highlight"),
            ]),
            buffer: Some(buffer_id),
            start: 0,
            end: 4,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::new(2),
        CharPos0::new(3),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source.next_item(&mut context).expect("display item");
    let pointer_source = item.pointer_appearance().expect("mouse-face").source();

    assert_eq!(
        (
            pointer_source.start_char_index(),
            pointer_source.end_char_index()
        ),
        (0, 4)
    );
}

#[test]
fn long_overlay_mouse_face_skips_unrelated_offscreen_overlay_endpoints() {
    use crate::buffer_source::mouse_face::{
        overlay_mouse_face_property_query_count, reset_overlay_mouse_face_property_query_count,
    };

    const BUFFER_LEN: usize = 8_192;
    const CURSOR: usize = BUFFER_LEN / 2;

    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert(&"x".repeat(BUFFER_LEN));
        let mouse_face = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 1,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("highlight"),
            ]),
            buffer: Some(buffer_id),
            start: 0,
            end: BUFFER_LEN,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(mouse_face);
        for index in 0..2_000 {
            let start = index * 2;
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial: index as u64 + 2,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
                buffer: Some(buffer_id),
                start,
                end: start + 1,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let start = CharPos0::new(CURSOR);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        start,
        CharPos0::new(CURSOR + 1),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    reset_overlay_mouse_face_property_query_count();
    let item = source.next_item(&mut context).expect("display item");

    assert!(item.pointer_appearance().is_some());
    assert_eq!(
        overlay_mouse_face_property_query_count(),
        1,
        "a positive mouse-face sweep must prune unrelated offscreen property endpoints"
    );
}

#[test]
fn short_text_mouse_face_does_not_sweep_unrelated_later_overlays() {
    use crate::buffer_source::mouse_face::{
        overlay_mouse_face_property_query_count, reset_overlay_mouse_face_property_query_count,
    };

    const TEXT_START: usize = 2_000;
    const OVERLAY_COUNT: usize = 2_000;

    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert(&"x".repeat(TEXT_START + OVERLAY_COUNT + 2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(
                EmacsBytePos::new(TEXT_START),
                EmacsBytePos::new(TEXT_START + 1),
            ),
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
        for index in 0..OVERLAY_COUNT {
            let start = TEXT_START + 2 + index;
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial: index as u64 + 1,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
                buffer: Some(buffer_id),
                start,
                end: start + 1,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let start = CharPos0::new(TEXT_START);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        start,
        CharPos0::new(TEXT_START + 1),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    reset_overlay_mouse_face_property_query_count();
    let item = source.next_item(&mut context).expect("display item");

    assert!(item.pointer_appearance().is_some());
    assert_eq!(
        overlay_mouse_face_property_query_count(),
        0,
        "a one-character text property must bound overlay traversal before later endpoints"
    );
}

#[test]
fn unrelated_property_and_overlay_boundaries_keep_one_mouse_face_extent() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        let whole = EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(4));
        buffer.text_props_put_property_in_emacs_byte_range(
            whole,
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(2)),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(2), EmacsBytePos::new(3)),
            Value::symbol("invisible"),
            Value::T,
        );
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 9,
            plist: Value::NIL,
            buffer: Some(buffer_id),
            start: 1,
            end: 3,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
        buffer
            .overlays_mut()
            .overlay_put(
                overlay,
                Value::symbol("help-echo"),
                Value::string("unrelated"),
            )
            .unwrap();
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert!(
        items.len() >= 3,
        "unrelated boundaries should still split display items"
    );
    let expected = items[0].pointer_appearance().cloned().unwrap();
    assert!(
        items
            .iter()
            .all(|item| item.pointer_appearance() == Some(&expected)),
        "pointer identities: {:?}",
        items
            .iter()
            .map(|item| item.pointer_appearance())
            .collect::<Vec<_>>()
    );
}

#[test]
fn absent_mouse_face_does_not_request_an_exact_extent() {
    use crate::buffer_source::mouse_face::{
        reset_text_mouse_face_extent_query_count, text_mouse_face_extent_query_count,
    };

    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert(&"x".repeat(200));
        for start in (0..200).step_by(2) {
            buffer.text_props_put_property_in_emacs_byte_range(
                EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(start + 1)),
                Value::symbol("face"),
                Value::symbol("bold"),
            );
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    reset_text_mouse_face_extent_query_count();
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert!(items.iter().all(|item| item.pointer_appearance().is_none()));
    assert_eq!(
        text_mouse_face_extent_query_count(),
        0,
        "a nil mouse-face produces no pointer output, so exact extents are unnecessary"
    );
}

#[test]
fn absent_mouse_face_skips_overlay_extent_across_unrelated_empty_overlays() {
    use crate::buffer_source::mouse_face::{
        overlay_mouse_face_sweep_start_count, reset_overlay_mouse_face_sweep_start_count,
    };

    const LINE_COUNT: usize = 4_096;

    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert(&"x\n".repeat(LINE_COUNT));
        let before_string = Value::string(" ");
        for line in 0..LINE_COUNT {
            let anchor = line * 2;
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial: line as u64 + 1,
                plist: Value::list(vec![
                    Value::symbol("before-string"),
                    before_string,
                    Value::symbol("git-gutter"),
                    Value::T,
                ]),
                buffer: Some(buffer_id),
                start: anchor,
                end: anchor,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let start = CharPos0::new(LINE_COUNT + 1);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        start,
        start.add_len(neovm_core::buffer::CharLen::new(1)),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    reset_overlay_mouse_face_sweep_start_count();
    let item = source.next_item(&mut context).expect("display item");

    assert!(item.pointer_appearance().is_none());
    assert_eq!(
        overlay_mouse_face_sweep_start_count(),
        0,
        "absence is valid through the known display run and must not start an endpoint sweep"
    );
}

#[test]
fn distinct_mouse_face_extents_keep_distinct_identities() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        for (start, end, face) in [(0, 2, "highlight"), (2, 4, "low-mouse")] {
            buffer.text_props_put_property_in_emacs_byte_range(
                EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end)),
                Value::symbol("mouse-face"),
                Value::symbol(face),
            );
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    assert_eq!(items.len(), 2);
    assert_ne!(items[0].pointer_appearance(), items[1].pointer_appearance());
}

#[test]
fn cons_priority_uses_gnu_primary_before_nested_subpriority() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        for (serial, start, end, priority, face) in [
            (
                1,
                0,
                4,
                Value::cons(Value::fixnum(10), Value::fixnum(0)),
                "low-mouse",
            ),
            (
                2,
                1,
                3,
                Value::cons(Value::fixnum(5), Value::fixnum(999)),
                "high-mouse",
            ),
        ] {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial,
                plist: Value::NIL,
                buffer: Some(buffer_id),
                start,
                end,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("priority"), priority)
                .unwrap();
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("mouse-face"), Value::symbol(face))
                .unwrap();
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();

    let expected = items[0].pointer_appearance().cloned().unwrap();
    assert_eq!(expected.face(), RenderFaceRef::FaceId(FaceId::new(12)));
    assert!(
        items
            .iter()
            .all(|item| item.pointer_appearance() == Some(&expected))
    );
}

#[test]
fn nested_equal_priority_overlay_owns_only_its_effective_segment() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcd");
        for (serial, start, end, face) in [(1, 0, 4, "low-mouse"), (2, 1, 3, "high-mouse")] {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial,
                plist: Value::NIL,
                buffer: Some(buffer_id),
                start,
                end,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("priority"), Value::fixnum(5))
                .unwrap();
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("mouse-face"), Value::symbol(face))
                .unwrap();
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let items = std::iter::from_fn(|| source.next_item(&mut context)).collect::<Vec<_>>();
    let faces = items
        .iter()
        .map(|item| item.pointer_appearance().unwrap().face())
        .collect::<Vec<_>>();

    assert_eq!(
        faces,
        vec![
            RenderFaceRef::FaceId(FaceId::new(12)),
            RenderFaceRef::FaceId(FaceId::new(13)),
            RenderFaceRef::FaceId(FaceId::new(12)),
        ]
    );
    assert_ne!(items[0].pointer_appearance(), items[2].pointer_appearance());
}

#[test]
fn equal_priority_tie_and_nil_overlay_match_shared_gnu_selection() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("xy");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(2)),
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        );
        for (serial, value) in [
            (10, Value::symbol("low-mouse")),
            (11, Value::symbol("high-mouse")),
            (12, Value::NIL),
        ] {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial,
                plist: Value::NIL,
                buffer: Some(buffer_id),
                start: 0,
                end: 2,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("priority"), Value::fixnum(7))
                .unwrap();
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol("mouse-face"), value)
                .unwrap();
        }
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let selected = buffer
        .overlays()
        .highest_priority_overlay_at_emacs_byte_pos(
            EmacsBytePos::new(0),
            Value::symbol("mouse-face"),
        )
        .unwrap();
    let selected_value = buffer
        .overlays()
        .overlay_get_named(selected, Value::symbol("mouse-face"))
        .unwrap();
    let expected = match selected_value.as_symbol_name() {
        Some("low-mouse") => FaceId::new(12),
        Some("high-mouse") => FaceId::new(13),
        other => panic!("unexpected selected mouse face: {other:?}"),
    };
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
    let item = source.next_item(&mut context).unwrap();

    assert_eq!(
        item.pointer_appearance().unwrap().face(),
        RenderFaceRef::FaceId(expected)
    );
}

#[test]
fn lisp_string_mouse_face_survives_display_replacement_mapping() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "xy",
        vec![StringTextPropertyRun {
            start: 0,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("highlight"),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        41,
        value,
        RenderFaceRef::FaceId(FaceId::new(3)),
        LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source.next_item(&mut context).expect("mouse-face item");
    let pointer = item
        .pointer_appearance()
        .expect("string property must publish pointer appearance");
    assert_eq!(pointer.face(), RenderFaceRef::FaceId(FaceId::new(11)));
    assert_eq!(pointer.source().source_id(), Some(DisplaySourceId::new(41)));
    assert_eq!(pointer.source().end_char_index(), 2);
}

#[test]
fn overlay_string_mouse_faces_are_scoped_to_the_overlay_occurrence() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "xy",
        vec![StringTextPropertyRun {
            start: 0,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("highlight"),
            ]),
        }],
    );
    let pointer_for = |overlay_id, kind| {
        let mut source = LispStringSourceCursor::new(
            1,
            value,
            RenderFaceRef::FaceId(FaceId::new(3)),
            LispStringSourceOrigin::OverlayString { overlay_id, kind },
        )
        .expect("overlay string source");
        let mut resolver = SymbolFaceResolver;
        let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
        source
            .next_item(&mut context)
            .and_then(|item| item.pointer_appearance().cloned())
            .expect("overlay string mouse face")
    };

    let before = pointer_for(Value::fixnum(10), OverlayStringKind::Before);
    let before_fragment = pointer_for(Value::fixnum(10), OverlayStringKind::Before);
    let after = pointer_for(Value::fixnum(10), OverlayStringKind::After);
    let other_overlay = pointer_for(Value::fixnum(11), OverlayStringKind::Before);

    assert_eq!(before, before_fragment);
    assert_ne!(before, after);
    assert_ne!(before, other_overlay);
}

#[test]
fn display_replacement_string_mouse_faces_are_scoped_to_the_buffer_anchor() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "xy",
        vec![StringTextPropertyRun {
            start: 0,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("highlight"),
            ]),
        }],
    );
    let pointer_for = |anchor| {
        let replacement_source = BufferDisplayReplacementSource::new(
            BufferId(7),
            CharPos0::new(anchor),
            EmacsBytePos::new(anchor),
        );
        let mut source = BufferDisplayReplacementStringRequest::new(1, value, replacement_source)
            .into_source(FaceId::new(3))
            .expect("replacement string source");
        let mut resolver = SymbolFaceResolver;
        let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);
        source
            .next_item(&mut context)
            .and_then(|item| item.pointer_appearance().cloned())
            .expect("replacement string mouse face")
    };

    assert_eq!(pointer_for(4), pointer_for(4));
    assert_ne!(pointer_for(4), pointer_for(9));
}

#[test]
fn buffer_text_source_cursor_renders_explicit_composition_property_replacement() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("* H");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("composition"),
            Value::list(vec![
                Value::fixnum(0),
                Value::fixnum(1),
                Value::vector(vec![Value::fixnum('◉' as i64)]),
            ]),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["◉", " H"]);
    assert_eq!(
        items[0].span.end,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(1), EmacsBytePos::new(1))
    );
}

#[test]
fn buffer_text_source_cursor_renders_org_superstar_composition_property() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("* H");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let org_superstar_composition = Value::cons(
            Value::cons(Value::fixnum(1), Value::fixnum('○' as i64)),
            Value::NIL,
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("composition"),
            org_superstar_composition,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["○", " H"]);
    assert_eq!(
        items[0].span.end,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(1), EmacsBytePos::new(1))
    );
}

#[test]
fn buffer_text_source_cursor_resolves_face_property_runs() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("abc");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }

    assert_eq!(item_texts(&items), ["a", "b", "c"]);
    assert_eq!(items[0].face, RenderFaceRef::FaceId(FaceId::new(3)));
    assert_eq!(items[1].face, RenderFaceRef::FaceId(FaceId::new(7)));
    assert_eq!(items[2].face, RenderFaceRef::FaceId(FaceId::new(3)));
}

#[test]
fn buffer_text_source_cursor_pushes_display_string_replacement_source() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("axb");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            Value::string("YZ"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "YZ", "b"]);
    assert!(matches!(
        items[1].kind,
        DisplayItemKind::SourceMappedText(_)
    ));
    assert_eq!(
        items[1].span.start,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(1), EmacsBytePos::new(1))
    );
    assert_eq!(
        items[1].span.end,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(2), EmacsBytePos::new(2))
    );
}

#[test]
fn buffer_text_source_cursor_emits_propertized_display_string_as_atomic_replacement() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("axb");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        let replacement = Value::string_with_text_properties(
            "YZ",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
            }],
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            replacement,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let Some(first) =
        source_consumption.next_source_consumption_item(&mut source, &mut context, &mut position)
    else {
        panic!("expected leading text step");
    };
    let first = first.into_renderable().expect("leading renderable item");
    let end_charpos = first.end_charpos();
    let (_, first_item) = first.into_test_render_parts().expect("render parts");
    assert_eq!(item_texts(std::slice::from_ref(&first_item)), ["a"]);
    position = DisplaySourceTextPosition::new(position.byte_idx(), end_charpos);

    let Some(replacement_item) =
        source_consumption.next_source_consumption_item(&mut source, &mut context, &mut position)
    else {
        panic!("expected atomic replacement string item");
    };
    let BufferSourceConsumedItem::DisplayPropertyReplacement(replacement) = replacement_item else {
        panic!("expected replacement item kind");
    };

    assert_eq!(replacement.start_byte_idx(0), Some(1));
    assert_eq!(replacement.start_charpos(), 1);
    assert_eq!(replacement.descriptor().resume_charpos(), 2);
    assert_eq!(
        replacement
            .descriptor()
            .classification()
            .replacement_spec()
            .as_utf8_str(),
        Some("YZ")
    );
}

#[test]
fn buffer_text_source_cursor_emits_display_space_as_atomic_replacement() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let display_space = Value::list(vec![
        Value::symbol("space"),
        Value::keyword(":width"),
        Value::fixnum(2),
    ]);
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("axb");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            display_space,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let Some(first) =
        source_consumption.next_source_consumption_item(&mut source, &mut context, &mut position)
    else {
        panic!("expected leading text step");
    };
    let first = first.into_renderable().expect("leading renderable item");
    let end_charpos = first.end_charpos();
    let (_, first_item) = first.into_test_render_parts().expect("render parts");
    assert_eq!(item_texts(std::slice::from_ref(&first_item)), ["a"]);
    position = DisplaySourceTextPosition::new(position.byte_idx(), end_charpos);

    let Some(replacement_item) =
        source_consumption.next_source_consumption_item(&mut source, &mut context, &mut position)
    else {
        panic!("expected atomic display space item");
    };
    let BufferSourceConsumedItem::DisplayPropertyReplacement(replacement) = replacement_item else {
        panic!("expected replacement item kind");
    };

    assert_eq!(replacement.start_byte_idx(0), Some(1));
    assert_eq!(replacement.start_charpos(), 1);
    assert_eq!(replacement.descriptor().resume_charpos(), 2);
    assert!(matches!(
        replacement.descriptor().classification().replacement(),
        Some(DisplayReplacementProperty::Stretch(_))
    ));
}

#[test]
fn buffer_text_source_consumption_keeps_plain_text_run_renderable() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("abc");
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        buffer.total_char_end_pos(),
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let consumed = source_consumption
        .next_source_consumption_item(&mut source, &mut context, &mut position)
        .expect("renderable text run");
    let renderable = consumed.into_renderable().expect("renderable item");
    let (_, item) = renderable.into_test_render_parts().expect("render parts");

    assert_eq!(item_texts(&[item]), ["abc"]);
    assert_eq!(position, DisplaySourceTextPosition::new(3, 0));
}

#[test]
fn buffer_text_source_cursor_reports_nested_replacement_source_position() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("x");
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(0));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let replacement = Value::string_with_text_properties(
            "YZ",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
            }],
        );
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("display"),
            replacement,
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );
    let mut resolver = SymbolFaceResolver;
    let mut context = DisplaySourceContext::with_face_resolver(&mut resolver);

    let item = source
        .next_item(&mut context)
        .expect("first replacement item");

    assert_eq!(
        item.kind,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "Y",
            DisplaySourcePosition::lisp_string(1, 0, 0),
        ))
    );
    assert_eq!(
        item.span.start,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(0), EmacsBytePos::new(0))
    );
    assert_eq!(
        item.span.end,
        DisplaySourcePosition::buffer(buffer_id, CharPos0::new(1), EmacsBytePos::new(1))
    );
    assert!(matches!(
        source.source_position(),
        DisplaySourcePosition::LispString { char_index: 1, .. }
    ));
}

#[test]
fn buffer_text_source_cursor_emits_explicit_newline_row_breaks() {
    let (buffer_id, snapshot, end) = snapshot_with_text("a\nb");
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "b"]);
    assert!(matches!(items[1].kind, DisplayItemKind::RowBreak(_)));
}

#[test]
fn buffer_text_source_cursor_emits_control_and_glyphless_items() {
    let (buffer_id, snapshot, end) = snapshot_with_text("a\u{0001}\u{200b}b");
    let mut source = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::FaceId(FaceId::new(3)),
    );

    let items = collect_items(&mut source);

    assert_eq!(item_texts(&items), ["a", "b"]);
    assert_eq!(
        items[1].kind,
        DisplayItemKind::ControlChar { ch: '\u{0001}' }
    );
    assert_eq!(
        items[2].kind,
        DisplayItemKind::Glyphless(DisplayGlyphless {
            ch: '\u{200b}',
            method: GlyphlessMethod::ZeroWidth,
        })
    );
}

#[test]
fn typed_buffer_source_events_match_expected_plain_text_coordinates() {
    let text = "abc\ndef\tghi\n";
    let (buffer_id, snapshot, end) = snapshot_with_text(text);
    let mut cursor = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::Inherit,
    );
    let mut context = DisplaySourceContext::empty();

    let mut typed = Vec::new();
    let mut byte_offset = 0usize;
    let mut charpos = 0i64;
    while let Some(item) = cursor.next_item(&mut context) {
        match item.kind {
            DisplayItemKind::TextRun(run) => {
                for ch in run.text.chars() {
                    typed.push((ch, byte_offset, charpos));
                    byte_offset += ch.len_utf8();
                    charpos += 1;
                }
            }
            DisplayItemKind::RowBreak(row_break)
                if row_break.reason == DisplayRowBreakReason::ExplicitNewline =>
            {
                typed.push(('\n', byte_offset, charpos));
                byte_offset += 1;
                charpos += 1;
            }
            DisplayItemKind::ControlChar { ch }
            | DisplayItemKind::Glyphless(DisplayGlyphless { ch, .. }) => {
                typed.push((ch, byte_offset, charpos));
                byte_offset += ch.len_utf8();
                charpos += 1;
            }
            other => panic!("unexpected display item for plain text: {:?}", other),
        }
    }

    assert_eq!(typed, expected_source_coords(text));
}

#[test]
fn typed_buffer_source_events_match_expected_control_and_glyphless_coordinates() {
    let text = "abc\u{0001}def\u{200b}ghi\n";
    let (buffer_id, snapshot, end) = snapshot_with_text(text);
    let mut cursor = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::Inherit,
    );
    let mut context = DisplaySourceContext::empty();

    let mut typed = Vec::new();
    let mut byte_offset = 0usize;
    let mut charpos = 0i64;
    while let Some(item) = cursor.next_item(&mut context) {
        match item.kind {
            DisplayItemKind::TextRun(run) => {
                for ch in run.text.chars() {
                    typed.push((ch, byte_offset, charpos));
                    byte_offset += ch.len_utf8();
                    charpos += 1;
                }
            }
            DisplayItemKind::RowBreak(row_break)
                if row_break.reason == DisplayRowBreakReason::ExplicitNewline =>
            {
                typed.push(('\n', byte_offset, charpos));
                byte_offset += 1;
                charpos += 1;
            }
            DisplayItemKind::ControlChar { ch }
            | DisplayItemKind::Glyphless(DisplayGlyphless { ch, .. }) => {
                typed.push((ch, byte_offset, charpos));
                byte_offset += ch.len_utf8();
                charpos += 1;
            }
            other => panic!("unexpected display item: {:?}", other),
        }
    }

    assert_eq!(typed, expected_source_coords(text));
}

#[test]
fn typed_buffer_source_events_match_expected_face_property_coordinates() {
    let text = "abc\ndef\n";
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert(text);
        let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
        let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(3));
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(start, end),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
    }
    let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
    let end = buffer.total_char_end_pos();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);

    let mut cursor = BufferTextSourceCursor::new(
        buffer_id,
        &snapshot,
        CharPos0::ZERO,
        end,
        RenderFaceRef::Inherit,
    );
    let mut context = DisplaySourceContext::empty();

    let mut typed = Vec::new();
    let mut byte_offset = 0usize;
    let mut charpos = 0i64;
    while let Some(item) = cursor.next_item(&mut context) {
        match item.kind {
            DisplayItemKind::TextRun(run) => {
                for ch in run.text.chars() {
                    typed.push((ch, byte_offset, charpos));
                    byte_offset += ch.len_utf8();
                    charpos += 1;
                }
            }
            DisplayItemKind::RowBreak(row_break)
                if row_break.reason == DisplayRowBreakReason::ExplicitNewline =>
            {
                typed.push(('\n', byte_offset, charpos));
                byte_offset += 1;
                charpos += 1;
            }
            DisplayItemKind::ControlChar { ch }
            | DisplayItemKind::Glyphless(DisplayGlyphless { ch, .. }) => {
                typed.push((ch, byte_offset, charpos));
                byte_offset += ch.len_utf8();
                charpos += 1;
            }
            other => panic!("unexpected display item: {:?}", other),
        }
    }

    assert_eq!(typed, expected_source_coords(text));
}

#[test]
fn line_spacing_uses_gnu_numeric_and_cons_scaling_rules() {
    use crate::display_item::{DisplayLineSpacingPolicy, DisplayLineSpacingReference};

    let _eval = Context::new();
    let float = DisplayLineSpacingPolicy::from_property(Some(Value::make_float(2.0)));
    assert_eq!(float.resolve(13.0, 0.0), 26.0);

    let current = DisplayLineSpacingPolicy::from_property(Some(Value::cons(
        Value::NIL,
        Value::make_float(1.5),
    )));
    assert_eq!(current.resolve(12.0, 0.0), 18.0);
    assert!(matches!(
        current,
        DisplayLineSpacingPolicy::Scale {
            reference: DisplayLineSpacingReference::CurrentFace,
            ..
        }
    ));

    let named = DisplayLineSpacingPolicy::from_property(Some(Value::cons(
        Value::symbol("mode-line"),
        Value::fixnum(3),
    )));
    assert_eq!(named.resolve(10.0, 0.0), 30.0);
    assert!(matches!(
        named,
        DisplayLineSpacingPolicy::Scale {
            reference: DisplayLineSpacingReference::NamedFace(face),
            ..
        } if face.is_symbol_named("mode-line")
    ));
}
