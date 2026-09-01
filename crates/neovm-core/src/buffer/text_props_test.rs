use super::*;

fn char_pos(pos: usize) -> CharPos0 {
    CharPos0::new(pos)
}

fn char_len(len: usize) -> CharLen {
    CharLen::new(len)
}

fn char_range(start: usize, end: usize) -> CharRange {
    CharRange::from_usize(start, end)
}

fn put_chars(
    table: &mut TextPropertyTable,
    start: usize,
    end: usize,
    name: Value,
    value: Value,
) -> bool {
    table.put_property_in_char_range(char_range(start, end), name, value)
}

fn put_chars_for_object_len(
    table: &mut TextPropertyTable,
    start: usize,
    end: usize,
    object_len: usize,
    name: Value,
    value: Value,
) -> bool {
    table.put_property_for_object_char_len(
        char_range(start, end),
        char_len(object_len),
        name,
        value,
    )
}

fn get_at_char(table: &TextPropertyTable, pos: usize, name: Value) -> Option<Value> {
    table.get_property_at_char_pos(char_pos(pos), name)
}

fn props_at_char(table: &TextPropertyTable, pos: usize) -> HashMap<Value, Value> {
    table.get_properties_at_char_pos(char_pos(pos))
}

fn remove_chars(table: &mut TextPropertyTable, start: usize, end: usize, name: Value) -> bool {
    table.remove_property_in_char_range(char_range(start, end), name)
}

fn clear_chars(table: &mut TextPropertyTable, start: usize, end: usize) {
    table.remove_all_properties_in_char_range(char_range(start, end));
}

fn set_chars(table: &mut TextPropertyTable, start: usize, end: usize, plist: Vec<(Value, Value)>) {
    table.set_properties_in_char_range(char_range(start, end), plist);
}

fn set_chars_for_object_len(
    table: &mut TextPropertyTable,
    start: usize,
    end: usize,
    object_len: usize,
    plist: Vec<(Value, Value)>,
) {
    table.set_properties_for_object_char_len(char_range(start, end), char_len(object_len), plist);
}

fn next_change_after_char(table: &TextPropertyTable, pos: usize) -> Option<usize> {
    table
        .next_property_change_after_char_pos(char_pos(pos))
        .map(CharPos0::get)
}

fn next_single_change_after_char(
    table: &TextPropertyTable,
    pos: usize,
    name: Value,
) -> Option<usize> {
    table
        .next_single_property_change_after_char_pos(char_pos(pos), name)
        .map(CharPos0::get)
}

fn next_single_change_after_char_bounded(
    table: &TextPropertyTable,
    pos: usize,
    name: Value,
    limit: usize,
) -> Option<usize> {
    table
        .next_single_property_change_after_char_pos_bounded(char_pos(pos), name, char_pos(limit))
        .map(CharPos0::get)
}

fn previous_change_before_char(table: &TextPropertyTable, pos: usize) -> Option<usize> {
    table
        .previous_property_change_before_char_pos(char_pos(pos))
        .map(CharPos0::get)
}

fn next_raw_boundary_after_char(table: &TextPropertyTable, pos: usize) -> Option<usize> {
    table
        .next_interval_boundary_after_char_pos(char_pos(pos))
        .map(CharPos0::get)
}

fn previous_raw_boundary_before_char(table: &TextPropertyTable, pos: usize) -> Option<usize> {
    table
        .previous_interval_boundary_before_char_pos(char_pos(pos))
        .map(CharPos0::get)
}

fn insert_chars_at(table: &mut TextPropertyTable, pos: usize, len: usize) {
    table.adjust_for_insert_at_char_pos(char_pos(pos), char_len(len));
}

fn delete_char_range(table: &mut TextPropertyTable, start: usize, end: usize) {
    table.adjust_for_delete_char_range(char_range(start, end));
}

fn replace_chars_at(table: &mut TextPropertyTable, start: usize, old_len: usize, new_len: usize) {
    table.adjust_for_replace_at_char_pos(char_pos(start), char_len(old_len), char_len(new_len));
}

fn object_runs_for_char_len(table: &TextPropertyTable, len: usize) -> Vec<ObjectIntervalRun> {
    table.object_interval_runs_for_char_len(char_len(len))
}

// -----------------------------------------------------------------------
// Basic put/get
// -----------------------------------------------------------------------

#[test]
fn put_and_get_basic() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    assert!(get_at_char(&table, 0, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 2, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 4, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none()); // exclusive end
}

#[test]
fn get_property_returns_correct_value() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    let val = get_at_char(&table, 5, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "bold")
    );
}

#[test]
fn get_property_nonexistent_name() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    assert!(get_at_char(&table, 5, Value::symbol("syntax-table")).is_none());
}

#[test]
fn get_properties_returns_all() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("help-echo"),
        Value::string("tooltip"),
    );
    let props = props_at_char(&table, 5);
    assert_eq!(props.len(), 2);
    assert!(props.contains_key(&Value::symbol("face")));
    assert!(props.contains_key(&Value::symbol("help-echo")));
}

#[test]
fn get_property_outside_any_interval() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    assert!(get_at_char(&table, 0, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 3, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 10, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 15, Value::symbol("face")).is_none());
}

// -----------------------------------------------------------------------
// Overlapping ranges
// -----------------------------------------------------------------------

#[test]
fn overlapping_put_fills_uncovered_gaps() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        2,
        7,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        4,
        Value::symbol("custom-prop"),
        Value::symbol("value1"),
    );

    assert_eq!(
        get_at_char(&table, 0, Value::symbol("custom-prop")),
        Some(Value::symbol("value1"))
    );
    assert_eq!(
        get_at_char(&table, 3, Value::symbol("custom-prop")),
        Some(Value::symbol("value1"))
    );
    assert!(get_at_char(&table, 0, Value::symbol("face")).is_none());
    assert_eq!(
        get_at_char(&table, 3, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn overlapping_put_splits_intervals() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    // [0, 5) should still have "bold"
    let val = get_at_char(&table, 3, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "bold")
    );

    // [5, 15) should have "italic" (overwritten)
    let val = get_at_char(&table, 7, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "italic")
    );

    let val = get_at_char(&table, 12, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "italic")
    );
}

#[test]
fn multiple_properties_on_same_range() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("mouse-face"),
        Value::symbol("highlight"),
    );

    let props = props_at_char(&table, 5);
    assert_eq!(props.len(), 2);
}

#[test]
fn put_property_inner_range() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        20,
        Value::symbol("face"),
        Value::symbol("default"),
    );
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    let val = get_at_char(&table, 3, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "default")
    );

    let val = get_at_char(&table, 10, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "bold")
    );

    let val = get_at_char(&table, 17, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "default")
    );
}

#[test]
fn put_different_properties_on_overlapping_ranges() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("syntax-table"),
        Value::fixnum(42),
    );

    // Position 3: only "face"
    let props = props_at_char(&table, 3);
    assert_eq!(props.len(), 1);
    assert!(props.contains_key(&Value::symbol("face")));

    // Position 7: both "face" and "syntax-table"
    let props = props_at_char(&table, 7);
    assert_eq!(props.len(), 2);

    // Position 12: only "syntax-table"
    let props = props_at_char(&table, 12);
    assert_eq!(props.len(), 1);
    assert!(props.contains_key(&Value::symbol("syntax-table")));
}

// -----------------------------------------------------------------------
// Remove
// -----------------------------------------------------------------------

