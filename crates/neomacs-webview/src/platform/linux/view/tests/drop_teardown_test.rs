use std::ptr;

use super::super::NetworkSession;
use super::super::engine::WpeBackend;
use super::super::native;
use super::super::sys::platform as plat;
use super::{WpeFrameTransport, WpeViewCreation, WpeWebView};
use crate::{
    StoragePartition, WebContentSize, WebProfileId, WebViewGeneration, WebViewId, WebViewPolicy,
    WebViewWake,
};

struct GObjectRefGuard(*mut plat::GObject);

impl GObjectRefGuard {
    unsafe fn retain(obj: *mut libc::c_void) -> Self {
        plat::g_object_ref(obj.cast());
        Self(obj.cast())
    }
}

impl Drop for GObjectRefGuard {
    fn drop(&mut self) {
        unsafe {
            plat::g_object_unref(self.0.cast());
        }
    }
}

#[test]
fn drop_disconnects_callbacks_even_when_external_gobject_ref_survives() {
    let backend = unsafe { WpeBackend::new(ptr::null_mut()) }.expect("WPE backend");
    let session = NetworkSession::create(&StoragePartition::Ephemeral(WebProfileId::new(1)), None)
        .expect("ephemeral network session");
    let view = WpeWebView::new(WpeViewCreation {
        id: WebViewId::new(1),
        generation: WebViewGeneration::new(1),
        platform_display: backend.platform_display(),
        network_session: session.raw(),
        related_view: None,
        size: WebContentSize::new(64, 64).expect("nonzero size"),
        policy: &WebViewPolicy::default(),
        frame_transport: WpeFrameTransport::SoftwarePixels,
        wake: WebViewWake::noop(),
    })
    .expect("WPE webview");

    let raw_web_view = view.web_view;
    let raw_wpe_view = view.wpe_view;
    let raw_callback_data = view.callbacks.as_ptr().cast::<libc::c_void>();
    let decide_policy_id = view.callbacks.decide_policy_handler_id;
    let process_failed_id = view.callbacks.process_failed_handler_id;

    // Hold extra GObject refs across WpeWebView::drop, modelling an in-flight
    // WebKit GTask (e.g. evaluate_javascript) or other owner that keeps the
    // native view alive past the Rust wrapper's Drop.
    let _web_view_ref = unsafe { GObjectRefGuard::retain(raw_web_view.cast()) };
    let _wpe_view_ref = unsafe { GObjectRefGuard::retain(raw_wpe_view.cast()) };

    unsafe {
        assert_ne!(decide_policy_id, 0);
        assert_ne!(process_failed_id, 0);
        assert_ne!(
            plat::g_signal_handler_is_connected(raw_web_view.cast(), decide_policy_id),
            0,
            "decide-policy signal handler should be connected while view is live"
        );
        assert_ne!(
            plat::g_signal_handler_is_connected(raw_web_view.cast(), process_failed_id),
            0,
            "web-process-terminated signal handler should be connected while view is live"
        );
        assert!(
            native::has_render_buffer_callback(raw_wpe_view),
            "render_buffer callback should be installed while view is live"
        );
        let before = plat::g_signal_handler_find(
            raw_web_view.cast(),
            plat::GSignalMatchType_G_SIGNAL_MATCH_DATA,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            raw_callback_data,
        );
        assert_ne!(
            before, 0,
            "GObject signal handlers should be connected with callback_data while view is live"
        );
    }

    drop(view);

    // After WpeWebView::drop frees callback_data, neither the WPEView render
    // sink nor any WebKitWebView signal handler may still retain that pointer.
    unsafe {
        assert!(
            !native::has_render_buffer_callback(raw_wpe_view),
            "WpeWebView::drop must clear the WPEView render_buffer callback before freeing callback_data"
        );
        assert_eq!(
            plat::g_signal_handler_is_connected(raw_web_view.cast(), decide_policy_id),
            0,
            "WpeWebView::drop must disconnect decide-policy signal handler"
        );
        assert_eq!(
            plat::g_signal_handler_is_connected(raw_web_view.cast(), process_failed_id),
            0,
            "WpeWebView::drop must disconnect web-process-terminated signal handler"
        );
        let after = plat::g_signal_handler_find(
            raw_web_view.cast(),
            plat::GSignalMatchType_G_SIGNAL_MATCH_DATA,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            raw_callback_data,
        );
        assert_eq!(
            after, 0,
            "WpeWebView::drop must disconnect all WebKitWebView signal handlers referencing callback_data"
        );
    }
}
