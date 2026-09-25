use super::perf_map::{LabelTier, anon_name, format_label, label_name, sanitize};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::value::{LambdaParams, Value};

#[test]
fn jit_perf_map_label_format() {
    assert_eq!(format_label("foo", 12, LabelTier::Mir), "lisp:foo#12:mir");
    assert_eq!(
        format_label("foo", 12, LabelTier::Baseline),
        "lisp:foo#12:baseline"
    );
    assert_eq!(
        format_label("foo", 12, LabelTier::Osr(7)),
        "lisp:foo#12:osr@7"
    );
    assert_eq!(label_name("lisp:foo#12:mir"), Some("foo"));
    assert_eq!(label_name("lisp:a#b#3:osr@7"), Some("a#b"), "last # wins");
    assert_eq!(label_name("__neovm_jit_leaf"), None);
}

/// A perf-map line is `start size name\n`: a Lisp name may not break it.
#[test]
fn jit_perf_map_sanitize_whitespace_and_length() {
    assert_eq!(&*sanitize("foo bar\tbaz\nq"), "foo_bar_baz_q");
    assert_eq!(&*sanitize("a\u{7}b"), "a_b");
    assert_eq!(&*sanitize("λ-ok"), "λ-ok");
    let long = "é".repeat(100); // 200 bytes
    let cut = sanitize(&long);
    assert!(cut.len() <= 128);
    assert_eq!(cut.len(), 128, "64 two-byte chars fit exactly");
    let odd = format!("x{}", "é".repeat(100)); // boundary falls mid-char
    let cut = sanitize(&odd);
    assert_eq!(cut.len(), 127, "never cut inside a char");
    assert!(cut.is_char_boundary(cut.len()));
}

#[test]
fn jit_perf_map_anon_fingerprint() {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.constants = vec![
        Value::make_int(1),
        Value::symbol("car"),
        Value::symbol("jit-anon-helper"),
        Value::symbol("cdr"),
    ]
    .into();
    assert_eq!(&*anon_name(&f), "anon[car;jit-anon-helper]");
    f.constants = vec![Value::make_int(1)].into();
    assert_eq!(&*anon_name(&f), "anon");
}
