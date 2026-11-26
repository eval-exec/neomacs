use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::sampling::{DrawRetirementGate, GpuAllocationTracker, SubmittedWorkRetirement};
use crate::{PreparedVideoDraw, VideoRenderPass, VideoSampleKind, VideoWake};

#[test]
fn prepared_draw_exposes_one_renderer_facing_sample_contract() {
    fn renderer_contract(draw: PreparedVideoDraw<'_>, pass: &mut VideoRenderPass<'_>) {
        match draw.sample_kind() {
            VideoSampleKind::Packed | VideoSampleKind::BiPlanar => {}
        }
        draw.bind_to_pass(pass, 1);
    }

    let _ = renderer_contract;
}

#[test]
fn gpu_allocation_is_counted_until_the_last_texture_owner_drops() {
    let tracker = GpuAllocationTracker::default();
    let first_owner = tracker.track(4096);
    let second_owner = first_owner.clone();

    assert_eq!(tracker.bytes(), 4096);
    drop(first_owner);
    assert_eq!(tracker.bytes(), 4096);
    drop(second_owner);
    assert_eq!(tracker.bytes(), 0);
}

#[test]
fn final_gpu_allocation_retirement_wakes_budget_observation() {
    let wake_count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&wake_count);
    let tracker = GpuAllocationTracker::new(VideoWake::new(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    }));

    let owner = tracker.track(4096);
    assert_eq!(wake_count.load(Ordering::Relaxed), 0);
    drop(owner);

    assert_eq!(tracker.bytes(), 0);
    assert_eq!(wake_count.load(Ordering::Relaxed), 1);
}

#[test]
fn native_lease_is_retained_until_the_gpu_completion_callback_runs() {
    struct NativeLease(Arc<AtomicUsize>);

    impl Drop for NativeLease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let releases = Arc::new(AtomicUsize::new(0));
    let retirement = SubmittedWorkRetirement::new(NativeLease(Arc::clone(&releases)));
    assert_eq!(releases.load(Ordering::Relaxed), 0);

    let gpu_completion = retirement.into_completion_callback();
    assert_eq!(releases.load(Ordering::Relaxed), 0);

    gpu_completion();
    assert_eq!(releases.load(Ordering::Relaxed), 1);
}

#[test]
fn prepared_draw_and_retirement_are_tied_to_submission() {
    let sampling_src = include_str!("../sampling.rs");
    assert!(
        !sampling_src.contains("pub fn bind_group("),
        "PreparedVideoDraw must not leak a cloneable &wgpu::BindGroup past submission (B8)"
    );
    assert!(
        !sampling_src.contains("pub fn packed_view("),
        "PreparedVideoDraw must not leak a cloneable &wgpu::TextureView past submission (B8)"
    );
    assert!(
        !sampling_src.contains("pub struct VideoSubmissionToken"),
        "VideoSubmissionToken must stay private inside VideoCommandEncoder (B8)"
    );
    let lib_src = include_str!("../lib.rs");
    assert!(
        !lib_src.contains("VideoSubmissionToken"),
        "neomacs_video root must not re-export VideoSubmissionToken (B8)"
    );
    let system_src = include_str!("../system.rs");
    assert!(
        system_src.contains("pub fn submit_command_encoder("),
        "VideoSystem must tie frame retirement to draw submission (B8)"
    );
    assert!(
        !system_src.contains("pub fn finish_draw_submission("),
        "finish_draw_submission must be removed so foreign SubmissionIndex values cannot clear the gate (B8)"
    );
    assert!(
        !system_src.contains("pub fn discard_unsubmitted_draws("),
        "discard_unsubmitted_draws must be removed so live encoders cannot outlive their token (B8)"
    );
    assert!(
        !system_src.contains("pub fn submit_idle_encoder("),
        "submit_idle_encoder must be unified into submit_command_encoder (B8)"
    );

    // Compile-time signature witnesses: draws bind only into VideoRenderPass
    // and submission consumes VideoCommandEncoder directly (carrying its
    // private retirement token).
    fn witness_bind<'a>(draw: &PreparedVideoDraw<'a>, pass: &mut VideoRenderPass<'_>, group: u32) {
        draw.bind_to_pass(pass, group);
    }
    let _ = witness_bind;
    let _: fn(&mut crate::VideoSystem, crate::VideoCommandEncoder) -> wgpu::SubmissionIndex =
        crate::VideoSystem::submit_command_encoder;
    let _: fn(&mut crate::VideoSystem, crate::VideoCommandEncoder) =
        crate::VideoSystem::discard_command_encoder;
}

