//! Advanced oracle parity tests for `internal-event-symbol-parse-modifiers`.
//!
//! Covers basic event symbols, single modifiers, combined modifiers,
//! mouse event symbols, and a complex modifier decomposition table builder.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Parse basic (unmodified) event symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iespm_basic_event_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Plain letter symbols — should return (symbol) with no modifiers
  (internal-event-symbol-parse-modifiers 'a)
  (internal-event-symbol-parse-modifiers 'z)
  (internal-event-symbol-parse-modifiers 'x)
  ;; Function keys
  (internal-event-symbol-parse-modifiers 'f1)
  (internal-event-symbol-parse-modifiers 'f12)
  ;; Special keys
  (internal-event-symbol-parse-modifiers 'return)
  (internal-event-symbol-parse-modifiers 'tab)
  (internal-event-symbol-parse-modifiers 'backspace)
  (internal-event-symbol-parse-modifiers 'escape)
  (internal-event-symbol-parse-modifiers 'home)
  (internal-event-symbol-parse-modifiers 'end)
  (internal-event-symbol-parse-modifiers 'delete)
  (internal-event-symbol-parse-modifiers 'insert)
  ;; Mouse events without modifiers
  (internal-event-symbol-parse-modifiers 'mouse-1)
  (internal-event-symbol-parse-modifiers 'mouse-2)
  (internal-event-symbol-parse-modifiers 'mouse-3)
  (internal-event-symbol-parse-modifiers 'down-mouse-1)
  (internal-event-symbol-parse-modifiers 'double-mouse-1))
"#;
    let expect = expect_test::expect![[
        r#""OK ((a) (z) (x) (f1) (f12) (return) (tab) (backspace) (escape) (home) (end) (delete) (insert) (mouse-1 click) (mouse-2 click) (mouse-3 click) (mouse-1 down) (mouse-1 double))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse single-modifier event symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iespm_single_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Control
  (internal-event-symbol-parse-modifiers 'C-a)
  (internal-event-symbol-parse-modifiers 'C-x)
  (internal-event-symbol-parse-modifiers 'C-return)
  (internal-event-symbol-parse-modifiers 'C-f1)
  ;; Meta
  (internal-event-symbol-parse-modifiers 'M-a)
  (internal-event-symbol-parse-modifiers 'M-x)
  (internal-event-symbol-parse-modifiers 'M-return)
  (internal-event-symbol-parse-modifiers 'M-f1)
  ;; Shift
  (internal-event-symbol-parse-modifiers 'S-a)
  (internal-event-symbol-parse-modifiers 'S-return)
  (internal-event-symbol-parse-modifiers 'S-tab)
  ;; Super
  (internal-event-symbol-parse-modifiers 's-a)
  (internal-event-symbol-parse-modifiers 's-x)
  (internal-event-symbol-parse-modifiers 's-f1)
  ;; Hyper
  (internal-event-symbol-parse-modifiers 'H-a)
  (internal-event-symbol-parse-modifiers 'H-x)
  (internal-event-symbol-parse-modifiers 'H-f1)
  ;; Control on mouse
  (internal-event-symbol-parse-modifiers 'C-mouse-1)
  (internal-event-symbol-parse-modifiers 'M-mouse-2)
  (internal-event-symbol-parse-modifiers 'S-mouse-3))
"#;
    let expect = expect_test::expect![[
        r#""OK ((a control) (x control) (return control) (f1 control) (a meta) (x meta) (return meta) (f1 meta) (a shift) (return shift) (tab shift) (a super) (x super) (f1 super) (a hyper) (x hyper) (f1 hyper) (mouse-1 control click) (mouse-2 meta click) (mouse-3 shift click))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse combined modifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iespm_combined_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Two modifiers
  (internal-event-symbol-parse-modifiers 'C-M-a)
  (internal-event-symbol-parse-modifiers 'C-M-x)
  (internal-event-symbol-parse-modifiers 'C-S-a)
  (internal-event-symbol-parse-modifiers 'M-S-a)
  (internal-event-symbol-parse-modifiers 'C-M-return)
  (internal-event-symbol-parse-modifiers 'C-S-f1)
  ;; Three modifiers
  (internal-event-symbol-parse-modifiers 'C-M-S-a)
  (internal-event-symbol-parse-modifiers 'C-M-S-z)
  (internal-event-symbol-parse-modifiers 'C-M-S-return)
  ;; With super and hyper
  (internal-event-symbol-parse-modifiers 'C-s-a)
  (internal-event-symbol-parse-modifiers 'M-H-a)
  (internal-event-symbol-parse-modifiers 'C-M-s-a)
  (internal-event-symbol-parse-modifiers 'C-M-H-a)
  ;; Mouse with combined modifiers
  (internal-event-symbol-parse-modifiers 'C-M-mouse-1)
  (internal-event-symbol-parse-modifiers 'C-M-S-mouse-3)
  (internal-event-symbol-parse-modifiers 'C-M-down-mouse-1))
"#;
    let expect = expect_test::expect![[
        r#""OK ((a meta control) (x meta control) (a control shift) (a meta shift) (return meta control) (f1 control shift) (a meta control shift) (z meta control shift) (return meta control shift) (a control super) (a meta hyper) (a meta control super) (a meta control hyper) (mouse-1 meta control click) (mouse-3 meta control shift click) (mouse-1 meta control down))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Verify consistency with event-modifiers and event-basic-type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iespm_consistency_with_event_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that internal-event-symbol-parse-modifiers results are
    // consistent with event-modifiers when applied to converted events.
    let form = r#"
(progn
  (fset 'neovm--iespm-check-consistency
    (lambda (sym)
      "Check that parse-modifiers on a symbol decomposes correctly."
      (let* ((parsed (internal-event-symbol-parse-modifiers sym))
             (base-sym (car parsed))
             (mods (cdr parsed))
             ;; Also check via event-convert-list roundtrip where possible
             (has-control (memq 'control mods))
             (has-meta (memq 'meta mods))
             (has-shift (memq 'shift mods)))
        (list sym base-sym (sort (copy-sequence mods) #'string<)))))

  (unwind-protect
      (let ((test-syms '(a C-a M-a C-M-a S-a C-S-a M-S-a C-M-S-a
                          s-a H-a C-s-a M-H-a
                          return C-return M-return C-M-return
                          f1 C-f1 M-f1 C-M-f1
                          mouse-1 C-mouse-1 C-M-mouse-1)))
        (mapcar (lambda (s) (funcall 'neovm--iespm-check-consistency s))
                test-syms))
    (fmakunbound 'neovm--iespm-check-consistency)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((a a nil) (C-a a (control)) (M-a a (meta)) (C-M-a a (control meta)) (S-a a (shift)) (C-S-a a (control shift)) (M-S-a a (meta shift)) (C-M-S-a a (control meta shift)) (s-a a (super)) (H-a a (hyper)) (C-s-a a (control super)) (M-H-a a (hyper meta)) (return return nil) (C-return return (control)) (M-return return (meta)) (C-M-return return (control meta)) (f1 f1 nil) (C-f1 f1 (control)) (M-f1 f1 (meta)) (C-M-f1 f1 (control meta)) (mouse-1 mouse-1 (click)) (C-mouse-1 mouse-1 (click control)) (C-M-mouse-1 mouse-1 (click control meta)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a complete modifier decomposition table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iespm_decomposition_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a table that for each of a set of symbols, records:
    // - the original symbol name
    // - the base event
    // - the modifier set
    // - a reconstructed description via single-key-description + event-convert-list
    // - whether the round-trip matches
    let form = r#"
(progn
  (fset 'neovm--iespm-decompose
    (lambda (sym)
      "Decompose a symbol and attempt to reconstruct."
      (let* ((parsed (internal-event-symbol-parse-modifiers sym))
             (base (car parsed))
             (mods (cdr parsed))
             (sorted-mods (sort (copy-sequence mods) #'string<))
             (mod-count (length mods))
             ;; Build a classification string
             (classification
              (cond
               ((= mod-count 0) "plain")
               ((= mod-count 1) (format "single-%s" (car mods)))
               ((= mod-count 2) "double-mod")
               ((= mod-count 3) "triple-mod")
               (t "multi-mod"))))
        (list (symbol-name sym) base sorted-mods classification))))

  (fset 'neovm--iespm-build-table
    (lambda (syms)
      "Build a decomposition table for a list of symbols."
      (let ((table nil)
            (stats (make-hash-table :test 'equal)))
        ;; Decompose each symbol
        (dolist (s syms)
          (let* ((entry (funcall 'neovm--iespm-decompose s))
                 (classification (nth 3 entry)))
            (setq table (cons entry table))
            ;; Count by classification
            (puthash classification
                     (1+ (or (gethash classification stats) 0))
                     stats)))
        ;; Build stats summary
        (let ((stat-list nil))
          (maphash (lambda (k v) (setq stat-list (cons (cons k v) stat-list)))
                   stats)
          (setq stat-list (sort stat-list (lambda (a b) (string< (car a) (car b)))))
          (list :entries (nreverse table)
                :total (length syms)
                :stats stat-list)))))

  (unwind-protect
      (let ((symbols '(a x z
                        C-a C-x C-z
                        M-a M-x M-z
                        S-a S-x
                        s-a H-a
                        C-M-a C-M-x
                        C-S-a M-S-a
                        C-M-S-a C-M-S-z
                        return C-return M-return C-M-return
                        f1 C-f1
                        mouse-1 C-mouse-1 C-M-mouse-1)))
        (funcall 'neovm--iespm-build-table symbols))
    (fmakunbound 'neovm--iespm-decompose)
    (fmakunbound 'neovm--iespm-build-table)))
"#;
    let expect = expect_test::expect![[
        r#""OK (:entries ((\"a\" a nil \"plain\") (\"x\" x nil \"plain\") (\"z\" z nil \"plain\") (\"C-a\" a (control) \"single-control\") (\"C-x\" x (control) \"single-control\") (\"C-z\" z (control) \"single-control\") (\"M-a\" a (meta) \"single-meta\") (\"M-x\" x (meta) \"single-meta\") (\"M-z\" z (meta) \"single-meta\") (\"S-a\" a (shift) \"single-shift\") (\"S-x\" x (shift) \"single-shift\") (\"s-a\" a (super) \"single-super\") (\"H-a\" a (hyper) \"single-hyper\") (\"C-M-a\" a (control meta) \"double-mod\") (\"C-M-x\" x (control meta) \"double-mod\") (\"C-S-a\" a (control shift) \"double-mod\") (\"M-S-a\" a (meta shift) \"double-mod\") (\"C-M-S-a\" a (control meta shift) \"triple-mod\") (\"C-M-S-z\" z (control meta shift) \"triple-mod\") (\"return\" return nil \"plain\") (\"C-return\" return (control) \"single-control\") (\"M-return\" return (meta) \"single-meta\") (\"C-M-return\" return (control meta) \"double-mod\") (\"f1\" f1 nil \"plain\") (\"C-f1\" f1 (control) \"single-control\") (\"mouse-1\" mouse-1 (click) \"single-click\") (\"C-mouse-1\" mouse-1 (click control) \"double-mod\") (\"C-M-mouse-1\" mouse-1 (click control meta) \"triple-mod\")) :total 28 :stats ((\"double-mod\" . 6) (\"plain\" . 5) (\"single-click\" . 1) (\"single-control\" . 5) (\"single-hyper\" . 1) (\"single-meta\" . 4) (\"single-shift\" . 2) (\"single-super\" . 1) (\"triple-mod\" . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
