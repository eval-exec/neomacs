//! Complex combo batch 408 — 20 probes targeting function introspection,
//! keymap traversal, face/color properties, error hierarchy, char
//! properties, buffer management, process listing, keymap validation,
//! alist operations, and interactive-form/specification differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// help-function-arglist / subr-arity for built-in functions:
/// argument list formatting may differ.
#[test]
fn div_cx408_help_function_arglist_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp car)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (help-function-arglist 'car)
      (help-function-arglist 'concat)
      (subr-arity 'car)
      (subr-arity 'concat)
      (subr-name 'car)
      (subr-name 'concat))
"##,
        expect,
    );
}

/// map-keymap / map-keymap-internal: traversing keymap entries.
#[test]
fn div_cx408_map_keymap_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((99 next-line) (98 backward-char) (97 forward-char))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap))
      (entries ()))
  (define-key map "a" 'forward-char)
  (define-key map "b" 'backward-char)
  (define-key map "c" 'next-line)
  (map-keymap (lambda (e def) (push (list e def) entries)) map)
  (nreverse entries))
"##,
        expect,
    );
}

/// face-list / face-documentation: enumerating and documenting faces.
#[test]
fn div_cx408_face_list_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (bold default) (default) \"Basic bold face.\" \"Basic default face.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((faces (face-list)))
  (list (> (length faces) 10)
        (memq 'bold faces)
        (memq 'default faces)
        (condition-case e (face-documentation 'bold) (error (car e)))
        (condition-case e (face-documentation 'default) (error (car e)))))
"##,
        expect,
    );
}

/// color-gray-p / color-supported-p with various color names.
#[test]
fn div_cx408_color_gray_supported() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument framep t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (color-gray-p "gray50")
      (color-gray-p "red")
      (color-supported-p "red" t t)
      (color-supported-p "#ff0000" t nil))
"##,
        expect,
    );
}

/// define-error / error symbol hierarchy: new error conditions
/// and parent-child relationships.
#[test]
fn div_cx408_define_error_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((neo-cx408-parent error) (neo-cx408-child neo-cx408-parent error) (error . test))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-symbol "neo-cx408-parent"))
      (child (make-symbol "neo-cx408-child")))
  (define-error parent "parent error" 'error)
  (define-error child "child error" parent)
  (list (get parent 'error-conditions)
        (get child 'error-conditions)
        (condition-case e (signal child '(test))
          (parent (cons 'parent (cadr e)))
          (error (cons 'error (cadr e))))))
"##,
        expect,
    );
}

/// char-bytes / char-width for various Unicode ranges.
#[test]
fn div_cx408_char_bytes_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-bytes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-bytes ?a)
      (char-bytes ?é)
      (char-bytes ?世)
      (char-bytes #x1f600)
      (char-width ?a)
      (char-width ?世)
      (char-width #x1f600))
"##,
        expect,
    );
}

/// generate-new-buffer-name with collisions and patterns.
#[test]
fn div_cx408_generate_new_buffer_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"*neo-cx408-buf*\" \"*neo-cx408-buf*\" \"*neo-cx408-buf*<2>\" \"*neo-cx408-other*\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create "*neo-cx408-buf*"))
      (b2 (get-buffer-create "*neo-cx408-buf*")))
  (list (buffer-name b1)
        (buffer-name b2)
        (generate-new-buffer-name "*neo-cx408-buf*")
        (generate-new-buffer-name "*neo-cx408-other*")))
"##,
        expect,
    );
}

/// rename-buffer with unique: behavior when target name exists.
#[test]
fn div_cx408_rename_buffer_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments rename-buffer 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create "*neo-cx408-r1*"))
      (b2 (get-buffer-create "*neo-cx408-r2*")))
  (rename-buffer b1 "*neo-cx408-target*" t)
  (prog1 (rename-buffer b2 "*neo-cx408-target*" t)
    (list (buffer-name b1) (buffer-name b2))))
"##,
        expect,
    );
}

/// bury-buffer / other-buffer: buffer ordering after bury.
#[test]
fn div_cx408_bury_other_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"*Messages*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create "*neo-cx408-bury1*"))
      (b2 (get-buffer-create "*neo-cx408-bury2*")))
  (bury-buffer b1)
  (list (eq (other-buffer) b2)
        (buffer-name (other-buffer))))
