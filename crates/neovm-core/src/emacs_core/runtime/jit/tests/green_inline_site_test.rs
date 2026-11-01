//! Lever 1 of the green inline-entry guard: a call-free intrinsic site
//! (`InlineSiteReentry::FastPathCannotReenter`) emits no per-site guard, and
//! the definition-change rule it relies on is the RELEASE-POINT
//! revalidation — one epoch compare at every point where this thread has
//! released the Baton (a call return that may re-enter, a landing pad, the
//! back-edge poll's serviced path) in frames whose leaf has inline
//! dependencies. The compiled green loop keeps its safe points too: the
//! back-edge poll for quit/throw-on-input/async attention, and the entry
//! guard itself at inlined FUNCTION entries
//! (`InlineSiteReentry::MayReenter`).

use super::*;
use crate::emacs_core::bytecode::StackDepth;
use crate::emacs_core::error::{FlowKind, FlowRef};
use crate::emacs_core::eval::ThreadingMode;
use crate::emacs_core::jit::{bg, cache, compile};
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = StackDepth::for_test(16);
    f.seal_hand_assembled_ops();
    f
}

/// The compiled-loop row's shape (`wc-loop`: `(while (< i n) ... (mod i 7)
/// ...)`), distilled to what lever 1 acts on: one backedge, inlineable arith
/// intrinsics (`mod` as in the row, plus two-operand `logand`) and nothing
/// else — no varref, no allocation, no real call inside the loop.
///
/// (lambda (n) (let ((i 0) (a 7)) (while (< i n)
///    (setq a (logand a (mod i 7)) i (1+ i))) a))
fn intrinsic_loop() -> ByteCodeFunction {
    let mut f = function(
        vec![
            Op::Constant(1),   // 0: i = 0            [n i]
            Op::Constant(2),   // 1: a = 7            [n i a]
            Op::StackRef(1),   // 2: i                [n i a i]   <- loop head
            Op::StackRef(3),   // 3: n                [n i a i n]
            Op::Lss,           // 4: (< i n)          [n i a b]
            Op::GotoIfNil(18), // 5: exit             [n i a]
            // Callee-FIRST convention (Op::Call reads the callee `nargs`
            // below the top), args left-to-right like GNU's byte compiler;
            // the nested (mod i 7) leaves its result exactly where the outer
            // call wants its last arg, so the body has no stranded temp and
            // the backedge closes at the loop head's depth.
            Op::Constant(4), // 6: 'logand          [n i a logand]
            Op::StackRef(1), // 7: a                [n i a logand a]
            Op::Constant(3), // 8: 'mod             [n i a logand a mod]
            Op::StackRef(4), // 9: i                [n i a logand a mod i]
            Op::Constant(0), // 10: 7               [n i a logand a mod i 7]
            Op::Call(2),     // 11: (mod i 7)       [n i a logand a r]
            Op::Call(2),     // 12: (logand a r)    [n i a a2]
            Op::StackSet(1), // 13: a := a2         [n i a]
            Op::StackRef(1), // 14: i               [n i a i]
            Op::Add1,        // 15:                 [n i a i+1]
            Op::StackSet(2), // 16: i := i+1        [n i a]
            Op::Goto(2),     // 17: backedge
            Op::StackRef(0), // 18: a               [n i a a]
            Op::Return,      // 19
        ],
        vec![
            Value::make_int(7),
            Value::make_int(0),
            Value::make_int(7),
            Value::symbol("mod"),
            Value::symbol("logand"),
        ],
        1,
    );
    f.jit_runtime()
        .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_mul(64));
    f
}

/// Enter the intrinsic loop's OSR leaf at the loop head with N bound large:
/// the only quit/attention service point inside the compiled body is the
/// back-edge poll (the arith sites inline; no shim call remains).
fn run_osr_loop_head(context: &mut Context, source: Value, n: i64) -> Option<NativeRun> {
    let source_data = source.get_bytecode_data().unwrap();
    cache::prepare_osr_for_test(context, source_data, 2);
    // SAFETY: `context` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    unsafe {
        cache::try_run_osr(
            context,
            source_data,
            2,
            &[Value::make_int(n), Value::make_int(0), Value::make_int(7)],
            &[],
        )
    }
}

