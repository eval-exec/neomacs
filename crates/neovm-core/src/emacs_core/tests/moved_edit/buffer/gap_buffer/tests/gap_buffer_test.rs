use neovm_edit::buffer::gap_buffer::GapBuffer;
use super::*;

use neovm_edit::buffer::{CharLen, CharPos0, EmacsBytePos, EmacsByteRange};

#[test]
fn text_range_spanning_gap() {
    neovm_edit::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    buf.move_gap_to(5);
    // Range spans the gap.
    assert_eq!(gap_text_range(&buf, 3, 8), "lo wo");
}

fn gap_text_range(buf: &GapBuffer, start: usize, end: usize) -> String {
    buf.text_emacs_byte_range(EmacsByteRange::from_usize(start, end))
}
