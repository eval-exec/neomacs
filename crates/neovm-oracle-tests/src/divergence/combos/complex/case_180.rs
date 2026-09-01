//! Complex combo batch 180 — MEGA final integration: combine 12+ subsystems
//! across every major axis to stress-test the parity surface.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx180_mega_all_subsystem_chaos_stress_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX180=v1" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX180"))))
      (exit-code (let ((p (make-process :name "neo-cx180-ec1"
                                          :command '("sh" "-c" "exit 4"))))
                   (accept-process-output p 2)
                   (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (weak-ht (make-hash-table :weakness 'key :test 'eq))
      (rec (record 'neo-cx180-tag :a :b :c)))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (puthash (cons 1 nil) :v weak-ht)
  (garbage-collect)
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (let ((buf (get-buffer-create " *neo-cx180-mega-1*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx180-counter 0)
      (insert "MEGA 1 buffer content café 世界")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (add-to-invisibility-spec 'neo-cx180-h)
      (let ((m (set-marker (make-marker) 10))
            (invis-ov (make-overlay 4 10))
            (face-ov (make-overlay 8 16)))
        (overlay-put invis-ov 'invisible 'neo-cx180-h)
        (overlay-put face-ov 'face 'italic)
        (overlay-put face-ov 'priority 5)
        (narrow-to-region 2 20)
        (cl-incf neo-cx180-counter)
        (aset rec 2 :modified)
        (delete-region 5 8)
        (insert "CHAOS")
        (cl-incf neo-cx180-counter)
        (let ((state (list timer-fired env-val exit-code
                           (hash-table-count ht) (hash-table-count weak-ht)
                           neo-cx180-counter
                           (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start invis-ov) (overlay-end invis-ov)
                           (overlay-start face-ov) (overlay-end face-ov)
                           (text-properties-at 1)
                           (get-char-property 8 'face))))
          (undo) (undo) (undo)
          (widen)
          (kill-buffer buf)
          (list state
                (buffer-live-p buf)
                (hash-table-count ht)
                (aref rec 2)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx180_mega_all_subsystem_chaos_stress_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx180-obj ()
        ((value :initarg :value :initform 0)))
      (let ((lexical-binding t)
            (timer-fired nil)
            (env-val (let ((process-environment (cons "NEO_CX180=v2" process-environment)))
                       (string-trim (shell-command-to-string "echo $NEO_CX180"))))
            (exit-code (let ((p (make-process :name "neo-cx180-ec2"
                                                :command '("sh" "-c" "exit 6")))
                             (weak-ht (make-hash-table :weakness 'key :test 'eq)))
                         (puthash (cons 1 nil) :v weak-ht)
                         (garbage-collect)
                         (accept-process-output p 2)
                         (process-exit-status p))))
        (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
        (sit-for 0.01)
        (letrec ((counter 0)
                 (inc-fn (lambda () (cl-incf counter))))
          (let ((obj (make-instance 'neo-cx180-obj :value 0))
                (shared-list (list 1 2 3)))
            (let ((buf (get-buffer-create " *neo-cx180-mega-2*")))
              (set-window-buffer (selected-window) buf)
              (with-current-buffer buf
                (buffer-enable-undo)
                (insert (format "MEGA 2: %s" (let ((print-circle t))
                                                (prin1-to-string (list shared-list shared-list)))))
                (put-text-property 1 5 'face 'bold))
              (window-configuration-to-register ?c)
              (let ((p (make-process :name "neo-cx180-mega-p"
                                     :command '("sh" "-c" "printf 'MEGA2'")
                                     :buffer buf)))
                (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
                (accept-process-output p 1)
                (sit-for 0.05))
              (with-current-buffer buf
                (widen)
                (funcall inc-fn) (funcall inc-fn) (funcall inc-fn)
                (oset obj value counter)
                (let ((state (list timer-fired env-val exit-code
                                   counter
                                   (slot-value obj 'value)
                                   (buffer-string) (length (buffer-string))
                                   (text-properties-at 1)
                                   (length (overlays-in 1 20)))))
                  (undo)
                  (jump-to-register ?c)
                  (kill-buffer buf)
                  (list state (buffer-live-p buf)))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx180_mega_advice_clloop_closure_coding_charset_obarray_secure_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls nil))
  (defun neo-cx180-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx180-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (let* ((text "café 世界 😀")
         (encoded (encode-coding-string text 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8-unix))
         (hash (secure-hash 'sha256 encoded))
         (ob (make-obarray 31))
         (sym1 (intern "neo-cx180-alpha" ob))
         (sym2 (intern "neo-cx180-beta" ob)))
    (put sym1 'neo-cx180-prop :v1)
    (put sym2 'neo-cx180-prop :v2)
    (let ((cl-loop-result
           (cl-loop for k being the hash-keys of
                    (let ((ht (make-hash-table :test 'equal)))
                      (puthash sym1 1 ht)
                      (puthash sym2 2 ht)
                      ht)
                    using (hash-values v)
                    collect (cons (symbol-name k) v))))
      (let* ((target-result (neo-cx180-target 21))
             (macro-result (eval '(macroexpand '(if t :yes :no)) t))
             (snapshot (list text encoded decoded hash
                             (string= text decoded)
                             (hash-table-count ob)
                             (get sym1 'neo-cx180-prop)
                             cl-loop-result
                             target-result macro-result (nreverse calls))))
        (advice-remove 'neo-cx180-target 'mega-advice)
        snapshot))))
"##,
        expect,
    );
}

#[test]
fn div_cx180_mega_pcase_rx_syntax_textprop_marker_overlay_register_window_config() {
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
fn div_cx180_mega_search_replace_format_time_buflocal_undo_invis_textprop_clloop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 0 30 14 16 6 2026 nil)))
  (with-temp-buffer
    (buffer-enable-undo)
    (setq-local neo-cx180-buf :active)
    (insert "alpha 123 BETA 456 gamma 789 delta 012 epsilon")
    (put-text-property 1 5 'face 'bold)
    (add-to-invisibility-spec 'neo-cx180-h)
    (let ((invis-ov (make-overlay 15 25)))
      (overlay-put invis-ov 'invisible 'neo-cx180-h)
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
               (snapshot (list neo-cx180-buf
                               cl-loop-result time-str
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (point-min) (point-max)
                               (text-properties-at 1)
                               (get-char-property 20 'invisible))))
          (undo) (undo)
          (widen)
          (list snapshot (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
