use std::path::Path;
use std::time::{Duration, Instant};

use super::file_navigation_uri;
use crate::{
    BrowsingRelationship, NavigationTarget, StoragePartition, WebContentSize, WebProfileId,
    WebViewCommand, WebViewCreate, WebViewFrameTransport, WebViewId, WebViewPolicy, WebViewState,
    WebViewSystem, WebViewSystemConfig, WebViewWake,
};

#[test]
fn local_file_navigation_uses_a_percent_encoded_file_uri() {
    assert_eq!(
        file_navigation_uri(Path::new("/tmp/web view#1.html")).as_deref(),
        Ok("file:///tmp/web%20view%231.html")
    );
}

/// Create a WPE web view, load a page, then destroy it.
///
/// `G_DEBUG=fatal-criticals` is installed before the reactor thread starts, so
/// a shutdown `g_object_unref` of a non-object aborts this process instead of
/// only logging. Nextest runs this test in its own process.
#[test]
fn webview_create_destroy_cycle_exits_clean_under_fatal_criticals() {
    // SAFETY: this is the only thread so far. GLib reads `G_DEBUG` from the
    // process environment on its first log, which happens on the reactor
    // thread spawned below.
    unsafe {
        std::env::set_var("G_DEBUG", "fatal-criticals");
    }

    let mut system = WebViewSystem::new(
        WebViewSystemConfig {
            profile_root: None,
            frame_transport: WebViewFrameTransport::SoftwarePixels,
            dma_buf_import_formats: Default::default(),
        },
        WebViewWake::noop(),
    )
    .expect("WPE webview system");

    let id = WebViewId::new(1);
    system
        .command(WebViewCommand::Create(WebViewCreate {
            id,
            storage: StoragePartition::Ephemeral(WebProfileId::new(1)),
            relationship: BrowsingRelationship::Independent,
            initial_size: WebContentSize::new(64, 64).expect("nonzero size"),
            policy: WebViewPolicy::default(),
            initial_navigation: Some(NavigationTarget::Html {
                contents: "<html><body style='background:#ff00ff'>x</body></html>".to_owned(),
                base_uri: None,
            }),
        }))
        .expect("submit webview create");

    let deadline = Instant::now() + Duration::from_secs(25);
    let mut saw_frame = false;
    let mut saw_failure = None;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        system.service();
        for event in system.drain_events() {
            if let crate::WebViewEvent::Failed { error, .. } = &event {
                saw_failure = Some(error.clone());
            }
            events.push(event);
        }
        if system.take_frame(id).is_some() {
            saw_frame = true;
            break;
        }
        if matches!(
            system.state(id),
            Some(WebViewState::Failed | WebViewState::Ready)
        ) && saw_failure.is_some()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let state = system.state(id);
    drop(system);

    assert!(
        saw_failure.is_none(),
        "webview create failed before destroy: {saw_failure:?} state={state:?} events={events:?}"
    );
    assert!(
        saw_frame,
        "webview produced no frame within 25s (state={state:?} events={events:?}); destroy did not cover a loaded page"
    );
}

#[test]
fn reactor_startup_does_not_mutate_process_environment() {
    // SAFETY: nextest runs each test in its own process, and before spawning
    // the reactor the only other thread is libtest's parked main thread.
    unsafe {
        std::env::remove_var("WPE_DRM_DEVICE");
        std::env::remove_var("WPE_DRM_RENDER_NODE");
    }

    let system = WebViewSystem::new(
        WebViewSystemConfig {
            profile_root: None,
            frame_transport: WebViewFrameTransport::SoftwarePixels,
            dma_buf_import_formats: Default::default(),
        },
        WebViewWake::noop(),
    )
    .expect("WPE webview system");

    // Joining the reactor thread on drop ensures `WpeRuntime::new` has run to
    // completion on the background reactor thread.
    drop(system);

    assert_eq!(
        std::env::var_os("WPE_DRM_DEVICE"),
        None,
        "WpeRuntime::new must not call std::env::set_var on the reactor thread"
    );
    assert_eq!(
        std::env::var_os("WPE_DRM_RENDER_NODE"),
        None,
        "WpeRuntime::new must not call std::env::set_var on the reactor thread"
    );
}

#[test]
fn configure_before_threads_exports_drm_override_while_single_threaded() {
    // SAFETY: nextest runs each test in its own process, and the only other
    // thread is libtest's parked main thread.
    unsafe {
        std::env::remove_var("WPE_DRM_DEVICE");
        std::env::set_var("WPE_DRM_RENDER_NODE", "/dev/dri/renderD-test-override");
        crate::configure_before_threads();
    }

    assert_eq!(
        std::env::var_os("WPE_DRM_DEVICE").as_deref(),
        Some(std::ffi::OsStr::new("/dev/dri/renderD-test-override"))
    );
    assert_eq!(
        std::env::var_os("WPE_DRM_RENDER_NODE").as_deref(),
        Some(std::ffi::OsStr::new("/dev/dri/renderD-test-override"))
    );
}
