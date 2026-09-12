//! Ordered, frontend-initiated request to flush unsaved work to disk.
//!
//! Android and iOS stop an application before the OS becomes free to kill its
//! process, and the kill arrives with no further notice. GNU's autosave is
//! driven by keystroke and idle counters (`src/keyboard.c`), which stop
//! counting the moment the process stops running — so on a mobile host nothing
//! would ever flush between the last keystroke and process death.
//!
//! This is deliberately `do-auto-save`, not `save-some-buffers`: autosave
//! writes `#file#` alongside the original and is recovered with `recover-file`,
//! which is the semantics an Emacs user already understands. Writing the user's
//! real files unprompted because they switched apps would be a surprise, and an
//! irreversible one.
//!
//! Serviced in input order like [`crate::ime::ImeRequest`], not as an urgent
//! interrupt: Lisp may itself be reading input, including inside a recursive
//! minibuffer, and cutting in front of that would run arbitrary Lisp at a point
//! the evaluator has not agreed to.

use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, bounded};

/// Outcome of one flush, reported back to the frontend that asked.
///
/// A frontend uses this only for diagnostics: it must not retry, because the
/// host may already be past the point where it is allowed to run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistOutcome {
    /// `do-auto-save` ran to completion.
    Flushed,
    /// `do-auto-save` signalled. Autosave is best-effort in GNU too — a single
    /// unwritable buffer must not prevent the host from stopping.
    Failed,
}

/// Reply channel for a [`PersistRequest`].
///
/// Mirrors the IME reply sender: bounded at one, and a send failure is ignored
/// so an abandoned request can never block the VM, including during shutdown.
#[derive(Clone)]
pub struct PersistReplySender {
    sender: Sender<PersistOutcome>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for PersistReplySender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistReplySender").finish_non_exhaustive()
    }
}

impl PersistReplySender {
    fn channel(notify: Arc<dyn Fn() + Send + Sync>) -> (Self, Receiver<PersistOutcome>) {
        let (sender, receiver) = bounded(1);
        (Self { sender, notify }, receiver)
    }

    fn complete(self, outcome: PersistOutcome) {
        if self.sender.try_send(outcome).is_ok() {
            (self.notify)();
        }
    }
}

/// Flush unsaved work, answered at the evaluator's next input read.
#[derive(Clone, Debug)]
pub struct PersistRequest {
    reply: PersistReplySender,
}

impl PersistRequest {
    /// Build a request and the receiver its outcome arrives on.
    ///
    /// `notify` runs on the VM thread and must only wake the frontend — it must
    /// not block, panic, or re-enter the evaluator.
    pub fn auto_save(notify: Arc<dyn Fn() + Send + Sync>) -> (Self, Receiver<PersistOutcome>) {
        let (reply, receiver) = PersistReplySender::channel(notify);
        (Self { reply }, receiver)
    }
}

impl crate::emacs_core::eval::Context {
    /// Service one [`PersistRequest`] on the VM thread.
    pub(crate) fn dispatch_persist_request(&mut self, request: PersistRequest) {
        // `t` is GNU's NO-MESSAGE argument (`src/fileio.c`, Fdo_auto_save):
        // the host is stopping, so there is no echo area left to read.
        let outcome = match self.eval_str("(do-auto-save t)") {
            Ok(_) => PersistOutcome::Flushed,
            Err(_) => PersistOutcome::Failed,
        };
        request.reply.complete(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::eval::Context;

    fn request() -> (PersistRequest, Receiver<PersistOutcome>) {
        PersistRequest::auto_save(Arc::new(|| {}))
    }

    #[test]
    fn dispatching_a_persist_request_runs_auto_save_and_reports_it() {
        let mut eval = Context::new();
        let (request, receiver) = request();

        eval.dispatch_persist_request(request);

        assert_eq!(
            receiver.try_recv(),
            Ok(PersistOutcome::Flushed),
            "the frontend must learn that the flush completed"
        );
    }

    #[test]
    fn a_modified_buffer_gets_an_auto_save_file_when_the_host_stops() {
        // The whole point of the request: work typed since the last autosave
        // must reach disk before Android is free to kill the process. Needs a
        // booted runtime -- `find-file` lives in files.el, so a bare
        // `Context::new()` cannot visit a file at all.
        crate::test_utils::init_test_tracing();
        let mut eval = crate::test_utils::runtime_startup_context();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        std::fs::create_dir_all(&root).expect("repo-local scratch root");
        let directory = tempfile::Builder::new()
            .prefix("neomacs-persist-")
            .tempdir_in(root)
            .expect("scratch directory");
        let path = directory.path().join("notes.txt");
        std::fs::write(&path, b"saved\n").expect("seed the visited file");

        eval.eval_str(&format!(
            "(progn (find-file {:?}) (goto-char (point-max)) (insert \"unsaved\"))",
            path.to_string_lossy()
        ))
        .expect("open and modify the buffer");

        let (request, receiver) = request();
        eval.dispatch_persist_request(request);
        assert_eq!(receiver.try_recv(), Ok(PersistOutcome::Flushed));

        // GNU names the autosave `#notes.txt#` beside the original
        // (`auto-save-file-name-transforms` default, files.el).
        let auto_save = directory.path().join("#notes.txt#");
        assert!(
            auto_save.exists(),
            "the host stopped with unsaved work and no recovery file appeared; \
             directory held {:?}",
            std::fs::read_dir(directory.path())
                .map(|entries| entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
    }

    #[test]
    fn an_abandoned_request_never_blocks_the_vm() {
        // The frontend may already be gone when the flush finishes: Android can
        // tear the Activity down while the evaluator is still working.
        let mut eval = Context::new();
        let (request, receiver) = request();
        drop(receiver);

        eval.dispatch_persist_request(request);
    }
}
