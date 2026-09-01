//! Complex combo batch 448 — 15 random cross-feature interaction tests
//! combining unrelated features to surface emergent divergences:
//! calc+process+display, url+case-fold+time, info+overlay+string-collate,
//! woman+font+face, diff+marker+undo, vc+coding+detect, hanoi+eight-bit,
//! life+column+display, rot13+multibyte+encode, zone+buffer-local,
//! doctor+case-table+char-fold, copyright+bidi+regex, calc+network+stub,
//! url+overlay-lists+split-char, info+features+provide.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// calc + display column + process exit status.
#[test]
fn div_cx448_calc_display_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"5\" #<process cx448>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'calc)
  (list (calc-eval "2+3")
        (condition-case e (make-process :name "cx448" :command '("echo" "hi") :connection-type 'pipe :buffer nil) (error (car e)))))"##,
        expect,
    );
}

/// url + case-fold + time.
#[test]
fn div_cx448_url_casefold_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"http\" (501485566126885707972608 . 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url-parse)
  (let ((case-fold-search t))
    (list (url-type (url-generic-parse-url "HTTP://EXAMPLE.COM"))
          (condition-case e (encode-time 30.5 30 14 16 6 2026 nil) (error (car e))))))"##,
        expect,
    );
}

/// info-lookup + overlay + string-collate.
#[test]
fn div_cx448_info_overlay_collate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'info-look)
  (with-temp-buffer
    (insert "info text")
    (let ((ov (make-overlay 1 5)))
      (overlay-put ov 'face 'bold)
      (list (length (overlays-in 1 10))
            (string-collate-lessp "a" "B")))))"##,
        expect,
    );
}

/// woman-browse + font-lock + face.
#[test]
fn div_cx448_woman_font_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'woman)
  (list (fboundp 'woman)
        (face-attribute 'bold :weight nil 'default)))"##,
        expect,
    );
}

/// diff-mode + marker + undo.
#[test]
fn div_cx448_diff_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'diff)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "a\nb\nc\n")
    (let ((m (set-marker (make-marker) 2)))
      (delete-region 1 3)
      (undo)
      (marker-position m))))"##,
        expect,
    );
}

/// vc + coding + detect-coding.
#[test]
fn div_cx448_vc_coding_detect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (undecided))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'vc)
  (list (boundp 'vc-handled-backends)
        (detect-coding-string "hello")))"##,
        expect,
    );
}

/// hanoi + eight-bit + multibyte.
#[test]
fn div_cx448_hanoi_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'hanoi)
  (let ((raw (unibyte-string 200 201 65 66)))
    (list (fboundp 'hanoi)
          (string-bytes raw)
          (length raw))))"##,
        expect,
    );
}

/// life + column + display.
#[test]
fn div_cx448_life_column_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'life)
  (with-temp-buffer
    (insert "abc")
    (put-text-property 2 3 'display "XX")
    (list (fboundp 'life) (current-column))))"##,
        expect,
    );
}

/// rot13 + multibyte + encode.
#[test]
fn div_cx448_rot13_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"uryyb\" \"café\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (rot13 "hello")
      (encode-coding-string "café" 'utf-8))"##,
        expect,
    );
}

/// zone + buffer-local + setq-local.
#[test]
fn div_cx448_zone_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t zone-val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'zone)
  (with-temp-buffer
    (setq-local neo-cx448-z 'zone-val)
    (list (fboundp 'zone) neo-cx448-z)))"##,
        expect,
    );
}

/// copyright + case-fold + char-equal.
#[test]
fn div_cx448_copyright_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'copyright)
  (let ((case-fold-search t))
    (list (fboundp 'copyright-update)
          (char-equal ?π ?Π))))"##,
        expect,
    );
}

/// doctor + buffer-local-variables + string-collate.
#[test]
fn div_cx448_doctor_local_collate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'doctor)
  (with-temp-buffer
    (let ((locals (buffer-local-variables)))
      (list (boundp 'doctor-doctors)
            (string-collate-lessp "a" "B")
            (length locals)))))"##,
        expect,
    );
}

/// calc + detect-coding + charset-priority.
#[test]
fn div_cx448_calc_coding_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"6\" (undecided) 179)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'calc)
  (list (calc-eval "2*3")
        (detect-coding-string "abc")
        (length (charset-priority-list))))"##,
        expect,
    );
}

/// url-parse + overlay-lists + split-char.
#[test]
fn div_cx448_url_overlay_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"https\" 1 (ascii 65))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url-parse)
  (with-temp-buffer
    (insert "abc")
    (let ((o (make-overlay 1 3)))
      (overlay-put o 'face 'bold)
      (list (url-type (url-generic-parse-url "https://test.com"))
            (length (car (overlay-lists)))
            (condition-case e (split-char ?A) (error (car e)))))))"##,
        expect,
    );
}

/// info + features + provide.
#[test]
fn div_cx448_info_features_provide() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'info)
  (let ((sym (make-symbol "neo-cx448-f")))
    (provide sym)
    (list (fboundp 'info)
          (featurep sym)
          (listp features))))"##,
        expect,
    );
}
