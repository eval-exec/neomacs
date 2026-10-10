use super::*;

fn event(stage: Stage) -> Record {
    Record {
        stage,
        frame: 7,
        presentation: 42,
        clock_id: 1,
        ns: 123,
        realtime_ns: (stage == Stage::Publish).then_some(456),
    }
}

fn queue(capacity: usize) -> (Recorder, Receiver<Record>) {
    let (events, incoming) = bounded(capacity);
    let (stop, _) = bounded(1);
    (
        Recorder {
            events,
            dropped: Arc::new(AtomicU64::new(0)),
            stop,
            writer: Mutex::new(None),
        },
        incoming,
    )
}

#[test]
fn env_parsing_accepts_only_absolute_nonempty_paths() {
    assert_eq!(parse_path(None), Ok(None));
    assert_eq!(parse_path(Some(OsString::new())), Ok(None));
    assert!(parse_path(Some("trace.jsonl".into())).is_err());
    assert!(parse_path(Some(" ./trace.jsonl".into())).is_err());
    assert_eq!(
        parse_path(Some("/run/trace.jsonl".into())),
        Ok(Some(PathBuf::from("/run/trace.jsonl")))
    );
}

#[test]
fn disabled_path_does_not_sample_or_send() {
    record_to(None, Stage::Publish, 7, PresentationId::new(42), |_| {
        panic!("disabled tracing must not sample a clock")
    });
}

#[test]
fn formatting_preserves_identity_domain_and_paired_publish_clocks() {
    let mut bytes = Vec::new();
    write_record(&mut bytes, &event(Stage::Publish)).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "{\"stage\":\"publish\",\"frame\":7,\"presentation\":42,\"clock_id\":1,\"ns\":123,\"realtime_ns\":456}\n"
    );
}

#[test]
fn stage_labels_and_optional_realtime_are_unambiguous() {
    for (stage, label) in [
        (Stage::Prepared, "prepared"),
        (Stage::RenderStart, "render_start"),
        (Stage::Submit, "submit"),
        (Stage::Present, "present"),
        (Stage::Presented, "presented"),
        (Stage::Superseded, "superseded"),
        (Stage::Discarded, "discarded"),
    ] {
        let mut bytes = Vec::new();
        write_record(&mut bytes, &event(stage)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["stage"], label);
        assert!(value.get("realtime_ns").is_none());
    }
}

#[test]
fn full_channel_drops_without_replacing_queued_evidence() {
    let (recorder, incoming) = queue(1);
    recorder.send(event(Stage::Publish));
    recorder.send(event(Stage::Prepared));
    recorder.send(event(Stage::Present));
    assert_eq!(recorder.dropped.load(Ordering::Relaxed), 2);
    assert_eq!(incoming.try_recv().unwrap().stage, Stage::Publish);
    assert!(incoming.try_recv().is_err());
    let mut bytes = Vec::new();
    flush_records(&mut bytes, &recorder.dropped).unwrap();
    assert_eq!(bytes, b"{\"stage\":\"dropped\",\"count\":2}\n");
    assert_eq!(recorder.dropped.load(Ordering::Relaxed), 0);
    flush_records(&mut bytes, &recorder.dropped).unwrap();
    assert_eq!(bytes, b"{\"stage\":\"dropped\",\"count\":2}\n");
}

#[test]
fn disconnected_writer_counts_unsent_records() {
    let (recorder, incoming) = queue(1);
    drop(incoming);
    recorder.send(event(Stage::Present));
    assert_eq!(recorder.dropped.load(Ordering::Relaxed), 1);
}

#[test]
fn failed_clock_sample_is_counted_not_recorded_as_zero() {
    let (recorder, incoming) = queue(1);
    record_to(
        Some(&recorder),
        Stage::Publish,
        7,
        PresentationId::new(42),
        |_| None,
    );
    assert_eq!(recorder.dropped.load(Ordering::Relaxed), 1);
    assert!(incoming.try_recv().is_err());
}

#[test]
fn record_samples_only_after_enablement() {
    let (recorder, incoming) = queue(1);
    record_to(
        Some(&recorder),
        Stage::Publish,
        7,
        PresentationId::new(42),
        |stage| {
            assert_eq!(stage, Stage::Publish);
            Some((1, 123, Some(456)))
        },
    );
    let record = incoming.try_recv().unwrap();
    assert_eq!(record.presentation, 42);
    assert_eq!(record.realtime_ns, Some(456));
}

