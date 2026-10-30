//! A continuously readable producer must yield to other process/input work.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::wait::MAX_MINIMUM_DRAIN_PASSES;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn round_robin_cursor_preserves_process_manager_footprint() {
    // WorldSlot<ProcessManager> reserves this original inline footprint in
    // Context. Growing it shifts the following fields, including the JIT ABI.
    // Mirror the pre-cursor fields so this guard works on every host, including
    // hosts that cannot run the Linux-only full Context offset table.
    #[allow(dead_code)]
    struct BeforeRoundRobin {
        processes: std::collections::HashMap<ProcessId, Process>,
        deleted_processes: std::collections::HashMap<ProcessId, Process>,
        next_id: ProcessId,
        process_tick: u64,
        default_read_config: ProcessReadConfig,
        env_overrides: std::collections::HashMap<LispString, Option<LispString>>,
        wait_backend: ProcessWaitBackend,
    }
    assert_eq!(
        std::mem::size_of::<ProcessManager>(),
        std::mem::size_of::<BeforeRoundRobin>()
    );
    assert_eq!(
        std::mem::align_of::<ProcessManager>(),
        std::mem::align_of::<BeforeRoundRobin>()
    );
}

struct ReadyChunks {
    remaining: usize,
    reads: Arc<AtomicUsize>,
}

impl Read for ReadyChunks {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.remaining -= 1;
        self.reads.fetch_add(1, Ordering::Relaxed);
        buffer[0] = b'x';
        Ok(1)
    }
}