#[test]
fn remove_property_basic() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("help-echo"),
        Value::string("help"),
    );

    remove_chars(&mut table, 0, 10, Value::symbol("face"));

    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 5, Value::symbol("help-echo")).is_some());
}

#[test]
fn remove_property_partial_range() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    remove_chars(&mut table, 3, 7, Value::symbol("face"));

    // [0, 3) still has face
    assert!(get_at_char(&table, 2, Value::symbol("face")).is_some());
    // [3, 7) no longer has face
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
    // [7, 10) still has face
    assert!(get_at_char(&table, 8, Value::symbol("face")).is_some());
}

#[test]
fn remove_property_preserves_nil_interval_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    assert!(remove_chars(&mut table, 0, 1, Value::symbol("face")));

    assert_eq!(table.debug_interval_bounds(), vec![(0, 1, true)]);
    assert_eq!(
        object_runs_for_char_len(&table, 2),
        vec![(0, 1, Vec::new()), (1, 2, Vec::new())]
    );
    assert_eq!(next_raw_boundary_after_char(&table, 0), Some(1));
    assert_eq!(next_change_after_char(&table, 0), None);
}

#[test]
fn remove_all_properties_basic() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("help-echo"),
        Value::string("help"),
    );

    clear_chars(&mut table, 0, 10);

    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 5, Value::symbol("help-echo")).is_none());
}

#[test]
fn remove_all_properties_partial() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    clear_chars(&mut table, 3, 7);

    assert!(get_at_char(&table, 2, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 8, Value::symbol("face")).is_some());
}

// -----------------------------------------------------------------------
// next/previous property change
// -----------------------------------------------------------------------

#[test]
fn next_property_change_basic() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        15,
        20,
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    // Before any interval
    assert_eq!(next_change_after_char(&table, 0), Some(5));
    // Inside first interval
    assert_eq!(next_change_after_char(&table, 7), Some(10));
    // Between intervals
    assert_eq!(next_change_after_char(&table, 12), Some(15));
    // Inside second interval
    assert_eq!(next_change_after_char(&table, 17), Some(20));
    // After all intervals
    assert_eq!(next_change_after_char(&table, 25), None);
}

#[test]
fn next_property_change_at_boundary() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    // At start of interval
    assert_eq!(next_change_after_char(&table, 5), Some(10));
}

#[test]
fn next_single_property_change_ignores_other_properties() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    // One contiguous `invisible` run over [0, 30) ...
    put_chars(
        &mut table,
        0,
        30,
        Value::symbol("invisible"),
        Value::symbol("outline"),
    );
    // ... with a `face` change in the MIDDLE of it, at [10, 20).
    put_chars(
        &mut table,
        10,
        20,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    // The any-property scan fragments at the face boundaries (10, 20).
    assert_eq!(next_change_after_char(&table, 0), Some(10));
    // The single-`invisible` scan ignores the face change and reports the end
    // of the whole invisible run (the next interval boundary at 30).
    assert_eq!(
        next_single_change_after_char(&table, 0, Value::symbol("invisible")),
        Some(30)
    );
    assert_eq!(
        next_single_change_after_char(&table, 12, Value::symbol("invisible")),
        Some(30)
    );
    // A `face` scan still sees the face boundary at 20.
    assert_eq!(
        next_single_change_after_char(&table, 12, Value::symbol("face")),
        Some(20)
    );
}

#[test]
fn property_name_presence_never_turns_a_possible_property_into_an_absent_one() {
    let face = Value::symbol("face");
    let category = Value::symbol("category");
    let mut table = TextPropertyTable::new();

    assert_eq!(
        table.property_name_presence(face),
        PropertyNamePresence::DefinitelyAbsent
    );

    put_chars(&mut table, 0, 10, face, Value::symbol("bold"));
    assert_eq!(
        table.property_name_presence(face),
        PropertyNamePresence::PossiblyPresent
    );
    assert_eq!(
        table.property_name_presence(category),
        PropertyNamePresence::DefinitelyAbsent
    );

    // Removal deliberately does not require an O(intervals) recount.  The
    // conservative answer may remain positive, but it must never become a
    // false `DefinitelyAbsent` while a property exists.
    assert!(remove_chars(&mut table, 0, 10, face));
    assert_eq!(
        table.property_name_presence(face),
        PropertyNamePresence::PossiblyPresent
    );

    let mut source = TextPropertyTable::new();
    put_chars(
        &mut source,
        0,
        5,
        category,
        Value::symbol("display-category"),
    );
    table.append_shifted_at_char_offset(&source, char_len(10));
    assert_eq!(
        table.property_name_presence(category),
        PropertyNamePresence::PossiblyPresent
    );

    let help_echo = Value::symbol("help-echo");
    set_chars(&mut table, 20, 25, vec![(help_echo, Value::string("help"))]);
    assert_eq!(
        table.property_name_presence(help_echo),
        PropertyNamePresence::PossiblyPresent
    );

    let slice = table.slice_char_range(char_range(10, 25));
    assert_eq!(
        slice.property_name_presence(category),
        PropertyNamePresence::PossiblyPresent
    );
    assert_eq!(
        slice.property_name_presence(help_echo),
        PropertyNamePresence::PossiblyPresent
    );

    let snapshot = table.clone();
    let (
        ConservativePropertyNames::Assigned(live_names),
        ConservativePropertyNames::Assigned(snapshot_names),
    ) = (&table.property_names, &snapshot.property_names)
    else {
        panic!("a table with properties must carry an assigned-name summary");
    };
    assert!(
        Arc::ptr_eq(live_names, snapshot_names),
        "snapshot clones must share the immutable summary"
    );

    let keymap = Value::symbol("keymap");
    put_chars(&mut table, 0, 1, keymap, Value::symbol("map"));
    assert_eq!(
        snapshot.property_name_presence(keymap),
        PropertyNamePresence::DefinitelyAbsent,
        "the next mutation must detach a shared summary"
    );
}

#[test]
fn merging_equal_intervals_preserves_the_shared_property_name_summary() {
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let mut table = TextPropertyTable::new();
    put_chars(&mut table, 0, 5, face, bold);
    put_chars(&mut table, 5, 10, face, bold);

    let ConservativePropertyNames::Assigned(before) = &table.property_names else {
        panic!("a table with properties must carry an assigned-name summary");
    };
    let before = Arc::clone(before);

    table.merge_adjacent_equal_properties_around_char_range(char_range(0, 10));

    let ConservativePropertyNames::Assigned(after) = &table.property_names else {
        panic!("merging intervals must preserve the property-name summary");
    };
    assert!(
        Arc::ptr_eq(&before, after),
        "a merge that cannot add names must not rebuild or detach the summary"
    );
}

#[test]
fn merging_missing_properties_includes_the_source_name_summary() {
    let face = Value::symbol("face");
    let category = Value::symbol("category");
    let mut target = TextPropertyTable::new();
    put_chars(&mut target, 0, 10, face, Value::symbol("bold"));
    let mut source = TextPropertyTable::new();
    put_chars(
        &mut source,
        0,
        10,
        category,
        Value::symbol("display-category"),
    );

    target.merge_missing_shifted_at_char_offset(&source, CharLen::ZERO);

    assert_eq!(
        target.property_name_presence(category),
        PropertyNamePresence::PossiblyPresent
    );
}

