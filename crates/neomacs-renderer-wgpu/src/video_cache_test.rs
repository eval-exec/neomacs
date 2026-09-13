use super::{
    CachedVideo, NativeVideoSessionId, VideoCache, VideoChannelPreparation, VideoGpuAccounting,
    VideoGpuAccountingChange, VideoState, VideoSystemState, video_channel_preparation,
};
use neomacs_display_protocol::types::VideoId;
use neomacs_video::{
    InitialPlayback, LoopMode, MissingVideoPlugin, MissingVideoPlugins, VideoCommandError,
    VideoEvent, VideoInstallerHint, VideoOpenRequest, VideoSessionState, VideoSource,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[test]
fn native_bi_planar_shader_parses_and_validates() {
    let module = naga::front::wgsl::parse_str(include_str!("shaders/video_biplanar.wgsl"))
        .expect("native bi-planar video shader should parse");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .expect("native bi-planar video shader should validate");
}

#[test]
fn shader_channels_materialize_native_video_instead_of_dropping_it() {
    assert_eq!(
        video_channel_preparation(neomacs_video::VideoSampleKind::Packed),
        VideoChannelPreparation::ReusePacked
    );
    assert_eq!(
        video_channel_preparation(neomacs_video::VideoSampleKind::BiPlanar),
        VideoChannelPreparation::ConvertBiPlanar
    );
}

#[test]
fn optional_backend_initializes_once_at_first_media_use() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut state = VideoSystemState::deferred(move || {
        observed.fetch_add(1, Ordering::Relaxed);
        Err("optional backend absent".to_owned())
    });

    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(state.ready().is_none());
    assert_eq!(
        state.get_or_initialize().err().unwrap(),
        "optional backend absent"
    );
    assert_eq!(
        state.get_or_initialize().err().unwrap(),
        "optional backend absent"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
#[tracing_test::traced_test]
fn absent_optional_backend_is_logged_once_across_media_requests() {
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::deferred(|| Err("optional backend absent".to_owned())),
        videos: HashMap::new(),
        next_id: 1,
        next_native_id: 1,
        native_to_video: HashMap::new(),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };

    cache.load_file("/tmp/first.mp4");
    cache.load_file("/tmp/second.mp4");

    logs_assert(|lines| {
        let unavailable = lines
            .iter()
            .filter(|line| line.contains("native video subsystem is unavailable"))
            .count();
        let repeated_playback = lines
            .iter()
            .filter(|line| {
                line.contains("video playback failed") && line.contains("optional backend absent")
            })
            .count();
        if unavailable == 1 && repeated_playback == 0 {
            Ok(())
        } else {
            Err(format!(
                "expected one unavailable diagnostic and no duplicate playback errors; got {unavailable} and {repeated_playback}: {lines:?}"
            ))
        }
    });
}

#[test]
fn typed_session_states_have_one_renderer_compatibility_mapping() {
    assert_eq!(
        VideoState::from(VideoSessionState::Opening),
        VideoState::Loading
    );
    assert_eq!(
        VideoState::from(VideoSessionState::Playing),
        VideoState::Playing
    );
    assert_eq!(
        VideoState::from(VideoSessionState::Paused),
        VideoState::Paused
    );
    assert_eq!(
        VideoState::from(VideoSessionState::Ended),
        VideoState::EndOfStream
    );
    assert_eq!(
        VideoState::from(VideoSessionState::Failed),
        VideoState::Error
    );
    assert_eq!(
        VideoState::from(VideoSessionState::Closed),
        VideoState::Stopped
    );
}

#[test]
fn video_gpu_pool_accounting_tracks_aggregate_texture_lifetime() {
    let mut accounting = VideoGpuAccounting::default();

    assert_eq!(
        accounting.observe(1920 * 1080 * 4),
        VideoGpuAccountingChange::Register(1920 * 1080 * 4)
    );
    assert_eq!(
        accounting.observe(1920 * 1080 * 4),
        VideoGpuAccountingChange::Unchanged
    );
    assert_eq!(
        accounting.observe(3 * 1920 * 1080 * 4),
        VideoGpuAccountingChange::Register(3 * 1920 * 1080 * 4)
    );
    assert_eq!(accounting.observe(0), VideoGpuAccountingChange::Free);
    assert_eq!(accounting.observe(0), VideoGpuAccountingChange::Unchanged);
}

#[test]
fn native_session_identity_is_distinct_from_stable_video_identity() {
    let stable = VideoId::new(7);
    let old_native = NativeVideoSessionId(VideoId::new(41));
    let new_native = NativeVideoSessionId(VideoId::new(42));

    assert_ne!(old_native, new_native);
    assert_ne!(old_native.protocol(), stable);
    assert_eq!(
        VideoEvent::Ended {
            id: new_native.protocol(),
        }
        .with_id(stable),
        VideoEvent::Ended { id: stable }
    );
}

#[test]
fn presentation_tracker_distinguishes_gpu_submission_from_surface_present() {
    let id = VideoId::new(17);
    let mut tracker = super::VideoSubmissionTracker::default();

    tracker.begin_surface();
    tracker.record_submitted([id, id]);
    assert_eq!(
        tracker.counts(id),
        neomacs_video::VideoPresentationCounts {
            submitted_frames: 1,
            presented_frames: 0,
        }
    );
    tracker.finish_submitted_surface();
    assert_eq!(
        tracker.counts(id),
        neomacs_video::VideoPresentationCounts {
            submitted_frames: 1,
            presented_frames: 1,
        }
    );

    tracker.begin_surface();
    tracker.record_submitted([id]);
    tracker.cancel_surface();
    assert_eq!(
        tracker.counts(id),
        neomacs_video::VideoPresentationCounts {
            submitted_frames: 2,
            presented_frames: 1,
        }
    );
}

#[test]
fn presentation_tracker_reports_exact_frame_pacing_percentiles() {
    let id = VideoId::new(18);
    let mut tracker = super::VideoSubmissionTracker::default();
    let started = Instant::now();

    for offset_ms in [0, 16, 33, 50, 100] {
        tracker.begin_surface();
        tracker.record_submitted([id]);
        tracker.finish_submitted_surface_at(started + Duration::from_millis(offset_ms));
    }

    assert_eq!(
        tracker.timing(id),
        neomacs_video::VideoPresentationTiming {
            interval_samples: 4,
            interval_total_us: 100_000,
            interval_min_us: Some(16_000),
            interval_max_us: Some(50_000),
            interval_p50_us: Some(17_000),
            interval_p95_us: Some(50_000),
            interval_p99_us: Some(50_000),
        }
    );
}

#[test]
fn presentation_tracker_aggregates_supported_gpu_pass_timings() {
    let first = VideoId::new(19);
    let second = VideoId::new(20);
    let mut tracker = super::VideoSubmissionTracker::default();

    tracker.set_gpu_timing_status(neomacs_video::VideoGpuTimingStatus::Enabled);
    tracker.record_gpu_frame_time([first, second, first], 750);
    tracker.record_gpu_frame_time([first], 1_250);

    assert_eq!(
        tracker.gpu_timing(first),
        neomacs_video::VideoGpuTiming {
            status: neomacs_video::VideoGpuTimingStatus::Enabled,
            pass_samples: 2,
            pass_total_us: 2_000,
            pass_min_us: Some(750),
            pass_max_us: Some(1_250),
        }
    );
    assert_eq!(tracker.gpu_timing(second).pass_samples, 1);
}

#[test]
fn measurement_epoch_discards_warmup_distributions() {
    let id = VideoId::new(19);
    let started = Instant::now();
    let mut tracker = super::VideoSubmissionTracker::default();
    tracker.set_gpu_timing_status(neomacs_video::VideoGpuTimingStatus::Enabled);
    for offset_ms in [0, 16, 116] {
        tracker.begin_surface();
        tracker.record_submitted([id]);
        tracker.finish_submitted_surface_at(started + Duration::from_millis(offset_ms));
    }
    tracker.record_gpu_frame_time([id], 9_000);

    tracker.begin_measurement_epoch();
    for offset_ms in [200, 216, 233] {
        tracker.begin_surface();
        tracker.record_submitted([id]);
        tracker.finish_submitted_surface_at(started + Duration::from_millis(offset_ms));
    }
    tracker.record_gpu_frame_time([id], 200);

    let timing = tracker.timing(id);
    assert_eq!(timing.interval_samples, 2);
    assert_eq!(timing.interval_max_us, Some(17_000));
    assert_eq!(timing.interval_p99_us, Some(17_000));
    assert_eq!(tracker.gpu_timing(id).pass_max_us, Some(200));
}

#[test]
fn delayed_gpu_completion_cannot_recreate_a_removed_video_entry() {
    let active = VideoId::new(7);
    let removed = VideoId::new(8);
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::unavailable("test fixture"),
        videos: HashMap::from([(
            active,
            CachedVideo {
                id: active,
                width: 1920,
                height: 1080,
                state: VideoState::Playing,
                frame_count: 3,
                failure: None,
                native_id: None,
                parked: None,
            },
        )]),
        next_id: 9,
        next_native_id: 1,
        native_to_video: HashMap::new(),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };

    cache.record_gpu_frame_time([active, removed], 500);

    assert_eq!(cache.presentation.gpu_timing(active).pass_samples, 1);
    assert_eq!(cache.presentation.gpu_timing(removed).pass_samples, 0);
}

