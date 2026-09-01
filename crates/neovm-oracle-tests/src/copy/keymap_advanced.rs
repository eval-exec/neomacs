//! Advanced oracle parity tests for `copy-keymap`.
//!
//! Tests independent copy semantics, nested sub-keymap deep copy,
//! parent keymap inheritance after copy, define-key independence
//! between copy and original, mode-specific keymap hierarchy building,
//! and copy-keymap on full (non-sparse) keymaps.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// copy-keymap creates an independent copy (basic mutations)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_creates_independent_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a keymap, copy it, then mutate both sides independently.
    // Verify that changes do not leak across the copy boundary.
    let form = r#"(progn
  (defvar neovm--ckadv-orig (make-sparse-keymap))
  (define-key neovm--ckadv-orig [?a] 'alpha)
  (define-key neovm--ckadv-orig [?b] 'bravo)
  (define-key neovm--ckadv-orig [?c] 'charlie)

  (unwind-protect
      (let ((copy (copy-keymap neovm--ckadv-orig)))
        ;; Mutate original: rebind a, add d
        (define-key neovm--ckadv-orig [?a] 'alpha-v2)
        (define-key neovm--ckadv-orig [?d] 'delta)
        ;; Mutate copy: rebind b, add e
        (define-key copy [?b] 'bravo-copy)
        (define-key copy [?e] 'echo-copy)
        (list
          ;; Original state
          (lookup-key neovm--ckadv-orig [?a])
          (lookup-key neovm--ckadv-orig [?b])
          (lookup-key neovm--ckadv-orig [?c])
          (lookup-key neovm--ckadv-orig [?d])
          (lookup-key neovm--ckadv-orig [?e])
          ;; Copy state
          (lookup-key copy [?a])
          (lookup-key copy [?b])
          (lookup-key copy [?c])
          (lookup-key copy [?d])
          (lookup-key copy [?e])
          ;; Both are still keymaps
          (keymapp neovm--ckadv-orig)
          (keymapp copy)
          ;; They are not eq
          (eq neovm--ckadv-orig copy)))
    (makunbound 'neovm--ckadv-orig)))"#;
    let expect = expect_test::expect![[
        r#""OK (alpha-v2 bravo charlie delta nil alpha bravo-copy charlie nil echo-copy t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modifying copy does NOT affect original (symmetric check)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_modify_copy_no_affect_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Make a keymap with several bindings, copy it, then ONLY modify the copy.
    // Verify the original is completely unchanged.
    let form = r#"(progn
  (defvar neovm--ckadv-m1 (make-sparse-keymap))
  (define-key neovm--ckadv-m1 [?x] 'cmd-x)
  (define-key neovm--ckadv-m1 [?y] 'cmd-y)
  (define-key neovm--ckadv-m1 [?z] 'cmd-z)

  (unwind-protect
      (let ((snapshot-before
             (list (lookup-key neovm--ckadv-m1 [?x])
                   (lookup-key neovm--ckadv-m1 [?y])
                   (lookup-key neovm--ckadv-m1 [?z]))))
        (let ((copy (copy-keymap neovm--ckadv-m1)))
          ;; Aggressively mutate the copy
          (define-key copy [?x] 'replaced-x)
          (define-key copy [?y] nil)
          (define-key copy [?z] 'replaced-z)
          (define-key copy [?w] 'new-w)
          (define-key copy [?v] 'new-v)
          ;; Snapshot original again
          (let ((snapshot-after
                 (list (lookup-key neovm--ckadv-m1 [?x])
                       (lookup-key neovm--ckadv-m1 [?y])
                       (lookup-key neovm--ckadv-m1 [?z])
                       (lookup-key neovm--ckadv-m1 [?w])
                       (lookup-key neovm--ckadv-m1 [?v]))))
            (list
              snapshot-before
              snapshot-after
              ;; Before and after should match for original's existing bindings
              (equal (nth 0 snapshot-before) (nth 0 snapshot-after))
              (equal (nth 1 snapshot-before) (nth 1 snapshot-after))
              (equal (nth 2 snapshot-before) (nth 2 snapshot-after))
              ;; Copy's state
              (lookup-key copy [?x])
              (lookup-key copy [?y])
              (lookup-key copy [?z])
              (lookup-key copy [?w])))))
    (makunbound 'neovm--ckadv-m1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cmd-x cmd-y cmd-z) (cmd-x cmd-y cmd-z nil nil) t t t replaced-x nil replaced-z new-w)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-keymap with nested sub-keymaps (prefix keys / submenus)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_nested_sub_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-keymap recursively copies sub-keymaps (prefix key maps).
    // Verify that modifying a sub-keymap in the copy does not affect
    // the original's sub-keymap.
    let form = r#"(progn
  (defvar neovm--ckadv-root (make-sparse-keymap))
  (defvar neovm--ckadv-sub (make-sparse-keymap))

  ;; Set up nested structure: root has prefix key C-x -> sub-keymap
  (define-key neovm--ckadv-sub [?f] 'find-file)
  (define-key neovm--ckadv-sub [?s] 'save-buffer)
  (define-key neovm--ckadv-sub [?b] 'switch-buffer)
  (define-key neovm--ckadv-root [24] neovm--ckadv-sub) ;; C-x = 24
  (define-key neovm--ckadv-root [?q] 'quit)

  (unwind-protect
      (let ((copy (copy-keymap neovm--ckadv-root)))
        ;; The sub-keymap in copy should be a different object
        (let ((orig-sub (lookup-key neovm--ckadv-root [24]))
              (copy-sub (lookup-key copy [24])))
          ;; Modify copy's sub-keymap
          (define-key copy [24 ?f] 'find-file-v2)
          (define-key copy [24 ?k] 'kill-buffer)
          ;; Modify original's sub-keymap
          (define-key neovm--ckadv-root [24 ?s] 'save-buffer-v2)
          (list
            ;; Original sub-keymap state
            (lookup-key neovm--ckadv-root [24 ?f])
            (lookup-key neovm--ckadv-root [24 ?s])
            (lookup-key neovm--ckadv-root [24 ?b])
            (lookup-key neovm--ckadv-root [24 ?k])
            ;; Copy sub-keymap state
            (lookup-key copy [24 ?f])
            (lookup-key copy [24 ?s])
            (lookup-key copy [24 ?b])
            (lookup-key copy [24 ?k])
            ;; Top-level bindings
            (lookup-key neovm--ckadv-root [?q])
            (lookup-key copy [?q])
            ;; Sub-keymaps are keymaps
            (keymapp orig-sub)
            (keymapp copy-sub)
            ;; Sub-keymaps are NOT eq (deep copy)
            (eq orig-sub copy-sub))))
    (makunbound 'neovm--ckadv-root)
    (makunbound 'neovm--ckadv-sub)))"#;
    let expect = expect_test::expect![[
        r#""OK (find-file save-buffer-v2 switch-buffer nil find-file-v2 save-buffer switch-buffer kill-buffer quit quit t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parent keymap inheritance after copy-keymap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-keymap preserves the parent keymap relationship.
    // The copy shares the same parent as the original.
    // Modifying the parent is visible through both original and copy.
    let form = r#"(progn
  (defvar neovm--ckadv-parent (make-sparse-keymap))
  (defvar neovm--ckadv-child (make-sparse-keymap))

  (define-key neovm--ckadv-parent [?a] 'parent-a)
  (define-key neovm--ckadv-parent [?b] 'parent-b)
  (define-key neovm--ckadv-parent [?c] 'parent-c)
  (define-key neovm--ckadv-child [?b] 'child-b)
  (set-keymap-parent neovm--ckadv-child neovm--ckadv-parent)

  (unwind-protect
      (let ((copy (copy-keymap neovm--ckadv-child)))
        ;; copy should have same parent
        (let ((orig-parent (keymap-parent neovm--ckadv-child))
              (copy-parent (keymap-parent copy)))
          ;; Modify parent — should be visible through both
          (define-key neovm--ckadv-parent [?a] 'parent-a-v2)
          (define-key neovm--ckadv-parent [?d] 'parent-d)
          ;; Add override in copy only
          (define-key copy [?c] 'copy-c)
          (list
            ;; Parent is shared (eq)
            (eq orig-parent copy-parent)
            ;; Inherited bindings through original
            (lookup-key neovm--ckadv-child [?a])
            (lookup-key neovm--ckadv-child [?b])
            (lookup-key neovm--ckadv-child [?c])
            (lookup-key neovm--ckadv-child [?d])
            ;; Inherited bindings through copy
            (lookup-key copy [?a])
            (lookup-key copy [?b])
            (lookup-key copy [?c])
            (lookup-key copy [?d])
            ;; Original child does not have copy's override
            (eq (lookup-key neovm--ckadv-child [?c])
                (lookup-key copy [?c])))))
    (makunbound 'neovm--ckadv-parent)
    (makunbound 'neovm--ckadv-child)))"#;
    let expect = expect_test::expect![[
        r#""OK (t parent-a-v2 child-b parent-c parent-d parent-a-v2 child-b copy-c parent-d nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key on copy vs original: full independence audit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_define_key_independence_audit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustively test that define-key operations on one keymap
    // do not interfere with the other. Uses a function to snapshot
    // all bindings at a given point in time.
    let form = r#"(progn
  (defvar neovm--ckadv-keys (list [?a] [?b] [?c] [?d] [?e] [?f]))

  (fset 'neovm--ckadv-snapshot
    (lambda (km keys)
      "Snapshot all bindings for KEYS in KM."
      (mapcar (lambda (k) (cons k (lookup-key km k))) keys)))

  (unwind-protect
      (let ((orig (make-sparse-keymap)))
        ;; Initial bindings: a, b, c
        (define-key orig [?a] 'cmd-a)
        (define-key orig [?b] 'cmd-b)
        (define-key orig [?c] 'cmd-c)
        (let* ((copy (copy-keymap orig))
               (snap-orig-0 (funcall 'neovm--ckadv-snapshot orig neovm--ckadv-keys))
               (snap-copy-0 (funcall 'neovm--ckadv-snapshot copy neovm--ckadv-keys)))
          ;; Phase 1: define-key only on original
          (define-key orig [?d] 'orig-d)
          (define-key orig [?a] 'orig-a-v2)
          (let ((snap-orig-1 (funcall 'neovm--ckadv-snapshot orig neovm--ckadv-keys))
                (snap-copy-1 (funcall 'neovm--ckadv-snapshot copy neovm--ckadv-keys)))
            ;; Phase 2: define-key only on copy
            (define-key copy [?e] 'copy-e)
            (define-key copy [?b] 'copy-b-v2)
            (let ((snap-orig-2 (funcall 'neovm--ckadv-snapshot orig neovm--ckadv-keys))
                  (snap-copy-2 (funcall 'neovm--ckadv-snapshot copy neovm--ckadv-keys)))
              ;; Phase 3: define-key on both simultaneously
              (define-key orig [?f] 'orig-f)
              (define-key copy [?f] 'copy-f)
              (let ((snap-orig-3 (funcall 'neovm--ckadv-snapshot orig neovm--ckadv-keys))
                    (snap-copy-3 (funcall 'neovm--ckadv-snapshot copy neovm--ckadv-keys)))
                (list
                  ;; After phase 0: snapshots should be equal
                  (equal snap-orig-0 snap-copy-0)
                  ;; After phase 1: copy unchanged from phase 0
                  (equal snap-copy-0 snap-copy-1)
                  ;; After phase 2: original unchanged from phase 1
                  (equal snap-orig-1 snap-orig-2)
                  ;; Phase 3: f is different between orig and copy
                  (lookup-key orig [?f])
                  (lookup-key copy [?f])
                  (eq (lookup-key orig [?f]) (lookup-key copy [?f]))
                  ;; Final snapshots
                  snap-orig-3
                  snap-copy-3))))))
    (fmakunbound 'neovm--ckadv-snapshot)
    (makunbound 'neovm--ckadv-keys)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t orig-f copy-f nil (([97] . orig-a-v2) ([98] . cmd-b) ([99] . cmd-c) ([100] . orig-d) ([101]) ([102] . orig-f)) (([97] . cmd-a) ([98] . copy-b-v2) ([99] . cmd-c) ([100]) ([101] . copy-e) ([102] . copy-f)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a mode-specific keymap hierarchy, copy and modify
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_mode_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a real mode keymap hierarchy:
    //   global-map -> text-mode-map -> markdown-mode-map
    // Then copy markdown-mode-map to create a variant (gfm-mode-map),
    // modify the variant, and verify full isolation.
    let form = r#"(progn
  (defvar neovm--ckadv-global (make-sparse-keymap))
  (defvar neovm--ckadv-text (make-sparse-keymap))
  (defvar neovm--ckadv-markdown (make-sparse-keymap))

  ;; Global bindings
  (define-key neovm--ckadv-global [?q] 'global-quit)
  (define-key neovm--ckadv-global [?h] 'global-help)
  (define-key neovm--ckadv-global [?s] 'global-save)

  ;; text-mode inherits from global
  (set-keymap-parent neovm--ckadv-text neovm--ckadv-global)
  (define-key neovm--ckadv-text [?f] 'text-fill)
  (define-key neovm--ckadv-text [?j] 'text-join)

  ;; markdown-mode inherits from text-mode
  (set-keymap-parent neovm--ckadv-markdown neovm--ckadv-text)
  (define-key neovm--ckadv-markdown [?b] 'md-bold)
  (define-key neovm--ckadv-markdown [?i] 'md-italic)
  (define-key neovm--ckadv-markdown [?l] 'md-link)
  ;; markdown-mode has a prefix sub-keymap for headings
  (let ((heading-map (make-sparse-keymap)))
    (define-key heading-map [?1] 'md-h1)
    (define-key heading-map [?2] 'md-h2)
    (define-key heading-map [?3] 'md-h3)
    (define-key neovm--ckadv-markdown [?#] heading-map))

  (unwind-protect
      (let ((gfm (copy-keymap neovm--ckadv-markdown)))
        ;; gfm-mode overrides and adds
        (define-key gfm [?t] 'gfm-table)
        (define-key gfm [?b] 'gfm-bold)
        (define-key gfm [?c] 'gfm-checkbox)
        ;; Modify heading sub-map in gfm
        (define-key gfm [?# ?4] 'gfm-h4)
        (define-key gfm [?# ?1] 'gfm-h1)
        ;; gfm gets a different parent (just text-mode, not modifying chain)
        (list
          ;; Inheritance chain for markdown
          (lookup-key neovm--ckadv-markdown [?q])  ;; global
          (lookup-key neovm--ckadv-markdown [?f])  ;; text
          (lookup-key neovm--ckadv-markdown [?b])  ;; markdown own
          (lookup-key neovm--ckadv-markdown [?# ?1]) ;; sub-keymap
          (lookup-key neovm--ckadv-markdown [?# ?3])
          (lookup-key neovm--ckadv-markdown [?t])  ;; not bound
          ;; GFM overrides
          (lookup-key gfm [?q])  ;; inherited from global via text
          (lookup-key gfm [?f])  ;; inherited from text
          (lookup-key gfm [?b])  ;; overridden by gfm
          (lookup-key gfm [?t])  ;; gfm-specific
          (lookup-key gfm [?c])  ;; gfm-specific
          (lookup-key gfm [?# ?1])  ;; overridden in gfm sub
          (lookup-key gfm [?# ?2])  ;; original from markdown sub
          (lookup-key gfm [?# ?4])  ;; gfm-specific sub
          ;; Verify markdown heading sub-map is untouched
          (lookup-key neovm--ckadv-markdown [?# ?1])
          (lookup-key neovm--ckadv-markdown [?# ?4])
          ;; Both keymaps share the same parent chain
          (eq (keymap-parent gfm) (keymap-parent neovm--ckadv-markdown))
          ;; Summary counts
          (keymapp gfm)
          (keymapp neovm--ckadv-markdown)))
    (makunbound 'neovm--ckadv-global)
    (makunbound 'neovm--ckadv-text)
    (makunbound 'neovm--ckadv-markdown)))"#;
    let expect = expect_test::expect![[
        r#""OK (global-quit text-fill md-bold md-h1 md-h3 nil global-quit text-fill gfm-bold gfm-table gfm-checkbox gfm-h1 md-h2 gfm-h4 md-h1 nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-keymap on a full (non-sparse) keymap with char-table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_keymap_full_keymap_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-keymap creates a full keymap with a char-table.
    // copy-keymap should copy that char-table too.
    let form = r#"(progn
  (defvar neovm--ckadv-full (make-keymap))
  (define-key neovm--ckadv-full [?a] 'cmd-a)
  (define-key neovm--ckadv-full [?z] 'cmd-z)
  (define-key neovm--ckadv-full [?\C-a] 'cmd-ctrl-a)

  (unwind-protect
      (let ((copy (copy-keymap neovm--ckadv-full)))
        ;; Both should have char-tables
        (let ((orig-ct (char-table-p (cadr neovm--ckadv-full)))
              (copy-ct (char-table-p (cadr copy))))
          ;; Modify copy
          (define-key copy [?a] 'copy-cmd-a)
          (define-key copy [?m] 'copy-cmd-m)
          ;; Modify original
          (define-key neovm--ckadv-full [?z] 'orig-cmd-z-v2)
          (list
            ;; Char-table presence
            orig-ct
            copy-ct
            ;; Original bindings
            (lookup-key neovm--ckadv-full [?a])
            (lookup-key neovm--ckadv-full [?z])
            (lookup-key neovm--ckadv-full [?\C-a])
            (lookup-key neovm--ckadv-full [?m])
            ;; Copy bindings
            (lookup-key copy [?a])
            (lookup-key copy [?z])
            (lookup-key copy [?\C-a])
            (lookup-key copy [?m])
            ;; Not eq
            (eq neovm--ckadv-full copy))))
    (makunbound 'neovm--ckadv-full)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t cmd-a orig-cmd-z-v2 cmd-ctrl-a nil copy-cmd-a cmd-z cmd-ctrl-a copy-cmd-m nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
