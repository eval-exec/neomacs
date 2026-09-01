//! Comprehensive oracle parity tests for keymap operations:
//! make-keymap, make-sparse-keymap with prompt, define-key with various
//! key sequences (string, vector, list), lookup-key with partial/full
//! matches and ACCEPT-DEFAULTS, keymap-parent / set-keymap-parent
//! inheritance, copy-keymap independence, keymapp predicate,
//! keymap-prompt for menu keymaps, and key binding inheritance chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-keymap and make-sparse-keymap: structural differences and prompt
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_make_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((full (make-keymap))
       (sparse (make-sparse-keymap))
       (prompted (make-sparse-keymap "Choose action"))
       (full-prompted (make-keymap "Full menu")))
  (list
   ;; Structural differences
   (keymapp full)
   (keymapp sparse)
   (keymapp prompted)
   ;; Full keymap has char-table as second element
   (char-table-p (car-safe (cdr full)))
   ;; Sparse does not
   (null (cdr sparse))
   ;; Prompted sparse has the prompt as an overall-header
   (keymap-prompt prompted)
   ;; Full keymap with prompt string
   (keymap-prompt full-prompted)
   ;; Keymap length differences
   (> (length full) (length sparse))
   ;; Both start with 'keymap symbol
   (eq (car full) 'keymap)
   (eq (car sparse) 'keymap)
   (eq (car prompted) 'keymap)
   ;; Prompted keymap preserves bindings after adding
   (progn
     (define-key prompted [?a] 'act-a)
     (define-key prompted [?b] 'act-b)
     (list (lookup-key prompted [?a])
           (lookup-key prompted [?b])
           (keymap-prompt prompted)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t \"Choose action\" \"Full menu\" t t t t (act-a act-b \"Choose action\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-key with string, vector, and list key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_define_key_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((m (make-sparse-keymap)))
  ;; Vector key sequence (most common)
  (define-key m [?a] 'cmd-a)
  (define-key m [?B] 'cmd-B)
  ;; Integer key in vector (control chars)
  (define-key m [1] 'cmd-ctrl-a)
  (define-key m [13] 'cmd-return)
  ;; Multi-key vector sequence
  (define-key m [?x ?y] 'cmd-xy)
  (define-key m [?x ?z] 'cmd-xz)
  ;; String key sequence (kbd-style)
  (define-key m (kbd "C-c C-k") 'cmd-cc-ck)
  ;; Overwrite an existing binding
  (define-key m [?a] 'cmd-a-new)
  ;; Bind to a lambda
  (define-key m [?l] (lambda () (interactive) (message "lambda")))
  ;; Bind to nil (unbind)
  (define-key m [?B] nil)
  (list
   (lookup-key m [?a])        ;; overwritten to cmd-a-new
   (lookup-key m [?B])        ;; unbound (nil)
   (lookup-key m [1])         ;; cmd-ctrl-a
   (lookup-key m [13])        ;; cmd-return
   (lookup-key m [?x ?y])     ;; cmd-xy
   (lookup-key m [?x ?z])     ;; cmd-xz
   (keymapp (lookup-key m [?x])) ;; prefix sub-keymap
   (lookup-key m (kbd "C-c C-k")) ;; cmd-cc-ck
   ;; Lambda binding is a function
   (functionp (lookup-key m [?l]))))
"#;
    let expect = expect_test::expect![[
        r#""OK (cmd-a-new nil cmd-ctrl-a cmd-return cmd-xy cmd-xz t cmd-cc-ck t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lookup-key: partial matches, full matches, ACCEPT-DEFAULTS
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_lookup_key_detailed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((m (make-sparse-keymap)))
  (define-key m [?a] 'cmd-a)
  (define-key m [?b ?c] 'cmd-bc)
  (define-key m [?b ?d] 'cmd-bd)
  (define-key m [?b ?e ?f] 'cmd-bef)
  (list
   ;; Full match
   (lookup-key m [?a])
   ;; Partial match: prefix key returns sub-keymap
   (keymapp (lookup-key m [?b]))
   ;; Full match on multi-key
   (lookup-key m [?b ?c])
   (lookup-key m [?b ?d])
   (lookup-key m [?b ?e ?f])
   ;; Too-long: ?a is bound directly, so [?a ?x] returns 1
   (lookup-key m [?a ?x])
   ;; Completely unbound key
   (lookup-key m [?z])
   ;; Too-long on multi-level: [?b ?c ?x] returns 2
   (lookup-key m [?b ?c ?x])
   ;; lookup-key with ACCEPT-DEFAULTS parameter (t)
   ;; For a sparse keymap, accept-defaults looks in the char-table default
   (let ((fk (make-keymap)))
     (define-key fk [?a] 'full-a)
     ;; lookup without accept-defaults on unbound key in full keymap
     (list (lookup-key fk [?z])
           (lookup-key fk [?z] t)))
   ;; Prefix key deeper nesting
   (keymapp (lookup-key m [?b ?e]))))
"#;
    let expect =
        expect_test::expect![[r#""OK (cmd-a t cmd-bc cmd-bd cmd-bef 1 nil 2 (nil nil) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymap-parent and set-keymap-parent: inheritance chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((gp (make-sparse-keymap))
      (p  (make-sparse-keymap))
      (c  (make-sparse-keymap))
      (gc (make-sparse-keymap)))
  ;; 4-level chain: gc -> c -> p -> gp
  (define-key gp [?a] 'gp-a)
  (define-key gp [?b] 'gp-b)
  (define-key gp [?c] 'gp-c)
  (define-key gp [?d] 'gp-d)
  (define-key p [?b] 'p-b)
  (define-key p [?c] 'p-c)
  (define-key c [?c] 'c-c)
  (define-key gc [?d] 'gc-d)
  (set-keymap-parent p gp)
  (set-keymap-parent c p)
  (set-keymap-parent gc c)
  (list
   ;; gc inherits through entire chain
   (lookup-key gc [?a])  ;; from gp (4 levels up)
   (lookup-key gc [?b])  ;; from p (3 levels up)
   (lookup-key gc [?c])  ;; from c (2 levels up)
   (lookup-key gc [?d])  ;; from gc itself
   ;; c gets its own and parent chain
   (lookup-key c [?a])   ;; from gp
   (lookup-key c [?b])   ;; from p
   (lookup-key c [?c])   ;; from c itself
   (lookup-key c [?d])   ;; from gp
   ;; Verify parent chain
   (eq (keymap-parent gc) c)
   (eq (keymap-parent c) p)
   (eq (keymap-parent p) gp)
   (null (keymap-parent gp))
   ;; Reparenting: disconnect c from p
   (set-keymap-parent c nil)
   (null (keymap-parent c))
   ;; Now c has no parent chain, so ?a and ?b unbound in c
   (lookup-key c [?a])
   (lookup-key c [?b])
   ;; But gc -> c still works for c's own binding
   (lookup-key gc [?c])))
"#;
    let expect = expect_test::expect![[
        r#""OK (gp-a p-b c-c gc-d gp-a p-b c-c gp-d t t t t nil t nil nil c-c)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-keymap: deep independence test with prefix keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_copy_independence_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((orig (make-sparse-keymap)))
  ;; Build a prefix key structure
  (define-key orig [?a] 'cmd-a)
  (define-key orig [?b ?c] 'cmd-bc)
  (define-key orig [?b ?d] 'cmd-bd)
  (define-key orig [?e] 'cmd-e)
  ;; Set a parent
  (let ((parent (make-sparse-keymap)))
    (define-key parent [?p] 'parent-p)
    (set-keymap-parent orig parent)
    ;; Copy
    (let ((copy (copy-keymap orig)))
      ;; Modify original
      (define-key orig [?a] 'new-cmd-a)
      (define-key orig [?f] 'cmd-f)
      ;; Modify copy
      (define-key copy [?e] 'copy-cmd-e)
      (define-key copy [?g] 'cmd-g)
      (list
       ;; Original state
       (lookup-key orig [?a])    ;; new-cmd-a
       (lookup-key orig [?b ?c]) ;; cmd-bc
       (lookup-key orig [?e])    ;; cmd-e
       (lookup-key orig [?f])    ;; cmd-f
       (lookup-key orig [?g])    ;; nil
       ;; Copy state
       (lookup-key copy [?a])    ;; cmd-a (original before modification)
       (lookup-key copy [?b ?c]) ;; cmd-bc
       (lookup-key copy [?e])    ;; copy-cmd-e
       (lookup-key copy [?f])    ;; nil
       (lookup-key copy [?g])    ;; cmd-g
       ;; Parent inheritance via copy
       (lookup-key copy [?p])    ;; parent-p (parent shared)
       ;; Both are still keymaps
       (keymapp orig)
       (keymapp copy)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (new-cmd-a cmd-bc cmd-e cmd-f nil cmd-a cmd-bc copy-cmd-e nil cmd-g parent-p t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymapp: comprehensive predicate testing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_keymapp_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Proper keymaps
 (keymapp (make-keymap))
 (keymapp (make-sparse-keymap))
 (keymapp (make-sparse-keymap "prompt"))
 ;; Cons cell starting with 'keymap symbol is a keymap
 (keymapp '(keymap))
 (keymapp '(keymap (97 . self-insert-command)))
 (keymapp (list 'keymap))
 ;; Not keymaps
 (keymapp nil)
 (keymapp t)
 (keymapp 0)
 (keymapp -1)
 (keymapp 3.14)
 (keymapp "keymap")
 (keymapp 'keymap)
 (keymapp '(notakeymap))
 (keymapp (make-hash-table))
 (keymapp [keymap])
 ;; Copied keymap is a keymap
 (keymapp (copy-keymap (make-sparse-keymap)))
 ;; Sub-keymap from prefix binding is a keymap
 (let ((m (make-sparse-keymap)))
   (define-key m [?x ?y] 'cmd)
   (keymapp (lookup-key m [?x])))
 ;; Keymap with parent
 (let ((child (make-sparse-keymap)))
   (set-keymap-parent child (make-sparse-keymap))
   (keymapp child)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil nil nil nil nil nil nil nil nil nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// keymap-prompt: menu keymaps and prompt extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_keymap_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Keymap with prompt
 (keymap-prompt (make-sparse-keymap "Menu Title"))
 ;; Keymap without prompt
 (keymap-prompt (make-sparse-keymap))
 ;; Full keymap without prompt
 (keymap-prompt (make-keymap))
 ;; Full keymap with prompt
 (keymap-prompt (make-keymap "Full Menu"))
 ;; Prompt survives copy-keymap
 (let ((m (make-sparse-keymap "Copied Menu")))
   (define-key m [?a] 'act-a)
   (keymap-prompt (copy-keymap m)))
 ;; Prompt survives adding bindings
 (let ((m (make-sparse-keymap "Persistent Prompt")))
   (define-key m [?x] 'cmd-x)
   (define-key m [?y] 'cmd-y)
   (define-key m [?z ?w] 'cmd-zw)
   (keymap-prompt m))
 ;; Prompt on sub-keymap created by prefix binding
 (let ((m (make-sparse-keymap)))
   (define-key m [?x ?y] 'cmd)
   (keymap-prompt (lookup-key m [?x])))
 ;; Parent's prompt is not inherited by child
 (let ((parent (make-sparse-keymap "Parent Prompt"))
       (child (make-sparse-keymap)))
   (set-keymap-parent child parent)
   (list (keymap-prompt child)
         (keymap-prompt parent))))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"Menu Title\" nil nil \"Full Menu\" \"Copied Menu\" \"Persistent Prompt\" nil (\"Parent Prompt\" \"Parent Prompt\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building an Emacs-like key dispatch system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_dispatch_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((global-map (make-sparse-keymap))
      (mode-map (make-sparse-keymap))
      (local-map (make-sparse-keymap)))
  ;; Global bindings
  (define-key global-map [?q] 'quit)
  (define-key global-map [?h] 'help)
  (define-key global-map [24 ?f] 'find-file)   ;; C-x f
  (define-key global-map [24 ?s] 'save-file)   ;; C-x s
  (define-key global-map [24 ?b] 'switch-buf)  ;; C-x b
  ;; Mode map overrides some globals
  (set-keymap-parent mode-map global-map)
  (define-key mode-map [?h] 'mode-help)
  (define-key mode-map [24 ?c] 'compile)   ;; C-x c (mode-specific)
  ;; Local map overrides mode
  (set-keymap-parent local-map mode-map)
  (define-key local-map [?q] 'local-quit)
  (define-key local-map [24 ?s] 'local-save)
  ;; Simulate key dispatch: look up various keys through the chain
  (let ((dispatch
         (lambda (keyseq)
           (or (lookup-key local-map keyseq)
               'unbound))))
    (list
     ;; local override
     (funcall dispatch [?q])
     ;; mode override
     (funcall dispatch [?h])
     ;; global binding through chain
     (funcall dispatch [24 ?f])
     ;; local override of global prefix sub-key
     (funcall dispatch [24 ?s])
     ;; mode-specific addition
     (funcall dispatch [24 ?c])
     ;; global binding through both
     (funcall dispatch [24 ?b])
     ;; completely unbound
     (lookup-key local-map [?z])
     ;; Verify the chain
     (eq (keymap-parent local-map) mode-map)
     (eq (keymap-parent mode-map) global-map)
     ;; Count approximate bindings at each level
     (let ((count 0))
       (map-keymap (lambda (k v) (setq count (1+ count))) local-map)
       count)
     ;; Reparent mode-map to nil, local still has its own bindings
     (progn
       (set-keymap-parent mode-map nil)
       (list (lookup-key local-map [?q])      ;; local-quit still there
             (lookup-key local-map [?h])       ;; mode-help still in mode-map
             (lookup-key local-map [24 ?f]))))))  ;; nil, global detached
"#;
    let expect = expect_test::expect![[
        r#""OK (local-quit mode-help find-file local-save compile switch-buf nil t t 7 (local-quit mode-help nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// map-keymap: iterate over keymap bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keymap_comprehensive_map_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((m (make-sparse-keymap)))
  (define-key m [?a] 'cmd-a)
  (define-key m [?b] 'cmd-b)
  (define-key m [?c] 'cmd-c)
  (define-key m [?d] 'cmd-d)
  ;; Collect all bindings via map-keymap
  (let ((bindings nil))
    (map-keymap (lambda (key binding)
                  (push (cons key binding) bindings))
                m)
    ;; Sort by key for deterministic output
    (let ((sorted (sort bindings (lambda (a b) (< (car a) (car b))))))
      (list
       ;; Number of bindings
       (length sorted)
       ;; All bindings present
       (mapcar #'cdr sorted)
       ;; All keys present
       (mapcar #'car sorted)
       ;; map-keymap on empty keymap
       (let ((empty (make-sparse-keymap))
             (count 0))
         (map-keymap (lambda (k v) (setq count (1+ count))) empty)
         count)))))
"#;
    let expect = expect_test::expect![[r#""OK (4 (cmd-a cmd-b cmd-c cmd-d) (97 98 99 100) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
