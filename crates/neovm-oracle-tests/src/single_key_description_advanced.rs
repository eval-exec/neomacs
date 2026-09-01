//! Advanced oracle parity tests for `single-key-description`.
//!
//! Covers printable characters, control characters, meta combinations,
//! the NO-ANGLES parameter, and a complex keymap legend builder.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Printable characters: letters, digits, punctuation, space
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_printable_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  (single-key-description ?a)
  (single-key-description ?z)
  (single-key-description ?A)
  (single-key-description ?Z)
  (single-key-description ?0)
  (single-key-description ?9)
  (single-key-description ?!)
  (single-key-description ?@)
  (single-key-description ?#)
  (single-key-description ?~)
  (single-key-description ?/)
  (single-key-description ?\\)
  (single-key-description ?\s)
  (single-key-description ?.)
  (single-key-description ?,)
  (single-key-description ?-)
  (single-key-description ?=)
  (single-key-description ?\[)
  (single-key-description ?\])
  (single-key-description ?{)
  (single-key-description ?}))
"#;
    let expect = expect_test::expect![[
        r##""OK (\"a\" \"z\" \"A\" \"Z\" \"0\" \"9\" \"!\" \"@\" \"#\" \"~\" \"/\" \"\\\\\" \"SPC\" \".\" \",\" \"-\" \"=\" \"[\" \"]\" \"{\" \"}\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Control characters: C-a through several common ones
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_control_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  (single-key-description ?\C-a)
  (single-key-description ?\C-b)
  (single-key-description ?\C-c)
  (single-key-description ?\C-d)
  (single-key-description ?\C-e)
  (single-key-description ?\C-g)
  (single-key-description ?\C-h)
  (single-key-description ?\C-i)
  (single-key-description ?\C-j)
  (single-key-description ?\C-k)
  (single-key-description ?\C-l)
  (single-key-description ?\C-m)
  (single-key-description ?\C-n)
  (single-key-description ?\C-o)
  (single-key-description ?\C-x)
  (single-key-description ?\C-z)
  ;; DEL (127)
  (single-key-description 127)
  ;; C-@ is NUL
  (single-key-description ?\C-@))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"C-a\" \"C-b\" \"C-c\" \"C-d\" \"C-e\" \"C-g\" \"C-h\" \"TAB\" \"C-j\" \"C-k\" \"C-l\" \"RET\" \"C-n\" \"C-o\" \"C-x\" \"C-z\" \"DEL\" \"C-@\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Meta combinations: M-a, M-x, C-M-a, C-M-x, etc.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_meta_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  (single-key-description ?\M-a)
  (single-key-description ?\M-z)
  (single-key-description ?\M-x)
  (single-key-description ?\M-0)
  (single-key-description ?\M-9)
  (single-key-description ?\M-!)
  (single-key-description ?\C-\M-a)
  (single-key-description ?\C-\M-x)
  (single-key-description ?\C-\M-z)
  (single-key-description ?\C-\M-@)
  ;; Meta + space
  (single-key-description ?\M-\s)
  ;; event-convert-list to create super, hyper combos
  (single-key-description (event-convert-list '(meta shift ?a)))
  (single-key-description (event-convert-list '(control meta shift ?z)))
  (single-key-description (event-convert-list '(super meta ?f)))
  (single-key-description (event-convert-list '(hyper meta ?h))))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"M-a\" \"M-z\" \"M-x\" \"M-0\" \"M-9\" \"M-!\" \"C-M-a\" \"C-M-x\" \"C-M-z\" \"C-M-@\" \"M-SPC\" \"M-A\" \"C-M-S-z\" \"M-s-f\" \"H-M-h\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NO-ANGLES parameter: suppress angle brackets for special keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_no_angles_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When NO-ANGLES is non-nil, function key symbols should appear
    // without angle brackets.
    let form = r#"
(list
  ;; With angle brackets (default)
  (single-key-description 'tab)
  (single-key-description 'return)
  (single-key-description 'backspace)
  (single-key-description 'escape)
  (single-key-description 'f1)
  (single-key-description 'f12)
  (single-key-description 'home)
  (single-key-description 'end)
  ;; Without angle brackets (NO-ANGLES = t)
  (single-key-description 'tab t)
  (single-key-description 'return t)
  (single-key-description 'backspace t)
  (single-key-description 'escape t)
  (single-key-description 'f1 t)
  (single-key-description 'f12 t)
  (single-key-description 'home t)
  (single-key-description 'end t)
  ;; Numeric keys should be unaffected by NO-ANGLES
  (single-key-description ?a t)
  (single-key-description ?\C-x t)
  (single-key-description ?\M-a t))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"<tab>\" \"<return>\" \"<backspace>\" \"<escape>\" \"<f1>\" \"<f12>\" \"<home>\" \"<end>\" \"tab\" \"return\" \"backspace\" \"escape\" \"f1\" \"f12\" \"home\" \"end\" \"a\" \"C-x\" \"M-a\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Super and hyper modifier keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_super_hyper_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Super modifier
  (single-key-description (event-convert-list '(super ?a)))
  (single-key-description (event-convert-list '(super ?x)))
  (single-key-description (event-convert-list '(super ?1)))
  ;; Hyper modifier
  (single-key-description (event-convert-list '(hyper ?a)))
  (single-key-description (event-convert-list '(hyper ?x)))
  (single-key-description (event-convert-list '(hyper ?1)))
  ;; Combined: control + super
  (single-key-description (event-convert-list '(control super ?a)))
  ;; Combined: meta + hyper
  (single-key-description (event-convert-list '(meta hyper ?z)))
  ;; All five modifiers
  (single-key-description (event-convert-list '(control meta shift super hyper ?q)))
  ;; Verify ordering is consistent
  (let ((k1 (event-convert-list '(control meta ?a)))
        (k2 (event-convert-list '(meta control ?a))))
    (equal (single-key-description k1)
           (single-key-description k2))))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"s-a\" \"s-x\" \"s-1\" \"H-a\" \"H-x\" \"H-1\" \"C-s-a\" \"H-M-z\" \"C-H-M-S-s-q\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a complete keymap legend using single-key-description
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_keymap_legend_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a keymap, enumerate its bindings, and produce a formatted
    // legend string using single-key-description for each key.
    let form = r#"
(progn
  (fset 'neovm--skd-format-binding
    (lambda (key command)
      "Format a single key binding as a legend entry."
      (format "  %-12s  %s"
              (if (integerp key)
                  (single-key-description key)
                (single-key-description key))
              (symbol-name command))))

  (fset 'neovm--skd-build-legend
    (lambda (keymap title)
      "Build a formatted legend string from a keymap."
      (let ((entries nil)
            (header (format "=== %s ===" title)))
        ;; Collect all direct bindings from the keymap
        (map-keymap
         (lambda (key binding)
           (when (and (symbolp binding)
                      (not (keymapp binding)))
             (setq entries
                   (cons (funcall 'neovm--skd-format-binding key binding)
                         entries))))
         keymap)
        (setq entries (sort entries #'string<))
        (mapconcat #'identity (cons header entries) "\n"))))

  (fset 'neovm--skd-count-bindings
    (lambda (keymap)
      "Count non-keymap bindings in a keymap."
      (let ((count 0))
        (map-keymap
         (lambda (key binding)
           (when (and (symbolp binding) (not (keymapp binding)))
             (setq count (1+ count))))
         keymap)
        count)))

  (unwind-protect
      (let ((my-map (make-sparse-keymap)))
        ;; Bind a variety of key types
        (define-key my-map (vector ?\C-s) 'save-buffer)
        (define-key my-map (vector ?\C-f) 'find-file)
        (define-key my-map (vector ?\C-g) 'keyboard-quit)
        (define-key my-map (vector ?\M-x) 'execute-command)
        (define-key my-map (vector ?q) 'quit)
        (define-key my-map (vector ?/) 'search-forward)
        (define-key my-map (vector ?n) 'next-line)
        (define-key my-map (vector ?p) 'previous-line)
        (let ((legend (funcall 'neovm--skd-build-legend my-map "Editor Bindings"))
              (binding-count (funcall 'neovm--skd-count-bindings my-map)))
          (list
           ;; Verify the legend is a string
           (stringp legend)
           ;; Verify binding count
           binding-count
           ;; Verify some specific descriptions are present
           (let ((descriptions
                  (list (single-key-description ?\C-s)
                        (single-key-description ?\C-f)
                        (single-key-description ?\C-g)
                        (single-key-description ?\M-x)
                        (single-key-description ?q)
                        (single-key-description ?/)
                        (single-key-description ?n)
                        (single-key-description ?p))))
             descriptions)
           ;; Verify we can round-trip: the description strings appear in legend
           (string-match-p "C-s" legend)
           (string-match-p "C-f" legend)
           (string-match-p "M-x" legend)
           ;; The full legend text
           legend)))
    (fmakunbound 'neovm--skd-format-binding)
    (fmakunbound 'neovm--skd-build-legend)
    (fmakunbound 'neovm--skd-count-bindings)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t 7 (\"C-s\" \"C-f\" \"C-g\" \"M-x\" \"q\" \"/\" \"n\" \"p\") 113 57 nil \"=== Editor Bindings ===\\n  /             search-forward\\n  C-f           find-file\\n  C-g           keyboard-quit\\n  C-s           save-buffer\\n  n             next-line\\n  p             previous-line\\n  q             quit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: batch description of key ranges with categorization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skd_batch_categorize_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Categorize a set of keys into control, meta, plain, and compound,
    // then produce a summary using single-key-description.
    let form = r#"
(progn
  (fset 'neovm--skd-categorize-key
    (lambda (key)
      "Categorize a numeric key into its modifier class."
      (let ((mods (event-modifiers key)))
        (cond
         ((and (memq 'control mods) (memq 'meta mods)) 'compound)
         ((memq 'control mods) 'control)
         ((memq 'meta mods) 'meta)
         ((memq 'shift mods) 'shift)
         (t 'plain)))))

  (fset 'neovm--skd-batch-describe
    (lambda (keys)
      "Describe and categorize a list of keys, returning an alist
       of (category . ((description . key) ...))."
      (let ((result nil))
        (dolist (k keys)
          (let* ((cat (funcall 'neovm--skd-categorize-key k))
                 (desc (single-key-description k))
                 (entry (assq cat result)))
            (if entry
                (setcdr entry (cons (cons desc k) (cdr entry)))
              (setq result (cons (list cat (cons desc k)) result)))))
        ;; Sort each category's entries by description
        (dolist (entry result)
          (setcdr entry
                  (sort (cdr entry)
                        (lambda (a b) (string< (car a) (car b))))))
        result)))

  (unwind-protect
      (let* ((test-keys
              (list ?a ?z ?0 ?9
                    ?\C-a ?\C-x ?\C-z
                    ?\M-a ?\M-x ?\M-z
                    ?\C-\M-a ?\C-\M-x ?\C-\M-z))
             (categorized (funcall 'neovm--skd-batch-describe test-keys)))
        (list
         ;; Number of categories found
         (length categorized)
         ;; Get just the descriptions from each category
         (mapcar (lambda (entry)
                   (cons (car entry)
                         (mapcar #'car (cdr entry))))
                 categorized)
         ;; Verify all keys got a description
         (= (apply #'+ (mapcar (lambda (e) (length (cdr e))) categorized))
            (length test-keys))))
    (fmakunbound 'neovm--skd-categorize-key)
    (fmakunbound 'neovm--skd-batch-describe)))
"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((compound \"C-M-a\" \"C-M-x\" \"C-M-z\") (meta \"M-a\" \"M-x\" \"M-z\") (control \"C-a\" \"C-x\" \"C-z\") (plain \"0\" \"9\" \"a\" \"z\")) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
