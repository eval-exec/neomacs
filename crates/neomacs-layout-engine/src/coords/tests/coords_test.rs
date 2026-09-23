use super::*;

#[test]
fn layout_char_position_boundary_is_lisp_one_based() {
    assert_eq!(layout_i64_char_pos_to_lisp_char_pos(0), LispCharPos1::ONE);
    assert_eq!(
        layout_i64_char_pos_to_lisp_char_pos(4),
        LispCharPos1::from_one_based_usize(5)
    );
}

#[test]
fn layout_char_position_boundary_clamps_negative_positions() {
    assert_eq!(layout_i64_char_pos_to_lisp_char_pos(-1), LispCharPos1::ONE);
}
