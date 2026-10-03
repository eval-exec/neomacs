//! The tier spine's trigger (P2.1 C6 + W1): the countdown, its request,
//! the upgrade at the next seam, and the knob-off identity.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::lowering::{RegallocChoice, forced_regalloc};
use crate::emacs_core::jit::compile::{
    Tier2PolicyKnob, force_profit_gate_for_test, force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::value::{LambdaParams, Value};

fn knob(window: u32, loop_credit: u32) -> Tier2Knob {
    // C6 tests use one additional stable work unit and unlimited compile
    // budget; the C7 family tests the full policy independently.
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }));
    Tier2Knob {
        on: true,
        window,
        loop_credit,
    }
}

/// Count entries and poll ticks (the report knobs' emission) on this
/// thread, so the tests can read where the work ran.
fn count_work() {
    force_observe_for_test(ObserveOverride {
        stats: false,
        naming: false,
        entry_count: true,
    });
}

fn lexical_fn(nargs: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

/// `(lambda () 7)`: straight-line, so the fast allocator (the T1' set).
fn seven() -> ByteCodeFunction {
    lexical_fn(
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(7)],
    )
}

/// `(lambda (n) (while (> (setq n (1- n)) 0)) n)`: a loop of `n`
/// iterations, each one taken back edge.
fn countdown_loop() -> ByteCodeFunction {
    lexical_fn(
        1,
        vec![
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNotNil(0),
            Op::Return,
        ],
        vec![Value::make_int(0)],
    )
}

fn run(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Option<Value> {
    let fv = Value::NIL;
    cache::try_run_compiled(ctx, f, fv, args)
        .expect("no signal")
        .map(|bits| Value::from_bits(bits))
}

/// The current leaf of `f` on this thread.
fn current(f: &ByteCodeFunction) -> &'static CompiledLeaf {
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let ptr = cache::compiled_leaf_ptr_for_test(id).expect("a cached leaf");
    // SAFETY: cached leaves stay allocated (retired ones too) until `clear`,
    // which these tests never reach while holding the reference.
    unsafe { &*ptr }
}

#[test]
fn tier2_knob_parses_the_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    };
    let off = Tier2Knob::from_env(env(&[]));
    assert!(!off.on);
    assert_eq!(off.window, Tier2Knob::DEFAULT_WINDOW);
    assert_eq!(off.loop_credit, Tier2Knob::DEFAULT_LOOP_CREDIT);
    let on = Tier2Knob::from_env(env(&[
        ("NEOVM_JIT_TIER2", "on"),
        ("NEOVM_JIT_T2_WINDOW", "100"),
        ("NEOVM_JIT_T2_LOOP_CREDIT", "0"),
    ]));
    assert_eq!(on, knob(100, 0));
    let zero_window = Tier2Knob::from_env(env(&[("NEOVM_JIT_T2_WINDOW", "0")]));
    assert_eq!(zero_window.window, 1, "one entry is the smallest window");
    assert!(!Tier2Knob::from_env(env(&[("NEOVM_JIT_TIER2", "off")])).on);
}

