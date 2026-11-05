#[test]
fn absent_removal_keeps_interval_mutation_ticks() {
    use crate::buffer::text_props::TextPropertyTable;
    use crate::buffer::{CharLen, CharRange};
    use neovm_value::value::Value;

    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    table.put_property_for_object_char_len(
        CharRange::from_usize(0, 6),
        CharLen::new(6),
        Value::symbol("keep"),
        Value::T,
    );
    let before = (table.mutation_tick(), table.syntax_prop_tick());
    assert!(
        !table.remove_property_in_char_range(
            CharRange::from_usize(1, 5),
            Value::symbol("syntax-table")
        )
    );
    assert_eq!(before, (table.mutation_tick(), table.syntax_prop_tick()));
}
