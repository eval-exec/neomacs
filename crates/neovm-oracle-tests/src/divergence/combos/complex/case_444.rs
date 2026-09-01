//! Complex combo batch 444 — 15 probes into untouched areas: widget-create,
//! custom-set-variables, checkdoc, elint, disassemble deeper, byte-opt,
//! benchmar-elapse, ewoc, setenv deep, substitute-in-file-name,
//! file-name-handler-alist, read-file-name in batch, minibuffer-depth,
//! tq-create/tq-enqueue, printenv.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// widget-create: basic widget creation.
#[test]
fn div_cx444_widget_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'wid-edit)
  (with-temp-buffer
    (let ((w (widget-create 'editable-field "hello")))
      (widget-value w))))
"##,
        expect,
    );
}

/// custom-set-variables: setting customization options.
#[test]
fn div_cx444_custom_set_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"customized\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (defcustom neo-cx444-opt "default" "test" :type 'string)
  (custom-set-variables '(neo-cx444-opt "customized"))
  neo-cx444-opt)
"##,
        expect,
    );
}

/// setenv / getenv deep with multibyte values.
#[test]
fn div_cx444_setenv_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"multibyte-世界\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((process-environment process-environment))
  (setenv "NEO_CX444" "multibyte-世界")
  (getenv "NEO_CX444"))
"##,
        expect,
    );
}

/// substitute-in-file-name: tilde and env var expansion.
#[test]
fn div_cx444_substitute_in_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[ORACLE-HOME]/test\" \"~\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (substitute-in-file-name "$HOME/test")
      (substitute-in-file-name "~"))
"##,
        expect,
    );
}

/// find-file-name-handler: handler detection.
#[test]
fn div_cx444_find_file_name_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil tramp-autoload-file-name-handler)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (find-file-name-handler "/tmp/test.el" 'file-exists-p)
      (find-file-name-handler "/ssh:host:file" 'file-exists-p))
"##,
        expect,
    );
}

/// read-file-name in batch mode.
#[test]
fn div_cx444_read_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-file-name "test: " "/tmp" nil t "default")
  (error (car e)))
"##,
    );
}

/// minibuffer-depth / minibuffer-depth-indicate-mode.
#[test]
fn div_cx444_minibuffer_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (minibuffer-depth)
      (fboundp 'minibuffer-depth-indicate-mode))
"##,
        expect,
    );
}

/// ewoc-create / ewoc-enter-first / ewoc-enter-last.
#[test]
fn div_cx444_ewoc_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ewoc)
  (with-temp-buffer
    (let ((ewoc (ewoc-create 'identity))))
      (fboundp 'ewoc-enter-first)))
"##,
        expect,
    );
}

/// tq-create / tq-enqueue: task queue.
#[test]
fn div_cx444_tq_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'tq)
  (list (fboundp 'tq-create) (fboundp 'tq-enqueue)))
"##,
        expect,
    );
}

/// printenv: printing environment.
#[test]
fn div_cx444_printenv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[ORACLE-HOME]\" \"exec\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (getenv "HOME") (getenv "USER"))
"##,
        expect,
    );
}

/// format-seconds with sub-second precision.
#[test]
fn div_cx444_format_seconds_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1:1:1\" \"1:1:1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-seconds "%h:%m:%s" 3661.5)
      (format-seconds "%h:%m:%s" 3661))
"##,
        expect,
    );
}

/// split-string with multibyte and field separators.
#[test]
fn div_cx444_split_string_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"one\" \"two\" \"three\") (\"a\" \"\" \"b\" \"c\") (\"αβγ\" \"δε\" \"ζ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (split-string "one  two   three" " +")
      (split-string "a,,b,c" ",")
      (split-string "αβγ||δε||ζ" "||"))
"##,
        expect,
    );
}

/// string-to-char / char-to-string edge.
#[test]
fn div_cx444_string_char_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 0 \"a\" \"世\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-to-char "abc")
      (string-to-char "")
      (char-to-string ?a)
      (char-to-string ?世))
"##,
        expect,
    );
}

/// user-initials / user-variant / user-emacs-directory.
#[test]
fn div_cx444_user_info_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function user-initials)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (user-initials)
      (user-real-login-name)
      (user-login-name)
      (user-full-name))
"##,
        expect,
    );
}

/// decode-time / encode-time with decoded-time structure.
#[test]
fn div_cx444_decode_encode_time_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2024 integer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (decoded-time-year (decode-time (encode-time 0 0 0 1 1 2024 nil)))))
  (list dt (type-of dt)))
"##,
        expect,
    );
}
