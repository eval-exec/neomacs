use super::*;

fn state(
    pt_byte: usize,
    pt: usize,
    begv_byte: usize,
    begv: usize,
    zv_byte: usize,
    zv: usize,
) -> BufferEditState {
    BufferEditState::new(
        TextPositionAnchor::from_usize(pt, pt_byte),
        TextPositionAnchor::from_usize(begv, begv_byte),
        TextPositionAnchor::from_usize(zv, zv_byte),
    )
}

fn replace_state(old: BufferEditState) -> BufferEditState {
    replace_state_after_edit(
        old,
        TextReplacement::new(
            TextEditRange::from_usize(20, 36, 10, 18),
            crate::buffer::TextExtent::from_usize(3, 5),
        ),
    )
}

fn same_len_edit() -> MeasuredSameLenEdit {
    MeasuredSameLenEdit::new(
        TextEditRange::from_usize(0, 10, 0, 10),
        TextEditRange::from_usize(3, 10, 3, 10),
    )
}

fn insertion() -> TextInsertion {
    TextInsertion::from_usize(20, 10, 3, 5)
}

fn deleted_range() -> TextEditRange {
    TextEditRange::from_usize(20, 36, 10, 18)
}

fn measured_insert_edit() -> MeasuredInsertEdit {
    MeasuredInsertEdit::by_insertion_type(insertion(), InsertMarkerPlacement::AfterMarkers)
}

fn measured_delete_edit() -> MeasuredDeleteEdit {
    MeasuredDeleteEdit::new(deleted_range())
}

fn measured_replace_edit() -> MeasuredReplaceEdit {
    MeasuredReplaceEdit::new(TextReplacement::new(
        deleted_range(),
        TextExtent::from_usize(3, 5),
    ))
}

fn state_policy_for_shared_sibling(edit: SharedTextEditMetadata) -> SharedTextEditStatePolicy {
    edit.state_policy_for_shared_sibling(|| SharedBufferStateUpdate::RefreshFromStateMarkers)
}

#[test]
fn shared_edit_metadata_derives_sibling_state_policy() {
    assert_eq!(
        state_policy_for_shared_sibling(SharedTextEditMetadata::Insert(measured_insert_edit())),
        SharedTextEditStatePolicy::StateFields(SharedBufferStateUpdate::RefreshFromStateMarkers)
    );
    assert_eq!(
        state_policy_for_shared_sibling(SharedTextEditMetadata::Delete(measured_delete_edit())),
        SharedTextEditStatePolicy::StateFields(SharedBufferStateUpdate::RefreshFromStateMarkers)
    );
    assert_eq!(
        state_policy_for_shared_sibling(SharedTextEditMetadata::Replace(measured_replace_edit())),
        SharedTextEditStatePolicy::StateFields(SharedBufferStateUpdate::RefreshFromStateMarkers)
    );
    assert_eq!(
        state_policy_for_shared_sibling(SharedTextEditMetadata::Transposition {
            edit: same_len_edit(),
            transposition: TextTransposition::from_usize(2, 5, 1, 3, 8, 10, 5, 7),
            modified_state: SameLenModifiedStatePolicy::PreserveUnmodifiedIfClean,
        }),
        SharedTextEditStatePolicy::StateFields(SharedBufferStateUpdate::RefreshFromStateMarkers)
    );
    assert_eq!(
        state_policy_for_shared_sibling(SharedTextEditMetadata::SameLen {
            edit: same_len_edit(),
            modified_state: SameLenModifiedStatePolicy::PreserveUnmodifiedIfClean,
        }),
        SharedTextEditStatePolicy::NoStateFields
    );
}

#[test]
fn insert_state_current_buffer_advances_point_at_insert_and_zv() {
    assert_eq!(
        insert_state_after_edit(
            state(20, 10, 0, 0, 60, 42),
            insertion(),
            InsertSideEffectPolicy::current_buffer(),
        ),
        state(25, 13, 0, 0, 65, 45)
    );
}

