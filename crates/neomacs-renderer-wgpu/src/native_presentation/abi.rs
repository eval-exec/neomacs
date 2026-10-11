//! Vulkan-Headers ABI for extensions newer than ash 0.38.
//! https://registry.khronos.org/vulkan/specs/latest/man/html/VK_EXT_present_timing.html
use ash::vk;
use std::ffi::c_void;

pub const TIMING: &std::ffi::CStr = c"VK_EXT_present_timing";
pub const ID2: &std::ffi::CStr = c"VK_KHR_present_id2";
pub const FIRST_PIXEL_OUT: u32 = 4;
pub const SWAPCHAIN_LOCAL: i32 = 1000208000;

macro_rules! structure {
    ($name:ident, $ty:expr, {$($field:ident: $field_ty:ty = $default:expr),* $(,)?}) => {
        #[repr(C)]
        pub struct $name {
            pub s_type: vk::StructureType,
            pub p_next: *mut c_void,
            $(pub $field: $field_ty,)*
        }
        impl Default for $name {
            fn default() -> Self {
                Self { s_type: vk::StructureType::from_raw($ty), p_next: std::ptr::null_mut(), $($field: $default,)* }
            }
        }
    }
}
structure!(TimingFeatures, 1000208000, {
    present_timing: vk::Bool32 = 0,
    absolute: vk::Bool32 = 0,
    relative: vk::Bool32 = 0,
});
structure!(IdFeatures, 1000479002, { present_id2: vk::Bool32 = 0 });
structure!(TimingCapabilities, 1000208008, {
    timing: vk::Bool32 = 0, absolute: vk::Bool32 = 0, relative: vk::Bool32 = 0,
    stages: u32 = 0,
});
structure!(IdCapabilities, 1000479000, { supported: vk::Bool32 = 0 });
structure!(Domains, 1000208002, {
    count: u32 = 0, domains: *mut vk::TimeDomainKHR = std::ptr::null_mut(),
    ids: *mut u64 = std::ptr::null_mut(),
});
structure!(SwapchainTimestamp, 1000208009, {
    swapchain: vk::SwapchainKHR = vk::SwapchainKHR::null(),
    stage: u32 = FIRST_PIXEL_OUT, domain_id: u64 = 0,
});
structure!(PastInfo, 1000208005, {
    flags: u32 = 2, // Allow out-of-order complete results, never partial reports.
    swapchain: vk::SwapchainKHR = vk::SwapchainKHR::null(),
});
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StageTime {
    pub stage: u32,
    pub time: u64,
}
structure!(PastTiming, 1000208007, {
    present_id: u64 = 0, target_time: u64 = 0, stage_count: u32 = 0,
    stages: *mut StageTime = std::ptr::null_mut(),
    domain: vk::TimeDomainKHR = vk::TimeDomainKHR::DEVICE,
    domain_id: u64 = 0, complete: vk::Bool32 = 0,
});
structure!(PastProperties, 1000208006, {
    timing_counter: u64 = 0, domains_counter: u64 = 0, count: u32 = 0,
    timings: *mut PastTiming = std::ptr::null_mut(),
});

pub type SetQueueSize = unsafe extern "system" fn(vk::Device, vk::SwapchainKHR, u32) -> vk::Result;
pub type GetDomains =
    unsafe extern "system" fn(vk::Device, vk::SwapchainKHR, *mut Domains, *mut u64) -> vk::Result;
pub type GetPast =
    unsafe extern "system" fn(vk::Device, *const PastInfo, *mut PastProperties) -> vk::Result;

#[cfg(test)]
#[path = "tests/abi_test.rs"]
mod tests;
