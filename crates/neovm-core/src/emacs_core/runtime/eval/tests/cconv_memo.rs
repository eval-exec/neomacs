//! P4.1 Stage 0 pins: the `cconv-make-interpreted-closure` hook's knob,
//! statistics and effect snapshot.

use crate::emacs_core::eval::{CconvMemoEvent, CconvMemoMode, Context, parse_cconv_memo_knob};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

fn startup(mode: CconvMemoMode) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.cconv_memo.set_mode(mode);
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

/// `(ARGS . BODY)` of a quoted lambda: `(lambda ARGS . BODY)`.
fn lambda_parts(eval: &mut Context, quoted_lambda: &str) -> (Value, Value) {
    let lambda = eval_ok(eval, &format!("(quote {quoted_lambda})"));
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

    let huge = eval_ok(&mut eval, &format!("(make-list {} 'x)", SHAPE_NODE_CAP));
    assert_eq!(
        ClosureShape::of(Value::NIL, huge),
        Err(ShapeRefusal::TooLarge)
    );

    let positioned = eval_ok(&mut eval, "(list (list (position-symbol 'car 12) 'x))");
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
