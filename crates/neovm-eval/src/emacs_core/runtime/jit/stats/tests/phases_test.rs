//! The compile-origin rows and the exclusive phase split (P0.6 L0).

use super::phases::{CompileClock, CompileOrigin, CompilePhase, enter_phase};
use super::*;
use std::time::Duration;

fn observe(stats: bool) {
    force_observe_for_test(ObserveOverride {
        stats,
        naming: false,
        entry_count: false,
    });
}

fn nap() {
    std::thread::sleep(Duration::from_millis(2));
}

/// With the split on, nested phases are charged exclusively and the phases
/// of one compile sum to exactly the stall the clock returns.
#[test]
fn jit_phases_nested_guards_sum_to_the_stall() {
    observe(true);
    reset_compile_stats();
    let clock = CompileClock::start(CompileOrigin::Dispatch);
    nap(); // Other
    {
        let _gate = enter_phase(CompilePhase::Gate);
        nap();
    }
    {
        let _lower = enter_phase(CompilePhase::Lower);
        nap();
        {
            let _codegen = enter_phase(CompilePhase::Codegen);
            nap();
        }
        nap(); // back in Lower
    }
    let elapsed = clock.finish(true);
    let s = compile_stats_snapshot();
    let ns = |p: CompilePhase| s.phase_ns[p as usize];
    assert_eq!(
        s.phase_ns.iter().sum::<u64>(),
        elapsed.as_nanos() as u64,
        "exclusive attribution: the split is the whole stall"
    );
    assert!(ns(CompilePhase::Gate) >= 2_000_000, "{s:?}");
    assert!(ns(CompilePhase::Codegen) >= 2_000_000, "{s:?}");
    assert!(
        ns(CompilePhase::Lower) >= 4_000_000,
        "Lower resumes after the nested Codegen: {s:?}"
    );
    assert!(ns(CompilePhase::Other) >= 2_000_000, "{s:?}");
    assert_eq!(ns(CompilePhase::Setup), 0);
    let row = s.origins[CompileOrigin::Dispatch as usize];
    assert_eq!((row.count, row.ok), (1, 1));
    assert_eq!(row.us, elapsed.as_micros() as u64);
}

/// With the split off a guard reads no clock and records nothing; the
/// origin row is still kept (it is always on, like the stall aggregate).
#[test]
fn jit_phases_off_records_only_the_origin_row() {
    observe(false);
    reset_compile_stats();
    let clock = CompileClock::start(CompileOrigin::Osr);
    {
        let _lower = enter_phase(CompilePhase::Lower);
        nap();
    }
    clock.finish(false);
    let s = compile_stats_snapshot();
    assert!(s.phase_ns.iter().all(|&ns| ns == 0), "{s:?}");
    let row = s.origins[CompileOrigin::Osr as usize];
    assert_eq!((row.count, row.ok), (1, 0));
    assert_eq!(s.total_compiles, 0, "an origin row is not a stall record");
}

/// A guard outside any compile clock is inert, and a clock dropped without
/// `finish` (an unwinding compile) leaves no split behind for the next one.
#[test]
fn jit_phases_guards_outside_a_compile_and_abandoned_clocks_are_inert() {
    observe(true);
    reset_compile_stats();
    {
        let _stray = enter_phase(CompilePhase::Codegen);
        nap();
    }
    {
        let _abandoned = CompileClock::start(CompileOrigin::Dispatch);
        let _g = enter_phase(CompilePhase::Codegen);
        nap();
    }
    let clock = CompileClock::start(CompileOrigin::FirstSight);
    let elapsed = clock.finish(true);
    let s = compile_stats_snapshot();
    assert_eq!(
        s.phase_ns.iter().sum::<u64>(),
        elapsed.as_nanos() as u64,
        "only the finished compile is folded in: {s:?}"
    );
    assert_eq!(s.origins[CompileOrigin::Dispatch as usize].count, 0);
    assert_eq!(s.origins[CompileOrigin::FirstSight as usize].count, 1);
}

/// The rendering names every phase in order and every non-empty origin.
#[test]
fn jit_phases_render() {
    let mut s = CompileStats::default();
    s.origins[CompileOrigin::Dispatch as usize] = phases::OriginStats {
        count: 3,
        ok: 2,
        us: 150,
    };
    s.origins[CompileOrigin::Osr as usize] = phases::OriginStats {
        count: 1,
        ok: 1,
        us: 40,
    };
    s.phase_ns[CompilePhase::Codegen as usize] = 120_000;
    s.phase_ns[CompilePhase::Lower as usize] = 70_500;
    assert_eq!(
        format_phases(&s),
        "origin[dispatch=3/2/150,osr=1/1/40] \
         phase_us[gate=0,mir_build=0,fuse=0,lower=70,setup=0,codegen=120,finalize=0,other=0] \
         phase_total_us=190"
    );
    let base = s;
    s.origins[CompileOrigin::Dispatch as usize].count += 2;
    s.phase_ns[CompilePhase::Codegen as usize] += 5_000;
    let delta = s.since(&base);
    assert_eq!(delta.origins[CompileOrigin::Dispatch as usize].count, 2);
    assert_eq!(delta.origins[CompileOrigin::Osr as usize].count, 0);
    assert_eq!(delta.phase_ns[CompilePhase::Codegen as usize], 5_000);
}
