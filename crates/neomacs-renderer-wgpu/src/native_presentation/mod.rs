//! Opt-in Vulkan presentation observations. No target times or pacing changes.
//! Native first-pixel output is distinct from physical display visibility.
mod abi;
mod clock;
mod query;

use ash::vk;
pub use clock::Calibration;
pub use query::{Observation, SwapchainTiming};
use wgpu::hal::{api::Vulkan, vulkan};

pub fn requested() -> bool {
    std::env::var_os("NEOMACS_GUI_PRESENTATION_RECEIPT").is_some()
        || std::env::var_os("NEOMACS_INPUT_LATENCY_FILE").is_some()
}

pub(crate) struct DeviceFeatures {
    timing: abi::TimingFeatures,
    ids: abi::IdFeatures,
}
impl DeviceFeatures {
    pub(crate) fn query(adapter: &vulkan::Adapter) -> Option<Self> {
        if adapter.shared_instance().instance_api_version() < vk::API_VERSION_1_1 {
            return None;
        }
        if !requested()
            || ![abi::TIMING, abi::ID2, ash::khr::calibrated_timestamps::NAME]
                .iter()
                .all(|ext| {
                    adapter
                        .physical_device_capabilities()
                        .supports_extension(ext)
                })
            || !adapter
                .shared_instance()
                .extensions()
                .contains(&ash::khr::get_surface_capabilities2::NAME)
        {
            return None;
        }
        let mut this = Self {
            timing: Default::default(),
            ids: Default::default(),
        };
        this.timing.p_next = (&mut this.ids as *mut abi::IdFeatures).cast();
        let mut features = vk::PhysicalDeviceFeatures2::default();
        features.p_next = (&mut this.timing as *mut abi::TimingFeatures).cast();
        // SAFETY: both extension structs and output feature storage remain live
        // during this query on the adapter's own physical device.
        unsafe {
            adapter
                .shared_instance()
                .raw_instance()
                .get_physical_device_features2(adapter.raw_physical_device(), &mut features);
        }
        if this.timing.present_timing == 0 || this.ids.present_id2 == 0 {
            return None;
        }
        this.timing.absolute = 0;
        this.timing.relative = 0;
        this.timing.p_next = std::ptr::null_mut();
        Some(this)
    }

    /// Storage must outlive vkCreateDevice, not just its setup callback.
    pub(crate) fn enable(&mut self, args: &mut vulkan::CreateDeviceCallbackArgs<'_, '_, '_>) {
        for extension in [abi::TIMING, abi::ID2, ash::khr::calibrated_timestamps::NAME] {
            if !args.extensions.contains(&extension) {
                args.extensions.push(extension);
            }
        }
        self.ids.p_next = args.create_info.p_next.cast_mut();
        self.timing.p_next = (&mut self.ids as *mut abi::IdFeatures).cast();
        args.create_info.p_next = (&self.timing as *const abi::TimingFeatures).cast();
    }
}

fn enabled(device: &vulkan::Device) -> bool {
    [abi::TIMING, abi::ID2, ash::khr::calibrated_timestamps::NAME]
        .iter()
        .all(|extension| device.enabled_device_extensions().contains(extension))
}

/// Call before each surface configure. Unsupported devices retain ordinary WSI.
///
/// # Safety
/// Device and surface must originate from the same Vulkan instance, and the
/// device must be the one used for the subsequent configure. Neither may be
/// reconfigured or destroyed concurrently with this call.
pub unsafe fn prepare_surface(device: &wgpu::Device, surface: &wgpu::Surface<'_>) -> bool {
    if !requested() {
        return false;
    }
    // SAFETY: immutable HAL guards borrow this live device/surface. No handle is
    // retained; native capability queries finish before either guard is dropped.
    unsafe {
        let Some(device) = device.as_hal::<Vulkan>() else {
            return false;
        };
        let Some(surface) = surface.as_hal::<Vulkan>() else {
            return false;
        };
        let _ = surface.set_present_timing_ext_enabled(false);
        if !enabled(&device) {
            return false;
        }
        let shared = device.shared_instance();
        if !shared
            .extensions()
            .contains(&ash::khr::get_surface_capabilities2::NAME)
        {
            return false;
        }
        let Some(raw_surface) = surface.raw_native_handle() else {
            return false;
        };
        let mut ids = abi::IdCapabilities::default();
        let mut timing = abi::TimingCapabilities::default();
        timing.p_next = (&mut ids as *mut abi::IdCapabilities).cast();
        let mut caps = vk::SurfaceCapabilities2KHR::default();
        caps.p_next = (&mut timing as *mut abi::TimingCapabilities).cast();
        let info = vk::PhysicalDeviceSurfaceInfo2KHR::default().surface(raw_surface);
        let query = ash::khr::get_surface_capabilities2::Instance::new(
            shared.entry(),
            shared.raw_instance(),
        );
        if query
            .get_physical_device_surface_capabilities2(
                device.raw_physical_device(),
                &info,
                &mut caps,
            )
            .is_err()
            || timing.timing == 0
            || ids.supported == 0
            || timing.stages & abi::FIRST_PIXEL_OUT == 0
        {
            return false;
        }
        surface.set_present_timing_ext_enabled(true)
    }
}
