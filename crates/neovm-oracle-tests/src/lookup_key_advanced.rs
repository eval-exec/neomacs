//! Advanced oracle parity tests for `lookup-key` with ALL parameter combinations:
//! string key sequences, vector key sequences, nil for unbound keys, integer
//! return for too-long sequences, parent keymap inheritance, sparse vs full
//! keymaps, multi-level prefix lookup, and remapped commands.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// lookup-key with string key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_string_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test lookup-key with various string-based (kbd) key sequences
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Set up a variety of bindings
  (define-key m (kbd "C-a") 'beginning-of-line)
  (define-key m (kbd "C-e") 'end-of-line)
  (define-key m (kbd "C-x C-f") 'find-file)
  (define-key m (kbd "C-x C-s") 'save-buffer)
  (define-key m (kbd "C-x C-b") 'list-buffers)
  (define-key m (kbd "C-c C-c") 'compile)
  (define-key m (kbd "M-x") 'execute-extended-command)
  (define-key m (kbd "M-g g") 'goto-line)
  (define-key m (kbd "C-h f") 'describe-function)
  (define-key m (kbd "C-h v") 'describe-variable)
  (define-key m (kbd "C-h k") 'describe-key)
  (list
    ;; Single key lookups
    (lookup-key m (kbd "C-a"))
    (lookup-key m (kbd "C-e"))
    (lookup-key m (kbd "M-x"))
    ;; Two-key sequences
    (lookup-key m (kbd "C-x C-f"))
    (lookup-key m (kbd "C-x C-s"))
    (lookup-key m (kbd "C-x C-b"))
    (lookup-key m (kbd "C-c C-c"))
    (lookup-key m (kbd "M-g g"))
    ;; Help prefix lookups
    (lookup-key m (kbd "C-h f"))
    (lookup-key m (kbd "C-h v"))
    (lookup-key m (kbd "C-h k"))
    ;; Prefix itself returns a keymap
    (keymapp (lookup-key m (kbd "C-x")))
    (keymapp (lookup-key m (kbd "C-c")))
    (keymapp (lookup-key m (kbd "C-h")))
    (keymapp (lookup-key m (kbd "M-g")))
    ;; Unbound under existing prefix
    (lookup-key m (kbd "C-x C-z"))
    (lookup-key m (kbd "C-h a"))))"#;
    let expect = expect_test::expect![[
        r#""OK (beginning-of-line end-of-line execute-extended-command find-file save-buffer list-buffers compile goto-line describe-function describe-variable describe-key t t t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key with vector key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_vector_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test lookup-key with vector-based key specifications
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Set up bindings with vectors
  (define-key m [?a] 'cmd-a)
  (define-key m [?b] 'cmd-b)
  (define-key m [f1] 'cmd-f1)
  (define-key m [f2] 'cmd-f2)
  (define-key m [return] 'cmd-ret)
  (define-key m [backspace] 'cmd-bs)
  ;; Multi-element vectors
  (define-key m [?x ?a] 'cmd-xa)
  (define-key m [?x ?b] 'cmd-xb)
  (define-key m [?x ?c ?d] 'cmd-xcd)
  ;; Mixed: function key + char
  (define-key m [f3 ?a] 'cmd-f3a)
  (define-key m [f3 ?b] 'cmd-f3b)
  (list
    ;; Single element
    (lookup-key m [?a])
    (lookup-key m [?b])
    (lookup-key m [f1])
    (lookup-key m [f2])
    (lookup-key m [return])
    (lookup-key m [backspace])
    ;; Multi-element
    (lookup-key m [?x ?a])
    (lookup-key m [?x ?b])
    (lookup-key m [?x ?c ?d])
    ;; Prefix
    (keymapp (lookup-key m [?x]))
    (keymapp (lookup-key m [?x ?c]))
    (keymapp (lookup-key m [f3]))
    ;; Function key prefix
    (lookup-key m [f3 ?a])
    (lookup-key m [f3 ?b])
    ;; Unbound in various forms
    (lookup-key m [?z])
    (lookup-key m [?x ?z])
    (lookup-key m [f4])))"#;
    let expect = expect_test::expect![[
        r#""OK (cmd-a cmd-b cmd-f1 cmd-f2 cmd-ret cmd-bs cmd-xa cmd-xb cmd-xcd t t t cmd-f3a cmd-f3b nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key returning nil for unbound keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_nil_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustive test of nil returns for various unbound scenarios
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Only a few bindings
  (define-key m [?a] 'cmd-a)
  (define-key m (kbd "C-x C-f") 'find-file)
  (list
    ;; Completely unbound single keys
    (lookup-key m [?b])
    (lookup-key m [?z])
    (lookup-key m [?A])
    (lookup-key m [f1])
    (lookup-key m [return])
    (lookup-key m [32])
    ;; Unbound under existing prefix
    (lookup-key m (kbd "C-x C-z"))
    (lookup-key m (kbd "C-x C-a"))
    (lookup-key m (kbd "C-x C-b"))
    ;; Unbound prefix entirely
    (lookup-key m (kbd "C-c"))
    (lookup-key m (kbd "M-x"))
    ;; All should be nil
    (null (lookup-key m [?b]))
    (null (lookup-key m [?z]))
    (null (lookup-key m [f1]))
    (null (lookup-key m (kbd "C-c")))
    ;; Empty keymap
    (let ((empty (make-sparse-keymap)))
      (list
        (lookup-key empty [?a])
        (lookup-key empty [f1])
        (lookup-key empty (kbd "C-x C-f"))
        (null (lookup-key empty [?a]))))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil nil nil nil nil nil nil nil t t t t (nil nil 1 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key returning integer for too-long key sequences (prefix match)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_integer_too_long() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When a non-prefix binding is found before all keys are consumed,
    // lookup-key returns the number of keys consumed
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Simple single-key binding
  (define-key m [?a] 'cmd-a)
  (define-key m [?b] 'cmd-b)
  ;; Two-key binding
  (define-key m (kbd "C-x C-f") 'find-file)
  ;; Three-key binding
  (define-key m (kbd "C-c p r") 'run)
  (list
    ;; ?a is bound to cmd-a (not a prefix), so [?a ?b] is too long
    ;; Returns 1 (consumed 1 key)
    (lookup-key m [?a ?b])
    (numberp (lookup-key m [?a ?b]))
    ;; [?a ?b ?c] also too long, still returns 1
    (lookup-key m [?a ?b ?c])
    ;; C-x C-f is bound, so [C-x C-f C-s] is too long
    ;; Returns 2 (consumed 2 keys)
    (lookup-key m [24 6 19])
    (numberp (lookup-key m [24 6 19]))
    ;; Three extra keys after a single binding
    (lookup-key m [?b ?x ?y ?z])
    ;; C-c p r is bound, so [C-c p r x] is too long
    ;; Returns 3 (consumed 3 keys)
    (lookup-key m [3 ?p ?r ?x])
    ;; Verify the integer value matches consumed prefix length
    (let ((r1 (lookup-key m [?a ?b]))
          (r2 (lookup-key m [24 6 19]))
          (r3 (lookup-key m [3 ?p ?r ?x])))
      (list
        (integerp r1) r1
        (integerp r2) r2
        (integerp r3) r3))))"#;
    let expect = expect_test::expect![[r#""OK (1 t 1 2 t 1 3 (t 1 t 2 t 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key with parent keymaps (inheritance)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_parent_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that lookup-key traverses parent keymap chains
    let form = r#"(let ((grandparent (make-sparse-keymap))
                        (parent (make-sparse-keymap))
                        (child (make-sparse-keymap)))
  ;; Grandparent bindings
  (define-key grandparent [?a] 'gp-a)
  (define-key grandparent [?b] 'gp-b)
  (define-key grandparent [?c] 'gp-c)
  (define-key grandparent (kbd "C-x f") 'gp-cxf)
  (define-key grandparent (kbd "C-x g") 'gp-cxg)

  ;; Parent overrides some, adds new
  (set-keymap-parent parent grandparent)
  (define-key parent [?b] 'p-b)
  (define-key parent [?d] 'p-d)
  (define-key parent (kbd "C-x f") 'p-cxf)
  (define-key parent (kbd "C-c a") 'p-cca)

  ;; Child overrides some more
  (set-keymap-parent child parent)
  (define-key child [?c] 'c-c)
  (define-key child [?e] 'c-e)
  (define-key child (kbd "C-c b") 'c-ccb)

  (list
    ;; Child sees everything with proper shadowing
    (lookup-key child [?a])        ;; gp-a (from grandparent)
    (lookup-key child [?b])        ;; p-b (parent shadows grandparent)
    (lookup-key child [?c])        ;; c-c (child shadows grandparent)
    (lookup-key child [?d])        ;; p-d (from parent)
    (lookup-key child [?e])        ;; c-e (child's own)
    (lookup-key child [?f])        ;; nil (unbound everywhere)
    ;; Prefix key lookups through chain
    (lookup-key child (kbd "C-x f"))   ;; p-cxf (parent shadows grandparent)
    (lookup-key child (kbd "C-x g"))   ;; gp-cxg (from grandparent)
    (lookup-key child (kbd "C-c a"))   ;; p-cca (from parent)
    (lookup-key child (kbd "C-c b"))   ;; c-ccb (child's own)
    ;; Parent doesn't see child
    (lookup-key parent [?c])       ;; gp-c (from grandparent, not child's c-c)
    (lookup-key parent [?e])       ;; nil (child's own)
    (lookup-key parent (kbd "C-c b"))  ;; nil (child's own)
    ;; Grandparent sees only its own
    (lookup-key grandparent [?b])  ;; gp-b (not shadowed at this level)
    (lookup-key grandparent [?d])  ;; nil
    ;; Prefixes are keymaps in child
    (keymapp (lookup-key child (kbd "C-x")))
    (keymapp (lookup-key child (kbd "C-c")))))"#;
    let expect = expect_test::expect![[
        r#""OK (gp-a p-b c-c p-d c-e nil p-cxf gp-cxg p-cca c-ccb gp-c nil nil gp-b nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key with sparse and full keymaps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_sparse_vs_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare behavior between sparse keymaps and full keymaps (with char-table)
    let form = r#"(let ((sparse (make-sparse-keymap))
                        (full (make-keymap)))
  ;; Bind the same keys in both
  (define-key sparse [?a] 'cmd-a)
  (define-key sparse [?z] 'cmd-z)
  (define-key sparse [f1] 'cmd-f1)
  (define-key sparse (kbd "C-x f") 'cmd-cxf)

  (define-key full [?a] 'cmd-a)
  (define-key full [?z] 'cmd-z)
  (define-key full [f1] 'cmd-f1)
  (define-key full (kbd "C-x f") 'cmd-cxf)

  (list
    ;; Both should return same results for bound keys
    (eq (lookup-key sparse [?a]) (lookup-key full [?a]))
    (eq (lookup-key sparse [?z]) (lookup-key full [?z]))
    (eq (lookup-key sparse [f1]) (lookup-key full [f1]))
    (eq (lookup-key sparse (kbd "C-x f")) (lookup-key full (kbd "C-x f")))

    ;; Unbound keys: both return nil for characters
    (lookup-key sparse [?m])
    (lookup-key full [?m])

    ;; Structural difference: full has char-table
    (char-table-p (cadr full))
    (not (char-table-p (cadr sparse)))

    ;; Parent keymap works with both types
    (let ((parent-s (make-sparse-keymap))
          (parent-f (make-keymap)))
      (define-key parent-s [?p] 'parent-p)
      (define-key parent-f [?p] 'parent-p)
      (set-keymap-parent sparse parent-s)
      (set-keymap-parent full parent-f)
      (list
        (lookup-key sparse [?p])
        (lookup-key full [?p])
        (eq (lookup-key sparse [?p]) (lookup-key full [?p]))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil t t (parent-p parent-p t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-level prefix key lookup with mixed depths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lookup_key_multi_level_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test intricate multi-level prefix key structures with overlapping prefixes
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Build a complex prefix tree:
  ;; C-c → prefix
  ;;   C-c a → cmd1
  ;;   C-c b → prefix
  ;;     C-c b a → cmd2
  ;;     C-c b b → cmd3
  ;;   C-c c → prefix
  ;;     C-c c a → prefix
  ;;       C-c c a a → cmd4
  ;;       C-c c a b → cmd5
  ;;     C-c c b → cmd6
  ;; C-x → prefix
  ;;   C-x 4 → prefix
  ;;     C-x 4 f → cmd7
  ;;   C-x 5 → prefix
  ;;     C-x 5 f → cmd8
  ;;   C-x r → prefix
  ;;     C-x r s → cmd9
  ;;     C-x r i → cmd10
  (define-key m (kbd "C-c a") 'cmd1)
  (define-key m (kbd "C-c b a") 'cmd2)
  (define-key m (kbd "C-c b b") 'cmd3)
  (define-key m [3 ?c ?a ?a] 'cmd4)
  (define-key m [3 ?c ?a ?b] 'cmd5)
  (define-key m (kbd "C-c c b") 'cmd6)
  (define-key m (kbd "C-x 4 f") 'cmd7)
  (define-key m (kbd "C-x 5 f") 'cmd8)
  (define-key m (kbd "C-x r s") 'cmd9)
  (define-key m (kbd "C-x r i") 'cmd10)

  (list
    ;; Leaf lookups
    (lookup-key m (kbd "C-c a"))
    (lookup-key m (kbd "C-c b a"))
    (lookup-key m (kbd "C-c b b"))
    (lookup-key m [3 ?c ?a ?a])
    (lookup-key m [3 ?c ?a ?b])
    (lookup-key m (kbd "C-c c b"))
    (lookup-key m (kbd "C-x 4 f"))
    (lookup-key m (kbd "C-x 5 f"))
    (lookup-key m (kbd "C-x r s"))
    (lookup-key m (kbd "C-x r i"))

    ;; Prefix lookups all return keymaps
    (keymapp (lookup-key m (kbd "C-c")))
    (keymapp (lookup-key m (kbd "C-c b")))
    (keymapp (lookup-key m (kbd "C-c c")))
    (keymapp (lookup-key m (kbd "C-c c a")))
    (keymapp (lookup-key m (kbd "C-x")))
    (keymapp (lookup-key m (kbd "C-x 4")))
    (keymapp (lookup-key m (kbd "C-x 5")))
    (keymapp (lookup-key m (kbd "C-x r")))

    ;; Unbound under existing prefix
    (lookup-key m (kbd "C-c d"))
    (lookup-key m (kbd "C-c b c"))
    (lookup-key m (kbd "C-x r x"))

    ;; Too-long after leaf
    (numberp (lookup-key m [3 ?a ?x]))      ;; C-c a is leaf, returns 2
    (lookup-key m [3 ?a ?x])

    ;; Unbound prefix entirely
    (lookup-key m (kbd "C-z"))))"#;
    let expect = expect_test::expect![[
        r#""OK (cmd1 cmd2 cmd3 cmd4 cmd5 cmd6 cmd7 cmd8 cmd9 cmd10 t t t t t t t t nil nil nil t 2 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