/// The helper-calling variant of the same loop: ONE real call inside the
/// body (the helper is a closure, never inlined away) with the intrinsics
/// still baking as machine ops.
///
/// (lambda (n) (let ((i 0) (a 7)) (while (< i n)
///    (green-shape-helper)
///    (setq a (logand a (mod i 7)) i (1+ i))) a))
fn helper_loop() -> ByteCodeFunction {
    let mut f = function(
        vec![
            Op::Constant(1),   // 0: i = 0            [n i]
            Op::Constant(2),   // 1: a = 7            [n i a]
            Op::StackRef(1),   // 2: i                [n i a i]   <- loop head
            Op::StackRef(3),   // 3: n                [n i a i n]
            Op::Lss,           // 4: (< i n)          [n i a b]
            Op::GotoIfNil(21), // 5: exit             [n i a]
            Op::Constant(5),   // 6: 'helper          [n i a h]
            Op::Call(0),       // 7: the call         [n i a r]  <- release point
            Op::Pop,           // 8:                  [n i a]
            Op::Constant(4),   // 9: 'logand          [n i a logand]
            Op::StackRef(1),   // 10: a               [n i a logand a]
            Op::Constant(3),   // 11: 'mod            [n i a logand a mod]
            Op::StackRef(4),   // 12: i               [n i a logand a mod i]
            Op::Constant(0),   // 13: 7               [n i a logand a mod i 7]
            Op::Call(2),       // 14: (mod i 7)       [n i a logand a r]
            Op::Call(2),       // 15: (logand a r)    [n i a a2]
            Op::StackSet(1),   // 16: a := a2         [n i a]
            Op::StackRef(1),   // 17: i               [n i a i]
            Op::Add1,          // 18:                 [n i a i+1]
            Op::StackSet(2),   // 19: i := i+1        [n i a]
            Op::Goto(2),       // 20: backedge
            Op::StackRef(0),   // 21: a               [n i a a]
            Op::Return,        // 22
        ],
        vec![
            Value::make_int(7),
            Value::make_int(0),
            Value::make_int(7),
            Value::symbol("mod"),
            Value::symbol("logand"),
            Value::symbol("green-shape-helper"),
        ],
        1,
    );
    f.jit_runtime()
        .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_mul(64));
    f
}

/// Release-point revalidation emissions so far on this thread (debug builds
/// only): the shape gate's measure, one per emitted call site plus one per
/// emitted back-edge poll — zero everywhere else.
#[cfg(debug_assertions)]
fn release_revalidation_emitted() -> u64 {
    crate::emacs_core::jit::compile::lowering::release_revalidations_emitted()
}

/// Per-ISA machine-op witnesses for the structural assertions below. The
/// dump is masked (`captured_code` keeps mnemonics and operand shapes,
/// never encodings or addresses), so each witness matches a SHAPE only the
/// baked lowering can produce in these bodies — never a shim name, which
/// the dump renders as a bare `load_ext_name userextnameN` and which a
/// name-absence check could therefore never catch.
///
/// A target without an arm here fails to COMPILE this file rather than
/// silently skipping the assertions: extend the module in the same change
/// that first lowers the JIT for a new ISA.
#[cfg(target_arch = "x86_64")]
mod baked_op_witnesses {
    /// `andq %r..`: a register-register AND. Tag checks test immediates,
    /// so a register operand is the baked `logand`.
    pub fn register_register_and(text: &str) -> bool {
        text.contains("andq %r")
    }

    /// The lowering's named remainder sequence (no other division exists
    /// in these bodies).
    pub fn truncating_remainder(text: &str) -> bool {
        text.contains("checked_srem_seq")
    }

