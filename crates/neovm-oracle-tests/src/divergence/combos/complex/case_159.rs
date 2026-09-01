//! Complex combo batch 159 — `final mega integration`: comprehensive
//! stress combos combining 10+ subsystems per test, mirroring real-world
//! Emacs workflows.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx159_mega_eieio_clloop_eval_macro_undo_buflocal_process_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx159-state ()
        ((counter :initarg :counter :initform 0)
         (history :initarg :history :initform nil)))
      (let ((lexical-binding t)
            (timer-fired nil)
            (exit-code (let ((p (make-process :name "neo-cx159-ec"
                                              :command '("sh" "-c" "exit 9"))))
                         (accept-process-output p 2)
                         (process-exit-status p))))
        (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
        (sit-for 0.01)
        (letrec ((state (make-instance 'neo-cx159-state :counter 0)))
          (let ((inc-fn (lambda (msg)
                          (oset state counter (1+ (slot-value state 'counter)))
                          (oset state history
                                (cons msg (slot-value state 'history))))))
            (with-temp-buffer
              (buffer-enable-undo)
              (setq-local neo-cx159-buf-local :active)
              (insert (cl-loop for i from 1 to 5
                               concat (format "step-%d\n" i)))
              (put-text-property 1 5 'face 'bold)
              (let ((m (set-marker (make-marker) 12))
                    (ov (make-overlay 4 22)))
                (overlay-put ov 'face 'italic)
                (overlay-put ov 'evaporate t)
                (narrow-to-region 2 30)
                (funcall inc-fn :a)
                (funcall inc-fn :b)
                (let ((macro-result (eval '(macroexpand '(if t :yes :no)) t)))
                  (let ((snapshot (list timer-fired exit-code
                                        (slot-value state 'counter)
                                        (nreverse (slot-value state 'history))
                                        neo-cx159-buf-local
                                        macro-result
                                        (buffer-string)
                                        (marker-position m)
                                        (overlay-start ov) (overlay-end ov)
                                        (text-properties-at 1)))))
                    (undo) (undo)
                    (widen)
                    (list snapshot (buffer-string) (marker-position m)
                          (overlay-start ov) (overlay-end ov)
                          (text-properties-at 1))))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx159_mega_coding_charset_obarray_print_circle_secure_hash_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (defvar neo-cx159-shared nil)
  (defun neo-cx159-target (x) (* x 2))
  (advice-add 'neo-cx159-target :around
              (lambda (fn x) (* (funcall fn x) 10))
              '((name . mega-advice)))
  (let* ((text "café 世界 😀")
         (encoded (encode-coding-string text 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8-unix))
         (hash (secure-hash 'sha256 encoded))
         (ob (make-obarray 31))
         (sym1 (intern "neo-cx159-alpha" ob))
         (sym2 (intern "neo-cx159-beta" ob))
         (shared (list 1 2 3))
         (data (list shared shared))
         (printed-circle (let ((print-circle t)) (prin1-to-string data))))
    (put sym1 'neo-cx159-prop :v1)
    (put sym2 'neo-cx159-prop :v2)
    (let* ((target-result (neo-cx159-target 5))
           (snapshot (list text encoded decoded hash
                           (string= text decoded)
                           (hash-table-count ob)
                           (get sym1 'neo-cx159-prop)
                           printed-circle
                           target-result)))
      (advice-remove 'neo-cx159-target 'mega-advice)
      snapshot)))
"##,
        expect,
    );
}

#[test]
fn div_cx159_mega_bufferlocal_undo_overlay_textprop_process_env_weak_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX159=v" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX159"))))
      (exit-code (let ((p (make-process :name "neo-cx159-ec2"
                                          :command '("sh" "-c" "exit 11"))))
                   (accept-process-output p 2)
                   (process-exit-status p)))
      (weak-ht (make-hash-table :weakness 'key :test 'eq)))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (puthash (cons 1 nil) :v weak-ht)
  (garbage-collect)
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx159-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx159-buf :local)
      (insert "Final mega combo buffer content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx159-final-p"
                           :command '("sh" "-c" "printf 'FINAL'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            (hash-table-count weak-ht)
                            neo-cx159-buf
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
fn div_cx159_mega_pcase_rx_syntax_textprop_marker_overlay_register_window_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (set-syntax-table (make-syntax-table))
      (modify-syntax-entry ?_ "w")
      (insert "var_alpha_1 (call_arg x) end_token")
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 12))
            (ov (make-overlay 4 24)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (window-configuration-to-register ?c)
        (narrow-to-region 2 28)
        (goto-char 1)
        (let ((matches
               (cl-loop for i from 0 below 3
                        while (re-search-forward "\\w+" nil t)
                        collect (match-string 0))))
          (let ((pcase-result
                 (pcase matches
                   (`(,a ,b ,c) (list :three-matches a b c))
                   (_ :other))))
            (let ((snapshot (list pcase-result
                                  (char-syntax (char-after 1))
                                  (buffer-string)
                                  (marker-position m)
                                  (overlay-start ov) (overlay-end ov)
                                  (text-properties-at 1))))
              (undo)
              (widen)
              (jump-to-register ?c)
              (list snapshot (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx159_mega_search_replace_format_time_advice_kmacro_cl_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 0 0 12 15 6 2024 nil)))
  (defun neo-cx159-search-target () :orig)
  (advice-add 'neo-cx159-search-target :override (lambda () :overridden))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "alpha 123 BETA 456 gamma 789 delta 012 epsilon")
    (put-text-property 1 5 'face 'bold)
    (let ((case-fold-search nil)
          (m (set-marker (make-marker) 12))
          (ov (make-overlay 5 20)))
      (overlay-put ov 'face 'region)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 4 45)
      (goto-char 1)
      (while (re-search-forward "\\b[a-z]+\\b" nil t)
        (replace-match (upcase (match-string 0))))
      (let* ((cl-loop-result (cl-loop for line in (split-string (buffer-string) "[ \t]+")
                                       when (string-match "^[A-Z]+$" line)
                                       collect line))
             (time-str (format-time-string "%Y-%m-%d %H:%M:%S" t0))
             (advice-result (neo-cx159-search-target))
             (snapshot (list cl-loop-result time-str advice-result
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (point-min) (point-max)
                             (text-properties-at 1))))
        (undo) (undo)
        (widen)
        (advice-remove 'neo-cx159-search-target (advice--p (advice-member-p nil 'neo-cx159-search-target)))
        (list snapshot (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
