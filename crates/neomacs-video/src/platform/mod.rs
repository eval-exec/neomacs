// Compile time chooses exactly one native adapter. Codec, decoder, format,
// device, and import capabilities remain runtime decisions inside that
// adapter; a target OS alone is not evidence of a zero-copy path.
std::cfg_select! {
    target_os = "linux" => {
        mod linux;
        pub(crate) use linux::LinuxPlatform as CurrentPlatform;
        #[cfg(feature = "native-validation-fixtures")]
        pub(crate) use linux::assert_native_dmabuf_plane_validation;
    }
    target_os = "macos" => {
        mod macos;
        pub(crate) use macos::MacPlatform as CurrentPlatform;
    }
    windows => {
        mod windows;
        pub(crate) use windows::WindowsPlatform as CurrentPlatform;
    }
    _ => {
        mod unsupported;
        pub(crate) use unsupported::UnsupportedPlatform as CurrentPlatform;
    }
}
