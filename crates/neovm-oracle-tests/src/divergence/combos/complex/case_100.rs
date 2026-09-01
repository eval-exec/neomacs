//! Complex combo batch 100 — MEGA multi-subsystem combos across text
//! properties / overlays / buffer-local variables / narrowing / undo /
//! markers / processes / timers / hooks. Maximum integration stress.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx100_mega_textprop_overlay_buflocal_marker_undo_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx100-mega-1*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (setq-local neo-cx100-counter 0)
    (insert "Header content for mega combo test")
    (put-text-property 1 6 'face 'bold)
    (put-text-property 8 14 'display "XX")
    (add-to-invisibility-spec 'neo-cx100-hide)
    (let ((m (set-marker (make-marker) 18))
          (invis-ov (make-overlay 5 10))
          (face-ov (make-overlay 12 25))
          (disp-ov (make-overlay 20 28)))
      (overlay-put invis-ov 'invisible 'neo-cx100-hide)
      (overlay-put face-ov 'face 'italic)
      (overlay-put face-ov 'priority 5)
      (overlay-put disp-ov 'display "[DISPLAY]")
      (undo-boundary)
      (narrow-to-region 3 30)
      (cl-incf neo-cx100-counter)
      (delete-region 7 11)
      (insert "INSERTED")
      (cl-incf neo-cx100-counter)
      (let ((state-1 (list neo-cx100-counter
                           (buffer-string)
                           (marker-position m)
                           (overlay-start invis-ov) (overlay-end invis-ov)
                           (overlay-start face-ov) (overlay-end face-ov)
                           (overlay-start disp-ov) (overlay-end disp-ov)
                           (text-properties-at 1)
                           (get-char-property 14 'face)
                           (get-char-property 22 'display))))
        (undo) (undo)
        (widen)
        (let ((state-2 (list neo-cx100-counter
                             (buffer-string)
                             (marker-position m)
                             (overlayp invis-ov) (overlay-start invis-ov)
                             (overlayp face-ov) (overlay-start face-ov)
                             (overlayp disp-ov) (overlay-start disp-ov)
                             (text-properties-at 1))))
          (kill-buffer buf)
          (list state-1 state-2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_process_timer_hook_buffer_undo_env_exitcode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (hook-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX100=v" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX100"))))
      (exit-code (let ((p (make-process :name "neo-cx100-ec"
                                          :command '("sh" "-c" "exit 11"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (add-hook 'after-change-functions
            (lambda (&rest _) (push :change hook-fired)) nil t)
  (let ((buf (get-buffer-create " *neo-cx100-mega-2*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "Process output test buffer")
      (put-text-property 1 7 'face 'bold))
    (let ((p (make-process :name "neo-cx100-mega-2-p"
                           :command '("sh" "-c" "printf 'CAFE'")
                           :buffer buf)))
      (accept-process-output p 1)
      (sit-for 0.05))
    (with-current-buffer buf
      (widen)
      (let ((state (list timer-fired env-val exit-code
                         (buffer-string)
                         (length hook-fired)
                         (text-properties-at 1))))
        (undo)
        (kill-buffer buf)
        (list state (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_eieio_clloop_textprop_marker_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx100-mega-obj ()
        ((items :initarg :items :initform nil)
         (count :initarg :count :initform 0)))
      (let ((obj (make-instance 'neo-cx100-mega-obj :items '("a" "b" "c"))))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert (cl-loop for x being the hash-values of
                           (let ((ht (make-hash-table)))
                             (dolist (i '("alpha" "beta" "gamma"))
                               (puthash i (length i) ht))
                             ht)
                           concat (format "%s\n" x)))
          (put-text-property 1 5 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 3 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (oset obj count (hash-table-count (make-hash-table)))
            (let ((state (list (slot-value obj 'items)
                               (slot-value obj 'count)
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
fn div_cx100_mega_coding_regex_format_print_circle_secure_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[Hello café 世界 😀] bytes=23 hash=e674b2aed8aed39df9e72cbea5d02dd760618a8220e24dc3b008826ee97b24d2\" \"(\\\"Hello café 世界 😀\\\" \\\"Hello caf\\\\303\\\\251 \\\\344\\\\270\\\\226\\\\347\\\\225\\\\214 \\\\360\\\\237\\\\230\\\\200\\\" \\\"Hello café 世界 😀\\\")\" t t 23 23)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "Hello café 世界 😀")
       (encoded (encode-coding-string text 'utf-8))
       (bytes (string-bytes encoded))
       (decoded (decode-coding-string encoded 'utf-8-unix))
       (hash (secure-hash 'sha256 encoded))
       (formatted (format "[%s] bytes=%d hash=%s" decoded bytes hash))
       (printed (let ((print-circle t)) (prin1-to-string (list text encoded decoded)))))
  (list formatted printed
        (string= text decoded)
        (equal text decoded)
        (length encoded) (string-bytes encoded)))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_window_config_marker_overlay_register_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx100-mega-3*")))
  (set-window-buffer (selected-window) buf)
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Window config mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 12))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (window-configuration-to-register ?c)
      (narrow-to-region 2 25)
      (let ((config (current-window-configuration)))
        (delete-region 5 10)
        (let ((state-1 (list (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
          (jump-to-register ?c)
          (widen)
          (let ((state-2 (list (buffer-string)
                               (marker-position m)
                               (overlayp ov) (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
            (undo)
            (kill-buffer buf)
            (list state-1 state-2))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_advice_clloop_closure_marker_overlay_undo_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defmacro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls nil))
  (cl-defmacro neo-cx100-trace (form)
    `(progn (push (list :entering ',form) calls) ,form))
  (defun neo-cx100-target (x) (* x 2))
  (advice-add 'neo-cx100-target :around
              (lambda (fn x)
                (push (list :around-enter x) calls)
                (let ((r (funcall fn x)))
                  (push (list :around-exit r) calls)
                  r)))
  (letrec ((acc 0)
           (loop-fn (lambda (n)
                      (when (> n 0)
                        (cl-incf acc (neo-cx100-target n))
                        (funcall loop-fn (1- n))))))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Closure advice mega test content")
      (put-text-property 1 7 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 3 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (funcall loop-fn 5)
        (let ((state (list acc (nreverse calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (advice-remove 'neo-cx100-target (advice--p (advice-member-p nil 'neo-cx100-target)))
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_pcase_rx_syntax_table_textprop_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
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
    (narrow-to-region 2 28)
    (goto-char 1)
    (let ((match-results
           (cl-loop for i from 0 below 3
                    while (re-search-forward "\\w+" nil t)
                    collect (match-string 0))))
      (let ((state (list match-results
                         (char-syntax (char-after 1))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_time_process_env_exitcode_buflocal_undo_invis_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((env-val (let ((process-environment (cons "NEO_CX100=v2" process-environment)))
                  (string-trim (shell-command-to-string "echo $NEO_CX100"))))
       (exit-code (let ((p (make-process :name "neo-cx100-mega-ec"
                                          :command '("sh" "-c" "exit 13"))))
                    (accept-process-output p 2)
                    (process-exit-status p)))
       (t0 (encode-time 0 0 12 15 6 2024 nil)))
  (with-temp-buffer
    (buffer-enable-undo)
    (setq-local neo-cx100-time t0)
    (insert (format-time-string "%Y-%m-%d %H:%M:%S" t0))
    (put-text-property 1 10 'face 'bold)
    (add-to-invisibility-spec 'neo-cx100-mega-h)
    (let ((m (set-marker (make-marker) 8))
          (invis-ov (make-overlay 4 12)))
      (overlay-put invis-ov 'invisible 'neo-cx100-mega-h)
      (narrow-to-region 2 18)
      (let ((state (list env-val exit-code
                         (format-time-string "%H:%M" neo-cx100-time)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start invis-ov) (overlay-end invis-ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start invis-ov) (overlay-end invis-ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_file_io_coding_charset_marker_overlay_undo_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx100-mega-file"))
       (data "Hello café 世界"))
  (delete-file path)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert data)
    (write-region (point-min) (point-max) path nil 'silent))
  (let ((attrs (file-attributes path)))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert-file-contents path)
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 14)
        (let ((state (list (file-attribute-size attrs)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (delete-file path)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_hash_table_obarray_symbol_plist_marker_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal))
      (ob (make-obarray 31))
      (sym (intern "neo-cx100-mega-sym")))
  (puthash :a 1 ht)
  (puthash :b 2 ht)
  (intern "alpha" ob)
  (intern "beta" ob)
  (put sym 'neo-cx100-prop :val)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Hash obarray plist mega test")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 16)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (let ((state (list (hash-table-count ht)
                         (hash-table-count ob)
                         (get sym 'neo-cx100-prop)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_kbd_macro_register_rectangle_window_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"AAA111\\nBBB222\\nCCC333\\n\" 5 2 10 #<marker in no buffer>) \"AAA111\\nBBB222\\nCCC333\\n\" 5 2 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((config (current-window-configuration)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "AAA111\nBBB222\nCCC333\n")
    (let ((m (set-marker (make-marker) 5))
          (ov (make-overlay 2 10)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (point-to-register ?p)
      (window-configuration-to-register ?w)
      (let ((state (list (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (get-register ?p))))
        (jump-to-register ?w)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov))))))
"##,
        expect,
    );
}

#[test]
fn div_cx100_mega_combination_eleven_subsystems_full_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX100=v3" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX100"))))
      (exit-code (let ((p (make-process :name "neo-cx100-final"
                                          :command '("sh" "-c" "exit 7"))))
                   (accept-process-output p 2)
                   (process-exit-status p)))
      (ht (make-hash-table :test 'equal))
      (rec (record 'neo-cx100-final-tag :a :b :c)))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (let ((buf (get-buffer-create " *neo-cx100-final-mega*")))
    (with-current-buffer buf
      (buffer-enable-undo)
      (setq-local neo-cx100-local-counter 0)
      (insert "Final mega combo content")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 7 12 'display "XX")
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (cl-incf neo-cx100-local-counter)
        (aset rec 2 :modified)
        (let ((state (list timer-fired env-val exit-code
                           (hash-table-count ht)
                           (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                           neo-cx100-local-counter
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1)
                           (text-properties-at 6))))
          (undo)
          (widen)
          (kill-buffer buf)
          (list state
                (buffer-live-p buf)
                (hash-table-count ht)
                (aref rec 2))))))
"##,
        expect,
    );
}