#[test]
fn insert_state_shared_buffer_keeps_point_at_insert_and_shifts_begv_after_insert() {
    assert_eq!(
        insert_state_after_edit(
            state(20, 10, 28, 14, 60, 42),
            insertion(),
            InsertSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
        ),
        state(20, 10, 33, 17, 65, 45)
    );
}

#[test]
fn insert_state_shifts_zv_at_insert_position() {
    assert_eq!(
        insert_state_after_edit(
            state(0, 0, 0, 0, 20, 10),
            insertion(),
            InsertSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
        ),
        state(0, 0, 0, 0, 25, 13)
    );
}

#[test]
fn delete_state_current_buffer_maps_point_inside_range_to_deleted_start() {
    assert_eq!(
        delete_state_after_edit(
            state(28, 14, 0, 0, 60, 42),
            deleted_range(),
            DeleteSideEffectPolicy::current_buffer(),
        ),
        state(20, 10, 0, 0, 44, 34)
    );
}

#[test]
fn delete_state_keeps_point_at_deleted_start() {
    assert_eq!(
        delete_state_after_edit(
            state(20, 10, 0, 0, 60, 42),
            deleted_range(),
            DeleteSideEffectPolicy::current_buffer(),
        ),
        state(20, 10, 0, 0, 44, 34)
    );
}

#[test]
fn delete_state_shared_buffer_shifts_point_begv_and_zv() {
    assert_eq!(
        delete_state_after_edit(
            state(44, 24, 28, 14, 60, 42),
            deleted_range(),
            DeleteSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
        ),
        state(28, 16, 20, 10, 44, 34)
    );
}

#[test]
fn insert_and_delete_state_skip_update_when_policy_disables_state_fields() {
    let original = state(44, 24, 28, 14, 60, 42);

    assert_eq!(
        insert_state_after_edit(
            original,
            insertion(),
            InsertSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::RefreshFromStateMarkers,),
        ),
        original
    );
    assert_eq!(
        delete_state_after_edit(
            original,
            deleted_range(),
            DeleteSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::RefreshFromStateMarkers,),
        ),
        original
    );
}

#[test]
fn replace_state_maps_point_inside_deleted_range_to_replacement_end() {
    assert_eq!(
        replace_state(state(28, 14, 0, 0, 60, 42)),
        state(25, 13, 0, 0, 49, 37)
    );
}

#[test]
fn replace_state_keeps_point_at_deleted_start() {
    assert_eq!(
        replace_state(state(20, 10, 0, 0, 60, 42)),
        state(20, 10, 0, 0, 49, 37)
    );
}

#[test]
fn replace_state_maps_point_at_deleted_end_to_replacement_end() {
    assert_eq!(
        replace_state(state(36, 18, 0, 0, 60, 42)),
        state(25, 13, 0, 0, 49, 37)
    );
}

#[test]
fn replace_state_shifts_point_after_deleted_range_by_extent_delta() {
    assert_eq!(
        replace_state(state(44, 24, 0, 0, 60, 42)),
        state(33, 19, 0, 0, 49, 37)
    );
}

#[test]
fn replace_state_clamps_begv_inside_deleted_range_to_deleted_start() {
    assert_eq!(
        replace_state(state(0, 0, 28, 14, 60, 42)),
        state(0, 0, 20, 10, 49, 37)
    );
}

#[test]
fn replace_state_maps_begv_at_deleted_end_to_deleted_start() {
    assert_eq!(
        replace_state(state(0, 0, 36, 18, 60, 42)),
        state(0, 0, 20, 10, 49, 37)
    );
}

#[test]
fn replace_state_maps_zv_inside_deleted_range_to_replacement_end() {
    assert_eq!(
        replace_state(state(0, 0, 0, 0, 28, 14)),
        state(0, 0, 0, 0, 25, 13)
    );
}

