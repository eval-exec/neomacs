//! P4.1 Stage 0 pins: the `cconv-make-interpreted-closure` hook's knob,
//! statistics and effect snapshot.

use crate::emacs_core::eval::{CconvMemoEvent, CconvMemoMode, Context, parse_cconv_memo_knob};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

/// A runtime-startup context with the memo in MODE and the native
/// untrimmed path off, whatever `NEOVM_CCONV_MEMO` / `NEOVM_CCONV_FAST` say
/// (the whole suite also runs under those knobs as a soak).
fn startup(mode: CconvMemoMode) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.cconv_memo.set_mode(mode);
    eval.cconv_memo.set_fast(false);
    // Built afresh at the test's first use, not during startup.
    eval.cconv_memo.reset_trusted_for_test();
    eval
}

fn eval_ok(eval: &mut Context, src: &str) -> Value {
    eval.eval_str(src)
        .unwrap_or_else(|err| panic!("{src}: {err:?}"))
}

fn printed(eval: &mut Context, src: &str) -> String {
    let value = eval_ok(eval, src);
    print_value(&value)
}

fn count(eval: &Context, event: CconvMemoEvent) -> u64 {
    eval.cconv_memo.stats().count(event)
}

#[test]
fn knob_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_cconv_memo_knob(None), CconvMemoMode::Off);
    for off in ["", "0", "off", "OFF", "no", "false"] {
        assert_eq!(
            parse_cconv_memo_knob(Some(off)),
            CconvMemoMode::Off,
            "{off}"
        );
    }
    assert_eq!(parse_cconv_memo_knob(Some("stats")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some(" Stats ")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some("bogus")), CconvMemoMode::Off);
}

/// The filter is the dumped `cconv-make-interpreted-closure` in a GNU `-Q`
/// equivalent image, so the hook sees every lexical closure creation.
#[test]
fn stats_classify_trimming_and_untrimmed_creations() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    // An environment with a lexical variable: the trimming path, identity.
    eval_ok(&mut eval, "(let ((cm-x 1)) (lambda () cm-x))");
    assert_eq!(count(&eval, CconvMemoEvent::Call), 1);
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 1);
    assert_eq!(count(&eval, CconvMemoEvent::RunIdentity), 1);
    assert_eq!(count(&eval, CconvMemoEvent::RunEffect), 0);

    // `(t)` only: no lexical variable, cconv.el returns the closure as is.
    eval_ok(&mut eval, "(lambda () 1)");
    assert_eq!(count(&eval, CconvMemoEvent::Call), 2);
    assert_eq!(count(&eval, CconvMemoEvent::NoLexvars), 1);
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 1);

    // A macro in the body is expanded: the body is rewritten.
    eval_ok(&mut eval, "(let ((cm-y 2)) (lambda () (when cm-y 3)))");
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 2);
    assert_eq!(count(&eval, CconvMemoEvent::RunRewritten), 1);

    // GNU's quirk (cconv.el:647-669): an uninitialized let variable that is
    // read makes the analysis call the unloaded `byte-compile-warn-x`.
    let err = eval.eval_str("(let ((cm-z 1)) (lambda () (let ((cm-w)) (list cm-w cm-z))))");
    assert!(err.is_err(), "{err:?}");
    assert_eq!(count(&eval, CconvMemoEvent::RunError), 1);

    let report = eval.cconv_memo_report();
    assert!(report.contains(" trim=3 "), "{report}");
}

#[test]
fn stats_mode_builds_the_same_closures_as_off() {
    crate::test_utils::init_test_tracing();
    let forms = [
        "(let ((a 1) (b 2)) (lambda (c) (list a c)))",
        "(let ((a 1)) (defvar cm-dyn) (let ((cm-dyn 2)) (lambda () (list a cm-dyn))))",
        "(let ((a 1) (b 2)) (lambda () 'nothing))",
        "(let ((a 1)) (lambda (x) \"doc\" (+ a x)))",
    ];
    let mut off = startup(CconvMemoMode::Off);
    let mut stats = startup(CconvMemoMode::Stats);
    for form in forms {
        assert_eq!(printed(&mut off, form), printed(&mut stats, form), "{form}");
    }
    assert_eq!(count(&off, CconvMemoEvent::Call), 0);
    assert_eq!(count(&stats, CconvMemoEvent::Trim), forms.len() as u64);
}

#[test]
fn effect_snapshot_sees_what_a_run_can_change() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let quiet = eval.cconv_effect_snapshot();
    eval_ok(&mut eval, "(list 1 2 (car '(3)))");
    assert_eq!(
        eval.cconv_effect_snapshot(),
        quiet,
        "a pure run moves nothing"
    );

    for (what, src) in [
        ("message", "(message \"cm effect\")"),
        ("buffer creation", "(generate-new-buffer \"cm-effect\")"),
        (
            "buffer text",
            "(with-current-buffer \"*scratch*\" (insert \"x\"))",
        ),
        ("gensym", "(gensym)"),
        ("intern", "(intern \"cm-effect-fresh-symbol-1\")"),
        ("fset", "(defalias 'cm-effect-fn #'car)"),
        ("match data", "(string-match \"b\" \"abc\")"),
    ] {
        let before = eval.cconv_effect_snapshot();
        eval_ok(&mut eval, src);
        assert_ne!(eval.cconv_effect_snapshot(), before, "{what}");
    }

    let before = eval.cconv_effect_snapshot();
    eval.note_load_effect();
    assert_ne!(eval.cconv_effect_snapshot(), before, "load");
}

// ---------------------------------------------------------------------------
// S0.3: closure shape, facts, environment summary, head verdicts
// ---------------------------------------------------------------------------

use crate::emacs_core::eval::{
    ClosureFacts, ClosureShape, EnvSummary, FactsRefusal, HeadVerdict, SHAPE_NODE_CAP,
    ShapeRefusal, ShapeTok,
};
use crate::emacs_core::intern::intern;

