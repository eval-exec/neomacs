use super::*;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("target")
        .join("neovm-core-file-notify-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("inotify-worker-")
        .tempdir_in(parent)
        .expect("create file notification fixture")
}

#[test]
fn failed_native_removal_retires_worker_registration_like_gnu() {
    let directory = workspace_temp_dir();
    let mut inotify = Inotify::init().expect("initialize inotify");
    let watch_descriptor = inotify
        .watches()
        .add(directory.path(), WatchMask::MODIFY)
        .expect("add native watch");
    let descriptor = watch_descriptor.get_watch_descriptor_id();
    let activity = WatchActivity::active();
    let mut descriptors = HashMap::from([(
        descriptor,
        NativeWatch::active(watch_descriptor.clone(), activity.clone()),
    )]);

    // Invalidate the kernel descriptor behind the worker map, then ask the
    // command path to remove it. GNU reports the syscall error after retiring
    // the public descriptor, so the worker must also forget the registration.
    inotify
        .watches()
        .remove(watch_descriptor)
        .expect("invalidate native watch");
    let (command_tx, command_rx) = crossbeam_channel::bounded(1);
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    command_tx
        .send(Command::Remove {
            descriptor,
            reply: reply_tx,
        })
        .expect("queue removal");
    let (event_tx, _event_rx) = delivery::channel(None);

    assert_eq!(
        apply_next_command(&mut inotify, &command_rx, &mut descriptors, &event_tx),
        CommandOutcome::Applied
    );
    assert!(reply_rx.recv().expect("receive removal result").is_err());
    assert!(!descriptors.contains_key(&descriptor));
    assert!(!activity.is_active());
}

#[test]
fn dropped_add_reply_rolls_back_new_native_registration() {
    let directory = workspace_temp_dir();
    let mut inotify = Inotify::init().expect("initialize inotify");
    let mut descriptors = HashMap::new();
    let (command_tx, command_rx) = crossbeam_channel::bounded(1);
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    drop(reply_rx);
    command_tx
        .send(Command::Add {
            path: directory.path().to_path_buf(),
            mask: WatchMask::MODIFY,
            reply: reply_tx,
        })
        .expect("queue add with abandoned reply");
    let (event_tx, event_rx) = delivery::channel(None);
    assert_eq!(
        apply_next_command(&mut inotify, &command_rx, &mut descriptors, &event_tx),
        CommandOutcome::Applied
    );
    assert!(
        descriptors.is_empty(),
        "unobserved add retained native ownership"
    );
    assert!(matches!(
        event_rx.drain_consistent().records.as_slice(),
        [DeliveryRecord::Event(NativeEvent { mask, .. })] if mask.contains(EventMask::IGNORED)
    ));
}

#[test]
fn remove_then_add_is_separated_by_the_native_event_boundary() {
    let first_directory = workspace_temp_dir();
    let second_directory = workspace_temp_dir();
    let mut inotify = Inotify::init().expect("initialize inotify");
    let watch_descriptor = inotify
        .watches()
        .add(first_directory.path(), WatchMask::MODIFY)
        .expect("add initial native watch");
    let descriptor = watch_descriptor.get_watch_descriptor_id();
    let old_activity = WatchActivity::active();
    let mut descriptors = HashMap::from([(
        descriptor,
        NativeWatch::active(watch_descriptor, old_activity.clone()),
    )]);
    let (command_tx, command_rx) = crossbeam_channel::bounded(2);
    let (remove_reply_tx, remove_reply_rx) = crossbeam_channel::bounded(1);
    let (add_reply_tx, add_reply_rx) = crossbeam_channel::bounded(1);
    command_tx
        .send(Command::Remove {
            descriptor,
            reply: remove_reply_tx,
        })
        .expect("queue removal");
    command_tx
        .send(Command::Add {
            path: second_directory.path().to_path_buf(),
            mask: WatchMask::MODIFY,
            reply: add_reply_tx,
        })
        .expect("queue replacement");

    let (event_tx, event_rx) = delivery::channel(None);
    assert_eq!(
        apply_next_command(&mut inotify, &command_rx, &mut descriptors, &event_tx),
        CommandOutcome::Applied
    );
    assert!(
        remove_reply_rx
            .recv()
            .expect("receive removal result")
            .is_ok()
    );
    assert!(
        add_reply_rx.try_recv().is_err(),
        "only one command is applied per turn"
    );
    assert!(!old_activity.is_active());
    let delivery = event_rx.drain_consistent();
    assert!(matches!(
        delivery.records.as_slice(),
        [DeliveryRecord::Event(NativeEvent {
            activity: None,
            mask,
            ..
        })] if mask.contains(EventMask::IGNORED)
    ));

    assert_eq!(
        apply_next_command(&mut inotify, &command_rx, &mut descriptors, &event_tx),
        CommandOutcome::Applied
    );
    add_reply_rx
        .recv()
        .expect("receive replacement result")
        .expect("add replacement watch");
}

