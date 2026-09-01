//! Complex combo batch 140 — `tab-bar` / `tab-line` / `tool-bar` /
//! `menu-bar` state queries, persistence, and per-tab buffer lists.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx140_tab_bar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tab-bar-mode)
          (fboundp 'tab-new)
          (fboundp 'tab-next)
          (fboundp 'tab-close)
          (boundp 'tab-bar-show))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_tab_bar_tabs_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((tabs (frame-parameter nil 'tabs)))
      (list (consp tabs)
            (boundp 'tab-bar-tabs-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_tab_line_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tab-line-mode)
          (global-tab-line-mode)
          (boundp 'tab-line-tabs-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_tool_bar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tool-bar-mode)
          (boundp 'tool-bar-map)
          (boundp 'tool-bar-style)
          (boundp 'auto-resize-tool-bars))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_menu_bar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'menu-bar-mode)
          (boundp 'menu-bar-final-items)
          (boundp 'menu-bar-crudified-menu))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_menu_item_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function menu-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((item (menu-item "My Item" 'neo-cx140-cmd
                        :help "Help text" :keys "C-c C-m")))
  (list (consp item)
        (eq (car item) 'menu-item)
        (plist-get (cddr item) :help)
        (plist-get (cddr item) :keys)))
"##,
        expect,
    );
}

#[test]
fn div_cx140_define_key_menu_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx140-cmd t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map [menu-bar neo-cx140]
    '(menu-item "Neo CX140" neo-cx140-cmd :help "Test menu entry"))
  (list (lookup-key map [menu-bar neo-cx140])
        (keymapp map)))
"##,
        expect,
    );
}

#[test]
fn div_cx140_easy_menu_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'easymenu)
      (easy-menu-define neo-cx140-menu nil "Test menu"
        '("CX140"
          ["Item 1" (message "1") t]
          ["Item 2" (message "2") t]
          "---"
          ["Toggle" (message "t") :style toggle :selected t]))
      (list (keymapp neo-cx140-menu)
            (lookup-key neo-cx140-menu [item-1])))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_active_maps_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 t t t forward-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((maps (current-active-maps t)))
  (list (length maps)
        (eq (nth 0 maps) (current-local-map))
        (eq (nth 1 maps) (current-global-map))
        (null (cddr maps))
        (lookup-key (cons 'keymap maps) "\C-f")))
"##,
        expect,
    );
}

#[test]
fn div_cx140_use_global_map_lookup_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx140-global-x t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((saved-global (current-global-map))
      (new-global (make-keymap)))
  (define-key new-global "x" 'neo-cx140-global-x)
  (use-global-map new-global)
  (let ((found (lookup-key (current-global-map) "x")))
    (use-global-map saved-global)
    (list found
          (eq (current-global-map) saved-global))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_minor_mode_menu_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((neo-cx140-minor keymap (menu-bar keymap (neo-cx140-minor menu-item \"CX140 Minor\" neo-cx140-cmd))) neo-cx140-cmd)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((map (make-sparse-keymap)))
      (define-key map [menu-bar neo-cx140-minor]
        '(menu-item "CX140 Minor" neo-cx140-cmd))
      (let ((minor-mode-map-alist (list (cons 'neo-cx140-minor map))))
        (list (assq 'neo-cx140-minor minor-mode-map-alist)
              (lookup-key map [menu-bar neo-cx140-minor]))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx140_tab_bar_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Tab-bar mega test buffer content")
      (put-text-property 1 7 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let ((state (list (fboundp 'tab-bar-mode)
                           (boundp 'tab-bar-show)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
