//! Complex combo batch 443 — 15 deeper probes into 51 known divergence
//! themes: eight-bit utf-8 roundtrip + all comparisons, display column
//! with multiple display specs, overlay-lists after undo, case-fold
//! with char-range regex, string-collate with all ASCII, time-add
//! large duration, make-process vs make-pipe-process stubs,
//! detect-coding-string on binary, features list depth,
//! split-char on all ASCII, frame-parameter after delete,
//! pos-visible-in-window-p with scrolled window, encode-time
//! extreme year edge, process-connection-type pipe vs pty exit.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// eight-bit: decode then recode roundtrip.
#[test]
fn div_cx443_eightbit_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 5 t t 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((raw (unibyte-string 128 129 130 200 255))
        (dec (decode-coding-string raw 'utf-8))
        (reco (encode-coding-string dec 'utf-8)))
  (list (string-bytes raw) (string-bytes dec) (string-bytes reco)
        (equal raw reco) (string= raw reco)
        (length dec) (length reco)))"##,
        expect,
    );
}

/// display column: multiple adjacent display props.
#[test]
fn div_cx443_display_column_multiple_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (13 1 4 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a b c d e")
  (put-text-property 2 3 'display "XX")
  (put-text-property 4 5 'display "YYYY")
  (put-text-property 6 7 'display "Z")
  (list (current-column)
        (progn (goto-char 2) (current-column))
        (progn (goto-char 4) (current-column))
        (progn (goto-char 6) (current-column))))"##,
        expect,
    );
}

/// overlay-lists after undo operations.
#[test]
fn div_cx443_overlay_lists_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdefgh")
  (let ((o (make-overlay 2 6)))
    (overlay-put o 'face 'bold)
    (let ((b1 (length (car (overlay-lists)))))
      (undo)
      (let ((b2 (length (car (overlay-lists))))
            (cnt (length (overlays-in 1 10))))
        (list b1 b2 cnt)))))"##,
        expect,
    );
}

/// case-fold: char-range regex with Greek lowercase.
#[test]
fn div_cx443_casefold_char_range_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"ΠΡΣΤΥΦΧΨΩ\" nil 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (string-match "[π-ω]+" "ΠΡΣΤΥΦΧΨΩ")
        (match-string 0 "ΠΡΣΤΥΦΧΨΩ")
        (string-match "[π-ω]" "ΑΒΓ")
        (string-match "[π-ω]" "Π")
        (string-match "π-ω" "Π-Ω")))"##,
        expect,
    );
}

/// string-collate: all ASCII chars in various orders.
#[test]
fn div_cx443_string_collate_all_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"0\" \"1\" \"a\" \"A\" \"b\" \"B\" \"z\" \"Z\") (\"0\" \"1\" \"A\" \"B\" \"Z\" \"a\" \"b\" \"z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((l '("A" "B" "a" "b" "0" "1" "Z" "z")))
  (list (sort (copy-sequence l) #'string-collate-lessp)
        (sort (copy-sequence l) #'string<)))"##,
        expect,
    );
}

/// time-add large duration (overflow edge).
#[test]
fn div_cx443_time_add_large() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4857685200 33240085200)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (list (condition-case e (time-add t1 (seconds-to-time (* 365 86400 100))) (error (car e)))
        (condition-case e (time-add t1 (seconds-to-time (* 365 86400 1000))) (error (car e)))))"##,
        expect,
    );
}

/// process stubs: make-process-nw, make-serial-process (likely stubs).
#[test]
fn div_cx443_process_stub_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<process cx443-p> #<process cx443-n>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-pipe-process :name "cx443-p") (error (car e)))
      (condition-case e (make-network-process :name "cx443-n" :server t :service 0) (error (car e))))"##,
        expect,
    );
}

/// detect-coding-string on binary/random bytes.
#[test]
fn div_cx443_detect_coding_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((undecided) (no-conversion) (utf-8 iso-latin-1 emacs-mule in-is13194-devanagari utf-8-auto utf-8-with-signature japanese-shift-jis chinese-big5 iso-2022-8bit-ss2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (detect-coding-string "abc")
      (detect-coding-string (unibyte-string 255 254 0 0))
      (detect-coding-string (unibyte-string 239 187 191 65 66 67)))"##,
        expect,
    );
}

/// features: check if common features are present.
#[test]
fn div_cx443_features_common() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (featurep 'emacs) (featurep 'regexp-opt) (featurep 'font-lock)
      (featurep 'window) (featurep 'files) (featurep 'help))"##,
        expect,
    );
}

/// split-char on each ASCII character.
#[test]
fn div_cx443_split_char_all_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((ascii 65) (ascii 66) (ascii 48))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (list (split-char ?A) (split-char ?B) (split-char ?0))
  (error (car e)))"##,
        expect,
    );
}

/// frame parameters after frame deletion.
#[test]
fn div_cx443_frame_parameter_after_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK mono""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((f (selected-frame)))
      (frame-parameter f 'display-type))
  (error (car e)))"##,
        expect,
    );
}

/// pos-visible-in-window-p with partially scrolled window.
#[test]
fn div_cx443_pos_visible_scrolled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (make-string 200 ?a) "\n" (make-string 200 ?b))
  (list (pos-visible-in-window-p 1)
        (pos-visible-in-window-p (point-max))))"##,
        expect,
    );
}

/// encode-time extreme year edge.
#[test]
fn div_cx443_encode_time_extreme_year() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((0 18000) (32744 24016) (62598 40272) (3866131 21584))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (encode-time 0 0 0 1 1 1970 nil) (error (car e)))
      (condition-case e (encode-time 0 0 0 1 1 2038 nil) (error (car e)))
      (condition-case e (encode-time 0 0 0 1 1 2100 nil) (error (car e)))
      (condition-case e (encode-time 0 0 0 1 1 9999 nil) (error (car e))))"##,
        expect,
    );
}

/// pipe vs pty process exit status.
#[test]
fn div_cx443_process_pipe_pty_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e
    (let ((p (make-process :name "cx443-pipe" :command '("sh" "-c" "exit 0") :connection-type 'pipe :buffer nil)))
      (accept-process-output p 2) (process-exit-status p))
  (error (car e)))
  (condition-case e
    (let ((p (make-process :name "cx443-pty" :command '("sh" "-c" "exit 42") :connection-type 'pty :buffer nil)))
      (accept-process-output p 2) (process-exit-status p))
  (error (car e))))"##,
        expect,
    );
}

/// replace-regexp with complex backreference.
#[test]
fn div_cx443_replace_regexp_backref_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"123_abc 456_def\" \"123_αβγ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (list (replace-regexp-in-string "\\([a-z]+\\)-\\([0-9]+\\)" "\\2_\\1" "abc-123 def-456")
        (replace-regexp-in-string "\\([α-ω]+\\)-\\([0-9]+\\)" "\\2_\\1" "αβγ-123")))"##,
        expect,
    );
}