#[test]
fn previous_single_property_change_ignores_other_properties() {
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        30,
        Value::symbol("mouse-face"),
        Value::symbol("highlight"),
    );
    put_chars(
        &mut table,
        10,
        20,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    assert_eq!(
        table
            .previous_single_property_change_before_char_pos(
                char_pos(25),
                Value::symbol("mouse-face"),
            )
            .map(CharPos0::get),
        Some(0)
    );
    assert_eq!(
        table
            .previous_single_property_change_before_char_pos(char_pos(12), Value::symbol("face"),)
            .map(CharPos0::get),
        Some(10)
    );
}

#[test]
fn previous_property_change_basic() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        15,
        20,
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    // After second interval
    assert_eq!(previous_change_before_char(&table, 25), Some(20));
    // Inside second interval
    assert_eq!(previous_change_before_char(&table, 17), Some(15));
    // Between intervals
    assert_eq!(previous_change_before_char(&table, 12), Some(10));
    // Inside first interval
    assert_eq!(previous_change_before_char(&table, 7), Some(5));
    // Before any interval
    assert_eq!(previous_change_before_char(&table, 3), None);
    assert_eq!(previous_change_before_char(&table, 0), None);
}

#[test]
fn previous_property_change_at_end() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    // At exclusive end of interval. GNU-verified via
    // `(previous-property-change 11)` with a `[6,11)` interval:
    // GNU returns 6 (the start), i.e. the position at the
    // exclusive end is treated as the scan still inside the run
    // going backward, so the change is at the interval start.
    assert_eq!(previous_change_before_char(&table, 10), Some(5));
}

#[test]
fn next_previous_empty_table() {
    crate::test_utils::init_test_tracing();
    let table = TextPropertyTable::new();
    assert_eq!(next_change_after_char(&table, 0), None);
    assert_eq!(previous_change_before_char(&table, 10), None);
}

// -----------------------------------------------------------------------
// adjust_for_insert
// -----------------------------------------------------------------------

#[test]
fn adjust_insert_shifts_intervals_after() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        10,
        20,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    insert_chars_at(&mut table, 5, 3);

    // Interval should now be [13, 23)
    assert!(get_at_char(&table, 12, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 13, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 22, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 23, Value::symbol("face")).is_none());
}

#[test]
fn adjust_insert_splits_spanning_interval_around_plain_inserted_text() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    insert_chars_at(&mut table, 10, 5);

    // Plain insert should leave the inserted range [10, 15) without properties.
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 9, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 10, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 12, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 14, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 15, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 20, Value::symbol("face")).is_none());
}

#[test]
fn adjust_insert_extends_plain_nil_interval_without_raw_split() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        4,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        8,
        10,
        Value::symbol("face"),
        Value::symbol("tail"),
    );

    insert_chars_at(&mut table, 6, 3);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 4, false), (4, 11, true), (11, 13, false)]
    );
    assert!(get_at_char(&table, 6, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 10, Value::symbol("face")).is_none());
    assert_eq!(
        get_at_char(&table, 11, Value::symbol("face")),
        Some(Value::symbol("tail"))
    );
}

#[test]
fn adjust_insert_at_nil_boundary_preserves_raw_default_interval() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        4,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        8,
        10,
        Value::symbol("face"),
        Value::symbol("tail"),
    );

    insert_chars_at(&mut table, 8, 2);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 4, false), (4, 8, true), (8, 10, true), (10, 12, false)]
    );
    assert!(get_at_char(&table, 8, Value::symbol("face")).is_none());
    assert_eq!(
        get_at_char(&table, 10, Value::symbol("face")),
        Some(Value::symbol("tail"))
    );
}

#[test]
fn full_object_property_mutation_keeps_trailing_nil_interval_for_inherited_insert() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars_for_object_len(
        &mut table,
        0,
        9,
        20,
        Value::symbol("org-todo-head"),
        Value::string("TODO"),
    );

    insert_chars_at(&mut table, 17, 1);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 9, false), (9, 21, true)]
    );
    assert!(get_at_char(&table, 17, Value::symbol("org-todo-head")).is_none());
}

#[test]
fn adjust_insert_at_interval_start() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    insert_chars_at(&mut table, 5, 3);

    // Interval should shift to [8, 13)
    assert!(get_at_char(&table, 7, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 8, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 12, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 13, Value::symbol("face")).is_none());
}

#[test]
fn adjust_insert_before_all() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    insert_chars_at(&mut table, 0, 2);

    assert!(get_at_char(&table, 7, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 6, Value::symbol("face")).is_none());
}

#[test]
fn adjust_insert_zero_length() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    insert_chars_at(&mut table, 7, 0);

    // No change
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 9, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 10, Value::symbol("face")).is_none());
}

#[test]
fn adjust_insert_preserves_raw_boundaries_without_rebuilding_tree() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    for i in 0..4 {
        put_chars(
            &mut table,
            i,
            i + 1,
            Value::symbol("slot"),
            Value::fixnum(i as i64),
        );
    }

    insert_chars_at(&mut table, 2, 2);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![
            (0, 1, false),
            (1, 2, false),
            (2, 4, true),
            (4, 5, false),
            (5, 6, false),
        ]
    );
    assert_eq!(
        get_at_char(&table, 1, Value::symbol("slot")),
        Some(Value::fixnum(1))
    );
    assert!(get_at_char(&table, 2, Value::symbol("slot")).is_none());
    assert!(get_at_char(&table, 3, Value::symbol("slot")).is_none());
    assert_eq!(
        get_at_char(&table, 4, Value::symbol("slot")),
        Some(Value::fixnum(2))
    );
}

#[test]
fn adjust_insert_beyond_trailing_property_preserves_insert_boundary() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        4,
        Value::symbol("part"),
        Value::symbol("state"),
    );
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("part"),
        Value::symbol("before"),
    );

    insert_chars_at(&mut table, 12, 8);
    put_chars(
        &mut table,
        11,
        19,
        Value::symbol("part"),
        Value::symbol("appended"),
    );

    // GNU `offset_intervals' still represents the default-property gap and
    // the inserted text as separate interval nodes.  `put-text-property'
    // updates both nodes but does not coalesce their now-equal plists.
    assert_eq!(
        table.debug_interval_bounds(),
        vec![
            (0, 4, false),
            (4, 5, true),
            (5, 10, false),
            (10, 11, true),
            (11, 12, false),
            (12, 19, false),
        ]
    );
    let snapshot = table.intervals_snapshot();
    assert_eq!(snapshot[2].start, 11);
    assert_eq!(snapshot[2].end, 12);
    assert_eq!(snapshot[3].start, 12);
    assert_eq!(snapshot[3].end, 19);
}

// -----------------------------------------------------------------------
// adjust_for_delete
// -----------------------------------------------------------------------

#[test]
fn adjust_delete_shifts_intervals_after() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        10,
        20,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 2, 5);

    // 3 bytes deleted before interval; interval becomes [7, 17)
    assert!(get_at_char(&table, 6, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 7, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 16, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 17, Value::symbol("face")).is_none());
}

#[test]
fn adjust_delete_removes_contained_interval() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 3, 12);

    // Entire interval was within deleted range
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 3, Value::symbol("face")).is_none());
}

#[test]
fn adjust_delete_truncates_start() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 10, 20);

    // Deletion overlaps end of interval; truncated to [5, 10)
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 9, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 10, Value::symbol("face")).is_none());
}

