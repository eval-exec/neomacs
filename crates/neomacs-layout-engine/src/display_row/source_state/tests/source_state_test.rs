use super::*;
use crate::display_item::{DisplayItemKind, DisplayTextRun, RenderFaceRef, SourceSpan};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_source::{
    DisplayMarginEmissionContent, DisplayNonTextAreaEmission, DisplaySourceContext,
    LispStringSourceCursor, LispStringSourceOrigin,
};
use crate::display_source_resolver::DisplaySourceFaceBasis;
use crate::neovm_bridge::FaceResolver;
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;

fn item(ch: char) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(0)),
        DisplayItemKind::TextRun(DisplayTextRun::new(ch.to_string())),
    )
}

fn params(resolver: &FaceResolver) -> DisplaySourceResolveParams<'_> {
    DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            resolver,
            FaceId::new(0),
            resolver.default_face(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        Default::default(),
    )
}

struct CountingSource {
    calls: usize,
    items: std::vec::IntoIter<DisplayItem>,
}

impl DisplayItemSource for CountingSource {
    fn next_item(&mut self, context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.calls += 1;
        let mut item = self.items.next()?;
        if self.calls == 1 {
            item.face = context.resolve_face_ref(
                item.face,
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            );
        }
        Some(item)
    }
}

#[test]
fn reused_output_slot_replaces_item_and_faces_at_every_source_step() {
    let _context = Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut state = DisplayRowSourceState::frame_local();
    let mut source = CountingSource {
        calls: 0,
        items: vec![item('a'), item('b')].into_iter(),
    };
    let mut output = ResolvedDisplaySourceItem::empty();
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert_eq!(
        output.item().unwrap().face,
        RenderFaceRef::FaceId(FaceId::new(20))
    );
    // Deliberately leave both outputs in the slot: the next step must replace
    // them, including the pending installation for the first item's face.
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert_eq!(
        output.item().unwrap().kind,
        DisplayItemKind::TextRun(DisplayTextRun::new("b"))
    );
    assert!(output.drain_pending_faces().next().is_none());
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert!(output.item().is_none());
    assert!(state.is_finished());
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert_eq!(source.calls, 3, "finished sources are not advanced again");
}

#[test]
fn resumed_and_deferred_items_replace_stale_slot_without_advancing_source() {
    let _context = Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0, 14.0, None);
    for deferred in [false, true] {
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
        let mut state = DisplayRowSourceState::frame_local();
        let mut source = CountingSource {
            calls: 0,
            items: vec![item('a')].into_iter(),
        };
        let mut output = ResolvedDisplaySourceItem::empty();
        state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
        let pending = output.item().unwrap().clone();
        state.remember_pending_item(if deferred {
            DisplayRowClippedRemainder::DeferWhole(pending.clone())
        } else {
            DisplayRowClippedRemainder::Resume(pending.clone())
        });
        state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
        assert_eq!(output.take_item(), Some(pending));
        assert!(output.drain_pending_faces().next().is_none());
        assert_eq!(source.calls, 1, "pending items already have resolved faces");
        state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
        assert!(state.is_finished());
        // A pending item remains consumable even after the source exhausted.
        let tail = item('t');
        state.remember_pending_item(DisplayRowClippedRemainder::Resume(tail.clone()));
        assert!(!state.is_finished());
        state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
        assert_eq!(output.take_item(), Some(tail));
        assert_eq!(source.calls, 2);
        state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
        assert!(output.item().is_none());
        assert_eq!(source.calls, 2);
    }
}

#[test]
fn exhausted_structural_source_drains_emission_once_outside_output_slot() {
    let _context = Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let content = Value::string("left marker");
    let value = Value::string_with_text_properties(
        "x",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::list(vec![Value::symbol("margin"), Value::symbol("left-margin")]),
                    content,
                ]),
            ]),
        }],
    );
    let mut source = LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(0)),
        LispStringSourceOrigin::Normal,
    )
    .unwrap();
    let mut state = DisplayRowSourceState::frame_local();
    let mut output = ResolvedDisplaySourceItem::empty();
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert!(output.item().is_none());
    assert!(state.is_finished());
    assert!(output.take_pending_non_text_area().is_empty());
    let emissions = state.take_pending_non_text_area();
    let [DisplayNonTextAreaEmission::Margin(margin)] = emissions.as_slice() else {
        panic!("one margin emission expected: {emissions:?}");
    };
    let DisplayMarginEmissionContent::String(actual) = margin.content() else {
        panic!("string margin content expected");
    };
    assert_eq!(*actual, content);
    state.next_resolved_item_into(&mut source, params(&resolver), &mut face_ids, &mut output);
    assert!(state.take_pending_non_text_area().is_empty());
}