/// `(ARGS . BODY)` of a quoted lambda: `(lambda ARGS . BODY)`.  The lambda
/// is kept on `cm-test-roots`, so it stays live across later evaluations
/// (the collector does not see Rust locals; this runs under GC stress).
fn lambda_parts(eval: &mut Context, quoted_lambda: &str) -> (Value, Value) {
    let lambda = eval_ok(
        eval,
        &format!(
            "(car (setq cm-test-roots
                        (cons (quote {quoted_lambda})
                              (and (boundp 'cm-test-roots) cm-test-roots))))"
        ),
    );
    let rest = lambda.cons_cdr();
    (rest.cons_car(), rest.cons_cdr())
}

fn shape_of(eval: &mut Context, quoted_lambda: &str) -> ClosureShape {
    let (args, body) = lambda_parts(eval, quoted_lambda);
    ClosureShape::of(args, body).unwrap_or_else(|why| panic!("{quoted_lambda}: {why:?}"))
}

#[test]
fn shape_is_structure_symbols_and_atom_types() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let a = shape_of(&mut eval, "(lambda (x) (list x \"one\" 1.5 [1 2] 7))");
    let same = shape_of(&mut eval, "(lambda (x) (list x \"two\" 2.5 [3] 7))");
    assert_eq!(a, same, "strings, floats and vectors are compared by type");
    for different in [
        "(lambda (y) (list y \"one\" 1.5 [1 2] 7))",
        "(lambda (x) (list x \"one\" 1.5 [1 2] 8))",
        "(lambda (x) (list x one 1.5 [1 2] 7))",
        "(lambda (x) (list x \"one\" 1.5 [1 2] 7) nil)",
        "(lambda (x) (list x \"one\" 1.5 (1 2) 7))",
        "(lambda (x . y) (list x \"one\" 1.5 [1 2] 7))",
    ] {
        assert_ne!(a, shape_of(&mut eval, different), "{different}");
    }
    assert!(!a.mentions_interactive);
    assert!(shape_of(&mut eval, "(lambda () (interactive) 1)").mentions_interactive);
    assert!(shape_of(&mut eval, "(lambda () '(interactive))").mentions_interactive);
    assert_eq!(a.toks[0], ShapeTok::Cons, "ARGS come first");
    assert_eq!(shape_of(&mut eval, "(lambda () 1)").toks[0], ShapeTok::Nil);
    assert_eq!(
        shape_of(&mut eval, "(lambda () (a (b (c (d)))))").car_depth,
        5,
        "car nesting counts the body list itself"
    );
}

#[test]
fn shape_matches_live_bodies_by_tokens() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let (args, body) = lambda_parts(&mut eval, "(lambda (x) (car x) (cdr x))");
    let shape = ClosureShape::of(args, body).expect("shape");
    assert!(shape.matches(args, body));
    let (args2, body2) = lambda_parts(&mut eval, "(lambda (x) (car x) (cdr x))");
    assert!(shape.matches(args2, body2), "an equal copy matches");
    // Mutating the body is seen.
    eval.set_variable("cm-body", body);
    eval_ok(&mut eval, "(setcar (cdr (car cm-body)) 'y)");
    assert!(!shape.matches(args, body));
    // A body that grew, and a cyclic one, stop at the recorded length.
    let (args3, body3) = lambda_parts(&mut eval, "(lambda (x) (car x) (cdr x) (car x))");
    assert!(!shape.matches(args3, body3));
    eval.set_variable("cm-cyclic", body2);
    eval_ok(&mut eval, "(setcdr (cdr cm-cyclic) cm-cyclic)");
    assert!(!shape.matches(args2, body2));
}

#[test]
fn shape_refuses_cycles_huge_bodies_and_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let (args, body) = lambda_parts(&mut eval, "(lambda (x) (car x))");
    eval.set_variable("cm-cyc", body);
    eval_ok(&mut eval, "(setcdr cm-cyc cm-cyc)");
    assert_eq!(ClosureShape::of(args, body), Err(ShapeRefusal::TooLarge));
    assert_eq!(ClosureFacts::of(args, body), Err(FactsRefusal::TooLarge));

    let huge = eval_ok(
        &mut eval,
        &format!("(setq cm-test-huge (make-list {} 'x))", SHAPE_NODE_CAP),
    );
    assert_eq!(
        ClosureShape::of(Value::NIL, huge),
        Err(ShapeRefusal::TooLarge)
    );

    let positioned = eval_ok(
        &mut eval,
        "(setq cm-test-pos (list (list (position-symbol 'car 12) 'x)))",
    );
    assert_eq!(
        ClosureShape::of(Value::NIL, positioned),
        Err(ShapeRefusal::SymbolWithPos)
    );
}

fn facts_of(eval: &mut Context, quoted_lambda: &str) -> Result<Vec<(String, bool)>, FactsRefusal> {
    let (args, body) = lambda_parts(eval, quoted_lambda);
    ClosureFacts::of(args, body).map(|facts| {
        facts
            .symbols
            .iter()
            .map(|role| {
                (
                    crate::emacs_core::intern::resolve_sym(role.id).to_string(),
                    role.head,
                )
            })
            .collect()
    })
}

fn heads(eval: &mut Context, quoted_lambda: &str) -> Vec<String> {
    let mut heads: Vec<String> = facts_of(eval, quoted_lambda)
        .unwrap_or_else(|why| panic!("{quoted_lambda}: {why:?}"))
        .into_iter()
        .filter_map(|(name, head)| head.then_some(name))
        .collect();
    heads.sort();
    heads
}

fn symbols(eval: &mut Context, quoted_lambda: &str) -> Vec<String> {
    let mut all: Vec<String> = facts_of(eval, quoted_lambda)
        .unwrap_or_else(|why| panic!("{quoted_lambda}: {why:?}"))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    all.sort();
    all
}

#[test]
fn facts_mark_the_positions_macroexpansion_can_reach() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    // let binders are not heads; their values and the body are forms.
    assert_eq!(
        heads(
            &mut eval,
            "(lambda () (let ((when (f 1)) unless) (g when unless)))"
        ),
        ["f", "g", "let"]
    );
    // Quoted data and (function SYMBOL) are data.
    assert_eq!(
        symbols(&mut eval, "(lambda () (h '(when x) #'unless))"),
        ["h"]
    );
    // An inner lambda's arglist binds; its body is forms.
    assert_eq!(
        heads(
            &mut eval,
            "(lambda () (mapcar #'(lambda (when) (k when)) l))"
        ),
        ["k", "mapcar"]
    );
    // cond clauses are lists of forms; condition-case's variable and
    // handler conditions are not heads.
    assert_eq!(
        heads(
            &mut eval,
            "(lambda () (cond (v (a)) ((b) c)) (condition-case when (d) (error (e) when)))"
        ),
        ["a", "b", "cond", "condition-case", "d", "e"]
    );
    // Anything else is a superset: the car of every list reached is a head.
    assert_eq!(
        heads(&mut eval, "(lambda () (foo (bar baz)))"),
        ["bar", "foo"]
    );
    // A form whose car is a list: both walked.
    assert_eq!(heads(&mut eval, "(lambda () ((m n) (o)))"), ["m", "o"]);
}

