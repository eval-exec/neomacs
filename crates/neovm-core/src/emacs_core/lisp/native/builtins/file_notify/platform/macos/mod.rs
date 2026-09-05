use enumflags2::BitFlags;

mod snapshot;
mod types;

use types::{KqueueAction, KqueueVnodeAction};

#[cfg(target_os = "macos")]
pub(in super::super) fn action_from_lisp_name(name: &str) -> Option<KqueueAction> {
    KqueueAction::from_lisp_name(name)
}

#[cfg(test)]
use snapshot::DirectoryEntrySnapshot;
use snapshot::{DirectoryChange, DirectorySnapshot};

/// Decode all vnode bits in GNU's observable consing order
/// (`kqueue_callback`, src/kqueue.c).  This accepts a set, rather than a
/// single event enum, so simultaneous NOTE_* flags cannot be lost.
pub(super) fn vnode_actions(flags: BitFlags<KqueueVnodeAction>) -> Vec<KqueueAction> {
    [
        (KqueueVnodeAction::Revoke, KqueueAction::Revoke),
        (KqueueVnodeAction::Rename, KqueueAction::Rename),
        (KqueueVnodeAction::Link, KqueueAction::Link),
        (KqueueVnodeAction::Attrib, KqueueAction::Attrib),
        (KqueueVnodeAction::Extend, KqueueAction::Extend),
        (KqueueVnodeAction::Write, KqueueAction::Write),
        (KqueueVnodeAction::Delete, KqueueAction::Delete),
    ]
    .into_iter()
    .filter_map(|(native, lisp)| flags.contains(native).then_some(lisp))
    .collect()
}

pub(super) fn requested_vnode_actions(
    flags: BitFlags<KqueueVnodeAction>,
    requested: BitFlags<KqueueAction>,
) -> Vec<KqueueAction> {
    vnode_actions(flags)
        .into_iter()
        .filter(|action| requested.contains(*action))
        .collect()
}

/// kqueue has no native queue-loss record. A synthetic `write` is the GNU
/// vocabulary's conservative invalidation event and deliberately bypasses a
/// watch's request filter when Neomacs' bounded delivery queue loses data.
fn overflow_recovery_action() -> KqueueAction {
    KqueueAction::Write
}

#[cfg(target_os = "macos")]
mod native {
    use super::super::super::delivery::{
        self, DeliveryReceiver, DeliveryRecord, DeliverySender, EVENT_CAPACITY, PublishOutcome,
    };
    use super::super::super::{
        DrainBatch, FileNotifyBackend, FileNotifyEvent, FileWatch, RemoveWatchOutcome,
        TrackedWatch, WatchActivity, WatchId, WatchIdAllocator, WatchRegistration,
        file_notify_error, finish_watch_drain,
    };
    use super::*;
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::process::WaitNotifier;
    use crate::emacs_core::value::Value;
    use rustix::event::kqueue::{Event, EventFilter, EventFlags, VnodeEvents, kevent, kqueue};
    use rustix::fd::{AsRawFd, OwnedFd, RawFd};
    use std::collections::HashMap;
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
    use std::thread::JoinHandle;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct KqueueRequest {
        actions: BitFlags<KqueueAction>,
    }

    impl KqueueRequest {
        pub(crate) fn new(actions: BitFlags<KqueueAction>) -> Self {
            Self { actions }
        }
    }

    #[derive(Clone, Debug)]
    enum KqueueEventName {
        Native(PathBuf),
        RegisteredWatch,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct KqueueEvent {
        watch_id: WatchId,
        actions: Vec<KqueueAction>,
        name: KqueueEventName,
        file1: Option<PathBuf>,
    }

    impl FileNotifyEvent for KqueueEvent {
        fn watch_id(&self) -> &WatchId {
            &self.watch_id
        }

        fn into_lisp(
            self,
            ctx: &crate::emacs_core::eval::Context,
            registration: WatchRegistration,
        ) -> Value {
            // GNU kqueue events use a bare-fixnum descriptor and have no
            // trailing cookie (`kqueue_generate_event`, src/kqueue.c:94-104).
            let name = match self.name {
                KqueueEventName::Native(path) => {
                    super::super::super::lisp::file_name_to_lisp(ctx, &path)
                }
                KqueueEventName::RegisteredWatch => registration.registered_file_name(),
            };
            let mut fields = vec![
                Value::fixnum(self.watch_id.slot()),
                Value::list(
                    self.actions
                        .into_iter()
                        .map(|action| Value::symbol(action.as_lisp_name()))
                        .collect(),
                ),
                name,
            ];
            if let Some(file1) = self.file1 {
                fields.push(super::super::super::lisp::file_name_to_lisp(ctx, &file1));
            }
            Value::list(fields)
        }
    }

