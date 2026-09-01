//! Complex combo batch 27 — extend reader-error finding (more malformed input),
//! plus remaining encoding/process/print edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx27_reader_malformed_edge_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (invalid-read-syntax \"Invalid modifier in string\" 9 48)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (read-from-string ".") (error (car e)))
      (condition-case e (read-from-string "..") (error (car e)))
      (condition-case e (read-from-string "(a . . b)") (error (car e)))
      (condition-case e (read-from-string "(a b .)") (error (car e)))
      (condition-case e (read-from-string "#") (error (car e)))
      (condition-case e (read-from-string "#x") (error (car e)))
      (condition-case e (read-from-string "?,") (error (car e)))
      (condition-case e (read-from-string "?\C-") (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_reader_valid_edge_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"integer, radix 16\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (car (read-from-string "?."))
      (car (read-from-string "?\\( "))
      (car (read-from-string "#x1g"))
      (car (read-from-string "#\\z"))
      (car (read-from-string "#^^[a b c]"))
      (car (read-from-string "''a"))
      (car (read-from-string "#1=(a . #1#)")))
"##,
        expect,
    );
}

#[test]
fn div_cx27_encode_region_utf8_with_eol_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((99 97 102 4194243 4194217 10) (99 97 102 4194243 4194217 13 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café\n"))
  (list (with-temp-buffer
          (insert s)
          (encode-coding-region (point-min) (point-max) 'utf-8-unix)
          (append (buffer-string) nil))
        (with-temp-buffer
          (insert s)
          (encode-coding-region (point-min) (point-max) 'utf-8-dos)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_process_exit_code_various_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx27-sig" :command '("sleep" "30"))))
  (accept-process-output p 0.1)
  (signal-process p 3)
  (accept-process-output p 1)
  (list (process-status p) (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx27_prin1_special_string_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"\\0\u{1}\u{7}\u{1b}\u{7f}\\\"\" \"\\\"\u{80}Èÿ\\\"\" \"\\\"\\\\377\\\"\" 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t))
  (list (prin1-to-string (string 0 1 7 27 127))
        (prin1-to-string (string 128 200 255))
        (prin1-to-string (string #x3FFFFF))
        (length (prin1-to-string (string 0 1 7 27 127)))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_coding_system_decode_then_encode_then_char_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((99 97 102 233 8364) (ascii ascii ascii unicode-bmp unicode-bmp) (99 97 102 195 169 226 130 172) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string 99 97 102 195 169 226 130 172))
       (decoded (decode-coding-string raw 'utf-8))
       (re-encoded (encode-coding-string decoded 'utf-8)))
  (list (append decoded nil)
        (mapcar #'char-charset (append decoded nil))
        (append re-encoded nil)
        (equal raw re-encoded)))
"##,
        expect,
    );
}

#[test]
fn div_cx27_overlay_invisible_buffer_substring_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "vis1 INVIS vis2 INVIS vis3")
      (put-text-property 6 11 'invisible t)
      (put-text-property 18 23 'invisible t)
      (filter-buffer-substring 1 28))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_cl_defmethod_qualifier_chain_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:result (:around-start :before :primary :after :around-end))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx27-base () ())
  (let (log)
    (cl-defgeneric neo-cx27-fn (obj))
    (cl-defmethod neo-cx27-fn :around ((obj neo-cx27-base))
      (push :around-start log)
      (let ((r (cl-call-next-method)))
        (push :around-end log) r))
    (cl-defmethod neo-cx27-fn :before ((obj neo-cx27-base))
      (push :before log))
    (cl-defmethod neo-cx27-fn ((obj neo-cx27-base))
      (push :primary log) :result)
    (cl-defmethod neo-cx27-fn :after ((obj neo-cx27-base))
      (push :after log))
    (list (neo-cx27-fn (neo-cx27-base))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_set_buffer_multibyte_overlay_text_prop_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"AAAAABBBBBCCCCC\" 0 4 (face bold)) (face bold) 6) #(\"AAAAABBBBBCCCCC\" 0 4 (face bold)) (face bold) 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 6 10)))
    (overlay-put ov 'face 'italic)
    (set-buffer-multibyte nil)
    (let ((uni-state (list (buffer-string) (text-properties-at 1) (overlay-start ov))))
      (set-buffer-multibyte t)
      (list uni-state (buffer-string) (text-properties-at 1) (overlay-start ov)))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_window_buffer_marker_point_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx27-wb*")))
  (with-current-buffer buf (insert "0123456789"))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 5)
  (let ((wp (window-point))
        (bp (with-current-buffer buf (point))))
    (goto-char 3)
    (prog1 (list wp bp (window-point) (point))
      (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
      (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx27_char_fold_search_then_replace_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café naïve résumé"))
  (replace-regexp-in-string (char-fold-to-regexp ?é) "E" s))
"##,
        expect,
    );
}

#[test]
fn div_cx27_coding_system_for_write_does_not_propagate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx27-np-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "café" nil f nil 'silent))
  (prog1 (with-temp-buffer
           (insert-file-contents f)
           (list (buffer-file-coding-system)
                 (> (buffer-size) 4)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_undo_redo_multiple_boundaries_precise() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (let (states)
    (dolist (s '("AAA" "BBB" "CCC" "DDD"))
      (insert s)
      (undo-boundary)
      (push (buffer-string) states))
    (let ((forward (nreverse states)))
      (undo) (undo) (undo)
      (let ((after-undo (buffer-string)))
        (let (redo-states)
          (condition-case nil
              (while t (redo) (push (buffer-string) redo-states))
            (error))
          (list forward after-undo (nreverse redo-states))))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_process_buffer_string_after_multiple_writes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"hello\\nworld\\nProcess neo-cx27-pb finished\\n\" 3)""#]];
    let form = r##"
(let ((buf (get-buffer-create " *neo-cx27-pb*")))
  (with-current-buffer buf (erase-buffer))
  (let ((p (make-process :name "neo-cx27-pb" :command '("printf" "%s" "hello\nworld")
                         :buffer buf)))
    (set-process-query-on-exit-flag p nil)
    (neovm--oracle-settle-process p))
  (prog1 (with-current-buffer buf
           (list (buffer-string) (count-lines 1 (point-max))))
    (kill-buffer buf)))
"##;
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    let with_status = r##"OK ("hello\nworld\nProcess neo-cx27-pb finished\n" 3)"##;
    for (label, value) in [("GNU", oracle.as_str()), ("Neomacs", neovm.as_str())] {
        assert_eq!(
            value, with_status,
            "{label} did not settle the process buffer"
        );
    }
}

#[test]
fn div_cx27_string_make_unibyte_then_aref_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 99 97 102 233 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((orig "café世界")
       (u (string-make-unibyte orig)))
  (list (length orig) (length u)
        (aref u 0) (aref u 1) (aref u 2) (aref u 3)
        (condition-case e (aref u 4) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_decode_coding_region_charset_property_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (charset iso-8859-1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx27-cs-")))
  (write-region "café" nil f nil 'silent)
  (prog1 (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents f)
           (decode-coding-region (point-min) (point-max) 'latin-1)
           (text-properties-at 0 (buffer-string)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_text_property_search_forward_backward_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "AAAA face=bold BBBB face=italic CCCC face=underline DDDD")
      (put-text-property 5 18 'face 'bold)
      (put-text-property 25 40 'face 'italic)
      (put-text-property 47 63 'face 'underline)
      (goto-char 1)
      (let (results)
        (while (text-property-search-forward 'face nil t)
          (push (cons (prop-match-beginning nil)
                      (get-char-property (point) 'face))
                results))
        (nreverse results)))
  (void-function :not-available)
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_coding_system_priority_after_prefer_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 20 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (copy-sequence (coding-system-priority-list))))
  (prefer-coding-system 'utf-8)
  (let ((after (coding-system-priority-list)))
    (list (length before) (length after)
          (eq (car after) 'utf-8)
          (eq (car before) (car after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_buffer_hash_deterministic_same_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h1 (with-temp-buffer (insert "deterministic content") (buffer-hash)))
      (h2 (with-temp-buffer (insert "deterministic content") (buffer-hash)))
      (h3 (with-temp-buffer (insert "different content") (buffer-hash))))
  (list (equal h1 h2) (equal h1 h3) (not (equal h2 h3))))
"##,
        expect,
    );
}

#[test]
fn div_cx27_process_send_string_then_query_buffer_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26 \"exactly 30 characters....\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx27-ss" :command '("cat")
                         :buffer (current-buffer) :connection-type 'pipe)))
    ;; This case asserts bytes echoed through the process, not default sentinel
    ;; timing.  GNU can append the default finished message in the same wait.
    (set-process-sentinel p #'ignore)
    (process-send-string p "exactly 30 characters....\n")
    (process-send-eof p)
    (accept-process-output p 1))
  (list (buffer-size) (buffer-string)))
"##,
        expect,
    );
}
