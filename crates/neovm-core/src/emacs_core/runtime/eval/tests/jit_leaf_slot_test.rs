#![cfg(feature = "jit")]
//! Slice C1 of the JIT call seam: once the tier-up entry (`try_run_compiled`)
//! has run an inline-dep-free leaf, the interpreter's `Bcall` arm enters it
//! DIRECTLY through `RuntimeState::leaf_slot` — no cache probe, no argument
//! marshaling. The slot reads as empty after every cache retire/clear (the
//! `cache::leaf_slot_epoch()` guard) and for any call that is not exact-arity.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::{LambdaParams, Value, ValueKind};

fn bc(
    required: u32,
    optional: u32,
    ops: Vec<Op>,
    constants: Vec<Value>,
    hot: bool,
) -> ByteCodeFunction {
    bc_with(required, optional, false, ops, constants, hot)
}

fn bc_with(
    required: u32,
    optional: u32,
    rest: bool,
    ops: Vec<Op>,
    constants: Vec<Value>,
    hot: bool,
) -> ByteCodeFunction {
    let nonrest = required + optional;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=required).map(SymId).collect(),
        optional: (required + 1..=nonrest).map(SymId).collect(),
        rest: rest.then_some(SymId(nonrest + 1)),
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    f
}

/// Bind `name`'s function cell to `bytecode` (roots it in the obarray) and
/// return the callable symbol and the bytecode object.
fn bind_fn(ev: &mut Context, name: &str, bytecode: ByteCodeFunction) -> (Value, Value) {
    let sym = Value::symbol(name);
    let ValueKind::Symbol(id) = sym.kind() else {
        panic!("symbol")
    };
    let obj = Value::make_bytecode(bytecode);
    ev.obarray.set_symbol_function_id(id, obj);
    (sym, obj)
}

/// `(lambda (a b) (+ a b))`, hot: exact arity 2, nothing inlined.
fn adder() -> ByteCodeFunction {
    bc(
        2,
        0,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
        vec![],
        true,
    )
}

/// `(lambda () (CALLEE A B))`, COLD: its `Bcall` runs in the interpreter, so
/// the call takes `dispatch_bytecode_call_from_stack`, not the JIT's seam.
fn caller_of(callee: Value, a: i64, b: i64) -> Value {
    Value::make_bytecode(bc(
        0,
        0,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![callee, Value::make_int(a), Value::make_int(b)],
        false,
    ))
}

fn call0(ev: &mut Context, f: Value) -> Value {
    ev.funcall_general_untraced(f, Vec::<Value>::new())
        .expect("call completes")
}

fn slot_armed(callee: Value) -> bool {
    let func = callee.get_bytecode_data().expect("bytecode object");
    func.jit_runtime()
        .armed_leaf_slot(cache::leaf_slot_epoch())
        .is_some()
}

fn compiled_id(callee: Value) -> u64 {
    callee
        .get_bytecode_data()
        .expect("bytecode object")
        .jit_runtime()
        .compiled_id()
        .expect("assigned by the first tier-up entry")
}

#[test]
fn direct_stack_entry_arms_after_the_tier_up_entry_and_agrees_with_it() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let (sym, callee) = bind_fn(&mut ev, "c1-leaf-slot-adder", adder());
    let caller = caller_of(sym, 20, 22);
    assert!(
        !slot_armed(callee),
        "nothing is armed before the first call"
    );
    assert_eq!(call0(&mut ev, caller), Value::make_int(42));
    assert_eq!(
        cache::cache_entry_kind_for_test(compiled_id(callee)),
        "compiled",
        "premise: the leaf compiled"
    );
    assert!(
        slot_armed(callee),
        "the tier-up entry arms the slot from the cache"
    );
    let heat = callee.get_bytecode_data().unwrap().jit_runtime().heat();
    for _ in 0..8 {
        assert_eq!(call0(&mut ev, caller), Value::make_int(42), "direct entry");
    }
    assert_eq!(
        callee.get_bytecode_data().unwrap().jit_runtime().heat(),
        heat + 8,
        "the direct entry advances the heat like the dispatcher (re-tier trigger)"
    );
}

