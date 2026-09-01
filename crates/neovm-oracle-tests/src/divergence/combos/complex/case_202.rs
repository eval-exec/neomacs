//! Complex combo batch 202 — `button` deep: make-button, insert-button,
//! button-at, next-button, previous-button, button-activate, button-get.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx202_button_make_and_query_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "Some text content here")
      (make-button 6 10 'action (lambda (b) (message "clicked"))
                   'help-echo "Click"
                   'face 'link
                   'mouse-face 'highlight)
      (let ((btn (button-at 7)))
        (list (buttonp btn)
              (when btn (button-start btn))
              (when btn (button-end btn))
              (when btn (button-get btn 'help-echo))
              (when btn (button-get btn 'face))
              (length (overlays-in 1 20)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_insert_button_with_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert-button "Click Me"
                      'action (lambda (_) (message "hi"))
                      'face 'link
                      'help-echo "Click to activate")
      (list (buffer-string)
            (length (overlays-in 1 20))
            (buttonp (button-at 1))
            (button-get (button-at 1) 'help-echo)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_next_previous_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 16 26 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "text one text two text three text four")
      (make-button 6 9)
      (make-button 16 19)
      (make-button 26 31)
      (goto-char 1)
      (let ((b1 (next-button (point))))
        (let ((b2 (when b1 (next-button (button-start b1)))))
          (let ((b3 (when b2 (next-button (button-start b2)))))
            (let ((back (when b3 (previous-button (button-start b3)))))
              (list (and b1 (button-start b1))
                    (and b2 (button-start b2))
                    (and b3 (button-start b3))
                    (and back (button-start back))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_at_edge_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "0123456789")
      (make-button 3 7 'face 'bold)
      (list (buttonp (button-at 2))
            (buttonp (button-at 3))
            (buttonp (button-at 5))
            (buttonp (button-at 6))
            (buttonp (button-at 7))
            (button-start (button-at 3))
            (button-end (button-at 3))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_category_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "clickable text")
      (make-button 1 9 'category 'neo-cx202-btn)
      (put 'neo-cx202-btn 'face 'link)
      (put 'neo-cx202-btn 'mouse-face 'highlight)
      (let ((btn (button-at 1)))
        (list (button-get btn 'category)
              (button-get btn 'face))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_delete_overlay_removes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "0123456789")
      (make-button 3 7 'face 'bold)
      (let ((btn (button-at 3)))
        (delete-overlay btn)
        (list (buttonp (button-at 3))
              (length (overlays-in 1 10)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_with_multiple_in_same_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 \"alpha\" \"beta\" \"gamma\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma delta epsilon")
      (make-button 1 5 'face 'link 'help-echo "alpha")
      (make-button 7 10 'face 'link 'help-echo "beta")
      (make-button 13 17 'face 'link 'help-echo "gamma")
      (list (length (overlays-in 1 30))
            (button-get (button-at 1) 'help-echo)
            (button-get (button-at 7) 'help-echo)
            (button-get (button-at 13) 'help-echo)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_overlay_face_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (link highlight \"Click\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "link text here")
      (let ((ov (make-overlay 1 5)))
        (overlay-put ov 'face 'link)
        (overlay-put ov 'mouse-face 'highlight)
        (overlay-put ov 'button t)
        (overlay-put ov 'help-echo "Click")
        (list (get-char-property 1 'face)
              (get-char-property 1 'mouse-face)
              (get-char-property 1 'help-echo)
              (overlay-get ov 'button))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_edit_button_label() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"click here for more\" 1 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "click here for more")
      (make-button 1 5 'face 'bold)
      (list (buffer-string)
            (button-start (button-at 1))
            (button-end (button-at 1))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx202_button_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Button mega test buffer content here")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (make-button 5 9 'action (lambda (_) :clicked) 'face 'link)
        (let ((btn (button-at 6)))
          (let ((state (list (buttonp btn)
                             (when btn (button-start btn))
                             (when btn (button-end btn))
                             (when btn (button-get btn 'face))
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
