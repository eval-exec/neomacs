//! Real native calls and installed inline dependencies across owned eras.
#[path = "p3sym_functions_test/intern_growth.rs"]
mod intern_growth;
#[path = "p3sym_functions_test/receiver.rs"]
mod receiver;
#[path = "p3sym_functions_test/reclamation.rs"]
mod reclamation;

use super::{cache, compile, test_support};
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::error::FlowRef;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::value::{LambdaParams, Value};
use crate::tagged::gc::function_boundary_testkit::{FunctionBoundaryPoint, FunctionEraEdit};
use crate::tagged::transport::SharedRoot;

struct Profit(compile::ProfitGateTestPolicy);
impl Profit {
    fn bypass() -> Self {
        Self(compile::replace_profit_gate_for_test(
            compile::ProfitGateTestPolicy::Bypass,
        ))
    }
}
impl Drop for Profit {
    fn drop(&mut self) {
        compile::replace_profit_gate_for_test(self.0);
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = StackDepth::for_test(4);
    f.jit_runtime().set_hot_for_test();
    Value::make_bytecode(f)
}
fn fixture(prefix: &str) -> (Context, [SymId; 2], [Value; 2], [Value; 2], Value) {
    let mut ctx = Context::new_minimal_vm_harness();
    // Established native-only fixture cleanup, before test roots/definitions.
    ctx.set_variable("window-dead-windows-table", Value::NIL);
    ctx.gc_collect_exact();
    let ids = [
        intern(&format!("{prefix}-leaf")),
        intern(&format!("{prefix}-other")),
    ];
    let targets = ids.map(|id| {
        let target = function(vec![Op::Constant(0), Op::Return], vec![Value::fixnum(73)]);
        ctx.obarray.set_symbol_function_id(id, target);
        target
    });
    let callers = ids.map(|id| {
        function(
            vec![Op::Constant(0), Op::Call(0), Op::Return],
            vec![Value::from_sym_id(id)],
        )
    });
    let replacement = Value::list(vec![Value::symbol("lambda"), Value::NIL, Value::fixnum(97)]);
    (ctx, ids, targets, callers, replacement)
}
fn input(ctx: &mut Context, value: Value) -> SharedRoot {
    // SAFETY: sole serial owning Context and its actual heap, before the era.
    unsafe { SharedRoot::new(&mut ctx.tagged_heap, value) }
}
fn native(ctx: &mut Context, source: Value) -> Result<usize, crate::emacs_core::error::Flow> {
    let f = source.get_bytecode_data().expect("retained serial source");
    test_support::prepare_entry(ctx, f);
    assert!(
        cache::is_compiled_for_test(f.jit_runtime().compiled_id_or_assign()),
        "actual native cache entry required"
    );
    match test_support::run_compiled(ctx, f, source, &[]) {
        Ok(Some(bits)) => Ok(bits),
        Ok(None) => panic!("required native entry was declined"),
        Err(flow) => Err(flow),
    }
}
fn printed(bits: usize) -> String {
    crate::emacs_core::print_value(&Value::from_bits(bits))
}
fn direct(ctx: &mut Context, function: Value) -> String {
    crate::emacs_core::print_value(&ctx.apply0(function).expect("serial GNU twin call"))
}
fn oracle() -> Vec<&'static str> {
    let rows: Vec<_> = include_str!(
        "../../../../tagged/gc/mutator_registration/symbol_function_cache_oracle.expected"
    )
    .lines()
    .collect();
    assert_eq!(rows.len(), 6);
    rows
}
fn coherent(ctx: &Context, clock: u64) {
    assert_eq!(ctx.obarray().function_epoch(), clock);
    assert_eq!(ctx.dispatch_projection().function_epoch(), clock);
    assert!(ctx.obarray.admission_matches_canonical());
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn p3sym_c4_jit_calls_across_owned_era() {
    let _profit = Profit::bypass();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let (mut ctx, ids, targets, callers, replacement) = fixture("p3sym-c4-native");
    let _roots = ctx.retain_gc_roots(targets.into_iter().chain(callers).chain([replacement]));
    let expected = oracle();
    assert_eq!(
        format!(
            "({} {})",
            printed(native(&mut ctx, callers[0]).unwrap()),
            direct(&mut ctx, Value::from_sym_id(ids[0]))
        ),
        expected[0]
    );
    for (edit, delta, row) in [
        (FunctionEraEdit::Same { symbol: 0 }, 0, 1),
        (FunctionEraEdit::Replace { symbol: 1 }, 1, 2),
        (FunctionEraEdit::Replace { symbol: 0 }, 1, 3),
        (FunctionEraEdit::Unbind { symbol: 0 }, 1, 4),
        (FunctionEraEdit::Replace { symbol: 0 }, 1, 5),
    ] {
        test_support::settle(&mut ctx);
        let clock = ctx.dispatch_projection().function_epoch();
        let source = input(&mut ctx, replacement);
        let receipt = ctx.c4_function_era_for_test(ids, edit, source, |_| {});
        coherent(&ctx, clock + delta);
        assert_eq!(receipt.before.facts[0].0, clock);
        assert_eq!(receipt.after.facts[0].0, clock + delta);
        let first = native(&mut ctx, callers[0]);
        let actual = match row {
            1 => format!(
                "({} {} {})",
                crate::emacs_core::print_value(&Value::bool_val(
                    receipt.writer_answer == targets[0].bits()
                )),
                printed(first.unwrap()),
                direct(&mut ctx, Value::from_sym_id(ids[0]))
            ),
            2 => format!(
                "({} {})",
                printed(first.unwrap()),
                direct(&mut ctx, Value::from_sym_id(ids[1]))
            ),
            3 => format!(
                "({} {} {})",
                direct(&mut ctx, targets[0]),
                printed(first.unwrap()),
                direct(&mut ctx, Value::from_sym_id(ids[0]))
            ),
            4 => {
                let error = first.expect_err("GNU unbound native call");
                let condition = match error.kind() {
                    FlowRef::Signal(data) => data.symbol_name().to_string(),
                    other => panic!("expected GNU error condition: {other:?}"),
                };
                drop(error);
                format!("({condition} {})", direct(&mut ctx, targets[0]))
            }
            5 => format!(
                "({} {})",
                printed(first.unwrap()),
                direct(&mut ctx, Value::from_sym_id(ids[0]))
            ),
            _ => unreachable!("fixture row"),
        };
        assert_eq!(
            actual, expected[row],
            "first native call after era row{row}"
        );
        coherent(&ctx, clock + delta);
        eprintln!(
            "P3SYM_C4_NATIVE_CACHE_ROW row={row} clock={} actual={actual}",
            clock + delta
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CacheSnapshot {
    // Opaque identities only: never borrow/dereference a possibly evicted leaf.
    leaves: [Option<usize>; 2],
    dependents: [usize; 2],
}
fn snapshot(ids: [u64; 2], symbols: [SymId; 2]) -> CacheSnapshot {
    CacheSnapshot {
        leaves: ids.map(|id| cache::compiled_leaf_ptr_for_test(id).map(|ptr| ptr as usize)),
        dependents: symbols.map(cache::inline_dependent_count_for_test),
    }
}

#[test]
fn p3sym_c4_jit_inline_dependents_reconcile_at_stop() {
    if !super::inline::jit_inline_on() {
        eprintln!("P3SYM_C4_INLINE_INAPPLICABLE NEOVM_JIT_INLINE=off");
        return;
    }
    let _profit = Profit::bypass();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
    let (mut ctx, symbols, targets, callers, replacement) = fixture("p3sym-c4-inline");
    let _roots = ctx.retain_gc_roots(targets.into_iter().chain(callers).chain([replacement]));
    let expected = oracle();
    for caller in callers {
        native(&mut ctx, caller).unwrap();
    }
    let ids = callers.map(|value| {
        value
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .compiled_id_or_assign()
    });
    let initial = snapshot(ids, symbols);
    assert!(
        initial.leaves.iter().all(Option::is_some),
        "both real callers compiled"
    );
    assert_eq!(
        initial.dependents,
        [1, 1],
        "both real named inline dependencies installed"
    );
    for (edit, row) in [
        (FunctionEraEdit::Same { symbol: 0 }, 1),
        (FunctionEraEdit::Replace { symbol: 1 }, 2),
        (FunctionEraEdit::Replace { symbol: 0 }, 3),
    ] {
        test_support::settle(&mut ctx);
        let before = snapshot(ids, symbols);
        let clock = ctx.dispatch_projection().function_epoch();
        let mut observations = Vec::new();
        let source = input(&mut ctx, replacement);
        // Only integer IDs/symbols and an output ledger enter the callback.
        // It queries existing keeper TLS metadata, with no Context or entry.
        let receipt = ctx.c4_function_era_for_test(symbols, edit, source, |point| {
            let after = matches!(point, FunctionBoundaryPoint::AfterReconcile);
            observations.push((after, snapshot(ids, symbols)));
        });
        let delta = u64::from(row != 1);
        coherent(&ctx, clock + delta);
        assert_eq!(receipt.before.facts[0].0, clock);
        assert_eq!(receipt.after.facts[0].0, clock + delta);
        assert_eq!(observations.len(), 2);
        assert_eq!(
            observations[0],
            (false, before.clone()),
            "no eager eviction before stopped reconciliation"
        );
        let mut retired = before;
        if row != 1 {
            let affected = if row == 2 { 1 } else { 0 };
            retired.leaves[affected] = None;
            retired.dependents[affected] = 0;
        }
        assert_eq!(
            observations[1],
            (true, retired.clone()),
            "only affected inline caller evicted at reconciliation"
        );
        assert_eq!(snapshot(ids, symbols), retired);
        let first = native(&mut ctx, callers[0]).unwrap();
        let actual = match row {
            1 => format!(
                "({} {} {})",
                crate::emacs_core::print_value(&Value::bool_val(
                    receipt.writer_answer == targets[0].bits()
                )),
                printed(first),
                direct(&mut ctx, Value::from_sym_id(symbols[0]))
            ),
            2 => format!(
                "({} {})",
                printed(first),
                direct(&mut ctx, Value::from_sym_id(symbols[1]))
            ),
            3 => format!(
                "({} {} {})",
                direct(&mut ctx, targets[0]),
                printed(first),
                direct(&mut ctx, Value::from_sym_id(symbols[0]))
            ),
            _ => unreachable!("fixture row"),
        };
        assert_eq!(actual, expected[row]);
        if row < 3 {
            assert_eq!(
                snapshot(ids, symbols).leaves[0],
                initial.leaves[0],
                "unaffected first caller remains the same native leaf after entry"
            );
        } else {
            assert!(
                cache::is_compiled_for_test(ids[0]),
                "first changed call used a newly prepared actual native entry"
            );
            assert_eq!(
                cache::inline_dependent_count_for_test(symbols[0]),
                0,
                "replacement lambda is called without stale inline dependency"
            );
        }
        coherent(&ctx, clock + delta);
        eprintln!(
            "P3SYM_C4_INLINE_RECONCILE_ROW row={row} clock={} deps={:?} actual={actual}",
            clock + delta,
            observations[1].1.dependents
        );
    }
}