    #[derive(Debug)]
    struct NativeEvent {
        watch_id: WatchId,
        actions: BitFlags<KqueueVnodeAction>,
    }

    enum WorkerControl {
        Terminal(NativeEvent),
        Failed(String),
    }

    struct NativeWatch {
        _fd: OwnedFd,
        watch_id: WatchId,
        activity: WatchActivity,
    }

    enum Command {
        Add {
            fd: OwnedFd,
            watch_id: WatchId,
            activity: WatchActivity,
            actions: BitFlags<KqueueVnodeAction>,
            snapshot_path: Option<PathBuf>,
            reply: SyncSender<Result<(RawFd, Option<DirectorySnapshot>), KqueueAddWatchError>>,
        },
        Remove {
            descriptor: i64,
            reply: SyncSender<bool>,
        },
        Shutdown,
    }

    #[derive(Debug)]
    enum KqueueAddWatchError {
        Worker(String),
        Register(String),
        Snapshot(String),
    }

    struct Worker {
        commands: Sender<Command>,
        command_socket: UnixDatagram,
        events: DeliveryReceiver<NativeEvent, WorkerControl>,
        join: Option<JoinHandle<()>>,
    }

    impl Worker {
        fn start(notifier: Option<WaitNotifier>) -> Result<Self, Flow> {
            let worker_kqueue = kqueue().map_err(|error| {
                file_notify_error(
                    "File watching is not available",
                    Some(error.to_string()),
                    None,
                )
            })?;
            let (worker_command_socket, command_socket) =
                UnixDatagram::pair().map_err(|error| {
                    file_notify_error(
                        "File watching is not available",
                        Some(error.to_string()),
                        None,
                    )
                })?;
            worker_command_socket
                .set_nonblocking(true)
                .map_err(|error| {
                    file_notify_error(
                        "File watching is not available",
                        Some(error.to_string()),
                        None,
                    )
                })?;
            register_command_socket(&worker_kqueue, &worker_command_socket).map_err(|error| {
                file_notify_error("File watching is not available", Some(error), None)
            })?;

            let (command_tx, command_rx) = mpsc::channel();
            let (event_tx, event_rx) = delivery::channel(notifier);
            let join = std::thread::Builder::new()
                .name("neomacs-kqueue".to_owned())
                .spawn(move || {
                    worker_loop(worker_kqueue, worker_command_socket, command_rx, event_tx)
                })
                .map_err(|error| {
                    file_notify_error(
                        "File watching is not available",
                        Some(error.to_string()),
                        None,
                    )
                })?;
            Ok(Self {
                commands: command_tx,
                command_socket,
                events: event_rx,
                join: Some(join),
            })
        }

