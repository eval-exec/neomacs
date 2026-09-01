//! Complex combination oracle parity tests: LR(0)/SLR parser in Elisp.
//! Implements grammar representation, LR(0) item sets and closure,
//! GOTO function, parse table construction (shift/reduce/accept),
//! shift-reduce parsing, and expression parsing with precedence.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// LR(0) item sets: closure computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LR(0) items: (rule-index . dot-position)
    // Grammar augmented: S' -> S, S -> "a" S "b" | "a" "b"
    // Compute closure of initial item set {[S' -> . S]}
    let form = r#"(progn
  ;; Grammar: list of (LHS . RHS) where terminals are strings, nonterminals are symbols
  ;; Item: (rule-index . dot-position)

  (fset 'neovm--lr-item-next-sym
    (lambda (grammar item)
      (let* ((rule (nth (car item) grammar))
             (rhs (cdr rule))
             (dot (cdr item)))
        (nth dot rhs))))

  (fset 'neovm--lr-item-complete-p
    (lambda (grammar item)
      (let* ((rule (nth (car item) grammar))
             (rhs (cdr rule)))
        (>= (cdr item) (length rhs)))))

  (fset 'neovm--lr-item-advance
    (lambda (item)
      (cons (car item) (1+ (cdr item)))))

  ;; Compute closure of an item set
  (fset 'neovm--lr-closure
    (lambda (grammar items)
      (let ((result (copy-sequence items))
            (changed t))
        (while changed
          (setq changed nil)
          (dolist (item result)
            (let ((sym (funcall 'neovm--lr-item-next-sym grammar item)))
              (when (and sym (symbolp sym))
                ;; Add all rules with sym as LHS
                (let ((ri 0))
                  (while (< ri (length grammar))
                    (when (eq (car (nth ri grammar)) sym)
                      (let ((new-item (cons ri 0)))
                        (unless (member new-item result)
                          (push new-item result)
                          (setq changed t))))
                    (setq ri (1+ ri))))))))
        ;; Sort for deterministic output
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b)) (< (cdr a) (cdr b)))))))))

  (unwind-protect
      (let* (;; Augmented grammar:
             ;; 0: Sp -> S
             ;; 1: S -> "a" S "b"
             ;; 2: S -> "a" "b"
             (grammar '((Sp S)
                        (S "a" S "b")
                        (S "a" "b"))))
        (list
         ;; Closure of {[Sp -> . S]}: should include S -> . "a" S "b" and S -> . "a" "b"
         (funcall 'neovm--lr-closure grammar (list '(0 . 0)))
         ;; Closure of {[S -> "a" . S "b"]}: kernel + S predictions
         (funcall 'neovm--lr-closure grammar (list '(1 . 1)))
         ;; Closure of completed item: no additions
         (funcall 'neovm--lr-closure grammar (list '(2 . 2)))
         ;; Closure with multiple kernel items
         (funcall 'neovm--lr-closure grammar (list '(1 . 1) '(2 . 1)))))
    (fmakunbound 'neovm--lr-item-next-sym)
    (fmakunbound 'neovm--lr-item-complete-p)
    (fmakunbound 'neovm--lr-item-advance)
    (fmakunbound 'neovm--lr-closure)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 0) (1 . 0) (2 . 0)) ((1 . 0) (1 . 1) (2 . 0)) ((2 . 2)) ((1 . 0) (1 . 1) (2 . 0) (2 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GOTO function: compute successor state for a symbol
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lr2-next-sym
    (lambda (grammar item)
      (let* ((rule (nth (car item) grammar))
             (rhs (cdr rule)))
        (nth (cdr item) rhs))))

  (fset 'neovm--lr2-advance
    (lambda (item)
      (cons (car item) (1+ (cdr item)))))

  (fset 'neovm--lr2-closure
    (lambda (grammar items)
      (let ((result (copy-sequence items)) (changed t))
        (while changed
          (setq changed nil)
          (dolist (item result)
            (let ((sym (funcall 'neovm--lr2-next-sym grammar item)))
              (when (and sym (symbolp sym))
                (let ((ri 0))
                  (while (< ri (length grammar))
                    (when (eq (car (nth ri grammar)) sym)
                      (let ((new-item (cons ri 0)))
                        (unless (member new-item result)
                          (push new-item result)
                          (setq changed t))))
                    (setq ri (1+ ri))))))))
        (sort result (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b)) (< (cdr a) (cdr b)))))))))

  ;; GOTO(I, X): advance all items in I that have X after the dot, then closure
  (fset 'neovm--lr2-goto
    (lambda (grammar items sym)
      (let ((kernel nil))
        (dolist (item items)
          (let ((next (funcall 'neovm--lr2-next-sym grammar item)))
            (when (cond
                   ((and (stringp sym) (stringp next)) (string= sym next))
                   ((and (symbolp sym) (symbolp next)) (eq sym next))
                   (t nil))
              (push (funcall 'neovm--lr2-advance item) kernel))))
        (if kernel
            (funcall 'neovm--lr2-closure grammar kernel)
          nil))))

  (unwind-protect
      (let* ((grammar '((Sp S)
                         (S "a" S "b")
                         (S "a" "b")))
             (I0 (funcall 'neovm--lr2-closure grammar (list '(0 . 0)))))
        (list
         ;; I0 state
         I0
         ;; GOTO(I0, S) = items after seeing S
         (funcall 'neovm--lr2-goto grammar I0 'S)
         ;; GOTO(I0, "a") = items after seeing "a"
         (funcall 'neovm--lr2-goto grammar I0 "a")
         ;; GOTO(I0, "b") = nil (no item has "b" after dot in I0)
         (funcall 'neovm--lr2-goto grammar I0 "b")
         ;; Chain: GOTO(GOTO(I0, "a"), "b")
         (let ((I2 (funcall 'neovm--lr2-goto grammar I0 "a")))
           (funcall 'neovm--lr2-goto grammar I2 "b"))
         ;; Chain: GOTO(GOTO(I0, "a"), S)
         (let ((I2 (funcall 'neovm--lr2-goto grammar I0 "a")))
           (funcall 'neovm--lr2-goto grammar I2 'S))))
    (fmakunbound 'neovm--lr2-next-sym)
    (fmakunbound 'neovm--lr2-advance)
    (fmakunbound 'neovm--lr2-closure)
    (fmakunbound 'neovm--lr2-goto)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 0) (1 . 0) (2 . 0)) ((0 . 1)) ((1 . 0) (1 . 1) (2 . 0) (2 . 1)) nil ((2 . 2)) ((1 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Build complete LR(0) automaton (canonical collection of item sets)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_automaton() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lr3-next-sym
    (lambda (g item) (nth (cdr item) (cdr (nth (car item) g)))))
  (fset 'neovm--lr3-advance (lambda (item) (cons (car item) (1+ (cdr item)))))
  (fset 'neovm--lr3-closure
    (lambda (g items)
      (let ((result (copy-sequence items)) (changed t))
        (while changed
          (setq changed nil)
          (dolist (item result)
            (let ((sym (funcall 'neovm--lr3-next-sym g item)))
              (when (and sym (symbolp sym))
                (let ((ri 0))
                  (while (< ri (length g))
                    (when (eq (car (nth ri g)) sym)
                      (let ((ni (cons ri 0)))
                        (unless (member ni result) (push ni result) (setq changed t))))
                    (setq ri (1+ ri))))))))
        (sort result (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b)) (< (cdr a) (cdr b)))))))))
  (fset 'neovm--lr3-goto
    (lambda (g items sym)
      (let ((kernel nil))
        (dolist (item items)
          (let ((next (funcall 'neovm--lr3-next-sym g item)))
            (when (cond ((and (stringp sym) (stringp next)) (string= sym next))
                        ((and (symbolp sym) (symbolp next)) (eq sym next))
                        (t nil))
              (push (funcall 'neovm--lr3-advance item) kernel))))
        (if kernel (funcall 'neovm--lr3-closure g kernel) nil))))

  ;; Collect all symbols that appear in the grammar
  (fset 'neovm--lr3-grammar-symbols
    (lambda (g)
      (let ((syms nil))
        (dolist (rule g)
          (dolist (sym (cdr rule))
            (unless (member sym syms) (push sym syms)))
          (unless (member (car rule) syms) (push (car rule) syms)))
        syms)))

  ;; Build canonical collection of LR(0) item sets
  (fset 'neovm--lr3-build-collection
    (lambda (g start-item)
      (let* ((I0 (funcall 'neovm--lr3-closure g (list start-item)))
             (collection (list I0))
             (transitions nil)
             (changed t)
             (all-syms (funcall 'neovm--lr3-grammar-symbols g)))
        (while changed
          (setq changed nil)
          (let ((state-idx 0))
            (while (< state-idx (length collection))
              (let ((state (nth state-idx collection)))
                (dolist (sym all-syms)
                  (let ((next-state (funcall 'neovm--lr3-goto g state sym)))
                    (when next-state
                      (let ((existing-idx nil) (i 0))
                        (while (< i (length collection))
                          (when (equal (nth i collection) next-state)
                            (setq existing-idx i))
                          (setq i (1+ i)))
                        (unless existing-idx
                          (setq existing-idx (length collection))
                          (setq collection (append collection (list next-state)))
                          (setq changed t))
                        ;; Record transition
                        (let ((trans (list state-idx sym existing-idx)))
                          (unless (member trans transitions)
                            (push trans transitions))))))))
              (setq state-idx (1+ state-idx)))))
        ;; Sort transitions for deterministic output
        (setq transitions
              (sort transitions
                    (lambda (a b)
                      (or (< (car a) (car b))
                          (and (= (car a) (car b))
                               (string< (format "%s" (nth 1 a))
                                        (format "%s" (nth 1 b))))))))
        (list 'states (length collection)
              'transitions transitions))))

  (unwind-protect
      (let* ((grammar '((Sp S)
                         (S "a" S "b")
                         (S "a" "b")))
             (result (funcall 'neovm--lr3-build-collection grammar '(0 . 0))))
        result)
    (fmakunbound 'neovm--lr3-next-sym)
    (fmakunbound 'neovm--lr3-advance)
    (fmakunbound 'neovm--lr3-closure)
    (fmakunbound 'neovm--lr3-goto)
    (fmakunbound 'neovm--lr3-grammar-symbols)
    (fmakunbound 'neovm--lr3-build-collection)))"#;
    let expect = expect_test::expect![[
        r#""OK (states 6 transitions ((0 S 2) (0 \"a\" 1) (1 S 4) (1 \"a\" 1) (1 \"b\" 3) (4 \"b\" 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SLR parse table construction with FOLLOW sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_slr_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Shared helpers
  (fset 'neovm--lr4-next-sym
    (lambda (g item) (nth (cdr item) (cdr (nth (car item) g)))))
  (fset 'neovm--lr4-done-p
    (lambda (g item) (>= (cdr item) (length (cdr (nth (car item) g))))))
  (fset 'neovm--lr4-advance (lambda (item) (cons (car item) (1+ (cdr item)))))
  (fset 'neovm--lr4-closure
    (lambda (g items)
      (let ((result (copy-sequence items)) (changed t))
        (while changed
          (setq changed nil)
          (dolist (item result)
            (let ((sym (funcall 'neovm--lr4-next-sym g item)))
              (when (and sym (symbolp sym))
                (let ((ri 0))
                  (while (< ri (length g))
                    (when (eq (car (nth ri g)) sym)
                      (let ((ni (cons ri 0)))
                        (unless (member ni result) (push ni result) (setq changed t))))
                    (setq ri (1+ ri))))))))
        (sort result (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b)) (< (cdr a) (cdr b)))))))))
  (fset 'neovm--lr4-goto
    (lambda (g items sym)
      (let ((kernel nil))
        (dolist (item items)
          (let ((next (funcall 'neovm--lr4-next-sym g item)))
            (when (cond ((and (stringp sym) (stringp next)) (string= sym next))
                        ((and (symbolp sym) (symbolp next)) (eq sym next))
                        (t nil))
              (push (funcall 'neovm--lr4-advance item) kernel))))
        (if kernel (funcall 'neovm--lr4-closure g kernel) nil))))

  ;; FIRST/FOLLOW for SLR
  (fset 'neovm--lr4-set-add
    (lambda (elem set) (if (member elem set) (cons nil set) (cons t (cons elem set)))))
  (fset 'neovm--lr4-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1) (let ((r (funcall 'neovm--lr4-set-add x result)))
                         (when (car r) (setq changed t)) (setq result (cdr r))))
        (cons changed result))))

  (fset 'neovm--lr4-compute-follow
    (lambda (g start)
      (let ((firsts (make-hash-table)) (follows (make-hash-table)) (changed t))
        ;; Compute FIRST sets (simple: only need terminals)
        (setq changed t)
        (while changed
          (setq changed nil)
          (dolist (rule g)
            (let ((nt (car rule)) (rhs (cdr rule)))
              (when rhs
                (let ((sym (car rhs)))
                  (when (stringp sym)
                    (let ((r (funcall 'neovm--lr4-set-add sym (gethash nt firsts nil))))
                      (when (car r) (setq changed t) (puthash nt (cdr r) firsts)))))))))
        ;; Initialize FOLLOW(start) = {"$"}
        (puthash start (list "$") follows)
        ;; Compute FOLLOW sets
        (setq changed t)
        (while changed
          (setq changed nil)
          (dolist (rule g)
            (let ((lhs (car rule)) (rhs (cdr rule)) (i 0))
              (while (< i (length rhs))
                (let ((sym (nth i rhs)))
                  (when (symbolp sym)
                    (let ((rest (nthcdr (1+ i) rhs)))
                      ;; If rest is empty or can derive epsilon, add FOLLOW(lhs) to FOLLOW(sym)
                      (if (null rest)
                          (let ((r (funcall 'neovm--lr4-set-union
                                            (gethash lhs follows nil)
                                            (gethash sym follows nil))))
                            (when (car r) (setq changed t) (puthash sym (cdr r) follows)))
                        ;; Add FIRST(rest) to FOLLOW(sym)
                        (let ((first-rest (car rest)))
                          (when (stringp first-rest)
                            (let ((r (funcall 'neovm--lr4-set-add first-rest
                                              (gethash sym follows nil))))
                              (when (car r) (setq changed t)
                                    (puthash sym (cdr r) follows)))))))))
                (setq i (1+ i))))))
        follows)))

  ;; Build SLR parse table
  ;; Returns list of (state terminal action) entries
  ;; action = (shift . state) | (reduce . rule-index) | accept
  (fset 'neovm--lr4-build-table
    (lambda (g start-rule)
      (let* ((I0 (funcall 'neovm--lr4-closure g (list (cons start-rule 0))))
             (collection (list I0))
             (trans-map (make-hash-table :test 'equal))
             (changed t)
             (all-syms nil))
        ;; Collect symbols
        (dolist (rule g)
          (dolist (sym (cdr rule))
            (unless (member sym all-syms) (push sym all-syms)))
          (unless (member (car rule) all-syms) (push (car rule) all-syms)))
        ;; Build collection
        (while changed
          (setq changed nil)
          (let ((si 0))
            (while (< si (length collection))
              (dolist (sym all-syms)
                (let ((ns (funcall 'neovm--lr4-goto g (nth si collection) sym)))
                  (when ns
                    (let ((ei nil) (i 0))
                      (while (< i (length collection))
                        (when (equal (nth i collection) ns) (setq ei i))
                        (setq i (1+ i)))
                      (unless ei
                        (setq ei (length collection))
                        (setq collection (append collection (list ns)))
                        (setq changed t))
                      (puthash (cons si (format "%s" sym)) ei trans-map)))))
              (setq si (1+ si)))))
        ;; Build table entries
        (let ((follows (funcall 'neovm--lr4-compute-follow g (car (nth start-rule g))))
              (table nil))
          ;; For each state
          (let ((si 0))
            (while (< si (length collection))
              (let ((state (nth si collection)))
                (dolist (item state)
                  (if (funcall 'neovm--lr4-done-p g item)
                      ;; Complete item: reduce or accept
                      (if (= (car item) start-rule)
                          (push (list si "$" 'accept) table)
                        (dolist (term (gethash (car (nth (car item) g)) follows nil))
                          (push (list si term (cons 'reduce (car item))) table)))
                    ;; Shift for terminals
                    (let ((sym (funcall 'neovm--lr4-next-sym g item)))
                      (when (stringp sym)
                        (let ((next (gethash (cons si (format "%s" sym)) trans-map)))
                          (when next
                            (let ((entry (list si sym (cons 'shift next))))
                              (unless (member entry table)
                                (push entry table))))))))))
              (setq si (1+ si))))
          ;; Sort table for deterministic output
          (sort table (lambda (a b)
                        (or (< (car a) (car b))
                            (and (= (car a) (car b))
                                 (string< (format "%s" (nth 1 a))
                                          (format "%s" (nth 1 b)))))))))))

  (unwind-protect
      (let* ((grammar '((Sp S)
                         (S "a" S "b")
                         (S "a" "b")))
             (table (funcall 'neovm--lr4-build-table grammar 0)))
        (list
         'table-entries (length table)
         'table table))
    (fmakunbound 'neovm--lr4-next-sym)
    (fmakunbound 'neovm--lr4-done-p)
    (fmakunbound 'neovm--lr4-advance)
    (fmakunbound 'neovm--lr4-closure)
    (fmakunbound 'neovm--lr4-goto)
    (fmakunbound 'neovm--lr4-set-add)
    (fmakunbound 'neovm--lr4-set-union)
    (fmakunbound 'neovm--lr4-compute-follow)
    (fmakunbound 'neovm--lr4-build-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (table-entries 9 table ((0 \"a\" (shift . 1)) (1 \"a\" (shift . 1)) (1 \"b\" (shift . 3)) (2 \"$\" accept) (3 \"$\" (reduce . 2)) (3 \"b\" (reduce . 2)) (4 \"b\" (shift . 5)) (5 \"$\" (reduce . 1)) (5 \"b\" (reduce . 1))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shift-reduce parser using the SLR table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_shift_reduce_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement the actual shift-reduce parsing algorithm
    let form = r#"(progn
  ;; Minimal grammar for balanced a/b pairs:
  ;; S -> "a" S "b" | "a" "b"
  ;; We hardcode the SLR table for this grammar to keep the test focused
  ;; on the shift-reduce mechanism rather than repeating table construction.
  ;;
  ;; States (from the automaton):
  ;; 0: initial (Sp -> .S, S -> ."a"S"b", S -> ."a""b")
  ;; 1: Sp -> S. (accept)
  ;; 2: S -> "a".S"b", S -> "a"."b" (also has S predictions)
  ;; 3: S -> "a""b". (reduce 2)
  ;; 4: S -> "a"S."b"
  ;; 5: S -> "a"S"b". (reduce 1)

  ;; action-table: hash (state . terminal) -> action
  ;; goto-table: hash (state . nonterminal) -> state
  (fset 'neovm--lr5-build-tables
    (lambda ()
      (let ((action (make-hash-table :test 'equal))
            (goto-t (make-hash-table :test 'equal)))
        ;; Shift actions
        (puthash '(0 . "a") '(shift . 2) action)
        (puthash '(2 . "a") '(shift . 2) action)
        (puthash '(2 . "b") '(shift . 3) action)
        (puthash '(4 . "b") '(shift . 5) action)
        ;; Reduce actions
        (puthash '(3 . "$") '(reduce . 2) action)  ;; S -> "a" "b" (rule 2, 2 symbols)
        (puthash '(3 . "b") '(reduce . 2) action)
        (puthash '(5 . "$") '(reduce . 1) action)  ;; S -> "a" S "b" (rule 1, 3 symbols)
        (puthash '(5 . "b") '(reduce . 1) action)
        ;; Accept
        (puthash '(1 . "$") 'accept action)
        ;; GOTO
        (puthash '(0 . S) 1 goto-t)
        (puthash '(2 . S) 4 goto-t)
        (cons action goto-t))))

  ;; rule-info: (lhs . rhs-length)
  (fset 'neovm--lr5-rule-info
    (lambda (rule-num)
      (cond
       ((= rule-num 1) '(S . 3))   ;; S -> "a" S "b"
       ((= rule-num 2) '(S . 2))   ;; S -> "a" "b"
       (t nil))))

  ;; Shift-reduce parser
  (fset 'neovm--lr5-parse
    (lambda (tokens)
      (let* ((tables (funcall 'neovm--lr5-build-tables))
             (action-table (car tables))
             (goto-table (cdr tables))
             (stack (list 0))  ;; state stack
             (input (append tokens (list "$")))
             (pos 0)
             (trace nil)
             (limit 100))
        (catch 'done
          (while (> limit 0)
            (setq limit (1- limit))
            (let* ((state (car stack))
                   (tok (nth pos input))
                   (act (gethash (cons state tok) action-table)))
              (cond
               ((null act)
                (push (list 'error 'no-action state tok pos) trace)
                (throw 'done nil))
               ((eq act 'accept)
                (push 'accept trace)
                (throw 'done nil))
               ((eq (car act) 'shift)
                (push (list 'shift tok (cdr act)) trace)
                (push (cdr act) stack)
                (setq pos (1+ pos)))
               ((eq (car act) 'reduce)
                (let* ((info (funcall 'neovm--lr5-rule-info (cdr act)))
                       (lhs (car info))
                       (rhs-len (cdr info)))
                  (push (list 'reduce (cdr act) lhs rhs-len) trace)
                  ;; Pop rhs-len states
                  (let ((i 0))
                    (while (< i rhs-len)
                      (setq stack (cdr stack))
                      (setq i (1+ i))))
                  ;; Push GOTO state
                  (let ((next (gethash (cons (car stack) lhs) goto-table)))
                    (if next
                        (push next stack)
                      (push (list 'error 'no-goto (car stack) lhs) trace)
                      (throw 'done nil)))))
               (t
                (push (list 'error 'unknown-action act) trace)
                (throw 'done nil))))))
        (nreverse trace))))

  (unwind-protect
      (list
       ;; Parse "a" "b" -> accept
       (funcall 'neovm--lr5-parse '("a" "b"))
       ;; Parse "a" "a" "b" "b" -> accept (nested)
       (funcall 'neovm--lr5-parse '("a" "a" "b" "b"))
       ;; Parse "a" "a" "a" "b" "b" "b" -> accept (deeply nested)
       (funcall 'neovm--lr5-parse '("a" "a" "a" "b" "b" "b"))
       ;; Parse "a" -> error (incomplete)
       (funcall 'neovm--lr5-parse '("a"))
       ;; Parse "b" -> error (wrong start)
       (funcall 'neovm--lr5-parse '("b"))
       ;; Parse "" -> error (empty input)
       (funcall 'neovm--lr5-parse '()))
    (fmakunbound 'neovm--lr5-build-tables)
    (fmakunbound 'neovm--lr5-rule-info)
    (fmakunbound 'neovm--lr5-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK (((shift \"a\" 2) (shift \"b\" 3) (reduce 2 S 2) accept) ((shift \"a\" 2) (shift \"a\" 2) (shift \"b\" 3) (reduce 2 S 2) (shift \"b\" 5) (reduce 1 S 3) accept) ((shift \"a\" 2) (shift \"a\" 2) (shift \"a\" 2) (shift \"b\" 3) (reduce 2 S 2) (shift \"b\" 5) (reduce 1 S 3) (shift \"b\" 5) (reduce 1 S 3) accept) ((shift \"a\" 2) (error no-action 2 \"$\" 1)) ((error no-action 0 \"b\" 0)) ((error no-action 0 \"$\" 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LR parsing of arithmetic expressions with operator precedence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_expression_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Operator-precedence parsing using a simplified scheme:
    // Build an AST from tokens using precedence climbing.
    // Grammar: E -> E "+" T | T, T -> T "*" F | F, F -> "(" E ")" | NUM
    // We use a recursive descent parser that mirrors LR precedence behavior.
    let form = r#"(progn
  ;; Token stream: ((type . value) ...)
  ;; Types: num, op, lparen, rparen, eof
  (fset 'neovm--lr6-tokenize
    (lambda (expr)
      (let ((tokens nil) (i 0) (len (length expr)))
        (while (< i len)
          (let ((ch (aref expr i)))
            (cond
             ((and (>= ch ?0) (<= ch ?9))
              (let ((num 0))
                (while (and (< i len) (>= (aref expr i) ?0) (<= (aref expr i) ?9))
                  (setq num (+ (* num 10) (- (aref expr i) ?0)))
                  (setq i (1+ i)))
                (push (cons 'num num) tokens)))
             ((= ch ?+) (push (cons 'op '+) tokens) (setq i (1+ i)))
             ((= ch ?*) (push (cons 'op '*) tokens) (setq i (1+ i)))
             ((= ch ?-) (push (cons 'op '-) tokens) (setq i (1+ i)))
             ((= ch ?\() (push '(lparen) tokens) (setq i (1+ i)))
             ((= ch ?\)) (push '(rparen) tokens) (setq i (1+ i)))
             ((= ch ? ) (setq i (1+ i)))  ;; skip space
             (t (setq i (1+ i))))))
        (setq tokens (nreverse tokens))
        (append tokens (list '(eof))))))

  ;; Precedence-climbing parser (produces postfix evaluation)
  (fset 'neovm--lr6-prec
    (lambda (op) (cond ((eq op '+) 1) ((eq op '-) 1) ((eq op '*) 2) (t 0))))

  ;; Parse and evaluate using a stack-based approach
  (fset 'neovm--lr6-eval
    (lambda (tokens)
      (let ((val-stack nil)
            (op-stack nil)
            (pos 0))
        ;; Apply top operator
        (fset 'neovm--lr6-apply-op
          (lambda ()
            (let ((op (car op-stack))
                  (b (car val-stack))
                  (a (cadr val-stack)))
              (setq op-stack (cdr op-stack))
              (setq val-stack (cddr val-stack))
              (push (cond ((eq op '+) (+ a b))
                          ((eq op '-) (- a b))
                          ((eq op '*) (* a b))
                          (t 0))
                    val-stack))))
        (while (< pos (length tokens))
          (let ((tok (nth pos tokens)))
            (cond
             ((eq (car tok) 'num)
              (push (cdr tok) val-stack)
              (setq pos (1+ pos)))
             ((eq (car tok) 'op)
              (let ((op (cdr tok)))
                ;; Pop ops with higher or equal precedence
                (while (and op-stack
                            (not (eq (car op-stack) 'lparen-marker))
                            (>= (funcall 'neovm--lr6-prec (car op-stack))
                                (funcall 'neovm--lr6-prec op)))
                  (funcall 'neovm--lr6-apply-op))
                (push op op-stack))
              (setq pos (1+ pos)))
             ((eq (car tok) 'lparen)
              (push 'lparen-marker op-stack)
              (setq pos (1+ pos)))
             ((eq (car tok) 'rparen)
              (while (and op-stack (not (eq (car op-stack) 'lparen-marker)))
                (funcall 'neovm--lr6-apply-op))
              (setq op-stack (cdr op-stack))  ;; pop lparen marker
              (setq pos (1+ pos)))
             ((eq (car tok) 'eof)
              (while op-stack
                (funcall 'neovm--lr6-apply-op))
              (setq pos (1+ pos))))))
        (car val-stack))))

  (unwind-protect
      (list
       ;; Simple: "3+4" = 7
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "3+4"))
       ;; Precedence: "3+4*5" = 23 (not 35)
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "3+4*5"))
       ;; Precedence: "3*4+5" = 17
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "3*4+5"))
       ;; Parentheses override: "(3+4)*5" = 35
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "(3+4)*5"))
       ;; Nested parens: "((2+3))*4" = 20
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "((2+3))*4"))
       ;; Complex: "1+2*3+4*5+6" = 1+6+20+6 = 33
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "1+2*3+4*5+6"))
       ;; Subtraction: "10-3-2" = 5 (left-to-right)
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "10-3-2"))
       ;; Mixed: "2*(3+4)-5" = 9
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "2*(3+4)-5"))
       ;; Single number
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "42"))
       ;; Multi-digit: "123+456" = 579
       (funcall 'neovm--lr6-eval (funcall 'neovm--lr6-tokenize "123+456")))
    (fmakunbound 'neovm--lr6-tokenize)
    (fmakunbound 'neovm--lr6-prec)
    (fmakunbound 'neovm--lr6-eval)
    (fmakunbound 'neovm--lr6-apply-op)))"#;
    let expect = expect_test::expect![[r#""OK (7 23 17 35 20 33 5 9 42 579)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LR: grammar analysis -- detect reduce/reduce and shift/reduce conflicts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lr_conflict_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze a grammar for SLR conflicts by building the item sets
    // and checking for overlapping actions.
    let form = r#"(progn
  (fset 'neovm--lr7-next-sym
    (lambda (g item) (nth (cdr item) (cdr (nth (car item) g)))))
  (fset 'neovm--lr7-done-p
    (lambda (g item) (>= (cdr item) (length (cdr (nth (car item) g))))))
  (fset 'neovm--lr7-closure
    (lambda (g items)
      (let ((result (copy-sequence items)) (changed t))
        (while changed
          (setq changed nil)
          (dolist (item result)
            (let ((sym (funcall 'neovm--lr7-next-sym g item)))
              (when (and sym (symbolp sym))
                (let ((ri 0))
                  (while (< ri (length g))
                    (when (eq (car (nth ri g)) sym)
                      (let ((ni (cons ri 0)))
                        (unless (member ni result) (push ni result) (setq changed t))))
                    (setq ri (1+ ri))))))))
        (sort result (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b)) (< (cdr a) (cdr b)))))))))
  (fset 'neovm--lr7-goto
    (lambda (g items sym)
      (let ((kernel nil))
        (dolist (item items)
          (let ((next (funcall 'neovm--lr7-next-sym g item)))
            (when (cond ((and (stringp sym) (stringp next)) (string= sym next))
                        ((and (symbolp sym) (symbolp next)) (eq sym next)) (t nil))
              (push (cons (car item) (1+ (cdr item))) kernel))))
        (if kernel (funcall 'neovm--lr7-closure g kernel) nil))))

  ;; Check a state for conflicts
  (fset 'neovm--lr7-state-conflicts
    (lambda (g state)
      (let ((has-shift nil) (reduce-count 0))
        (dolist (item state)
          (if (funcall 'neovm--lr7-done-p g item)
              (setq reduce-count (1+ reduce-count))
            (when (stringp (funcall 'neovm--lr7-next-sym g item))
              (setq has-shift t))))
        (list
         (and has-shift (> reduce-count 0))   ;; shift/reduce conflict
         (> reduce-count 1)))))               ;; reduce/reduce conflict

  (unwind-protect
      (let* (;; Non-conflicting grammar
             (g1 '((Sp S) (S "a" S "b") (S "a" "b")))
             (I0-g1 (funcall 'neovm--lr7-closure g1 (list '(0 . 0))))
             ;; Ambiguous grammar (has conflicts): S -> S "+" S | "n"
             ;; Augmented: Sp -> S, S -> S "+" S, S -> "n"
             (g2 '((Sp S) (S S "+" S) (S "n")))
             (I0-g2 (funcall 'neovm--lr7-closure g2 (list '(0 . 0))))
             ;; State after parsing "n" "+" "n" in g2
             ;; This state should have a shift/reduce conflict
             (I-after-n (funcall 'neovm--lr7-goto g2 I0-g2 "n"))
             (I-after-S (funcall 'neovm--lr7-goto g2 I0-g2 'S))
             (I-after-S-plus (if I-after-S (funcall 'neovm--lr7-goto g2 I-after-S "+") nil))
             (I-after-S-plus-n (if I-after-S-plus (funcall 'neovm--lr7-goto g2 I-after-S-plus "n") nil))
             (I-after-S-plus-S (if I-after-S-plus (funcall 'neovm--lr7-goto g2 I-after-S-plus 'S) nil)))
        (list
         ;; g1 initial state: no conflicts
         (funcall 'neovm--lr7-state-conflicts g1 I0-g1)
         ;; g2: state after S "+" S should have shift/reduce conflict
         ;; because S -> S "+" S . (reduce) but also S -> S . "+" S (shift on "+")
         (if I-after-S-plus-S
             (funcall 'neovm--lr7-state-conflicts g2 I-after-S-plus-S)
           '(unknown nil))
         ;; Number of states in each item set
         (length I0-g1)
         (length I0-g2)))
    (fmakunbound 'neovm--lr7-next-sym)
    (fmakunbound 'neovm--lr7-done-p)
    (fmakunbound 'neovm--lr7-closure)
    (fmakunbound 'neovm--lr7-goto)
    (fmakunbound 'neovm--lr7-state-conflicts)))"#;
    let expect = expect_test::expect![[r#""OK ((nil nil) (t nil) 3 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
