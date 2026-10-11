use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};
use crate::emacs_core::jit::inline::CensusShape;
use crate::emacs_core::value::LambdaParams;

fn function(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| crate::emacs_core::intern::intern(&format!("census-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops_for_test();
    f
}

/// Census selection recognizes named and constant calls without changing
/// feedback, source identities, or assigning compiled-cache ids.
#[test]
fn inline_census_classifies_sites_without_mutating_sources() {
    let mut ev = Context::new();
    let callee = function(1, vec![Op::StackRef(0), Op::Return], vec![]);
    callee.jit_runtime().set_hot_for_test();
    let target = Value::make_bytecode(callee);
    let named = Value::symbol("inline-census-named");
    ev.obarray
        .set_symbol_function_id(named.as_symbol_id().unwrap(), target);
    let caller = function(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![named, target],
    );
    let target_bc = target.get_bytecode_data().unwrap();
    let heat = target_bc.jit_runtime().heat();
    let epoch = ev.obarray.function_epoch();
    let rows = census_sites(&caller, Some(&ev.obarray));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].shape, CensusShape::Named);
    assert!(rows[0].v2.is_ok());
    assert!(rows[0].replay.is_err());
    assert_eq!(rows[1].shape, CensusShape::Constant);
    assert!(rows[1].v2.is_ok());
    assert!(rows[1].replay.is_ok());
    assert_eq!(rows[0].target_source, rows[1].target_source);
    assert_eq!(target_bc.jit_runtime().heat(), heat);
    assert_eq!(target_bc.jit_runtime().compiled_id(), None);
    assert_eq!(caller.jit_runtime().compiled_id(), None);
    assert_eq!(ev.obarray.function_epoch(), epoch);
}

/// The common map-closure shape carries make-closure provenance through
/// the straight-line bytecode into mapc's callback argument.
#[test]
fn inline_census_recognizes_an_in_unit_map_closure() {
    let ev = Context::new();
    let template = function(1, vec![Op::StackRef(0), Op::Return], vec![]);
    template.jit_runtime().set_hot_for_test();
    let target = Value::make_bytecode(template);
    let caller = function(
        0,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(1),
            Op::Constant(3),
            Op::Call(2),
            Op::Return,
        ],
        vec![
            Value::symbol("mapc"),
            Value::symbol("make-closure"),
            target,
            Value::NIL,
        ],
    );
    let rows = census_sites(&caller, Some(&ev.obarray));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].shape, CensusShape::Hof);
    assert_eq!(
        rows[1].target_source,
        Some(target.get_bytecode_data().unwrap().source_id)
    );
    assert!(rows[1].v2.is_ok(), "{:?}", rows[1].v2);
    assert!(rows[1].replay.is_err());
}

/// The off wrapper records nothing; closure instances share their source
/// target count once enabled, with startup excluded by the loop mark.
#[test]
fn inline_census_callbacks_are_opt_in_and_share_source_identity() {
    let _ev = Context::new();
    let f = function(1, vec![Op::StackRef(0), Op::Return], vec![]);
    let source = f.source_id;
    let instance = Value::make_bytecode(f.clone());
    let target = Value::make_bytecode(f);
    FORCE_ENABLED.with(|on| on.set(Some(false)));
    note_callback(target);
    assert!(!snapshot().callbacks.contains_key(&source));
    FORCE_ENABLED.with(|on| on.set(Some(true)));
    note_callback(target);
    mark_command_loop_entry();
    note_callback(instance);
    note_callback(target);
    let snapshot = snapshot();
    let row = &snapshot.callbacks[&source];
    assert_eq!(row.count, 3);
    assert!(row.v2.is_ok());
    assert_eq!(snapshot.loop_base.as_ref().unwrap()[&source], 1);
    assert_eq!(
        target
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .compiled_id(),
        None
    );
    FORCE_ENABLED.with(|on| on.set(None));
}

