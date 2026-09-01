//! Complex combo batch 58 — continued fresh edges + MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx58_org_babel_confirm_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (let ((org-confirm-babel-evaluate nil))
        (with-temp-buffer
          (org-mode)
          (insert "#+BEGIN_SRC emacs-lisp\n(+ 40 2)\n#+END_SRC\n")
          (org-babel-execute-src-block))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_org_clock_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task 1\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00]--[2024-01-01 Mon 11:00] =>  1:00\n:END:\n")
        (let ((org-time-clocksum-format '(:hours "%d" :require-timer nil)))
          (org-clock-sum)))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_org_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2:00\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task\n:PROPERTIES:\n:Effort: 2:00\n:Priority: A\n:END:\n")
        (list (org-entry-get (point) "Effort")
              (org-entry-get (point) "Priority")
              (org-entry-get (point) "NonExistent"))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_org_todo_state_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* TODO Task 1\n** DONE Sub task\n")
        (list (org-get-todo-state)
              (save-excursion (forward-line 1) (org-get-todo-state)
              (org-outline-level)
              (length (org-map-entries t)))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_imenu_custom_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (emacs-lisp-mode)
      (let ((imenu-generic-expression
             '((nil "^\\s-*(def\\(un\\|var\\|custom\\|macro\\)\\s-+\\(\\(\\w\\|\\s_\\)+\\)" 2)))
        (insert "(defun func-a () 1)\n(defvar var-a 2)\n(defmacro mac-a (x) nil)\n")
        (let ((index (imenu--index-alist)))
          (list (consp index) (length index)))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_add_log_current_defun_with_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"my-test-fn\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'add-log)
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(defun my-test-fn (x y)\n  \"Doc.\"\n  (+ x y))\n")
        (goto-char 20)
        (add-log-current-defun)))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_cl_defmethod_combination_max_with_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx58-fn: (max)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx58-cls () ())
  (cl-defgeneric neo-cx58-fn (obj) (:method-combination max))
  (cl-defmethod neo-cx58-fn max ((obj neo-cx58-cls)) 10)
  (cl-defmethod neo-cx58-fn max ((obj neo-cx58-cls)) 30)
  (cl-defmethod neo-cx58-fn max ((obj neo-cx58-cls)) 20)
  (list (neo-cx58-fn (neo-cx58-cls))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_char_fold_search_mode_toggle_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 101)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (let ((search-default-mode nil))
        (list (string-match "cafe" "café")
              (string-match (char-fold-to-regexp ?e) "café")))
      (let ((search-default-mode 'char-fold-to-regexp))
        (list (string-match "cafe" "café")
              (string-match (char-fold-to-regexp ?e) "café"))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_process_kill_query_exit_flag_with_buffer_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx58-pk*")))
  (with-current-buffer buf
    (insert "HEADER\n")
    (narrow-to-region 1 4))
  (let ((p (make-process :name "neo-cx58-pk" :command '("sleep" "10") :buffer buf)))
    (accept-process-output p 0.1)
    (set-process-query-on-exit-flag p nil)
    (kill-buffer buf))
  (list (buffer-live-p buf) (process-live-p p)))
"##,
        expect,
    );
}

#[test]
fn div_cx58_overlay_evaporate_undo_display_marker_narrow_textprop_evaporate_env_exitcode_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX58=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX58"))))
      (exit-code
       (let ((p (make-process :name "neo-cx58-ec" :command '("sh" "-c" "exit 5")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
    (puthash (cons 1 nil) :v weak-ht)
    (garbage-collect)
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "0123456789ABCDEF0123456789")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 5 9 'display "XX")
      (let ((ov (make-overlay 10 18)) (m (set-marker (make-marker) 14)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 3 24)
        (undo-boundary)
        (delete-region 8 16)
        (let ((state (list (buffer-string) (marker-position m)
                           (overlayp ov) (text-properties-at 1)
                           (current-column))))
          (undo)
          (list env-val exit-code state (buffer-string)
                (marker-position m) (overlayp ov) (overlay-start ov)
                (text-properties-at 1) (text-properties-at 5)
                (current-column) (hash-table-count weak-ht)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_json_xml_dom_struct_backquote_cl_loop_hash_secure_print_circle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx58-item id text)
  (condition-case e
      (progn (require 'json) (require 'dom) (require 'xml)
        (let* ((recs (list (make-neo-cx58-item :id 1 :text "café")
                            (make-neo-cx58-item :id 2 :text "世界")))
               (json-enc (json-encode (mapcar (lambda (r) `((id . ,(neo-cx58-item-id r)) (text . ,(neo-cx58-item-text r)))) recs)))
               (json-dec (json-read-from-string json-enc))
               (ht (make-hash-table :test 'equal)))
          (cl-loop for bd across json-dec do (puthash (cdr (assoc 'id bd)) (cdr (assoc 'text bd)) ht))
          (let ((print-circle t))
            (list (mapcar #'neo-cx58-item-text recs)
                  (mapcar (lambda (bd) (cdr (assoc 'text bd))) json-dec)
                  (hash-table-count ht)
                  (secure-hash 'sha256 json-enc)
                  (length json-enc)))))
    (error (cons 'errored (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_window_split_merge_marker_overlay_textprop_display_evaporate_undo_narrow_widen_env_exitcode_timer_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX58=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX58"))))
        (exit-code
         (let ((p (make-process :name "neo-cx58-ec" :command '("sh" "-c" "exit 4")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
      (puthash (cons 1 nil) :v weak-ht)
      (garbage-collect)
      (condition-case e
          (let ((buf (get-buffer-create " *neo-cx58-wc*")))
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
            (let ((cfg (current-window-configuration)))
              (set-register ?w cfg)
              (split-window nil nil 'right)
              (let ((split-count (count-windows)))
                (set-window-configuration cfg)
                (with-current-buffer buf (undo) (widen))
                (prog1 (list env-val exit-code timer-fired split-count (count-windows)
                             (with-current-buffer buf (marker-position (cdar buffer-markers)))
                             (with-current-buffer buf (length (overlays-at 15)))
                             (with-current-buffer buf (text-properties-at 1))
                             (with-current-buffer buf (buffer-string))
                             (hash-table-count weak-ht))
                  (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
                  (kill-buffer buf))))
        (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_regex_casefold_replace_undo_superword_subword_kill_marker_overlay_narrow_display_textprop_env_exitcode_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX58=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX58"))))
      (exit-code
       (let ((p (make-process :name "neo-cx58-ec" :command '("sh" "-c" "exit 7")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
    (puthash (cons 1 nil) :v weak-ht)
    (garbage-collect)
    (let ((case-fold-search t))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Café Συν snake_case_var camelCase 世界 test")
        (put-text-property 1 4 'face 'bold)
        (put-text-property 5 8 'display "XX")
        (let ((ov (make-overlay 5 25)) (m (set-marker (make-marker) 15)))
          (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
          (narrow-to-region 1 48)
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
                  (text-properties-at 1) (current-column)
                  (hash-table-count weak-ht)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx58_read_eval_backquote_destructuring_lexical_cl_loop_hash_secure_print_circle_env_exitcode_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX58=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX58"))))
      (exit-code
       (let ((p (make-process :name "neo-cx58-ec" :command '("sh" "-c" "exit 2")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (let ((lexical-binding t))
    (let ((data '((("café" . 1) ("世界" . 2) ("😀" . 3))))
      (let* ((processed (eval (car (read-from-string
                                   "`(:names ,(mapcar #'car (caar ,data))
                                      :vals ,(mapcar #'cdr (caar ,data))
                                      :sum ,(cl-loop for v in (mapcar #'cdr (caar ,data)) sum v))")) t))
             (ht (make-hash-table :test 'equal)))
        (cl-loop for n in (plist-get processed :names)
                 for v in (plist-get processed :vals)
                 do (puthash n v ht))
        (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
          (puthash (cons 1 nil) :v weak-ht)
          (garbage-collect)
          (let ((print-circle t))
            (list env-val exit-code
                  (plist-get processed :names)
                  (plist-get processed :sum)
                  (hash-table-count ht)
                  (secure-hash 'sha256 (prin1-to-string processed))
                  (hash-table-count weak-ht)))))))
"##,
        expect,
    );
}
