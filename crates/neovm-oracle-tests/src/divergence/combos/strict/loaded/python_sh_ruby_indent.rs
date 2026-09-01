//! Strict combo oracle probes, batch 49: language-mode indentation engines
//! via assert_oracle_parity_with_load — python.el (python-mode indent),
//! sh-script.el (sh-mode indent), and ruby-mode.el (ruby-mode indent).
//! cc-mode (C) was already at parity in batch 48.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i6_python_mode_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"def foo():\\nif x:\\nreturn 1\\nprint(x)\\nfor i in r:\\npass\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (python-mode)
  (insert "def foo():\nif x:\nreturn 1\nprint(x)\nfor i in r:\npass\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        &["progmodes/python.el"],
        expect,
    );
}

#[test]
fn div_i6_sh_mode_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"if [ $x ]; then\\n    echo hi\\n    for i in 1 2; do\\n\techo $i\\n    done\\nfi\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (sh-mode)
  (insert "if [ $x ]; then\necho hi\nfor i in 1 2; do\necho $i\ndone\nfi\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        &["progmodes/sh-script.el"],
        expect,
    );
}

#[test]
fn div_i6_ruby_mode_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"def foo\\n  if x\\n    return 1\\n  end\\nend\\n\"""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (ruby-mode)
  (insert "def foo\nif x\nreturn 1\nend\nend\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        &["progmodes/ruby-mode.el"],
        expect,
    );
}

#[test]
fn div_i6_python_nested_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"class C:\\ndef m(self):\\nif self.x:\\nreturn [i\\nfor i in self.y\\nif i > 0]\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (python-mode)
  (insert "class C:\ndef m(self):\nif self.x:\nreturn [i\nfor i in self.y\nif i > 0]\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        &["progmodes/python.el"],
        expect,
    );
}
