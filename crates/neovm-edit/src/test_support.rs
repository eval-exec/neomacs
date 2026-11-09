//! Test-only fence surface for core-owned tests of the moved edit tests.
pub mod authority;

/// Byte offset of `combine_after_change_buffer` inside `EditRuntime`.
/// The field stays private. This is a layout read, not a write.
pub const fn offset_of_combine_after_change_buffer() -> usize {
    std::mem::offset_of!(crate::buffer::EditRuntime, combine_after_change_buffer)
}

/// Byte offset of `combine_after_change_list` inside `EditRuntime`.
/// The field stays private. This is a layout read, not a write.
pub const fn offset_of_combine_after_change_list() -> usize {
    std::mem::offset_of!(crate::buffer::EditRuntime, combine_after_change_list)
}

/// Byte offset of `last_overlay_modification_hooks` inside `EditRuntime`.
/// The field stays private. This is a layout read, not a write.
pub const fn offset_of_last_overlay_modification_hooks() -> usize {
    std::mem::offset_of!(crate::buffer::EditRuntime, last_overlay_modification_hooks)
}

/// Byte offset of `interval_insert_behind_hooks` inside `EditRuntime`.
/// The field stays private. This is a layout read, not a write.
pub const fn offset_of_interval_insert_behind_hooks() -> usize {
    std::mem::offset_of!(crate::buffer::EditRuntime, interval_insert_behind_hooks)
}

/// Byte offset of `interval_insert_in_front_hooks` inside `EditRuntime`.
/// The field stays private. This is a layout read, not a write.
pub const fn offset_of_interval_insert_in_front_hooks() -> usize {
    std::mem::offset_of!(crate::buffer::EditRuntime, interval_insert_in_front_hooks)
}
