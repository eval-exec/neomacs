//! Force pipe status/read ordering with owned, already-written pipe bytes.

use crate::emacs_core::process::process_status_exit_value;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::data::process_state::pipe_exit_test_support::{
    PipeReader, ReadStep,
};
use neovm_system::emacs_core::process::*;
use neovm_value::value::Value;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    let buffer = eval
        .eval_str(r#"(get-buffer-create " *pipe-exit-drain*")"#)
        .expect("buffer");
    if unibyte {
        eval.eval_str(
            r#"(save-current-buffer (set-buffer " *pipe-exit-drain*") (set-buffer-multibyte nil))"#,
        )
        .expect("unibyte buffer");
    }
    let id = eval.processes_mut().create_process(
        "pipe-exit-drain".into(),
        buffer,
        "/bin/sh".into(),
        vec![],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    let (processes, buffers) = eval.processes_and_buffers_mut();
    processes.sync_process_mark(buffers, id).expect("mark");
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
    let proc = eval.processes_mut().get_mut(id).expect("record");
    proc.live_io.child = ChildOwnership::Reaped;
    proc.live_io.child_stdout = Some(ChildOutputReader::Injected(PipeReader::new(
        Box::new(reader),
        steps,
        Arc::clone(&calls),
    )));
    proc.coding_decode = Value::symbol("utf-8-dos");
    eval.obarray_mut()
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
    let mut options = neovm_eval::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    let expected = if unibyte {
        r#"(exit ("finished\n" nil (99 97 102 195 169 10)))"#
    } else {
        r#"(exit ("finished\n" nil (99 97 102 233 10)))"#
    };
    assert_eq!(
        neovm_eval::emacs_core::print::print_value_with_options(&actual, options),
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
            eval.processes_mut()
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
            eval.processes_mut()
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
            eval.processes_mut()
                .set_child_status_pending(id, process_status_exit_value(0));
            assert_pipe_finished(&mut eval, unibyte);
            assert!(calls.load(Ordering::Relaxed) >= 2, "data then decoder EOF");
        }
    }
}
