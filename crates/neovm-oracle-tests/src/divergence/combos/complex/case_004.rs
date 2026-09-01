//! Complex combo divergence probes batch 4 — exotic interaction edges.
//!
//! process-environment passing, default-process-coding-system + subprocess,
//! bool-vector bitwise, keymap parent chains, obarray lifecycle, syntax-table
//! switching, print-gensym, cl-rotatef on hash-table places, char-table parent
//! chains, map-char-table iteration, buffer-undo-list structure, overlay
//! before-string under narrowing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx4_process_environment_subprocess() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello123\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "NEO_CX4_ENV=hello123" process-environment)))
  (car (split-string (shell-command-to-string "echo $NEO_CX4_ENV"))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_process_coding_subprocess_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\\n\" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
  (with-temp-buffer
    (call-process "echo" nil t nil "café世界")
    (list (buffer-string) (string-bytes (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_bool_vector_bitwise() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 8 1 void-function t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bv1 (make-bool-vector 8 t)) (bv2 (make-bool-vector 8 nil)))
  (aset bv2 3 t)
  (list (bool-vector-p bv1)
        (bool-vector-count-population bv1)
        (bool-vector-count-population bv2)
        (condition-case e (bool-vector-count-population (bool-vector-and bv1 bv2)) (error (car e)))
        (aref bv1 0) (aref bv2 0) (aref bv2 3)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_keymap_parent_chain_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (grand mid t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((g (make-sparse-keymap)) (m (make-sparse-keymap)) (l (make-sparse-keymap)))
  (define-key g "a" 'grand)
  (define-key m "b" 'mid)
  (set-keymap-parent m g)
  (set-keymap-parent l m)
  (list (lookup-key l "a") (lookup-key l "b")
        (eq (keymap-parent l) m)
        (eq (keymap-parent (keymap-parent l)) g)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_obarray_intern_unintern_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (obarray)))
  (intern "neo-cx4-obs" ob)
  (list (eq (intern-soft "neo-cx4-obs" ob) (intern "neo-cx4-obs" ob))
        (progn (unintern "neo-cx4-obs" ob) (if (intern-soft "neo-cx4-obs" ob) t nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_syntax_table_switch_forward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?_ "w" st)
  (with-temp-buffer
    (with-syntax-table st
      (insert "foo_bar baz")
      (goto-char 1)
      (forward-word 1)
      (point))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_print_gensym_notation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-gensym t))
  (let ((gs (gensym)))
    (list (string-match "#:" (prin1-to-string gs))
          (eq gs (car (read-from-string (prin1-to-string gs)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_cl_rotatef_hash_table_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table)))
  (puthash 'a 1 ht)
  (puthash 'b 2 ht)
  (cl-rotatef (gethash 'a ht) (gethash 'b ht))
  (list (gethash 'a ht) (gethash 'b ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_char_table_parent_chain_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:parent-val #^[nil nil cx4-test #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil :parent-val nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil :parent-val nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-char-table 'cx4-test nil)) (c (make-char-table 'cx4-test nil)))
  (aset p ?a :parent-val)
  (set-char-table-parent c p)
  (list (aref c ?a) (char-table-parent c) (eq (char-table-parent c) p)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_map_char_table_range_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx4-map nil)) (count 0))
  (set-char-table-range ct '(?a . ?z) t)
  (map-char-table (lambda (k v) (when v (setq count (1+ count)))) ct)
  count)
"##,
        expect,
    );
}

#[test]
fn div_cx4_buffer_undo_list_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello")
  (let ((n1 (length buffer-undo-list)))
    (undo-boundary)
    (insert " world")
    (let ((n2 (length buffer-undo-list)))
      (undo)
      (list (> n2 n1) (buffer-string) (> (length buffer-undo-list) n2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_overlay_before_string_under_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 4 1 \"012\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'before-string ">>")
    (overlay-put ov 'face 'bold))
  (narrow-to-region 1 4)
  (list (point-min) (point-max)
        (length (overlays-in (point-min) (point-max)))
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_print_gensym_circular_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(#1=#:g0 #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-gensym t) (print-circle t))
  (let ((gs (gensym)))
    (prin1-to-string (list gs gs))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_set_char_table_extra_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments make-char-table 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx4-extra nil 3)))
  (set-char-table-extra-slot ct 0 :slot0)
  (set-char-table-extra-slot ct 2 :slot2)
  (list (char-table-extra-slot ct 0)
        (char-table-extra-slot ct 1)
        (char-table-extra-slot ct 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_process_plist_put_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:cx4-prop :value) :value)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx4-pl" :command '("true"))))
  (process-put p :cx4-prop :value)
  (accept-process-output p 1)
  (prog1 (list (process-plist p)
               (process-get p :cx4-prop))
    (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_with_current_buffer_nested_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aaa\" \"bbb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (generate-new-buffer " *cx4-b1*"))
      (b2 (generate-new-buffer " *cx4-b2*")))
  (with-current-buffer b1 (insert "aaa"))
  (with-current-buffer b2 (insert "bbb"))
  (prog1 (list (with-current-buffer b1
                 (with-current-buffer b2 (buffer-string))
                 (buffer-string))
               (with-current-buffer b2 (buffer-string)))
    (kill-buffer b1) (kill-buffer b2)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_prefer_coding_system_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable prefer-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig prefer-coding-system))
  (prefer-coding-system 'utf-8)
  (prog1 (list (coding-system-p (car (last (coding-system-priority-list))))
               (memq 'utf-8 (coding-system-priority-list)))
    (when (functionp orig) (funcall interprogram-coding-system))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_buffer_locals_set_then_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local global t neo-cx4-bl global)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx4-bl 'global)
  (with-temp-buffer
    (setq-local neo-cx4-bl :local)
    (list neo-cx4-bl (default-value 'neo-cx4-bl)
          (local-variable-p 'neo-cx4-bl)
          (kill-local-variable 'neo-cx4-bl)
          neo-cx4-bl)))
"##,
        expect,
    );
}

#[test]
fn div_cx4_marker_kill_buffer_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable last)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (generate-new-buffer " *cx4-mk*")))
  (with-current-buffer buf
    (insert "abcdef")
    (let ((m (set-marker (make-marker) 3 (current-buffer))))
      (prog1 (list (marker-position m) (marker-buffer m))
        (kill-buffer buf)
        (push (marker-position m) last)
        (push (marker-buffer m) last)))
    (list (nth 0 last) (nth 1 last))))
"##,
        expect,
    );
}

#[test]
fn div_cx4_format_multibyte_s_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"\\\"café\\\"\" \"café\" #(\"x\" 0 1 (face bold)) \"#(\\\"x\\\" 0 1 (face bold))\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%S" "café")
      (format "%s" "café")
      (format "%s" (propertize "x" 'face 'bold))
      (format "%S" (propertize "x" 'face 'bold)))
"##,
        expect,
    );
}
