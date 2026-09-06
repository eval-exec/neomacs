//! Linux inotify adapter.
//!
//! This layer preserves the native mask and rename cookie until Lisp event
//! encoding.  A generic filesystem-event vocabulary is too lossy for GNU's
//! low-level `inotify-*` contract (notably `dont-follow`, `onlydir`, combined
//! bits, `isdir`, `unmount`, and terminal `ignored`).

use super::super::delivery::DeliveryRecord;
use super::super::{
    DrainBatch, FileNotifyBackend, FileNotifyEvent, FileWatch, RemoveWatchOutcome, TrackedWatch,
    WatchActivity, WatchId, WatchIdAllocator, WatchRegistration, file_notify_error,
    finish_watch_drain,
};
use crate::emacs_core::error::Flow;
use crate::emacs_core::process::WaitNotifier;
use crate::emacs_core::value::Value;
use inotify::{EventMask, WatchMask};
use std::path::{Path, PathBuf};

mod lisp;
mod worker;

pub(crate) use lisp::{inotify_add_watch, inotify_rm_watch, inotify_valid_p};

#[cfg(test)]
#[path = "tests/linux.rs"]
mod linux_test;

use worker::{NativeEvent, Worker, WorkerControl};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct InotifyRequest {
    watch_mask: WatchMask,
    event_mask: EventMask,
}

impl InotifyRequest {
    pub(in super::super) fn new(aspects: Vec<String>) -> Self {
        let mut watch_mask = WatchMask::EXCL_UNLINK | WatchMask::MASK_ADD;
        let mut event_mask = EventMask::empty();
        for aspect in aspects {
            match aspect.as_str() {
                "access" => {
                    watch_mask.insert(WatchMask::ACCESS);
                    event_mask.insert(EventMask::ACCESS);
                }
                "attrib" => {
                    watch_mask.insert(WatchMask::ATTRIB);
                    event_mask.insert(EventMask::ATTRIB);
                }
                "close-write" => {
                    watch_mask.insert(WatchMask::CLOSE_WRITE);
                    event_mask.insert(EventMask::CLOSE_WRITE);
                }
                "close-nowrite" => {
                    watch_mask.insert(WatchMask::CLOSE_NOWRITE);
                    event_mask.insert(EventMask::CLOSE_NOWRITE);
                }
                "create" => {
                    watch_mask.insert(WatchMask::CREATE);
                    event_mask.insert(EventMask::CREATE);
                }
                "delete" => {
                    watch_mask.insert(WatchMask::DELETE);
                    event_mask.insert(EventMask::DELETE);
                }
                "delete-self" => {
                    watch_mask.insert(WatchMask::DELETE_SELF);
                    event_mask.insert(EventMask::DELETE_SELF);
                }
                "modify" => {
                    watch_mask.insert(WatchMask::MODIFY);
                    event_mask.insert(EventMask::MODIFY);
                }
                "move-self" => {
                    watch_mask.insert(WatchMask::MOVE_SELF);
                    event_mask.insert(EventMask::MOVE_SELF);
                }
                "moved-from" => {
                    watch_mask.insert(WatchMask::MOVED_FROM);
                    event_mask.insert(EventMask::MOVED_FROM);
                }
                "moved-to" => {
                    watch_mask.insert(WatchMask::MOVED_TO);
                    event_mask.insert(EventMask::MOVED_TO);
                }
                "open" => {
                    watch_mask.insert(WatchMask::OPEN);
                    event_mask.insert(EventMask::OPEN);
                }
                "move" => {
                    watch_mask.insert(WatchMask::MOVE);
                    event_mask.insert(EventMask::MOVED_FROM | EventMask::MOVED_TO);
                }
                "close" => {
                    watch_mask.insert(WatchMask::CLOSE);
                    event_mask.insert(EventMask::CLOSE_WRITE | EventMask::CLOSE_NOWRITE);
                }
                "dont-follow" => watch_mask.insert(WatchMask::DONT_FOLLOW),
                "onlydir" => watch_mask.insert(WatchMask::ONLYDIR),
                "all-events" | "t" => {
                    watch_mask.insert(WatchMask::ALL_EVENTS);
                    event_mask.insert(EventMask::from_bits_retain(WatchMask::ALL_EVENTS.bits()));
                }
                // These are kernel-generated result bits.  GNU accepts them in
                // ASPECT, while delivery remains unconditional when they occur.
                "ignored" | "unmount" => {}
                _ => unreachable!("Lisp validation rejects unknown inotify aspects"),
            }
        }
        Self {
            watch_mask,
            event_mask,
        }
    }

