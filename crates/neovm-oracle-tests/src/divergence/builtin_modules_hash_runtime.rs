//! Builtin-module presence + hashing parity: sqlite-available-p / treesit-
//! available-p return a boolean, json builtin fns fboundp, module-file-suffix /
//! system-configuration, secure-hash sha224/384/512 lengths + md5 of empty,
//! base64url encode/decode, buffer-hash, benchmark-run shape.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn base64_url_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function base64url-decode-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (base64url-encode-string "subjects?_d" t)
        (base64-encode-string "Hello" t)
        (base64url-decode-string "c3ViamVjdHM_X2Q"))"##,
        expect,
    );
}

#[test]
fn benchmark_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (numberp (car (benchmark-run 1 (+ 1 2))))
        (= (length (benchmark-run 2 (ignore))) 3))"##,
        expect,
    );
}

#[test]
fn buffer_hash_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (insert "content")
  (list (stringp (buffer-hash)) (= (length (buffer-hash)) (length (buffer-hash)))))"##,
        expect,
    );
}

#[test]
fn json_builtin_fns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'json-serialize) (fboundp 'json-parse-string)
        (fboundp 'json-parse-buffer) (fboundp 'json-insert))"##,
        expect,
    );
}

#[test]
fn module_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (booleanp (featurep 'dynamic-setting))
        (booleanp (featurep 'json)) (booleanp module-file-suffix)
        (stringp (or system-configuration "")))"##,
        expect,
    );
}

#[test]
fn secure_hash_algos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (56 96 128 \"d41d8cd98f00b204e9800998ecf8427e\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length (secure-hash 'sha224 "x"))
        (length (secure-hash 'sha384 "x"))
        (length (secure-hash 'sha512 "x"))
        (secure-hash 'md5 ""))"##,
        expect,
    );
}

#[test]
fn sqlite_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'sqlite-available-p)
        (if (fboundp 'sqlite-available-p) (booleanp (sqlite-available-p)) 'no-fn))"##,
        expect,
    );
}

#[test]
fn treesit_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'treesit-available-p)
        (if (fboundp 'treesit-available-p) (booleanp (treesit-available-p)) 'no-fn))"##,
        expect,
    );
}
