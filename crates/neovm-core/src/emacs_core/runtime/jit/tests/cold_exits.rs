//! `NEOVM_JIT_COLD_EXITS` (P2.2 O0.2): off, the CLIF is what it was; `on`,
//! it differs only in cold marks, every exit block carries one, and the
//! compile census counts them by kind; `share` also sends every precise
//! deopt through one tail per function.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::compile::{
    ColdExitsMode, STATUS_DEOPT, STATUS_DEOPT_AT, STATUS_OK, STATUS_SIGNAL,
    compile_bytecode_function_with, force_cold_exits_for_test, force_deopt_for_test,
    force_profit_gate_for_test, lower_leaf_full, lowering,
};
use crate::emacs_core::jit::stats;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

/// Which compile path a corpus body takes.
#[derive(Clone, Copy, Debug)]
enum Path {
    /// `lower_leaf_full`: the baseline, whatever the body.
    Baseline,
    /// The tier-up choice (MIR where it takes the body).
    Tiered,
}

/// Bodies that between them reach every exit kind.
fn corpus() -> Vec<(&'static str, Path, ByteCodeFunction)> {
    let callee = Value::symbol("cold-exits-callee");
    // (lambda (n) (let ((i 0)) (while (< i n) (cold-exits-callee i)
    //                             (setq i (1+ i))) i))
    let call_loop = vec![
        Op::Constant(0),   // 0: i = 0        [n i]
        Op::StackRef(0),   // 1: i            [n i i]
        Op::StackRef(2),   // 2: n            [n i i n]
        Op::Lss,           // 3: (< i n)      [n i b]
        Op::GotoIfNil(13), // 4:              [n i]
        Op::Constant(1),   // 5: callee       [n i f]
        Op::StackRef(1),   // 6: i            [n i f i]
        Op::Call(1),       // 7:              [n i r]
        Op::Pop,           // 8:              [n i]
        Op::StackRef(0),   // 9: i            [n i i]
        Op::Add1,          // 10:             [n i i+1]
        Op::StackSet(1),   // 11: i = i+1     [n i]
        Op::Goto(1),       // 12
        Op::Return,        // 13: i
    ];
    vec![
        (
            "call-loop",
            Path::Baseline,
            function(call_loop.clone(), vec![Value::make_int(0), callee], 1),
        ),
        (
            "call-loop-tiered",
            Path::Tiered,
            function(call_loop, vec![Value::make_int(0), callee], 1),
        ),
        // (lambda (x) (+ x 1)): pure MIR, the shared rerun block.
        (
            "pure-add",
            Path::Tiered,
            function(
                vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
                vec![Value::make_int(1)],
                1,
            ),
        ),
        // (lambda (x) (1+ (cold-exits-callee x))): one call, not in a loop,
        // so its root window has its own grow check.
        (
            "call-then-add1",
            Path::Baseline,
            function(
                vec![
                    Op::Constant(0),
                    Op::StackRef(1),
                    Op::Call(1),
                    Op::Add1,
                    Op::Return,
                ],
                vec![callee],
                1,
            ),
        ),
        // (lambda () (condition-case err cold-exits-void
        //              (void-variable (list 'caught err)))): a handler
        // dispatch at the variable read.
        (
            "condition-case",
            Path::Baseline,
            function(
                vec![
                    Op::PushConditionCase(4),
                    Op::VarRef(0),
                    Op::PopHandler,
                    Op::Return,
                    Op::Constant(1),
                    Op::StackRef(1),
                    Op::List(2),
                    Op::Return,
                ],
                vec![Value::symbol("cold-exits-void"), Value::symbol("caught")],
                0,
            ),
        ),
    ]
}

fn compile(ev: &Context, path: Path, f: &ByteCodeFunction) {
    match path {
        Path::Baseline => {
            lower_leaf_full(
                &f.ops,
                &f.constants,
                f.params.required.len(),
                None,
                Some(&ev.obarray),
                0,
            )
            .expect("baseline compiles");
        }
        Path::Tiered => {
            compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles");
        }
    }
}

