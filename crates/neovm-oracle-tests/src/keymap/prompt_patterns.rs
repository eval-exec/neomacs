//! Oracle parity tests for `keymap-prompt`, `keymapp`, `keymap-parent`, and
//! `set-keymap-parent` with complex patterns: hierarchical keymap systems,
//! introspection, prompt strings, and deep inheritance chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// keymapp on various object types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymapp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test keymapp on a wide variety of Elisp objects: keymaps, non-keymaps,
    // and edge cases like cons cells starting with 'keymap.
    let form = r#"(list
                    ;; True keymaps
                    (keymapp (make-keymap))
                    (keymapp (make-sparse-keymap))
                    (keymapp (make-sparse-keymap "prompt"))
                    ;; Cons cells starting with 'keymap are keymaps
                    (keymapp '(keymap))
                    (keymapp '(keymap (97 . self-insert-command)))
                    (keymapp (list 'keymap))
                    ;; Non-keymaps
                    (keymapp nil)
                    (keymapp t)
                    (keymapp 42)
                    (keymapp 3.14)
                    (keymapp "keymap")
                    (keymapp 'keymap)
                    (keymapp '(not-keymap))
                    (keymapp '(1 2 3))
                    (keymapp [1 2 3])
                    (keymapp (make-hash-table))
                    (keymapp (lambda (x) x))
                    ;; Nested keymap inside another structure
                    (keymapp (car (list (make-sparse-keymap))))
                    ;; Copy of a keymap is still a keymap
                    (keymapp (copy-keymap (make-sparse-keymap))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil nil nil nil nil nil nil nil nil nil nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymap-parent with and without parents
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_parent_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test keymap-parent returning nil for parentless keymaps,
    // and returning the actual parent after set-keymap-parent.
    let form = r#"(let ((m1 (make-sparse-keymap))
                        (m2 (make-sparse-keymap))
                        (m3 (make-keymap)))
                    ;; No parents initially
                    (let ((r1 (list (keymap-parent m1)
                                   (keymap-parent m2)
                                   (keymap-parent m3))))
                      ;; Set parents
                      (set-keymap-parent m1 m2)
                      (set-keymap-parent m2 m3)
                      (let ((r2 (list (eq (keymap-parent m1) m2)
                                      (eq (keymap-parent m2) m3)
                                      (keymap-parent m3))))
                        ;; Remove parent
                        (set-keymap-parent m1 nil)
                        (let ((r3 (list (keymap-parent m1)
                                        (eq (keymap-parent m2) m3))))
                          (list r1 r2 r3)))))"#;
    let expect = expect_test::expect![[r#""OK ((nil nil nil) (t t nil) (nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymap-prompt with and without prompt strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_prompt_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test keymap-prompt returning the prompt string or nil.
    let form = r#"(list
                    ;; Keymap with prompt
                    (keymap-prompt (make-sparse-keymap "Select action"))
                    (keymap-prompt (make-sparse-keymap "Menu"))
                    ;; Keymap without prompt
                    (keymap-prompt (make-sparse-keymap))
                    (keymap-prompt (make-keymap))
                    ;; Minimal keymap cons cell
                    (keymap-prompt '(keymap))
                    ;; Keymap with prompt string as overall-prompt
                    (keymap-prompt '(keymap "Choose one" (65 . cmd-a)))
                    ;; Keymap without prompt but with bindings
                    (keymap-prompt '(keymap (65 . cmd-a) (66 . cmd-b))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Select action\" \"Menu\" nil nil nil \"Choose one\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-keymap-parent with lookup inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_parent_lookup_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that setting a parent keymap enables key lookup inheritance,
    // and that child bindings shadow parent bindings properly.
    let form = r#"(let ((parent (make-sparse-keymap))
                        (child (make-sparse-keymap)))
                    ;; Define in parent
                    (define-key parent [?a] 'parent-cmd-a)
                    (define-key parent [?b] 'parent-cmd-b)
                    (define-key parent [?c] 'parent-cmd-c)
                    (define-key parent [?d] 'parent-cmd-d)
                    ;; Define some in child (overlapping + unique)
                    (define-key child [?a] 'child-cmd-a)
                    (define-key child [?e] 'child-cmd-e)
                    ;; Before setting parent
                    (let ((before (list (lookup-key child [?a])
                                        (lookup-key child [?b])
                                        (lookup-key child [?c])
                                        (lookup-key child [?e]))))
                      ;; Set parent
                      (set-keymap-parent child parent)
                      ;; After setting parent
                      (let ((after (list
                                     ;; child shadows parent for ?a
                                     (lookup-key child [?a])
                                     ;; inherited from parent
                                     (lookup-key child [?b])
                                     (lookup-key child [?c])
                                     (lookup-key child [?d])
                                     ;; child's own
                                     (lookup-key child [?e])
                                     ;; unbound in both
                                     (lookup-key child [?z]))))
                        ;; Verify parent is unchanged
                        (let ((parent-state (list
                                              (lookup-key parent [?a])
                                              (lookup-key parent [?b])
                                              (lookup-key parent [?e]))))
                          (list before after parent-state)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((child-cmd-a nil nil child-cmd-e) (child-cmd-a parent-cmd-b parent-cmd-c parent-cmd-d child-cmd-e nil) (parent-cmd-a parent-cmd-b nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hierarchical keymap system: multi-level inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_hierarchical_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a four-level keymap hierarchy simulating a mode system:
    // fundamental-mode -> text-mode -> prog-mode -> emacs-lisp-mode
    let form = r#"(let ((fundamental (make-sparse-keymap))
                        (text-mode (make-sparse-keymap))
                        (prog-mode (make-sparse-keymap))
                        (elisp-mode (make-sparse-keymap)))
                    ;; Set up hierarchy
                    (set-keymap-parent text-mode fundamental)
                    (set-keymap-parent prog-mode text-mode)
                    (set-keymap-parent elisp-mode prog-mode)
                    ;; Define bindings at each level
                    (define-key fundamental [?q] 'quit)
                    (define-key fundamental [?s] 'save)
                    (define-key fundamental [?h] 'help)
                    (define-key text-mode [?i] 'insert-text)
                    (define-key text-mode [?h] 'text-help)
                    (define-key prog-mode [?c] 'compile)
                    (define-key prog-mode [?d] 'debug)
                    (define-key prog-mode [?h] 'prog-help)
                    (define-key elisp-mode [?e] 'eval-defun)
                    (define-key elisp-mode [?d] 'elisp-debug)
                    ;; Test resolution at elisp-mode level
                    (list
                      ;; elisp-mode's own binding
                      (lookup-key elisp-mode [?e])
                      ;; elisp-mode shadows prog-mode for ?d
                      (lookup-key elisp-mode [?d])
                      ;; prog-mode's binding inherited through
                      (lookup-key elisp-mode [?c])
                      ;; text-mode's binding inherited through
                      (lookup-key elisp-mode [?i])
                      ;; fundamental's binding inherited (not shadowed)
                      (lookup-key elisp-mode [?q])
                      (lookup-key elisp-mode [?s])
                      ;; ?h shadowed at each level: closest wins (prog-mode)
                      (lookup-key elisp-mode [?h])
                      (lookup-key prog-mode [?h])
                      (lookup-key text-mode [?h])
                      (lookup-key fundamental [?h])
                      ;; Unbound at all levels
                      (lookup-key elisp-mode [?z])
                      ;; Verify parent chain
                      (eq (keymap-parent elisp-mode) prog-mode)
                      (eq (keymap-parent prog-mode) text-mode)
                      (eq (keymap-parent text-mode) fundamental)
                      (keymap-parent fundamental)))"#;
    let expect = expect_test::expect![[
        r#""OK (eval-defun elisp-debug compile insert-text quit save prog-help prog-help text-help help nil t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Keymap introspection: collecting all bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use map-keymap to collect all direct bindings of a keymap,
    // and compare with inherited bindings via parent chain.
    let form = r#"(let ((parent (make-sparse-keymap))
                        (child (make-sparse-keymap)))
                    (define-key parent [?a] 'cmd-a)
                    (define-key parent [?b] 'cmd-b)
                    (define-key parent [?c] 'cmd-c)
                    (define-key child [?c] 'child-cmd-c)
                    (define-key child [?d] 'cmd-d)
                    (define-key child [?e] 'cmd-e)
                    (set-keymap-parent child parent)
                    ;; Collect direct bindings of child
                    (let ((child-direct nil))
                      (map-keymap (lambda (key binding)
                                    (setq child-direct
                                          (cons (cons key binding) child-direct)))
                                  child)
                      ;; Collect direct bindings of parent
                      (let ((parent-direct nil))
                        (map-keymap (lambda (key binding)
                                      (setq parent-direct
                                            (cons (cons key binding) parent-direct)))
                                    parent)
                        (list
                          ;; child's direct bindings (sorted by key)
                          (sort child-direct (lambda (a b) (< (car a) (car b))))
                          ;; parent's direct bindings (sorted by key)
                          (sort parent-direct (lambda (a b) (< (car a) (car b))))
                          ;; Effective bindings at child level for specific keys
                          (list (lookup-key child [?a])    ;; from parent
                                (lookup-key child [?b])    ;; from parent
                                (lookup-key child [?c])    ;; child shadows
                                (lookup-key child [?d])    ;; child's own
                                (lookup-key child [?e]))   ;; child's own
                          ;; keymap structure checks
                          (keymapp child)
                          (keymapp parent)
                          (eq (keymap-parent child) parent)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . cmd-a) (98 . cmd-b) (99 . cmd-c) (99 . child-cmd-c) (100 . cmd-d) (101 . cmd-e)) ((97 . cmd-a) (98 . cmd-b) (99 . cmd-c)) (cmd-a cmd-b child-cmd-c cmd-d cmd-e) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: keymap with prefix keys, prompts, and parent chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_prefix_with_prompts_and_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complex keymap system with prefix keys that have their own
    // prompt strings, combined with parent inheritance for the prefix sub-keymaps.
    let form = r#"(let ((base-map (make-sparse-keymap))
                        (base-help (make-sparse-keymap "Help commands"))
                        (ext-map (make-sparse-keymap))
                        (ext-help (make-sparse-keymap "Extended help")))
                    ;; Base map has a help prefix at C-h (8)
                    (define-key base-map [8] base-help)
                    (define-key base-help [?a] 'apropos-help)
                    (define-key base-help [?k] 'describe-key)
                    (define-key base-help [?f] 'describe-function)
                    ;; Base map has some direct bindings
                    (define-key base-map [?q] 'quit)
                    (define-key base-map [?s] 'save)
                    ;; Extended map inherits from base
                    (set-keymap-parent ext-map base-map)
                    ;; Extended help inherits from base help
                    (set-keymap-parent ext-help base-help)
                    ;; Extended map overrides the help prefix with extended version
                    (define-key ext-map [8] ext-help)
                    ;; Extended help adds new bindings
                    (define-key ext-help [?v] 'describe-variable)
                    (define-key ext-help [?p] 'describe-package)
                    ;; Extended map adds its own bindings
                    (define-key ext-map [?r] 'reload)
                    (list
                      ;; Prompt strings
                      (keymap-prompt base-help)
                      (keymap-prompt ext-help)
                      (keymap-prompt base-map)
                      (keymap-prompt ext-map)
                      ;; Base map help lookups
                      (lookup-key base-map [8 ?a])
                      (lookup-key base-map [8 ?k])
                      (lookup-key base-map [8 ?f])
                      (lookup-key base-map [8 ?v])
                      ;; Extended map help lookups (inherits base + adds new)
                      (lookup-key ext-map [8 ?a])
                      (lookup-key ext-map [8 ?k])
                      (lookup-key ext-map [8 ?v])
                      (lookup-key ext-map [8 ?p])
                      ;; Direct bindings
                      (lookup-key ext-map [?q])
                      (lookup-key ext-map [?s])
                      (lookup-key ext-map [?r])
                      ;; Verify prefix sub-keymap is a keymap
                      (keymapp (lookup-key ext-map [8]))
                      ;; Parent chain verification
                      (eq (keymap-parent ext-map) base-map)
                      (eq (keymap-parent ext-help) base-help)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Help commands\" \"Extended help\" nil nil apropos-help describe-key describe-function nil apropos-help describe-key describe-variable describe-package quit save reload t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reassigning keymap-parent and observing lookup changes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_parent_reassignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that changing the parent of a keymap dynamically alters
    // what keys resolve to, and that setting parent to nil removes inheritance.
    let form = r#"(let ((p1 (make-sparse-keymap))
                        (p2 (make-sparse-keymap))
                        (child (make-sparse-keymap)))
                    ;; Different parents provide different bindings
                    (define-key p1 [?x] 'p1-cmd-x)
                    (define-key p1 [?y] 'p1-cmd-y)
                    (define-key p2 [?x] 'p2-cmd-x)
                    (define-key p2 [?z] 'p2-cmd-z)
                    ;; Child has its own binding
                    (define-key child [?w] 'child-cmd-w)
                    ;; Phase 1: no parent
                    (let ((phase1 (list (lookup-key child [?x])
                                        (lookup-key child [?w]))))
                      ;; Phase 2: parent is p1
                      (set-keymap-parent child p1)
                      (let ((phase2 (list (lookup-key child [?x])
                                          (lookup-key child [?y])
                                          (lookup-key child [?z])
                                          (lookup-key child [?w]))))
                        ;; Phase 3: switch parent to p2
                        (set-keymap-parent child p2)
                        (let ((phase3 (list (lookup-key child [?x])
                                            (lookup-key child [?y])
                                            (lookup-key child [?z])
                                            (lookup-key child [?w]))))
                          ;; Phase 4: remove parent
                          (set-keymap-parent child nil)
                          (let ((phase4 (list (lookup-key child [?x])
                                              (lookup-key child [?y])
                                              (lookup-key child [?z])
                                              (lookup-key child [?w]))))
                            (list phase1 phase2 phase3 phase4
                                  (keymap-parent child))))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