    /// `shrq $0x3f`: the sign-bit extract of GNU's floor-sign adjustment
    /// (nothing else in the body shifts by 63).
    pub fn floor_sign_adjustment(text: &str) -> bool {
        text.contains("shrq $0x3f")
    }

    /// `addb $0x1`: the every-255th back-edge poll's counter bump — a
    /// byte add no fused-loop-free body can produce.
    pub fn back_edge_poll_bump(text: &str) -> bool {
        text.contains("addb $0x1")
    }

    /// `movq -3(%r..)`: the fused cursor's inline car deref at the cons
    /// tag bias (the spec-call fallback hands the cons to the shim).
    pub fn inline_car_deref(text: &str) -> bool {
        text.contains("movq -3(%r")
    }
}

#[cfg(target_arch = "aarch64")]
mod baked_op_witnesses {
    fn is_reg(s: &str) -> bool {
        let rest = s.strip_prefix(['x', 'w']).unwrap_or("");
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
    }

    /// `and xN, xM, xK` — a register-register AND. Tag checks test
    /// immediates (`and x3, x19, #3`), so a register third operand is the
    /// baked `logand` (or the sign path's own `and w2, w2, w4`, pinned
    /// together with the remainder and floor-sign witnesses).
    pub fn register_register_and(text: &str) -> bool {
        text.lines().any(|line| {
            let Some(rest) = line.trim().strip_prefix("and ") else {
                return false;
            };
            let ops: Vec<&str> = rest.split(", ").collect();
            ops.len() == 3 && is_reg(ops[0]) && is_reg(ops[1]) && is_reg(ops[2])
        })
    }

    /// `sdiv` + `msub`: the truncating remainder pair (nothing else in
    /// these bodies divides).
    pub fn truncating_remainder(text: &str) -> bool {
        text.contains("sdiv") && text.contains("msub")
    }

    /// `cset xN, lt`: the signed flag extract feeding GNU's floor-sign
    /// `csel` (the only `lt` condition extract in these bodies).
    pub fn floor_sign_adjustment(text: &str) -> bool {
        text.lines()
            .any(|line| line.trim().starts_with("cset ") && line.trim().ends_with(", lt"))
    }

    /// The every-255th back-edge poll: a same-register `add N, N, #1`
    /// whose 255 mask of THAT register follows within two lines — either
    /// `ands wzr, N, #255` (register-allocated counter) or `and N, N, #255`
    /// (spilled counter). A bare same-register bump is NOT a witness:
    /// cursor index bumps and slot counters also add `#1` to themselves.
    pub fn back_edge_poll_bump(text: &str) -> bool {
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        (0..lines.len()).any(|i| {
            let Some(rest) = lines[i].strip_prefix("add ") else {
                return false;
            };
            let mut ops = rest.split(", ");
            let Some(dst) = ops.next() else {
                return false;
            };
            if !is_reg(dst) || ops.next() != Some(dst) || ops.next() != Some("#1") {
                return false;
            }
            lines[i + 1..(i + 3).min(lines.len())].iter().any(|l| {
                *l == format!("ands wzr, {dst}, #255") || *l == format!("and {dst}, {dst}, #255")
            })
        })
    }

