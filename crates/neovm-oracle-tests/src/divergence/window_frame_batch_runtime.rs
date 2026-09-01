//! Window/frame query parity in batch (Rust layout engine): selected-window/
//! frame liveness, window-buffer/point/start/end, window width/height/edges/
//! body-width/total-*, frame-parameters + get/set, get-buffer-window,
//! minibuffer-window, set-window-buffer.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn frame_basic_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (framep (selected-frame)) (frame-live-p (selected-frame))
        (>= (length (frame-list)) 1) (integerp (frame-width)) (integerp (frame-height))
        (windowp (frame-selected-window)))"##,
        expect,
    );
}

#[test]
fn frame_parameter_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (val42 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (set-frame-parameter nil 'neo-test-param-xyz 'val42)
  (list (frame-parameter nil 'neo-test-param-xyz)
        (frame-parameter nil 'nonexistent-param-xyz)))"##,
        expect,
    );
}

#[test]
fn frame_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (name . \"F1\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (frame-parameters)))
  (list (listp p) (assq 'name p) (consp (assq 'width p))))"##,
        expect,
    );
}

#[test]
fn get_buffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (eq (get-buffer-window (current-buffer)) (selected-window))
        (windowp (get-buffer-window))
        (null (get-buffer-window (generate-new-buffer " neo-nowin-xxx"))))"##,
        expect,
    );
}

#[test]
fn minibuffer_window_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (windowp (minibuffer-window))
        (window-live-p (minibuffer-window))
        (not (eq (minibuffer-window) (selected-window))))"##,
        expect,
    );
}

#[test]
fn window_basic_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (windowp (selected-window)) (window-live-p (selected-window))
        (bufferp (window-buffer)) (integerp (window-width)) (integerp (window-height))
        (= (length (window-list)) 1))"##,
        expect,
    );
}

#[test]
fn window_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b (generate-new-buffer " neo-wbs-xxx")))
  (set-window-buffer (selected-window) b)
  (prog1 (list (eq (window-buffer) b) (eq (current-buffer) b))
    (set-window-buffer (selected-window) (other-buffer))
    (kill-buffer b)))"##,
        expect,
    );
}

#[test]
fn window_edges_geom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (= (length (window-edges)) 4)
        (integerp (window-total-width)) (integerp (window-total-height))
        (integerp (window-body-width)) (booleanp (window-minibuffer-p)))"##,
        expect,
    );
}

#[test]
fn window_point_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-current-buffer (window-buffer)
  (list (integerp (window-point)) (integerp (window-start))
        (markerp (copy-marker (window-point)))))"##,
        expect,
    );
}

#[test]
fn window_scroll_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (dotimes (i 100) (insert (format "line %d\n" i)))
  (set-window-buffer (selected-window) (current-buffer))
  (list (integerp (window-start)) (integerp (window-end nil t))
        (>= (window-end) (window-start))))"##,
        expect,
    );
}
