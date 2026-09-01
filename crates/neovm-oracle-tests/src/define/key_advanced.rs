//! Advanced oracle parity tests for `define-key` with ALL parameter combinations
//! and complex patterns: string key sequences, vector key sequences, lambda bindings,
//! symbol bindings, nil unbinding, prefix keys with nested keymaps, modifier keys,
//! complete keymap hierarchies, and keymap inheritance with define-key overrides.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// define-key with string key sequences (kbd-style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_string_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test define-key with various string-based key specifications via kbd
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Single key strings
  (define-key m (kbd "a") 'cmd-a)
  (define-key m (kbd "b") 'cmd-b)
  (define-key m (kbd "RET") 'cmd-ret)
  (define-key m (kbd "TAB") 'cmd-tab)
  (define-key m (kbd "SPC") 'cmd-spc)
  (define-key m (kbd "DEL") 'cmd-del)
  ;; Multi-key string sequences
  (define-key m (kbd "C-x f") 'cmd-cx-f)
  (define-key m (kbd "C-c C-c") 'cmd-cc-cc)
  (define-key m (kbd "C-x 4 f") 'cmd-cx-4-f)
  (list
    (lookup-key m (kbd "a"))
    (lookup-key m (kbd "b"))
    (lookup-key m (kbd "RET"))
    (lookup-key m (kbd "TAB"))
    (lookup-key m (kbd "SPC"))
    (lookup-key m (kbd "DEL"))
    (lookup-key m (kbd "C-x f"))
    (lookup-key m (kbd "C-c C-c"))
    ;; Three-key sequence creates two levels of prefix
    (lookup-key m (kbd "C-x 4 f"))
    (keymapp (lookup-key m (kbd "C-x")))
    (keymapp (lookup-key m (kbd "C-x 4")))
    ;; Verify the intermediate prefix for C-c
    (keymapp (lookup-key m (kbd "C-c")))))"#;
    let expect = expect_test::expect![[
        r#""OK (cmd-a cmd-b cmd-ret cmd-tab cmd-spc cmd-del cmd-cx-f cmd-cc-cc cmd-cx-4-f t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with vector key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_vector_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test define-key with vector-based key specifications
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Single character vectors
  (define-key m [?a] 'cmd-a)
  (define-key m [?Z] 'cmd-z-upper)
  (define-key m [?0] 'cmd-zero)
  ;; Integer key codes
  (define-key m [13] 'cmd-return)    ;; RET
  (define-key m [9] 'cmd-tab)        ;; TAB
  (define-key m [27] 'cmd-escape)    ;; ESC
  ;; Function key symbols in vectors
  (define-key m [f1] 'cmd-f1)
  (define-key m [f12] 'cmd-f12)
  (define-key m [home] 'cmd-home)
  (define-key m [end] 'cmd-end)
  ;; Multi-element vectors
  (define-key m [?a ?b] 'cmd-ab)
  (define-key m [?a ?c] 'cmd-ac)
  (define-key m [f1 ?x] 'cmd-f1x)
  (define-key m [f1 ?y ?z] 'cmd-f1yz)
  (list
    ;; Single keys
    (lookup-key m [?Z])
    (lookup-key m [?0])
    (lookup-key m [13])
    (lookup-key m [9])
    (lookup-key m [27])
    (lookup-key m [f1])
    ;; f1 is now a prefix because we also defined [f1 ?x]
    (keymapp (lookup-key m [f1]))
    (lookup-key m [f12])
    (lookup-key m [home])
    (lookup-key m [end])
    ;; Multi-key via vector
    (lookup-key m [?a ?b])
    (lookup-key m [?a ?c])
    ;; ?a is now a prefix
    (keymapp (lookup-key m [?a]))
    ;; Three-level: f1 -> y -> z
    (lookup-key m [f1 ?x])
    (lookup-key m [f1 ?y ?z])
    (keymapp (lookup-key m [f1 ?y]))))"#;
    let expect = expect_test::expect![[
        r#""ERR (error \"Key sequence a b starts with non-prefix key a\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with lambda function bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_lambda_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bind keys to lambda functions and verify they are retrievable
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Bind to a simple lambda
  (define-key m [?a] (lambda () (+ 1 2)))
  ;; Bind to a lambda with args
  (define-key m [?b] (lambda (x) (* x x)))
  ;; Bind to an interactive lambda (a command)
  (define-key m [?c] (lambda () (interactive) (message "hello")))
  ;; Bind to a lambda with docstring
  (define-key m [?d] (lambda () "my docstring" (interactive) nil))
  ;; Bind prefix key to lambda at the leaf
  (define-key m (kbd "C-c a") (lambda () (interactive) 'compiled))
  (list
    ;; All bindings should be functions
    (functionp (lookup-key m [?a]))
    (functionp (lookup-key m [?b]))
    (functionp (lookup-key m [?c]))
    (functionp (lookup-key m [?d]))
    (functionp (lookup-key m (kbd "C-c a")))
    ;; Interactive lambdas should be commands
    (commandp (lookup-key m [?c]))
    (commandp (lookup-key m [?d]))
    ;; Non-interactive lambdas are not commands
    (commandp (lookup-key m [?a]))
    (commandp (lookup-key m [?b]))
    ;; Call the lambda to check it works
    (funcall (lookup-key m [?a]))
    (funcall (lookup-key m [?b]) 7)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil nil 3 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with symbol bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_symbol_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bind keys to symbol names (the normal way) and verify
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Bind to various symbols
  (define-key m [?a] 'self-insert-command)
  (define-key m [?b] 'forward-char)
  (define-key m [?c] 'backward-char)
  (define-key m (kbd "C-x C-f") 'find-file)
  (define-key m (kbd "C-x C-s") 'save-buffer)
  (define-key m [f1] 'help-command)
  ;; Bind to a symbol that might not be fboundp
  (define-key m [?z] 'nonexistent-command-xyz)
  (list
    ;; All lookups should return the symbol
    (eq (lookup-key m [?a]) 'self-insert-command)
    (eq (lookup-key m [?b]) 'forward-char)
    (eq (lookup-key m [?c]) 'backward-char)
    (eq (lookup-key m (kbd "C-x C-f")) 'find-file)
    (eq (lookup-key m (kbd "C-x C-s")) 'save-buffer)
    (eq (lookup-key m [f1]) 'help-command)
    ;; Even unbound symbols are stored as-is
    (eq (lookup-key m [?z]) 'nonexistent-command-xyz)
    ;; Symbolp check
    (symbolp (lookup-key m [?a]))
    (symbolp (lookup-key m [?z]))
    ;; Verify the symbol is exactly what we set
    (lookup-key m [?a])
    (lookup-key m (kbd "C-x C-f"))))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t t self-insert-command find-file)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with nil (unbinding keys)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_nil_unbind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bind keys then unbind them with nil, verify behavior
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Set up initial bindings
  (define-key m [?a] 'cmd-a)
  (define-key m [?b] 'cmd-b)
  (define-key m [?c] 'cmd-c)
  (define-key m (kbd "C-x C-f") 'cmd-find)
  (define-key m (kbd "C-x C-s") 'cmd-save)
  ;; Capture before state
  (let ((before-a (lookup-key m [?a]))
        (before-b (lookup-key m [?b]))
        (before-c (lookup-key m [?c]))
        (before-find (lookup-key m (kbd "C-x C-f")))
        (before-save (lookup-key m (kbd "C-x C-s"))))
    ;; Unbind some keys
    (define-key m [?a] nil)
    (define-key m [?c] nil)
    (define-key m (kbd "C-x C-f") nil)
    ;; Capture after state
    (let ((after-a (lookup-key m [?a]))
          (after-b (lookup-key m [?b]))
          (after-c (lookup-key m [?c]))
          (after-find (lookup-key m (kbd "C-x C-f")))
          (after-save (lookup-key m (kbd "C-x C-s"))))
      ;; Rebind a to something new
      (define-key m [?a] 'new-cmd-a)
      (list
        ;; Before
        before-a before-b before-c before-find before-save
        ;; After unbind
        after-a after-b after-c after-find after-save
        ;; Rebind works
        (lookup-key m [?a])
        ;; C-x prefix still exists even though C-x C-f was unbound
        (keymapp (lookup-key m (kbd "C-x")))))))"#;
    let expect = expect_test::expect![[
        r#""OK (cmd-a cmd-b cmd-c cmd-find cmd-save nil cmd-b nil nil cmd-save new-cmd-a t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with prefix keys and nested keymaps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_prefix_nested_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test deeply nested prefix key structures and explicit sub-keymap assignment
    let form = r#"(let ((root (make-sparse-keymap))
                        (sub1 (make-sparse-keymap))
                        (sub2 (make-sparse-keymap))
                        (sub3 (make-sparse-keymap)))
  ;; Manually assign sub-keymaps as prefix bindings
  (define-key root [?x] sub1)
  (define-key sub1 [?y] sub2)
  (define-key sub2 [?z] sub3)
  ;; Add leaf bindings at various depths
  (define-key root [?a] 'root-a)
  (define-key sub1 [?a] 'sub1-a)
  (define-key sub2 [?a] 'sub2-a)
  (define-key sub3 [?a] 'sub3-a)
  ;; Also use the multi-key define-key which auto-creates prefix keymaps
  (define-key root (kbd "C-c p q r") 'deep-cmd)
  (list
    ;; Direct lookups
    (lookup-key root [?a])
    ;; Through explicit sub-keymaps
    (lookup-key root [?x ?a])
    (lookup-key root [?x ?y ?a])
    (lookup-key root [?x ?y ?z ?a])
    ;; Prefix lookups return keymaps
    (keymapp (lookup-key root [?x]))
    (keymapp (lookup-key root [?x ?y]))
    (keymapp (lookup-key root [?x ?y ?z]))
    ;; The explicit sub-keymaps are the same objects
    (eq (lookup-key root [?x]) sub1)
    (eq (lookup-key root [?x ?y]) sub2)
    (eq (lookup-key root [?x ?y ?z]) sub3)
    ;; Auto-created prefix structure
    (lookup-key root (kbd "C-c p q r"))
    (keymapp (lookup-key root (kbd "C-c")))
    (keymapp (lookup-key root (kbd "C-c p")))
    (keymapp (lookup-key root (kbd "C-c p q")))
    ;; Too-long returns integer
    (numberp (lookup-key root [?a ?b]))))"#;
    let expect = expect_test::expect![[
        r#""OK (root-a sub1-a sub2-a sub3-a t t t t t t deep-cmd t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with modifier keys (C-, M-, S-, etc.)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_modifier_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test all major modifier key combinations
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Control modifier
  (define-key m (kbd "C-a") 'ctrl-a)
  (define-key m (kbd "C-z") 'ctrl-z)
  ;; Meta modifier
  (define-key m (kbd "M-a") 'meta-a)
  (define-key m (kbd "M-z") 'meta-z)
  ;; Control-Meta
  (define-key m (kbd "C-M-a") 'ctrl-meta-a)
  (define-key m (kbd "C-M-z") 'ctrl-meta-z)
  ;; Shift modifier (with function keys and special keys)
  (define-key m (kbd "S-<return>") 'shift-return)
  (define-key m [S-f1] 'shift-f1)
  (define-key m [C-f1] 'ctrl-f1)
  (define-key m [M-f1] 'meta-f1)
  ;; Combined modifiers on function keys
  (define-key m [C-S-f2] 'ctrl-shift-f2)
  ;; Modifier key sequences
  (define-key m (kbd "C-x C-a") 'cx-ca)
  (define-key m (kbd "C-c M-a") 'cc-ma)
  (define-key m (kbd "M-g M-g") 'mg-mg)
  (list
    ;; Single modifier
    (lookup-key m (kbd "C-a"))
    (lookup-key m (kbd "C-z"))
    (lookup-key m (kbd "M-a"))
    (lookup-key m (kbd "M-z"))
    ;; Double modifier
    (lookup-key m (kbd "C-M-a"))
    (lookup-key m (kbd "C-M-z"))
    ;; Shift + special
    (lookup-key m (kbd "S-<return>"))
    (lookup-key m [S-f1])
    (lookup-key m [C-f1])
    (lookup-key m [M-f1])
    ;; Combined
    (lookup-key m [C-S-f2])
    ;; Modifier sequences
    (lookup-key m (kbd "C-x C-a"))
    (lookup-key m (kbd "C-c M-a"))
    (lookup-key m (kbd "M-g M-g"))
    ;; Unbound modifier combos
    (lookup-key m (kbd "C-b"))
    (lookup-key m (kbd "M-b"))))"#;
    let expect = expect_test::expect![[
        r#""OK (ctrl-a ctrl-z meta-a meta-z ctrl-meta-a ctrl-meta-z shift-return shift-f1 ctrl-f1 meta-f1 ctrl-shift-f2 cx-ca cc-ma mg-mg nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a complete keymap hierarchy with multiple prefixes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_complete_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a realistic Emacs-like keymap hierarchy with global, mode, and local
    let form = r#"(let ((global (make-sparse-keymap))
                        (mode (make-sparse-keymap))
                        (local (make-sparse-keymap)))
  ;; Global map: essential bindings
  (define-key global (kbd "C-x C-f") 'find-file)
  (define-key global (kbd "C-x C-s") 'save-buffer)
  (define-key global (kbd "C-x C-c") 'save-buffers-kill-terminal)
  (define-key global (kbd "C-x b") 'switch-to-buffer)
  (define-key global (kbd "C-x k") 'kill-buffer)
  (define-key global (kbd "C-x 0") 'delete-window)
  (define-key global (kbd "C-x 1") 'delete-other-windows)
  (define-key global (kbd "C-x 2") 'split-window-below)
  (define-key global (kbd "C-x 3") 'split-window-right)
  (define-key global (kbd "C-g") 'keyboard-quit)
  (define-key global (kbd "M-x") 'execute-extended-command)
  (define-key global [f1] 'help-command)

  ;; Mode map: programming mode bindings (inherits global)
  (set-keymap-parent mode global)
  (define-key mode (kbd "C-c C-c") 'compile)
  (define-key mode (kbd "C-c C-k") 'kill-compilation)
  (define-key mode (kbd "C-c C-l") 'lint)
  (define-key mode (kbd "C-c C-r") 'run)
  (define-key mode (kbd "C-c C-t") 'test)
  (define-key mode (kbd "C-c C-d") 'debug)
  (define-key mode (kbd "M-.") 'find-definition)
  (define-key mode (kbd "M-,") 'pop-tag-mark)

  ;; Local map: buffer-specific overrides (inherits mode)
  (set-keymap-parent local mode)
  (define-key local (kbd "C-c C-c") 'local-compile)
  (define-key local (kbd "C-c C-e") 'local-eval)
  (define-key local (kbd "C-x C-s") 'local-save-and-lint)

  (list
    ;; Local inherits everything, with overrides
    (lookup-key local (kbd "C-c C-c"))   ;; local-compile (overrides mode)
    (lookup-key local (kbd "C-c C-e"))   ;; local-eval (local only)
    (lookup-key local (kbd "C-c C-k"))   ;; kill-compilation (from mode)
    (lookup-key local (kbd "C-c C-l"))   ;; lint (from mode)
    (lookup-key local (kbd "C-x C-f"))   ;; find-file (from global)
    (lookup-key local (kbd "C-x C-s"))   ;; local-save-and-lint (overrides global)
    (lookup-key local (kbd "C-g"))       ;; keyboard-quit (from global)
    (lookup-key local (kbd "M-x"))       ;; execute-extended-command (from global)
    (lookup-key local (kbd "M-."))       ;; find-definition (from mode)
    (lookup-key local [f1])              ;; help-command (from global)

    ;; Mode doesn't see local bindings
    (lookup-key mode (kbd "C-c C-c"))    ;; compile (mode's own)
    (lookup-key mode (kbd "C-c C-e"))    ;; nil (local only)
    (lookup-key mode (kbd "C-x C-s"))    ;; save-buffer (from global)

    ;; Global doesn't see any children
    (lookup-key global (kbd "C-c C-c"))  ;; nil
    (lookup-key global (kbd "M-."))      ;; nil

    ;; Window management from global visible everywhere
    (lookup-key local (kbd "C-x 0"))
    (lookup-key local (kbd "C-x 1"))
    (lookup-key local (kbd "C-x 2"))
    (lookup-key local (kbd "C-x 3"))
    (lookup-key local (kbd "C-x b"))
    (lookup-key local (kbd "C-x k"))))"#;
    let expect = expect_test::expect![[
        r#""OK (local-compile local-eval kill-compilation lint find-file local-save-and-lint keyboard-quit execute-extended-command find-definition help-command compile nil save-buffer 1 nil delete-window delete-other-windows split-window-below split-window-right switch-to-buffer kill-buffer)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: keymap inheritance with define-key overrides and restoration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_inheritance_overrides() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that overriding and then removing bindings properly falls through
    // to parent, and that re-parenting changes visible bindings
    let form = r#"(let ((base (make-sparse-keymap))
                        (mid (make-sparse-keymap))
                        (top (make-sparse-keymap))
                        (alt-base (make-sparse-keymap)))
  ;; Base bindings
  (define-key base [?a] 'base-a)
  (define-key base [?b] 'base-b)
  (define-key base [?c] 'base-c)
  (define-key base (kbd "C-x f") 'base-cxf)

  ;; Mid overrides some, adds new
  (set-keymap-parent mid base)
  (define-key mid [?a] 'mid-a)
  (define-key mid [?d] 'mid-d)
  (define-key mid (kbd "C-x f") 'mid-cxf)

  ;; Top overrides some more
  (set-keymap-parent top mid)
  (define-key top [?b] 'top-b)
  (define-key top [?e] 'top-e)

  ;; Alternative base for re-parenting test
  (define-key alt-base [?a] 'alt-a)
  (define-key alt-base [?f] 'alt-f)

  ;; Phase 1: normal inheritance
  (let ((p1 (list
              (lookup-key top [?a])      ;; mid-a (mid shadows base)
              (lookup-key top [?b])      ;; top-b (top shadows base)
              (lookup-key top [?c])      ;; base-c (from base)
              (lookup-key top [?d])      ;; mid-d (from mid)
              (lookup-key top [?e])      ;; top-e (top's own)
              (lookup-key top (kbd "C-x f")))))  ;; mid-cxf (mid shadows base)

    ;; Phase 2: remove top's override of ?b
    (define-key top [?b] nil)
    (let ((p2-b (lookup-key top [?b])))  ;; should see base-b (through mid)

      ;; Phase 3: remove mid's override of ?a
      (define-key mid [?a] nil)
      (let ((p3-a (lookup-key top [?a])))  ;; should see base-a now

        ;; Phase 4: re-parent mid to alt-base instead of base
        (set-keymap-parent mid alt-base)
        (let ((p4 (list
                    (lookup-key top [?a])   ;; alt-a (from alt-base)
                    (lookup-key top [?b])   ;; nil (base-b gone, alt-base has no ?b)
                    (lookup-key top [?c])   ;; nil (base-c gone)
                    (lookup-key top [?d])   ;; mid-d (mid's own)
                    (lookup-key top [?e])   ;; top-e (top's own)
                    (lookup-key top [?f])   ;; alt-f (from alt-base)
                    (lookup-key top (kbd "C-x f")))))  ;; mid-cxf (mid's own)
          (list p1 p2-b p3-a p4))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((mid-a top-b base-c mid-d top-e mid-cxf) nil nil (nil nil nil mid-d top-e alt-f mid-cxf))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key: overwrite binding, keymap-set equivalence, numeric keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_define_key_overwrite_and_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test repeated define-key overwrites, and numeric key codes
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Bind and overwrite repeatedly
  (define-key m [?a] 'first)
  (define-key m [?a] 'second)
  (define-key m [?a] 'third)
  ;; Overwrite prefix leaf
  (define-key m (kbd "C-c a") 'prefix-first)
  (define-key m (kbd "C-c a") 'prefix-second)
  ;; Add more under same prefix
  (define-key m (kbd "C-c b") 'prefix-b)
  ;; Numeric key codes for control chars
  (define-key m [1] 'ctrl-a-num)     ;; C-a = 1
  (define-key m [2] 'ctrl-b-num)     ;; C-b = 2
  (define-key m [26] 'ctrl-z-num)    ;; C-z = 26
  ;; Overwrite numeric
  (define-key m [1] 'ctrl-a-new)
  ;; Bind to integer (rare but valid for self-insert type rebinding)
  ;; Actually define-key can bind to a keymap or a command, let's bind to a string
  (define-key m [?q] "macro-string")
  (list
    ;; Last write wins
    (lookup-key m [?a])
    ;; Prefix last write wins
    (lookup-key m (kbd "C-c a"))
    ;; Other prefix binding unaffected
    (lookup-key m (kbd "C-c b"))
    ;; Numeric keys
    (lookup-key m [1])
    (lookup-key m [2])
    (lookup-key m [26])
    ;; String binding (keyboard macro)
    (lookup-key m [?q])
    (stringp (lookup-key m [?q]))
    ;; Length of keymap (varies by implementation but should match)
    ;; Actually just check binding count consistency
    (let ((count 0))
      (map-keymap (lambda (key binding) (setq count (1+ count))) m)
      (> count 0))))"#;
    let expect = expect_test::expect![[
        r#""OK (third prefix-second prefix-b ctrl-a-new ctrl-b-num ctrl-z-num \"macro-string\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
