//! GNU lisp.h:2098-2120, 2248-2259 and print.c:2250-2265 slot-budget contract.
use super::super::*;

#[test]
fn packed_subchar_header_uses_storage_slots_for_child_budget() {
    crate::test_utils::init_test_tracing();
    let value = Value::make_sub_char_table(3, 0, vec![Value::fixnum(42); 128]);
    // Both C ints always print, while their packed storage consumes one slot
    // on 64-bit targets and two on 32-bit targets. Reaching the header's budget
    // must emit no child, then one additional slot permits exactly one child.
    #[cfg(target_pointer_width = "64")]
    let header_budget = 1;
    #[cfg(target_pointer_width = "32")]
    let header_budget = 2;
    assert_eq!(SUB_CHAR_TABLE_HEADER_SLOTS, header_budget);
    let mut options = PrintOptions::default();
    options.print_length = Some(header_budget as i64);
    assert_eq!(print_value_with_options(&value, options), "#^^[3 0 ...]");
    options.print_length = Some(header_budget as i64 + 1);
    assert_eq!(print_value_with_options(&value, options), "#^^[3 0 42 ...]");
}
