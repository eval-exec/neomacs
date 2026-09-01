//! Complex combo batch 200 — MEGA 200th batch milestone: combine every
//! major subsystem axis into 5 extreme stress tests.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx200_mega_milestone_1_coding_charset_undo_overlay_marker_narrow_process_timer_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 2 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX200=v1" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX200"))))
      (exit-code (let ((p (make-process :name "neo-cx200-ec1"
                                          :command '("sh" "-c" "exit 5")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let* ((text "café 世界 😀 mega")
         (enc (encode-coding-string text 'utf-8))
         (hash (secure-hash 'sha256 enc))
         (buf (get-buffer-create " *neo-cx200-mega-1*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx200-counter 0)
      (insert text)
      (put-text-property 1 4 'face 'bold)
      (put-text-property 6 10 'display "XX")
      (add-to-invisibility-spec 'neo-cx200-h)
      (let ((m (set-marker (make-marker) 8))
            (invis-ov (make-overlay 3 8))
            (face-ov (make-overlay 6 14)))
        (overlay-put invis-ov 'invisible 'neo-cx200-h)
        (overlay-put face-ov 'face 'italic)
        (overlay-put face-ov 'priority 5)
        (narrow-to-region 2 16)
        (cl-incf neo-cx200-counter)
        (delete-region 5 8)
        (insert "MEGA")
        (cl-incf neo-cx200-counter)
        (let ((state (list timer-fired env-val exit-code
                           (hash-table-count weak-ht)
                           neo-cx200-counter
                           hash (length enc)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start invis-ov) (overlay-end invis-ov)
                           (overlay-start face-ov) (overlay-end face-ov)
                           (text-properties-at 1)
                           (get-char-property 7 'face))))
          (undo) (undo) (undo)
          (widen)
          (kill-buffer buf)
          (list state (buffer-live-p buf)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx200_mega_milestone_2_eieio_clloop_pcase_advice_closure_coding_obarray_secure_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx200-obj ()
        ((items :initarg :items :initform nil)
         (count :initarg :count :initform 0)))
      (let ((lexical-binding t)
            (calls nil))
        (defun neo-cx200-target (x) (push (list :primary x) calls) (* x 2))
        (advice-add 'neo-cx200-target :before
                    (lambda (x) (push (list :before x) calls))
                    '((name . mega-advice)))
        (let* ((text "Hello 世界 café")
               (enc (encode-coding-string text 'utf-8))
               (hash (secure-hash 'sha256 enc))
               (ob (make-obarray 31))
               (sym1 (intern "neo-cx200-alpha" ob))
               (sym2 (intern "neo-cx200-beta" ob)))
          (put sym1 'neo-cx200-prop :v1)
          (put sym2 'neo-cx200-prop :v2)
          (let ((ht (make-hash-table :test 'equal)))
            (puthash sym1 1 ht)
            (puthash sym2 2 ht)
            (let* ((obj (make-instance 'neo-cx200-obj :items ht :count (hash-table-count ht)))
                   (cl-loop-result
                    (cl-loop for k being the hash-keys of ht using (hash-values v)
                             collect (cons (symbol-name k) v)))
                   (macro-result (eval '(macroexpand '(if t :yes :no)) t))
                   (target-result (neo-cx200-target 21))
                   (pcase-result
                    (pcase cl-loop-result
                      (`(,a ,b) (list :two-pairs a b))
                      (_ :other))))
              (let ((snapshot (list text enc hash
                                    (string= text (decode-coding-string enc 'utf-8-unix))
                                    (hash-table-count ob)
                                    (get sym1 'neo-cx200-prop)
                                    (slot-value obj 'count)
                                    cl-loop-result pcase-result
                                    target-result macro-result
                                    (nreverse calls))))
                (advice-remove 'neo-cx200-target 'mega-advice)
                snapshot))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx200_mega_milestone_3_process_buflocal_textprop_overlay_marker_undo_narrow_coding_env_exitcode_timer()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX200=v2" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX200"))))
      (exit-code (let ((p (make-process :name "neo-cx200-ec2"
                                          :command '("sh" "-c" "exit 9"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx200-mega-3*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx200-buf :local)
      (insert "Mega 3 buffer café 世界 content")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 8 14 'display "XX"))
    (let ((p (make-process :name "neo-cx200-mega-3-p"
                           :command '("sh" "-c" "printf 'MEGA3'")
                           :buffer buf)))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((snapshot (with-current-buffer buf
                      (widen)
                      (list timer-fired env-val exit-code
                            neo-cx200-buf
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
fn div_cx200_mega_milestone_4_pcase_rx_syntax_search_replace_format_time_advice_clloop_register_window_config()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (t0 (encode-time 30 30 14 16 6 2026 nil))
      (calls nil))
  (defun neo-cx200-target () :orig)
  (advice-add 'neo-cx200-target :override (lambda () :overridden))
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
             (advice-result (neo-cx200-target))
             (snapshot (list matches pcase-result time-str advice-result
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
        (undo) (undo)
        (widen)
        (jump-to-register ?c)
        (advice-remove 'neo-cx200-target (advice--p (advice-member-p nil 'neo-cx200-target)))
        (list snapshot (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

#[test]
fn div_cx200_mega_milestone_5_all_twelve_subsystems_full_chaos_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX200=v3" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX200"))))
      (exit-code (let ((p (make-process :name "neo-cx200-final-ec"
                                          :command '("sh" "-c" "exit 7")))
                       (weak-ht (make-hash-table :weakness 'key :test 'eq)))
        (puthash (cons 1 nil) :v weak-ht)
        (garbage-collect)
        (accept-process-output p 2)
        (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx200-tag :a :b :c))
      (lexical-binding t)
      (calls nil))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (defun neo-cx200-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx200-target :before
              (lambda (x) (push (list :before x) calls))
              '((name . mega-advice)))
  (let ((buf (get-buffer-create " *neo-cx200-final-mega*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx200-local-counter 0)
      (insert "Final mega combo café 世界 content")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (cl-incf neo-cx200-local-counter)
        (aset rec 2 :modified)
        (let ((target-result (neo-cx200-target 5))
              (macro-result (eval '(macroexpand '(if t :yes :no)) t))
              (cl-loop-result (cl-loop for k being the hash-keys of ht using (hash-values v)
                                        collect (cons k v))))
          (let ((snapshot (list timer-fired env-val exit-code
                                (hash-table-count ht) (hash-table-count weak-ht)
                                neo-cx200-local-counter
                                (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                                target-result macro-result cl-loop-result
                                (buffer-string)
                                (marker-position m)
                                (overlay-start ov) (overlay-end ov)
                                (text-properties-at 1)
                                (text-properties-at 6)
                                (get-char-property 7 'display))))
            (undo)
            (widen)
            (advice-remove 'neo-cx200-target 'mega-advice)
            (kill-buffer buf)
            (list snapshot
                  (buffer-live-p buf)
                  (hash-table-count ht)
                  (aref rec 2))))))))
"##,
        expect,
    );
}
