use super::classes::ClassSelection;
use super::*;
fn reset() {
    BUFFER.with(|b| *b.borrow_mut() = Buffer::default());
}
fn entry_class() -> Option<DiagnosticClass> {
    Some(DiagnosticClass::Job(JobClass::Entry))
}
#[test]
fn jit_front_diag_full_return_includes_push_tail_and_exact_one_ms_boundary() {
    reset();
    let start = Instant::now();
    for (id, ns) in [999_999, 1_000_000, 1_000_001].into_iter().enumerate() {
        let a = owned(id as u64, CompileOrigin::Dispatch, entry_class());
        handoff(
            Route::WorkerPushOk,
            id as u64,
            start + Duration::from_nanos(10),
        );
        a.finish(start, Duration::from_nanos(ns), true);
    }
    BUFFER.with(|b| {
        let b = b.borrow();
        assert_eq!(b.finished, 3);
        for (sample, ns) in b.samples.iter().zip([999_999, 1_000_000, 1_000_001]) {
            assert_eq!(sample.elapsed.as_nanos(), ns);
            assert_eq!(sample.pre_push.expect("prepush").as_nanos(), 10);
        }
        let mut out = Vec::new();
        render(&b, &mut out).expect("owned sink");
        let out = String::from_utf8(out).expect("utf8");
        assert!(out.contains("full_return_ns=1000001 pre_push_ns=10"));
        assert!(out.contains("started=3 finished=3 aborted=0 lost=0 records=3"));
    });
}
#[test]
fn jit_front_diag_nested_nonentry_masks_marker_and_unwind_restores_outer() {
    reset();
    let start = Instant::now();
    let outer = owned(1, CompileOrigin::Dispatch, entry_class());
    handoff(Route::WorkerPushOk, 42, start);
    let panic = std::panic::catch_unwind(|| {
        let _inner = owned(2, CompileOrigin::Retier, None);
        handoff(Route::WorkerFallback, 43, start);
        panic!("owned nested scope");
    });
    assert!(panic.is_err());
    assert_eq!(ACTIVE.with(Cell::get).expect("outer").seq, Some(42));
    outer.finish(start, Duration::from_millis(1), true);
    let _aborted = owned(3, CompileOrigin::Dispatch, entry_class());
    drop(_aborted);
    assert!(ACTIVE.with(Cell::get).is_none());
    BUFFER.with(|b| {
        let b = b.borrow();
        assert_eq!(b.started, 2);
        assert_eq!(b.finished, 1);
        assert_eq!(b.aborted, 1);
        assert_eq!(b.samples[0].seq, Some(42));
    });
}
#[test]
fn jit_front_diag_owned_thread_buffers_and_sink_failure_are_independent() {
    reset();
    let threads: Vec<_> = (1..=2)
        .map(|id| {
            std::thread::spawn(move || {
                let a = owned(id, CompileOrigin::Dispatch, entry_class());
                handoff(Route::Inline, id, Instant::now());
                a.finish(Instant::now(), Duration::from_nanos(id), true);
                BUFFER.with(|b| {
                    let b = b.borrow();
                    (b.thread, b.samples[0].id, b.finished)
                })
            })
        })
        .collect();
    let rows: Vec<_> = threads
        .into_iter()
        .map(|t| t.join().expect("owned thread"))
        .collect();
    assert_ne!(rows[0].0, rows[1].0);
    assert_eq!(rows[0].1, 1);
    assert_eq!(rows[1].1, 2);
    BUFFER.with(|b| assert_eq!(b.borrow().finished, 0));
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("owned write fault"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(render(&Buffer::default(), &mut Fail).is_err());
}

#[test]
fn jit_front_diag_supplied_anchor_retains_projection_bracket_and_signed_offsets() {
    let instant = Instant::now();
    let b = Buffer {
        thread: 9,
        started: 1,
        finished: 1,
        anchor: Some(Anchor {
            instant,
            before_ns: 1000,
            after_ns: 1010,
        }),
        samples: vec![Sample {
            id: 7,
            attempt: 1,
            origin: CompileOrigin::Dispatch,
            class: DiagnosticClass::Job(JobClass::Entry),
            route: Route::WorkerPushOk,
            seq: Some(3),
            start: instant - Duration::from_nanos(20),
            elapsed: Duration::from_nanos(50),
            pre_push: None,
            ok: true,
        }],
        ..Buffer::default()
    };
    let mut out = Vec::new();
    render(&b, &mut out).expect("owned projection");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("start_ns=-20 return_ns=30 full_return_ns=50"));
    assert!(text.contains("anchor_before_ns=1000 anchor_after_ns=1010"));
    // Entire interval projects into [980,1040], not a point at either bound.
    let lower_start = b.anchor.as_ref().unwrap().before_ns - 20;
    let upper_end = b.anchor.as_ref().unwrap().after_ns + 30;
    assert!(lower_start >= 970 && upper_end <= 1050);
    assert!(!(lower_start >= 985 && upper_end <= 1050));
}

