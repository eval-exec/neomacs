//! Inject child status ahead of the final PTY read, without a clock race.

use super::*;
use crate::emacs_core::Context;
use std::io::{self, Cursor, Read};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Owned by one fixture's process manager; no global or thread-local hooks.
/// The inner reader supplies the unread master bytes at the process I/O seam.
struct InterruptedPtyRead {
    inner: Box<dyn Read + Send>,
    interruptions: usize,
    calls: Arc<AtomicUsize>,
}

impl Read for InterruptedPtyRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.interruptions > 0 {
            self.interruptions -= 1;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.inner.read(buffer)
    }
}

/// Inject the reaper's recorded status and unread master bytes independently.
/// Darwin can wait for output to drain during slave close, so a real blocking
/// child wait before reading cannot establish this ordering portably. These
/// fixture-owned events exercise the production status/read paths on every OS;
/// native PTY lifecycle coverage lives in the immediate-exit GNU oracle test.
fn recorded_pty_status_before_read(interruptions: usize) -> (Context, ProcessId, Arc<AtomicUsize>) {
    let mut eval = Context::new();
    let buffer = eval
        .eval_str(r#"(get-buffer-create " *pty-exit-drain*")"#)
        .expect("fixture buffer");
    let id = eval.processes.create_process(
        "pty-exit-drain".into(),
        buffer,
        "/bin/sh".into(),
        vec!["-c".into(), "printf 'process-output\\n'".into()],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    eval.processes
        .sync_process_mark(&mut eval.buffers, id)
        .expect("process mark");
    let proc = eval.processes.get_mut(id).expect("fixture process");
    // No actual pid is attached to this injected reap: teardown must never
    // signal or wait for an unrelated process on behalf of this fixture.
    proc.live_io.child = ChildOwnership::Reaped;
    let calls = Arc::new(AtomicUsize::new(0));
    proc.live_io.pty_reader = Some(PtyOutputReader::new(Box::new(InterruptedPtyRead {
        inner: Box::new(Cursor::new(b"process-output\n".to_vec())),
        interruptions,
        calls: Arc::clone(&calls),
    })));
    // The same status-recording seam used after the owning WNOHANG reaper.
    eval.processes
        .set_child_status_pending(id, process_status_exit_value(0));
    (eval, id, calls)
}

fn assert_complete_output_before_finished(eval: &mut Context, id: ProcessId) {
    eval.obarray
        .set_symbol_value("pty-exit-child", Value::make_process(id));
    eval.eval_str("(accept-process-output pty-exit-child 0)")
        .expect("service already-reaped PTY child");
    let result = eval
        .eval_str(
            r#"(list (process-status pty-exit-child)
             (save-current-buffer
               (set-buffer " *pty-exit-drain*") (buffer-string)))"#,
        )
        .expect("observe drained child");
    let mut options = crate::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    assert_eq!(
        crate::emacs_core::print::print_value_with_options(&result, options),
        r#"(exit "process-output\n\nProcess pty-exit-drain finished\n")"#,
        "exit publication must follow the final master read, including EINTR"
    );
}

#[test]
fn accept_process_output_pty_recorded_status_before_final_read_drains_output() {
    let (mut eval, id, calls) = recorded_pty_status_before_read(0);
    assert_complete_output_before_finished(&mut eval, id);
    assert!(
        calls.load(Ordering::Relaxed) >= 2,
        "read data then PTY EOF/EIO"
    );
}

#[test]
fn accept_process_output_pty_recorded_status_before_interrupted_read_drains_output() {
    let (mut eval, id, calls) = recorded_pty_status_before_read(2);
    assert_complete_output_before_finished(&mut eval, id);
    assert!(
        calls.load(Ordering::Relaxed) >= 4,
        "two interruptions, data, then PTY EOF/EIO must all precede retirement"
    );
}

/// Model the readiness/closure boundary before the final drain. portable-pty
/// translates an EIO from the PTY master to Ok(0), which must stop monitoring
/// without giving up the descriptor GNU status_notify reads once more.
struct EioBeforeFinalPtyRead {
    inner: Box<dyn Read + Send>,
    step: EioReadStep,
}

enum EioReadStep {
    Prefix,
    Eio,
    Remaining,
}

impl Read for EioBeforeFinalPtyRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.step {
            EioReadStep::Prefix => {
                self.step = EioReadStep::Eio;
                let limit = buffer.len().min(b"process-".len());
                let count = self.inner.read(&mut buffer[..limit])?;
                assert_eq!(
                    count, limit,
                    "fixture must expose the complete prefix before EIO"
                );
                Ok(count)
            }
            EioReadStep::Eio => {
                self.step = EioReadStep::Remaining;
                Ok(0)
            }
            EioReadStep::Remaining => self.inner.read(buffer),
        }
    }
}

#[test]
fn accept_process_output_pty_eio_keeps_master_for_exit_drain() {
    let (mut eval, id, _) = recorded_pty_status_before_read(0);
    // The injected reap has already happened; delay its notification record
    // until after a readable-event pass encounters PTY closure. This forces
    // both orders independently of whether the kernel delivers SIGCHLD first.
    eval.processes.clear_status_notify_pending(id);
    eval.processes.mark_status_change_notified(id);
    let proc = eval.processes.get_mut(id).expect("live process");
    let reader = proc.live_io.pty_reader.take().expect("PTY reader");
    proc.live_io.pty_reader = Some(PtyOutputReader::new(Box::new(EioBeforeFinalPtyRead {
        inner: Box::new(reader),
        step: EioReadStep::Prefix,
    })));
    eval.poll_process_output_for_readiness(vec![id], Some(id), false)
        .expect("read prefix before status notification");
    // A live service pass yields after one read. Observe EIO on the next
    // pass, before publishing the deliberately delayed child status.
    eval.poll_process_output_for_readiness(vec![id], Some(id), false)
        .expect("observe EIO before status notification");
    eval.processes
        .set_child_status_pending(id, process_status_exit_value(0));
    assert_complete_output_before_finished(&mut eval, id);
}

#[test]
fn pty_eof_zero_timeout_polls_do_not_read_master() {
    let (mut eval, id, calls) = recorded_pty_status_before_read(0);
    eval.processes.clear_status_notify_pending(id);
    eval.processes.mark_status_change_notified(id);
    let proc = eval.processes.get_mut(id).expect("live process");
    let reader = proc.live_io.pty_reader.take().expect("PTY reader");
    proc.live_io.pty_reader = Some(PtyOutputReader::new(Box::new(EioBeforeFinalPtyRead {
        inner: Box::new(reader),
        step: EioReadStep::Prefix,
    })));
    eval.poll_process_output_for_readiness(vec![id], Some(id), false)
        .expect("read prefix");
    eval.poll_process_output_for_readiness(vec![id], Some(id), false)
        .expect("remove EIO master from readiness");
    let before = calls.load(Ordering::Relaxed);
    eval.obarray
        .set_symbol_value("pty-exit-child", Value::make_process(id));
    eval.eval_str(
        r#"(let ((remaining 1000))
        (while (> remaining 0)
          (accept-process-output pty-exit-child 0)
          (input-pending-p)
          (setq remaining (1- remaining))))"#,
    )
    .expect("nonblocking polls after EOF");
    assert_eq!(
        calls.load(Ordering::Relaxed),
        before,
        "only status_notify may read an EOF-seen master again"
    );
}
