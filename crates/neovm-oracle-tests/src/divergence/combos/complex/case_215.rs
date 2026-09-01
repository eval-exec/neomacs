//! Complex combo batch 215 — final MEGA integration: 5 extreme stress
//! tests covering all subsystems for maximum divergence surface coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx215_mega_1_coding_charset_eieio_clloop_advice_process_timer_env_buflocal_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx215-obj ()
        ((value :initarg :value :initform 0)))
      (let ((lexical-binding t)
            (timer-fired nil)
            (calls nil)
            (env-val (let ((process-environment (cons "NEO_CX215=v1" process-environment)))
                       (string-trim (shell-command-to-string "echo $NEO_CX215"))))
            (exit-code (let ((p (make-process :name "neo-cx215-ec1"
                                                :command '("sh" "-c" "exit 3")))
                             (weak-ht (make-hash-table :weakness 'key :test 'eq)))
                         (puthash (cons 1 nil) :v weak-ht)
                         (garbage-collect)
                         (accept-process-output p 2)
                         (process-exit-status p))))
        (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
        (sit-for 0.01)
        (defun neo-cx215-target (x) (push (list :primary x) calls) (* x 2))
        (advice-add 'neo-cx215-target :before
                    (lambda (x) (push (list :before x) calls))
                    '((name . mega-advice)))
        (let* ((text "café 世界 😀 mega")
               (enc (encode-coding-string text 'utf-8))
               (hash (secure-hash 'sha256 enc))
               (ht (make-hash-table :test 'equal)))
          (puthash "alpha" 1 ht)
          (puthash "beta" 2 ht)
          (let ((obj (make-instance 'neo-cx215-obj :value 0))
                (cl-loop-result
                 (cl-loop for k being the hash-keys of ht using (hash-values v)
                          collect (cons k v))))
            (let ((buf (get-buffer-create " *neo-cx215-mega-1*")))
              (with-current-buffer buf
                (buffer-enable-undo)
                (setq-local neo-cx215-buf :local)
                (insert text)
                (put-text-property 1 4 'face 'bold)
                (put-text-property 6 10 'display "XX")
                (let ((m (set-marker (make-marker) 8))
                      (ov (make-overlay 4 14)))
                  (overlay-put ov 'face 'italic)
                  (overlay-put ov 'evaporate t)
                  (narrow-to-region 2 16)
                  (oset obj value (hash-table-count ht))
                  (let ((r (neo-cx215-target 5)))
                    (let ((state (list timer-fired env-val exit-code
                                       r hash
                                       neo-cx215-buf
                                       (slot-value obj 'value)
                                       cl-loop-result
                                       (eval '(macroexpand '(if t :yes :no)) t)
                                       (buffer-string)
                                       (marker-position m)
                                       (overlay-start ov) (overlay-end ov)
                                       (text-properties-at 1))))
                      (undo)
                      (widen)
                      (advice-remove 'neo-cx215-target 'mega-advice)
                      (kill-buffer buf)
                      (list state (buffer-live-p buf))))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx215_mega_2_pcase_rx_syntax_search_replace_format_time_register_window_config_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 30 30 14 16 6 2026 nil))
      (calls nil))
  (defun neo-cx215-target () :orig)
  (advice-add 'neo-cx215-target :override (lambda () :overridden))
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
      (while (re-search-forward "\\b[a-z_]+\\b" nil t)
        (replace-match (upcase (match-string 0))))
      (let* ((matches (cl-loop for i from 0 below 3
                               while (re-search-forward "\\w+" nil t)
                               collect (match-string 0)))
             (pcase-result (pcase matches
                             (`(,a ,b ,c) (list :three a b c))
                             (_ :other)))
             (time-str (format-time-string "%Y-%m-%d %H:%M:%S" t0))
             (advice-result (neo-cx215-target))
             (snapshot (list matches pcase-result time-str advice-result
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
        (undo) (undo)
        (widen)
        (jump-to-register ?c)
        (advice-remove 'neo-cx215-target (advice--p (advice-member-p nil 'neo-cx215-target)))
        (list snapshot (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx215_mega_3_process_buflocal_undo_textprop_overlay_marker_narrow_coding_env_timer_weak_hash()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX215=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX215"))))
      (exit-code (let ((p (make-process :name "neo-cx215-ec2"
                                          :command '("sh" "-c" "exit 6")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx215-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx215-buf :local)
      (insert "Mega 3 café 世界 buffer content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx215-mega-3-p"
                           :command '("sh" "-c" "printf 'MEGA3'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            neo-cx215-buf
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
fn div_cx215_mega_4_keymap_eval_macro_closure_advice_clloop_hash_obarray_secure_hash_marker_overlay()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls nil))
  (defmacro neo-cx215-double (form) `(progn ,form ,form))
  (defun neo-cx215-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx215-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-a") 'neo-cx215-cmd-a)
    (define-key map [f5] 'neo-cx215-f5)
    (letrec ((counter 0)
             (inc-fn (lambda () (cl-incf counter))))
      (let* ((ht (make-hash-table :test 'equal))
             (ob (make-obarray 31))
             (sym1 (intern "neo-cx215-alpha" ob))
             (text "café 世界"))
        (puthash "a" 1 ht)
        (puthash "b" 2 ht)
        (put sym1 'neo-cx215-prop :val)
        (let* ((hash (secure-hash 'sha256 text))
               (cl-loop-result (cl-loop for k being the hash-keys of ht using (hash-values v)
                                        collect (cons k v)))
               (macro-result (eval '(macroexpand '(neo-cx215-double (cl-incf counter))) t))
               (target-result (neo-cx215-target 5)))
          (with-temp-buffer
            (buffer-enable-undo)
            (insert "Keymap/eval/hash mega test")
            (put-text-property 1 6 'face 'bold)
            (let ((m (set-marker (make-marker) 8))
                  (ov (make-overlay 4 14)))
              (overlay-put ov 'face 'italic)
              (overlay-put ov 'evaporate t)
              (narrow-to-region 2 18)
              (funcall inc-fn) (funcall inc-fn)
              (let ((state (list counter macro-result target-result
                                 (lookup-key map (kbd "C-c C-a"))
                                 (lookup-key map [f5])
                                 hash cl-loop-result
                                 (hash-table-count ob)
                                 (get sym1 'neo-cx215-prop)
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (advice-remove 'neo-cx215-target 'mega-advice)
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1)))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx215_mega_5_all_subsystem_final_chaos_record_eieio_hash_coding_process_timer_env_marker_overlay_undo_narrow()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable weak-ht)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'cl-lib)
  (let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX215=v3" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX215"))))
      (exit-code (let ((p (make-process :name "neo-cx215-final-ec"
                                          :command '("sh" "-c" "exit 7")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx215-tag :a :b :c))
      (lexical-binding t))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (let* ((text "café 世界 final mega")
         (enc (encode-coding-string text 'utf-8))
         (hash (secure-hash 'sha256 enc))
         (buf (get-buffer-create " *neo-cx215-ultimate*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx215-local-counter 0)
      (insert text)
      (put-text-property 1 4 'face 'bold)
      (put-text-property 6 10 'display "XX")
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 16)
        (cl-incf neo-cx215-local-counter)
        (aset rec 2 :modified)
        (let ((cl-loop-result (cl-loop for k being the hash-keys of ht using (hash-values v)
                                        collect (cons k v))))
          (let ((macro-result (eval '(macroexpand '(if t :yes :no)) t)))
            (let ((snapshot (list timer-fired env-val exit-code
                                  (hash-table-count ht) (hash-table-count weak-ht)
                                  neo-cx215-local-counter
                                  (aref rec 0) (aref rec 1) (arec rec 2) (aref rec 3)
                                  hash cl-loop-result macro-result
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
                    (aref rec 2))))))))))
"##,
        expect,
    );
}
