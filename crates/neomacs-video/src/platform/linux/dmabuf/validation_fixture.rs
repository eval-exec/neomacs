//! Validate actual DMA-BUF imports and plane views with an owned native producer.
//! Object-creation validation only: no submit, acquire, release, draw, or decode.
//! Feature-gated hardware fixture; inspects the actual prepared sampling views.
//! Missing hardware, validation layers, or artifact configuration fails explicitly.
use super::*;
use crate::sampling::GpuVideoContext;
use crate::{
    BiPlanarVideoFormat, GpuGeneration, VideoColorimetry, VideoGeometry, VideoSamplingResources,
    VideoWake,
};
use ash::vk::Handle;
use std::{
    ffi::{CStr, c_void},
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    path::PathBuf,
    pin::pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";
const MODIFIER: u64 = 0; // DRM_FORMAT_MOD_LINEAR; support must be queried.
const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

// Missing prerequisites fail this explicitly selected hardware tier.
// A successful return requires creation of both real consumer plane views.
fn supported<T, E: std::fmt::Debug>(result: Result<T, E>, what: &str) -> T {
    result.unwrap_or_else(|error| panic!("UNSUPPORTED: {what}: {error:?}"))
}

struct ValidationLog {
    file: PathBuf,
    records: Mutex<Vec<(vk::DebugUtilsMessageSeverityFlagsEXT, String)>>,
}

impl ValidationLog {
    fn record(&self, severity: vk::DebugUtilsMessageSeverityFlagsEXT, message: String) {
        // No Vulkan calls while holding this lock; callbacks may arrive on
        // arbitrary threads. Recover poisoning so FFI never unwinds here.
        let mut records = self.records.lock().unwrap_or_else(|p| p.into_inner());
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
        {
            let _ = writeln!(file, "{severity:?}: {message}");
        }
        records.push((severity, message));
    }

    fn errors(&self) -> Vec<String> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(severity, _)| severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR))
            .map(|(_, message)| message.clone())
            .collect()
    }

    fn witness(&self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("{message}");
        self.record(vk::DebugUtilsMessageSeverityFlagsEXT::INFO, message);
    }
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut c_void,
) -> vk::Bool32 {
    // SAFETY: Arc owners keep the stable log alive through messenger/instance destruction;
    // Vulkan supplies callback data and strings for this call's duration.
    let _ = std::panic::catch_unwind(|| unsafe {
        if data.is_null() || user.is_null() {
            return;
        }
        let data = &*data;
        let id = if data.p_message_id_name.is_null() {
            "<no-id>".into()
        } else {
            CStr::from_ptr(data.p_message_id_name).to_string_lossy()
        };
        let text = if data.p_message.is_null() {
            "<no-message>".into()
        } else {
            CStr::from_ptr(data.p_message).to_string_lossy()
        };
        (&*user.cast::<ValidationLog>()).record(severity, format!("{id}: {text}"));
    });
    vk::FALSE
}

struct ValidatedInstance {
    instance: wgpu::Instance,
    debug: ash::ext::debug_utils::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
    log: Arc<ValidationLog>,
}

