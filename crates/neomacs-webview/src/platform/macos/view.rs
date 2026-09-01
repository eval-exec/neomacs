//! One native-overlay WKWebView.
//!
//! GNU Emacs uses a flipped clip view around each WKWebView
//! (`src/nsxwidget.m`) and positions that pair from the clipped xwidget
//! geometry computed at the end of redisplay (`src/xwidget.c`).  Neomacs uses
//! the same native hierarchy, but receives already-resolved content and
//! visible rectangles from `ResolvedWebViewPlacement`.

use std::cell::RefCell;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSView;
use objc2_foundation::{
    NSError, NSJSONSerialization, NSJSONWritingOptions, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSSize, NSString, NSURL, NSURLRequest, NSUTF8StringEncoding,
};
use objc2_web_kit::{
    WKContentWorld, WKNavigation, WKNavigationDelegate, WKWebView, WKWebViewConfiguration,
    WKWebsiteDataStore,
};

use crate::backend::NavigationMilestone;
use crate::{
    FocusIntent, HistoryAction, HostWindowId, NavigationTarget, ResolvedWebViewPlacement,
    ScriptError, ScriptRequest, ScriptWorld, WebContentSize, WebProcessFailure, WebValue,
    WebViewEvent, WebViewGeneration, WebViewId, WebViewPolicy, WebViewWake,
};

