//! Leaf equivalence harness (design `p1-2-builtin-intrinsics` §6.4): every
//! leaf against its reference on a shared argument pool.
//!
//! * A Bcall leaf's reference is the REGISTERED builtin, called through its
//!   function pointer (the body `funcall_subr` runs).
//! * An opcode leaf's reference is the interpreter's opcode arm, run as
//!   `(lambda (a [b]) (OP a [b]))` on the VM.
//!
//! Every successful answer must be the reference's own bits (every leaf here
//! answers an existing object or an immediate); every signal the same
//! condition with the same data; `Generic` exactly on the declared bounce
//! shapes, each of which the pool exercises. Both settings of
//! `symbols-with-pos-enabled` are run. The pool is built by ONE evaluation
//! and rooted, so a collection between cases cannot free half of it.

use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::error::Flow;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::leaf::LEAVES;
use crate::emacs_core::value::LambdaParams;
use crate::tagged::header::SubrFn;

/// The general pool: every shape a list, sequence, symbol or string builtin
/// distinguishes, including improper and circular lists, symbols with
/// positions, and each string storage kind.
const POOL: &[&str] = &[
    "0",
    "1",
    "2",
    "-1",
    "127",
    "128",
    "most-positive-fixnum",
    "(expt 2 70)",
    "(- (expt 2 70))",
    "1.5",
    "-0.0",
    "nil",
    "t",
    "'a",
    "'b",
    "'leaf-sym",
    ":kw",
    "(position-symbol 'a 7)",
    "\"abc\"",
    "\"ABC\"",
    "\"\"",
    "(string-to-multibyte \"abc\")",
    "\"a\u{3b2}c\"",
    "(string-to-unibyte \"a\\377c\")",
    "'(a b c)",
    "'(1 2 3)",
    "'((a . 1) (b . 2) (\"s\" . 3))",
    "'(a . b)",
    "'(a b . c)",
    "(let ((l (list 1 2 3))) (setcdr (cdr (cdr l)) l) l)",
    "(make-list 200 'x)",
    "[1 2 3]",
    "(record 'foo 1)",
    "(make-bool-vector 3 t)",
    "(make-char-table 'foo)",
    "(make-hash-table)",
    "(symbol-function 'car)",
];

fn params(n: usize) -> LambdaParams {
    LambdaParams {
        required: (1..=n as u32).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    }
}

/// `(lambda (a [b]) (OP a [b]))`.
fn opcode_fn(op: Op, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(params(nargs));
    f.lexical = true;
    f.ops = match nargs {
        1 => vec![Op::StackRef(0), op, Op::Return],
        _ => vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
    };
    f.max_stack = 8;
    f
}

/// The opcode an opcode leaf stands in for.
fn leaf_opcode(id: LeafId) -> Option<Op> {
    Some(match id {
        LeafId::Get => Op::Get,
        LeafId::Length => Op::Length,
        LeafId::Nth => Op::Nth,
        LeafId::Nthcdr => Op::Nthcdr,
        LeafId::Elt => Op::Elt,
        LeafId::Memq => Op::Memq,
        LeafId::Assq => Op::Assq,
        LeafId::Member => Op::Member,
        LeafId::Equal => Op::Equal,
        LeafId::StringEqual => Op::StringEqual,
        LeafId::StringLessp => Op::StringLessp,
        LeafId::Gethash | LeafId::PlistGet | LeafId::GetCharProperty => return None,
    })
}

/// A reference's or a leaf's outcome, comparable across the two.
#[derive(Debug)]
enum Outcome {
    Value(Value),
    Signal(SymId, Vec<Value>),
    Generic,
}

fn outcome_of_flow(flow: Flow) -> Outcome {
    match flow {
        Flow::Signal(sig) => Outcome::Signal(sig.symbol, sig.data.clone()),
        other => panic!("a leaf reference exited with {other:?}"),
    }
}

