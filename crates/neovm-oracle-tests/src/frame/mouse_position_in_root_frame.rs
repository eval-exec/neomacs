//! GNU parity for `mouse-position-in-root-frame`.
//!
//! GNU `Fmouse_position_in_root_frame` (`src/frame.c:7010-7039`) takes the
//! value of `mouse-position` (after `mouse-position-function`) and adds the
//! frame's `left_pos`/`top_pos` plus every parent frame's.  On a mouseless
//! batch frame `mouse-position` is `(FRAME nil . nil)`; the reference build's
//! `XFIXNUM` reads nil as zero, so the frame offsets remain observable.
//! `mouse-drag-line` uses this primitive on a tty (`lisp/mouse.el:862-871`).
//!
//! Not covered here: a non-cons cdr makes the reference build segfault, so
//! Neomacs deliberately answers nil instead (unit-tested), and a deleted
//! frame has no Neomacs geometry to report (also unit-tested).

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn mouse_position_in_root_frame_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(list
  (mouse-position-in-root-frame)
  (let ((mouse-position-function
         (lambda (_pos) (cons (selected-frame) (cons 19 45)))))
    (mouse-position-in-root-frame))
  (let ((mouse-position-function
         (lambda (_pos) (cons (selected-frame) (cons nil 45)))))
    (mouse-position-in-root-frame))
  (let ((mouse-position-function
         (lambda (_pos) (cons (selected-frame) (cons 19 nil)))))
    (mouse-position-in-root-frame))
  (let ((mouse-position-function
         (lambda (_pos) (cons 42 (cons 1 2)))))
    (mouse-position-in-root-frame)))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq("((0 . 0) (19 . 45) (0 . 45) (19 . 0) nil)", &gnu, &neomacs);
}
