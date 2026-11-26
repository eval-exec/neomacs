//! Scalar observations for cold serial setup in the restricted-executor tests.

use crate::tagged::value::TaggedValue;

/// The exclusive host borrow confines this observation to the serial fixture
/// phase, outside any admitted owner session. Return no payload reference.
pub(super) fn serial_table_storage_bytes(
    _host: &mut impl super::HostParts,
    table: TaggedValue,
) -> usize {
    table
        .as_hash_table()
        .expect("the serial fixture keeps its table rooted")
        .data
        .known_storage_bytes()
}