define_class!(
    /// A flipped clipping view makes all placement arithmetic top-down, as it
    /// is in GNU Emacs and in the display protocol.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "NeomacsWebViewClipView"]
    pub(crate) struct WebViewClipView;

    impl WebViewClipView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl WebViewClipView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        // SAFETY: `initWithFrame:` is NSView's designated initializer.  The
        // subclass adds no ivars and is main-thread-only.
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

struct NavigationDelegateIvars {
    id: WebViewId,
    generation: WebViewGeneration,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavigationDelegateIvars]
    #[name = "NeomacsWebViewNavigationDelegate"]
    struct WebViewNavigationDelegate;

    unsafe impl NSObjectProtocol for WebViewNavigationDelegate {}

    unsafe impl WKNavigationDelegate for WebViewNavigationDelegate {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didStartProvisionalNavigation(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(NavigationMilestone::Started);
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didCommitNavigation(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(NavigationMilestone::StateChanged);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFinishNavigation(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(NavigationMilestone::Finished);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFailProvisionalNavigation_withError(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            _error: &NSError,
        ) {
            self.publish_navigation(NavigationMilestone::Finished);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFailNavigation_withError(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            _error: &NSError,
        ) {
            self.publish_navigation(NavigationMilestone::Finished);
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        #[allow(non_snake_case)]
        unsafe fn webViewWebContentProcessDidTerminate(&self, _web_view: &WKWebView) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.ivars()
                    .events
                    .borrow_mut()
                    .push(WebViewEvent::ProcessFailed {
                        id: self.ivars().id,
                        generation: self.ivars().generation,
                        failure: WebProcessFailure::Terminated,
                    });
                self.ivars().wake.notify();
            }));
        }
    }
);

impl WebViewNavigationDelegate {
    fn publish_navigation(&self, milestone: NavigationMilestone) {
        // Objective-C delegate entry points are an FFI boundary.  Publish only
        // the backend-neutral milestone here; the Rust-owned view samples
        // title, URI, and finer progress on the next event-loop turn.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.ivars()
                .events
                .borrow_mut()
                .extend(milestone.normalized_events(self.ivars().id, self.ivars().generation));
            self.ivars().wake.notify();
        }));
    }

    fn new(
        mtm: MainThreadMarker,
        id: WebViewId,
        generation: WebViewGeneration,
        events: Rc<RefCell<Vec<WebViewEvent>>>,
        wake: WebViewWake,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NavigationDelegateIvars {
            id,
            generation,
            events,
            wake,
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// Native objects for one logical WebView generation.
pub(crate) struct MacWebView {
    id: WebViewId,
    generation: WebViewGeneration,
    mtm: MainThreadMarker,
    clip: Retained<WebViewClipView>,
    web: Retained<WKWebView>,
    _navigation_delegate: Retained<WebViewNavigationDelegate>,
    attached_host: Option<HostWindowId>,
    hidden: bool,
    title: Option<String>,
    uri: Option<String>,
    progress: f64,
    focused: bool,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

impl MacWebView {
    #[allow(deprecated)]
    pub(crate) fn new(
        id: WebViewId,
        generation: WebViewGeneration,
        mtm: MainThreadMarker,
        size: WebContentSize,
        policy: &WebViewPolicy,
        data_store: &WKWebsiteDataStore,
        wake: WebViewWake,
    ) -> Self {
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(f64::from(size.width()), f64::from(size.height())),
        );
        let clip = WebViewClipView::new(mtm, frame);
        clip.setClipsToBounds(true);
        clip.setHidden(true);

        // SAFETY: all objects and calls in this module are confined by the
        // MainThreadMarker stored alongside the view.
        let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
        unsafe {
            configuration.setWebsiteDataStore(data_store);
            configuration
                .preferences()
                .setJavaScriptEnabled(policy.javascript());
        }
        let web = unsafe {
            WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, &configuration)
        };
        if policy.developer_tools() && objc2::available!(macos = 13.3) {
            unsafe { web.setInspectable(true) };
        }
        clip.addSubview(&web);
        let events = Rc::new(RefCell::new(Vec::new()));
        let navigation_delegate =
            WebViewNavigationDelegate::new(mtm, id, generation, events.clone(), wake.clone());
        unsafe {
            web.setNavigationDelegate(Some(ProtocolObject::from_ref(&*navigation_delegate)));
        }

        Self {
            id,
            generation,
            mtm,
            clip,
            web,
            _navigation_delegate: navigation_delegate,
            attached_host: None,
            hidden: true,
            title: None,
            uri: None,
            progress: 0.0,
            focused: false,
            events,
            wake,
        }
    }

    pub(crate) fn navigate(&self, target: &NavigationTarget) -> Result<(), String> {
        match target {
            NavigationTarget::Uri(uri) => self.load_uri(uri),
            NavigationTarget::Html { contents, base_uri } => {
                let base = base_uri
                    .as_ref()
                    .map(|uri| {
                        NSURL::URLWithString(&NSString::from_str(uri))
                            .ok_or_else(|| format!("invalid base URI {uri:?}"))
                    })
                    .transpose()?;
                unsafe {
                    let _ = self
                        .web
                        .loadHTMLString_baseURL(&NSString::from_str(contents), base.as_deref());
                }
                Ok(())
            }
            NavigationTarget::File(path) => {
                let file = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                let read_root = path.parent().unwrap_or(path);
                let read_root =
                    NSURL::fileURLWithPath(&NSString::from_str(&read_root.to_string_lossy()));
                unsafe {
                    let _ = self
                        .web
                        .loadFileURL_allowingReadAccessToURL(&file, &read_root);
                }
                Ok(())
            }
        }
    }

    fn load_uri(&self, uri: &str) -> Result<(), String> {
        let url = NSURL::URLWithString(&NSString::from_str(uri))
            .ok_or_else(|| format!("invalid URI {uri:?}"))?;
        let request = NSURLRequest::requestWithURL(&url);
        unsafe {
            let _ = self.web.loadRequest(&request);
        }
        Ok(())
    }

    pub(crate) fn history(&self, action: HistoryAction) {
        unsafe {
            match action {
                HistoryAction::Back => {
                    let _ = self.web.goBack();
                }
                HistoryAction::Forward => {
                    let _ = self.web.goForward();
                }
                HistoryAction::Reload => {
                    let _ = self.web.reload();
                }
            }
        }
    }

    pub(crate) fn evaluate_script(&self, request: &ScriptRequest) {
        let world = unsafe {
            match request.world {
                ScriptWorld::Page => WKContentWorld::pageWorld(self.mtm),
                ScriptWorld::Isolated => WKContentWorld::defaultClientWorld(self.mtm),
            }
        };
        let events = self.events.clone();
        let wake = self.wake.clone();
        let view = self.id;
        let generation = self.generation;
        let request_id = request.request;
        let completion = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                if let Some(error) = error.as_ref() {
                    return Err(ScriptError::Rejected(
                        error.localizedDescription().to_string(),
                    ));
                }
                mac_web_value(value)
            }))
            .unwrap_or_else(|_| {
                Err(ScriptError::Rejected(
                    "WKWebView script completion panicked".to_owned(),
                ))
            });
            events.borrow_mut().push(WebViewEvent::ScriptFinished {
                view,
                generation,
                request: request_id,
                result,
            });
            wake.notify();
        });
        unsafe {
            self.web
                .evaluateJavaScript_inFrame_inContentWorld_completionHandler(
                    &NSString::from_str(&request.source),
                    None,
                    &world,
                    Some(&completion),
                );
        }
    }

    pub(crate) fn resize(&self, size: WebContentSize) {
        self.web.setFrameSize(NSSize::new(
            f64::from(size.width()),
            f64::from(size.height()),
        ));
    }

    pub(crate) fn focus(&mut self, intent: FocusIntent) {
        match intent {
            FocusIntent::Focus => {
                let _ = self.web.becomeFirstResponder();
            }
            FocusIntent::Blur => {
                let _ = self.web.resignFirstResponder();
            }
        }
        let focused = intent == FocusIntent::Focus;
        if self.focused != focused {
            self.focused = focused;
            self.events.borrow_mut().push(WebViewEvent::FocusChanged {
                id: self.id,
                generation: self.generation,
                focused,
            });
        }
    }

    pub(crate) fn service_events(&mut self) -> Vec<WebViewEvent> {
        let title = unsafe { self.web.title() }.map(|value| value.to_string());
        let uri = unsafe { self.web.URL() }
            .and_then(|url| url.absoluteString())
            .map(|value| value.to_string());
        let progress = unsafe { self.web.estimatedProgress() };
        let mut events = std::mem::take(&mut *self.events.borrow_mut());
        if title != self.title {
            self.title = title.clone();
            if let Some(title) = title {
                events.push(WebViewEvent::TitleChanged {
                    id: self.id,
                    generation: self.generation,
                    title,
                });
            }
        }
        if uri != self.uri {
            self.uri = uri.clone();
            if let Some(uri) = uri {
                events.push(WebViewEvent::UriChanged {
                    id: self.id,
                    generation: self.generation,
                    uri,
                });
            }
        }
        if (progress - self.progress).abs() > f64::EPSILON {
            self.progress = progress;
            events.push(WebViewEvent::LoadProgressChanged {
                id: self.id,
                generation: self.generation,
                progress,
            });
        }
        events
    }

    pub(crate) fn present(
        &mut self,
        host_id: HostWindowId,
        host: &NSView,
        placement: &ResolvedWebViewPlacement,
    ) {
        if self.attached_host != Some(host_id) {
            self.clip.removeFromSuperview();
            host.addSubview(&self.clip);
            self.attached_host = Some(host_id);
        }

        // Frame geometry is expressed in root-surface device pixels; AppKit
        // consumes logical points.  The typed scale is validated at scene
        // construction, so this is the only conversion boundary.
        let scale = f64::from(placement.device_scale().get());
        let content = placement.content_rect();
        let visible = placement.visible_rect();
        let offset = placement.content_offset();
        let visible_width = f64::from(visible.width()) / scale;
        let visible_height = f64::from(visible.height()) / scale;
        let visible_x = f64::from(visible.x()) / scale;
        let visible_y = f64::from(visible.y()) / scale;
        let host_y = if host.isFlipped() {
            visible_y
        } else {
            host.bounds().size.height - visible_y - visible_height
        };

        self.clip.setFrame(NSRect::new(
            NSPoint::new(visible_x, host_y),
            NSSize::new(visible_width, visible_height),
        ));
        self.web.setFrame(NSRect::new(
            NSPoint::new(
                -f64::from(offset.x()) / scale,
                -f64::from(offset.y()) / scale,
            ),
            NSSize::new(
                f64::from(content.width()) / scale,
                f64::from(content.height()) / scale,
            ),
        ));
        self.clip.setHidden(false);
        self.hidden = false;
    }

    pub(crate) fn hide(&mut self) {
        if !self.hidden {
            self.clip.setHidden(true);
            self.hidden = true;
        }
    }
}

impl Drop for MacWebView {
    fn drop(&mut self) {
        self.web.removeFromSuperview();
        self.clip.removeFromSuperview();
    }
}

unsafe fn mac_web_value(value: *mut AnyObject) -> Result<WebValue, ScriptError> {
    let Some(value) = (unsafe { value.as_ref() }) else {
        return Ok(WebValue::Null);
    };
    let data = unsafe {
        NSJSONSerialization::dataWithJSONObject_options_error(
            value,
            NSJSONWritingOptions::FragmentsAllowed,
        )
    }
    .map_err(|error| ScriptError::Rejected(error.localizedDescription().to_string()))?;
    let json = NSString::initWithData_encoding(NSString::alloc(), &data, NSUTF8StringEncoding)
        .ok_or_else(|| ScriptError::Rejected("WKWebView returned non-UTF-8 JSON".to_owned()))?;
    serde_json::from_str(&json.to_string())
        .map(WebValue::from_json)
        .map_err(|error| ScriptError::Rejected(error.to_string()))
}
