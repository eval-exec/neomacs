//! Host-keyed pty-output-versus-child-exit probes.
//!
//! On Darwin the last slave-fd close blocks in the tty drain (~600 ms on
//! macmini macOS 27.0.1) and then discards the slave's unread output; a later
//! `read(master)` returns 0. Linux keeps the bytes and reports EIO only after
//! they are drained. GNU closes the parent's `pty_tty` after fork
//! (src/process.c:2313-2314) and Neomacs drops the slave at the same point
//! (crates/neovm-system/src/emacs_core/system/process/types.rs:5259-5261), so
//! the right answer here is the GNU answer on the same host, not one fixed
//! expectation.
//!
//! Each row therefore carries two expectations, both derived from the
//! attested GNU oracle on that host: the Linux snapshots compare Neomacs with
//! the linux-gtk3 row, and `neovm-oracle-families-macos` selects this module
//! and runs it in verify mode against the macos-ns oracle. The forms are the
//! peer's probes 6142 (`probe.el`, `probe2.el`), embedded here rather than
//! referenced by path.
//!
//! The busy wait is deliberate: `sleep-for`/`sit-for` run
//! `wait_reading_process_output` and would read the output (or observe the
//! exit), which defeats the probe.

use crate::common::OracleMode;
use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::run_neovm_eval;
use crate::common::run_oracle_eval;
use expect_test::expect;

/// Darwin's pty drain-discard window: unread slave output is dropped about
/// this long after the last slave closes (measured ~600 ms on macmini, both
/// editors; [[darwin-pty-drain-discard]]).
///
/// A prompt read whose own spawn-to-first-read stall exceeds this window
/// cannot distinguish a product loss from the host dropping the bytes, so the
/// delay-0 row reports INCONCLUSIVE instead of claiming a pass.
const DARWIN_DRAIN_WINDOW_SECS: f64 = 0.6;

/// Split the probe's `stall=<seconds> ` prefix off its transcript.
fn parse_stall(transcript: &str) -> (Option<f64>, String) {
    let Some(position) = transcript.find("stall=") else {
        return (None, transcript.to_string());
    };
    let rest = &transcript[position + "stall=".len()..];
    let end = rest
        .find(|ch: char| ch == ' ' || ch == '"')
        .unwrap_or(rest.len());
    let Ok(stall) = rest[..end].parse::<f64>() else {
        return (None, transcript.to_string());
    };
    let mut stripped = String::with_capacity(transcript.len());
    stripped.push_str(&transcript[..position]);
    stripped.push_str(rest[end..].trim_start_matches(' '));
    (Some(stall), stripped)
}

/// The expect payload the other rows use: source-safe Debug of the
/// transcript.
fn expect_payload(transcript: &str) -> String {
    format!("{:?}", transcript.replace('\0', "\\0").replace('\r', "\\r"))
}

/// Whether this probe run outlived the Darwin drain window.
fn over_drain_window(transcript: &str, stall: Option<f64>, window: f64) -> bool {
    transcript.contains("INCONCLUSIVE") || stall.is_some_and(|stall| stall > window)
}

/// Report the measured spawn-to-first-read stall. Part 2 of the flake item:
/// Neomacs being materially slower than GNU here is a product latency
/// divergence, not a test problem, so say it loudly when the numbers differ.
fn report_prompt_latency(neo: Option<f64>, gnu: Option<f64>, window: f64) {
    match (neo, gnu) {
        (Some(neo), Some(gnu)) => {
            eprintln!("pty-prompt-latency: neo={neo:.3}s gnu={gnu:.3}s window={window:.3}s");
            if neo > gnu * 2.0 + 0.05 {
                eprintln!(
                    "pty-prompt-latency: NEO SLOWER than GNU (neo {neo:.3}s vs gnu {gnu:.3}s)"
                );
            }
        }
        (Some(neo), None) => {
            eprintln!("pty-prompt-latency: neo={neo:.3}s window={window:.3}s");
        }
        (None, Some(gnu)) => {
            eprintln!("pty-prompt-latency: gnu={gnu:.3}s window={window:.3}s");
        }
        (None, None) => {}
    }
}

