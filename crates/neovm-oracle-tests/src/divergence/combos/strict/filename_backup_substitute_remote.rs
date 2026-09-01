//! Strict combo oracle probes, batch 66: file-name string manipulation —
//! backup-name construction, version suffix stripping, $VAR substitution,
//! OS-standard conversion, tramp remote detection, and filename abbreviation.
//! These are pure string operations (no filesystem access needed).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o0_backup_name_and_version_strip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/path/to/file.txt~\" \"relative.el~\" \"/path/to/file.txt\" \"/path/to/file.txt\" \"file.txt%\" \"/path/to/file.txt\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (make-backup-file-name "/path/to/file.txt")
      (make-backup-file-name "relative.el")
      (file-name-sans-versions "/path/to/file.txt~")
      (file-name-sans-versions "/path/to/file.txt.~3~")
      (file-name-sans-versions "file.txt%")
      (file-name-sans-versions "/path/to/file.txt"))
"##,
        expect,
    );
}

#[test]
fn div_o0_substitute_in_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/test/path/foo\" \"/plain/path\" \"~/expanded/sub\" \"$UNDEFINED_VAR\" \"/before/test/path/after\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "PROBE_FN_DIR=/test/path" process-environment)))
  (list (substitute-in-file-name "$PROBE_FN_DIR/foo")
        (substitute-in-file-name "/plain/path")
        (substitute-in-file-name "~/expanded/sub")
        (substitute-in-file-name "$UNDEFINED_VAR")
        (substitute-in-file-name "/before$PROBE_FN_DIR/after")))
"##,
        expect,
    );
}

#[test]
fn div_o0_convert_standard_filename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/path/to/file.txt\" \"/path/with space/file\" \"/path/with(maybe)/file\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (convert-standard-filename "/path/to/file.txt")
      (convert-standard-filename "/path/with space/file")
      (convert-standard-filename "/path/with(maybe)/file"))
"##,
        expect,
    );
}

#[test]
fn div_o0_file_remote_and_local_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil \"/method:host:\" \"/ssh:user@host:\" \"/remote/path\" \"/local/path\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-remote-p "/local/path")
      (file-remote-p "/method:host:/remote/path")
      (file-remote-p "/ssh:user@host:/remote/path")
      (file-local-name "/method:host:/remote/path")
      (file-local-name "/local/path"))
"##,
        expect,
    );
}

#[test]
fn div_o0_abbreviate_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/\" \"/absolute/unmodified/path\" \"c\" \"../b/c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (abbreviate-file-name default-directory)
      (abbreviate-file-name "/absolute/unmodified/path")
      (file-relative-name "/a/b/c" "/a/b")
      (file-relative-name "/a/b/c" "/a/d"))
"##,
        expect,
    );
}
