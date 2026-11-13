use super::*;

struct TestWatch {
    id: WatchId,
}

impl TrackedWatch for TestWatch {
    fn watch_id(&self) -> &WatchId {
        &self.id
    }
}

#[test]
fn recovery_observes_terminal_watches_before_they_are_retired() {
    let terminal_id = WatchId::new(1, 0);
    let mut watches = vec![
        TestWatch {
            id: terminal_id.clone(),
        },
        TestWatch {
            id: WatchId::new(2, 0),
        },
    ];
    let mut recoverable = Vec::new();

    finish_watch_drain(
        &mut watches,
        std::slice::from_ref(&terminal_id),
        |affected| {
            recoverable.extend(affected.iter().map(|watch| watch.id.clone()));
        },
    );

    assert_eq!(recoverable, [terminal_id.clone(), WatchId::new(2, 0)]);
    assert_eq!(watches.len(), 1);
    assert_eq!(watches[0].id, WatchId::new(2, 0));
}