#[test]
fn jit_front_diag_default_entry_excludes_unadmitted_and_osr_attempts() {
    reset();
    let start = Instant::now();
    for value in [None, Some("entry"), Some(""), Some("bogus"), Some(" all ")] {
        let selection = ClassSelection::parse(value);
        assert_eq!(selection, ClassSelection::EntryOnly);
        assert_eq!(selection.select(CompileOrigin::Dispatch, None), None);
        assert_eq!(
            selection.select(CompileOrigin::Osr, Some(JobClass::Osr)),
            None
        );
    }
    let selection = ClassSelection::parse(None);
    let outer = owned(
        1,
        CompileOrigin::Dispatch,
        selection.select(CompileOrigin::Dispatch, Some(JobClass::Entry)),
    );
    handoff(Route::WorkerPushOk, 42, start);
    let osr = owned(
        2,
        CompileOrigin::Osr,
        selection.select(CompileOrigin::Osr, None),
    );
    handoff(Route::WorkerPushOk, 43, start);
    osr.finish(start, Duration::from_nanos(20), true);
    outer.finish(start, Duration::from_nanos(30), true);
    drop(owned(
        3,
        CompileOrigin::Dispatch,
        selection.select(CompileOrigin::Dispatch, None),
    ));
    assert!(ACTIVE.with(Cell::get).is_none());
    BUFFER.with(|buffer| {
        let b = buffer.borrow();
        assert_eq!(
            (b.started, b.finished, b.aborted, b.samples.len()),
            (1, 1, 0, 1)
        );
        assert_eq!(b.samples[0].class, DiagnosticClass::Job(JobClass::Entry));
        assert_eq!(
            b.samples[0].seq,
            Some(42),
            "unselected OSR cannot stamp its enclosing entry"
        );
    });
}

