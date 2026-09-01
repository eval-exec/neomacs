//! Oracle parity tests for constraint logic programming in Elisp.
//!
//! Covers finite domain variables, arithmetic constraints, all-different,
//! constraint propagation with arc consistency, SEND+MORE=MONEY puzzle,
//! and a simplified Einstein's zebra puzzle.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Finite domain variables with propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_finite_domains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement finite domain variables: create with a range, constrain,
    // and propagate to narrow domains
    let form = r#"
(progn
  (fset 'neovm--cl-fd-create
    (lambda (name lo hi)
      "Create a finite domain variable with values from LO to HI inclusive."
      (let ((domain nil) (i lo))
        (while (<= i hi)
          (setq domain (cons i domain))
          (setq i (1+ i)))
        (list :name name :domain (nreverse domain)))))

  (fset 'neovm--cl-fd-domain
    (lambda (var)
      (plist-get var :domain)))

  (fset 'neovm--cl-fd-assigned-p
    (lambda (var)
      (= (length (plist-get var :domain)) 1)))

  (fset 'neovm--cl-fd-value
    (lambda (var)
      (when (funcall 'neovm--cl-fd-assigned-p var)
        (car (plist-get var :domain)))))

  (fset 'neovm--cl-fd-constrain
    (lambda (var pred)
      "Remove values from domain that don't satisfy PRED."
      (let ((new-domain nil))
        (dolist (v (plist-get var :domain))
          (when (funcall pred v)
            (setq new-domain (cons v new-domain))))
        (plist-put var :domain (nreverse new-domain))
        var)))

  (fset 'neovm--cl-fd-intersect
    (lambda (var values)
      "Restrict domain to intersection with VALUES."
      (let ((new-domain nil))
        (dolist (v (plist-get var :domain))
          (when (memq v values)
            (setq new-domain (cons v new-domain))))
        (plist-put var :domain (nreverse new-domain))
        var)))

  (unwind-protect
      (let ((x (funcall 'neovm--cl-fd-create 'x 1 10))
            (y (funcall 'neovm--cl-fd-create 'y 1 10)))
        ;; Constrain x to be even
        (funcall 'neovm--cl-fd-constrain x (lambda (v) (= (% v 2) 0)))
        ;; Constrain y to be > 5
        (funcall 'neovm--cl-fd-constrain y (lambda (v) (> v 5)))
        ;; Intersect x with {2,4,6}
        (funcall 'neovm--cl-fd-intersect x '(2 4 6))
        (list
         ;; x domain: {2, 4, 6}
         (funcall 'neovm--cl-fd-domain x)
         ;; y domain: {6, 7, 8, 9, 10}
         (funcall 'neovm--cl-fd-domain y)
         ;; Not yet assigned
         (funcall 'neovm--cl-fd-assigned-p x)
         ;; Constrain x to be < 5
         (progn (funcall 'neovm--cl-fd-constrain x (lambda (v) (< v 5))) nil)
         ;; x domain: {2, 4}
         (funcall 'neovm--cl-fd-domain x)
         ;; Constrain x to be 4
         (progn (funcall 'neovm--cl-fd-constrain x (lambda (v) (= v 4))) nil)
         ;; Now assigned
         (funcall 'neovm--cl-fd-assigned-p x)
         (funcall 'neovm--cl-fd-value x)))
    (fmakunbound 'neovm--cl-fd-create)
    (fmakunbound 'neovm--cl-fd-domain)
    (fmakunbound 'neovm--cl-fd-assigned-p)
    (fmakunbound 'neovm--cl-fd-value)
    (fmakunbound 'neovm--cl-fd-constrain)
    (fmakunbound 'neovm--cl-fd-intersect)))
"#;
    let expect = expect_test::expect![[r#""OK ((2 4 6) (6 7 8 9 10) nil nil (2 4) nil t 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Arithmetic constraints: =, <, >, <=, >=
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_arithmetic_constraints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build and solve systems of arithmetic constraints over finite domains
    let form = r#"
(progn
  (fset 'neovm--cl-arith-solve
    (lambda (vars constraints)
      "Solve a constraint system by backtracking.
       VARS: alist of (name . domain-list).
       CONSTRAINTS: list of (lambda (assignment) -> bool).
       Returns first solution as alist or nil."
      (let ((result nil))
        (fset 'neovm--cl-arith-bt
          (lambda (remaining assignment)
            (if (null remaining)
                ;; All assigned: check all constraints
                (let ((ok t))
                  (dolist (c constraints)
                    (when ok
                      (unless (funcall c assignment)
                        (setq ok nil))))
                  (when ok
                    (setq result (copy-sequence assignment))
                    t))
              (let ((var-name (caar remaining))
                    (var-domain (cdar remaining))
                    (found nil))
                (dolist (val var-domain)
                  (unless found
                    (let ((new-assign (cons (cons var-name val) assignment)))
                      ;; Early pruning: check constraints involving only assigned vars
                      (let ((ok t))
                        (dolist (c constraints)
                          (when ok
                            ;; Only check if all vars in constraint are assigned
                            (unless (funcall c new-assign)
                              (setq ok nil))))
                        (when ok
                          (when (funcall 'neovm--cl-arith-bt (cdr remaining) new-assign)
                            (setq found t)))))))
                found))))
        (funcall 'neovm--cl-arith-bt vars nil)
        result)))

  (fset 'neovm--cl-arith-get
    (lambda (name assignment)
      "Get value of NAME from assignment, or nil if not assigned."
      (cdr (assq name assignment))))

  (unwind-protect
      ;; Problem: find x, y, z in {1..5} such that
      ;; x < y, y < z, x + y + z = 9, x * y = z
      (let* ((domain '(1 2 3 4 5))
             (vars (list (cons 'x domain) (cons 'y domain) (cons 'z domain)))
             (constraints
              (list
               ;; x < y (skip if either not assigned)
               (lambda (a)
                 (let ((x (cdr (assq 'x a))) (y (cdr (assq 'y a))))
                   (if (and x y) (< x y) t)))
               ;; y < z
               (lambda (a)
                 (let ((y (cdr (assq 'y a))) (z (cdr (assq 'z a))))
                   (if (and y z) (< y z) t)))
               ;; x + y + z = 9
               (lambda (a)
                 (let ((x (cdr (assq 'x a)))
                       (y (cdr (assq 'y a)))
                       (z (cdr (assq 'z a))))
                   (if (and x y z) (= (+ x y z) 9) t)))
               ;; x * y = z
               (lambda (a)
                 (let ((x (cdr (assq 'x a)))
                       (y (cdr (assq 'y a)))
                       (z (cdr (assq 'z a))))
                   (if (and x y z) (= (* x y) z) t)))))
             (solution (funcall 'neovm--cl-arith-solve vars constraints)))
        (list
         ;; Found a solution
         (not (null solution))
         ;; Verify constraints
         (when solution
           (let ((x (cdr (assq 'x solution)))
                 (y (cdr (assq 'y solution)))
                 (z (cdr (assq 'z solution))))
             (list
              x y z
              (< x y)
              (< y z)
              (= (+ x y z) 9)
              (= (* x y) z))))))
    (fmakunbound 'neovm--cl-arith-solve)
    (fmakunbound 'neovm--cl-arith-bt)
    (fmakunbound 'neovm--cl-arith-get)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All-different constraint with propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_all_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement all-different constraint and use it to solve
    // a permutation problem with additional constraints
    let form = r#"
(progn
  (fset 'neovm--cl-ad-all-different-p
    (lambda (values)
      "Check that all non-nil values are distinct."
      (let ((seen nil) (ok t))
        (dolist (v values)
          (when (and ok v)
            (if (memq v seen)
                (setq ok nil)
              (setq seen (cons v seen)))))
        ok)))

  (fset 'neovm--cl-ad-solve-permutation
    (lambda (n extra-constraints)
      "Find a permutation of 1..N satisfying EXTRA-CONSTRAINTS.
       Returns assignment as list of N values."
      (let ((domain nil) (result nil))
        ;; Build domain {1..n}
        (let ((i 1))
          (while (<= i n)
            (setq domain (cons i domain))
            (setq i (1+ i))))
        (setq domain (nreverse domain))

        (fset 'neovm--cl-ad-bt2
          (lambda (pos current)
            (if (= pos n)
                ;; Check extra constraints
                (let ((ok t))
                  (dolist (c extra-constraints)
                    (when ok (unless (funcall c current) (setq ok nil))))
                  (when ok
                    (setq result (copy-sequence current))
                    t))
              (let ((found nil))
                (dolist (val domain)
                  (unless found
                    ;; Check all-different so far
                    (unless (memq val current)
                      (let ((new-current (append current (list val))))
                        (when (funcall 'neovm--cl-ad-bt2 (1+ pos) new-current)
                          (setq found t))))))
                found))))
        (funcall 'neovm--cl-ad-bt2 0 nil)
        result)))

  (unwind-protect
      ;; Find a permutation of {1,2,3,4} where:
      ;; - position 0 value + position 1 value = 5
      ;; - position 2 value > position 3 value
      (let* ((constraints
              (list
               ;; pos0 + pos1 = 5
               (lambda (perm) (= (+ (nth 0 perm) (nth 1 perm)) 5))
               ;; pos2 > pos3
               (lambda (perm) (> (nth 2 perm) (nth 3 perm)))))
             (solution (funcall 'neovm--cl-ad-solve-permutation 4 constraints)))
        (list
         ;; Found solution
         (not (null solution))
         ;; Is a permutation of {1,2,3,4}
         (when solution
           (equal (sort (copy-sequence solution) #'<) '(1 2 3 4)))
         ;; Verify constraints
         (when solution
           (list
            (= (+ (nth 0 solution) (nth 1 solution)) 5)
            (> (nth 2 solution) (nth 3 solution))))
         ;; All different
         (when solution
           (funcall 'neovm--cl-ad-all-different-p solution))
         ;; The actual solution
         solution
         ;; Count all valid permutations (should be more than 1)
         ;; Test with simpler constraint: just all-different (= n! permutations)
         (let ((count 0))
           (fset 'neovm--cl-ad-count-perms
             (lambda (n remaining current)
               (if (= (length current) n)
                   (setq count (1+ count))
                 (dolist (val remaining)
                   (funcall 'neovm--cl-ad-count-perms
                            n
                            (delq val (copy-sequence remaining))
                            (cons val current))))))
           (funcall 'neovm--cl-ad-count-perms 4 '(1 2 3 4) nil)
           count)))  ;; should be 24
    (fmakunbound 'neovm--cl-ad-all-different-p)
    (fmakunbound 'neovm--cl-ad-solve-permutation)
    (fmakunbound 'neovm--cl-ad-bt2)
    (fmakunbound 'neovm--cl-ad-count-perms)))
"#;
    let expect = expect_test::expect![[r#""OK (t t (t t) t (1 4 3 2) 24)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint propagation with arc consistency (AC-3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_arc_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement AC-3 algorithm for binary constraints
    let form = r#"
(progn
  (fset 'neovm--cl-ac3-make-csp
    (lambda (var-domains constraints)
      "Create a CSP. VAR-DOMAINS: hash var->domain-list.
       CONSTRAINTS: list of (var1 var2 . pred) where pred(v1,v2)->bool."
      (list :domains var-domains :constraints constraints)))

  (fset 'neovm--cl-ac3-revise
    (lambda (domains xi xj pred)
      "Revise domain of XI w.r.t. XJ using PRED.
       Returns t if domain changed."
      (let ((di (gethash xi domains))
            (dj (gethash xj domains))
            (new-di nil)
            (revised nil))
        (dolist (vi di)
          (let ((has-support nil))
            (dolist (vj dj)
              (when (funcall pred vi vj)
                (setq has-support t)))
            (if has-support
                (setq new-di (cons vi new-di))
              (setq revised t))))
        (when revised
          (puthash xi (nreverse new-di) domains))
        revised)))

  (fset 'neovm--cl-ac3-propagate
    (lambda (csp)
      "Run AC-3 on CSP. Returns t if consistent."
      (let* ((domains (plist-get csp :domains))
             (constraints (plist-get csp :constraints))
             ;; Build queue of arcs (both directions)
             (queue nil))
        (dolist (c constraints)
          (let ((v1 (car c)) (v2 (cadr c)) (pred (cddr c)))
            (setq queue (cons (list v1 v2 pred) queue))
            (setq queue (cons (list v2 v1 (lambda (a b) (funcall pred b a))) queue))))
        ;; Process queue
        (while queue
          (let* ((arc (car queue))
                 (xi (nth 0 arc))
                 (xj (nth 1 arc))
                 (pred (nth 2 arc)))
            (setq queue (cdr queue))
            (when (funcall 'neovm--cl-ac3-revise domains xi xj pred)
              (when (null (gethash xi domains))
                (setq queue nil))  ;; Failure
              ;; Re-enqueue arcs to xi
              (dolist (c constraints)
                (let ((v1 (car c)) (v2 (cadr c)) (p (cddr c)))
                  (when (and (eq v2 xi) (not (eq v1 xj)))
                    (setq queue (cons (list v1 v2 p) queue)))
                  (when (and (eq v1 xi) (not (eq v2 xj)))
                    (setq queue (cons (list v2 v1
                                            (lambda (a b) (funcall p b a)))
                                      queue))))))))
        ;; Check no empty domains
        (let ((ok t))
          (maphash (lambda (k v) (when (null v) (setq ok nil))) domains)
          ok))))

  (unwind-protect
      ;; Problem: x, y, z in {1..5}, x != y, y != z, x != z, x + y > z
      (let ((domains (make-hash-table)))
        (puthash 'x '(1 2 3 4 5) domains)
        (puthash 'y '(1 2 3 4 5) domains)
        (puthash 'z '(1 2 3 4 5) domains)
        (let* ((neq (lambda (a b) (/= a b)))
               (constraints (list
                             (cons 'x (cons 'y neq))
                             (cons 'y (cons 'z neq))
                             (cons 'x (cons 'z neq))))
               (csp (funcall 'neovm--cl-ac3-make-csp domains constraints))
               (consistent (funcall 'neovm--cl-ac3-propagate csp)))
          (list
           consistent
           ;; After AC-3 with != constraints, domains stay same size (full support)
           (sort (copy-sequence (gethash 'x domains)) #'<)
           (sort (copy-sequence (gethash 'y domains)) #'<)
           (sort (copy-sequence (gethash 'z domains)) #'<)
           ;; Now add a tighter constraint: x < y, y < z
           ;; Start fresh
           (progn
             (puthash 'x '(1 2 3 4 5) domains)
             (puthash 'y '(1 2 3 4 5) domains)
             (puthash 'z '(1 2 3 4 5) domains)
             (let* ((lt (lambda (a b) (< a b)))
                    (csp2 (funcall 'neovm--cl-ac3-make-csp
                                    domains
                                    (list (cons 'x (cons 'y lt))
                                          (cons 'y (cons 'z lt))))))
               (funcall 'neovm--cl-ac3-propagate csp2)
               (list
                ;; x can't be 4 or 5 (need y > x and z > y)
                (sort (copy-sequence (gethash 'x domains)) #'<)
                ;; y can't be 1 or 5
                (sort (copy-sequence (gethash 'y domains)) #'<)
                ;; z can't be 1 or 2
                (sort (copy-sequence (gethash 'z domains)) #'<)))))))
    (fmakunbound 'neovm--cl-ac3-make-csp)
    (fmakunbound 'neovm--cl-ac3-revise)
    (fmakunbound 'neovm--cl-ac3-propagate)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t (1 2 3 4 5) (1 2 3 4 5) (1 2 3 4 5) ((1 2 3) (2 3 4) (3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SEND + MORE = MONEY puzzle (smaller variant: AB + CD = EF)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_send_more_money() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a cryptarithmetic puzzle using constraint logic:
    // AB + CD = EFG where A,B,C,D,E,F,G are distinct digits,
    // A >= 1, C >= 1, E >= 1
    // Actually solve a manageable variant: AB + CD = EF
    // (10A+B) + (10C+D) = (10E+F)
    let form = r#"
(progn
  (fset 'neovm--cl-money-solve
    (lambda ()
      "Solve AB + CD = EF with all distinct digits, A>=1, C>=1, E>=1."
      (let ((solutions nil))
        ;; Enumerate with constraints for speed
        (let ((a 1))
          (while (<= a 9)
            (let ((c 1))
              (while (<= c 9)
                (when (/= c a)
                  (let ((b 0))
                    (while (<= b 9)
                      (when (and (/= b a) (/= b c))
                        (let ((d 0))
                          (while (<= d 9)
                            (when (and (/= d a) (/= d b) (/= d c))
                              (let* ((ab (+ (* 10 a) b))
                                     (cd (+ (* 10 c) d))
                                     (ef (+ ab cd)))
                                (when (and (>= ef 10) (< ef 100))
                                  (let ((e (/ ef 10))
                                        (f (% ef 10)))
                                    (when (and (>= e 1)
                                               (/= e a) (/= e b) (/= e c) (/= e d)
                                               (/= f a) (/= f b) (/= f c) (/= f d)
                                               (/= f e))
                                      (setq solutions
                                            (cons (list a b c d e f ab cd ef)
                                                  solutions)))))))
                            (setq d (1+ d)))))
                      (setq b (1+ b)))))
                (setq c (1+ c))))
            (setq a (1+ a))))
        (nreverse solutions))))

  (unwind-protect
      (let ((solutions (funcall 'neovm--cl-money-solve)))
        (list
         ;; Number of solutions
         (length solutions)
         ;; All solutions satisfy AB + CD = EF
         (let ((ok t))
           (dolist (s solutions)
             (let ((ab (nth 6 s)) (cd (nth 7 s)) (ef (nth 8 s)))
               (unless (= (+ ab cd) ef)
                 (setq ok nil))))
           ok)
         ;; All digits distinct in each solution
         (let ((ok t))
           (dolist (s solutions)
             (let ((digits (list (nth 0 s) (nth 1 s) (nth 2 s)
                                 (nth 3 s) (nth 4 s) (nth 5 s))))
               (unless (= (length digits)
                          (length (delete-dups (copy-sequence digits))))
                 (setq ok nil))))
           ok)
         ;; First 5 solutions (sorted)
         (let ((sorted (sort (copy-sequence solutions)
                             (lambda (a b) (< (nth 6 a) (nth 6 b))))))
           (let ((first5 nil) (count 0))
             (dolist (s sorted)
               (when (< count 5)
                 (setq first5 (cons (list (nth 6 s) (nth 7 s) (nth 8 s)) first5))
                 (setq count (1+ count))))
             (nreverse first5)))
         ;; Minimum and maximum EF values
         (let ((min-ef 999) (max-ef 0))
           (dolist (s solutions)
             (let ((ef (nth 8 s)))
               (when (< ef min-ef) (setq min-ef ef))
               (when (> ef max-ef) (setq max-ef ef))))
           (list min-ef max-ef))))
    (fmakunbound 'neovm--cl-money-solve)))
"#;
    let expect = expect_test::expect![[
        r#""OK (476 t t ((12 35 47) (12 36 48) (12 37 49) (12 38 50) (12 46 58)) (39 98))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simplified Einstein's zebra puzzle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_zebra_puzzle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified 3-house zebra puzzle:
    // 3 houses in a row (positions 1, 2, 3), each with:
    //   - Color: red, blue, green
    //   - Pet: cat, dog, fish
    //   - Drink: tea, coffee, milk
    // Clues:
    //   1. The red house owner drinks tea
    //   2. The dog owner lives in the blue house
    //   3. The green house is to the right of the red house
    //   4. The coffee drinker has a cat
    //   5. The middle house owner drinks milk
    // Question: Who owns the fish?
    let form = r#"
(progn
  (fset 'neovm--cl-zebra-solve
    (lambda ()
      "Solve the 3-house zebra puzzle by enumeration."
      (let ((solutions nil)
            (colors '(red blue green))
            (pets '(cat dog fish))
            (drinks '(tea coffee milk)))
        ;; Generate all permutations of 3 elements
        (fset 'neovm--cl-zebra-perms
          (lambda (lst)
            (if (null lst) '(nil)
              (let ((result nil))
                (dolist (x lst)
                  (dolist (rest (funcall 'neovm--cl-zebra-perms
                                         (delq x (copy-sequence lst))))
                    (setq result (cons (cons x rest) result))))
                result))))
        ;; Try all combinations
        (dolist (cp (funcall 'neovm--cl-zebra-perms colors))
          (dolist (pp (funcall 'neovm--cl-zebra-perms pets))
            (dolist (dp (funcall 'neovm--cl-zebra-perms drinks))
              ;; cp, pp, dp are lists: (house1-val house2-val house3-val)
              (let ((ok t))
                ;; Clue 1: red house owner drinks tea
                (let ((i 0))
                  (while (and ok (< i 3))
                    (when (eq (nth i cp) 'red)
                      (unless (eq (nth i dp) 'tea)
                        (setq ok nil)))
                    (setq i (1+ i))))
                ;; Clue 2: dog owner lives in blue house
                (let ((i 0))
                  (while (and ok (< i 3))
                    (when (eq (nth i pp) 'dog)
                      (unless (eq (nth i cp) 'blue)
                        (setq ok nil)))
                    (setq i (1+ i))))
                ;; Clue 3: green house is to the right of red house
                (when ok
                  (let ((red-pos nil) (green-pos nil) (i 0))
                    (while (< i 3)
                      (when (eq (nth i cp) 'red) (setq red-pos i))
                      (when (eq (nth i cp) 'green) (setq green-pos i))
                      (setq i (1+ i)))
                    (unless (and red-pos green-pos (= green-pos (1+ red-pos)))
                      (setq ok nil))))
                ;; Clue 4: coffee drinker has a cat
                (let ((i 0))
                  (while (and ok (< i 3))
                    (when (eq (nth i dp) 'coffee)
                      (unless (eq (nth i pp) 'cat)
                        (setq ok nil)))
                    (setq i (1+ i))))
                ;; Clue 5: middle house owner drinks milk
                (when ok
                  (unless (eq (nth 1 dp) 'milk)
                    (setq ok nil)))
                (when ok
                  (setq solutions
                        (cons (list :colors cp :pets pp :drinks dp)
                              solutions)))))))
        solutions)))

  (unwind-protect
      (let ((solutions (funcall 'neovm--cl-zebra-solve)))
        (list
         ;; Number of solutions (should be exactly 1 for well-formed puzzle)
         (length solutions)
         ;; The solution
         (when solutions
           (let ((sol (car solutions)))
             (list
              (plist-get sol :colors)
              (plist-get sol :pets)
              (plist-get sol :drinks))))
         ;; Who owns the fish?
         (when solutions
           (let* ((sol (car solutions))
                  (pets (plist-get sol :pets))
                  (colors (plist-get sol :colors))
                  (fish-pos nil)
                  (i 0))
             (while (< i 3)
               (when (eq (nth i pets) 'fish) (setq fish-pos i))
               (setq i (1+ i)))
             (when fish-pos
               (list :house (1+ fish-pos)
                     :color (nth fish-pos colors)))))
         ;; Verify all clues
         (when solutions
           (let* ((sol (car solutions))
                  (cp (plist-get sol :colors))
                  (pp (plist-get sol :pets))
                  (dp (plist-get sol :drinks)))
             (list
              ;; Clue 1: red->tea
              (let ((ok nil) (i 0))
                (while (< i 3)
                  (when (and (eq (nth i cp) 'red) (eq (nth i dp) 'tea))
                    (setq ok t))
                  (setq i (1+ i)))
                ok)
              ;; Clue 2: dog->blue
              (let ((ok nil) (i 0))
                (while (< i 3)
                  (when (and (eq (nth i pp) 'dog) (eq (nth i cp) 'blue))
                    (setq ok t))
                  (setq i (1+ i)))
                ok)
              ;; Clue 5: middle=milk
              (eq (nth 1 dp) 'milk))))))
    (fmakunbound 'neovm--cl-zebra-solve)
    (fmakunbound 'neovm--cl-zebra-perms)))
"#;
    let expect = expect_test::expect![[r#""OK (0 nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint logic: magic square solver
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_logic_magic_square() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find a 3x3 magic square using digits 1-9 (each used once)
    // where all rows, columns, and diagonals sum to 15
    let form = r#"
(progn
  (fset 'neovm--cl-magic-solve
    (lambda ()
      "Find all 3x3 magic squares."
      (let ((solutions nil))
        ;; The magic constant for 3x3 with 1-9 is 15
        ;; Enumerate with pruning
        (fset 'neovm--cl-magic-bt
          (lambda (pos grid used)
            (if (= pos 9)
                ;; Check diagonals
                (when (and (= (+ (nth 0 grid) (nth 4 grid) (nth 8 grid)) 15)
                           (= (+ (nth 2 grid) (nth 4 grid) (nth 6 grid)) 15))
                  (setq solutions (cons (copy-sequence grid) solutions)))
              (let ((d 1))
                (while (<= d 9)
                  (unless (memq d used)
                    (let ((new-grid (append grid (list d)))
                          (ok t))
                      ;; Prune: check completed rows
                      (cond
                       ((= pos 2)  ;; End of row 0
                        (unless (= (+ (nth 0 new-grid) (nth 1 new-grid) (nth 2 new-grid)) 15)
                          (setq ok nil)))
                       ((= pos 5)  ;; End of row 1
                        (unless (= (+ (nth 3 new-grid) (nth 4 new-grid) (nth 5 new-grid)) 15)
                          (setq ok nil)))
                       ((= pos 8)  ;; End of row 2
                        (unless (= (+ (nth 6 new-grid) (nth 7 new-grid) (nth 8 new-grid)) 15)
                          (setq ok nil))))
                      ;; Check completed columns at row 2
                      (when (and ok (>= pos 6))
                        (let ((col (- pos 6)))
                          (unless (= (+ (nth col new-grid)
                                        (nth (+ col 3) new-grid)
                                        (nth (+ col 6) new-grid))
                                     15)
                            (setq ok nil))))
                      (when ok
                        (funcall 'neovm--cl-magic-bt (1+ pos) new-grid (cons d used)))))
                  (setq d (1+ d)))))))
        (funcall 'neovm--cl-magic-bt 0 nil nil)
        solutions)))

  (unwind-protect
      (let ((solutions (funcall 'neovm--cl-magic-solve)))
        (list
         ;; Number of solutions (should be 8: 1 essentially different + rotations/reflections)
         (length solutions)
         ;; Verify first solution
         (when solutions
           (let ((s (car solutions)))
             (list
              ;; All rows sum to 15
              (= (+ (nth 0 s) (nth 1 s) (nth 2 s)) 15)
              (= (+ (nth 3 s) (nth 4 s) (nth 5 s)) 15)
              (= (+ (nth 6 s) (nth 7 s) (nth 8 s)) 15)
              ;; All columns sum to 15
              (= (+ (nth 0 s) (nth 3 s) (nth 6 s)) 15)
              (= (+ (nth 1 s) (nth 4 s) (nth 7 s)) 15)
              (= (+ (nth 2 s) (nth 5 s) (nth 8 s)) 15)
              ;; Diagonals
              (= (+ (nth 0 s) (nth 4 s) (nth 8 s)) 15)
              (= (+ (nth 2 s) (nth 4 s) (nth 6 s)) 15)
              ;; Uses 1-9 exactly once
              (equal (sort (copy-sequence s) #'<) '(1 2 3 4 5 6 7 8 9)))))
         ;; Center element is always 5 in all solutions
         (let ((ok t))
           (dolist (s solutions)
             (unless (= (nth 4 s) 5) (setq ok nil)))
           ok)))
    (fmakunbound 'neovm--cl-magic-solve)
    (fmakunbound 'neovm--cl-magic-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (8 (t t t t t t t t t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
