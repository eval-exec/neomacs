use super::{
    FileNotifyBackend, FileNotifyEvent, FileNotifyWatchDescriptor, FileWatch, WatchDialect,
    file_notify_error,
};
use crate::emacs_core::error::Flow;
use crate::emacs_core::process::WaitNotifier;
use crate::emacs_core::value::Value;
use notify::Watcher;
use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct NotifyRsInotifyBackend {
    watcher: Option<notify::RecommendedWatcher>,
    rx: Option<std::sync::mpsc::Receiver<Result<notify::Event, notify::Error>>>,
    watches: Vec<FileWatch>,
    next_id: i64,
}

impl NotifyRsInotifyBackend {
    fn ensure_watcher(&mut self, notifier: Option<WaitNotifier>) -> Result<(), Flow> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if tx.send(res).is_ok()
                    && let Some(notifier) = notifier.as_ref()
                    && let Err(error) = notifier.notify()
                {
                    tracing::error!(%error, "failed to wake evaluator for file notification");
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| {
            file_notify_error("File watching is not available", Some(e.to_string()), None)
        })?;
        self.watcher = Some(watcher);
        self.rx = Some(rx);
        Ok(())
    }

    fn allocate_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn watch_requests(watch: &FileWatch, aspect: &str) -> bool {
        watch
            .aspects
            .iter()
            .any(|requested| match requested.as_str() {
                "t" | "all-events" => true,
                "move" => matches!(aspect, "moved-from" | "moved-to"),
                "close" => matches!(aspect, "close-write" | "close-nowrite"),
                requested => requested == aspect,
            })
    }

    fn watch_matches_path(watch: &FileWatch, event_path: &Path) -> bool {
        if watch.is_directory {
            event_path == watch.path || event_path.parent() == Some(watch.path.as_path())
        } else {
            event_path == watch.path
        }
    }

    fn reported_path(watch: &FileWatch, event_path: &Path) -> PathBuf {
        if watch.is_directory && event_path != watch.path {
            event_path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| event_path.to_path_buf())
        } else {
            event_path.to_path_buf()
        }
    }

    fn event_aspects(
        event: &notify::Event,
        path_index: usize,
        watch: &FileWatch,
    ) -> Vec<&'static str> {
        if event.need_rescan() {
            return vec!["q-overflow"];
        }

        match event.kind {
            notify::EventKind::Access(AccessKind::Open(_)) => vec!["open"],
            notify::EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                vec!["close-write"]
            }
            notify::EventKind::Access(AccessKind::Close(_)) => vec!["close-nowrite"],
            notify::EventKind::Access(_) => vec!["access"],
            notify::EventKind::Create(_) => vec!["create"],
            notify::EventKind::Modify(ModifyKind::Data(_)) => vec!["modify"],
            notify::EventKind::Modify(ModifyKind::Metadata(_)) => vec!["attrib"],
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)) => vec!["moved-from"],
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::To)) => vec!["moved-to"],
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if path_index == 0 {
                    vec!["moved-from"]
                } else {
                    vec!["moved-to"]
                }
            }
            notify::EventKind::Modify(ModifyKind::Name(_)) => vec!["move-self"],
            notify::EventKind::Modify(_) => vec!["modify"],
            notify::EventKind::Remove(_) => {
                if event.paths.get(path_index) == Some(&watch.path) {
                    vec!["delete-self"]
                } else {
                    vec!["delete"]
                }
            }
            notify::EventKind::Any | notify::EventKind::Other => Vec::new(),
        }
    }

    /// The kqueue action GNU would report for this notify event, and (for a
    /// two-path rename) the FILE1 that goes with it.
    ///
    /// GNU has two producers: `kqueue_callback` decodes kernel fflags for the
    /// watched file itself (src/kqueue.c:301-327 -- delete, write, extend,
    /// attrib, link, rename, revoke), and `kqueue_compare_dir_list` diffs
    /// directory listings into per-file `create'/`delete'/`write'/`rename'
    /// events (:110-273).  The `notify` crate hands this port the per-path
    /// events both producers reconstruct, so the mapping is direct; `extend',
    /// `link' and `revoke' have no `notify` equivalent and are never emitted.
    fn kqueue_action(
        event: &notify::Event,
        path_index: usize,
        watch: &FileWatch,
    ) -> Option<(&'static str, Option<PathBuf>)> {
        match event.kind {
            notify::EventKind::Create(_) => Some(("create", None)),
            notify::EventKind::Remove(_) => Some(("delete", None)),
            notify::EventKind::Modify(ModifyKind::Metadata(_)) => Some(("attrib", None)),
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)) => Some(("rename", None)),
            // A file appearing under a new name is what GNU's directory diff
            // reports as `create' when the old name was never in this
            // directory (src/kqueue.c:224-231).
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::To)) => Some(("create", None)),
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if path_index == 0 {
                    let file1 = event
                        .paths
                        .get(1)
                        .map(|file1| Self::reported_path(watch, file1));
                    Some(("rename", file1))
                } else {
                    // Folded into the `rename' produced for the first path,
                    // as GNU's single `(rename FILE FILE1)` event is.
                    None
                }
            }
            notify::EventKind::Modify(_) => Some(("write", None)),
            notify::EventKind::Access(_) => None,
            notify::EventKind::Any | notify::EventKind::Other => None,
        }
    }

    fn translate_event(&self, event: notify::Event) -> Vec<FileNotifyEvent> {
        let cookie = event.tracker().unwrap_or(0);
        let mut translated = Vec::new();

        for watch in &self.watches {
            for (path_index, path) in event.paths.iter().enumerate() {
                if !Self::watch_matches_path(watch, path) {
                    continue;
                }
                match watch.dialect {
                    WatchDialect::Inotify => {
                        let aspects = Self::event_aspects(&event, path_index, watch);
                        let aspects: Vec<_> = aspects
                            .into_iter()
                            .filter(|aspect| {
                                matches!(*aspect, "q-overflow" | "ignored" | "unmount")
                                    || Self::watch_requests(watch, aspect)
                            })
                            .collect();
                        if aspects.is_empty() {
                            continue;
                        }
                        translated.push(FileNotifyEvent {
                            descriptor: FileNotifyWatchDescriptor::new(watch.id, watch.generation),
                            aspects,
                            path: Self::reported_path(watch, path),
                            cookie,
                            callback: watch.callback,
                            dialect: WatchDialect::Inotify,
                            file1: None,
                        });
                    }
                    WatchDialect::Kqueue => {
                        let Some((action, file1)) = Self::kqueue_action(&event, path_index, watch)
                        else {
                            continue;
                        };
                        // GNU `kqueue_generate_event` (src/kqueue.c:84-90)
                        // drops every action absent from the watch's FLAGS by
                        // exact `Fmember` -- no aliases, no always-delivered
                        // administrative aspects.
                        if !watch.aspects.iter().any(|requested| requested == action) {
                            continue;
                        }
                        translated.push(FileNotifyEvent {
                            descriptor: FileNotifyWatchDescriptor::new(watch.id, watch.generation),
                            aspects: vec![action],
                            // GNU reports the watch's own stored FILE for the
                            // watched file (src/kqueue.c:296-299), relative
                            // names for directory children (:110-273).
                            path: if watch.is_directory {
                                Self::reported_path(watch, path)
                            } else {
                                watch.path.clone()
                            },
                            cookie,
                            callback: watch.callback,
                            dialect: WatchDialect::Kqueue,
                            file1,
                        });
                    }
                }
            }
        }

        translated
    }

    /// GNU `kqueue_callback` cancels the monitor itself when the watched file
    /// is deleted or renamed (src/kqueue.c:330-333, NOTE_DELETE | NOTE_RENAME
    /// | NOTE_REVOKE -> `Fkqueue_rm_watch`); inotify watches have no such
    /// rule.  Returns the kqueue watches this raw event kills.
    fn kqueue_watches_cancelled_by(&self, event: &notify::Event) -> Vec<FileNotifyWatchDescriptor> {
        if !matches!(
            event.kind,
            notify::EventKind::Remove(_) | notify::EventKind::Modify(ModifyKind::Name(_))
        ) {
            return Vec::new();
        }
        self.watches
            .iter()
            .filter(|watch| {
                watch.dialect == WatchDialect::Kqueue && event.paths.contains(&watch.path)
            })
            .map(|watch| FileNotifyWatchDescriptor::new(watch.id, watch.generation))
            .collect()
    }
}

