use super::*;

#[test]
fn jit_stats_bucket_index_boundaries() {
    assert_eq!(bucket_index(0), 0);
    assert_eq!(bucket_index(99), 0);
    assert_eq!(bucket_index(100), 1);
    assert_eq!(bucket_index(249), 1);
    assert_eq!(bucket_index(250), 2);
    assert_eq!(bucket_index(500), 3);
    assert_eq!(bucket_index(999), 3);
    assert_eq!(bucket_index(1_000), 4);
    assert_eq!(bucket_index(2_500), 5);
    assert_eq!(bucket_index(5_000), 6);
    assert_eq!(bucket_index(9_999), 6);
    assert_eq!(bucket_index(10_000), 7);
    assert_eq!(bucket_index(u64::MAX), 7);
}

/// `NEOVM_JIT_STATS_FILE`: unset reports to stderr, a writable path appends
/// `[tag] body` lines to that file, and an unopenable path falls back to
/// stderr instead of failing the run.
#[test]
fn jit_stats_report_sink_choice() {
    assert!(matches!(ReportSink::choose(None), ReportSink::Stderr));

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jit-stats.txt");
    let sink = ReportSink::choose(Some(path.as_os_str()));
    assert!(matches!(sink, ReportSink::File(_)));
    sink.write_line(ReportTag::Compile, "compiles=1");
    sink.write_line(ReportTag::MirBails, "gate:x=2");
    drop(sink);
    // A second open appends rather than truncating.
    ReportSink::choose(Some(path.as_os_str())).write_line(ReportTag::Dispatch, "d=3");
    let text = std::fs::read_to_string(&path).expect("read sink file");
    assert_eq!(
        text,
        "[neovm-jit-compile] compiles=1\n[neovm-jit-mir-bails] gate:x=2\n[neovm-jit-dispatch] d=3\n"
    );

    let missing = dir.path().join("no-such-dir").join("x.txt");
    assert!(matches!(
        ReportSink::choose(Some(missing.as_os_str())),
        ReportSink::Stderr
    ));
}
