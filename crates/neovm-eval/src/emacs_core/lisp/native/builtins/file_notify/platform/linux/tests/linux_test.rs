use super::*;

#[test]
fn combined_inotify_masks_keep_gnus_observable_aspect_order() {
    let mask =
        EventMask::ATTRIB | EventMask::CLOSE_WRITE | EventMask::MOVED_FROM | EventMask::ISDIR;
    assert_eq!(
        InotifyBackend::aspects(mask),
        ["isdir", "moved-from", "close-write", "attrib"]
    );
}

#[test]
fn activity_alone_does_not_retire_a_watch_without_control_acknowledgement() {
    let watch_id = WatchId::new(11, 0);
    let activity = WatchActivity::active();
    activity.terminate();
    let mut backend = InotifyBackend {
        worker: None,
        watches: vec![InotifyWatch {
            common: FileWatch {
                id: watch_id.clone(),
                request: InotifyRequest::new(vec!["modify".to_owned()]),
            },
            native_descriptor: 5,
            activity,
        }],
        ids: WatchIdAllocator::default(),
    };

    assert!(!backend.valid_p(&watch_id));
    let batch = backend.drain_events().expect("reconcile native lifecycle");
    assert!(batch.events.is_empty());
    assert!(batch.terminated.is_empty());
    assert!(backend.has_watches());
}

#[test]
fn stale_native_descriptor_event_does_not_target_a_reused_watch() {
    let old_activity = WatchActivity::active();
    old_activity.terminate();
    let new_activity = WatchActivity::active();
    let backend = InotifyBackend {
        worker: None,
        watches: vec![InotifyWatch {
            common: FileWatch {
                id: WatchId::new(12, 0),
                request: InotifyRequest::new(vec!["ignored".to_owned()]),
            },
            native_descriptor: 5,
            activity: new_activity,
        }],
        ids: WatchIdAllocator::default(),
    };

    let translated = backend.translate_event(NativeEvent {
        descriptor: 5,
        activity: Some(old_activity),
        mask: EventMask::IGNORED,
        cookie: 0,
        name: None,
    });

    assert!(translated.is_empty());
}

#[test]
fn inotify_request_keeps_creation_controls_out_of_the_event_filter() {
    let request = InotifyRequest::new(vec![
        "modify".to_owned(),
        "dont-follow".to_owned(),
        "onlydir".to_owned(),
    ]);

    assert!(request.watch_mask.contains(WatchMask::MODIFY));
    assert!(request.watch_mask.contains(WatchMask::DONT_FOLLOW));
    assert!(request.watch_mask.contains(WatchMask::ONLYDIR));
    assert!(request.event_mask.contains(EventMask::MODIFY));
    assert!(!request.event_mask.contains(EventMask::ATTRIB));
}