fn run_leaf(ctx: &Context, spec: &LeafSpec, args: &[Value]) -> Outcome {
    match spec.entry.call(ctx, args) {
        Ok(v) => Outcome::Value(v),
        Err(LeafExit::Signal(flow)) => outcome_of_flow(flow),
        Err(LeafExit::Generic) => Outcome::Generic,
    }
}

fn run_reference(ctx: &mut Context, spec: &LeafSpec, args: &[Value]) -> Outcome {
    let result = match spec.shape {
        LeafShape::Bcall => {
            let entry = crate::emacs_core::eval::lookup_global_subr_entry(intern(spec.name))
                .expect("registered");
            let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
            match entry.function.expect("a function") {
                SubrFn::A1(f) => f(ctx, arg(0)),
                SubrFn::A2(f) => f(ctx, arg(0), arg(1)),
                SubrFn::A3(f) => f(ctx, arg(0), arg(1), arg(2)),
                _ => panic!("{}: not a fixed-arity builtin", spec.name),
            }
        }
        LeafShape::Opcode => {
            let op = leaf_opcode(spec.id).expect("an opcode leaf has an opcode");
            let f = opcode_fn(op, spec.entry.slots() as usize);
            Vm::from_context(ctx).execute(&f, args.to_vec())
        }
    };
    match result {
        Ok(v) => Outcome::Value(v),
        Err(flow) => outcome_of_flow(flow),
    }
}

fn same_value(a: Value, b: Value) -> bool {
    a.bits() == b.bits()
        || (a.is_string()
            && b.is_string()
            && crate::emacs_core::value::try_equal_value_swp(&a, &b, 0, false).unwrap_or(false))
}

fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Value(v) if v.is_cons() => format!("value #x{:x}", v.bits()),
        Outcome::Value(v) => format!("value {}", print_value(v)),
        Outcome::Signal(sym, data) => format!(
            "signal {} ({} data)",
            crate::emacs_core::intern::resolve_sym(*sym),
            data.len()
        ),
        Outcome::Generic => "generic".to_string(),
    }
}

/// Run one case; `bounce` says whether its arguments are a declared bounce
/// shape. Returns whether the leaf bounced.
fn check(ctx: &mut Context, spec: &LeafSpec, args: &[Value], bounce: bool, what: &str) -> bool {
    let got = run_leaf(ctx, spec, args);
    if let Outcome::Generic = got {
        assert!(
            bounce,
            "{what}: the leaf bounced a shape it does not declare"
        );
        return true;
    }
    assert!(
        !bounce,
        "{what}: a declared bounce shape was answered: {}",
        describe(&got)
    );
    let want = run_reference(ctx, spec, args);
    let agree = match (&got, &want) {
        (Outcome::Value(a), Outcome::Value(b)) => same_value(*a, *b),
        (Outcome::Signal(sa, da), Outcome::Signal(sb, db)) => {
            sa == sb && da.len() == db.len() && da.iter().zip(db).all(|(a, b)| same_value(*a, *b))
        }
        _ => false,
    };
    assert!(
        agree,
        "{what}: leaf {} vs reference {}",
        describe(&got),
        describe(&want)
    );
    false
}

/// Evaluate `(list SRC...)` once and root the result.
fn pool(ctx: &mut Context, sources: &[&str]) -> Vec<Value> {
    let list = ctx
        .eval_str(&format!("(list {})", sources.join(" ")))
        .expect("pool evaluates");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    crate::emacs_core::value::list_to_vec(&list).expect("a proper list")
}

