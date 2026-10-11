//! U2.8: [`Context::builtin_var_value`] answers exactly what the general
//! reader answers, for every variable shape a builtin's control variable can
//! take, with the front-end knob on and off; and the knob parses as
//! documented.
use crate::emacs_core::eval::{parse_builtin_frontend_knob, set_builtin_frontend_for_test};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;

const SHAPES: &[&str] = &[
    "bv-plain",
    "bv-void",
    "bv-local",
    "bv-made-local",
    "bv-alias",
    "case-fold-search",
    "inhibit-changing-match-data",
    "parse-sexp-lookup-properties",
    "syntax-propertize--done",
    "fill-column",
    "indent-tabs-mode",
    "char-script-table",
    "default-text-properties",
];

const SETUP: &str = r#"
(progn
  (defvar bv-plain 1)
  (defvar bv-void)
  (defvar-local bv-local 'default)
  (defvar bv-made-local 'global)
  (defvaralias 'bv-alias 'bv-plain)
  (set-buffer (get-buffer-create " bv-home"))
  (setq bv-local 'local)
  (make-local-variable 'bv-made-local)
  (setq bv-made-local 'local)
  (setq-local case-fold-search nil)
  (setq-local parse-sexp-lookup-properties t)
  (setq-local syntax-propertize--done 42)
  (setq fill-column 33)
  t)
"#;

fn read_all(
    eval: &crate::emacs_core::eval::Context,
) -> Vec<(String, Option<String>, Option<String>)> {
    SHAPES
        .iter()
        .map(|name| {
            let id = intern(name);
            let typed = eval.builtin_var_value(id).map(|v| print_value(&v));
            let general = eval
                .special_variable_value_by_id(id)
                .map(|v| print_value(&v));
            (name.to_string(), typed, general)
        })
        .collect()
}

#[test]
fn typed_reads_answer_like_the_general_reader() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(SETUP).expect("setup evaluates");
    for buffer in [" bv-home", " bv-other"] {
        eval.eval_str(&format!(r#"(set-buffer (get-buffer-create "{buffer}"))"#))
            .expect("switch buffer");
        for frontend in [true, false] {
            set_builtin_frontend_for_test(Some(frontend));
            for (name, typed, general) in read_all(&eval) {
                assert_eq!(typed, general, "{name} in {buffer}, frontend {frontend}");
            }
        }
    }
    set_builtin_frontend_for_test(None);
    // The shapes are what they claim: a local binding here, the default
    // there, a void variable void.
    eval.eval_str(r#"(set-buffer " bv-home")"#).expect("home");
    let home = read_all(&eval);
    eval.eval_str(r#"(set-buffer " bv-other")"#).expect("other");
    let other = read_all(&eval);
    let value = |rows: &[(String, Option<String>, Option<String>)], name: &str| {
        rows.iter()
            .find(|row| row.0 == name)
            .and_then(|row| row.1.clone())
    };
    assert_eq!(value(&home, "bv-local").as_deref(), Some("local"));
    assert_eq!(value(&other, "bv-local").as_deref(), Some("default"));
    assert_eq!(value(&home, "bv-made-local").as_deref(), Some("local"));
    assert_eq!(value(&other, "bv-made-local").as_deref(), Some("global"));
    assert_eq!(value(&home, "case-fold-search").as_deref(), Some("nil"));
    assert_eq!(value(&other, "case-fold-search").as_deref(), Some("t"));
    assert_eq!(
        value(&home, "syntax-propertize--done").as_deref(),
        Some("42")
    );
    assert_eq!(value(&home, "bv-void"), None);
    assert_eq!(value(&home, "bv-alias").as_deref(), Some("1"));
}

#[test]
fn a_dynamic_binding_is_what_the_typed_read_sees() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    // `looking-at` reads `case-fold-search` and `inhibit-changing-match-data`
    // through the typed read; a `let` of either must be what it sees.
    let result = eval
        .eval_str(
            r#"(with-temp-buffer
                 (insert "ABC")
                 (goto-char 1)
                 (list (let ((case-fold-search t)) (looking-at "abc"))
                       (let ((case-fold-search nil)) (looking-at "abc"))
                       (progn (set-match-data (list 7 7))
                              (let ((inhibit-changing-match-data t)) (looking-at "AB"))
                              (match-data))))"#,
        )
        .expect("looking-at evaluates");
    assert_eq!(print_value(&result), "(t nil (7 7))");
}

#[test]
fn the_frontend_knob_parses_as_documented() {
    for (value, on) in [
        (None, true),
        (Some(""), true),
        (Some("1"), true),
        (Some("on"), true),
        (Some("ALL"), true),
        (Some("0"), false),
        (Some("off"), false),
        (Some("none"), false),
        (Some("bogus"), true),
    ] {
        assert_eq!(parse_builtin_frontend_knob(value), on, "{value:?}");
    }
}
