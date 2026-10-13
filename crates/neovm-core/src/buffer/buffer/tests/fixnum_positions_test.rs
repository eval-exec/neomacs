//! Point, BEGV and ZV read from a live buffer convert to fixnums without a
//! range check; the result must equal their checked Lisp positions.

use super::*;
use crate::buffer::{EmacsBytePos, EmacsByteRange};
use crate::tagged::value::Fixnum;

#[test]
fn buffer_positions_convert_to_the_fixnums_of_their_lisp_positions() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("h\u{e9}llo world");
    buf.goto_emacs_byte_pos(EmacsBytePos::new(9));
    buf.narrow_to_emacs_byte_range(EmacsByteRange::from_usize(7, 12));
    for (position, lisp) in [
        (buf.point_position(), buf.point_lisp_char_pos()),
        (buf.point_min_position(), buf.point_min_lisp_char_pos()),
        (buf.point_max_position(), buf.point_max_lisp_char_pos()),
        (
            buf.emacs_byte_pos_to_position(EmacsBytePos::new(3)),
            buf.emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(3)),
        ),
    ] {
        assert_eq!(i64::from(Fixnum::from(position)), lisp.as_i64());
    }
}