#[test]
fn retirement_gate_defers_replaced_frames_until_draw_submission() {
    struct NativeLease {
        id: u32,
        drops: Arc<AtomicUsize>,
    }

    impl Drop for NativeLease {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let mut gate = DrawRetirementGate::new();

    // Without prepared draws, retirement passes straight through.
    let immediate = gate.stage_or_retire(vec![NativeLease {
        id: 1,
        drops: Arc::clone(&drops),
    }]);
    assert_eq!(immediate.len(), 1);
    assert_eq!(immediate[0].id, 1);
    drop(immediate);
    assert_eq!(drops.load(Ordering::Relaxed), 1);

    // Two overlapping encoder tokens hold replaced frames until BOTH finish.
    let token_a = gate.issue_token();
    let token_b = gate.issue_token();
    assert!(gate.has_unsubmitted_draws());
    let deferred_first = gate.stage_or_retire(vec![NativeLease {
        id: 2,
        drops: Arc::clone(&drops),
    }]);
    let deferred_second = gate.stage_or_retire(vec![NativeLease {
        id: 3,
        drops: Arc::clone(&drops),
    }]);
    assert!(deferred_first.is_empty());
    assert!(deferred_second.is_empty());
    assert_eq!(gate.pending_len(), 2);
    assert_eq!(drops.load(Ordering::Relaxed), 1);

    // Submitting an idle encoder (`None`) while tokens remain live does NOT flush pending.
    assert!(gate.finish_encoder_token(None).is_empty());
    assert_eq!(gate.pending_len(), 2);

    // Consuming only token_a keeps pending frames held while token_b is live.
    assert!(gate.finish_encoder_token(Some(token_a)).is_empty());
    assert!(gate.has_unsubmitted_draws());
    assert_eq!(gate.pending_len(), 2);

    // Consuming token_b drains all staged frames in order and clears the hold.
    let submitted = gate.finish_encoder_token(Some(token_b));
    assert!(!gate.has_unsubmitted_draws());
    assert_eq!(gate.pending_len(), 0);
    let ids: Vec<u32> = submitted.iter().map(|lease| lease.id).collect();
    assert_eq!(ids, vec![2, 3]);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(submitted);
    assert_eq!(drops.load(Ordering::Relaxed), 3);

    // Dropping a private token (only reachable after VideoCommandEncoder::drop
    // destroys its unsubmitted wgpu::CommandEncoder) releases its active hold so
    // the next service (`stage_or_retire`) flushes pending frames cleanly without
    // stalling the decoder pool.
    let abandoned = gate.issue_token();
    assert!(
        gate.stage_or_retire(vec![NativeLease {
            id: 4,
            drops: Arc::clone(&drops),
        }])
        .is_empty()
    );
    assert_eq!(gate.pending_len(), 1);
    drop(abandoned);
    assert!(!gate.has_unsubmitted_draws());
    let flushed_after_abandon = gate.stage_or_retire(Vec::new());
    assert_eq!(flushed_after_abandon.len(), 1);
    assert_eq!(flushed_after_abandon[0].id, 4);
    drop(flushed_after_abandon);
    assert_eq!(drops.load(Ordering::Relaxed), 4);

    // VideoSystem teardown (`take_all_on_drop`) must drain pending frames even
    // while an encoder token is still live.
    let _still_live = gate.issue_token();
    assert!(
        gate.stage_or_retire(vec![NativeLease {
            id: 5,
            drops: Arc::clone(&drops),
        }])
        .is_empty()
    );
    assert_eq!(gate.pending_len(), 1);
    let flushed_on_drop = gate.take_all_on_drop();
    assert_eq!(flushed_on_drop.len(), 1);
    assert_eq!(flushed_on_drop[0].id, 5);
    assert_eq!(gate.pending_len(), 0);
    drop(flushed_on_drop);
    assert_eq!(drops.load(Ordering::Relaxed), 5);

    // A token from another VideoSystem is rejected.
    let other_gate: DrawRetirementGate<NativeLease> = DrawRetirementGate::new();
    let foreign_token = other_gate.issue_token();
    let mismatched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = gate.finish_encoder_token(Some(foreign_token));
    }));
    assert!(
        mismatched.is_err(),
        "foreign submission token must be rejected"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn submit_buffer_with_retirements_handles_render_pass_plus_raw_vulkan_release() {
    use std::time::Instant;

    use crate::{FrameImportPolicy, GpuGeneration, VideoGeometry, VideoSamplingResources};

    struct NativeLease(Arc<AtomicUsize>);
    impl Drop for NativeLease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let Some((_instance, _adapter, device, queue)) =
        crate::platform::open_test_vulkan_foreign_device("video-b8-submit-test")
    else {
        assert!(
            !crate::platform::require_gpu_video_tests(),
            "NEOMACS_REQUIRE_GPU_VIDEO_TESTS is set, but no headless Vulkan adapter with VK_EXT_queue_family_foreign is available"
        );
        eprintln!(
            "skipping B8 GPU submit test: no headless Vulkan adapter with VK_EXT_queue_family_foreign"
        );
        return;
    };

    let packed_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("video-b8-packed-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("video-b8-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let sampling = VideoSamplingResources::new(&device, &packed_layout, &sampler);
    let mut system = crate::VideoSystem::with_sampling_resources(
        device.clone(),
        queue.clone(),
        sampling,
        GpuGeneration::INITIAL,
        FrameImportPolicy::AllowCpuUpload,
        VideoWake::noop(),
    )
    .expect("VideoSystem must initialize on headless Vulkan device");

    let make_source_tex = |label: &'static str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let target_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video-b8-target"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let lease_drops = Arc::new(AtomicUsize::new(0));
    let draw_frame = system.test_gpu().wrap_prepared_texture_with_release(
        VideoGeometry::packed(4, 4),
        system
            .test_gpu()
            .prepare_texture(make_source_tex("video-b8-draw-source"), 64),
        crate::platform::create_test_foreign_surface_release(&device, &queue)
            .expect("real ForeignRelease for draw frame"),
        NativeLease(Arc::new(AtomicUsize::new(0))),
    );
    let retired_frame = system.test_gpu().wrap_prepared_texture_with_release(
        VideoGeometry::packed(4, 4),
        system
            .test_gpu()
            .prepare_texture(make_source_tex("video-b8-retired-source"), 64),
        crate::platform::create_test_foreign_surface_release(&device, &queue)
            .expect("real ForeignRelease for retired frame"),
        NativeLease(Arc::clone(&lease_drops)),
    );

    // 1. Bind a real PreparedVideoDraw into a VideoRenderPass on a
    //    VideoCommandEncoder (arming its private retirement token).
    let mut encoder = system.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("video-b8-render-pass-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video-b8-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let draw = system.test_make_prepared_draw(&draw_frame);
        draw.bind_to_pass(&mut pass, 1);
    }

    // 2. Replace/retire a frame and call `service()` BEFORE submitting
    //    `encoder`: the private token inside `encoder` must keep the replaced
    //    frame (and its real `ForeignRelease` + native lease) staged.
    system.test_stage_retired_frame(retired_frame);
    let _ = system.service(Instant::now());
    assert_eq!(system.test_pending_retirements(), 1);
    assert_eq!(lease_drops.load(Ordering::Relaxed), 0);

    // 3. Submit via the real `VideoSystem::submit_command_encoder(encoder)`:
    //    executes the real `ForeignRelease::record` (VkDevice identity check +
    //    QUEUE_FAMILY_FOREIGN_EXT barrier in a separate raw encoder) alongside
    //    the RenderPass draw buffer in one `queue.submit` batch.
    let _submission = system.submit_command_encoder(encoder);
    assert_eq!(system.test_pending_retirements(), 0);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    assert_eq!(
        lease_drops.load(Ordering::Relaxed),
        1,
        "native lease must drop once VideoSystem::submit_command_encoder finishes on GPU"
    );

    // 4. Abandoning an unsubmitted VideoCommandEncoder destroys its private
    //    wgpu::CommandEncoder before releasing its token, so the next `service`
    //    retires staged frames cleanly without stalling the decoder pool.
    let second_retired = system.test_gpu().wrap_prepared_texture_with_release(
        VideoGeometry::packed(4, 4),
        system
            .test_gpu()
            .prepare_texture(make_source_tex("video-b8-abandoned-source"), 64),
        crate::platform::create_test_foreign_surface_release(&device, &queue)
            .expect("real ForeignRelease for abandoned-encoder frame"),
        NativeLease(Arc::clone(&lease_drops)),
    );
    let mut abandoned = system.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("video-b8-abandoned-encoder"),
    });
    {
        let mut pass = abandoned.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video-b8-abandoned-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let draw = system.test_make_prepared_draw(&draw_frame);
        draw.bind_to_pass(&mut pass, 1);
    }
    system.test_stage_retired_frame(second_retired);
    assert_eq!(system.test_pending_retirements(), 1);
    drop(abandoned);
    let _ = system.service(Instant::now());
    assert_eq!(system.test_pending_retirements(), 0);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    assert_eq!(
        lease_drops.load(Ordering::Relaxed),
        2,
        "abandoned encoder drop + next service must retire staged frames cleanly"
    );

    // 5. Cross-system VideoRenderPass and VideoCommandEncoder are rejected.
    let other_sampling = VideoSamplingResources::new(&device, &packed_layout, &sampler);
    let other_system = crate::VideoSystem::with_sampling_resources(
        device.clone(),
        queue.clone(),
        other_sampling,
        GpuGeneration::INITIAL,
        FrameImportPolicy::AllowCpuUpload,
        VideoWake::noop(),
    )
    .expect("second VideoSystem on headless Vulkan device");

    let mut foreign_for_pass =
        other_system.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video-b8-foreign-pass-encoder"),
        });
    let rejected_pass = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut foreign_pass = foreign_for_pass.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video-b8-foreign-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let draw = system.test_make_prepared_draw(&draw_frame);
        draw.bind_to_pass(&mut foreign_pass, 1);
    }));
    assert!(
        rejected_pass.is_err(),
        "PreparedVideoDraw::bind_to_pass must reject another system's VideoRenderPass"
    );

    let foreign_encoder = other_system.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("video-b8-foreign-encoder"),
    });
    let rejected_submit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = system.submit_command_encoder(foreign_encoder);
    }));
    assert!(
        rejected_submit.is_err(),
        "VideoSystem::submit_command_encoder must reject another system's VideoCommandEncoder"
    );
    eprintln!("GPU_TEST_RAN: b8_video_system_submit_and_foreign_release");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_render_device_numbers_requires_vulkan_1_1_on_both_sides() {
    use ash::vk;

    assert!(
        !crate::sampling::supports_core_properties2(vk::API_VERSION_1_0, vk::API_VERSION_1_1),
        "Vulkan 1.0 physical device with 1.1 instance must not dispatch core properties2"
    );
    assert!(
        !crate::sampling::supports_core_properties2(vk::API_VERSION_1_1, vk::API_VERSION_1_0),
        "Vulkan 1.1 physical device with 1.0 instance must not dispatch core properties2"
    );
    assert!(
        crate::sampling::supports_core_properties2(vk::API_VERSION_1_1, vk::API_VERSION_1_1),
        "Vulkan 1.1 on both physical device and instance supports core properties2"
    );
}
