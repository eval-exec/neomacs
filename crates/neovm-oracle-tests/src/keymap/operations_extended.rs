//! Extended oracle parity tests for keymap operations.
//!
//! Tests `make-keymap`, `make-sparse-keymap`, `define-key` with various
//! key sequences, `lookup-key` with fallthrough, `set-keymap-parent` chains,
//! `keymap-parent`, `copy-keymap` independence, `keymapp`, prefix keymaps,
//! and menu-item entries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Complex multi-level prefix keymap hierarchy with inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_multi_level_prefix_with_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a 4-level keymap hierarchy with prefix keys at each level.
    // Parent bindings should be visible in children unless shadowed.
    let form = r#"(let ((base (make-sparse-keymap))
                        (mode (make-sparse-keymap))
                        (local (make-sparse-keymap)))
  ;; Base map: global bindings
  (define-key base [?q] 'base-quit)
  (define-key base (kbd "C-x C-s") 'base-save)
  (define-key base (kbd "C-x C-f") 'base-find)
  (define-key base (kbd "C-c C-c") 'base-compile)
  ;; Mode map inherits from base, adds and overrides
  (set-keymap-parent mode base)
  (define-key mode (kbd "C-c C-c") 'mode-compile)
  (define-key mode (kbd "C-c C-k") 'mode-kill)
  (define-key mode [?q] 'mode-quit)
  ;; Local map inherits from mode, further overrides
  (set-keymap-parent local mode)
  (define-key local (kbd "C-x C-s") 'local-save)
  (define-key local (kbd "C-c C-r") 'local-run)
  (list
    ;; Lookup through full chain
    (lookup-key local [?q])             ;; mode-quit (mode shadows base)
    (lookup-key local (kbd "C-x C-s"))  ;; local-save (local shadows base)
    (lookup-key local (kbd "C-x C-f"))  ;; base-find (inherited from base)
    (lookup-key local (kbd "C-c C-c"))  ;; mode-compile (mode shadows base)
    (lookup-key local (kbd "C-c C-k"))  ;; mode-kill (from mode)
    (lookup-key local (kbd "C-c C-r"))  ;; local-run (from local)
    ;; Lookup in mode only
    (lookup-key mode (kbd "C-x C-s"))   ;; base-save (base visible)
    (lookup-key mode (kbd "C-c C-r"))   ;; nil (not in mode or base)
    ;; Verify parent chain
    (eq (keymap-parent local) mode)
    (eq (keymap-parent mode) base)
    (null (keymap-parent base))
    ;; Prefix sub-keymaps are themselves keymaps
    (keymapp (lookup-key local (kbd "C-x")))
    (keymapp (lookup-key local (kbd "C-c")))))"#;
    let expect = expect_test::expect![[
        r#""OK (mode-quit local-save base-find mode-compile mode-kill local-run base-save nil t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-keymap with prefix sub-keymaps: deep copy independence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_copy_prefix_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a keymap with prefix keys (multi-key bindings create sub-keymaps).
    // Copy it. Modify both independently. Verify sub-keymaps are also independent.
    let form = r#"(let ((orig (make-sparse-keymap)))
  (define-key orig (kbd "C-x C-f") 'orig-find)
  (define-key orig (kbd "C-x C-s") 'orig-save)
  (define-key orig (kbd "C-c C-c") 'orig-compile)
  (define-key orig [?a] 'orig-a)
  (let ((copy (copy-keymap orig)))
    ;; Modify orig's prefix sub-keymap
    (define-key orig (kbd "C-x C-f") 'new-find)
    (define-key orig (kbd "C-x C-b") 'new-buf)
    ;; Modify copy's prefix sub-keymap
    (define-key copy (kbd "C-c C-c") 'copy-compile)
    (define-key copy (kbd "C-c C-k") 'copy-kill)
    (define-key copy [?a] 'copy-a)
    (list
      ;; Orig state
      (lookup-key orig (kbd "C-x C-f"))  ;; new-find
      (lookup-key orig (kbd "C-x C-s"))  ;; orig-save
      (lookup-key orig (kbd "C-x C-b"))  ;; new-buf
      (lookup-key orig (kbd "C-c C-c"))  ;; orig-compile (copy's change didn't affect)
      (lookup-key orig (kbd "C-c C-k"))  ;; nil
      (lookup-key orig [?a])             ;; orig-a
      ;; Copy state
      (lookup-key copy (kbd "C-x C-f"))  ;; orig-find (orig's change didn't affect)
      (lookup-key copy (kbd "C-x C-s"))  ;; orig-save
      (lookup-key copy (kbd "C-x C-b"))  ;; nil (orig's addition didn't affect)
      (lookup-key copy (kbd "C-c C-c"))  ;; copy-compile
      (lookup-key copy (kbd "C-c C-k"))  ;; copy-kill
      (lookup-key copy [?a])             ;; copy-a
      ;; Both still keymaps
      (keymapp orig)
      (keymapp copy))))"#;
    let expect = expect_test::expect![[
        r#""OK (new-find orig-save new-buf orig-compile nil orig-a orig-find orig-save nil copy-compile copy-kill copy-a t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with different key specification forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_define_key_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test various ways to specify keys: vectors with char literals,
    // kbd strings, integer key codes, and function key symbols.
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Character via vector with char literal
  (define-key m [?x] 'cmd-x)
  ;; Integer key code (tab = 9)
  (define-key m [9] 'cmd-tab)
  ;; kbd-based string
  (define-key m (kbd "C-a") 'cmd-c-a)
  ;; Meta key via kbd
  (define-key m (kbd "M-x") 'cmd-m-x)
  ;; Function key symbol
  (define-key m [f1] 'cmd-f1)
  (define-key m [f2] 'cmd-f2)
  ;; Return / backspace
  (define-key m [return] 'cmd-return)
  (define-key m [backspace] 'cmd-backspace)
  ;; Multi-key with function key prefix
  (define-key m [f1 ?a] 'cmd-f1-a)
  (define-key m [f1 ?b] 'cmd-f1-b)
  (list
    (lookup-key m [?x])
    (lookup-key m [9])
    (lookup-key m (kbd "C-a"))
    (lookup-key m (kbd "M-x"))
    (lookup-key m [f1])
    ;; f1 is now a prefix since we defined f1-a and f1-b
    ;; So lookup-key [f1] should return a sub-keymap
    (keymapp (lookup-key m [f1]))
    (lookup-key m [f1 ?a])
    (lookup-key m [f1 ?b])
    (lookup-key m [f2])
    (lookup-key m [return])
    (lookup-key m [backspace])
    ;; Unbound
    (lookup-key m [f3])
    (lookup-key m [?z])))"#;
    let expect = expect_test::expect![[
        r#""ERR (error \"Key sequence <f1> a starts with non-prefix key <f1>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-keymap-parent chain modification and re-parenting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_reparenting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create keymaps, set up parent chain, then re-parent to test
    // that lookups change accordingly.
    let form = r#"(let ((gp1 (make-sparse-keymap))
                        (gp2 (make-sparse-keymap))
                        (parent (make-sparse-keymap))
                        (child (make-sparse-keymap)))
  ;; Two different grandparent candidates
  (define-key gp1 [?a] 'gp1-a)
  (define-key gp1 [?b] 'gp1-b)
  (define-key gp2 [?a] 'gp2-a)
  (define-key gp2 [?c] 'gp2-c)
  ;; Parent has its own binding
  (define-key parent [?d] 'parent-d)
  (define-key child [?e] 'child-e)
  ;; Set up chain: child -> parent -> gp1
  (set-keymap-parent parent gp1)
  (set-keymap-parent child parent)
  (let ((before-a (lookup-key child [?a]))   ;; gp1-a
        (before-b (lookup-key child [?b]))   ;; gp1-b
        (before-c (lookup-key child [?c]))   ;; nil
        (before-d (lookup-key child [?d]))   ;; parent-d
        (before-e (lookup-key child [?e])))  ;; child-e
    ;; Re-parent: parent -> gp2 (instead of gp1)
    (set-keymap-parent parent gp2)
    (let ((after-a (lookup-key child [?a]))   ;; gp2-a (changed!)
          (after-b (lookup-key child [?b]))   ;; nil (gp1-b gone)
          (after-c (lookup-key child [?c]))   ;; gp2-c (new!)
          (after-d (lookup-key child [?d]))   ;; parent-d (unchanged)
          (after-e (lookup-key child [?e])))  ;; child-e (unchanged)
      ;; Now detach child from parent entirely
      (set-keymap-parent child nil)
      (let ((detached-a (lookup-key child [?a]))
            (detached-d (lookup-key child [?d]))
            (detached-e (lookup-key child [?e])))
        (list
          before-a before-b before-c before-d before-e
          after-a after-b after-c after-d after-e
          detached-a detached-d detached-e
          ;; parent chain verification
          (null (keymap-parent child))
          (eq (keymap-parent parent) gp2))))))"#;
    let expect = expect_test::expect![[
        r#""OK (gp1-a gp1-b nil parent-d child-e gp2-a nil gp2-c parent-d child-e nil nil child-e t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full keymap with char-table bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_full_keymap_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-keymap creates a full keymap with a char-table as default bindings.
    // Test that bindings set in the char-table work, and that sparse additions
    // also work alongside.
    let form = r#"(let ((full (make-keymap)))
  ;; Define keys in the full keymap
  (define-key full [?a] 'cmd-a)
  (define-key full [?b] 'cmd-b)
  (define-key full [?z] 'cmd-z)
  ;; Function keys (go into the sparse part)
  (define-key full [f5] 'cmd-f5)
  ;; Prefix key
  (define-key full (kbd "C-c C-t") 'cmd-cc-ct)
  ;; Verify char-table is there
  (let ((has-char-table (char-table-p (cadr full))))
    (list
      has-char-table
      (lookup-key full [?a])
      (lookup-key full [?b])
      (lookup-key full [?z])
      ;; Characters not explicitly bound should return nil
      ;; (the char-table default is nil)
      (lookup-key full [?m])
      ;; Function key
      (lookup-key full [f5])
      ;; Prefix
      (lookup-key full (kbd "C-c C-t"))
      (keymapp (lookup-key full (kbd "C-c"))))))"#;
    let expect = expect_test::expect![[r#""OK (t cmd-a cmd-b cmd-z nil cmd-f5 cmd-cc-ct t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key with accept-default and integer return for too-long keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_lookup_key_too_long() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When lookup-key encounters a non-prefix binding before consuming
    // all keys in the sequence, it returns the number of keys consumed.
    let form = r#"(let ((m (make-sparse-keymap)))
  (define-key m [?a] 'cmd-a)
  (define-key m (kbd "C-x C-f") 'cmd-find)
  (define-key m (kbd "C-x C-s") 'cmd-save)
  (list
    ;; Normal lookup
    (lookup-key m [?a])
    ;; Too-long: ?a is bound to a command, not a prefix
    ;; looking up [?a ?b] returns 1 (consumed 1 key before hitting non-prefix)
    (lookup-key m [?a ?b])
    (lookup-key m [?a ?b ?c])
    ;; Prefix works fine for two keys
    (lookup-key m (kbd "C-x C-f"))
    ;; Too-long: C-x C-f is a command, C-x C-f C-x is too long
    ;; Returns 2 (consumed 2 keys)
    (let ((r (lookup-key m [24 6 24])))
      (list (numberp r) r))
    ;; Completely unbound: returns nil
    (lookup-key m [?z])
    ;; Unbound prefix continuation: C-x C-z is nil (C-x exists but C-z doesn't)
    (lookup-key m [24 26])))"#;
    let expect = expect_test::expect![[r#""OK (cmd-a 1 1 cmd-find (t 2) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building a mode keymap system with multiple inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_ext_mode_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a major mode keymap system: global-map -> prog-mode-map -> my-mode-map,
    // with a minor mode override map composed on top.
    let form = r#"(let ((global-km (make-sparse-keymap))
                        (prog-km (make-sparse-keymap))
                        (my-km (make-sparse-keymap))
                        (minor-km (make-sparse-keymap)))
  ;; Global bindings
  (define-key global-km [?q] 'global-quit)
  (define-key global-km (kbd "C-g") 'global-keyboard-quit)
  (define-key global-km (kbd "C-x C-c") 'global-exit)
  (define-key global-km [f1] 'global-help)
  ;; Prog-mode inherits from global
  (set-keymap-parent prog-km global-km)
  (define-key prog-km (kbd "C-c C-c") 'prog-compile)
  (define-key prog-km (kbd "C-c C-l") 'prog-lint)
  (define-key prog-km [f5] 'prog-run)
  ;; My-mode inherits from prog-mode
  (set-keymap-parent my-km prog-km)
  (define-key my-km (kbd "C-c C-c") 'my-compile)
  (define-key my-km (kbd "C-c C-t") 'my-test)
  (define-key my-km [f6] 'my-debug)
  ;; Minor mode has highest priority (composed via make-composed-keymap or manual override)
  ;; We'll simulate by checking minor-km first, then my-km
  (define-key minor-km (kbd "C-c C-c") 'minor-override-compile)
  (define-key minor-km [f7] 'minor-special)
  ;; Lookup function: check minor first, then my-km chain
  (let ((lookup (lambda (key)
                  (let ((r (lookup-key minor-km key)))
                    (if (and r (not (numberp r)))
                        r
                      (lookup-key my-km key))))))
    (list
      ;; Minor overrides
      (funcall lookup (kbd "C-c C-c"))   ;; minor-override-compile
      (funcall lookup [f7])              ;; minor-special
      ;; Falls through to my-km chain
      (funcall lookup (kbd "C-c C-t"))   ;; my-test
      (funcall lookup [f5])              ;; prog-run
      (funcall lookup [f6])              ;; my-debug
      (funcall lookup (kbd "C-g"))       ;; global-keyboard-quit
      (funcall lookup (kbd "C-x C-c"))   ;; global-exit
      (funcall lookup [f1])              ;; global-help
      ;; prog-lint still accessible
      (funcall lookup (kbd "C-c C-l"))   ;; prog-lint
      ;; Unbound
      (funcall lookup [f8])              ;; nil
      ;; Count bindings per level
      (length global-km)
      (length prog-km)
      (length my-km))))"#;
    let expect = expect_test::expect![[
        r#""OK (minor-override-compile minor-special my-test prog-run my-debug global-keyboard-quit global-exit global-help prog-lint nil 5 8 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
