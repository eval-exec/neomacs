use super::*;
fn reset() {
    BUFFER.with(|b| *b.borrow_mut() = Buffer::default());
}
#[test]
fn jit_front_diag_full_return_includes_push_tail_and_exact_one_ms_boundary() {
    reset();
    let start = Instant::now();
    for (id, ns) in [999_999, 1_000_000, 1_000_001].into_iter().enumerate() {
        let a = owned(id as u64, CompileOrigin::Dispatch, true);
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
    let outer = owned(1, CompileOrigin::Dispatch, true);
    handoff(Route::WorkerPushOk, 42, start);
    let panic = std::panic::catch_unwind(|| {
        let _inner = owned(2, CompileOrigin::Retier, false);
        handoff(Route::WorkerFallback, 43, start);
        panic!("owned nested scope");
    });
    assert!(panic.is_err());
    assert_eq!(ACTIVE.with(Cell::get).expect("outer").seq, Some(42));
    outer.finish(start, Duration::from_millis(1), true);
    let _aborted = owned(3, CompileOrigin::Dispatch, true);
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
                let a = owned(id, CompileOrigin::Dispatch, true);
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
