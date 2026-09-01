//! Strict combo oracle probes, batch 346: ewoc (embedded widget of
//! collections). ewoc-create with pretty-printer, ewoc-enter-last/-first,
//! ewoc-count, ewoc-map, ewoc-locate, ewoc-buffer.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_ewoc_create_enter_count_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ewoc)
(with-current-buffer (get-buffer-create " *probe-ewoc*")
  (erase-buffer)
  (let ((ew (ewoc-create (lambda (node) (insert (format "node: %S\n" node)))
                          nil nil t)))
    (ewoc-enter-last ew 'alpha)
    (ewoc-enter-last ew 'beta)
    (ewoc-enter-last ew 'gamma)
    (list (ewoc-p ew)
          (ewoc-count ew)
          (buffer-string)
          (eq (ewoc-buffer ew) (current-buffer)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function ewoc-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ewoc_map_locate_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ewoc)
(with-current-buffer (get-buffer-create " *probe-ewoc2*")
  (erase-buffer)
  (let ((ew (ewoc-create (lambda (node) (insert (format "[%S]\n" node)))
                          nil nil t))
        (collected nil))
    (ewoc-enter-last ew 1)
    (ewoc-enter-last ew 2)
    (ewoc-enter-last ew 3)
    (ewoc-enter-first ew 0)
    (ewoc-map (lambda (node) (push (* node 10) collected)) ew)
    (let ((cnt (ewoc-count ew)))
      (list cnt
            (nreverse collected)
            (buffer-string)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function ewoc-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ewoc_delete_refresh_invalidate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ewoc)
(with-current-buffer (get-buffer-create " *probe-ewoc3*")
  (erase-buffer)
  (let* ((ew (ewoc-create (lambda (node) (insert (format "<%S>\n" node)))
                           nil nil t))
         (n1 (ewoc-enter-last ew 'first))
         (n2 (ewoc-enter-last ew 'second))
         (n3 (ewoc-enter-last ew 'third)))
    (ewoc-delete ew n2)
    (let ((c1 (ewoc-count ew))
          (buf1 (buffer-string)))
      (ewoc-refresh ew)
      (list c1 buf1 (buffer-string) (ewoc-count ew)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function ewoc-count)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
