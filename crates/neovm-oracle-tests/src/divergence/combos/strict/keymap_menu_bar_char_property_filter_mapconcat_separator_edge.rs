//! Strict combo oracle probes, batch 132: keymap inheritance with remap
//! chains, char-property-filter, mapconcat edge cases, and eieio method
//! dispatch on parent classes.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u6_keymap_inheritance_remap_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((g (make-sparse-keymap))
      (l (make-sparse-keymap)))
  (define-key g "a" 'g-cmd)
  (define-key g [remap original] 'remapped)
  (define-key l "b" 'l-cmd)
  (define-key l [remap remapped] 'double-remapped)
  (set-keymap-parent l g)
  (list (lookup-key l "a")
        (lookup-key l "b")
        (command-remapping 'original nil (list l))
        (command-remapping 'remapped nil (list l))
        (eq (lookup-key l [remap original]) 'remapped)
        (eq (lookup-key l [remap remapped]) 'double-remapped)))
"##;
    let expect = expect_test::expect![[r#""OK (g-cmd l-cmd remapped double-remapped t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u6_char_property_filter_and_mapconcat_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "aaaabbbbccccdddd")
  (put-text-property 1 5 '(face a))
  (put-text-property 5 9 '(face b))
  (put-text-property 9 13 '(face c))
  (put-text-property 13 17 '(face d))
  (list (mapconcat #'identity '() "")
        (mapconcat #'identity '("x") "-")
        (mapconcat #'identity '("a" "b" "c") nil)
        (mapconcat (lambda (s) (concat "[" s "]")) '("a" "b") "-")
        (mapconcat #'identity nil "-")
        (mapcar (lambda (pos) (get-char-property pos 'face))
                (list 1 5 9 13))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments put-text-property 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u6_eieio_method_dispatch_parent_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (cl-defgeneric probe-ep (obj))
  (cl-defclass probe-ep-base () ())
  (cl-defclass probe-ep-child (probe-ep-base) ())
  (cl-defclass probe-ep-grand (probe-ep-child) ())
  (cl-defmethod probe-ep ((obj probe-ep-base))
    'base)
  (cl-defmethod probe-ep ((obj probe-ep-child))
    'child)
  (let ((b (probe-ep-base))
        (c (probe-ep-child))
        (g (probe-ep-grand)))
    (list (probe-ep b)
          (probe-ep c)
          (probe-ep g)
          (eieio-object-class b)
          (eieio-object-class c)
          (eieio-object-class g)
          (child-of-class-p 'probe-ep-child 'probe-ep-base)
          (child-of-class-p 'probe-ep-grand 'probe-ep-base))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u6_buffer_local_vars_dump_and_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-bvd*")))
  (unwind-protect
      (with-current-buffer b
        (setq-local fill-column 50)
        (setq-local tab-width 3)
        (setq-local case-fold-search nil)
        (let ((locals (buffer-local-variables)))
          (list (assq 'fill-column locals)
                (assq 'tab-width locals)
                (assq 'case-fold-search locals)
                (> (length locals) 3)
                (progn (kill-all-local-variables)
                       (list (assq 'fill-column (buffer-local-variables))
                             (default-value 'fill-column)
                             (default-value 'tab-width))))))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((fill-column . 50) (tab-width . 3) (case-fold-search) t (nil 70 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u6_print_escape_newlines_and_control_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s (string 9 10 13 7 8 27 12)))
  (list (let ((print-escape-newlines t)) (prin1-to-string s))
        (let ((print-escape-control-characters t)) (prin1-to-string s))
        (let ((print-escape-newlines t)
              (print-escape-control-characters t)) (prin1-to-string s))
        (let ((print-escape-newlines nil)
              (print-escape-control-characters nil)) (prin1-to-string s))
        (let ((print-escape-text t)) (prin1-to-string s))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"\t\\\\n\\r\u{7}\u{8}\u{1b}\\\\f\\\"\" \"\\\"\\\\11\\\\12\\\\15\\\\7\\\\10\\\\33\\\\14\\\"\" \"\\\"\\\\11\\\\n\\\\15\\\\7\\\\10\\\\33\\\\f\\\"\" \"\\\"\t\\n\\r\u{7}\u{8}\u{1b}\\f\\\"\" \"\\\"\t\\n\\r\u{7}\u{8}\u{1b}\\f\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