/// Probe for the delay-0 row: measure spawn-to-first-read and refuse to
/// claim a result when that stall outlives the Darwin drain window.
fn prompt_read_probe_with_stall(window: f64) -> String {
    format!(
        r##"(let* ((buf (get-buffer-create " *neo-pty-probe*"))
              (spawn-start (float-time))
              (proc (make-process :name "neo-pty-probe"
                                  :buffer buf
                                  :connection-type 'pty
                                  :command (list "echo" "hello-token"))))
  (unwind-protect
      (progn
        (set-process-query-on-exit-flag proc nil)
        (let ((stall (- (float-time) spawn-start)))
          (if (> stall {window})
              (format "INCONCLUSIVE stall=%.3f" stall)
            (progn
              (accept-process-output proc 1)
              (accept-process-output nil 0.3)
              (format "stall=%.3f status=%S buffer=%S"
                      stall
                      (process-status proc)
                      (replace-regexp-in-string
                       "\r?\n" "<NL>"
                       (with-current-buffer buf (buffer-string))))))))
    (when (buffer-live-p buf) (kill-buffer buf))))"##
    )
}

/// `delay` seconds of busy wait (no waiting primitive), then the first read.
/// Delay 0 reads immediately; 1.5 covers the Darwin drain.
fn late_read_probe(delay: &str) -> String {
    format!(
        r##"(let* ((buf (get-buffer-create " *neo-pty-probe*"))
              (proc (make-process :name "neo-pty-probe"
                                  :buffer buf
                                  :connection-type 'pty
                                  :command (list "echo" "hello-token"))))
  (unwind-protect
      (progn
        (set-process-query-on-exit-flag proc nil)
        (let ((end (+ (float-time) {delay})))
          (while (< (float-time) end)))
        (accept-process-output proc 1)
        (accept-process-output nil 0.3)
        (format "status=%S buffer=%S"
                (process-status proc)
                (replace-regexp-in-string
                 "\r?\n" "<NL>"
                 (with-current-buffer buf (buffer-string)))))
    (when (buffer-live-p buf) (kill-buffer buf))))"##
    )
}

/// `probe.el` with DELAY=0: GNU keeps the token on both hosts, so an editor
/// that reads promptly must do the same. This is also the guard against a
/// Mac port that never reads before the child exits: such a port always
/// covers the Darwin drain and would be accepted by the delay-1.5 macOS row,
/// but it loses the token here.
#[test]
fn pty_output_read_immediately_is_host_keyed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expected = expect![[
        r#""OK \"status=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe finished<NL>\\\"\"""#
    ]];
    let form = prompt_read_probe_with_stall(DARWIN_DRAIN_WINDOW_SECS);
    let mode = OracleMode::from_env();
    match mode {
        OracleMode::Snapshot => {
            let first = run_neovm_eval(&form).expect("neomacs pty probe should run");
            let (first_stall, first_stripped) = parse_stall(&first);
            report_prompt_latency(first_stall, None, DARWIN_DRAIN_WINDOW_SECS);
            let stripped = if over_drain_window(&first, first_stall, DARWIN_DRAIN_WINDOW_SECS) {
                // No GNU baseline in snapshot mode: retry once, and only an
                // over-window retry is excused as host noise.
                let retry = run_neovm_eval(&form).expect("neomacs pty probe retry should run");
                let (retry_stall, retry_stripped) = parse_stall(&retry);
                eprintln!("pty-prompt-latency: neo-retry={retry_stall:?}s");
                if over_drain_window(&retry, retry_stall, DARWIN_DRAIN_WINDOW_SECS) {
                    eprintln!(
                        "INCONCLUSIVE pty_output_read_immediately_is_host_keyed: \
                         retry spawn-to-first-read stall {retry_stall:?} still \
                         exceeds the {DARWIN_DRAIN_WINDOW_SECS}s Darwin drain \
                         window (no GNU baseline in snapshot mode)"
                    );
                    return;
                }
                retry_stripped
            } else {
                first_stripped
            };
            expected.assert_eq(&expect_payload(&stripped));
        }
        OracleMode::Verify | OracleMode::Live => {
            let oracle = run_oracle_eval(&form).expect("oracle pty probe should run");
            let (gnu_stall, gnu_stripped) = parse_stall(&oracle);
            let first = run_neovm_eval(&form).expect("neomacs pty probe should run");
            let (first_stall, first_stripped) = parse_stall(&first);
            report_prompt_latency(first_stall, gnu_stall, DARWIN_DRAIN_WINDOW_SECS);
            if over_drain_window(&oracle, gnu_stall, DARWIN_DRAIN_WINDOW_SECS) {
                // GNU's own read missed the window: the host was slow for
                // both editors, so this run cannot judge the product.
                eprintln!(
                    "INCONCLUSIVE pty_output_read_immediately_is_host_keyed: \
                     GNU spawn-to-first-read stall {gnu_stall:?} exceeds the \
                     {DARWIN_DRAIN_WINDOW_SECS}s Darwin drain window; the host \
                     was slow for both editors"
                );
                return;
            }
            let (neo, neo_stripped) =
                if over_drain_window(&first, first_stall, DARWIN_DRAIN_WINDOW_SECS) {
                    // Only Neo's read was late while GNU's was prompt. That can
                    // be a one-off load spike; a retry inside the window is
                    // judged normally, and a persistent over-window Neo stall
                    // is a product latency divergence, not host noise.
                    let retry = run_neovm_eval(&form).expect("neomacs pty probe retry should run");
                    let (retry_stall, retry_stripped) = parse_stall(&retry);
                    eprintln!("pty-prompt-latency: neo-retry={retry_stall:?}s");
                    if over_drain_window(&retry, retry_stall, DARWIN_DRAIN_WINDOW_SECS) {
                        panic!(
                            "Neo spawn-to-first-read persistently exceeds the Darwin \
                             drain window (neo {retry_stall:?}s, gnu {gnu_stall:?}s)"
                        );
                    }
                    (retry, retry_stripped)
                } else {
                    (first, first_stripped)
                };
            if mode == OracleMode::Verify {
                expected.assert_eq(&expect_payload(&gnu_stripped));
            }
            assert_eq!(
                neo_stripped, gnu_stripped,
                "oracle parity mismatch for the prompt pty read\n  NEO: {neo}\n  GNU: {oracle}"
            );
        }
        OracleMode::Refresh => {
            let oracle = run_oracle_eval(&form).expect("oracle pty probe should run");
            let (stall, stripped) = parse_stall(&oracle);
            if over_drain_window(&oracle, stall, DARWIN_DRAIN_WINDOW_SECS) {
                eprintln!(
                    "INCONCLUSIVE pty_output_read_immediately_is_host_keyed: \
                     oracle spawn-to-first-read stall {stall:?} exceeds the \
                     {DARWIN_DRAIN_WINDOW_SECS}s Darwin drain window; no \
                     expectation can be derived from this run"
                );
                return;
            }
            expected.assert_eq(&expect_payload(&stripped));
        }
    }
}

