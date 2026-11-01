//! Strict combo oracle probes, batch 25: user identity, file/directory paths,
//! load-path/suffixes, locale and default coding system, and safe emacs-state
//! queries (emacs-pid type, system-configuration/type, memory-use-counts
//! shape). PID and raw addresses are deliberately avoided.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g0_user_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function user-mail-address)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (stringp (user-login-name))
      (stringp (user-real-login-name))
      (user-full-name)
      (stringp (user-mail-address))
      (integerp (user-uid))
      (integerp (user-gid)))
"##,
        expect,
    );
}

#[test]
fn div_g0_file_paths_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/\" \"[SESSION-TMPDIR]/\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list default-directory
      temporary-file-directory
      (stringp data-directory)
      (stringp exec-directory)
      (stringp invocation-directory))
"##,
        expect,
    );
}

#[test]
fn div_g0_doc_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK t
    // Neomacs:   OK nil
    // doc-directory is a string in GNU Emacs but nil in Neomacs (Neomacs does
    // not set the documentation directory).
    crate::common::assert_oracle_parity_expect(
        r##"
(stringp doc-directory)
"##,
        expect,
    );
}

#[test]
fn div_g0_load_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // `load-suffixes' is a build fact: on Darwin configure sets
    // DYNAMIC_LIB_SUFFIX to ".dylib" (configure.ac:5028) and `init_lread'
    // conses MODULES_SUFFIX and MODULES_SECONDARY_SUFFIX onto ".elc"
    // (src/lread.c:5623-5629), so the list carries one more entry than on
    // GNU/Linux.  The local GNU is the oracle for which suffixes this host's
    // editor loads; the Linux list is not a portable literal.
    crate::common::assert_oracle_parity(
        r##"
(list (> (length load-path) 5)
      load-suffixes
      load-file-name
      (stringp source-directory))
"##,
    );
}

#[test]
fn div_g0_locale_and_coding_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"UTF-8\" utf-8-unix utf-8-unix utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (locale-info 'codeset)
      coding-system-for-read
      coding-system-for-write
      locale-coding-system)
"##,
        expect,
    );
}

#[test]
fn div_g0_emacs_state_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // `system-type' is one of the host facts GNU itself reports differently
    // per platform (`gnu/linux' vs `darwin'); the shapes around it are
    // stable.  Derive all four values from the local GNU at run time.
    crate::common::assert_oracle_parity(
        r##"
(list (integerp (emacs-pid))
      system-type
      (vectorp (memory-use-counts))
      (length (memory-use-counts)))
"##,
    );
}

#[test]
fn div_g0_system_configuration_triple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU records the configure-time `config.guess` triple.  On GNU/Linux
    // that is the canonical system triple (`x86_64-pc-linux-gnu`); on Darwin
    // config.guess appends the build host's kernel release
    // (`aarch64-apple-darwin$UNAME_RELEASE`, build-aux/config.guess:1457), so
    // the raw string is a host input, not a Neo constant.  Pin the GNU side at
    // the reference seam (the build manifest records the value the oracle was
    // configured with, and a live run must still see it), then compare Neo
    // against that same GNU: the local GNU's own triple is the expectation.
    if crate::common::oracle_mode_runs_live_gnu()
        && let Some(recorded) = crate::common::reference_manifest_value("system_configuration")
    {
        let live = crate::common::run_oracle_eval("system-configuration")
            .expect("the resolved GNU answers system-configuration");
        // `run_oracle_eval` returns the legacy snapshot transcript: stdout
        // followed by the encoded outcome, i.e. `OK "<printed value>"`.
        assert_eq!(
            live.trim(),
            format!("OK \"{recorded}\""),
            "the reference manifest records system-configuration {recorded:?}, \
             but the resolved GNU reports {}",
            live.trim()
        );
    }
    crate::common::assert_oracle_parity(
        r##"
system-configuration
"##,
    );
}

#[test]
fn div_g0_environment_exported_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    // getenv presence checks only (NOT process-environment ordering/content,
    // which trivially differs between the two processes and would dump the
    // whole env).
    crate::common::assert_oracle_parity_expect(
        r##"
(list (stringp (getenv "SHELL"))
      (stringp (getenv "USER"))
      (stringp (getenv "TERM"))
      (eq (getenv "NEO_PROBE_UNSET_XYZ") nil))
"##,
        expect,
    );
}