#[test]
fn adjust_delete_shrinks_spanning_interval() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        20,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 10, 15);

    // Deletion within interval; shrinks to [5, 15)
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 14, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 15, Value::symbol("face")).is_none());
}

#[test]
fn adjust_delete_inside_one_interval_does_not_create_raw_boundary() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        1,
        4,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 2, 3);

    let snapshot = table.intervals_snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].start, 1);
    assert_eq!(snapshot[0].end, 3);
    assert_eq!(next_raw_boundary_after_char(&table, 1), Some(3));
}

#[test]
fn adjust_delete_overlaps_interval_start() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        15,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 2, 10);

    // Deletion overlaps beginning of interval: [5,15) minus [2,10)
    // After: interval becomes [2, 7) (shifted: start=2, end=15-8=7)
    assert!(get_at_char(&table, 2, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 6, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 7, Value::symbol("face")).is_none());
}

#[test]
fn adjust_delete_empty_range() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    delete_char_range(&mut table, 7, 7);

    // No change
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 9, Value::symbol("face")).is_some());
}

#[test]
fn adjust_delete_removes_multiple_nodes_without_rebuilding_tree() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    for i in 0..6 {
        put_chars(
            &mut table,
            i,
            i + 1,
            Value::symbol("slot"),
            Value::fixnum(i as i64),
        );
    }

    delete_char_range(&mut table, 2, 5);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 1, false), (1, 2, false), (2, 3, false)]
    );
    assert_eq!(
        get_at_char(&table, 0, Value::symbol("slot")),
        Some(Value::fixnum(0))
    );
    assert_eq!(
        get_at_char(&table, 1, Value::symbol("slot")),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        get_at_char(&table, 2, Value::symbol("slot")),
        Some(Value::fixnum(5))
    );
}

// -----------------------------------------------------------------------
// replace_range interval offset
// -----------------------------------------------------------------------

#[test]
fn adjust_replace_growth_then_clear_preserves_gnu_nil_boundaries() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let org_table = Value::symbol("org-table");
    let mut table = TextPropertyTable::new();

    put_chars_for_object_len(&mut table, 0, 24, 80, face, org_table);

    // GNU `replace_range' calls `offset_intervals' once with the net
    // replacement delta, then `graft_intervals_into_buffer' clears the
    // inserted text when the replacement string has no intervals.
    replace_chars_at(&mut table, 46, 3, 5);
    set_chars_for_object_len(&mut table, 46, 51, 82, Vec::new());

    put_chars_for_object_len(&mut table, 42, 53, 82, face, org_table);

    assert_eq!(
        table.debug_interval_bounds(),
        vec![
            (0, 24, false),
            (24, 42, true),
            (42, 46, false),
            (46, 51, false),
            (51, 53, false),
            (53, 82, true),
        ]
    );
}

// -----------------------------------------------------------------------
// GNU raw interval boundaries vs semantic property changes
// -----------------------------------------------------------------------

#[test]
fn adjacent_equal_intervals_preserve_raw_boundary_but_skip_semantic_change() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );

    // GNU `put-text-property' preserves adjacent interval boundaries even when
    // the resulting plists are `eq' equal.  Ordinary property-change queries
    // skip that boundary, but the raw boundary remains observable through the
    // LIMIT=t path in `next-property-change'.
    assert!(get_at_char(&table, 0, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 7, Value::symbol("face")).is_some());
    assert_eq!(table.intervals_snapshot().len(), 2);

    assert_eq!(next_change_after_char(&table, 0), Some(10));
    assert_eq!(next_raw_boundary_after_char(&table, 0), Some(5));
    assert_eq!(next_change_after_char(&table, 5), Some(10));
    assert_eq!(previous_change_before_char(&table, 10), None);
    assert_eq!(previous_raw_boundary_before_char(&table, 10), Some(5));
}

#[test]
fn no_merge_different_properties() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        5,
        10,
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    // Should remain as two intervals.
    assert_eq!(next_change_after_char(&table, 0), Some(5));
    assert_eq!(next_change_after_char(&table, 5), Some(10));
}

#[test]
fn adjacent_equal_but_not_eq_values_do_not_merge() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    let left = Value::string("v");
    let right = Value::string("v");
    assert!(!crate::emacs_core::value::eq_value(&left, &right));
    assert!(crate::emacs_core::value::equal_value(&left, &right, 0));

    put_chars(&mut table, 0, 5, Value::symbol("p"), left);
    put_chars(&mut table, 5, 10, Value::symbol("p"), right);

    assert_eq!(table.intervals_snapshot().len(), 2);
    assert_eq!(next_change_after_char(&table, 0), Some(5));
    assert_eq!(next_change_after_char(&table, 5), Some(10));
}

#[test]
fn set_properties_merges_replaced_intervals_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        2,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        2,
        4,
        Value::symbol("face"),
        Value::symbol("italic"),
    );
    put_chars(
        &mut table,
        4,
        6,
        Value::symbol("face"),
        Value::symbol("underline"),
    );

    set_chars(
        &mut table,
        1,
        5,
        vec![(Value::symbol("category"), Value::T)],
    );

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 1, false), (1, 5, false), (5, 6, false)]
    );
    assert_eq!(next_raw_boundary_after_char(&table, 1), Some(5));
    assert_eq!(
        get_at_char(&table, 1, Value::symbol("category")),
        Some(Value::T)
    );
    assert_eq!(
        get_at_char(&table, 4, Value::symbol("category")),
        Some(Value::T)
    );
    assert_eq!(
        get_at_char(&table, 0, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        get_at_char(&table, 5, Value::symbol("face")),
        Some(Value::symbol("underline"))
    );
}

#[test]
fn set_properties_nil_merges_replaced_intervals_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        2,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        2,
        4,
        Value::symbol("face"),
        Value::symbol("italic"),
    );
    put_chars(
        &mut table,
        4,
        6,
        Value::symbol("face"),
        Value::symbol("underline"),
    );

    set_chars(&mut table, 1, 5, Vec::new());

    assert_eq!(
        table.debug_interval_bounds(),
        vec![(0, 1, false), (1, 5, true), (5, 6, false)]
    );
    assert_eq!(next_raw_boundary_after_char(&table, 1), Some(5));
    assert!(get_at_char(&table, 1, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 4, Value::symbol("face")).is_none());
    assert_eq!(
        get_at_char(&table, 0, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        get_at_char(&table, 5, Value::symbol("face")),
        Some(Value::symbol("underline"))
    );
}

#[test]
fn set_properties_merges_large_replaced_range_without_rebuilding_tree() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    for i in 0..10 {
        put_chars(
            &mut table,
            i,
            i + 1,
            Value::symbol("slot"),
            Value::fixnum(i as i64),
        );
    }

    set_chars(
        &mut table,
        2,
        8,
        vec![(Value::symbol("category"), Value::T)],
    );

    assert_eq!(
        table.debug_interval_bounds(),
        vec![
            (0, 1, false),
            (1, 2, false),
            (2, 8, false),
            (8, 9, false),
            (9, 10, false),
        ]
    );
    assert_eq!(next_raw_boundary_after_char(&table, 2), Some(8));
    assert_eq!(
        get_at_char(&table, 2, Value::symbol("category")),
        Some(Value::T)
    );
    assert_eq!(
        get_at_char(&table, 7, Value::symbol("category")),
        Some(Value::T)
    );
    assert_eq!(
        get_at_char(&table, 1, Value::symbol("slot")),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        get_at_char(&table, 8, Value::symbol("slot")),
        Some(Value::fixnum(8))
    );
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn put_property_empty_range() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        5,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    assert!(get_at_char(&table, 5, Value::symbol("face")).is_none());
}

