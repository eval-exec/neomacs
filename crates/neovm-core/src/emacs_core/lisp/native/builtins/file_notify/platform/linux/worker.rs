//! Inotify ownership and blocking I/O.
//!
//! The worker is the only owner of the inotify instance and its native watch
//! descriptors.  The evaluator sends typed commands and receives owned event
//! records; neither side shares kernel handles or Lisp values.

use super::super::super::WatchActivity;
use super::super::super::delivery::{
    self, DeliveryBatch, DeliveryReceiver, DeliverySender, PublishOutcome,
};
use crate::emacs_core::process::WaitNotifier;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};
use polling::{Event, Events, PollMode, Poller};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

const INOTIFY_KEY: usize = 1;

#[cfg(test)]
#[path = "tests/worker.rs"]
mod worker_test;

#[cfg(test)]
use super::super::super::delivery::DeliveryRecord;

#[derive(Clone, Debug)]
pub(super) struct NativeEvent {
    pub(super) descriptor: i32,
    pub(super) activity: Option<WatchActivity>,
    pub(super) mask: EventMask,
    pub(super) cookie: u32,
    pub(super) name: Option<OsString>,
}

#[derive(Clone)]
struct NativeWatch {
    descriptor: WatchDescriptor,
    activity: WatchActivity,
    phase: NativeWatchPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeWatchPhase {
    Provisional,
    Active,
}

impl NativeWatch {
    fn provisional(descriptor: WatchDescriptor, activity: WatchActivity) -> Self {
        Self {
            descriptor,
            activity,
            phase: NativeWatchPhase::Provisional,
        }
    }

    #[cfg(test)]
    fn active(descriptor: WatchDescriptor, activity: WatchActivity) -> Self {
        Self {
            descriptor,
            activity,
            phase: NativeWatchPhase::Active,
        }
    }

    fn is_registration(&self, activity: &WatchActivity) -> bool {
        self.activity.same_registration(activity)
    }
}

pub(super) enum WorkerControl {
    Terminal(NativeEvent),
    Failed(WorkerFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WorkerFailure {
    Native(String),
    QueueEpochLost,
}

impl std::fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native(error) => formatter.write_str(error),
            Self::QueueEpochLost => formatter
                .write_str("inotify queue overflowed; native watch ownership epoch was lost"),
        }
    }
}

enum Command {
    Add {
        path: PathBuf,
        mask: WatchMask,
        reply: Sender<Result<(i32, WatchActivity), String>>,
    },
    Remove {
        descriptor: i32,
        reply: Sender<Result<bool, String>>,
    },
    Shutdown,
}

pub(super) struct Worker {
    commands: Sender<Command>,
    poller: Arc<Poller>,
    events: DeliveryReceiver<NativeEvent, WorkerControl>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub(super) fn start(notifier: Option<WaitNotifier>) -> Result<Self, String> {
        let inotify = Inotify::init().map_err(|error| error.to_string())?;
        let poller = Arc::new(Poller::new().map_err(|error| error.to_string())?);
        // SAFETY: the worker owns `inotify` until it deletes the registration
        // after its wait loop.  No command can drop or replace that value.
        unsafe { poller.add_with_mode(&inotify, Event::readable(INOTIFY_KEY), PollMode::Level) }
            .map_err(|error| error.to_string())?;

        let (command_tx, command_rx) = crossbeam_channel::bounded(64);
        let (event_tx, event_rx) = delivery::channel(notifier);
        let worker_poller = Arc::clone(&poller);
        let join = std::thread::Builder::new()
            .name("neomacs-inotify".to_owned())
            .spawn(move || worker_loop(inotify, worker_poller, command_rx, event_tx))
            .map_err(|error| error.to_string())?;

        Ok(Self {
            commands: command_tx,
            poller,
            events: event_rx,
            join: Some(join),
        })
    }

