//! U2.8: `skip-syntax-forward`/`-backward` parse their class string from its
//! bytes (GNU `skip_syntaxes`) instead of decoding it first; both parses
//! must name the same classes, and the builtins must answer alike either
//! way.
use super::*;
use crate::emacs_core::eval::set_builtin_frontend_for_test;

#[test]
fn byte_and_decoded_class_parses_agree() {
    let unibyte = crate::heap_types::LispString::from_unibyte(vec![b'w', 0xA0, b'_', 0xFF]);
    let multibyte = crate::heap_types::LispString::from_utf8("w\u{e9}_\u{6f22}^\"");
    let mut strings: Vec<Vec<u8>> = [
        "",
        "^",
        "w",
        "^w",
        "w_",
        "^w_",
        " -",
        "^ -.",
        "()'\"$\\/<>@!|",
        "^^w",
        "xyz",
        "w^",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    strings.push(unibyte.as_bytes().to_vec());
    strings.push(multibyte.as_bytes().to_vec());
    for bytes in strings {
        let decoded = crate::emacs_core::emacs_char::to_utf8_lossy(&bytes);
        assert_eq!(
            SkipSyntaxClasses::parse_spec_bytes(&bytes),
            SkipSyntaxClasses::parse_str(&decoded),
            "{bytes:?}"
        );
    }
}

#[test]
fn skip_syntax_answers_alike_with_the_knob_on_and_off() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let ((out nil))
  (dolist (mode '(fundamental-mode emacs-lisp-mode))
    (with-temp-buffer
      (insert "  foo_bar (baz) \"q\" ; c\nÀé 漢字-x ?\\( 'y")
      (funcall mode)
      (dolist (spec '("w" "^w" "w_" " " "^ " "-" "_w" "()" "^()" "\"" "." "<>" "" "^"
                      "wé" "@"))
        (dolist (pos '(1 3 8 12 20 25 30 40))
          (dolist (limit '(nil 1 15 35 100))
            (dolist (fn '(skip-syntax-forward skip-syntax-backward))
              (goto-char (min pos (point-max)))
              (push (list mode spec pos limit fn
                          (condition-case err (funcall fn spec limit)
                            (error (list 'signal err)))
                          (point))
                    out)))))))
  (nreverse out))
"#;
    let run = |frontend: bool| {
        set_builtin_frontend_for_test(Some(frontend));
        let mut eval = crate::test_utils::runtime_startup_context();
        let result = eval.eval_str(form).expect("the skip matrix evaluates");
        set_builtin_frontend_for_test(None);
        crate::emacs_core::print::print_value(&result)
    };
    assert_eq!(run(true), run(false));
}
