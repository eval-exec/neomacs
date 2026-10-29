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
    let expect = expect_test::expect![[r#""OK (t (\".so\" \".elc\" \".el\") nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (> (length load-path) 5)
      load-suffixes
      load-file-name
      (stringp source-directory))
"##,
        expect,
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
    let expect = expect_test::expect![[r#""OK (t gnu/linux nil 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (integerp (emacs-pid))
      system-type
      (vectorp (memory-use-counts))
      (length (memory-use-counts)))
"##,
        expect,
    );
}

#[test]
fn div_g0_system_configuration_triple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x86_64-pc-linux-gnu\"""#]];
    // GNU records the configure-time `config.guess` triple.  On GNU/Linux
    // that is the canonical system triple; on Darwin config.guess appends
    // the build host's kernel release
    // (`aarch64-apple-darwin$UNAME_RELEASE`, build-aux/config.guess:1457),
    // so the raw string is a host input, not a Neo constant.  Pin it at the
    // reference seam: the build manifest records the value the oracle was
    // configured with, and a live run must still see that exact value.  The
    // parity assertion below then compares Neo against a GNU whose host
    // input is proven, not against one that has drifted from its recipe.
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
    crate::common::assert_oracle_parity_expect(
        r##"
system-configuration
"##,
        expect,
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
