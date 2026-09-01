//! Advanced oracle parity tests for keymap operations.
//!
//! Tests make-keymap vs make-sparse-keymap differences, define-key with
//! various key sequences, lookup-key return values, keymap-parent inheritance,
//! copy-keymap independence, key binding shadowing, multi-level hierarchy
//! with prefix keys, and keymapp predicate on various objects.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-keymap vs make-sparse-keymap differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_make_vs_sparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full keymap has a char-table, sparse keymap does not
    let form = "(let ((full (make-keymap))
                      (sparse (make-sparse-keymap)))
                  (list
                    (keymapp full)
                    (keymapp sparse)
                    ;; Both are cons cells starting with 'keymap
                    (eq (car full) 'keymap)
                    (eq (car sparse) 'keymap)
                    ;; Full keymap's second element is a char-table
                    (char-table-p (cadr full))
                    ;; Sparse keymap has no char-table (just the keymap symbol)
                    (if (cdr sparse) (char-table-p (cadr sparse)) nil)
                    ;; Sparse keymap with prompt string
                    (let ((sp (make-sparse-keymap \"My menu\")))
                      (list (keymapp sp)
                            (keymap-prompt sp)))))";
    let expect = expect_test::expect![[r#""OK (t t t t t nil (t \"My menu\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with various key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_various_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Single char, vector key, multi-char string key
    let form = r#"(let ((m (make-sparse-keymap)))
                    ;; Single char via vector
                    (define-key m [?a] 'cmd-a)
                    ;; Another char
                    (define-key m [?b] 'cmd-b)
                    ;; Control char (C-c = 3)
                    (define-key m [3] 'cmd-ctrl-c)
                    ;; String key sequence
                    (define-key m (kbd "C-x C-f") 'find-file-cmd)
                    (list
                      (lookup-key m [?a])
                      (lookup-key m [?b])
                      (lookup-key m [3])
                      ;; Multi-key: C-x should return a sub-keymap
                      (keymapp (lookup-key m (kbd "C-x")))
                      ;; Full sequence
                      (lookup-key m (kbd "C-x C-f"))))"#;
    let expect = expect_test::expect![[r#""OK (cmd-a cmd-b cmd-ctrl-c t find-file-cmd)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key: all return value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_return_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // lookup-key returns: bound command, keymap (prefix), integer (too-long), nil (unbound)
    let form = r#"(let ((m (make-sparse-keymap)))
                    (define-key m (kbd "C-x C-f") 'find-file-cmd)
                    (define-key m [?a] 'cmd-a)
                    (list
                      ;; Bound command
                      (lookup-key m [?a])
                      ;; Prefix key returns sub-keymap
                      (keymapp (lookup-key m (kbd "C-x")))
                      ;; Full sequence returns command
                      (lookup-key m (kbd "C-x C-f"))
                      ;; Unbound key returns nil
                      (lookup-key m [?z])
                      ;; Too-long prefix: key seq longer than any binding
                      ;; Returns number = length of prefix that was a valid prefix
                      (let ((result (lookup-key m [?a ?b])))
                        (if (numberp result)
                            (list 'too-long result)
                          result))))"#;
    let expect = expect_test::expect![[r#""OK (cmd-a t find-file-cmd nil (too-long 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymap-parent / set-keymap-parent inheritance chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three-level parent chain: grandparent -> parent -> child
    let form = "(let ((grandparent (make-sparse-keymap))
                      (parent (make-sparse-keymap))
                      (child (make-sparse-keymap)))
                  (define-key grandparent [?a] 'gp-cmd-a)
                  (define-key grandparent [?b] 'gp-cmd-b)
                  (define-key grandparent [?c] 'gp-cmd-c)
                  (define-key parent [?b] 'parent-cmd-b)
                  (define-key child [?c] 'child-cmd-c)
                  (set-keymap-parent parent grandparent)
                  (set-keymap-parent child parent)
                  (list
                    ;; Child inherits from grandparent through parent
                    (lookup-key child [?a])    ;; from grandparent
                    (lookup-key child [?b])    ;; from parent (shadows grandparent)
                    (lookup-key child [?c])    ;; from child (shadows both)
                    ;; Parent inherits from grandparent
                    (lookup-key parent [?a])   ;; from grandparent
                    (lookup-key parent [?b])   ;; parent's own
                    (lookup-key parent [?c])   ;; from grandparent
                    ;; Verify parent relationships
                    (eq (keymap-parent child) parent)
                    (eq (keymap-parent parent) grandparent)
                    (keymap-parent grandparent)))";
    let expect = expect_test::expect![[
        r#""OK (gp-cmd-a parent-cmd-b child-cmd-c gp-cmd-a parent-cmd-b gp-cmd-c t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-keymap independence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy a keymap, then modify original and verify copy is independent
    let form = "(let ((orig (make-sparse-keymap)))
                  (define-key orig [?a] 'cmd-a)
                  (define-key orig [?b] 'cmd-b)
                  (let ((copy (copy-keymap orig)))
                    ;; Modify original
                    (define-key orig [?a] 'new-cmd-a)
                    (define-key orig [?c] 'cmd-c)
                    ;; Modify copy differently
                    (define-key copy [?b] 'copy-cmd-b)
                    (define-key copy [?d] 'cmd-d)
                    (list
                      ;; Original state
                      (lookup-key orig [?a])
                      (lookup-key orig [?b])
                      (lookup-key orig [?c])
                      (lookup-key orig [?d])
                      ;; Copy state (should retain original bindings for [?a])
                      (lookup-key copy [?a])
                      (lookup-key copy [?b])
                      (lookup-key copy [?c])
                      (lookup-key copy [?d])
                      ;; Both are keymaps
                      (keymapp orig)
                      (keymapp copy))))";
    let expect = expect_test::expect![[
        r#""OK (new-cmd-a cmd-b cmd-c nil cmd-a copy-cmd-b nil cmd-d t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Key binding shadowing: child overrides parent at specific keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that child bindings shadow parent, and removing child binding reveals parent
    let form = "(let ((parent (make-sparse-keymap))
                      (child (make-sparse-keymap)))
                  (define-key parent [?x] 'parent-x)
                  (define-key parent [?y] 'parent-y)
                  (define-key parent [?z] 'parent-z)
                  (set-keymap-parent child parent)
                  ;; Shadow some parent bindings
                  (define-key child [?x] 'child-x)
                  (define-key child [?y] 'child-y)
                  (let ((before-x (lookup-key child [?x]))
                        (before-y (lookup-key child [?y]))
                        (before-z (lookup-key child [?z])))
                    ;; Remove child's shadow for ?x
                    (define-key child [?x] nil)
                    (let ((after-x (lookup-key child [?x]))
                          (after-y (lookup-key child [?y]))
                          (after-z (lookup-key child [?z])))
                      (list
                        before-x before-y before-z
                        after-x after-y after-z
                        ;; Parent unchanged
                        (lookup-key parent [?x])
                        (lookup-key parent [?y])))))";
    let expect = expect_test::expect![[
        r#""OK (child-x child-y parent-z nil child-y parent-z parent-x parent-y)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-level keymap hierarchy with prefix keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_prefix_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complex prefix key structure like Emacs C-x and C-c maps
    let form = r#"(let ((root (make-sparse-keymap))
                        (cx-map (make-sparse-keymap))
                        (cc-map (make-sparse-keymap))
                        (cxc-map (make-sparse-keymap)))
                    ;; C-x prefix
                    (define-key root [24] cx-map)   ;; C-x = 24
                    ;; C-c prefix
                    (define-key root [3] cc-map)    ;; C-c = 3
                    ;; C-x sub-bindings
                    (define-key cx-map [?f] 'find-file)
                    (define-key cx-map [?s] 'save-buffer)
                    (define-key cx-map [?b] 'switch-buffer)
                    ;; C-x C-c sub-prefix (for deeply nested)
                    (define-key cx-map [3] cxc-map)
                    (define-key cxc-map [?q] 'quit-emacs)
                    ;; C-c sub-bindings
                    (define-key cc-map [?c] 'compile)
                    (define-key cc-map [?l] 'lint)
                    (list
                      ;; Direct lookups via root
                      (lookup-key root [24 ?f])
                      (lookup-key root [24 ?s])
                      (lookup-key root [3 ?c])
                      ;; Deep nesting: C-x C-c q
                      (lookup-key root [24 3 ?q])
                      ;; Prefix returns sub-keymap
                      (keymapp (lookup-key root [24]))
                      (keymapp (lookup-key root [3]))
                      ;; Unbound
                      (lookup-key root [24 ?z])
                      (lookup-key root [99])))"#;
    let expect =
        expect_test::expect![[r#""OK (find-file save-buffer compile quit-emacs t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymapp predicate on various objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymapp_various_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // keymapp should return t only for actual keymaps
    let form = r#"(list
                    (keymapp (make-keymap))
                    (keymapp (make-sparse-keymap))
                    (keymapp (make-sparse-keymap "menu"))
                    ;; Not keymaps
                    (keymapp nil)
                    (keymapp t)
                    (keymapp 42)
                    (keymapp "string")
                    (keymapp '(1 2 3))
                    (keymapp [1 2 3])
                    ;; A cons starting with 'keymap IS a keymap
                    (keymapp '(keymap))
                    (keymapp '(keymap (65 . self-insert-command)))
                    ;; But not just any list
                    (keymapp '(not-keymap))
                    ;; Keymap extracted from define-key prefix
                    (let ((m (make-sparse-keymap)))
                      (define-key m (kbd "C-x C-f") 'find-file)
                      (keymapp (lookup-key m (kbd "C-x")))))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil nil nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
