//! Complex combo batch 51 — continued fresh subsystem edges + MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx51_ibuffer_format_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'ibuffer)
      (list (fboundp 'ibuffer)
            (fboundp 'ibuffer-update)
            (boundp 'ibuffer-formats)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_view_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "line1\nline2\nline3\n")
      (view-mode 1)
      (list (buffer-local-value 'buffer-read-only (current-buffer))
            view-mode))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_bookmark_set_jump_with_annotation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'bookmark)
      (with-temp-buffer
        (insert "target line here\n")
        (goto-char 8)
        (bookmark-set "neo-cx51-bm")
        (let ((loc (bookmark-get-position "neo-cx51-bm")))
          (bookmark-set-annotation "neo-cx51-bm" "test annotation")
          (goto-char 1)
          (list loc
                (bookmark-get-annotation "neo-cx51-bm")
                (bookmark-get-filename "neo-cx51-bm")
                (member "neo-cx51-bm" (bookmark-all-names)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_with_temp_message_format_args_extends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (msg)
  (with-temp-message "test %d %s" 42 "hello"
    (setq msg (current-message)))
  msg)
"##,
        expect,
    );
}

#[test]
fn div_cx51_abbrev_expansion_count_persist_across_expansions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function abbrev-expansion-count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "neoabbr" "expanded")
  (list
   (with-temp-buffer
     (set (make-local-variable 'local-abbrev-table) tbl)
     (abbrev-mode 1)
     (insert "neoabbr ") (expand-abbrev)
     (abbrev-expansion-count (abbrev-symbol "neoabbr" tbl)))
   (with-temp-buffer
     (set (make-local-variable 'local-abbrev-table) tbl)
     (abbrev-mode 1)
     (insert "neoabbr ") (expand-abbrev)
     (abbrev-expansion-count (abbrev-symbol "neoabbr" tbl)))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_cl_loop_complex_accumulators_into_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for i from 1 to 10
         for j = (* i i)
         if (cl-evenp i) collect j into evens
         else collect j into odds
         sum j into total
         count (= (mod i 3) 0) into multiples-of-3
         maximize j into max-val
         finally (return (list (length evens) (length odds) total multiples-of-3 max-val)))
"##,
        expect,
    );
}

#[test]
fn div_cx51_format_spec_with_complex_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (format-spec-make ?a "café" ?b "世界" ?c "😀")))
      (list (format-spec "%a-%b-%c" spec)
            (format-spec "%c-%b-%a" spec)
            (length (format-spec "%a-%b-%c" spec))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_elide_head() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert ";; This is a long license header\n;; that should be elided\n;; line 3\nactual code\n")
      (let ((elide-head-default-header-regexp "^;; "))
        (elide-head))
      (list (buffer-string) (buffer-invisibility-spec)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_refill_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"the quick brown fox\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (refill-mode 1)
      (let ((fill-column 10))
        (insert "the quick brown fox")
        (buffer-string)))
  (error (list :not-available)))
"##,
        expect,
    );
}

#[test]
fn div_cx51_subword_forward_word_multiple_with_marker_tracking_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 1) (8 1) (12 1) (15 1) (20 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "myCamelCaseVar test_String last")
      (let ((m (set-marker (make-marker) 1)))
        (goto-char 1)
        (let (positions)
          (dotimes (_ 5)
            (forward-word 1)
            (push (list (point) (marker-position m)) positions))
          (nreverse positions))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_superword_backward_word_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "pre snake_case_var rest")
      (let ((m (set-marker (make-marker) 20)) (ov (make-overlay 5 19)))
        (overlay-put ov 'face 'bold)
        (narrow-to-region 3 27)
        (goto-char (marker-position m))
        (backward-word 1)
        (list (point) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (buffer-substring (point) (point-max)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_process_output_decode_encode_narrow_buffer_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"PREcafé世界\\n\" 15 \"PREcafé世界\\n\" t \"995fb302062d04e724e608f29185652d\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx51-rt*")))
  (with-current-buffer buf
    (insert "PRE\n")
    (narrow-to-region 1 4))
  (let ((p (make-process :name "neo-cx51-rt" :command '("printf" "%s" "café世界")
                         :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (encode-coding-region 1 (point-max) 'utf-8)
             (let ((enc-len (length (buffer-string))))
               (decode-coding-region 1 (point-max) 'utf-8)
               (list content enc-len (buffer-string)
                     (equal content (buffer-string))
                     (secure-hash 'md5 (buffer-string))))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx51_undo_redo_textprop_overlay_evaporate_display_marker_narrow_full_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 9 'display "XX")
  (put-text-property 9 13 'mouse-face 'highlight)
  (let ((ov (make-overlay 14 22)) (m (set-marker (make-marker) 18)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 30)
    (undo-boundary)
    (goto-char 15) (insert "PQR")
    (undo-boundary)
    (delete-region 8 20)
    (undo-boundary)
    (put-text-property 5 12 'font-lock-face 'keyword)
    (let ((state (list (point-min) (point-max) (marker-position m)
                       (overlayp ov) (overlay-start ov)
                       (buffer-string) (text-properties-at 1)
                       (text-properties-at 5) (current-column))))
      (undo) (undo) (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlayp ov) (overlay-start ov) (overlay-end ov)
            (text-properties-at 1) (text-properties-at 5)
            (text-properties-at 9) (current-column))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_weak_hash_key_eviction_multiple_types_gc_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq)))
  (puthash (cons 1 nil) :cons ht)
  (puthash (vector 1) :vec ht)
  (puthash "string-key" :str ht)
  (let ((before (hash-table-count ht)))
    (garbage-collect)
    (list before (hash-table-count ht))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_cl_defmethod_combination_and_or_progn_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx51-and-fn: (and)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx51-cls () ())
  (let (log)
    (cl-defgeneric neo-cx51-and-fn (obj) (:method-combination and))
    (cl-defgeneric neo-cx51-or-fn (obj) (:method-combination or))
    (cl-defgeneric neo-cx51-progn-fn (obj) (:method-combination progn))
    (cl-defmethod neo-cx51-and-fn and ((obj neo-cx51-cls)) t)
    (cl-defmethod neo-cx51-and-fn and ((obj neo-cx51-cls)) nil)
    (cl-defmethod neo-cx51-or-fn or ((obj neo-cx51-cls)) nil)
    (cl-defmethod neo-cx51-or-fn or ((obj neo-cx51-cls)) :found)
    (cl-defmethod neo-cx51-progn-fn progn ((obj neo-cx51-cls)) (push :a log))
    (cl-defmethod neo-cx51-progn-fn progn ((obj neo-cx51-cls)) (push :b log))
    (list (neo-cx51-and-fn (neo-cx51-cls))
          (neo-cx51-or-fn (neo-cx51-cls))
          (neo-cx51-progn-fn (neo-cx51-cls))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_json_null_true_false_encoding_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"null_val\\\":\\\"null\\\",\\\"true_val\\\":true,\\\"false_val\\\":\\\"json-false\\\"}\" ((a) (b . t) (c . :json-false)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (list (json-encode '((null_val . :null) (true_val . t) (false_val . json-false)))
            (json-read-from-string "{\"a\": null, \"b\": true, \"c\": false}")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx51_read_from_string_nested_quote_backquote_comma_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (('a 'b) ``x #'sym (1 5 6) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (list (car (read-from-string "('a 'b)"))
        (car (read-from-string "``x"))
        (car (read-from-string "#'sym"))
        (eval (car (read-from-string "`(1 ,(+ 2 3) 6)")) t)
        (eval (car (read-from-string "`(1 ,@(list 2 3) 4)")) t)))
"##,
        expect,
    );
}

#[test]
fn div_cx51_coding_system_base_all_common_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-8 utf-8 utf-8 utf-8-auto utf-8-with-signature utf-8-with-signature iso-latin-1 iso-latin-1 iso-latin-1 utf-16 utf-16be utf-16le utf-16be-with-signature no-conversion raw-text emacs-mule undecided)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar #'coding-system-base
        '(utf-8 utf-8-unix utf-8-dos utf-8-mac utf-8-auto
          utf-8-with-signature utf-8-with-signature-unix
          latin-1 latin-1-unix latin-1-dos
          utf-16 utf-16be utf-16le utf-16be-with-signature
          no-conversion raw-text emacs-mule undecided))
"##,
        expect,
    );
}

#[test]
fn div_cx51_window_margins_fringes_vscroll_hscroll_combined_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 . 2) (0 0 nil nil) 2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((w (selected-window)))
      (set-window-margins w 3 2)
      (set-window-fringes w 8 4 nil)
      (set-window-hscroll w 2)
      (set-window-vscroll w 1)
      (let ((m (window-margins)) (f (window-fringes))
            (hs (window-hscroll)) (vs (window-vscroll)))
        (set-window-margins w 0 0)
        (set-window-fringes w 0 0 nil)
        (set-window-hscroll w 0)
        (set-window-vscroll w 0)
        (list m f hs vs)))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx51_process_kill_then_buffer_content_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx51-pk*")))
  (with-current-buffer buf (insert "PREEXIST"))
  (let ((p (make-process :name "neo-cx51-pk" :command '("echo" "output")
                         :buffer buf)))
    (accept-process-output p 1)
    (let ((content (with-current-buffer buf (buffer-string))))
      (delete-process p)
      (prog1 (list content
                   (with-current-buffer buf (buffer-string))
                   (process-live-p p))
        (kill-buffer buf))))
"##,
        expect,
    );
}
