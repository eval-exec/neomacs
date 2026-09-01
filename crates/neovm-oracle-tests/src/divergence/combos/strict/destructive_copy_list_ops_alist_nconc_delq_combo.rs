//! Strict combo oracle probes, batch 151: destructive and copy list/alist ops.
//! nconc aliasing + tail-sharing, delq/delete/remq/remq-return semantics,
//! nbutlast/nreverse/ndestructive, copy-alist vs copy-tree vs copy-sequence
//! depth (mutation isolation), and member/memq/memql/assoc/assq/rassq with
//! mixed-type keys.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_nconc_aliasing_tail_share_delq_delete_remq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((l1 (list 1 2 3))
       (l2 (list 4 5 6))
       (nc (nconc l1 l2)))
  (list nc l1 l2
        (eq (last l1) (last l2))
        (length nc)
        (delq 2 (list 1 2 3 2 4))
        (delete 2 (list 1 2 3 2 4))
        (remq 'b '(a b c b d))
        (delq nil (list nil 1 nil 2 nil))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6) (1 2 3 4 5 6) (4 5 6) t 6 (1 3 4) (1 3 4) (a c d) (1 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_nreverse_nbutlast_nconc_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((l (list 1 2 3 4 5))
       (nr (nreverse l)))
  (list nr
        l
        (nbutlast (list 'a 'b 'c 'd 'e))
        (nbutlast (list 'a 'b 'c 'd 'e) 2)
        (nbutlast (list 'a))
        (nconc (list 1 2) (list 3 4) (list 5))
        (nreverse (list))
        ;; nconc with a dotted first arg already sharing
        (let ((x (list 1 2 3)))
          (setcdr (nthcdr 2 x) '(tail))
          (nconc x '(end)))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((5 4 3 2 1) (1) (a b c d) (a b c) nil (1 2 3 4 5) nil (1 2 3 tail end))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_copy_alist_tree_sequence_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((al (list (cons 'a 1) (cons 'b (list 2 3)) (cons 'c "str")))
       (ca (copy-alist al))
       (ct (copy-tree al))
       (cs (copy-sequence al)))
  (setcdr (car al) 99)
  (setcar (cadr al) 'B)
  (list al
        ca
        ct
        cs
        (eq (car al) (car ca))
        (eq (car al) (car ct))
        (eq (car al) (car cs))
        (eq al cs)))
"##;
    let expect = expect_test::expect![[
        r#""OK (((a . 99) (B 2 3) (c . \"str\")) ((a . 1) (b 2 3) (c . \"str\")) ((a . 1) (b 2 3) (c . \"str\")) ((a . 99) (B 2 3) (c . \"str\")) nil nil t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_member_memq_memql_assoc_assq_rassq_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((l '(1 2 3 4 3 2 1))
      (al '((a . 1) (b . 2) (3 . three) ("k" . v))))
  (list (member 3 l)
        (memq 3 l)
        (memql 3 l)
        (member 'missing l)
        (assoc 'a al)
        (assq 'b al)
        (rassq 2 al)
        (assoc 3 al)
        (assoc "k" al)
        (assoc-string "k" al)
        (assoc-string "K" al nil)
        (assoc-string 'b al)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((3 4 3 2 1) (3 4 3 2 1) (3 4 3 2 1) nil (a . 1) (b . 2) (b . 2) (3 . three) (\"k\" . v) (\"k\" . v) nil (b . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
