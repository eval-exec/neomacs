//! Complex oracle tests for recursion patterns: mutual recursion,
//! tree traversal (pre/in/post-order), recursive descent parsing,
//! accumulator-passing style, depth-limited flattening, Tower of Hanoi,
//! and recursive glob-style pattern matching.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Mutual recursion: even/odd predicates with Collatz-like twist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_mutual_even_odd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic mutual recursion plus using it to classify numbers
    let form = r#"(progn
                    (fset 'neovm--test-my-even-p
                      (lambda (n)
                        (cond
                         ((< n 0) (funcall 'neovm--test-my-even-p (- n)))
                         ((= n 0) t)
                         (t (funcall 'neovm--test-my-odd-p (1- n))))))
                    (fset 'neovm--test-my-odd-p
                      (lambda (n)
                        (cond
                         ((< n 0) (funcall 'neovm--test-my-odd-p (- n)))
                         ((= n 0) nil)
                         (t (funcall 'neovm--test-my-even-p (1- n))))))
                    (unwind-protect
                        (let ((results nil))
                          (dolist (n '(0 1 2 7 13 20 -4 -7) (nreverse results))
                            (setq results
                                  (cons (list n
                                              (funcall 'neovm--test-my-even-p n)
                                              (funcall 'neovm--test-my-odd-p n))
                                        results))))
                      (fmakunbound 'neovm--test-my-even-p)
                      (fmakunbound 'neovm--test-my-odd-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 t nil) (1 nil t) (2 t nil) (7 nil t) (13 nil t) (20 t nil) (-4 t nil) (-7 nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary tree traversal: pre-order, in-order, post-order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_tree_traversals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binary tree represented as (value left right) or leaf atoms
    let form = r#"(progn
                    (fset 'neovm--test-tree-val (lambda (t) (car t)))
                    (fset 'neovm--test-tree-left (lambda (t) (cadr t)))
                    (fset 'neovm--test-tree-right (lambda (t) (caddr t)))
                    (fset 'neovm--test-tree-leaf-p (lambda (t) (atom t)))
                    ;; Pre-order: root, left, right
                    (fset 'neovm--test-preorder
                      (lambda (tree)
                        (if (funcall 'neovm--test-tree-leaf-p tree)
                            (if tree (list tree) nil)
                          (append (list (funcall 'neovm--test-tree-val tree))
                                  (funcall 'neovm--test-preorder
                                           (funcall 'neovm--test-tree-left tree))
                                  (funcall 'neovm--test-preorder
                                           (funcall 'neovm--test-tree-right tree))))))
                    ;; In-order: left, root, right
                    (fset 'neovm--test-inorder
                      (lambda (tree)
                        (if (funcall 'neovm--test-tree-leaf-p tree)
                            (if tree (list tree) nil)
                          (append (funcall 'neovm--test-inorder
                                           (funcall 'neovm--test-tree-left tree))
                                  (list (funcall 'neovm--test-tree-val tree))
                                  (funcall 'neovm--test-inorder
                                           (funcall 'neovm--test-tree-right tree))))))
                    ;; Post-order: left, right, root
                    (fset 'neovm--test-postorder
                      (lambda (tree)
                        (if (funcall 'neovm--test-tree-leaf-p tree)
                            (if tree (list tree) nil)
                          (append (funcall 'neovm--test-postorder
                                           (funcall 'neovm--test-tree-left tree))
                                  (funcall 'neovm--test-postorder
                                           (funcall 'neovm--test-tree-right tree))
                                  (list (funcall 'neovm--test-tree-val tree))))))
                    (unwind-protect
                        ;; Tree:        4
                        ;;            /   \
                        ;;           2     6
                        ;;          / \   / \
                        ;;         1   3 5   7
                        (let ((tree '(4 (2 1 3) (6 5 7))))
                          (list
                           (funcall 'neovm--test-preorder tree)
                           (funcall 'neovm--test-inorder tree)
                           (funcall 'neovm--test-postorder tree)))
                      (fmakunbound 'neovm--test-tree-val)
                      (fmakunbound 'neovm--test-tree-left)
                      (fmakunbound 'neovm--test-tree-right)
                      (fmakunbound 'neovm--test-tree-leaf-p)
                      (fmakunbound 'neovm--test-preorder)
                      (fmakunbound 'neovm--test-inorder)
                      (fmakunbound 'neovm--test-postorder)))"#;
    let expect =
        expect_test::expect![[r#""OK ((4 2 1 3 6 5 7) (1 2 3 4 5 6 7) (1 3 2 5 7 6 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive descent parser for arithmetic with subtraction and division
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_arithmetic_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive descent parser that builds an AST then evaluates it
    let form = r#"(progn
                    (defvar neovm--test-tokens nil)
                    (fset 'neovm--test-peek (lambda () (car neovm--test-tokens)))
                    (fset 'neovm--test-eat
                      (lambda ()
                        (prog1 (car neovm--test-tokens)
                          (setq neovm--test-tokens (cdr neovm--test-tokens)))))
                    ;; expr -> term ((+|-) term)*
                    (fset 'neovm--test-parse-expr
                      (lambda ()
                        (let ((node (funcall 'neovm--test-parse-term)))
                          (while (memq (funcall 'neovm--test-peek) '(+ -))
                            (let ((op (funcall 'neovm--test-eat))
                                  (right (funcall 'neovm--test-parse-term)))
                              (setq node (list op node right))))
                          node)))
                    ;; term -> factor ((*|/) factor)*
                    (fset 'neovm--test-parse-term
                      (lambda ()
                        (let ((node (funcall 'neovm--test-parse-factor)))
                          (while (memq (funcall 'neovm--test-peek) '(* /))
                            (let ((op (funcall 'neovm--test-eat))
                                  (right (funcall 'neovm--test-parse-factor)))
                              (setq node (list op node right))))
                          node)))
                    ;; factor -> NUM | ( expr )
                    (fset 'neovm--test-parse-factor
                      (lambda ()
                        (if (eq (funcall 'neovm--test-peek) 'lp)
                            (progn
                              (funcall 'neovm--test-eat)
                              (let ((node (funcall 'neovm--test-parse-expr)))
                                (funcall 'neovm--test-eat) ; rp
                                node))
                          (funcall 'neovm--test-eat))))
                    ;; Context for AST
                    (fset 'neovm--test-eval-ast
                      (lambda (ast)
                        (if (numberp ast)
                            ast
                          (let ((op (car ast))
                                (l (funcall 'neovm--test-eval-ast (cadr ast)))
                                (r (funcall 'neovm--test-eval-ast (caddr ast))))
                            (cond ((eq op '+) (+ l r))
                                  ((eq op '-) (- l r))
                                  ((eq op '*) (* l r))
                                  ((eq op '/) (/ l r)))))))
                    (unwind-protect
                        (list
                         ;; 3 + 4 * 2 - 1 => 3 + 8 - 1 = 10
                         (progn
                           (setq neovm--test-tokens '(3 + 4 * 2 - 1))
                           (let ((ast (funcall 'neovm--test-parse-expr)))
                             (list ast (funcall 'neovm--test-eval-ast ast))))
                         ;; (10 - 3) * (2 + 1) = 21
                         (progn
                           (setq neovm--test-tokens '(lp 10 - 3 rp * lp 2 + 1 rp))
                           (let ((ast (funcall 'neovm--test-parse-expr)))
                             (list ast (funcall 'neovm--test-eval-ast ast))))
                         ;; 100 / 5 / 4 = 5 (left-associative)
                         (progn
                           (setq neovm--test-tokens '(100 / 5 / 4))
                           (let ((ast (funcall 'neovm--test-parse-expr)))
                             (list ast (funcall 'neovm--test-eval-ast ast)))))
                      (fmakunbound 'neovm--test-peek)
                      (fmakunbound 'neovm--test-eat)
                      (fmakunbound 'neovm--test-parse-expr)
                      (fmakunbound 'neovm--test-parse-term)
                      (fmakunbound 'neovm--test-parse-factor)
                      (fmakunbound 'neovm--test-eval-ast)
                      (makunbound 'neovm--test-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK (((- (+ 3 (* 4 2)) 1) 10) ((* (- 10 3) (+ 2 1)) 21) ((/ (/ 100 5) 4) 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Accumulator-passing style (tail-recursive patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_accumulator_passing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Several functions using accumulator-passing style
    let form = r#"(progn
                    ;; Reverse with accumulator
                    (fset 'neovm--test-rev-acc
                      (lambda (lst acc)
                        (if (null lst) acc
                          (funcall 'neovm--test-rev-acc
                                   (cdr lst)
                                   (cons (car lst) acc)))))
                    ;; Sum with accumulator
                    (fset 'neovm--test-sum-acc
                      (lambda (lst acc)
                        (if (null lst) acc
                          (funcall 'neovm--test-sum-acc
                                   (cdr lst)
                                   (+ acc (car lst))))))
                    ;; Map with accumulator (builds result in reverse, then reverses)
                    (fset 'neovm--test-map-acc
                      (lambda (fn lst acc)
                        (if (null lst)
                            (funcall 'neovm--test-rev-acc acc nil)
                          (funcall 'neovm--test-map-acc
                                   fn (cdr lst)
                                   (cons (funcall fn (car lst)) acc)))))
                    ;; Filter with accumulator
                    (fset 'neovm--test-filter-acc
                      (lambda (pred lst acc)
                        (if (null lst)
                            (funcall 'neovm--test-rev-acc acc nil)
                          (funcall 'neovm--test-filter-acc
                                   pred (cdr lst)
                                   (if (funcall pred (car lst))
                                       (cons (car lst) acc)
                                     acc)))))
                    (unwind-protect
                        (list
                         (funcall 'neovm--test-rev-acc '(1 2 3 4 5) nil)
                         (funcall 'neovm--test-sum-acc '(1 2 3 4 5 6 7 8 9 10) 0)
                         (funcall 'neovm--test-map-acc
                                  (lambda (x) (* x x))
                                  '(1 2 3 4 5) nil)
                         (funcall 'neovm--test-filter-acc
                                  (lambda (x) (= (% x 2) 0))
                                  '(1 2 3 4 5 6 7 8 9 10) nil))
                      (fmakunbound 'neovm--test-rev-acc)
                      (fmakunbound 'neovm--test-sum-acc)
                      (fmakunbound 'neovm--test-map-acc)
                      (fmakunbound 'neovm--test-filter-acc)))"#;
    let expect = expect_test::expect![[r#""OK ((5 4 3 2 1) 55 (1 4 9 16 25) (2 4 6 8 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive list flattening with depth limit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_flatten_depth_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten nested lists to a specified depth, leaving deeper nesting intact
    let form = r#"(progn
                    (fset 'neovm--test-flatten-depth
                      (lambda (lst depth)
                        (cond
                         ((null lst) nil)
                         ((atom lst) (list lst))
                         ((<= depth 0) (list lst))
                         (t (append
                             (funcall 'neovm--test-flatten-depth
                                      (car lst) (1- depth))
                             (funcall 'neovm--test-flatten-depth
                                      (cdr lst) depth))))))
                    (unwind-protect
                        (let ((deeply-nested '(1 (2 (3 (4 (5)))))))
                          (list
                           ;; Depth 0: no flattening
                           (funcall 'neovm--test-flatten-depth deeply-nested 0)
                           ;; Depth 1: flatten one level
                           (funcall 'neovm--test-flatten-depth deeply-nested 1)
                           ;; Depth 2: flatten two levels
                           (funcall 'neovm--test-flatten-depth deeply-nested 2)
                           ;; Depth 3: flatten three levels
                           (funcall 'neovm--test-flatten-depth deeply-nested 3)
                           ;; Depth 10: fully flat
                           (funcall 'neovm--test-flatten-depth deeply-nested 10)
                           ;; Complex structure at depth 1
                           (funcall 'neovm--test-flatten-depth
                                    '((a b) ((c d) (e f)) (((g h))))
                                    1)))
                      (fmakunbound 'neovm--test-flatten-depth)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 (2 (3 (4 (5)))))) (1 (2 (3 (4 (5))))) (1 2 (3 (4 (5)))) (1 2 3 (4 (5))) (1 2 3 4 5) ((a b) ((c d) (e f)) (((g h)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tower of Hanoi with move recording
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_tower_of_hanoi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic Tower of Hanoi, recording each move as (from . to)
    let form = r#"(progn
                    (defvar neovm--test-hanoi-moves nil)
                    (fset 'neovm--test-hanoi
                      (lambda (n from to via)
                        (when (> n 0)
                          (funcall 'neovm--test-hanoi (1- n) from via to)
                          (setq neovm--test-hanoi-moves
                                (cons (list n from to) neovm--test-hanoi-moves))
                          (funcall 'neovm--test-hanoi (1- n) via to from))))
                    (unwind-protect
                        (list
                         ;; 3 disks
                         (progn
                           (setq neovm--test-hanoi-moves nil)
                           (funcall 'neovm--test-hanoi 3 'A 'C 'B)
                           (list (length neovm--test-hanoi-moves)
                                 (nreverse neovm--test-hanoi-moves)))
                         ;; 4 disks: just count moves (2^4 - 1 = 15)
                         (progn
                           (setq neovm--test-hanoi-moves nil)
                           (funcall 'neovm--test-hanoi 4 'L 'R 'M)
                           (length neovm--test-hanoi-moves)))
                      (fmakunbound 'neovm--test-hanoi)
                      (makunbound 'neovm--test-hanoi-moves)))"#;
    let expect = expect_test::expect![[
        r#""OK ((7 ((1 A C) (2 A B) (1 C B) (3 A C) (1 B A) (2 B C) (1 A C))) 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive string matching (simple glob pattern: *, ?)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_glob_pattern_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive glob matcher: * matches zero or more chars, ? matches exactly one
    let form = r#"(progn
                    (fset 'neovm--test-glob-match
                      (lambda (pattern str pi si)
                        (let ((plen (length pattern))
                              (slen (length str)))
                          (cond
                           ;; Both exhausted: match
                           ((and (= pi plen) (= si slen)) t)
                           ;; Pattern exhausted but string remains: no match
                           ((= pi plen) nil)
                           ;; Star: try matching zero chars or one char from string
                           ((= (aref pattern pi) ?*)
                            (or
                             ;; * matches zero chars: advance pattern
                             (funcall 'neovm--test-glob-match pattern str (1+ pi) si)
                             ;; * matches one char: advance string (if chars remain)
                             (and (< si slen)
                                  (funcall 'neovm--test-glob-match pattern str pi (1+ si)))))
                           ;; String exhausted but pattern has non-star: no match
                           ((= si slen) nil)
                           ;; Question mark: match any single char
                           ((= (aref pattern pi) ??)
                            (funcall 'neovm--test-glob-match pattern str (1+ pi) (1+ si)))
                           ;; Literal match
                           ((= (aref pattern pi) (aref str si))
                            (funcall 'neovm--test-glob-match pattern str (1+ pi) (1+ si)))
                           ;; No match
                           (t nil)))))
                    (unwind-protect
                        (let ((cases '(("hello" "hello" t)
                                       ("h*o" "hello" t)
                                       ("h*o" "helo" t)
                                       ("h*o" "h" nil)
                                       ("h?llo" "hello" t)
                                       ("h?llo" "hllo" nil)
                                       ("*" "anything" t)
                                       ("*" "" t)
                                       ("a*b*c" "abc" t)
                                       ("a*b*c" "aXXbYYc" t)
                                       ("a*b*c" "aXXbYY" nil)
                                       ("???" "abc" t)
                                       ("???" "ab" nil)
                                       ("*?*" "x" t)
                                       ("*?*" "" nil)))
                              (results nil))
                          (dolist (c cases (nreverse results))
                            (let* ((pat (nth 0 c))
                                   (str (nth 1 c))
                                   (expected (nth 2 c))
                                   (actual (funcall 'neovm--test-glob-match
                                                    pat str 0 0)))
                              (setq results
                                    (cons (list pat str expected
                                               (if actual t nil)
                                               (eq expected (if actual t nil)))
                                          results)))))
                      (fmakunbound 'neovm--test-glob-match)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"hello\" t t t) (\"h*o\" \"hello\" t t t) (\"h*o\" \"helo\" t t t) (\"h*o\" \"h\" nil nil t) (\"h?llo\" \"hello\" t t t) (\"h?llo\" \"hllo\" nil nil t) (\"*\" \"anything\" t t t) (\"*\" \"\" t t t) (\"a*b*c\" \"abc\" t t t) (\"a*b*c\" \"aXXbYYc\" t t t) (\"a*b*c\" \"aXXbYY\" nil nil t) (\"???\" \"abc\" t t t) (\"???\" \"ab\" nil nil t) (\"*?*\" \"x\" t t t) (\"*?*\" \"\" nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive mergesort
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_mergesort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full recursive mergesort implementation
    let form = r#"(progn
                    ;; Split list into two halves
                    (fset 'neovm--test-split
                      (lambda (lst)
                        (let ((left nil) (right nil) (toggle t))
                          (while lst
                            (if toggle
                                (setq left (cons (car lst) left))
                              (setq right (cons (car lst) right)))
                            (setq toggle (not toggle)
                                  lst (cdr lst)))
                          (list (nreverse left) (nreverse right)))))
                    ;; Merge two sorted lists
                    (fset 'neovm--test-merge
                      (lambda (a b)
                        (cond
                         ((null a) b)
                         ((null b) a)
                         ((<= (car a) (car b))
                          (cons (car a)
                                (funcall 'neovm--test-merge (cdr a) b)))
                         (t
                          (cons (car b)
                                (funcall 'neovm--test-merge a (cdr b)))))))
                    ;; Mergesort
                    (fset 'neovm--test-msort
                      (lambda (lst)
                        (if (or (null lst) (null (cdr lst)))
                            lst
                          (let* ((halves (funcall 'neovm--test-split lst))
                                 (left (funcall 'neovm--test-msort (car halves)))
                                 (right (funcall 'neovm--test-msort (cadr halves))))
                            (funcall 'neovm--test-merge left right)))))
                    (unwind-protect
                        (list
                         (funcall 'neovm--test-msort nil)
                         (funcall 'neovm--test-msort '(1))
                         (funcall 'neovm--test-msort '(3 1))
                         (funcall 'neovm--test-msort '(5 3 8 1 9 2 7 4 6))
                         (funcall 'neovm--test-msort '(10 9 8 7 6 5 4 3 2 1))
                         (funcall 'neovm--test-msort '(1 1 1 2 2 3)))
                      (fmakunbound 'neovm--test-split)
                      (fmakunbound 'neovm--test-merge)
                      (fmakunbound 'neovm--test-msort)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1) (1 3) (1 2 3 4 5 6 7 8 9) (1 2 3 4 5 6 7 8 9 10) (1 1 1 2 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