    /// `ldur xN, [xM, #-3]`: the fused cursor's inline car deref at the
    /// cons tag bias (the spec-call fallback hands the cons to the shim).
    pub fn inline_car_deref(text: &str) -> bool {
        text.lines()
            .any(|line| line.trim().starts_with("ldur ") && line.contains("#-3]"))
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
mod baked_op_witnesses {
    compile_error!(
        "no baked-op witnesses for this target; add them alongside its first JIT lowering"
    );
}

/// Green and sim lower the intrinsic-loop leaf IDENTICALLY: with the guard
/// gone from the non-reentrant arith sites there is no green-only emission
/// left in the loop's compiled code (the pre-lever-1 leaf carried +23 CLIF
/// instructions — two guards per iteration — and differed from sim).
#[test]
fn green_intrinsic_loop_leaf_is_sim_identical() {
    let leaves = |mode: ThreadingMode| {
        let _backend = opt_mode_scope_for_test(OptMode::Off);
        let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
        let _arith = InlineArithScope::on();
        let mut context = Context::new_with_threading(mode);
        context.set_lexical_binding(true);
        let source = Value::make_bytecode(intrinsic_loop());
        let source_data = source.get_bytecode_data().unwrap();
        let code = compile_pipeline_tests::captured_code(|| {
            cache::prepare_entry_for_test(&mut context, source_data);
        });
        if mode == ThreadingMode::Green {
            context.green_teardown();
        }
        assert_eq!(code.len(), 1, "one entry leaf: {code:#?}");
        code
    };
    let sim = leaves(ThreadingMode::Sim);
    let green = leaves(ThreadingMode::Green);
    assert_eq!(
        sim, green,
        "the green intrinsic-loop leaf must be identical to sim's"
    );
    // The leaf really inlined the intrinsics — STRUCTURALLY, not by shim
    // name: the dump renders every shim call as a bare `load_ext_name
    // userextnameN`, so a name-absence check can never fail. The two baked
    // arith sites leave machine ops nothing else in this body can produce:
    // logand is a register-register AND (tag checks test immediates, they
    // never AND two registers), and `mod i 7` bakes the remainder op plus
    // GNU's floor-sign adjustment — truncate-rem then add the divisor when
    // the signs disagree (the ISA's sign extract; nothing else in this
    // body extracts a signed flag).
    let text = green.join("\n");
    assert!(
        baked_op_witnesses::register_register_and(&text),
        "logand must be baked as a register AND: {text}"
    );
    assert!(
        baked_op_witnesses::truncating_remainder(&text),
        "mod must be baked as a remainder machine op: {text}"
    );
    assert!(
        baked_op_witnesses::floor_sign_adjustment(&text),
        "mod must bake GNU's floor-sign adjustment: {text}"
    );
}

/// The wc-loop's shape gate (the compiled-loop row): the leaf HAS inline
/// dependencies (mod/logand bake as machine ops) but no emitted call, so
/// the entire leaf carries exactly ONE release revalidation — the
/// back-edge poll's, in the every-255th serviced block. The fast back-edge
/// path and both arith sites contribute nothing: the hot loop pays no
/// epoch compare at all.
#[test]
#[cfg(debug_assertions)]
fn wc_loop_carries_only_the_polls_release_check() {
    let _backend = opt_mode_scope_for_test(OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let _arith = InlineArithScope::on();
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.set_lexical_binding(true);
    let source = Value::make_bytecode(intrinsic_loop());
    let source_data = source.get_bytecode_data().unwrap();
    let before = release_revalidation_emitted();
    let code = compile_pipeline_tests::captured_code(|| {
        cache::prepare_entry_for_test(&mut context, source_data);
    });
    assert_eq!(code.len(), 1, "one entry leaf: {code:#?}");
    assert_eq!(
        release_revalidation_emitted() - before,
        1,
        "exactly one release check in the wc-loop leaf: the poll's"
    );
}

/// The helper-calling loop's shape gate: one REAL call inside the loop (the
/// helper closure) plus the same poll — exactly TWO release checks, the
/// call's return-side compare and the poll's, and NONE at the arith sites
/// (mod/logand still inline; an inlined intrinsic is not a call boundary).
#[test]
#[cfg(debug_assertions)]
fn helper_loop_carries_exactly_the_return_side_checks() {
    let _backend = opt_mode_scope_for_test(OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let _arith = InlineArithScope::on();
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.set_lexical_binding(true);
    context
        .eval_str("(fset 'green-shape-helper (lambda () nil))")
        .expect("helper");
    let source = Value::make_bytecode(helper_loop());
    let source_data = source.get_bytecode_data().unwrap();
    let before = release_revalidation_emitted();
    let code = compile_pipeline_tests::captured_code(|| {
        cache::prepare_entry_for_test(&mut context, source_data);
    });
    assert_eq!(code.len(), 1, "one entry leaf: {code:#?}");
    assert_eq!(
        release_revalidation_emitted() - before,
        2,
        "one return-side check per call site plus the poll's — nothing else"
    );
}

/// The emission-scope gate for a MAPPING-ONLY leaf (the map-closure row's
/// shape): a real helper call in the physical frame plus `mapc` with an
/// admitted, CALL-FREE bytecode callback (callbacks cannot contain
/// `Op::Call` — `v2_depths` refuses it — so the per-element body is machine
/// ops over values in registers, like the row's `gtr`/`lss`). The in-flight
/// map activation has nothing stale in it — the cursor shims read the live
/// list per step and the callback object was resolved once at entry,
/// exactly like GNU's own `mapc` — and the frame's remaining calls resolve
/// through the live cell (spec sites self-guard per use), so the leaf bakes
/// NO redefinable definition, arms no epoch, and emits ZERO release
/// revalidations: not at the helper's return, not at the map site exit,
/// nowhere. The pre-narrowing epoch armed here (any inline dep) and its
/// checks reshaped the whole leaf's register allocation — the map-closure
/// row's +5.5% (+4 instructions per element, in the loop body's ripple, not
/// at any check). `inline_deps` still records the site (fset mapc evicts
/// the leaf from the cache); a leaf that ALSO bakes an arith callee or a
/// named region keeps arming, and then the map site-exit check protects
/// those.
#[test]
#[cfg(debug_assertions)]
fn hof_only_leaf_emits_no_release_revalidation() {
    let _backend = opt_mode_scope_for_test(OptMode::Legacy);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    struct HofKnobs;
    impl Drop for HofKnobs {
        fn drop(&mut self) {
            crate::emacs_core::jit::inline::force_inline_for_test(None);
            crate::emacs_core::jit::compile::force_inline2_for_test(None);
        }
    }
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    crate::emacs_core::jit::compile::force_inline2_for_test(Some(
        crate::emacs_core::jit::compile::Inline2Mode::Hof,
    ));
    let _knobs = HofKnobs;
    // Bypass the profitability gate for this call-bearing wrapper, restoring
    // the exact previous override (including the unset/Runtime state).
    struct ProfitGuard(compile::ProfitGateTestPolicy);
    impl Drop for ProfitGuard {
        fn drop(&mut self) {
            let _ = compile::replace_profit_gate_for_test(self.0);
        }
    }
    let _profit = ProfitGuard(compile::replace_profit_gate_for_test(
        compile::ProfitGateTestPolicy::Bypass,
    ));
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.set_lexical_binding(true);
    context
        .eval_str("(fset 'green-hof-helper (lambda () nil))")
        .expect("helper");
    // The row's callback shape, call-free by construction: (> x 750) is a
    // machine compare (fixnum feedback), never a call. Hot, or the HOF
    // front's v2_depths rejects it as cold.
    let mut callback_fn = function(
        vec![
            Op::StackRef(0), // 0: x                  [x x]
            Op::Constant(0), // 1: 750                [x x 750]
            Op::Gtr,         // 2: (> x 750)          [x r]
            Op::Return,      // 3
        ],
        vec![Value::make_int(750)],
        1,
    );
    callback_fn.jit_runtime().set_hot_for_test();
    let callback = Value::make_bytecode(callback_fn);
    let f = function(
        vec![
            Op::Constant(2), // 0: 'green-hof-helper  [h]
            Op::Call(0),     // 1: (green-hof-helper) [r]  <- a real call whose
            //                    return-side compare was the old arming's
            Op::Pop,         // 2:                    []
            Op::Constant(0), // 3: 'mapc
            Op::Constant(1), // 4: the callback object
            Op::StackRef(2), // 5: the list
            Op::Call(2),     // 6: (mapc callback list)
            Op::Return,      // 7
        ],
        vec![
            Value::symbol("mapc"),
            callback,
            Value::symbol("green-hof-helper"),
        ],
        1,
    );
    // Admission idiom (inline_hof_runtime_test): hot caller AND callback.
    f.jit_runtime().set_hot_for_test();
    let before = release_revalidation_emitted();
    let code = compile_pipeline_tests::captured_code(|| {
        let _ = compile_bytecode_function_with(&f, Some(&context.obarray))
            .expect("the mapc leaf compiles");
    });
    assert_eq!(
        release_revalidation_emitted() - before,
        0,
        "a mapping-only leaf emits no release revalidation anywhere"
    );
    let text = code.join("\n");
    // Admission witness: the map loop is NATIVE in this leaf. The dump's
    // masking hides shim names AND pool-loaded constants (the callback's 750
    // lives behind a masked pool address, so no immediate ever appears), but
    // the fused cursor leaves two plain instruction shapes nothing else in
    // this body can produce: the every-255 back-edge poll's counter bump
    // plus its mask on the SAME register (no fused loop => no back-edge
    // anywhere in this body; a bare `+1` bump is not a witness — index and
    // slot counters bump themselves too) and the cursor's inline car deref
    // at the cons tag bias (the spec-call fallback hands the cons to the
    // shim and never derefs it here).
    assert!(
        baked_op_witnesses::back_edge_poll_bump(&text),
        "admission check: the native map loop's back-edge poll must be \
         inside the leaf\n{text}"
    );
    assert!(
        baked_op_witnesses::inline_car_deref(&text),
        "admission check: the cursor's inline car deref must be inside the \
         leaf\n{text}"
    );
}

/// The guard still arms at an inlined FUNCTION entry in green: a variable
/// watcher redefines the inlined callee mid-run, and the next `InlineEntry`
/// marker deopts precisely (mir_inline_guards_test covers sim).
#[test]
fn green_inlined_function_entry_still_guards() {
    let _backend = opt_mode_scope_for_test(OptMode::Legacy);
    let _arith = InlineArithScope::on();
    let mut ev = Context::new_with_threading(ThreadingMode::Green);
    ev.eval_str(
        "(setq green-inline-v 0)
        (add-variable-watcher 'green-inline-v (lambda (&rest _)
          (fset 'green-inline-id (lambda (x) (+ x 10)))))",
    )
    .unwrap();
    let id = Value::symbol("green-inline-id");
    let callee = function(vec![Op::StackRef(0), Op::Return], vec![], 1);
    ev.obarray.set_symbol_function_id(
        id.as_bare_symbol_id().unwrap(),
        Value::make_bytecode(callee),
    );
    let f = function(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::Constant(2),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::make_int(1), Value::symbol("green-inline-v"), id],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).unwrap();
    assert_eq!(leaf.tier, leaf::LeafTier::Mir);
    let ctx = &mut ev as *mut Context as *mut u8;
    let result = leaf.call(ctx, &[Value::make_int(5)]);
    let NativeRun::DeoptAt(frame) = &result else {
        panic!("the redefined inlined callee must deopt: {result:?}")
    };
    assert_eq!(frame.pc, 4, "deopt at the original call boundary");
    // The resumed interpreter re-runs the REAL call: the new definition wins.
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
        ..
    } = frame.as_ref();
    let resumed = Vm::from_context(&mut ev).run_resumed_frame(
        &f,
        Value::NIL,
        *pc,
        stack,
        *handlers,
        binds,
        *spec_base,
        *cond_base,
    );
    assert_eq!(resumed.unwrap(), Value::make_int(15));
    assert_eq!(ev.jit_root_stack_top, 0);
    ev.green_teardown();
}

/// Quit interrupts a compiled green loop whose only inline sites are
/// intrinsics: the back-edge poll serves it (`neovm_jit_backedge` →
/// `maybe_quit`), GNU's own `quitcounter` cadence.
#[test]
fn quit_interrupts_compiled_green_intrinsic_loop() {
    let _backend = opt_mode_scope_for_test(OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let _arith = InlineArithScope::on();
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    let source = Value::make_bytecode(intrinsic_loop());
    let _root = context.retain_gc_roots([source]);
    // Raised before entry; the leaf prologue does not poll, so the FIRST
    // service point is the loop's back-edge poll.
    context.quit_requested.request();
    let run = run_osr_loop_head(&mut context, source, 2_000_000)
        .expect("the exact snapshot enters native OSR");
    let NativeRun::Signal = run else {
        panic!("the back-edge poll must serve the quit: {run:?}")
    };
    match shims::take_pending_flow()
        .expect("quit flow stashed")
        .into_kind()
    {
        FlowKind::Signal(sig) => assert!(
            sig.symbol_name().contains("quit"),
            "expected quit, got {}",
            sig.symbol_name()
        ),
        other => panic!("expected a quit signal, got {other:?}"),
    }
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
    context.green_teardown();
}

/// `throw-on-input` interrupts the same loop: the poll promotes queued
/// frontend input into the throw.
#[test]
fn throw_on_input_interrupts_compiled_green_intrinsic_loop() {
    let _backend = opt_mode_scope_for_test(OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let _arith = InlineArithScope::on();
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    let source = Value::make_bytecode(intrinsic_loop());
    let _root = context.retain_gc_roots([source]);
    let tx = crate::host_input::HostInputPublisher::install(&mut context);
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('l'),
    ))
    .expect("queue ordinary frontend input");
    let sentinel = Value::symbol("green-inline-throw-tag");
    context.set_variable("throw-on-input", sentinel);
    let run = run_osr_loop_head(&mut context, source, 2_000_000)
        .expect("the exact snapshot enters native OSR");
    let NativeRun::Signal = run else {
        panic!("the back-edge poll must serve throw-on-input: {run:?}")
    };
    match shims::take_pending_flow()
        .expect("throw-on-input flow stashed")
        .kind()
    {
        FlowRef::Throw(thrown) => {
            assert_eq!(thrown.tag, sentinel);
            assert_eq!(thrown.value, Value::T);
        }
        other => panic!("expected a throw, got {other:?}"),
    }
    assert_eq!(context.jit_root_stack_top, 0);
    context.green_teardown();
}

/// The call-free classification is DERIVED from the same set the lowering
/// bakes: `CannotReenter` exactly for names that are arith intrinsics AND
/// inline (`ash` is an intrinsic that does not inline — it keeps the
/// self-arming shim and stays an original call boundary). Cross-module
/// pin: the effect vocabulary vs the dispatch table.
#[test]
fn call_free_classification_matches_the_inlined_intrinsic_set() {
    use crate::emacs_core::subr::effect_reentry::{FastPathReentry, fast_path_reentry};
    for (name, nargs) in [
        ("logand", 2),
        ("logior", 2),
        ("lognot", 1),
        ("logxor", 2),
        ("mod", 2),
        ("ash", 2),
        ("list", 2),
        ("+", 2),
    ] {
        let expected = arith_intrinsic_op_by_name(name, nargs)
            .map(arith_op_inlines)
            .unwrap_or(false);
        let classified = fast_path_reentry(name) == FastPathReentry::CannotReenter;
        assert_eq!(classified, expected, "{name}");
    }
}

/// Force Level-B inline-arith on for the test (the kill switch reads the
/// process env, which the test must not depend on).
struct InlineArithScope {
    was_on: bool,
}

impl InlineArithScope {
    fn on() -> Self {
        let was_on = jit_inline_arith_on();
        force_inline_arith_for_test(true);
        Self { was_on }
    }
}

impl Drop for InlineArithScope {
    fn drop(&mut self) {
        force_inline_arith_for_test(self.was_on);
    }
}
