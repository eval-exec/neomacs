//! Each startup case runs in a fresh test process. The parent never mutates
//! its environment, and the workers only read the published process switch.
use super::*;
use std::ffi::OsString;
use std::process::Command;
use std::sync::atomic::Ordering;

const WORKER_MODE: &str = "NEOMACS_HOST_SYNC_TEST_WORKER";
const EXPECTED: &str = "NEOMACS_HOST_SYNC_TEST_EXPECTED";
const WORKER_TEST: &str = "keyboard::diagnostic_knobs::tests::process_startup_worker";

#[test]
fn process_startup_values_keep_exact_one_semantics() {
    let cases: Vec<(Option<OsString>, bool)> = vec![
        (None, false),
        (Some(OsString::from("")), false),
        (Some(OsString::from("0")), false),
        (Some(OsString::from("1")), true),
        (Some(OsString::from("01")), false),
        (Some(OsString::from("true")), false),
        (Some(OsString::from(" 1")), false),
        (Some(OsString::from("1 ")), false),
    ];
    #[cfg(unix)]
    let cases = {
        use std::os::unix::ffi::OsStringExt;
        let mut cases = cases;
        cases.push((Some(OsString::from_vec(vec![0xff])), false));
        cases
    };
    for (value, expected) in cases {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--test-threads=1")
            .env(WORKER_MODE, "1")
            .env(EXPECTED, if expected { "enabled" } else { "disabled" })
            .env_remove("NEOMACS_TRACE_HOST_SYNC");
        if let Some(value) = value.as_ref() {
            command.env("NEOMACS_TRACE_HOST_SYNC", value);
        }
        let output = command.output().expect("isolated diagnostic worker");
        assert!(
            output.status.success(),
            "startup value {value:?}: status={:?} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "worker filter must execute exactly one test: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn process_startup_worker() {
    if std::env::var_os(WORKER_MODE).as_deref() != Some(std::ffi::OsStr::new("1")) {
        return;
    }
    let expected = match std::env::var(EXPECTED).as_deref() {
        Ok("enabled") => true,
        Ok("disabled") => false,
        value => panic!("invalid worker expectation {value:?}"),
    };
    assert_eq!(HOST_SYNC_ENV_READS.load(Ordering::Relaxed), 0);
    let workers: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..64 {
                    assert_eq!(trace_host_sync_enabled(), expected);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("diagnostic reader worker");
    }
    assert_eq!(HOST_SYNC_ENV_READS.load(Ordering::Relaxed), 1);
}
