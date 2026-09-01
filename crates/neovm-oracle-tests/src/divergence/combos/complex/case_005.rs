//! Complex combo divergence probes batch 5 — adjacent to known bugs.
//!
//! coding-system-for-read let-binding + subprocess (parallel to process-env
//! propagation bug), buffer-read-only interactions, modification-hook
//! inhibition, print truncation + circular, window-config + overlay/marker
//! restore, circular vector, closure-over-loop, timer + process wait,
//! remap key binding, custom error hierarchy, print-length + print-level.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx5_coding_system_for_read_subprocess() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coding-system-for-read 'utf-8-unix))
  (with-temp-buffer
    (call-process "printf" nil t nil "caf\\303\\251")
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx5_buffer_read_only_set_modified() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (setq buffer-read-only t)
  (let ((inhibit-read-only t))
    (insert "X")
    (list (buffer-modified-p) (buffer-string)))
  (setq buffer-read-only nil)
  (set-buffer-modified-p nil)
  (list (buffer-modified-p) buffer-read-only))
"##,
        expect,
    );
}

#[test]
fn div_cx5_inhibit_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:fired)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (with-temp-buffer
    (add-hook 'after-change-functions
              (lambda (beg end len) (push :fired fired)) nil t)
    (let ((inhibit-modification-hooks t))
      (insert "X"))
    (insert "Y"))
  fired)
"##,
        expect,
    );
}

#[test]
fn div_cx5_print_length_plus_level_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"((1 2 3 ...) (6 7 8 ...) (11 12 13) ...)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-length 3) (print-level 2) (print-circle t))
  (prin1-to-string '((1 2 3 4 5) (6 7 8 9 10) (11 12 13) (14 15))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_window_config_overlay_marker_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 3 3 2 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefgh")
  (let ((m (set-marker (make-marker) 3))
        (ov (make-overlay 2 5)))
    (overlay-put ov 'face 'bold)
    (let ((cfg (current-window-configuration)))
      (goto-char 6)
      (let ((p1 (point)) (m1 (marker-position m)))
        (set-window-configuration cfg)
        (list p1 (point) m1 (marker-position m)
              (overlay-start ov) (overlay-end ov))))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_circular_vector_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#1=[1 2 #1#]\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3)) (print-circle t))
  (aset v 2 v)
  (prin1-to-string v))
"##,
        expect,
    );
}

#[test]
fn div_cx5_closure_over_loop_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (mapcar #'funcall
          (let (acc)
            (dotimes (i 3)
              (push (byte-compile (lambda () i)) acc))
            (nreverse acc))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_timer_fires_during_process_wait() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :timer-fired""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (run-with-timer 0 nil (lambda () (setq fired :timer-fired)))
  (let ((p (make-process :name "neo-cx5-t" :command '("true"))))
    (accept-process-output p 1))
  fired)
"##,
        expect,
    );
}

#[test]
fn div_cx5_remap_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (my-forward nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m [remap forward-char] 'my-forward)
  (list (lookup-key m [remap forward-char])
        (command-remapping 'forward-char m)
        (eq (lookup-key m (kbd "C-f")) 'my-forward)))
"##,
        expect,
    );
}

#[test]
fn div_cx5_custom_error_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-parent :caught-exact)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (define-error 'neo-cx5-error "Custom error" '(error))
  (define-error 'neo-cx5-sub-error "Sub error" '(neo-cx5-error))
  (list (condition-case e (signal 'neo-cx5-sub-error "msg") (neo-cx5-error :caught-parent) (error :missed))
        (condition-case e (signal 'neo-cx5-sub-error "msg") (neo-cx5-sub-error :caught-exact) (error :missed))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_set_multibyte_multiple_raw_bytes_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 6 4194248 4194249 4194250 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 202 65 66))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (point-max)
        (char-after 1) (char-after 2) (char-after 3) (char-after 4)))
"##,
        expect,
    );
}

#[test]
fn div_cx5_process_sentinel_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"finished\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (sentinel-fired)
  (let ((p (make-process :name "neo-cx5-sl" :command '("true")
                         :sentinel (lambda (proc event) (push event sentinel-fired)))))
    (accept-process-output p 2))
  (if sentinel-fired (car sentinel-fired) :no-sentinel))
"##,
        expect,
    );
}

#[test]
fn div_cx5_read_circle_vector_labels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (car (read-from-string "#1=[a b #1#]"))))
  (eq (aref v 2) v))
"##,
        expect,
    );
}

#[test]
fn div_cx5_sort_stability_plist_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((1 . :a) (1 . :c) (1 . :e) (2 . :b) (2 . :f) (3 . :d))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(sort (copy-sequence '((1 . :a) (2 . :b) (1 . :c) (3 . :d) (1 . :e) (2 . :f)))
      (lambda (x y) (< (car x) (car y))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_char_table_range_t_syntax_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (make-syntax-table)))
  (set-char-table-range st t (string-to-syntax "."))
  (with-temp-buffer
    (with-syntax-table st
      (insert "(a)b(c)")
      (goto-char 1)
      (condition-case e (progn (forward-sexp) (point)) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_after_change_functions_inhibit_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:change :change)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (log)
  (with-temp-buffer
    (add-hook 'after-change-functions (lambda (b e l) (push :change log)) nil t)
    (insert "a")
    (let ((inhibit-modification-hooks t))
      (insert "b")
      (insert "c"))
    (insert "d"))
  (reverse log))
"##,
        expect,
    );
}

#[test]
fn div_cx5_buffer_read_only_var_write_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (buffer-read-only #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx5-ro-")))
  (with-temp-buffer
    (setq buffer-read-only t)
    (insert "hello")
    (write-region (buffer-string) nil f nil 0))
  (prog1 (with-temp-buffer (insert-file-contents f) (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_overlay_invisible_line_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 13""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\n")
  (put-text-property 7 13 'invisible t)
  (goto-char 1)
  (forward-line 2)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_cx5_cl_typep_defstruct_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx5-base)
  (cl-defstruct (neo-cx5-sub (:include neo-cx5-base)) field)
  (let ((o (make-neo-cx5-sub :field 42)))
    (list (cl-typep o 'neo-cx5-base)
          (cl-typep o 'neo-cx5-sub)
          (cl-typep o 'neo-cx5-base))))
"##,
        expect,
    );
}

#[test]
fn div_cx5_format_spec_nested_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x y % literal\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (format-spec "%a %b %% literal" '((97 . "x") (98 . "y")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}