    fn send_command(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "inotify worker exited".to_owned())?;
        self.poller.notify().map_err(|error| error.to_string())
    }

    pub(super) fn add(
        &self,
        path: PathBuf,
        mask: WatchMask,
    ) -> Result<(i32, WatchActivity), String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.send_command(Command::Add {
            path,
            mask,
            reply: reply_tx,
        })?;
        reply_rx
            .recv()
            .map_err(|_| "inotify worker exited while adding a watch".to_owned())?
    }

    pub(super) fn remove(&self, descriptor: i32) -> Result<bool, String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.send_command(Command::Remove {
            descriptor,
            reply: reply_tx,
        })?;
        reply_rx
            .recv()
            .map_err(|_| "inotify worker exited while removing a watch".to_owned())?
    }

    pub(super) fn drain(&self) -> DeliveryBatch<NativeEvent, WorkerControl> {
        self.events.drain_consistent()
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

fn worker_loop(
    mut inotify: Inotify,
    poller: Arc<Poller>,
    commands: Receiver<Command>,
    events: DeliverySender<NativeEvent, WorkerControl>,
) {
    let mut descriptors = HashMap::<i32, NativeWatch>::new();
    let mut poll_events = Events::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match drain_native_events(&mut inotify, &mut descriptors, &mut buffer, &events)
            .map_err(WorkerFailure::Native)
            .and_then(NativeDrain::require_intact_epoch)
        {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                let _ = poller.delete(&inotify);
                events.finish_with(WorkerControl::Failed(error), || terminate_all(&descriptors));
                return;
            }
        }

        match apply_next_command(&mut inotify, &commands, &mut descriptors, &events) {
            CommandOutcome::Applied => continue,
            CommandOutcome::Stop => break,
            CommandOutcome::Failed(error) => {
                let _ = poller.delete(&inotify);
                events.finish_with(WorkerControl::Failed(error), || terminate_all(&descriptors));
                return;
            }
            CommandOutcome::Idle => {}
        }

        poll_events.clear();
        if let Err(error) = poller.wait(&mut poll_events, None) {
            let _ = poller.delete(&inotify);
            events.finish_with(
                WorkerControl::Failed(WorkerFailure::Native(error.to_string())),
                || terminate_all(&descriptors),
            );
            return;
        }
    }

    let _ = poller.delete(&inotify);
}

