//! Complex combo batch 56 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx56_process_filter_hash_env_exitcode_coding_narrow_overlay_textprop_evaporate_undo_display_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX56=v" process-environment)))
         (string-trim (shell-command-to_string "echo $NEO_CX56"))))
      (exit-code
       (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 4")))
         (accept-process-output p 2)
         (process-exit-status p)))
      (buf (get-buffer-create " *neo-cx56-fp*"))
      (received nil))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (put-text-property 3 5 'display "XX")
    (let ((ov (make-overlay 4 6))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx56-fp" :command '("printf" "%s" "café世界")
                         :buffer buf :connection-type 'pipe
                         :filter (lambda (proc str) (push str received))))
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (accept-process-output p 0.5)))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (undo)
             (list env-val exit-code content (buffer-string)
                   (text-properties-at 0) (text-properties-at 7)
                   (length (overlays-in 1 20))
                   (secure-hash 'md5 (apply #'concat (nreverse received))))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx56_subword_superword_default_cycle_kill_capitalize_undo_marker_overlay_narrow_env_exitcode_timer_display_evaporate_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX56=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX56"))))
        (exit-code
         (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 3")))
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
            (goto-char 5) (forward-word 1)
            (let ((default-pos (point)))
              (subword-mode 1) (goto-char 5) (forward-word 1)
              (let ((sub-pos (point)))
                (superword-mode 1) (goto-char 5) (forward-word 1)
                (let ((super-pos (point)))
                  (goto-char 5) (kill-word 1)
                  (let ((state (list (buffer-string) (marker-position m)
                                     (overlayp ov) (overlay-start ov)
                                     (text-properties-at 1))))
                    (subword-mode -1) (superword-mode -1)
                    (undo)
                    (list env-val exit-code timer-fired default-pos sub-pos super-pos
                          state (buffer-string) (marker-position m)
                          (overlayp ov) (overlay-start ov)
                          (text-properties-at 1) (current-column))))))))
      (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_json_xml_dom_coerce_hash_struct_backquote_read_print_circle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx56-item id text)
  (condition-case e
      (progn (require 'json) (require 'dom) (require 'xml)
        (let* ((xml (with-temp-buffer
                     (insert "<data><i id=\"1\">café</i><i id=\"2\">世界</i></data>")
                     (car (xml-parse-region (point-min) (point-max)))))
               (dom-items (dom-by-tag xml 'i))
               (items (mapcar (lambda (el) (make-neo-cx56-item :id (dom-attr el 'id) :text (dom-text el))) dom-items))
               (json-enc (json-encode (mapcar (lambda (it) `((id . ,(neo-cx56-item-id it)) (text . ,(neo-cx56-item-text it)))) items)))
               (json-dec (json-read-from-string json-enc))
               (ht (make-hash-table :test 'equal)))
          (cl-loop for bd across json-dec do (puthash (cdr (assoc 'id bd)) (cdr (assoc 'text bd)) ht))
          (let ((print-circle t) (print-gensym t))
            (list (mapcar #'neo-cx56-item-text items)
                  (mapcar (lambda (bd) (cdr (assoc 'text bd))) json-dec)
                  (hash-table-count ht)
                  (secure-hash 'sha256 json-enc)
                  (string-match "#s" (prin1-to-string (car items)))
                  (neo-cx56-item-id (car items))))))
    (error (cons 'errored (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_cl_defmethod_full_hierarchy_advice_eql_static_error_recovery_call_next_print_read_circle_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx56-fn: (:static)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx56-a () ((val :initarg :val)))
  (defclass neo-cx56-b (neo-cx56-a) ())
  (defclass neo-cx56-c (neo-cx56-b) ())
  (let (log)
    (cl-defgeneric neo-cx56-fn (obj))
    (cl-defmethod neo-cx56-fn :static ((c neo-cx56-a)) (push :static log) :cls)
    (cl-defmethod neo-cx56-fn :around ((obj neo-cx56-a))
      (push :around log) (condition-case e (let ((r (cl-call-next-method))) (push :ok log) r) (error (push :caught log) :recovered)))
    (cl-defmethod neo-cx56-fn :before ((obj neo-cx56-a)) (push :before log))
    (cl-defmethod neo-cx56-fn ((obj (eql :kw))) (push :kw log) :kw)
    (cl-defmethod neo-cx56-fn ((obj neo-cx56-a)) (push :primary log) (if (< (oref obj val) 0) (error "neg") (oref obj val)))
    (cl-defmethod neo-cx56-fn :after ((obj neo-cx56-b)) (push :after log))
    (advice-add 'neo-cx56-fn :before (lambda (&rest a) (push :adv log)))
    (let ((results (list (neo-cx56-fn (neo-cx56-c :val 42))
                        (neo-cx56-fn (neo-cx56-c :val -1))
                        (neo-cx56-fn :kw)
                        (neo-cx56-fn 'neo-cx56-a)
                        (length (nreverse log)))))
      (let ((print-circle t))
        (string-match "lambda" (prin1-to-string results))))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_window_register_split_merge_narrow_widen_hscroll_vscroll_margins_fringes_dedicated_marker_overlay_textprop_display_evaporate_undo_env_exitcode_timer_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX56=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX56"))))
        (exit-code
         (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 1")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (condition-case e
        (let ((buf (get-buffer-create " *neo-cx56-wc*")))
          (with-current-buffer buf
            (buffer-enable-undo)
            (insert (make-string 80 ?x))
            (put-text-property 1 5 'face 'bold)
            (put-text-property 50 55 'display "XX")
            (let ((m (set-marker (make-marker) 30)) (ov (make-overlay 10 40)))
              (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
              (undo-boundary) (goto-char 25) (insert "YZ")
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
            (split-window nil nil 'below)
            (let ((split-count (count-windows)))
              (set-window-configuration cfg)
              (with-current-buffer buf (undo) (widen))
              (prog1 (list env-val exit-code timer-fired split-count (count-windows)
                           (window-point) (window-hscroll) (window-dedicated-p)
                           (window-margins)
                           (with-current-buffer buf (marker-position (cdar buffer-markers)))
                           (with-current-buffer buf (length (overlays-at 15)))
                           (with-current-buffer buf (text-properties-at 1))
                           (with-current-buffer buf (buffer-string)))
                (set-window-margins (selected-window) 0 0)
                (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
                (kill-buffer buf)))))
      (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_set_buffer_multibyte_narrow_overlay_marker_textprop_display_evaporate_undo_env_exitcode_coding_weak_hash_full_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX56=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX56"))))
      (exit-code
       (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 2")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
    (puthash (cons 1 nil) :v weak-ht)
    (garbage-collect)
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "café世界0123456789")
      (put-text-property 1 3 'face 'bold)
      (put-text-property 4 6 'display "XX")
      (let ((ov (make-overlay 7 12)) (m (set-marker (make-marker) 9)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 3 15)
        (undo-boundary)
        (set-buffer-multibyte nil)
        (let ((nil-1 (list (length (buffer-string)) (marker-position m)
                           (overlayp ov) (overlay-start ov)
                           (text-properties-at 1))))
          (set-buffer-multibyte t)
          (undo-boundary)
          (delete-region 5 10)
          (let ((del-1 (list (length (buffer-string)) (marker-position m)
                             (overlayp ov))))
            (undo)
            (list env-val exit-code nil-1 del-1 (length (buffer-string))
                  (marker-position m) (overlayp ov) (overlay-start ov)
                  (text-properties-at 1) (buffer-string)
                  (hash-table-count weak-ht))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_regex_casefold_replace_undo_marker_overlay_narrow_display_textprop_evaporate_superword_subword_env_exitcode_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX56=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX56"))))
      (exit-code
       (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 6")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((case-fold-search t))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Café Συν κόσμο straße 世界 snake_case_var")
      (put-text-property 1 4 'face 'bold)
      (put-text-property 5 8 'display "XX")
      (let ((ov (make-overlay 5 20)) (m (set-marker (make-marker) 12)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 1 45)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "[a-zéàüß_]+" nil t)
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
fn div_cx56_print_circle_gensym_struct_vector_propertized_escape_length_level_backquote_read_cl_coerce_hash_secure_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx56-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-length 3) (print-level 2))
    (let* ((r1 (make-neo-cx56-rec :a "café" :b 1))
           (gs (gensym))
           (s (propertize "data\n" 'face 'bold))
           (v [1 2 3 4 5])
           (circ (list 0)))
      (setcdr circ circ)
      (let* ((p (prin1-to-string `[,r1 ,r1 ,gs ,s ,v ,circ ,circ]))
             (back (car (read-from-string p))))
        (let ((ht (make-hash-table :test 'equal)))
          (puthash (aref back 0) 1 ht)
          (puthash (aref back 1) 2 ht)
          (list (length p)
                (aref back 3)
                (text-properties-at 0 (aref back 3))
                (eq (aref back 0) (aref back 1))
                (hash-table-count ht)
                (secure-hash 'sha256 p)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_process_output_decode_encode_hash_overlay_narrow_textprop_evaporate_undo_display_cl_coerce_json_xml_env_exitcode_timer_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX56=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX56"))))
        (exit-code
         (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 9")))
           (accept-process-output p 2)
           (process-exit-status p)))
        (buf (get-buffer-create " *neo-cx56-po*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "HEADER\n")
      (put-text-property 1 7 'face 'bold)
      (put-text-property 3 5 'display "XX")
      (let ((ov (make-overlay 4 6))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
      (narrow-to-region 1 7))
    (let ((p (make-process :name "neo-cx56-po" :command '("printf" "%s" "café世界")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1))
    (sit-for 0.01)
    (prog1 (with-current-buffer buf
             (widen)
             (let ((content (buffer-string)))
               (encode-coding-region 1 (point-max) 'utf-8)
               (let ((enc-len (length (buffer-string))))
                 (decode-coding-region 1 (point-max) 'utf-8)
                 (let ((back (buffer-string)))
                   (undo)
                   (list env-val exit-code timer-fired content enc-len back
                         (equal content back)
                         (secure-hash 'md5 back)
                         (text-properties-at 0) (text-properties-at 7)
                         (length (overlays-in 1 20))))))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx56_read_eval_backquote_destructuring_lexical_cl_loop_hash_secure_print_circle_env_exitcode_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX56=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX56"))))
      (exit-code
       (let ((p (make-process :name "neo-cx56-ec" :command '("sh" "-c" "exit 8")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((lexical-binding t))
    (let ((data '((("café" . 1) ("世界" . 2) ("😀" . 3))))
      (let* ((processed (eval (car (read-from-string
                                   "`(:names ,(mapcar #'car (caar ,data))
                                      :vals ,(mapcar #'cdr (caar ,data))
                                      :sum ,(cl-loop for v in (mapcar #'cdr (caar ,data)) sum v)
                                      :count ,(cl-loop for v in (mapcar #'cdr (caar ,data)) count t))")) t))
             (ht (make-hash-table :test 'equal)))
        (cl-loop for n in (plist-get processed :names)
                 for v in (plist-get processed :vals)
                 do (puthash n v ht))
        (let ((print-circle t))
          (list env-val exit-code
                (plist-get processed :names)
                (plist-get processed :sum)
                (hash-table-count ht)
                (secure-hash 'sha256 (prin1-to-string processed)))))))
"##,
        expect,
    );
}
