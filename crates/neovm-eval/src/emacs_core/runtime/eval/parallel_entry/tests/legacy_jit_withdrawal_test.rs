//! Legacy serial JIT withdrawal enforcement regressions (#16614).
//!
//! Proves that once a live parallel era opens:
//! 1. Tree-walk / `apply.rs` seams (`execute_bytecode_call`,
//!    `execute_bytecode_call_dispatched`, `execute_bytecode_call_1`,
//!    `execute_bytecode_call_2`, `dispatch_bytecode_call_from_stack`) record
//!    ZERO new compiles, ZERO new native entries, ZERO `COMPILED` cache
//!    entries, and `SHARED_JIT == None`, while returning exact Lisp results
//!    (both on Main and inside admitted parallel owners).
//! 2. `Vm` paths — including a **stale outer `Vm`** constructed BEFORE era open
//!    that later runs both `Bcall` (`Vm::dispatch_bytecode_tier`) and hot
//!    backward-branch OSR (`vm.rs`) AFTER era open — record ZERO new compiles,
//!    ZERO new native entries, ZERO `COMPILED` entries, and `SHARED_JIT == None`.
//! 3. Opening an era while `native_depth() != 0` (from inside a native-executing
//!    compiled leaf) refuses cleanly (`NativeActivation`), falls back to Baton
//!    with `neomacs-thread-parallel-p` = `nil`, and leaves `SoloActive` intact.
//!
//! GNU Emacs Oracle: UNATTESTED — legacy serial JIT tier residency and
//! withdrawal state are Neomacs-internal runtime invariants, not Elisp-visible
//! GNU Emacs semantics.

#![cfg(feature = "jit")]

use super::live_admission::{LiveEraOpenFault, set_era_open_fault_for_test};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::function_slots::StackDepth;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt as _;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::jit;
use crate::emacs_core::print::print_value;
use crate::emacs_core::threads::with_parallel_admission_enabled_for_test;
use neovm_value::intern::intern;
use neovm_value::value::{LambdaParams, Value};

fn green_jit_context() -> Context {
    jit::force_jit_off_for_test(false);
    jit::stats::force_observe_for_test(jit::stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
    let _ = jit::cache::clear_with_serial_receipt();
    jit::compile::shared::reset_carrier_tls();
    jit::stats::reset_compile_stats();

    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    // SAFETY: fresh serial test fixture before any concurrent activity.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut ctx.tagged_heap,
            true,
        );
    }
    ctx.setup_thread_locals();
    ctx.start_green_threads()
        .expect("start Green threads on Main");
    ctx
}

fn eval_printed(ctx: &mut Context, src: &str) -> String {
    let val = ctx
        .eval_str(src)
        .unwrap_or_else(|err| panic!("eval failed for `{src}`: {err:?}"));
    print_value(&val)
}

/// `(lambda (x) (1+ (1+ x)))` — pure arithmetic, JIT-profitable.
fn make_bc_unary_add2() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("x")]));
    f.set_lexical(true);
    f.set_ops(vec![Op::StackRef(0), Op::Add1, Op::Add1, Op::Return]);
    f.set_constants(vec![].into());
    f.set_max_stack(StackDepth::for_test(4));
    Value::make_bytecode(f)
}

/// `(lambda (x y) (1+ (+ x y)))` — pure 2-arg arithmetic, JIT-profitable.
fn make_bc_binary_add1() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("x"), intern("y")]));
    f.set_lexical(true);
    f.set_ops(vec![
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Add,
        Op::Add1,
        Op::Return,
    ]);
    f.set_constants(vec![].into());
    f.set_max_stack(StackDepth::for_test(4));
    Value::make_bytecode(f)
}

/// `(lambda (x y) (< x y))` — 2-arg comparator for `sort` (`apply2_bytecode`).
fn make_bc_binary_lss() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("x"), intern("y")]));
    f.set_lexical(true);
    f.set_ops(vec![Op::StackRef(1), Op::StackRef(1), Op::Lss, Op::Return]);
    f.set_constants(vec![].into());
    f.set_max_stack(StackDepth::for_test(4));
    Value::make_bytecode(f)
}

