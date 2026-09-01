use super::{KqueueAction, KqueueVnodeAction};
use enumflags2::BitFlags;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DirectoryEntrySnapshot {
    pub(super) inode: u64,
    pub(super) name: PathBuf,
    pub(super) modified: (i64, i64),
    pub(super) changed: (i64, i64),
    pub(super) size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DirectorySnapshot {
    entries: Vec<DirectoryEntrySnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DirectoryChange {
    Action { action: KqueueAction, path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
}

impl DirectorySnapshot {
    #[cfg(test)]
    pub(super) fn from_entries(entries: Vec<DirectoryEntrySnapshot>) -> Self {
        Self { entries }
    }

    #[cfg(target_os = "macos")]
    fn read(directory: &Path) -> std::io::Result<Self> {
        use std::os::unix::fs::MetadataExt;

        let mut entries = Vec::new();
        for result in std::fs::read_dir(directory)? {
            let entry = result?;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            entries.push(DirectoryEntrySnapshot {
                inode: metadata.ino(),
                name: PathBuf::from(entry.file_name()),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
                size: metadata.size(),
            });
        }
        Ok(Self { entries })
    }

    /// Reproduce GNU `kqueue_compare_dir_list' as a pure transition.  Keeping
    /// the old and new snapshots explicit makes rename pairing, replacement,
    /// and metadata classification independently testable from kqueue I/O.
    pub(super) fn diff(&self, new: &Self) -> Vec<DirectoryChange> {
        let mut available_new = new.entries.clone();
        let mut pending = Vec::<DirectoryEntrySnapshot>::new();
        let mut renamed_destinations = Vec::<DirectoryEntrySnapshot>::new();
        let mut changes = Vec::new();

        for old_entry in &self.entries {
            if let Some(index) = available_new
                .iter()
                .position(|new_entry| new_entry.inode == old_entry.inode)
            {
                let new_entry = available_new.remove(index);
                if *old_entry == new_entry {
                    continue;
                }
                if old_entry.name == new_entry.name {
                    if old_entry.modified != new_entry.modified {
                        changes.push(DirectoryChange::Action {
                            action: KqueueAction::Write,
                            path: old_entry.name.clone(),
                        });
                    }
                    if old_entry.changed != new_entry.changed {
                        changes.push(DirectoryChange::Action {
                            action: KqueueAction::Attrib,
                            path: old_entry.name.clone(),
                        });
                    }
                } else {
                    changes.push(DirectoryChange::Rename {
                        from: old_entry.name.clone(),
                        to: new_entry.name.clone(),
                    });
                    renamed_destinations.push(new_entry);
                }
                continue;
            }

            if let Some(index) = available_new
                .iter()
                .position(|new_entry| new_entry.name == old_entry.name)
            {
                pending.push(available_new.remove(index));
                continue;
            }

            if let Some(index) = pending
                .iter()
                .position(|new_entry| new_entry.inode == old_entry.inode)
            {
                let new_entry = pending.remove(index);
                changes.push(DirectoryChange::Rename {
                    from: old_entry.name.clone(),
                    to: new_entry.name,
                });
                continue;
            }

            if let Some(index) = renamed_destinations
                .iter()
                .position(|new_entry| new_entry.name == old_entry.name)
            {
                renamed_destinations.remove(index);
                continue;
            }

            changes.push(DirectoryChange::Action {
                action: KqueueAction::Delete,
                path: old_entry.name.clone(),
            });
        }

        for entry in available_new {
            changes.push(DirectoryChange::Action {
                action: KqueueAction::Create,
                path: entry.name.clone(),
            });
            if entry.size > 0 {
                changes.push(DirectoryChange::Action {
                    action: KqueueAction::Write,
                    path: entry.name,
                });
            }
        }
        for entry in pending {
            changes.push(DirectoryChange::Action {
                action: KqueueAction::Write,
                path: entry.name,
            });
        }

        changes
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::super::{
        FileNotifyBackend, FileNotifyEvent, FileNotifyWatchDescriptor, FileWatch, WatchDialect,
        WatchRequest, file_notify_error,
    };
    use super::*;
    use crate::emacs_core::error::Flow;
    use crate::emacs_core::process::WaitNotifier;
    use crate::emacs_core::value::Value;
    use rustix::event::kqueue::{
        Event, EventFilter, EventFlags, UserDefinedFlags, UserFlags, VnodeEvents, kevent, kqueue,
    };
    use rustix::fd::{AsRawFd, OwnedFd, RawFd};
    use std::collections::HashMap;
    use std::ptr;
    use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
    use std::thread::JoinHandle;

    const COMMAND_EVENT_IDENT: isize = 1;

    #[derive(Debug)]
    struct NativeEvent {
        descriptor: i64,
        actions: BitFlags<KqueueVnodeAction>,
    }

    enum Command {
        Add {
            fd: OwnedFd,
            actions: BitFlags<KqueueVnodeAction>,
            reply: SyncSender<Result<RawFd, String>>,
        },
        Remove {
            descriptor: i64,
            reply: SyncSender<bool>,
        },
        Shutdown,
    }

    struct Worker {
        commands: Sender<Command>,
        control_kqueue: OwnedFd,
        events: Receiver<Result<NativeEvent, String>>,
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
            let control_kqueue = rustix::io::dup(&worker_kqueue).map_err(|error| {
                file_notify_error(
                    "File watching is not available",
                    Some(error.to_string()),
                    None,
                )
            })?;
            register_command_event(&worker_kqueue).map_err(|error| {
                file_notify_error("File watching is not available", Some(error), None)
            })?;

            let (command_tx, command_rx) = mpsc::channel();
            let (event_tx, event_rx) = mpsc::channel();
            let join = std::thread::Builder::new()
                .name("neomacs-kqueue".to_owned())
                .spawn(move || worker_loop(worker_kqueue, command_rx, event_tx, notifier))
                .map_err(|error| {
                    file_notify_error(
                        "File watching is not available",
                        Some(error.to_string()),
                        None,
                    )
                })?;
            Ok(Self {
                commands: command_tx,
                control_kqueue,
                events: event_rx,
                join: Some(join),
            })
        }

        fn send_command(&self, command: Command) -> Result<(), String> {
            self.commands
                .send(command)
                .map_err(|_| "kqueue worker exited".to_owned())?;
            trigger_command_event(&self.control_kqueue)
        }

        fn add(&self, fd: OwnedFd, actions: BitFlags<KqueueVnodeAction>) -> Result<RawFd, String> {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            self.send_command(Command::Add {
                fd,
                actions,
                reply: reply_tx,
            })?;
            reply_rx
                .recv()
                .map_err(|_| "kqueue worker exited while adding a watch".to_owned())?
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

    fn register_command_event(kqueue_fd: &OwnedFd) -> Result<(), String> {
        let change = Event::new(
            EventFilter::User {
                ident: COMMAND_EVENT_IDENT,
                flags: UserFlags::NOINPUT,
                user_flags: UserDefinedFlags::new(0),
            },
            EventFlags::ADD | EventFlags::CLEAR,
            ptr::null_mut(),
        );
        let events: &mut [Event] = &mut [];
        // SAFETY: the user filter contains no borrowed descriptor; kqueue_fd
        // is owned for this call and remains owned by the worker afterwards.
        unsafe { kevent(kqueue_fd, &[change], events, None) }
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn trigger_command_event(kqueue_fd: &OwnedFd) -> Result<(), String> {
        let change = Event::new(
            EventFilter::User {
                ident: COMMAND_EVENT_IDENT,
                flags: UserFlags::TRIGGER,
                user_flags: UserDefinedFlags::new(0),
            },
            EventFlags::empty(),
            ptr::null_mut(),
        );
        let events: &mut [Event] = &mut [];
        // SAFETY: this only triggers the previously registered user filter;
        // it refers to no vnode descriptor.
        unsafe { kevent(kqueue_fd, &[change], events, None) }
            .map(|_| ())
            .map_err(|error| error.to_string())
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
        commands: Receiver<Command>,
        events: Sender<Result<NativeEvent, String>>,
        notifier: Option<WaitNotifier>,
    ) {
        let mut watches = HashMap::<RawFd, OwnedFd>::new();
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
                let _ = events.send(Err(error.to_string()));
                notify_evaluator(notifier.as_ref());
                return;
            }

            let command_ready = ready.iter().any(|event| {
                matches!(
                    event.filter(),
                    EventFilter::User {
                        ident: COMMAND_EVENT_IDENT,
                        ..
                    }
                )
            });
            if command_ready {
                loop {
                    match commands.try_recv() {
                        Ok(Command::Add { fd, actions, reply }) => {
                            let raw_fd = fd.as_raw_fd();
                            let result = register_vnode(&kqueue_fd, raw_fd, actions).map(|()| {
                                watches.insert(raw_fd, fd);
                                raw_fd
                            });
                            let _ = reply.send(result);
                        }
                        Ok(Command::Remove { descriptor, reply }) => {
                            let removed = i32::try_from(descriptor)
                                .ok()
                                .and_then(|fd| watches.remove(&fd))
                                .is_some();
                            let _ = reply.send(removed);
                        }
                        Ok(Command::Shutdown) => return,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
            }

            for event in ready {
                let EventFilter::Vnode { vnode, flags } = event.filter() else {
                    continue;
                };
                if !watches.contains_key(&vnode) {
                    continue;
                }
                let actions = from_rustix_vnode_events(flags);
                if actions.is_empty() {
                    continue;
                }
                let terminal = actions.intersects(
                    KqueueVnodeAction::Delete
                        | KqueueVnodeAction::Rename
                        | KqueueVnodeAction::Revoke,
                );
                let published = events.send(Ok(NativeEvent {
                    descriptor: i64::from(vnode),
                    actions,
                }));
                if terminal {
                    watches.remove(&vnode);
                }
                if published.is_err() {
                    return;
                }
                notify_evaluator(notifier.as_ref());
            }
        }
    }

    fn notify_evaluator(notifier: Option<&WaitNotifier>) {
        if let Some(notifier) = notifier
            && let Err(error) = notifier.notify()
        {
            tracing::error!(%error, "failed to wake evaluator for kqueue notification");
        }
    }

    struct KqueueWatch {
        common: FileWatch,
        directory: Option<DirectorySnapshot>,
    }

    #[derive(Default)]
    pub(crate) struct KqueueBackend {
        worker: Option<Worker>,
        watches: Vec<KqueueWatch>,
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

            let flags = OFlags::from_bits_retain(libc::O_EVTONLY as u32)
                | OFlags::NONBLOCK
                | OFlags::NOFOLLOW;
            rustix::fs::open(path, flags, Mode::empty()).map_err(|error| {
                file_notify_error(
                    "File cannot be opened",
                    Some(error.to_string()),
                    Some(Value::string(path.display().to_string())),
                )
            })
        }

        fn translate_event(
            watch: &mut KqueueWatch,
            mut native_actions: BitFlags<KqueueVnodeAction>,
        ) -> Result<Vec<FileNotifyEvent>, Flow> {
            let WatchRequest::Kqueue { actions: requested } = &watch.common.request else {
                unreachable!("the macOS backend only stores kqueue watches")
            };
            let descriptor = FileNotifyWatchDescriptor::new(watch.common.id, 0);
            let mut translated = Vec::new();

            if native_actions.contains(KqueueVnodeAction::Write)
                && let Some(old_snapshot) = watch.directory.as_ref()
            {
                native_actions.remove(KqueueVnodeAction::Write);
                if watch.common.path.is_dir() {
                    let new_snapshot =
                        DirectorySnapshot::read(&watch.common.path).map_err(|error| {
                            file_notify_error(
                                "Error while reading watched directory",
                                Some(error.to_string()),
                                Some(Value::string(watch.common.path.display().to_string())),
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
                            translated.push(FileNotifyEvent::Kqueue {
                                descriptor: descriptor.clone(),
                                actions: vec![action],
                                path,
                                callback: watch.common.callback,
                                file1,
                            });
                        }
                    }
                    watch.directory = Some(new_snapshot);
                } else if requested.contains(KqueueAction::Delete) {
                    translated.push(FileNotifyEvent::Kqueue {
                        descriptor: descriptor.clone(),
                        actions: vec![KqueueAction::Delete],
                        path: watch.common.path.clone(),
                        callback: watch.common.callback,
                        file1: None,
                    });
                }
            }

            let actions = requested_vnode_actions(native_actions, *requested);
            if !actions.is_empty() {
                translated.push(FileNotifyEvent::Kqueue {
                    descriptor,
                    actions,
                    path: watch.common.path.clone(),
                    callback: watch.common.callback,
                    file1: None,
                });
            }
            Ok(translated)
        }
    }

    impl FileNotifyBackend for KqueueBackend {
        fn allocated_p(&self) -> bool {
            self.worker.is_some()
        }

        fn watch_list(&self) -> Vec<FileWatch> {
            self.watches
                .iter()
                .map(|watch| watch.common.clone())
                .collect()
        }

        fn add_watch(
            &mut self,
            path: &Path,
            request: WatchRequest,
            callback: Value,
            notifier: Option<WaitNotifier>,
        ) -> Result<FileNotifyWatchDescriptor, Flow> {
            let WatchRequest::Kqueue { actions } = request else {
                return Err(file_notify_error(
                    "Wrong file notification backend",
                    Some("inotify watch requested from kqueue".to_owned()),
                    None,
                ));
            };
            let is_directory = path.is_dir();
            let fd = Self::open_watch(path)?;
            let descriptor = self
                .ensure_worker(notifier)?
                .add(fd, Self::requested_native_actions(actions))
                .map_err(|error| {
                    file_notify_error(
                        "Cannot watch file",
                        Some(error),
                        Some(Value::string(path.display().to_string())),
                    )
                })?;
            let directory = if is_directory {
                match DirectorySnapshot::read(path) {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        let _ = self
                            .worker
                            .as_ref()
                            .expect("worker exists")
                            .remove(i64::from(descriptor));
                        return Err(file_notify_error(
                            "Cannot read watched directory",
                            Some(error.to_string()),
                            Some(Value::string(path.display().to_string())),
                        ));
                    }
                }
            } else {
                None
            };
            let descriptor = FileNotifyWatchDescriptor::new(i64::from(descriptor), 0);
            self.watches.push(KqueueWatch {
                common: FileWatch {
                    id: descriptor.id(),
                    generation: 0,
                    path: path.to_path_buf(),
                    is_directory,
                    callback,
                    request: WatchRequest::Kqueue { actions },
                },
                directory,
            });
            Ok(descriptor)
        }

        fn remove_watch(
            &mut self,
            descriptor: &FileNotifyWatchDescriptor,
            dialect: WatchDialect,
        ) -> Result<bool, Flow> {
            if dialect != WatchDialect::Kqueue {
                return Ok(false);
            }
            let Some(index) = self.watches.iter().position(|watch| {
                watch.common.id == descriptor.id()
                    && watch.common.generation == descriptor.generation()
            }) else {
                return Ok(false);
            };
            self.watches.remove(index);
            let _worker_had_watch = self
                .worker
                .as_ref()
                .expect("a live watch has a worker")
                .remove(descriptor.id())
                .map_err(|error| file_notify_error("Cannot remove watch", Some(error), None))?;
            if self.watches.is_empty() {
                self.worker = None;
            }
            // The descriptor's presence in `self.watches' is authoritative.
            // A terminal NOTE_* may already have made the worker close its fd
            // before the evaluator drains that event; GNU still considers an
            // explicit removal successful while its watch object is present.
            Ok(true)
        }

        fn valid_p(&self, descriptor: &FileNotifyWatchDescriptor, dialect: WatchDialect) -> bool {
            dialect == WatchDialect::Kqueue
                && self.watches.iter().any(|watch| {
                    watch.common.id == descriptor.id()
                        && watch.common.generation == descriptor.generation()
                })
        }

        fn drain_events(&mut self) -> Result<Vec<FileNotifyEvent>, Flow> {
            let mut raw_events = Vec::new();
            if let Some(worker) = self.worker.as_ref() {
                loop {
                    match worker.events.try_recv() {
                        Ok(Ok(event)) => raw_events.push(event),
                        Ok(Err(error)) => {
                            return Err(file_notify_error(
                                "Error while retrieving file system events",
                                Some(error),
                                None,
                            ));
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                    }
                }
            }

            let mut translated = Vec::new();
            for event in raw_events {
                let Some(index) = self
                    .watches
                    .iter()
                    .position(|watch| watch.common.id == event.descriptor)
                else {
                    continue;
                };
                let terminal = event.actions.intersects(
                    KqueueVnodeAction::Delete
                        | KqueueVnodeAction::Rename
                        | KqueueVnodeAction::Revoke,
                );
                translated.extend(Self::translate_event(
                    &mut self.watches[index],
                    event.actions,
                )?);
                if terminal {
                    self.watches.remove(index);
                }
            }
            if self.watches.is_empty() {
                self.worker = None;
            }
            Ok(translated)
        }

        fn has_watches(&self) -> bool {
            !self.watches.is_empty()
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) use native::KqueueBackend;