/// The CLIF of every function compiled for one corpus body in `mode`.
fn clif_of(ev: &Context, path: Path, f: &ByteCodeFunction, mode: ColdExitsMode) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("leaf.clif");
    force_cold_exits_for_test(Some(mode));
    lowering::force_clif_dump_for_test(Some(file.to_string_lossy().into_owned()));
    compile(ev, path, f);
    lowering::force_clif_dump_for_test(None);
    force_cold_exits_for_test(None);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    text.split("\n;; ")
        .filter(|chunk| !chunk.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Mask what differs between two compiles of one body: the baked
/// addresses of each leaf's own buffers (Cranelift prints an immediate of
/// 10000 or more in hex).
fn mask_addresses(clif: &str) -> String {
    let mut out = String::with_capacity(clif.len());
    let mut rest = clif;
    while let Some(at) = rest.find("0x") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let len = tail
            .find(|c: char| !(c.is_ascii_hexdigit() || c == '_'))
            .unwrap_or(tail.len());
        out.push_str("0xADDR");
        rest = &tail[len..];
    }
    out.push_str(rest);
    out
}

/// One CLIF block: its header line and instruction lines.
struct ClifBlock<'a> {
    header: &'a str,
    insts: Vec<&'a str>,
}

impl ClifBlock<'_> {
    fn is_cold(&self) -> bool {
        self.header.trim_end().ends_with(" cold:")
    }

    /// The status this block returns, when it returns a constant it defines.
    fn returned_status(&self) -> Option<i64> {
        let ret = self
            .insts
            .iter()
            .find_map(|line| line.trim().strip_prefix("return "))?;
        // `return v11  ; v11 = 2`: the value is the first token.
        let value = ret.split_whitespace().next()?;
        self.insts.iter().find_map(|line| {
            let (lhs, rhs) = line.trim().split_once(" = ")?;
            (lhs == value)
                .then(|| rhs.strip_prefix("iconst.i64 "))
                .flatten()
                .and_then(|k| k.trim().parse().ok())
        })
    }
}

fn blocks(clif: &str) -> Vec<ClifBlock<'_>> {
    let mut out: Vec<ClifBlock<'_>> = Vec::new();
    for line in clif.lines() {
        if line.starts_with("block") {
            out.push(ClifBlock {
                header: line,
                insts: Vec::new(),
            });
        } else if let Some(block) = out.last_mut()
            && line.starts_with("    ")
        {
            block.insts.push(line);
        }
    }
    out
}

fn cold_marks(clif: &str) -> usize {
    blocks(clif).iter().filter(|b| b.is_cold()).count()
}

/// Off, nothing new is marked; on, the CLIF differs from off only in cold
/// marks, and there are more of them.
#[test]
fn cold_exits_change_only_the_cold_marks() {
    force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    let ev = Context::new();
    for (name, path, f) in corpus() {
        let off = clif_of(&ev, path, &f, ColdExitsMode::Off);
        let on = clif_of(&ev, path, &f, ColdExitsMode::On);
        assert!(!off.is_empty(), "{name}: nothing compiled");
        assert_eq!(off.len(), on.len(), "{name}");
        for (off, on) in off.iter().zip(&on) {
            assert_eq!(
                mask_addresses(&on.replace(" cold:", ":")),
                mask_addresses(&off.replace(" cold:", ":")),
                "{name}: the knob changed more than cold marks"
            );
            assert!(
                cold_marks(on) > cold_marks(off),
                "{name}: nothing new is cold\n{on}"
            );
        }
    }
}

/// On (and shared), every block that leaves through a deopt or signal
/// status is cold, and no block that returns a value is. Shared, a
/// function returns `STATUS_DEOPT_AT` from one block at most.
#[test]
fn cold_exits_mark_every_exit_block() {
    force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    let ev = Context::new();
    for mode in [ColdExitsMode::On, ColdExitsMode::Share] {
        for (name, path, f) in corpus() {
            for clif in clif_of(&ev, path, &f, mode) {
                let blocks = blocks(&clif);
                assert!(!blocks[0].is_cold(), "{name}: the entry is cold\n{clif}");
                let mut exits = 0;
                let mut deopt_returns = 0;
                for block in &blocks {
                    match block.returned_status() {
                        Some(status @ (STATUS_DEOPT | STATUS_DEOPT_AT | STATUS_SIGNAL)) => {
                            exits += 1;
                            deopt_returns += usize::from(status == STATUS_DEOPT_AT);
                            assert!(block.is_cold(), "{name}: {} is hot\n{clif}", block.header);
                        }
                        Some(STATUS_OK) => {
                            assert!(!block.is_cold(), "{name}: {} is cold\n{clif}", block.header)
                        }
                        _ => {}
                    }
                }
                assert!(exits > 0, "{name}: no exit found\n{clif}");
                if mode == ColdExitsMode::Share {
                    assert!(
                        deopt_returns <= 1,
                        "{name}: {deopt_returns} deopt returns\n{clif}"
                    );
                }
            }
        }
    }
}

