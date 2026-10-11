mod common;

fn parity(form: &str, expected: expect_test::Expect) {
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        expected.assert_eq(&common::run_oracle_eval(form).expect("GNU 31.1 oracle"));
    } else {
        expected.assert_eq(&common::run_neovm_eval(form).expect("neomacs runtime"));
    }
}

// GNU casefiddle.c:221-237 applies final sigma to every in-word downcasing.
#[test]
fn gdm_casing_final_sigma() {
    parity(
        r#"(list (capitalize "ΣΑΣ ΑΣ ΣΣ ΣAΣ!") (with-temp-buffer (insert "ΣΑΣ") (capitalize-region 1 4) (buffer-string)) (with-temp-buffer (insert "ΣΑΣ") (goto-char 1) (capitalize-word 1) (buffer-string)) (let ((table (copy-case-table (standard-case-table)))) (with-case-table table (capitalize "ΣΑΣ ΑΣ"))))"#,
        expect_test::expect![[r###"OK ("Σας Ας Σς Σaς!" "Σας" "Σας" "Σας Ας")"###]],
    );
}

// GNU casefiddle.c:248-277 strips event modifiers then restores them.
#[test]
fn gdm_casing_modifier_events() {
    parity(
        r#"(mapcar (lambda (fn) (mapcar (lambda (ch) (condition-case e (funcall fn ch) (error e))) (list ?\M-a ?\C-\S-a #x400000 (+ ?a (ash 1 27)) (ash 1 30) (+ (ash 1 32) 97) (+ (ash 1 40) 97) most-positive-fixnum ?\s-a (+ ?A (ash 1 22)) -1 (expt 2 80)))) '(upcase downcase capitalize upcase-initials))"#,
        expect_test::expect![[
            r###"OK ((134217793 33554433 4194304 134217793 1073741824 65 65 2305843009213693951 8388673 4194369 (wrong-type-argument char-or-string-p -1) (wrong-type-argument char-or-string-p 1208925819614629174706176)) (134217825 33554433 4194304 134217825 1073741824 4294967393 1099511627873 2305843009213693951 8388705 4194401 (wrong-type-argument char-or-string-p -1) (wrong-type-argument char-or-string-p 1208925819614629174706176)) (134217793 33554433 4194304 134217793 1073741824 65 65 2305843009213693951 8388673 4194369 (wrong-type-argument char-or-string-p -1) (wrong-type-argument char-or-string-p 1208925819614629174706176)) (134217793 33554433 4194304 134217793 1073741824 65 65 2305843009213693951 8388673 4194369 (wrong-type-argument char-or-string-p -1) (wrong-type-argument char-or-string-p 1208925819614629174706176)))"###
        ]],
    );
}

// GNU casefiddle.c:264-277 translates high unibyte bytes to byte8 characters.
#[test]
fn gdm_casing_unibyte_characters() {
    parity(
        r#"(mapcar (lambda (multi) (with-temp-buffer (set-buffer-multibyte multi) (mapcar (lambda (fn) (mapcar fn '(97 65 233 201 255 305 134218025))) '(upcase downcase capitalize upcase-initials)))) '(nil t))"#,
        expect_test::expect![[
            r###"OK (((65 65 233 201 255 305 134218024) (97 97 233 201 255 305 134218025) (65 65 233 201 255 73 134218024) (65 65 233 201 255 73 134218024)) ((65 65 201 201 376 305 134218024) (97 97 233 233 255 305 134218025) (65 65 201 201 376 73 134218024) (65 65 201 201 376 73 134218024)))"###
        ]],
    );
}

// GNU casefiddle.c:328-347 falls back to Unicode ASCII casing, never truncation.
#[test]
fn gdm_casing_unibyte_custom_table() {
    parity(
        r#"(let ((tbl (copy-case-table (standard-case-table)))) (set-case-syntax-pair ?İ ?i tbl) (set-case-syntax-pair ?I ?ı tbl) (with-case-table tbl (mapcar (lambda (fn) (list (funcall fn "ii XI") (string-to-list (funcall fn (unibyte-string 233 201 105 73))) (funcall fn (propertize "ii XI" 'face 'bold)))) '(upcase downcase capitalize upcase-initials))))"#,
        expect_test::expect![[
            r###"OK (("II XI" (233 201 73 73) #("II XI" 0 5 (face bold))) ("ii xi" (233 201 105 105) #("ii xi" 0 5 (face bold))) ("Ii Xi" (233 201 105 105) #("Ii Xi" 0 5 (face bold))) ("Ii XI" (233 201 105 73) #("Ii XI" 0 5 (face bold))))"###
        ]],
    );
}

// GNU casefiddle.c:74-85,137-151 special casing precedes all case tables.
#[test]
fn gdm_casing_custom_specials() {
    parity(
        r#"(let ((tbl (copy-case-table (standard-case-table)))) (set-case-syntax-pair ?İ ?i tbl) (set-case-syntax-pair ?ẞ ?ß tbl) (with-case-table tbl (mapcar (lambda (fn) (string-to-list (funcall fn "İ ß ﬃ"))) '(upcase downcase capitalize upcase-initials))))"#,
        expect_test::expect![[
            r###"OK ((304 32 83 83 32 70 70 73) (105 775 32 223 32 64259) (304 32 83 115 32 70 102 105) (304 32 83 115 32 70 102 105))"###
        ]],
    );
}

// GNU buffer.h:1649-1663 non-natnum table entries map a character to itself.
#[test]
fn gdm_casing_empty_table() {
    parity(
        r#"(with-case-table (make-char-table 'case-table) (mapcar (lambda (fn) (list (funcall fn "A a É é ΣΑΣ") (mapcar fn '(65 97 201 233 223 304 64259 329 452 453 454 8064 8115 4304)) (funcall fn "ß İ ﬃ"))) '(upcase downcase capitalize upcase-initials)))"#,
        expect_test::expect![[
            r###"OK (("A a É é ΣΑΣ" (65 97 201 233 223 304 64259 329 452 453 454 8064 8115 4304) "SS İ FFI") ("A a É é ΣΑΣ" (65 97 201 233 223 304 64259 329 452 453 454 8064 8115 4304) "ß i̇ ﬃ") ("A A É É ΣΑΣ" (65 65 201 201 223 304 64259 329 453 453 453 8072 8124 4304) "Ss İ Ffi") ("A A É É ΣΑΣ" (65 65 201 201 223 304 64259 329 453 453 453 8072 8124 4304) "Ss İ Ffi"))"###
        ]],
    );
}

// GNU editfns.c:1769-1771,1860-1864 compares through the current canon table.
#[test]
fn gdm_casing_compare_canon() {
    parity(
        r#"(list (with-temp-buffer (insert "az") (let ((tbl (copy-case-table (standard-case-table)))) (set-case-syntax-pair ?Z ?a tbl) (set-case-table tbl)) (mapcar (lambda (fold) (let ((case-fold-search fold)) (compare-buffer-substrings nil 1 2 nil 2 3))) '(nil t))) (mapcar (lambda (text) (with-temp-buffer (insert text) (let ((case-fold-search t)) (compare-buffer-substrings nil 1 2 nil 2 3)))) '("σς" "İi" "µμ" "aA")) (with-temp-buffer (insert "aA") (set-case-table (make-char-table 'case-table)) (compare-buffer-substrings nil 1 2 nil 2 3)))"#,
        expect_test::expect![[r###"OK ((-1 0) (0 1 0 0) 1)"###]],
    );
}

// GNU bytecode.c:Bupcase/Bdowncase and Bcall share the casing primitives.
#[test]
fn gdm_casing_compiled_calls() {
    parity(
        r#"(progn (require 'bytecomp) (let ((f (byte-compile '(lambda (obj) (list (upcase obj) (downcase obj) (capitalize obj) (upcase-initials obj)))))) (mapcar (lambda (obj) (mapcar (lambda (value) (if (stringp value) (list (string-to-list value) (multibyte-string-p value)) value)) (funcall f obj))) (list ?\M-a "ΣΑΣ" "ß" (unibyte-string 233 65) (+ ?A (ash 1 22))))))"#,
        expect_test::expect![[
            r###"OK ((134217793 134217825 134217793 134217793) (((931 913 931) t) ((963 945 962) t) ((931 945 962) t) ((931 913 931) t)) (((83 83) t) ((223) t) ((83 115) t) ((83 115) t)) (((233 65) nil) ((233 97) nil) ((233 97) nil) ((233 65) nil)) (4194369 4194401 4194369 4194369))"###
        ]],
    );
}

// GNU casefiddle.c:343-344 applies fallback to strings;440-460 writes region
// bytes directly.221-237 uses changed sigma even for a custom upcase table.
#[test]
fn gdm_casing_custom_table_callers() {
    parity(
        r#"(list (let ((tbl (copy-case-table (standard-case-table)))) (set-case-syntax-pair ?İ ?i tbl) (with-case-table tbl (list (upcase "ii") (with-temp-buffer (set-buffer-multibyte nil) (set-case-table tbl) (insert "ii") (upcase-region 1 3) (string-to-list (buffer-string))) (with-temp-buffer (set-buffer-multibyte nil) (set-case-table tbl) (insert "ii") (goto-char 1) (upcase-word 1) (string-to-list (buffer-string)))))) (let ((tbl (copy-case-table (standard-case-table)))) (set-case-syntax-pair ?σ ?Σ tbl) (with-case-table tbl (list (upcase "ΑΣ ΣΣ") (with-temp-buffer (set-case-table tbl) (insert "ΑΣ ΣΣ") (upcase-region 1 (point-max)) (buffer-string)) (with-temp-buffer (set-case-table tbl) (insert "ΑΣ") (goto-char 1) (upcase-word 1) (buffer-string))))))"#,
        expect_test::expect![[r###"OK (("II" (48 48) (48 48)) ("Ας σς" "Ας σς" "Ας"))"###]],
    );
}

// GNU buffer.h:1649-1663 feeds both equality and replacement classification.
#[test]
fn gdm_casing_case_table_shared_consumers() {
    parity(
        r#"(mapcar (lambda (empty) (let ((ct (if empty (make-char-table 'case-table) (copy-case-table (standard-case-table))))) (with-case-table ct (list (mapcar (lambda (pair) (char-equal (car pair) (cadr pair))) '((97 65) (233 201) (963 962) (181 956))) (mapcar (lambda (multi) (with-temp-buffer (set-buffer-multibyte multi) (set-case-table ct) (insert (propertize "AAA" 'face 'bold)) (goto-char 1) (search-forward "AAA") (replace-match "mix" nil nil) (buffer-string))) '(nil t)))))) '(nil t))"#,
        expect_test::expect![[
            r###"OK (((t t nil nil) ("MIX" "MIX")) ((nil nil nil nil) ("mix" "mix")))"###
        ]],
    );
}
