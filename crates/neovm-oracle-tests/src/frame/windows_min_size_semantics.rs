//! Oracle parity tests for GNU `frame-windows-min-size` semantics.
//!
//! GNU's C definition in `src/frame.c` is a temacs placeholder; after loadup,
//! `lisp/window.el` computes the minimum size from root and minibuffer windows.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_frame_windows_min_size_matches_loaded_window_el() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (frame-windows-min-size nil nil nil nil)
 (frame-windows-min-size nil t nil nil)
 (frame-windows-min-size nil nil nil t)
 (frame-windows-min-size nil t nil t)
 (condition-case err
     (frame-windows-min-size "not-frame" nil nil nil)
   (error (cons (car err) (cdr err)))))
"#;

    let expect =
        expect_test::expect![[r#""OK (8 10 5 10 (error \"not-frame is not a live frame\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
