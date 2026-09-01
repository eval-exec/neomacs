//! Advanced oracle parity tests for event-convert-list, key-description,
//! single-key-description, and internal-event-symbol-parse-modifiers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// event-convert-list with full modifier combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_all_modifier_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every single modifier and multi-modifier combination
    let form = r#"(list
      (event-convert-list '(control ?a))
      (event-convert-list '(meta ?a))
      (event-convert-list '(shift ?a))
      (event-convert-list '(super ?a))
      (event-convert-list '(hyper ?a))
      (event-convert-list '(control meta ?a))
      (event-convert-list '(control shift ?a))
      (event-convert-list '(control super ?a))
      (event-convert-list '(control hyper ?a))
      (event-convert-list '(meta shift ?a))
      (event-convert-list '(meta super ?a))
      (event-convert-list '(meta hyper ?a))
      (event-convert-list '(control meta shift ?a))
      (event-convert-list '(control meta shift super ?a))
      (event-convert-list '(control meta shift super hyper ?a)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 134217825 65 8388705 16777313 134217729 33554433 8388609 16777217 134217793 142606433 150995041 167772161 176160769 192937985)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// event-convert-list with mouse events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_mouse_events() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mouse button symbols with modifiers
    let form = r#"(list
      (event-convert-list '(mouse-1))
      (event-convert-list '(control mouse-1))
      (event-convert-list '(meta mouse-1))
      (event-convert-list '(control meta mouse-1))
      (event-convert-list '(shift mouse-2))
      (event-convert-list '(control shift mouse-3))
      (event-convert-list '(down-mouse-1))
      (event-convert-list '(control down-mouse-1))
      (event-convert-list '(double-mouse-1))
      (event-convert-list '(meta double-mouse-1)))"#;
    let expect = expect_test::expect![[
        r#""OK (mouse-1 C-mouse-1 M-mouse-1 C-M-mouse-1 S-mouse-2 C-S-mouse-3 down-mouse-1 C-down-mouse-1 double-mouse-1 M-double-mouse-1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// key-description with various key sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_key_description_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // key-description on vectors and strings with various key events
    let form = r#"(list
      (key-description [?a])
      (key-description [?A])
      (key-description [?\C-a])
      (key-description [?\M-a])
      (key-description [?\C-\M-a])
      (key-description [?\C-x ?f])
      (key-description [?\C-x ?\C-f])
      (key-description [?\C-x ?\C-s])
      (key-description [?\M-x])
      (key-description "abc")
      (key-description "\C-a\C-b\C-c")
      (key-description [escape ?x])
      (key-description [tab])
      (key-description [return])
      (key-description [backspace]))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a\" \"A\" \"C-a\" \"M-a\" \"C-M-a\" \"C-x f\" \"C-x C-f\" \"C-x C-s\" \"M-x\" \"a b c\" \"C-a C-b C-c\" \"<escape> x\" \"<tab>\" \"<return>\" \"<backspace>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// single-key-description for individual events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_single_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (single-key-description ?a)
      (single-key-description ?A)
      (single-key-description ?\C-a)
      (single-key-description ?\M-a)
      (single-key-description ?\C-\M-a)
      (single-key-description ?\s)
      (single-key-description ?\t)
      (single-key-description 127)
      (single-key-description (event-convert-list '(control ?x)))
      (single-key-description (event-convert-list '(meta control shift ?z)))
      (single-key-description (event-convert-list '(super ?q)))
      (single-key-description (event-convert-list '(hyper ?h))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a\" \"A\" \"C-a\" \"M-a\" \"C-M-a\" \"SPC\" \"TAB\" \"DEL\" \"C-x\" \"C-M-S-z\" \"s-q\" \"H-h\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// internal-event-symbol-parse-modifiers with various symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_parse_modifiers_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (internal-event-symbol-parse-modifiers 'C-x)
      (internal-event-symbol-parse-modifiers 'M-x)
      (internal-event-symbol-parse-modifiers 'C-M-x)
      (internal-event-symbol-parse-modifiers 'S-return)
      (internal-event-symbol-parse-modifiers 's-a)
      (internal-event-symbol-parse-modifiers 'H-f1)
      (internal-event-symbol-parse-modifiers 'C-S-M-z)
      (internal-event-symbol-parse-modifiers 'mouse-1)
      (internal-event-symbol-parse-modifiers 'C-mouse-1)
      (internal-event-symbol-parse-modifiers 'C-M-mouse-3)
      (internal-event-symbol-parse-modifiers 'down-mouse-1)
      (internal-event-symbol-parse-modifiers 'C-down-mouse-1))"#;
    let expect = expect_test::expect![[
        r#""OK ((x control) (x meta) (x meta control) (return shift) (a super) (f1 hyper) (z meta control shift) (mouse-1 click) (mouse-1 control click) (mouse-3 meta control click) (mouse-1 down) (mouse-1 control down))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// key-description roundtrip: convert -> describe -> verify
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_roundtrip_describe() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build key sequences from event-convert-list and describe them
    let form = r#"(list
      (key-description
       (vector (event-convert-list '(control ?x))
               (event-convert-list '(control ?s))))
      (key-description
       (vector (event-convert-list '(control ?x))
               ?b))
      (key-description
       (vector (event-convert-list '(meta ?x))))
      (key-description
       (vector (event-convert-list '(control meta ?c))
               (event-convert-list '(control meta ?k))))
      (key-description
       (vector (event-convert-list '(super ?l))
               (event-convert-list '(hyper ?r)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"C-x C-s\" \"C-x b\" \"M-x\" \"C-M-c C-M-k\" \"s-l H-r\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: keymap binding with event-convert-list keys and lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_keymap_multi_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a multi-key binding using event-convert-list and look it up
    let form = r#"(let ((m (make-sparse-keymap))
                        (prefix-map (make-sparse-keymap)))
                   (define-key prefix-map
                     (vector (event-convert-list '(control ?s)))
                     'save-fn)
                   (define-key prefix-map
                     (vector (event-convert-list '(control ?f)))
                     'find-fn)
                   (define-key m
                     (vector (event-convert-list '(control ?x)))
                     prefix-map)
                   (list
                    (lookup-key m
                      (vector (event-convert-list '(control ?x))
                              (event-convert-list '(control ?s))))
                    (lookup-key m
                      (vector (event-convert-list '(control ?x))
                              (event-convert-list '(control ?f))))
                    ;; describe what we bound
                    (key-description
                     (vector (event-convert-list '(control ?x))
                             (event-convert-list '(control ?s))))
                    (key-description
                     (vector (event-convert-list '(control ?x))
                             (event-convert-list '(control ?f))))))"#;
    let expect = expect_test::expect![[r#""OK (save-fn find-fn \"C-x C-s\" \"C-x C-f\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: modifier bit-level consistency checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_convert_advanced_modifier_bit_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify modifier encoding is consistent across the API
    let form = r#"(let ((ctl-a (event-convert-list '(control ?a)))
                        (meta-a (event-convert-list '(meta ?a)))
                        (cm-a (event-convert-list '(control meta ?a))))
                   (list
                    ;; basic modifier checks via event-modifiers
                    (event-modifiers ctl-a)
                    (event-modifiers meta-a)
                    (event-modifiers cm-a)
                    ;; event-basic-type should strip modifiers
                    (event-basic-type ctl-a)
                    (event-basic-type meta-a)
                    (event-basic-type cm-a)
                    ;; single-key-description consistency
                    (equal (single-key-description ctl-a) "C-a")
                    (equal (single-key-description meta-a) "M-a")
                    (equal (single-key-description cm-a) "C-M-a")
                    ;; Verify event-convert-list order independence
                    (= (event-convert-list '(control meta ?z))
                       (event-convert-list '(meta control ?z)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((control) (meta) (control meta) 97 97 97 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
