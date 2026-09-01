//! Oracle parity tests for a text-based railroad diagram generator.
//!
//! Implements a railroad diagram renderer for a simple grammar in Elisp.
//! Supports terminal nodes, non-terminal references, sequences, choices,
//! optional elements, and repetition. Renders to ASCII art.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Terminal and non-terminal representation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_terminal_nonterminal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar nodes: (terminal "text") and (nonterminal "name")
    // Render terminal as: --[ text ]-->
    // Render nonterminal as: --< name >-->
    let form = r#"(progn
  (fset 'neovm--rr-render-node
    (lambda (node)
      "Render a single grammar node to a string."
      (let ((type (car node))
            (text (cadr node)))
        (cond
          ((eq type 'terminal)
           (concat "--[ " text " ]-->"))
          ((eq type 'nonterminal)
           (concat "--< " text " >-->"))
          ((eq type 'epsilon)
           "------>")
          (t (concat "--[?" (format "%s" type) "?]-->"))))))

  (fset 'neovm--rr-node-width
    (lambda (node)
      "Compute the display width of a rendered node."
      (length (funcall 'neovm--rr-render-node node))))

  (unwind-protect
      (list
        ;; Terminal rendering
        (funcall 'neovm--rr-render-node '(terminal "if"))
        (funcall 'neovm--rr-render-node '(terminal "+"))
        (funcall 'neovm--rr-render-node '(terminal "123"))
        ;; Non-terminal rendering
        (funcall 'neovm--rr-render-node '(nonterminal "expr"))
        (funcall 'neovm--rr-render-node '(nonterminal "statement"))
        ;; Epsilon (empty)
        (funcall 'neovm--rr-render-node '(epsilon))
        ;; Widths
        (funcall 'neovm--rr-node-width '(terminal "x"))
        (funcall 'neovm--rr-node-width '(nonterminal "expr"))
        ;; Width comparison: longer text -> wider node
        (< (funcall 'neovm--rr-node-width '(terminal "a"))
           (funcall 'neovm--rr-node-width '(terminal "abcdef"))))
    (fmakunbound 'neovm--rr-render-node)
    (fmakunbound 'neovm--rr-node-width)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"--[ if ]-->\" \"--[ + ]-->\" \"--[ 123 ]-->\" \"--< expr >-->\" \"--< statement >-->\" \"------>\" 10 13 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequence: A then B then C
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rrs-render-node
    (lambda (node)
      (let ((type (car node))
            (text (cadr node)))
        (cond
          ((eq type 'terminal) (concat "[ " text " ]"))
          ((eq type 'nonterminal) (concat "< " text " >"))
          ((eq type 'epsilon) "---")
          (t "???")))))

  (fset 'neovm--rrs-render-sequence
    (lambda (nodes)
      "Render a sequence of nodes connected by arrows."
      (concat "--"
              (mapconcat (lambda (n)
                           (funcall 'neovm--rrs-render-node n))
                         nodes
                         "--")
              "-->")))

  (unwind-protect
      (list
        ;; Simple two-element sequence
        (funcall 'neovm--rrs-render-sequence
                 '((terminal "if") (nonterminal "expr")))
        ;; Three-element sequence
        (funcall 'neovm--rrs-render-sequence
                 '((terminal "while") (terminal "(") (nonterminal "cond")))
        ;; Single element
        (funcall 'neovm--rrs-render-sequence
                 '((terminal "return")))
        ;; Longer sequence for a full statement
        (funcall 'neovm--rrs-render-sequence
                 '((terminal "for") (terminal "(")
                   (nonterminal "init") (terminal ";")
                   (nonterminal "cond") (terminal ";")
                   (nonterminal "step") (terminal ")")))
        ;; Empty sequence
        (funcall 'neovm--rrs-render-sequence nil))
    (fmakunbound 'neovm--rrs-render-node)
    (fmakunbound 'neovm--rrs-render-sequence)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"--[ if ]--< expr >-->\" \"--[ while ]--[ ( ]--< cond >-->\" \"--[ return ]-->\" \"--[ for ]--[ ( ]--< init >--[ ; ]--< cond >--[ ; ]--< step >--[ ) ]-->\" \"---->\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Choice: A or B (vertically stacked alternatives)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_choice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rrc-render-node
    (lambda (node)
      (let ((type (car node)))
        (cond
          ((eq type 'terminal) (concat "[ " (cadr node) " ]"))
          ((eq type 'nonterminal) (concat "< " (cadr node) " >"))
          ((eq type 'epsilon) "---")
          (t "???")))))

  (fset 'neovm--rrc-pad-to
    (lambda (s width)
      "Pad string S with spaces on the right to WIDTH."
      (let ((len (length s)))
        (if (>= len width) s
          (concat s (make-string (- width len) ? ))))))

  (fset 'neovm--rrc-render-choice
    (lambda (alternatives)
      "Render alternatives as a choice diagram (list of lines)."
      (let* ((rendered (mapcar (lambda (a) (funcall 'neovm--rrc-render-node a))
                               alternatives))
             (max-w (apply #'max (mapcar #'length rendered)))
             (lines nil)
             (n (length rendered))
             (i 0))
        ;; Top line with branch
        (setq lines (cons (concat "--+-" (funcall 'neovm--rrc-pad-to (nth 0 rendered) max-w) "-+-->") lines))
        ;; Middle alternatives
        (setq i 1)
        (while (< i (1- n))
          (setq lines (cons (concat "  +-" (funcall 'neovm--rrc-pad-to (nth i rendered) max-w) "-+") lines))
          (setq i (1+ i)))
        ;; Bottom line (if more than 1)
        (when (> n 1)
          (setq lines (cons (concat "  +-" (funcall 'neovm--rrc-pad-to (nth (1- n) rendered) max-w) "-+") lines)))
        (nreverse lines))))

  (unwind-protect
      (list
        ;; Two alternatives
        (funcall 'neovm--rrc-render-choice
                 '((terminal "+") (terminal "-")))
        ;; Three alternatives
        (funcall 'neovm--rrc-render-choice
                 '((terminal "int") (terminal "float") (terminal "string")))
        ;; Mixed terminal and nonterminal
        (funcall 'neovm--rrc-render-choice
                 '((terminal "null") (nonterminal "expr")))
        ;; Single alternative (degenerate)
        (funcall 'neovm--rrc-render-choice
                 '((terminal "only"))))
    (fmakunbound 'neovm--rrc-render-node)
    (fmakunbound 'neovm--rrc-pad-to)
    (fmakunbound 'neovm--rrc-render-choice)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"--+-[ + ]-+-->\" \"  +-[ - ]-+\") (\"--+-[ int ]   -+-->\" \"  +-[ float ] -+\" \"  +-[ string ]-+\") (\"--+-[ null ]-+-->\" \"  +-< expr >-+\") (\"--+-[ only ]-+-->\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Optional (A?) and Repetition (A+, A*)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_optional_repetition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rro-render-node
    (lambda (node)
      (let ((type (car node)))
        (cond
          ((eq type 'terminal) (concat "[ " (cadr node) " ]"))
          ((eq type 'nonterminal) (concat "< " (cadr node) " >"))
          ((eq type 'epsilon) "---")
          (t "???")))))

  (fset 'neovm--rro-pad
    (lambda (s w) (if (>= (length s) w) s (concat s (make-string (- w (length s)) ? )))))

  (fset 'neovm--rro-render-optional
    (lambda (node)
      "Render optional element: main path or bypass."
      (let* ((rendered (funcall 'neovm--rro-render-node node))
             (w (length rendered))
             (bypass (funcall 'neovm--rro-pad "---" w)))
        (list (concat "--+-" rendered "-+-->")
              (concat "  +-" bypass "-+")))))

  (fset 'neovm--rro-render-one-or-more
    (lambda (node)
      "Render A+ (one or more): forward path with loop back."
      (let* ((rendered (funcall 'neovm--rro-render-node node))
             (w (length rendered))
             (loop-line (make-string w ?-)))
        (list (concat "---" rendered "-+-->")
              (concat "  +-" loop-line "-+")
              (concat "    " (funcall 'neovm--rro-pad "(loop)" w))))))

  (fset 'neovm--rro-render-zero-or-more
    (lambda (node)
      "Render A* (zero or more): optional + loop."
      (let* ((rendered (funcall 'neovm--rro-render-node node))
             (w (length rendered))
             (bypass (funcall 'neovm--rro-pad "---" w)))
        (list (concat "--+-" rendered "-+-+-->")
              (concat "  |  " (make-string w ? ) " | ")
              (concat "  +--" bypass "-+-+")
              (concat "     " (funcall 'neovm--rro-pad "(loop)" w))))))

  (unwind-protect
      (list
        ;; Optional: expression?
        (funcall 'neovm--rro-render-optional '(nonterminal "expr"))
        ;; Optional: terminal
        (funcall 'neovm--rro-render-optional '(terminal "else"))
        ;; One-or-more: digit+
        (funcall 'neovm--rro-render-one-or-more '(terminal "digit"))
        ;; Zero-or-more: statement*
        (funcall 'neovm--rro-render-zero-or-more '(nonterminal "stmt"))
        ;; One-or-more: single char
        (funcall 'neovm--rro-render-one-or-more '(terminal "a")))
    (fmakunbound 'neovm--rro-render-node)
    (fmakunbound 'neovm--rro-pad)
    (fmakunbound 'neovm--rro-render-optional)
    (fmakunbound 'neovm--rro-render-one-or-more)
    (fmakunbound 'neovm--rro-render-zero-or-more)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"--+-< expr >-+-->\" \"  +----     -+\") (\"--+-[ else ]-+-->\" \"  +----     -+\") (\"---[ digit ]-+-->\" \"  +-----------+\" \"    (loop)   \") (\"--+-< stmt >-+-+-->\" \"  |           | \" \"  +-----     -+-+\" \"     (loop)  \") (\"---[ a ]-+-->\" \"  +-------+\" \"    (loop)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: render full ASCII art for a grammar rule
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_full_ascii_art() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complete diagram renderer that handles nested grammar
    // constructs and produces multi-line ASCII art.
    let form = r#"(progn
  (fset 'neovm--rrf-render
    (lambda (grammar-node)
      "Recursively render a grammar node to a list of strings (lines)."
      (let ((type (car grammar-node)))
        (cond
          ;; Terminal: --[ text ]-->
          ((eq type 'terminal)
           (list (concat "--[ " (cadr grammar-node) " ]-->")))

          ;; Non-terminal: --< name >-->
          ((eq type 'nonterminal)
           (list (concat "--< " (cadr grammar-node) " >-->")))

          ;; Sequence: join rendered parts horizontally
          ((eq type 'seq)
           (let ((parts (cdr grammar-node))
                 (result ""))
             (dolist (p parts)
               (let ((r (car (funcall 'neovm--rrf-render p))))
                 ;; Strip leading -- from subsequent parts for clean join
                 (if (string= result "")
                     (setq result r)
                   ;; Remove trailing --> from prev and leading -- from current
                   (let ((prev (substring result 0 (- (length result) 3)))
                         (curr (if (string-match "\\`--" r) (substring r 2) r)))
                     (setq result (concat prev curr))))))
             (list result)))

          ;; Choice: stack alternatives vertically
          ((eq type 'choice)
           (let* ((alts (cdr grammar-node))
                  (rendered (mapcar (lambda (a)
                                     (car (funcall 'neovm--rrf-render a)))
                                   alts))
                  ;; Strip --/-->  to get inner labels
                  (labels (mapcar (lambda (r)
                                   (if (and (string-match "\\`--" r)
                                            (string-match "-->\\'" r))
                                       (substring r 2 (- (length r) 3))
                                     r))
                                 rendered))
                  (max-w (apply #'max (mapcar #'length labels)))
                  (lines nil)
                  (i 0))
             (dolist (lab labels)
               (let ((padded (concat lab (make-string (max 0 (- max-w (length lab))) ? ))))
                 (if (= i 0)
                     (setq lines (cons (concat "--+-" padded "-+-->") lines))
                   (setq lines (cons (concat "  +-" padded "-+" (if (= i (1- (length labels))) "" "")) lines))))
               (setq i (1+ i)))
             (nreverse lines)))

          ;; Optional
          ((eq type 'optional)
           (let* ((inner (car (funcall 'neovm--rrf-render (cadr grammar-node))))
                  (lab (if (and (string-match "\\`--" inner)
                                (string-match "-->\\'" inner))
                           (substring inner 2 (- (length inner) 3))
                         inner))
                  (w (length lab))
                  (bypass (make-string w ?-)))
             (list (concat "--+-" lab "-+-->")
                   (concat "  +-" bypass "-+"))))

          (t (list (format "--%s-->" grammar-node)))))))

  (unwind-protect
      (list
        ;; Simple sequence: if ( expr )
        (funcall 'neovm--rrf-render
                 '(seq (terminal "if") (terminal "(") (nonterminal "expr") (terminal ")")))
        ;; Choice between terminals
        (funcall 'neovm--rrf-render
                 '(choice (terminal "+") (terminal "-") (terminal "*")))
        ;; Optional clause
        (funcall 'neovm--rrf-render
                 '(optional (terminal "else")))
        ;; Combined: if ( expr ) stmt [else stmt]
        (funcall 'neovm--rrf-render
                 '(seq (terminal "if") (terminal "(") (nonterminal "expr") (terminal ")")
                       (nonterminal "stmt")))
        ;; Choice of sequences
        (funcall 'neovm--rrf-render
                 '(choice (terminal "true") (terminal "false") (nonterminal "expr"))))
    (fmakunbound 'neovm--rrf-render)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"--[ if ][ ( ]< expr >[ ) ]-->\") (\"--+-[ + ]-+-->\" \"  +-[ - ]-+\" \"  +-[ * ]-+\") (\"--+-[ else ]-+-->\" \"  +----------+\") (\"--[ if ][ ( ]< expr >[ ) ]< stmt >-->\") (\"--+-[ true ] -+-->\" \"  +-[ false ]-+\" \"  +-< expr > -+\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: grammar for simple arithmetic expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_arithmetic_grammar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a grammar for arithmetic expressions and generate diagrams.
    // expr   -> term (('+' | '-') term)*
    // term   -> factor (('*' | '/') factor)*
    // factor -> NUMBER | '(' expr ')'
    let form = r#"(progn
  ;; Grammar representation as an alist
  (fset 'neovm--rrg-define-grammar
    (lambda ()
      (list
        (cons 'expr
              '(description "Expression: term followed by additive operations"
                components ((nonterminal "term")
                            (terminal "+")
                            (terminal "-"))))
        (cons 'term
              '(description "Term: factor followed by multiplicative operations"
                components ((nonterminal "factor")
                            (terminal "*")
                            (terminal "/"))))
        (cons 'factor
              '(description "Factor: number or parenthesized expression"
                components ((terminal "NUMBER")
                            (terminal "(")
                            (nonterminal "expr")
                            (terminal ")")))))))

  ;; Render a production rule diagram
  (fset 'neovm--rrg-render-rule
    (lambda (name components)
      "Render a named grammar rule."
      (let* ((header (concat "  " (symbol-name name) ":"))
             (parts (mapcar (lambda (c)
                              (let ((type (car c))
                                    (text (cadr c)))
                                (cond
                                  ((eq type 'terminal) (concat "[ " text " ]"))
                                  ((eq type 'nonterminal) (concat "< " text " >"))
                                  (t "???"))))
                            components))
             (line (concat "  --" (mapconcat #'identity parts "--") "-->")))
        (list header line))))

  ;; Compute grammar statistics
  (fset 'neovm--rrg-stats
    (lambda (grammar)
      "Compute statistics about the grammar."
      (let ((num-rules (length grammar))
            (total-components 0)
            (terminals 0)
            (nonterminals 0))
        (dolist (rule grammar)
          (let ((comps (plist-get (cdr rule) 'components)))
            (setq total-components (+ total-components (length comps)))
            (dolist (c comps)
              (cond
                ((eq (car c) 'terminal) (setq terminals (1+ terminals)))
                ((eq (car c) 'nonterminal) (setq nonterminals (1+ nonterminals)))))))
        (list num-rules total-components terminals nonterminals))))

  (unwind-protect
      (let* ((grammar (funcall 'neovm--rrg-define-grammar))
             (stats (funcall 'neovm--rrg-stats grammar)))
        (list
          ;; Grammar statistics
          stats
          ;; Render each rule
          (mapcar (lambda (rule)
                    (let ((name (car rule))
                          (comps (plist-get (cdr rule) 'components)))
                      (funcall 'neovm--rrg-render-rule name comps)))
                  grammar)
          ;; Descriptions
          (mapcar (lambda (rule)
                    (list (car rule) (plist-get (cdr rule) 'description)))
                  grammar)
          ;; Rule names
          (mapcar #'car grammar)
          ;; Cross-reference: which rules reference which nonterminals
          (mapcar (lambda (rule)
                    (let ((name (car rule))
                          (comps (plist-get (cdr rule) 'components))
                          (refs nil))
                      (dolist (c comps)
                        (when (eq (car c) 'nonterminal)
                          (setq refs (cons (cadr c) refs))))
                      (list name (nreverse refs))))
                  grammar)))
    (fmakunbound 'neovm--rrg-define-grammar)
    (fmakunbound 'neovm--rrg-render-rule)
    (fmakunbound 'neovm--rrg-stats)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 10 7 3) ((\"  expr:\" \"  --< term >--[ + ]--[ - ]-->\") (\"  term:\" \"  --< factor >--[ * ]--[ / ]-->\") (\"  factor:\" \"  --[ NUMBER ]--[ ( ]--< expr >--[ ) ]-->\")) ((expr \"Expression: term followed by additive operations\") (term \"Term: factor followed by multiplicative operations\") (factor \"Factor: number or parenthesized expression\")) (expr term factor) ((expr (\"term\")) (term (\"factor\")) (factor (\"expr\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: diagram width computation and centering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_railroad_width_and_centering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute widths for diagram elements and center shorter elements
    // within the maximum width.
    let form = r#"(progn
  (fset 'neovm--rrw-render
    (lambda (node)
      (let ((type (car node)) (text (cadr node)))
        (cond
          ((eq type 'terminal) (concat "[ " text " ]"))
          ((eq type 'nonterminal) (concat "< " text " >"))
          ((eq type 'epsilon) "---")
          (t "???")))))

  (fset 'neovm--rrw-center
    (lambda (s width)
      "Center string S within WIDTH by padding both sides."
      (let* ((len (length s))
             (total-pad (max 0 (- width len)))
             (left-pad (/ total-pad 2))
             (right-pad (- total-pad left-pad)))
        (concat (make-string left-pad ? ) s (make-string right-pad ? )))))

  (fset 'neovm--rrw-box
    (lambda (nodes)
      "Render nodes in a vertical box with centered alignment."
      (let* ((rendered (mapcar (lambda (n) (funcall 'neovm--rrw-render n)) nodes))
             (max-w (apply #'max (cons 0 (mapcar #'length rendered))))
             (border (make-string (+ max-w 4) ?-))
             (lines (list border)))
        (dolist (r rendered)
          (setq lines (cons (concat "| " (funcall 'neovm--rrw-center r max-w) " |") lines)))
        (setq lines (cons border lines))
        (nreverse lines))))

  (unwind-protect
      (list
        ;; Center short in wide
        (funcall 'neovm--rrw-center "ab" 10)
        (funcall 'neovm--rrw-center "x" 5)
        (funcall 'neovm--rrw-center "hello" 5)    ;; exact fit
        (funcall 'neovm--rrw-center "toolong" 3)   ;; longer than width
        ;; Box with varying widths
        (funcall 'neovm--rrw-box
                 '((terminal "x") (terminal "long_name") (nonterminal "expr")))
        ;; Box with uniform sizes
        (funcall 'neovm--rrw-box
                 '((terminal "a") (terminal "b") (terminal "c")))
        ;; Single-element box
        (funcall 'neovm--rrw-box
                 '((nonterminal "program")))
        ;; Width calculations
        (let ((nodes '((terminal "if")
                       (terminal "while")
                       (nonterminal "expression")
                       (terminal "+"))))
          (mapcar (lambda (n) (length (funcall 'neovm--rrw-render n))) nodes)))
    (fmakunbound 'neovm--rrw-render)
    (fmakunbound 'neovm--rrw-center)
    (fmakunbound 'neovm--rrw-box)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"    ab    \" \"  x  \" \"hello\" \"toolong\" (\"-----------------\" \"|     [ x ]     |\" \"| [ long_name ] |\" \"|   < expr >    |\" \"-----------------\") (\"---------\" \"| [ a ] |\" \"| [ b ] |\" \"| [ c ] |\" \"---------\") (\"---------------\" \"| < program > |\" \"---------------\") (6 9 14 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