impl ValidatedInstance {
    fn new(format: BiPlanarVideoFormat) -> Self {
        let directory = PathBuf::from(
            std::env::var_os("NEOMACS_VIDEO_VALIDATION_ARTIFACTS")
                .expect("UNSUPPORTED: set owned NEOMACS_VIDEO_VALIDATION_ARTIFACTS"),
        );
        fs::create_dir_all(&directory).expect("validation artifacts");
        let file = directory.join(format!(
            "{format:?}-Shared-{}.validation.log",
            std::process::id()
        ));
        assert!(!file.exists(), "refuse stale validation log {file:?}");
        let log = Arc::new(ValidationLog {
            file,
            records: Mutex::new(Vec::new()),
        });
        // SAFETY: loading the installed Vulkan loader performs no rendering.
        let entry = supported(unsafe { ash::Entry::load() }, "Vulkan loader");
        let layers = supported(
            unsafe { entry.enumerate_instance_layer_properties() },
            "layer list",
        );
        let layer = layers
            .iter()
            .find(|layer| unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) == LAYER })
            .expect("UNSUPPORTED: actual VK_LAYER_KHRONOS_validation is missing");
        log.witness(format!(
            "enabled-layer-request={LAYER:?}; spec={:#x}; implementation={}",
            layer.spec_version, layer.implementation_version
        ));
        // Match the pinned HAL's normal instance request (instance.rs:614-627).
        // Forcing 1.2 while HAL sees a 1.3 device makes its internal SPIR-V1.6
        // shaders invalid before the import under test is reached.
        let api = vk::API_VERSION_1_3;
        let available_api = supported(
            unsafe { entry.try_enumerate_instance_version() },
            "instance API",
        )
        .unwrap_or(vk::API_VERSION_1_0);
        assert!(
            available_api >= api,
            "UNSUPPORTED: hardware tier requires Vulkan1.3"
        );
        let flags = wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION;
        let extensions = supported(
            wgpu_hal::vulkan::Instance::desired_extensions(&entry, api, flags),
            "wgpu required instance extensions",
        );
        assert!(
            extensions.contains(&ash::ext::debug_utils::NAME),
            "UNSUPPORTED: debug_utils"
        );
        let extension_names: Vec<_> = extensions.iter().map(|name| name.as_ptr()).collect();
        let layer_names = [LAYER.as_ptr()];
        let application = vk::ApplicationInfo::default().api_version(api);
        // Explicit raw instance creation proves the layer was ENABLED, not just
        // enumerated. It must fail rather than let wgpu silently drop validation.
        let mut messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback))
            .user_data(Arc::as_ptr(&log).cast_mut().cast());
        let info = vk::InstanceCreateInfo::default()
            .application_info(&application)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&extension_names)
            .push_next(&mut messenger_info);
        let raw = supported(
            unsafe { entry.create_instance(&info, None) },
            "validated instance",
        );
        assert!(
            log.records
                .lock()
                .unwrap()
                .iter()
                .any(
                    |(_, message)| message.contains("WARNING-CreateInstance-status-message")
                        && message.contains("Khronos Validation Layer Active:")
                        && message.contains("Current Disables: None.")
                ),
            "UNSUPPORTED: validation layer did not attest active instance interception"
        );
        let debug = ash::ext::debug_utils::Instance::new(&entry, &raw);
        let messenger = supported(
            unsafe { debug.create_debug_utils_messenger(&messenger_info, None) },
            "validation messenger",
        );
        let message = vk::DebugUtilsMessengerCallbackDataEXT::default()
            .message_id_name(c"neomacs-validation-callback-witness")
            .message(c"owned debug-utils callback is live");
        unsafe {
            debug.submit_debug_utils_message(
                vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL,
                &message,
            );
        }
        assert!(
            log.records
                .lock()
                .unwrap()
                .iter()
                .any(|(_, message)| message.contains("neomacs-validation-callback-witness")),
            "debug callback must really run"
        );
        // SAFETY: extensions are the full desired set, API/flags match creation,
        // raw comes from entry; HAL takes instance ownership. Our messenger is
        // destroyed in Drop BEFORE the wgpu instance field is dropped.
        let hal = supported(
            unsafe {
                wgpu_hal::vulkan::Instance::from_raw(
                    entry,
                    raw,
                    api,
                    0,
                    None,
                    extensions,
                    flags,
                    Default::default(),
                    false,
                    None,
                )
            },
            "wgpu instance wrapping",
        );
        let instance = unsafe { wgpu::Instance::from_hal::<wgpu_hal::api::Vulkan>(hal) };
        Self {
            instance,
            debug,
            messenger,
            log,
        }
    }
}

impl Drop for ValidatedInstance {
    fn drop(&mut self) {
        // SAFETY: the instance field still owns this instance, callbacks are
        // finished by messenger destruction, and Arc-owned user data is still live.
        // The caller retains another Arc through raw instance destruction,
        // including the creation-time pNext callback used for teardown.
        unsafe {
            self.debug
                .destroy_debug_utils_messenger(self.messenger, None);
        }
    }
}

struct Producer {
    // This explicit wgpu owner outlives all raw producer Vulkan allocations.
    _device: wgpu::Device,
    raw: ash::Device,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    surface: Option<DmaBufSurface>,
}

