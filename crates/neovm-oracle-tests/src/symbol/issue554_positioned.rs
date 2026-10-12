//! GNU SYMBOLP / XSYMBOL / EQ boundaries, including real byte-compiler inputs.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

fn check(form: &str, expected: &str) {
    let (oracle, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(expected, &oracle, &neomacs);
}

#[test]
fn issue554_oracle_positioned_builtin_hash_designators_and_keywords_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((symbols-with-pos-enabled t))
               (let ((table (make-hash-table
                              (position-symbol :test 0) (position-symbol 'eq 1)
                              (position-symbol :weakness 2) (position-symbol t 3))))
                 (list (hash-table-test table) (hash-table-weakness table))))",
        "(eq key-and-value)",
    );
    check(
        "(let ((symbols-with-pos-enabled nil))
               (condition-case err
                   (make-hash-table (position-symbol :test 0) 'eq)
                 (error (car err))))",
        "error",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_char_table_subtype_check_follows_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((subtype (position-symbol 'issue554-chartable 0)))
               (list (let ((symbols-with-pos-enabled t))
                       (eq (char-table-subtype (make-char-table subtype))
                           'issue554-chartable))
                     (let ((symbols-with-pos-enabled nil))
                       (condition-case err (make-char-table subtype)
                         (error (car err))))))",
        "(t wrong-type-argument)",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_charset_names_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((charset (position-symbol 'ascii 0)))
               (list (let ((symbols-with-pos-enabled t))
                       (list (charsetp charset) (decode-char charset 65)
                             (encode-char 65 charset)))
                     (let ((symbols-with-pos-enabled nil)) (charsetp charset))))",
        "((t 65 65) nil)",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_json_keys_options_and_sentinels_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((symbols-with-pos-enabled t))
               (list (json-serialize (list (cons (position-symbol 'key 0) 1)))
                     (json-serialize (position-symbol :null 0))
                     (json-serialize (position-symbol t 0))
                     (json-parse-string \"{\\\"key\\\":1}\"
                                        (position-symbol :object-type 0)
                                        (position-symbol 'plist 0))))",
        "(\"{\\\"key\\\":1}\" \"null\" \"true\" (:key 1))",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_hash_algorithms_and_md5_coding_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((symbols-with-pos-enabled t))
               (list (equal (secure-hash (position-symbol 'sha256 0) \"x\")
                            (secure-hash 'sha256 \"x\"))
                     (equal (md5 \"x\" nil nil (position-symbol 'utf-8-unix 0))
                            (md5 \"x\" nil nil 'utf-8-unix))))",
        "(t t)",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_ccl_program_registration_and_lookup_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((name (position-symbol 'issue554-ccl-program 0)))
               (let ((symbols-with-pos-enabled t))
                 (register-ccl-program name [0 0 0]))
               (list (let ((symbols-with-pos-enabled t)) (ccl-program-p name))
                     (let ((symbols-with-pos-enabled nil)) (ccl-program-p name))))",
        "(t nil)",
    );
}

#[test]
fn issue554_oracle_positioned_builtin_treesit_symbol_language_and_pattern_selectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let ((symbols-with-pos-enabled t))
               (list (treesit-pattern-expand (position-symbol :anchor 0))
                     (treesit-query-p
                       (treesit-query-compile
                         (position-symbol 'issue554-unavailable-language 0)
                         \"(_)\" nil))))",
        "(\"#<symbol :anchor at 0>\" t)",
    );
}

#[test]
fn issue554_oracle_coding_and_lexical_repros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(let ((symbols-with-pos-enabled t))
      (list (coding-system-p (position-symbol 'utf-8-unix 0))
            (eq (check-coding-system (position-symbol 'utf-8-unix 0)) 'utf-8-unix)
            (decode-coding-string "abc" (position-symbol 'utf-8-unix 0))
            (eval 'n (list (cons (position-symbol 'n 0) 7)))))"#,
        "(t t \"abc\" 7)",
    );
}

#[test]
fn issue554_oracle_type_and_symbol_identity_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(let ((sp (position-symbol 'issue554-symbol 3)))
      (list (let ((symbols-with-pos-enabled nil))
              (list (symbolp sp) (type-of sp) (cl-type-of sp)
                    (eq sp 'issue554-symbol) (memq sp '(issue554-symbol))))
            (let ((symbols-with-pos-enabled t))
              (list (symbolp sp) (type-of sp) (cl-type-of sp)
                    (eq sp 'issue554-symbol) (car (memq sp '(issue554-symbol)))
                    (cdr (assq sp '((issue554-symbol . 7))))))))"#,
        "((nil symbol-with-pos symbol-with-pos nil nil) (t symbol symbol-with-pos t issue554-symbol 7))",
    );
}

fn compile_roundtrip(source: &str, function: &str, expected: &str) {
    // Each runtime owns its temporary directory. Only the return value is compared.
    let quoted = format!("{:?}", source);
    let form = format!(
        r#"(progn (require 'bytecomp)
      (let* ((dir (make-temp-file "issue554-" t))
             (file (expand-file-name "r.el" dir)))
        (unwind-protect
            (progn (with-temp-file file
                     (insert ";;; -*- lexical-binding: t -*-\n" {quoted}))
                   (byte-compile-file file)
                   (load (concat file "c") nil t t)
                   ({function}))
          (delete-directory dir t))))"#
    );
    check(&form, expected);
}

#[test]
fn issue554_oracle_compile_load_coding_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    compile_roundtrip(
        "(defmacro issue554-m (x) (list 'quote (eval x)))\n(defun issue554-f () (issue554-m (coding-system-p 'utf-8-unix)))\n",
        "issue554-f",
        "t",
    );
}

#[test]
fn issue554_oracle_compile_load_markdown_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Offline markdown-mode:2172-2179 and its invocation:2187-2199.
    compile_roundtrip(
        r#"(defmacro markdown--dotimes-when-compile (i-n body)
      (let ((var (car i-n)) (n (cadr i-n)) (code ()))
        (dotimes (i (eval n t)) (push (eval body `((,var . ,i))) code))
        `(progn ,@(nreverse code))))
      (defun issue554-markdown ()
        (markdown--dotimes-when-compile (num 1) `(quote ,(1+ num))))"#,
        "issue554-markdown",
        "1",
    );
}

#[test]
fn issue554_oracle_compile_load_sly_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Offline sly-tests:178 and utf8-source:744-753, without its test framework.
    compile_roundtrip(
        r#"(defmacro issue554-sly-inputs (inputs) (list 'quote (eval inputs)))
      (defun issue554-sly ()
        (issue554-sly-inputs
          (list (decode-coding-string "\343\201\212" 'utf-8-unix))))"#,
        "issue554-sly",
        "(\"お\")",
    );
}

#[test]
fn issue554_positioned_feature_membership_and_original_storage_oracle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Exercise dynamic binding explicitly inside the lexical oracle wrapper.
    check(
        "(eval '(let ((features nil) (sp (position-symbol 'issue554-feature 0)))
           (list (let ((symbols-with-pos-enabled t))
                   (provide sp)
                   (list (featurep 'issue554-feature)
                         (symbol-with-pos-p (car features))))
                 (let ((symbols-with-pos-enabled nil)) (featurep 'issue554-feature))
                 (let ((symbols-with-pos-enabled t)) (featurep 'issue554-feature)))) nil)",
        "((t t) nil t)",
    );
}

#[test]
fn issue554_intern_soft_preserves_position_and_exact_symbol_identity_oracle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(let* ((oa (obarray-make 2)) (s (intern \"issue554-intern\" oa))
                (sp (position-symbol s 0)) (other (make-symbol \"issue554-intern\")))
           (let ((symbols-with-pos-enabled t))
             (list (symbol-with-pos-p (intern-soft sp oa))
                   (intern-soft other oa)
                   (symbol-with-pos-p (intern-soft (position-symbol 'car 0)))
                   (intern-soft (make-symbol \"car\")))))",
        "(t nil t nil)",
    );
}

#[test]
fn issue554_functionp_rejects_non_nil_autoload_type_in_both_modes_oracle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        "(progn (fset 'issue554-autoload-function
                   (list 'autoload \"issue554-missing\" nil nil (position-symbol 'macro 0)))
           (list (let ((symbols-with-pos-enabled nil)) (functionp 'issue554-autoload-function))
                 (let ((symbols-with-pos-enabled t)) (functionp 'issue554-autoload-function))))",
        "(nil nil)",
    );
}
