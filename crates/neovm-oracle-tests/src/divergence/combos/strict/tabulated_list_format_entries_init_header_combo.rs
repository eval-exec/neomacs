//! Strict combo oracle probes, batch 222: tabulated-list mode. tabulated-list-
//! mode setup, tabulated-list-format + tabulated-list-entries, header init,
//! derived-mode-p over special-mode, and tabulated-list-print.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_tabulated_list_mode_format_entries_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'tabulated-list)
(with-current-buffer (get-buffer-create " *probe-tl*")
  (tabulated-list-mode)
  (setq tabulated-list-format [("Name" 20 t) ("Value" 15 nil)])
  (setq tabulated-list-entries
        `(("a" ["Alpha" "1"])
          ("b" ["Bravo" "2"])
          ("c" ["Charlie" "3"])))
  (tabulated-list-init-header)
  (let ((result (list (eq major-mode 'tabulated-list-mode)
                      (derived-mode-p 'special-mode)
                      (vectorp tabulated-list-format)
                      (length tabulated-list-entries)
                      (aref (aref tabulated-list-format 0) 0))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp (\"Name\" 20 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_tabulated_list_print_entries_renders() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'tabulated-list)
(with-current-buffer (get-buffer-create " *probe-tl2*")
  (tabulated-list-mode)
  (setq tabulated-list-format [("Key" 10 nil) ("Desc" 20 nil)])
  (setq tabulated-list-entries
        `(("k1" ["First" "desc-one"])
          ("k2" ["Second" "desc-two"])))
  (tabulated-list-init-header)
  (tabulated-list-print t)
  (let ((line-count (count-lines (point-min) (point-max)))
        (has-header (> (length (buffer-substring (point-min) (line-end-position))) 0)))
    (kill-buffer (current-buffer))
    (list line-count has-header)))
"##;
    let expect = expect_test::expect![[r#""OK (2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_tabulated_list_get_entry_id_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'tabulated-list)
(with-current-buffer (get-buffer-create " *probe-tl3*")
  (tabulated-list-mode)
  (setq tabulated-list-format [("Col" 10 nil)])
  (setq tabulated-list-entries
        `(("id1" ["A"])
          ("id2" ["B"])))
  (tabulated-list-init-header)
  (tabulated-list-print t)
  (goto-char (point-min))
  (forward-line 2)
  (let ((entry-id (tabulated-list-get-id))
        (entry (tabulated-list-get-entry)))
    (kill-buffer (current-buffer))
    (list entry-id (vectorp entry))))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