/// Outer bytecode function `(lambda (n) ...)` that calls `hook_sym` once,
/// then loops `n` times calling `(callee_sym acc)` via `Op::Call(1)` (`Bcall`)
/// with a backward branch (`Op::Goto`).
fn make_bc_outer_bcall_loop(hook_sym: &'static str, callee_sym: &'static str) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("n")]));
    f.set_lexical(true);
    // Stack on entry: [n]
    f.set_ops(vec![
        // 0..=2: (hook_sym)
        Op::Constant(0),
        Op::Call(0),
        Op::Pop,
        // 3: acc = 0 -> stack: [n, acc]
        Op::Constant(2),
        // 4..=7: loop header — while 0 < n
        Op::Constant(2),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(16),
        // 8..=11: acc = (callee_sym acc) via Bcall
        Op::Constant(1),
        Op::StackRef(1),
        Op::Call(1),
        Op::StackSet(1),
        // 12..=15: n = (1- n); goto 4 (backward branch)
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(4),
        // 16: return acc
        Op::Return,
    ]);
    f.set_constants(
        vec![
            Value::symbol(hook_sym),
            Value::symbol(callee_sym),
            Value::fixnum(0),
        ]
        .into(),
    );
    f.set_max_stack(StackDepth::for_test(8));
    Value::make_bytecode(f)
}

/// Outer bytecode function `(lambda (n) ...)` that calls `hook_sym` once,
/// then runs a pure-arithmetic backward-branch loop (`n` iterations of
/// `acc = (1+ (1+ acc))`) guaranteed to trigger OSR when JIT is active.
fn make_bc_outer_pure_osr_loop(hook_sym: &'static str) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("n")]));
    f.set_lexical(true);
    // Stack on entry: [n]
    f.set_ops(vec![
        // 0..=2: (hook_sym)
        Op::Constant(0),
        Op::Call(0),
        Op::Pop,
        // 3: acc = 0 -> stack: [n, acc]
        Op::Constant(1),
        // 4..=7: loop header — while 0 < n
        Op::Constant(1),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(16),
        // 8..=11: acc = (1+ (1+ acc))
        Op::StackRef(0),
        Op::Add1,
        Op::Add1,
        Op::StackSet(1),
        // 12..=15: n = (1- n); goto 4 (backward branch -> OSR probe)
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(4),
        // 16: return acc
        Op::Return,
    ]);
    f.set_constants(vec![Value::symbol(hook_sym), Value::fixnum(0)].into());
    f.set_max_stack(StackDepth::for_test(8));
    Value::make_bytecode(f)
}

/// `(lambda (x) (let ((y (1+ (1+ (1+ (+ x x)))))) (if (< y 0) (hook_sym) y)))`
/// Arithmetic-heavy so it compiles to a native leaf during positive warm-up,
/// then invokes `hook_sym` from INSIDE the native leaf (`native_depth() > 0`)
/// when called with a negative `x`.
fn make_bc_native_trigger(hook_sym: &'static str) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("x")]));
    f.set_lexical(true);
    // Stack on entry: [x]
    f.set_ops(vec![
        // 0..=5: y = 1+(1+(1+(x + x))) -> stack: [x, y]
        Op::StackRef(0),
        Op::StackRef(1),
        Op::Add,
        Op::Add1,
        Op::Add1,
        Op::Add1,
        // 6..=9: if (< y 0) goto 11 else return y
        Op::Dup,
        Op::Constant(1),
        Op::Lss,
        Op::GotoIfNil(13),
        // 10..=12: (hook_sym) and return its value
        Op::Constant(0),
        Op::Call(0),
        Op::Return,
        // 13: return y
        Op::Return,
    ]);
    f.set_constants(vec![Value::symbol(hook_sym), Value::fixnum(0)].into());
    f.set_max_stack(StackDepth::for_test(8));
    Value::make_bytecode(f)
}