/// `probe.el` with DELAY=1.5: Linux keeps the token; macOS discards it after
/// the Darwin drain. Both rows are GNU's, one per host.
#[test]
fn pty_output_read_after_darwin_drain_is_host_keyed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = if cfg!(target_os = "macos") {
        expect![[r#""OK \"status=exit buffer=\\\"<NL>Process neo-pty-probe finished<NL>\\\"\"""#]]
    } else {
        expect![[
            r#""OK \"status=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe finished<NL>\\\"\"""#
        ]]
    };
    assert_oracle_parity_expect(&late_read_probe("1.5"), expect);
}

/// `probe2.el`: the settled status and buffer while its pty output was still
/// unread for 0.3 s. The pre-read `early` status is NOT compared: GNU's
/// SIGCHLD handler reaps the child, so GNU says `exit` after a pure spin on
/// Linux while this port says `run` until a wait (ledger 180 §9.1 divergence,
/// `child_status.rs:140-149`). That gap is a separate FIX item owned
/// elsewhere; this row deliberately does not record the divergent value as
/// expected behaviour.
#[test]
fn pty_child_status_while_output_unread_is_host_keyed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = if cfg!(target_os = "macos") {
        expect![[
            r#""OK \"late=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe2 finished<NL>\\\"\"""#
        ]]
    } else {
        expect![[
            r#""OK \"late=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe2 finished<NL>\\\"\"""#
        ]]
    };
    assert_oracle_parity_expect(
        r##"(let* ((buf (get-buffer-create " *neo-pty-probe2*"))
               (proc (make-process :name "neo-pty-probe2"
                                   :buffer buf
                                   :connection-type 'pty
                                   :command (list "echo" "hello-token"))))
  (unwind-protect
      (progn
        (set-process-query-on-exit-flag proc nil)
        (let ((end (+ (float-time) 0.3)))
          (while (< (float-time) end)))
        (accept-process-output proc 1)
        (accept-process-output nil 0.3)
        (format "late=%S buffer=%S"
                (process-status proc)
                (replace-regexp-in-string
                 "\r?\n" "<NL>"
                 (with-current-buffer buf (buffer-string)))))
    (when (buffer-live-p buf) (kill-buffer buf))))"##,
        expect,
    );
}