#[test]
fn facts_refuse_used_underscore_variables_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    assert!(facts_of(&mut eval, "(lambda (_x) 1)").is_ok());
    assert!(
        facts_of(
            &mut eval,
            "(lambda () (list #'(lambda (_) 1) #'(lambda (_) 2)))"
        )
        .is_ok()
    );
    // A bare (lambda ...) is a macro call, walked as one: its arglist is
    // not a binding position there (and the `lambda' head refuses it).
    assert_eq!(
        facts_of(&mut eval, "(lambda () (list (lambda (_) 1)))"),
        Err(FactsRefusal::UnderscoreUse)
    );
    assert!(facts_of(&mut eval, "(lambda () (let ((_y 1)) 2))").is_ok());
    for used in [
        "(lambda (_x) _x)",
        "(lambda () (let ((_y 1)) _y))",
        "(lambda () (setq _z 1))",
        "(lambda () (_f 1))",
    ] {
        assert_eq!(
            facts_of(&mut eval, used),
            Err(FactsRefusal::UnderscoreUse),
            "{used}"
        );
    }
}

#[test]
fn env_summary_splits_lexical_and_dynamic_entries() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let env = eval_ok(&mut eval, "'((x . 1) y (z . 2) (x . 3) t)");
    let summary = EnvSummary::of(env).expect("summary");
    let names = |ids: &[crate::emacs_core::intern::SymId]| {
        ids.iter()
            .map(|id| crate::emacs_core::intern::resolve_sym(*id).to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&summary.lex), ["x", "z", "x"]);
    assert_eq!(names(&summary.dynamic), ["y", "t"]);
    for odd in [
        "'((x . 1) . 3)",
        "'((nil . 1))",
        "'(nil)",
        "'((1 . 2))",
        "'(3)",
    ] {
        let env = eval_ok(&mut eval, odd);
        assert!(EnvSummary::of(env).is_none(), "{odd}");
    }
    assert_eq!(EnvSummary::of(Value::NIL), Some(EnvSummary::default()));
}

