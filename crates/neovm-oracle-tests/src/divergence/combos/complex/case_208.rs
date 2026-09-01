//! Complex combo batch 208 — MEGA integration: final 5 stress tests
//! combining 12+ subsystems for maximum divergence surface coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx208_mega_1_full_subsystem_chaos_coding_eieio_clloop_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx208-obj ()
        ((value :initarg :value :initform 0)))
      (let ((lexical-binding t)
            (timer-fired nil)
            (calls nil)
            (env-val (let ((process-environment (cons "NEO_CX208=v1" process-environment)))
                       (string-trim (shell-command-to-string "echo $NEO_CX208"))))
            (exit-code (let ((p (make-process :name "neo-cx208-ec1"
                                                :command '("sh" "-c" "exit 3")))
                             (weak-ht (make-hash-table :weakness 'key :test 'eq)))
                         (puthash (cons 1 nil) :v weak-ht)
                         (garbage-collect)
                         (accept-process-output p 2)
                         (process-exit-status p))))
        (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
        (sit-for 0.01)
        (defun neo-cx208-target (x) (push (list :primary x) calls) (* x 2))
        (advice-add 'neo-cx208-target :before
                    (lambda (x) (push (list :before x) calls))
                    '((name . mega-advice)))
        (let* ((text "café 世界 😀 end")
               (enc (encode-coding-string text 'utf-8))
               (hash (secure-hash 'sha256 enc))
               (ht (make-hash-table :test 'equal)))
          (puthash "alpha" 1 ht)
          (puthash "beta" 2 ht)
          (let ((obj (make-instance 'neo-cx208-obj :value 0))
                (cl-loop-result
                 (cl-loop for k being the hash-keys of ht using (hash-values v)
                          collect (cons k v))))
            (let ((buf (get-buffer-create " *neo-cx208-mega-1*")))
              (with-current-buffer buf
                (buffer-enable-undo)
                (insert text)
                (put-text-property 1 4 'face 'bold)
                (let ((m (set-marker (make-marker) 8))
                      (ov (make-overlay 4 14)))
                  (overlay-put ov 'face 'italic)
                  (overlay-put ov 'evaporate t)
                  (narrow-to-region 2 16)
                  (oset obj value (hash-table-count ht))
                  (let ((r (neo-cx208-target 5)))
                    (let ((state (list timer-fired env-val exit-code
                                       r hash
                                       (slot-value obj 'value)
                                       cl-loop-result
                                       (eval '(macroexpand '(if t :yes :no)) t)
                                       (buffer-string)
                                       (marker-position m)
                                       (overlay-start ov) (overlay-end ov)
                                       (text-properties-at 1))))
                      (undo)
                      (widen)
                      (advice-remove 'neo-cx208-target 'mega-advice)
                      (kill-buffer buf)
                      (list state (buffer-live-p buf))))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx208_mega_2_process_buflocal_undo_textprop_overlay_narrow_timer_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX208=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX208"))))
      (exit-code (let ((p (make-process :name "neo-cx208-ec2"
                                          :command '("sh" "-c" "exit 6")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx208-mega-2*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx208-buf :local)
      (insert "Mega 2 café 世界 buffer content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx208-mega-2-p"
                           :command '("sh" "-c" "printf 'MEGA2'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            neo-cx208-buf
                            (buffer-string) (length (buffer-string))
                            (text-properties-at 1)
                            (text-properties-at 8)
                            (length (overlays-in 1 20))))))
      (with-current-buffer buf
        (undo)
        (kill-buffer buf))
      (list snapshot (buffer-live-p buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx208_mega_3_pcase_rx_syntax_search_replace_format_time_register_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 30 30 14 16 6 2026 nil)))
  (with-temp-buffer
    (buffer-enable-undo)
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?_ "w")
    (insert "var_alpha_1 BETA 456 gamma end_token")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 12))
          (ov (make-overlay 4 24)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (window-configuration-to-register ?c)
      (narrow-to-region 2 30)
      (goto-char 1)
      (let ((matches
             (cl-loop for i from 0 below 3
                      while (re-search-forward "\\w+" nil t)
                      collect (match-string 0))))
        (let ((pcase-result (pcase matches
                              (`(,a ,b ,c) (list :three a b c))
                              (_ :other))))
          (let ((snapshot (list pcase-result
                                (char-syntax (char-after 1))
                                (format-time-string "%H:%M:%S" t0)
                                (buffer-string)
                                (marker-position m)
                                (overlay-start ov) (overlay-end ov)
                                (text-properties-at 1))))
            (undo)
            (widen)
            (jump-to-register ?c)
            (list snapshot (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx208_mega_4_keymap_command_loop_eval_macro_closure_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls nil))
  (defmacro neo-cx208-double (form) `(progn ,form ,form))
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-a") 'neo-cx208-cmd-a)
    (define-key map (kbd "C-c C-b") 'neo-cx208-cmd-b)
    (letrec ((counter 0)
             (inc-fn (lambda () (cl-incf counter))))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Keymap/eval mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (funcall inc-fn) (funcall inc-fn) (funcall inc-fn)
          (let ((macro-result (eval '(macroexpand '(neo-cx208-double (cl-incf counter))) t)))
            (let ((state (list counter macro-result
                               (lookup-key map (kbd "C-c C-a"))
                               (lookup-key map (kbd "C-c C-b"))
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx208_mega_5_all_subsystem_final_chaos_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX208=v3" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX208"))))
      (exit-code (let ((p (make-process :name "neo-cx208-final-ec"
                                          :command '("sh" "-c" "exit 7")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx208-tag :a :b :c))
      (lexical-binding t))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (let ((buf (get-buffer-create " *neo-cx208-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx208-local-counter 0)
      (insert "Final mega café 世界 content")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (cl-incf neo-cx208-local-counter)
        (aset rec 2 :modified)
        (let ((cl-loop-result (cl-loop for k being the hash-keys of ht using (hash-values v)
                                        collect (cons k v))))
          (let ((macro-result (eval '(macroexpand '(if t :yes :no)) t)))
            (let ((snapshot (list timer-fired env-val exit-code
                                  (hash-table-count ht) (hash-table-count weak-ht)
                                  neo-cx208-local-counter
                                  (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                                  cl-loop-result macro-result
                                  (buffer-string)
                                  (marker-position m)
                                  (overlay-start ov) (overlay-end ov)
                                  (text-properties-at 1)
                                  (text-properties-at 6))))
              (undo)
              (widen)
              (kill-buffer buf)
              (list snapshot
                    (buffer-live-p buf)
                    (hash-table-count ht)
                    (aref rec 2)))))))))
"##,
        expect,
    );
}