#[test]
fn legacy_jit_withdraw_row1_tree_walk_apply_seams_zero_compile_and_zero_native_entries() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let warm_calls = jit::hot_threshold() as i64 + 200;

        let bc_u1 = make_bc_unary_add2();
        let bc_b1 = make_bc_binary_add1();
        let bc_lt = make_bc_binary_lss();
        ctx.obarray.set_symbol_function("ljw-u1", bc_u1);
        ctx.obarray.set_symbol_function("ljw-b1", bc_b1);
        ctx.obarray.set_symbol_function("ljw-lt", bc_lt);

        // Pre-era warm-up: prove the legacy serial JIT ACTUALLY compiles and
        // enters native leaves before a parallel era opens.
        let pre_res = eval_printed(
            &mut ctx,
            &format!(
                r#"(let ((i 0) (acc 0))
                     (while (< i {warm_calls})
                       (setq acc (ljw-u1 i))
                       (setq acc (funcall #'ljw-b1 acc 1))
                       (setq i (1+ i)))
                     (list acc
                           (length (mapcar #'ljw-u1 (make-list 300 5)))
                           (car (sort (list 9 4 7 1 8 2 6 3 5 0) #'ljw-lt))))"#
            ),
        );
        assert_eq!(pre_res, format!("({} 300 0)", warm_calls + 3));

        let pre_stats = jit::stats::compile_stats_snapshot();
        assert!(
            pre_stats.total_compiles >= 1,
            "pre-era warm-up must compile at least once (got {pre_stats:?})"
        );
        assert!(
            pre_stats.native_entries >= 1,
            "pre-era warm-up must enter native code at least once (got {pre_stats:?})"
        );
        assert!(
            jit::cache::compiled_entry_count_for_test() > 0,
            "pre-era warm-up must populate COMPILED cache"
        );
        assert!(
            jit::compile::shared::shared_jit_is_installed_for_test(),
            "pre-era warm-up must install SHARED_JIT"
        );

        // Open the live parallel era.
        assert!(
            !ctx.legacy_jit_withdrawn(),
            "legacy_jit_withdrawn must be false before era open"
        );
        let open_res = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 99) "ljw-r1-open")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(open_res, "(99 t)");
        assert!(ctx.green_control().era_is_open());
        assert!(
            ctx.legacy_jit_withdrawn(),
            "legacy_jit_withdrawn must be true once parallel era opens"
        );

        // Immediately after era open, legacy cache and backend MUST be withdrawn.
        assert_eq!(
            jit::cache::compiled_entry_count_for_test(),
            0,
            "COMPILED cache must have ZERO entries after era open"
        );
        assert!(
            !jit::compile::shared::shared_jit_is_installed_for_test(),
            "SHARED_JIT must be None after era open"
        );

        // Bind fresh cold byte-compiled functions post-era too, to prove neither
        // previously-hot nor newly-hot functions can re-tier or re-arm.
        let bc_u2 = make_bc_unary_add2();
        let bc_b2 = make_bc_binary_add1();
        let bc_lt2 = make_bc_binary_lss();
        ctx.obarray.set_symbol_function("ljw-u2", bc_u2);
        ctx.obarray.set_symbol_function("ljw-b2", bc_b2);
        ctx.obarray.set_symbol_function("ljw-lt2", bc_lt2);

        let snap_after_open = jit::stats::compile_stats_snapshot();

        // Exercise ALL 5 apply.rs bytecode call seams past `hot_threshold()`:
        // - `execute_bytecode_call` + `execute_bytecode_call_dispatched` (eval / funcall / apply)
        // - `execute_bytecode_call_1` (`mapcar` + direct `apply1_bytecode`)
        // - `execute_bytecode_call_2` (`sort` + direct `apply2_bytecode`)
        // - `dispatch_bytecode_call_from_stack` (also covered via Row 2)
        // Plus execution inside an admitted parallel owner!
        let post_res = eval_printed(
            &mut ctx,
            &format!(
                r#"(let ((i 0) (acc 0))
                     (while (< i {warm_calls})
                       (setq acc (ljw-u1 i))
                       (setq acc (ljw-u2 acc))
                       (setq acc (funcall #'ljw-b1 acc 1))
                       (setq acc (apply #'ljw-b2 (list acc 2)))
                       (setq i (1+ i)))
                     (let* ((m1 (mapcar #'ljw-u1 (make-list 300 10)))
                            (m2 (mapcar #'ljw-u2 (make-list 300 20)))
                            (s1 (sort (list 9 4 7 1 8 2 6 3 5 0) #'ljw-lt))
                            (s2 (sort (list 9 4 7 1 8 2 6 3 5 0) #'ljw-lt2))
                            (par (neomacs-make-parallel-thread
                                  (lambda ()
                                    (let ((j 0) (p 0))
                                      (while (< j {warm_calls})
                                        (setq p (ljw-u2 j))
                                        (setq j (1+ j)))
                                      p))
                                  "ljw-r1-par")))
                       (list acc (car m1) (car m2) (car s1) (car s2)
                             (thread-join par) (neomacs-thread-parallel-p par))))"#
            ),
        );
        assert_eq!(
            post_res,
            format!("({} 12 22 0 0 {} t)", warm_calls + 8, warm_calls + 1)
        );

        // Also directly exercise `apply1_bytecode` and `apply2_bytecode` past `hot_threshold()`.
        let mut last1 = Value::NIL;
        let mut last2 = Value::NIL;
        for i in 0..warm_calls {
            last1 = ctx
                .apply1_bytecode::<true>(bc_u2, Value::fixnum(i))
                .expect("apply1_bytecode succeeds");
            last2 = ctx
                .apply2_bytecode(bc_b2, Value::fixnum(i), Value::fixnum(3))
                .expect("apply2_bytecode succeeds");
        }
        assert_eq!(print_value(&last1), format!("{}", warm_calls + 1));
        assert_eq!(print_value(&last2), format!("{}", warm_calls + 3));

        let snap_after_post = jit::stats::compile_stats_snapshot();
        assert_eq!(
            snap_after_post.total_compiles, snap_after_open.total_compiles,
            "ZERO new JIT compiles allowed after parallel era open (before={snap_after_open:?}, after={snap_after_post:?})"
        );
        assert_eq!(
            snap_after_post.native_entries, snap_after_open.native_entries,
            "ZERO new native entries allowed after parallel era open (before={snap_after_open:?}, after={snap_after_post:?})"
        );
        assert_eq!(
            jit::cache::compiled_entry_count_for_test(),
            0,
            "COMPILED cache must remain empty after post-era calls"
        );
        assert!(
            !jit::compile::shared::shared_jit_is_installed_for_test(),
            "SHARED_JIT must remain None after post-era calls"
        );

        ctx.green_teardown();
    });
}

#[test]
fn legacy_jit_withdraw_row2_stale_outer_vm_bcall_and_osr_zero_compile() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let warm_calls = jit::hot_threshold() as i64 + 200;
        // 256 backward branches = 1 wrap (+8 loop heat); cross `hot_threshold()` with margin.
        let osr_iters = ((jit::hot_threshold() as i64 / 8) + 20) * 256;

        // First prove pre-era OSR actually compiles & enters natively on the
        // exact same pure-arith loop shape!
        eval_printed(&mut ctx, "(fset 'ljw-noop (lambda () nil))");
        let bc_pre_osr = make_bc_outer_pure_osr_loop("ljw-noop");
        ctx.obarray.set_symbol_function("ljw-pre-osr", bc_pre_osr);
        assert_eq!(
            eval_printed(&mut ctx, &format!("(ljw-pre-osr {osr_iters})")),
            format!("{}", osr_iters * 2)
        );

        let pre_stats = jit::stats::compile_stats_snapshot();
        let pre_origin_compiles: u64 = pre_stats.origins.iter().map(|o| o.count).sum();
        assert!(
            pre_origin_compiles >= 1,
            "pre-era pure-arith OSR loop must compile via OSR (got {pre_stats:?})"
        );
        assert!(
            jit::cache::osr_entry_count_for_test() > 0,
            "pre-era pure-arith OSR loop must populate OSR_CACHE"
        );
        assert!(jit::compile::shared::shared_jit_is_installed_for_test());

        // Also warm up `ljw-callee` pre-era so it is already hot when the stale
        // outer `Vm` calls it via `Bcall` after opening the era.
        let bc_callee = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-callee", bc_callee);
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((i 0) (a 0)) (while (< i {warm_calls}) (setq a (ljw-callee i)) (setq i (1+ i))) a)"
                )
            ),
            format!("{}", warm_calls + 1)
        );

        // Hook called from INSIDE the cold Tier-0 outer `Vm` (`native_depth() == 0`)
        // to open the live parallel era mid-activation, making the outer `Vm`'s
        // cached `bytecode_tier_policy` STALE!
        eval_printed(
            &mut ctx,
            r#"(fset 'ljw-open-era-hook
                     (lambda ()
                       (let ((th (neomacs-make-parallel-thread (lambda () 7) "ljw-r2-open")))
                         (if (null (and (eq (thread-join th) 7)
                                        (neomacs-thread-parallel-p th)))
                             (error "expected parallel admission inside outer Vm")))))"#,
        );

        // 2a: Stale outer `Vm` opens the era, then runs `osr_iters` pure-arith
        // backward branches (OSR hot path) inside that SAME stale `Vm`.
        let bc_stale_osr = make_bc_outer_pure_osr_loop("ljw-open-era-hook");
        ctx.obarray
            .set_symbol_function("ljw-stale-osr", bc_stale_osr);

        assert_eq!(
            eval_printed(&mut ctx, &format!("(ljw-stale-osr {osr_iters})")),
            format!("{}", osr_iters * 2)
        );
        assert!(ctx.green_control().era_is_open());
        assert!(ctx.legacy_jit_withdrawn());

        let snap_after_stale_osr = jit::stats::compile_stats_snapshot();
        let after_osr_origins: u64 = snap_after_stale_osr.origins.iter().map(|o| o.count).sum();
        assert_eq!(
            after_osr_origins, 0,
            "stale outer Vm OSR loop must record ZERO compiles across all origins after era open (after={snap_after_stale_osr:?})"
        );
        assert_eq!(
            snap_after_stale_osr.total_compiles, 0,
            "stale outer Vm OSR loop must record ZERO entry compiles after era open"
        );
        assert_eq!(
            snap_after_stale_osr.native_entries, 0,
            "stale outer Vm OSR loop must record ZERO native entries after era open"
        );
        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());

        // 2b: Now test a SECOND fresh Context where the stale outer `Vm` opens
        // the era mid-activation and then runs `warm_calls` `Op::Call(1)` (`Bcall`)
        // calls to a pre-warmed hot bytecode callee + backward branches in that
        // SAME stale outer `Vm`.
        ctx.green_teardown();
        drop(ctx);

        let mut ctx2 = green_jit_context();
        let bc_callee2 = make_bc_unary_add2();
        ctx2.obarray.set_symbol_function("ljw-callee2", bc_callee2);
        assert_eq!(
            eval_printed(
                &mut ctx2,
                &format!(
                    "(let ((i 0) (a 0)) (while (< i {warm_calls}) (setq a (ljw-callee2 i)) (setq i (1+ i))) a)"
                )
            ),
            format!("{}", warm_calls + 1)
        );
        let pre2 = jit::stats::compile_stats_snapshot();
        assert!(pre2.total_compiles >= 1 && pre2.native_entries >= 1);

        eval_printed(
            &mut ctx2,
            r#"(fset 'ljw-open-era-hook2
                     (lambda ()
                       (let ((th (neomacs-make-parallel-thread (lambda () 11) "ljw-r2b-open")))
                         (if (null (and (eq (thread-join th) 11)
                                        (neomacs-thread-parallel-p th)))
                             (error "expected parallel admission inside outer Vm (Bcall)")))))"#,
        );
        let bc_stale_bcall = make_bc_outer_bcall_loop("ljw-open-era-hook2", "ljw-callee2");
        ctx2.obarray
            .set_symbol_function("ljw-stale-bcall", bc_stale_bcall);

        assert_eq!(
            eval_printed(&mut ctx2, &format!("(ljw-stale-bcall {warm_calls})")),
            format!("{}", warm_calls * 2)
        );
        assert!(ctx2.green_control().era_is_open());
        assert!(ctx2.legacy_jit_withdrawn());

        let snap_after_bcall = jit::stats::compile_stats_snapshot();
        let after_bcall_origins: u64 = snap_after_bcall.origins.iter().map(|o| o.count).sum();
        assert_eq!(
            after_bcall_origins, 0,
            "stale outer Vm Bcall loop must record ZERO compiles across all origins after era open (after={snap_after_bcall:?})"
        );
        assert_eq!(
            snap_after_bcall.total_compiles, 0,
            "stale outer Vm Bcall loop must record ZERO entry compiles after era open"
        );
        assert_eq!(
            snap_after_bcall.native_entries, 0,
            "stale outer Vm Bcall loop must record ZERO native entries after era open"
        );
        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());

        ctx2.green_teardown();
    });
}

#[test]
fn legacy_jit_withdraw_row3_native_depth_nonzero_refuses_cleanly() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let warm_calls = jit::hot_threshold() as i64 + 200;

        eval_printed(
            &mut ctx,
            r#"(fset 'ljw-native-hook
                     (lambda ()
                       (let ((th (neomacs-make-parallel-thread (lambda () 123) "ljw-r3-native")))
                         (list (thread-join th) (neomacs-thread-parallel-p th)))))"#,
        );
        let bc_trigger = make_bc_native_trigger("ljw-native-hook");
        ctx.obarray.set_symbol_function("ljw-trigger", bc_trigger);

        // Warm up `ljw-trigger` with positive `x` so it compiles and enters a
        // native leaf without invoking `ljw-native-hook`.
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((i 1) (r 0)) (while (< i {warm_calls}) (setq r (ljw-trigger i)) (setq i (1+ i))) r)"
                )
            ),
            format!("{}", (warm_calls - 1) * 2 + 3)
        );
        let pre = jit::stats::compile_stats_snapshot();
        assert!(
            pre.total_compiles >= 1 && pre.native_entries >= 1,
            "ljw-trigger must be running natively before testing native_depth() != 0 (got {pre:?})"
        );

        // Call `ljw-trigger` with negative `x`: enters native leaf (`native_depth() == 1`)
        // and calls `ljw-native-hook` -> `neomacs-make-parallel-thread`.
        // Must refuse cleanly (NativeActivation) and fall back to Baton with hint `nil`.
        let native_out = eval_printed(&mut ctx, "(ljw-trigger -10)");
        assert_eq!(
            native_out, "(123 nil)",
            "opt-in inside active native JIT leaf must refuse and fall back to Baton"
        );
        let control = ctx.green_control();
        assert!(
            !control.era_is_open(),
            "native_depth() != 0 refusal must NOT open a parallel era"
        );
        assert!(
            control.era_state_is_solo_active(),
            "native_depth() != 0 refusal must leave Control in SoloActive"
        );

        // Once back outside native code (`native_depth() == 0`), opt-in succeeds!
        let later_out = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 456) "ljw-r3-later")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(later_out, "(456 t)");
        assert!(control.era_is_open());
        assert!(ctx.legacy_jit_withdrawn());
        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());

        ctx.green_teardown();
    });
}

