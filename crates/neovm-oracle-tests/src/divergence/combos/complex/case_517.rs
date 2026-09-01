/// Batch 517: elisp internals bootstrap, load history, byte-code, native-comp.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx517_bootstrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'bootstrap-version) (featurep 'bootstrap))
"##,
        expect,
    );
}

#[test]
fn div_cx517_load_history_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((lh load-history))
  (list (listp lh) (> (length lh) 0) (consp (car lh))))
"##,
        expect,
    );
}

#[test]
fn div_cx517_load_suffixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"\" \".gz\") (\".so\" \".elc\" \".el\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list load-file-rep-suffixes load-suffixes)
"##,
        expect,
    );
}

#[test]
fn div_cx517_source_etc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'source-directory) (stringp source-directory))
"##,
        expect,
    );
}

#[test]
fn div_cx517_byte_compiler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'byte-compile) (fboundp 'byte-optimize-form))
"##,
        expect,
    );
}

#[test]
fn div_cx517_byte_code_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (byte-code-function t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (byte-compile (lambda (x) (* x 2)))))
  (list (type-of f) (byte-code-function-p f)))
"##,
        expect,
    );
}

#[test]
fn div_cx517_compiled_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'compiled-function-p)
      (fboundp 'interpreted-function-p))
"##,
        expect,
    );
}

#[test]
fn div_cx517_native_comp_avail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (native-comp-available-p)
      (fboundp 'native-comp-unit-file))
"##,
        expect,
    );
}

#[test]
fn div_cx517_pure_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable pure-space-used)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'pure-space-used) (numberp pure-space-used))
"##,
        expect,
    );
}

#[test]
fn div_cx517_garbage_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((g (garbage-collect)))
  (list (listp g) (> (length g) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx517_memory_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'memory-full))
"##,
        expect,
    );
}

#[test]
fn div_cx517_emacs_lisp_native() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'emacs-lisp-native-compile)
      (fboundp 'emacs-lisp-compilation-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx517_disassemble_bc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (byte-compile (lambda () 42))))
  (with-temp-buffer
    (disassemble f (current-buffer))
    (> (buffer-size) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx517_byte_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(fboundp 'message)
"##,
        expect,
    );
}

#[test]
fn div_cx517_message_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'format-spec)
  (list (fboundp 'format-spec) (fboundp 'format-spec-make)))
"##,
        expect,
    );
}
