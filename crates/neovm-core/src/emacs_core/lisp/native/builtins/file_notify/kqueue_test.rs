use super::kqueue::{
    DirectoryChange, DirectoryEntrySnapshot, DirectorySnapshot, requested_vnode_actions,
    vnode_actions,
};
use super::{KqueueAction, KqueueVnodeAction};
use enumflags2::BitFlags;
use std::path::PathBuf;

fn entry(inode: u64, name: &str, modified: i64, changed: i64, size: u64) -> DirectoryEntrySnapshot {
    DirectoryEntrySnapshot {
        inode,
        name: PathBuf::from(name),
        modified: (modified, 0),
        changed: (changed, 0),
        size,
    }
}

#[test]
fn native_kqueue_flags_keep_every_simultaneous_action() {
    let flags = KqueueVnodeAction::Delete
        | KqueueVnodeAction::Write
        | KqueueVnodeAction::Extend
        | KqueueVnodeAction::Attrib
        | KqueueVnodeAction::Link
        | KqueueVnodeAction::Rename
        | KqueueVnodeAction::Revoke;

    assert_eq!(
        vnode_actions(flags),
        [
            KqueueAction::Revoke,
            KqueueAction::Rename,
            KqueueAction::Link,
            KqueueAction::Attrib,
            KqueueAction::Extend,
            KqueueAction::Write,
            KqueueAction::Delete,
        ],
        "GNU decodes every NOTE_* bit and its consing order is observable"
    );
}

#[test]
fn directory_diff_preserves_gnus_rename_and_metadata_semantics() {
    let old = DirectorySnapshot::from_entries(vec![
        entry(10, "renamed-from", 1, 1, 4),
        entry(20, "modified", 1, 1, 4),
    ]);
    let new = DirectorySnapshot::from_entries(vec![
        entry(10, "renamed-to", 1, 1, 4),
        entry(20, "modified", 2, 2, 4),
    ]);

    assert_eq!(
        old.diff(&new),
        [
            DirectoryChange::Rename {
                from: PathBuf::from("renamed-from"),
                to: PathBuf::from("renamed-to"),
            },
            DirectoryChange::Action {
                action: KqueueAction::Write,
                path: PathBuf::from("modified"),
            },
            DirectoryChange::Action {
                action: KqueueAction::Attrib,
                path: PathBuf::from("modified"),
            },
        ]
    );
}

#[test]
fn directory_diff_reports_new_nonempty_files_as_create_then_write() {
    let old = DirectorySnapshot::from_entries(Vec::new());
    let new = DirectorySnapshot::from_entries(vec![entry(30, "new", 1, 1, 5)]);

    assert_eq!(
        old.diff(&new),
        [
            DirectoryChange::Action {
                action: KqueueAction::Create,
                path: PathBuf::from("new"),
            },
            DirectoryChange::Action {
                action: KqueueAction::Write,
                path: PathBuf::from("new"),
            },
        ]
    );
}

#[test]
fn directory_diff_treats_same_name_new_inode_as_pending_write_like_gnu() {
    let old = DirectorySnapshot::from_entries(vec![entry(40, "replaced", 1, 1, 4)]);
    let new = DirectorySnapshot::from_entries(vec![entry(41, "replaced", 2, 2, 8)]);

    assert_eq!(
        old.diff(&new),
        [DirectoryChange::Action {
            action: KqueueAction::Write,
            path: PathBuf::from("replaced"),
        }]
    );
}

#[test]
fn action_sets_cannot_contain_duplicates() {
    let mut actions = BitFlags::<KqueueAction>::empty();
    actions.insert(KqueueAction::Write);
    actions.insert(KqueueAction::Write);
    assert_eq!(actions.iter().collect::<Vec<_>>(), [KqueueAction::Write]);
}

#[test]
fn unrequested_native_actions_are_filtered_without_aliases() {
    let native = KqueueVnodeAction::Write | KqueueVnodeAction::Extend;
    let requested = KqueueAction::Extend | KqueueAction::Rename;
    assert_eq!(
        requested_vnode_actions(native, requested),
        [KqueueAction::Extend]
    );
}