fn setup(ctx: &mut Context) {
    ctx.eval_str(
        "(progn (put 'leaf-sym 'p 'v) (put 'leaf-sym 'q nil) (put 'a 'p 'a-p)
                (define-hash-table-test 'leaf-ci
                  #'(lambda (x y) (equal x y)) #'(lambda (k) (sxhash-equal k))))",
    )
    .expect("setup");
}

fn set_swp(ctx: &mut Context, on: bool) {
    ctx.symbols_with_pos_enabled = on;
}

/// Every opcode leaf over the whole pool (pairs for two-slot leaves).
#[test]
fn opcode_leaves_match_their_opcode_arm() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    setup(&mut ctx);
    let values = pool(&mut ctx, POOL);
    let mut checked = 0usize;
    for on in [false, true] {
        set_swp(&mut ctx, on);
        for spec in LEAVES.iter().filter(|s| s.shape == LeafShape::Opcode) {
            match spec.entry.slots() {
                1 => {
                    for (i, &a) in values.iter().enumerate() {
                        let what = format!("({} {}) swp={on}", spec.name, POOL[i]);
                        check(&mut ctx, spec, &[a], false, &what);
                        checked += 1;
                    }
                }
                _ => {
                    for (i, &a) in values.iter().enumerate() {
                        for (j, &b) in values.iter().enumerate() {
                            let what = format!("({} {} {}) swp={on}", spec.name, POOL[i], POOL[j]);
                            check(&mut ctx, spec, &[a, b], false, &what);
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    set_swp(&mut ctx, false);
    assert!(checked > 10_000, "the matrix ran ({checked} cases)");
}

/// `gethash` over key × table × default, including a user-test table (the
/// declared bounce) and non-tables (the signal).
#[test]
fn gethash_leaf_matches_the_builtin() {
    let mut ctx = Context::new();
    setup(&mut ctx);
    let tables = pool(
        &mut ctx,
        &[
            "(let ((h (make-hash-table :test 'eq))) (puthash 7 'seven h) (puthash 'a 1 h) h)",
            "(let ((h (make-hash-table :test 'eql))) (puthash 1.5 'f h) (puthash (expt 2 70) 'big h) h)",
            "(let ((h (make-hash-table :test 'equal))) (puthash \"s\" 3 h) (puthash '(1 2) 'l h) h)",
            "(let ((h (make-hash-table :test 'leaf-ci))) (puthash \"s\" 4 h) h)",
            "(make-hash-table)",
            "nil",
            "5",
            "'(a . 1)",
        ],
    );
    let keys = pool(
        &mut ctx,
        &[
            "7",
            "'a",
            "\"s\"",
            "(copy-sequence \"s\")",
            "1.5",
            "(expt 2 70)",
            "'(1 2)",
            "nil",
            "(position-symbol 'a 3)",
        ],
    );
    let defaults = pool(&mut ctx, &["nil", "'dflt"]);
    let mut bounced = 0;
    for on in [false, true] {
        set_swp(&mut ctx, on);
        for (ti, &table) in tables.iter().enumerate() {
            let user = ti == 3;
            for &key in &keys {
                for &default in &defaults {
                    let what = format!("(gethash key table#{ti} ..) swp={on}");
                    if check(&mut ctx, &GETHASH, &[key, table, default], user, &what) {
                        bounced += 1;
                    }
                }
                // The two-argument call: DEFAULT arrives as nil.
                let what = format!("(gethash key table#{ti}) swp={on}");
                if check(&mut ctx, &GETHASH, &[key, table], user, &what) {
                    bounced += 1;
                }
            }
        }
    }
    set_swp(&mut ctx, false);
    assert!(bounced > 0, "the user-test bounce shape was exercised");
}

/// `plist-get` over plist × prop × predicate: a non-nil PREDICATE bounces.
#[test]
fn plist_get_leaf_matches_the_builtin() {
    let mut ctx = Context::new();
    setup(&mut ctx);
    let plists = pool(
        &mut ctx,
        &[
            "'(a 1 b 2 c 3)",
            "'(a 1 . b)",
            "'(a)",
            "'(1 x \"s\" y)",
            "nil",
            "5",
            "(let ((l (list 'a 1 'b 2))) (setcdr (cdr (cdr (cdr l))) l) l)",
        ],
    );
    let props = pool(
        &mut ctx,
        &[
            "'a",
            "'b",
            "'c",
            "'z",
            "1",
            "\"s\"",
            "(position-symbol 'b 2)",
        ],
    );
    let predicates = pool(&mut ctx, &["nil", "#'equal"]);
    let mut bounced = 0;
    for on in [false, true] {
        set_swp(&mut ctx, on);
        for (pi, &plist) in plists.iter().enumerate() {
            for &prop in &props {
                for (qi, &predicate) in predicates.iter().enumerate() {
                    let what = format!("(plist-get plist#{pi} prop pred#{qi}) swp={on}");
                    if check(
                        &mut ctx,
                        &PLIST_GET,
                        &[plist, prop, predicate],
                        qi == 1,
                        &what,
                    ) {
                        bounced += 1;
                    }
                }
            }
        }
    }
    set_swp(&mut ctx, false);
    assert!(bounced > 0, "the predicate bounce shape was exercised");
}

/// `get-char-property` over positions × props × objects: text properties,
/// an overlay, a string object, another buffer, and every position error.
#[test]
fn get_char_property_leaf_matches_the_builtin() {
    let mut ctx = Context::new();
    ctx.eval_str(
        "(progn
           (set-buffer (get-buffer-create \" leaf-gcp\"))
           (insert \"hello world\")
           (put-text-property 3 5 'p 'text)
           (overlay-put (make-overlay 4 7) 'p 'overlay)
           (overlay-put (make-overlay 8 9) 'q 'only-q)
           (save-current-buffer
             (set-buffer (get-buffer-create \" leaf-gcp-other\"))
             (insert \"xyz\") (put-text-property 1 2 'p 'other)))",
    )
    .expect("buffer setup");
    let positions = pool(
        &mut ctx,
        &[
            "1",
            "3",
            "4",
            "5",
            "8",
            "11",
            "12",
            "0",
            "100",
            "-1",
            "'x",
            "(point-marker)",
            "1.0",
        ],
    );
    let props = pool(&mut ctx, &["'p", "'q", "'r", "nil", "5"]);
    let objects = pool(
        &mut ctx,
        &[
            "nil",
            "(current-buffer)",
            "(get-buffer-create \" leaf-gcp-other\")",
            "(let ((s (copy-sequence \"abcd\"))) (put-text-property 1 3 'p 'str s) s)",
            "5",
        ],
    );
    for (xi, &pos) in positions.iter().enumerate() {
        for (pi, &prop) in props.iter().enumerate() {
            for (oi, &object) in objects.iter().enumerate() {
                let what = format!("(get-char-property pos#{xi} prop#{pi} obj#{oi})");
                check(
                    &mut ctx,
                    &GET_CHAR_PROPERTY,
                    &[pos, prop, object],
                    false,
                    &what,
                );
            }
            let what = format!("(get-char-property pos#{xi} prop#{pi})");
            check(&mut ctx, &GET_CHAR_PROPERTY, &[pos, prop], false, &what);
        }
    }
}

/// Every declared bounce shape is one the harness above exercises: a new
/// shape needs a case.
#[test]
fn every_generic_shape_is_exercised() {
    let exercised = [BounceShape::UserHashTest, BounceShape::PlistPredicate];
    for spec in LEAVES {
        for shape in spec.generic_when {
            assert!(
                exercised.contains(shape),
                "{}: {shape:?} has no harness case",
                spec.name
            );
        }
    }
}

/// `nth`'s fast half answers exactly as its contained body wherever it
/// answers, over counts 0..=130 and every list shape in the pool.
#[test]
fn nth_fast_half_agrees_with_its_body() {
    let mut ctx = Context::new();
    let values = pool(&mut ctx, POOL);
    let counts: Vec<Value> = (-2..=130).map(Value::fixnum).collect();
    let Containment::FastOutside(fast) = NTH.containment else {
        panic!("nth has a fast half");
    };
    for &list in &values {
        for &n in counts.iter().chain(&values) {
            if let Some(v) = fast(&ctx, &[n, list, Value::NIL, Value::NIL]) {
                let body = run_leaf(&ctx, &NTH, &[n, list]);
                assert!(
                    matches!(body, Outcome::Value(b) if b.bits() == v.bits()),
                    "(nth {} ..): fast {} vs body {}",
                    print_value(&n),
                    describe(&Outcome::Value(v)),
                    describe(&body)
                );
            }
        }
    }
}
