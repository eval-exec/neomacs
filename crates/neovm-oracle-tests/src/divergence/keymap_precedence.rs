//! Keymap precedence divergence probes (calibration).
//!
//! Probes lookup-key, keymap-parent inheritance, and key-binding precedence
//! across local / global / minor-mode / text-property / overlay keymaps, plus
//! current-active-maps ordering, where-is-internal, and command-remapping.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_km_lookup_key_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (act-a nil act-cc)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "a" 'act-a)
  (define-key m (kbd "C-c C-d") 'act-cc)
  (list (lookup-key m "a")
        (lookup-key m "b")
        (lookup-key m "\C-c\C-d")))
"##,
        expect,
    );
}

#[test]
fn div_km_keymap_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (parent-act (keymap (97 . parent-act)) parent-act)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-sparse-keymap)) (c (make-sparse-keymap)))
  (define-key p "a" 'parent-act)
  (set-keymap-parent c p)
  (list (lookup-key c "a")
        (keymap-parent c)
        (lookup-key p "a")))
"##,
        expect,
    );
}

#[test]
fn div_km_keymap_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (grand mid)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((g (make-sparse-keymap)) (m (make-sparse-keymap)) (l (make-sparse-keymap)))
  (define-key g "x" 'grand)
  (define-key m "y" 'mid)
  (set-keymap-parent l m)
  (set-keymap-parent m g)
  (list (lookup-key l "x") (lookup-key l "y")))
"##,
        expect,
    );
}

#[test]
fn div_km_local_overrides_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK local-act""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((gm (make-sparse-keymap)) (lm (make-sparse-keymap)))
    (define-key gm "a" 'global-act)
    (define-key lm "a" 'local-act)
    (use-global-map gm)
    (use-local-map lm)
    (key-binding "a")))
"##,
        expect,
    );
}

#[test]
fn div_km_minor_mode_overrides_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK local-act""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((gm (make-sparse-keymap)) (lm (make-sparse-keymap))
        (mm (make-sparse-keymap)))
    (define-key gm "a" 'global-act)
    (define-key lm "a" 'local-act)
    (define-key mm "a" 'minor-act)
    (use-global-map gm)
    (use-local-map lm)
    (let ((minor-mode-map-alist (list (cons 'fake-mode mm)))
          (fake-mode t))
      (key-binding "a"))))
"##,
        expect,
    );
}

#[test]
fn div_km_text_property_local_map_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-act""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((gm (make-sparse-keymap)) (tm (make-sparse-keymap)))
  (define-key gm "a" 'global-act)
  (define-key tm "a" 'text-act)
  (use-global-map gm)
  (with-temp-buffer
    (insert "hello")
    (put-text-property 1 3 'local-map tm)
    (goto-char 2)
    (key-binding "a")))
"##,
        expect,
    );
}

#[test]
fn div_km_overlay_keymap_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK overlay-act""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((gm (make-sparse-keymap)) (om (make-sparse-keymap)))
  (define-key gm "a" 'global-act)
  (define-key om "a" 'overlay-act)
  (use-global-map gm)
  (with-temp-buffer
    (insert "hello")
    (let ((ov (make-overlay 1 3)))
      (overlay-put ov 'keymap om))
    (goto-char 2)
    (key-binding "a")))
"##,
        expect,
    );
}

#[test]
fn div_km_current_active_maps_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (length (current-active-maps t)))
"##,
        expect,
    );
}

