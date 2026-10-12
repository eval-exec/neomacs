use super::*;

#[test]
fn darwin_argument_blob_preserves_spaces_and_empty_arguments() {
    let mut blob = 4i32.to_ne_bytes().to_vec();
    blob.extend_from_slice(b"/bin/sh\0\0\0/bin/sh\0-c\0\0sleep 300\0IGNORED=environment\0");
    assert_eq!(
        parse_darwin_args(&blob).as_deref(),
        Some("/bin/sh -c  sleep 300")
    );
}

#[test]
fn darwin_argument_blob_rejects_truncated_or_invalid_counts() {
    assert!(parse_darwin_args(&[0, 1, 2]).is_none());
    assert!(parse_darwin_args(&(-1i32).to_ne_bytes()).is_none());
    let mut blob = 2i32.to_ne_bytes().to_vec();
    blob.extend_from_slice(b"/bin/sh\0/bin/sh\0unterminated");
    assert!(parse_darwin_args(&blob).is_none());
}

#[test]
fn darwin_cpu_time_preserves_nanoseconds_and_checks_overflow() {
    assert_eq!(
        DarwinCpuTime(1_000_000_123).seconds_and_nanoseconds(),
        (1, 123)
    );
    assert_eq!(
        DarwinCpuTime(999_999_999)
            .checked_add(DarwinCpuTime(2))
            .unwrap()
            .seconds_and_nanoseconds(),
        (1, 1)
    );
    assert!(
        DarwinCpuTime(u64::MAX)
            .checked_add(DarwinCpuTime(1))
            .is_none()
    );
}

#[test]
fn darwin_native_self_snapshot_has_host_identity_and_real_units() {
    let snapshot =
        darwin_process_snapshot(i64::from(std::process::id())).expect("own process is inspectable");
    // SAFETY: these identity queries have no pointer arguments or preconditions.
    let (uid, ppid, pgrp) = unsafe { (libc::geteuid(), libc::getppid(), libc::getpgrp()) };
    assert_eq!(snapshot.euid, uid);
    assert_eq!(snapshot.ppid, i64::from(ppid));
    assert_eq!(snapshot.pgrp, i64::from(pgrp));
    assert!(!snapshot.comm.is_empty());
    assert!(matches!(snapshot.state.as_str(), "R" | "S"));
    assert!(snapshot.start.seconds > 1_000_000_000);
    assert!(snapshot.start.microseconds < 1_000_000);
    let memory = snapshot.memory.expect("own task memory is inspectable");
    assert!(memory.virtual_kilobytes > 0);
    assert!(memory.resident_kilobytes > 0);
    assert!(memory.threads > 0);
    assert!(snapshot.usage.is_some());
    assert!(snapshot.args.is_some_and(|args| !args.is_empty()));
}

#[test]
fn darwin_process_enumeration_contains_this_process() {
    assert!(list_process_ids().contains(&i64::from(std::process::id())));
}
