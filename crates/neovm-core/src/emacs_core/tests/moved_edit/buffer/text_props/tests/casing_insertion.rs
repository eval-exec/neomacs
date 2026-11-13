use super::*;

use neovm_edit::emacs_core::textprop::{
    DirectCharProperties, StickinessProperty, TextPropertyControlVariable,
    resolve_effective_char_property,
};
use neovm_value::error::{Flow, LispCondition, signal};
use neovm_value::symbol::Obarray;

use neovm_edit::buffer::text_props::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;
use neovm_value::value::positions::CharLen;
use neovm_value::value::positions::CharPos0;
use neovm_value::value::positions::CharRange;

#[test]
fn category_front_stickiness_respects_positioned_symbol_equality() {
    let mut ctx = crate::test_utils::new_context();
    let roots = CasingPropertyRoots::new();
    let positioned_t =
        // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
        unsafe { ctx.tagged_heap_mut() }.alloc_symbol_with_pos(Value::T, Value::fixnum(77));
    roots.root(positioned_t);
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let category_key = Value::symbol("category");
    let category = Value::symbol("gdm-casing-positioned-front-category");
    let front = Value::symbol("front-sticky");
    for (equality, category_front, expected_boundaries) in [
        (
            CasingPropertyEquality::Exact,
            Value::T,
            vec![(0, 1), (1, 4)],
        ),
        (
            CasingPropertyEquality::PositionedSymbolsTransparent,
            Value::T,
            vec![(0, 1), (1, 4)],
        ),
        (
            CasingPropertyEquality::Exact,
            positioned_t,
            vec![(0, 1), (1, 2), (2, 4)],
        ),
        (
            CasingPropertyEquality::PositionedSymbolsTransparent,
            positioned_t,
            vec![(0, 1), (1, 4)],
        ),
    ] {
        ctx.obarray_mut()
            .put_property(
                "gdm-casing-positioned-front-category",
                "front-sticky",
                category_front,
            )
            .unwrap();
        let base_controls = controls(Value::NIL);
        let context = neovm_edit::test_support::authority::casing_property_context_for_test(
            ctx.obarray(),
            neovm_edit::test_support::authority::casing_property_controls_for_test(
                equality,
                base_controls.nonsticky_for_test(),
                base_controls.aliases_for_test(),
                base_controls.defaults_for_test(),
                std::marker::PhantomData,
            ),
        );
        let mut table = TextPropertyTable::new();
        set_run(
            &mut table,
            1,
            3,
            vec![(category_key, category), (face, bold)],
        );
        table
            .adjust_for_casify_insertion(
                CharPos0::new(1),
                CharLen::new(1),
                CharLen::new(3),
                &context,
                &roots,
            )
            .unwrap();
        table.assert_tree_invariants_for_test();
        assert_eq!(
            faces(&table, 4),
            vec![None, Some(bold), Some(bold), Some(bold)]
        );
        assert_eq!(
            table.get_property_at_char_pos(CharPos0::new(1), category_key),
            Some(category)
        );
        assert_eq!(
            table
                .interval_plist_runs_for_test()
                .into_iter()
                .map(|(start, end, _)| (start, end))
                .collect::<Vec<_>>(),
            expected_boundaries
        );
        let expects_front_metadata = match equality {
            CasingPropertyEquality::Exact => category_front == positioned_t,
            CasingPropertyEquality::PositionedSymbolsTransparent => false,
        };
        if expects_front_metadata {
            let explicit = table
                .get_property_at_char_pos(CharPos0::new(1), front)
                .unwrap();
            assert_eq!(explicit.cons_car(), category_key);
            assert_eq!(explicit.cons_cdr().cons_car(), face);
            assert!(explicit.cons_cdr().cons_cdr().is_nil());
        } else {
            assert_eq!(
                table.get_property_at_char_pos(CharPos0::new(1), front),
                None
            );
        }
    }
}

fn controls(nonsticky: Value) -> CasingPropertyControls {
    neovm_edit::test_support::authority::casing_property_controls_for_test(
        CasingPropertyEquality::Exact,
        nonsticky,
        Value::NIL,
        Value::NIL,
        std::marker::PhantomData,
    )
}

fn set_run(table: &mut TextPropertyTable, start: usize, end: usize, plist: Vec<(Value, Value)>) {
    table.set_properties_for_object_char_len(
        CharRange::from_usize(start, end),
        CharLen::new(3),
        plist,
    );
}

fn faces(table: &TextPropertyTable, len: usize) -> Vec<Option<Value>> {
    let face = Value::symbol("face");
    (0..len)
        .map(|pos| table.get_property_at_char_pos(CharPos0::new(pos), face))
        .collect()
}
