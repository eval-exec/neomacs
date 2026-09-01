//! Oracle parity tests for advanced kbd and event operations:
//! kbd with modifier keys (C-, M-, S-, s-, H-), multi-key sequences,
//! key-description roundtrip, event-convert-list, single-key-description,
//! function keys, mouse events, complex key sequences.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// kbd with all modifier prefixes and combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_all_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test kbd with every modifier prefix: C- (control), M- (meta),
    // S- (shift), s- (super), H- (hyper), and combinations thereof
    let form = r#"(list
  ;; Single modifiers
  (kbd "C-a")
  (kbd "M-a")
  (kbd "S-a")
  (kbd "s-a")
  (kbd "H-a")
  ;; Double modifier combinations
  (kbd "C-M-a")
  (kbd "C-S-a")
  (kbd "C-s-a")
  (kbd "C-H-a")
  (kbd "M-S-a")
  (kbd "M-s-a")
  (kbd "M-H-a")
  ;; Triple combinations
  (kbd "C-M-S-a")
  (kbd "C-M-s-a")
  (kbd "C-M-H-a")
  ;; All five modifiers
  (kbd "C-M-S-s-H-a"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\u{1}\" [134217825] [33554529] [8388705] [16777313] [134217729] [33554433] [8388609] [16777217] [167772257] [142606433] [150995041] [167772161] [142606337] [150994945] [192937985])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// kbd multi-key sequences (common Emacs bindings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_multi_key_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test multi-key sequences that are common in Emacs
    let form = r#"(list
  ;; Classic two-key sequences
  (kbd "C-x C-f")
  (kbd "C-x C-s")
  (kbd "C-x C-c")
  (kbd "C-x b")
  (kbd "C-x o")
  (kbd "C-x k")
  (kbd "C-c C-c")
  (kbd "C-c C-k")
  ;; Three-key sequences
  (kbd "C-x r s")
  (kbd "C-x r i")
  (kbd "C-x 4 f")
  (kbd "C-x 5 2")
  ;; Mixed modifier sequences
  (kbd "C-x M-x")
  (kbd "M-g M-g")
  (kbd "C-x C-x")
  ;; Sequence with plain keys between modified
  (kbd "C-x a i g"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\u{18}\u{6}\" \"\u{18}\u{13}\" \"\u{18}\u{3}\" \"\u{18}b\" \"\u{18}o\" \"\u{18}k\" \"\u{3}\u{3}\" \"\u{3}\u{b}\" \"\u{18}rs\" \"\u{18}ri\" \"\u{18}4f\" \"\u{18}52\" [24 134217848] [134217831 134217831] \"\u{18}\u{18}\" \"\u{18}aig\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// kbd with function keys and special keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_function_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test function keys, special keys, and modified function keys
    let form = r#"(list
  ;; Named function keys
  (kbd "<f1>")
  (kbd "<f2>")
  (kbd "<f10>")
  (kbd "<f12>")
  ;; Modified function keys
  (kbd "C-<f1>")
  (kbd "M-<f5>")
  (kbd "S-<f3>")
  (kbd "C-M-<f8>")
  ;; Special keys
  (kbd "<return>")
  (kbd "<tab>")
  (kbd "<backspace>")
  (kbd "<delete>")
  (kbd "<escape>")
  (kbd "<home>")
  (kbd "<end>")
  ;; Modified special keys
  (kbd "C-<return>")
  (kbd "M-<tab>")
  (kbd "S-<backspace>")
  (kbd "C-<home>")
  (kbd "M-<end>")
  ;; Arrow keys
  (kbd "<up>")
  (kbd "<down>")
  (kbd "<left>")
  (kbd "<right>")
  (kbd "C-<up>")
  (kbd "M-<down>")
  (kbd "S-<left>")
  (kbd "C-M-<right>"))"#;
    let expect = expect_test::expect![[
        r#""OK ([f1] [f2] [f10] [f12] [C-f1] [M-f5] [S-f3] [C-M-f8] [return] [tab] [backspace] [delete] [escape] [home] [end] [C-return] [M-tab] [S-backspace] [C-home] [M-end] [up] [down] [left] [right] [C-up] [M-down] [S-left] [C-M-right])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// key-description roundtrip: kbd -> key-description -> verify string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_key_description_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // kbd produces a key sequence; key-description converts it back to
    // a human-readable string. Verify the roundtrip is consistent.
    let form = r#"(list
  ;; Simple keys
  (key-description (kbd "a"))
  (key-description (kbd "A"))
  (key-description (kbd "C-a"))
  (key-description (kbd "M-x"))
  (key-description (kbd "C-M-a"))
  ;; Multi-key sequences
  (key-description (kbd "C-x C-f"))
  (key-description (kbd "C-x C-s"))
  (key-description (kbd "C-c C-c"))
  (key-description (kbd "C-x b"))
  ;; Function keys
  (key-description (kbd "<f1>"))
  (key-description (kbd "C-<f5>"))
  (key-description (kbd "M-<return>"))
  ;; Special keys
  (key-description (kbd "<tab>"))
  (key-description (kbd "<backspace>"))
  (key-description (kbd "<escape>"))
  ;; Arrow keys
  (key-description (kbd "<up>"))
  (key-description (kbd "C-<down>"))
  ;; Three-key
  (key-description (kbd "C-x r s"))
  ;; Verify roundtrip: description of kbd matches input for simple cases
  (equal (key-description (kbd "C-x C-f")) "C-x C-f")
  (equal (key-description (kbd "M-x")) "M-x")
  (equal (key-description (kbd "C-M-a")) "C-M-a"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a\" \"A\" \"C-a\" \"M-x\" \"C-M-a\" \"C-x C-f\" \"C-x C-s\" \"C-c C-c\" \"C-x b\" \"<f1>\" \"C-<f5>\" \"M-<return>\" \"<tab>\" \"<backspace>\" \"<escape>\" \"<up>\" \"C-<down>\" \"C-x r s\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// event-convert-list with function key symbols and mouse events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_convert_list_extended() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // event-convert-list handles function key symbols, mouse events,
    // and character events with modifiers
    let form = r#"(list
  ;; Characters with modifiers
  (event-convert-list '(control ?a))
  (event-convert-list '(meta ?b))
  (event-convert-list '(control meta ?c))
  (event-convert-list '(shift ?d))
  (event-convert-list '(super ?e))
  (event-convert-list '(hyper ?f))
  ;; Order independence of modifiers
  (= (event-convert-list '(control meta ?x))
     (event-convert-list '(meta control ?x)))
  (= (event-convert-list '(control shift meta ?z))
     (event-convert-list '(meta control shift ?z)))
  ;; Function key symbols
  (event-convert-list '(f1))
  (event-convert-list '(control f1))
  (event-convert-list '(meta f5))
  (event-convert-list '(control meta f10))
  ;; Special keys
  (event-convert-list '(return))
  (event-convert-list '(control return))
  (event-convert-list '(tab))
  (event-convert-list '(meta tab))
  (event-convert-list '(backspace))
  ;; Mouse events
  (event-convert-list '(mouse-1))
  (event-convert-list '(control mouse-1))
  (event-convert-list '(meta mouse-2))
  (event-convert-list '(control meta mouse-3))
  (event-convert-list '(down-mouse-1))
  (event-convert-list '(double-mouse-1)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 134217826 134217731 68 8388709 16777318 t t f1 C-f1 M-f5 C-M-f10 return C-return tab M-tab backspace mouse-1 C-mouse-1 M-mouse-2 C-M-mouse-3 down-mouse-1 double-mouse-1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// single-key-description for various event types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_single_key_description_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // single-key-description converts a single event to its string description
    let form = r#"(list
  ;; Plain characters
  (single-key-description ?a)
  (single-key-description ?Z)
  (single-key-description ?0)
  (single-key-description ?!)
  (single-key-description ?@)
  ;; Space and special chars
  (single-key-description ?\s)   ;; space
  (single-key-description ?\t)   ;; tab
  (single-key-description 127)   ;; DEL
  (single-key-description 13)    ;; RET
  (single-key-description 27)    ;; ESC
  ;; Control characters
  (single-key-description ?\C-a)
  (single-key-description ?\C-z)
  (single-key-description ?\C-@)  ;; C-@ = NUL
  ;; Meta characters
  (single-key-description ?\M-a)
  (single-key-description ?\M-z)
  ;; Control-Meta
  (single-key-description ?\C-\M-a)
  (single-key-description ?\C-\M-z)
  ;; Events from event-convert-list
  (single-key-description (event-convert-list '(control ?x)))
  (single-key-description (event-convert-list '(meta ?x)))
  (single-key-description (event-convert-list '(super ?q)))
  (single-key-description (event-convert-list '(hyper ?h)))
  (single-key-description (event-convert-list '(control meta shift ?a)))
  ;; Verify consistency: single-key of kbd result
  (equal (single-key-description (aref (kbd "C-a") 0)) "C-a")
  (equal (single-key-description (aref (kbd "M-x") 0)) "M-x"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a\" \"Z\" \"0\" \"!\" \"@\" \"SPC\" \"TAB\" \"DEL\" \"RET\" \"ESC\" \"C-a\" \"C-z\" \"C-@\" \"M-a\" \"M-z\" \"C-M-a\" \"C-M-z\" \"C-x\" \"M-x\" \"s-q\" \"H-h\" \"C-M-S-a\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build keymap with kbd sequences, look up, describe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_keymap_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a real keymap using kbd for key definitions, then look up
    // bindings and describe the keys
    let form = r#"(let ((m (make-sparse-keymap)))
  ;; Define bindings using kbd
  (define-key m (kbd "C-c C-c") 'my-compile)
  (define-key m (kbd "C-c C-k") 'my-kill)
  (define-key m (kbd "C-x f") 'my-find)
  (define-key m (kbd "M-g g") 'my-goto)
  (define-key m (kbd "<f5>") 'my-refresh)
  (define-key m (kbd "C-<f5>") 'my-force-refresh)
  ;; Look up bindings
  (list
    ;; Direct lookups
    (lookup-key m (kbd "C-c C-c"))
    (lookup-key m (kbd "C-c C-k"))
    (lookup-key m (kbd "C-x f"))
    (lookup-key m (kbd "M-g g"))
    (lookup-key m (kbd "<f5>"))
    (lookup-key m (kbd "C-<f5>"))
    ;; Non-existent binding
    (lookup-key m (kbd "C-c C-z"))
    ;; Key description of what we bound
    (key-description (kbd "C-c C-c"))
    (key-description (kbd "C-c C-k"))
    (key-description (kbd "C-x f"))
    (key-description (kbd "M-g g"))
    (key-description (kbd "<f5>"))
    (key-description (kbd "C-<f5>"))
    ;; Verify kbd produces the same vector each time
    (equal (kbd "C-x C-f") (kbd "C-x C-f"))
    ;; Verify different keys produce different vectors
    (not (equal (kbd "C-x C-f") (kbd "C-x C-s")))))"#;
    let expect = expect_test::expect![[
        r#""OK (my-compile my-kill my-find my-goto my-refresh my-force-refresh nil \"C-c C-c\" \"C-c C-k\" \"C-x f\" \"M-g g\" \"<f5>\" \"C-<f5>\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// event-modifiers and event-basic-type with complex events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kbd_event_modifiers_and_basic_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test event-modifiers and event-basic-type to decompose events
    let form = r#"(let ((events (list
                   (cons "C-a" (event-convert-list '(control ?a)))
                   (cons "M-b" (event-convert-list '(meta ?b)))
                   (cons "C-M-c" (event-convert-list '(control meta ?c)))
                   (cons "S-d" (event-convert-list '(shift ?d)))
                   (cons "s-e" (event-convert-list '(super ?e)))
                   (cons "H-f" (event-convert-list '(hyper ?f)))
                   (cons "C-M-S-g" (event-convert-list '(control meta shift ?g)))
                   (cons "C-M-S-s-H-h" (event-convert-list '(control meta shift super hyper ?h))))))
  (mapcar
   (lambda (pair)
     (let ((name (car pair))
           (event (cdr pair)))
       (list name
             (event-modifiers event)
             (event-basic-type event)
             (single-key-description event))))
   events))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"C-a\" (control) 97 \"C-a\") (\"M-b\" (meta) 98 \"M-b\") (\"C-M-c\" (control meta) 99 \"C-M-c\") (\"S-d\" (shift) 100 \"D\") (\"s-e\" (super) 101 \"s-e\") (\"H-f\" (hyper) 102 \"H-f\") (\"C-M-S-g\" (shift control meta) 103 \"C-M-S-g\") (\"C-M-S-s-H-h\" (super hyper shift control meta) 104 \"C-H-M-S-s-h\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
