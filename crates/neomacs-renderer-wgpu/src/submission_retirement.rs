use std::sync::mpsc;
use std::thread::{self, JoinHandle};

/// Type-erased ownership retained until one queue submission has completed.
///
/// This is deliberately an ownership-only trait: the retirement thread never
/// inspects a browser frame or imported texture. Dropping the value is the
/// acknowledgement that the exact GPU submission which consumed it retired.
trait RetainedSubmission: Send {}

impl<T: Send> RetainedSubmission for T {}

enum RetirementCommand<S> {
    Retire {
        submission: S,
        retained: Box<dyn RetainedSubmission>,
    },
    Shutdown,
}

/// Serial submission fence which retires foreign resources off the render
/// thread.
///
/// A single FIFO worker matches wgpu queue ordering and avoids one thread per
/// frame. `S` is generic so the ordering/lifetime contract can be tested
/// without constructing a GPU device.
pub(crate) struct SubmissionRetirementQueue<S> {
    sender: mpsc::Sender<RetirementCommand<S>>,
    worker: Option<JoinHandle<()>>,
}

impl<S: Send + 'static> SubmissionRetirementQueue<S> {
    fn with_waiter(mut wait: impl FnMut(S) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("neomacs-gpu-retirement".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        RetirementCommand::Retire {
                            submission,
                            retained,
                        } => {
                            wait(submission);
                            drop(retained);
                        }
                        RetirementCommand::Shutdown => break,
                    }
                }
            })
            .expect("spawn GPU submission retirement worker");
        Self {
            sender,
            worker: Some(worker),
        }
    }

    /// Transfer ownership of resources used by `submission` to the retirement
    /// worker. The resources cannot be observed or released by safe code before
    /// the supplied waiter reports that submission complete.
    pub(crate) fn retire_after<R: Send + 'static>(&self, submission: S, retained: R) {
        // A disconnected worker means the renderer is already shutting down.
        // The failed command owns `retained`, so dropping the send error still
        // releases it exactly once.
        let _ = self.sender.send(RetirementCommand::Retire {
            submission,
            retained: Box::new(retained),
        });
    }
}

impl SubmissionRetirementQueue<wgpu::SubmissionIndex> {
    pub(crate) fn for_device(device: wgpu::Device) -> Self {
        Self::with_waiter(move |submission| {
            if let Err(error) = device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            }) {
                tracing::warn!(?error, "failed while retiring a WebView GPU submission");
            }
        })
    }
}

impl<S> Drop for SubmissionRetirementQueue<S> {
    fn drop(&mut self) {
        let _ = self.sender.send(RetirementCommand::Shutdown);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("GPU submission retirement worker panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};

    use super::SubmissionRetirementQueue;

    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn retained_frame_is_released_only_after_its_submission_retires() {
        let (wait_started_tx, wait_started_rx) = mpsc::channel();
        let (complete_tx, complete_rx) = mpsc::channel();
        let releases = Arc::new(AtomicUsize::new(0));
        let retirement = SubmissionRetirementQueue::with_waiter(move |submission| {
            wait_started_tx.send(submission).unwrap();
            complete_rx.recv().unwrap();
        });

        retirement.retire_after(41_u64, DropCounter(releases.clone()));
        assert_eq!(wait_started_rx.recv().unwrap(), 41);
        assert_eq!(releases.load(Ordering::SeqCst), 0);

        complete_tx.send(()).unwrap();
        drop(retirement);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
    }
}
