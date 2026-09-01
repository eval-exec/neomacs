use super::{
    CachedVideo, NativeVideoSessionId, VideoCache, VideoChannelPreparation, VideoGpuAccounting,
    VideoGpuAccountingChange, VideoState, VideoSystemState, remap_event, video_channel_preparation,
};
use neomacs_display_protocol::types::VideoId;
use neomacs_video::{
    MissingVideoPlugin, MissingVideoPlugins, VideoCommandError, VideoEvent, VideoInstallerHint,
    VideoSessionState,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
        remap_event(
            VideoEvent::Ended {
                id: new_native.protocol(),
            },
            stable,
        ),
        VideoEvent::Ended { id: stable }
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
        last_service: Default::default(),
    };

    cache.detach_failed_native_session(stable, native, "import failed".into());

    let video = &cache.videos[&stable];
    assert_eq!(video.state, VideoState::Error);
    assert_eq!(video.native_id, None);
    assert_eq!(video.parked, None);
    assert!(!cache.native_to_video.contains_key(&native));
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