fn assert_contract_breach_panic(label: &str, f: impl FnOnce()) {
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .expect_err(&format!("{label} must panic fail-closed after withdrawal"));
    let msg = err
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| err.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic>");
    assert!(
        msg.contains("contract breach"),
        "{label} panic must report a contract breach (got {msg:?})"
    );
}

#[test]
fn legacy_jit_withdraw_row4_fail_closed_all_five_cache_guards_panic_while_era_open() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let bc_val = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-fc", bc_val);

        let open_res = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 5) "ljw-r4")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(open_res, "(5 t)");
        assert!(ctx.green_control().era_is_open());
        assert!(ctx.legacy_jit_withdrawn());

        let bc_data = bc_val.get_bytecode_data().expect("bytecode function");
        let ctx_ptr: *mut Context = &mut ctx;

        assert_contract_breach_panic("try_run_compiled", || {
            let _ = unsafe {
                jit::cache::try_run_compiled(ctx_ptr, bc_data, bc_val, &[Value::fixnum(1)])
            };
        });
        assert_contract_breach_panic("arm_leaf_slot", || unsafe {
            jit::cache::arm_leaf_slot(ctx_ptr, bc_data);
        });
        assert_contract_breach_panic("run_armed_leaf", || {
            let arg = Value::fixnum(1).bits() as i64;
            let _ = unsafe {
                jit::cache::run_armed_leaf(
                    ctx_ptr,
                    bc_data,
                    bc_val,
                    std::ptr::null(),
                    std::ptr::from_ref(&arg),
                )
            };
        });
        assert_contract_breach_panic("try_run_osr_probe", || {
            let _ = unsafe { jit::cache::try_run_osr_probe(ctx_ptr, bc_data, 0, &[], &[]) };
        });
        assert_contract_breach_panic("resolve_spec_call_leaf", || {
            let _ = unsafe { jit::cache::resolve_spec_call_leaf(ctx_ptr, bc_data) };
        });

        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());

        ctx.green_teardown();
    });
}