#[test]
fn retire_and_clear_disarm_the_slot_and_the_next_call_re_arms_it() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let (sym, callee) = bind_fn(&mut ev, "c1-leaf-slot-retired", adder());
    let caller = caller_of(sym, 1, 2);
    assert_eq!(call0(&mut ev, caller), Value::make_int(3));
    assert!(slot_armed(callee));
    // The retire path (`note_patched_prefix`, re-tier): the leaf stays
    // allocated but must never be CALLED through the slot again.
    cache::evict_compiled(compiled_id(callee));
    assert!(!slot_armed(callee), "a retired leaf is not armed");
    assert_eq!(
        call0(&mut ev, caller),
        Value::make_int(3),
        "tier-up entry recompiles"
    );
    assert!(slot_armed(callee), "re-armed from the fresh cache entry");
    // `clear()` FREES leaves (image reload): every slot must read empty.
    cache::clear();
    assert!(!slot_armed(callee), "clear() disarms every slot");
    assert_eq!(call0(&mut ev, caller), Value::make_int(3));
    assert!(slot_armed(callee));
}

/// `(lambda (a &optional b) b)`: arity 2 with one optional. The direct entry
/// nil-pads an omitted optional exactly as the tier-up entry's `call_consts`
/// does, so a 1-arg and a 2-arg call agree, interleaved, from the same slot.
#[test]
fn omitted_optionals_are_nil_padded_on_the_direct_entry() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let (sym, callee) = bind_fn(
        &mut ev,
        "c1-leaf-slot-optional",
        bc(1, 1, vec![Op::StackRef(0), Op::Return], vec![], true),
    );
    let two = caller_of(sym, 5, 7);
    let one = Value::make_bytecode(bc(
        0,
        0,
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![sym, Value::make_int(5)],
        false,
    ));
    assert_eq!(call0(&mut ev, two), Value::make_int(7));
    assert_eq!(
        cache::cache_entry_kind_for_test(compiled_id(callee)),
        "compiled",
        "premise: an &optional leaf compiles"
    );
    assert!(slot_armed(callee));
    for _ in 0..3 {
        assert_eq!(
            call0(&mut ev, one),
            Value::NIL,
            "1-arg call: nil-padded by the entry"
        );
        assert_eq!(
            call0(&mut ev, two),
            Value::make_int(7),
            "2-arg call: direct"
        );
    }
}

/// `(lambda (a &rest r) r)`: the direct entry conses the tail into the rest
/// list exactly as the tier-up entry's `call_consts` does — nil for none,
/// a fresh list otherwise — from the same slot, interleaved arities.
#[test]
fn rest_callees_take_the_direct_entry_with_a_consed_list() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let (sym, callee) = bind_fn(
        &mut ev,
        "c1-leaf-slot-rest",
        bc_with(1, 0, true, vec![Op::StackRef(0), Op::Return], vec![], true),
    );
    let call_n = |n: usize| {
        let mut ops = vec![Op::Constant(0)];
        let mut constants = vec![sym];
        for i in 0..n {
            ops.push(Op::Constant(1 + i as u16));
            constants.push(Value::make_int(5 + 2 * i as i64));
        }
        ops.push(Op::Call(n as u16));
        ops.push(Op::Return);
        Value::make_bytecode(bc(0, 0, ops, constants, false))
    };
    let (one, two, three) = (call_n(1), call_n(2), call_n(3));
    assert_eq!(
        call0(&mut ev, two),
        Value::list_from_slice(&[Value::make_int(7)])
    );
    assert_eq!(
        cache::cache_entry_kind_for_test(compiled_id(callee)),
        "compiled",
        "premise: a &rest leaf compiles"
    );
    assert!(slot_armed(callee));
    for _ in 0..3 {
        assert_eq!(call0(&mut ev, one), Value::NIL, "no rest args: nil");
        assert_eq!(
            call0(&mut ev, two),
            Value::list_from_slice(&[Value::make_int(7)])
        );
        assert_eq!(
            call0(&mut ev, three),
            Value::list_from_slice(&[Value::make_int(7), Value::make_int(9)])
        );
    }
}
