//! Oracle parity tests for a regex-to-NFA converter implemented in Elisp.
//!
//! Implements Thompson's construction for basic operators (concat, union,
//! Kleene star), NFA state representation, epsilon closure computation,
//! NFA simulation for string matching, character classes, and full
//! regex compilation and execution pipeline.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// NFA state representation and basic construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_state_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // NFA state: (id transitions)
    // Transition: (label target-id) where label is a char, 'eps for epsilon, or 'any for dot
    // NFA: (states start-id accept-id)
    let form = r#"(progn
  ;; State counter for unique IDs
  (defvar neovm--nfa-counter 0)
  (fset 'neovm--nfa-new-id (lambda () (setq neovm--nfa-counter (1+ neovm--nfa-counter)) neovm--nfa-counter))
  (fset 'neovm--nfa-reset (lambda () (setq neovm--nfa-counter 0)))

  ;; Create a single state with transitions
  (fset 'neovm--nfa-state (lambda (id transitions) (list id transitions)))
  (fset 'neovm--nfa-state-id (lambda (s) (car s)))
  (fset 'neovm--nfa-state-trans (lambda (s) (cadr s)))

  ;; NFA structure: (states start accept)
  (fset 'neovm--nfa-make (lambda (states start accept) (list states start accept)))
  (fset 'neovm--nfa-states (lambda (nfa) (car nfa)))
  (fset 'neovm--nfa-start (lambda (nfa) (cadr nfa)))
  (fset 'neovm--nfa-accept (lambda (nfa) (caddr nfa)))

  ;; Thompson's construction: literal character
  ;; Creates two states: start --char--> accept
  (fset 'neovm--nfa-literal
    (lambda (ch)
      (funcall 'neovm--nfa-reset)
      (let ((s (funcall 'neovm--nfa-new-id))
            (a (funcall 'neovm--nfa-new-id)))
        (funcall 'neovm--nfa-make
                 (list (funcall 'neovm--nfa-state s (list (list ch a)))
                       (funcall 'neovm--nfa-state a nil))
                 s a))))

  ;; Test basic state structure
  (let ((nfa (funcall 'neovm--nfa-literal ?a)))
    (unwind-protect
        (list
         ;; Should have 2 states
         (length (funcall 'neovm--nfa-states nfa))
         ;; Start and accept should differ
         (not (= (funcall 'neovm--nfa-start nfa)
                 (funcall 'neovm--nfa-accept nfa)))
         ;; Start state has one transition
         (let ((start-state (car (funcall 'neovm--nfa-states nfa))))
           (length (funcall 'neovm--nfa-state-trans start-state)))
         ;; Accept state has no transitions
         (let ((accept-state (cadr (funcall 'neovm--nfa-states nfa))))
           (length (funcall 'neovm--nfa-state-trans accept-state))))
      (progn
        (makunbound 'neovm--nfa-counter)
        (fmakunbound 'neovm--nfa-new-id) (fmakunbound 'neovm--nfa-reset)
        (fmakunbound 'neovm--nfa-state) (fmakunbound 'neovm--nfa-state-id)
        (fmakunbound 'neovm--nfa-state-trans) (fmakunbound 'neovm--nfa-make)
        (fmakunbound 'neovm--nfa-states) (fmakunbound 'neovm--nfa-start)
        (fmakunbound 'neovm--nfa-accept) (fmakunbound 'neovm--nfa-literal)))))"#;
    let expect = expect_test::expect![[r#""OK (2 t 1 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Thompson's construction: concat, union, star
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_thompson_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--nfa-ctr 0)
  (fset 'neovm--nfa-id (lambda () (setq neovm--nfa-ctr (1+ neovm--nfa-ctr)) neovm--nfa-ctr))
  (fset 'neovm--nfa-st (lambda (id tr) (list id tr)))
  (fset 'neovm--nfa-mk (lambda (sts s a) (list sts s a)))

  ;; Find a state by id in a state list
  (fset 'neovm--nfa-find-state
    (lambda (states id)
      (let ((found nil))
        (dolist (s states)
          (when (= (car s) id) (setq found s)))
        found)))

  ;; Add transition to a state (returns new state)
  (fset 'neovm--nfa-add-trans
    (lambda (state label target)
      (funcall 'neovm--nfa-st (car state)
               (cons (list label target) (cadr state)))))

  ;; Thompson: literal char
  (fset 'neovm--nfa-lit
    (lambda (ch)
      (let ((s (funcall 'neovm--nfa-id)) (a (funcall 'neovm--nfa-id)))
        (funcall 'neovm--nfa-mk
                 (list (funcall 'neovm--nfa-st s (list (list ch a)))
                       (funcall 'neovm--nfa-st a nil))
                 s a))))

  ;; Thompson: concatenation (a then b)
  ;; Connect a's accept to b's start via epsilon
  (fset 'neovm--nfa-concat
    (lambda (nfa-a nfa-b)
      (let* ((states-a (car nfa-a)) (start-a (cadr nfa-a)) (accept-a (caddr nfa-a))
             (states-b (car nfa-b)) (start-b (cadr nfa-b)) (accept-b (caddr nfa-b))
             ;; Add epsilon from accept-a to start-b
             (acc-state (funcall 'neovm--nfa-find-state states-a accept-a))
             (new-acc (funcall 'neovm--nfa-add-trans acc-state 'eps start-b))
             ;; Replace accept-a in states-a
             (new-states (append (mapcar (lambda (s) (if (= (car s) accept-a) new-acc s)) states-a)
                                 states-b)))
        (funcall 'neovm--nfa-mk new-states start-a accept-b))))

  ;; Thompson: union (a | b)
  ;; New start with eps to both a-start and b-start
  ;; Both a-accept and b-accept have eps to new accept
  (fset 'neovm--nfa-union
    (lambda (nfa-a nfa-b)
      (let* ((s (funcall 'neovm--nfa-id)) (a (funcall 'neovm--nfa-id))
             (states-a (car nfa-a)) (start-a (cadr nfa-a)) (accept-a (caddr nfa-a))
             (states-b (car nfa-b)) (start-b (cadr nfa-b)) (accept-b (caddr nfa-b))
             ;; Accept states of both get eps to new accept
             (acc-a (funcall 'neovm--nfa-find-state states-a accept-a))
             (new-acc-a (funcall 'neovm--nfa-add-trans acc-a 'eps a))
             (acc-b (funcall 'neovm--nfa-find-state states-b accept-b))
             (new-acc-b (funcall 'neovm--nfa-add-trans acc-b 'eps a))
             (new-states (append (list (funcall 'neovm--nfa-st s (list (list 'eps start-a) (list 'eps start-b)))
                                       (funcall 'neovm--nfa-st a nil))
                                 (mapcar (lambda (st) (if (= (car st) accept-a) new-acc-a st)) states-a)
                                 (mapcar (lambda (st) (if (= (car st) accept-b) new-acc-b st)) states-b))))
        (funcall 'neovm--nfa-mk new-states s a))))

  ;; Thompson: Kleene star (a*)
  ;; New start --eps--> a-start, a-accept --eps--> a-start and new-accept
  ;; new-start --eps--> new-accept (for zero matches)
  (fset 'neovm--nfa-star
    (lambda (nfa-a)
      (let* ((s (funcall 'neovm--nfa-id)) (a (funcall 'neovm--nfa-id))
             (states-a (car nfa-a)) (start-a (cadr nfa-a)) (accept-a (caddr nfa-a))
             (acc-st (funcall 'neovm--nfa-find-state states-a accept-a))
             (new-acc-st (funcall 'neovm--nfa-st (car acc-st)
                                  (cons (list 'eps start-a) (cons (list 'eps a) (cadr acc-st)))))
             (new-states (append (list (funcall 'neovm--nfa-st s (list (list 'eps start-a) (list 'eps a)))
                                       (funcall 'neovm--nfa-st a nil))
                                 (mapcar (lambda (st) (if (= (car st) accept-a) new-acc-st st)) states-a))))
        (funcall 'neovm--nfa-mk new-states s a))))

  ;; Count states in each construction
  (setq neovm--nfa-ctr 0)
  (let* ((lit-a (funcall 'neovm--nfa-lit ?a))
         (lit-b (funcall 'neovm--nfa-lit ?b))
         (lit-c (funcall 'neovm--nfa-lit ?c))
         ;; ab: concatenation
         (ab (funcall 'neovm--nfa-concat lit-a lit-b))
         ;; a|c: union
         (a-or-c (funcall 'neovm--nfa-union (funcall 'neovm--nfa-lit ?a) lit-c))
         ;; a*: star
         (a-star (funcall 'neovm--nfa-star (funcall 'neovm--nfa-lit ?a))))
    (unwind-protect
        (list
         ;; literal: 2 states
         (length (car lit-a))
         ;; concat: 4 states (2+2, shared accept/start via eps)
         (length (car ab))
         ;; union: 6 states (2+2+2 new)
         (length (car a-or-c))
         ;; star: 4 states (2+2 new)
         (length (car a-star))
         ;; Start/accept differ for all
         (not (= (cadr ab) (caddr ab)))
         (not (= (cadr a-or-c) (caddr a-or-c)))
         (not (= (cadr a-star) (caddr a-star))))
      (progn
        (makunbound 'neovm--nfa-ctr)
        (fmakunbound 'neovm--nfa-id) (fmakunbound 'neovm--nfa-st)
        (fmakunbound 'neovm--nfa-mk) (fmakunbound 'neovm--nfa-find-state)
        (fmakunbound 'neovm--nfa-add-trans) (fmakunbound 'neovm--nfa-lit)
        (fmakunbound 'neovm--nfa-concat) (fmakunbound 'neovm--nfa-union)
        (fmakunbound 'neovm--nfa-star)))))"#;
    let expect = expect_test::expect![[r#""OK (2 4 6 4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Epsilon closure computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_epsilon_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--ec-ctr 0)
  (fset 'neovm--ec-id (lambda () (setq neovm--ec-ctr (1+ neovm--ec-ctr)) neovm--ec-ctr))
  (fset 'neovm--ec-st (lambda (id tr) (list id tr)))
  (fset 'neovm--ec-mk (lambda (sts s a) (list sts s a)))
  (fset 'neovm--ec-find
    (lambda (states id)
      (let ((found nil)) (dolist (s states) (when (= (car s) id) (setq found s))) found)))

  ;; Epsilon closure: given a set of state IDs, find all states reachable via epsilon transitions
  (fset 'neovm--ec-closure
    (lambda (states id-set)
      (let ((stack (copy-sequence id-set))
            (visited nil))
        (while stack
          (let ((current (car stack)))
            (setq stack (cdr stack))
            (unless (memq current visited)
              (setq visited (cons current visited))
              ;; Find state and follow eps transitions
              (let ((st (funcall 'neovm--ec-find states current)))
                (when st
                  (dolist (tr (cadr st))
                    (when (eq (car tr) 'eps)
                      (unless (memq (cadr tr) visited)
                        (setq stack (cons (cadr tr) stack))))))))))
        (sort visited '<))))

  ;; Build an NFA with epsilon transitions: chain of eps
  ;; 1 --eps--> 2 --eps--> 3 --a--> 4 --eps--> 5
  (setq neovm--ec-ctr 0)
  (let ((states (list
                 (funcall 'neovm--ec-st 1 (list (list 'eps 2)))
                 (funcall 'neovm--ec-st 2 (list (list 'eps 3)))
                 (funcall 'neovm--ec-st 3 (list (list ?a 4)))
                 (funcall 'neovm--ec-st 4 (list (list 'eps 5)))
                 (funcall 'neovm--ec-st 5 nil))))
    ;; Epsilon closure of {1} should be {1, 2, 3}
    (let ((ec1 (funcall 'neovm--ec-closure states '(1)))
          ;; Epsilon closure of {4} should be {4, 5}
          (ec4 (funcall 'neovm--ec-closure states '(4)))
          ;; Epsilon closure of {3} should be {3}
          (ec3 (funcall 'neovm--ec-closure states '(3)))
          ;; Epsilon closure of {1, 4} should be {1, 2, 3, 4, 5}
          (ec14 (funcall 'neovm--ec-closure states '(1 4)))
          ;; Epsilon closure of empty set
          (ec-empty (funcall 'neovm--ec-closure states nil)))

      ;; Also test with a branching epsilon structure:
      ;; 10 --eps--> 11, 10 --eps--> 12, 11 --eps--> 13, 12 --eps--> 13
      (let ((branch-states (list
                            (funcall 'neovm--ec-st 10 (list (list 'eps 11) (list 'eps 12)))
                            (funcall 'neovm--ec-st 11 (list (list 'eps 13)))
                            (funcall 'neovm--ec-st 12 (list (list 'eps 13)))
                            (funcall 'neovm--ec-st 13 nil))))
        (let ((ec10 (funcall 'neovm--ec-closure branch-states '(10))))
          (unwind-protect
              (list ec1 ec4 ec3 ec14 ec-empty ec10)
            (progn
              (makunbound 'neovm--ec-ctr)
              (fmakunbound 'neovm--ec-id) (fmakunbound 'neovm--ec-st)
              (fmakunbound 'neovm--ec-mk) (fmakunbound 'neovm--ec-find)
              (fmakunbound 'neovm--ec-closure))))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3) (4 5) (3) (1 2 3 4 5) nil (10 11 12 13))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA simulation: matching strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--sim-ctr 0)
  (fset 'neovm--sim-id (lambda () (setq neovm--sim-ctr (1+ neovm--sim-ctr)) neovm--sim-ctr))
  (fset 'neovm--sim-st (lambda (id tr) (list id tr)))
  (fset 'neovm--sim-mk (lambda (sts s a) (list sts s a)))
  (fset 'neovm--sim-find
    (lambda (states id)
      (let ((found nil)) (dolist (s states) (when (= (car s) id) (setq found s))) found)))
  (fset 'neovm--sim-add-trans
    (lambda (state label target)
      (funcall 'neovm--sim-st (car state) (cons (list label target) (cadr state)))))

  ;; Epsilon closure
  (fset 'neovm--sim-closure
    (lambda (states id-set)
      (let ((stack (copy-sequence id-set)) (visited nil))
        (while stack
          (let ((cur (car stack)))
            (setq stack (cdr stack))
            (unless (memq cur visited)
              (setq visited (cons cur visited))
              (let ((st (funcall 'neovm--sim-find states cur)))
                (when st (dolist (tr (cadr st))
                           (when (eq (car tr) 'eps)
                             (unless (memq (cadr tr) visited)
                               (setq stack (cons (cadr tr) stack))))))))))
        (sort visited '<))))

  ;; Move: given current states and a character, find next states
  (fset 'neovm--sim-move
    (lambda (states current-ids ch)
      (let ((next nil))
        (dolist (id current-ids)
          (let ((st (funcall 'neovm--sim-find states id)))
            (when st
              (dolist (tr (cadr st))
                (when (or (and (integerp (car tr)) (= (car tr) ch))
                          (eq (car tr) 'any))
                  (unless (memq (cadr tr) next)
                    (setq next (cons (cadr tr) next))))))))
        next)))

  ;; Simulate NFA: returns t if string is accepted
  (fset 'neovm--sim-run
    (lambda (nfa str)
      (let* ((states (car nfa)) (start (cadr nfa)) (accept (caddr nfa))
             (current (funcall 'neovm--sim-closure states (list start)))
             (i 0) (len (length str)))
        (while (< i len)
          (let* ((ch (aref str i))
                 (moved (funcall 'neovm--sim-move states current ch))
                 (next (funcall 'neovm--sim-closure states moved)))
            (setq current next)
            (setq i (1+ i))))
        (if (memq accept current) t nil))))

  ;; Thompson builders
  (fset 'neovm--sim-lit
    (lambda (ch)
      (let ((s (funcall 'neovm--sim-id)) (a (funcall 'neovm--sim-id)))
        (funcall 'neovm--sim-mk
                 (list (funcall 'neovm--sim-st s (list (list ch a)))
                       (funcall 'neovm--sim-st a nil)) s a))))
  (fset 'neovm--sim-dot
    (lambda ()
      (let ((s (funcall 'neovm--sim-id)) (a (funcall 'neovm--sim-id)))
        (funcall 'neovm--sim-mk
                 (list (funcall 'neovm--sim-st s (list (list 'any a)))
                       (funcall 'neovm--sim-st a nil)) s a))))
  (fset 'neovm--sim-cat
    (lambda (na nb)
      (let* ((sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (acc (funcall 'neovm--sim-find sa aa))
             (nacc (funcall 'neovm--sim-add-trans acc 'eps stb))
             (ns (append (mapcar (lambda (s) (if (= (car s) aa) nacc s)) sa) sb)))
        (funcall 'neovm--sim-mk ns sta ab))))
  (fset 'neovm--sim-alt
    (lambda (na nb)
      (let* ((s (funcall 'neovm--sim-id)) (a (funcall 'neovm--sim-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (ac-a (funcall 'neovm--sim-find sa aa))
             (nac-a (funcall 'neovm--sim-add-trans ac-a 'eps a))
             (ac-b (funcall 'neovm--sim-find sb ab))
             (nac-b (funcall 'neovm--sim-add-trans ac-b 'eps a))
             (ns (append (list (funcall 'neovm--sim-st s (list (list 'eps sta) (list 'eps stb)))
                               (funcall 'neovm--sim-st a nil))
                         (mapcar (lambda (s) (if (= (car s) aa) nac-a s)) sa)
                         (mapcar (lambda (s) (if (= (car s) ab) nac-b s)) sb))))
        (funcall 'neovm--sim-mk ns s a))))
  (fset 'neovm--sim-kleene
    (lambda (na)
      (let* ((s (funcall 'neovm--sim-id)) (a (funcall 'neovm--sim-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (acc (funcall 'neovm--sim-find sa aa))
             (nacc (funcall 'neovm--sim-st (car acc)
                            (cons (list 'eps sta) (cons (list 'eps a) (cadr acc)))))
             (ns (append (list (funcall 'neovm--sim-st s (list (list 'eps sta) (list 'eps a)))
                               (funcall 'neovm--sim-st a nil))
                         (mapcar (lambda (st) (if (= (car st) aa) nacc st)) sa))))
        (funcall 'neovm--sim-mk ns s a))))

  ;; Build regex "ab" and test
  (setq neovm--sim-ctr 0)
  (let* ((nfa-ab (funcall 'neovm--sim-cat (funcall 'neovm--sim-lit ?a) (funcall 'neovm--sim-lit ?b)))
         ;; "a|b"
         (nfa-aorb (funcall 'neovm--sim-alt (funcall 'neovm--sim-lit ?a) (funcall 'neovm--sim-lit ?b)))
         ;; "a*"
         (nfa-astar (funcall 'neovm--sim-kleene (funcall 'neovm--sim-lit ?a)))
         ;; "a*b": concat of a* and b
         (nfa-astarb (funcall 'neovm--sim-cat
                              (funcall 'neovm--sim-kleene (funcall 'neovm--sim-lit ?a))
                              (funcall 'neovm--sim-lit ?b)))
         ;; "." (any char)
         (nfa-dot (funcall 'neovm--sim-dot)))
    (unwind-protect
        (list
         ;; "ab" matches
         (funcall 'neovm--sim-run nfa-ab "ab")
         (funcall 'neovm--sim-run nfa-ab "a")
         (funcall 'neovm--sim-run nfa-ab "b")
         (funcall 'neovm--sim-run nfa-ab "abc")
         (funcall 'neovm--sim-run nfa-ab "")
         ;; "a|b" matches
         (funcall 'neovm--sim-run nfa-aorb "a")
         (funcall 'neovm--sim-run nfa-aorb "b")
         (funcall 'neovm--sim-run nfa-aorb "c")
         (funcall 'neovm--sim-run nfa-aorb "ab")
         (funcall 'neovm--sim-run nfa-aorb "")
         ;; "a*" matches
         (funcall 'neovm--sim-run nfa-astar "")
         (funcall 'neovm--sim-run nfa-astar "a")
         (funcall 'neovm--sim-run nfa-astar "aaa")
         (funcall 'neovm--sim-run nfa-astar "b")
         ;; "a*b" matches
         (funcall 'neovm--sim-run nfa-astarb "b")
         (funcall 'neovm--sim-run nfa-astarb "ab")
         (funcall 'neovm--sim-run nfa-astarb "aaab")
         (funcall 'neovm--sim-run nfa-astarb "a")
         (funcall 'neovm--sim-run nfa-astarb "")
         ;; "." matches
         (funcall 'neovm--sim-run nfa-dot "x")
         (funcall 'neovm--sim-run nfa-dot "9")
         (funcall 'neovm--sim-run nfa-dot "")
         (funcall 'neovm--sim-run nfa-dot "ab"))
      (progn
        (makunbound 'neovm--sim-ctr)
        (fmakunbound 'neovm--sim-id) (fmakunbound 'neovm--sim-st)
        (fmakunbound 'neovm--sim-mk) (fmakunbound 'neovm--sim-find)
        (fmakunbound 'neovm--sim-add-trans) (fmakunbound 'neovm--sim-closure)
        (fmakunbound 'neovm--sim-move) (fmakunbound 'neovm--sim-run)
        (fmakunbound 'neovm--sim-lit) (fmakunbound 'neovm--sim-dot)
        (fmakunbound 'neovm--sim-cat) (fmakunbound 'neovm--sim-alt)
        (fmakunbound 'neovm--sim-kleene)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil nil nil nil t t nil nil nil t t t nil t t t nil nil t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character classes: [abc], [a-z]
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_character_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--cc-ctr 0)
  (fset 'neovm--cc-id (lambda () (setq neovm--cc-ctr (1+ neovm--cc-ctr)) neovm--cc-ctr))
  (fset 'neovm--cc-st (lambda (id tr) (list id tr)))
  (fset 'neovm--cc-mk (lambda (sts s a) (list sts s a)))
  (fset 'neovm--cc-find
    (lambda (states id)
      (let ((found nil)) (dolist (s states) (when (= (car s) id) (setq found s))) found)))
  (fset 'neovm--cc-add-trans
    (lambda (state label target)
      (funcall 'neovm--cc-st (car state) (cons (list label target) (cadr state)))))
  (fset 'neovm--cc-closure
    (lambda (states id-set)
      (let ((stack (copy-sequence id-set)) (visited nil))
        (while stack
          (let ((cur (car stack)))
            (setq stack (cdr stack))
            (unless (memq cur visited)
              (setq visited (cons cur visited))
              (let ((st (funcall 'neovm--cc-find states cur)))
                (when st (dolist (tr (cadr st))
                           (when (eq (car tr) 'eps)
                             (unless (memq (cadr tr) visited)
                               (setq stack (cons (cadr tr) stack))))))))))
        (sort visited '<))))
  (fset 'neovm--cc-move
    (lambda (states current-ids ch)
      (let ((next nil))
        (dolist (id current-ids)
          (let ((st (funcall 'neovm--cc-find states id)))
            (when st
              (dolist (tr (cadr st))
                (let ((label (car tr)))
                  (when (or (and (integerp label) (= label ch))
                            (eq label 'any)
                            ;; Character class: (class . chars-list)
                            (and (consp label) (eq (car label) 'class)
                                 (memq ch (cdr label))))
                    (unless (memq (cadr tr) next)
                      (setq next (cons (cadr tr) next)))))))))
        next)))
  (fset 'neovm--cc-run
    (lambda (nfa str)
      (let* ((states (car nfa)) (start (cadr nfa)) (accept (caddr nfa))
             (current (funcall 'neovm--cc-closure states (list start)))
             (i 0) (len (length str)))
        (while (< i len)
          (let* ((ch (aref str i))
                 (moved (funcall 'neovm--cc-move states current ch))
                 (next (funcall 'neovm--cc-closure states moved)))
            (setq current next) (setq i (1+ i))))
        (if (memq accept current) t nil))))

  ;; Build NFA for character class [abc]: single transition with class label
  (fset 'neovm--cc-charclass
    (lambda (chars)
      (let ((s (funcall 'neovm--cc-id)) (a (funcall 'neovm--cc-id)))
        (funcall 'neovm--cc-mk
                 (list (funcall 'neovm--cc-st s (list (list (cons 'class chars) a)))
                       (funcall 'neovm--cc-st a nil)) s a))))

  ;; Build NFA for character range [a-z]
  (fset 'neovm--cc-charrange
    (lambda (from to)
      (let ((chars nil) (c from))
        (while (<= c to)
          (setq chars (cons c chars))
          (setq c (1+ c)))
        (funcall 'neovm--cc-charclass (nreverse chars)))))

  ;; Concat helper
  (fset 'neovm--cc-cat
    (lambda (na nb)
      (let* ((sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (acc (funcall 'neovm--cc-find sa aa))
             (nacc (funcall 'neovm--cc-add-trans acc 'eps stb))
             (ns (append (mapcar (lambda (s) (if (= (car s) aa) nacc s)) sa) sb)))
        (funcall 'neovm--cc-mk ns sta ab))))
  (fset 'neovm--cc-lit
    (lambda (ch)
      (let ((s (funcall 'neovm--cc-id)) (a (funcall 'neovm--cc-id)))
        (funcall 'neovm--cc-mk
                 (list (funcall 'neovm--cc-st s (list (list ch a)))
                       (funcall 'neovm--cc-st a nil)) s a))))

  (setq neovm--cc-ctr 0)
  (let* (;; [abc]
         (nfa-abc (funcall 'neovm--cc-charclass (list ?a ?b ?c)))
         ;; [a-z]
         (nfa-az (funcall 'neovm--cc-charrange ?a ?z))
         ;; [0-9]
         (nfa-digits (funcall 'neovm--cc-charrange ?0 ?9))
         ;; [a-z][0-9]: letter followed by digit
         (nfa-ld (funcall 'neovm--cc-cat
                          (funcall 'neovm--cc-charrange ?a ?z)
                          (funcall 'neovm--cc-charrange ?0 ?9))))
    (unwind-protect
        (list
         ;; [abc] tests
         (funcall 'neovm--cc-run nfa-abc "a")
         (funcall 'neovm--cc-run nfa-abc "b")
         (funcall 'neovm--cc-run nfa-abc "c")
         (funcall 'neovm--cc-run nfa-abc "d")
         (funcall 'neovm--cc-run nfa-abc "")
         (funcall 'neovm--cc-run nfa-abc "ab")
         ;; [a-z] tests
         (funcall 'neovm--cc-run nfa-az "a")
         (funcall 'neovm--cc-run nfa-az "m")
         (funcall 'neovm--cc-run nfa-az "z")
         (funcall 'neovm--cc-run nfa-az "A")
         (funcall 'neovm--cc-run nfa-az "0")
         ;; [0-9] tests
         (funcall 'neovm--cc-run nfa-digits "0")
         (funcall 'neovm--cc-run nfa-digits "5")
         (funcall 'neovm--cc-run nfa-digits "9")
         (funcall 'neovm--cc-run nfa-digits "a")
         ;; [a-z][0-9] tests
         (funcall 'neovm--cc-run nfa-ld "a1")
         (funcall 'neovm--cc-run nfa-ld "z9")
         (funcall 'neovm--cc-run nfa-ld "a")
         (funcall 'neovm--cc-run nfa-ld "1a")
         (funcall 'neovm--cc-run nfa-ld "ab"))
      (progn
        (makunbound 'neovm--cc-ctr)
        (fmakunbound 'neovm--cc-id) (fmakunbound 'neovm--cc-st)
        (fmakunbound 'neovm--cc-mk) (fmakunbound 'neovm--cc-find)
        (fmakunbound 'neovm--cc-add-trans) (fmakunbound 'neovm--cc-closure)
        (fmakunbound 'neovm--cc-move) (fmakunbound 'neovm--cc-run)
        (fmakunbound 'neovm--cc-charclass) (fmakunbound 'neovm--cc-charrange)
        (fmakunbound 'neovm--cc-cat) (fmakunbound 'neovm--cc-lit)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil nil nil t t t nil nil t t t nil t t nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full regex compiler: parse regex string, compile to NFA, execute
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_full_compiler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rc-ctr 0)
  (fset 'neovm--rc-id (lambda () (setq neovm--rc-ctr (1+ neovm--rc-ctr)) neovm--rc-ctr))
  (fset 'neovm--rc-st (lambda (id tr) (list id tr)))
  (fset 'neovm--rc-mk (lambda (sts s a) (list sts s a)))
  (fset 'neovm--rc-find
    (lambda (states id)
      (let ((found nil)) (dolist (s states) (when (= (car s) id) (setq found s))) found)))
  (fset 'neovm--rc-add-trans
    (lambda (state label target)
      (funcall 'neovm--rc-st (car state) (cons (list label target) (cadr state)))))
  (fset 'neovm--rc-closure
    (lambda (states id-set)
      (let ((stack (copy-sequence id-set)) (visited nil))
        (while stack
          (let ((cur (car stack)))
            (setq stack (cdr stack))
            (unless (memq cur visited)
              (setq visited (cons cur visited))
              (let ((st (funcall 'neovm--rc-find states cur)))
                (when st (dolist (tr (cadr st))
                           (when (eq (car tr) 'eps)
                             (unless (memq (cadr tr) visited)
                               (setq stack (cons (cadr tr) stack))))))))))
        (sort visited '<))))
  (fset 'neovm--rc-move
    (lambda (states current-ids ch)
      (let ((next nil))
        (dolist (id current-ids)
          (let ((st (funcall 'neovm--rc-find states id)))
            (when st (dolist (tr (cadr st))
                       (when (or (and (integerp (car tr)) (= (car tr) ch))
                                 (eq (car tr) 'any))
                         (unless (memq (cadr tr) next)
                           (setq next (cons (cadr tr) next))))))))
        next)))
  (fset 'neovm--rc-run
    (lambda (nfa str)
      (let* ((states (car nfa)) (start (cadr nfa)) (accept (caddr nfa))
             (current (funcall 'neovm--rc-closure states (list start)))
             (i 0) (len (length str)))
        (while (< i len)
          (let* ((ch (aref str i))
                 (moved (funcall 'neovm--rc-move states current ch))
                 (next (funcall 'neovm--rc-closure states moved)))
            (setq current next) (setq i (1+ i))))
        (if (memq accept current) t nil))))

  ;; NFA builders
  (fset 'neovm--rc-lit
    (lambda (ch)
      (let ((s (funcall 'neovm--rc-id)) (a (funcall 'neovm--rc-id)))
        (funcall 'neovm--rc-mk (list (funcall 'neovm--rc-st s (list (list ch a)))
                                     (funcall 'neovm--rc-st a nil)) s a))))
  (fset 'neovm--rc-dot
    (lambda ()
      (let ((s (funcall 'neovm--rc-id)) (a (funcall 'neovm--rc-id)))
        (funcall 'neovm--rc-mk (list (funcall 'neovm--rc-st s (list (list 'any a)))
                                     (funcall 'neovm--rc-st a nil)) s a))))
  (fset 'neovm--rc-cat
    (lambda (na nb)
      (let* ((sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (acc (funcall 'neovm--rc-find sa aa))
             (nacc (funcall 'neovm--rc-add-trans acc 'eps stb)))
        (funcall 'neovm--rc-mk (append (mapcar (lambda (s) (if (= (car s) aa) nacc s)) sa) sb) sta ab))))
  (fset 'neovm--rc-alt
    (lambda (na nb)
      (let* ((s (funcall 'neovm--rc-id)) (a (funcall 'neovm--rc-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (ac-a (funcall 'neovm--rc-add-trans (funcall 'neovm--rc-find sa aa) 'eps a))
             (ac-b (funcall 'neovm--rc-add-trans (funcall 'neovm--rc-find sb ab) 'eps a)))
        (funcall 'neovm--rc-mk
                 (append (list (funcall 'neovm--rc-st s (list (list 'eps sta) (list 'eps stb)))
                               (funcall 'neovm--rc-st a nil))
                         (mapcar (lambda (s) (if (= (car s) aa) ac-a s)) sa)
                         (mapcar (lambda (s) (if (= (car s) ab) ac-b s)) sb))
                 s a))))
  (fset 'neovm--rc-star
    (lambda (na)
      (let* ((s (funcall 'neovm--rc-id)) (a (funcall 'neovm--rc-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (acc (funcall 'neovm--rc-find sa aa))
             (nacc (funcall 'neovm--rc-st (car acc) (cons (list 'eps sta) (cons (list 'eps a) (cadr acc))))))
        (funcall 'neovm--rc-mk
                 (append (list (funcall 'neovm--rc-st s (list (list 'eps sta) (list 'eps a)))
                               (funcall 'neovm--rc-st a nil))
                         (mapcar (lambda (st) (if (= (car st) aa) nacc st)) sa))
                 s a))))
  (fset 'neovm--rc-plus
    (lambda (na)
      ;; a+ = aa*
      (let ((copy-a na)
            (star-a (funcall 'neovm--rc-star na)))
        ;; We need a fresh copy... simplified: just build cat(lit, star)
        ;; Actually: for a+, start -> a-start, a-accept -> a-start (loop) and -> new-accept
        (let* ((s2 (funcall 'neovm--rc-id)) (a2 (funcall 'neovm--rc-id))
               (sa (car na)) (sta (cadr na)) (aa (caddr na))
               (acc (funcall 'neovm--rc-find sa aa))
               (nacc (funcall 'neovm--rc-st (car acc) (cons (list 'eps sta) (cons (list 'eps a2) (cadr acc))))))
          (funcall 'neovm--rc-mk
                   (append (list (funcall 'neovm--rc-st s2 (list (list 'eps sta)))
                                 (funcall 'neovm--rc-st a2 nil))
                           (mapcar (lambda (st) (if (= (car st) aa) nacc st)) sa))
                   s2 a2)))))

  ;; Simple regex parser: supports literals, ., *, +, |, ()
  ;; Returns an NFA
  (fset 'neovm--rc-parse
    (lambda (pattern)
      (let ((pos 0) (len (length pattern)))
        ;; Parse alternation (lowest precedence)
        (fset 'neovm--rc-parse-alt
          (lambda ()
            (let ((left (funcall 'neovm--rc-parse-seq)))
              (while (and (< pos len) (= (aref pattern pos) ?|))
                (setq pos (1+ pos))
                (let ((right (funcall 'neovm--rc-parse-seq)))
                  (setq left (funcall 'neovm--rc-alt left right))))
              left)))
        ;; Parse sequence (concatenation)
        (fset 'neovm--rc-parse-seq
          (lambda ()
            (let ((result nil))
              (while (and (< pos len)
                          (not (= (aref pattern pos) ?|))
                          (not (= (aref pattern pos) ?\))))
                (let ((atom (funcall 'neovm--rc-parse-atom)))
                  (setq result (if result (funcall 'neovm--rc-cat result atom) atom))))
              (or result
                  ;; Empty regex: matches empty string
                  (let ((s (funcall 'neovm--rc-id)) (a (funcall 'neovm--rc-id)))
                    (funcall 'neovm--rc-mk
                             (list (funcall 'neovm--rc-st s (list (list 'eps a)))
                                   (funcall 'neovm--rc-st a nil)) s a))))))
        ;; Parse atom with postfix operators
        (fset 'neovm--rc-parse-atom
          (lambda ()
            (let ((base
                   (cond
                    ((= (aref pattern pos) ?\()
                     (setq pos (1+ pos))
                     (let ((inner (funcall 'neovm--rc-parse-alt)))
                       (when (and (< pos len) (= (aref pattern pos) ?\)))
                         (setq pos (1+ pos)))
                       inner))
                    ((= (aref pattern pos) ?.)
                     (setq pos (1+ pos))
                     (funcall 'neovm--rc-dot))
                    (t
                     (let ((ch (aref pattern pos)))
                       (setq pos (1+ pos))
                       (funcall 'neovm--rc-lit ch))))))
              ;; Postfix operators
              (while (and (< pos len)
                          (memq (aref pattern pos) '(?* ?+)))
                (let ((op (aref pattern pos)))
                  (setq pos (1+ pos))
                  (cond ((= op ?*) (setq base (funcall 'neovm--rc-star base)))
                        ((= op ?+) (setq base (funcall 'neovm--rc-plus base))))))
              base)))
        (funcall 'neovm--rc-parse-alt))))

  ;; Test the full pipeline
  (setq neovm--rc-ctr 0)
  (let ((results nil))
    ;; "ab" - literal concat
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse "ab")))
      (setq results (cons (list "ab"
                                (funcall 'neovm--rc-run nfa "ab")
                                (funcall 'neovm--rc-run nfa "a")
                                (funcall 'neovm--rc-run nfa ""))
                          results)))
    ;; "a|b"
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse "a|b")))
      (setq results (cons (list "a|b"
                                (funcall 'neovm--rc-run nfa "a")
                                (funcall 'neovm--rc-run nfa "b")
                                (funcall 'neovm--rc-run nfa "c"))
                          results)))
    ;; "a*"
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse "a*")))
      (setq results (cons (list "a*"
                                (funcall 'neovm--rc-run nfa "")
                                (funcall 'neovm--rc-run nfa "a")
                                (funcall 'neovm--rc-run nfa "aaa")
                                (funcall 'neovm--rc-run nfa "b"))
                          results)))
    ;; "a+"
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse "a+")))
      (setq results (cons (list "a+"
                                (funcall 'neovm--rc-run nfa "")
                                (funcall 'neovm--rc-run nfa "a")
                                (funcall 'neovm--rc-run nfa "aaa")
                                (funcall 'neovm--rc-run nfa "b"))
                          results)))
    ;; "(a|b)*"
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse "(a|b)*")))
      (setq results (cons (list "(a|b)*"
                                (funcall 'neovm--rc-run nfa "")
                                (funcall 'neovm--rc-run nfa "a")
                                (funcall 'neovm--rc-run nfa "ababba")
                                (funcall 'neovm--rc-run nfa "c"))
                          results)))
    ;; ".*" - match anything
    (setq neovm--rc-ctr 0)
    (let ((nfa (funcall 'neovm--rc-parse ".*")))
      (setq results (cons (list ".*"
                                (funcall 'neovm--rc-run nfa "")
                                (funcall 'neovm--rc-run nfa "anything")
                                (funcall 'neovm--rc-run nfa "123"))
                          results)))
    (unwind-protect
        (nreverse results)
      (progn
        (makunbound 'neovm--rc-ctr)
        (fmakunbound 'neovm--rc-id) (fmakunbound 'neovm--rc-st)
        (fmakunbound 'neovm--rc-mk) (fmakunbound 'neovm--rc-find)
        (fmakunbound 'neovm--rc-add-trans) (fmakunbound 'neovm--rc-closure)
        (fmakunbound 'neovm--rc-move) (fmakunbound 'neovm--rc-run)
        (fmakunbound 'neovm--rc-lit) (fmakunbound 'neovm--rc-dot)
        (fmakunbound 'neovm--rc-cat) (fmakunbound 'neovm--rc-alt)
        (fmakunbound 'neovm--rc-star) (fmakunbound 'neovm--rc-plus)
        (fmakunbound 'neovm--rc-parse) (fmakunbound 'neovm--rc-parse-alt)
        (fmakunbound 'neovm--rc-parse-seq) (fmakunbound 'neovm--rc-parse-atom)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"ab\" t nil nil) (\"a|b\" t t nil) (\"a*\" t t t nil) (\"a+\" nil t t nil) (\"(a|b)*\" t t t nil) (\".*\" t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex regex patterns: nested groups, alternation with concat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_nfa_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--cx-ctr 0)
  (fset 'neovm--cx-id (lambda () (setq neovm--cx-ctr (1+ neovm--cx-ctr)) neovm--cx-ctr))
  (fset 'neovm--cx-st (lambda (id tr) (list id tr)))
  (fset 'neovm--cx-mk (lambda (sts s a) (list sts s a)))
  (fset 'neovm--cx-find
    (lambda (states id)
      (let ((found nil)) (dolist (s states) (when (= (car s) id) (setq found s))) found)))
  (fset 'neovm--cx-add-trans
    (lambda (state label target)
      (funcall 'neovm--cx-st (car state) (cons (list label target) (cadr state)))))
  (fset 'neovm--cx-closure
    (lambda (states id-set)
      (let ((stack (copy-sequence id-set)) (visited nil))
        (while stack
          (let ((cur (car stack)))
            (setq stack (cdr stack))
            (unless (memq cur visited)
              (setq visited (cons cur visited))
              (let ((st (funcall 'neovm--cx-find states cur)))
                (when st (dolist (tr (cadr st))
                           (when (eq (car tr) 'eps)
                             (unless (memq (cadr tr) visited)
                               (setq stack (cons (cadr tr) stack))))))))))
        (sort visited '<))))
  (fset 'neovm--cx-move
    (lambda (states current-ids ch)
      (let ((next nil))
        (dolist (id current-ids)
          (let ((st (funcall 'neovm--cx-find states id)))
            (when st (dolist (tr (cadr st))
                       (when (or (and (integerp (car tr)) (= (car tr) ch))
                                 (eq (car tr) 'any))
                         (unless (memq (cadr tr) next)
                           (setq next (cons (cadr tr) next))))))))
        next)))
  (fset 'neovm--cx-run
    (lambda (nfa str)
      (let* ((states (car nfa)) (start (cadr nfa)) (accept (caddr nfa))
             (current (funcall 'neovm--cx-closure states (list start)))
             (i 0) (len (length str)))
        (while (< i len)
          (let* ((ch (aref str i))
                 (moved (funcall 'neovm--cx-move states current ch))
                 (next (funcall 'neovm--cx-closure states moved)))
            (setq current next) (setq i (1+ i))))
        (if (memq accept current) t nil))))
  (fset 'neovm--cx-lit
    (lambda (ch)
      (let ((s (funcall 'neovm--cx-id)) (a (funcall 'neovm--cx-id)))
        (funcall 'neovm--cx-mk (list (funcall 'neovm--cx-st s (list (list ch a)))
                                     (funcall 'neovm--cx-st a nil)) s a))))
  (fset 'neovm--cx-dot
    (lambda ()
      (let ((s (funcall 'neovm--cx-id)) (a (funcall 'neovm--cx-id)))
        (funcall 'neovm--cx-mk (list (funcall 'neovm--cx-st s (list (list 'any a)))
                                     (funcall 'neovm--cx-st a nil)) s a))))
  (fset 'neovm--cx-cat
    (lambda (na nb)
      (let* ((sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (acc (funcall 'neovm--cx-find sa aa))
             (nacc (funcall 'neovm--cx-add-trans acc 'eps stb)))
        (funcall 'neovm--cx-mk (append (mapcar (lambda (s) (if (= (car s) aa) nacc s)) sa) sb) sta ab))))
  (fset 'neovm--cx-alt
    (lambda (na nb)
      (let* ((s (funcall 'neovm--cx-id)) (a (funcall 'neovm--cx-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (sb (car nb)) (stb (cadr nb)) (ab (caddr nb))
             (ac-a (funcall 'neovm--cx-add-trans (funcall 'neovm--cx-find sa aa) 'eps a))
             (ac-b (funcall 'neovm--cx-add-trans (funcall 'neovm--cx-find sb ab) 'eps a)))
        (funcall 'neovm--cx-mk
                 (append (list (funcall 'neovm--cx-st s (list (list 'eps sta) (list 'eps stb)))
                               (funcall 'neovm--cx-st a nil))
                         (mapcar (lambda (s) (if (= (car s) aa) ac-a s)) sa)
                         (mapcar (lambda (s) (if (= (car s) ab) ac-b s)) sb)
                         ) s a))))
  (fset 'neovm--cx-star
    (lambda (na)
      (let* ((s (funcall 'neovm--cx-id)) (a (funcall 'neovm--cx-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (acc (funcall 'neovm--cx-find sa aa))
             (nacc (funcall 'neovm--cx-st (car acc) (cons (list 'eps sta) (cons (list 'eps a) (cadr acc))))))
        (funcall 'neovm--cx-mk
                 (append (list (funcall 'neovm--cx-st s (list (list 'eps sta) (list 'eps a)))
                               (funcall 'neovm--cx-st a nil))
                         (mapcar (lambda (st) (if (= (car st) aa) nacc st)) sa))
                 s a))))
  (fset 'neovm--cx-plus
    (lambda (na)
      (let* ((s2 (funcall 'neovm--cx-id)) (a2 (funcall 'neovm--cx-id))
             (sa (car na)) (sta (cadr na)) (aa (caddr na))
             (acc (funcall 'neovm--cx-find sa aa))
             (nacc (funcall 'neovm--cx-st (car acc) (cons (list 'eps sta) (cons (list 'eps a2) (cadr acc))))))
        (funcall 'neovm--cx-mk
                 (append (list (funcall 'neovm--cx-st s2 (list (list 'eps sta)))
                               (funcall 'neovm--cx-st a2 nil))
                         (mapcar (lambda (st) (if (= (car st) aa) nacc st)) sa))
                 s2 a2))))
  (fset 'neovm--cx-parse
    (lambda (pattern)
      (let ((pos 0) (len (length pattern)))
        (fset 'neovm--cx-parse-alt
          (lambda ()
            (let ((left (funcall 'neovm--cx-parse-seq)))
              (while (and (< pos len) (= (aref pattern pos) ?|))
                (setq pos (1+ pos))
                (setq left (funcall 'neovm--cx-alt left (funcall 'neovm--cx-parse-seq))))
              left)))
        (fset 'neovm--cx-parse-seq
          (lambda ()
            (let ((result nil))
              (while (and (< pos len) (not (= (aref pattern pos) ?|)) (not (= (aref pattern pos) ?\))))
                (let ((atom (funcall 'neovm--cx-parse-atom)))
                  (setq result (if result (funcall 'neovm--cx-cat result atom) atom))))
              (or result (let ((s (funcall 'neovm--cx-id)) (a (funcall 'neovm--cx-id)))
                           (funcall 'neovm--cx-mk (list (funcall 'neovm--cx-st s (list (list 'eps a)))
                                                        (funcall 'neovm--cx-st a nil)) s a))))))
        (fset 'neovm--cx-parse-atom
          (lambda ()
            (let ((base (cond
                         ((= (aref pattern pos) ?\()
                          (setq pos (1+ pos))
                          (let ((inner (funcall 'neovm--cx-parse-alt)))
                            (when (and (< pos len) (= (aref pattern pos) ?\))) (setq pos (1+ pos)))
                            inner))
                         ((= (aref pattern pos) ?.) (setq pos (1+ pos)) (funcall 'neovm--cx-dot))
                         (t (let ((ch (aref pattern pos))) (setq pos (1+ pos)) (funcall 'neovm--cx-lit ch))))))
              (while (and (< pos len) (memq (aref pattern pos) '(?* ?+)))
                (let ((op (aref pattern pos)))
                  (setq pos (1+ pos))
                  (cond ((= op ?*) (setq base (funcall 'neovm--cx-star base)))
                        ((= op ?+) (setq base (funcall 'neovm--cx-plus base))))))
              base)))
        (funcall 'neovm--cx-parse-alt))))

  ;; Complex pattern tests
  (let ((results nil))
    ;; "a(b|c)d" - group with alternation inside concat
    (setq neovm--cx-ctr 0)
    (let ((nfa (funcall 'neovm--cx-parse "a(b|c)d")))
      (setq results (cons (list "a(b|c)d"
                                (funcall 'neovm--cx-run nfa "abd")
                                (funcall 'neovm--cx-run nfa "acd")
                                (funcall 'neovm--cx-run nfa "ad")
                                (funcall 'neovm--cx-run nfa "abcd"))
                          results)))
    ;; "(ab)+" - one or more "ab"
    (setq neovm--cx-ctr 0)
    (let ((nfa (funcall 'neovm--cx-parse "(ab)+")))
      (setq results (cons (list "(ab)+"
                                (funcall 'neovm--cx-run nfa "ab")
                                (funcall 'neovm--cx-run nfa "abab")
                                (funcall 'neovm--cx-run nfa "ababab")
                                (funcall 'neovm--cx-run nfa "")
                                (funcall 'neovm--cx-run nfa "a"))
                          results)))
    ;; "a.b" - any char between a and b
    (setq neovm--cx-ctr 0)
    (let ((nfa (funcall 'neovm--cx-parse "a.b")))
      (setq results (cons (list "a.b"
                                (funcall 'neovm--cx-run nfa "axb")
                                (funcall 'neovm--cx-run nfa "a1b")
                                (funcall 'neovm--cx-run nfa "ab")
                                (funcall 'neovm--cx-run nfa "axxb"))
                          results)))
    ;; "(a|b|c)*d" - zero or more a/b/c followed by d
    (setq neovm--cx-ctr 0)
    (let ((nfa (funcall 'neovm--cx-parse "(a|b|c)*d")))
      (setq results (cons (list "(a|b|c)*d"
                                (funcall 'neovm--cx-run nfa "d")
                                (funcall 'neovm--cx-run nfa "ad")
                                (funcall 'neovm--cx-run nfa "abcd")
                                (funcall 'neovm--cx-run nfa "cbad")
                                (funcall 'neovm--cx-run nfa "abc")
                                (funcall 'neovm--cx-run nfa ""))
                          results)))
    ;; "x(y|z)*x" - x then any mix of y/z then x
    (setq neovm--cx-ctr 0)
    (let ((nfa (funcall 'neovm--cx-parse "x(y|z)*x")))
      (setq results (cons (list "x(y|z)*x"
                                (funcall 'neovm--cx-run nfa "xx")
                                (funcall 'neovm--cx-run nfa "xyx")
                                (funcall 'neovm--cx-run nfa "xyzyzx")
                                (funcall 'neovm--cx-run nfa "x")
                                (funcall 'neovm--cx-run nfa "xax"))
                          results)))
    (unwind-protect
        (nreverse results)
      (progn
        (makunbound 'neovm--cx-ctr)
        (fmakunbound 'neovm--cx-id) (fmakunbound 'neovm--cx-st)
        (fmakunbound 'neovm--cx-mk) (fmakunbound 'neovm--cx-find)
        (fmakunbound 'neovm--cx-add-trans) (fmakunbound 'neovm--cx-closure)
        (fmakunbound 'neovm--cx-move) (fmakunbound 'neovm--cx-run)
        (fmakunbound 'neovm--cx-lit) (fmakunbound 'neovm--cx-dot)
        (fmakunbound 'neovm--cx-cat) (fmakunbound 'neovm--cx-alt)
        (fmakunbound 'neovm--cx-star) (fmakunbound 'neovm--cx-plus)
        (fmakunbound 'neovm--cx-parse) (fmakunbound 'neovm--cx-parse-alt)
        (fmakunbound 'neovm--cx-parse-seq) (fmakunbound 'neovm--cx-parse-atom)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a(b|c)d\" t t nil nil) (\"(ab)+\" t t t nil nil) (\"a.b\" t t nil nil) (\"(a|b|c)*d\" t t t t nil nil) (\"x(y|z)*x\" t t t nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