#[test]
fn reopening_stable_session_detaches_the_previous_native_incarnation() {
    let stable = VideoId::new(7);
    let previous_native = NativeVideoSessionId(VideoId::new(41));
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::unavailable("test fixture"),
        videos: HashMap::from([(
            stable,
            CachedVideo {
                id: stable,
                width: 1920,
                height: 1080,
                state: VideoState::Playing,
                frame_count: 3,
                failure: None,
                native_id: Some(previous_native),
                parked: None,
            },
        )]),
        next_id: 8,
        next_native_id: 42,
        native_to_video: HashMap::from([(previous_native, stable)]),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };

    cache.open(
        stable,
        VideoOpenRequest {
            source: VideoSource::Uri("https://example.com/reopened.mp4".to_owned()),
            loop_mode: LoopMode::Off,
            initial_playback: InitialPlayback::Paused,
        },
    );

    assert!(
        !cache.native_to_video.contains_key(&previous_native),
        "replacing a stable session must not leave delayed events from its previous decoder routable"
    );
}

#[test]
fn terminal_failure_detaches_the_ephemeral_native_incarnation() {
    let stable = VideoId::new(7);
    let native = NativeVideoSessionId(VideoId::new(41));
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::unavailable("test fixture"),
        videos: HashMap::from([(
            stable,
            CachedVideo {
                id: stable,
                width: 1920,
                height: 1080,
                state: VideoState::Playing,
                frame_count: 3,
                failure: None,
                native_id: Some(native),
                parked: None,
            },
        )]),
        next_id: 8,
        next_native_id: 42,
        native_to_video: HashMap::from([(native, stable)]),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };

    cache.detach_failed_native_session(stable, native, "import failed".into());

    let video = &cache.videos[&stable];
    assert_eq!(video.state, VideoState::Error);
    assert_eq!(video.native_id, None);
    assert_eq!(video.parked, None);
    assert!(!cache.native_to_video.contains_key(&native));

    let diagnostics = cache
        .diagnostics()
        .expect("a stable failure tombstone remains diagnosable");
    assert_eq!(diagnostics.sessions.len(), 1);
    assert_eq!(diagnostics.sessions[0].id, stable);
    assert_eq!(
        diagnostics.sessions[0].state,
        neomacs_video::VideoSessionState::Failed
    );
    assert_eq!(
        diagnostics.sessions[0].terminal_error,
        Some(VideoCommandError::Backend {
            message: "import failed".to_owned(),
        })
    );
}