/// Entry counting precedes quit and depth checks, and survives refreshes of
/// the calling mutator's attention word. Ordinary quit polls stay clear.
#[test]
fn inline_census_callback_entries_include_quit_and_depth_refusals() {
    FORCE_ENABLED.with(|on| on.set(Some(true)));
    let mut ev = Context::new();
    let _roots = ev.save_vm_roots();
    let f = function(1, vec![Op::StackRef(0), Op::Return], vec![]);
    let source = f.source_id;
    let target = Value::make_bytecode(f);
    ev.push_vm_frame_root(target);
    assert!(ev.maybe_quit_hot_ok());
    assert_eq!(ev.apply1(target, Value::T).unwrap(), Value::T);
    assert_eq!(snapshot().callbacks[&source].count, 1);

    ev.set_quit_flag_value(Value::T);
    assert!(matches!(
        ev.apply1(target, Value::NIL).kinded(),
        Err(FlowKind::Signal(signal)) if signal.symbol_name() == "quit"
    ));
    assert_eq!(snapshot().callbacks[&source].count, 2);
    ev.set_quit_flag_value(Value::NIL);
    assert!(ev.maybe_quit_hot_ok());
    assert_eq!(ev.apply1(target, Value::T).unwrap(), Value::T);
    assert_eq!(snapshot().callbacks[&source].count, 3);

    ev.depth = ev.max_depth;
    assert!(matches!(
        ev.apply1(target, Value::NIL).kinded(),
        Err(FlowKind::Signal(signal)) if signal.symbol_name() == "excessive-lisp-nesting"
    ));
    assert_eq!(snapshot().callbacks[&source].count, 4);
    ev.depth = 0;

    FORCE_ENABLED.with(|on| on.set(Some(false)));
    ev.refresh_attention_for_test();
    assert_eq!(ev.apply1(target, Value::T).unwrap(), Value::T);
    assert_eq!(snapshot().callbacks[&source].count, 4);
    FORCE_ENABLED.with(|on| on.set(None));
    ev.refresh_attention_for_test();
}

/// A recompile refreshes cold-target feedback, while a profitability
/// refusal keeps the source out of the compiled-leaf candidate totals.
#[test]
fn inline_census_refreshes_verdicts_and_records_compile_refusals() {
    let ev = Context::new();
    let target = Value::make_bytecode(function(1, vec![Op::StackRef(0), Op::Return], vec![]));
    let caller = function(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![target],
    );
    FORCE_ENABLED.with(|on| on.set(Some(true)));
    note_compile(&caller, Some(&ev.obarray));
    assert_eq!(
        snapshot().sources[&caller.source_id].sites[0].v2,
        Err("cold".into())
    );
    target
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    note_compile(&caller, Some(&ev.obarray));
    note_compile_outcome(&caller, &Err(CompileError::NotProfitable));
    let snapshot = snapshot();
    let source = &snapshot.sources[&caller.source_id];
    assert_eq!(source.compiles, 2);
    assert!(source.sites[0].v2.is_ok());
    assert_eq!(source.outcome.name(), "not_profitable");
    assert!(!source.outcome.compiled());
    FORCE_ENABLED.with(|on| on.set(None));
}

/// All sites are reported, including rejection reasons and callback deltas;
/// the census is not a top-N sample that could hide editor candidates.
#[test]
fn inline_census_report_keeps_verdicts_and_loop_deltas() {
    let snapshot = Snapshot {
        sources: BTreeMap::from([(
            41,
            Source {
                name: "org-caller".into(),
                compiled_id: Some(7),
                compiles: 2,
                outcome: CompileOutcome::Compiled(LeafTier::Baseline),
                sites: vec![
                    CensusSite {
                        pc: 3,
                        shape: CensusShape::Named,
                        target_source: Some(42),
                        replay: Err("shape".into()),
                        v2: Ok(()),
                    },
                    CensusSite {
                        pc: 6,
                        shape: CensusShape::Dynamic,
                        target_source: None,
                        replay: Err("shape".into()),
                        v2: Err("unresolved-target".into()),
                    },
                ],
            },
        )]),
        callbacks: BTreeMap::from([(
            42,
            Callback {
                name: "anon".into(),
                count: 11,
                v2: Ok(()),
            },
        )]),
        loop_base: Some(BTreeMap::from([(42, 4)])),
    };
    let lines = snapshot.render(&BTreeMap::from([(42, "named-callback".into())]));
    assert_eq!(lines.len(), 4);
    assert!(
        lines[0]
            .1
            .contains("sites=2 replay_eligible=0 v2_eligible=1")
    );
    assert!(lines[0].1.contains("callbacks_since_command_loop=7"));
    assert!(
        lines[0]
            .1
            .contains("compiled_sources=1 compiled_v2_eligible=1")
    );
    assert!(lines[1].1.contains("fn=org-caller"));
    assert!(lines[1].1.contains("outcome=baseline"));
    assert!(lines[2].1.contains("v2=unresolved-target"));
    assert!(
        lines[3]
            .1
            .contains("fn=named-callback callbacks=11 since_command_loop=7")
    );
}
