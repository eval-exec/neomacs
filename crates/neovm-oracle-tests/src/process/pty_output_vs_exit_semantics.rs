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

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use expect_test::expect;

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
    let expect = if cfg!(target_os = "macos") {
        expect![[
            r#""OK \"status=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe finished<NL>\\\"\"""#
        ]]
    } else {
        expect![[
            r#""OK \"status=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe finished<NL>\\\"\"""#
        ]]
    };
    assert_oracle_parity_expect(&late_read_probe("0"), expect);
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