#[test]
#[should_panic(
    expected = "contract breach: try_run_compiled called after legacy serial JIT withdrawal"
)]
fn legacy_jit_withdraw_row4b_fail_closed_should_panic_try_run_compiled() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let bc_val = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-fc-sp", bc_val);
        let open_res = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 6) "ljw-r4b")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(open_res, "(6 t)");
        ctx.green_teardown();
        assert!(ctx.legacy_jit_withdrawn());

        let bc_data = bc_val.get_bytecode_data().expect("bytecode function");
        let ctx_ptr: *mut Context = &mut ctx;
        let _ =
            unsafe { jit::cache::try_run_compiled(ctx_ptr, bc_data, bc_val, &[Value::fixnum(1)]) };
    });
}

/// P1 rollback-safety regression (#16631): injecting `FailAfterOpenBeforeAcquire`
/// (after `transfer_to_keeper` + `await_ready` + `open_era_on_control`, before
/// `commit()`) must NOT invoke one-way `DetachedSourceCache::withdraw`, must
/// roll back cleanly to `SoloActive` with `legacy_jit_withdrawn() == false`,
/// and must leave Main's legacy serial JIT fully operational until a subsequent
/// era open actually commits.
#[test]
fn legacy_jit_withdraw_row5a_late_rollback_does_not_withdraw_and_preserves_jit() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let warm_calls = jit::hot_threshold() as i64 + 200;

        let bc_u1 = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-r5a-u1", bc_u1);

        // Warm up pre-era so COMPILED and SHARED_JIT are populated.
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((i 0) (a 0)) (while (< i {warm_calls}) (setq a (ljw-r5a-u1 i)) (setq i (1+ i))) a)"
                )
            ),
            format!("{}", warm_calls + 1)
        );
        let pre = jit::stats::compile_stats_snapshot();
        assert!(pre.total_compiles >= 1 && pre.native_entries >= 1);
        assert!(jit::cache::compiled_entry_count_for_test() > 0);
        assert!(jit::compile::shared::shared_jit_is_installed_for_test());
        assert!(!ctx.legacy_jit_withdrawn());

        // Record rejection epoch immediately before injecting late rollback.
        // Clean rollback runs `resume_with_turn_right` -> `clear()` once (+1);
        // a premature `DetachedSourceCache::withdraw` before commit would bump
        // it a second time (+2) and leak ManuallyDrop receipts.
        let epoch_before = jit::cache::rejection_epoch();
        set_era_open_fault_for_test(Some(LiveEraOpenFault::FailAfterOpenBeforeAcquire));
        let rollback_out = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 77) "ljw-r5a-rb")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(rollback_out, "(77 nil)");
        let control = ctx.green_control();
        assert!(!control.era_is_open());
        assert!(control.era_state_is_solo_active());
        assert!(
            !ctx.legacy_jit_withdrawn(),
            "late rollback before commit must keep legacy_jit_withdrawn == false"
        );
        assert_eq!(
            jit::cache::rejection_epoch(),
            epoch_before + 1,
            "late rollback must NOT run DetachedSourceCache::withdraw before commit"
        );

        // Prove Main's legacy serial JIT is still fully operational after rollback.
        let bc_u2 = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-r5a-u2", bc_u2);
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((i 0) (a 0)) (while (< i {warm_calls}) (setq a (ljw-r5a-u2 i)) (setq i (1+ i))) a)"
                )
            ),
            format!("{}", warm_calls + 1)
        );
        let post_rb = jit::stats::compile_stats_snapshot();
        assert!(
            post_rb.total_compiles >= 1 && post_rb.native_entries >= 1,
            "Main's legacy JIT must still compile & enter natively after late rollback (got {post_rb:?})"
        );
        assert!(jit::cache::compiled_entry_count_for_test() > 0);
        assert!(jit::compile::shared::shared_jit_is_installed_for_test());

        // Subsequent clean opt-in commits, withdraws JIT, and enforces zero compiles.
        let commit_out = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 88) "ljw-r5a-ok")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(commit_out, "(88 t)");
        assert!(control.era_is_open());
        assert!(ctx.legacy_jit_withdrawn());
        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());

        ctx.green_teardown();
    });
}

