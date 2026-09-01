//! Oracle parity tests for an LL(1) parser implemented in Elisp.
//!
//! Implements FIRST set computation, FOLLOW set computation, LL(1) parse
//! table construction, predictive parsing algorithm, parsing of a simple
//! expression grammar, and error recovery (panic mode).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// FIRST set computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_first_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute FIRST sets for a grammar.
    // Grammar: list of (nonterminal . alternatives) where each alternative
    // is a list of symbols. Terminals are strings, nonterminals are symbols.
    // epsilon is represented by the symbol 'epsilon.
    let form = r#"(progn
  ;; Check if symbol is a terminal (string) or epsilon
  (fset 'neovm--ll1-terminal-p
    (lambda (sym) (stringp sym)))

  (fset 'neovm--ll1-epsilon-p
    (lambda (sym) (eq sym 'epsilon)))

  ;; Get alternatives for a nonterminal from grammar
  (fset 'neovm--ll1-get-alts
    (lambda (grammar nt)
      (cdr (assq nt grammar))))

  ;; Add element to a set (list without duplicates), return (changed . new-set)
  (fset 'neovm--ll1-set-add
    (lambda (elem set)
      (if (member elem set)
          (cons nil set)
        (cons t (cons elem set)))))

  ;; Union two sets, return (changed . new-set)
  (fset 'neovm--ll1-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1)
          (let ((r (funcall 'neovm--ll1-set-add x result)))
            (when (car r) (setq changed t))
            (setq result (cdr r))))
        (cons changed result))))

  ;; Compute FIRST sets for all symbols in grammar.
  ;; Returns alist of (symbol . first-set).
  (fset 'neovm--ll1-compute-first
    (lambda (grammar)
      (let ((firsts nil)
            (changed t))
        ;; Initialize: terminals have FIRST = {self}, nonterminals = {}
        (dolist (rule grammar)
          (let ((nt (car rule)))
            (unless (assq nt firsts)
              (setq firsts (cons (cons nt nil) firsts)))))
        ;; Iterate until fixpoint
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (let ((nt (car rule))
                  (alts (cdr rule)))
              (dolist (alt alts)
                (let ((add-epsilon t)
                      (symbols alt))
                  ;; For each symbol in the alternative
                  (while (and symbols add-epsilon)
                    (let ((sym (car symbols)))
                      (cond
                       ((funcall 'neovm--ll1-epsilon-p sym)
                        ;; epsilon: add epsilon to FIRST(nt)
                        (let* ((entry (assq nt firsts))
                               (r (funcall 'neovm--ll1-set-add 'epsilon (cdr entry))))
                          (when (car r) (setq changed t))
                          (setcdr entry (cdr r)))
                        (setq symbols (cdr symbols)))
                       ((funcall 'neovm--ll1-terminal-p sym)
                        ;; Terminal: add it to FIRST(nt), stop
                        (let* ((entry (assq nt firsts))
                               (r (funcall 'neovm--ll1-set-add sym (cdr entry))))
                          (when (car r) (setq changed t))
                          (setcdr entry (cdr r)))
                        (setq add-epsilon nil)
                        (setq symbols nil))
                       (t
                        ;; Nonterminal: add FIRST(sym) - epsilon to FIRST(nt)
                        (let* ((sym-first (cdr (assq sym firsts)))
                               (without-eps (delq 'epsilon (copy-sequence sym-first)))
                               (entry (assq nt firsts))
                               (r (funcall 'neovm--ll1-set-union without-eps (cdr entry))))
                          (when (car r) (setq changed t))
                          (setcdr entry (cdr r)))
                        ;; If epsilon in FIRST(sym), continue to next symbol
                        (if (memq 'epsilon (cdr (assq sym firsts)))
                            (setq symbols (cdr symbols))
                          (setq add-epsilon nil)
                          (setq symbols nil))))))
                  ;; If all symbols can derive epsilon, add epsilon
                  (when add-epsilon
                    (let* ((entry (assq nt firsts))
                           (r (funcall 'neovm--ll1-set-add 'epsilon (cdr entry))))
                      (when (car r) (setq changed t))
                      (setcdr entry (cdr r)))))))))
        ;; Sort each set for deterministic output
        (dolist (entry firsts)
          (setcdr entry (sort (cdr entry)
                              (lambda (a b)
                                (string< (format "%s" a) (format "%s" b))))))
        (sort firsts (lambda (a b) (string< (symbol-name (car a))
                                             (symbol-name (car b))))))))

  (unwind-protect
      (let* (;; Grammar:
             ;; E  -> T E'
             ;; E' -> "+" T E' | epsilon
             ;; T  -> F T'
             ;; T' -> "*" F T' | epsilon
             ;; F  -> "(" E ")" | "id"
             (grammar '((E    (T Ep))
                        (Ep   ("+" T Ep) (epsilon))
                        (T    (F Tp))
                        (Tp   ("*" F Tp) (epsilon))
                        (F    ("(" E ")") ("id"))))
             (firsts (funcall 'neovm--ll1-compute-first grammar)))
        ;; Also test a simpler grammar:
        ;; S -> A B
        ;; A -> "a" | epsilon
        ;; B -> "b"
        (let* ((simple-grammar '((S (A B))
                                 (A ("a") (epsilon))
                                 (B ("b"))))
               (simple-firsts (funcall 'neovm--ll1-compute-first simple-grammar)))
          (list
           'expr-firsts firsts
           'simple-firsts simple-firsts)))
    (fmakunbound 'neovm--ll1-terminal-p)
    (fmakunbound 'neovm--ll1-epsilon-p)
    (fmakunbound 'neovm--ll1-get-alts)
    (fmakunbound 'neovm--ll1-set-add)
    (fmakunbound 'neovm--ll1-set-union)
    (fmakunbound 'neovm--ll1-compute-first)))"#;
    let expect = expect_test::expect![[
        r#""OK (expr-firsts ((E \"(\" \"id\") (Ep \"+\" epsilon) (F \"(\" \"id\") (T \"(\" \"id\") (Tp \"*\" epsilon)) simple-firsts ((A \"a\" epsilon) (B \"b\") (S \"a\" \"b\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// FOLLOW set computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_follow_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute FOLLOW sets using FIRST sets. FOLLOW(A) contains terminals
    // that can appear immediately after A in some derivation.
    let form = r#"(progn
  (fset 'neovm--ll2-terminal-p (lambda (sym) (stringp sym)))
  (fset 'neovm--ll2-epsilon-p (lambda (sym) (eq sym 'epsilon)))

  (fset 'neovm--ll2-set-add
    (lambda (elem set)
      (if (member elem set) (cons nil set)
        (cons t (cons elem set)))))

  (fset 'neovm--ll2-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1)
          (let ((r (funcall 'neovm--ll2-set-add x result)))
            (when (car r) (setq changed t))
            (setq result (cdr r))))
        (cons changed result))))

  ;; Compute FIRST of a sequence of symbols
  (fset 'neovm--ll2-first-of-seq
    (lambda (seq firsts)
      (let ((result nil) (all-nullable t))
        (dolist (sym seq)
          (when all-nullable
            (cond
             ((funcall 'neovm--ll2-terminal-p sym)
              (setq result (cons sym result))
              (setq all-nullable nil))
             ((funcall 'neovm--ll2-epsilon-p sym)
              nil)
             (t
              (let ((f (cdr (assq sym firsts))))
                (dolist (x f)
                  (unless (eq x 'epsilon)
                    (unless (member x result) (setq result (cons x result)))))
                (unless (memq 'epsilon f)
                  (setq all-nullable nil)))))))
        (when all-nullable
          (setq result (cons 'epsilon result)))
        result)))

  ;; Compute FIRST sets (simplified, same algorithm as before)
  (fset 'neovm--ll2-compute-first
    (lambda (grammar)
      (let ((firsts nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) firsts)
            (setq firsts (cons (cons (car rule) nil) firsts))))
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (dolist (alt (cdr rule))
              (let ((first-seq (funcall 'neovm--ll2-first-of-seq alt firsts))
                    (entry (assq (car rule) firsts)))
                (let ((r (funcall 'neovm--ll2-set-union first-seq (cdr entry))))
                  (when (car r) (setq changed t))
                  (setcdr entry (cdr r)))))))
        firsts)))

  ;; Compute FOLLOW sets
  (fset 'neovm--ll2-compute-follow
    (lambda (grammar start firsts)
      (let ((follows nil) (changed t))
        ;; Initialize all nonterminals with empty set
        (dolist (rule grammar)
          (unless (assq (car rule) follows)
            (setq follows (cons (cons (car rule) nil) follows))))
        ;; Add $ to FOLLOW(start)
        (let ((entry (assq start follows)))
          (setcdr entry (list "$")))
        ;; Iterate until fixpoint
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (let ((lhs (car rule)))
              (dolist (alt (cdr rule))
                ;; For each position in the alternative
                (let ((i 0))
                  (while (< i (length alt))
                    (let ((sym (nth i alt)))
                      (when (and (symbolp sym)
                                 (not (funcall 'neovm--ll2-epsilon-p sym))
                                 (assq sym follows))
                        ;; sym is a nonterminal in the alternative
                        (let* ((rest (nthcdr (1+ i) alt))
                               (first-rest (funcall 'neovm--ll2-first-of-seq rest firsts))
                               (entry (assq sym follows)))
                          ;; Add FIRST(rest) - epsilon to FOLLOW(sym)
                          (dolist (x first-rest)
                            (unless (eq x 'epsilon)
                              (let ((r (funcall 'neovm--ll2-set-add x (cdr entry))))
                                (when (car r) (setq changed t))
                                (setcdr entry (cdr r)))))
                          ;; If rest can derive epsilon, add FOLLOW(lhs) to FOLLOW(sym)
                          (when (or (null rest) (memq 'epsilon first-rest))
                            (let ((lhs-follow (cdr (assq lhs follows)))
                                  (r (funcall 'neovm--ll2-set-union
                                              (cdr (assq lhs follows))
                                              (cdr entry))))
                              (when (car r) (setq changed t))
                              (setcdr entry (cdr r)))))))
                    (setq i (1+ i))))))))
        ;; Sort for deterministic output
        (dolist (entry follows)
          (setcdr entry (sort (cdr entry)
                              (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        (sort follows (lambda (a b) (string< (symbol-name (car a))
                                              (symbol-name (car b))))))))

  (unwind-protect
      (let* ((grammar '((E  (T Ep))
                         (Ep ("+" T Ep) (epsilon))
                         (T  (F Tp))
                         (Tp ("*" F Tp) (epsilon))
                         (F  ("(" E ")") ("id"))))
             (firsts (funcall 'neovm--ll2-compute-first grammar))
             (follows (funcall 'neovm--ll2-compute-follow grammar 'E firsts)))
        (list 'follows follows))
    (fmakunbound 'neovm--ll2-terminal-p)
    (fmakunbound 'neovm--ll2-epsilon-p)
    (fmakunbound 'neovm--ll2-set-add)
    (fmakunbound 'neovm--ll2-set-union)
    (fmakunbound 'neovm--ll2-first-of-seq)
    (fmakunbound 'neovm--ll2-compute-first)
    (fmakunbound 'neovm--ll2-compute-follow)))"#;
    let expect = expect_test::expect![[
        r#""OK (follows ((E \"$\" \")\") (Ep \"$\" \")\") (F \"$\" \")\" \"*\" \"+\") (T \"$\" \")\" \"+\") (Tp \"$\" \")\" \"+\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LL(1) parse table construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_parse_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build the LL(1) parse table from FIRST and FOLLOW sets.
    // Table[A, a] = production to use when nonterminal A sees terminal a.
    let form = r#"(progn
  (fset 'neovm--ll3-terminal-p (lambda (sym) (stringp sym)))
  (fset 'neovm--ll3-epsilon-p (lambda (sym) (eq sym 'epsilon)))

  (fset 'neovm--ll3-set-add
    (lambda (elem set)
      (if (member elem set) (cons nil set) (cons t (cons elem set)))))

  (fset 'neovm--ll3-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1)
          (let ((r (funcall 'neovm--ll3-set-add x result)))
            (when (car r) (setq changed t))
            (setq result (cdr r))))
        (cons changed result))))

  (fset 'neovm--ll3-first-of-seq
    (lambda (seq firsts)
      (let ((result nil) (all-nullable t))
        (dolist (sym seq)
          (when all-nullable
            (cond
             ((funcall 'neovm--ll3-terminal-p sym)
              (setq result (cons sym result)) (setq all-nullable nil))
             ((funcall 'neovm--ll3-epsilon-p sym) nil)
             (t (let ((f (cdr (assq sym firsts))))
                  (dolist (x f) (unless (eq x 'epsilon)
                                  (unless (member x result) (setq result (cons x result)))))
                  (unless (memq 'epsilon f) (setq all-nullable nil)))))))
        (when all-nullable (setq result (cons 'epsilon result)))
        result)))

  (fset 'neovm--ll3-compute-first
    (lambda (grammar)
      (let ((firsts nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) firsts)
            (setq firsts (cons (cons (car rule) nil) firsts))))
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (dolist (alt (cdr rule))
              (let ((fs (funcall 'neovm--ll3-first-of-seq alt firsts))
                    (entry (assq (car rule) firsts)))
                (let ((r (funcall 'neovm--ll3-set-union fs (cdr entry))))
                  (when (car r) (setq changed t))
                  (setcdr entry (cdr r)))))))
        firsts)))

  (fset 'neovm--ll3-compute-follow
    (lambda (grammar start firsts)
      (let ((follows nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) follows)
            (setq follows (cons (cons (car rule) nil) follows))))
        (setcdr (assq start follows) (list "$"))
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (let ((lhs (car rule)))
              (dolist (alt (cdr rule))
                (let ((i 0))
                  (while (< i (length alt))
                    (let ((sym (nth i alt)))
                      (when (and (symbolp sym) (not (funcall 'neovm--ll3-epsilon-p sym))
                                 (assq sym follows))
                        (let* ((rest (nthcdr (1+ i) alt))
                               (fr (funcall 'neovm--ll3-first-of-seq rest firsts))
                               (entry (assq sym follows)))
                          (dolist (x fr)
                            (unless (eq x 'epsilon)
                              (let ((r (funcall 'neovm--ll3-set-add x (cdr entry))))
                                (when (car r) (setq changed t))
                                (setcdr entry (cdr r)))))
                          (when (or (null rest) (memq 'epsilon fr))
                            (let ((r (funcall 'neovm--ll3-set-union
                                              (cdr (assq lhs follows)) (cdr entry))))
                              (when (car r) (setq changed t))
                              (setcdr entry (cdr r)))))))
                    (setq i (1+ i))))))))
        follows)))

  ;; Build parse table: hash-table keyed by (nonterminal . terminal)
  ;; Value is the production (list of symbols).
  (fset 'neovm--ll3-build-table
    (lambda (grammar firsts follows)
      (let ((table (make-hash-table :test 'equal)))
        (dolist (rule grammar)
          (let ((nt (car rule)))
            (dolist (alt (cdr rule))
              (let ((first-alt (funcall 'neovm--ll3-first-of-seq alt firsts)))
                ;; For each terminal in FIRST(alt), add entry
                (dolist (t-sym first-alt)
                  (unless (eq t-sym 'epsilon)
                    (puthash (cons nt t-sym) alt table)))
                ;; If epsilon in FIRST(alt), for each terminal in FOLLOW(nt), add entry
                (when (memq 'epsilon first-alt)
                  (dolist (f-sym (cdr (assq nt follows)))
                    (puthash (cons nt f-sym) alt table)))))))
        table)))

  ;; Extract table entries as sorted list for deterministic comparison
  (fset 'neovm--ll3-table-to-alist
    (lambda (table)
      (let ((entries nil))
        (maphash (lambda (k v) (setq entries (cons (cons k v) entries))) table)
        (sort entries (lambda (a b)
                        (string< (format "%s" (car a)) (format "%s" (car b))))))))

  (unwind-protect
      (let* ((grammar '((E  (T Ep))
                         (Ep ("+" T Ep) (epsilon))
                         (T  (F Tp))
                         (Tp ("*" F Tp) (epsilon))
                         (F  ("(" E ")") ("id"))))
             (firsts (funcall 'neovm--ll3-compute-first grammar))
             (follows (funcall 'neovm--ll3-compute-follow grammar 'E firsts))
             (table (funcall 'neovm--ll3-build-table grammar firsts follows))
             (entries (funcall 'neovm--ll3-table-to-alist table)))
        (list
         'table-size (hash-table-count table)
         'entries entries))
    (fmakunbound 'neovm--ll3-terminal-p)
    (fmakunbound 'neovm--ll3-epsilon-p)
    (fmakunbound 'neovm--ll3-set-add)
    (fmakunbound 'neovm--ll3-set-union)
    (fmakunbound 'neovm--ll3-first-of-seq)
    (fmakunbound 'neovm--ll3-compute-first)
    (fmakunbound 'neovm--ll3-compute-follow)
    (fmakunbound 'neovm--ll3-build-table)
    (fmakunbound 'neovm--ll3-table-to-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (table-size 13 entries (((E . \"(\") T Ep) ((E . \"id\") T Ep) ((Ep . \"$\") epsilon) ((Ep . \")\") epsilon) ((Ep . \"+\") \"+\" T Ep) ((F . \"(\") \"(\" E \")\") ((F . \"id\") \"id\") ((T . \"(\") F Tp) ((T . \"id\") F Tp) ((Tp . \"$\") epsilon) ((Tp . \")\") epsilon) ((Tp . \"*\") \"*\" F Tp) ((Tp . \"+\") epsilon)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predictive parsing algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_predictive_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LL(1) predictive parser: uses parse table to drive a stack-based parser.
    // Input is a list of tokens (strings), output is a trace of productions used.
    let form = r#"(progn
  (fset 'neovm--ll4-terminal-p (lambda (sym) (stringp sym)))
  (fset 'neovm--ll4-epsilon-p (lambda (sym) (eq sym 'epsilon)))

  (fset 'neovm--ll4-set-add
    (lambda (elem set)
      (if (member elem set) (cons nil set) (cons t (cons elem set)))))
  (fset 'neovm--ll4-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1) (let ((r (funcall 'neovm--ll4-set-add x result)))
                         (when (car r) (setq changed t)) (setq result (cdr r))))
        (cons changed result))))
  (fset 'neovm--ll4-first-of-seq
    (lambda (seq firsts)
      (let ((result nil) (all-nullable t))
        (dolist (sym seq)
          (when all-nullable
            (cond ((funcall 'neovm--ll4-terminal-p sym)
                   (setq result (cons sym result)) (setq all-nullable nil))
                  ((funcall 'neovm--ll4-epsilon-p sym) nil)
                  (t (let ((f (cdr (assq sym firsts))))
                       (dolist (x f) (unless (eq x 'epsilon)
                                       (unless (member x result)
                                         (setq result (cons x result)))))
                       (unless (memq 'epsilon f) (setq all-nullable nil)))))))
        (when all-nullable (setq result (cons 'epsilon result)))
        result)))
  (fset 'neovm--ll4-compute-first
    (lambda (grammar)
      (let ((firsts nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) firsts)
            (setq firsts (cons (cons (car rule) nil) firsts))))
        (while changed (setq changed nil)
          (dolist (rule grammar)
            (dolist (alt (cdr rule))
              (let ((fs (funcall 'neovm--ll4-first-of-seq alt firsts))
                    (entry (assq (car rule) firsts)))
                (let ((r (funcall 'neovm--ll4-set-union fs (cdr entry))))
                  (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
        firsts)))
  (fset 'neovm--ll4-compute-follow
    (lambda (grammar start firsts)
      (let ((follows nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) follows)
            (setq follows (cons (cons (car rule) nil) follows))))
        (setcdr (assq start follows) (list "$"))
        (while changed (setq changed nil)
          (dolist (rule grammar) (let ((lhs (car rule)))
            (dolist (alt (cdr rule))
              (let ((i 0)) (while (< i (length alt))
                (let ((sym (nth i alt)))
                  (when (and (symbolp sym) (not (funcall 'neovm--ll4-epsilon-p sym))
                             (assq sym follows))
                    (let* ((rest (nthcdr (1+ i) alt))
                           (fr (funcall 'neovm--ll4-first-of-seq rest firsts))
                           (entry (assq sym follows)))
                      (dolist (x fr) (unless (eq x 'epsilon)
                        (let ((r (funcall 'neovm--ll4-set-add x (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))
                      (when (or (null rest) (memq 'epsilon fr))
                        (let ((r (funcall 'neovm--ll4-set-union
                                          (cdr (assq lhs follows)) (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
                (setq i (1+ i))))))))
        follows)))
  (fset 'neovm--ll4-build-table
    (lambda (grammar firsts follows)
      (let ((table (make-hash-table :test 'equal)))
        (dolist (rule grammar) (let ((nt (car rule)))
          (dolist (alt (cdr rule))
            (let ((fa (funcall 'neovm--ll4-first-of-seq alt firsts)))
              (dolist (ts fa) (unless (eq ts 'epsilon)
                (puthash (cons nt ts) alt table)))
              (when (memq 'epsilon fa)
                (dolist (fs (cdr (assq nt follows)))
                  (puthash (cons nt fs) alt table)))))))
        table)))

  ;; Predictive parser: takes token list, returns list of productions used
  (fset 'neovm--ll4-parse
    (lambda (table start tokens)
      (let ((stack (list "$" start))
            (input (append tokens (list "$")))
            (trace nil)
            (pos 0)
            (error nil)
            (limit 100))
        (while (and (not error) (> limit 0))
          (setq limit (1- limit))
          (let ((top (car (last stack)))
                (lookahead (nth pos input)))
            (cond
             ;; Stack and input both at $: success
             ((and (equal top "$") (equal lookahead "$"))
              (setq limit 0))
             ;; Top is terminal: match against input
             ((stringp top)
              (if (equal top lookahead)
                  (progn
                    (setq stack (butlast stack))
                    (setq pos (1+ pos)))
                (setq error (list 'mismatch top lookahead pos))))
             ;; Top is nonterminal: consult table
             ((symbolp top)
              (let ((production (gethash (cons top lookahead) table)))
                (if production
                    (progn
                      (setq trace (cons (list top '-> production) trace))
                      (setq stack (butlast stack))
                      ;; Push production in reverse (rightmost first onto stack)
                      (let ((syms production))
                        (dolist (sym syms)
                          (unless (funcall 'neovm--ll4-epsilon-p sym)
                            (setq stack (append stack (list sym)))))))
                  (setq error (list 'no-entry top lookahead pos)))))
             (t (setq error (list 'unexpected top))))))
        (if error
            (list 'error error)
          (list 'ok (nreverse trace))))))

  (unwind-protect
      (let* ((grammar '((E  (T Ep))
                         (Ep ("+" T Ep) (epsilon))
                         (T  (F Tp))
                         (Tp ("*" F Tp) (epsilon))
                         (F  ("(" E ")") ("id"))))
             (firsts (funcall 'neovm--ll4-compute-first grammar))
             (follows (funcall 'neovm--ll4-compute-follow grammar 'E firsts))
             (table (funcall 'neovm--ll4-build-table grammar firsts follows)))
        (list
         ;; Parse "id"
         (funcall 'neovm--ll4-parse table 'E '("id"))
         ;; Parse "id + id"
         (funcall 'neovm--ll4-parse table 'E '("id" "+" "id"))
         ;; Parse "id * id"
         (funcall 'neovm--ll4-parse table 'E '("id" "*" "id"))
         ;; Parse "id + id * id"
         (funcall 'neovm--ll4-parse table 'E '("id" "+" "id" "*" "id"))
         ;; Parse "( id + id ) * id"
         (funcall 'neovm--ll4-parse table 'E '("(" "id" "+" "id" ")" "*" "id"))
         ;; Error: unexpected token
         (funcall 'neovm--ll4-parse table 'E '("+"))))
    (fmakunbound 'neovm--ll4-terminal-p)
    (fmakunbound 'neovm--ll4-epsilon-p)
    (fmakunbound 'neovm--ll4-set-add)
    (fmakunbound 'neovm--ll4-set-union)
    (fmakunbound 'neovm--ll4-first-of-seq)
    (fmakunbound 'neovm--ll4-compute-first)
    (fmakunbound 'neovm--ll4-compute-follow)
    (fmakunbound 'neovm--ll4-build-table)
    (fmakunbound 'neovm--ll4-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((error (no-entry Ep \"id\" 0)) (error (no-entry Ep \"id\" 0)) (error (no-entry Ep \"id\" 0)) (error (no-entry Ep \"id\" 0)) (error (no-entry Ep \"(\" 0)) (error (no-entry E \"+\" 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse a simple statement grammar
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_statement_grammar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test a different grammar for simple statements:
    //   S  -> "if" E "then" S S'  |  "id" ":=" E
    //   S' -> "else" S  |  epsilon
    //   E  -> "id"  |  "num"
    // This grammar tests a different structure than the expression grammar.
    let form = r#"(progn
  (fset 'neovm--ll5-terminal-p (lambda (sym) (stringp sym)))
  (fset 'neovm--ll5-epsilon-p (lambda (sym) (eq sym 'epsilon)))
  (fset 'neovm--ll5-set-add
    (lambda (elem set) (if (member elem set) (cons nil set) (cons t (cons elem set)))))
  (fset 'neovm--ll5-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1) (let ((r (funcall 'neovm--ll5-set-add x result)))
                         (when (car r) (setq changed t)) (setq result (cdr r))))
        (cons changed result))))
  (fset 'neovm--ll5-first-of-seq
    (lambda (seq firsts)
      (let ((result nil) (all-nullable t))
        (dolist (sym seq) (when all-nullable
          (cond ((funcall 'neovm--ll5-terminal-p sym)
                 (setq result (cons sym result)) (setq all-nullable nil))
                ((funcall 'neovm--ll5-epsilon-p sym) nil)
                (t (let ((f (cdr (assq sym firsts))))
                     (dolist (x f) (unless (eq x 'epsilon)
                       (unless (member x result) (setq result (cons x result)))))
                     (unless (memq 'epsilon f) (setq all-nullable nil)))))))
        (when all-nullable (setq result (cons 'epsilon result)))
        result)))
  (fset 'neovm--ll5-compute-first
    (lambda (grammar)
      (let ((firsts nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) firsts)
            (setq firsts (cons (cons (car rule) nil) firsts))))
        (while changed (setq changed nil)
          (dolist (rule grammar)
            (dolist (alt (cdr rule))
              (let ((fs (funcall 'neovm--ll5-first-of-seq alt firsts))
                    (entry (assq (car rule) firsts)))
                (let ((r (funcall 'neovm--ll5-set-union fs (cdr entry))))
                  (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
        firsts)))
  (fset 'neovm--ll5-compute-follow
    (lambda (grammar start firsts)
      (let ((follows nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) follows)
            (setq follows (cons (cons (car rule) nil) follows))))
        (setcdr (assq start follows) (list "$"))
        (while changed (setq changed nil)
          (dolist (rule grammar) (let ((lhs (car rule)))
            (dolist (alt (cdr rule))
              (let ((i 0)) (while (< i (length alt))
                (let ((sym (nth i alt)))
                  (when (and (symbolp sym) (not (funcall 'neovm--ll5-epsilon-p sym))
                             (assq sym follows))
                    (let* ((rest (nthcdr (1+ i) alt))
                           (fr (funcall 'neovm--ll5-first-of-seq rest firsts))
                           (entry (assq sym follows)))
                      (dolist (x fr) (unless (eq x 'epsilon)
                        (let ((r (funcall 'neovm--ll5-set-add x (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))
                      (when (or (null rest) (memq 'epsilon fr))
                        (let ((r (funcall 'neovm--ll5-set-union
                                          (cdr (assq lhs follows)) (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
                (setq i (1+ i))))))))
        follows)))
  (fset 'neovm--ll5-build-table
    (lambda (grammar firsts follows)
      (let ((table (make-hash-table :test 'equal)))
        (dolist (rule grammar) (let ((nt (car rule)))
          (dolist (alt (cdr rule))
            (let ((fa (funcall 'neovm--ll5-first-of-seq alt firsts)))
              (dolist (ts fa) (unless (eq ts 'epsilon)
                (puthash (cons nt ts) alt table)))
              (when (memq 'epsilon fa)
                (dolist (fs (cdr (assq nt follows)))
                  (puthash (cons nt fs) alt table)))))))
        table)))

  (unwind-protect
      (let* ((grammar '((S  ("if" E "then" S Sp) ("id" ":=" E))
                         (Sp ("else" S) (epsilon))
                         (E  ("id") ("num"))))
             (firsts (funcall 'neovm--ll5-compute-first grammar))
             (follows (funcall 'neovm--ll5-compute-follow grammar 'S firsts)))
        ;; Sort for deterministic output
        (dolist (entry firsts)
          (setcdr entry (sort (cdr entry)
                              (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        (dolist (entry follows)
          (setcdr entry (sort (cdr entry)
                              (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        (list
         'firsts (sort firsts (lambda (a b) (string< (symbol-name (car a))
                                                      (symbol-name (car b)))))
         'follows (sort follows (lambda (a b) (string< (symbol-name (car a))
                                                        (symbol-name (car b)))))))
    (fmakunbound 'neovm--ll5-terminal-p)
    (fmakunbound 'neovm--ll5-epsilon-p)
    (fmakunbound 'neovm--ll5-set-add)
    (fmakunbound 'neovm--ll5-set-union)
    (fmakunbound 'neovm--ll5-first-of-seq)
    (fmakunbound 'neovm--ll5-compute-first)
    (fmakunbound 'neovm--ll5-compute-follow)
    (fmakunbound 'neovm--ll5-build-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (firsts ((E \"id\" \"num\") (S \"id\" \"if\") (Sp \"else\" epsilon)) follows ((E \"$\" \"else\" \"then\") (S \"$\" \"else\") (Sp \"$\" \"else\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error recovery (panic mode)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ll1_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement panic-mode error recovery: on parse error, skip input tokens
    // until a synchronizing token (from FOLLOW set) is found, then pop the
    // offending nonterminal and continue parsing.
    let form = r#"(progn
  (fset 'neovm--ll6-terminal-p (lambda (sym) (stringp sym)))
  (fset 'neovm--ll6-epsilon-p (lambda (sym) (eq sym 'epsilon)))
  (fset 'neovm--ll6-set-add
    (lambda (elem set) (if (member elem set) (cons nil set) (cons t (cons elem set)))))
  (fset 'neovm--ll6-set-union
    (lambda (s1 s2)
      (let ((changed nil) (result s2))
        (dolist (x s1) (let ((r (funcall 'neovm--ll6-set-add x result)))
                         (when (car r) (setq changed t)) (setq result (cdr r))))
        (cons changed result))))
  (fset 'neovm--ll6-first-of-seq
    (lambda (seq firsts)
      (let ((result nil) (all-nullable t))
        (dolist (sym seq) (when all-nullable
          (cond ((funcall 'neovm--ll6-terminal-p sym)
                 (setq result (cons sym result)) (setq all-nullable nil))
                ((funcall 'neovm--ll6-epsilon-p sym) nil)
                (t (let ((f (cdr (assq sym firsts))))
                     (dolist (x f) (unless (eq x 'epsilon)
                       (unless (member x result) (setq result (cons x result)))))
                     (unless (memq 'epsilon f) (setq all-nullable nil)))))))
        (when all-nullable (setq result (cons 'epsilon result)))
        result)))
  (fset 'neovm--ll6-compute-first
    (lambda (grammar)
      (let ((firsts nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) firsts)
            (setq firsts (cons (cons (car rule) nil) firsts))))
        (while changed (setq changed nil)
          (dolist (rule grammar)
            (dolist (alt (cdr rule))
              (let ((fs (funcall 'neovm--ll6-first-of-seq alt firsts))
                    (entry (assq (car rule) firsts)))
                (let ((r (funcall 'neovm--ll6-set-union fs (cdr entry))))
                  (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
        firsts)))
  (fset 'neovm--ll6-compute-follow
    (lambda (grammar start firsts)
      (let ((follows nil) (changed t))
        (dolist (rule grammar)
          (unless (assq (car rule) follows)
            (setq follows (cons (cons (car rule) nil) follows))))
        (setcdr (assq start follows) (list "$"))
        (while changed (setq changed nil)
          (dolist (rule grammar) (let ((lhs (car rule)))
            (dolist (alt (cdr rule))
              (let ((i 0)) (while (< i (length alt))
                (let ((sym (nth i alt)))
                  (when (and (symbolp sym) (not (funcall 'neovm--ll6-epsilon-p sym))
                             (assq sym follows))
                    (let* ((rest (nthcdr (1+ i) alt))
                           (fr (funcall 'neovm--ll6-first-of-seq rest firsts))
                           (entry (assq sym follows)))
                      (dolist (x fr) (unless (eq x 'epsilon)
                        (let ((r (funcall 'neovm--ll6-set-add x (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))
                      (when (or (null rest) (memq 'epsilon fr))
                        (let ((r (funcall 'neovm--ll6-set-union
                                          (cdr (assq lhs follows)) (cdr entry))))
                          (when (car r) (setq changed t)) (setcdr entry (cdr r)))))))
                (setq i (1+ i))))))))
        follows)))
  (fset 'neovm--ll6-build-table
    (lambda (grammar firsts follows)
      (let ((table (make-hash-table :test 'equal)))
        (dolist (rule grammar) (let ((nt (car rule)))
          (dolist (alt (cdr rule))
            (let ((fa (funcall 'neovm--ll6-first-of-seq alt firsts)))
              (dolist (ts fa) (unless (eq ts 'epsilon)
                (puthash (cons nt ts) alt table)))
              (when (memq 'epsilon fa)
                (dolist (fs (cdr (assq nt follows)))
                  (puthash (cons nt fs) alt table)))))))
        table)))

  ;; Parser with panic-mode error recovery
  (fset 'neovm--ll6-parse-recover
    (lambda (table follows start tokens)
      (let ((stack (list "$" start))
            (input (append tokens (list "$")))
            (trace nil)
            (errors nil)
            (pos 0)
            (limit 200))
        (while (and (> limit 0)
                    (not (and (equal (car (last stack)) "$")
                              (equal (nth pos input) "$"))))
          (setq limit (1- limit))
          (let ((top (car (last stack)))
                (lookahead (nth pos input)))
            (cond
             ;; Terminal on stack
             ((stringp top)
              (if (equal top lookahead)
                  (progn (setq stack (butlast stack))
                         (setq pos (1+ pos)))
                ;; Error: expected terminal not found, skip it from stack
                (setq errors (cons (list 'expected top 'got lookahead 'at pos) errors))
                (setq stack (butlast stack))))
             ;; Nonterminal
             ((symbolp top)
              (let ((production (gethash (cons top lookahead) table)))
                (if production
                    (progn
                      (setq trace (cons (list top '-> production) trace))
                      (setq stack (butlast stack))
                      (let ((syms production))
                        (dolist (sym syms)
                          (unless (funcall 'neovm--ll6-epsilon-p sym)
                            (setq stack (append stack (list sym)))))))
                  ;; Panic mode: skip tokens until FOLLOW(top) or $
                  (let ((follow-set (cdr (assq top follows)))
                        (skipped nil))
                    (setq errors (cons (list 'panic top lookahead pos) errors))
                    ;; Skip input tokens
                    (while (and (not (member (nth pos input) follow-set))
                                (not (equal (nth pos input) "$"))
                                (< pos (length input)))
                      (setq skipped (cons (nth pos input) skipped))
                      (setq pos (1+ pos)))
                    ;; Pop the nonterminal
                    (setq stack (butlast stack))))))
             (t (setq limit 0)))))
        (list
         'trace (nreverse trace)
         'errors (nreverse errors)
         'success (null errors)))))

  (unwind-protect
      (let* ((grammar '((E  (T Ep))
                         (Ep ("+" T Ep) (epsilon))
                         (T  (F Tp))
                         (Tp ("*" F Tp) (epsilon))
                         (F  ("(" E ")") ("id"))))
             (firsts (funcall 'neovm--ll6-compute-first grammar))
             (follows (funcall 'neovm--ll6-compute-follow grammar 'E firsts))
             (table (funcall 'neovm--ll6-build-table grammar firsts follows)))
        (list
         ;; Valid input: no errors
         (nth 5 (funcall 'neovm--ll6-parse-recover table follows 'E '("id" "+" "id")))
         ;; Error: starts with + (missing id)
         (funcall 'neovm--ll6-parse-recover table follows 'E '("+"))
         ;; Error: double operator "id + + id"
         (funcall 'neovm--ll6-parse-recover table follows 'E '("id" "+" "+" "id"))
         ;; Error: missing closing paren "( id + id"
         (funcall 'neovm--ll6-parse-recover table follows 'E '("(" "id" "+" "id"))))
    (fmakunbound 'neovm--ll6-terminal-p)
    (fmakunbound 'neovm--ll6-epsilon-p)
    (fmakunbound 'neovm--ll6-set-add)
    (fmakunbound 'neovm--ll6-set-union)
    (fmakunbound 'neovm--ll6-first-of-seq)
    (fmakunbound 'neovm--ll6-compute-first)
    (fmakunbound 'neovm--ll6-compute-follow)
    (fmakunbound 'neovm--ll6-build-table)
    (fmakunbound 'neovm--ll6-parse-recover)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (trace nil errors ((panic E \"+\" 0)) success nil) (trace ((E -> (T Ep))) errors ((panic Ep \"id\" 0) (panic T \"$\" 4)) success nil) (trace ((E -> (T Ep))) errors ((panic Ep \"(\" 0) (panic T \"$\" 4)) success nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
