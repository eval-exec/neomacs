//! Existing constant bytecode is an opaque rooted replacement target only.
use super::*;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::tagged::bytecode::chunk::ByteCodeFunction;
use crate::tagged::bytecode::function_slots::StackDepth;
use crate::tagged::bytecode::opcode::Op;
use crate::value::LambdaParams;

fn constant_function() -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.lexical = true;
    function.ops = vec![Op::Constant(0), Op::Return];
    function.constants = vec![Value::fixnum(73)].into();
    function.max_stack = StackDepth::for_test(4);
    function
}
fn constant_bytecode() -> Value {
    Value::make_bytecode(constant_function())
}

#[test]
fn p3sym_c4_initial_constant_bytecode_admission() {
    crate::test_utils::ensure_eval_hooks_installed();
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        let prepared = neovm_eval::test_support::authority::with_test_symbol_functions_init_era(
            kind,
            |obarray, ids| obarray.set_symbol_function_id(ids[0], constant_bytecode()),
            |era, ids| {
                let handle = era.admit_plain_symbol(ids[0]).unwrap();
                let prepared = era.prepare_function_symbol(&handle);
                let (collector, keeper) = era.finish().expect("admission fixture teardown");
                drop(collector);
                drop(keeper);
                eprintln!("P3SYM_C4_BYTECODE_ADMISSION_TEARDOWN_OK {kind:?}");
                prepared
            },
        );
        assert_eq!(
            prepared,
            Ok(()),
            "existing constant-bytecode admission missing"
        );
    }
}

#[test]
fn p3sym_c4_initial_bytecode_exact_noop() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture_initialized(
            kind,
            true,
            |obarray, ids| {
                let definition = constant_bytecode();
                obarray.seed_value_id(ids[0], definition);
                obarray.set_symbol_function_id(ids[0], definition);
            },
            |run, handles, homes| {
                let before = run.function_facts_for_test(&handles[0]).unwrap();
                assert_eq!(
                    run.sym_fset_into(&handles[0], &homes.old, &mut homes.dst),
                    Ok(()),
                    "existing bytecode exact fset no-op missing"
                );
                assert_eq!(homes.dst.word_for_test(run), homes.old.word_for_test(run));
                assert_eq!(run.function_facts_for_test(&handles[0]).unwrap(), before);
            },
        )
        .expect("bytecode no-op assertion with independent clean teardown");
    }
}

#[test]
fn p3sym_c4_initial_bytecode_exhaustion_refusal_precedence() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture_initialized(
            kind,
            true,
            |obarray, ids| {
                let definition = constant_bytecode();
                obarray.seed_value_id(ids[0], definition);
                obarray.set_symbol_function_id(ids[0], definition);
                obarray.set_symbol_function_id(ids[1], constant_bytecode());
                neovm_value::test_support::authority::exhaust_function_epoch_for_test(obarray);
            },
            |run, handles, homes| {
                let before = run.function_facts_for_test(&handles[0]).unwrap();
                run.sym_fset_into(&handles[0], &homes.old, &mut homes.dst)
                    .unwrap();
                let answer = homes.dst.word_for_test(run);
                run.sym_function_into(&handles[1], &mut homes.cache)
                    .unwrap();
                assert_ne!(homes.cache.word_for_test(run), homes.old.word_for_test(run));
                assert_eq!(
                    run.sym_fset_into(&handles[0], &homes.cache, &mut homes.dst),
                    Err(FunctionError::UnsupportedDefinition)
                );
                assert_eq!(
                    run.sym_fset_into(&handles[0], &homes.new, &mut homes.dst),
                    Err(FunctionError::EpochExhausted)
                );
                assert_eq!(run.function_facts_for_test(&handles[0]).unwrap(), before);
                assert_eq!(homes.dst.word_for_test(run), answer);
                run.sym_function_into(&handles[0], &mut homes.activation)
                    .unwrap();
                assert_eq!(
                    homes.activation.word_for_test(run),
                    homes.old.word_for_test(run)
                );
            },
        )
        .expect("same bytecode no-op precedes exhaustion; changed bytecode remains unsupported");
    }
}

