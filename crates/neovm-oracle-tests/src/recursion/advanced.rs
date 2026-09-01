//! Advanced oracle parity tests for recursion patterns: mutual recursion,
//! tree operations, recursive descent parsing, accumulator-passing style,
//! continuation-passing style, and complex algorithmic recursion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Mutual recursion: Collatz-like with two functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_mutual_collatz() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two mutually recursive functions that classify numbers by a
    // Collatz-like rule, collecting the chain of operations.
    let form = r#"(progn
      (fset 'neovm--radv-step-even
        (lambda (n acc)
          (if (<= n 1) (nreverse (cons n acc))
            (if (= (% n 2) 0)
                (funcall 'neovm--radv-step-even (/ n 2) (cons (list 'halve n) acc))
              (funcall 'neovm--radv-step-odd n acc)))))
      (fset 'neovm--radv-step-odd
        (lambda (n acc)
          (funcall 'neovm--radv-step-even (1+ (* 3 n)) (cons (list 'triple+1 n) acc))))
      (unwind-protect
          (list
           (funcall 'neovm--radv-step-even 1 nil)
           (funcall 'neovm--radv-step-even 6 nil)
           (funcall 'neovm--radv-step-even 7 nil)
           (funcall 'neovm--radv-step-even 12 nil))
        (fmakunbound 'neovm--radv-step-even)
        (fmakunbound 'neovm--radv-step-odd)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1) ((halve 6) (triple+1 3) (halve 10) (triple+1 5) (halve 16) (halve 8) (halve 4) (halve 2) 1) ((triple+1 7) (halve 22) (triple+1 11) (halve 34) (triple+1 17) (halve 52) (halve 26) (triple+1 13) (halve 40) (halve 20) (halve 10) (triple+1 5) (halve 16) (halve 8) (halve 4) (halve 2) 1) ((halve 12) (halve 6) (triple+1 3) (halve 10) (triple+1 5) (halve 16) (halve 8) (halve 4) (halve 2) 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree operations: count, fold, zip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_tree_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      ;; Count all atoms (leaves) in a tree
      (fset 'neovm--radv-tree-count
        (lambda (tree)
          (cond
           ((null tree) 0)
           ((atom tree) 1)
           (t (+ (funcall 'neovm--radv-tree-count (car tree))
                 (funcall 'neovm--radv-tree-count (cdr tree)))))))
      ;; Fold (reduce) over all atoms left-to-right
      (fset 'neovm--radv-tree-fold
        (lambda (fn init tree)
          (cond
           ((null tree) init)
           ((atom tree) (funcall fn init tree))
           (t (funcall 'neovm--radv-tree-fold
                       fn
                       (funcall 'neovm--radv-tree-fold fn init (car tree))
                       (cdr tree))))))
      ;; Map over tree preserving structure
      (fset 'neovm--radv-tree-map
        (lambda (fn tree)
          (cond
           ((null tree) nil)
           ((atom tree) (funcall fn tree))
           (t (cons (funcall 'neovm--radv-tree-map fn (car tree))
                    (funcall 'neovm--radv-tree-map fn (cdr tree)))))))
      (unwind-protect
          (let ((tree '(1 (2 (3 4)) (5 (6 7 8)))))
            (list
             (funcall 'neovm--radv-tree-count tree)
             (funcall 'neovm--radv-tree-fold '+ 0 tree)
             (funcall 'neovm--radv-tree-fold 'max 0 tree)
             (funcall 'neovm--radv-tree-map '1+ tree)))
        (fmakunbound 'neovm--radv-tree-count)
        (fmakunbound 'neovm--radv-tree-fold)
        (fmakunbound 'neovm--radv-tree-map)))"#;
    let expect = expect_test::expect![[r#""OK (8 36 8 (2 (3 (4 5)) (6 (7 8 9))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive descent parser with subtraction and division
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_recursive_descent_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full four-operation arithmetic parser: +, -, *, /
    let form = r#"(progn
      (defvar neovm--radv-tokens nil)
      (fset 'neovm--radv-peek (lambda () (car neovm--radv-tokens)))
      (fset 'neovm--radv-eat
        (lambda ()
          (prog1 (car neovm--radv-tokens)
            (setq neovm--radv-tokens (cdr neovm--radv-tokens)))))
      ;; expr = term (('+' | '-') term)*
      (fset 'neovm--radv-parse-expr
        (lambda ()
          (let ((left (funcall 'neovm--radv-parse-term)))
            (while (memq (funcall 'neovm--radv-peek) '(+ -))
              (let ((op (funcall 'neovm--radv-eat)))
                (let ((right (funcall 'neovm--radv-parse-term)))
                  (setq left (if (eq op '+) (+ left right) (- left right))))))
            left)))
      ;; term = factor (('*' | '/) factor)*
      (fset 'neovm--radv-parse-term
        (lambda ()
          (let ((left (funcall 'neovm--radv-parse-factor)))
            (while (memq (funcall 'neovm--radv-peek) '(* /))
              (let ((op (funcall 'neovm--radv-eat)))
                (let ((right (funcall 'neovm--radv-parse-factor)))
                  (setq left (if (eq op '*) (* left right) (/ left right))))))
            left)))
      ;; factor = number | '(' expr ')' | '-' factor (unary minus)
      (fset 'neovm--radv-parse-factor
        (lambda ()
          (let ((tok (funcall 'neovm--radv-peek)))
            (cond
             ((numberp tok) (funcall 'neovm--radv-eat))
             ((eq tok 'lp)
              (funcall 'neovm--radv-eat)
              (let ((val (funcall 'neovm--radv-parse-expr)))
                (funcall 'neovm--radv-eat)  ;; consume rp
                val))
             ((eq tok '-)
              (funcall 'neovm--radv-eat)
              (- (funcall 'neovm--radv-parse-factor)))))))
      (unwind-protect
          (list
           ;; 2 + 3 * 4 - 1 = 13
           (progn (setq neovm--radv-tokens '(2 + 3 * 4 - 1))
                  (funcall 'neovm--radv-parse-expr))
           ;; (2 + 3) * (4 - 1) = 15
           (progn (setq neovm--radv-tokens '(lp 2 + 3 rp * lp 4 - 1 rp))
                  (funcall 'neovm--radv-parse-expr))
           ;; 100 / 5 / 4 = 5  (left-associative)
           (progn (setq neovm--radv-tokens '(100 / 5 / 4))
                  (funcall 'neovm--radv-parse-expr))
           ;; -3 * -2 = 6  (unary minus)
           (progn (setq neovm--radv-tokens '(- 3 * - 2))
                  (funcall 'neovm--radv-parse-expr))
           ;; 1 + 2 + 3 + 4 + 5 = 15
           (progn (setq neovm--radv-tokens '(1 + 2 + 3 + 4 + 5))
                  (funcall 'neovm--radv-parse-expr)))
        (fmakunbound 'neovm--radv-peek)
        (fmakunbound 'neovm--radv-eat)
        (fmakunbound 'neovm--radv-parse-expr)
        (fmakunbound 'neovm--radv-parse-term)
        (fmakunbound 'neovm--radv-parse-factor)
        (makunbound 'neovm--radv-tokens)))"#;
    let expect = expect_test::expect![[r#""OK (13 15 5 6 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Accumulator-passing style
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_accumulator_passing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      ;; Reverse a list with accumulator (tail-recursive style)
      (fset 'neovm--radv-rev-acc
        (lambda (lst acc)
          (if (null lst) acc
            (funcall 'neovm--radv-rev-acc (cdr lst) (cons (car lst) acc)))))
      ;; Flatten a tree with accumulator
      (fset 'neovm--radv-flat-acc
        (lambda (tree acc)
          (cond
           ((null tree) acc)
           ((atom tree) (cons tree acc))
           (t (funcall 'neovm--radv-flat-acc
                       (car tree)
                       (funcall 'neovm--radv-flat-acc (cdr tree) acc))))))
      ;; Map with accumulator (preserves order via reverse at end)
      (fset 'neovm--radv-map-acc
        (lambda (fn lst acc)
          (if (null lst)
              (funcall 'neovm--radv-rev-acc acc nil)
            (funcall 'neovm--radv-map-acc fn (cdr lst)
                     (cons (funcall fn (car lst)) acc)))))
      (unwind-protect
          (list
           (funcall 'neovm--radv-rev-acc '(1 2 3 4 5) nil)
           (funcall 'neovm--radv-flat-acc '(1 (2 (3)) (4 5)) nil)
           (funcall 'neovm--radv-map-acc
                    (lambda (x) (* x x))
                    '(1 2 3 4 5) nil))
        (fmakunbound 'neovm--radv-rev-acc)
        (fmakunbound 'neovm--radv-flat-acc)
        (fmakunbound 'neovm--radv-map-acc)))"#;
    let expect = expect_test::expect![[r#""OK ((5 4 3 2 1) (1 2 3 4 5) (1 4 9 16 25))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Continuation-passing style (CPS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_continuation_passing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      ;; CPS factorial: (cps-fact n k) calls (funcall k result)
      (fset 'neovm--radv-cps-fact
        (lambda (n k)
          (if (<= n 1)
              (funcall k 1)
            (funcall 'neovm--radv-cps-fact
                     (1- n)
                     (lambda (v) (funcall k (* n v)))))))
      ;; CPS fibonacci
      (fset 'neovm--radv-cps-fib
        (lambda (n k)
          (cond
           ((= n 0) (funcall k 0))
           ((= n 1) (funcall k 1))
           (t (funcall 'neovm--radv-cps-fib
                       (- n 1)
                       (lambda (v1)
                         (funcall 'neovm--radv-cps-fib
                                  (- n 2)
                                  (lambda (v2)
                                    (funcall k (+ v1 v2))))))))))
      ;; CPS map over list
      (fset 'neovm--radv-cps-map
        (lambda (fn lst k)
          (if (null lst)
              (funcall k nil)
            (funcall 'neovm--radv-cps-map
                     fn (cdr lst)
                     (lambda (rest)
                       (funcall k (cons (funcall fn (car lst)) rest)))))))
      (unwind-protect
          (list
           (funcall 'neovm--radv-cps-fact 5 #'identity)
           (funcall 'neovm--radv-cps-fact 10 #'identity)
           (funcall 'neovm--radv-cps-fib 8 #'identity)
           (funcall 'neovm--radv-cps-map
                    (lambda (x) (* x x))
                    '(1 2 3 4 5)
                    #'identity))
        (fmakunbound 'neovm--radv-cps-fact)
        (fmakunbound 'neovm--radv-cps-fib)
        (fmakunbound 'neovm--radv-cps-map)))"#;
    let expect = expect_test::expect![[r#""OK (120 3628800 21 (1 4 9 16 25))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive pattern matcher
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_pattern_matcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple recursive pattern matcher for lists.
    // Patterns: 'any matches anything, (quote x) matches literal x,
    // a list pattern matches element-by-element, nil matches nil.
    let form = r#"(progn
      (fset 'neovm--radv-pmatch
        (lambda (pattern data)
          (cond
           ;; 'any matches anything
           ((eq pattern 'any) t)
           ;; nil matches nil
           ((and (null pattern) (null data)) t)
           ;; atom equality
           ((and (atom pattern) (atom data)) (equal pattern data))
           ;; both cons: match car and cdr
           ((and (consp pattern) (consp data))
            (and (funcall 'neovm--radv-pmatch (car pattern) (car data))
                 (funcall 'neovm--radv-pmatch (cdr pattern) (cdr data))))
           (t nil))))
      (unwind-protect
          (list
           ;; exact match
           (funcall 'neovm--radv-pmatch '(1 2 3) '(1 2 3))
           ;; wildcard
           (funcall 'neovm--radv-pmatch '(1 any 3) '(1 99 3))
           ;; nested wildcard
           (funcall 'neovm--radv-pmatch '(1 (any 3) 4) '(1 (2 3) 4))
           ;; mismatch
           (funcall 'neovm--radv-pmatch '(1 2 3) '(1 2 4))
           ;; length mismatch
           (funcall 'neovm--radv-pmatch '(1 2) '(1 2 3))
           ;; all wildcards
           (funcall 'neovm--radv-pmatch '(any any any) '(a b c))
           ;; deep nesting
           (funcall 'neovm--radv-pmatch
                    '((any (any any)) any)
                    '((1 (2 3)) 4)))
        (fmakunbound 'neovm--radv-pmatch)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive alist-tree flattener (simulated directory tree)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_alist_tree_flattener() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An alist tree represents a file system: directories are alists,
    // files are strings. Flatten to a list of (path . content) pairs.
    let form = r#"(progn
      (fset 'neovm--radv-flatten-dir
        (lambda (tree prefix)
          (let ((result nil))
            (dolist (entry tree)
              (let ((name (car entry))
                    (val (cdr entry)))
                (let ((path (if (string= prefix "")
                                (symbol-name name)
                              (concat prefix "/" (symbol-name name)))))
                  (if (stringp val)
                      ;; file
                      (setq result (cons (cons path val) result))
                    ;; directory (alist)
                    (setq result (append result
                                        (funcall 'neovm--radv-flatten-dir val path)))))))
            result)))
      (unwind-protect
          (let ((tree '((src . ((main . "int main() {}")
                                (util . ((helper . "void help() {}")))
                                (lib . ((math . "double sqrt()")
                                        (io . "FILE* open()")))))
                        (doc . ((readme . "Hello"))))))
            (funcall 'neovm--radv-flatten-dir tree ""))
        (fmakunbound 'neovm--radv-flatten-dir)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"src/main\" . \"int main() {}\") (\"src/util/helper\" . \"void help() {}\") (\"src/lib/io\" . \"FILE* open()\") (\"src/lib/math\" . \"double sqrt()\") (\"doc/readme\" . \"Hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Towers of Hanoi with move tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_advanced_towers_of_hanoi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (defvar neovm--radv-hanoi-moves nil)
      (fset 'neovm--radv-hanoi
        (lambda (n from to aux)
          (when (> n 0)
            (funcall 'neovm--radv-hanoi (1- n) from aux to)
            (setq neovm--radv-hanoi-moves
                  (cons (list n from to) neovm--radv-hanoi-moves))
            (funcall 'neovm--radv-hanoi (1- n) aux to from))))
      (unwind-protect
          (list
           ;; 1 disk
           (progn
             (setq neovm--radv-hanoi-moves nil)
             (funcall 'neovm--radv-hanoi 1 'A 'C 'B)
             (nreverse neovm--radv-hanoi-moves))
           ;; 2 disks
           (progn
             (setq neovm--radv-hanoi-moves nil)
             (funcall 'neovm--radv-hanoi 2 'A 'C 'B)
             (nreverse neovm--radv-hanoi-moves))
           ;; 3 disks
           (progn
             (setq neovm--radv-hanoi-moves nil)
             (funcall 'neovm--radv-hanoi 3 'A 'C 'B)
             (nreverse neovm--radv-hanoi-moves))
           ;; 4 disks: just the count
           (progn
             (setq neovm--radv-hanoi-moves nil)
             (funcall 'neovm--radv-hanoi 4 'A 'C 'B)
             (length neovm--radv-hanoi-moves)))
        (fmakunbound 'neovm--radv-hanoi)
        (makunbound 'neovm--radv-hanoi-moves)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 A C)) ((1 A B) (2 A C) (1 B C)) ((1 A C) (2 A B) (1 C B) (3 A C) (1 B A) (2 B C) (1 A C)) 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
