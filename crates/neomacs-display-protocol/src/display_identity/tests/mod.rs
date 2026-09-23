use super::*;
use strum::IntoEnumIterator;

#[test]
fn only_an_x11_connection_name_is_an_x_display() {
    let wayland = GraphicalDisplayIdentity::named(GraphicalBackend::Wayland, "wayland-7").unwrap();
    assert_eq!(wayland.terminal_name(), "wayland-7");
    assert_eq!(wayland.x_display(), None);

    let x11 = GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":42").unwrap();
    assert_eq!(x11.terminal_name(), ":42");
    assert_eq!(x11.x_display(), Some(":42"));
}

#[test]
fn every_graphical_backend_has_a_nonbootstrap_terminal_label() {
    for backend in GraphicalBackend::iter() {
        let identity = GraphicalDisplayIdentity::anonymous_connection(backend);
        assert!(!identity.terminal_name().is_empty());
        assert_ne!(identity.terminal_name(), "initial_terminal");
        assert_eq!(identity.backend(), backend);
        assert_eq!(
            identity.x_display(),
            None,
            "a backend label is not an X11 address"
        );
    }
}

#[test]
fn graphical_identity_rejects_invalid_names() {
    for (name, error) in [
        ("", InvalidDisplayName::Empty),
        ("initial_terminal", InvalidDisplayName::BootstrapName),
        ("bad\0name", InvalidDisplayName::ContainsNul),
    ] {
        assert_eq!(
            GraphicalDisplayIdentity::named(GraphicalBackend::Cocoa, name),
            Err(error)
        );
    }
}