#[test]
fn put_property_overwrites_same_name() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        0,
        10,
        Value::symbol("face"),
        Value::symbol("italic"),
    );

    let val = get_at_char(&table, 5, Value::symbol("face")).unwrap();
    assert!(
        val.as_symbol_id()
            .map_or(false, |id| crate::emacs_core::intern::resolve_sym(id)
                == "italic")
    );
}

#[test]
fn multiple_non_contiguous_intervals() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    put_chars(
        &mut table,
        0,
        5,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    put_chars(
        &mut table,
        10,
        15,
        Value::symbol("face"),
        Value::symbol("italic"),
    );
    put_chars(
        &mut table,
        20,
        25,
        Value::symbol("face"),
        Value::symbol("underline"),
    );

    assert!(get_at_char(&table, 3, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 7, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 12, Value::symbol("face")).is_some());
    assert!(get_at_char(&table, 17, Value::symbol("face")).is_none());
    assert!(get_at_char(&table, 22, Value::symbol("face")).is_some());
}

// -----------------------------------------------------------------------
// Dired-style multi-step operations (simulating insert-directory decode loop)
// -----------------------------------------------------------------------

/// Put `dired-filename` property on non-contiguous ranges, then simulate
/// the decode-coding-region loop that deletes and reinserts text in each
/// chunk.  This catches the bug where `next_property_change` returns None
/// after buffer modifications shift the interval runs.
#[test]
fn dired_decode_loop_property_survival() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    let prop = Value::symbol("dired-filename");
    let val = Value::T;

    // Simulate ls --dired output with 4 filenames at non-contiguous ranges.
    // Buffer layout: [header...][file1][spaces...][file2][spaces...][file3][spaces...][file4]
    put_chars(&mut table, 58, 64, prop, val); // file1 at [58, 64)
    put_chars(&mut table, 80, 86, prop, val); // file2 at [80, 86)
    put_chars(&mut table, 102, 108, prop, val); // file3 at [102, 108)
    put_chars(&mut table, 124, 130, prop, val); // file4 at [124, 130)

    // Verify initial next_property_change works for all ranges
    assert_eq!(
        next_change_after_char(&table, 0),
        Some(58),
        "should find file1"
    );
    assert_eq!(
        next_change_after_char(&table, 58),
        Some(64),
        "should find end of file1"
    );
    assert_eq!(
        next_change_after_char(&table, 64),
        Some(80),
        "should find file2 from gap"
    );
    assert_eq!(
        next_change_after_char(&table, 80),
        Some(86),
        "should find end of file2"
    );
    assert_eq!(
        next_change_after_char(&table, 86),
        Some(102),
        "should find file3 from gap"
    );
    assert_eq!(
        next_change_after_char(&table, 102),
        Some(108),
        "should find end of file3"
    );
    assert_eq!(
        next_change_after_char(&table, 108),
        Some(124),
        "should find file4 from gap"
    );
    assert_eq!(
        next_change_after_char(&table, 124),
        Some(130),
        "should find end of file4"
    );
    assert_eq!(
        next_change_after_char(&table, 130),
        None,
        "should be done after file4"
    );

    // Now simulate the decode loop. In insert-directory, the decode loop:
    // 1. Starts at point-min (char position 0)
    // 2. Finds next dired-filename change via next_property_change
    // 3. decode-coding-region from current to next position
    //    (which does: delete old text + insert new text)
    // 4. If the chunk had dired-filename, re-put the property
    // 5. Repeat until eobp

    // Simulate first chunk: pos=0 to 58 (header text, no dired-filename)
    // decode-coding-region on [0, 58) — text may shrink or expand
    // For this test, simulate that the decoded text is 2 chars shorter
    let mut old_len = 58;
    let mut new_len = 56;
    delete_char_range(&mut table, 0, old_len);
    insert_chars_at(&mut table, 0, new_len);
    // After this, all property positions should shift by (new_len - old_len) = -2
    // file1 was at [58,64), now at [56,62)
    // file2 was at [80,86), now at [78,84)
    // etc.

    assert_eq!(
        next_change_after_char(&table, 0),
        Some(56),
        "file1 should shift by -2 after first decode"
    );
    assert_eq!(next_change_after_char(&table, 56), Some(62), "end of file1");

    // Now simulate second chunk: pos=56 to 62 (file1, has dired-filename)
    // decode-coding-region with coding-no-eol — text may change length
    old_len = 6; // 62 - 56
    new_len = 4; // file1 decoded (UTF-8 multibyte chars become single chars, text shrinks)
    delete_char_range(&mut table, 56, 56 + old_len); // delete old file1 text
    insert_chars_at(&mut table, 56, new_len); // insert decoded text
    // file1 is now at [56, 60) — shift of -2 from previous
    // file2 was at [78,84), now shifts by (4-6) = -2 → [76, 82)
    // Re-put dired-filename on the decoded chunk
    put_chars(&mut table, 56, 60, prop, val);

    assert_eq!(
        next_change_after_char(&table, 0),
        Some(56),
        "file1 still at 56"
    );
    assert_eq!(
        next_change_after_char(&table, 56),
        Some(60),
        "end of decoded file1"
    );
    assert_eq!(
        next_change_after_char(&table, 60),
        Some(76),
        "file2 shifted correctly"
    );
    assert_eq!(next_change_after_char(&table, 76), Some(82), "end of file2");

    // Third chunk: gaps between file2 and file3
    // This tests that iterate-through-gaps correctly finds the next property
    assert_eq!(
        next_change_after_char(&table, 82),
        Some(98),
        "file3 should be after gap"
    );
    assert_eq!(
        next_change_after_char(&table, 98),
        Some(104),
        "end of file3"
    );
    assert_eq!(
        next_change_after_char(&table, 104),
        Some(120),
        "file4 should be after gap"
    );
    assert_eq!(
        next_change_after_char(&table, 120),
        Some(126),
        "end of file4"
    );
    assert_eq!(
        next_change_after_char(&table, 126),
        None,
        "no more properties"
    );
}

/// Simulate the exact sequence from insert-directory-clean:
/// put properties, then delete lines from the buffer.
#[test]
fn insert_directory_clean_then_delete_lines() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    let prop = Value::symbol("dired-filename");
    let val = Value::T;

    // Put properties at non-contiguous ranges (filenames in ls output)
    put_chars(&mut table, 58, 64, prop, val);
    put_chars(&mut table, 80, 86, prop, val);
    put_chars(&mut table, 102, 108, prop, val);

    // Simulate delete-region of //DIRED// lines at the end of buffer
    // In insert-directory-clean, lines are deleted from the dired section
    // (typically near the end of the ls output)
    // Delete region [130, 160) which is AFTER all the filename properties
    delete_char_range(&mut table, 130, 160);

    // Properties before the deleted region should be unaffected
    assert_eq!(next_change_after_char(&table, 0), Some(58));
    assert_eq!(next_change_after_char(&table, 58), Some(64));
    assert_eq!(next_change_after_char(&table, 64), Some(80));
    assert_eq!(next_change_after_char(&table, 80), Some(86));
    assert_eq!(next_change_after_char(&table, 86), Some(102));
    assert_eq!(next_change_after_char(&table, 102), Some(108));
    assert_eq!(next_change_after_char(&table, 108), None);

    // Now simulate decoding a chunk that is BEFORE all properties
    // This is the first iteration of the decode loop
    delete_char_range(&mut table, 0, 58);
    insert_chars_at(&mut table, 0, 55); // decoded text is shorter

    // All properties should shift by -3
    assert_eq!(
        next_change_after_char(&table, 0),
        Some(55),
        "file1 shifted to 55"
    );
    assert_eq!(next_change_after_char(&table, 55), Some(61));
    assert_eq!(
        next_change_after_char(&table, 61),
        Some(77),
        "file2 shifted to 77"
    );
    assert_eq!(next_change_after_char(&table, 77), Some(83));
    assert_eq!(
        next_change_after_char(&table, 83),
        Some(99),
        "file3 shifted to 99"
    );
    assert_eq!(next_change_after_char(&table, 99), Some(105));
    assert_eq!(next_change_after_char(&table, 105), None);
}

