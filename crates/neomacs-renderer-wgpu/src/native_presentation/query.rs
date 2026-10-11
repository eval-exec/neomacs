//! Bounded native timing queries. Every raw handle is borrowed from a live HAL
//! guard; reconfiguration is detected by both handle and owner generation.
use super::{abi, clock::Calibration};
use ash::vk;
use wgpu::hal::api::Vulkan;

const CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub present_id: u64,
    pub monotonic_ns: u64,
    pub uncertainty_ns: u64,
}

pub struct SwapchainTiming {
    swapchain: vk::SwapchainKHR,
    generation: u64,
    domain: vk::TimeDomainKHR,
    domain_id: u64,
    counter: u64,
    outstanding: std::collections::HashSet<u64>,
    last_id: u64,
}

struct Functions {
    set_size: abi::SetQueueSize,
    domains: abi::GetDomains,
    past: abi::GetPast,
}
impl Functions {
    unsafe fn load(device: &wgpu::hal::vulkan::Device) -> Option<Self> {
        let instance = device.shared_instance().raw_instance();
        let raw = device.raw_device().handle();
        // SAFETY: extension support is checked before loading; each symbol has
        // the canonical Vulkan-Headers signature declared in abi.rs.
        unsafe {
            Some(Self {
                set_size: std::mem::transmute::<unsafe extern "system" fn(), abi::SetQueueSize>(
                    instance.get_device_proc_addr(
                        raw,
                        c"vkSetSwapchainPresentTimingQueueSizeEXT".as_ptr(),
                    )?,
                ),
                domains: std::mem::transmute::<unsafe extern "system" fn(), abi::GetDomains>(
                    instance.get_device_proc_addr(
                        raw,
                        c"vkGetSwapchainTimeDomainPropertiesEXT".as_ptr(),
                    )?,
                ),
                past: std::mem::transmute::<unsafe extern "system" fn(), abi::GetPast>(
                    instance
                        .get_device_proc_addr(raw, c"vkGetPastPresentationTimingEXT".as_ptr())?,
                ),
            })
        }
    }
}

impl SwapchainTiming {
    /// # Safety
    /// The surface must currently be configured with this device; the caller
    /// owns reconfiguration and assigns a unique generation to every configure.
    pub unsafe fn new(
        device: &wgpu::Device,
        surface: &wgpu::Surface<'_>,
        generation: u64,
    ) -> Option<Self> {
        // SAFETY: queries operate synchronously under live device/surface guards.
        unsafe {
            let device = device.as_hal::<Vulkan>()?;
            let surface = surface.as_hal::<Vulkan>()?;
            if !super::enabled(&device) || !surface.present_timing_ext_enabled() {
                return None;
            }
            let swapchain = surface.raw_native_swapchain()?;
            let functions = Functions::load(&device)?;
            let mut domains = abi::Domains::default();
            let mut counter = 0;
            if (functions.domains)(
                device.raw_device().handle(),
                swapchain,
                &mut domains,
                &mut counter,
            ) != vk::Result::SUCCESS
                || domains.count == 0
                || domains.count > 16
            {
                return None;
            }
            let mut types = [vk::TimeDomainKHR::DEVICE; 16];
            let mut ids = [0; 16];
            domains.domains = types.as_mut_ptr();
            domains.ids = ids.as_mut_ptr();
            if (functions.domains)(
                device.raw_device().handle(),
                swapchain,
                &mut domains,
                &mut counter,
            ) != vk::Result::SUCCESS
                || domains.count > 16
            {
                return None;
            }
            let index = types[..domains.count as usize].iter().position(|domain| {
                domain.as_raw() == abi::SWAPCHAIN_LOCAL
                    || *domain == vk::TimeDomainKHR::CLOCK_MONOTONIC
            })?;
            let timing = Self {
                swapchain,
                generation,
                domain: types[index],
                domain_id: ids[index],
                counter,
                outstanding: Default::default(),
                last_id: 0,
            };
            // Verify CLOCK_MONOTONIC support before calibration. The extension
            // instance entry point only needs the physical-device capability.
            let shared = device.shared_instance();
            let calibration = ash::khr::calibrated_timestamps::Instance::new(
                shared.entry(),
                shared.raw_instance(),
            );
            if !calibration
                .get_physical_device_calibrateable_time_domains(device.raw_physical_device())
                .ok()?
                .contains(&vk::TimeDomainKHR::CLOCK_MONOTONIC)
            {
                return None;
            }
            timing.calibrate(&device)?;
            if (functions.set_size)(device.raw_device().handle(), swapchain, CAPACITY as u32)
                != vk::Result::SUCCESS
            {
                return None;
            }
            Some(timing)
        }
    }

    pub fn matches(&self, surface: &wgpu::Surface<'_>, generation: u64) -> bool {
        // SAFETY: read-only identity lookup; no raw operation on saved handles.
        self.generation == generation
            && unsafe { surface.as_hal::<Vulkan>() }
                .is_some_and(|surface| surface.raw_native_swapchain() == Some(self.swapchain))
    }

