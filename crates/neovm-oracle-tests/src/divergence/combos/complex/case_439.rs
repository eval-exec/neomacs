//! Complex combo batch 439 — 15 multi-divergence interaction probes combining
//! known broken areas: display+eight-bit+overlay, case-fold+coding+time,
//! set-buffer-multibyte+encode-coding+process, overlay-lists+display+column,
//! string-collate+buffer-local+error, make-frame+process+display,
//! encode-time+string-collate+case-fold, detect-coding+charset+split-char,
//! current-message+format-message+error, features+load+provide,
//! posn-at-point+display+invisible, face-attribute+font+frame.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// display + eight-bit + overlay: column accounting with raw bytes.
#[test]
fn div_cx439_display_eightbit_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 \"\\200\\201AB\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string #x80 #x81 65 66))
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'display "XX"))
  (set-buffer-multibyte t)
  (list (current-column)
        (buffer-string)
        (length (buffer-string))))"##,
        expect,
    );
}

/// case-fold + coding + time: Greek case-fold with coding roundtrip.
#[test]
fn div_cx439_casefold_coding_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"ΠΡΣΤΥ\" 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (string-match "πρστυ" "ΠΡΣΤΥ")
        (encode-coding-string "ΠΡΣΤΥ" 'utf-8)
        (string-bytes (encode-coding-string "πρστυ" 'utf-8))))"##,
        expect,
    );
}

/// set-buffer-multibyte + encode-coding + process data loss.
#[test]
fn div_cx439_multibyte_encode_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 \"��ABC\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66 67))
  (let ((data (buffer-string)))
    (set-buffer-multibyte t)
    (list (length data)
          (length (buffer-string))
          (encode-coding-string data 'utf-8))))"##,
        expect,
    );
}

/// overlay-lists + display + column: overlay position tracking.
#[test]
fn div_cx439_overlay_lists_display_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 9 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcd efgh")
  (let ((o1 (make-overlay 2 5)) (o2 (make-overlay 6 9)))
    (overlay-put o1 'display "XXX")
    (overlay-put o2 'face 'bold)
    (list (length (car (overlay-lists)))
          (current-column)
          (progn (goto-char 4) (current-column)))))"##,
        expect,
    );
}

/// string-collate + buffer-local + error: locale + local vars.
#[test]
fn div_cx439_string_collate_local_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((s1 (make-local-variable 'neo-cx439-loc)))
    (setq neo-cx439-loc "test")
    (list (condition-case e (string-collate-lessp "ä" "z") (error (car e)))
          (buffer-local-value 'neo-cx439-loc (current-buffer)))))"##,
        expect,
    );
}

/// make-frame + process + display: frame creation with process backend.
#[test]
fn div_cx439_frame_process_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error #<process np> nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-frame '((name . "test"))) (error (car e)))
      (condition-case e (make-network-process :name "np" :server t :service 0) (error (car e)))
      (display-color-p))"##,
        expect,
    );
}

/// encode-time + string-collate + case-fold: time locale interaction.
#[test]
fn div_cx439_time_locale_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((501485566126885707972608 . 0) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (condition-case e (encode-time 30.5 30 14 16 6 2026 nil) (error (car e)))
        (string-collate-lessp "a" "B")
        (char-equal ?π ?Π)))"##,
        expect,
    );
}

/// detect-coding + charset + split-char: coding system analysis.
#[test]
fn div_cx439_detect_coding_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((undecided) (ascii 97) (unicode-bmp 0 233) unicode-bmp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (detect-coding-string "abc")
      (condition-case e (split-char ?a) (error (car e)))
      (condition-case e (split-char ?é) (error (car e)))
      (condition-case e (char-charset ?é) (error (car e))))"##,
        expect,
    );
}

/// current-message + format-message + error: message pipeline.
#[test]
fn div_cx439_message_format_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"‘quoted’\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (message "test %s" "msg")
  (list (current-message)
        (format-message "`%s'" "quoted")
        (condition-case e (replace-regexp-in-string "x" "\\1" "x") (error (cadr e)))))"##,
        expect,
    );
}

/// features + load + provide: tracking loaded features.
#[test]
fn div_cx439_features_load_provide() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((sym (make-symbol "neo-cx439-feat"))
      (before features))
  (provide sym)
  (list (featurep sym)
        (eq (car features) sym)
        (eq (cdr features) before)
        (and (memq sym features) t)
        (listp features)))"##,
        expect,
    );
}

/// posn-at-point + display + invisible: position under visual changes.
#[test]
fn div_cx439_posn_display_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (put-text-property 3 4 'display "XXXX")
  (put-text-property 8 10 'invisible t)
  (condition-case e
      (list (posn-at-point 3) (posn-at-point 8))
    (error (car e))))"##,
        expect,
    );
}

/// face-attribute + font + frame: face resolution chain.
#[test]
fn div_cx439_face_attribute_font_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'bold :weight nil 'default)
      (face-attribute 'italic :slant nil 'default)
      (fontp (face-font 'default)))"##,
        expect,
    );
}

/// buffer-local-variables + overlay-local + marker: state tracking.
#[test]
fn div_cx439_buffer_local_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((neo-cx439-v . val) 1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((v (make-local-variable 'neo-cx439-v)))
    (setq neo-cx439-v 'val)
    (insert "abcde")
    (let ((ov (make-overlay 2 4))) (overlay-put ov 'face 'bold))
    (let ((m (set-marker (make-marker) 3)))
      (let ((locals (buffer-local-variables)))
        (list (assq 'neo-cx439-v locals)
              (length (overlays-in 1 10))
              (marker-position m))))))"##,
        expect,
    );
}

/// time-add + time-subtract + float-time: time arithmetic stack.
#[test]
fn div_cx439_time_arithmetic_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1704088800 86400)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (list (condition-case e (time-add t1 (seconds-to-time 3600)) (error (car e)))
        (condition-case e (time-subtract (time-add t1 (seconds-to-time 86400)) t1) (error (car e)))))"##,
        expect,
    );
}

/// format-message + error-condition + signal: error message quoting.
#[test]
fn div_cx439_format_message_error_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"‘hello’ ‘world’\" car \"Invalid use of ‘\\\\’ in replacement text\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-message "`hello' `world'")
      (condition-case e (car 1 2 3) (error (cadr e)))
      (condition-case e (replace-regexp-in-string "x" "\\g<bad>" "x") (error (cadr e))))"##,
        expect,
    );
}
