//! Force pipe status/read ordering with owned, already-written pipe bytes.

use super::*;
use crate::emacs_core::Context;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Test-only read seam around a real owned pipe. The fixture's sole mutator
/// injects interruptions; no global hooks, sleeps, signals or borrowed PIDs.
pub(super) struct PipeReader {
    reader: Box<ChildOutputReader>,
    steps: VecDeque<ReadStep>,
    calls: Arc<AtomicUsize>,
}

impl std::fmt::Debug for PipeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeReader")
            .field("fd", &self.reader.as_raw_fd())
            .field("steps", &self.steps)
            .field("calls", &self.calls)
            .finish()
    }
}

#[derive(Debug)]
enum ReadStep {
    Prefix(usize),
    Interrupted,
}

impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match self.steps.pop_front() {
            Some(ReadStep::Interrupted) => Err(io::ErrorKind::Interrupted.into()),
            Some(ReadStep::Prefix(limit)) => {
                let len = buffer.len().min(limit);
                self.reader.read(&mut buffer[..len])
            }
            None => self.reader.read(buffer),
        }
    }
}

impl AsRawFd for PipeReader {
    fn as_raw_fd(&self) -> RawFd {
        self.reader.as_raw_fd()
    }
}

#[derive(Clone, Copy, Debug)]
enum PipeFlavor {
    Shared,
    Stdout,
}

fn recorded_pipe_before_read(
    flavor: PipeFlavor,
    unibyte: bool,
    steps: VecDeque<ReadStep>,
) -> (Context, ProcessId, Arc<AtomicUsize>) {
    let mut eval = Context::new();
    let buffer = eval
        .eval_str(r#"(get-buffer-create " *pipe-exit-drain*")"#)
        .expect("buffer");
    if unibyte {
        eval.eval_str(
            r#"(save-current-buffer (set-buffer " *pipe-exit-drain*") (set-buffer-multibyte nil))"#,
        )
        .expect("unibyte buffer");
    }
    let id = eval.processes.create_process(
        "pipe-exit-drain".into(),
        buffer,
        "/bin/sh".into(),
        vec![],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    eval.processes
        .sync_process_mark(&mut eval.buffers, id)
        .expect("mark");
    let (reader, mut writer) = os_pipe::pipe().expect("owned pipe");
    writer
        .write_all(b"caf\xc3\xa9\r\n")
        .expect("queued final output");
    drop(writer);
    let reader = match flavor {
        PipeFlavor::Shared => ChildOutputReader::Shared(reader),
        PipeFlavor::Stdout => {
            ChildOutputReader::Stdout(std::fs::File::from(std::os::fd::OwnedFd::from(reader)))
        }
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let proc = eval.processes.get_mut(id).expect("record");
    proc.live_io.child = ChildOwnership::Reaped;
    proc.live_io.child_stdout = Some(ChildOutputReader::Injected(PipeReader {
        reader: Box::new(reader),
        steps,
        calls: Arc::clone(&calls),
    }));
    proc.coding_decode = Value::symbol("utf-8-dos");
    eval.obarray
        .set_symbol_value("pipe-exit-child", Value::make_process(id));
    eval.eval_str(
        r#"(progn
      (setq pipe-exit-seen nil)
      (set-process-sentinel pipe-exit-child
        (lambda (p message)
          (setq pipe-exit-seen
            (list message (memq p (process-list))
              (save-current-buffer (set-buffer (process-buffer p))
                (append (buffer-string) nil)))))))"#,
    )
    .expect("sentinel observes output before retirement");
    (eval, id, calls)
}

fn assert_pipe_finished(eval: &mut Context, unibyte: bool) {
    eval.eval_str("(accept-process-output pipe-exit-child 0)")
        .expect("service pipe");
    let actual = eval
        .eval_str("(list (process-status pipe-exit-child) pipe-exit-seen)")
        .expect("sentinel result");
    let mut options = crate::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    let expected = if unibyte {
        r#"(exit ("finished\n" nil (99 97 102 195 169 10)))"#
    } else {
        r#"(exit ("finished\n" nil (99 97 102 233 10)))"#
    };
    assert_eq!(
        crate::emacs_core::print::print_value_with_options(&actual, options),
        expected,
        "the sentinel must see all pipe bytes decoded before terminal retirement"
    );
}

#[test]
fn accept_process_output_pipe_recorded_status_before_interrupted_read_drains_output() {
    for flavor in [PipeFlavor::Shared, PipeFlavor::Stdout] {
        for unibyte in [false, true] {
            let (mut eval, id, calls) = recorded_pipe_before_read(
                flavor,
                unibyte,
                VecDeque::from([ReadStep::Interrupted, ReadStep::Interrupted]),
            );
            eval.processes
                .set_child_status_pending(id, process_status_exit_value(0));
            assert_pipe_finished(&mut eval, unibyte);
            assert!(
                calls.load(Ordering::Relaxed) >= 4,
                "interruptions, data, EOF"
            );
        }
    }
}

#[test]
fn accept_process_output_pipe_interrupted_read_before_status_keeps_remaining_output() {
    for flavor in [PipeFlavor::Shared, PipeFlavor::Stdout] {
        for unibyte in [false, true] {
            let (mut eval, id, _) = recorded_pipe_before_read(
                flavor,
                unibyte,
                VecDeque::from([ReadStep::Prefix(4), ReadStep::Interrupted]),
            );
            // No recorded status yet: a readiness pass must not mistake EINTR
            // after a UTF-8 prefix for permanent failure and close the pipe.
            eval.poll_process_output_for_readiness(vec![id], Some(id), false)
                .expect("readiness pass");
            eval.processes
                .set_child_status_pending(id, process_status_exit_value(0));
            assert_pipe_finished(&mut eval, unibyte);
        }
    }
}

#[test]
fn accept_process_output_pipe_recorded_status_drains_data_and_eof_before_sentinel() {
    for flavor in [PipeFlavor::Shared, PipeFlavor::Stdout] {
        for unibyte in [false, true] {
            let (mut eval, id, calls) = recorded_pipe_before_read(flavor, unibyte, VecDeque::new());
            eval.processes
                .set_child_status_pending(id, process_status_exit_value(0));
            assert_pipe_finished(&mut eval, unibyte);
            assert!(calls.load(Ordering::Relaxed) >= 2, "data then decoder EOF");
        }
    }
}
