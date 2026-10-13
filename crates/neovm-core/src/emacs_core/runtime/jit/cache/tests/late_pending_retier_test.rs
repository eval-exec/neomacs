//! Late publication must not arm a Fast source after its heat crossing.
//! Sync + held publication makes the pending interval independent of workers.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::ReoptLevel;
use crate::emacs_core::jit::bg::{self, BgMode, JobClass, TestModeScope};
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{
    OptMode, Tier2Knob, force_deopt_for_test, force_profit_gate_for_test, force_tier2_for_test,
    jit_regalloc_small_max, opt_mode_scope_for_test,
};
use crate::emacs_core::value::Value;

struct Hooks {
    _mode: TestModeScope,
    active: bool,
}

impl Hooks {
    fn new() -> Self {
        Self {
            _mode: TestModeScope::enter(BgMode::Sync),
            active: true,
        }
    }
    fn reset(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        clear();
        bg::hold_publish_for_test(false);
        bg::publish_held_for_test();
        bg::force_deferred_install_for_test(false);
        force_tier2_for_test(None);
        bg::force_stress_for_test(None);
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        self.reset();
    }
}

struct Fixture {
    ev: Context,
    proto: ByteCodeFunction,
    one: Value,
    nine: Value,
    at: u32,
    hooks: Hooks,
}

impl Fixture {
    fn new() -> Self {
        let hooks = Hooks::new();
        force_tier2_for_test(Some(Tier2Knob::from_env(|_| None)));
        force_deopt_for_test(false);
        force_profit_gate_for_test(false);
        bg::force_stress_for_test(Some(false));
        bg::force_deferred_install_for_test(true);
        bg::hold_publish_for_test(true);
        assert!(
            forced_regalloc().is_none(),
            "run with NEOVM_JIT_REGALLOC=auto"
        );
        assert_eq!(jit_regalloc_small_max(), 0);
        let at = crate::emacs_core::jit::retier_heat().expect("enable legacy heat re-tier");
        assert!(at > 3);
        let mut ev = Context::new();
        let proto = function(
            vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
            vec![Value::make_int(1)],
            1,
        );
        proto
            .jit_runtime()
            .set_reopt_level_for_test(ReoptLevel::BaselineOnly);
        assert_eq!(proto.jit_runtime().note_patched_prefix(1), None);
        let one = instance(&proto, 1);
        let nine = instance(&proto, 9);
        ev.set_variable("neovm--late-retier-one", one);
        ev.set_variable("neovm--late-retier-nine", nine);
        Self {
            ev,
            proto,
            one,
            nine,
            at,
            hooks,
        }
    }

    fn ctx(&mut self) -> *mut Context {
        &mut self.ev
    }
    fn id(&self) -> u64 {
        self.proto.jit_runtime().compiled_id().expect("assigned")
    }
    fn allocator(&self) -> RegallocChoice {
        compiled_regalloc_for_test(self.id()).expect("compiled leaf")
    }
    fn slot(&self) -> Option<*const CompiledLeaf> {
        self.proto.jit_runtime().armed_leaf_slot(leaf_slot_epoch())
    }

    fn request(&mut self, class: JobClass) {
        let before = bg::stats_snapshot().enqueued[class as usize];
        let ctx = self.ctx();
        match class {
            JobClass::FirstSight => assert!(resolve_compiled_leaf_ptr(ctx, &self.proto).is_none()),
            JobClass::Entry => assert_eq!(
                try_run_compiled(ctx, &self.proto, self.one, &[Value::make_int(8)]).unwrap(),
                None,
            ),
            JobClass::Osr | JobClass::Upgrade => panic!("fixture requests only entry sources"),
        }
        assert_eq!(cache_entry_kind_for_test(self.id()), "pending");
        assert_eq!(bg::stats_snapshot().enqueued[class as usize], before + 1);
        COMPILED.with(|cache| {
            let cache = cache.borrow();
            let Some(CacheEntry::Pending(job)) = cache.get(self.id()) else {
                panic!("held backend must leave its front pending")
            };
            assert_eq!(job.leaf().regalloc, RegallocChoice::Fast);
            assert!(
                !job.leaf().call_heavy,
                "call-heavy Fast is deliberately exempt"
            );
        });
        for value in [self.one, self.nine] {
            assert_eq!(
                value
                    .get_bytecode_data()
                    .unwrap()
                    .jit_runtime()
                    .compiled_id(),
                Some(self.id())
            );
        }
        assert!(self.slot().is_none());
    }

