//! Oracle parity tests for GNU `get-load-suffixes` semantics.
//!
//! GNU implements `get-load-suffixes` in `src/lread.c`.  With module support,
//! compressed representations listed in `jka-compr-load-suffixes` are skipped
//! for module suffixes, while still being tried for `.elc` and `.el`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
use std::path::PathBuf;

fn load_suffix_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/load-suffixes")
}

#[test]
fn oracle_get_load_suffixes_skips_compressed_module_representations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-suffixes (list module-file-suffix ".elc" ".el"))
      (load-file-rep-suffixes '("" ".gz" ".br"))
      (jka-compr-load-suffixes '(".gz")))
  (get-load-suffixes))
"#;

    let expect = expect_test::expect![[
        r#""OK (\".so\" \".so.br\" \".elc\" \".elc.gz\" \".elc.br\" \".el\" \".el.gz\" \".el.br\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_require_honors_live_load_suffix_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-suffixes '(".preferred" ".fallback"))
      (load-file-rep-suffixes '("")))
  (require 'neovm--oracle-live-require-suffix)
  neovm--oracle-live-require-suffix-result)
"#;

    let expect = expect_test::expect![r#""OK preferred""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &load_suffix_fixture_root(),
        expect,
    );
}