#[test]
fn div_km_current_active_maps_include_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (if (memq global-map (current-active-maps t)) t nil))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([97] [99])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "a" 'target)
  (define-key m "b" 'other)
  (define-key m "c" 'target)
  (sort (where-is-internal 'target m) (lambda (a b) (string< (key-description a) (key-description b)))))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal_reduces_menu_item_leaves() {
    // A command bound as an old-style `(MENU-STRING . COMMAND)` leaf (what
    // general.el `:desc` produces for Doom's leader) or a new-style
    // `(menu-item NAME COMMAND)` must be discoverable by `where-is-internal`'s
    // reverse scan, mirroring GNU `where_is_internal_1`'s `get_keyelt` call
    // (issue #164 dashboard leader hints).
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([114] [103])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "r" '("Recent files" . my-recentf))
  (define-key m "g" '(menu-item "Grep" my-grep))
  (list (where-is-internal 'my-recentf (list m) t)
        (where-is-internal 'my-grep (list m) t)))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal_reports_inline_vector_bindings() {
    // A keymap element may be an inline vector indexing bindings by char code
    // (GNU `map_keymap_internal`). Forward `lookup-key` and the reverse
    // `where-is-internal` scan must agree about it.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (vec-cmd [97] [112 97])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((v (make-vector 128 nil)) (sub (list 'keymap v)) (m (make-sparse-keymap)))
  (aset v ?a 'vec-cmd)
  (define-key m "p" sub)
  (list (lookup-key m "pa")
        (where-is-internal 'vec-cmd (list sub) t)
        (where-is-internal 'vec-cmd (list m) t)))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal_descends_composed_keymaps() {
    // `where-is-internal` must descend into a composed keymap's inline submaps
    // (`make-composed-keymap`), not just its parent, mirroring GNU `map_keymap`.
    // evil/general build active state keymaps this way, so a leader binding
    // reachable only through a composed submap (Doom's `SPC f r`) must resolve
    // (issue #164).
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [32 102 114]""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lead (make-sparse-keymap)) (sub (make-sparse-keymap))
      (a (make-sparse-keymap)) (b (make-sparse-keymap)))
  (define-key sub "r" '("Recent" . my-recentf))
  (define-key lead "f" sub)
  (fset 'my-lead lead)
  (define-key a " " 'my-lead)
  (where-is-internal 'my-recentf (list (make-composed-keymap (list a b))) t))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal_descends_string_labelled_symbol_prefix() {
    // Real evil/Doom leader shape: SPC bound as `("<leader>" . doom/leader)` --
    // a `(STRING . SYMBOL)` label wrapping a symbol prefix command -- inside a
    // composed active-state keymap. Descent must reduce the label then resolve
    // the symbol to its keymap (GNU `accessible_keymaps_1`: get_keyelt +
    // get_keymap). This is the actual #164 dashboard blocker.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [32 102 114]""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((leader (make-sparse-keymap)) (sub (make-sparse-keymap)) (aux (make-sparse-keymap)))
  (define-key sub "r" '("Recent" . my-recentf))
  (define-key leader "f" sub)
  (fset 'my-lead leader)
  (define-key aux " " '("<leader>" . my-lead))
  (where-is-internal 'my-recentf (list (make-composed-keymap aux)) t))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_internal_noindirect_keeps_menu_item_opaque() {
    // GNU's 4th arg NOINDIRECT: when non-nil, the menu-item wrapper is NOT
    // reduced, so searching for the underlying command finds nothing.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([114] nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "r" '("Recent files" . my-recentf))
  (list (where-is-internal 'my-recentf (list m) t)
        (where-is-internal 'my-recentf (list m) t t)))
"##,
        expect,
    );
}

#[test]
fn div_km_command_remapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (my-forward nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m [remap forward-char] 'my-forward)
  (list (lookup-key m [remap forward-char])
        (command-remapping 'forward-char m)))
"##,
        expect,
    );
}

#[test]
fn div_km_accessible_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)) (sub (make-sparse-keymap)))
  (define-key m "a" 'act)
  (define-key m "p" sub)
  (length (accessible-keymaps m)))
"##,
        expect,
    );
}

#[test]
fn div_km_map_keymap_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((97 . 1) (98 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)) (acc nil))
  (define-key m "a" 1)
  (define-key m "b" 2)
  (map-keymap (lambda (k v) (push (cons k v) acc)) m)
  (sort acc (lambda (a b) (< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_km_keymapp_and_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t act)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (define-prefix-command 'neo-prefix-map)))
  (define-key m "a" 'act)
  (list (keymapp m)
        (fboundp 'neo-prefix-map)
        (lookup-key m "a")))
"##,
        expect,
    );
}

#[test]
fn div_km_lookup_key_returns_keymap_for_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)) (sub (make-sparse-keymap)))
  (define-key sub "a" 'deep)
  (define-key m "p" sub)
  (keymapp (lookup-key m "p")))
"##,
        expect,
    );
}

// ---------------------------------------------------------------------
// `:advertised-binding' (DIVERGENCES.md 185 §1)
//
// GNU `Fwhere_is_internal' (src/keymap.c:2669-2684) consults the symbol's
// `:advertised-binding' property before the reverse search whenever FIRSTONLY
// is non-nil.  `lisp/bindings.el:1331-1334' is the reason it matters: it binds
// `set-mark-command' to both `C-@' (0) and `C-SPC' (67108896) and advertises
// the latter, so every docstring that says \\[set-mark-command] reads `C-SPC'
// in GNU.  These probes run against the bootstrapped `global-map', which the
// unit tests in neovm-core cannot reach.
// ---------------------------------------------------------------------

#[test]
fn div_km_where_is_advertised_binding_wins_over_the_ascii_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([67108896] [67108896] t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (where-is-internal 'set-mark-command global-map t)
      (get 'set-mark-command :advertised-binding)
      ;; both bindings really are present -- the property picks between them,
      ;; it does not invent one.
      (and (memq 0 (mapcar (lambda (k) (aref k 0))
                           (where-is-internal 'set-mark-command global-map)))
           (memq 67108896 (mapcar (lambda (k) (aref k 0))
                                  (where-is-internal 'set-mark-command global-map)))
           t))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_advertised_binding_names_the_key_in_docstrings() {
    // The user-visible consequence, and the symptom entry 178 handed over:
    // `set-mark-command-repeat-pop' renders as `C-SPC' in GNU and rendered as
    // `C-@' here.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C-SPC\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (substring-no-properties (substitute-command-keys "\\[set-mark-command]"))
      (and (string-match-p
            "C-SPC"
            (substitute-command-keys
             (documentation-property 'set-mark-command-repeat-pop 'variable-documentation)))
           t))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_advertised_binding_is_verified_before_it_is_trusted() {
    // A property whose key no longer runs the command is ignored; one that
    // does is returned VERBATIM, so a string property stays a string.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([120] \"m\" t [113])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)) (n (make-sparse-keymap)) (o (make-sparse-keymap)))
  (define-key m "x" 'neo-adv-stale)
  (put 'neo-adv-stale :advertised-binding [?y])
  (define-key n "m" 'neo-adv-string)
  (put 'neo-adv-string :advertised-binding "m")
  (define-key o "p" 'neo-adv-list)
  (define-key o "q" 'neo-adv-list)
  (put 'neo-adv-list :advertised-binding (list [?z] [?q] [?p]))
  (list (where-is-internal 'neo-adv-stale m t)
        (where-is-internal 'neo-adv-string n t)
        (stringp (where-is-internal 'neo-adv-string n t))
        (where-is-internal 'neo-adv-list o t)))
"##,
        expect,
    );
}

#[test]
fn div_km_where_is_advertised_binding_list_that_matches_nothing_signals() {
    // GNU offers the exhausted list's nil TAIL to `shadow_lookup' as one more
    // candidate, and `Flookup_key' rejects nil as a key sequence.  Pinned
    // because it is GNU's real answer and not an obvious one.
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-type-argument arrayp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "d" 'neo-adv-none)
  (put 'neo-adv-none :advertised-binding (list [?z] [?y]))
  (condition-case e (where-is-internal 'neo-adv-none m t) (error e)))
"##,
        expect,
    );
}