    fn publish_at(&mut self, heat: u32) {
        self.proto.jit_runtime().set_heat_for_test(heat);
        assert_eq!(bg::publish_held_for_test(), 1);
        bg::hold_publish_for_test(false);
        bg::force_deferred_install_for_test(false);
    }

    fn callback(&mut self, nine: bool) {
        let (callee, expected) = if nine { (self.nine, 17) } else { (self.one, 9) };
        assert_eq!(
            self.ev.apply1(callee, Value::make_int(8)).unwrap(),
            Value::make_int(expected)
        );
    }

    fn assert_no_overdue_fast_slot(&self) {
        if self.allocator() == RegallocChoice::Fast {
            assert!(
                self.slot().is_none(),
                "overdue Fast must leave the strict re-tier seam reachable"
            );
        }
    }

    fn assert_stable_full(&mut self) {
        assert_eq!(self.allocator(), RegallocChoice::Full);
        let full = compiled_leaf_ptr_for_test(self.id()).unwrap();
        let ctx = self.ctx();
        arm_leaf_slot(ctx, &self.proto);
        assert_eq!(self.slot(), Some(full));
        for i in 0..32 {
            self.callback(i % 2 != 0);
        }
        assert_eq!(self.allocator(), RegallocChoice::Full);
        assert_eq!(compiled_leaf_ptr_for_test(self.id()), Some(full));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Cancel before releasing any locally held completion, with the owner
        // and both closure roots still alive. No worker hold/quiesce is used.
        self.hooks.reset();
        // The fresh thread owns the bool-only deopt/profit overrides; thread
        // exit discards those TLS values without inventing a previous setting.
    }
}

fn instance(proto: &ByteCodeFunction, captured: i64) -> Value {
    let mut f = proto.clone();
    let mut constants: Vec<Value> = f.constants.iter().copied().collect();
    constants[0] = Value::make_int(captured);
    f.constants = constants.into();
    Value::make_bytecode(f)
}

fn isolated(test: fn(&mut Fixture)) {
    std::thread::spawn(move || {
        let _backend = opt_mode_scope_for_test(OptMode::Legacy);
        let mut fixture = Fixture::new();
        test(&mut fixture);
    })
    .join()
    .expect("late-install mutator");
}

#[test]
fn jit_bg_late_first_sight_retiers_before_callback_arming() {
    isolated(|f| {
        f.request(JobClass::FirstSight);
        f.publish_at(f.at + 3);
        let ctx = f.ctx();
        let resolved = resolve_compiled_leaf_ptr(ctx, &f.proto);
        if f.allocator() == RegallocChoice::Fast {
            assert!(
                resolved.is_none(),
                "late Fast cannot escape through source resolution"
            );
        }
        arm_leaf_slot(ctx, &f.proto);
        f.assert_no_overdue_fast_slot();
        f.callback(false);
        f.callback(true);
        f.assert_stable_full();
    });
}

#[test]
fn jit_bg_late_entry_install_leaves_retier_reachable() {
    isolated(|f| {
        f.request(JobClass::Entry);
        f.publish_at(f.at + 3);
        // Pending's first ready probe may run Fast once; it must not arm that
        // overdue result and hide the normal heat-driven replacement forever.
        f.callback(false);
        f.assert_no_overdue_fast_slot();
        f.callback(true);
        f.assert_stable_full();
    });
}

#[test]
fn jit_bg_timely_first_sight_keeps_fast_arming_and_retiers_at_crossing() {
    isolated(|f| {
        f.request(JobClass::FirstSight);
        f.publish_at(f.at - 3);
        let ctx = f.ctx();
        assert!(resolve_compiled_leaf_ptr(ctx, &f.proto).is_some());
        assert_eq!(f.allocator(), RegallocChoice::Fast);
        arm_leaf_slot(ctx, &f.proto);
        assert!(f.slot().is_some(), "timely Fast still earns direct entry");
        for i in 0..8 {
            f.callback(i % 2 != 0);
        }
        f.assert_stable_full();
    });
}