/// Regression test: adjust_for_delete can produce runs where start >= end,
/// which then causes next_property_change to loop infinitely or miss intervals.
#[test]
fn adjust_delete_produces_no_negative_len_runs() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    let prop = Value::symbol("dired-filename");
    let val = Value::T;

    // Put properties
    put_chars(&mut table, 10, 20, prop, val);
    put_chars(&mut table, 30, 40, prop, val);

    // Delete a region that partially overlaps the end of the first interval
    delete_char_range(&mut table, 15, 25);

    // First interval should be truncated to [10, 15)
    assert_eq!(next_change_after_char(&table, 0), Some(10));
    assert_eq!(
        next_change_after_char(&table, 10),
        Some(15),
        "first interval truncated at 15"
    );

    // Second interval should shift left by 10 (25-15=10)
    // Was [30, 40), now [20, 30)
    assert_eq!(
        next_change_after_char(&table, 15),
        Some(20),
        "second interval at 20"
    );
    assert_eq!(
        next_change_after_char(&table, 20),
        Some(30),
        "second interval ends at 30"
    );
    assert_eq!(next_change_after_char(&table, 30), None);

    // Verify no intervals have start >= end
    let interval_bounds = table.debug_interval_bounds();
    for (start, end, _) in &interval_bounds {
        assert!(start < end, "interval [{},{}) has start >= end", start, end);
    }
    // There should be exactly 2 non-empty intervals
    let non_empty: Vec<_> = interval_bounds
        .iter()
        .filter(|(_, _, is_empty)| !is_empty)
        .collect();
    assert_eq!(non_empty.len(), 2);
}

/// Exact simulation of the decode loop in insert-directory.
/// This test reproduces the exact sequence of operations that should happen
/// during the decode loop.  After putting dired-filename on 4 non-contiguous
/// filename ranges, we simulate decode-coding-region on each chunk.  The key
/// assertion: get_property at the start of a GAP chunk returns None (nil),
/// so the Lisp (if val ...) guard should prevent put-text-property on gaps.
#[test]
fn decode_loop_get_property_at_gap_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    let prop = Value::symbol("dired-filename");
    let val = Value::T;

    // Initial state: 4 filename properties at non-contiguous ranges.
    // These match the pattern from the trace: [57..58), [105..107), etc.
    put_chars(&mut table, 57, 58, prop, val); // file1: 1 char
    put_chars(&mut table, 105, 107, prop, val); // file2: 2 chars
    put_chars(&mut table, 154, 163, prop, val); // file3: 9 chars
    put_chars(&mut table, 210, 218, prop, val); // file4: 8 chars

    // Verify initial state
    assert_eq!(next_change_after_char(&table, 0), Some(57));
    assert!(
        get_at_char(&table, 57, prop).is_some(),
        "pos 57 should have df"
    );
    assert!(
        get_at_char(&table, 58, prop).is_none(),
        "pos 58 should NOT have df (end of file1)"
    );

    // === Iteration 1: decode header [0, 57) ===
    // val = get_at_char(0) = nil → do NOT re-put
    let mut old_len = 57;
    let mut new_len = 57; // decoded text same length
    delete_char_range(&mut table, 0, old_len);
    insert_chars_at(&mut table, 0, new_len);
    // No put-text-property because val was nil

    // Verify: positions unchanged (same length insert)
    assert_eq!(next_change_after_char(&table, 0), Some(57));
    assert!(get_at_char(&table, 57, prop).is_some());
    assert!(
        get_at_char(&table, 58, prop).is_none(),
        "pos 58 should still NOT have df after iter1"
    );

    // === Iteration 2: decode file1 [57, 58) ===
    // val = get_at_char(57) = t → re-put after decode
    old_len = 1; // 58 - 57
    new_len = 1; // decoded text same length
    delete_char_range(&mut table, 57, 57 + old_len);
    insert_chars_at(&mut table, 57, new_len);
    put_chars(&mut table, 57, 58, prop, val); // re-put (val was t)

    // Verify: file1 property preserved, pos 58 still gap
    assert!(get_at_char(&table, 57, prop).is_some());
    assert!(
        get_at_char(&table, 58, prop).is_none(),
        "CRITICAL: pos 58 must be nil - next iteration captures val here"
    );
    assert_eq!(
        next_change_after_char(&table, 58),
        Some(105),
        "next change from pos 58 should be file2 at 105"
    );

    // === Iteration 3: decode GAP [58, 105) ===
    // val = get_at_char(58) = nil → should NOT re-put
    // BUT the trace shows put [58..105) IS happening!
    // This test checks whether get_at_char(58) correctly returns nil.
    assert!(
        get_at_char(&table, 58, prop).is_none(),
        "BUG CONFIRMATION: if this fails, get_property returns non-nil at pos 58"
    );

    old_len = 47; // 105 - 58
    new_len = 47; // same length decode
    delete_char_range(&mut table, 58, 58 + old_len);
    // After delete, file2 [105..107) shifts to [58..60), merges with [57..58) → [57..60)
    // This is correct behavior — the gap between file1 and file2 is eliminated by delete.
    // The subsequent insert re-creates the gap.
    insert_chars_at(&mut table, 58, new_len);

    // After insert, file2 shifts back. Check that pos 58 is still nil.
    assert!(
        get_at_char(&table, 58, prop).is_none(),
        "CRITICAL: pos 58 must be nil after decode of gap - put should NOT be called"
    );

    // Now simulate what the BUG does: put dired-filename on [58, 105) even
    // though val was nil (this is what we observe in the trace).  GNU keeps
    // raw interval boundaries here, but semantic property-change queries skip
    // the equal adjacent dired-filename intervals.
    put_chars(&mut table, 58, 58 + new_len, prop, val);
    assert_eq!(
        next_change_after_char(&table, 58),
        Some(107),
        "after erroneous put on gap, file1 and file2 form one semantic run"
    );

    // Verify that the raw GNU-style boundaries were preserved.
    let snapshot = table.intervals_snapshot();
    assert_eq!(
        snapshot.len(),
        5,
        "raw intervals remain split after gap put"
    );
    // Note: if get_at_char(58) correctly returns nil, this put would NEVER happen
    // because the Lisp (if val ...) guard prevents it
}

