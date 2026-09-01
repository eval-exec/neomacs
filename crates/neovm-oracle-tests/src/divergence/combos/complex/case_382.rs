//! Complex combo batch 382 — `keymap`/`event`/`kbd` ultimate: define-key
//! with all key types, lookup-key, where-is-internal, command-remap,
//! event-modifiers, key-description, listify-key-sequence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx382_define_key_all_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (neo-cx382-a neo-cx382-b neo-cx382-find neo-cx382-f5 neo-cx382-mdown nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-a") 'neo-cx382-a)
  (define-key map (kbd "C-c C-b") 'neo-cx382-b)
  (define-key map [?\C-c ?\C-c] 'neo-cx382-c)
  (define-key map "\C-x\C-f" 'neo-cx382-find)
  (define-key map [f5] 'neo-cx382-f5)
  (define-key map [M-down] 'neo-cx382-mdown)
  (list (lookup-key map (kbd "C-c C-a"))
        (lookup-key map (kbd "C-c C-b"))
        (lookup-key map "\C-x\C-f")
        (lookup-key map [f5])
        (lookup-key map [M-down])
        (lookup-key map (kbd "C-c C-x"))))
"##,
        expect,
    )
}

#[test]
fn div_cx382_where_is_internal_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (([3 1]) ([3 2]) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-a") 'neo-cx382-a)
  (define-key map (kbd "C-c C-b") 'neo-cx382-b)
  (list (where-is-internal 'neo-cx382-a map)
        (where-is-internal 'neo-cx382-b map)
        (where-is-internal 'neo-cx382-missing map)))
"##,
        expect,
    )
}

#[test]
fn div_cx382_command_remap_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil neo-cx382-new nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map [remap neo-cx382-old] 'neo-cx382-new)
  (list (command-remapping 'neo-cx382-old map)
        (lookup-key map [remap neo-cx382-old])
        (command-remapping 'neo-cx382-other map)))
"##,
        expect,
    )
}

#[test]
fn div_cx382_prefix_command_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t inner-a inner-b inner-c nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((outer (make-sparse-keymap))
      (inner (make-sparse-keymap)))
  (define-key inner "a" 'inner-a)
  (define-key inner "b" 'inner-b)
  (define-key inner "c" 'inner-c)
  (define-key outer "\C-c" inner)
  (list (keymapp outer) (keymapp inner)
        (lookup-key outer "\C-ca")
        (lookup-key outer "\C-cb")
        (lookup-key outer "\C-cc")
        (lookup-key outer "\C-cx")))
"##,
        expect,
    )
}

#[test]
fn div_cx382_event_modifiers_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 nil) (C-a (control)) (M-a (meta)) (C-M-a (meta control)) (S-a (shift)) (return nil) (C-return (control)) (M-return (meta)) (mouse-1 (click)) (C-down-mouse-1 (control down)) (M-mouse-3 (meta click)) ((control meta shift 97) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (e) (list e (event-modifiers e)))
        '(?a C-a M-a C-M-a S-a
          return C-return M-return
          mouse-1 C-down-mouse-1 M-mouse-3
          (control meta shift ?a)))
"##,
        expect,
    )
}

#[test]
fn div_cx382_key_description_and_single_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"C-x C-f\" \"C-x C-f\" \"a\" \"<return>\" \"C-x\" \"C-M-a\" \"C-<return>\" \"M-<mouse-1>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-description (kbd "C-x C-f"))
      (key-description "\C-x\C-f")
      (single-key-description ?a)
      (single-key-description 'return)
      (single-key-description '(control ?x))
      (single-key-description '(control meta ?a))
      (single-key-description 'C-return)
      (single-key-description 'M-mouse-1))
"##,
        expect,
    )
}

#[test]
fn div_cx382_accessible_keymaps_and_map_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t ((97 . cmd-a) (98 . cmd-b) (99 keymap (120 . cmd-x))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap))
      (sub (make-sparse-keymap)))
  (define-key map "a" 'cmd-a)
  (define-key map "b" 'cmd-b)
  (define-key map "c" sub)
  (define-key sub "x" 'cmd-x)
  (let ((accessible (accessible-keymaps map))
        (collected nil))
    (map-keymap (lambda (key def) (push (cons key def) collected)) map)
    (list (consp accessible) (>= (length accessible) 1)
          (sort collected (lambda (a b)
                            (string< (prin1-to-string (car a))
                                     (prin1-to-string (car b))))))))
"##,
        expect,
    )
}

#[test]
fn div_cx382_listify_key_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((24 6) (134217848) (f5) (M-down))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (listify-key-sequence (kbd "C-x C-f"))
      (listify-key-sequence (kbd "M-x"))
      (listify-key-sequence [f5])
      (listify-key-sequence [M-down]))
"##,
        expect,
    )
}

#[test]
fn div_cx382_define_prefix_command_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil neo-cx382-prefix neo-cx382-action-a)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-prefix-command 'neo-cx382-prefix)
      (global-set-key "\C-cn" 'neo-cx382-prefix)
      (define-key 'neo-cx382-prefix "a" 'neo-cx382-action-a)
      (define-key 'neo-cx382-prefix "b" 'neo-cx382-action-b)
      (list (commandp 'neo-cx382-prefix)
            (lookup-key (current-global-map) "\C-cn")
            (lookup-key 'neo-cx382-prefix "a")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx382_keymap_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-a") 'neo-cx382-a)
  (define-key map (kbd "C-c C-b") 'neo-cx382-b)
  (define-key map [f5] 'neo-cx382-f5)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Keymap ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (lookup-key map (kbd "C-c C-a"))
                         (lookup-key map [f5])
                         (where-is-internal 'neo-cx382-a map)
                         (keymapp map)
                         (accessible-keymaps map)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
