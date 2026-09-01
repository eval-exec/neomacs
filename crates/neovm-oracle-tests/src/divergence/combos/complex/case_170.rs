//! Complex combo batch 170 — MEGA integration across process / eieio /
//! hash / textprop / undo / narrowing with multi-buffer interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx170_mega_eieio_clloop_hash_marker_overlay_undo_narrow_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx170-state ()
        ((items :initarg :items :initform nil)
         (count :initarg :count :initform 0)))
      (let* ((env-val (let ((process-environment (cons "NEO_CX170=v" process-environment)))
                        (string-trim (shell-command-to-string "echo $NEO_CX170"))))
             (exit-code (let ((p (make-process :name "neo-cx170-ec"
                                                :command '("sh" "-c" "exit 5"))))
                          (accept-process-output p 2)
                          (process-exit-status p)))
             (ht (make-hash-table :test 'equal)))
        (puthash "alpha" 1 ht)
        (puthash "beta" 2 ht)
        (puthash "gamma" 3 ht)
        (let ((state-obj (make-instance 'neo-cx170-state :items ht :count (hash-table-count ht))))
          (with-temp-buffer
            (buffer-enable-undo)
            (insert (cl-loop for k being the hash-keys of ht using (hash-values v)
                             concat (format "%s=%d\n" k v)))
            (put-text-property 1 5 'face 'bold)
            (let ((m (set-marker (make-marker) 10))
                  (ov (make-overlay 4 20)))
              (overlay-put ov 'face 'italic)
              (overlay-put ov 'evaporate t)
              (narrow-to-region 2 25)
              (oset state-obj count (hash-table-count ht))
              (let ((snapshot (list env-val exit-code
                                    (slot-value state-obj 'count)
                                    (buffer-string)
                                    (marker-position m)
                                    (overlay-start ov) (overlay-end ov)
                                    (text-properties-at 1))))
                (undo) (undo)
                (widen)
                (list snapshot (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx170_mega_subprocess_buflocal_textprop_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx170-mega-2*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (setq-local neo-cx170-counter 0)
    (insert "Subprocess mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (put-text-property 8 14 'display "XX")
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)))
  (let ((p (make-process :name "neo-cx170-mega-p"
                         :command '("sh" "-c" "printf 'SUBPROC'")
                         :buffer buf)))
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (sit-for 0.05))
  (with-current-buffer buf
    (widen)
    (let ((state (list neo-cx170-counter
                       (buffer-string)
                       (length (buffer-string))
                       (length (overlays-in 1 20))
                       (text-properties-at 1)
                       (text-properties-at 8))))
      (undo)
      (kill-buffer buf)
      (list state (buffer-live-p buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx170_mega_advice_closure_eval_macro_kbd_register_window_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls nil))
  (defun neo-cx170-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx170-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (letrec ((counter 0)
           (inc (lambda () (cl-incf counter))))
    (let ((buf (get-buffer-create " *neo-cx170-window-mega*")))
      (set-window-buffer (selected-window) buf)
      (with-current-buffer buf
        (buffer-enable-undo)
        (insert "Window mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (window-configuration-to-register ?c)
          (narrow-to-region 2 18)
          (funcall inc) (funcall inc) (funcall inc)
          (let ((r (neo-cx170-target 21))
                (macro-result (eval '(macroexpand '(if t :yes :no)) t)))
            (let ((snapshot (list r counter macro-result (nreverse calls)
                                  (buffer-string)
                                  (marker-position m)
                                  (overlay-start ov) (overlay-end ov)
                                  (text-properties-at 1))))
              (jump-to-register ?c)
              (widen)
              (advice-remove 'neo-cx170-target 'mega-advice)
              (kill-buffer buf)
              (list snapshot (buffer-live-p buf))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx170_mega_coding_charset_print_circle_secure_hash_obarray_cl_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café 世界 😀")
       (encoded (encode-coding-string text 'utf-8))
       (decoded (decode-coding-string encoded 'utf-8-unix))
       (hash (secure-hash 'sha256 encoded))
       (shared (list 1 2 3))
       (data (list shared shared))
       (printed-circle (let ((print-circle t)) (prin1-to-string data)))
       (ob (make-obarray 31))
       (sym1 (intern "neo-cx170-alpha" ob))
       (sym2 (intern "neo-cx170-beta" ob)))
  (put sym1 'neo-cx170-prop :v1)
  (put sym2 'neo-cx170-prop :v2)
  (let ((ht (make-hash-table :test 'equal)))
    (puthash sym1 1 ht)
    (puthash sym2 2 ht)
    (let ((cl-loop-result
           (cl-loop for k being the hash-keys of ht using (hash-values v)
                    collect (cons (symbol-name k) v))))
      (list text encoded decoded hash
            (string= text decoded)
            printed-circle
            (hash-table-count ob)
            (get sym1 'neo-cx170-prop)
            cl-loop-result))))
"##,
        expect,
    );
}

#[test]
fn div_cx170_mega_full_eleven_subsystem_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX170=v3" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX170"))))
      (exit-code (let ((p (make-process :name "neo-cx170-mega-ec"
                                          :command '("sh" "-c" "exit 7"))))
                   (accept-process-output p 2)
                   (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx170-mega-tag :a :b :c)))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (let ((buf (get-buffer-create " *neo-cx170-final-mega*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx170-local-counter 0)
      (insert "Final mega combo content")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (cl-incf neo-cx170-local-counter)
        (aset rec 2 :modified)
        (let ((snapshot (list timer-fired env-val exit-code
                              (hash-table-count ht)
                              (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                              neo-cx170-local-counter
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
                (aref rec 2)))))))
"##,
        expect,
    );
}