        fn send_command(&self, command: Command) -> Result<(), String> {
            self.commands
                .send(command)
                .map_err(|_| "kqueue worker exited".to_owned())?;
            loop {
                match self.command_socket.send(&[1]) {
                    Ok(_) => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
        }

        fn add(
            &self,
            fd: OwnedFd,
            watch_id: WatchId,
            activity: WatchActivity,
            actions: BitFlags<KqueueVnodeAction>,
            snapshot_path: Option<PathBuf>,
        ) -> Result<(RawFd, Option<DirectorySnapshot>), KqueueAddWatchError> {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            self.send_command(Command::Add {
                fd,
                watch_id,
                activity,
                actions,
                snapshot_path,
                reply: reply_tx,
            })
            .map_err(KqueueAddWatchError::Worker)?;
            reply_rx.recv().map_err(|_| {
                KqueueAddWatchError::Worker("kqueue worker exited while adding a watch".to_owned())
            })?
        }

        fn remove(&self, descriptor: i64) -> Result<bool, String> {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            self.send_command(Command::Remove {
                descriptor,
                reply: reply_tx,
            })?;
            reply_rx
                .recv()
                .map_err(|_| "kqueue worker exited while removing a watch".to_owned())
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.send_command(Command::Shutdown);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn register_command_socket(
        kqueue_fd: &OwnedFd,
        command_socket: &UnixDatagram,
    ) -> Result<(), String> {
        let change = Event::new(
            EventFilter::Read(command_socket.as_raw_fd()),
            EventFlags::ADD | EventFlags::ENABLE | EventFlags::CLEAR,
            ptr::null_mut(),
        );
        let events: &mut [Event] = &mut [];
        // SAFETY: the worker owns the registered socket until its kqueue loop
        // exits, and command application never replaces that socket.
        unsafe { kevent(kqueue_fd, &[change], events, None) }
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn drain_command_socket(command_socket: &UnixDatagram) -> io::Result<()> {
        let mut buffer = [0_u8; 64];
        loop {
            match command_socket.recv(&mut buffer) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }

    fn register_vnode(
        kqueue_fd: &OwnedFd,
        fd: RawFd,
        actions: BitFlags<KqueueVnodeAction>,
    ) -> Result<(), String> {
        let flags = to_rustix_vnode_events(actions);
        let change = Event::new(
            EventFilter::Vnode { vnode: fd, flags },
            EventFlags::ADD | EventFlags::ENABLE | EventFlags::CLEAR,
            ptr::null_mut(),
        );
        let events: &mut [Event] = &mut [];
        // SAFETY: `fd' is owned by the worker's watch map from successful
        // registration until the filter is removed by closing that fd.
        unsafe { kevent(kqueue_fd, &[change], events, None) }
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn to_rustix_vnode_events(actions: BitFlags<KqueueVnodeAction>) -> VnodeEvents {
        let mut flags = VnodeEvents::empty();
        for (action, native) in [
            (KqueueVnodeAction::Delete, VnodeEvents::DELETE),
            (KqueueVnodeAction::Write, VnodeEvents::WRITE),
            (KqueueVnodeAction::Extend, VnodeEvents::EXTEND),
            (KqueueVnodeAction::Attrib, VnodeEvents::ATTRIBUTES),
            (KqueueVnodeAction::Link, VnodeEvents::LINK),
            (KqueueVnodeAction::Rename, VnodeEvents::RENAME),
            (KqueueVnodeAction::Revoke, VnodeEvents::REVOKE),
        ] {
            if actions.contains(action) {
                flags.insert(native);
            }
        }
        flags
    }

    fn from_rustix_vnode_events(flags: VnodeEvents) -> BitFlags<KqueueVnodeAction> {
        let mut actions = BitFlags::empty();
        for (native, action) in [
            (VnodeEvents::DELETE, KqueueVnodeAction::Delete),
            (VnodeEvents::WRITE, KqueueVnodeAction::Write),
            (VnodeEvents::EXTEND, KqueueVnodeAction::Extend),
            (VnodeEvents::ATTRIBUTES, KqueueVnodeAction::Attrib),
            (VnodeEvents::LINK, KqueueVnodeAction::Link),
            (VnodeEvents::RENAME, KqueueVnodeAction::Rename),
            (VnodeEvents::REVOKE, KqueueVnodeAction::Revoke),
        ] {
            if flags.contains(native) {
                actions.insert(action);
            }
        }
        actions
    }

    fn worker_loop(
        kqueue_fd: OwnedFd,
        command_socket: UnixDatagram,
        commands: Receiver<Command>,
        events: DeliverySender<NativeEvent, WorkerControl>,
    ) {
        let mut watches = HashMap::<RawFd, NativeWatch>::new();
        loop {
            let mut ready = Vec::<Event>::with_capacity(32);
            // SAFETY: every vnode fd registered in this kqueue is owned by
            // `watches' for the full wait. Commands are applied only after
            // kevent returns, so no descriptor can be dropped concurrently.
            let wait_result = unsafe {
                kevent(
                    &kqueue_fd,
                    &[],
                    rustix::buffer::spare_capacity(&mut ready),
                    None,
                )
            };
            if let Err(error) = wait_result {
                events.finish_with(WorkerControl::Failed(error.to_string()), || {
                    for watch in watches.values() {
                        watch.activity.terminate();
                    }
                });
                return;
            }

            // Resolve every ready vnode against the map that existed when
            // kevent returned. Commands can close and reuse descriptor
            // integers, so applying them first would let an old readiness
            // record alias a newly added watch.
            for event in &ready {
                let EventFilter::Vnode { vnode, flags } = event.filter() else {
                    continue;
                };
                let Some(watch) = watches.get(&vnode) else {
                    continue;
                };
                let watch_id = watch.watch_id.clone();
                let actions = from_rustix_vnode_events(flags);
                if actions.is_empty() {
                    continue;
                }
                let terminal = actions.intersects(
                    KqueueVnodeAction::Delete
                        | KqueueVnodeAction::Rename
                        | KqueueVnodeAction::Revoke,
                );
                let outcome = if terminal {
                    let watch = watches
                        .remove(&vnode)
                        .expect("ready vnode remained registered");
                    events.publish_control(
                        WorkerControl::Terminal(NativeEvent { watch_id, actions }),
                        move || watch.activity.terminate(),
                    )
                } else {
                    events.publish(NativeEvent { watch_id, actions })
                };
                if outcome == PublishOutcome::Closed {
                    return;
                }
            }

            // The socket is the sleeping wake mechanism, not a scheduling
            // gate: a capacity-limited kevent batch can omit its readiness
            // while vnode activity remains hot. Always poll commands after
            // resolving the complete pre-command vnode batch.
            if let Err(error) = drain_command_socket(&command_socket) {
                events.finish_with(WorkerControl::Failed(error.to_string()), || {
                    for watch in watches.values() {
                        watch.activity.terminate();
                    }
                });
                return;
            }
            loop {
                match commands.try_recv() {
                    Ok(Command::Add {
                        fd,
                        watch_id,
                        activity,
                        actions,
                        snapshot_path,
                        reply,
                    }) => {
                        let raw_fd = fd.as_raw_fd();
                        let result = register_vnode(&kqueue_fd, raw_fd, actions)
                            .map_err(KqueueAddWatchError::Register)
                            .and_then(|()| {
                                snapshot_path
                                    .as_deref()
                                    .map(DirectorySnapshot::read)
                                    .transpose()
                                    .map_err(|error| {
                                        KqueueAddWatchError::Snapshot(error.to_string())
                                    })
                            });
                        match result {
                            Ok(directory) => {
                                watches.insert(
                                    raw_fd,
                                    NativeWatch {
                                        _fd: fd,
                                        watch_id,
                                        activity,
                                    },
                                );
                                if reply.send(Ok((raw_fd, directory))).is_err()
                                    && let Some(watch) = watches.remove(&raw_fd)
                                {
                                    // The evaluator did not observe a successful add.
                                    // Closing the owned fd rolls the kqueue filter back.
                                    watch.activity.terminate();
                                }
                            }
                            Err(error) => {
                                // `fd` remains local and closes here, so a snapshot
                                // failure also rolls back the registered vnode filter.
                                let _ = reply.send(Err(error));
                            }
                        }
                    }
                    Ok(Command::Remove { descriptor, reply }) => {
                        let removed = i32::try_from(descriptor)
                            .ok()
                            .and_then(|fd| watches.remove(&fd));
                        if let Some(watch) = removed.as_ref() {
                            watch.activity.terminate();
                        }
                        let removed = removed.is_some();
                        let _ = reply.send(removed);
                    }
                    Ok(Command::Shutdown) => return,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        }
    }

    struct KqueueWatch {
        common: FileWatch<KqueueRequest>,
        path: PathBuf,
        native_descriptor: i64,
        activity: WatchActivity,
        directory: Option<DirectorySnapshot>,
    }

    impl TrackedWatch for KqueueWatch {
        fn watch_id(&self) -> &WatchId {
            &self.common.id
        }
    }

    impl KqueueWatch {
        fn translate(
            &mut self,
            mut native_actions: BitFlags<KqueueVnodeAction>,
        ) -> Result<Vec<KqueueEvent>, Flow> {
            let requested = self.common.request.actions;
            let watch_id = self.common.id.clone();
            let mut translated = Vec::new();

            if native_actions.contains(KqueueVnodeAction::Write)
                && let Some(old_snapshot) = self.directory.as_ref()
            {
                native_actions.remove(KqueueVnodeAction::Write);
                if self.path.is_dir() {
                    let new_snapshot = DirectorySnapshot::read(&self.path).map_err(|error| {
                        file_notify_error(
                            "Error while reading watched directory",
                            Some(error.to_string()),
                            Some(Value::string(self.path.display().to_string())),
                        )
                    })?;
                    for change in old_snapshot.diff(&new_snapshot) {
                        let (action, path, file1) = match change {
                            DirectoryChange::Action { action, path } => (action, path, None),
                            DirectoryChange::Rename { from, to } => {
                                (KqueueAction::Rename, from, Some(to))
                            }
                        };
                        if requested.contains(action) {
                            translated.push(KqueueEvent {
                                watch_id: watch_id.clone(),
                                actions: vec![action],
                                name: KqueueEventName::Native(path),
                                file1,
                            });
                        }
                    }
                    self.directory = Some(new_snapshot);
                } else if requested.contains(KqueueAction::Delete) {
                    translated.push(KqueueEvent {
                        watch_id: watch_id.clone(),
                        actions: vec![KqueueAction::Delete],
                        name: KqueueEventName::RegisteredWatch,
                        file1: None,
                    });
                }
            }

            let actions = requested_vnode_actions(native_actions, requested);
            if !actions.is_empty() {
                translated.push(KqueueEvent {
                    watch_id,
                    actions,
                    name: KqueueEventName::RegisteredWatch,
                    file1: None,
                });
            }
            Ok(translated)
        }

        fn recover_from_overflow(&mut self) -> KqueueEvent {
            if self.directory.is_some() && self.path.is_dir() {
                match DirectorySnapshot::read(&self.path) {
                    Ok(snapshot) => self.directory = Some(snapshot),
                    Err(error) => tracing::warn!(
                        path = %self.path.display(),
                        %error,
                        "could not refresh kqueue snapshot after delivery overflow"
                    ),
                }
            }
            KqueueEvent {
                watch_id: self.common.id.clone(),
                actions: vec![overflow_recovery_action()],
                name: KqueueEventName::RegisteredWatch,
                file1: None,
            }
        }
    }

    #[derive(Default)]
    pub(crate) struct KqueueBackend {
        worker: Option<Worker>,
        watches: Vec<KqueueWatch>,
        ids: WatchIdAllocator,
    }

    impl KqueueBackend {
        fn ensure_worker(&mut self, notifier: Option<WaitNotifier>) -> Result<&mut Worker, Flow> {
            if self.worker.is_none() {
                self.worker = Some(Worker::start(notifier)?);
            }
            Ok(self.worker.as_mut().expect("worker was initialized"))
        }

        fn requested_native_actions(
            actions: BitFlags<KqueueAction>,
        ) -> BitFlags<KqueueVnodeAction> {
            let mut native = BitFlags::empty();
            for (lisp, vnode) in [
                (KqueueAction::Delete, KqueueVnodeAction::Delete),
                (KqueueAction::Write, KqueueVnodeAction::Write),
                (KqueueAction::Extend, KqueueVnodeAction::Extend),
                (KqueueAction::Attrib, KqueueVnodeAction::Attrib),
                (KqueueAction::Link, KqueueVnodeAction::Link),
                (KqueueAction::Rename, KqueueVnodeAction::Rename),
                (KqueueAction::Revoke, KqueueVnodeAction::Revoke),
            ] {
                if actions.contains(lisp) {
                    native.insert(vnode);
                }
            }
            native
        }

        fn open_watch(path: &Path) -> Result<OwnedFd, Flow> {
            use rustix::fs::{Mode, OFlags};

            let flags = OFlags::from_bits_retain((libc::O_EVTONLY | libc::O_SYMLINK) as u32)
                | OFlags::NONBLOCK;
            rustix::fs::open(path, flags, Mode::empty()).map_err(|error| {
                file_notify_error(
                    "File cannot be opened",
                    Some(error.to_string()),
                    Some(Value::string(path.display().to_string())),
                )
            })
        }
    }

    impl FileNotifyBackend for KqueueBackend {
        type Request = KqueueRequest;
        type Event = KqueueEvent;

        fn add_watch(
            &mut self,
            path: &Path,
            request: Self::Request,
            notifier: Option<WaitNotifier>,
        ) -> Result<WatchId, Flow> {
            let actions = request.actions;
            let is_directory = path.is_dir();
            let fd = Self::open_watch(path)?;
            let watch_id = self.ids.allocate();
            let activity = WatchActivity::active();
            let add_result = self.ensure_worker(notifier)?.add(
                fd,
                watch_id.clone(),
                activity.clone(),
                Self::requested_native_actions(actions),
                is_directory.then(|| path.to_path_buf()),
            );
            let (native_descriptor, directory) = match add_result {
                Ok(registered) => registered,
                Err(error) => {
                    if self.watches.is_empty() {
                        self.worker = None;
                    }
                    return Err(match error {
                        KqueueAddWatchError::Worker(detail)
                        | KqueueAddWatchError::Register(detail) => file_notify_error(
                            "Cannot watch file",
                            Some(detail),
                            Some(Value::string(path.display().to_string())),
                        ),
                        KqueueAddWatchError::Snapshot(detail) => file_notify_error(
                            "Cannot read watched directory",
                            Some(detail),
                            Some(Value::string(path.display().to_string())),
                        ),
                    });
                }
            };
            self.watches.push(KqueueWatch {
                common: FileWatch {
                    id: watch_id.clone(),
                    request,
                },
                path: path.to_path_buf(),
                native_descriptor: i64::from(native_descriptor),
                activity,
                directory,
            });
            Ok(watch_id)
        }

        fn remove_watch(&mut self, descriptor: &WatchId) -> RemoveWatchOutcome {
            let Some(index) = self
                .watches
                .iter()
                .position(|watch| watch.common.id == *descriptor)
            else {
                return RemoveWatchOutcome::NotFound;
            };
            let native_descriptor = self.watches[index].native_descriptor;
            let native_result = self
                .worker
                .as_ref()
                .expect("a live watch has a worker")
                .remove(native_descriptor);
            self.watches.remove(index);
            if self.watches.is_empty() {
                self.worker = None;
            }
            // The descriptor's presence in `self.watches' is authoritative.
            // A terminal NOTE_* may already have made the worker close its fd
            // before the evaluator drains that event; GNU still considers an
            // explicit removal successful while its watch object is present.
            match native_result {
                Ok(_) => RemoveWatchOutcome::Removed,
                Err(error) => RemoveWatchOutcome::RemovedWithError(file_notify_error(
                    "Cannot remove watch",
                    Some(error),
                    None,
                )),
            }
        }

        fn valid_p(&self, descriptor: &WatchId) -> bool {
            self.watches
                .iter()
                .any(|watch| watch.common.id == *descriptor && watch.activity.is_active())
        }

        fn drain_events(&mut self) -> Result<DrainBatch<Self::Event>, Flow> {
            let mut raw_events = Vec::new();
            let mut overflowed = false;
            let mut failures = Vec::new();
            let mut terminated = Vec::new();
            if let Some(worker) = self.worker.as_ref() {
                let delivery = worker.events.drain_consistent();
                overflowed = delivery.overflowed;
                for record in delivery.records {
                    match record {
                        DeliveryRecord::Event(event) => raw_events.push(event),
                        DeliveryRecord::Control(control) => match control {
                            WorkerControl::Terminal(event) => {
                                terminated.push(event.watch_id.clone());
                                raw_events.push(event);
                            }
                            WorkerControl::Failed(error) => {
                                failures.push(error);
                                terminated.extend(
                                    self.watches.iter().map(|watch| watch.common.id.clone()),
                                );
                            }
                        },
                    }
                }
            }

            let mut failure = (!failures.is_empty()).then(|| {
                file_notify_error(
                    "Error while retrieving file system events",
                    Some(failures.join("\n")),
                    None,
                )
            });
            let mut translated = Vec::new();
            for event in raw_events {
                let Some(index) = self
                    .watches
                    .iter()
                    .position(|watch| watch.common.id == event.watch_id)
                else {
                    continue;
                };
                match self.watches[index].translate(event.actions) {
                    Ok(events) => translated.extend(events),
                    Err(error) if failure.is_none() => failure = Some(error),
                    Err(_) => {}
                }
            }
            terminated.sort_by_key(WatchId::slot);
            terminated.dedup();
            finish_watch_drain(&mut self.watches, &terminated, |watches| {
                if overflowed {
                    tracing::warn!(
                        capacity = EVENT_CAPACITY,
                        "kqueue delivery queue overflowed; requesting conservative rescans"
                    );
                    for watch in watches {
                        translated.push(watch.recover_from_overflow());
                    }
                }
            });
            if self.watches.is_empty() {
                self.worker = None;
            }
            Ok(DrainBatch {
                events: translated,
                terminated,
                failure,
            })
        }

        fn has_watches(&self) -> bool {
            !self.watches.is_empty()
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) use native::{KqueueBackend, KqueueRequest};

#[cfg(target_os = "macos")]
mod lisp;
#[cfg(target_os = "macos")]
pub(crate) use lisp::{kqueue_add_watch, kqueue_rm_watch, kqueue_valid_p};

#[cfg(test)]
#[path = "tests/mod.rs"]
mod macos_test;