#[test]
fn next_single_property_change_bounded_matches_unbounded_within_limit_and_soft_stops_beyond() {
    // Ten `face` runs create interval boundaries every 10 chars across [0, 100)
    // -- the fontified-buffer shape -- with a single `invisible` run far along
    // at [80, 90). This is the scenario the display scan hits: many interval
    // boundaries to walk before any `invisible` change.
    let mut table = TextPropertyTable::new();
    for i in 0..10 {
        let face = if i % 2 == 0 { "a" } else { "b" };
        put_chars(
            &mut table,
            i * 10,
            i * 10 + 10,
            Value::symbol("face"),
            Value::symbol(face),
        );
    }
    put_chars(
        &mut table,
        80,
        90,
        Value::symbol("invisible"),
        Value::symbol("t"),
    );
    let invisible = Value::symbol("invisible");

    // Unbounded: the scan coalesces `invisible == nil` across every face
    // boundary and reports the first real change, at 80.
    assert_eq!(
        next_single_change_after_char(&table, 0, invisible),
        Some(80)
    );
    // A generous limit reproduces the exact answer.
    assert_eq!(
        next_single_change_after_char_bounded(&table, 0, invisible, 200),
        Some(80),
    );
    // A limit at 30 (before the invisible run) soft-stops: the interval boundary
    // at 30 is >= limit, so the walk stops early and returns the limit. Never
    // larger than the true boundary (80), so the caller only re-checks sooner.
    assert_eq!(
        next_single_change_after_char_bounded(&table, 0, invisible, 30),
        Some(30),
    );
    // A limit that is not on a boundary still soft-stops at the limit itself.
    assert_eq!(
        next_single_change_after_char_bounded(&table, 0, invisible, 32),
        Some(32),
    );
}

#[test]
fn non_nil_property_range_query_ignores_matching_properties_outside_the_range() {
    let mut table = TextPropertyTable::new();
    for i in 0..10 {
        put_chars(
            &mut table,
            i * 10,
            i * 10 + 10,
            Value::symbol("face"),
            Value::symbol(if i % 2 == 0 { "a" } else { "b" }),
        );
    }
    put_chars(
        &mut table,
        80,
        90,
        Value::symbol("display"),
        Value::string("replacement"),
    );
    put_chars(&mut table, 20, 21, Value::symbol("invisible"), Value::T);
    let structural = [Value::symbol("display"), Value::symbol("invisible")];

    assert!(!table.has_any_non_nil_property_in_char_range(char_range(0, 1), &structural));
    assert!(table.has_any_non_nil_property_in_char_range(char_range(20, 21), &structural));
    assert!(table.has_any_non_nil_property_in_char_range(char_range(79, 81), &structural));
    assert!(!table.has_any_non_nil_property_in_char_range(char_range(80, 80), &structural));
}

#[test]
fn find_id_memo_never_disagrees_with_fresh_descent() {
    // Differential fuzz for the find_id positional memo (version/cache_* fields):
    // apply a randomized sequence of splits (put), merges (remove), and position
    // shifts (insert/delete adjust), interleaving cache-populating find_id calls
    // with the uncached ground truth, and assert they never disagree. A missing
    // invalidation would surface here as a stale hit.
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let fontified = Value::symbol("fontified");

    let mut table = TextPropertyTable::new();
    table.put_property_in_char_range(char_range(0, 200), face, bold);

    // Deterministic LCG -- no rng dependency, reproducible failures.
    let mut rng: u64 = 0x1234_5678_9abc_def0;
    let mut next = |bound: usize| -> usize {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((rng >> 33) as usize) % bound.max(1)
    };

    let mut hi = 200usize;
    for _ in 0..4000 {
        // Populate the memo before mutating, so a stale entry could survive.
        let _ = table.intervals.find_id(char_pos(next(hi + 4)));

        let a = next(hi + 1);
        let b = next(hi + 1);
        let (lo, up) = if a <= b { (a, b) } else { (b, a) };
        match next(5) {
            0 if up > lo => {
                let v = if next(2) == 0 { bold } else { italic };
                table.put_property_in_char_range(char_range(lo, up), face, v);
            }
            1 if up > lo => {
                table.put_property_in_char_range(char_range(lo, up), fontified, Value::NIL);
            }
            2 if up > lo => {
                table.remove_property_in_char_range(char_range(lo, up), face);
            }
            3 => {
                let len = 1 + next(8);
                table.adjust_for_insert_at_char_pos(char_pos(lo.min(hi)), char_len(len));
                hi += len;
            }
            _ if up > lo => {
                table.adjust_for_delete_char_range(char_range(lo, up));
                hi -= up - lo;
            }
            _ => {}
        }

        for _ in 0..6 {
            let pos = char_pos(next(hi + 4));
            assert_eq!(
                table.intervals.find_id(pos),
                table.intervals.find_id_uncached(pos),
                "find_id memo disagreed with fresh descent at {pos:?} (hi={hi})"
            );
        }

        for run in table.intervals.runs() {
            for (name, _) in plist_pairs(run.plist) {
                assert_eq!(
                    table.property_name_presence(name),
                    PropertyNamePresence::PossiblyPresent,
                    "presence index lost a live property after a randomized mutation"
                );
            }
        }
    }
}

#[test]
fn find_id_sequential_finger_never_disagrees_with_fresh_descent() {
    // Differential fuzz for the SEQUENTIAL-SCAN FINGER in find_id (the
    // `pos == cache_end` arm that steps to the tree-order successor instead of
    // re-descending).
    //
    // The existing find_id_memo_never_disagrees_with_fresh_descent probes
    // RANDOM positions, which is exactly the pattern the finger does not serve,
    // so it leaves this path nearly untested. Here every probe walks FORWARD
    // from the end of the run just returned -- what a font-lock property scan
    // does, and the only pattern that reaches the finger.
    //
    // Mutations are interleaved so a finger surviving a structural change (a
    // stale parent link or a missed version bump) shows up as a disagreement.
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");

    let mut table = TextPropertyTable::new();
    table.put_property_in_char_range(char_range(0, 400), face, bold);

    // Deterministic LCG -- no rng dependency, reproducible failures.
    let mut rng: u64 = 0x0fed_cba9_8765_4321;
    let mut next = |bound: usize| -> usize {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((rng >> 33) as usize) % bound.max(1)
    };

    // Fragment the text into many intervals so the successor walk has work to
    // do and a root descent would be genuinely deeper.
    let mut hi = 400usize;
    for i in 0..60 {
        let lo = (i * 6) % hi;
        let up = (lo + 3).min(hi);
        if up > lo {
            let v = if i % 2 == 0 { bold } else { italic };
            table.put_property_in_char_range(char_range(lo, up), face, v);
        }
    }

    for round in 0..400 {
        // Full sequential sweep: each probe lands exactly on the end of the
        // previously located run, which is the finger's entry condition.
        let mut pos = 0usize;
        let mut steps = 0;
        while pos < hi && steps < 4096 {
            let got = table.intervals.find_id(char_pos(pos));
            let want = table.intervals.find_id_uncached(char_pos(pos));
            assert_eq!(
                got, want,
                "finger disagreed with fresh descent at pos={pos} (hi={hi}, round={round})"
            );
            let Some((start, id)) = got else { break };
            // Advance to one past this run -- the successor lookup.
            let len = table.intervals.node_len(id).get().max(1);
            pos = start.get() + len;
            steps += 1;
        }

        // Mutate, then sweep again; a finger that outlived the change is caught
        // on the next round's very first probes.
        let a = next(hi + 1);
        let b = next(hi + 1);
        let (lo, up) = if a <= b { (a, b) } else { (b, a) };
        match next(5) {
            0 if up > lo => {
                let v = if next(2) == 0 { bold } else { italic };
                table.put_property_in_char_range(char_range(lo, up), face, v);
            }
            1 if up > lo => {
                table.remove_property_in_char_range(char_range(lo, up), face);
            }
            2 => {
                let len = 1 + next(8);
                table.adjust_for_insert_at_char_pos(char_pos(lo.min(hi)), char_len(len));
                hi += len;
            }
            3 if up > lo => {
                table.adjust_for_delete_char_range(char_range(lo, up));
                hi -= up - lo;
            }
            _ => {}
        }
    }
}