/// T0.10: the Rust head verdict against GNU's own `macrop`, `autoloadp`
/// and `function-get` for every interned symbol, with cl-lib and bytecomp
/// loaded (compiler macros on `eq`, `memq`, cl-lib's `cl-first`, ...).
#[test]
fn head_verdicts_match_the_lisp_predicates_for_every_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    eval_ok(
        &mut eval,
        "(progn (require 'cl-lib) (require 'bytecomp) (require 'subr-x))",
    );
    eval_ok(
        &mut eval,
        "(progn (defalias 'cm-alias-to-when 'when)
                (defalias 'cm-alias-to-car 'car)
                (put 'cm-cmacro-fn 'compiler-macro (lambda (form &rest _) form))
                (defalias 'cm-alias-to-cmacro 'cm-cmacro-fn)
                (autoload 'cm-autoload-macro \"cm-nowhere\" nil nil 'macro)
                (autoload 'cm-autoload-t \"cm-nowhere\" nil nil t)
                (autoload 'cm-autoload-fn \"cm-nowhere\"))",
    );
    let lisp = eval_ok(
        &mut eval,
        "(let (out)
           (mapatoms
            (lambda (s)
              (when (condition-case nil
                        (or (and (fboundp s)
                                 (let ((d (symbol-function s)))
                                   (or (and (eq (car-safe d) 'autoload)
                                            (memq (nth 4 d) '(macro t)))
                                       (and (symbolp d) (macrop d))
                                       (eq (car-safe d) 'macro))))
                            (function-get s 'compiler-macro))
                      (error t))
                (push s out))))
           out)",
    );
    let mut lisp_names: Vec<String> = crate::emacs_core::value::list_to_vec(&lisp)
        .expect("list")
        .into_iter()
        .map(|s| s.as_symbol_name().expect("symbol").to_string())
        .collect();
    lisp_names.sort();
    let mut rust_names: Vec<String> = eval
        .obarray()
        .all_symbols()
        .into_iter()
        .filter(|name| eval.cconv_head_verdict(intern(name)) != HeadVerdict::Plain)
        .map(str::to_string)
        .collect();
    rust_names.sort();
    rust_names.dedup();
    assert!(lisp_names.len() > 100, "{}", lisp_names.len());
    assert_eq!(rust_names, lisp_names);

    let verdict = |eval: &Context, name: &str| eval.cconv_head_verdict(intern(name));
    assert_eq!(verdict(&eval, "car"), HeadVerdict::Plain);
    assert_eq!(verdict(&eval, "when"), HeadVerdict::Macro);
    assert_eq!(verdict(&eval, "cm-alias-to-when"), HeadVerdict::Macro);
    assert_eq!(verdict(&eval, "cm-alias-to-car"), HeadVerdict::Plain);
    assert_eq!(verdict(&eval, "cm-cmacro-fn"), HeadVerdict::CompilerMacro);
    assert_eq!(
        verdict(&eval, "cm-alias-to-cmacro"),
        HeadVerdict::CompilerMacro
    );
    assert_eq!(
        verdict(&eval, "cm-autoload-macro"),
        HeadVerdict::AutoloadMacro
    );
    assert_eq!(verdict(&eval, "cm-autoload-t"), HeadVerdict::AutoloadMacro);
    assert_eq!(verdict(&eval, "cm-autoload-fn"), HeadVerdict::Plain);
    assert_eq!(
        verdict(&eval, "eq"),
        HeadVerdict::CompilerMacro,
        "bytecomp's"
    );
}

#[test]
fn stats_classify_eligibility() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    eval_ok(
        &mut eval,
        "(put 'cm-cm2 'compiler-macro (lambda (form &rest _) form))",
    );
    for (form, event) in [
        (
            "(let ((a 1)) (lambda () (list a)))",
            CconvMemoEvent::Eligible,
        ),
        (
            "(let ((a 1)) (lambda () (when a 1)))",
            CconvMemoEvent::RefuseMacroHead,
        ),
        (
            "(let ((a 1)) (lambda () (cm-cm2 a)))",
            CconvMemoEvent::RefuseCompilerMacroHead,
        ),
        (
            "(let ((a 1)) (lambda (_b) (list a _b)))",
            CconvMemoEvent::RefuseUnderscore,
        ),
        (
            "(let ((a 1)) (lambda () (interactive) a))",
            CconvMemoEvent::RefuseInteractive,
        ),
    ] {
        let before = count(&eval, event);
        eval_ok(&mut eval, form);
        assert_eq!(count(&eval, event), before + 1, "{form}");
    }
}

// ---------------------------------------------------------------------------
// S0.4: the trusted set and its audit
// ---------------------------------------------------------------------------

use crate::emacs_core::eval::{
    EXCLUDED_CALLEES, TRUSTED_LISP, TRUSTED_VARIABLES, TrustRefusal, TrustedVariableRole,
};

fn constant_symbols(function: Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut stack: Vec<Value> = function
        .get_bytecode_data()
        .expect("byte-code")
        .constants
        .iter()
        .copied()
        .collect();
    while let Some(value) = stack.pop() {
        if value.is_symbol() && !value.is_nil() {
            out.push(value);
        } else if value.is_cons() {
            stack.push(value.cons_car());
            stack.push(value.cons_cdr());
        } else if let Some(code) = value.get_bytecode_data() {
            stack.extend(code.constants.iter().copied());
        } else if let Some(items) = value.as_vector_data() {
            stack.extend(items.iter().copied());
        }
    }
    out
}

/// T0.11: what the dumped trusted byte-code calls and reads, pinned.  A GNU
/// sync that changes `cconv.el' or `macroexp.el' fails here and forces a
/// re-audit of the memo's exactness argument (p4-1 design, section 5.5).
#[test]
fn trusted_set_audit_pins_the_dumped_byte_code() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    assert!(
        eval.cconv_trusted_set_valid(),
        "{:?}",
        eval.cconv_memo.trusted().refusal()
    );

    let mut callees: Vec<String> = Vec::new();
    let mut specials: Vec<String> = Vec::new();
    for name in TRUSTED_LISP {
        let function = eval
            .obarray()
            .symbol_function(name)
            .unwrap_or_else(|| panic!("{name} is void"));
        assert!(
            function.get_bytecode_data().is_some(),
            "{name} is not byte-code"
        );
        for symbol in constant_symbols(function) {
            let id = symbol.as_symbol_id().expect("symbol");
            let name = crate::emacs_core::intern::resolve_sym(id).to_string();
            if eval.obarray().is_special_id(id) && !name.starts_with(':') {
                specials.push(name.clone());
            }
            let Some(cell) = eval
                .obarray()
                .symbol_function_id(id)
                .filter(|c| !c.is_nil())
            else {
                continue;
            };
            let subr = cell.is_subr() || cell.as_subr_id().is_some();
            let macro_cell = cell.is_cons() && cell.cons_car().is_symbol_named("macro");
            if !subr && !macro_cell {
                callees.push(name);
            }
        }
    }
    callees.sort();
    callees.dedup();
    specials.sort();
    specials.dedup();

    // Every Lisp callee is trusted or excluded with a reason.
    let mut unaudited: Vec<&String> = callees
        .iter()
        .filter(|name| {
            !TRUSTED_LISP.contains(&name.as_str())
                && !EXCLUDED_CALLEES
                    .iter()
                    .any(|(excluded, _)| excluded == name)
        })
        .collect();
    unaudited.sort();
    assert!(unaudited.is_empty(), "unaudited callees: {unaudited:?}");
    let mut expected_callees: Vec<String> = TRUSTED_LISP
        .iter()
        .filter(|name| **name != "cconv-make-interpreted-closure")
        .chain(EXCLUDED_CALLEES.iter().map(|(name, _)| name))
        .map(|name| name.to_string())
        .collect();
    expected_callees.sort();
    assert_eq!(callees, expected_callees, "the audited callee set moved");

    // Every special variable they mention is classified.
    let mut classified: Vec<String> = TRUSTED_VARIABLES
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    classified.sort();
    assert_eq!(specials, classified, "the audited variable set moved");
    assert_eq!(
        TRUSTED_VARIABLES
            .iter()
            .filter(|(_, role)| *role == TrustedVariableRole::Key)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        ["lexical-binding"]
    );

    // The snapshot holds the trusted functions and their subrs, and the
    // `(t)' constant.
    let cells = eval.cconv_memo.trusted().trusted_cells();
    for name in TRUSTED_LISP {
        assert!(cells.iter().any(|(id, _)| *id == intern(name)), "{name}");
    }
    for subr in [
        "make-interpreted-closure",
        "mapcar",
        "delq",
        "special-variable-p",
    ] {
        assert!(cells.iter().any(|(id, _)| *id == intern(subr)), "{subr}");
    }
    let empty = eval.cconv_memo.trusted().empty_env();
    assert_eq!(print_value(&empty), "(t)");
}

#[test]
fn trusted_set_follows_advice_and_redefinition() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    assert!(eval.cconv_trusted_set_valid());
    eval_ok(&mut eval, "(defun cm-around (f &rest args) (apply f args))");
    eval_ok(
        &mut eval,
        "(advice-add 'macroexp--all-forms :around #'cm-around)",
    );
    assert!(
        !eval.cconv_trusted_set_valid(),
        "advice on a trusted function"
    );
    eval_ok(
        &mut eval,
        "(advice-remove 'macroexp--all-forms #'cm-around)",
    );
    assert!(
        eval.cconv_trusted_set_valid(),
        "the original object is back"
    );

    // An advised callee subr counts too.
    eval_ok(
        &mut eval,
        "(advice-add 'special-variable-p :around #'cm-around)",
    );
    assert!(!eval.cconv_trusted_set_valid());
    eval_ok(&mut eval, "(advice-remove 'special-variable-p #'cm-around)");
    assert!(eval.cconv_trusted_set_valid());

    // Unrelated definitions move function_epoch but not the set.
    eval_ok(&mut eval, "(defalias 'cm-unrelated #'car)");
    assert!(eval.cconv_trusted_set_valid());

    // A redefinition stays untrusted.
    eval_ok(
        &mut eval,
        "(defalias 'macroexp-const-p (lambda (_exp) nil))",
    );
    assert!(!eval.cconv_trusted_set_valid());
    let before = count(&eval, CconvMemoEvent::Untrusted);
    eval_ok(&mut eval, "(let ((a 1)) (lambda () a))");
    assert_eq!(count(&eval, CconvMemoEvent::Untrusted), before + 1);
}

#[test]
fn trusted_set_refuses_source_loaded_cconv() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    // Before first use: an interpreted trusted function refuses the build.
    eval_ok(&mut eval, "(defalias 'caar (lambda (x) (car (car x))))");
    assert!(!eval.cconv_trusted_set_valid());
    assert_eq!(
        eval.cconv_memo.trusted().refusal(),
        Some(&TrustRefusal::NotCompiled("caar"))
    );
}

// ---------------------------------------------------------------------------
// S0.5: the memo
// ---------------------------------------------------------------------------

/// Closure creations covering the shapes cconv.el trims: captured and
/// uncaptured variables, shadowed duplicates, dynamic variables in the
/// environment, nothing captured (the `(t)` constant), docstrings, nested
/// lambdas, let/let*/cond/condition-case/setq bodies.
const CORPUS: &[&str] = &[
    "(let ((a 1) (b 2)) (lambda (c) (list a c)))",
    "(let ((a 1) (a 2)) (lambda () a))",
    "(let ((a 1)) (let ((a 2) (b 3)) (lambda () (list a b))))",
    "(let ((a 1) (b 2)) (lambda () 'nothing))",
    "(let ((a 1)) (lambda (x) \"doc\" (+ a x)))",
    "(let ((a 1) (b 2)) (lambda () (mapcar #'(lambda (x) (list x b)) '(1 2))))",
    "(let ((a 1) (b 2)) (lambda () (let* ((c a) (d c)) (list c d))))",
    "(let ((a 1) (b 2)) (lambda (x) (cond ((eq x 1) a) (t b))))",
    "(let ((a 1)) (lambda () (condition-case err (car a) (error (list err a)))))",
    "(let ((a 1) (b 2)) (lambda () (setq a (1+ b))))",
    "(let ((a 1)) (defvar cm-dyn-var) (let ((cm-dyn-var 2)) (lambda () (list a cm-dyn-var))))",
    "(let ((a 1)) (lambda (&optional x &rest y) (list a x y)))",
];

/// Every corpus closure, created twice in a row by one function (the same
/// body object), printed, with its environment cells' identity checked.
fn corpus_transcript(mode: CconvMemoMode) -> (Vec<String>, Context) {
    let mut eval = startup(mode);
    let mut out = Vec::new();
    for (i, form) in CORPUS.iter().enumerate() {
        eval_ok(&mut eval, &format!("(defun cm-corpus-{i} () {form})"));
        for _ in 0..3 {
            out.push(printed(&mut eval, &format!("(cm-corpus-{i})")));
        }
        // The closure body is the source body, and every environment cell
        // is the live environment's own cell.
        let check = format!(
            "(let* ((f (cm-corpus-{i}))
                    (env (aref f 2)))
               (list (eq (aref f 1) (aref (cm-corpus-{i}) 1))
                     (or (equal env '(t))
                         (let ((ok t))
                           (dolist (cell env ok)
                             (when (and (consp cell) (not (eq (cdr cell) (cdr cell))))
                               (setq ok nil)))))))"
        );
        out.push(printed(&mut eval, &check));
    }
    (out, eval)
}

#[test]
fn on_mode_builds_what_the_lisp_builds() {
    crate::test_utils::init_test_tracing();
    let (off, _) = corpus_transcript(CconvMemoMode::Off);
    let (on, eval) = corpus_transcript(CconvMemoMode::On);
    assert_eq!(on, off);
    let recorded = count(&eval, CconvMemoEvent::Recorded);
    let served = count(&eval, CconvMemoEvent::Served);
    assert!(
        recorded >= CORPUS.len() as u64 - 1,
        "{}",
        eval.cconv_memo_report()
    );
    assert!(served >= 2 * recorded, "{}", eval.cconv_memo_report());
    assert_eq!(count(&eval, CconvMemoEvent::VerifyMismatch), 0);
}

#[test]
fn verify_mode_matches_on_the_corpus() {
    crate::test_utils::init_test_tracing();
    let (off, _) = corpus_transcript(CconvMemoMode::Off);
    let (verify, eval) = corpus_transcript(CconvMemoMode::Verify);
    assert_eq!(verify, off);
    assert!(
        count(&eval, CconvMemoEvent::VerifyMatch) > 0,
        "{}",
        eval.cconv_memo_report()
    );
    assert_eq!(count(&eval, CconvMemoEvent::VerifyMismatch), 0);
    assert_eq!(count(&eval, CconvMemoEvent::Served), 0);
}

/// The environment cells of a served closure are `eq` to the creating
/// environment's (a `setq` through one is seen by the other), the body is
/// `eq` to the source, and `(t)` is cconv's shared constant.
#[test]
fn served_closures_share_cells_body_and_the_empty_env_constant() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::On);
    eval_ok(
        &mut eval,
        "(defun cm-share (v) (let ((a v) (b 2)) (list (lambda () a) (lambda (x) (setq a x)))))",
    );
    for _ in 0..3 {
        let result = printed(
            &mut eval,
            "(let* ((pair (cm-share 5)) (get (car pair)) (set (cadr pair)))
               (funcall set 9)
               (list (funcall get) (eq (car (aref get 2)) (car (aref set 2)))))",
        );
        assert_eq!(result, "(9 t)");
    }
    eval_ok(&mut eval, "(defun cm-empty () (let ((a 1)) (lambda () 3)))");
    assert_eq!(
        printed(
            &mut eval,
            "(list (eq (aref (cm-empty) 2) (aref (cm-empty) 2)) (aref (cm-empty) 2))"
        ),
        "(t (t))"
    );
    assert!(
        count(&eval, CconvMemoEvent::Served) >= 4,
        "{}",
        eval.cconv_memo_report()
    );
}

/// T0.3: a change between two creations that changes the analysis misses.
#[test]
fn facts_changes_between_creations_miss() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::On);
    eval_ok(
        &mut eval,
        "(defun cm-facts () (let ((a 1)) (lambda () (let ((cm-later-special 2)) (cm-f2 a cm-later-special)))))",
    );
    let first = printed(&mut eval, "(cm-facts)");
    // The first run may intern the symbols' global membership (an effect,
    // so it is not recorded); by the third creation it is served.
    printed(&mut eval, "(cm-facts)");
    printed(&mut eval, "(cm-facts)");
    assert!(
        count(&eval, CconvMemoEvent::Served) >= 1,
        "{}",
        eval.cconv_memo_report()
    );
    // A defvar makes the let binder dynamic: the closure no longer needs...
    // whatever it needs, it must be recomputed.
    eval_ok(&mut eval, "(defvar cm-later-special 0)");
    let misses = count(&eval, CconvMemoEvent::MissFacts);
    let after_defvar = printed(&mut eval, "(cm-facts)");
    assert_eq!(count(&eval, CconvMemoEvent::MissFacts), misses + 1);
    let mut off = startup(CconvMemoMode::Off);
    eval_ok(
        &mut off,
        "(defun cm-facts () (let ((a 1)) (lambda () (let ((cm-later-special 2)) (cm-f2 a cm-later-special)))))",
    );
    assert_eq!(first, printed(&mut off, "(cm-facts)"));
    eval_ok(&mut off, "(defvar cm-later-special 0)");
    assert_eq!(after_defvar, printed(&mut off, "(cm-facts)"));

    // A head that becomes a macro, a compiler macro, or an autoloaded macro.
    for (change, undo) in [
        (
            "(defmacro cm-f2 (&rest args) `(list ,@args))",
            "(fmakunbound 'cm-f2)",
        ),
        (
            "(put 'cm-f2 'compiler-macro (lambda (form &rest _) form))",
            "(put 'cm-f2 'compiler-macro nil)",
        ),
        (
            "(autoload 'cm-f2 \"cm-nowhere\" nil nil 'macro)",
            "(fmakunbound 'cm-f2)",
        ),
    ] {
        printed(&mut eval, "(cm-facts)");
        eval_ok(&mut eval, change);
        let misses = count(&eval, CconvMemoEvent::MissFacts);
        let refused = count(&eval, CconvMemoEvent::RefuseMacroHead)
            + count(&eval, CconvMemoEvent::RefuseCompilerMacroHead);
        let _ = eval.eval_str("(cm-facts)");
        assert_eq!(
            count(&eval, CconvMemoEvent::MissFacts),
            misses + 1,
            "{change}"
        );
        assert_eq!(
            count(&eval, CconvMemoEvent::RefuseMacroHead)
                + count(&eval, CconvMemoEvent::RefuseCompilerMacroHead),
            refused + 1,
            "{change}"
        );
        eval_ok(&mut eval, undo);
    }
}

