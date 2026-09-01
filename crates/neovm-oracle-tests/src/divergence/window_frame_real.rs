//! Divergence tests: real window/frame behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((w (selected-window)))
  (list (windowp w)
        (window-live-p w)
        (>= (window-height w) 0)
        (>= (window-width w) 0)
        (>= (window-body-height w) 0)
        (>= (window-body-width w) 0)
        (eq w (selected-window)))) ",
        expect,
    );
}

#[test]
fn divergence_window_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((w (selected-window))
        (edges (window-edges w))
        (body-edges (window-body-edges w)))
  (list (length edges)
        (>= (nth 2 edges) (nth 0 edges))
        (>= (nth 3 edges) (nth 1 edges))
        (>= (nth 2 body-edges) (nth 0 body-edges)))) ",
        expect,
    );
}

#[test]
fn divergence_window_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((w (selected-window)))
  (list (bufferp (window-buffer w))
        (eq (window-buffer w) (current-buffer))
        (integer-or-marker-p (window-point w))
        (= (window-point w) (point)))) ",
        expect,
    );
}

#[test]
fn divergence_window_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert (make-string 500 ?x))
  (let ((w (selected-window)))
    (list (integer-or-marker-p (window-start w))
          (integer-or-marker-p (window-end w))
          (>= (window-end w) (window-start w))))) ",
        expect,
    );
}

#[test]
fn divergence_frame_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((f (selected-frame)))
  (list (framep f)
        (frame-live-p f)
        (stringp (frame-parameter f 'name))
        (integerp (frame-parameter f 'height))
        (integerp (frame-parameter f 'width)))) ",
        expect,
    );
}

#[test]
fn divergence_window_configuration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((wc (current-window-configuration)))
  (list (window-configuration-p wc)
        (set-window-configuration wc)
        (eq (window-configuration-frame wc) (selected-frame)))) ",
        expect,
    );
}

#[test]
fn divergence_minibuffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((mw (minibuffer-window)))
  (list (windowp mw)
        (bufferp (window-buffer mw))
        (window-live-p mw)
        (minibufferp (window-buffer mw)))) ",
        expect,
    );
}

#[test]
fn divergence_window_parent_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function window-children)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((root (frame-root-window))
        (children (window-children root)))
  (list (windowp root)
        (not (window-parent root))
        (listp children)
        (window-valid-p root))) ",
        expect,
    );
}

#[test]
fn divergence_split_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((w1 (selected-window))
        (w2 (split-window nil nil 'right)))
  (list (windowp w2)
        (not (eq w1 w2))
        (window-live-p w2)
        (delete-window w2)
        (eq (selected-window) w1))) ",
        expect,
    );
}

#[test]
fn divergence_focused_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((f (selected-frame)))
  (list (eq f (terminal-live-p (frame-terminal f)))
        (framep f)
        (> (length (frame-list)) 0))) ",
        expect,
    );
}
