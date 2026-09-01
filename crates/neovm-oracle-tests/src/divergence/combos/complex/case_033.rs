//! Complex combo batch 33 — char-width/syntax in unibyte, sentinel-collision
//! range post-fix, remaining process/coding/timer combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx33_char_width_high_codepoint_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 1 2 2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (mapcar #'char-width (list ?a ?A ?1 #x3042 #x4e2d #x1f600)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_format_c_sentinel_range_post_fix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (57472 57504 57599 58112 58367)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (aref (format "%c" #xe080) 0)
      (aref (format "%c" #xe0a0) 0)
      (aref (format "%c" #xe0ff) 0)
      (aref (format "%c" #xe300) 0)
      (aref (format "%c" #xe3ff) 0))
"##,
        expect,
    );
}

#[test]
fn div_cx33_syntax_after_in_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2) (2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert "ab")
  (list (syntax-after 1) (syntax-after 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_string_make_unibyte_data_loss_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((3 (97 98 99)) (4 (99 97 102 233)) (2 (22 76)) (1 (0)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (let ((u (string-make-unibyte s)))
            (list (length u) (append u nil))))
        (list "abc" "café" "世界" "😀"))
"##,
        expect,
    );
}

#[test]
fn div_cx33_process_kill_query_off_then_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (run open listen connect stop) run)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx33-qo*")))
  (let ((p (make-process :name "neo-cx33-qo" :command '("sleep" "10")
                         :buffer buf)))
    (accept-process-output p 0.1)
    (set-process-query-on-exit-flag p nil)
    (kill-buffer buf)
    (list (buffer-live-p buf) (process-live-p p) (process-status p))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_coding_system_for_write_doesnt_propagate_to_subprocess() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\\r\\n\" 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coding-system-for-write 'utf-8-dos))
  (with-temp-buffer
    (call-process "printf" nil t nil "café\n")
    (list (buffer-string) (string-bytes (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_timer_cancel_all_after_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timers (list (run-with-timer 100 nil (lambda ()))
                     (run-with-timer 200 nil (lambda ()))
                     (run-with-idle-timer 100 nil (lambda ())))))
  (let ((active (length timer-list))
        (idle (length timer-idle-list)))
    (mapc #'cancel-timer timers)
    (list active idle (length timer-list) (length timer-idle-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_overlay_before_string_with_display_and_face_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (nil nil #(\">>\" 0 2 (face bold display \"XX\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'before-string (propertize ">>" 'face 'bold 'display "XX"))
    (overlay-put ov 'after-string (propertize "<<" 'face 'italic)))
  (list (get-char-property 2 'face)
        (get-char-property 3 'face)
        (overlay-get (car (overlays-at 3)) 'before-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_cl_defstruct_with_reader_writer_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-cx33-box (:conc-name neo-cx33-box-)
                              (:reader neo-cx33-read-box))
    (val 0) name)
  (let ((b (make-neo-cx33-box :val 42 :name "test")))
    (list (neo-cx33-box-val b)
          (neo-cx33-box-name b)
          (neo-cx33-read-box b))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_decode_coding_string_then_set_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" (face bold) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((decoded (decode-coding-string (unibyte-string 99 97 102 195 169) 'utf-8))
       (proped (propertize decoded 'face 'bold)))
  (list decoded (text-properties-at 0 proped) (length proped)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_undo_after_delete_then_insert_text_prop_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"04XYZ56789\" 0 1 (face bold)) 3 2 7 (face bold)) #(\"0123456789\" 0 1 (face bold) 1 4 (face bold)) 6 2 (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 3 7)) (m (set-marker (make-marker) 6)))
    (overlay-put ov 'face 'italic)
    (undo-boundary)
    (delete-region 2 5)
    (undo-boundary)
    (goto-char 3) (insert "XYZ")
    (let ((state (list (buffer-string) (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo) (undo)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_window_text_height_and_body_after_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (23 11 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig (window-body-height)))
  (condition-case e
      (progn
        (split-window nil nil 'below)
        (let ((after (window-body-height)))
          (delete-other-windows)
          (list orig after (>= orig after))))
    (error (list orig :errored))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_process_output_with_explicit_coding_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café世界\\n\" 1 \"02fca083f0cb33c491b610f4366f9ef1a1ef007dff6f5e6d02af4fdcf901b9eb\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((coding-system-for-read 'utf-8-unix))
    (call-process "printf" nil t nil "café世界\n"))
  (list (buffer-string) (count-lines 1 (point-max))
        (secure-hash 'sha256 (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_set_match_data_vector_then_search_again() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp [0 2 0 1 1 2])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (string-match "\\(.\\)\\(.\\)" "xy")
  (set-match-data [0 2 0 1 1 2])
  (let ((md1 (list (match-string 1) (match-string 2))))
    (string-match "z" "xyz")
    (list md1 (match-string 0) (match-beginning 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_coding_system_priority_list_contains_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 iso-2022-7bit iso-latin-1 iso-2022-7bit-lock iso-2022-8bit-ss2 emacs-mule raw-text iso-2022-jp in-is13194-devanagari chinese-iso-8bit utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided) (utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided) (emacs-mule raw-text iso-2022-jp in-is13194-devanagari chinese-iso-8bit utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((prio (coding-system-priority-list)))
  (list (memq 'utf-8 prio)
        (memq 'utf-8-auto prio)
        (memq 'emacs-mule prio)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_char_category_in_multibyte_for_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-category)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-category ?\x4e2d)
      (char-category ?\x3042)
      (char-category ?\xac00))
"##,
        expect,
    );
}

#[test]
fn div_cx33_print_escape_nonascii_with_eight_bit_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\"\\\\310\\\\311A\\\"\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-nonascii t))
  (list (prin1-to-string (string-make-multibyte (unibyte-string 200 201 65)))
        (length (prin1-to-string (string-make-multibyte (unibyte-string 200))))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_overlay_evaporate_delete_undo_text_prop_all_restored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (put-text-property 1 4 'face 'bold)
  (let ((ov (make-overlay 2 5)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (delete-region 2 5)
    (let ((evaporated (list (overlayp ov) (text-properties-at 1))))
      (undo)
      (list evaporated (overlayp ov) (overlay-start ov)
            (text-properties-at 1) (text-properties-at 2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx33_format_c_with_codepoint_then_concat_then_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 1 4 2 7 (12354 128512))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((c1 (format "%c" #x3042))
       (c2 (format "%c" #x1f600))
       (cat (concat c1 c2)))
  (list (length c1) (string-bytes c1)
        (length c2) (string-bytes c2)
        (length cat) (string-bytes cat)
        (append cat nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx33_buffer_local_then_let_shadow_then_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local :shadowed :set-in-shadow :local :global)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx33-var :global)
  (with-temp-buffer
    (setq-local neo-cx33-var :local)
    (list neo-cx33-var
          (let ((neo-cx33-var :shadowed)) neo-cx33-var)
          (let ((neo-cx33-var :shadowed)) (setq neo-cx33-var :set-in-shadow) neo-cx33-var)
          neo-cx33-var
          (default-value 'neo-cx33-var))))
"##,
        expect,
    );
}
