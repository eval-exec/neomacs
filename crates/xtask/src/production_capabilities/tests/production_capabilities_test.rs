use super::{
    CargoCapability, HostPlatform, PlatformCapabilities, ProductionVideoBackend,
    validate_capabilities,
};

#[test]
fn windows_helpers_belong_only_to_the_windows_product() {
    let windows_without_helpers = PlatformCapabilities {
        cargo_features: vec![],
        video_backend: ProductionVideoBackend::None,
    };
    assert!(validate_capabilities(HostPlatform::Windows, &windows_without_helpers).is_err());
    let windows = PlatformCapabilities {
        cargo_features: vec![CargoCapability::WindowsTools],
        video_backend: ProductionVideoBackend::None,
    };
    assert_eq!(
        validate_capabilities(HostPlatform::Windows, &windows),
        Ok(())
    );
    assert!(validate_capabilities(HostPlatform::Darwin, &windows).is_err());
    let linux_with_helpers = PlatformCapabilities {
        cargo_features: vec![CargoCapability::Video, CargoCapability::WindowsTools],
        video_backend: ProductionVideoBackend::LinkedGstreamer,
    };
    assert!(validate_capabilities(HostPlatform::Linux, &linux_with_helpers).is_err());
}

#[test]
fn platform_and_video_backend_must_describe_one_real_product() {
    let linux_without_video = PlatformCapabilities {
        cargo_features: Vec::new(),
        video_backend: ProductionVideoBackend::None,
    };
    assert!(validate_capabilities(HostPlatform::Linux, &linux_without_video).is_err());

    let darwin_with_gstreamer = PlatformCapabilities {
        cargo_features: vec![CargoCapability::Video],
        video_backend: ProductionVideoBackend::LinkedGstreamer,
    };
    assert!(validate_capabilities(HostPlatform::Darwin, &darwin_with_gstreamer).is_err());

    let linux_full = PlatformCapabilities {
        cargo_features: vec![CargoCapability::Video],
        video_backend: ProductionVideoBackend::LinkedGstreamer,
    };
    assert_eq!(
        validate_capabilities(HostPlatform::Linux, &linux_full),
        Ok(())
    );
}