/// T0.4: advice on the filter or on the expander runs on every creation.
#[test]
fn advice_on_trusted_functions_runs_every_time() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::On);
    eval_ok(
        &mut eval,
        "(progn (defvar cm-advice-calls 0)
                (defun cm-count (f &rest args) (setq cm-advice-calls (1+ cm-advice-calls)) (apply f args))
                (defun cm-adv () (let ((a 1)) (lambda () a))))",
    );
    printed(&mut eval, "(cm-adv)");
    printed(&mut eval, "(cm-adv)");
    for target in ["cconv-make-interpreted-closure", "macroexp--expand-all"] {
        eval_ok(&mut eval, &format!("(setq cm-advice-calls 0)"));
        eval_ok(
            &mut eval,
            &format!("(advice-add '{target} :around #'cm-count)"),
        );
        for _ in 0..3 {
            printed(&mut eval, "(cm-adv)");
        }
        assert!(
            eval_ok(&mut eval, "cm-advice-calls")
                .as_fixnum()
                .unwrap_or(0)
                >= 3,
            "{target}"
        );
        eval_ok(&mut eval, &format!("(advice-remove '{target} #'cm-count)"));
    }
    let served = count(&eval, CconvMemoEvent::Served);
    printed(&mut eval, "(cm-adv)");
    assert_eq!(
        count(&eval, CconvMemoEvent::Served),
        served + 1,
        "served again"
    );
}

