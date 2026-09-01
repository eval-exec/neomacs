//! Complex combo batch 55 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx55_superword_kill_upcase_undo_marker_overlay_narrow_textprop_display_evaporate_env_exitcode_timer_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX55=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX55"))))
        (exit-code
         (let ((p (make-process :name "neo-cx55-ec" :command '("sh" "-c" "exit 2")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (condition-case e
        (with-temp-buffer
          (buffer-enable-undo)
          (superword-mode 1)
          (insert "pre snake_case_var camelCase rest end")
          (put-text-property 1 3 'face 'bold)
          (put-text-property 4 6 'display "XX")
          (let ((ov (make-overlay 5 19)) (m (set-marker (make-marker) 12)))
            (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
            (narrow-to-region 3 38)
            (undo-boundary)
            (goto-char 5) (kill-word 1)
            (undo-boundary)
            (goto-char 5) (upcase-word 1)
            (let ((state (list (buffer-string) (marker-position m)
                               (overlayp ov) (overlay-start ov)
                               (text-properties-at 1) (text-properties-at 3)
                               (current-column))))
              (undo) (undo)
              (list env-val exit-code timer-fired state (buffer-string)
                    (marker-position m) (overlayp ov) (overlay-start ov)
                    (text-properties-at 1) (current-column))))
      (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_process_output_narrow_decode_encode_hash_overlay_textprop_evaporate_undo_env_exitcode_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX55=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX55"))))
      (exit-code
       (let ((p (make-process :name "neo-cx55-ec" :command '("sh" "-c" "exit 8")))
         (accept-process-output p 2)
         (process-exit-status p)))
      (buf (get-buffer-create " *neo-cx55-po*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (put-text-property 3 5 'display "XX")
    (let ((ov (make-overlay 4 6))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx55-po" :command '("printf" "%s" "café世界")
                         :buffer buf)))
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (encode-coding-region 1 (point-max) 'utf-8)
             (let ((enc-len (length (buffer-string))))
               (decode-coding-region 1 (point-max) 'utf-8)
               (let ((back (buffer-string)))
                 (undo)
                 (list env-val exit-code content enc-len back
                       (equal content back)
                       (secure-hash 'md5 back)
                       (text-properties-at 0) (text-properties-at 7)
                       (length (overlays-in 1 20)))))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx55_json_xml_dom_full_chain_with_coerce_hash_backquote_struct_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx55-rec name value)
  (condition-case e
      (progn (require 'json) (require 'dom) (require 'xml)
        (let* ((rec (make-neo-cx55-rec :name "café" :value 42))
               (xml-dom (with-temp-buffer
                          (insert "<data><item>世界</item><item>😀</item></data>")
                          (car (xml-parse-region (point-min) (point-max)))))
               (items (mapcar #'dom-text (dom-by-tag xml-dom 'item)))
               (json-enc (json-encode `((rec . ((name . ,(neo-cx55-rec-name rec))
                                                (value . ,(neo-cx55-rec-value rec))))
                                       (items . ,items))))
               (json-dec (json-read-from-string json-enc))
               (back-rec (cdr (assoc 'rec json-dec)))
               (back-items (cdr (assoc 'items json-dec))))
          (list (neo-cx55-rec-name rec) (neo-cx55-rec-value rec)
                items back-items (equal items back-items)
                (cdr (assoc 'name back-rec)) (cdr (assoc 'value back-rec))
                (secure-hash 'sha256 json-enc))))
    (error (cons 'errored (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_cl_defmethod_full_hierarchy_5_methods_advice_eql_error_call_next_undo_superword_subword_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx55-fn: (:static)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx55-a () ((val :initarg :val)))
  (defclass neo-cx55-b (neo-cx55-a) ())
  (defclass neo-cx55-c (neo-cx55-b) ())
  (let (log)
    (cl-defgeneric neo-cx55-fn (obj))
    (cl-defmethod neo-cx55-fn :static ((c neo-cx55-a)) (push :static log) :cls)
    (cl-defmethod neo-cx55-fn :around ((obj neo-cx55-a))
      (push :around log)
      (let ((r (condition-case e (cl-call-next-method) (error :recovered))))
        (push :around-out log) r))
    (cl-defmethod neo-cx55-fn :before ((obj neo-cx55-a)) (push :before log))
    (cl-defmethod neo-cx55-fn ((obj (eql :kw))) (push :kw log) :kw)
    (cl-defmethod neo-cx55-fn ((obj neo-cx55-a))
      (push :primary log) (if (< (oref obj val) 0) (error "neg") (oref obj val)))
    (cl-defmethod neo-cx55-fn :after ((obj neo-cx55-b)) (push :b-after log))
    (cl-defmethod neo-cx55-fn :extra "tag" ((obj neo-cx55-a)) (push :extra log))
    (advice-add 'neo-cx55-fn :before (lambda (&rest a) (push :adv log)))
    (list (neo-cx55-fn (neo-cx55-c :val 42))
          (neo-cx55-fn (neo-cx55-c :val -1))
          (neo-cx55-fn :kw)
          (neo-cx55-fn 'neo-cx55-a)
          (length (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_window_config_register_split_merge_narrow_widen_hscroll_vscroll_margins_fringes_dedicated_marker_overlay_textprop_display_evaporate_undo_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx55-wc*")))
      (with-current-buffer buf
        (buffer-enable-undo)
        (insert (make-string 80 ?x))
        (put-text-property 1 5 'face 'bold)
        (put-text-property 50 55 'display "XX")
        (let ((m (set-marker (make-marker) 30)) (ov (make-overlay 10 40)))
          (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
          (undo-boundary)
          (goto-char 25) (insert "YZ")
          (narrow-to-region 5 70)))
      (set-window-buffer (selected-window) buf)
      (set-window-point (selected-window) 35)
      (set-window-hscroll (selected-window) 5)
      (set-window-vscroll (selected-window) 2)
      (set-window-dedicated-p (selected-window) t)
      (set-window-margins (selected-window) 4 2)
      (let ((cfg (current-window-configuration)))
        (set-register ?w cfg)
        (split-window nil nil 'right)
        (let ((split-count (count-windows)))
          (set-window-configuration cfg)
          (with-current-buffer buf
            (undo)
            (widen))
          (prog1 (list split-count (count-windows)
                       (window-point) (window-hscroll) (window-dedicated-p)
                       (window-margins)
                       (with-current-buffer buf (marker-position (cdar buffer-markers)))
                       (with-current-buffer buf (length (overlays-at 15)))
                       (with-current-buffer buf (text-properties-at 1))
                       (with-current-buffer buf (buffer-string)))
            (set-window-margins (selected-window) 0 0)
            (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
            (kill-buffer buf))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx55_weak_hash_key_value_eviction_after_gc_cons_vector_marker_string_symbol_comparison_mega()
{
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((cons-key 1 0) (vector-key 1 0) (string-key 1 0) (symbol-key 1 0) (marker-key 1 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((results nil))
  (dolist (type '(cons-key vector-key string-key symbol-key marker-key))
    (let ((ht (make-hash-table :weakness 'key :test 'eq)))
      (pcase type
        ('cons-key (puthash (cons 1 nil) :v ht))
        ('vector-key (puthash (vector 1) :v ht))
        ('string-key (puthash (copy-sequence "key") :v ht))
        ('symbol-key (puthash (make-symbol "temp") :v ht))
        ('marker-key (with-temp-buffer (insert "x") (puthash (point-marker) :v ht))))
      (let ((before (hash-table-count ht)))
        (garbage-collect)
        (push (cons type (list before (hash-table-count ht))) results))))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx55_regex_casefold_replace_undo_marker_overlay_narrow_display_textprop_evaporate_env_exitcode_coding_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX55=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX55"))))
      (exit-code
       (let ((p (make-process :name "neo-cx55-ec" :command '("sh" "-c" "exit 5")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((case-fold-search t))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Café Συν κόσμο straße 世界 test")
      (put-text-property 1 4 'face 'bold)
      (put-text-property 5 8 'display "XX")
      (let ((ov (make-overlay 5 18)) (m (set-marker (make-marker) 12)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 1 30)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "[a-zéàüß]+" nil t)
          (replace-match (upcase (match-string 0))))
        (let ((state (list (buffer-string) (marker-position m)
                           (overlayp ov) (overlay-start ov)
                           (text-properties-at 1) (text-properties-at 4)
                           (current-column))))
          (undo)
          (list env-val exit-code state (buffer-string)
                (marker-position m) (overlayp ov) (overlay-start ov)
                (text-properties-at 1) (current-column))))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_print_circle_gensym_struct_vector_propertized_escape_length_level_circle_backquote_read_roundtrip_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx55-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-length 4) (print-level 3))
    (let* ((r1 (make-neo-cx55-rec :a "café" :b 1))
           (r2 (make-neo-cx55-rec :a "世界" :b 2))
           (gs (gensym))
           (s (propertize "data\n" 'face 'bold))
           (circ (vector 0)))
      (aset circ 0 circ)
      (let* ((p (prin1-to-string `[,r1 ,r2 ,gs ,s ,circ]))
             (back (car (read-from-string p))))
        (list (length p)
              (> (length p) 20)
              (string-match "#s" p) (string-match "#:" p) (string-match "#[0-9]=" p)
              (aref back 0) (aref back 1) (aref back 3)
              (text-properties-at 0 (aref back 3)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_read_eval_backquote_destructuring_lexical_macro_propertized_hash_cl_loop_secure_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((data '((("café" . 1) ("世界" . 2) ("😀" . 3))))
    (let* ((processed (eval (car (read-from-string
                                  "`(:names ,(mapcar #'car (caar ,data))
                                     :vals ,(mapcar #'cdr (caar ,data))
                                     :sum ,(cl-loop for v in (mapcar #'cdr (caar ,data)) sum v)
                                     :max ,(cl-loop for v in (mapcar #'cdr (caar ,data)) maximize v))")) t))
           (ht (make-hash-table :test 'equal)))
      (cl-loop for n in (plist-get processed :names)
               for v in (plist-get processed :vals)
               do (puthash n v ht))
      (list (plist-get processed :names)
            (plist-get processed :vals)
            (plist-get processed :sum) (plist-get processed :max)
            (hash-table-count ht)
            (secure-hash 'sha256 (prin1-to-string processed))))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_set_buffer_multibyte_undo_redo_narrow_widen_overlay_marker_textprop_display_evaporate_full_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'display "XX")
  (put-text-property 11 15 'mouse-face 'highlight)
  (let ((ov (make-overlay 16 26)) (m (set-marker (make-marker) 20)))
    (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
    (put-text-property 27 31 'font-lock-face 'keyword)
    (narrow-to-region 3 40)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((s1 (list (length (buffer-string)) (marker-position m)
                    (overlayp ov) (overlay-start ov)
                    (text-properties-at 1) (text-properties-at 5))))
      (set-buffer-multibyte t)
      (undo-boundary)
      (goto-char 18) (insert "PQR")
      (undo-boundary)
      (delete-region 10 25)
      (undo-boundary)
      (set-text-properties 6 15 '(face underline))
      (let ((s2 (list (length (buffer-string)) (marker-position m)
                      (overlayp ov) (overlay-start ov)
                      (text-properties-at 1) (text-properties-at 5))))
        (undo) (undo) (undo) (undo)
        (widen)
        (list s1 s2 (length (buffer-string))
              (marker-position m) (overlayp ov) (overlay-start ov)
              (text-properties-at 1) (text-properties-at 5) (text-properties-at 10)
              (text-properties-at 15) (current-column) (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx55_subword_superword_toggle_kill_upcase_capitalize_undo_marker_overlay_narrow_textprop_display_env_exitcode_timer_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX55=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX55"))))
        (exit-code
         (let ((p (make-process :name "neo-cx55-ec" :command '("sh" "-c" "exit 7")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (condition-case e
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "pre myCamelCaseVar snake_case_var rest end")
          (put-text-property 1 3 'face 'bold)
          (put-text-property 4 6 'display "XX")
          (let ((ov (make-overlay 5 35)) (m (set-marker (make-marker) 15)))
            (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
            (narrow-to-region 3 43)
            (subword-mode 1)
            (goto-char 5) (kill-word 1)
            (let ((sub-kill (buffer-string)))
              (superword-mode 1)
              (goto-char 5) (upcase-word 1)
              (let ((sw-upcase (buffer-string)))
                (subword-mode -1) (superword-mode -1)
                (undo) (undo)
                (list env-val exit-code timer-fired
                      sub-kill sw-upcase (buffer-string)
                      (marker-position m) (overlay-start ov)
                      (text-properties-at 1) (current-column))))))
      (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}