    /// # Safety
    /// The surface and generation must identify the current configuration of
    /// the same surface/device passed to new, without concurrent reconfigure.
    pub unsafe fn request(
        &mut self,
        surface: &wgpu::Surface<'_>,
        generation: u64,
        id: u64,
    ) -> bool {
        if id <= self.last_id
            || self.outstanding.len() >= CAPACITY
            || !self.matches(surface, generation)
        {
            return false;
        }
        // SAFETY: new() verified the domain and allocated bounded queue capacity;
        // the caller prepared this surface before configure. IDs only increase.
        let accepted = unsafe { surface.as_hal::<Vulkan>() }.is_some_and(|surface| unsafe {
            surface.set_next_present_timing_ext(
                wgpu::hal::vulkan::present_timing::PresentTimingRequest {
                    present_id: id,
                    time_domain_id: self.domain_id,
                    stages: abi::FIRST_PIXEL_OUT,
                },
            )
        });
        if accepted {
            self.last_id = id;
            self.outstanding.insert(id);
        }
        accepted
    }

    unsafe fn calibrate(&self, device: &wgpu::hal::vulkan::Device) -> Option<Calibration> {
        let mut local = abi::SwapchainTimestamp {
            swapchain: self.swapchain,
            domain_id: self.domain_id,
            ..Default::default()
        };
        let mut infos = [
            vk::CalibratedTimestampInfoKHR::default()
                .time_domain(vk::TimeDomainKHR::CLOCK_MONOTONIC),
            vk::CalibratedTimestampInfoKHR::default().time_domain(self.domain),
        ];
        if self.domain.as_raw() == abi::SWAPCHAIN_LOCAL {
            infos[1].p_next = (&mut local as *mut abi::SwapchainTimestamp).cast();
        }
        let fun = ash::khr::calibrated_timestamps::Device::new(
            device.shared_instance().raw_instance(),
            device.raw_device(),
        );
        // SAFETY: both clock domains were queried; chained swapchain info lives
        // through the call, and the surface is borrowed by our caller.
        let (times, uncertainty_ns) = unsafe { fun.get_calibrated_timestamps(&infos) }.ok()?;
        let calibration = Calibration {
            monotonic: times[0],
            local: times[1],
            uncertainty_ns,
        };
        calibration.convert(times[1])?;
        Some(calibration)
    }

    /// None invalidates this session; no polling can turn unavailable evidence
    /// into a confirmed observation. Complete unknown IDs are never attributed.
    /// # Safety
    /// Device, surface and generation must describe their current shared
    /// configuration, without concurrent destruction or reconfiguration.
    pub unsafe fn poll(
        &mut self,
        device: &wgpu::Device,
        surface: &wgpu::Surface<'_>,
        generation: u64,
    ) -> Option<Vec<Observation>> {
        if !self.matches(surface, generation) {
            return None;
        }
        if self.outstanding.is_empty() {
            return Some(Vec::new());
        }
        // SAFETY: all C outputs have fixed, bounded capacity and stay stationary
        // throughout the synchronous call under live HAL guards.
        unsafe {
            let device = device.as_hal::<Vulkan>()?;
            let surface_guard = surface.as_hal::<Vulkan>()?;
            if surface_guard.raw_native_swapchain()? != self.swapchain {
                return None;
            }
            let functions = Functions::load(&device)?;
            let mut stages = [abi::StageTime::default(); CAPACITY];
            let mut timings: Vec<_> = stages
                .iter_mut()
                .map(|stage| abi::PastTiming {
                    stage_count: 1,
                    stages: stage,
                    ..Default::default()
                })
                .collect();
            let mut properties = abi::PastProperties {
                count: CAPACITY as u32,
                timings: timings.as_mut_ptr(),
                ..Default::default()
            };
            let info = abi::PastInfo {
                swapchain: self.swapchain,
                ..Default::default()
            };
            let result = (functions.past)(device.raw_device().handle(), &info, &mut properties);
            if ![vk::Result::SUCCESS, vk::Result::INCOMPLETE].contains(&result)
                || properties.count as usize > CAPACITY
                || properties.domains_counter != self.counter
            {
                return None;
            }
            if properties.count == 0 {
                return Some(Vec::new());
            }
            let calibration = self.calibrate(&device)?;
            // Calibration must refer to the same clock generation as the
            // completed record, even if the platform changed domains mid-poll.
            let mut domains = abi::Domains::default();
            let mut counter = 0;
            if (functions.domains)(
                device.raw_device().handle(),
                self.swapchain,
                &mut domains,
                &mut counter,
            ) != vk::Result::SUCCESS
                || counter != self.counter
            {
                return None;
            }
            self.decode(
                properties.domains_counter,
                properties.count as usize,
                &timings,
                &stages,
                calibration,
            )
        }
    }
    fn decode(
        &mut self,
        counter: u64,
        count: usize,
        timings: &[abi::PastTiming],
        stages: &[abi::StageTime],
        calibration: Calibration,
    ) -> Option<Vec<Observation>> {
        if counter != self.counter
            || count > CAPACITY
            || count > timings.len()
            || count > stages.len()
        {
            return None;
        }
        let mut observations = Vec::new();
        for (timing, stage) in timings.iter().zip(stages).take(count) {
            if timing.complete == 0 {
                continue;
            }
            let known = self.outstanding.remove(&timing.present_id);
            if !known
                || timing.domain != self.domain
                || timing.domain_id != self.domain_id
                || timing.stage_count != 1
                || stage.stage != abi::FIRST_PIXEL_OUT
            {
                continue;
            }
            if let Some(monotonic_ns) = calibration.convert(stage.time) {
                observations.push(Observation {
                    present_id: timing.present_id,
                    monotonic_ns,
                    uncertainty_ns: calibration.uncertainty_ns,
                });
            }
        }
        Some(observations)
    }
}

#[cfg(test)]
#[path = "tests/query_test.rs"]
mod tests;
