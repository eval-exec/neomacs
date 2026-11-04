use crate::test_utils::runtime_startup_eval_one;

#[test]
fn printer_accepts_oversized_character_bytes_in_a_shorthand_symbol_name() {
    crate::test_utils::init_test_tracing();
    // A multibyte replacement followed by a raw unibyte suffix is a public
    // way to create a supplied-count symbol name. The suffix must not let
    // printing construct a character outside the Emacs character domain.
    assert_eq!(
        runtime_startup_eval_one(
            r#"(let* ((read-symbol-shorthands (list (cons "q" (string #xe9))))
                      (symbol (intern (unibyte-string ?q #xf8 #xb8 #x96 #xb8 #x96)
                                      (obarray-make)))
                      (name (symbol-name symbol))
                      (printed (prin1-to-string symbol)))
                 (list (stringp printed)
                       (eq name (symbol-name symbol))
                       (append (string-as-unibyte name) nil)))"#,
        ),
        "OK (t t (195 169 248 184 150 184 150))",
    );
}

#[test]
fn delete_preserves_an_oversized_shorthand_name_when_the_character_is_absent() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_one(
            r#"(let* ((read-symbol-shorthands (list (cons "q" (string #xe9))))
                      (symbol (intern (unibyte-string ?q #xf8 #xb8 #x96 #xb8 #x96)
                                      (obarray-make)))
                      (name (symbol-name symbol))
                      (result (delete 0 name)))
                 (list (eq result name)
                       (append (string-as-unibyte result) nil)
                       (append (string-as-unibyte (symbol-name symbol)) nil)))"#,
        ),
        "OK (t (195 169 248 184 150 184 150) (195 169 248 184 150 184 150))",
    );
}

#[test]
fn delete_removes_an_ascii_suffix_from_an_oversized_shorthand_name() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_one(
            r#"(let* ((read-symbol-shorthands (list (cons "q" (string #xe9))))
                      (symbol (intern (unibyte-string ?q #xf8 #xb8 #x96 #xb8 #x96 ?z)
                                      (obarray-make)))
                      (name (symbol-name symbol))
                      (result (delete ?z name)))
                 (list (length name) (string-bytes name)
                       (length result) (string-bytes result)
                       (multibyte-string-p result)
                       (append (string-as-unibyte result) nil)
                       (append (string-as-unibyte name) nil)))"#,
        ),
        "OK (7 8 6 7 t (195 169 248 184 150 184 150) (195 169 248 184 150 184 150 122))",
    );
}

#[test]
fn delete_preserves_supplied_count_above_physical_decoding_count() {
    crate::test_utils::init_test_tracing();
    // The declared third character is beyond the initialized payload.
    // Clamping copied spans to physical storage is a bounded emulator
    // policy; GNU can copy its terminal NUL, so this is not an oracle.
    assert_eq!(
        runtime_startup_eval_one(
            r#"(let* ((read-symbol-shorthands
                       (list (cons (string #xe9) (string #x3bb))))
                      (symbol (intern (unibyte-string #xc3 #xa9 ?z)
                                      (obarray-make)))
                      (name (symbol-name symbol))
                      (result (delete ?z name)))
                 (list (length name) (length result) (string-bytes result)
                       (multibyte-string-p result)
                       (append (string-as-unibyte result) nil)
                       (append (string-as-unibyte name) nil)))"#,
        ),
        "OK (3 2 2 t (206 187) (206 187 122))",
    );
}

#[test]
fn delete_does_not_visit_bytes_outside_the_supplied_character_count() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_one(
            r#"(let* ((read-symbol-shorthands
                       (list (cons (unibyte-string #xc3 #xa9) (string #x3bb))))
                      (symbol (intern (concat (string #xe9) "z")
                                      (obarray-make)))
                      (name (symbol-name symbol))
                      (result (delete ?z name)))
                 (list (length name) (eq result name) (length result)
                       (append (string-as-unibyte result) nil)
                       (append (string-as-unibyte name) nil)))"#,
        ),
        "OK (1 t 1 (206 187 122) (206 187 122))",
    );
}
