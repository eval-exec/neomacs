use neovm_edit::buffer::BufferId;

use neovm_edit::buffer::overlay::*;

#[test]
fn cached_overlay_snapshot_survives_exact_evaluator_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(progn (insert \"abcd\") (overlay-put (make-overlay 1 4) 'before-string (copy-sequence \"frozen\")))").unwrap();
    let buffer = eval.edit.buffers.current_buffer().unwrap().id;
    // No Rust-held Value roots the copied overlay. The cache belongs to the
    // live buffer and must keep its independent overlay/plist objects alive.
    drop(eval.edit.buffers.get(buffer).unwrap().overlays().snapshot());
    for _ in 0..3 {
        eval.gc_collect_exact();
        let snapshot = eval.edit.buffers.get(buffer).unwrap().overlays().snapshot();
        let overlay = overlays_at(&snapshot, 0)[0];
        assert_eq!(
            snapshot.overlay_get_named(overlay, Value::symbol("before-string")),
            Some(Value::string("frozen"))
        );
        assert_eq!(overlay_end(&snapshot, overlay), Some(3));
    }
}

#[test]
fn overlay_put_prepends_new_properties_and_updates_in_place() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(0, 1);
    list.insert_overlay(overlay);
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    list.overlay_put(overlay, face, Value::symbol("bold"))
        .unwrap();
    list.overlay_put(overlay, help, Value::string("tip"))
        .unwrap();
    list.overlay_put(overlay, face, Value::symbol("italic"))
        .unwrap();
    let plist = list.overlay_plist(overlay).unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&plist),
        "(help-echo \"tip\" face italic)"
    );
}

fn alloc_overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

fn overlay_end(list: &OverlayList, overlay: Value) -> Option<usize> {
    list.overlay_end_emacs_byte_pos(overlay)
        .map(EmacsBytePos::get)
}

fn overlays_at(list: &OverlayList, pos: usize) -> Vec<Value> {
    list.overlays_at_emacs_byte_pos(emacs_byte_pos(pos))
}

struct FilteredPropertyResolver<'a> {
    overlays: &'a OverlayList,
    lookup_order: &'a [Value],
}

impl OverlayPropertyResolver for FilteredPropertyResolver<'_> {
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
        let (canonical, aliases) = self.lookup_order.split_first()?;
        if let Some(value) = self.overlays.overlay_get_named(overlay, *canonical) {
            return Some(value);
        }
        aliases.iter().find_map(|alias| {
            self.overlays
                .overlay_get_named(overlay, *alias)
                .filter(|value| !value.is_nil())
        })
    }

    fn endpoint_filter(&self) -> OverlayPropertyFilter {
        OverlayPropertyFilter::for_properties(
            self.lookup_order
                .iter()
                .copied()
                .chain(std::iter::once(Value::symbol("category"))),
        )
    }
}

fn emacs_byte_pos(byte: usize) -> EmacsBytePos {
    EmacsBytePos::new(byte)
}

#[cfg(test)]
#[path = "gnu_identity.rs"]
mod gnu_identity;

#[cfg(test)]
mod property_filter_collision;
