use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::subr_abi::SubrFn;
use crate::emacs_core::print::print_value;
use neovm_value::error::Flow;
use neovm_value::value::LambdaParams;

fn params(n: usize) -> LambdaParams {
    LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: (1..=n as u32)
            .map(|id| unsafe { SymId::from_raw(id) })
            .collect(),
        optional: Vec::new(),
        rest: None,
    }
}

fn opcode_fn(op: Op, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(params(nargs));
    f.lexical = true;
    f.ops = match nargs {
        1 => vec![Op::StackRef(0), op, Op::Return],
        _ => vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
    };
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    f
}

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
        LeafId::SymbolValue => Op::SymbolValue,
        LeafId::Gethash
        | LeafId::PlistGet
        | LeafId::GetCharProperty
        | LeafId::BufferLocalValue
        | LeafId::Assoc
        | LeafId::Rassq
        | LeafId::Delq
        | LeafId::CopySequence
        | LeafId::SymbolName
        | LeafId::Boundp
        | LeafId::Keywordp => return None,
    })
}

#[derive(Debug)]
enum Outcome {
    Value(Value),
    Signal(SymId, Vec<Value>),
    Generic,
}

fn outcome_of_flow(flow: Flow) -> Outcome {
    match flow.as_signal() {
        Some(sig) => Outcome::Signal(sig.symbol, sig.data.clone()),
        None => panic!("a leaf reference exited with {flow:?}"),
    }
}

fn run_leaf(ctx: &Context, spec: &'static LeafSpec, args: &[Value]) -> Outcome {
    match crate::emacs_core::subr::leaf::call_checked(spec, ctx, args) {
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
            && neovm_value::value::try_equal_value_swp(&a, &b, 0, false).unwrap_or(false))
}

fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Value(v) if v.is_cons() => format!("value #x{:x}", v.bits()),
        Outcome::Value(v) => format!("value {}", print_value(v)),
        Outcome::Signal(sym, data) => format!(
            "signal {} ({} data)",
            neovm_value::intern::resolve_sym(*sym),
            data.len()
        ),
        Outcome::Generic => "generic".to_string(),
    }
}

fn check(
    ctx: &mut Context,
    spec: &'static LeafSpec,
    args: &[Value],
    bounce: bool,
    what: &str,
) -> bool {
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
    // An allocating leaf answers a fresh object: compare those by `equal`.
    let fresh = spec.effects.contains(Effects::ALLOCATES);
    let agree = match (&got, &want) {
        (Outcome::Value(a), Outcome::Value(b)) if fresh => {
            same_value(*a, *b)
                || neovm_value::value::try_equal_value_swp(a, b, 0, false).unwrap_or(false)
        }
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

fn pool(ctx: &mut Context, sources: &[&str]) -> Vec<Value> {
    let list = ctx
        .eval_str(&format!("(list {})", sources.join(" ")))
        .expect("pool evaluates");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    neovm_value::value::list_to_vec(&list).expect("a proper list")
}

const VARIABLES: &[&str] = &[
    "'leaf-plain",
    "'leaf-nil",
    "'leaf-void",
    "'leaf-local",
    "'leaf-lset",
    "'leaf-alias",
    "'fill-column",
    "'case-fold-search",
    "'tab-width",
    "'gc-cons-threshold",
    "'debug-on-error",
    "'load-path",
    "'last-command",
    "'prefix-arg",
    "'buffer-undo-list",
    "'buffer-file-name",
    "nil",
    "t",
    ":kw",
    "(position-symbol 'leaf-plain 3)",
    "5",
    "\"leaf-plain\"",
];

fn variable_setup(ctx: &mut Context) {
    ctx.eval_str(
        "(progn
           (defvar leaf-plain 'plain)
           (defvar leaf-nil nil)
           (defvar leaf-local 'local-default)
           (make-variable-buffer-local 'leaf-local)
           (defvar leaf-lset 'lset-global)
           (defvaralias 'leaf-alias 'leaf-plain)
           (get-buffer-create \" leaf-vars-b\")
           (set-buffer (get-buffer-create \" leaf-vars-a\"))
           (setq leaf-local 'a-local)
           (set (make-local-variable 'leaf-lset) 'a-lset)
           (setq fill-column 33)
           (setq buffer-undo-list t)
           (setq leaf-local leaf-local leaf-lset leaf-lset))",
    )
    .expect("variable setup");
}

/// `buffer-local-value` over variable × buffer, from buffers `a` and `b`,
/// against the registered builtin.
pub(crate) fn run_buffer_local_value_leaf_matches_the_builtin_on_every_variable_shape_fixture(
    make_context: fn() -> Context,
) {
    use crate::emacs_core::eval::{VarCacheEvent, var_cache_event_count};
    let mut ctx = make_context();
    variable_setup(&mut ctx);
    let variables = pool(&mut ctx, VARIABLES);
    let buffers = pool(
        &mut ctx,
        &[
            "(get-buffer \" leaf-vars-a\")",
            "(get-buffer \" leaf-vars-b\")",
            "(let ((b (get-buffer-create \" leaf-vars-dead\"))) (kill-buffer b) b)",
            "nil",
            "5",
        ],
    );
    let hits0 = var_cache_event_count(VarCacheEvent::ReadLocalized);
    for current in [" leaf-vars-a", " leaf-vars-b"] {
        ctx.eval_str(&format!("(set-buffer {current:?})"))
            .expect("set-buffer");
        for (i, &v) in variables.iter().enumerate() {
            for (j, &b) in buffers.iter().enumerate() {
                let what = format!(
                    "(buffer-local-value {} buffer#{j}) in {current:?}",
                    VARIABLES[i]
                );
                check(&mut ctx, &BUFFER_LOCAL_VALUE, &[v, b], false, &what);
            }
        }
    }
    assert!(
        var_cache_event_count(VarCacheEvent::ReadLocalized) > hits0,
        "the leaf's cached tier answered some reads"
    );
}
