//! GNU primitive opcodes bypass function cells; genuine Bcall/eval/funcall
//! and apply still resolve them. Refresh fixtures from GNU 31.1 with
//! `UPDATE_EXPECT=1 EMACS=$HOME/.local/bin/emacs cargo nextest run ...`.
//! Threading: evaluator state and Lisp roots belong to this test's mutator;
//! scoped compiler overrides contain only scalar settings and restore on exit.

use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{Value, list_to_vec};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Advice,
    Defalias,
}

struct Case {
    name: &'static str,
    body: &'static str,
    args: &'static str,
}

// Parameters prevent the compiler folding a primitive out of the body.
const OPCODES: &[Case] = &[
    Case {
        name: "aset",
        body: "(aset a b c)",
        args: "(vector 0 0) 1 7",
    },
    Case {
        name: "car",
        body: "(car a)",
        args: "'(4 . 5) nil nil",
    },
    Case {
        name: "nth",
        body: "(nth a b)",
        args: "1 '(4 5 6) nil",
    },
    Case {
        name: "aref",
        body: "(aref a b)",
        args: "[4 5 6] 1 nil",
    },
    Case {
        name: "length",
        body: "(length a)",
        args: "[4 5 6] nil nil",
    },
    Case {
        name: "substring",
        body: "(substring a b c)",
        args: "\"abcd\" 1 3",
    },
    Case {
        name: "nthcdr",
        body: "(nthcdr a b)",
        args: "1 '(4 5 6) nil",
    },
    Case {
        name: "elt",
        body: "(elt a b)",
        args: "[4 5 6] 1 nil",
    },
    Case {
        name: "string=",
        body: "(string= a b)",
        args: "\"ab\" \"ab\" nil",
    },
    Case {
        name: "string<",
        body: "(string< a b)",
        args: "\"ab\" \"ac\" nil",
    },
    Case {
        name: "upcase",
        body: "(upcase a)",
        args: "\"ab\" nil nil",
    },
    Case {
        name: "downcase",
        body: "(downcase a)",
        args: "\"AB\" nil nil",
    },
];
const BCALL: Case = Case {
    name: "string-match",
    body: "(string-match a b)",
    args: "\"b\" \"abc\" nil",
};