impl FileNotifyBackend for NotifyRsInotifyBackend {
    fn allocated_p(&self) -> bool {
        self.watcher.is_some()
    }

    fn watch_list(&self) -> Vec<FileWatch> {
        self.watches.clone()
    }

    fn add_watch(
        &mut self,
        path: &Path,
        aspects: Vec<String>,
        callback: Value,
        notifier: Option<WaitNotifier>,
        dialect: WatchDialect,
    ) -> Result<FileNotifyWatchDescriptor, Flow> {
        self.ensure_watcher(notifier)?;

        if !path.exists() {
            return Err(file_notify_error(
                "Could not add watch for file",
                Some("No such file or directory".to_string()),
                Some(Value::string(path.display().to_string())),
            ));
        }
        let path_already_watched = self.watches.iter().any(|watch| watch.path == path);
        if !path_already_watched && let Some(ref mut watcher) = self.watcher {
            watcher
                .watch(path, notify::RecursiveMode::NonRecursive)
                .map_err(|e| {
                    file_notify_error(
                        "Could not add watch for file",
                        Some(e.to_string()),
                        Some(Value::string(path.display().to_string())),
                    )
                })?;
        }

        let id = self.allocate_id();
        let descriptor = FileNotifyWatchDescriptor::new(id, 0);
        self.watches.push(FileWatch {
            id,
            generation: descriptor.generation(),
            path: path.to_path_buf(),
            is_directory: path.is_dir(),
            aspects,
            callback,
            dialect,
        });

        Ok(descriptor)
    }

