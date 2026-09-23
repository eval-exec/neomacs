use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use neomacs_display_protocol::VideoId;

use super::{
    GpuFrameTimer, MeasurementEpoch, WaitProgress, receive_until_cancelled, timestamp_delta_us,
    wait_until,
};

#[test]
fn delayed_warmup_timing_belongs_to_the_previous_measurement_epoch() {
    let warmup = MeasurementEpoch::default();
    let measured = warmup.next();
    let delayed_warmup = super::CompletedGpuFrameTiming {
        video_ids: vec![VideoId::new(7)],
        duration_us: 10_000,
        epoch: warmup,
    };

    assert!(!measured.accepts(&delayed_warmup));
}

#[test]
fn cancelled_timestamp_wait_returns_before_its_overall_timeout() {
    let cancelled = AtomicBool::new(false);
    let mut calls = 0;

    let result = wait_until(Instant::now() + Duration::from_secs(5), &cancelled, |_| {
        calls += 1;
        if calls == 3 {
            cancelled.store(true, Ordering::Release);
        }
        WaitProgress::<()>::Pending
    });

    assert_eq!(result, None);
    assert_eq!(calls, 3);
}

#[test]
fn timestamp_wait_uses_one_overall_deadline() {
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();
    let deadline = started + Duration::from_millis(25);

    let first = wait_until(deadline, &cancelled, |quantum| {
        std::thread::sleep(quantum);
        WaitProgress::<()>::Pending
    });
    let second = wait_until(deadline, &cancelled, |_| WaitProgress::Ready(()));

    assert_eq!(first, None);
    assert_eq!(second, None, "a second phase must not restart the timeout");
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn worker_cancellation_skips_reads_already_queued_before_shutdown() {
    let cancelled = AtomicBool::new(false);
    let (sender, receiver) = mpsc::channel();
    sender.send(1_u8).expect("queue first read");
    sender.send(2_u8).expect("queue second read");
    cancelled.store(true, Ordering::Release);

    assert_eq!(receive_until_cancelled(&receiver, &cancelled), None);
    assert_eq!(receiver.try_recv(), Ok(2), "queued work was not handled");
}

#[test]
fn timestamp_ticks_are_converted_with_the_queue_period() {
    assert_eq!(timestamp_delta_us(100, 1_100, 2.5), Some(3));
}

#[test]
fn wrapped_timestamp_is_rejected_instead_of_becoming_a_huge_sample() {
    assert_eq!(timestamp_delta_us(1_100, 100, 1.0), None);
}

#[test]
fn timestamp_query_submission_retires_into_a_video_sample() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("skipping: no headless wgpu adapter");
        return;
    };
    if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        eprintln!("skipping: adapter has no timestamp-query support");
        return;
    }
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("gpu-frame-timing-test"),
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    })) else {
        eprintln!("skipping: timestamp-query device request failed");
        return;
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu-frame-timing-test-target"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut timer = GpuFrameTimer::with_requested(&device, &queue, true);
    let pending = timer.begin().expect("requested timer owns a query slot");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu-frame-timing-test-encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gpu-frame-timing-test-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: Some(pending.timestamp_writes()),
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    pending.resolve(&mut encoder);
    let submission = queue.submit([encoder.finish()]);
    timer.submit(pending, submission, vec![VideoId::new(7)]);

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(sample) = timer.drain().into_iter().next() {
            assert_eq!(sample.video_ids, vec![VideoId::new(7)]);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timestamp query did not retire into a sample"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
