use super::*;

#[test]
fn packaged_desktop_entry_matches_typed_runtime_identity() {
    let desktop_entry = include_str!("../../assets/neomacs.desktop");

    assert_eq!(NEOMACS_APPLICATION.app_id().as_str(), "neomacs");
    assert_eq!(
        NEOMACS_APPLICATION.desktop_file_id().as_str(),
        "neomacs.desktop"
    );
    assert_eq!(NEOMACS_APPLICATION.icon_name().as_str(), "neomacs");
    assert!(desktop_entry.contains("\nIcon=neomacs\n"));
    assert!(desktop_entry.contains("\nStartupWMClass=neomacs\n"));
}

#[test]
fn linux_window_attributes_use_packaged_desktop_id() {
    for wayland in [true, false] {
        let attrs = linux_window_identity(WindowAttributes::default(), wayland);
        assert!(format!("{attrs:?}").contains(NEOMACS_APPLICATION.app_id().as_str()));
    }
}
