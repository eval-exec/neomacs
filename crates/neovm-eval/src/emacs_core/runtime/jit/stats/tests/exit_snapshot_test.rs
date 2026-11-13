use super::*;

fn render(snapshot: &Snapshot, stats: &CompileStats, mode: bg::BgMode, pending: usize) -> String {
    let mut out = Vec::new();
    write_final(&mut out, 73, snapshot, stats, mode, pending).expect("render exit record");
    String::from_utf8(out).expect("ASCII record")
}

#[test]
fn jit_exit_snapshot_unselected_attempt_is_absent() {
    // No environment mutation or process-global counter reset: declining the
    // constructor must return before even consulting the cached path.
    assert!(Attempt::begin(ConstructionSite::Unselected).is_none());
}

#[test]
fn jit_exit_snapshot_zero_routes_seal_once() {
    let counters = Counters::new();
    let snapshot = counters.seal().expect("first seal");
    assert!(snapshot.complete(bg::BgMode::Legacy, 0));
    assert!(counters.seal().is_none());
    let text = render(&snapshot, &CompileStats::default(), bg::BgMode::Legacy, 0);
    assert!(text.starts_with("[neovm-jit-exit] v=1 pid=73 sealed=1 complete=1 "));
    assert!(text.contains("normal_attempts=0 normal_refused=0 normal_constructed=0"));
    assert!(text.contains("osr_attempts=0 osr_refused=0 osr_constructed=0"));
    assert!(text.ends_with("origin[-] compile_total_us=0\n"));
}

#[test]
fn jit_exit_snapshot_normal_and_osr_outcomes_cover_refused_and_transient() {
    let counters = Counters::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    Attempt::begin_with(&counters, Route::Normal, &path).constructed(ConstructedState::Ready);
    Attempt::begin_with(&counters, Route::Normal, &path).constructed(ConstructedState::Deferred);
    drop(Attempt::begin_with(&counters, Route::Normal, &path));
    Attempt::begin_with(&counters, Route::Osr, &path).constructed(ConstructedState::Ready);
    drop(Attempt::begin_with(&counters, Route::Osr, &path));
    assert!(
        !path.exists(),
        "compiler counters must not perform per-attempt IO"
    );
    let snapshot = counters.seal().expect("seal");
    assert!(snapshot.complete(bg::BgMode::Sync, 0));
    let normal = snapshot.routes[0];
    assert_eq!(
        (
            normal.attempts,
            normal.refused,
            normal.ready,
            normal.deferred,
            normal.active
        ),
        (3, 1, 1, 1, 0)
    );
    let osr = snapshot.routes[1];
    assert_eq!(
        (osr.attempts, osr.refused, osr.constructed(), osr.active),
        (2, 1, Some(1), 0)
    );
}

#[test]
fn jit_exit_snapshot_unwinding_attempt_is_refused() {
    let counters = Counters::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    let result = std::panic::catch_unwind(|| {
        let _attempt = Attempt::begin_with(&counters, Route::Osr, &path);
        panic!("contained compiler failure");
    });
    assert!(result.is_err());
    let snapshot = counters.seal().expect("seal");
    assert!(snapshot.complete(bg::BgMode::Legacy, 0));
    assert_eq!(
        (
            snapshot.routes[1].attempts,
            snapshot.routes[1].refused,
            snapshot.routes[1].active
        ),
        (1, 1, 0)
    );
    assert!(!path.exists());
}

#[test]
fn jit_exit_snapshot_active_work_is_incomplete_and_late_finish_invalidates() {
    let counters = Counters::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    let attempt = Attempt::begin_with(&counters, Route::Normal, &path);
    let snapshot = counters.seal().expect("seal");
    assert!(!snapshot.complete(bg::BgMode::Legacy, 0));
    assert!(
        render(&snapshot, &CompileStats::default(), bg::BgMode::Legacy, 0).contains(" complete=0 ")
    );
    attempt.constructed(ConstructedState::Ready);
    let late = std::fs::read_to_string(&path).expect("late invalidation");
    assert!(late.contains("[neovm-jit-exit-invalidated] v=1 "));
    assert!(late.ends_with("sealed=1 late=1 kind=finish\n"));
}

#[test]
fn jit_exit_snapshot_begin_after_seal_invalidates_before_final_record() {
    let counters = Counters::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    let snapshot = counters.seal().expect("seal");
    assert!(snapshot.complete(bg::BgMode::Legacy, 0));
    let attempt = Attempt::begin_with(&counters, Route::Osr, &path);
    let late = std::fs::read_to_string(&path).expect("late begin");
    assert!(late.ends_with("sealed=1 late=1 kind=begin\n"));
    append(
        &path,
        &render(&snapshot, &CompileStats::default(), bg::BgMode::Legacy, 0),
    )
    .expect("final append");
    drop(attempt);
    let all = std::fs::read_to_string(&path).expect("final plus invalidations");
    assert_eq!(all.lines().count(), 3);
    assert!(all.lines().next().expect("first").contains("kind=begin"));
    assert!(
        all.lines()
            .last()
            .expect("last")
            .ends_with("late=2 kind=finish")
    );
}

