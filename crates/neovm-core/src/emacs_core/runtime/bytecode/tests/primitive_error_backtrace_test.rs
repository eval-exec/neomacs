//! GNU bytecode.c records primitive frames only at its explicit cold-error
//! sites. Fixtures are refreshed exclusively from GNU with UPDATE_EXPECT=1.
//! Threading: each test owns its evaluator, roots, and debugger observations.

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::print::print_value;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
enum Primitive {
    Car,
    Cdr,
    Nth,
    Aref,
    Aset,
    Elt,
    Setcar,
    Setcdr,
    Bcall,
}

impl Primitive {
    fn symbol(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Cdr => "cdr",
            Self::Nth => "nth",
            Self::Aref => "aref",
            Self::Aset => "aset",
            Self::Elt => "elt",
            Self::Setcar => "setcar",
            Self::Setcdr => "setcdr",
            Self::Bcall => "string-match",
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::Car => "(car a)",
            Self::Cdr => "(cdr a)",
            Self::Nth => "(nth a b)",
            Self::Aref => "(aref a b)",
            Self::Aset => "(aset a b c)",
            Self::Elt => "(elt a b)",
            Self::Setcar => "(setcar a b)",
            Self::Setcdr => "(setcdr a b)",
            Self::Bcall => "(string-match a b)",
        }
    }

    fn valid_args(self) -> &'static str {
        match self {
            Self::Car | Self::Cdr | Self::Setcar | Self::Setcdr => "(cons 4 5) 7 nil",
            Self::Nth => "0 '(4 5) nil",
            Self::Aref | Self::Aset => "(vector 4 5) 0 7",
            Self::Elt => "'(4 5) 0 nil",
            Self::Bcall => "\"a\" \"abc\" nil",
        }
    }

    fn matches_op(self, op: &Op) -> bool {
        match self {
            Self::Car => matches!(op, Op::Car),
            Self::Cdr => matches!(op, Op::Cdr),
            Self::Nth => matches!(op, Op::Nth),
            Self::Aref => matches!(op, Op::Aref),
            Self::Aset => matches!(op, Op::Aset),
            Self::Elt => matches!(op, Op::Elt),
            Self::Setcar => matches!(op, Op::Setcar),
            Self::Setcdr => matches!(op, Op::Setcdr),
            Self::Bcall => matches!(op, Op::Call(2)),
        }
    }
}