    fn accepts(&self, mask: EventMask) -> bool {
        mask.intersects(self.event_mask)
            || mask.intersects(
                EventMask::IGNORED | EventMask::ISDIR | EventMask::Q_OVERFLOW | EventMask::UNMOUNT,
            )
    }
}

#[derive(Clone, Debug)]
enum InotifyEventName {
    /// A child name supplied by the kernel and decoded at evaluator time.
    Native(PathBuf),
    /// A nameless event for the watched object. GNU returns the exact Lisp
    /// filename object retained by `inotify-add-watch` for this case.
    RegisteredWatch,
}

#[derive(Clone, Debug)]
pub(in super::super) struct InotifyEvent {
    watch_id: WatchId,
    aspects: Vec<&'static str>,
    name: InotifyEventName,
    cookie: u32,
}

impl FileNotifyEvent for InotifyEvent {
    fn watch_id(&self) -> &WatchId {
        &self.watch_id
    }

    fn into_lisp(
        self,
        ctx: &crate::emacs_core::eval::Context,
        registration: WatchRegistration,
    ) -> Value {
        // GNU inotify events are `(DESCRIPTOR ASPECTS NAME COOKIE)`.
        let name = match self.name {
            InotifyEventName::Native(path) => super::super::lisp::file_name_to_lisp(ctx, &path),
            InotifyEventName::RegisteredWatch => registration.registered_file_name(),
        };
        Value::list(vec![
            self.watch_id.to_inotify_lisp(),
            Value::list(self.aspects.into_iter().map(Value::symbol).collect()),
            name,
            Value::fixnum(i64::from(self.cookie)),
        ])
    }
}

#[derive(Clone, Debug)]
struct InotifyWatch {
    common: FileWatch<InotifyRequest>,
    native_descriptor: i32,
    activity: WatchActivity,
}

impl TrackedWatch for InotifyWatch {
    fn watch_id(&self) -> &WatchId {
        &self.common.id
    }
}

#[derive(Default)]
pub(in super::super) struct InotifyBackend {
    worker: Option<Worker>,
    watches: Vec<InotifyWatch>,
    ids: WatchIdAllocator,
}

impl InotifyBackend {
    fn ensure_worker(&mut self, notifier: Option<WaitNotifier>) -> Result<&mut Worker, Flow> {
        if self.worker.is_none() {
            self.worker = Some(Worker::start(notifier).map_err(|error| {
                file_notify_error("File watching is not available", Some(error), None)
            })?);
        }
        Ok(self.worker.as_mut().expect("worker was initialized"))
    }

    fn aspects(mask: EventMask) -> Vec<&'static str> {
        // GNU's C implementation conses in the opposite order from its bit
        // probes, producing this observable list order.
        [
            (EventMask::UNMOUNT, "unmount"),
            (EventMask::Q_OVERFLOW, "q-overflow"),
            (EventMask::ISDIR, "isdir"),
            (EventMask::IGNORED, "ignored"),
            (EventMask::OPEN, "open"),
            (EventMask::MOVED_TO, "moved-to"),
            (EventMask::MOVED_FROM, "moved-from"),
            (EventMask::MOVE_SELF, "move-self"),
            (EventMask::MODIFY, "modify"),
            (EventMask::DELETE_SELF, "delete-self"),
            (EventMask::DELETE, "delete"),
            (EventMask::CREATE, "create"),
            (EventMask::CLOSE_NOWRITE, "close-nowrite"),
            (EventMask::CLOSE_WRITE, "close-write"),
            (EventMask::ATTRIB, "attrib"),
            (EventMask::ACCESS, "access"),
        ]
        .into_iter()
        .filter_map(|(bit, name)| mask.contains(bit).then_some(name))
        .collect()
    }

    fn translate_event(&self, event: NativeEvent) -> Vec<InotifyEvent> {
        let queue_overflow = event.mask.contains(EventMask::Q_OVERFLOW);
        self.watches
            .iter()
            .filter(|watch| {
                queue_overflow
                    || (watch.native_descriptor == event.descriptor
                        && event
                            .activity
                            .as_ref()
                            .is_some_and(|activity| activity.same_registration(&watch.activity)))
            })
            .filter(|watch| watch.common.request.accepts(event.mask))
            .map(|watch| InotifyEvent {
                watch_id: watch.common.id.clone(),
                aspects: Self::aspects(event.mask),
                name: event
                    .name
                    .as_ref()
                    .map(PathBuf::from)
                    .map(InotifyEventName::Native)
                    .unwrap_or(InotifyEventName::RegisteredWatch),
                cookie: event.cookie,
            })
            .collect()
    }

    fn overflow_events(watches: &[InotifyWatch]) -> Vec<InotifyEvent> {
        watches
            .iter()
            .map(|watch| InotifyEvent {
                watch_id: watch.common.id.clone(),
                aspects: vec!["q-overflow"],
                name: InotifyEventName::RegisteredWatch,
                cookie: 0,
            })
            .collect()
    }
}