#[test]
fn writer_drains_and_flushes_at_normal_shutdown() {
    let (events, incoming) = bounded(2);
    let (stop, stopped) = bounded(1);
    events.try_send(event(Stage::Publish)).unwrap();
    events.try_send(event(Stage::Prepared)).unwrap();
    stop.try_send(()).unwrap();
    let dropped = AtomicU64::new(3);
    let mut bytes = Vec::new();
    write_records(BufWriter::new(&mut bytes), incoming, stopped, &dropped).unwrap();
    let lines: Vec<_> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 3);
    let stages: Vec<_> = lines
        .iter()
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap()["stage"].clone())
        .collect();
    assert_eq!(stages, ["publish", "prepared", "dropped"]);
}

#[test]
fn deferred_stamp_keeps_its_sample_time() {
    let (recorder, incoming) = queue(2);
    let record = stamp_with(
        &recorder,
        Stage::Publish,
        7,
        PresentationId::new(42),
        |_| Some((1, 123, Some(456))),
    )
    .unwrap();
    assert!(incoming.try_recv().is_err(), "stamping must not queue");
    recorder.send(record);
    let queued = incoming.try_recv().unwrap();
    assert_eq!((queued.ns, queued.realtime_ns), (123, Some(456)));
}

#[test]
fn writer_stops_promptly_with_an_empty_queue() {
    let (_events, incoming) = bounded::<Record>(1);
    let (stop, stopped) = bounded(1);
    stop.try_send(()).unwrap();
    let mut bytes = Vec::new();
    write_records(&mut bytes, incoming, stopped, &AtomicU64::new(0)).unwrap();
    assert!(bytes.is_empty());
}

#[test]
fn writer_flushes_when_producers_disconnect() {
    let (events, incoming) = bounded(1);
    let (_stop, stopped) = bounded(1);
    events.try_send(event(Stage::Presented)).unwrap();
    drop(events);
    let mut bytes = Vec::new();
    write_records(
        BufWriter::new(&mut bytes),
        incoming,
        stopped,
        &AtomicU64::new(0),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["stage"],
        "presented"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn environment_trace_writes_and_drains_jsonl() {
    const CHILD: &str = "NEOMACS_PRESENT_TRACE_TEST_CHILD";
    if let Ok(expected) = std::env::var(CHILD) {
        assert_eq!(enabled(), expected == "enabled");
        for stage in [
            Stage::Publish,
            Stage::Prepared,
            Stage::RenderStart,
            Stage::Submit,
            Stage::Present,
            Stage::Superseded,
            Stage::Discarded,
        ] {
            if stage == Stage::Superseded {
                // The deferred form queues exactly what it sampled.
                if let Some(stamp) = stamp(stage, 7, PresentationId::new(42)) {
                    stamp.emit();
                }
            } else {
                record(stage, 7, PresentationId::new(42));
            }
        }
        presented(7, PresentationId::new(42), 17, 999);
        shutdown();
        return;
    }

    // Separate processes isolate the cached environment without mutating the
    // environment of concurrent tests. No GUI or compositor is involved.
    let path = std::env::temp_dir().join(format!(
        "neomacs-present-trace-{}-{}.jsonl",
        std::process::id(),
        clock_ns(libc::CLOCK_MONOTONIC).unwrap()
    ));
    for configured in [
        None,
        Some(OsString::new()),
        Some("relative.jsonl".into()),
        Some(path.clone().into_os_string()),
    ] {
        let expected = if configured.as_ref() == Some(&path.clone().into_os_string()) {
            "enabled"
        } else {
            "disabled"
        };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .arg("--exact")
            .arg("present_trace::tests::environment_trace_writes_and_drains_jsonl")
            .env(CHILD, expected)
            .env_remove("NEOMACS_PRESENT_TRACE");
        if let Some(value) = configured {
            child.env("NEOMACS_PRESENT_TRACE", value);
        }
        let output = child.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        if expected == "disabled" {
            assert!(!path.exists());
        }
    }
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let records: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 8);
    for record in &records {
        assert_eq!(record["frame"], 7);
        assert_eq!(record["presentation"], 42);
    }
    assert_eq!(records[0]["stage"], "publish");
    assert!(records[0]["realtime_ns"].is_u64());
    assert_eq!(records[7]["stage"], "presented");
    assert_eq!(records[7]["clock_id"], 17);
    assert_eq!(records[7]["ns"], 999);
}

#[cfg(target_os = "linux")]
#[test]
fn local_stages_share_monotonic_clock_and_publish_pairs_realtime() {
    let (clock, ns, realtime) = sample(Stage::Publish).unwrap();
    let (next_clock, next_ns, next_realtime) = sample(Stage::Prepared).unwrap();
    assert_eq!(clock, libc::CLOCK_MONOTONIC as u32);
    assert_eq!(next_clock, clock);
    assert!(next_ns >= ns);
    assert!(realtime.unwrap() > ns);
    assert_eq!(next_realtime, None);
}