/// T0.5: `lexical-binding` nil in the current buffer changes the analysis
/// (cconv--not-lexical-var-p), so it is part of the key.
#[test]
fn lexical_binding_of_the_current_buffer_is_part_of_the_key() {
    crate::test_utils::init_test_tracing();
    let form = "(defun cm-lb () (let ((a 1)) (lambda () (let ((b 2)) (list a b)))))";
    let probe = "(list (cm-lb) (with-temp-buffer (setq lexical-binding nil) (cm-lb)) (cm-lb)
                       (with-temp-buffer (setq lexical-binding nil) (cm-lb)))";
    let mut off = startup(CconvMemoMode::Off);
    eval_ok(&mut off, form);
    let expected = printed(&mut off, probe);
    let mut on = startup(CconvMemoMode::On);
    eval_ok(&mut on, form);
    assert_eq!(printed(&mut on, probe), expected);
    assert_eq!(printed(&mut on, probe), expected);
    assert!(
        count(&on, CconvMemoEvent::Served) >= 4,
        "{}",
        on.cconv_memo_report()
    );
}

/// T0.6 and T0.7: `:closure-dont-trim-context`, interactive lambdas and
/// non-identity bodies are never served; a mutated body is re-analysed.
#[test]
fn unmemoizable_and_mutated_bodies() {
    crate::test_utils::init_test_tracing();
    let forms = [
        "(defun cm-u1 () (let ((a 1)) (lambda () :closure-dont-trim-context a)))",
        "(defun cm-u2 () (let ((a 1)) (lambda () (interactive) a)))",
        "(defun cm-u3 () (let ((a 1)) (lambda () (funcall #'(lambda (x) x) a))))",
        "(defun cm-u4 () (let ((a 1) (b 2)) (lambda () (list a))))",
    ];
    let probe = "(list (cm-u1) (cm-u1) (cm-u2) (cm-u2) (cm-u3) (cm-u3)
                       (cm-u4)
                       (progn (setcar (cdr (car (aref (cm-u4) 1))) 'b) (cm-u4))
                       (cm-u4))";
    let mut off = startup(CconvMemoMode::Off);
    let mut on = startup(CconvMemoMode::On);
    for form in forms {
        eval_ok(&mut off, form);
        eval_ok(&mut on, form);
    }
    let expected = printed(&mut off, probe);
    assert_eq!(printed(&mut on, probe), expected);
    assert!(
        count(&on, CconvMemoEvent::MissShape) >= 1,
        "{}",
        on.cconv_memo_report()
    );
}

/// T0.12: verify detects a wrong analysis (injected), counts it, and
/// returns the Lisp's closure.
#[test]
fn verify_detects_an_injected_wrong_analysis() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Verify);
    eval.cconv_memo.set_strict(false);
    eval_ok(
        &mut eval,
        "(defun cm-inj () (let ((a 1) (b 2)) (lambda () (list a))))",
    );
    let lisp = printed(&mut eval, "(cm-inj)");
    assert!(
        eval.cconv_memo.inject_wrong_analysis_for_test(),
        "an entry to corrupt"
    );
    assert_eq!(
        printed(&mut eval, "(cm-inj)"),
        lisp,
        "the Lisp's closure is returned"
    );
    assert_eq!(count(&eval, CconvMemoEvent::VerifyMismatch), 1);
}

