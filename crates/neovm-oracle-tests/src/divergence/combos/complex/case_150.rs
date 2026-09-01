//! Complex combo batch 150 — MEGA combo across all subsystems: maximum
//! integration stress combining 10+ subsystems per test.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx150_mega_buflocal_textprop_overlay_marker_undo_narrow_process_env_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX150=v" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX150"))))
      (exit-code (let ((p (make-process :name "neo-cx150-ec"
                                          :command '("sh" "-c" "exit 12"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx150-mega-1*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx150-counter 0)
      (insert "Mega combo 1 buffer content")
      (put-text-property 1 4 'face 'bold)
      (put-text-property 6 12 'display "XX")
      (add-to-invisibility-spec 'neo-cx150-h)
      (let ((m (set-marker (make-marker) 14))
            (invis-ov (make-overlay 4 10))
            (face-ov (make-overlay 8 16)))
        (overlay-put invis-ov 'invisible 'neo-cx150-h)
        (overlay-put face-ov 'face 'italic)
        (overlay-put face-ov 'priority 5)
        (narrow-to-region 2 22)
        (cl-incf neo-cx150-counter)
        (delete-region 5 8)
        (insert "MEGA")
        (cl-incf neo-cx150-counter)
        (let ((state (list timer-fired env-val exit-code
                           neo-cx150-counter
                           (buffer-string)
                           (marker-position m)
                           (overlay-start invis-ov) (overlay-end invis-ov)
                           (overlay-start face-ov) (overlay-end face-ov)
                           (text-properties-at 1)
                           (get-char-property 8 'face))))
          (undo) (undo)
          (widen)
          (kill-buffer buf)
          (list state)))))
"##,
        expect,
    );
}

#[test]
fn div_cx150_mega_clloop_eieio_hash_pcase_textprop_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx150-obj ()
        ((items :initarg :items :initform nil)
         (count :initarg :count :initform 0)))
      (let ((ht (make-hash-table :test 'equal)))
        (puthash "alpha" 1 ht)
        (puthash "beta" 2 ht)
        (puthash "gamma" 3 ht)
        (let ((obj (make-instance 'neo-cx150-obj :items ht :count (hash-table-count ht))))
          (with-temp-buffer
            (buffer-enable-undo)
            (insert (cl-loop for k being the hash-keys of ht using (hash-values v)
                             concat (format "%s=%d\n" k v)))
            (put-text-property 1 5 'face 'bold)
            (let ((m (set-marker (make-marker) 8))
                  (ov (make-overlay 4 14)))
              (overlay-put ov 'face 'italic)
              (overlay-put ov 'evaporate t)
              (narrow-to-region 2 18)
              (let ((pcase-result
                     (pcase (slot-value obj 'count)
                       ((and (pred integerp) (pred (> _ 2))) :big)
                       (_ :small)))
                    (state (list pcase-result
                                 (hash-table-count (slot-value obj 'items))
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx150_mega_eval_macro_closure_marker_overlay_undo_narrow_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (timer-fired nil))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((exit-code (let ((p (make-process :name "neo-cx150-eval-ec"
                                          :command '("sh" "-c" "exit 3"))))
                     (accept-process-output p 2)
                     (process-exit-status p))))
    (sit-for 0.01)
    (letrec ((counter 0)
             (inc (lambda () (cl-incf counter))))
      (let ((buf (get-buffer-create " *neo-cx150-eval-mega*")))
        (with-current-buffer buf
          (buffer-enable-undo)
          (insert "Eval/macro mega test buffer")
          (put-text-property 1 5 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)))
        (let ((p (make-process :name "neo-cx150-eval-p"
                               :command '("sh" "-c" "printf 'EVAL'")
                               :buffer buf)))
          (accept-process-output p 1)
          (sit-for 0.05))
        (let ((macro-result (eval '(macroexpand '(if t :yes :no)) t)))
          (with-current-buffer buf
            (widen)
            (funcall inc) (funcall inc) (funcall inc)
            (let ((state (list timer-fired exit-code
                               counter macro-result
                               (buffer-string)
                               (length (overlays-in 1 20))
                               (text-properties-at 1))))
              (undo)
              (kill-buffer buf)
              (list state (buffer-string)))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx150_mega_coding_charset_print_circle_secure_hash_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "Hello café 世界 😀")
       (encoded (encode-coding-string text 'utf-8))
       (bytes (string-bytes encoded))
       (decoded (decode-coding-string encoded 'utf-8-unix))
       (hash (secure-hash 'sha256 encoded))
       (shared (list 1 2 3))
       (circular-data (list shared shared (list :a :b)))
       (printed-circle (let ((print-circle t)) (prin1-to-string circular-data)))
       (ob (make-obarray 31))
       (sym1 (intern "neo-cx150-alpha" ob))
       (sym2 (intern "neo-cx150-beta" ob)))
  (put sym1 'neo-cx150-prop :val1)
  (put sym2 'neo-cx150-prop :val2)
  (list text encoded decoded hash bytes
        (string= text decoded) (equal text decoded)
        printed-circle
        (hash-table-count ob)
        (get sym1 'neo-cx150-prop)
        (get sym2 'neo-cx150-prop)))
"##,
        expect,
    );
}

#[test]
fn div_cx150_mega_advice_kmacro_register_window_config_buflocal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx150-mega-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx150-mega-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (let ((buf (get-buffer-create " *neo-cx150-mega-2*")))
    (set-window-buffer (selected-window) buf)
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx150-buf-local :val)
      (insert "Window config mega test content")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (window-configuration-to-register ?c)
        (narrow-to-region 2 18)
        (let ((r (neo-cx150-mega-target 21)))
          (let ((state (list r (nreverse calls)
                             neo-cx150-buf-local
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (jump-to-register ?c)
            (widen)
            (advice-remove 'neo-cx150-mega-target 'mega-advice)
            (kill-buffer buf)
            (list state (buffer-live-p buf)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx150_mega_subprocess_marker_overlay_textprop_undo_narrow_env_exitcode_timer_weak_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX150=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX150"))))
      (exit-code (let ((p (make-process :name "neo-cx150-final-ec"
                                          :command '("sh" "-c" "exit 5")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (set-process-query-on-exit-flag p nil)
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p)))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx150-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "Final mega combo buffer content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx150-final-p"
                           :command '("sh" "-c" "printf 'FINAL'")
                           :buffer buf)))
      (set-process-sentinel p #'ignore)
      (set-process-query-on-exit-flag p nil)
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (with-current-buffer buf
      (widen)
      (let ((state (list timer-fired env-val exit-code
                         (buffer-string) (length (buffer-string))
                         (text-properties-at 1)
                         (text-properties-at 8)
                         (length (overlays-in 1 20)))))
        (undo)
        (kill-buffer buf)
        (list state (buffer-string))))))
"##,
        expect,
    );
}
