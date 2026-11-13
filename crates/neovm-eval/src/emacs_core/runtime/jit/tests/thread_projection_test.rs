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
//! Either way the shim answers as the VM does: the canonical value, read by
//! the VM's projection-aware slow path, never through a pointer native code
//! cached into the cell.

use super::inline_vars::with_compile_env_for_test;
use super::shims::VARREF_SHIM_CALLS;
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::forward::alloc_objfwd;
use neovm_value::intern::intern;
use neovm_value::value::LambdaParams;

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
    let mut ev = Context::new(crate::test_utils::unit_runtime());
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

fn answer(result: Result<Value, neovm_value::error::Flow>) -> String {
    match result {
        Ok(value) => print_value(&value),
        Err(flow) => match flow.into_kind() {
            neovm_value::error::FlowKind::Signal(sig) => sig.symbol_name().to_string(),
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
        assert_eq!(want, before, "{name}: VM reads the canonical value");
        let (got, shim_calls) = run(&mut ev, &leaf);
        assert_eq!(got, want, "{name}: native answers as the VM");
        assert_eq!(shim_calls, 1, "{name}: the guard refused, the shim read");
    }
}

#[test]
fn reads_compiled_after_projection_refuse_it() {
    let mut ev = fixture();
    for &(name, before) in SHAPES {
        project(&mut ev, name);
        let want = interpret(&mut ev, name);
        assert_eq!(want, before, "{name}: VM reads the canonical value");
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

/// `(list (let ((V 50)) (setq V 51) V) V)` through `varbind`, `varset`,
/// `varref` and `unbind`.
const WRITE_OPS: [Op; 9] = [
    Op::Constant(1),
    Op::VarBind(0),
    Op::Constant(2),
    Op::VarSet(0),
    Op::VarRef(0),
    Op::Unbind(1),
    Op::VarRef(0),
    Op::List(2),
    Op::Return,
];

fn write_constants(name: &str) -> [Value; 3] {
    [Value::symbol(name), Value::fixnum(50), Value::fixnum(51)]
}

fn write_function(name: &str) -> ByteCodeFunction {
    let mut f = function(name);
    f.ops = WRITE_OPS.to_vec();
    f.constants = write_constants(name).to_vec().into();
    f
}

fn compile_writer_inline(ev: &Context, name: &str) -> CompiledLeaf {
    force_inline_vars_for_test(Some(InlineVarsKnob::ALL));
    let constants = write_constants(name);
    let leaf = with_compile_env_for_test(ev, || lower_leaf(&WRITE_OPS, &constants, 0));
    force_inline_vars_for_test(None);
    leaf.expect("writer lowers")
}

/// Run LEAF: `(answer, varbind + varset + unbind shim calls)`.
fn run_writer(ev: &mut Context, leaf: &CompiledLeaf) -> (String, usize) {
    use super::shims::{UNBIND_SHIM_CALLS, VARBIND_SHIM_CALLS, VARSET_SHIM_CALLS};
    let calls = || {
        VARBIND_SHIM_CALLS.with(|c| c.get())
            + VARSET_SHIM_CALLS.with(|c| c.get())
            + UNBIND_SHIM_CALLS.with(|c| c.get())
    };
    let start = calls();
    let (got, _) = run(ev, leaf);
    (got, calls() - start)
}

/// Compiled binds and sets, lowered before or after projection, reach the
/// shims (their guards refuse the projected cell) and through them the
/// variable's canonical storage: the answer and the value left behind are
/// the unprojected ones. The localized shape is left out: its `let` over a
/// buffer-local binding is the round-3 `LetLocal` case.
#[test]
fn writes_compiled_before_and_after_projection_reach_canonical_storage() {
    let mut ev = fixture();
    for &(name, before) in SHAPES.iter().filter(|(name, _)| *name != "tpj-local") {
        let want = {
            let mut vm = Vm::from_context(&mut ev);
            answer(vm.execute(&write_function(name), vec![]))
        };
        let compiled_before = compile_writer_inline(&ev, name);
        let (got, _) = run_writer(&mut ev, &compiled_before);
        assert_eq!(got, want, "{name}: native before projection");

        project(&mut ev, name);
        let vm_after = {
            let mut vm = Vm::from_context(&mut ev);
            answer(vm.execute(&write_function(name), vec![]))
        };
        assert_eq!(vm_after, want, "{name}: VM after projection");
        let (got, shim_calls) = run_writer(&mut ev, &compiled_before);
        assert_eq!(got, want, "{name}: compiled before projection");
        assert!(shim_calls >= 3, "{name}: the guards refused ({shim_calls})");
        let compiled_after = compile_writer_inline(&ev, name);
        let (got, shim_calls) = run_writer(&mut ev, &compiled_after);
        assert_eq!(got, want, "{name}: compiled after projection");
        assert!(shim_calls >= 3, "{name}: the guards refused ({shim_calls})");
        let default = compile_bytecode_function(&write_function(name)).expect("compiles");
        let (got, _) = run_writer(&mut ev, &default);
        assert_eq!(got, want, "{name}: default lowering");
        assert_eq!(interpret(&mut ev, name), before, "{name}: restored");
    }
}

/// A compiled `setq` of a projected variable stores its canonical value,
/// whichever side of the projection it was lowered on.
#[test]
fn setq_compiled_across_projection_stores_canonically() {
    const OPS: [Op; 4] = [Op::Constant(1), Op::VarSet(0), Op::VarRef(0), Op::Return];
    let compile = |ev: &Context, name: &str| {
        force_inline_vars_for_test(Some(InlineVarsKnob::ALL));
        let constants = [Value::symbol(name), Value::fixnum(7)];
        let leaf = with_compile_env_for_test(ev, || lower_leaf(&OPS, &constants, 0));
        force_inline_vars_for_test(None);
        leaf.expect("setter lowers")
    };
    let mut ev = fixture();
    for &(name, _) in SHAPES {
        let compiled_before = compile(&ev, name);
        project(&mut ev, name);
        let want = if name == "tpj-bool" { "t" } else { "7" };
        let (got, _) = run(&mut ev, &compiled_before);
        assert_eq!(got, want, "{name}: compiled before projection");
        assert_eq!(interpret(&mut ev, name), want, "{name}: canonical");
        let compiled_after = compile(&ev, name);
        let (got, _) = run(&mut ev, &compiled_after);
        assert_eq!(got, want, "{name}: compiled after projection");
    }
}