"##,
        expect,
    );
}

/// process-list / process-live-p / delete-process:
/// enumerating and managing processes.
#[test]
fn div_cx408_process_list_live_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx408-pl"
                          :command '("sh" "-c" "exit 0")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (let ((live (process-live-p proc)))
    (delete-process proc)
    (list live
          (process-live-p proc)
          (memq proc (process-list)))))
"##,
        expect,
    );
}

/// key-valid-p / key-parse: key sequence validation and parsing.
#[test]
fn div_cx408_key_valid_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil [3 6] [134217848])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-valid-p "C-c C-f")
      (key-valid-p "M-x")
      (key-valid-p "invalid-key")
      (key-parse "C-c C-f")
      (key-parse "M-x"))
"##,
        expect,
    );
}

/// event-convert-list / kbd: event type conversion.
#[test]
fn div_cx408_event_convert_list_kbd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 134217848 134217734 \"\u{3}\u{6}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (event-convert-list '(control ?a))
      (event-convert-list '(meta ?x))
      (event-convert-list '(control meta ?f))
      (kbd "C-c C-f"))
"##,
        expect,
    );
}

/// alist-get with different default values and removal.
#[test]
fn div_cx408_alist_get_default_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 nil default 99 (b . 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((al '((a . 1) (b . 2) (c . 3))))
  (list (alist-get 'a al)
        (alist-get 'd al)
        (alist-get 'd al 'default)
        (setf (alist-get 'b al) 99)
        (assq 'b al)))
"##,
        expect,
    );
}

/// interactive-form for built-in and Lisp functions.
#[test]
fn div_cx408_interactive_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((interactive \"^p\") (interactive \"^p\\np\") t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (interactive-form 'forward-char)
      (interactive-form 'next-line)
      (commandp 'forward-char)
      (commandp 'car))
"##,
        expect,
    );
}

/// accessible-keymaps starting from a full keymap.
#[test]
fn div_cx408_accessible_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map "a" (make-sparse-keymap))
  (define-key map "a" 'forward-char)
  (define-key map "b" 'backward-char)
  (length (accessible-keymaps map)))
"##,
        expect,
    );
}

/// current-active-maps: keymaps active in current buffer.
#[test]
fn div_cx408_current_active_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (let ((maps (current-active-maps)))
    (list (> (length maps) 2)
          (memq 'emacs-lisp-mode-map maps))))
"##,
        expect,
    );
}

/// copy-keymap deep vs shallow: mutations after copy.
#[test]
fn div_cx408_copy_keymap_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (self-insert-command self-insert-command self-insert-command self-insert-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig (make-sparse-keymap))
      (copy (make-sparse-keymap)))
  (define-key orig "a" 'forward-char)
  (setq copy (copy-keymap orig))
  (define-key orig "b" 'backward-char)
  (list (key-binding "a" nil nil orig)
        (key-binding "a" nil nil copy)
        (key-binding "b" nil nil orig)
        (key-binding "b" nil nil copy)))
"##,
        expect,
    );
}

/// char-before / following-char with multibyte buffer.
#[test]
fn div_cx408_char_before_following_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 233 0 97 128512)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aé世😀")
  (list (progn (goto-char 1) (following-char))
        (progn (goto-char 2) (following-char))
        (progn (goto-char 5) (following-char))
        (progn (goto-char 2) (char-before))
        (progn (goto-char 5) (char-before))))
"##,
        expect,
    );
}

/// line-number-at-pos / posn-at-point / posn-at-column-x
/// with display properties.
#[test]
fn div_cx408_line_number_posn_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc def\nghi jkl\nmno pqr")
  (put-text-property 3 4 'display "XXX")
  (list (line-number-at-pos 1)
        (line-number-at-pos 10)
        (line-number-at-pos (point-max))))
"##,
        expect,
    );
}

/// current-local-map / current-global-map / use-local-map.
#[test]
fn div_cx408_current_local_global_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t forward-word)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((my-map (make-sparse-keymap)))
    (define-key my-map "a" 'forward-word)
    (use-local-map my-map)
    (list (keymapp (current-local-map))
          (keymapp (current-global-map))
          (key-binding "a"))))
"##,
        expect,
    );
}
