//! Oracle parity tests for GNU `subr.el' `define-key-after'.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_define_key_after_orders_and_deduplicates_direct_bindings() {
    let form = r#"
(let ((m (make-sparse-keymap)))
  (define-key-after m [?a] 'cmd-a t)
  (define-key-after m [?b] 'cmd-b ?a)
  (define-key-after m [?c] 'cmd-c ?a)
  (define-key-after m [?b] 'cmd-b2 ?c)
  (let ((keys nil)
        (tail (cdr m)))
    (while (and (consp tail) (not (keymapp tail)))
      (when (consp (car tail))
        (push (caar tail) keys))
      (setq tail (cdr tail)))
    (list (nreverse keys)
          (lookup-key m [?a])
          (lookup-key m [?b])
          (lookup-key m [?c]))))"#;
    let expect = expect_test::expect![[r#""OK ((97 99 98) cmd-a cmd-b2 cmd-c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_define_key_after_inserts_before_inherited_parent() {
    let form = r#"
(let ((m (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key m [?a] 'cmd-a)
  (define-key parent [?p] 'parent-p)
  (set-keymap-parent m parent)
  (define-key-after m [?z] 'cmd-z t)
  (let ((own-keys nil)
        (tail (cdr m)))
    (while (and (consp tail) (not (keymapp tail)))
      (when (consp (car tail))
        (push (caar tail) own-keys))
      (setq tail (cdr tail)))
    (list (nreverse own-keys)
          (lookup-key m [?z])
          (lookup-key m [?p])
          (lookup-key parent [?z])
          (eq (keymap-parent m) parent))))"#;
    let expect = expect_test::expect![[r#""OK ((97 122) cmd-z parent-p nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_define_key_after_descends_into_prefix_keymap_for_multi_key() {
    let form = r#"
(let ((m (make-sparse-keymap)))
  (define-key m [?x ?a] 'cmd-x-a)
  (define-key-after m [?x ?b] 'cmd-x-b ?a)
  (define-key-after m [?x ?c] 'cmd-x-c ?a)
  (let* ((prefix (lookup-key m [?x]))
         (keys nil)
         (tail (cdr prefix)))
    (while (and (consp tail) (not (keymapp tail)))
      (when (consp (car tail))
        (push (caar tail) keys))
      (setq tail (cdr tail)))
    (list (keymapp prefix)
          (nreverse keys)
          (lookup-key m [?x ?a])
          (lookup-key m [?x ?b])
          (lookup-key m [?x ?c]))))"#;
    let expect = expect_test::expect![[r#""OK (t (97 99 98) cmd-x-a cmd-x-b cmd-x-c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