fn setup(case: &Case) -> String {
    format!(
        "(progn (require 'bytecomp)
           (defvar gnuop--calls 0)
           (defun gnuop--before (&rest _) (setq gnuop--calls (1+ gnuop--calls)))
           (defun gnuop--f (a b c) {})
           (byte-compile 'gnuop--f)
           (setq gnuop--original (symbol-function '{}))
           (setq gnuop--args (list {})))",
        case.body, case.name, case.args
    )
}

fn mutation(case: &Case, mutation: Mutation) -> String {
    match mutation {
        Mutation::Advice => format!(
            "(progn (advice-add '{} :before 'gnuop--before) (setq gnuop--calls 0))",
            case.name
        ),
        Mutation::Defalias => format!(
            "(progn (defalias '{} (lambda (&rest _)
                       (setq gnuop--calls (1+ gnuop--calls)) 'overridden))
                    (setq gnuop--calls 0))",
            case.name
        ),
    }
}

fn observation(case: &Case) -> String {
    // Fresh operands keep aset's mutation from influencing later calls. The
    // lambda remains interpreted, distinguishing eval from compiled opcodes.
    format!(
        "(progn
          (setq gnuop--compiled (list gnuop--result gnuop--calls))
          (setq gnuop--calls 0)
          (setq gnuop--interpreted
                (apply (lambda (a b c) {}) (list {})))
          (setq gnuop--interpreted (list gnuop--interpreted gnuop--calls))
          (setq gnuop--calls 0)
          (setq gnuop--funcall
                (apply (lambda (a b c) (funcall '{} {})) (list {})))
          (setq gnuop--funcall (list gnuop--funcall gnuop--calls))
          (setq gnuop--calls 0)
          (setq gnuop--apply
                (apply (lambda (a b c) (apply '{} (list {}))) (list {})))
          (setq gnuop--apply (list gnuop--apply gnuop--calls))
          (fset '{} gnuop--original)
          (list '{} gnuop--compiled gnuop--interpreted gnuop--funcall gnuop--apply))",
        case.body,
        case.args,
        case.name,
        operands(case),
        case.args,
        case.name,
        operands(case),
        case.args,
        case.name,
        case.name,
    )
}

fn operands(case: &Case) -> &'static str {
    match case.name {
        "car" | "length" | "upcase" | "downcase" => "a",
        "aset" | "substring" => "a b c",
        _ => "a b",
    }
}

fn gnu_program(cases: &[Case], mutation_kind: Mutation) -> String {
    let mut program = String::from("(let (gnuop--rows)\n");
    for case in cases {
        program.push_str(&format!(
            "{}\n{}\n(setq gnuop--result (apply 'gnuop--f gnuop--args))\n(push {} gnuop--rows)\n",
            setup(case),
            mutation(case, mutation_kind),
            observation(case)
        ));
    }
    program.push_str("(nreverse gnuop--rows))");
    program
}

fn gnu_expect(name: &str, program: &str, fixture: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return fixture.trim_end().to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = root.join("../../tmp");
    std::fs::create_dir_all(&tmp).expect("oracle temporary directory");
    let script = tmp.join(format!("gnu-opcodes-{name}-{}.el", std::process::id()));
    std::fs::write(&script, format!(
        ";;; -*- lexical-binding: t; -*-\n(require 'bytecomp)\n(let ((print-length nil) (print-level nil)) (prin1 {program}))\n"
    )).expect("GNU oracle input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    let output = std::process::Command::new(emacs)
        .args(["--batch", "-Q", "-l"])
        .arg(&script)
        .output()
        .expect("GNU 31.1 oracle");
    assert!(
        output.status.success(),
        "GNU {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout).expect("GNU output UTF-8");
    assert!(!expected.is_empty(), "GNU must produce an expectation");
    let fixture_path = root
        .join("src/emacs_core/runtime/bytecode/tests/primitive_opcode_cells")
        .join(format!("{name}.expect"));
    let refreshed = format!("{}\n", expected.trim_end());
    // Refresh from GNU on every request, but preserve include_str! mtimes
    // when bytes match so isolated nextest invocations reuse their binary.
    if std::fs::read_to_string(&fixture_path).ok().as_deref() != Some(refreshed.as_str()) {
        std::fs::write(fixture_path, refreshed).expect("save GNU fixture");
    }
    expected.trim_end().to_owned()
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Vm,
    #[cfg(feature = "jit")]
    Jit,
}

#[cfg(feature = "jit")]
fn compile_native(f: &ByteCodeFunction) -> crate::emacs_core::jit::compile::CompiledLeaf {
    use crate::emacs_core::jit::compile::{self, CompileRequest, lowering::RegallocPolicy};
    compile::compile_bytecode_function_requested(
        f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Auto,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: crate::emacs_core::jit::tier2::CompileTier::Plain,
        },
    )
    .expect("compile native behaviour even when the body is call-heavy")
}

fn assert_opcode_present(case: &Case, ops: &[Op]) {
    assert!(
        ops.iter().any(|op| match case.name {
            "aset" => matches!(op, Op::Aset),
            "car" => matches!(op, Op::Car),
            "nth" => matches!(op, Op::Nth),
            "aref" => matches!(op, Op::Aref),
            "length" => matches!(op, Op::Length),
            "substring" => matches!(op, Op::Substring),
            "nthcdr" => matches!(op, Op::Nthcdr),
            "elt" => matches!(op, Op::Elt),
            "string=" => matches!(op, Op::StringEqual),
            "string<" => matches!(op, Op::StringLessp),
            "upcase" | "downcase" => matches!(op, Op::CallBuiltinSym(_, 1)),
            "string-match" => matches!(op, Op::Call(2)),
            _ => unreachable!(),
        }),
        "{} must compile to its expected opcode: {ops:?}",
        case.name
    );
}

fn run_cases(cases: &[Case], kind: Mutation, mode: Mode) -> String {
    run_cases_with_constant_pool(cases, kind, mode, false)
}

fn run_cases_with_constant_pool(
    cases: &[Case],
    kind: Mutation,
    mode: Mode,
    constant_pool_builtin: bool,
) -> String {
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    let mut rows = Vec::new();
    for case in cases {
        ctx.eval_str(&setup(case))
            .expect("compile before function-cell mutation");
        let value = ctx
            .obarray
            .symbol_function("gnuop--f")
            .expect("compiled function");
        let source = value
            .get_bytecode_data()
            .expect("byte-compile returns bytecode");
        assert_opcode_present(case, source.executable_ops());
        let mut adapted = None;
        let f = if constant_pool_builtin {
            // CallBuiltin is the older constants-index primitive adapter.
            // It is not emitted by today's GNU decoder, so exercise it with
            // the exact decoded opcode body and an equivalent symbol pool.
            let mut f = ByteCodeFunction::new(source.params.clone());
            f.lexical = source.lexical;
            f.arglist = source.arglist;
            f.max_stack = source.max_stack;
            let mut constants = source.constants.to_vec();
            let mut replaced = false;
            f.ops = source
                .executable_ops()
                .iter()
                .map(|op| match *op {
                    Op::CallBuiltinSym(symbol, arity) => {
                        let index = u16::try_from(constants.len()).unwrap();
                        constants.push(Value::from_sym_id(symbol));
                        replaced = true;
                        Op::CallBuiltin(index, arity)
                    }
                    _ => op.clone(),
                })
                .collect();
            assert!(replaced, "constant-pool primitive adapter was installed");
            f.constants = constants.into();
            f.seal_hand_assembled_ops_for_test();
            adapted.insert(f)
        } else {
            source
        };
        #[cfg(feature = "jit")]
        let leaf = match mode {
            Mode::Vm => None,
            Mode::Jit => Some(compile_native(f)),
        };
        ctx.eval_str(&mutation(case, kind))
            .expect("mutate function cell after compilation");
        let args = list_to_vec(&ctx.eval_str("gnuop--args").expect("rooted operands"))
            .expect("argument list");
        let result = match mode {
            Mode::Vm => Vm::from_context(&mut ctx)
                .execute_with_func_value(f, args, value)
                .expect("VM primitive runs"),
            #[cfg(feature = "jit")]
            Mode::Jit => match leaf
                .as_ref()
                .unwrap()
                .call((&mut ctx as *mut Context).cast(), &args)
            {
                crate::emacs_core::jit::compile::NativeRun::Ok(bits) => Value::from_bits(bits),
                other => panic!("{} native primitive: {other:?}", case.name),
            },
        };
        ctx.set_variable("gnuop--result", result);
        let row = ctx
            .eval_str(&observation(case))
            .expect("observe calls and restore function cell");
        rows.push(row);
        // Keep every row rooted while subsequent cases allocate or collect.
        ctx.set_variable("gnuop--rows", Value::list(rows.clone()));
    }
    print_value(&Value::list(rows))
}

fn constant_pool_builtin_matches_gnu(mode: Mode) {
    let upcase = &OPCODES[10..11];
    for (name, kind, fixture) in [
        (
            "constant-builtin-advice",
            Mutation::Advice,
            include_str!("primitive_opcode_cells/constant-builtin-advice.expect"),
        ),
        (
            "constant-builtin-defalias",
            Mutation::Defalias,
            include_str!("primitive_opcode_cells/constant-builtin-defalias.expect"),
        ),
    ] {
        let expected = gnu_expect(name, &gnu_program(upcase, kind), fixture);
        assert_eq!(
            run_cases_with_constant_pool(upcase, kind, mode, true),
            expected,
            "constants-index primitive adapter: {kind:?} {mode:?}"
        );
    }
}

#[test]
fn gnu_primitive_constant_pool_builtin_vm() {
    constant_pool_builtin_matches_gnu(Mode::Vm);
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_constant_pool_builtin_jit() {
    constant_pool_builtin_matches_gnu(Mode::Jit);
}

fn assert_cases(name: &str, cases: &[Case], kind: Mutation, mode: Mode, fixture: &str) {
    let expected = gnu_expect(name, &gnu_program(cases, kind), fixture);
    assert_eq!(run_cases(cases, kind, mode), expected, "{kind:?} {mode:?}");
}

#[test]
fn gnu_primitive_opcode_advice_vm() {
    assert_cases(
        "advice",
        OPCODES,
        Mutation::Advice,
        Mode::Vm,
        include_str!("primitive_opcode_cells/advice.expect"),
    );
}

#[test]
fn gnu_primitive_opcode_defalias_vm() {
    assert_cases(
        "defalias",
        OPCODES,
        Mutation::Defalias,
        Mode::Vm,
        include_str!("primitive_opcode_cells/defalias.expect"),
    );
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_opcode_advice_jit() {
    assert_cases(
        "advice",
        OPCODES,
        Mutation::Advice,
        Mode::Jit,
        include_str!("primitive_opcode_cells/advice.expect"),
    );
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_opcode_defalias_jit() {
    assert_cases(
        "defalias",
        OPCODES,
        Mutation::Defalias,
        Mode::Jit,
        include_str!("primitive_opcode_cells/defalias.expect"),
    );
}

#[test]
fn gnu_primitive_bcall_keeps_advice_vm() {
    assert_cases(
        "bcall-advice",
        std::slice::from_ref(&BCALL),
        Mutation::Advice,
        Mode::Vm,
        include_str!("primitive_opcode_cells/bcall-advice.expect"),
    );
}

#[test]
fn gnu_primitive_bcall_keeps_defalias_vm() {
    assert_cases(
        "bcall-defalias",
        std::slice::from_ref(&BCALL),
        Mutation::Defalias,
        Mode::Vm,
        include_str!("primitive_opcode_cells/bcall-defalias.expect"),
    );
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_bcall_keeps_advice_jit() {
    assert_cases(
        "bcall-advice",
        std::slice::from_ref(&BCALL),
        Mutation::Advice,
        Mode::Jit,
        include_str!("primitive_opcode_cells/bcall-advice.expect"),
    );
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_bcall_keeps_defalias_jit() {
    assert_cases(
        "bcall-defalias",
        std::slice::from_ref(&BCALL),
        Mutation::Defalias,
        Mode::Jit,
        include_str!("primitive_opcode_cells/bcall-defalias.expect"),
    );
}

fn overrides_only_affect_genuine_calls(mode: Mode) {
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(
        r#"(progn
      (require 'bytecomp)
      (defvar gnuop--override-calls 0)
      (defun gnuop--override-f (v s)
        (list (aset v 0 7) (upcase s) (string-match "a" s)))
      (byte-compile 'gnuop--override-f)
      (setq gnuop--override-args (list (vector 0) "ab")))"#,
    )
    .expect("compile before overrides");
    let value = ctx.obarray.symbol_function("gnuop--override-f").unwrap();
    let f = value.get_bytecode_data().unwrap();
    assert!(f.executable_ops().iter().any(|op| matches!(op, Op::Aset)));
    assert!(
        f.executable_ops()
            .iter()
            .any(|op| matches!(op, Op::CallBuiltinSym(_, 1)))
    );
    assert!(
        f.executable_ops()
            .iter()
            .any(|op| matches!(op, Op::Call(2)))
    );
    #[cfg(feature = "jit")]
    let leaf = match mode {
        Mode::Vm => None,
        Mode::Jit => Some(compile_native(f)),
    };
    ctx.eval_str(
        r#"(setq internal--compiler-function-overrides
      (mapcar (lambda (name)
                (cons name (lambda (&rest _)
                  (setq gnuop--override-calls (1+ gnuop--override-calls)) 'overridden)))
              '(aset upcase string-match)))"#,
    )
    .expect("local compiler extension");
    let args = list_to_vec(&ctx.eval_str("gnuop--override-args").unwrap()).unwrap();
    let result = match mode {
        Mode::Vm => Vm::from_context(&mut ctx)
            .execute_with_func_value(f, args, value)
            .unwrap(),
        #[cfg(feature = "jit")]
        Mode::Jit => match leaf.unwrap().call((&mut ctx as *mut Context).cast(), &args) {
            crate::emacs_core::jit::compile::NativeRun::Ok(bits) => Value::from_bits(bits),
            other => panic!("override native body: {other:?}"),
        },
    };
    ctx.set_variable("gnuop--override-result", result);
    assert_eq!(print_value(&result), "(7 \"AB\" overridden)");
    assert_eq!(
        ctx.eval_str("gnuop--override-calls").unwrap(),
        Value::make_int(1)
    );
    assert_eq!(
        ctx.eval_str("(funcall 'aset (vector 0) 0 1)").unwrap(),
        Value::symbol("overridden")
    );
    assert_eq!(
        ctx.eval_str("(upcase \"ab\")").unwrap(),
        Value::symbol("overridden")
    );
    assert_eq!(
        ctx.eval_str("gnuop--override-calls").unwrap(),
        Value::make_int(3)
    );
    ctx.eval_str("(setq internal--compiler-function-overrides nil)")
        .unwrap();
}

#[test]
fn compiler_overrides_only_affect_genuine_calls_vm() {
    overrides_only_affect_genuine_calls(Mode::Vm);
}

#[cfg(feature = "jit")]
#[test]
fn compiler_overrides_only_affect_genuine_calls_jit() {
    overrides_only_affect_genuine_calls(Mode::Jit);
}

#[cfg(feature = "jit")]
const SINK_SETUP: &str = r#"(progn
  (require 'bytecomp)
  (defvar gnuop--sink-calls 0)
  (defun gnuop--sink-f (v x)
    (let ((p (cons (copy-sequence x) nil))) (aset v 0 1) (car p)))
  (byte-compile 'gnuop--sink-f)
  (dotimes (_ 100000) (gnuop--sink-f (make-vector 1 0) "abc")))"#;
#[cfg(feature = "jit")]
const SINK_ADVICE: &str = r#"(advice-add 'aset :before
  (lambda (&rest _) (setq gnuop--sink-calls (1+ gnuop--sink-calls)) (garbage-collect)))"#;
#[cfg(feature = "jit")]
const SINK_ARGS: &str = "(setq gnuop--sink-args (list (make-vector 1 0) (make-string 64 ?a)))";
#[cfg(feature = "jit")]
const SINK_OBSERVE: &str =
    "(progn (make-string 64 ?z) (list gnuop--sink-result gnuop--sink-calls))";

#[cfg(feature = "jit")]
fn assert_sink_only_cons_is_virtual_at_aset(
    source: &ByteCodeFunction,
    selected_stats: &crate::emacs_core::jit::opt::sink_recipes::SinkStats,
) {
    use crate::emacs_core::jit::compile;
    use crate::emacs_core::jit::opt::{build, ir, passes::sink, sink_recipes, verify};
    let cfg = compile::analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        source.params.required.len(),
    )
    .expect("exact sink repro CFG");
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    let mut plan = build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: source.params.required.len(),
            ..Default::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("exact sink-only builder input");
    let stats = sink::run(&mut plan, &[]).expect("numeric-free sink-only plan");
    assert_eq!(
        &stats, selected_stats,
        "independent exact-source plan matches selected native census"
    );
    let (aset_index, aset) = plan
        .insts
        .iter()
        .enumerate()
        .find(|(_, inst)| {
            matches!(
                inst.op,
                ir::Opcode::Opaque(Op::Aset) | ir::Opcode::Builtin(Op::Aset)
            )
        })
        .expect("Baset stays in the selected plan");
    let aset_id = ir::Inst(u32::try_from(aset_index).unwrap());
    let owner = plan
        .sink_recipes
        .owners
        .values()
        .find(|owner| matches!(owner.origin, sink_recipes::RecipeOrigin::ConsSource { .. }))
        .expect("fresh cons recipe")
        .owner;
    let native_proof = verify::verify_for_native(&plan).expect("independent complete native proof");
    let recipes = native_proof
        .sink()
        .expect("verified sink recipe capability");
    let before_aset = recipes
        .version_at(sink_recipes::RecipePoint::Before(aset_id), owner)
        .expect("exact cons recipe immediately before Baset");
    assert!(
        !recipes.guaranteed_boxed(before_aset),
        "cons must still be virtual across Baset"
    );
    let mut materializations = 0;
    for inst in &plan.insts {
        if matches!(
            inst.op,
            ir::Opcode::Sink(sink_recipes::SinkOp::MaterializeCons)
        ) {
            materializations += 1;
            assert!(
                inst.pc > aset.pc,
                "cons materialization must occur after Baset"
            );
        }
    }
    assert_eq!(
        materializations, 1,
        "sink-only materializes at Car's later CheckType"
    );
}

#[cfg(feature = "jit")]
fn sink_cannot_call_advised_aset(all_passes: bool) {
    use crate::emacs_core::jit::compile::opt_census::{SelectedTier, snapshot_leaf};
    use crate::emacs_core::jit::compile::{self, NativeRun, OptAdmit, OptMode, OptPasses};
    struct Settings;
    impl Drop for Settings {
        fn drop(&mut self) {
            compile::force_opt_for_test(None, None);
            compile::force_opt_passes_for_test(None);
            compile::force_profit_gate_for_test(true);
        }
    }
    let _settings = Settings;
    compile::force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
    compile::force_opt_passes_for_test(Some(if all_passes {
        OptPasses::ALL
    } else {
        OptPasses {
            sink: true,
            ..OptPasses::default()
        }
    }));
    compile::force_profit_gate_for_test(false);
    let expected = gnu_expect(
        "sink",
        &format!(
            "(progn {SINK_SETUP} {SINK_ADVICE} {SINK_ARGS}
          (setq gnuop--sink-result (apply 'gnuop--sink-f gnuop--sink-args))
          {SINK_OBSERVE})"
        ),
        include_str!("primitive_opcode_cells/sink.expect"),
    );
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(SINK_SETUP).expect("warm exact sink repro");
    let value = ctx.obarray.symbol_function("gnuop--sink-f").unwrap();
    let source = value.get_bytecode_data().unwrap();
    // Opt plans are admitted by an actual tier-two Feedback upgrade request;
    // a Plain compile intentionally retains the baseline even in OptMode.
    let leaf = compile::compile_bytecode_function_requested(
        source,
        Some(&ctx.obarray),
        compile::CompileRequest {
            regalloc: compile::lowering::RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: crate::emacs_core::jit::tier2::CompileTier::Upgrade(
                crate::emacs_core::jit::tier2::T2Upgrade::Feedback,
            ),
        },
    )
    .expect("sink repro tier-two feedback upgrade compiles");
    assert_eq!(
        leaf.selected_tier(),
        SelectedTier::Opt,
        "actual native optimizing tier"
    );
    let sink = snapshot_leaf(&leaf).opt_sink.expect("sink pass selected");
    assert!(
        sink.cons_sources > 0,
        "fresh cons must enter Sink: {sink:?}"
    );
    if all_passes {
        assert!(
            sink.cons_reads_elided > 0,
            "all passes elide the virtual cons read: {sink:?}"
        );
    } else {
        // With only Sink, Car's original CheckType materializes the cons.
        // It does so after Baset: the vulnerable copied string was virtual
        // at the precise operation where old function-cell dispatch ran GC.
        assert_sink_only_cons_is_virtual_at_aset(source, &sink);
    }
    ctx.eval_str(SINK_ADVICE)
        .expect("collecting advice after warmup");
    ctx.eval_str(SINK_ARGS).expect("rooted operands");
    let args = list_to_vec(&ctx.eval_str("gnuop--sink-args").unwrap()).unwrap();
    let NativeRun::Ok(bits) = leaf.call((&mut ctx as *mut Context).cast(), &args) else {
        panic!("sink repro must execute natively without VM fallback");
    };
    // Check before dereferencing the returned string: the old implementation
    // could already have collected the virtual cons's copied string.
    assert_eq!(
        ctx.obarray.symbol_value("gnuop--sink-calls").copied(),
        Some(Value::make_int(0)),
        "Baset never invokes collecting advice"
    );
    ctx.set_variable("gnuop--sink-result", Value::from_bits(bits));
    let result = ctx
        .eval_str(SINK_OBSERVE)
        .expect("allocation after native return");
    assert_eq!(print_value(&result), expected, "all_passes={all_passes}");
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_opcode_sink_ignores_collecting_aset_advice() {
    sink_cannot_call_advised_aset(false);
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_opcode_all_passes_ignore_collecting_aset_advice() {
    sink_cannot_call_advised_aset(true);
}

const INSERT_SETUP: &str = r#"(progn
  (require 'bytecomp)
  (set-buffer (get-buffer-create " *gnuop-hooks*"))
  (erase-buffer)
  (defvar gnuop--hook-log nil)
  (defun gnuop--insert-f (s x)
    (let ((p (cons (copy-sequence x) nil))) (insert s) (car p)))
  (byte-compile 'gnuop--insert-f)
  (setq before-change-functions
    (list (lambda (beg end)
      (push (list 'before beg end) gnuop--hook-log)
      (garbage-collect))))
  (setq after-change-functions
    (list (lambda (beg end old)
      (push (list 'after beg end old) gnuop--hook-log)
      (garbage-collect))))
  (setq gnuop--insert-args (list "abc" (make-string 64 ?a))))"#;
const INSERT_OBSERVE: &str = "(progn (make-string 64 ?z) (list gnuop--insert-result (buffer-string) (nreverse gnuop--hook-log)))";

fn primitive_insert_keeps_its_collecting_hooks(mode: Mode) {
    let expected = gnu_expect(
        "insert-hooks",
        &format!(
            "(progn {INSERT_SETUP}
           (setq gnuop--insert-result (apply 'gnuop--insert-f gnuop--insert-args))
           {INSERT_OBSERVE})"
        ),
        include_str!("primitive_opcode_cells/insert-hooks.expect"),
    );
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(INSERT_SETUP)
        .expect("primitive insert with GC hooks");
    let value = ctx.obarray.symbol_function("gnuop--insert-f").unwrap();
    let source = value.get_bytecode_data().unwrap();
    assert!(
        source
            .executable_ops()
            .iter()
            .any(|op| matches!(op, Op::CallBuiltinSym(_, 1))),
        "insert must use the primitive opcode adapter"
    );
    let args = list_to_vec(&ctx.eval_str("gnuop--insert-args").unwrap()).unwrap();
    let result = match mode {
        Mode::Vm => Vm::from_context(&mut ctx)
            .execute_with_func_value(source, args, value)
            .unwrap(),
        #[cfg(feature = "jit")]
        Mode::Jit => {
            let leaf = compile_native(source);
            match leaf.call((&mut ctx as *mut Context).cast(), &args) {
                crate::emacs_core::jit::compile::NativeRun::Ok(bits) => Value::from_bits(bits),
                other => panic!("insert hook native body: {other:?}"),
            }
        }
    };
    ctx.set_variable("gnuop--insert-result", result);
    let result = ctx
        .eval_str(INSERT_OBSERVE)
        .expect("rooted live string survives primitive hooks");
    assert_eq!(print_value(&result), expected, "{mode:?}");
}

#[test]
fn gnu_primitive_insert_still_runs_collecting_hooks_vm() {
    primitive_insert_keeps_its_collecting_hooks(Mode::Vm);
}

#[cfg(feature = "jit")]
#[test]
fn gnu_primitive_insert_still_runs_collecting_hooks_jit() {
    primitive_insert_keeps_its_collecting_hooks(Mode::Jit);
}