/// The heat crossing is off under the knob: the countdown owns the
/// re-tier. Off, it is today's.
#[test]
fn tier2_turns_the_heat_retier_off() {
    force_tier2_for_test(Some(knob(10, 64)));
    assert_eq!(crate::emacs_core::jit::retier_heat(), None);
    force_tier2_for_test(Some(Tier2Knob {
        on: false,
        ..knob(10, 64)
    }));
    let factor = crate::emacs_core::jit::retier_factor();
    assert_eq!(
        crate::emacs_core::jit::retier_heat(),
        (factor != 0).then(|| crate::emacs_core::jit::hot_threshold() * factor)
    );
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// With the knob off a tier-up leaf carries no countdown and never
/// requests, however often it runs.
#[test]
fn tier2_off_leaves_carry_no_countdown() {
    force_tier2_for_test(Some(Tier2Knob {
        on: false,
        ..knob(2, 64)
    }));
    let mut ev = Context::new();
    let f = seven();
    for _ in 0..10 {
        assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
    }
    let leaf = current(&f);
    assert_eq!(leaf.obs.t2.origin, T2Origin::Unprofiled);
    assert_eq!(leaf.obs.t2.budget.get(), DISARMED);
    assert_eq!(leaf.obs.t2.state.get(), T2State::Idle);
    assert_eq!(stats().requests, 0);
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// C6: a fast-allocator leaf entered through the tier-up seam requests
/// after exactly `window` entries, and the next entry rebuilds it with the
/// full allocator -- the old re-tier, at the countdown instead of the heat
/// crossing. The new leaf carries no profiling code; the old one is
/// retired.
#[test]
fn tier2_fast_leaf_retiers_at_the_countdown() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    if forced_regalloc().is_some() || crate::emacs_core::jit::retier_factor() == 0 {
        return; // the allocator knobs veto the re-tier
    }
    const WINDOW: u32 = 6;
    force_tier2_for_test(Some(knob(WINDOW, 64)));
    let mut ev = Context::new();
    let f = seven();
    assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
    let t1 = current(&f);
    assert_eq!(t1.regalloc, RegallocChoice::Fast);
    assert_eq!(t1.obs.t2.origin, T2Origin::Profiling);
    for _ in 1..WINDOW {
        assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
        assert!(std::ptr::eq(current(&f), t1), "still the T1 leaf");
    }
    assert_eq!(
        t1.obs.t2.state.get(),
        T2State::Idle,
        "first request starts the stable window"
    );
    assert_eq!(t1.obs.t2.budget.get(), 1);
    assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
    assert_eq!(
        t1.obs.t2.state.get(),
        T2State::Due(T2Upgrade::Retier),
        "the stable entry requested the upgrade"
    );
    assert_eq!(t1.obs.t2.budget.get(), DISARMED);
    assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
    let t1p = current(&f);
    assert!(!std::ptr::eq(t1p, t1), "the seam compiled the upgrade");
    assert_eq!(t1p.regalloc, RegallocChoice::Full);
    assert_eq!(t1p.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Retier));
    assert_eq!(t1p.obs.t2.budget.get(), DISARMED, "no profiling code");
    assert_eq!(t1.obs.t2.state.get(), T2State::Upgraded(T2Upgrade::Retier));
    assert!(t1.retired.get());
    let s = stats();
    assert_eq!((s.requests, s.due, s.upgraded), (2, 1, 1));
    for _ in 0..50 {
        assert_eq!(run(&mut ev, &f, &[]), Some(Value::make_int(7)));
    }
    assert!(std::ptr::eq(current(&f), t1p), "upgraded once");
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// X7 (T2.1's form): a leaf reached only through its caller's spec slot
/// (no heat ever reaches it there) still upgrades: its request unlinks the
/// slot, the slot's next call re-resolves through the cache, which
/// compiles the upgrade, and the upgraded leaf serves the work after the
/// window.
#[test]
fn tier2_spec_reached_leaf_upgrades_and_serves_the_work() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    if forced_regalloc().is_some() || crate::emacs_core::jit::retier_factor() == 0 {
        return;
    }
    const WINDOW: u32 = 20;
    const CALLS: u32 = 400;
    force_tier2_for_test(Some(knob(WINDOW, 64)));
    force_profit_gate_for_test(false);
    count_work();
    let mut ev = Context::new();
    // Not lexical, so the MIR tier's pure inliner leaves the call a call.
    let mut callee = seven();
    callee.lexical = false;
    let callee = Value::make_bytecode(callee);
    let callee_sym = intern("neovm--t2-x7-callee");
    ev.obarray.set_symbol_function_id(callee_sym, callee);
    // (lambda () (neovm--t2-x7-callee)): a speculated call site.
    let caller = Value::make_bytecode(lexical_fn(
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![Value::from_sym_id(callee_sym)],
    ));
    crate::emacs_core::eval::push_scratch_gc_root(callee);
    crate::emacs_core::eval::push_scratch_gc_root(caller);
    let callee_data = callee.get_bytecode_data().expect("byte-code");
    let caller_data = caller.get_bytecode_data().expect("byte-code");
    let ctx = &mut ev as *mut Context;
    for _ in 0..CALLS {
        assert_eq!(
            cache::try_run_compiled(ctx, caller_data, caller, &[]).unwrap(),
            Some(Value::make_int(7).bits())
        );
    }
    let id = callee_data
        .jit_runtime()
        .compiled_id()
        .expect("callee compiled");
    let upgraded = current(callee_data);
    assert_eq!(upgraded.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Retier));
    assert_eq!(upgraded.regalloc, RegallocChoice::Full);
    // Every leaf of the callee: the retired T1 leaf and the upgrade.
    let (rows, _) = cache::leaf_report_rows();
    let callee_rows: Vec<_> = rows.iter().filter(|r| r.id == id).collect();
    let total: u64 = callee_rows.iter().map(|r| r.obs.entries).sum();
    let upgraded_entries = upgraded.obs.entries.get();
    assert_eq!(total, u64::from(CALLS), "every call counted once");
    let after_window = total - u64::from(WINDOW + 1);
    assert!(
        upgraded_entries * 10 >= after_window * 9,
        "the upgrade serves >= 90% of the entries after the window: {upgraded_entries} of {after_window}"
    );
    // The T1 leaf is unlinked from every caller's slot.
    let old = callee_rows
        .iter()
        .find(|r| r.obs.t2.state == "upgraded")
        .expect("the retired T1 leaf");
    assert_eq!(old.obs.t2.origin, "profiling");
    assert!(old.obs.t2.requested);
    let caller_leaf = current(caller_data);
    for slot in caller_leaf.spec_slots.iter() {
        assert!(
            slot.leaf_ptr().is_null() || std::ptr::eq(slot.leaf_ptr(), upgraded),
            "no slot still calls the T1 leaf"
        );
    }
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// X8 (W1): a loop leaf called once requests during that call, from its
/// poll's cold block, while the poll cadence -- every 255 taken back edges
/// -- is exactly the knob-off cadence.
#[test]
fn tier2_loop_credit_requests_inside_one_call_at_the_poll_cadence() {
    const N: i64 = 1_000_000;
    let polls_with = |k: Tier2Knob| {
        force_tier2_for_test(Some(k));
        let mut ev = Context::new();
        let f = countdown_loop();
        reset_bytecode_branch_poll_count();
        assert_eq!(
            run(&mut ev, &f, &[Value::make_int(N)]),
            Some(Value::make_int(0))
        );
        let polls = bytecode_branch_poll_count();
        let state = current(&f).obs.t2.state.get();
        cache::clear();
        (polls, state)
    };
    let (off_polls, off_state) = polls_with(Tier2Knob {
        on: false,
        ..knob(15_000, 64)
    });
    assert_eq!(off_state, T2State::Idle);
    assert_eq!(
        off_polls as i64,
        N / 255,
        "one poll per 255 taken back edges"
    );
    let (on_polls, on_state) = polls_with(knob(15_000, 64));
    assert_eq!(
        on_polls, off_polls,
        "the loop credit leaves the cadence alone"
    );
    assert_eq!(
        on_state,
        T2State::Due(T2Upgrade::Feedback),
        "cold polls finish both profile windows"
    );
    let (entry_only_polls, entry_only_state) = polls_with(knob(15_000, 0));
    assert_eq!(entry_only_polls, off_polls);
    assert_eq!(
        entry_only_state,
        T2State::Idle,
        "LOOP_CREDIT=0: one entry never reaches the window"
    );
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// Both requests fire within the first call: tick 235 starts the stable
/// window, and tick 298 requests the upgrade without changing the quit polls.
#[test]
fn tier2_loop_credit_requests_at_the_expected_tick() {
    count_work();
    force_tier2_for_test(Some(knob(15_000, 64)));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 4_000,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }));
    let mut ev = Context::new();
    let f = countdown_loop();
    assert_eq!(
        run(&mut ev, &f, &[Value::make_int(1_000_000)]),
        Some(Value::make_int(0))
    );
    let obs = &current(&f).obs;
    let (entries_at, polls_at) = obs.t2.at_request.get();
    assert_eq!(entries_at, 1);
    // First request at tick 235, then ceil(4000/64)=63 stable-window ticks.
    assert_eq!(polls_at, 298);
    assert_eq!(obs.t2.polls.get(), 1_000_000 / 255);
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// W1-HOF: a mapping builtin's sequence length credits the countdown of a
/// profiling caller (one call with a long list requests), and not with the
/// loop credit at 0.
#[test]
fn tier2_mapping_builtin_credits_its_sequence() {
    force_profit_gate_for_test(false);
    let requested_with = |k: Tier2Knob| {
        force_tier2_for_test(Some(k));
        let mut ev = Context::new();
        // (lambda (f l) (mapc f l))
        let caller = lexical_fn(
            2,
            vec![
                Op::Constant(0),
                Op::StackRef(2),
                Op::StackRef(2),
                Op::Call(2),
                Op::Return,
            ],
            vec![Value::from_sym_id(intern("mapc"))],
        );
        let identity =
            Value::make_bytecode(lexical_fn(1, vec![Op::StackRef(0), Op::Return], Vec::new()));
        // 64 x (window + 1) elements: one call is worth the window.
        let len = 64 * (k.window as usize + 1);
        let list = Value::list((0..len as i64).map(Value::make_int).collect());
        crate::emacs_core::eval::push_scratch_gc_root(identity);
        crate::emacs_core::eval::push_scratch_gc_root(list);
        assert_eq!(run(&mut ev, &caller, &[identity, list]), Some(list));
        assert_eq!(current(&caller).obs.t2.state.get(), T2State::Idle);
        assert_eq!(run(&mut ev, &caller, &[identity, list]), Some(list));
        let state = current(&caller).obs.t2.state.get();
        cache::clear();
        state
    };
    let before = stats().hof_credits;
    assert_eq!(
        requested_with(knob(10, 64)),
        T2State::Due(T2Upgrade::Feedback)
    );
    assert_eq!(stats().hof_credits, before + 1);
    assert_eq!(requested_with(knob(10, 0)), T2State::Idle);
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// A request from a leaf that is no longer its source's current one (here:
/// built outside the cache) changes nothing but its own countdown.
#[test]
fn tier2_request_of_a_leaf_outside_the_cache_is_stale() {
    force_tier2_for_test(Some(knob(3, 64)));
    let f = seven();
    let mut leaf = crate::emacs_core::jit::compile::compile_bytecode_function_with(&f, None)
        .expect("compiles");
    // A hand-made profiling state: a direct compile never profiles.
    assert_eq!(leaf.obs.t2.origin, T2Origin::Unprofiled);
    leaf.obs.t2 = T2Cells::with_origin(T2Origin::Profiling, 1, None);
    request(&leaf.obs);
    assert_eq!(leaf.obs.t2.state.get(), T2State::Stale);
    assert_eq!(leaf.obs.t2.budget.get(), DISARMED);
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