fn setup(primitive: Primitive) -> String {
    format!(
        "(progn
          (require 'bytecomp)
          (defvar carframe--seen nil)
          (defvar carframe--debugger-calls 0)
          (defun carframe--debugger (&rest ignored)
            (setq carframe--debugger-calls (1+ carframe--debugger-calls))
            (garbage-collect)
            (setq carframe--seen
              (delq nil
                (mapcar (lambda (frame)
                  (if (memq (nth 1 frame)
                        '(carframe--debugger carframe--f {})) frame))
                  (backtrace-frames))))
            (throw 'carframe--done nil))
          (defun carframe--f (a b c) {})
          (byte-compile 'carframe--f)
          (dotimes (_ 10000) (carframe--f {})))",
        primitive.symbol(),
        primitive.body(),
        primitive.valid_args()
    )
}

fn observe(args: &str) -> String {
    format!(
        "(progn
          (let ((debug-on-error t) (debugger 'carframe--debugger))
            (catch 'carframe--done (carframe--f {args})))
          (list carframe--debugger-calls carframe--seen))"
    )
}

fn gnu_expect(name: &str, program: &str, fixture: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        assert!(!fixture.is_empty(), "refresh {name} from GNU first");
        return fixture.trim_end().to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = root.join("../../tmp");
    std::fs::create_dir_all(&tmp).expect("oracle temporary directory");
    let script = tmp.join(format!("carframe-{name}-{}.el", std::process::id()));
    std::fs::write(&script, format!(
        ";;; -*- lexical-binding: t; -*-\n(let* ((print-length nil) (print-level nil) (result {program})) (princ \"\\nCARFRAME-RESULT\\n\") (prin1 result))\n"
    )).expect("GNU oracle input");
    // One path for both contexts: `gnu_probe_command` applies the canonical
    // environment and, outside an outer sandbox, the campaign wrapper (an
    // outer sandbox already isolates this test process and its children;
    // nesting systemd scopes from its PID namespace cannot contact the bus).
    let mut command = crate::test_utils::gnu_probe_command();
    let output = command
        .args(["--batch", "-Q", "-l"])
        .arg(&script)
        .output()
        .expect("sandboxed GNU oracle");
    assert!(
        output.status.success(),
        "GNU {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("GNU output UTF-8");
    let expected = stdout
        .rsplit_once("\nCARFRAME-RESULT\n")
        .expect("GNU result sentinel")
        .1
        .trim_end()
        .to_owned();
    assert!(!expected.is_empty(), "GNU must produce an expectation");
    let path = root
        .join("src/emacs_core/runtime/bytecode/tests/primitive_error_backtrace")
        .join(format!("{name}.expect"));
    let refreshed = format!("{}\n", expected.trim_end());
    if std::fs::read_to_string(&path).ok().as_deref() != Some(refreshed.as_str()) {
        std::fs::write(path, refreshed).expect("save GNU fixture");
    }
    expected.trim_end().to_owned()
}

#[derive(Clone, Copy, Debug)]
enum Execution {
    Dispatch,
    #[cfg(feature = "jit")]
    Native(NativeTier),
}

#[cfg(feature = "jit")]
#[derive(Clone, Copy, Debug)]
enum NativeTier {
    Baseline,
    Mir,
    Opt,
}

fn assert_case(name: &str, primitive: Primitive, args: &str, fixture: &str) {
    assert_case_in(name, primitive, args, fixture, Execution::Dispatch);
}

fn assert_case_in(
    name: &str,
    primitive: Primitive,
    args: &str,
    fixture: &str,
    execution: Execution,
) {
    let setup = setup(primitive);
    let observe = observe(args);
    let expected = gnu_expect(name, &format!("(progn {setup} {observe})"), fixture);
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(&setup)
        .expect("compile and warm successful operands");
    let function = ctx
        .obarray
        .symbol_function("carframe--f")
        .expect("compiled function");
    let source = function
        .get_bytecode_data()
        .expect("real GNU bytecode body");
    assert!(
        source
            .executable_ops()
            .iter()
            .any(|op| primitive.matches_op(op)),
        "{primitive:?}: expected primitive opcode, got {:?}",
        source.executable_ops()
    );
    #[cfg(feature = "jit")]
    let native_leaf = match execution {
        Execution::Dispatch => None,
        Execution::Native(tier) => Some(install_exact_tier(&mut ctx, function, tier)),
    };
    #[cfg(not(feature = "jit"))]
    let _ = execution;
    let frontier = (
        ctx.specpdl.len(),
        ctx.condition_stack.len(),
        ctx.bc_buf.len(),
        ctx.depth,
    );
    let value = ctx
        .eval_str(&observe)
        .expect("debugger throws to outer catch");
    assert_eq!(print_value(&value), expected, "{name}");
    #[cfg(feature = "jit")]
    if let Some(leaf) = native_leaf {
        assert_eq!(
            leaf.obs
                .entry_counter()
                .expect("native entry witness")
                .get(),
            1,
            "{name}: the installed selected tier must actually enter native code"
        );
    }
    assert_eq!(
        (
            ctx.specpdl.len(),
            ctx.condition_stack.len(),
            ctx.bc_buf.len(),
            ctx.depth
        ),
        frontier,
        "{name}: debugger unwind restores all activation frontiers"
    );
}

macro_rules! oracle_case {
    ($test:ident, $primitive:ident, $args:expr, $fixture:literal) => {
        #[test]
        fn $test() {
            assert_case(
                $fixture,
                Primitive::$primitive,
                $args,
                include_str!(concat!("primitive_error_backtrace/", $fixture, ".expect")),
            );
        }
    };
}

// Every explicit primitive record_in_backtrace site in bytecode.c:663-1659.
oracle_case!(gnu_bytecode_error_car_frame, Car, "3 nil nil", "car");
oracle_case!(gnu_bytecode_error_cdr_frame, Cdr, "3 nil nil", "cdr");
oracle_case!(gnu_bytecode_error_nth_frame, Nth, "1 '(4 . 5) nil", "nth");
oracle_case!(
    gnu_bytecode_error_aref_index_frame,
    Aref,
    "[4] 'bad nil",
    "aref-index"
);
oracle_case!(
    gnu_bytecode_error_aref_vector_bounds_frame,
    Aref,
    "[4] 2 nil",
    "aref-vector-bounds"
);
oracle_case!(
    gnu_bytecode_error_aref_record_bounds_frame,
    Aref,
    "(record 'tag 4) 9 nil",
    "aref-record-bounds"
);
oracle_case!(
    gnu_bytecode_error_aset_index_frame,
    Aset,
    "(vector 4) 'bad 7",
    "aset-index"
);
oracle_case!(
    gnu_bytecode_error_aset_vector_bounds_frame,
    Aset,
    "(vector 4) 2 7",
    "aset-vector-bounds"
);
oracle_case!(
    gnu_bytecode_error_aset_record_bounds_frame,
    Aset,
    "(record 'tag 4) 9 7",
    "aset-record-bounds"
);
oracle_case!(gnu_bytecode_error_elt_frame, Elt, "'(4 . 5) 1 nil", "elt");
oracle_case!(gnu_bytecode_error_setcar_frame, Setcar, "3 7 nil", "setcar");
oracle_case!(gnu_bytecode_error_setcdr_frame, Setcdr, "3 7 nil", "setcdr");
// C builtin fallbacks have no synthetic primitive frame in GNU bytecode.c.
oracle_case!(
    gnu_bytecode_error_aref_fallback_has_no_frame,
    Aref,
    "3 0 nil",
    "aref-fallback"
);
oracle_case!(
    gnu_bytecode_error_aref_string_bounds_has_no_frame,
    Aref,
    "\"x\" 2 nil",
    "aref-string-bounds"
);
oracle_case!(
    gnu_bytecode_error_aset_fallback_has_no_frame,
    Aset,
    "3 0 7",
    "aset-fallback"
);
oracle_case!(
    gnu_bytecode_error_aset_string_bounds_has_no_frame,
    Aset,
    "(copy-sequence \"x\") 2 7",
    "aset-string-bounds"
);
oracle_case!(
    gnu_bytecode_error_nth_index_has_no_frame,
    Nth,
    "'bad '(4) nil",
    "nth-index"
);
// GNU lisp.h:5835 limits the inlined nth/elt traversal to 127 steps.
oracle_case!(
    gnu_bytecode_error_nth_large_index_has_no_frame,
    Nth,
    "128 '(4 . 5) nil",
    "nth-large-index"
);
oracle_case!(
    gnu_bytecode_error_elt_noncons_has_no_frame,
    Elt,
    "3 1 nil",
    "elt-noncons"
);
// bytecode.c:795's ordinary Bcall record remains a single frame.
oracle_case!(
    gnu_bytecode_error_bcall_frame_control,
    Bcall,
    "3 \"abc\" nil",
    "bcall"
);

#[test]
fn gnu_bytecode_error_condition_case_pops_frame() {
    let setup = setup(Primitive::Car);
    let observe = "(let ((debug-on-error t) (debugger 'carframe--debugger))
      (list (condition-case err (carframe--f 3 nil nil)
              (error (list err (backtrace-frame 0 'car)) ))
            carframe--debugger-calls (backtrace-frame 0 'car)))";
    let expected = gnu_expect(
        "condition-case",
        &format!("(progn {setup} {observe})"),
        include_str!("primitive_error_backtrace/condition-case.expect"),
    );
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(&setup).expect("compile and warm car");
    let frontier = (
        ctx.specpdl.len(),
        ctx.condition_stack.len(),
        ctx.bc_buf.len(),
        ctx.depth,
    );
    for _ in 0..3 {
        let value = ctx
            .eval_str(observe)
            .expect("condition-case catches error without debugger");
        assert_eq!(print_value(&value), expected);
        assert_eq!(
            (
                ctx.specpdl.len(),
                ctx.condition_stack.len(),
                ctx.bc_buf.len(),
                ctx.depth
            ),
            frontier,
            "condition-case must remove the primitive frame and balance specpdl"
        );
    }
}

// The cleanup is installed before the primitive frame. GNU unbind_to pops
// the primitive frame before it evaluates this unwind-protect cleanup.
#[test]
fn gnu_bytecode_error_unwind_protect_pops_primitive_before_cleanup() {
    let setup = setup(Primitive::Car).replace(
        "(car a)",
        "(unwind-protect (car a) (setq carframe--cleanup (backtrace-frame 0 'car)))",
    );
    let observe = format!(
        "(progn {} (list carframe--seen carframe--cleanup (backtrace-frame 0 'car)))",
        observe("3 nil nil")
    );
    let expected = gnu_expect(
        "unwind-protect",
        &format!("(progn {setup} {observe})"),
        include_str!("primitive_error_backtrace/unwind-protect.expect"),
    );
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.set_lexical_binding(true);
    ctx.eval_str(&setup)
        .expect("compile and warm unwind-protect");
    let frontier = (
        ctx.specpdl.len(),
        ctx.condition_stack.len(),
        ctx.bc_buf.len(),
        ctx.depth,
    );
    let value = ctx.eval_str(&observe).expect("debugger throw runs cleanup");
    assert_eq!(print_value(&value), expected);
    assert_eq!(
        (
            ctx.specpdl.len(),
            ctx.condition_stack.len(),
            ctx.bc_buf.len(),
            ctx.depth
        ),
        frontier
    );
}

#[cfg(feature = "jit")]
fn install_exact_tier(
    ctx: &mut crate::emacs_core::eval::Context,
    function: crate::emacs_core::value::Value,
    tier: NativeTier,
) -> std::rc::Rc<crate::emacs_core::jit::compile::CompiledLeaf> {
    use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
    use crate::emacs_core::jit::compile::opt_census::SelectedTier;
    use crate::emacs_core::jit::compile::{
        self, CompileRequest, OptAdmit, OptEarlyMode, OptMode, OptProfitMode,
    };
    use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};
    // This thread owns only compiler scalar overrides. The non-Send guard
    // restores them on early return and assertion failures on the same thread.
    struct Settings(std::marker::PhantomData<std::rc::Rc<()>>);
    static_assertions::assert_not_impl_any!(Settings: Send, Sync);
    impl Drop for Settings {
        fn drop(&mut self) {
            compile::force_opt_for_test(None, None);
            compile::force_opt_profit_for_test(None);
            compile::force_opt_require_osr_for_test(None);
            compile::force_opt_early_for_test(None);
            crate::emacs_core::jit::stats::force_observe_for_test(Default::default());
        }
    }
    let _settings = Settings(std::marker::PhantomData);
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            entry_count: true,
            ..Default::default()
        },
    );
    let (mode, regalloc, request_tier, selected) = match tier {
        NativeTier::Baseline => (
            OptMode::Off,
            RegallocPolicy::Auto,
            CompileTier::Plain,
            SelectedTier::Baseline,
        ),
        NativeTier::Mir => (
            OptMode::Legacy,
            RegallocPolicy::Full,
            CompileTier::Plain,
            SelectedTier::Mir,
        ),
        NativeTier::Opt => (
            OptMode::Opt,
            RegallocPolicy::Full,
            CompileTier::Upgrade(T2Upgrade::Feedback),
            SelectedTier::Opt,
        ),
    };
    compile::force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
    compile::force_opt_profit_for_test(Some(OptProfitMode::Off));
    compile::force_opt_require_osr_for_test(Some(false));
    compile::force_opt_early_for_test(Some(OptEarlyMode::Off));
    let source = function.get_bytecode_data().expect("rooted source");
    let leaf = compile::compile_bytecode_function_requested(
        source,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: request_tier,
        },
    )
    .expect("exact requested native tier compiles");
    assert_eq!(
        leaf.selected_tier(),
        selected,
        "{tier:?} must really execute its selected code producer"
    );
    crate::emacs_core::jit::cache::install_leaf_for_test(ctx, source, leaf)
}

#[cfg(feature = "jit")]
fn assert_native_sites(tier: NativeTier) {
    if !crate::emacs_core::jit::jit_runtime_enabled() {
        return;
    }
    macro_rules! site {
        ($primitive:ident, $args:expr, $fixture:literal) => {
            assert_case_in(
                $fixture,
                Primitive::$primitive,
                $args,
                include_str!(concat!("primitive_error_backtrace/", $fixture, ".expect")),
                Execution::Native(tier),
            );
        };
    }
    site!(Car, "3 nil nil", "car");
    site!(Cdr, "3 nil nil", "cdr");
    site!(Nth, "1 '(4 . 5) nil", "nth");
    site!(Aref, "[4] 'bad nil", "aref-index");
    site!(Aref, "[4] 2 nil", "aref-vector-bounds");
    site!(Aset, "(vector 4) 'bad 7", "aset-index");
    site!(Aset, "(vector 4) 2 7", "aset-vector-bounds");
    site!(Elt, "'(4 . 5) 1 nil", "elt");
    site!(Setcar, "3 7 nil", "setcar");
    site!(Setcdr, "3 7 nil", "setcdr");
}

#[cfg(feature = "jit")]
#[test]
fn gnu_bytecode_error_frames_exact_baseline() {
    assert_native_sites(NativeTier::Baseline);
}

#[cfg(feature = "jit")]
#[test]
fn gnu_bytecode_error_frames_exact_mir() {
    assert_native_sites(NativeTier::Mir);
}

#[cfg(feature = "jit")]
#[test]
fn gnu_bytecode_error_frames_exact_opt() {
    assert_native_sites(NativeTier::Opt);
}
