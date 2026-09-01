use std::time::Duration;

use crate::{CachedMelpaOracle, MAGIT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod blame;
mod clone;
mod formatting;
mod git;
mod prompts;
mod status;
mod workflows;

const MAGIT_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn magit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAGIT_MELPA_PIN, "magit.el")
        .expect("prepare pinned Magit source and dependencies below ./tmp")
        .with_prelude(
            r##"(progn
                   (setq magit-git-global-arguments
                         (append
                          '("-c" "init.defaultBranch=master"
                            "-c" "user.name=A U Thor"
                            "-c" "user.email=a.u.thor@example.com")
                          (and (boundp 'magit-git-global-arguments)
                               magit-git-global-arguments)))
                   (defun neomacs-magit-test-wait-for-process (process)
                     (while (process-live-p process)
                       ;; Magit gives Git a separate stderr pipe.  Drain every
                       ;; ready descriptor so its pipe closes before Magit's
                       ;; main-process sentinel kills the stderr buffer.
                       (accept-process-output nil 0.05)))
                   (defun neomacs-magit-test-new-processes (processes-before)
                     "Every process this scenario started since PROCESSES-BEFORE."
                     (seq-remove (lambda (process)
                                   (memq process processes-before))
                                 (process-list)))
                   (defun neomacs-magit-test-disown (processes-before)
                     "Stop this scenario's processes from querying on exit.
Magit's blame sentinel kills the Git stderr buffer, and
`process-kill-buffer-query-function' prompts for any attached process
whose status is still run/stop/open/listen and whose query-on-exit flag
is set.  Under load the stderr pipe has not always been retired by the
time that sentinel runs, so the prompt fires -- and in batch it reads
EOF from stdin, which kills the whole session before the case can even
reach its teardown.  Clearing the flag is the switch GNU provides for
exactly this, and it is symmetric across both editors."
                     (dolist (process (neomacs-magit-test-new-processes
                                       processes-before))
                       (set-process-query-on-exit-flag process nil)))
                   (defun neomacs-magit-test-wait-for-blame (processes-before)
                     "Wait for Magit's blame pipeline to finish.
Magit blames in two phases: a quickstart process whose sentinel installs
a full-file process, whose sentinel installs the final overlays and then
clears `magit-blame-process'.  Between the phases the process object is
dead but the variable is not yet nil, so stopping at `process-live-p'
can return before a single overlay exists -- which under load surfaces
as the case's own \"blame process completed without deterministic
overlays\" guard.  Cleared-to-nil is the real completion signal, so wait
for that.  Liveness was standing in as a hang guard; a deadline is the
right tool for that job and does not fire on a merely slow run.
Disown on every pass, because the replacement process is born after the
first one and would otherwise inherit a set query-on-exit flag."
                     (let ((deadline (+ (float-time) 60)))
                       (while (and magit-blame-process
                                   (< (float-time) deadline))
                         (neomacs-magit-test-disown processes-before)
                         (accept-process-output nil 0.05)))
                     (neomacs-magit-test-disown processes-before))
                   (defun neomacs-magit-test-settle (processes-before)
                     "Let Magit finish reaping before the scenario deletes anything.
Deleting a Git process while its sentinel is still in flight leaves the
sentinel acting on a dead process, which surfaces as \"Process git is
not active\".  Wait for this scenario's processes to exit and for the
sentinels those exits queued to run, then delete whatever is left.  The
wait is bounded so a genuinely stuck process fails the case instead of
hanging it."
                     (let ((deadline (+ (float-time) 30)))
                       (while (and (seq-some
                                    #'process-live-p
                                    (neomacs-magit-test-new-processes
                                     processes-before))
                                   (< (float-time) deadline))
                         (neomacs-magit-test-disown processes-before)
                         (accept-process-output nil 0.05))
                       ;; Exiting only queues the sentinels; drain until no
                       ;; descriptor has anything left to deliver.
                       (while (and (accept-process-output nil 0.05)
                                   (< (float-time) deadline))))
                     (dolist (process (neomacs-magit-test-new-processes
                                       processes-before))
                       (set-process-query-on-exit-flag process nil)
                       (delete-process process))))"##,
        )
        .with_timeout(MAGIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Magit parity test").into()
}

/// Multi-probe batch for `assert_magit_parity` cases (2a).
pub(crate) fn assert_magit_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(magit_oracle(), &name, "magit_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn magit_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        blame::blame_public_surface_batch_cases(),
        clone::clone_public_surface_batch_cases(),
        formatting::formatting_public_surface_batch_cases(),
        git::git_public_surface_batch_cases(),
        prompts::prompts_public_surface_batch_cases(),
        status::status_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_magit_batch(&cases);
}

// END generated package batch tests
