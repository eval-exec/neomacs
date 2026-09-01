//! Complex combo batch 192 — MEGA final-final integration: 5 stress tests
//! each combining 10+ subsystems for maximum coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx192_mega_1_buflocal_undo_textprop_overlay_marker_narrow_process_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX192=v1" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX192"))))
      (exit-code (let ((p (make-process :name "neo-cx192-ec1"
                                          :command '("sh" "-c" "exit 4"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx192-mega-1*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx192-counter 0)
      (insert "MEGA 1 café 世界 buffer")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (add-to-invisibility-spec 'neo-cx192-h)
      (let ((m (set-marker (make-marker) 10))
            (invis-ov (make-overlay 4 10))
            (face-ov (make-overlay 8 16)))
        (overlay-put invis-ov 'invisible 'neo-cx192-h)
        (overlay-put face-ov 'face 'italic)
        (overlay-put face-ov 'priority 5)
        (narrow-to-region 2 20)
        (cl-incf neo-cx192-counter)
        (delete-region 5 8)
        (insert "MEGA")
        (cl-incf neo-cx192-counter)
        (let ((state (list timer-fired env-val exit-code
                           neo-cx192-counter
                           (buffer-string)
                           (marker-position m)
                           (overlay-start invis-ov) (overlay-end invis-ov)
                           (overlay-start face-ov) (overlay-end face-ov)
                           (text-properties-at 1))))
          (undo) (undo)
          (widen)
          (kill-buffer buf)
          (list state (buffer-live-p buf)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx192_mega_2_eieio_clloop_closure_coding_charset_secure_hash_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café 世界 😀\" \"café 世界 😀\" \"café 世界 😀\" \"17941ef6be03a9dd6fc80b897dd78db65f774c5fb06a8a821892535f1bd2ecbc\" t 2 ((\"alpha\" . 1) (\"beta\" . 2)) 42 (if t :yes :no) ((:before 21) (:primary 21)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx192-obj ()
        ((items :initarg :items :initform nil)
         (count :initarg :count :initform 0)))
      (let ((lexical-binding t)
            (calls nil))
        (defun neo-cx192-target (x) (push (list :primary x) calls) (* x 2))
        (advice-add 'neo-cx192-target :before
                    (lambda (x) (push (list :before x) calls))
                    '((name . mega-advice)))
        (let* ((text "café 世界 😀")
               (encoded (encode-coding-string text 'utf-8))
               (decoded (decode-coding-string encoded 'utf-8-unix))
               (hash (secure-hash 'sha256 encoded))
               (ht (make-hash-table :test 'equal)))
          (puthash "alpha" 1 ht)
          (puthash "beta" 2 ht)
          (let ((obj (make-instance 'neo-cx192-obj :items ht :count (hash-table-count ht))))
            (let ((cl-loop-result
                   (cl-loop for k being the hash-keys of ht using (hash-values v)
                            collect (cons k v))))
              (let* ((target-result (neo-cx192-target 21))
                     (macro-result (eval '(macroexpand '(if t :yes :no)) t))
                     (snapshot (list text encoded decoded hash
                                     (string= text decoded)
                                     (slot-value obj 'count)
                                     cl-loop-result
                                     target-result macro-result (nreverse calls))))
                (advice-remove 'neo-cx192-target 'mega-advice)
                snapshot))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx192_mega_3_pcase_rx_syntax_textprop_marker_overlay_register_window_config() {
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
fn div_cx192_mega_4_process_buflocal_undo_textprop_marker_overlay_narrow_coding_env_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX192=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX192"))))
      (exit-code (let ((p (make-process :name "neo-cx192-ec2"
                                          :command '("sh" "-c" "exit 8")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx192-final*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx192-buf :local)
      (insert "Final mega combo buffer content café 世界")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx192-final-p"
                           :command '("sh" "-c" "printf 'FINAL'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            neo-cx192-buf
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
fn div_cx192_mega_5_search_replace_format_time_buflocal_undo_invis_textprop_clloop_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 0 30 14 16 6 2026 nil))
      (calls nil))
  (defun neo-cx192-search-target () :orig)
  (advice-add 'neo-cx192-search-target :override (lambda () :overridden))
  (with-temp-buffer
    (buffer-enable-undo)
    (setq-local neo-cx192-buf :active)
    (insert "alpha 123 BETA 456 gamma 789 delta 012 epsilon")
    (put-text-property 1 5 'face 'bold)
    (add-to-invisibility-spec 'neo-cx192-h)
    (let ((invis-ov (make-overlay 15 25)))
      (overlay-put invis-ov 'invisible 'neo-cx192-h)
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
               (advice-result (neo-cx192-search-target))
               (snapshot (list neo-cx192-buf
                               cl-loop-result time-str advice-result
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (point-min) (point-max)
                               (text-properties-at 1))))
          (undo) (undo)
          (widen)
          (advice-remove 'neo-cx192-search-target (advice--p (advice-member-p nil 'neo-cx192-search-target)))
          (list snapshot (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
