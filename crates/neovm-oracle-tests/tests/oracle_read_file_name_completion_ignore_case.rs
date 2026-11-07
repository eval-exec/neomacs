//! GNU has no C DEFVAR for `read-file-name-completion-ignore-case`: its
//! defcustom (`lisp/minibuffer.el:3117`) selects the platform default, which
//! is t on darwin/windows-nt/ms-dos/cygwin. Pre-seeding the name in the Rust
//! bootstrap shadowed that initializer (#667).

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn read_file_name_completion_ignore_case_matches_gnus_platform_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(eq read-file-name-completion-ignore-case
               (if (memq system-type '(ms-dos windows-nt darwin cygwin)) t nil))"#,
        expect_test::expect![[r#""OK t""#]],
    );
}
