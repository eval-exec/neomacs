use super::{
    DmaBufMemoryLayout, NativeVideoFormatSupport, ParsedDrmFormat, PipelineDrmIdentity,
    PipelineDrmTopology, classify_pipeline_error, dma_buf_transfer_path, frame_format_from_fourcc,
    missing_video_plugin, preferred_sink_caps, retain_unready_decoder_writes,
    rotation_from_gstreamer_tag,
};
use crate::sampling::LinuxDrmDevice;
use crate::{
    FrameTransferPolicy, LoopMode, MissingVideoPlugin, MissingVideoPlugins, VideoCommandError,
    VideoInstallerHint, VideoRotation, VideoTransferPath,
};
use std::num::NonZeroU32;

#[test]
fn finite_loop_count_means_additional_replays() {
    let mut mode = LoopMode::Count(NonZeroU32::new(2).unwrap());
    assert!(mode.consume_replay());
    assert_eq!(mode, LoopMode::Count(NonZeroU32::new(1).unwrap()));
    assert!(mode.consume_replay());
    assert_eq!(mode, LoopMode::Off);
    assert!(!mode.consume_replay());
}

#[test]
fn missing_plugin_diagnostics_win_over_the_generic_pipeline_error() {
    let plugins = MissingVideoPlugins::new(MissingVideoPlugin::new(
        "H.264 decoder",
        Some(VideoInstallerHint::gstreamer(
            "gstreamer|1.0|neomacs|H.264 decoder|decoder-video/x-h264",
        )),
    ));

    assert_eq!(
        classify_pipeline_error(
            Some(plugins.clone()),
            "streaming stopped, reason not-linked"
        ),
        VideoCommandError::MissingPlugins { plugins }
    );
}

#[test]
fn gstreamer_missing_plugin_messages_preserve_the_installer_token() {
    gstreamer::init().unwrap();
    let caps = gstreamer::Caps::builder("video/x-neomacs-test-codec").build();
    let source = gstreamer::ElementFactory::make("fakesrc").build().unwrap();
    let message = gstreamer_pbutils::MissingPluginMessage::builder_for_decoder(&caps)
        .src(&source)
        .build();

    let plugin = missing_video_plugin(&message).expect("recognize GStreamer pbutils message");
    assert!(!plugin.description().is_empty());
    assert!(matches!(
        plugin.installer_hint(),
        Some(VideoInstallerHint::GStreamer { detail })
            if detail.contains("video/x-neomacs-test-codec")
    ));
}

#[test]
fn dmabuf_plane_memory_layout_accepts_shared_or_complete_descriptors_only() {
    let shared = DmaBufMemoryLayout::classify(1, 3).unwrap();
    assert_eq!(shared.memory_index(0), 0);
    assert_eq!(shared.memory_index(2), 0);

    let per_plane = DmaBufMemoryLayout::classify(3, 3).unwrap();
    assert_eq!(per_plane.memory_index(0), 0);
    assert_eq!(per_plane.memory_index(2), 2);

    assert!(DmaBufMemoryLayout::classify(2, 3).is_err());
}

#[test]
fn dmabuf_wait_requires_every_memory_object_to_finish() {
    let mut pending = vec![
        libc::pollfd {
            fd: 11,
            events: libc::POLLIN,
            revents: libc::POLLIN,
        },
        libc::pollfd {
            fd: 12,
            events: libc::POLLIN,
            revents: 0,
        },
    ];

    assert!(!retain_unready_decoder_writes(&mut pending).unwrap());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].fd, 12);

    pending[0].revents = libc::POLLIN;
    assert!(retain_unready_decoder_writes(&mut pending).unwrap());
    assert!(pending.is_empty());
}

#[test]
fn packed_dmabuf_fallback_retains_its_srgb_contract() {
    gstreamer::init().unwrap();
    let caps = preferred_sink_caps(
        FrameTransferPolicy::AllowCpuUpload,
        NativeVideoFormatSupport {
            nv12: true,
            p010: true,
        },
    );

    assert_eq!(caps.size(), 5);
    assert!(
        caps.structure(0)
            .unwrap()
            .get::<String>("colorimetry")
            .is_err()
    );
    assert_eq!(
        caps.structure(2)
            .unwrap()
            .get::<String>("colorimetry")
            .unwrap(),
        "sRGB"
    );
    assert_eq!(
        caps.structure(3)
            .unwrap()
            .get::<String>("colorimetry")
            .unwrap(),
        "sRGB"
    );
    assert_eq!(
        caps.structure(4)
            .unwrap()
            .get::<String>("colorimetry")
            .unwrap(),
        "sRGB"
    );
}

#[test]
fn sink_caps_offer_only_native_formats_the_renderer_can_sample() {
    gstreamer::init().unwrap();
    let caps = preferred_sink_caps(
        FrameTransferPolicy::AllowGpuInteropCopy,
        NativeVideoFormatSupport {
            nv12: true,
            p010: false,
        },
    );
    let formats = caps
        .structure(0)
        .unwrap()
        .get::<gstreamer::List>("drm-format")
        .unwrap();
    let formats: Vec<_> = formats
        .iter()
        .map(|format| format.get::<String>().unwrap())
        .collect();

    assert_eq!(formats, ["NV12"]);
    let legacy_formats = caps
        .structure(1)
        .unwrap()
        .get::<gstreamer::List>("format")
        .unwrap();
    let legacy_formats: Vec<_> = legacy_formats
        .iter()
        .map(|format| format.get::<String>().unwrap())
        .collect();

    assert_eq!(legacy_formats, ["NV12"]);
    assert_eq!(caps.size(), 4);

    let packed_only = preferred_sink_caps(
        FrameTransferPolicy::AllowGpuInteropCopy,
        NativeVideoFormatSupport {
            nv12: false,
            p010: false,
        },
    );
    assert_eq!(packed_only.size(), 2);
    assert_eq!(
        packed_only
            .structure(0)
            .unwrap()
            .get::<String>("colorimetry")
            .unwrap(),
        "sRGB"
    );
}

