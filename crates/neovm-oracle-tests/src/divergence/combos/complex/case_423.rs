//! Complex combo batch 423 — 20 probes targeting remaining edge cases:
//! process-send-eof, overlay-recenter, with-propertized-buffer-substring,
//! keymap-unset/canonicalize, terminal-parameter, buffer-swap-text
//! with narrowing, window-state-put with params, font-driver-available-p,
//! encode-coding-region, read with different readtables, print-gensym,
//! eval-after-load deeper, process-send-region, marker with undo,
//! overlay-recenter, and make-frame on tty deeper.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// process-send-eof: sending EOF to a process.
#[test]
fn div_cx423_process_send_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\\n\\nProcess neo-cx423-eof finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *cx423-eof*")))
  (let ((proc (make-process :name "neo-cx423-eof"
                            :command '("cat")
                            :connection-type 'pipe :buffer buf)))
    (process-send-string proc "hello\n")
    (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (prog1 (with-current-buffer buf
             (string-trim-right (buffer-string)))
      (kill-buffer buf))))
"##,
        expect,
    );
}

/// overlay-recenter: rearranging overlay list.
#[test]
fn div_cx423_overlay_recenter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (let ((o1 (make-overlay 2 4)) (o2 (make-overlay 6 8)))
    (overlay-recenter (point-max))
    (length (overlays-in 1 10))))
"##,
        expect,
    );
}

/// with-propertized-buffer-substring: buffer text with
/// properties without font-lock interference.
#[test]
fn div_cx423_with_propertized_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-function with-propertized-buffer-substring)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 3 4 'face 'italic)
  (let ((sub (with-propertized-buffer-substring (point-min) (point-max))))
    (list (length sub)
          (text-properties-at 0 sub)
          (text-properties-at 2 sub))))
"##,
        expect,
    );
}

/// keymap-unset / keymap-canonicalize.
#[test]
fn div_cx423_keymap_unset_canonicalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (keymap (98 . backward-char) (97))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map "a" 'forward-char)
  (define-key map "b" 'backward-char)
  (keymap-unset map "a" nil)
  (keymap-canonicalize map))
"##,
        expect,
    );
}

/// terminal-parameter / set-terminal-parameter.
#[test]
fn div_cx423_terminal_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (test-val nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((term (frame-terminal (selected-frame))))
  (set-terminal-parameter term 'cx423-param 'test-val)
  (list (terminal-parameter term 'cx423-param)
        (terminal-parameter term 'nonexistent)))
"##,
        expect,
    );
}

/// buffer-swap-text with narrowed regions.
#[test]
fn div_cx423_buffer_swap_text_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *cx423-swap-a*"))
      (b (get-buffer-create " *cx423-swap-b*")))
  (with-current-buffer a (insert "AAAA") (narrow-to-region 2 4))
  (with-current-buffer b (insert "BBBB"))
  (buffer-swap-text a b)
  (prog1 (list (with-current-buffer a (buffer-string))
               (with-current-buffer b (buffer-string)))
    (kill-buffer a)
    (kill-buffer b)))
"##,
        expect,
    );
}

/// window-state-put with buffer parameter.
#[test]
fn div_cx423_window_state_put_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "state test")
  (let ((state (window-state-get (selected-window))))
    (list (window-state-put state (selected-window) 'safe)
          (buffer-string))))
"##,
        expect,
    );
}

/// font-driver-available-p: checking font driver support.
#[test]
fn div_cx423_font_driver_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (font-driver-available-p) (error (car e)))
      (font-family-list))
"##,
        expect,
    );
}

/// encode-coding-region / decode-coding-region in buffer.
#[test]
fn div_cx423_encode_decode_coding_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 \"h\\303\\251llo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "héllo")
  (encode-coding-region (point-min) (point-max) 'utf-8)
  (list (buffer-size)
        (buffer-string)))
"##,
        expect,
    );
}

/// read with different readtables (standard vs custom).
#[test]
fn div_cx423_read_with_readtable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK '(a b c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "'(a b c)")
  (goto-char 1)
  (read (current-buffer)))
"##,
        expect,
    );
}

/// print-gensym: printing uninterned symbols with #: notation.
#[test]
fn div_cx423_print_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#:test-sym\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-gensym t)
      (s (make-symbol "test-sym")))
  (prin1-to-string s))
"##,
        expect,
    );
}

/// eval-after-load: deferred evaluation after feature load.
#[test]
fn div_cx423_eval_after_load() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable emacs)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((flag nil))
  (with-eval-after-load 'nonexistent-cx423
    (setq flag 'loaded))
  (list flag
        (eval-after-load 'emacs (setq flag 'emacs))
        flag))
"##,
        expect,
    );
}

/// process-send-string: sending data and checking process-status.
#[test]
fn div_cx423_process_send_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx423-pss"
                          :command '("sh" "-c" "read line; echo ok")
                          :connection-type 'pipe :buffer nil)))
  (set-process-query-on-exit-flag proc nil)
  (process-send-string proc "data\n")
  (let ((i 0))
    (while (and (memq (process-status proc) '(run open listen connect stop))
                (< i 40))
      (accept-process-output proc 0.05)
      (setq i (1+ i))))
  (let ((status (process-status proc)))
    (delete-process proc)
    (eq status 'exit)))
"##,
        expect,
    );
}

/// marker positioning through undo operations.
#[test]
fn div_cx423_marker_undo_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcde")
  (let ((m (set-marker (make-marker) 4)))
    (delete-region 2 4)
    (undo)
    (marker-position m)))
"##,
        expect,
    );
}

/// overlay evaporate on deletion: overlay auto-removal.
#[test]
fn div_cx423_overlay_evaporate_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'evaporate t)
    (delete-region 1 5)
    (list (overlay-live-p ov)
          (length (overlays-in 1 10)))))
"##,
        expect,
    );
}

/// make-frame on tty with different parameters.
#[test]
fn div_cx423_make_frame_tty_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frames-before (length (frame-list))))
  (condition-case e
      (delete-frame (make-frame '((name . "cx423-frame"))))
    (error (car e)))
  (length (frame-list)))
"##,
        expect,
    );
}

/// insert-buffer-substring with properties.
#[test]
fn div_cx423_insert_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((src (get-buffer-create " *cx423-ibs*")))
  (with-current-buffer src
    (insert "src text")
    (put-text-property 1 4 'face 'bold))
  (with-temp-buffer
    (insert-buffer-substring src 1 4)
    (list (buffer-string)
          (get-text-property 1 'face)))
  (kill-buffer src))
"##,
        expect,
    );
}

/// function called with many arguments (apply with large arg list).
#[test]
fn div_cx423_apply_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1275""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((args (number-sequence 1 50)))
  (apply #'+ 0 args))
"##,
        expect,
    );
}

/// narrowing + widen + point restoration.
#[test]
fn div_cx423_narrow_widen_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (goto-char 8)
  (narrow-to-region 3 7)
  (point))
"##,
        expect,
    );
}