#[test]
fn jit_front_diag_all_classes_record_legacy_population_and_schema() {
    reset();
    let start = Instant::now();
    let selection = ClassSelection::parse(Some("all"));
    let cases = [
        (
            CompileOrigin::Dispatch,
            None,
            DiagnosticClass::Job(JobClass::Entry),
        ),
        (
            CompileOrigin::DeferralExpired,
            None,
            DiagnosticClass::Job(JobClass::Entry),
        ),
        (
            CompileOrigin::FirstSight,
            None,
            DiagnosticClass::Job(JobClass::FirstSight),
        ),
        (
            CompileOrigin::Osr,
            None,
            DiagnosticClass::Job(JobClass::Osr),
        ),
        (CompileOrigin::Retier, None, DiagnosticClass::Synchronous),
        (CompileOrigin::AotDrain, None, DiagnosticClass::Synchronous),
        (CompileOrigin::Direct, None, DiagnosticClass::Synchronous),
        (
            CompileOrigin::Retier,
            Some(JobClass::Upgrade),
            DiagnosticClass::Job(JobClass::Upgrade),
        ),
    ];
    for (id, (origin, admitted, expected)) in cases.iter().copied().enumerate() {
        let class = selection.select(origin, admitted);
        assert_eq!(class, Some(expected));
        let attempt = owned(id as u64, origin, class);
        attempt.finish(start, Duration::from_nanos(id as u64 + 1), id % 2 == 0);
    }
    assert!(ACTIVE.with(Cell::get).is_none());
    BUFFER.with(|buffer| {
        let b = buffer.borrow();
        assert_eq!(
            (b.started, b.finished, b.aborted, b.samples.len()),
            (8, 8, 0, 8)
        );
        let mut out = Vec::new();
        render(&b, &mut out).expect("owned all-class sink");
        let out = String::from_utf8(out).expect("UTF8");
        let rows: Vec<_> = out
            .lines()
            .filter(|line| line.starts_with("front_v2 "))
            .collect();
        assert_eq!(rows.len(), cases.len());
        for (index, ((origin, _, class), row)) in cases.iter().zip(rows).enumerate() {
            let fields: std::collections::BTreeMap<_, _> = row
                .split_whitespace()
                .skip(1)
                .map(|field| field.split_once('=').expect("scalar field"))
                .collect();
            assert_eq!(
                fields
                    .keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>(),
                [
                    "pid",
                    "thread",
                    "attempt",
                    "id",
                    "origin",
                    "class",
                    "route",
                    "seq",
                    "ok",
                    "start_ns",
                    "return_ns",
                    "full_return_ns",
                    "pre_push_ns"
                ]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            );
            let origin_name: &'static str = (*origin).into();
            let class_name: &'static str = match class {
                DiagnosticClass::Job(class) => (*class).into(),
                DiagnosticClass::Synchronous => "synchronous",
            };
            assert_eq!(fields["origin"], origin_name);
            assert_eq!(fields["class"], class_name);
            assert_eq!(fields["route"], "synchronous");
            assert_eq!(fields["seq"], "-");
            assert_eq!(fields["pre_push_ns"], "-");
            assert_eq!(fields["ok"], if index % 2 == 0 { "1" } else { "0" });
            assert_eq!(fields["attempt"].parse::<usize>().unwrap(), index + 1);
        }
        assert!(out.contains("started=8 finished=8 aborted=0 lost=0 records=8"));
    });
}

#[test]
fn jit_front_diag_all_class_nested_osr_unwind_preserves_outer_entry_handoff() {
    reset();
    let start = Instant::now();
    let selection = ClassSelection::All;
    let outer = owned(
        1,
        CompileOrigin::Dispatch,
        selection.select(CompileOrigin::Dispatch, None),
    );
    handoff(Route::WorkerPushOk, 42, start);
    let panic = std::panic::catch_unwind(|| {
        let _osr = owned(
            2,
            CompileOrigin::Osr,
            selection.select(CompileOrigin::Osr, None),
        );
        handoff(Route::WorkerFallback, 43, start);
        panic!("owned OSR unwind");
    });
    assert!(panic.is_err());
    assert_eq!(
        ACTIVE.with(Cell::get).expect("outer survives unwind").seq,
        Some(42)
    );
    let osr = owned(
        3,
        CompileOrigin::Osr,
        selection.select(CompileOrigin::Osr, None),
    );
    handoff(Route::WorkerPushOk, 44, start);
    osr.finish(start, Duration::from_nanos(20), true);
    outer.finish(start, Duration::from_nanos(30), true);
    assert!(ACTIVE.with(Cell::get).is_none());
    BUFFER.with(|buffer| {
        let b = buffer.borrow();
        assert_eq!(
            (b.started, b.finished, b.aborted, b.samples.len()),
            (3, 2, 1, 2)
        );
        assert_eq!(
            (b.samples[0].id, b.samples[0].class, b.samples[0].seq),
            (3, DiagnosticClass::Job(JobClass::Osr), Some(44))
        );
        assert_eq!(
            (b.samples[1].id, b.samples[1].class, b.samples[1].seq),
            (1, DiagnosticClass::Job(JobClass::Entry), Some(42))
        );
    });
}
