#[cfg(target_os = "windows")]
use super::super::{
    DrainBatch, FileNotifyBackend, FileWatch, RemoveWatchOutcome, TrackedWatch, WatchActivity,
    WatchIdAllocator, file_notify_error, finish_watch_drain,
};
use super::super::{FileNotifyEvent, WatchId, WatchRegistration};
#[cfg(target_os = "windows")]
use crate::emacs_core::error::Flow;
use crate::emacs_core::value::Value;
use enumflags2::BitFlags;
#[cfg(target_os = "windows")]
use std::path::Path;
use std::path::PathBuf;

mod codec;
#[cfg(target_os = "windows")]
mod lisp;
#[cfg(target_os = "windows")]
mod worker;

#[cfg(target_os = "windows")]
pub(crate) use lisp::{w32notify_add_watch, w32notify_rm_watch, w32notify_valid_p};

#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum W32Filter {
    FileName = 1 << 0,
    DirectoryName = 1 << 1,
    Attributes = 1 << 2,
    Size = 1 << 3,
    LastWriteTime = 1 << 4,
    LastAccessTime = 1 << 5,
    CreationTime = 1 << 6,
    SecurityDescriptor = 1 << 7,
    Subtree = 1 << 8,
}

impl W32Filter {
    pub(in super::super) fn from_lisp_name(name: &str) -> Option<Self> {
        match name {
            "file-name" => Some(Self::FileName),
            "directory-name" => Some(Self::DirectoryName),
            "attributes" => Some(Self::Attributes),
            "size" => Some(Self::Size),
            "last-write-time" => Some(Self::LastWriteTime),
            "last-access-time" => Some(Self::LastAccessTime),
            "creation-time" => Some(Self::CreationTime),
            "security-desc" => Some(Self::SecurityDescriptor),
            "subtree" => Some(Self::Subtree),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct W32Request {
    filters: BitFlags<W32Filter>,
}

impl W32Request {
    pub(in super::super) fn new(filters: BitFlags<W32Filter>) -> Self {
        Self { filters }
    }

    fn recursive(&self) -> bool {
        self.filters.contains(W32Filter::Subtree)
    }

    /// Exact `ReadDirectoryChangesW` filter bits used by GNU w32notify.
    fn native_filter_bits(&self) -> u32 {
        let mut bits = 0;
        for (filter, native) in [
            (W32Filter::FileName, 0x0000_0001),
            (W32Filter::DirectoryName, 0x0000_0002),
            (W32Filter::Attributes, 0x0000_0004),
            (W32Filter::Size, 0x0000_0008),
            (W32Filter::LastWriteTime, 0x0000_0010),
            (W32Filter::LastAccessTime, 0x0000_0020),
            (W32Filter::CreationTime, 0x0000_0040),
            (W32Filter::SecurityDescriptor, 0x0000_0100),
        ] {
            if self.filters.contains(filter) {
                bits |= native;
            }
        }
        bits
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum W32Action {
    Added,
    Removed,
    Modified,
    RenamedFrom,
    RenamedTo,
}

impl W32Action {
    const fn as_lisp_name(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
            Self::RenamedFrom => "renamed-from",
            Self::RenamedTo => "renamed-to",
        }
    }
}

#[derive(Clone, Debug)]
pub(in super::super) struct W32Event {
    watch_id: WatchId,
    action: W32Action,
    path: PathBuf,
}

impl FileNotifyEvent for W32Event {
    fn watch_id(&self) -> &WatchId {
        &self.watch_id
    }

    fn into_lisp(
        self,
        ctx: &crate::emacs_core::eval::Context,
        _registration: WatchRegistration,
    ) -> Value {
        // GNU w32notify events are `(DESCRIPTOR ACTION FILE)` and use a
        // pointer-like integer as the opaque descriptor.
        Value::list(vec![
            Value::fixnum(self.watch_id.slot()),
            Value::symbol(self.action.as_lisp_name()),
            super::super::lisp::file_name_to_lisp(ctx, &self.path),
        ])
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::super::super::delivery::{
        self, DeliveryReceiver, DeliveryRecord, DeliverySender, EVENT_CAPACITY,
    };
    use super::*;
    use crate::emacs_core::process::WaitNotifier;
    use worker::{Worker, WorkerMessage, WorkerTermination};

    struct W32Watch {
        common: FileWatch<W32Request>,
        path: PathBuf,
        activity: WatchActivity,
        _worker: Worker,
    }

    impl TrackedWatch for W32Watch {
        fn watch_id(&self) -> &WatchId {
            &self.common.id
        }
    }

    #[derive(Default)]
    pub(crate) struct W32NotifyBackend {
        tx: Option<DeliverySender<WorkerMessage, WorkerTermination>>,
        rx: Option<DeliveryReceiver<WorkerMessage, WorkerTermination>>,
        watches: Vec<W32Watch>,
        ids: WatchIdAllocator,
    }

    impl W32NotifyBackend {
        fn ensure_delivery(&mut self, notifier: Option<WaitNotifier>) {
            if self.tx.is_some() {
                return;
            }
            let (tx, rx) = delivery::channel(notifier);
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
    }

    impl FileNotifyBackend for W32NotifyBackend {
        type Request = W32Request;
        type Event = W32Event;

        fn add_watch(
            &mut self,
            path: &Path,
            request: Self::Request,
            notifier: Option<WaitNotifier>,
        ) -> Result<WatchId, Flow> {
            self.ensure_delivery(notifier);
            if !path.exists() {
                return Err(file_notify_error(
                    "Cannot watch file",
                    Some("No such file or directory".to_owned()),
                    Some(Value::string(path.display().to_string())),
                ));
            }
            let descriptor = self.ids.allocate();
            let activity = WatchActivity::active();
            let worker = Worker::start(
                path,
                request.recursive(),
                request.native_filter_bits(),
                descriptor.clone(),
                activity.clone(),
                self.tx.as_ref().expect("delivery was initialized").clone(),
            )
            .map_err(|error| {
                file_notify_error(
                    "Cannot watch file",
                    Some(error),
                    Some(Value::string(path.display().to_string())),
                )
            })?;
            self.watches.push(W32Watch {
                common: FileWatch {
                    id: descriptor.clone(),
                    request,
                },
                path: path.to_path_buf(),
                activity,
                _worker: worker,
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
            self.watches.remove(index);
            if self.watches.is_empty() {
                self.tx = None;
                self.rx = None;
            }
            RemoveWatchOutcome::Removed
        }

        fn valid_p(&self, descriptor: &WatchId) -> bool {
            self.watches
                .iter()
                .any(|watch| watch.common.id == *descriptor && watch.activity.is_active())
        }

        fn drain_events(&mut self) -> Result<DrainBatch<Self::Event>, Flow> {
            let mut events = Vec::new();
            let mut rescans = Vec::new();
            let mut terminated = Vec::new();
            let mut delivery_overflow = false;
            if let Some(rx) = self.rx.as_ref() {
                let delivery = rx.drain_consistent();
                delivery_overflow = delivery.overflowed;
                for record in delivery.records {
                    match record {
                        DeliveryRecord::Event(message) => match message {
                            WorkerMessage::Event(event) => events.push(event),
                            WorkerMessage::Overflow(watch_id) => rescans.push(watch_id),
                        },
                        DeliveryRecord::Control(termination) => match termination {
                            WorkerTermination::Invalidated { watch_id } => {
                                terminated.push(watch_id);
                            }
                            WorkerTermination::Failed { watch_id, error } => {
                                tracing::warn!(
                                    watch = watch_id.slot(),
                                    %error,
                                    "Windows file-notification worker exited"
                                );
                                terminated.push(watch_id);
                            }
                        },
                    }
                }
            }
            terminated.sort_by_key(WatchId::slot);
            terminated.dedup();
            finish_watch_drain(&mut self.watches, &terminated, |watches| {
                if delivery_overflow {
                    tracing::warn!(
                        capacity = EVENT_CAPACITY,
                        "Windows file-notification queue overflowed; emitting conservative changes"
                    );
                    rescans.extend(watches.iter().map(|watch| watch.common.id.clone()));
                }
                rescans.sort_by_key(WatchId::slot);
                rescans.dedup();
                events.extend(
                    rescans
                        .drain(..)
                        .filter_map(|watch_id| {
                            watches.iter().find(|watch| watch.common.id == watch_id)
                        })
                        .map(|watch| W32Event {
                            watch_id: watch.common.id.clone(),
                            action: W32Action::Modified,
                            path: watch.path.clone(),
                        }),
                );
            });
            if self.watches.is_empty() {
                self.tx = None;
                self.rx = None;
            }
            Ok(DrainBatch {
                events,
                terminated,
                failure: None,
            })
        }

        fn has_watches(&self) -> bool {
            !self.watches.is_empty()
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) use native::W32NotifyBackend;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod windows_test;