#[test]
fn jit_exit_snapshot_delayed_begin_seal_check_cannot_escape_reservation() {
    let counters = Counters::new();
    // Deliberately pause between begin's reservation and its seal check. This
    // is the race a pre-reservation sealed load would miss.
    counters.reserve(Route::Normal);
    let snapshot = counters.seal().expect("seal");
    assert_eq!(
        (snapshot.routes[0].attempts, snapshot.routes[0].active),
        (1, 1)
    );
    assert!(!snapshot.complete(bg::BgMode::Legacy, 0));
    let late = counters
        .late_record(73, "begin")
        .expect("post-reservation check invalidates");
    assert!(late.ends_with("late=1 kind=begin\n"));
    counters.finish(Route::Normal, Outcome::Refused);
}

#[test]
fn jit_exit_snapshot_background_and_pending_work_never_complete() {
    let snapshot = Counters::new().seal().expect("seal");
    assert!(snapshot.complete(bg::BgMode::Legacy, 0));
    assert!(snapshot.complete(bg::BgMode::Sync, 0));
    assert!(!snapshot.complete(bg::BgMode::Threaded, 0));
    assert!(!snapshot.complete(bg::BgMode::Legacy, 1));
    assert!(
        render(&snapshot, &CompileStats::default(), bg::BgMode::Threaded, 0)
            .contains("complete=0 bg=threaded")
    );
}

#[test]
fn jit_exit_snapshot_existing_origin_time_includes_osr_without_phase_clocks() {
    let mut stats = CompileStats::default();
    stats.total_us = 999;
    stats.origins[super::super::phases::CompileOrigin::Dispatch as usize] =
        super::super::phases::OriginStats {
            count: 2,
            ok: 1,
            us: 50,
        };
    stats.origins[super::super::phases::CompileOrigin::Osr as usize] =
        super::super::phases::OriginStats {
            count: 1,
            ok: 1,
            us: 70,
        };
    let text = render(
        &Counters::new().seal().expect("seal"),
        &stats,
        bg::BgMode::Legacy,
        0,
    );
    assert!(text.contains("compile_time_scope=reporting_mutator"));
    assert!(text.ends_with("origin[dispatch=2/1/50,osr=1/1/70] compile_total_us=120\n"));
    assert!(stats.phase_ns.iter().all(|&ns| ns == 0));
    assert!(!text.contains("phase_us") && !text.contains("999"));
}

#[test]
fn jit_exit_snapshot_torn_or_overflowing_arithmetic_is_incomplete() {
    let mut snapshot = Counters::new().seal().expect("seal");
    snapshot.routes[0].attempts = 1;
    assert!(!snapshot.complete(bg::BgMode::Legacy, 0));
    snapshot.routes[0].ready = u64::MAX;
    snapshot.routes[0].deferred = 1;
    assert!(!snapshot.complete(bg::BgMode::Legacy, 0));
    let mut stats = CompileStats::default();
    stats.origins[0].us = u64::MAX;
    stats.origins[1].us = 1;
    let zero = Counters::new().seal().expect("seal");
    assert!(render(&zero, &stats, bg::BgMode::Legacy, 0).contains(" complete=0 "));
}

#[test]
fn jit_exit_snapshot_parallel_scalar_constructors_publish_complete_totals() {
    let counters = Counters::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    std::thread::scope(|scope| {
        for actor in 0..8 {
            let counters = &counters;
            let path = &path;
            scope.spawn(move || {
                for outcome in 0..30 {
                    let route = if actor % 2 != 0 {
                        Route::Osr
                    } else {
                        Route::Normal
                    };
                    let attempt = Attempt::begin_with(counters, route, path);
                    match outcome % 3 {
                        0 => attempt.constructed(ConstructedState::Ready),
                        1 => attempt.constructed(ConstructedState::Deferred),
                        _ => drop(attempt),
                    }
                }
            });
        }
    });
    let snapshot = counters.seal().expect("seal");
    assert!(snapshot.complete(bg::BgMode::Legacy, 0));
    for route in snapshot.routes {
        assert_eq!(
            (
                route.attempts,
                route.refused,
                route.ready,
                route.deferred,
                route.active
            ),
            (120, 40, 40, 40, 0)
        );
    }
    assert!(!path.exists());
}

#[test]
fn jit_exit_snapshot_io_failure_is_explicit_and_records_are_append_only() {
    /// Threading: invocation-owned failing writer; no shared or Lisp state.
    struct Reject;
    impl Write for Reject {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("rejected"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let snapshot = Counters::new().seal().expect("seal");
    assert!(
        write_final(
            &mut Reject,
            73,
            &snapshot,
            &CompileStats::default(),
            bg::BgMode::Legacy,
            0
        )
        .is_err()
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit.txt");
    append(&path, "prior\n").expect("first append");
    append(&path, "next\n").expect("second append");
    assert_eq!(
        std::fs::read_to_string(path).expect("read appended"),
        "prior\nnext\n"
    );
    assert!(append(&dir.path().join("missing/exit.txt"), "late\n").is_err());
}
