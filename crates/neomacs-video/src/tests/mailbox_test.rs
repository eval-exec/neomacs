use super::mailbox::PresentationFrameQueue;

#[test]
fn invalidation_clears_the_stable_head_and_coalesced_successor() {
    let mut queue = PresentationFrameQueue::default();

    assert_eq!(queue.publish(10), None);
    assert_eq!(queue.publish(20), None);
    queue.clear();

    assert_eq!(queue.take(), None);
}
