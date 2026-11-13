//! Issue #590: every printer traversal must be bounded, either by
//! `print-circle` labels or by the print-depth guard. GNU Emacs 31.1
//! (oracle a360712c) is the reference for every assertion below.
//!
//! Both walkways into the printer are pinned on purpose:
//! - no `print-number-table`: preprocessing fills the private
//!   `PrintCircleState`, so `state.circle` is `Some`;
//! - a caller-supplied eq `print-number-table`: preprocessing fills the Lisp
//!   table and `state.circle` is `None`, so only the table entries can bound
//!   the traversal. That is the configuration the report suspected, and a
//!   missing entry there doubles the output at every sharing level.
use crate::test_utils::runtime_startup_context;

fn eval(context: &mut crate::emacs_core::Context, source: &str) -> neovm_value::value::Value {
    context.eval_str(source).expect("evaluation must succeed")
}

fn eval_bytes(context: &mut crate::emacs_core::Context, source: &str) -> Vec<u8> {
    eval(context, source)
        .as_lisp_string()
        .expect("string result")
        .as_bytes()
        .to_vec()
}

/// GNU checks `print_depth >= 200` only on the non-circle path
/// (print.c:2285-2289), so a deep acyclic list still prints under
/// `print-circle`. 150 levels render 303 bytes and 300 levels 603 bytes.
#[test]
fn deep_nesting_under_print_circle_prints_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    for levels in [150i64, 300] {
        let value = eval(
            &mut context,
            &format!(
                "(let ((print-circle t) (x nil)) (dotimes (_ {levels}) (setq x (list x))) \
                 (length (prin1-to-string x)))"
            ),
        );
        let length = value.as_fixnum().expect("fixnum length");
        assert_eq!(length, levels * 2 + 3, "depth {levels}");
    }
}

/// Without `print-circle` the same list must signal instead of exhausting the
/// Rust stack.
#[test]
fn deep_nesting_without_print_circle_signals_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    assert_eq!(
        eval_bytes(
            &mut context,
            "(let ((print-circle nil) (x nil)) (dotimes (_ 300) (setq x (list x))) \
             (condition-case e (prin1-to-string x) (error (error-message-string e))))"
        ),
        b"Apparently circular structure being printed".to_vec()
    );
}

/// A shared (DAG) structure must be labelled once and referenced afterwards on
/// both bookkeeping paths. Without labels the output doubles per level, which
/// is the shape that can drive the printer into gigabytes.
#[test]
fn shared_dag_is_labelled_on_both_circle_bookkeeping_paths() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    for levels in [10i64, 16, 18] {
        let length_form = |table: &str| {
            format!(
                "(let ((print-circle t) {table} (x (list 1))) \
                   (dotimes (_ {levels}) (setq x (list x x))) (length (prin1-to-string x)))"
            )
        };
        let internal = eval(&mut context, &length_form(""));
        let external = eval(
            &mut context,
            &length_form("(print-number-table (make-hash-table :test 'eq))"),
        );
        assert_eq!(internal, external, "depth {levels}: table path diverged");
        let length = internal.as_fixnum().expect("fixnum length");
        assert!(
            length < 20 * levels + 100,
            "depth {levels}: {length} bytes means sharing was not labelled"
        );
    }
}

/// A genuine cycle must become a `#N#` reference on both paths rather than
/// recurse. GNU prints `#1=(1 2 . #1#)`.
#[test]
fn cycle_bounded_on_both_circle_bookkeeping_paths() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    let form = |table: &str| {
        format!(
            "(let ((print-circle t) {table} (l (list 1 2))) \
               (setcdr (cdr l) l) (prin1-to-string l))"
        )
    };
    for table in ["", "(print-number-table (make-hash-table :test 'eq))"] {
        assert_eq!(
            eval_bytes(&mut context, &form(table)),
            b"#1=(1 2 . #1#)".to_vec(),
            "table binding: {table:?}"
        );
    }
}

/// Each container kind that participates in circle detection must earn a label
/// through the Lisp eq table (that lookup runs through `object_identity_key`).
/// A missing key silently disables the label and reopens exponential
/// expansion; these are the exact GNU renderings.
#[test]
fn repeated_values_are_labelled_per_container_kind_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    let cases: [(&str, &[u8]); 5] = [
        ("\"abc\"", b"(#1=\"abc\" #1#)"),
        (
            "(propertize \"abc\" 'p 1)",
            b"(#1=#(\"abc\" 0 3 (p 1)) #1#)",
        ),
        ("(vector 1 2)", b"(#1=[1 2] #1#)"),
        ("(record 'foo 1 2)", b"(#1=#s(foo 1 2) #1#)"),
        (
            "(let ((h (make-hash-table))) (puthash 'a 1 h) h)",
            b"(#1=#s(hash-table data (a 1)) #1#)",
        ),
    ];
    for (make, expected) in cases {
        let form = format!(
            "(let ((print-circle t) (print-number-table (make-hash-table :test 'eq))) \
               (let ((v {make})) (prin1-to-string (list v v))))"
        );
        assert_eq!(
            eval_bytes(&mut context, &form),
            expected.to_vec(),
            "case {make}"
        );
    }
}

/// GNU `print_circle_candidate_p` (print.c:1329-1349) omits obarrays: they are
/// `PVEC_OBARRAY`, not `VECTORP`, and are absent from the vectorlike list. A
/// repeated obarray therefore prints in full, with no `#N=` label.
#[test]
fn repeated_obarray_is_not_labelled_like_gnu() {
    crate::test_utils::init_test_tracing();
    let obarray = neovm_value::value::Value::obarray(5);
    assert_eq!(
        crate::emacs_core::print::print_value_with_options(
            &neovm_value::value::Value::list(vec![obarray, obarray]),
            crate::emacs_core::print::PrintOptions::new(false, true, None, None),
        ),
        "(#<obarray n=0> #<obarray n=0>)"
    );
}

/// The same predicate lists `FONTP`, so a repeated font object is a circle
/// candidate and must be labelled once and referenced afterwards.
#[test]
fn repeated_font_object_is_labelled_like_gnu() {
    use neovm_display_identity::font::{FontBackendKind, ResolvedFontIdentity};
    use neovm_value::tagged::header::{FontObjectData, FontObjectMetrics};
    use neovm_value::value::Value;

    crate::test_utils::init_test_tracing();
    let font = Value::make_font(FontObjectData {
        fields: vec![].into(),
        metrics: FontObjectMetrics {
            pixel_size: 16,
            height: 19,
            max_width: 9,
            ascent: 14,
            descent: 5,
            space_width: 8,
            average_width: 8,
        },
        capability: Value::NIL,
        identity: ResolvedFontIdentity::from_memory(
            FontBackendKind::Fontconfig,
            "test:font".to_string(),
            0,
            None,
        ),
    });
    assert_eq!(
        crate::emacs_core::print::print_value_with_options(
            &Value::list(vec![font, font]),
            crate::emacs_core::print::PrintOptions::new(false, true, None, None),
        ),
        "(#1=#<font-object> #1#)"
    );
}