/// T0.8/T0.9: near `max-lisp-eval-depth` the memo runs the Lisp, so every
/// depth succeeds or signals exactly as without the memo.
#[test]
fn creation_near_the_depth_limit_matches_the_lisp() {
    crate::test_utils::init_test_tracing();
    let setup = "(progn
       (defun cm-deep (n)
         (if (> n 0) (cm-deep (1- n))
           (let ((a 1)) (lambda () (list (list (list (list a))))))))
       (defun cm-try (n)
         (condition-case err (progn (cm-deep n) 'ok)
           (error (car err)))))";
    let probe = "(let ((max-lisp-eval-depth 400) (out nil))
                   (dotimes (i 140) (push (cm-try (+ 60 i)) out))
                   (nreverse out))";
    let mut off = startup(CconvMemoMode::Off);
    eval_ok(&mut off, setup);
    let expected = printed(&mut off, probe);
    assert!(
        expected.contains("ok") && expected.contains("excessive-lisp-nesting"),
        "{expected}"
    );
    let mut on = startup(CconvMemoMode::On);
    eval_ok(&mut on, setup);
    eval_ok(&mut on, "(cm-deep 0)");
    assert_eq!(printed(&mut on, probe), expected);
    assert!(
        count(&on, CconvMemoEvent::Served) > 0,
        "{}",
        on.cconv_memo_report()
    );
    assert!(
        count(&on, CconvMemoEvent::MissDepth) > 0,
        "{}",
        on.cconv_memo_report()
    );
}

/// T0.13: entries survive collections; recycled or mutated bodies are
/// caught by the shape compare.
#[test]
fn memo_survives_garbage_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::On);
    let mut off = startup(CconvMemoMode::Off);
    for ctx in [&mut eval, &mut off] {
        eval_ok(
            ctx,
            "(defun cm-gc (v) (let ((a v) (b 2)) (lambda () (list a))))",
        );
    }
    let probe = "(let (out) (dotimes (i 20) (push (cm-gc i) out) (garbage-collect)) out)";
    assert_eq!(printed(&mut eval, probe), printed(&mut off, probe));
    // Fresh bodies each time (read anew), created and dropped under GC.
    let fresh = "(let (out) (dotimes (i 20)
                   (push (funcall (eval (read \"(lambda (v) (let ((a v) (b 2)) (lambda () (list a b))))\") t) i) out)
                   (garbage-collect)) out)";
    assert_eq!(printed(&mut eval, fresh), printed(&mut off, fresh));
    assert!(
        count(&eval, CconvMemoEvent::Served) >= 19,
        "{}",
        eval.cconv_memo_report()
    );
}

/// A collection that completes while the memo observes a Lisp run (here a
/// verify run) leaves its finalizers and `post-gc-hook` to the end of the
/// run: GNU runs them wherever allocation trips a collection
/// (src/alloc.c `garbage_collect`), so what they do is no effect of the
/// conversion.  The MELPA closql soak caught an emacsql connection's
/// finalizer writing a function cell inside a verify run.
#[test]
fn collection_hooks_wait_for_the_end_of_an_observed_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Verify);
    eval.cconv_memo.set_strict(true);
    eval_ok(
        &mut eval,
        "(defun cm-hooks (v) (let ((a v) (b 2)) (lambda () (list a))))",
    );
    for _ in 0..3 {
        printed(&mut eval, "(cm-hooks 1)");
    }
    let matches = count(&eval, CconvMemoEvent::VerifyMatch);
    assert!(matches > 0, "{}", eval.cconv_memo_report());
    // Both hooks do what the snapshot sees: a function-cell write and a
    // gensym.
    eval_ok(
        &mut eval,
        "(progn (make-finalizer (lambda () (defalias 'cm-hooks-finalized #'ignore))) nil)",
    );
    eval_ok(
        &mut eval,
        "(progn (setq cm-hooks-gc-runs 0)
                (add-hook 'post-gc-hook
                          (lambda () (gensym) (setq cm-hooks-gc-runs (1+ cm-hooks-gc-runs)))))",
    );
    eval.cconv_memo.collect_in_next_observed_run_for_test();
    let closure = printed(&mut eval, "(cm-hooks 1)");
    assert_eq!(closure, "#[nil ((list a)) ((a . 1))]");
    assert_eq!(count(&eval, CconvMemoEvent::VerifyMatch), matches + 1);
    assert_eq!(count(&eval, CconvMemoEvent::VerifyMismatch), 0);
    assert_eq!(count(&eval, CconvMemoEvent::GcHooksDeferred), 1);
    // They ran before the creation returned.
    assert_eq!(
        printed(
            &mut eval,
            "(list (fboundp 'cm-hooks-finalized) (> cm-hooks-gc-runs 0))"
        ),
        "(t t)"
    );
}

#[test]
fn bypasses() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::On);
    eval_ok(&mut eval, "(defun cm-by () (let ((a 1)) (lambda () a)))");
    printed(&mut eval, "(cm-by)");
    let before = count(&eval, CconvMemoEvent::BypassCompileEnv);
    printed(
        &mut eval,
        "(let ((macroexp-inhibit-compiler-macros t)) (cm-by))",
    );
    printed(
        &mut eval,
        "(let ((overriding-plist-environment '((x a 1)))) (cm-by))",
    );
    assert_eq!(count(&eval, CconvMemoEvent::BypassCompileEnv), before + 2);
}

// ---------------------------------------------------------------------------
// S0.6: the native untrimmed path
// ---------------------------------------------------------------------------

use crate::emacs_core::eval::parse_cconv_fast_knob;

fn startup_fast(mode: CconvMemoMode) -> Context {
    let mut eval = startup(mode);
    eval.cconv_memo.set_fast(true);
    eval
}

const UNTRIMMED: &[&str] = &[
    // `(t)' only.
    "(defun cm-n1 () (lambda (x) x))",
    // Dynamic entries only.
    "(defun cm-n2 () (defvar cm-n2-var) (lambda () cm-n2-var))",
    // The marker with forms after it: dropped, the environment kept whole.
    "(defun cm-n3 () (let ((a 1) (b 2)) (lambda () :closure-dont-trim-context a)))",
    // The marker alone is the body (not dropped).
    "(defun cm-n4 () (lambda () :closure-dont-trim-context))",
    // Docstring and interactive form pass through.
    "(defun cm-n5 () (lambda (x) \"doc\" (interactive \"p\") x))",
    "(defun cm-n6 () (lambda (&optional x &rest r) (list x r)))",
];

