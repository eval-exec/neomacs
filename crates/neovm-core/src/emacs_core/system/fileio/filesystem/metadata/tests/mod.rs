use super::super::{EditorFileSystem, MemoryFileSystem, MountTableFileSystem};
use std::path::Path;

#[test]
fn virtual_policy_does_not_invent_identity_or_unobserved_times() {
    let mut mounts = MountTableFileSystem::new();
    mounts
        .mount(
            Path::new("/virtual/home"),
            Box::new(MemoryFileSystem::new()),
        )
        .unwrap();
    let attributes = mounts.attributes(Path::new("/virtual")).unwrap();
    assert_eq!(attributes.links, Some(1));
    assert_eq!(
        attributes.user.as_ref().unwrap().name.as_deref(),
        Some("virtual")
    );
    assert_eq!(attributes.group.as_ref().unwrap().id, 0);
    assert_eq!(attributes.identity, None);
    assert_eq!(attributes.accessed, None);
    assert_eq!(attributes.modified, None);
    assert_eq!(attributes.changed, None);
}