#[test]
fn modification_tick_delta_is_logarithmic_and_never_zero() {
    assert_eq!(modification_tick_delta(CharLen::new(0)), 1);
    assert_eq!(modification_tick_delta(CharLen::new(1)), 1);
    assert_eq!(modification_tick_delta(CharLen::new(2)), 2);
    assert_eq!(modification_tick_delta(CharLen::new(3)), 2);
    assert_eq!(modification_tick_delta(CharLen::new(4)), 3);
    assert_eq!(modification_tick_delta(CharLen::new(8)), 4);
}

#[test]
fn same_len_edit_keeps_storage_and_modified_ranges_separate() {
    let edit = same_len_edit();

    assert_eq!(
        edit.storage_range(),
        TextEditRange::from_usize(0, 10, 0, 10)
    );
    assert_eq!(
        edit.modified_range(),
        TextEditRange::from_usize(3, 10, 3, 10)
    );
    assert_eq!(edit.changed_chars(), CharLen::new(7));
}

#[test]
fn same_len_substitution_plan_records_per_character_multibyte_ranges() {
    let range = TextEditRange::from_usize(0, "a日本日".len(), 0, 4);
    let plan = SameLenSubstitutionPlan::new(
        range,
        "a日本日".as_bytes(),
        true,
        '日' as u32,
        "本".as_bytes(),
    )
    .expect("matching chars should produce a substitution plan");

    assert_eq!(plan.replacement_bytes(), "a本本本".as_bytes());
    assert_eq!(
        plan.changed_ranges(),
        &[
            TextEditRange::from_usize(1, 4, 1, 2),
            TextEditRange::from_usize(7, 10, 3, 4),
        ]
    );
    assert_eq!(
        plan.first_to_last_changed_range(),
        TextEditRange::from_usize(1, 10, 1, 4)
    );
    assert_eq!(
        plan.replacement_for_range(range, true),
        TextReplacement::new(range, TextExtent::from_usize(4, "a本本本".len()))
    );
}

#[test]
fn same_len_substitution_plan_records_unibyte_ranges_and_rejects_non_bytes() {
    let range = TextEditRange::from_usize(20, 25, 10, 15);
    let plan = SameLenSubstitutionPlan::new(range, b"ababa", false, b'a' as u32, b"z")
        .expect("matching unibyte chars should produce a substitution plan");

    assert_eq!(plan.replacement_bytes(), b"zbzbz");
    assert_eq!(
        plan.changed_ranges(),
        &[
            TextEditRange::from_usize(20, 21, 10, 11),
            TextEditRange::from_usize(22, 23, 12, 13),
            TextEditRange::from_usize(24, 25, 14, 15),
        ]
    );
    assert_eq!(
        plan.first_to_last_changed_range(),
        TextEditRange::from_usize(20, 25, 10, 15)
    );
    assert!(SameLenSubstitutionPlan::new(range, b"ababa", false, 0x100, b"z").is_none());
    assert!(SameLenSubstitutionPlan::new(range, b"ababa", false, b'a' as u32, b"zz").is_none());
}

#[test]
fn same_len_substitution_plan_returns_none_without_matches() {
    let range = TextEditRange::from_usize(0, 5, 0, 5);

    assert!(SameLenSubstitutionPlan::new(range, b"abcde", true, b'z' as u32, b"q").is_none());
}

#[test]
fn transposition_storage_plan_swaps_outer_regions_over_full_span() {
    let transposition = TextTransposition::from_usize(2, 5, 1, 3, 8, 10, 5, 7);
    let plan = TranspositionStoragePlan::new(transposition, b"abc", b"XYZ", b"de");
    let span = TextEditRange::from_usize(2, 10, 1, 7);

    assert_eq!(plan.replacement_bytes(), b"deXYZabc");
    assert_eq!(
        plan.replacement(),
        TextReplacement::new(span, span.extent())
    );
    assert_eq!(plan.edit(), MeasuredSameLenEdit::covering(span));
}