struct NativeDrain {
    receiver_open: bool,
    epoch: NativeQueueEpoch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeQueueEpoch {
    Intact,
    Lost,
}

impl NativeQueueEpoch {
    fn observe(&mut self, mask: EventMask) {
        if mask.contains(EventMask::Q_OVERFLOW) {
            *self = Self::Lost;
        }
    }
}

impl NativeDrain {
    fn require_intact_epoch(self) -> Result<bool, WorkerFailure> {
        match (self.receiver_open, self.epoch) {
            (false, _) => Ok(false),
            (true, NativeQueueEpoch::Intact) => Ok(true),
            (true, NativeQueueEpoch::Lost) => Err(WorkerFailure::QueueEpochLost),
        }
    }
}

fn drain_native_events(
    inotify: &mut Inotify,
    descriptors: &mut HashMap<i32, NativeWatch>,
    buffer: &mut [u8],
    events: &DeliverySender<NativeEvent, WorkerControl>,
) -> Result<NativeDrain, String> {
    match inotify.read_events(buffer) {
        Ok(raw_events) => {
            let mut epoch = NativeQueueEpoch::Intact;
            for event in raw_events {
                epoch.observe(event.mask);
                let descriptor = event.wd.get_watch_descriptor_id();
                let terminal = event.mask.contains(EventMask::IGNORED);
                let activity = if terminal {
                    descriptors.remove(&descriptor).map(|watch| watch.activity)
                } else {
                    descriptors
                        .get(&descriptor)
                        .map(|watch| watch.activity.clone())
                };
                let native_event = NativeEvent {
                    descriptor,
                    activity: activity.clone(),
                    mask: event.mask,
                    cookie: event.cookie,
                    name: event.name.map(ToOwned::to_owned),
                };
                let outcome = match (terminal, activity) {
                    (true, Some(activity)) => events
                        .publish_control(WorkerControl::Terminal(native_event), move || {
                            activity.terminate()
                        }),
                    _ => events.publish(native_event),
                };
                if outcome == PublishOutcome::Closed {
                    return Ok(NativeDrain {
                        receiver_open: false,
                        epoch,
                    });
                }
            }
            Ok(NativeDrain {
                receiver_open: true,
                epoch,
            })
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(NativeDrain {
            receiver_open: true,
            epoch: NativeQueueEpoch::Intact,
        }),
        Err(error) => Err(error.to_string()),
    }
}

/// Drain the exact kernel-queue prefix that exists after a native mutation.
/// Linux queues IN_IGNORED before releasing a descriptor, but it can replace
/// that record with IN_Q_OVERFLOW. FIONREAD gives us a finite byte boundary:
/// later records cannot overtake this prefix, and the typed epoch result keeps
/// either condition from being mistaken for a reusable live descriptor.
fn drain_native_boundary(
    inotify: &mut Inotify,
    descriptors: &mut HashMap<i32, NativeWatch>,
    events: &DeliverySender<NativeEvent, WorkerControl>,
) -> Result<NativeDrain, String> {
    let bytes = rustix::io::ioctl_fionread(&*inotify).map_err(|error| error.to_string())?;
    let length = usize::try_from(bytes)
        .map_err(|_| format!("inotify removal boundary is too large: {bytes} bytes"))?;
    if length == 0 {
        return Ok(NativeDrain {
            receiver_open: true,
            epoch: NativeQueueEpoch::Intact,
        });
    }
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(length)
        .map_err(|error| format!("could not allocate inotify removal boundary: {error}"))?;
    buffer.resize(length, 0);
    drain_native_events(inotify, descriptors, &mut buffer, events)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommandOutcome {
    Applied,
    Idle,
    Stop,
    Failed(WorkerFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingAddKind {
    ExistingEpoch,
    NewEpoch,
}

struct PendingAdd {
    descriptor: WatchDescriptor,
    activity: WatchActivity,
    kind: PendingAddKind,
}

enum PendingAddOutcome {
    Ready {
        activity: WatchActivity,
        created_native_watch: bool,
    },
    Invalidated,
    ReceiverClosed,
    Failed(WorkerFailure),
}

impl PendingAdd {
    /// Establish ownership immediately after `inotify_add_watch` returns.
    ///
    /// A vacant numeric descriptor is provisional until the finite native
    /// boundary has been consumed.  An occupied descriptor may be the same
    /// physical watch or a just-recycled number; completion treats a terminal
    /// record in that ambiguous interval conservatively instead of aliasing
    /// two kernel ownership epochs.
    fn begin(descriptor: WatchDescriptor, descriptors: &mut HashMap<i32, NativeWatch>) -> Self {
        let id = descriptor.get_watch_descriptor_id();
        match descriptors.entry(id) {
            std::collections::hash_map::Entry::Occupied(entry) => Self {
                descriptor,
                activity: entry.get().activity.clone(),
                kind: PendingAddKind::ExistingEpoch,
            },
            std::collections::hash_map::Entry::Vacant(entry) => {
                let activity = WatchActivity::active();
                entry.insert(NativeWatch::provisional(
                    descriptor.clone(),
                    activity.clone(),
                ));
                Self {
                    descriptor,
                    activity,
                    kind: PendingAddKind::NewEpoch,
                }
            }
        }
    }

    #[cfg(test)]
    fn activity(&self) -> &WatchActivity {
        &self.activity
    }

    fn complete(
        self,
        inotify: &mut Inotify,
        descriptors: &mut HashMap<i32, NativeWatch>,
        events: &DeliverySender<NativeEvent, WorkerControl>,
    ) -> PendingAddOutcome {
        match drain_native_boundary(inotify, descriptors, events)
            .map_err(WorkerFailure::Native)
            .and_then(NativeDrain::require_intact_epoch)
        {
            Ok(true) => {}
            Ok(false) => return PendingAddOutcome::ReceiverClosed,
            Err(error) => return PendingAddOutcome::Failed(error),
        }

        let id = self.descriptor.get_watch_descriptor_id();
        match self.kind {
            PendingAddKind::NewEpoch => match descriptors.get_mut(&id) {
                Some(watch)
                    if watch.phase == NativeWatchPhase::Provisional
                        && watch.is_registration(&self.activity) =>
                {
                    watch.phase = NativeWatchPhase::Active;
                    PendingAddOutcome::Ready {
                        activity: self.activity,
                        created_native_watch: true,
                    }
                }
                None => PendingAddOutcome::Invalidated,
                Some(_) => PendingAddOutcome::Failed(WorkerFailure::Native(
                    "inotify provisional watch ownership changed unexpectedly".to_owned(),
                )),
            },
            PendingAddKind::ExistingEpoch => match descriptors.get(&id) {
                Some(watch)
                    if watch.phase == NativeWatchPhase::Active
                        && watch.is_registration(&self.activity) =>
                {
                    PendingAddOutcome::Ready {
                        activity: self.activity,
                        created_native_watch: false,
                    }
                }
                None => {
                    // One terminal record is ambiguous here: it can retire the
                    // existing watch or precede reuse of the same numeric id.
                    // Remove the syscall's returned descriptor in either case;
                    // EINVAL means the former, success cleans up the latter.
                    let remove_result = inotify.watches().remove(self.descriptor);
                    if let Err(error) = &remove_result
                        && error.kind() != io::ErrorKind::InvalidInput
                    {
                        return PendingAddOutcome::Failed(WorkerFailure::Native(format!(
                            "could not resolve an inotify add race: {error}"
                        )));
                    }
                    match drain_native_boundary(inotify, descriptors, events)
                        .map_err(WorkerFailure::Native)
                        .and_then(NativeDrain::require_intact_epoch)
                    {
                        Ok(true) => PendingAddOutcome::Invalidated,
                        Ok(false) => PendingAddOutcome::ReceiverClosed,
                        Err(error) => PendingAddOutcome::Failed(error),
                    }
                }
                Some(_) => PendingAddOutcome::Failed(WorkerFailure::Native(
                    "inotify existing watch ownership changed unexpectedly".to_owned(),
                )),
            },
        }
    }
}

fn apply_next_command(
    inotify: &mut Inotify,
    commands: &Receiver<Command>,
    descriptors: &mut HashMap<i32, NativeWatch>,
    events: &DeliverySender<NativeEvent, WorkerControl>,
) -> CommandOutcome {
    match commands.try_recv() {
        Ok(Command::Add { path, mask, reply }) => {
            match inotify.watches().add(path, mask) {
                Ok(watch_descriptor) => {
                    let id = watch_descriptor.get_watch_descriptor_id();
                    match PendingAdd::begin(watch_descriptor, descriptors).complete(
                        inotify,
                        descriptors,
                        events,
                    ) {
                        PendingAddOutcome::Ready {
                            activity,
                            created_native_watch,
                        } => {
                            if reply.send(Ok((id, activity.clone()))).is_err()
                                && created_native_watch
                            {
                                let Some(watch) = descriptors.remove(&id) else {
                                    return CommandOutcome::Failed(WorkerFailure::Native(
                                        "new inotify watch disappeared before rollback".to_owned(),
                                    ));
                                };
                                if !watch.is_registration(&activity) {
                                    return CommandOutcome::Failed(WorkerFailure::Native(
                                        "new inotify watch changed ownership before rollback"
                                            .to_owned(),
                                    ));
                                }
                                activity.terminate();
                                let native_result = inotify.watches().remove(watch.descriptor);
                                match drain_native_boundary(inotify, descriptors, events)
                                    .map_err(WorkerFailure::Native)
                                    .and_then(NativeDrain::require_intact_epoch)
                                {
                                    Ok(true) => {}
                                    Ok(false) => return CommandOutcome::Stop,
                                    Err(error) => return CommandOutcome::Failed(error),
                                }
                                if let Err(error) = native_result
                                    && error.kind() != io::ErrorKind::InvalidInput
                                {
                                    return CommandOutcome::Failed(WorkerFailure::Native(format!(
                                        "could not roll back unclaimed inotify watch: {error}"
                                    )));
                                }
                            }
                        }
                        PendingAddOutcome::Invalidated => {
                            let _ = reply.send(Err(
                                "inotify watch was invalidated while it was being registered"
                                    .to_owned(),
                            ));
                        }
                        PendingAddOutcome::ReceiverClosed => return CommandOutcome::Stop,
                        PendingAddOutcome::Failed(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            return CommandOutcome::Failed(error);
                        }
                    }
                }
                Err(error) => {
                    let _ = reply.send(Err(error.to_string()));
                }
            }
            CommandOutcome::Applied
        }
        Ok(Command::Remove { descriptor, reply }) => {
            match descriptors.remove(&descriptor) {
                Some(watch) => {
                    // GNU retires the public descriptor even if the native
                    // removal reports an error.
                    watch.activity.terminate();
                    let native_result = inotify
                        .watches()
                        .remove(watch.descriptor)
                        .map(|()| true)
                        .map_err(|error| error.to_string());
                    match drain_native_boundary(inotify, descriptors, events)
                        .map_err(WorkerFailure::Native)
                        .and_then(NativeDrain::require_intact_epoch)
                    {
                        Ok(true) => {
                            let _ = reply.send(native_result);
                            CommandOutcome::Applied
                        }
                        Ok(false) => {
                            let _ = reply.send(Err("inotify event receiver closed".to_owned()));
                            CommandOutcome::Stop
                        }
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            CommandOutcome::Failed(error)
                        }
                    }
                }
                None => {
                    let _ = reply.send(Ok(false));
                    CommandOutcome::Applied
                }
            }
        }
        Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => CommandOutcome::Stop,
        Err(TryRecvError::Empty) => CommandOutcome::Idle,
    }
}

fn terminate_all(descriptors: &HashMap<i32, NativeWatch>) {
    for watch in descriptors.values() {
        watch.activity.terminate();
    }
}
