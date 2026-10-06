//! P2.3's annotated fuser front and legacy-MIR refusal.
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::lowering;
use crate::emacs_core::jit::compile::{self, Inline2Mode, NativeRun, force_inline2_for_test};
use crate::emacs_core::value::LambdaParams;

fn lexical(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| crate::emacs_core::intern::intern(&format!("inline-v2-arg-{i}")))
            .collect(),
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(32);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller(callee: ByteCodeFunction) -> ByteCodeFunction {
    let target = Value::make_bytecode(callee);
    lexical(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Constant(1),
            Op::Cons,
            Op::Return,
        ],
        vec![target, Value::NIL],
    )
}

struct Knobs;
impl Knobs {
    fn enter(mode: Inline2Mode) -> Self {
        force_inline_for_test(Some(true));
        force_inline2_for_test(Some(mode));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        crate::emacs_core::jit::stats::force_observe_for_test(Default::default());
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        force_inline2_for_test(None);
        lowering::force_clif_dump_for_test(None);
    }
}

#[test]
fn inline_v2_knob_modes_are_default_off() {
    assert_eq!(Inline2Mode::parse(None), Inline2Mode::Off);
    for value in ["", "off", "0", "false", "unknown"] {
        assert_eq!(Inline2Mode::parse(Some(value)), Inline2Mode::Off);
    }
    for (value, mode) in [
        ("named", Inline2Mode::Named),
        ("closure", Inline2Mode::Closure),
        ("hof", Inline2Mode::Hof),
        ("all", Inline2Mode::All),
    ] {
        assert_eq!(Inline2Mode::parse(Some(value)), mode);
        assert!(mode.enabled());
    }
}

/// Side tables annotate both returns and all instructions of each return
/// expansion without changing the old fuser's ops, constants or feedback.
#[test]
fn inline_v2_side_tables_preserve_the_legacy_splice() {
    let _knobs = Knobs::enter(Inline2Mode::All);
    let _ev = Context::new();
    let f = caller(lexical(
        1,
        vec![
            Op::StackRef(0),
            Op::GotoIfNil(4),
            Op::Constant(0),
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::make_int(9)],
    ));
    let feedback = vec![NumericFeedback::FixnumOnly; f.executable_ops().len()];
    let old = fuse_calls(f.executable_ops(), &f.constants, None, 1, &feedback).unwrap();
    let new = fuse_calls_v2(f.executable_ops(), &f.constants, None, 1, &feedback).unwrap();
    assert!(!old.is_v2());
    assert!(new.is_v2());
    assert_eq!(old.ops, new.ops);
    assert_eq!(old.constants, new.constants);
    assert_eq!(old.feedback, new.feedback);
    assert_eq!(old.caller_of_fused, new.caller_of_fused);
    assert_eq!(old.region_of, new.region_of);
    let side = new.v2.as_ref().unwrap();
    let region = &new.regions[0];
    assert_eq!(region.parent, None);
    assert!(side.entry_at.contains(region.start));
    assert_eq!(side.callee_pc_of_fused.len(), new.ops.len());
    let exits: Vec<_> = (region.start..region.end)
        .filter(|&pc| side.exit_at.contains(pc))
        .collect();
    assert_eq!(exits.len(), 2);
    for (pc, source_pc) in exits.into_iter().zip([3, 5]) {
        assert!(matches!(new.ops[pc], Op::DiscardN(_)));
        assert_eq!(side.callee_pc_of_fused[pc], source_pc);
        assert_eq!(side.callee_pc_of_fused[pc + 1], source_pc);
    }
    assert!(side.materialize_at.is_empty());
    assert!(!side.entry_at.contains(new.ops.len() + 100));
}

