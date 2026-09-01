use super::{
    RawX11DisplayObservation, SelectedLinuxBackend, WindowCoordinateSystem, classify_x_server,
    coordinate_system_for_observation, observe_linux_backend, query_x11_display_with_timeout,
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
    let observation = RawX11DisplayObservation {
        has_xwayland_extension: true,
        vendor: Some("The X.Org Foundation".to_owned()),
        xft_dpi: Some(-12.0),
        display_height_px: 1080,
        display_height_mm: 0,
    }
    .validate(DeviceScale::ONE);

    assert_eq!(observation.server(), XServerKind::Xwayland);
    assert_eq!(observation.xft_dpi(), None);
    assert_eq!(observation.geometry(), None);
}

#[test]
fn x11_adapter_preserves_valid_raw_facts_without_applying_policy() {
    let observation = RawX11DisplayObservation {
        has_xwayland_extension: false,
        vendor: Some("The X.Org Foundation".to_owned()),
        xft_dpi: Some(144.0),
        display_height_px: 1080,
        display_height_mm: 800,
    }
    .validate(DeviceScale::ONE);

    assert_eq!(observation.server(), XServerKind::Xorg);
    assert_eq!(observation.xft_dpi().map(|dpi| dpi.get()), Some(144.0));
    let geometry = observation.geometry().expect("valid geometry");
    assert_eq!(geometry.height_px(), 1080);
    assert_eq!(geometry.height_mm(), 800);
}

#[test]
fn selected_x11_backend_carries_the_probe_result_into_the_observation() {
    let xwayland = X11DisplayObservation::new(XServerKind::Xwayland, None, None, DeviceScale::ONE);

    let observation = observe_linux_backend(SelectedLinuxBackend::X11, || xwayland);

    assert_eq!(observation, DisplayObservation::X11(xwayland));
}

#[test]
fn selected_wayland_backend_does_not_probe_x11() {
    let observation = observe_linux_backend(SelectedLinuxBackend::Wayland, || {
        panic!("Wayland selection must not open an X11 connection")
    });

    assert_eq!(
        observation,
        DisplayObservation::Wayland {
            device_scale: DeviceScale::ONE,
        }
    );
}

#[test]
fn slow_x11_probe_has_a_bounded_fallback() {
    let observation = query_x11_display_with_timeout(std::time::Duration::ZERO, || {
        std::thread::sleep(std::time::Duration::from_millis(10));
        X11DisplayObservation::new(XServerKind::Xwayland, None, None, DeviceScale::ONE)
    });

    assert_eq!(observation.server(), XServerKind::Unknown);
    assert_eq!(observation.xft_dpi(), None);
    assert_eq!(observation.geometry(), None);
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
