use crate::{MetricName, MetricUnit, parse_perf_stat_csv};

#[test]
fn perf_stat_csv_becomes_typed_canonical_measurements() {
    let measurements = parse_perf_stat_csv(
        "1234,,cycles:u,100000,100.00\n\
         2345,,instructions:u,100000,100.00\n\
         12,,page-faults,100000,100.00\n\
         7,,branch-misses:u,100000,100.00\n\
         9,,cache-misses:u,100000,100.00\n",
    )
    .expect("parse supported perf stat output");

    assert_eq!(measurements.len(), 5);
    assert_eq!(measurements[0].name, MetricName::CpuCycles);
    assert_eq!(measurements[0].value, 1234.0);
    assert_eq!(measurements[0].unit, MetricUnit::Count);
    assert_eq!(measurements[1].name, MetricName::Instructions);
    assert_eq!(measurements[2].name, MetricName::PageFaults);
    assert_eq!(measurements[3].name, MetricName::BranchMisses);
    assert_eq!(measurements[4].name, MetricName::CacheMisses);
}

#[test]
fn requested_but_unsupported_counters_reject_the_collection() {
    let error = parse_perf_stat_csv("<not supported>,,cycles:u,0,0.00\n")
        .expect_err("missing hardware counters are not valid zeroes");
    assert!(error.contains("cycles"));
    assert!(error.contains("not supported"));
}

#[test]
fn hybrid_cpu_rows_use_the_counted_pmu_and_ignore_its_disabled_sibling() {
    let measurements = parse_perf_stat_csv(
        "<not counted>,,cpu_atom/cycles/u,0,0.00\n\
         19269453,,cpu_core/cycles/u,4295962,100.00\n",
    )
    .expect("one hybrid PMU supplied the requested logical event");
    assert_eq!(measurements.len(), 1);
    assert_eq!(measurements[0].name, MetricName::CpuCycles);
    assert_eq!(measurements[0].value, 19_269_453.0);
}

/// The main-thread scopes add `--no-inherit` (the launched thread only) and
/// keep the edit-loop gate of the scope they narrow.
#[test]
fn main_thread_scopes_count_only_the_launched_thread() {
    use crate::CounterScope;
    use std::time::Duration;
    let dir = std::env::temp_dir();
    for (scope, gated, main_only) in [
        (CounterScope::EditLoop, true, false),
        (CounterScope::WholeProcess, false, false),
        (CounterScope::EditLoopMainThread, true, true),
        (CounterScope::WholeProcessMainThread, false, true),
    ] {
        assert_eq!(scope.gated_to_edit_loop(), gated, "{scope:?}");
        assert_eq!(scope.main_thread_only(), main_only, "{scope:?}");
        let capture = super::PerfStatCapture::new(&dir, scope, Duration::from_secs(1));
        let arguments = capture.stat_arguments();
        assert_eq!(
            arguments.iter().any(|a| a == "--no-inherit"),
            main_only,
            "{scope:?}: {arguments:?}"
        );
    }
}

/// The PTY adapter receives the thread selection for its editor capture.
#[test]
fn adapter_frontends_preserve_counter_thread_scope() {
    use crate::{CaptureRoute, CounterScope};
    use std::process::Command;
    use std::time::Duration;
    let dir = std::env::temp_dir();
    for (scope, expected) in [
        (CounterScope::WholeProcessMainThread, "main-thread"),
        (CounterScope::WholeProcess, "process"),
    ] {
        let mut capture = super::PerfStatCapture::new(&dir, scope, Duration::from_secs(1));
        let command = capture
            .wrap(Command::new("true"), CaptureRoute::Adapter("PTY"))
            .expect("the PTY adapter supports both thread scopes");
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[std::ffi::OsStr::new("PTY_PERF_THREAD_SCOPE")],
            Some(std::ffi::OsStr::new(expected))
        );
    }
}

/// Exercise the actual adapter: it must put `--no-inherit` on perf, rather
/// than just recording the requested scope in the harness metadata.
#[cfg(target_os = "linux")]
#[test]
fn pty_stat_capture_selects_the_launched_editor_thread() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-stat-thread-")
        .tempdir_in(crate::workspace_root().join("tmp"))
        .unwrap();
    let fake_perf = scratch.path().join("perf");
    fs::write(
        &fake_perf,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FAKE_PERF_ARGUMENTS\"\n: > \"$SENTINEL\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake_perf, fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = vec![scratch.path().to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    for (scope, main_only) in [("main-thread", true), ("process", false)] {
        let arguments_path = scratch.path().join("arguments");
        let output = Command::new("python3")
            .arg(crate::workspace_root().join("tools/bench/pty-run.py"))
            .arg("neomacs")
            .env("PATH", std::env::join_paths(&path).unwrap())
            .env("SENTINEL", scratch.path().join("sentinel"))
            .env("FAKE_PERF_ARGUMENTS", &arguments_path)
            .env("PTY_TIMEOUT", "5")
            .env("PTY_PERF_STAT", scratch.path().join("counters.csv"))
            .env("PTY_PERF_EVENTS", "cpu_core/instructions/u")
            .env("PTY_PERF_THREAD_SCOPE", scope)
            .env("PTY_PERF_CONTROL", "fifo:command,ack")
            .env_remove("PTY_PERF_RECORD")
            .env_remove("PTY_CPU")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let arguments = fs::read_to_string(arguments_path).unwrap();
        assert_eq!(
            arguments.lines().any(|arg| arg == "--no-inherit"),
            main_only
        );
        assert!(arguments.contains("--event\ncpu_core/instructions/u\n"));
        assert!(arguments.contains("--delay=-1\n--control=fifo:command,ack\n"));
    }
}

#[test]
fn paired_runtime_and_collector_controls_reach_the_editor() {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    let names = [
        "NEOMACS_RUNTIME_ROOT",
        "NEOVM_GC_GENERATIONAL",
        "NEOVM_GC_STRESS",
        "NEOVM_GC_TRACE",
    ];
    let variables = names.map(|name| (OsString::from(name), OsString::from("controlled")));
    let forwarded: BTreeMap<_, _> = crate::harness::passthrough_from(variables)
        .into_iter()
        .collect();
    for name in names {
        assert_eq!(forwarded[name], OsString::from("controlled"));
    }
}