#[test]
fn active_session_command_failure_does_not_create_a_terminal_tombstone() {
    let stable = VideoId::new(7);
    let native = NativeVideoSessionId(VideoId::new(41));
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::unavailable("test fixture"),
        videos: HashMap::from([(
            stable,
            CachedVideo {
                id: stable,
                width: 1920,
                height: 1080,
                state: VideoState::Playing,
                frame_count: 3,
                failure: None,
                native_id: Some(native),
                parked: None,
            },
        )]),
        next_id: 8,
        next_native_id: 42,
        native_to_video: HashMap::from([(native, stable)]),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };

    cache.handle_operation_error(stable, "command failed".into());

    assert_eq!(cache.videos[&stable].native_id, Some(native));
    assert_eq!(cache.videos[&stable].state, VideoState::Playing);
    assert_eq!(cache.videos[&stable].failure(), None);
    assert!(
        cache.terminal_diagnostics.is_empty(),
        "an attached decoder incarnation cannot also have a terminal tombstone"
    );
}

#[test]
fn typed_missing_plugin_failure_survives_the_renderer_cache_boundary() {
    let stable = VideoId::new(7);
    let native = NativeVideoSessionId(VideoId::new(41));
    let mut cache = VideoCache {
        sampling: None,
        channel_targets: None,
        system: VideoSystemState::unavailable("test fixture"),
        videos: HashMap::from([(
            stable,
            CachedVideo {
                id: stable,
                width: 0,
                height: 0,
                state: VideoState::Loading,
                frame_count: 0,
                failure: None,
                native_id: Some(native),
                parked: None,
            },
        )]),
        next_id: 8,
        next_native_id: 42,
        native_to_video: HashMap::from([(native, stable)]),
        accounting: Vec::new(),
        gpu_accounting: VideoGpuAccounting::default(),
        presentation: Default::default(),
        terminal_diagnostics: HashMap::new(),
        last_service: Default::default(),
    };
    let failure = VideoCommandError::MissingPlugins {
        plugins: MissingVideoPlugins::new(MissingVideoPlugin::new(
            "H.264 decoder",
            Some(VideoInstallerHint::gstreamer(
                "gstreamer|1.0|neomacs|H.264|decoder-video/x-h264",
            )),
        )),
    };

    cache.detach_failed_native_session(stable, native, failure.clone());

    assert_eq!(cache.get(stable.get()).unwrap().failure(), Some(&failure));
}