#[test]
fn dma_drm_caps_parser_accepts_linear_and_modified_supported_formats() {
    assert_eq!(
        ParsedDrmFormat::parse("NV12").unwrap(),
        ParsedDrmFormat {
            fourcc: 0x3231_564e,
            modifier: 0,
        }
    );
    assert_eq!(
        ParsedDrmFormat::parse("P010:0x0100000000000002").unwrap(),
        ParsedDrmFormat {
            fourcc: 0x3031_3050,
            modifier: 0x0100_0000_0000_0002,
        }
    );
    assert!(ParsedDrmFormat::parse("YUYV").is_err());
    assert!(ParsedDrmFormat::parse("NV12:not-hex").is_err());
}

#[test]
fn gstreamer_orientation_tag_enters_the_common_sampling_transform() {
    assert_eq!(
        rotation_from_gstreamer_tag("rotate-90"),
        VideoRotation::Clockwise90
    );
    assert_eq!(
        rotation_from_gstreamer_tag("rotate-270"),
        VideoRotation::Clockwise270
    );
    assert_eq!(
        rotation_from_gstreamer_tag("flip-rotate-90"),
        VideoRotation::None,
        "mirrored orientation must not be mislabeled as a pure rotation"
    );
}

#[test]
fn native_planes_are_direct_only_on_one_proven_physical_gpu() {
    let renderer = LinuxDrmDevice::from_device_numbers(226, 128);
    let same_decoder = LinuxDrmDevice::from_device_numbers(226, 128);
    let other_decoder = LinuxDrmDevice::from_device_numbers(226, 129);

    assert_eq!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Single(same_decoder),
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::Packed(crate::PackedVideoFormat::Bgra8),
        )
        .unwrap(),
        VideoTransferPath::GpuInteropCopy,
        "the packed sink can require a native GPU colorspace conversion"
    );
    assert_eq!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Single(same_decoder),
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .unwrap(),
        VideoTransferPath::DirectExternalSurface,
    );
    assert_eq!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Single(same_decoder),
                postprocess: true,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .unwrap(),
        VideoTransferPath::GpuInteropCopy,
        "a same-adapter postprocessor still performs a GPU conversion"
    );
    assert!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(other_decoder),
                surface_path: PipelineDrmIdentity::Single(other_decoder),
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .is_err(),
        "a proven cross-adapter DMA-BUF must not be imported by the renderer"
    );
    assert!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Conflict,
                surface_path: PipelineDrmIdentity::Conflict,
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .is_err(),
        "a pipeline that reports multiple DRM devices is a proven conflict"
    );
    assert_eq!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology::UNKNOWN,
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .unwrap(),
        VideoTransferPath::GpuInteropCopy
    );
    assert_eq!(
        dma_buf_transfer_path(
            None,
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Single(same_decoder),
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .unwrap(),
        VideoTransferPath::GpuInteropCopy
    );

    assert!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Conflict,
                postprocess: false,
                inspection_failed: false,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .is_err(),
        "a same-GPU decoder cannot hide a cross-GPU packed-surface producer"
    );
    assert!(
        dma_buf_transfer_path(
            Some(renderer),
            PipelineDrmTopology {
                decoder: PipelineDrmIdentity::Single(same_decoder),
                surface_path: PipelineDrmIdentity::Single(same_decoder),
                postprocess: false,
                inspection_failed: true,
            },
            crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12),
        )
        .is_err(),
        "an incomplete topology inspection must fail closed"
    );
}

#[test]
fn non_drm_character_devices_are_not_physical_gpu_identities() {
    assert!(LinuxDrmDevice::from_path(std::path::Path::new("/dev/null")).is_none());
}

#[test]
fn opaque_xrgb_dmabufs_do_not_enter_the_alpha_sampling_pipeline() {
    assert_eq!(
        frame_format_from_fourcc(0x3432_5241).unwrap(),
        crate::VideoFrameFormat::Packed(crate::PackedVideoFormat::Bgra8)
    );
    assert_eq!(
        frame_format_from_fourcc(0x3432_4241).unwrap(),
        crate::VideoFrameFormat::Packed(crate::PackedVideoFormat::Rgba8)
    );
    assert_eq!(
        frame_format_from_fourcc(0x3231_564e).unwrap(),
        crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::Nv12)
    );
    assert_eq!(
        frame_format_from_fourcc(0x3031_3050).unwrap(),
        crate::VideoFrameFormat::BiPlanar420(crate::BiPlanarVideoFormat::P010)
    );
    assert!(frame_format_from_fourcc(0x3432_5258).is_err());
    assert!(frame_format_from_fourcc(0x3432_4258).is_err());
}