impl FileNotifyBackend for InotifyBackend {
    type Request = InotifyRequest;
    type Event = InotifyEvent;

    fn add_watch(
        &mut self,
        path: &Path,
        request: Self::Request,
        notifier: Option<WaitNotifier>,
    ) -> Result<WatchId, Flow> {
        let add_result = self
            .ensure_worker(notifier)?
            .add(path.to_path_buf(), request.watch_mask);
        let (native_descriptor, activity) = match add_result {
            Ok((descriptor, activity)) => (descriptor, activity),
            Err(error) => {
                if self.watches.is_empty() {
                    self.worker = None;
                }
                return Err(file_notify_error(
                    "Could not add watch for file",
                    Some(error),
                    Some(Value::string(path.display().to_string())),
                ));
            }
        };
        let descriptor = self.ids.allocate();
        self.watches.push(InotifyWatch {
            common: FileWatch {
                id: descriptor.clone(),
                request,
            },
            native_descriptor,
            activity,
        });
        Ok(descriptor)
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
        let remove_native = !self.watches.iter().enumerate().any(|(other_index, watch)| {
            other_index != index && watch.native_descriptor == native_descriptor
        });
        let native_error = remove_native
            .then(|| {
                self.worker
                    .as_ref()
                    .expect("a live watch has a worker")
                    .remove(native_descriptor)
            })
            .transpose()
            .err();
        self.watches.remove(index);
        if self.watches.is_empty() {
            self.worker = None;
        }
        match native_error {
            Some(error) => RemoveWatchOutcome::RemovedWithError(file_notify_error(
                "Could not rm watch",
                Some(error),
                None,
            )),
            None => RemoveWatchOutcome::Removed,
        }
    }

    fn valid_p(&self, descriptor: &WatchId) -> bool {
        self.watches
            .iter()
            .any(|watch| watch.common.id == *descriptor && watch.activity.is_active())
    }

    fn drain_events(&mut self) -> Result<DrainBatch<Self::Event>, Flow> {
        let mut events = Vec::new();
        let mut overflowed = false;
        let mut failures = Vec::new();
        let mut terminated = Vec::new();
        if let Some(worker) = self.worker.as_ref() {
            let delivery = worker.drain();
            if delivery.overflowed {
                overflowed = true;
                tracing::warn!(
                    capacity = super::super::delivery::EVENT_CAPACITY,
                    "inotify delivery queue overflowed; requesting conservative rescan"
                );
            }
            for record in delivery.records {
                match record {
                    DeliveryRecord::Event(event) => events.extend(self.translate_event(event)),
                    DeliveryRecord::Control(control) => match control {
                        WorkerControl::Terminal(event) => {
                            let translated = self.translate_event(event);
                            terminated
                                .extend(translated.iter().map(|event| event.watch_id.clone()));
                            events.extend(translated);
                        }
                        WorkerControl::Failed(error) => {
                            failures.push(error.to_string());
                            terminated
                                .extend(self.watches.iter().map(|watch| watch.common.id.clone()));
                        }
                    },
                }
            }
        }
        terminated.sort_by_key(WatchId::slot);
        terminated.dedup();
        finish_watch_drain(&mut self.watches, &terminated, |watches| {
            if overflowed {
                events.extend(Self::overflow_events(watches));
            }
        });
        if self.watches.is_empty() {
            self.worker = None;
        }
        let failure = (!failures.is_empty()).then(|| {
            file_notify_error(
                "Error while retrieving file system events",
                Some(failures.join("\n")),
                None,
            )
        });
        Ok(DrainBatch {
            events,
            terminated,
            failure,
        })
    }

    fn has_watches(&self) -> bool {
        !self.watches.is_empty()
    }
}