impl Drop for Producer {
    fn drop(&mut self) {
        // No commands using this image are ever submitted by this harness.
        unsafe {
            self.raw.destroy_image(self.image, None);
            for memory in self.memories.drain(..) {
                self.raw.free_memory(memory, None);
            }
        }
    }
}

fn producer(device: &wgpu::Device, fourcc: u32, log: &ValidationLog) -> Producer {
    let (format, _) = sampled_format(fourcc).expect("known fixture format");
    let views = match fourcc {
        0x3231_564e => [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM],
        0x3031_3050 => [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM],
        _ => unreachable!(),
    };
    let flags = vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;
    let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
    let hal = unsafe { device.as_hal::<wgpu_hal::api::Vulkan>() }.expect("Vulkan device");
    let raw = hal.raw_device();
    let instance = hal.shared_instance().raw_instance();
    let physical = hal.raw_physical_device();
    let api = unsafe { instance.get_physical_device_properties(physical) }
        .api_version
        .min(hal.shared_instance().instance_api_version());
    assert!(
        api >= vk::API_VERSION_1_2,
        "UNSUPPORTED: effective API below fixture1.2"
    );
    // image-format-list is CORE1.2 and extended-usage CORE1.1 here. Requiring
    // their old extension names would incorrectly reject promoted support.
    for extension in [
        ash::khr::external_memory_fd::NAME,
        ash::ext::external_memory_dma_buf::NAME,
        ash::ext::image_drm_format_modifier::NAME,
        ash::ext::queue_family_foreign::NAME,
    ] {
        assert!(
            hal.enabled_device_extensions().contains(&extension),
            "UNSUPPORTED: device extension {extension:?} not enabled"
        );
    }
    let modifier = unsafe { modifier_properties(instance, physical, format, MODIFIER) }
        .expect("UNSUPPORTED: LINEAR0 modifier absent for this actual YUV format");
    let planes = modifier.drm_format_modifier_plane_count;
    assert_eq!(
        planes, 2,
        "UNSUPPORTED: fixture supports exactly two LINEAR memory planes"
    );
    let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(MODIFIER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut view_list = vk::ImageFormatListCreateInfo::default().view_formats(&views);
    let query = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .flags(flags)
        .push_next(&mut drm)
        .push_next(&mut external)
        .push_next(&mut view_list);
    let mut external_result = vk::ExternalImageFormatProperties::default();
    let mut result = vk::ImageFormatProperties2::default().push_next(&mut external_result);
    supported(
        unsafe {
            instance.get_physical_device_image_format_properties2(physical, &query, &mut result)
        },
        "exact LINEAR mutable/extended/view-list/DMA_BUF image tuple",
    );
    let properties = result.image_format_properties;
    assert!(
        properties.max_extent.width >= WIDTH
            && properties.max_extent.height >= HEIGHT
            && properties.max_array_layers >= 1
            && properties.max_mip_levels >= 1
            && properties
                .sample_counts
                .contains(vk::SampleCountFlags::TYPE_1),
        "UNSUPPORTED: fixture extent"
    );
    let external = external_result.external_memory_properties;
    assert!(
        external.external_memory_features.contains(
            vk::ExternalMemoryFeatureFlags::IMPORTABLE | vk::ExternalMemoryFeatureFlags::EXPORTABLE
        ) && external
            .compatible_handle_types
            .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
        "UNSUPPORTED: exact tuple is not both DMA_BUF importable/exportable"
    );
    log.witness(format!("producer-support binding=Shared format={format:?} modifier={MODIFIER:#x} flags={flags:?} \
        usage={usage:?} plane-view-list={views:?} effective-api={api:#x} properties={properties:?} external={external:?}"));
    let modifiers = [MODIFIER];
    let mut drm_create =
        vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(&modifiers);
    let mut external_create = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut view_list = vk::ImageFormatListCreateInfo::default().view_formats(&views);
    let info = vk::ImageCreateInfo::default()
        .flags(flags)
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: WIDTH,
            height: HEIGHT,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut drm_create)
        .push_next(&mut external_create)
        .push_next(&mut view_list);
    let image = supported(
        unsafe { raw.create_image(&info, None) },
        "valid export producer image",
    );
    // Own the image before any fallible allocation/binding/export step.
    let mut producer = Producer {
        _device: device.clone(),
        raw: raw.clone(),
        image,
        memories: Vec::new(),
        surface: None,
    };
    let aspects = [
        vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
    ];
    let requirement = unsafe { raw.get_image_memory_requirements(image) };
    let memory_type = requirement.memory_type_bits.trailing_zeros();
    assert!(memory_type < 32, "UNSUPPORTED: producer memory type");
    let mut export = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocation = vk::MemoryAllocateInfo::default()
        .allocation_size(requirement.size)
        .memory_type_index(memory_type)
        .push_next(&mut export)
        .push_next(&mut dedicated);
    producer.memories.push(supported(
        unsafe { raw.allocate_memory(&allocation, None) },
        "export shared producer memory",
    ));
    supported(
        unsafe { raw.bind_image_memory(image, producer.memories[0], 0) },
        "bind shared export producer",
    );
    let external_fd = ash::khr::external_memory_fd::Device::new(instance, raw);
    let objects = producer
        .memories
        .iter()
        .map(|&memory| {
            let info = vk::MemoryGetFdInfoKHR::default()
                .memory(memory)
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
            let fd = unsafe {
                OwnedFd::from_raw_fd(supported(
                    external_fd.get_memory_fd(&info),
                    "export DMA_BUF fd",
                ))
            };
            super::super::frame::DmaBufObject {
                fd,
                modifier: MODIFIER,
            }
        })
        .collect();
    let mut planes = Vec::new();
    for aspect in aspects {
        let subresource = vk::ImageSubresource::default()
            .aspect_mask(aspect)
            .mip_level(0)
            .array_layer(0);
        let layout = unsafe { raw.get_image_subresource_layout(image, subresource) };
        planes.push(super::super::frame::DmaBufPlane {
            object_index: 0,
            stride: u32::try_from(layout.row_pitch).expect("producer stride fits API"),
            offset: u32::try_from(layout.offset).expect("producer offset fits API"),
        });
        log.witness(format!(
            "producer-plane aspect={aspect:?} layout={layout:?}"
        ));
    }
    producer.surface = Some(DmaBufSurface {
        objects,
        planes,
        fourcc,
    });
    producer
}