fn untrimmed_transcript(eval: &mut Context) -> Vec<String> {
    for form in UNTRIMMED {
        eval_ok(eval, form);
    }
    let mut out = Vec::new();
    for i in 1..=UNTRIMMED.len() {
        out.push(printed(eval, &format!("(cm-n{i})")));
        out.push(printed(
            eval,
            &format!("(let ((f (cm-n{i}))) (list (eq (aref f 1) (aref (cm-n{i}) 1)) (length f)))"),
        ));
    }
    // A non-list arglist: cconv's cl-assert signals from the Lisp.
    out.push(format!(
        "{:?}",
        eval.eval_str("(let ((a 1)) (lambda x :closure-dont-trim-context a))")
            .is_err()
    ));
    out
}

#[test]
fn fast_knob_values() {
    crate::test_utils::init_test_tracing();
    assert!(!parse_cconv_fast_knob(None));
    assert!(!parse_cconv_fast_knob(Some("off")));
    assert!(!parse_cconv_fast_knob(Some("0")));
    assert!(parse_cconv_fast_knob(Some("on")));
    assert!(parse_cconv_fast_knob(Some(" 1 ")));
    assert_eq!(parse_cconv_memo_knob(Some("on")), CconvMemoMode::On);
    assert_eq!(parse_cconv_memo_knob(Some("1")), CconvMemoMode::On);
    assert_eq!(parse_cconv_memo_knob(Some("verify")), CconvMemoMode::Verify);
}

#[test]
fn fast_untrimmed_path_builds_what_the_lisp_builds() {
    crate::test_utils::init_test_tracing();
    let expected = untrimmed_transcript(&mut startup(CconvMemoMode::Off));
    let mut fast = startup_fast(CconvMemoMode::Off);
    assert_eq!(untrimmed_transcript(&mut fast), expected);
    assert!(
        count(&fast, CconvMemoEvent::FastServed) >= 10,
        "{}",
        fast.cconv_memo_report()
    );
    assert!(
        count(&fast, CconvMemoEvent::DontTrim) >= 2,
        "{}",
        fast.cconv_memo_report()
    );
    let mut verify = startup_fast(CconvMemoMode::Verify);
    assert_eq!(untrimmed_transcript(&mut verify), expected);
    assert!(
        count(&verify, CconvMemoEvent::VerifyMatch) >= 10,
        "{}",
        verify.cconv_memo_report()
    );
    assert_eq!(count(&verify, CconvMemoEvent::VerifyMismatch), 0);
}

#[test]
fn fast_untrimmed_path_leaves_edge_cases_to_the_lisp() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup_fast(CconvMemoMode::Off);
    eval_ok(&mut eval, "(defun cm-e1 () (lambda () 1))");
    // Advice on the filter: the Lisp runs (and the advice with it).
    eval_ok(
        &mut eval,
        "(progn (defvar cm-e-calls 0)
                (defun cm-e-count (f &rest args) (setq cm-e-calls (1+ cm-e-calls)) (apply f args))
                (advice-add 'cconv-make-interpreted-closure :around #'cm-e-count))",
    );
    printed(&mut eval, "(cm-e1)");
    assert_eq!(eval_ok(&mut eval, "cm-e-calls"), Value::fixnum(1));
    eval_ok(
        &mut eval,
        "(advice-remove 'cconv-make-interpreted-closure #'cm-e-count)",
    );
    let served = count(&eval, CconvMemoEvent::FastServed);
    printed(&mut eval, "(cm-e1)");
    assert_eq!(count(&eval, CconvMemoEvent::FastServed), served + 1);
    // Near the depth limit: the same outcome as the Lisp at every depth.
    let setup = "(defun cm-e-deep (n) (if (> n 0) (cm-e-deep (1- n)) (lambda () 1)))";
    let probe = "(let ((max-lisp-eval-depth 200) (out nil))
                   (dotimes (i 60)
                     (push (condition-case err (progn (cm-e-deep (+ 150 i)) 'ok) (error (car err))) out))
                   (nreverse out))";
    let mut off = startup(CconvMemoMode::Off);
    eval_ok(&mut off, setup);
    eval_ok(&mut eval, setup);
    assert_eq!(printed(&mut eval, probe), printed(&mut off, probe));
    assert!(
        count(&eval, CconvMemoEvent::FastRefused) > 0,
        "{}",
        eval.cconv_memo_report()
    );
}

// ---------------------------------------------------------------------------
// S0.7: verify soak on real GNU sources
// ---------------------------------------------------------------------------

/// Load GNU libraries from source (their eager expansion creates closures by
/// the thousand) and run code that creates more, with the memo and the
/// native path in verify: every hit is compared with the Lisp (a mismatch
/// panics in test builds).  Then the same workload must print the same
/// under `on` as under `off`.
#[test]
fn verify_soak_on_gnu_sources() {
    crate::test_utils::init_test_tracing();
    let load = "(let ((load-suffixes '(\".el\")))
                  (dolist (lib '(\"emacs-lisp/cl-seq\" \"emacs-lisp/cl-extra\" \"emacs-lisp/seq\"
                                 \"emacs-lisp/subr-x\" \"emacs-lisp/ring\"))
                    (load lib nil t)))";
    let work = "(let ((acc nil))
                  (dotimes (i 30)
                    (push (list (seq-filter (lambda (x) (> x i)) '(1 5 10 20 40))
                                (cl-remove-if (lambda (x) (= x i)) (number-sequence 0 5))
                                (seq-reduce (lambda (a b) (+ a b i)) '(1 2 3) 0)
                                (cl-some (lambda (x) (and (> x i) x)) '(3 7 11))
                                (seq-map-indexed (lambda (e n) (cons e (+ n i))) '(a b))
                                (let ((r (make-ring 3))) (ring-insert r i) (ring-elements r))
                                (string-join (mapcar (lambda (s) (format \"%s%d\" s i)) '(\"a\" \"b\")) \",\"))
                          acc))
                  acc)";
    let mut verify = startup_fast(CconvMemoMode::Verify);
    eval_ok(&mut verify, load);
    let verified = printed(&mut verify, work);
    let report = verify.cconv_memo_report();
    assert!(
        count(&verify, CconvMemoEvent::VerifyMatch) > 100,
        "{report}"
    );
    assert_eq!(
        count(&verify, CconvMemoEvent::VerifyMismatch),
        0,
        "{report}"
    );
    tracing::info!("verify soak: {report}");

    let mut off = startup(CconvMemoMode::Off);
    eval_ok(&mut off, load);
    let expected = printed(&mut off, work);
    assert_eq!(verified, expected);
    let mut on = startup_fast(CconvMemoMode::On);
    eval_ok(&mut on, load);
    assert_eq!(printed(&mut on, work), expected);
    assert!(
        count(&on, CconvMemoEvent::Served) > 100,
        "{}",
        on.cconv_memo_report()
    );
}
