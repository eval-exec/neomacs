//! Complex combo batch 50 (MILESTONE) — 50th combo batch. ~1000 total combo tests.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx50_superword_kill_word_undo_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (superword-mode 1)
      (insert "pre snake_case_var post rest")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 19)) (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'italic)
        (narrow-to-region 3 27)
        (goto-char (marker-position m))
        (kill-word 1)
        (let ((after-kill (list (buffer-string) (marker-position m)
                                (overlay-start ov))))
          (undo)
          (list after-kill (buffer-string) (marker-position m)
                (overlay-start ov) (text-properties-at 1)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_subword_upcase_backward_undo_textprop_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "pre camelCaseVar post rest")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 18)) (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'italic)
        (narrow-to-region 3 27)
        (goto-char 18)
        (upcase-word -1)
        (let ((after (list (buffer-string) (marker-position m)
                           (overlay-start ov) (text-properties-at 1))))
          (undo)
          (list after (buffer-string) (marker-position m)
                (overlay-start ov) (text-properties-at 1)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_process_output_buffer_overlay_narrow_textprop_undo_evaporate_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx50-po*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (let ((ov (make-overlay 3 5))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx50-po" :command '("echo" "café")
                         :buffer buf)))
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (undo)
             (list content (buffer-string)
                   (text-properties-at 0) (text-properties-at 7)
                   (length (overlays-in 1 20))
                   (get-char-property 2 'face))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx50_json_dom_xml_roundtrip_multibyte_hash_deep_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"café\" \"世界\" \"😀\") \"{\\\"items\\\":[\\\"café\\\",\\\"世界\\\",\\\"😀\\\"]}\" [\"café\" \"世界\" \"😀\"] nil \"6cd84efbf2885b026fa87bfc8165381ec269a2e0048a47177e7cfeebc1427557\" 27)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((xml (with-temp-buffer
                    (insert "<data><item>café</item><item>世界</item><item>😀</item></data>")
                    (car (xml-parse-region (point-min) (point-max)))))
             (items (mapcar #'dom-text (dom-by-tag xml 'item)))
             (json-enc (json-encode `((items . ,items))))
             (json-dec (json-read-from-string json-enc))
             (back (cdr (assoc 'items json-dec))))
        (list items json-enc back (equal items back)
              (secure-hash 'sha256 json-enc)
              (length json-enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_coding_env_process_sentinel_timer_undo_window_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired se)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX50=val" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX50"))))
        (exit-code
         (let ((p (make-process :name "neo-cx50-te" :command '("sh" "-c" "exit 6")
                                :sentinel (lambda (proc ev) (setq se ev))))
           (accept-process-output p 2)
           (process-exit-status p)))
        (coding-out
         (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix))
             (call-process "printf" nil t nil "café世界"))
           (secure-hash 'sha256 (buffer-string)))))
    (sit-for 0.01)
    (list timer-fired env-val exit-code coding-out
          (if se (string-match "code 6" se) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_print_circle_gensym_struct_vector_propertized_escape_all_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx50-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-escape-multibyte t) (print-length 3) (print-level 2))
    (let* ((r (make-neo-cx50-rec :a "café" :b 42))
           (gs (gensym))
           (s (propertize "world\n\t" 'face 'bold))
           (x (vector r r)))
      (list (prin1-to-string x)
            (prin1-to-string `(',gs))
            (prin1-to-string s)
            (length (prin1-to-string s))
            (length (prin1-to-string (list 1 2 3 4 5 6 7 8 9)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_set_buffer_multibyte_narrow_overlay_marker_textprop_display_evaporate_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'display "XX")
  (let ((ov (make-overlay 8 14)) (m (set-marker (make-marker) 11)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 24)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((nil-state (list (length (buffer-string)) (marker-position m)
                           (overlay-start ov) (text-properties-at 1)
                           (current-column))))
      (set-buffer-multibyte t)
      (undo-boundary)
      (delete-region 5 15)
      (let ((del-state (list (length (buffer-string)) (marker-position m)
                             (overlayp ov))))
        (undo)
        (list nil-state del-state (length (buffer-string))
              (marker-position m) (overlayp ov) (overlay-start ov)
              (text-properties-at 1) (text-properties-at 5)
              (current-column) (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_cl_defmethod_full_hierarchy_advice_error_recover_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ok :recovered (:advice :around :before :base :sub-after :advice :around :before :base :caught))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx50-base () ((val :initarg :val)))
  (defclass neo-cx50-sub (neo-cx50-base) ())
  (defclass neo-cx50-subsub (neo-cx50-sub) ())
  (let (log)
    (cl-defgeneric neo-cx50-fn (obj))
    (cl-defmethod neo-cx50-fn :around ((obj neo-cx50-base))
      (push :around log) (condition-case e (cl-call-next-method) (error (push :caught log) :recovered)))
    (cl-defmethod neo-cx50-fn :before ((obj neo-cx50-base)) (push :before log))
    (cl-defmethod neo-cx50-fn ((obj neo-cx50-base)) (push :base log) (if (< (oref obj val) 0) (error "neg") :ok))
    (cl-defmethod neo-cx50-fn :after ((obj neo-cx50-sub)) (push :sub-after log))
    (advice-add 'neo-cx50-fn :before (lambda (&rest _) (push :advice log)))
    (list (neo-cx50-fn (neo-cx50-subsub :val 42))
          (neo-cx50-fn (neo-cx50-subsub :val -1))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_regex_casefold_multibyte_replace_match_data_cyrillic_greek_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"XXXXXX test\" \"YYYYYY data\" (0 3) (0 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (replace-regexp-in-string "[фгд]" "X" "ФГДфгд test")
        (replace-regexp-in-string "[αβγ]" "Y" "ΑΒΓαβγ data")
        (progn (string-match "син" "Синий") (match-data))
        (progn (string-match "ω" "Ωμέγα") (match-data))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_weak_hash_all_weakness_types_gc_eviction_comparison_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (let ((ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v ht) (garbage-collect) (hash-table-count ht))
      (let ((ht (make-hash-table :weakness 'value :test 'eq)))
        (puthash :k (cons 1 nil) ht) (garbage-collect) (hash-table-count ht))
      (let ((ht (make-hash-table :weakness 'key-and-value :test 'eq)))
        (puthash (cons 1 nil) (cons 2 nil) ht) (garbage-collect) (hash-table-count ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx50_window_config_register_buffer_point_marker_overlay_dedicated_split_hscroll_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx50-wc*")))
  (with-current-buffer buf
    (insert (make-string 40 ?x))
    (let ((m (set-marker (make-marker) 15)) (ov (make-overlay 5 25)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 5 'face 'italic)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 20)
  (set-window-hscroll (selected-window) 4)
  (set-window-dedicated-p (selected-window) t)
  (let ((cfg (current-window-configuration)))
    (set-register ?w cfg)
    (split-window nil nil 'below)
    (let ((split-count (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list split-count (count-windows)
                   (window-point) (window-hscroll) (window-dedicated-p)
                   (with-current-buffer buf (marker-position (cdar buffer-markers)))
                   (with-current-buffer buf (length (overlays-at 10)))
                   (with-current-buffer buf (text-properties-at 1)))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_subword_capitalize_backward_word_then_forward_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"pre camelCaseVar rest\" 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "pre camelCaseVar rest")
      (goto-char 18)
      (capitalize-word -1)
      (let ((after-cap (buffer-string)))
        (forward-word 1)
        (list after-cap (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_process_send_region_narrowed_filter_hash_env_exitcode_coding_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (received env-val exit-code)
  (setq env-val
        (let ((process-environment (cons "NEO_CX50=v" process-environment)))
          (string-trim (shell-command-to-string "echo $NEO_CX50"))))
  (with-temp-buffer
    (insert "AAAAsend-me-thisBBBB")
    (narrow-to-region 5 18)
    (let ((p (make-process :name "neo-cx50-sr" :command '("cat")
                           :buffer nil :connection-type 'pipe
                           :filter (lambda (proc str) (push str received))))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (process-send-region p (point-min) (point-max))
      (process-send-eof p)
      (accept-process-output p 1)))
  (setq exit-code
        (let ((p (make-process :name "neo-cx50-ec" :command '("sh" "-c" "exit 3")))
          (accept-process-output p 2)
          (process-exit-status p)))
  (list env-val exit-code (secure-hash 'md5 (apply #'concat (nreverse received)))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_read_eval_backquote_destructuring_lexical_macro_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((data '((("café" . 1) ("世界" . 2) ("😀" . 3))))
    (list (eval (car (read-from-string "`(,(mapcar #'car (caar ,data)))")) t)
          (eval (car (read-from-string "`(,@(mapcar #'cdr (caar ,data)) end)")) t)
          (eval (car (read-from-string "`(:items ,(length (caar ,data)))")) t))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_coding_priority_list_exact_after_prefer_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 20 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (length (coding-system-priority-list))))
  (prefer-coding-system 'utf-16)
  (let ((after-utf16 (length (coding-system-priority-list))))
    (prefer-coding-system 'utf-8)
    (let ((after-utf8 (length (coding-system-priority-list))))
      (list before after-utf16 after-utf8))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_overlay_modification_hooks_insert_delete_before_after_evaporate_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (before-hooks after-hooks ov-hooks)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "0123456789ABCDEF")
    (put-text-property 1 5 'face 'bold)
    (let ((ov (make-overlay 5 10)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (overlay-put ov 'modification-hooks (list (lambda (o b e &rest _) (push :ov ov-hooks))))
      (add-hook 'before-change-functions (lambda (b e) (push :before before-hooks)) nil t)
      (add-hook 'after-change-functions (lambda (b e l) (push :after after-hooks)) nil t)
      (narrow-to-region 3 14)
      (undo-boundary)
      (goto-char 7) (insert "XYZ")
      (undo-boundary)
      (delete-region 4 10)
      (let ((state (list (length before-hooks) (length after-hooks) (length ov-hooks)
                         (buffer-string) (overlayp ov) (text-properties-at 1))))
        (undo) (undo)
        (list state (buffer-string) (overlayp ov)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1) (text-properties-at 5))))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_superword_upcase_word_multiple_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"SNAKE_CASE_VAR CAMELCASE rest\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var camelCase rest")
      (goto-char 26)
      (upcase-word -2)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx50_cl_coerce_cycle_multibyte_propertized_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café世界" 'face 'bold))
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string)))
  (list (length s) (length l) (length v) (length s2)
        (text-properties-at 0 s) (text-properties-at 0 s2)
        (equal s s2) (append s nil)))
"##,
        expect,
    );
}
