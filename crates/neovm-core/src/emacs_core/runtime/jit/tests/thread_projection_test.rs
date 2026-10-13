//! Phase 0 step 2: compiled variable reads refuse a thread-projected cell
//! with the guards they already have, and reach the unchanged `varref` shim.
//!
//! - Compiled before projection, an inline forwarded read's baked-descriptor
//!   guard (`emit_varref_fast`) and an inline localized read's redirect
//!   guard fail; the plain read (`emit_symbol_cell_read`, shared with
//!   `Op::VarRef`'s non-inline lowering) fails on the redirect.
//! - Compiled after projection, `FwdKind::of` classifies the site `Plain`,
//!   whose redirect guard refuses it at run time.
//!
//! Either way the shim answers as the VM does (void until owner-aware slow
//! paths exist), and never with the value the cell held.

use super::inline_vars::with_compile_env_for_test;
use super::shims::VARREF_SHIM_CALLS;
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::forward::alloc_objfwd;
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

const OPS: [Op; 2] = [Op::VarRef(0), Op::Return];

/// Each shape a projection can wrap, with the value it reads before.
const SHAPES: &[(&str, &str)] = &[
    ("tpj-plain", "4242"),
    ("tpj-obj", "4343"),
    ("tpj-int", "4444"),
    ("tpj-bool", "t"),
    ("tpj-local", "4546"),
];

fn fixture() -> Context {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    let mut ev = Context::new();
    crate::emacs_core::jit::cache::clear();
    ev.obarray.intern("tpj-obj");
    ev.obarray
        .install_objfwd(intern("tpj-obj"), alloc_objfwd(Value::fixnum(4343)));
    ev.obarray.define_int_variable("tpj-int", 4444);
    ev.obarray
        .define_bool_variable("tpj-bool", true, ByteBooleanVars::ErasedByLreadInit);
    ev.eval_str(
        "(progn
           (defvar tpj-plain 4242)
           (defvar tpj-local 4545)
           (make-local-variable 'tpj-local)
           (setq tpj-local 4546))",
    )
    .expect("fixture evaluates");
    ev
}

fn function(name: &str) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = OPS.to_vec();
    f.constants = vec![Value::symbol(name)].into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(4);
    f
}

fn answer(result: Result<Value, crate::emacs_core::error::Flow>) -> String {
    match result {
        Ok(value) => print_value(&value),
        Err(flow) => match flow.into_kind() {
            crate::emacs_core::error::FlowKind::Signal(sig) => sig.symbol_name().to_string(),
            other => format!("{other:?}"),
        },
    }
}

fn interpret(ev: &mut Context, name: &str) -> String {
    let f = function(name);
    let mut vm = Vm::from_context(ev);
    answer(vm.execute(&f, vec![]))
}

/// NAME's reader compiled against EV with every inline variable path on.
fn compile_inline(ev: &Context, name: &str) -> CompiledLeaf {
    force_inline_vars_for_test(Some(InlineVarsKnob::ALL));
    let constants = [Value::symbol(name)];
    let leaf = with_compile_env_for_test(ev, || lower_leaf(&OPS, &constants, 0));
    force_inline_vars_for_test(None);
    leaf.expect("reader lowers")
}

/// Run LEAF: `(answer, varref shim calls)`.
fn run(ev: &mut Context, leaf: &CompiledLeaf) -> (String, usize) {
    VARREF_SHIM_CALLS.with(|c| c.set(0));
    let ctx = ev as *mut Context as *mut u8;
    let got = match leaf.call(ctx, &[]) {
        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
        NativeRun::Signal => answer(Err(take_pending_flow().expect("flow stashed"))),
        other => panic!("must not leave native code: {other:?}"),
    };
    (got, VARREF_SHIM_CALLS.with(|c| c.get()))
}

fn project(ev: &mut Context, name: &str) {
    ev.project_thread_variable_for_test(intern(name))
        .unwrap_or_else(|| panic!("{name} projects"));
}

#[test]
fn inline_reads_compiled_before_projection_refuse_it() {
    let mut ev = fixture();
    for &(name, before) in SHAPES {
        // Load a buffer-local's cache for this buffer, as a first read does.
        assert_eq!(interpret(&mut ev, name), before, "{name}");
        let leaf = compile_inline(&ev, name);
        let (got, shim_calls) = run(&mut ev, &leaf);
        assert_eq!(
            (got.as_str(), shim_calls),
            (before, 0),
            "{name}: inline read"
        );

        project(&mut ev, name);
        let want = interpret(&mut ev, name);
        assert_eq!(want, "void-variable", "{name}: VM refuses");
        let (got, shim_calls) = run(&mut ev, &leaf);
        assert_eq!(got, want, "{name}: native answers as the VM");
        assert_eq!(shim_calls, 1, "{name}: the guard refused, the shim read");
    }
}

#[test]
fn reads_compiled_after_projection_refuse_it() {
    let mut ev = fixture();
    for &(name, _) in SHAPES {
        project(&mut ev, name);
        let want = interpret(&mut ev, name);
        assert_eq!(want, "void-variable", "{name}: VM refuses");
        // Inline knob on: classified `Plain` (no inline forwarded read).
        let leaf = compile_inline(&ev, name);
        let (got, shim_calls) = run(&mut ev, &leaf);
        assert_eq!(
            (got.as_str(), shim_calls),
            (want.as_str(), 1),
            "{name} inline"
        );
        // Default lowering: the plain symbol-cell read.
        let leaf = compile_bytecode_function(&function(name)).expect("compiles");
        let (got, shim_calls) = run(&mut ev, &leaf);
        assert_eq!(
            (got.as_str(), shim_calls),
            (want.as_str(), 1),
            "{name} plain"
        );
    }
}