    fn remove_watch(
        &mut self,
        descriptor: &FileNotifyWatchDescriptor,
        dialect: WatchDialect,
    ) -> Result<bool, Flow> {
        let Some(pos) = self.watches.iter().position(|w| {
            w.id == descriptor.id()
                && w.generation == descriptor.generation()
                && w.dialect == dialect
        }) else {
            return Ok(false);
        };

        let removed = self.watches.remove(pos);
        let path_still_watched = self.watches.iter().any(|watch| watch.path == removed.path);
        if !path_still_watched && let Some(ref mut watcher) = self.watcher {
            let _ = watcher.unwatch(&removed.path);
        }

        if self.watches.is_empty() {
            self.watcher = None;
            self.rx = None;
        }

        Ok(true)
    }

    fn valid_p(&self, descriptor: &FileNotifyWatchDescriptor, dialect: WatchDialect) -> bool {
        self.watches.iter().any(|w| {
            w.id == descriptor.id()
                && w.generation == descriptor.generation()
                && w.dialect == dialect
        })
    }

    fn drain_events(&mut self) -> Result<Vec<FileNotifyEvent>, Flow> {
        let mut raw_events = Vec::new();
        if let Some(rx) = self.rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(Ok(event)) => raw_events.push(event),
                    Ok(Err(error)) => {
                        return Err(file_notify_error(
                            "Error while retrieving file system events",
                            Some(error.to_string()),
                            None,
                        ));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }

        let mut translated = Vec::new();
        for event in raw_events {
            let cancelled = self.kqueue_watches_cancelled_by(&event);
            translated.extend(self.translate_event(event));
            // GNU cancels AFTER generating the event for it
            // (src/kqueue.c:328-333), so the `delete' still reaches the
            // callback and only later events find the watch gone.
            for descriptor in cancelled {
                let _ = self.remove_watch(&descriptor, WatchDialect::Kqueue);
            }
        }
        Ok(translated)
    }

    fn has_watches(&self) -> bool {
        !self.watches.is_empty()
    }
}
