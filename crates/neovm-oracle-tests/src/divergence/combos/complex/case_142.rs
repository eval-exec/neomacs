//! Complex combo batch 142 — `button` / `link` / `browse-url` /
//! `goto-address` / `ffap` (find file at point) interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx142_button_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'button)
      (list (fboundp 'make-button)
            (fboundp 'insert-button)
            (fboundp 'button-activate)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_make_button_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "Some text content here")
      (make-button 6 10 'action (lambda (b) (message "clicked")) 'help-echo "Click")
      (let ((btn (button-at 7)))
        (list (buttonp btn)
              (when btn (button-start btn))
              (when btn (button-end btn))
              (when btn (button-get btn 'help-echo))
              (length (overlays-in 1 20)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_insert_button_creates_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert-button "Click Me" 'action (lambda (_) (message "hi"))
                                    'face 'link
                                    'help-echo "Click")
      (list (buffer-string)
            (length (overlays-in 1 20))
            (buttonp (button-at 1))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_browse_url_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'browse-url)
      (list (fboundp 'browse-url)
            (fboundp 'browse-url-at-point)
            (boundp 'browse-url-browser-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_goto_address_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'goto-addr)
      (list (fboundp 'goto-address)
            (fboundp 'goto-address-at-point)
            (boundp 'goto-address-url-face)
            (boundp 'goto-address-mail-face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_ffap_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ffap)
      (list (fboundp 'ffap)
            (fboundp 'find-file-at-point)
            (boundp 'ffap-url-regexp)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_thing_at_point_url_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"https://example.com/path\" (5 . 29) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "see https://example.com/path for details")
  (goto-char 5)
  (let ((url (thing-at-point 'url)))
    (list url
          (bounds-of-thing-at-point 'url)
          (stringp url))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_thing_at_point_email() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"user@example.com\" t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "contact user@example.com for info")
  (goto-char 10)
  (let ((email (thing-at-point 'email)))
    (list email
          (stringp email)
          (when email (string-match "@" email)))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_thing_at_point_filename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/home/user/file.txt\" t (6 . 25))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "edit /home/user/file.txt for changes")
  (goto-char 6)
  (let ((fname (thing-at-point 'filename)))
    (list fname
          (stringp fname)
          (bounds-of-thing-at-point 'filename))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_button_next_previous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
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
          (list (buttonp b1)
                (buttonp b2)
                (when b1 (button-start b1))
                (when b2 (button-start b2))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_link_overlay_face_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (link highlight \"Click to open\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "see https://example.com end")
      (let ((ov (make-overlay 5 23)))
        (overlay-put ov 'face 'link)
        (overlay-put ov 'mouse-face 'highlight)
        (overlay-put ov 'help-echo "Click to open")
        (list (get-char-property 8 'face)
              (get-char-property 8 'mouse-face)
              (get-char-property 8 'help-echo))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx142_button_with_marker_overlay_undo_narrow_mega() {
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
        (make-button 5 9 'action (lambda (_) :clicked))
        (let ((btn (button-at 6)))
          (let ((state (list (buttonp btn)
                             (when btn (button-start btn))
                             (when btn (button-end btn))
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