pub(super) fn exercise(format: BiPlanarVideoFormat) {
    let validated = ValidatedInstance::new(format);
    // This owner outlives all Vulkan/wgpu teardown, including the instance's
    // creation-time debug callback. Inspect errors only after every owner drops.
    let log = Arc::clone(&validated.log);
    let adapter = supported(
        block_on(
            validated
                .instance
                .request_adapter(&wgpu::RequestAdapterOptions::default()),
        ),
        "Vulkan adapter",
    );
    validated
        .log
        .witness(format!("adapter={:?}", adapter.get_info()));
    let (fourcc, features, expected_views) = match format {
        BiPlanarVideoFormat::Nv12 => (
            0x3231_564e,
            wgpu::Features::TEXTURE_FORMAT_NV12,
            [wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm],
        ),
        BiPlanarVideoFormat::P010 => (
            0x3031_3050,
            wgpu::Features::TEXTURE_FORMAT_P010 | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM,
            [
                wgpu::TextureFormat::R16Unorm,
                wgpu::TextureFormat::Rg16Unorm,
            ],
        ),
    };
    assert!(
        adapter.features().contains(features),
        "UNSUPPORTED: requested {features:?}"
    );
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("native DMA-BUF validation fixture"),
        required_features: features,
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    };
    // Mirror the renderer's interop device setup. An ordinary wgpu request
    // omits queue_family_foreign even when the physical device supports it.
    // SAFETY: this is the exact adapter being adopted below. Only supported
    // extensions are added; every wgpu-required extension/feature is retained.
    let (device, queue) = unsafe {
        let hal = adapter
            .as_hal::<wgpu_hal::api::Vulkan>()
            .expect("Vulkan adapter");
        let extensions = [
            ash::khr::external_memory_fd::NAME,
            ash::ext::external_memory_dma_buf::NAME,
            ash::ext::image_drm_format_modifier::NAME,
            ash::ext::queue_family_foreign::NAME,
        ];
        for extension in extensions {
            assert!(
                hal.physical_device_capabilities()
                    .supports_extension(extension),
                "UNSUPPORTED: physical device extension {extension:?}"
            );
        }
        let opened = supported(
            hal.open_with_callback(
                features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(Box::new(|args| {
                    for extension in extensions {
                        if !args.extensions.contains(&extension) {
                            args.extensions.push(extension);
                        }
                    }
                })),
            ),
            "Vulkan interop device",
        );
        supported(
            adapter.create_device_from_hal(opened, &descriptor),
            "wgpu interop device adoption",
        )
    };
    let producer = producer(&device, fourcc, &log);
    let disjoint_fds = {
        let fds: Vec<_> = producer
            .surface
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .map(|object| object.fd.as_fd())
            .collect();
        unsafe { fds_are_disjoint(&fds) }
    };
    assert_eq!(
        disjoint_fds, false,
        "producer must exercise the actual shared import binding path"
    );
    assert!(
        validated.log.errors().is_empty(),
        "producer must be independently valid: {:?}",
        validated.log.errors()
    );
    validated
        .log
        .witness("PRODUCER_VALID; arming actual production import/plane-view consumer");
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("unused packed video fixture layout"),
        entries: &[],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("production video plane filtering sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let gpu = GpuVideoContext::with_sampling_resources(
        device.clone(),
        queue.clone(),
        VideoSamplingResources::new(&device, &layout, &sampler),
        GpuGeneration::INITIAL,
        VideoWake::noop(),
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (texture, imported) = import_dmabuf(
            &device,
            &queue,
            producer.surface.as_ref().expect("completed producer"),
            VideoGeometry::packed(WIDTH, HEIGHT),
            crate::VideoFrameFormat::BiPlanar420(format),
        )
        .expect("supported real DMA-BUF must import");
        let identity = texture.clone();
        let prepared = gpu
            .prepare_multi_planar_texture(
                texture,
                format,
                VideoColorimetry::BT709_LIMITED,
                VideoGeometry::packed(WIDTH, HEIGHT),
            )
            .expect("actual production multi-planar preparation");
        // The feature-gated hook borrows the ACTUAL prepared plane views,
        // not reconstructed validation views or fabricated descriptors.
        let views = prepared.native_plane_views_for_validation();
        let mut handles = Vec::new();
        for (plane, (view, expected)) in views.into_iter().zip(expected_views).enumerate() {
            assert_eq!(
                view.texture(),
                &identity,
                "both planes alias the actual imported texture"
            );
            let hal =
                unsafe { view.as_hal::<wgpu_hal::api::Vulkan>() }.expect("real Vulkan plane view");
            let raw = unsafe { hal.raw_handle() };
            assert_ne!(raw, vk::ImageView::null());
            handles.push(raw);
            validated.log.witness(format!(
                "consumer-plane-view plane={plane} expected-format={expected:?} raw={:#x}",
                raw.as_raw()
            ));
        }
        assert_ne!(handles[0], handles[1], "two distinct actual plane views");
        drop(prepared);
        drop(identity);
        drop(imported);
    }));
    // All consumer owners drop before producer destruction and Device stays
    // alive throughout. No queue submissions or unfinished retirements exist.
    drop(gpu);
    drop(producer);
    drop(sampler);
    drop(layout);
    drop(queue);
    drop(device);
    drop(adapter);
    drop(validated);
    let errors = log.errors();
    assert!(
        result.is_ok(),
        "consumer panicked; validation errors: {errors:#?}"
    );
    assert!(
        errors.is_empty(),
        "native plane import/teardown violated Vulkan: {errors:#?}"
    );
    log.witness("CONSUMER_VALID; both real plane views created and all owners destroyed; zero validation errors");
}

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(
                    Instant::now() < deadline,
                    "UNSUPPORTED: adapter/device request timed out"
                );
                std::thread::park_timeout(Duration::from_millis(20));
            }
        }
    }
}