/// Shared, a body with several guarded ops returns through one tail, which
/// the sites reach by jumping with their pc, depth and handler count.
#[test]
fn shared_deopt_tail_serves_every_site() {
    force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    let ev = Context::new();
    let (name, path, f) = corpus().remove(0);
    let on = clif_of(&ev, path, &f, ColdExitsMode::On);
    let share = clif_of(&ev, path, &f, ColdExitsMode::Share);
    let deopt_returns = |clif: &str| {
        blocks(clif)
            .iter()
            .filter(|b| b.returned_status() == Some(STATUS_DEOPT_AT))
            .count()
    };
    assert!(deopt_returns(&on[0]) >= 2, "{name}: {}", on[0]);
    assert_eq!(deopt_returns(&share[0]), 1, "{name}: {}", share[0]);
    let tail = blocks(&share[0])
        .into_iter()
        .find(|b| b.returned_status() == Some(STATUS_DEOPT_AT))
        .expect("the tail");
    assert!(tail.is_cold());
    let params = tail.header.matches(": i64").count();
    assert_eq!(params, 3, "pc, depth, handlers: {}", tail.header);
}

fn cold_census() -> [u64; stats::COLD_EXIT_KINDS] {
    stats::compile_stats_snapshot().cold_exits
}

/// The census counts each kind the lowering marked, and nothing off.
#[test]
fn cold_exit_census_counts_each_kind() {
    use strum::IntoEnumIterator;
    force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    let ev = Context::new();
    let mut seen = [0u64; stats::COLD_EXIT_KINDS];
    for (name, path, f) in corpus() {
        let before = cold_census();
        let _ = clif_of(&ev, path, &f, ColdExitsMode::Off);
        assert_eq!(cold_census(), before, "{name}: counted with the knob off");
        let _ = clif_of(&ev, path, &f, ColdExitsMode::Share);
        let after = cold_census();
        for (i, slot) in seen.iter_mut().enumerate() {
            *slot += after[i] - before[i];
        }
    }
    for kind in ColdExit::iter() {
        assert!(seen[kind as usize] > 0, "{kind:?} never marked: {seen:?}");
    }
    let mut s = stats::CompileStats::default();
    assert!(!stats::format_summary(&s).contains("cold_exits["));
    s.cold_exits[ColdExit::Deopt as usize] = 2;
    s.cold_exits[ColdExit::Poll as usize] = 1;
    assert!(
        stats::format_summary(&s).ends_with(" cold_exits[deopt=2 poll=1]"),
        "{}",
        stats::format_summary(&s)
    );
}

/// The corpus runs the same in every mode: a cold mark moves code and the
/// shared tail writes the same cells; neither changes what the code does.
#[test]
fn cold_exits_run_the_corpus_unchanged() {
    force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(fset 'cold-exits-callee (lambda (x) (* x 2)))")
        .expect("defines the callee");
    let run = |ev: &mut Context, mode: ColdExitsMode, f: &ByteCodeFunction, args: &[Value]| {
        force_cold_exits_for_test(Some(mode));
        let leaf = lower_leaf_full(
            &f.ops,
            &f.constants,
            f.params.required.len(),
            None,
            Some(&ev.obarray),
            0,
        )
        .expect("compiles");
        force_cold_exits_for_test(None);
        leaf.call(ev as *mut Context as *mut u8, args)
    };
    // Fresh objects differ in identity between two runs: compare printed.
    let describe = |run: crate::emacs_core::jit::compile::NativeRun| match run {
        crate::emacs_core::jit::compile::NativeRun::Ok(bits) => {
            format!("ok {}", print_value(&Value::from_bits(bits)))
        }
        crate::emacs_core::jit::compile::NativeRun::DeoptAt(resume) => format!(
            "deopt-at {} {:?}",
            resume.pc,
            resume.stack.iter().map(print_value).collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    };
    for (name, _, f) in corpus() {
        for arg in [Value::make_int(300), Value::make_float(2.5)] {
            // A float operand deopts at the first guard, in both.
            let args = vec![arg; f.params.required.len()];
            let off = describe(run(&mut ev, ColdExitsMode::Off, &f, &args));
            for mode in [ColdExitsMode::On, ColdExitsMode::Share] {
                let got = describe(run(&mut ev, mode, &f, &args));
                assert_eq!(got, off, "{name} {mode:?}");
            }
        }
    }
}
