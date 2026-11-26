use std::path::Path;
use std::sync::Arc;

use super::{net, process_attributes, signals};

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root two levels above neovm-system")
}

fn read_source(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Strip line comments so doc comments explaining the old libc hazard do not
/// trip the source guard.
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Soundness C2 + C3 guard: the Unix lookup and error/signal formatting helpers
/// in `neovm-system`, `neovm-edit`, and `neovm-value` must not call
/// unsynchronized non-reentrant libc functions (`getpwuid`, `getgrgid`,
/// `getservbyname`, `strerror`, `strsignal`) on Unix.
#[test]
fn c2_c3_no_unsynchronized_non_reentrant_libc_calls() {
    let mut violations = Vec::new();

    for rel in [
        "crates/neovm-value/src/system_vocab/posix_lookup.rs",
        "crates/neovm-core/src/emacs_core/editing/dired/mod.rs",
        "crates/neovm-edit/src/emacs_core/editing/editfns/mod.rs",
        "crates/neovm-system/src/emacs_core/system/process/sys/process_attributes.rs",
        "crates/neovm-system/src/emacs_core/system/process/sys/net.rs",
        "crates/neovm-system/src/emacs_core/system/process/sys/signals.rs",
        "crates/neovm-system/src/emacs_core/system/callproc/mod.rs",
    ] {
        let code = strip_line_comments(&read_source(rel));
        for forbidden in ["libc::getpwuid(", "libc::getgrgid(", "libc::strerror("] {
            if code.contains(forbidden) {
                violations.push(format!("{rel}: calls non-reentrant `{forbidden}`"));
            }
        }
    }

    // `neovm-value/src/system_vocab/errno.rs` must use `libc::strerror_r` on
    // Unix (Windows CRT `strerror` stays behind `#[cfg(not(unix))]`).
    let errno_code =
        strip_line_comments(&read_source("crates/neovm-value/src/system_vocab/errno.rs"));
    if !errno_code.contains("libc::strerror_r(") {
        violations.push("errno.rs: must use reentrant `libc::strerror_r(` on Unix".into());
    }

    // `callproc/mod.rs` must delegate to `signals::raw_signal_description`
    // rather than calling `libc::strsignal(` directly without the shared lock.
    let callproc_code = strip_line_comments(&read_source(
        "crates/neovm-system/src/emacs_core/system/callproc/mod.rs",
    ));
    if callproc_code.contains("libc::strsignal(") {
        violations.push("callproc/mod.rs: must not call `libc::strsignal(` directly".into());
    }

    // `signals.rs` must route every `libc::strsignal(` call through a single
    // `STRSIGNAL_LOCK`-synchronized helper.
    let signals_code = strip_line_comments(&read_source(
        "crates/neovm-system/src/emacs_core/system/process/sys/signals.rs",
    ));
    let strsignal_count = signals_code.matches("libc::strsignal(").count();
    if strsignal_count > 1 || (strsignal_count == 1 && !signals_code.contains("STRSIGNAL_LOCK")) {
        violations.push(format!(
            "signals.rs: expected at most one STRSIGNAL_LOCK-guarded `libc::strsignal(` call, found {strsignal_count}"
        ));
    }

    // `net.rs` must use `getservbyname_r` on Linux and synchronize any
    // non-Linux fallback with `SERVENT_LOCK`.
    let net_code = strip_line_comments(&read_source(
        "crates/neovm-system/src/emacs_core/system/process/sys/net.rs",
    ));
    if !net_code.contains("getservbyname_r") {
        violations.push("net.rs: must use `getservbyname_r` on Linux".into());
    }
    if net_code.contains("libc::getservbyname(") && !net_code.contains("SERVENT_LOCK") {
        violations.push(
            "net.rs: non-Linux `libc::getservbyname(` fallback must hold `SERVENT_LOCK`".into(),
        );
    }

    assert!(
        violations.is_empty(),
        "C2/C3 non-reentrant libc violations:\n{}",
        violations.join("\n")
    );
}

/// Concurrent-call stress row for C2 + C3: hammer `user_name`, `group_name`,
/// `service_port`, `errno_description` / `emacs_strerror`, `signal_description`,
/// and `signal_number_from_description` across 8 OS threads and assert every
/// call matches the single-threaded baseline byte-for-byte.
#[test]
fn c2_c3_concurrent_os_lookups_match_single_threaded_baselines() {
    let euid = unsafe { libc::geteuid() } as u32;
    let egid = unsafe { libc::getegid() } as u32;
    let missing_id = u32::MAX - 1;

    let expected_user = process_attributes::user_name(euid);
    let expected_missing_user = process_attributes::user_name(missing_id);
    let expected_group = process_attributes::group_name(egid);
    let expected_missing_group = process_attributes::group_name(missing_id);
    let expected_http = net::service_port("http", "tcp");
    let expected_missing_svc = net::service_port("neomacs-nonexistent-service-c2", "tcp");
    assert_eq!(expected_missing_svc, None);

    let errnos = [libc::ENOENT, libc::EACCES, libc::EINVAL, libc::EPERM, 99999];
    let expected_errnos: Vec<(i32, String, String)> = errnos
        .into_iter()
        .map(|e| {
            (
                e,
                signals::errno_description(e),
                neovm_value::system_vocab::errno::emacs_strerror(e),
            )
        })
        .collect();
    assert_eq!(expected_errnos[0].1, "No such file or directory");
    assert_eq!(expected_errnos[0].2, "No such file or directory");

    let sigs = [
        libc::SIGHUP,
        libc::SIGINT,
        libc::SIGKILL,
        libc::SIGSEGV,
        libc::SIGTERM,
        99,
    ];
    let expected_sigs: Vec<(i32, String)> = sigs
        .into_iter()
        .map(|s| (s, signals::signal_description(s)))
        .collect();
    let expected_term_num = signals::signal_number_from_description("Terminated");

    let expected = Arc::new((
        expected_user,
        expected_missing_user,
        expected_group,
        expected_missing_group,
        expected_http,
        expected_errnos,
        expected_sigs,
        expected_term_num,
    ));

    let threads: Vec<_> = (0..8)
        .map(|tid| {
            let expected = Arc::clone(&expected);
            std::thread::spawn(move || {
                let (
                    ref exp_user,
                    ref exp_missing_user,
                    ref exp_group,
                    ref exp_missing_group,
                    exp_http,
                    ref exp_errnos,
                    ref exp_sigs,
                    exp_term_num,
                ) = *expected;

                for iter in 0..150usize {
                    assert_eq!(process_attributes::user_name(euid), *exp_user);
                    assert_eq!(process_attributes::user_name(missing_id), *exp_missing_user);
                    assert_eq!(process_attributes::group_name(egid), *exp_group);
                    assert_eq!(
                        process_attributes::group_name(missing_id),
                        *exp_missing_group
                    );
                    assert_eq!(net::service_port("http", "tcp"), exp_http);
                    assert_eq!(
                        net::service_port("neomacs-nonexistent-service-c2", "tcp"),
                        None
                    );

                    let &(errno, ref exp_desc, ref exp_val) =
                        &exp_errnos[(tid + iter) % exp_errnos.len()];
                    assert_eq!(signals::errno_description(errno), *exp_desc);
                    assert_eq!(
                        neovm_value::system_vocab::errno::emacs_strerror(errno),
                        *exp_val
                    );

                    let &(sig, ref exp_sig_desc) = &exp_sigs[(tid + iter) % exp_sigs.len()];
                    assert_eq!(signals::signal_description(sig), *exp_sig_desc);
                    assert_eq!(
                        signals::signal_number_from_description("Terminated"),
                        exp_term_num
                    );
                }
            })
        })
        .collect();

    for handle in threads {
        handle.join().expect("worker thread panicked");
    }
}
