//! One renderer-device policy for every Neomacs GPU entry point.

#[cfg(target_os = "linux")]
pub(crate) const LINUX_DMA_BUF_EXTENSIONS: [&std::ffi::CStr; 4] = [
    ash::khr::external_memory_fd::NAME,
    ash::ext::external_memory_dma_buf::NAME,
    ash::ext::image_drm_format_modifier::NAME,
    ash::ext::queue_family_foreign::NAME,
];

fn requested_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let requested = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgpu::Features::TEXTURE_FORMAT_NV12
        | wgpu::Features::TEXTURE_FORMAT_P010;
    // Shadowed rather than `mut`: only the `video` build adds a feature, and a
    // `mut` binding is an unused-mut warning in every other build.
    #[cfg(feature = "video")]
    let requested = if std::env::var_os("NEOMACS_GPU_FRAME_TIMING").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        requested | wgpu::Features::TIMESTAMP_QUERY
    } else {
        requested
    };
    adapter.features() & requested
}

/// Install the uncaptured-error handler for a freshly created device.
///
/// wgpu's default handler panics for every uncaptured error ("Handling wgpu
/// errors as fatal by default").  A lost presentation surface is a RUNTIME
/// condition, not a programming error: the compositor can exit, a monitor can
/// be unplugged, or a window can be destroyed while its final resize is still
/// queued.  Panicking there aborts the editor with status 101 instead of
/// letting winit report the dead display and take the display-loss exit path
/// that the `startup_failure` LoseDisplay scenarios assert (a non-zero exit,
/// never a panic).
///
/// Only surface acquisition/configure failures are tolerated, and they stay
/// visible at error level; every other uncaptured error keeps wgpu's fatal
/// behaviour, so a genuine validation bug still fails loudly instead of being
/// logged away.
fn install_error_handler(device: &wgpu::Device) {
    device.on_uncaptured_error(std::sync::Arc::new(|error| {
        let text = error.to_string();
        let surface_lost = text.contains("Surface::configure")
            || text.contains("Surface::get_current_texture")
            || text.contains("surface lost")
            || text.contains("SURFACE_LOST");
        if surface_lost {
            tracing::error!(%error, "wgpu surface error (logged, not fatal)");
        } else {
            panic!("wgpu error: {error}");
        }
    }));
}

fn with_error_handler((device, queue): (wgpu::Device, wgpu::Queue)) -> (wgpu::Device, wgpu::Queue) {
    install_error_handler(&device);
    (device, queue)
}

/// Create the renderer's device and queue with platform interop enabled when
/// the selected adapter can support it.
///
/// Keeping this policy here prevents standalone and windowed renderer entry
/// points from silently creating devices with different native-video
/// capabilities.
pub async fn request_renderer_device(
    adapter: &wgpu::Adapter,
    label: &'static str,
) -> Result<(wgpu::Device, wgpu::Queue), String> {
    let descriptor = wgpu::DeviceDescriptor {
        label: Some(label),
        required_features: requested_features(adapter),
        // Window surfaces are sized by the display, not by wgpu's default
        // 8192 cap, so request the adapter's own texture dimension limits.
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    };

    std::cfg_select! {
        target_os = "linux" => {
            if linux::supports_dma_buf_extensions(adapter) || crate::native_presentation::requested() {
                match linux::request_interop_device(adapter, &descriptor) {
                    Ok(device) => return Ok(with_error_handler(device)),
                    Err(error) => tracing::warn!(
                        %error,
                        "failed to enable Vulkan interop device extensions; falling back to a standard renderer device"
                    ),
                }
            } else {
                tracing::info!(
                    "Vulkan adapter does not expose the complete Linux DMA-BUF extension set"
                );
            }
        }
        _ => {}
    }

    adapter
        .request_device(&descriptor)
        .await
        .map(with_error_handler)
        .map_err(|error| format!("failed to create renderer device: {error}"))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::LINUX_DMA_BUF_EXTENSIONS;
    use wgpu::hal::api::Vulkan;

    pub(super) fn supports_dma_buf_extensions(adapter: &wgpu::Adapter) -> bool {
        if !cfg!(any(feature = "video", feature = "webview")) {
            return false;
        }
        // SAFETY: the guard is used only for immutable capability inspection
        // and is dropped before this function returns.
        unsafe { adapter.as_hal::<Vulkan>() }.is_some_and(|hal| {
            LINUX_DMA_BUF_EXTENSIONS.iter().all(|extension| {
                hal.physical_device_capabilities()
                    .supports_extension(extension)
            })
        })
    }

    pub(super) fn request_interop_device(
        adapter: &wgpu::Adapter,
        descriptor: &wgpu::DeviceDescriptor<'_>,
    ) -> Result<(wgpu::Device, wgpu::Queue), String> {
        // SAFETY: `hal_adapter` is obtained from this exact wgpu adapter. The
        // callback only adds extensions proven supported above, preserves all
        // wgpu-required features/extensions, and the resulting HAL device is
        // immediately returned to the same adapter for wgpu-core ownership.
        unsafe {
            let hal_adapter = adapter
                .as_hal::<Vulkan>()
                .ok_or_else(|| "selected renderer adapter is not Vulkan".to_owned())?;
            let dma_buf = supports_dma_buf_extensions(adapter);
            let mut timing = crate::native_presentation::DeviceFeatures::query(&hal_adapter);
            let hal_device = hal_adapter
                .open_with_callback(
                    descriptor.required_features,
                    &descriptor.required_limits,
                    &descriptor.memory_hints,
                    Some(Box::new(|mut args| {
                        if let Some(timing) = &mut timing {
                            timing.enable(&mut args);
                        }
                        for extension in LINUX_DMA_BUF_EXTENSIONS.into_iter().filter(|_| dma_buf) {
                            if !args.extensions.contains(&extension) {
                                args.extensions.push(extension);
                            }
                        }
                    })),
                )
                .map_err(|error| format!("failed to open Vulkan interop device: {error:?}"))?;
            adapter
                .create_device_from_hal(hal_device, descriptor)
                .map_err(|error| format!("failed to adopt Vulkan interop device: {error}"))
        }
    }
}
