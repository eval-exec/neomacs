//! Pure directory snapshot model used to synthesize kqueue child events.
//!
//! Kqueue reports that a directory changed, but not which child changed.  GNU
//! Emacs therefore compares directory snapshots.  Keeping that policy pure
//! lets us test it without a macOS kernel or a live worker thread.

use super::KqueueAction;
use hashbrown::HashMap;
use std::collections::VecDeque;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;

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

/// Ordered multimap over a snapshot.
///
/// Multiple names can share an inode (hard links), and snapshot order affects
/// GNU's rename pairing.  Queues preserve that order while lazy removal keeps
/// each slot/index entry O(1) amortized instead of repeatedly scanning and
/// shifting a Vec.
struct EntryIndex {
    entries: Vec<Option<DirectoryEntrySnapshot>>,
    by_inode: HashMap<u64, VecDeque<usize>>,
    by_name: HashMap<PathBuf, VecDeque<usize>>,
}

impl EntryIndex {
    #[doc(hidden)]
    pub fn new(entries: Vec<DirectoryEntrySnapshot>) -> Self {
        let mut index = Self {
            entries: Vec::with_capacity(entries.len()),
            by_inode: HashMap::with_capacity(entries.len()),
            by_name: HashMap::with_capacity(entries.len()),
        };
        for entry in entries {
            index.push(entry);
        }
        index
    }

    #[doc(hidden)]
    pub fn push(&mut self, entry: DirectoryEntrySnapshot) {
        let index = self.entries.len();
        self.by_inode
            .entry(entry.inode)
            .or_default()
            .push_back(index);
        self.by_name
            .entry(entry.name.clone())
            .or_default()
            .push_back(index);
        self.entries.push(Some(entry));
    }

    #[doc(hidden)]
    pub fn take_inode(&mut self, inode: u64) -> Option<DirectoryEntrySnapshot> {
        Self::take_from(&mut self.entries, self.by_inode.get_mut(&inode)?)
    }

    #[doc(hidden)]
    pub fn take_name(&mut self, name: &PathBuf) -> Option<DirectoryEntrySnapshot> {
        Self::take_from(&mut self.entries, self.by_name.get_mut(name)?)
    }

    #[doc(hidden)]
    pub fn take_from(
        entries: &mut [Option<DirectoryEntrySnapshot>],
        indexes: &mut VecDeque<usize>,
    ) -> Option<DirectoryEntrySnapshot> {
        while let Some(index) = indexes.pop_front() {
            if let Some(entry) = entries[index].take() {
                return Some(entry);
            }
        }
        None
    }

    #[doc(hidden)]
    pub fn remaining(self) -> impl Iterator<Item = DirectoryEntrySnapshot> {
        self.entries.into_iter().flatten()
    }
}

impl DirectorySnapshot {
    #[cfg(test)]
    #[doc(hidden)]
    pub fn from_entries(entries: Vec<DirectoryEntrySnapshot>) -> Self {
        Self { entries }
    }

    #[cfg(target_os = "macos")]
    #[doc(hidden)]
    pub fn read(directory: &Path) -> std::io::Result<Self> {
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
    #[doc(hidden)]
    pub fn diff(&self, new: &Self) -> Vec<DirectoryChange> {
        let mut available_new = EntryIndex::new(new.entries.clone());
        let mut pending = EntryIndex::new(Vec::new());
        let mut renamed_destinations = HashMap::<PathBuf, usize>::new();
        let mut changes = Vec::new();

        for old_entry in &self.entries {
            if let Some(new_entry) = available_new.take_inode(old_entry.inode) {
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
                    *renamed_destinations.entry(new_entry.name).or_default() += 1;
                }
                continue;
            }

            if let Some(new_entry) = available_new.take_name(&old_entry.name) {
                pending.push(new_entry);
                continue;
            }

            if let Some(new_entry) = pending.take_inode(old_entry.inode) {
                changes.push(DirectoryChange::Rename {
                    from: old_entry.name.clone(),
                    to: new_entry.name,
                });
                continue;
            }

            if let hashbrown::hash_map::Entry::Occupied(mut destination) =
                renamed_destinations.entry(old_entry.name.clone())
            {
                let remaining = *destination.get() - 1;
                if remaining == 0 {
                    destination.remove();
                } else {
                    *destination.get_mut() = remaining;
                }
                continue;
            }

            changes.push(DirectoryChange::Action {
                action: KqueueAction::Delete,
                path: old_entry.name.clone(),
            });
        }

        for entry in available_new.remaining() {
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
        for entry in pending.remaining() {
            changes.push(DirectoryChange::Action {
                action: KqueueAction::Write,
                path: entry.name,
            });
        }

        changes
    }
}
