use super::{
    WindowCoordinateSystem, classify_x_server, coordinate_system_for_observation,
    x11_observation_from_raw,
};
use neomacs_display_protocol::{
    DeviceScale, DisplayObservation, X11DisplayObservation, XServerKind,
};

#[test]
fn xwayland_extension_is_the_authoritative_server_identity() {
    assert_eq!(
        classify_x_server(true, Some("The X.Org Foundation")),
        XServerKind::Xwayland
    );
}

#[test]
fn xorg_vendor_is_classified_only_when_xwayland_extension_is_absent() {
    assert_eq!(
        classify_x_server(false, Some("The X.Org Foundation")),
        XServerKind::Xorg
    );
    assert_eq!(
        classify_x_server(false, Some("XQuartz")),
        XServerKind::Unknown
    );
}

#[test]
fn x11_adapter_validates_untrusted_resource_and_geometry_values() {
    let observation = x11_observation_from_raw(
        true,
        Some("The X.Org Foundation"),
        Some(-12.0),
        1080,
        0,
        DeviceScale::ONE,
    );

    assert_eq!(observation.server(), XServerKind::Xwayland);
    assert_eq!(observation.xft_dpi(), None);
    assert_eq!(observation.geometry(), None);
}

#[test]
fn x11_adapter_preserves_valid_raw_facts_without_applying_policy() {
    let observation = x11_observation_from_raw(
        false,
        Some("The X.Org Foundation"),
        Some(144.0),
        1080,
        800,
        DeviceScale::ONE,
    );

    assert_eq!(observation.server(), XServerKind::Xorg);
    assert_eq!(observation.xft_dpi().map(|dpi| dpi.get()), Some(144.0));
    let geometry = observation.geometry().expect("valid geometry");
    assert_eq!(geometry.height_px(), 1080);
    assert_eq!(geometry.height_mm(), 800);
}

#[test]
fn selected_backend_controls_window_coordinate_units_without_environment_guessing() {
    let x11 = DisplayObservation::X11(X11DisplayObservation::new(
        XServerKind::Unknown,
        None,
        None,
        DeviceScale::ONE,
    ));
    let wayland = DisplayObservation::Wayland {
        device_scale: DeviceScale::ONE,
    };

    assert_eq!(
        coordinate_system_for_observation(x11),
        WindowCoordinateSystem::X11Physical
    );
    assert_eq!(
        coordinate_system_for_observation(wayland),
        WindowCoordinateSystem::WinitLogical
    );
}
