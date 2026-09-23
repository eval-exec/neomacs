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