/// An enabled v2 body goes through the baseline and resumes at the failing
/// inner instruction, retaining the original physical call snapshot.
#[test]
fn inline_v2_refuses_legacy_mir_and_resumes_inner_chain() {
    let _knobs = Knobs::enter(Inline2Mode::All);
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            stats: true,
            ..Default::default()
        },
    );
    let mut ev = Context::new();
    let f = caller(lexical(
        1,
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ));
    let leaf = compile::compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier().name(), "baseline");
    assert!(
        leaf.obs
            .mir_verdict
            .as_deref()
            .unwrap_or("")
            .contains("fused-v2"),
        "{:?}",
        leaf.obs.mir_verdict
    );
    let arg = Value::symbol("inline-v2-not-a-number");
    let _ = ev.debug_on_next_call_is_armed();
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[arg]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("expected inner chain deopt: {run:?}")
    };
    assert_eq!(resume.pc, 2);
    assert_eq!(resume.stack, vec![arg, f.constants[0], arg]);
    assert!(resume.chain.is_some());
    let frames = &resume.inlined.as_ref().expect("chain readback").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].function, f.constants[0]);
    assert_eq!(frames[0].pc, 1);
    assert_eq!(frames[0].stack, vec![arg, arg]);
}

fn mask_addresses(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(at) = rest.find("0x") {
        output.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let n = tail
            .find(|c: char| !c.is_ascii_hexdigit() && c != '_')
            .unwrap_or(tail.len());
        let digits = tail[..n].replace('_', "");
        let is_pointer = u64::from_str_radix(&digits, 16)
            .is_ok_and(|bits| (0x100_0000_0000..=0x7fff_ffff_ffff).contains(&bits));
        if is_pointer {
            output.push_str("0xADDR");
        } else {
            output.push_str(&rest[at..at + 2 + n]);
        }
        rest = &tail[n..];
    }
    output.push_str(rest);
    output
}

/// Compare off against the original baseline path explicitly: legacy
/// fusion, feedback publication, then lower_leaf_full. The corpus covers
/// arithmetic guards, branch/return rebasing and stack shuffles.
#[test]
fn inline_v2_off_clif_matches_the_legacy_fuser_corpus() {
    let _knobs = Knobs::enter(Inline2Mode::Off);
    let ev = Context::new();
    let corpus = [
        lexical(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]),
        lexical(
            1,
            vec![
                Op::StackRef(0),
                Op::GotoIfNil(4),
                Op::Constant(0),
                Op::Return,
                Op::StackRef(0),
                Op::Return,
            ],
            vec![Value::make_int(9)],
        ),
        lexical(
            1,
            vec![Op::Dup, Op::Pop, Op::StackRef(0), Op::Return],
            vec![],
        ),
    ];
    for (index, callee) in corpus.into_iter().enumerate() {
        let f = caller(callee);
        let dir = tempfile::tempdir().unwrap();
        let off_path = dir.path().join("off.clif");
        lowering::force_clif_dump_for_test(Some(off_path.to_string_lossy().into_owned()));
        let off = compile::compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
        assert_eq!(off.tier().name(), "baseline");
        let legacy_path = dir.path().join("legacy.clif");
        lowering::force_clif_dump_for_test(Some(legacy_path.to_string_lossy().into_owned()));
        let feedback = vec![NumericFeedback::FixnumOnly; f.executable_ops().len()];
        let body = std::rc::Rc::new(
            fuse_calls(f.executable_ops(), &f.constants, None, 1, &feedback).unwrap(),
        );
        let _scope = FusedScope::enter(body.clone());
        let _feedback = compile::publish_numeric_feedback_vec(body.feedback.clone());
        compile::lower_leaf_full(
            &body.ops,
            &body.constants,
            1,
            body.offset_map.as_deref(),
            Some(&ev.obarray),
            0,
        )
        .unwrap();
        lowering::force_clif_dump_for_test(None);
        assert_eq!(
            mask_addresses(&std::fs::read_to_string(off_path).unwrap()),
            mask_addresses(&std::fs::read_to_string(legacy_path).unwrap()),
            "corpus body {index}"
        );
    }
}
