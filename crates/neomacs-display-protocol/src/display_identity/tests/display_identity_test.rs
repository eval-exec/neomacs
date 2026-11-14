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

#[test]
fn opened_font_minimum_is_shared_by_connection_clones_only() {
    let first = GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":42").unwrap();
    let same_connection = first.clone();
    let separate_connection =
        GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":42").unwrap();
    assert_eq!(
        first, separate_connection,
        "label equality remains unchanged"
    );
    assert_eq!(first.smallest_opened_font_height(), 1);
    assert!(first.observe_opened_font_metrics(15, 5));
    assert_eq!(same_connection.smallest_opened_font_height(), 20);
    assert_eq!(separate_connection.smallest_opened_font_height(), 1);
    assert!(!same_connection.observe_opened_font_metrics(20, 6));
    assert_eq!(first.smallest_opened_font_height(), 20);
    assert!(same_connection.observe_opened_font_metrics(9, 3));
    assert_eq!(first.smallest_opened_font_height(), 12);
    assert!(separate_connection.observe_opened_font_metrics(18, 6));
    assert_eq!(separate_connection.smallest_opened_font_height(), 24);
    assert_eq!(first.smallest_opened_font_height(), 12);
    assert_eq!(
        first, separate_connection,
        "metric changes do not rename a display"
    );
}

#[test]
fn anonymous_connections_keep_separate_opened_font_lifetimes() {
    let first = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    let another = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    assert_eq!(first, another);
    first.observe_opened_font_metrics(10, 4);
    assert_eq!(first.clone().smallest_opened_font_height(), 14);
    assert_eq!(another.smallest_opened_font_height(), 1);
    another.observe_opened_font_metrics(16, 5);
    assert_eq!(another.smallest_opened_font_height(), 21);
    assert_eq!(first.smallest_opened_font_height(), 14);
}

#[test]
fn successful_open_normalizes_nonpositive_ascent_descent_sum() {
    let connection = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11);
    assert!(connection.observe_opened_font_metrics(0, 0));
    assert_eq!(connection.smallest_opened_font_height(), 1);
    assert!(!connection.observe_opened_font_metrics(10, 4));
    assert_eq!(connection.smallest_opened_font_height(), 1);
}
