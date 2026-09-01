//! Complex oracle parity tests for abstract data type implementations in Elisp.
//!
//! Tests stack with min/max tracking, deque, ordered map (BST),
//! multimap, circular buffer with iterator, and immutable persistent lists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Stack with O(1) min/max tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_min_max_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each stack frame stores (value . (current-min . current-max))
    let form = r#"(progn
  (fset 'neovm--adt-mms-push
    (lambda (stack val)
      (let ((cur-min (if stack (car (cdr (car stack))) val))
            (cur-max (if stack (cdr (cdr (car stack))) val)))
        (cons (cons val (cons (min val cur-min) (max val cur-max)))
              stack))))

  (fset 'neovm--adt-mms-pop
    (lambda (stack) (cdr stack)))

  (fset 'neovm--adt-mms-top
    (lambda (stack) (car (car stack))))

  (fset 'neovm--adt-mms-min
    (lambda (stack) (car (cdr (car stack)))))

  (fset 'neovm--adt-mms-max
    (lambda (stack) (cdr (cdr (car stack)))))

  (unwind-protect
      (let ((s nil))
        (setq s (funcall 'neovm--adt-mms-push s 5))
        (setq s (funcall 'neovm--adt-mms-push s 3))
        (setq s (funcall 'neovm--adt-mms-push s 7))
        (setq s (funcall 'neovm--adt-mms-push s 1))
        (setq s (funcall 'neovm--adt-mms-push s 9))
        (let ((r1 (list (funcall 'neovm--adt-mms-top s)
                        (funcall 'neovm--adt-mms-min s)
                        (funcall 'neovm--adt-mms-max s))))
          ;; Pop 9, min/max should update
          (setq s (funcall 'neovm--adt-mms-pop s))
          (let ((r2 (list (funcall 'neovm--adt-mms-top s)
                          (funcall 'neovm--adt-mms-min s)
                          (funcall 'neovm--adt-mms-max s))))
            ;; Pop 1
            (setq s (funcall 'neovm--adt-mms-pop s))
            (let ((r3 (list (funcall 'neovm--adt-mms-top s)
                            (funcall 'neovm--adt-mms-min s)
                            (funcall 'neovm--adt-mms-max s))))
              ;; Pop 7
              (setq s (funcall 'neovm--adt-mms-pop s))
              (let ((r4 (list (funcall 'neovm--adt-mms-top s)
                              (funcall 'neovm--adt-mms-min s)
                              (funcall 'neovm--adt-mms-max s))))
                (list r1 r2 r3 r4))))))
    (fmakunbound 'neovm--adt-mms-push)
    (fmakunbound 'neovm--adt-mms-pop)
    (fmakunbound 'neovm--adt-mms-top)
    (fmakunbound 'neovm--adt-mms-min)
    (fmakunbound 'neovm--adt-mms-max)))"#;
    let expect = expect_test::expect![[r#""OK ((9 1 9) (1 1 7) (7 3 7) (3 3 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deque (double-ended queue) with all operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_deque() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deque as (front-list . rear-list) with lazy transfer
    let form = r#"(progn
  (fset 'neovm--adt-dq-make (lambda () (cons nil nil)))

  (fset 'neovm--adt-dq-push-front
    (lambda (dq val) (cons (cons val (car dq)) (cdr dq))))

  (fset 'neovm--adt-dq-push-back
    (lambda (dq val) (cons (car dq) (cons val (cdr dq)))))

  (fset 'neovm--adt-dq-normalize
    (lambda (dq)
      "If front is empty, reverse rear into front."
      (if (null (car dq))
          (cons (nreverse (cdr dq)) nil)
        dq)))

  (fset 'neovm--adt-dq-pop-front
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--adt-dq-normalize dq)))
        (if (null (car dq2))
            (cons nil dq2)  ;; empty
          (cons (caar dq2)
                (cons (cdar dq2) (cdr dq2)))))))

  (fset 'neovm--adt-dq-normalize-back
    (lambda (dq)
      "If rear is empty, reverse front into rear."
      (if (null (cdr dq))
          (cons nil (nreverse (car dq)))
        dq)))

  (fset 'neovm--adt-dq-pop-back
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--adt-dq-normalize-back dq)))
        (if (null (cdr dq2))
            (cons nil dq2)
          (cons (cadr dq2)
                (cons (car dq2) (cddr dq2)))))))

  (fset 'neovm--adt-dq-to-list
    (lambda (dq)
      (append (car dq) (nreverse (copy-sequence (cdr dq))))))

  (fset 'neovm--adt-dq-size
    (lambda (dq) (+ (length (car dq)) (length (cdr dq)))))

  (unwind-protect
      (let ((dq (funcall 'neovm--adt-dq-make)))
        ;; Push to both ends
        (setq dq (funcall 'neovm--adt-dq-push-front dq 2))
        (setq dq (funcall 'neovm--adt-dq-push-front dq 1))
        (setq dq (funcall 'neovm--adt-dq-push-back dq 3))
        (setq dq (funcall 'neovm--adt-dq-push-back dq 4))
        (let ((contents1 (funcall 'neovm--adt-dq-to-list dq))
              (size1 (funcall 'neovm--adt-dq-size dq)))
          ;; Pop from front
          (let* ((r1 (funcall 'neovm--adt-dq-pop-front dq))
                 (val1 (car r1)))
            (setq dq (cdr r1))
            ;; Pop from back
            (let* ((r2 (funcall 'neovm--adt-dq-pop-back dq))
                   (val2 (car r2)))
              (setq dq (cdr r2))
              (let ((contents2 (funcall 'neovm--adt-dq-to-list dq))
                    (size2 (funcall 'neovm--adt-dq-size dq)))
                (list contents1 size1 val1 val2 contents2 size2))))))
    (fmakunbound 'neovm--adt-dq-make)
    (fmakunbound 'neovm--adt-dq-push-front)
    (fmakunbound 'neovm--adt-dq-push-back)
    (fmakunbound 'neovm--adt-dq-normalize)
    (fmakunbound 'neovm--adt-dq-pop-front)
    (fmakunbound 'neovm--adt-dq-normalize-back)
    (fmakunbound 'neovm--adt-dq-pop-back)
    (fmakunbound 'neovm--adt-dq-to-list)
    (fmakunbound 'neovm--adt-dq-size)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) 4 1 4 (2 3) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ordered map (BST-based with insert/lookup/delete/range-query)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_bst_ordered_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BST node: (key value left right)
    let form = r#"(progn
  (fset 'neovm--adt-bst-insert
    (lambda (tree key val)
      (if (null tree)
          (list key val nil nil)
        (let ((k (car tree)) (v (cadr tree))
              (left (caddr tree)) (right (cadddr tree)))
          (cond
            ((< key k) (list k v (funcall 'neovm--adt-bst-insert left key val) right))
            ((> key k) (list k v left (funcall 'neovm--adt-bst-insert right key val)))
            (t (list k val left right)))))))  ;; update value

  (fset 'neovm--adt-bst-lookup
    (lambda (tree key)
      (if (null tree)
          nil
        (let ((k (car tree)) (v (cadr tree))
              (left (caddr tree)) (right (cadddr tree)))
          (cond
            ((< key k) (funcall 'neovm--adt-bst-lookup left key))
            ((> key k) (funcall 'neovm--adt-bst-lookup right key))
            (t v))))))

  (fset 'neovm--adt-bst-min-node
    (lambda (tree)
      (if (null (caddr tree))
          tree
        (funcall 'neovm--adt-bst-min-node (caddr tree)))))

  (fset 'neovm--adt-bst-delete
    (lambda (tree key)
      (if (null tree)
          nil
        (let ((k (car tree)) (v (cadr tree))
              (left (caddr tree)) (right (cadddr tree)))
          (cond
            ((< key k) (list k v (funcall 'neovm--adt-bst-delete left key) right))
            ((> key k) (list k v left (funcall 'neovm--adt-bst-delete right key)))
            ;; Found: three cases
            ((null left) right)
            ((null right) left)
            (t (let ((succ (funcall 'neovm--adt-bst-min-node right)))
                 (list (car succ) (cadr succ)
                       left
                       (funcall 'neovm--adt-bst-delete right (car succ))))))))))

  (fset 'neovm--adt-bst-inorder
    (lambda (tree)
      "Return sorted list of (key . value) pairs."
      (if (null tree)
          nil
        (append (funcall 'neovm--adt-bst-inorder (caddr tree))
                (list (cons (car tree) (cadr tree)))
                (funcall 'neovm--adt-bst-inorder (cadddr tree))))))

  (fset 'neovm--adt-bst-range
    (lambda (tree lo hi)
      "Return all (key . value) pairs where lo <= key <= hi."
      (if (null tree)
          nil
        (let ((k (car tree)) (v (cadr tree))
              (left (caddr tree)) (right (cadddr tree))
              (result nil))
          (when (> k lo)
            (setq result (funcall 'neovm--adt-bst-range left lo hi)))
          (when (and (>= k lo) (<= k hi))
            (setq result (append result (list (cons k v)))))
          (when (< k hi)
            (setq result (append result (funcall 'neovm--adt-bst-range right lo hi))))
          result))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert keys: 5 3 7 1 4 6 8 2 9
        (dolist (pair '((5 . "five") (3 . "three") (7 . "seven") (1 . "one")
                        (4 . "four") (6 . "six") (8 . "eight") (2 . "two") (9 . "nine")))
          (setq tree (funcall 'neovm--adt-bst-insert tree (car pair) (cdr pair))))
        (let ((sorted (funcall 'neovm--adt-bst-inorder tree))
              (lookup5 (funcall 'neovm--adt-bst-lookup tree 5))
              (lookup10 (funcall 'neovm--adt-bst-lookup tree 10))
              (range (funcall 'neovm--adt-bst-range tree 3 7)))
          ;; Delete node 5 (has two children)
          (setq tree (funcall 'neovm--adt-bst-delete tree 5))
          (let ((after-delete (funcall 'neovm--adt-bst-inorder tree))
                (lookup5-after (funcall 'neovm--adt-bst-lookup tree 5)))
            ;; Update existing key
            (setq tree (funcall 'neovm--adt-bst-insert tree 3 "THREE"))
            (list sorted lookup5 lookup10 range
                  after-delete lookup5-after
                  (funcall 'neovm--adt-bst-lookup tree 3)))))
    (fmakunbound 'neovm--adt-bst-insert)
    (fmakunbound 'neovm--adt-bst-lookup)
    (fmakunbound 'neovm--adt-bst-min-node)
    (fmakunbound 'neovm--adt-bst-delete)
    (fmakunbound 'neovm--adt-bst-inorder)
    (fmakunbound 'neovm--adt-bst-range)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . \"one\") (2 . \"two\") (3 . \"three\") (4 . \"four\") (5 . \"five\") (6 . \"six\") (7 . \"seven\") (8 . \"eight\") (9 . \"nine\")) \"five\" nil ((3 . \"three\") (4 . \"four\") (5 . \"five\") (6 . \"six\") (7 . \"seven\")) ((1 . \"one\") (2 . \"two\") (3 . \"three\") (4 . \"four\") (6 . \"six\") (7 . \"seven\") (8 . \"eight\") (9 . \"nine\")) nil \"THREE\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multimap (key -> list of values)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_multimap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multimap using hash table where each value is a list
    let form = r#"(progn
  (fset 'neovm--adt-mm-make
    (lambda () (make-hash-table :test 'equal)))

  (fset 'neovm--adt-mm-put
    (lambda (mm key val)
      (let ((existing (gethash key mm)))
        (puthash key (append existing (list val)) mm))))

  (fset 'neovm--adt-mm-get
    (lambda (mm key) (gethash key mm)))

  (fset 'neovm--adt-mm-remove-value
    (lambda (mm key val)
      (let ((existing (gethash key mm)))
        (puthash key (delete val existing) mm)
        (when (null (gethash key mm))
          (remhash key mm)))))

  (fset 'neovm--adt-mm-keys
    (lambda (mm)
      (let ((ks nil))
        (maphash (lambda (k _v) (setq ks (cons k ks))) mm)
        (sort ks (lambda (a b) (string< (format "%s" a) (format "%s" b)))))))

  (fset 'neovm--adt-mm-count
    (lambda (mm key) (length (gethash key mm))))

  (fset 'neovm--adt-mm-total-values
    (lambda (mm)
      (let ((total 0))
        (maphash (lambda (_k v) (setq total (+ total (length v)))) mm)
        total)))

  (unwind-protect
      (let ((mm (funcall 'neovm--adt-mm-make)))
        ;; Build multimap: tags -> articles
        (funcall 'neovm--adt-mm-put mm "elisp" "intro-to-elisp")
        (funcall 'neovm--adt-mm-put mm "elisp" "advanced-macros")
        (funcall 'neovm--adt-mm-put mm "elisp" "testing-patterns")
        (funcall 'neovm--adt-mm-put mm "rust" "ownership-guide")
        (funcall 'neovm--adt-mm-put mm "rust" "lifetimes-explained")
        (funcall 'neovm--adt-mm-put mm "python" "list-comprehensions")
        (let ((keys1 (funcall 'neovm--adt-mm-keys mm))
              (elisp-articles (funcall 'neovm--adt-mm-get mm "elisp"))
              (elisp-count (funcall 'neovm--adt-mm-count mm "elisp"))
              (total1 (funcall 'neovm--adt-mm-total-values mm)))
          ;; Remove a value
          (funcall 'neovm--adt-mm-remove-value mm "elisp" "advanced-macros")
          (let ((elisp-after (funcall 'neovm--adt-mm-get mm "elisp"))
                (elisp-count-after (funcall 'neovm--adt-mm-count mm "elisp")))
            ;; Remove all values for a key
            (funcall 'neovm--adt-mm-remove-value mm "python" "list-comprehensions")
            (let ((keys2 (funcall 'neovm--adt-mm-keys mm))
                  (total2 (funcall 'neovm--adt-mm-total-values mm)))
              (list keys1 elisp-articles elisp-count total1
                    elisp-after elisp-count-after
                    keys2 total2)))))
    (fmakunbound 'neovm--adt-mm-make)
    (fmakunbound 'neovm--adt-mm-put)
    (fmakunbound 'neovm--adt-mm-get)
    (fmakunbound 'neovm--adt-mm-remove-value)
    (fmakunbound 'neovm--adt-mm-keys)
    (fmakunbound 'neovm--adt-mm-count)
    (fmakunbound 'neovm--adt-mm-total-values)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"elisp\" \"python\" \"rust\") (\"intro-to-elisp\" \"testing-patterns\") 3 6 (\"intro-to-elisp\" \"testing-patterns\") 2 (\"elisp\" \"rust\") 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circular buffer with iterator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_circular_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Circular buffer: (vector write-idx count capacity)
    let form = r#"(progn
  (fset 'neovm--adt-cb-make
    (lambda (cap) (list (make-vector cap nil) 0 0 cap)))

  (fset 'neovm--adt-cb-write
    (lambda (cb val)
      (let ((buf (nth 0 cb)) (wr (nth 1 cb))
            (cnt (nth 2 cb)) (cap (nth 3 cb)))
        (aset buf wr val)
        (list buf (% (1+ wr) cap) (min (1+ cnt) cap) cap))))

  (fset 'neovm--adt-cb-read-idx
    (lambda (cb i)
      "Read the i-th oldest element."
      (let ((buf (nth 0 cb)) (wr (nth 1 cb))
            (cnt (nth 2 cb)) (cap (nth 3 cb)))
        (if (>= i cnt)
            nil
          (aref buf (% (+ (- wr cnt) i cap) cap))))))

  (fset 'neovm--adt-cb-to-list
    (lambda (cb)
      "Return all elements in order from oldest to newest."
      (let ((cnt (nth 2 cb)) (result nil))
        (let ((i 0))
          (while (< i cnt)
            (setq result (cons (funcall 'neovm--adt-cb-read-idx cb i) result))
            (setq i (1+ i))))
        (nreverse result))))

  (fset 'neovm--adt-cb-full-p
    (lambda (cb) (= (nth 2 cb) (nth 3 cb))))

  (fset 'neovm--adt-cb-count
    (lambda (cb) (nth 2 cb)))

  (unwind-protect
      (let ((cb (funcall 'neovm--adt-cb-make 4)))
        ;; Write 1,2,3
        (setq cb (funcall 'neovm--adt-cb-write cb 'a))
        (setq cb (funcall 'neovm--adt-cb-write cb 'b))
        (setq cb (funcall 'neovm--adt-cb-write cb 'c))
        (let ((state1 (funcall 'neovm--adt-cb-to-list cb))
              (full1 (funcall 'neovm--adt-cb-full-p cb))
              (cnt1 (funcall 'neovm--adt-cb-count cb)))
          ;; Fill it
          (setq cb (funcall 'neovm--adt-cb-write cb 'd))
          (let ((state2 (funcall 'neovm--adt-cb-to-list cb))
                (full2 (funcall 'neovm--adt-cb-full-p cb)))
            ;; Overflow: e overwrites a, f overwrites b
            (setq cb (funcall 'neovm--adt-cb-write cb 'e))
            (setq cb (funcall 'neovm--adt-cb-write cb 'f))
            (let ((state3 (funcall 'neovm--adt-cb-to-list cb))
                  (oldest (funcall 'neovm--adt-cb-read-idx cb 0))
                  (newest (funcall 'neovm--adt-cb-read-idx cb 3)))
              (list state1 full1 cnt1
                    state2 full2
                    state3 oldest newest)))))
    (fmakunbound 'neovm--adt-cb-make)
    (fmakunbound 'neovm--adt-cb-write)
    (fmakunbound 'neovm--adt-cb-read-idx)
    (fmakunbound 'neovm--adt-cb-to-list)
    (fmakunbound 'neovm--adt-cb-full-p)
    (fmakunbound 'neovm--adt-cb-count)))"#;
    let expect = expect_test::expect![[r#""OK ((a b c) nil 3 (a b c d) t (c d e f) c f)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Immutable persistent list (functional cons/car/cdr with sharing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_persistent_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate structural sharing: multiple "versions" of a list
    // that share common tails, simulating persistent data structures
    let form = r#"(let* (;; Base list
            (base '(3 4 5))
            ;; Two extensions sharing the same tail
            (v1 (cons 1 (cons 2 base)))
            (v2 (cons 10 (cons 20 base)))
            ;; Modifying v1 doesn't affect v2 or base
            (v3 (cons 0 v1))
            ;; Functional "update" by rebuilding prefix
            (update-nth
             (lambda (lst n val)
               "Return new list with nth element replaced."
               (if (= n 0)
                   (cons val (cdr lst))
                 (cons (car lst)
                       (funcall update-nth (cdr lst) (1- n) val)))))
            (v4 (funcall update-nth v1 2 99)))
       (list
         ;; Base is unchanged
         base
         ;; v1 and v2 share tail
         v1 v2
         ;; v3 extends v1
         v3
         ;; Structural sharing: cdr of cdr of v1 IS base
         (eq (cddr v1) base)
         (eq (cddr v2) base)
         ;; v4 has updated element but original v1 unchanged
         v4 v1
         ;; Verify the sharing didn't break
         (eq (cdddr v4) (cdddr v1))
         ;; Nested persistent updates
         (let* ((orig '(1 2 3 4 5))
                (mod1 (funcall update-nth orig 0 10))
                (mod2 (funcall update-nth orig 4 50))
                (mod3 (funcall update-nth mod1 2 30)))
           (list orig mod1 mod2 mod3
                 ;; Verify tail sharing where possible
                 (eq (nthcdr 1 orig) (nthcdr 1 mod1))  ;; no: both rebuilt
                 (equal (nthcdr 1 orig) (nthcdr 1 mod2))  ;; content equal
                 (length mod3)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable update-nth)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union-Find (Disjoint Set) data structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_union_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Union-Find with path compression and union by rank
    let form = r#"(progn
  (fset 'neovm--adt-uf-make
    (lambda (n)
      "Create UF structure: parent vector and rank vector."
      (let ((parent (make-vector n 0))
            (rank (make-vector n 0)))
        (dotimes (i n)
          (aset parent i i))
        (cons parent rank))))

  (fset 'neovm--adt-uf-find
    (lambda (uf x)
      "Find root with path compression."
      (let ((parent (car uf)))
        (while (/= (aref parent x) x)
          ;; Path compression: point to grandparent
          (aset parent x (aref parent (aref parent x)))
          (setq x (aref parent x)))
        x)))

  (fset 'neovm--adt-uf-union
    (lambda (uf x y)
      "Union by rank."
      (let ((rx (funcall 'neovm--adt-uf-find uf x))
            (ry (funcall 'neovm--adt-uf-find uf y)))
        (unless (= rx ry)
          (let ((rank (cdr uf)) (parent (car uf)))
            (cond
              ((< (aref rank rx) (aref rank ry))
               (aset parent rx ry))
              ((> (aref rank rx) (aref rank ry))
               (aset parent ry rx))
              (t
               (aset parent ry rx)
               (aset rank rx (1+ (aref rank rx))))))))))

  (fset 'neovm--adt-uf-connected
    (lambda (uf x y)
      (= (funcall 'neovm--adt-uf-find uf x)
         (funcall 'neovm--adt-uf-find uf y))))

  (fset 'neovm--adt-uf-components
    (lambda (uf n)
      "Count distinct components."
      (let ((roots (make-hash-table)))
        (dotimes (i n)
          (puthash (funcall 'neovm--adt-uf-find uf i) t roots))
        (hash-table-count roots))))

  (unwind-protect
      (let ((uf (funcall 'neovm--adt-uf-make 8)))
        ;; Initially 8 components
        (let ((c0 (funcall 'neovm--adt-uf-components uf 8)))
          ;; Union pairs: {0,1}, {2,3}, {4,5}, {6,7}
          (funcall 'neovm--adt-uf-union uf 0 1)
          (funcall 'neovm--adt-uf-union uf 2 3)
          (funcall 'neovm--adt-uf-union uf 4 5)
          (funcall 'neovm--adt-uf-union uf 6 7)
          (let ((c1 (funcall 'neovm--adt-uf-components uf 8))
                (conn01 (funcall 'neovm--adt-uf-connected uf 0 1))
                (conn02 (funcall 'neovm--adt-uf-connected uf 0 2)))
            ;; Merge {0,1} with {2,3} and {4,5} with {6,7}
            (funcall 'neovm--adt-uf-union uf 0 3)
            (funcall 'neovm--adt-uf-union uf 5 6)
            (let ((c2 (funcall 'neovm--adt-uf-components uf 8))
                  (conn03 (funcall 'neovm--adt-uf-connected uf 0 3))
                  (conn04 (funcall 'neovm--adt-uf-connected uf 0 4)))
              ;; Final merge: all into one
              (funcall 'neovm--adt-uf-union uf 1 7)
              (let ((c3 (funcall 'neovm--adt-uf-components uf 8))
                    (all-connected
                     (let ((ok t))
                       (dotimes (i 7)
                         (unless (funcall 'neovm--adt-uf-connected uf i (1+ i))
                           (setq ok nil)))
                       ok)))
                (list c0 c1 conn01 conn02
                      c2 conn03 conn04
                      c3 all-connected))))))
    (fmakunbound 'neovm--adt-uf-make)
    (fmakunbound 'neovm--adt-uf-find)
    (fmakunbound 'neovm--adt-uf-union)
    (fmakunbound 'neovm--adt-uf-connected)
    (fmakunbound 'neovm--adt-uf-components)))"#;
    let expect = expect_test::expect![[r#""OK (8 4 t nil 2 t nil 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval tree (insert and query overlapping intervals)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_adt_interval_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple interval collection with overlap query (not a balanced tree,
    // but uses sorted insertion and scan for correctness testing).
    let form = r#"(progn
  (fset 'neovm--adt-it-make (lambda () nil))

  (fset 'neovm--adt-it-insert
    (lambda (tree lo hi val)
      "Insert interval [lo, hi) with associated val."
      (let ((entry (list lo hi val))
            (prev nil) (curr tree) (inserted nil))
        (while (and curr (not inserted))
          (if (<= lo (car (car curr)))
              (progn
                (if prev
                    (setcdr prev (cons entry curr))
                  (setq tree (cons entry curr)))
                (setq inserted t))
            (setq prev curr curr (cdr curr))))
        (unless inserted
          (if prev
              (setcdr prev (list entry))
            (setq tree (list entry))))
        tree)))

  (fset 'neovm--adt-it-overlaps
    (lambda (tree qlo qhi)
      "Return all intervals overlapping [qlo, qhi)."
      (let ((result nil))
        (dolist (interval tree)
          (let ((lo (nth 0 interval)) (hi (nth 1 interval)))
            ;; Overlap: lo < qhi AND hi > qlo
            (when (and (< lo qhi) (> hi qlo))
              (setq result (cons interval result)))))
        (nreverse result))))

  (fset 'neovm--adt-it-point-query
    (lambda (tree pt)
      "Return all intervals containing point PT."
      (funcall 'neovm--adt-it-overlaps tree pt (1+ pt))))

  (fset 'neovm--adt-it-count
    (lambda (tree) (length tree)))

  (unwind-protect
      (let ((tree (funcall 'neovm--adt-it-make)))
        (setq tree (funcall 'neovm--adt-it-insert tree 1 5 "a"))
        (setq tree (funcall 'neovm--adt-it-insert tree 3 8 "b"))
        (setq tree (funcall 'neovm--adt-it-insert tree 6 10 "c"))
        (setq tree (funcall 'neovm--adt-it-insert tree 12 15 "d"))
        (setq tree (funcall 'neovm--adt-it-insert tree 0 20 "e"))
        (list
          ;; Total intervals
          (funcall 'neovm--adt-it-count tree)
          ;; Query [4, 7): should overlap a, b, c, e
          (mapcar #'caddr (funcall 'neovm--adt-it-overlaps tree 4 7))
          ;; Point query at 3: a, b, e
          (mapcar #'caddr (funcall 'neovm--adt-it-point-query tree 3))
          ;; Point query at 13: d, e
          (mapcar #'caddr (funcall 'neovm--adt-it-point-query tree 13))
          ;; Query [10, 12): only e
          (mapcar #'caddr (funcall 'neovm--adt-it-overlaps tree 10 12))
          ;; Query [20, 25): nothing (e ends at 20, exclusive)
          (mapcar #'caddr (funcall 'neovm--adt-it-overlaps tree 20 25))
          ;; All intervals sorted by start
          (mapcar (lambda (iv) (list (nth 0 iv) (nth 1 iv) (nth 2 iv))) tree)))
    (fmakunbound 'neovm--adt-it-make)
    (fmakunbound 'neovm--adt-it-insert)
    (fmakunbound 'neovm--adt-it-overlaps)
    (fmakunbound 'neovm--adt-it-point-query)
    (fmakunbound 'neovm--adt-it-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (\"e\" \"a\" \"b\" \"c\") (\"e\" \"a\" \"b\") (\"e\" \"d\") (\"e\") nil ((0 20 \"e\") (1 5 \"a\") (3 8 \"b\") (6 10 \"c\") (12 15 \"d\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