#[test]
fn p3sym_c4_initial_bytecode_rooted_read_replace_and_restricted_call_refusal() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture_initialized(
            kind,
            true,
            |obarray, ids| {
                let definition = constant_bytecode();
                obarray.seed_value_id(ids[0], definition);
                obarray.set_symbol_function_id(ids[0], definition);
                obarray.set_symbol_function_id(ids[1], constant_bytecode());
            },
            |run, handles, homes| {
                let before = run.function_facts_for_test(&handles[0]).unwrap();
                run.sym_function_into(&handles[0], &mut homes.dst).unwrap();
                assert_eq!(homes.dst.word_for_test(run), homes.old.word_for_test(run));
                let answer = homes.dst.word_for_test(run);
                let mut cache = FunctionCallCache::new(run).unwrap();
                let mut activation = FunctionActivation::new(run).unwrap();
                assert_eq!(
                    run.sym_call_begin(&handles[0], &mut cache, &mut activation),
                    Err(FunctionError::UnsupportedDefinition)
                );
                assert_eq!(activation.selected_epoch(), None);
                assert_eq!(cache.observed_epoch(), None);
                assert_eq!(cache.counts_for_test(), (0, 0, 0));
                run.sym_function_into(&handles[1], &mut homes.cache)
                    .unwrap();
                assert_eq!(
                    run.sym_fset_into(&handles[0], &homes.cache, &mut homes.dst),
                    Err(FunctionError::UnsupportedDefinition)
                );
                assert_eq!(run.function_facts_for_test(&handles[0]).unwrap(), before);
                assert_eq!(homes.dst.word_for_test(run), answer);
                run.sym_fset_into(&handles[0], &homes.new, &mut homes.dst)
                    .unwrap();
                assert_eq!(
                    run.function_facts_for_test(&handles[0]).unwrap(),
                    (before.0 + 1, before.0 + 1, true)
                );
                run.sym_function_into(&handles[0], &mut homes.dst).unwrap();
                assert_eq!(homes.dst.word_for_test(run), homes.new.word_for_test(run));
                run.sym_call_begin(&handles[0], &mut cache, &mut activation)
                    .unwrap();
                assert_eq!(
                    run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                    Ok(FunctionStep::Yielded)
                );
                assert_eq!(
                    run.sym_call_step(&mut activation, &mut homes.dst, NonZeroU32::MIN),
                    Ok(FunctionStep::Returned)
                );
                let expected = oracle()[5]
                    .trim_end_matches(')')
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                assert_eq!(
                    crate::emacs_core::print_value(&value_from_raw_bits(
                        homes.dst.word_for_test(run)
                    )),
                    expected
                );
            },
        )
        .expect("opaque initial target can be read/replaced, never executed in the era");
    }
}

#[test]
fn p3sym_c4_initial_bytecode_unbind_and_reinstall_refusal() {
    fixture_initialized(
        PlainSymbolBacking::Projected,
        true,
        |obarray, ids| {
            let definition = constant_bytecode();
            obarray.seed_value_id(ids[0], definition);
            obarray.set_symbol_function_id(ids[0], definition);
        },
        |run, handles, homes| {
            let before = run.function_facts_for_test(&handles[0]).unwrap();
            run.sym_fmakunbound_into(&handles[0], &mut homes.dst)
                .unwrap();
            let after = run.function_facts_for_test(&handles[0]).unwrap();
            assert_eq!(after, (before.0 + 1, before.0 + 1, true));
            let answer = homes.dst.word_for_test(run);
            assert_eq!(answer, Value::from_sym_id(homes.symbol).bits());
            assert_eq!(
                run.sym_fset_into(&handles[0], &homes.old, &mut homes.dst),
                Err(FunctionError::UnsupportedDefinition)
            );
            assert_eq!(homes.dst.word_for_test(run), answer);
            assert_eq!(run.function_facts_for_test(&handles[0]).unwrap(), after);
            run.sym_function_into(&handles[0], &mut homes.cache)
                .unwrap();
            assert_eq!(homes.cache.word_for_test(run), Value::NIL.bits());
        },
    )
    .expect("an unbound initial bytecode is not an identity no-op");
}

#[test]
fn p3sym_c4_initial_bytecode_shape_and_materialization_fence() {
    crate::test_utils::ensure_eval_hooks_installed();
    for excluded in [
        "interactive",
        "environment",
        "heap-constant",
        "nonconstant",
        "arity",
        "documentation",
        "extra-slot",
        "pdump-marker",
    ] {
        let (prepared, accesses) =
            neovm_eval::test_support::authority::with_test_symbol_functions_init_era(
                PlainSymbolBacking::Projected,
                |obarray, ids| {
                    let mut function = constant_function();
                    match excluded {
                        "interactive" => function.interactive = Some(Value::T),
                        "environment" => function.env = Some(Value::T),
                        "heap-constant" => {
                            function.constants = vec![Value::cons(Value::T, Value::NIL)].into()
                        }
                        "nonconstant" => {
                            function.ops =
                                vec![Op::Constant(0), Op::Constant(0), Op::Cons, Op::Return]
                        }
                        "arity" => {
                            let parameter = crate::intern::intern("p3sym-c4-bytecode-argument");
                            function.params = LambdaParams::simple(vec![parameter]).into();
                            function.arglist = Value::list(vec![Value::from_sym_id(parameter)]);
                        }
                        "documentation" => function.doc_form = Some(Value::T),
                        "extra-slot" => function.extra_slots.push(Value::T),
                        "pdump-marker" => {
                            // Only exercise the saved-image discriminator: no
                            // mapped payload access, decoding or collection here.
                            neovm_value::test_support::authority::forge_pdump_marker_ops_for_test(
                                &mut function,
                            );
                        }
                        _ => unreachable!("fixture variant"),
                    }
                    obarray.set_symbol_function_id(ids[0], Value::make_bytecode(function));
                },
                |era, ids| {
                    neovm_value::test_support::authority::reset_bytecode_data_access_count_for_test(
                    );
                    let handle = era.admit_plain_symbol(ids[0]).unwrap();
                    let prepared = era.prepare_function_symbol(&handle);
                    let accesses = crate::value::bytecode_data_access_count();
                    let (collector, keeper) =
                        era.finish().expect("excluded bytecode fixture teardown");
                    drop(collector);
                    drop(keeper);
                    (prepared, accesses)
                },
            );
        assert_eq!(
            prepared,
            Err(FunctionError::UnsupportedDefinition),
            "{excluded}"
        );
        assert_eq!(accesses, 0, "no materializing reader for {excluded}");
    }
}