/// P2 explicit session-lifetime contract (#16631): legacy serial JIT withdrawal
/// is monotonic (`false -> true` at first era-open commit) and remains `true`
/// after `green_teardown()` because `WithdrawnLegacyJit`'s `ManuallyDrop`
/// receipts are permanently one-way. Post-teardown execution on the same
/// session must stay interpreter-only with ZERO compiles and ZERO native entries.
#[test]
fn legacy_jit_withdraw_row5b_monotonic_session_lifetime_stays_withdrawn_after_teardown() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_jit_context();
        let warm_calls = jit::hot_threshold() as i64 + 200;

        let open_out = eval_printed(
            &mut ctx,
            r#"(let ((th (neomacs-make-parallel-thread (lambda () 55) "ljw-r5b")))
                 (list (thread-join th) (neomacs-thread-parallel-p th)))"#,
        );
        assert_eq!(open_out, "(55 t)");
        let control = ctx.green_control();
        assert!(control.era_is_open());
        assert!(ctx.legacy_jit_withdrawn());

        // Tear down the live era (`teardown_live_parallel_era` drops `LiveEraRuntime`).
        ctx.green_teardown();
        assert!(!control.era_is_open());
        assert!(
            ctx.legacy_jit_withdrawn(),
            "monotonic session-lifetime contract: legacy_jit_withdrawn stays true after green_teardown()"
        );

        let snap_after_td = jit::stats::compile_stats_snapshot();
        let origins_after_td: u64 = snap_after_td.origins.iter().map(|o| o.count).sum();

        // Execute hot bytecode past `hot_threshold()` AFTER teardown on the same session.
        let bc_post = make_bc_unary_add2();
        ctx.obarray.set_symbol_function("ljw-r5b-post", bc_post);
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((i 0) (a 0)) (while (< i {warm_calls}) (setq a (ljw-r5b-post i)) (setq i (1+ i))) a)"
                )
            ),
            format!("{}", warm_calls + 1)
        );

        let snap_final = jit::stats::compile_stats_snapshot();
        let origins_final: u64 = snap_final.origins.iter().map(|o| o.count).sum();
        assert_eq!(origins_final, origins_after_td);
        assert_eq!(snap_final.total_compiles, snap_after_td.total_compiles);
        assert_eq!(snap_final.native_entries, snap_after_td.native_entries);
        assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
        assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
        assert!(!jit::compile::shared::shared_jit_is_installed_for_test());
    });
}
