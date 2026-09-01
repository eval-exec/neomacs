//! Complex combo batch 333 — final MEGA: 3 extreme integration stress
//! tests covering all subsystems for maximum divergence surface coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx333_mega_1_full_subsystem_ultimate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX333=v1" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX333"))))
      (exit-code (let ((p (make-process :name "neo-cx333-ec1"
                                          :command '("sh" "-c" "exit 5")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx333-tag :a :b :c))
      (lexical-binding t)
      (calls nil))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (defun neo-cx333-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx333-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (let* ((text "café 世界 😀 mega-333")
         (enc (encode-coding-string text 'utf-8))
         (hash (secure-hash 'sha256 enc))
         (cl-loop-result (cl-loop for k being the hash-keys of ht using (hash-values v)
                                   collect (cons k v)))
         (macro-result (eval '(macroexpand '(if t :yes :no)) t)))
    (let ((buf (get-buffer-create " *neo-cx333-mega-1*")))
      (with-current-buffer buf
        (buffer-enable-undo)
        (setq-local neo-cx333-counter 0)
        (insert text)
        (put-text-property 1 4 'face 'bold)
        (put-text-property 6 10 'display "XX")
        (add-to-invisibility-spec 'neo-cx333-h)
        (let ((m (set-marker (make-marker) 8))
              (invis-ov (make-overlay 3 8))
              (face-ov (make-overlay 6 14)))
          (overlay-put invis-ov 'invisible 'neo-cx333-h)
          (overlay-put face-ov 'face 'italic)
          (overlay-put face-ov 'priority 5)
          (narrow-to-region 2 16)
          (cl-incf neo-cx333-counter)
          (aset rec 2 :modified)
          (delete-region 5 8)
          (insert "MEGA")
          (cl-incf neo-cx333-counter)
          (let ((r (neo-cx333-target 5)))
            (let ((state (list timer-fired env-val exit-code
                               (hash-table-count ht) (hash-table-count weak-ht)
                               r hash cl-loop-result macro-result
                               neo-cx333-counter
                               (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                               (buffer-string)
                               (marker-position m)
                               (overlay-start invis-ov) (overlay-end invis-ov)
                               (overlay-start face-ov) (overlay-end face-ov)
                               (text-properties-at 1)
                               (get-char-property 7 'face))))
              (undo) (undo) (undo)
              (widen)
              (advice-remove 'neo-cx333-target 'mega-advice)
              (kill-buffer buf)
              (list state (buffer-live-p buf))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx333_mega_2_pcase_rx_syntax_advice_register_window_format_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 30 30 14 16 6 2026 nil)))
  (defun neo-cx333-target () :orig)
  (advice-add 'neo-cx333-target :override (lambda () :overridden))
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
             (advice-result (neo-cx333-target())))
        (let ((snapshot (list matches pcase-result time-str advice-result
                              (buffer-string)
                              (marker-position m)
                              (overlay-start ov) (overlay-end ov)
                              (text-properties-at 1))))
          (undo) (undo)
          (widen)
          (jump-to-register ?c)
          (advice-remove 'neo-cx333-target (advice--p (advice-member-p nil 'neo-cx333-target)))
          (list snapshot (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx333_mega_3_process_buflocal_coding_env_timer_weak_hash_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX333=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX333"))))
      (exit-code (let ((p (make-process :name "neo-cx333-ec2"
                                          :command '("sh" "-c" "exit 8")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx333-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx333-buf :local)
      (insert "Mega 333 café 世界 buffer content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx333-mega-p"
                           :command '("sh" "-c" "printf 'MEGA333'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            neo-cx333-buf
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