fn ready_process(eval: &mut Context, name: &str, chunks: usize) -> (ProcessId, Arc<AtomicUsize>) {
    let buffer = eval
        .eval_str(&format!("(get-buffer-create \"{name}\")"))
        .unwrap();
    let id = eval.processes.create_process(
        name.into(),
        buffer,
        "/bin/sh".into(),
        vec!["-c".into(), "producer".into()],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    eval.processes
        .sync_process_mark(&mut eval.buffers, id)
        .unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    eval.processes.get_mut(id).unwrap().live_io.pty_reader =
        Some(PtyOutputReader::new(Box::new(ReadyChunks {
            remaining: chunks,
            reads: Arc::clone(&reads),
        })));
    (id, reads)
}

#[test]
fn continuous_output_yields_before_exhaustion_and_preserves_remaining_bytes() {
    for targeted in [false, true] {
        let mut eval = Context::new();
        let (hot, hot_reads) = ready_process(&mut eval, "hot-output", 128);
        let (peer, peer_reads) = ready_process(&mut eval, "peer-output", 1);
        let target = targeted.then_some(hot);

        eval.poll_process_output_for_readiness(vec![hot, peer], target, false)
            .unwrap();
        let first_pass = hot_reads.load(Ordering::Relaxed);
        assert!(
            first_pass > 0 && first_pass < 128,
            "return to the wait loop for timers, input and redisplay while output remains; read {first_pass} chunks"
        );
        assert_eq!(
            peer_reads.load(Ordering::Relaxed),
            0,
            "one readiness pass must stop after the first source with output"
        );

        // The cursor resumes after `hot`, including when it is the wait
        // target: another ready source must not wait for hot to block.
        let outcome = eval
            .poll_process_output_for_readiness(vec![hot, peer], target, false)
            .unwrap();
        assert!(outcome.has_any_process_activity());
        assert_eq!(outcome.has_target_process_activity(), !targeted);
        assert_eq!(
            peer_reads.load(Ordering::Relaxed),
            1,
            "round-robin must service the other ready source next"
        );
        assert_eq!(
            hot_reads.load(Ordering::Relaxed),
            first_pass,
            "the second pass must yield to the peer before reading hot again"
        );

        for _ in 0..128 {
            eval.poll_process_output_for_readiness(vec![hot, peer], target, false)
                .unwrap();
            if hot_reads.load(Ordering::Relaxed) == 128 {
                break;
            }
        }
        assert_eq!(hot_reads.load(Ordering::Relaxed), 128);
        let length = eval
            .eval_str("(save-current-buffer (set-buffer \"hot-output\") (buffer-size))")
            .unwrap();
        assert_eq!(
            length.as_int(),
            Some(128),
            "yielding must not discard output"
        );
    }
}

#[test]
fn accept_process_output_drains_a_ready_burst_in_one_call() {
    for (targeted, just_this_one) in [(false, false), (true, false), (true, true)] {
        let mut eval = Context::new();
        eval.set_variable("read-process-output-max", Value::make_int(1));
        eval.sync_process_read_config_from_visible_variables();
        let (hot, _) = ready_process(&mut eval, "accept-burst", 0);
        let (peer, _) = ready_process(&mut eval, "accept-peer", 0);
        // Real, already-written pipes participate in production live-source
        // discovery. Keep the writers open so EOF/final drain cannot hide an
        // incomplete readiness drain. Each read is capped at one byte.
        let mut writers = Vec::new();
        for (id, bytes) in [(hot, 128), (peer, 3)] {
            let (reader, mut writer) = os_pipe::pipe().unwrap();
            sys::set_fd_nonblocking(reader.as_raw_fd()).unwrap();
            writer.write_all(&vec![b'x'; bytes]).unwrap();
            let process = eval.processes.get_mut(id).unwrap();
            process.kind = ProcessKind::Pipe;
            process.childp = Value::NIL;
            process.live_io.pty_reader = None;
            process.live_io.child_stdout = Some(ChildOutputReader::Shared(reader));
            writers.push(writer);
        }
        eval.set_variable("accept-burst-process", Value::make_process(hot));
        eval.eval_str("(set-process-sentinel accept-burst-process (lambda (_p _event) nil))")
            .unwrap();
        let result = builtin_accept_process_output(
            &mut eval,
            vec![
                if targeted {
                    Value::make_process(hot)
                } else {
                    Value::NIL
                },
                Value::NIL,
                Value::NIL,
                Value::bool(just_this_one),
            ],
        )
        .unwrap();

        assert!(result.is_truthy());
        assert_eq!(
            eval.eval_str("(save-current-buffer (set-buffer \"accept-peer\") (buffer-size))")
                .unwrap()
                .as_int(),
            Some(if just_this_one { 0 } else { 3 })
        );
        assert_eq!(
            eval.eval_str("(save-current-buffer (set-buffer \"accept-burst\") (buffer-size))")
                .unwrap()
                .as_int(),
            Some(128),
            "one accept must drain the burst"
        );
        builtin_delete_process(&mut eval, vec![Value::make_process(hot)]).unwrap();
        assert_eq!(
            eval.eval_str("(save-current-buffer (set-buffer \"accept-burst\") (buffer-size))")
                .unwrap()
                .as_int(),
            Some(128),
            "deleting the process after one accept must retain its entire output"
        );
    }
}

#[test]
fn completed_accept_yields_after_a_bounded_minimum_drain() {
    let mut eval = Context::new();
    eval.set_variable("read-process-output-max", Value::make_int(1));
    eval.sync_process_read_config_from_visible_variables();
    let (hot, _) = ready_process(&mut eval, "bounded-drain", 0);
    // Fill a real pipe and keep the writer open: the source stays readable at
    // every follow-up pass, like a producer that never pauses. The pipe must
    // hold more than one follow-up bound so only the bound can end the drain.
    let (reader, mut writer) = os_pipe::pipe().unwrap();
    sys::set_fd_nonblocking(reader.as_raw_fd()).unwrap();
    sys::set_fd_nonblocking(writer.as_raw_fd()).unwrap();
    let chunk = [b'x'; 4096];
    let mut written = 0usize;
    loop {
        match writer.write(&chunk) {
            Ok(0) => break,
            Ok(n) => written += n,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("fill pipe: {error}"),
        }
    }
    assert!(
        written > MAX_MINIMUM_DRAIN_PASSES + 1,
        "pipe holds {written} bytes, too little to observe the follow-up bound"
    );
    {
        let process = eval.processes.get_mut(hot).unwrap();
        process.kind = ProcessKind::Pipe;
        process.childp = Value::NIL;
        process.live_io.pty_reader = None;
        process.live_io.child_stdout = Some(ChildOutputReader::Shared(reader));
    }
    eval.set_variable("bounded-drain-process", Value::make_process(hot));
    eval.eval_str("(set-process-sentinel bounded-drain-process (lambda (_p _event) nil))")
        .unwrap();
    let result = builtin_accept_process_output(
        &mut eval,
        vec![Value::make_process(hot), Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap();

    assert!(result.is_truthy());
    let drained = eval
        .eval_str(r#"(save-current-buffer (set-buffer "bounded-drain") (buffer-size))"#)
        .unwrap()
        .as_int()
        .unwrap() as usize;
    assert_eq!(
        drained,
        MAX_MINIMUM_DRAIN_PASSES + 1,
        "one readiness read followed by the bounded follow-up drain"
    );
    assert!(
        written > drained,
        "the rest of the ready burst must stay in the pipe for the next accept"
    );
    drop(writer);
}

#[test]
fn status_notification_pass_services_every_ready_source() {
    let mut eval = Context::new();
    let (hot, hot_reads) = ready_process(&mut eval, "status-hot", 128);
    let (peer, peer_reads) = ready_process(&mut eval, "status-peer", 1);

    eval.poll_process_output_for_status_notification(vec![hot, peer], None, false)
        .unwrap();
    assert_eq!(
        hot_reads.load(Ordering::Relaxed),
        1,
        "status walks each source once"
    );
    assert_eq!(
        peer_reads.load(Ordering::Relaxed),
        1,
        "status notification must not yield between sources"
    );
}

#[test]
fn stdout_status_probe_yields_while_output_remains() {
    let mut eval = Context::new();
    let (owner, reads) = ready_process(&mut eval, "probe-output", 128);
    let (_, disposition) = eval
        .poll_process_stdout_output_without_status_detailed(owner, Some(owner))
        .unwrap();
    assert_eq!(disposition, ProcessOutputDrainDisposition::Output);
    assert!(
        reads.load(Ordering::Relaxed) < 128,
        "the stderr-EOF/status probe must not monopolize the evaluator either"
    );
}

#[test]
fn split_stderr_yields_while_live_and_drains_before_the_exit_sentinel() {
    let mut eval = Context::new();
    let (owner, _) = ready_process(&mut eval, "owner-output", 1);
    let (stderr, reads) = ready_process(&mut eval, "stderr-output", 128);
    eval.processes.get_mut(owner).unwrap().stderrproc = Value::make_process(stderr);
    eval.set_variable("fairness-owner", Value::make_process(owner));
    eval.eval_str(
        r#"(set-process-sentinel fairness-owner
             (lambda (_p _message)
               (setq fairness-final-stderr-size
                 (save-current-buffer (set-buffer "stderr-output") (buffer-size)))))"#,
    )
    .unwrap();

    eval.poll_process_output_for_readiness(vec![owner], Some(owner), false)
        .unwrap();
    let live_reads = reads.load(Ordering::Relaxed);
    assert!(
        live_reads > 0 && live_reads < 128,
        "a busy stderr must yield too; read {live_reads} chunks"
    );

    eval.processes
        .set_child_status_pending(owner, process_status_exit_value(0));
    eval.run_process_status_notification(owner, Some(owner))
        .unwrap();
    assert_eq!(reads.load(Ordering::Relaxed), 128);
    assert_eq!(
        eval.eval_str("fairness-final-stderr-size")
            .unwrap()
            .as_int(),
        Some(128)
    );
}
