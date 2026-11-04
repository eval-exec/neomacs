//! Public builtin boundaries use the active SYMBOLP/XSYMBOL/EQ policy.

use crate::emacs_core::{eval::Context, print::print_value};

fn answer(source: &str) -> String {
    let mut context = Context::new();
    print_value(&context.eval_str(source).expect("positioned builtin form"))
}

#[test]
fn positioned_builtin_hash_designators_and_keywords_follow_flag() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (let ((table (make-hash-table
                              (position-symbol :test 0) (position-symbol 'eq 1)
                              (position-symbol :weakness 2) (position-symbol t 3))))
                 (list (hash-table-test table) (hash-table-weakness table))))"
        ),
        "(eq key-and-value)"
    );
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled nil))
               (condition-case err
                   (make-hash-table (position-symbol :test 0) 'eq)
                 (error (car err))))"
        ),
        "error"
    );
}

#[test]
fn positioned_builtin_char_table_subtype_check_follows_flag() {
    assert_eq!(
        answer(
            "(let ((subtype (position-symbol 'issue554-chartable 0)))
               (list (let ((symbols-with-pos-enabled t))
                       (eq (char-table-subtype (make-char-table subtype))
                           'issue554-chartable))
                     (let ((symbols-with-pos-enabled nil))
                       (condition-case err (make-char-table subtype)
                         (error (car err))))))"
        ),
        "(t wrong-type-argument)"
    );
}

#[test]
fn positioned_builtin_charset_names_follow_flag() {
    assert_eq!(
        answer(
            "(let ((charset (position-symbol 'ascii 0)))
               (list (let ((symbols-with-pos-enabled t))
                       (list (charsetp charset) (decode-char charset 65)
                             (encode-char 65 charset)))
                     (let ((symbols-with-pos-enabled nil)) (charsetp charset))))"
        ),
        "((t 65 65) nil)"
    );
}

#[test]
fn positioned_builtin_json_keys_options_and_sentinels_follow_flag() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (list (json-serialize (list (cons (position-symbol 'key 0) 1)))
                     (json-serialize (position-symbol :null 0))
                     (json-serialize (position-symbol t 0))
                     (json-parse-string \"{\\\"key\\\":1}\"
                                        (position-symbol :object-type 0)
                                        (position-symbol 'plist 0))))"
        ),
        "(\"{\\\"key\\\":1}\" \"null\" \"true\" (:key 1))"
    );
}

#[test]
fn positioned_builtin_hash_algorithms_and_md5_coding_follow_flag() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (list (equal (secure-hash (position-symbol 'sha256 0) \"x\")
                            (secure-hash 'sha256 \"x\"))
                     (equal (md5 \"x\" nil nil (position-symbol 'utf-8-unix 0))
                            (md5 \"x\" nil nil 'utf-8-unix))))"
        ),
        "(t t)"
    );
}

#[test]
fn positioned_builtin_ccl_program_registration_and_lookup_follow_flag() {
    assert_eq!(
        answer(
            "(let ((name (position-symbol 'issue554-ccl-program 0)))
               (let ((symbols-with-pos-enabled t))
                 (register-ccl-program name [0 0 0]))
               (list (let ((symbols-with-pos-enabled t)) (ccl-program-p name))
                     (let ((symbols-with-pos-enabled nil)) (ccl-program-p name))))"
        ),
        "(t nil)"
    );
}

#[test]
fn positioned_builtin_treesit_symbol_language_and_pattern_selectors() {
    assert_eq!(
        answer(
            "(let ((symbols-with-pos-enabled t))
               (list (treesit-pattern-expand (position-symbol :anchor 0))
                     (treesit-query-p
                       (treesit-query-compile
                         (position-symbol 'issue554-unavailable-language 0)
                         \"(_)\" nil))))"
        ),
        "(\"#<symbol :anchor at 0>\" t)"
    );
}
