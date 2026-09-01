use expect_test::{Expect, expect};

use super::zeal_at_point_oracle_for_load_profile;

const LOAD_STATE_PROBE: &str = r##"
(let ((elisp-docset
       (cdr (assq 'emacs-lisp-mode zeal-at-point-mode-alist))))
  (list :version zeal-at-point-zeal-version
        :emacs-lisp-docset elisp-docset
        :known (and (member elisp-docset zeal-at-point-docsets) t)
        :feature (featurep 'zeal-at-point)))
"##;

fn assert_load_profile(name: &str, script: Option<&str>, expected: Expect) {
    let report = zeal_at_point_oracle_for_load_profile(script)
        .run_value(name, LOAD_STATE_PROBE)
        .unwrap_or_else(|error| panic!("zeal-at-point load profile `{name}` failed: {error}"));
    expected.assert_eq(&report.gnu_emacs.to_string());
    expected.assert_eq(&report.neomacs.to_string());
}

#[test]
fn pre_0_3_version_output_selects_the_legacy_emacs_lisp_docset() {
    let expected =
        expect![[r##"OK (:version "0.2.9" :emacs-lisp-docset "emacs lisp" :known t :feature t)"##]];
    assert_load_profile(
        "zeal_at_point_pre_0_3_load",
        Some("#!/bin/sh\nprintf '%s\\n' 'Zeal 0.2.9'\n"),
        expected,
    );
}

#[test]
fn malformed_version_output_leaves_the_version_unknown() {
    let expected =
        expect![[r##"OK (:version nil :emacs-lisp-docset "elisp" :known t :feature t)"##]];
    assert_load_profile(
        "zeal_at_point_malformed_version_load",
        Some("#!/bin/sh\nprintf '%s\\n' 'unexpected version banner'\n"),
        expected,
    );
}

#[test]
fn absent_executable_leaves_the_version_unknown() {
    let expected =
        expect![[r##"OK (:version nil :emacs-lisp-docset "elisp" :known t :feature t)"##]];
    assert_load_profile("zeal_at_point_absent_executable_load", None, expected);
}
