//! Complex combo batch 54 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx54_superword_subword_toggle_kill_undo_marker_overlay_textprop_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "pre snake_case_var camelCase rest end")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 30)) (m (set-marker (make-marker) 15)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 3 42)
        (superword-mode 1)
        (goto-char 5) (kill-word 1)
        (let ((sw-kill (list (buffer-string) (marker-position m) (overlay-start ov))))
          (subword-mode 1)
          (goto-char 5) (kill-word 1)
          (let ((sub-kill (list (buffer-string) (marker-position m) (overlay-start ov))))
            (undo) (undo) (undo)
            (subword-mode -1) (superword-mode -1)
            (list sw-kill sub-kill (buffer-string) (marker-position m)
                  (overlay-start ov) (text-properties-at 1)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_process_output_env_coding_exitcode_timer_narrow_overlay_textprop_undo_evaporate_display_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired se)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX54=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX54"))))
        (exit-code
         (let ((p (make-process :name "neo-cx54-ec" :command '("sh" "-c" "exit 6")
                                :sentinel (lambda (proc ev) (setq se ev))))
           (accept-process-output p 2)
           (process-exit-status p)))
        (buf (get-buffer-create " *neo-cx54-po*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "HEADER\n")
      (put-text-property 1 7 'face 'bold)
      (put-text-property 4 6 'display "XX")
      (let ((ov (make-overlay 3 5)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
      (narrow-to-region 1 7))
    (let ((p (make-process :name "neo-cx54-po" :command '("printf" "%s" "café")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1))
    (sit-for 0.01)
    (prog1 (with-current-buffer buf
             (widen)
             (let ((content (buffer-string)))
               (undo)
               (list env-val exit-code timer-fired content (buffer-string)
                     (text-properties-at 0) (text-properties-at 7)
                     (length (overlays-in 1 20))
                     (if se (string-match "code 6" se) nil))))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_json_xml_dom_serialize_parse_chain_with_hash_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((json-str "{\"items\":[{\"id\":1,\"text\":\"café\"},{\"id\":2,\"text\":\"世界\"}]}")
             (json-data (json-read-from-string json-str))
             (items (cdr (assoc 'items json-data)))
             (xml-str (with-temp-buffer
                        (insert "<root><item>café</item><item>世界</item></root>")
                        (buffer-string)))
             (xml-dom (with-temp-buffer
                        (insert xml-str)
                        (car (xml-parse-region (point-min) (point-max)))))
        (list (length items)
              (cdr (assoc 'text (aref items 0)))
              (cdr (assoc 'text (aref items 1)))
              (mapcar #'dom-text (dom-by-tag xml-dom 'item))
              (secure-hash 'sha256 json-str)
              (secure-hash 'md5 xml-str))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_cl_defmethod_full_hierarchy_eql_static_around_before_after_primary_advice_error_call_next_undo_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx54-fn: (:static)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx54-a () ((val :initarg :val)))
  (defclass neo-cx54-b (neo-cx54-a) ())
  (defclass neo-cx54-c (neo-cx54-b) ())
  (let (log)
    (cl-defgeneric neo-cx54-fn (obj))
    (cl-defmethod neo-cx54-fn :static ((class neo-cx54-a)) (push :static log) :cls)
    (cl-defmethod neo-cx54-fn :around ((obj neo-cx54-a))
      (push :around-in log)
      (let ((r (condition-case e (cl-call-next-method) (error (push :caught log) :recovered))))
        (push :around-out log) r))
    (cl-defmethod neo-cx54-fn :before ((obj neo-cx54-a)) (push :before log))
    (cl-defmethod neo-cx54-fn ((obj (eql :kw))) (push :kw log) :kw)
    (cl-defmethod neo-cx54-fn ((obj neo-cx54-a))
      (push :primary log)
      (if (< (oref obj val) 0) (error "neg") (oref obj val)))
    (cl-defmethod neo-cx54-fn :after ((obj neo-cx54-b)) (push :b-after log))
    (advice-add 'neo-cx54-fn :before (lambda (&rest a) (push :adv log)))
    (list (neo-cx54-fn (neo-cx54-c :val 42))
          (neo-cx54-fn (neo-cx54-c :val -1))
          (neo-cx54-fn :kw)
          (length (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_window_config_register_split_merge_narrow_widen_marker_overlay_textprop_display_dedicated_hscroll_vscroll_margins_fringes_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx54-wc*")))
      (with-current-buffer buf
        (insert (make-string 80 ?x))
        (let ((m (set-marker (make-marker) 30)) (ov (make-overlay 10 40)))
          (overlay-put ov 'face 'bold)
          (put-text-property 1 5 'face 'italic)
          (put-text-property 50 55 'display "XX")))
      (set-window-buffer (selected-window) buf)
      (set-window-point (selected-window) 35)
      (set-window-hscroll (selected-window) 5)
      (set-window-vscroll (selected-window) 2)
      (set-window-dedicated-p (selected-window) t)
      (set-window-margins (selected-window) 4 2)
      (with-current-buffer buf (narrow-to-region 5 70))
      (let ((cfg (current-window-configuration)))
        (set-register ?w cfg)
        (split-window nil nil 'right)
        (let ((split-count (count-windows)))
          (set-window-configuration cfg)
          (with-current-buffer buf (widen))
          (prog1 (list split-count (count-windows)
                       (window-point) (window-hscroll) (window-vscroll) (window-dedicated-p)
                       (window-margins)
                       (with-current-buffer buf (marker-position (cdar buffer-markers)))
                       (with-current-buffer buf (length (overlays-at 20)))
                       (with-current-buffer buf (text-properties-at 1))
                       (with-current-buffer buf (text-properties-at 50)))
            (set-window-margins (selected-window) 0 0)
            (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
            (kill-buffer buf))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx54_set_buffer_multibyte_undo_redo_narrow_widen_marker_overlay_textprop_display_evaporate_env_exitcode_coding_timer_full_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX54=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX54"))))
        (exit-code
         (let ((p (make-process :name "neo-cx54-ec" :command '("sh" "-c" "exit 3")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "0123456789ABCDEF0123456789ABCDEF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 5 9 'display "XX")
      (let ((ov (make-overlay 10 18)) (m (set-marker (make-marker) 14)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 3 30)
        (undo-boundary)
        (set-buffer-multibyte nil)
        (let ((nil-1 (list (length (buffer-string)) (marker-position m)
                          (overlayp ov) (text-properties-at 1))))
          (set-buffer-multibyte t)
          (undo-boundary)
          (delete-region 8 20)
          (undo-boundary)
          (put-text-property 5 12 'font-lock-face 'keyword)
          (let ((final (list (length (buffer-string)) (marker-position m)
                             (overlayp ov) (text-properties-at 1)
                             (text-properties-at 5))))
            (undo) (undo) (undo)
            (widen)
            (list env-val exit-code timer-fired nil-1 final
                  (length (buffer-string)) (marker-position m)
                  (overlayp ov) (overlay-start ov)
                  (text-properties-at 1) (buffer-string))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_weak_hash_key_value_eviction_after_gc_multiple_types_marker_cons_vector_string_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2 1 1 :sym-k :stable-val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht-k (make-hash-table :weakness 'key :test 'eq))
      (ht-v (make-hash-table :weakness 'value :test 'eq)))
  (puthash (cons 1 nil) :cons-k ht-k)
  (puthash (vector 2) :vec-k ht-k)
  (puthash :stable :sym-k ht-k)
  (puthash :key1 (cons 3 nil) ht-v)
  (puthash :key2 :stable-val ht-v)
  (let ((k-before (hash-table-count ht-k))
        (v-before (hash-table-count ht-v)))
    (garbage-collect)
    (list k-before v-before
          (hash-table-count ht-k) (hash-table-count ht-v)
          (gethash :stable ht-k) (gethash :key2 ht-v))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_regex_casefold_replace_undo_marker_overlay_narrow_display_textprop_evaporate_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Café Συν κόσμο straße 世界 test")
    (put-text-property 1 4 'face 'bold)
    (put-text-property 6 9 'display "XX")
    (let ((ov (make-overlay 5 15)) (m (set-marker (make-marker) 10)))
      (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
      (narrow-to-region 1 28)
      (undo-boundary)
      (goto-char 1)
      (while (re-search-forward "[a-zéàüß]+" nil t)
        (replace-match (upcase (match-string 0))))
      (let ((state (list (buffer-string) (marker-position m)
                        (overlayp ov) (overlay-start ov)
                        (text-properties-at 1) (text-properties-at 5)
                        (current-column))))
        (undo)
        (list state (buffer-string) (marker-position m)
              (overlayp ov) (overlay-start ov)
              (text-properties-at 1) (current-column)))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_read_eval_backquote_destructuring_lexical_macro_propertized_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((data '((("café" . 1) ("世界" . 2))))
    (let* ((names (eval (car (read-from-string "`(,@(mapcar #'car (caar ,data)))")) t))
           (vals (eval (car (read-from-string "`(,@(mapcar #'cdr (caar ,data)))")) t))
           (ht (make-hash-table :test 'equal)))
      (cl-loop for n in names for v in vals do (puthash n v ht))
      (list names vals (hash-table-count ht)
            (mapcar (lambda (n) (gethash n ht)) names)
            (secure-hash 'md5 (prin1-to-string ht))))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_cl_coerce_list_vector_string_char_cycle_propertized_multibyte_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café世界😀" 'face 'bold 'mouse-face 'highlight))
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string))
       (ht (make-hash-table :test 'equal)))
  (puthash s2 :val ht)
  (puthash s :val2 ht)
  (list (length s) (length l) (length v) (length s2)
        (text-properties-at 0 s) (text-properties-at 0 s2)
        (equal s s2)
        (hash-table-count ht)
        (secure-hash 'sha256 (apply #'string l))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_print_circle_gensym_struct_vector_propertized_escape_length_level_circle_roundtrip_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx54-rec a b c)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-length 5) (print-level 3))
    (let* ((r1 (make-neo-cx54-rec :a "café" :b 1 :c t))
           (r2 (make-neo-cx54-rec :a "世界" :b 2 :c nil))
           (gs1 (gensym)) (gs2 (gensym))
           (s (propertize "data\n" 'face 'bold))
           (circ (list 0)))
      (setcdr circ circ)
      (let* ((p (prin1-to-string (list r1 r2 gs1 gs2 s circ circ)))
             (back (car (read-from-string p))))
        (list (length p)
              (> (length p) 30)
              (string-match "#s" p)
              (string-match "#:" p)
              (string-match "#[0-9]=" p))))))
"##,
        expect,
    );
}

#[test]
fn div_cx54_coding_system_priority_category_base_aliases_eol_change_combined_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (20 ((utf-8 utf-8 coding-category-utf-8 utf-8) (utf-8-auto utf-8-auto coding-category-utf-8-auto utf-8) (utf-8-with-signature utf-8-with-signature coding-category-utf-8-sig utf-8) (latin-1 iso-latin-1 coding-category-charset charset) (emacs-mule emacs-mule coding-category-emacs-mule emacs-mule) (big5 chinese-big5 coding-category-big5 big5)) utf-8-dos utf-8-mac)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((prio (coding-system-priority-list)))
  (list (length prio)
        (mapcar (lambda (cs)
                  (list cs (coding-system-base cs) (coding-system-category cs)
                        (coding-system-type cs)))
                '(utf-8 utf-8-auto utf-8-with-signature latin-1 emacs-mule big5))
        (coding-system-change-eol-conversion 'utf-8 'dos)
        (coding-system-change-eol-conversion 'utf-8 'mac)))
"##,
        expect,
    );
}