#[test]
fn add_drains_an_automatic_terminal_boundary_before_it_commits() {
    let first_directory = workspace_temp_dir();
    let first_file = first_directory.path().join("retired");
    std::fs::write(&first_file, b"old").expect("create initial watched file");
    let second_directory = workspace_temp_dir();
    let second_file = second_directory.path().join("replacement");
    std::fs::write(&second_file, b"new").expect("create replacement watched file");

    let mut inotify = Inotify::init().expect("initialize inotify");
    let watch_descriptor = inotify
        .watches()
        .add(&first_file, WatchMask::DELETE_SELF | WatchMask::MODIFY)
        .expect("add initial native watch");
    let descriptor = watch_descriptor.get_watch_descriptor_id();
    let old_activity = WatchActivity::active();
    let mut descriptors = HashMap::from([(
        descriptor,
        NativeWatch::active(watch_descriptor, old_activity.clone()),
    )]);
    std::fs::remove_file(&first_file).expect("automatically invalidate initial watch");

    let (command_tx, command_rx) = crossbeam_channel::bounded(1);
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    command_tx
        .send(Command::Add {
            path: second_file,
            mask: WatchMask::MODIFY,
            reply: reply_tx,
        })
        .expect("queue replacement add");
    let (event_tx, event_rx) = delivery::channel(None);

    assert_eq!(
        apply_next_command(&mut inotify, &command_rx, &mut descriptors, &event_tx),
        CommandOutcome::Applied
    );
    let (_, new_activity) = reply_rx
        .recv()
        .expect("receive replacement result")
        .expect("add replacement watch");
    assert!(!old_activity.is_active());
    assert!(!new_activity.same_registration(&old_activity));
    assert!(event_rx.drain_consistent().records.iter().any(|record| {
        matches!(
            record,
            DeliveryRecord::Control(WorkerControl::Terminal(NativeEvent {
                activity: Some(activity),
                mask,
                ..
            })) if activity.same_registration(&old_activity)
                && mask.contains(EventMask::IGNORED)
        )
    }));
}

#[test]
fn newly_added_watch_must_survive_its_provisional_boundary() {
    let directory = workspace_temp_dir();
    let file = directory.path().join("short-lived");
    std::fs::write(&file, b"contents").expect("create watched file");
    let mut inotify = Inotify::init().expect("initialize inotify");
    let watch_descriptor = inotify
        .watches()
        .add(&file, WatchMask::DELETE_SELF | WatchMask::MODIFY)
        .expect("add native watch");
    let mut descriptors = HashMap::new();
    let pending = PendingAdd::begin(watch_descriptor, &mut descriptors);
    let activity = pending.activity().clone();

    // Exercise the exact syscall-to-commit interval: the provisional entry
    // must own terminal records before the registration can become public.
    std::fs::remove_file(&file).expect("invalidate provisional watch");
    let (event_tx, event_rx) = delivery::channel(None);
    let outcome = pending.complete(&mut inotify, &mut descriptors, &event_tx);

    assert!(matches!(outcome, PendingAddOutcome::Invalidated));
    assert!(descriptors.is_empty());
    assert!(!activity.is_active());
    assert!(event_rx.drain_consistent().records.iter().any(|record| {
        matches!(
            record,
            DeliveryRecord::Control(WorkerControl::Terminal(NativeEvent {
                activity: Some(event_activity),
                mask,
                ..
            })) if event_activity.same_registration(&activity)
                && mask.contains(EventMask::IGNORED)
        )
    }));
}

#[test]
fn shared_add_cannot_reanimate_an_epoch_that_terminates_at_its_boundary() {
    let directory = workspace_temp_dir();
    let file = directory.path().join("shared-short-lived");
    std::fs::write(&file, b"contents").expect("create watched file");
    let mut inotify = Inotify::init().expect("initialize inotify");
    let watch_descriptor = inotify
        .watches()
        .add(&file, WatchMask::DELETE_SELF | WatchMask::MODIFY)
        .expect("add initial native watch");
    let descriptor = watch_descriptor.get_watch_descriptor_id();
    let activity = WatchActivity::active();
    let mut descriptors = HashMap::from([(
        descriptor,
        NativeWatch::active(watch_descriptor, activity.clone()),
    )]);

    let shared_descriptor = inotify
        .watches()
        .add(
            &file,
            WatchMask::MASK_ADD | WatchMask::DELETE_SELF | WatchMask::ATTRIB,
        )
        .expect("extend the existing native watch");
    assert_eq!(shared_descriptor.get_watch_descriptor_id(), descriptor);
    let pending = PendingAdd::begin(shared_descriptor, &mut descriptors);
    std::fs::remove_file(&file).expect("invalidate shared watch during add");
    let (event_tx, _event_rx) = delivery::channel(None);

    assert!(matches!(
        pending.complete(&mut inotify, &mut descriptors, &event_tx),
        PendingAddOutcome::Invalidated
    ));
    assert!(descriptors.is_empty());
    assert!(!activity.is_active());
}

#[test]
fn native_queue_overflow_cannot_continue_an_ambiguous_watch_epoch() {
    let mut epoch = NativeQueueEpoch::Intact;
    epoch.observe(EventMask::Q_OVERFLOW);
    let drain = NativeDrain {
        receiver_open: true,
        epoch,
    };

    assert!(
        drain
            .require_intact_epoch()
            .expect_err("overflow must invalidate native ownership")
            .to_string()
            .contains("ownership epoch was lost")
    );
}