#[test]
fn a_default_constructed_tree_reports_no_memo() {
    // The property that MemoGeneration exists to hold: a tree that has cached
    // nothing must not report a memo as valid, no matter how it was built.
    //
    // This is the bug that shipped when IntervalTree derived Default --
    // cache_gen == 0 == the starting version, so the memo read as populated
    // while cache_id pointed at a node that did not exist. It stayed invisible
    // until a lookup arm other than the containment check consulted it.
    // Asserting it directly means a future field addition cannot reintroduce
    // it silently.
    for (label, tree) in [
        ("Default::default", IntervalTree::default()),
        ("IntervalTree::new", IntervalTree::new()),
        ("clone of empty", IntervalTree::new().clone()),
    ] {
        assert_eq!(
            tree.cache_gen.get(),
            MemoGeneration::EMPTY,
            "{label} published a memo generation before caching anything"
        );
        // And the lookup path must survive being asked, on an empty tree, for
        // the exact position the finger arm keys on.
        assert_eq!(tree.find_id(char_pos(0)), None, "{label}");
    }
}

// ---- graft (append_shifted_*): GNU `graft_intervals_into_buffer` -------------

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

fn random_plist(rng: &mut u64) -> Vec<(Value, Value)> {
    match lcg(rng) % 4 {
        0 => vec![],
        1 => vec![(Value::symbol("face"), Value::symbol("bold"))],
        2 => vec![
            (Value::symbol("face"), Value::symbol("italic")),
            (Value::symbol("fontified"), Value::T),
        ],
        _ => vec![
            (Value::symbol("help-echo"), Value::string("h")),
            (Value::symbol("face"), Value::symbol("bold")),
        ],
    }
}

fn random_table(rng: &mut u64, len: usize, segments: usize) -> TextPropertyTable {
    let mut table = TextPropertyTable::new();
    for _ in 0..segments {
        let a = (lcg(rng) as usize) % len;
        let b = (lcg(rng) as usize) % len;
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        if a == b {
            continue;
        }
        let plist = random_plist(rng);
        if plist.is_empty() {
            clear_chars(&mut table, a, b);
        } else {
            set_chars(&mut table, a, b, plist);
        }
    }
    table
}

#[test]
fn graft_matches_the_per_run_reference_on_random_layouts() {
    let mut rng = 0x5eed_u64;
    for case in 0..400 {
        let target_len = 1 + (lcg(&mut rng) as usize) % 120;
        let base_segments = (lcg(&mut rng) as usize) % 8;
        let base = random_table(&mut rng, target_len, base_segments);
        let source_len = 1 + (lcg(&mut rng) as usize) % 40;
        let source_segments = 1 + (lcg(&mut rng) as usize) % 6;
        let source = random_table(&mut rng, source_len, source_segments);
        // Offsets past the current cover exercise `ensure_cover`.
        let offset = (lcg(&mut rng) as usize) % (target_len + 10);

        let mut expected = base.clone();
        expected.append_shifted_reference_for_test(&source, char_len(offset));
        let mut actual = base.clone();
        actual.append_shifted_at_char_offset(&source, char_len(offset));
        actual.assert_tree_invariants_for_test();
        assert_eq!(
            actual.interval_plist_runs_for_test(),
            expected.interval_plist_runs_for_test(),
            "case {case}: graft at offset {offset}"
        );
    }
}

#[test]
fn graft_rehomes_the_left_remainder_and_keeps_the_right_remainder_plist_object() {
    let face = Value::symbol("face");
    let mut table = TextPropertyTable::new();
    set_chars(&mut table, 0, 30, vec![(face, Value::symbol("bold"))]);
    let original = table.raw_plist_at_for_test(char_pos(5)).unwrap();
    let mut source = TextPropertyTable::new();
    set_chars(&mut source, 0, 10, vec![(face, Value::symbol("italic"))]);
    let source_plist = source.raw_plist_at_for_test(char_pos(0)).unwrap();

    table.append_shifted_at_char_offset(&source, char_len(10));

    let left = table.raw_plist_at_for_test(char_pos(5)).unwrap();
    let grafted = table.raw_plist_at_for_test(char_pos(15)).unwrap();
    let right = table.raw_plist_at_for_test(char_pos(25)).unwrap();
    // GNU `copy_properties (under, end_unchanged)`: the unchanged left part is
    // re-homed onto a fresh plist ...
    assert_ne!(left.bits(), original.bits());
    assert_eq!(left, original);
    // ... the grafted text never aliases the source string's plist ...
    assert_ne!(grafted.bits(), source_plist.bits());
    assert_eq!(grafted, source_plist);
    // ... and `under` itself keeps its plist object past the graft.
    assert_eq!(right.bits(), original.bits());
    assert_eq!(get_at_char(&table, 15, face), Some(Value::symbol("italic")));
    assert_eq!(get_at_char(&table, 25, face), Some(Value::symbol("bold")));
    assert_eq!(table.intervals_snapshot().len(), 3);
}

#[test]
fn graft_of_many_runs_balances_once_and_keeps_the_tree_shallow() {
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let mut table = TextPropertyTable::new();
    for i in 0..1000 {
        set_chars(
            &mut table,
            i,
            i + 1,
            vec![(face, if i % 2 == 0 { bold } else { italic })],
        );
    }
    let mut source = TextPropertyTable::new();
    for i in 0..300 {
        set_chars(
            &mut source,
            i * 2,
            i * 2 + 2,
            vec![(face, if i % 2 == 0 { bold } else { italic })],
        );
    }
    // The buffer insert stretches the tree by a property-free span first
    // (GNU `adjust_intervals_for_insertion`), then the string's runs are
    // grafted over it.
    table.adjust_for_insert_at_char_pos(char_pos(500), char_len(600));
    reset_interval_balance_calls_for_test();
    table.append_shifted_at_char_offset(&source, char_len(500));
    let balance_calls = interval_balance_calls_for_test();
    table.assert_tree_invariants_for_test();

    assert_eq!(table.intervals_snapshot().len(), 1300);
    // The spine is relinked balanced in O(k); only the climb above the anchor
    // rebalances (the old per-run split climbed twice per run, ~7K calls).
    assert!(balance_calls <= 64, "balance calls: {balance_calls}");
    let depth = table.tree_max_depth_for_test();
    assert!(depth <= 40, "tree depth {depth}");
    assert_eq!(get_at_char(&table, 500, face), Some(bold));
    assert_eq!(get_at_char(&table, 1099, face), Some(italic));
    assert_eq!(get_at_char(&table, 1100, face), Some(bold));
    assert_eq!(get_at_char(&table, 499, face), Some(italic));
}
