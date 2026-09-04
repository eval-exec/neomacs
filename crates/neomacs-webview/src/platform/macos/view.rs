//! One native-overlay WKWebView.
//!
//! GNU Emacs uses a flipped clip view around each WKWebView
//! (`src/nsxwidget.m`) and positions that pair from the clipped xwidget
//! geometry computed at the end of redisplay (`src/xwidget.c`).  Neomacs uses
//! the same native hierarchy, but receives already-resolved content and
//! visible rectangles from `ResolvedWebViewPlacement`.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
};
use objc2_app_kit::{NSEvent, NSResponder, NSView};
use objc2_core_foundation::{CFBoolean, CFType};
use objc2_foundation::{
    NSError, NSJSONSerialization, NSJSONWritingOptions, NSKeyValueObservingOptions, NSNumber,
    NSObject, NSObjectNSKeyValueObserverRegistration, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSURL, NSURLRequest, NSUTF8StringEncoding, ns_string,
};
use objc2_web_kit::{
    WKContentWorld, WKNavigation, WKNavigationDelegate, WKScriptMessage, WKScriptMessageHandler,
    WKUserContentController, WKUserScript, WKUserScriptInjectionTime, WKWebView,
    WKWebViewConfiguration, WKWebsiteDataStore,
};

use super::focus::{
    FOCUS_PROBE, FocusProbe, GNU_XW_SCRIPT, HostEpoch, KEY_DOWN_MESSAGE_HANDLER, KeyDownMessage,
    KeyRoute, focus_transition, key_down_message, route_key,
};
use crate::backend::NavigationMilestone;
use crate::load_state::{NativeNavigation, PageLoadState};
use crate::{
    FocusIntent, HistoryAction, HostWindowId, NavigationTarget, ResolvedWebViewPlacement,
    ScriptError, ScriptRequest, ScriptWorld, WebContentSize, WebProcessFailure, WebValue,
    WebViewEvent, WebViewGeneration, WebViewId, WebViewPolicy, WebViewWake,
};

/// Run a void-returning Objective-C callback without allowing a Rust panic to
/// unwind through the foreign ABI.  The panic hook still reports the failure;
/// the callback itself has no error channel through which to return it.
fn run_objc_callback(callback: impl FnOnce()) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
}

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
    /// The one owner of load state, shared with the KVO observer and the
    /// per-turn sampler: milestones and readings are folded into one
    /// progress sequence there.
    state: Rc<RefCell<PageLoadState>>,
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
            navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Started);
        }

        #[unsafe(method(webView:didReceiveServerRedirectForProvisionalNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didReceiveServerRedirectForProvisionalNavigation(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Redirected);
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didCommitNavigation(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Committed);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFinishNavigation(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Finished);
        }

        // GNU's NS delegate has no failure methods (nsxwidget.m:104-133: a
        // failed load reports nothing on macOS); its GTK build receives
        // `load-changed FINISHED` after `load-failed`, and that is what
        // `xwidget-webkit-callback`'s progress timer needs to stop, so a
        // failure is delivered as the finished milestone here.
        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFailProvisionalNavigation_withError(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
            _error: &NSError,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Finished);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        #[allow(non_snake_case)]
        unsafe fn webView_didFailNavigation_withError(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
            _error: &NSError,
        ) {
            self.publish_navigation(navigation, NavigationMilestone::Finished);
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        #[allow(non_snake_case)]
        unsafe fn webViewWebContentProcessDidTerminate(&self, _web_view: &WKWebView) {
            run_objc_callback(|| {
                self.ivars()
                    .events
                    .borrow_mut()
                    .push(WebViewEvent::ProcessFailed {
                        id: self.ivars().id,
                        generation: self.ivars().generation,
                        failure: WebProcessFailure::Terminated,
                    });
                self.ivars().wake.notify();
            });
        }
    }
);

impl WebViewNavigationDelegate {
    fn publish_navigation(
        &self,
        navigation: Option<&WKNavigation>,
        milestone: NavigationMilestone,
    ) {
        // Objective-C delegate entry points are an FFI boundary.  Hand the
        // backend-neutral milestone to the load state, which decides what it
        // means next to the progress the observer already reported.
        run_objc_callback(|| {
            let ivars = self.ivars();
            let navigation = navigation
                .map(NativeNavigation::of)
                .unwrap_or(NativeNavigation::Unidentified);
            let events = ivars.state.borrow_mut().milestone(
                ivars.id,
                ivars.generation,
                navigation,
                milestone,
            );
            ivars.events.borrow_mut().extend(events);
            ivars.wake.notify();
        });
    }

    fn new(
        mtm: MainThreadMarker,
        id: WebViewId,
        generation: WebViewGeneration,
        state: Rc<RefCell<PageLoadState>>,
        events: Rc<RefCell<Vec<WebViewEvent>>>,
        wake: WebViewWake,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NavigationDelegateIvars {
            id,
            generation,
            state,
            events,
            wake,
        });
        unsafe { msg_send![super(this), init] }
    }
}

struct WebViewObserverIvars {
    id: WebViewId,
    generation: WebViewGeneration,
    web: Retained<WKWebView>,
    state: Rc<RefCell<PageLoadState>>,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

define_class!(
    /// KVO observer for the page properties GNU reads as GObject
    /// properties: `estimatedProgress` (`xwidget-webkit-estimated-load-
    /// progress`), `title` and `URL`.  The callback samples the view and
    /// wakes the event loop, so intermediate progress reaches Lisp while the
    /// session is otherwise idle -- the sampling in `service_events` alone
    /// only ran when something else woke the loop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WebViewObserverIvars]
    #[name = "NeomacsWebViewObserver"]
    struct WebViewObserver;

    unsafe impl NSObjectProtocol for WebViewObserver {}

    impl WebViewObserver {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value_for_key_path(
            &self,
            _key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            _change: Option<&AnyObject>,
            _context: *mut c_void,
        ) {
            run_objc_callback(|| self.sample());
        }
    }
);

impl WebViewObserver {
    fn key_paths() -> [&'static NSString; 3] {
        [
            ns_string!("estimatedProgress"),
            ns_string!("title"),
            ns_string!("URL"),
        ]
    }

    fn new(
        mtm: MainThreadMarker,
        id: WebViewId,
        generation: WebViewGeneration,
        web: Retained<WKWebView>,
        state: Rc<RefCell<PageLoadState>>,
        events: Rc<RefCell<Vec<WebViewEvent>>>,
        wake: WebViewWake,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WebViewObserverIvars {
            id,
            generation,
            web,
            state,
            events,
            wake,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        for key_path in Self::key_paths() {
            // SAFETY: the observer outlives the registration; `unregister`
            // runs before the owning `MacWebView` releases it.
            unsafe {
                this.ivars().web.addObserver_forKeyPath_options_context(
                    &this,
                    key_path,
                    NSKeyValueObservingOptions::New,
                    ptr::null_mut(),
                );
            }
        }
        this
    }

    fn unregister(&self) {
        for key_path in Self::key_paths() {
            // SAFETY: every key path was registered in `new` with this
            // observer, and KVO tolerates the removal order.
            unsafe { self.ivars().web.removeObserver_forKeyPath(self, key_path) };
        }
    }

    fn sample(&self) {
        let ivars = self.ivars();
        let (title, uri, progress) = sample_web_view(&ivars.web);
        let events =
            ivars
                .state
                .borrow_mut()
                .observe(ivars.id, ivars.generation, title, uri, progress);
        if !events.is_empty() {
            ivars.events.borrow_mut().extend(events);
            ivars.wake.notify();
        }
    }
}

/// The three observed properties, read together so one event batch reflects
/// one moment of the page.
fn sample_web_view(web: &WKWebView) -> (Option<String>, Option<String>, f64) {
    let title = unsafe { web.title() }.map(|value| value.to_string());
    let uri = unsafe { web.URL() }
        .and_then(|url| url.absoluteString())
        .map(|value| value.to_string());
    let progress = unsafe { web.estimatedProgress() };
    (title, uri, progress)
}

pub(crate) struct EmacsWebViewIvars {
    /// The host view Emacs draws in and winit listens on; key events the
    /// page does not keep are delivered to its `keyDown:`, exactly as GNU
    /// forwards to `emacswindow` (nsxwidget.m:244-278).
    emacs_view: RefCell<Option<Retained<NSView>>>,
    /// Bumped on every change of `emacs_view`, so a key probe that completes
    /// after the host changed can tell (see `HostEpoch`).
    host_epoch: Cell<HostEpoch>,
}

define_class!(
    /// WKWebView with GNU's keyboard model (nsxwidget.m:239-325): a key that
    /// reaches the web view stays with Emacs unless the page answers
    /// `xwHasFocus()` with true, and `interpretKeyEvents:` is a no-op so
    /// Emacs alone collects key events.
    #[unsafe(super(WKWebView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = EmacsWebViewIvars]
    #[name = "NeomacsWebView"]
    pub(crate) struct EmacsWebView;

    impl EmacsWebView {
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if self.ivars().emacs_view.borrow().is_none() {
                // Not presented in any Emacs frame: nothing to forward to.
                unsafe {
                    let _: () = msg_send![super(self), keyDown: event];
                }
                return;
            }
            let sent = self.ivars().host_epoch.get();
            let event = event.retain();
            let this = self.retain();
            let completion = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
                run_objc_callback(|| {
                    let probe = classify_focus_probe(result, error);
                    let now = this.ivars().host_epoch.get();
                    match route_key(probe, sent, now) {
                        KeyRoute::Emacs => {
                            // Re-read the host: the epoch proved it unchanged,
                            // so this is the view the key was typed into.
                            let emacs_view = this.ivars().emacs_view.borrow().clone();
                            if let Some(emacs_view) = emacs_view {
                                emacs_view.keyDown(&event);
                            }
                        }
                        KeyRoute::WebView => unsafe {
                            let _: () = msg_send![super(&*this), keyDown: &*event];
                        },
                        KeyRoute::Dropped => {}
                    }
                });
            });
            unsafe {
                self.evaluateJavaScript_completionHandler(
                    ns_string!(FOCUS_PROBE),
                    Some(&completion),
                );
            }
        }

        /// GNU: "do nothing and do not forward ... to let emacs collect key
        /// events and ask interpretKeyEvents to its superclass".
        #[unsafe(method(interpretKeyEvents:))]
        fn interpret_key_events(&self, _events: &AnyObject) {}
    }
);

/// Classify what `evaluateJavaScript:completionHandler:` handed back for
/// `xwHasFocus()` without trusting its class: page JavaScript can redefine
/// the function and return anything.  WebKit bridges JavaScript Booleans to
/// `CFBoolean`, which is an `NSNumber` subclass but has a distinct Core
/// Foundation type ID; ordinary numeric zero and one are not accepted.
fn classify_focus_probe(result: *mut AnyObject, error: *mut NSError) -> FocusProbe {
    if !error.is_null() {
        return FocusProbe::Failed;
    }
    // SAFETY: WebKit passes a live object (or nil) for the duration of the
    // completion block; it is only borrowed here.
    let Some(result) = (unsafe { result.as_ref() }) else {
        return FocusProbe::Absent;
    };
    let Some(number) = result.downcast_ref::<NSNumber>() else {
        return FocusProbe::NotABoolean;
    };
    // SAFETY: NSNumber is toll-free bridged to CFNumber/CFBoolean, so the
    // object may be borrowed as CFType for the exact type-ID check.
    let value = unsafe { &*std::ptr::from_ref(number).cast::<CFType>() };
    let Some(boolean) = value.downcast_ref::<CFBoolean>() else {
        return FocusProbe::NotABoolean;
    };
    if boolean.value() {
        FocusProbe::Focused
    } else {
        FocusProbe::Unfocused
    }
}

#[cfg(test)]
mod focus_probe_tests {
    use super::*;

    fn classify_number(number: &Retained<NSNumber>) -> FocusProbe {
        classify_focus_probe(
            Retained::as_ptr(number).cast::<AnyObject>().cast_mut(),
            ptr::null_mut(),
        )
    }

    #[test]
    fn only_core_foundation_booleans_are_focus_answers() {
        assert_eq!(
            classify_number(&NSNumber::new_bool(true)),
            FocusProbe::Focused
        );
        assert_eq!(
            classify_number(&NSNumber::new_bool(false)),
            FocusProbe::Unfocused
        );
        assert_eq!(
            classify_number(&NSNumber::new_i32(1)),
            FocusProbe::NotABoolean,
            "numeric one is truthy but is not a JavaScript Boolean"
        );
        assert_eq!(
            classify_number(&NSNumber::new_i32(0)),
            FocusProbe::NotABoolean,
            "numeric zero is falsey but is not a JavaScript Boolean"
        );
    }

    #[test]
    fn objective_c_callback_panics_are_contained() {
        let escaped = std::panic::catch_unwind(|| {
            run_objc_callback(|| panic!("synthetic callback panic"));
        });
        assert!(escaped.is_ok(), "a panic must not unwind into Objective-C");
    }
}

impl EmacsWebView {
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        configuration: &WKWebViewConfiguration,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(EmacsWebViewIvars {
            emacs_view: RefCell::new(None),
            host_epoch: Cell::new(HostEpoch::first()),
        });
        // SAFETY: `initWithFrame:configuration:` is WKWebView's designated
        // initializer; the subclass adds only the ivars set above.
        unsafe { msg_send![super(this), initWithFrame: frame, configuration: configuration] }
    }

    /// Change (or clear) the Emacs host view keys are forwarded to.  Every
    /// change opens a new host epoch, so key probes issued against the old
    /// host complete into nothing.
    fn set_emacs_view(&self, view: Option<Retained<NSView>>) {
        let ivars = self.ivars();
        *ivars.emacs_view.borrow_mut() = view;
        ivars.host_epoch.set(ivars.host_epoch.get().next());
    }

    /// Whether `view` is, by identity, the host keys are forwarded to.  A
    /// retained view cannot be freed and its address reused, so pointer
    /// equality is the identity.
    fn hosted_by(&self, view: &NSView) -> bool {
        self.ivars()
            .emacs_view
            .borrow()
            .as_deref()
            .is_some_and(|hosted| ptr::eq::<NSView>(hosted, view))
    }

    /// Hand first responder back to the Emacs view (GNU's answer to the
    /// page's "C-g", nsxwidget.m:317-321), if the view is in a window.
    fn give_focus_back_to_emacs(&self) {
        if let (Some(window), Some(emacs_view)) =
            (self.window(), self.ivars().emacs_view.borrow().as_deref())
        {
            let responder: &NSResponder = emacs_view;
            let _ = window.makeFirstResponder(Some(responder));
        }
    }
}

struct KeyDownMessageHandlerIvars {
    web: Retained<EmacsWebView>,
}

define_class!(
    /// The `keyDown` script-message handler GNU registers: the page posts
    /// "C-g" from its keydown listener, and focus returns to Emacs without
    /// relaying the key ("another C-g follows will be handled by emacs").
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = KeyDownMessageHandlerIvars]
    #[name = "NeomacsWebViewKeyDownHandler"]
    struct KeyDownMessageHandler;

    unsafe impl NSObjectProtocol for KeyDownMessageHandler {}

    unsafe impl WKScriptMessageHandler for KeyDownMessageHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        #[allow(non_snake_case)]
        unsafe fn userContentController_didReceiveScriptMessage(
            &self,
            _controller: &WKUserContentController,
            message: &WKScriptMessage,
        ) {
            run_objc_callback(|| {
                // The body is whatever JSON value the page posted; only a
                // string can be GNU's "C-g", so check the class before
                // reading it rather than sending `isEqualToString:` blind.
                let body = unsafe { message.body() };
                let text = body.downcast_ref::<NSString>().map(|text| text.to_string());
                match key_down_message(text.as_deref()) {
                    KeyDownMessage::GiveFocusBackToEmacs => {
                        self.ivars().web.give_focus_back_to_emacs();
                    }
                    KeyDownMessage::Ignored => {}
                }
            });
        }
    }
);

impl KeyDownMessageHandler {
    fn new(mtm: MainThreadMarker, web: Retained<EmacsWebView>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(KeyDownMessageHandlerIvars { web });
        unsafe { msg_send![super(this), init] }
    }
}

/// Native objects for one logical WebView generation.
pub(crate) struct MacWebView {
    id: WebViewId,
    generation: WebViewGeneration,
    mtm: MainThreadMarker,
    clip: Retained<WebViewClipView>,
    web: Retained<EmacsWebView>,
    _navigation_delegate: Retained<WebViewNavigationDelegate>,
    observer: Retained<WebViewObserver>,
    _key_down_handler: Retained<KeyDownMessageHandler>,
    /// The host window the clip view is a subview of.  The native view that
    /// stands for it is held once, by `EmacsWebView`, and checked by identity
    /// on every `present`: `register_host` may replace the `NSView` behind an
    /// unchanged `HostWindowId` (winit recreates the window; the Emacs frame
    /// id is reused), and a clip view left in the old view would keep drawing
    /// into, and forwarding keys to, a window that is gone.
    attached: Option<HostWindowId>,
    hidden: bool,
    load_state: Rc<RefCell<PageLoadState>>,
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
        let web = EmacsWebView::new(mtm, frame, &configuration);
        if policy.developer_tools() && objc2::available!(macos = 13.3) {
            unsafe { web.setInspectable(true) };
        }
        // GNU's keyboard model needs its script in every page and a handler
        // for the page's "C-g" (nsxwidget.m:93-99).
        let key_down_handler = KeyDownMessageHandler::new(mtm, web.clone());
        unsafe {
            let scriptor = configuration.userContentController();
            scriptor.addScriptMessageHandler_name(
                ProtocolObject::from_ref(&*key_down_handler),
                ns_string!(KEY_DOWN_MESSAGE_HANDLER),
            );
            let script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(mtm),
                ns_string!(GNU_XW_SCRIPT),
                WKUserScriptInjectionTime::AtDocumentStart,
                false,
            );
            scriptor.addUserScript(&script);
        }
        clip.addSubview(&web);
        let events = Rc::new(RefCell::new(Vec::new()));
        let load_state = Rc::new(RefCell::new(PageLoadState::new()));
        let navigation_delegate = WebViewNavigationDelegate::new(
            mtm,
            id,
            generation,
            load_state.clone(),
            events.clone(),
            wake.clone(),
        );
        unsafe {
            web.setNavigationDelegate(Some(ProtocolObject::from_ref(&*navigation_delegate)));
        }
        let observer = WebViewObserver::new(
            mtm,
            id,
            generation,
            Retained::into_super(web.clone()),
            load_state.clone(),
            events.clone(),
            wake.clone(),
        );

        Self {
            id,
            generation,
            mtm,
            clip,
            web,
            _navigation_delegate: navigation_delegate,
            observer,
            _key_down_handler: key_down_handler,
            attached: None,
            hidden: true,
            load_state,
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

    /// Move keyboard focus between Emacs and the page.
    ///
    /// Only `-[NSWindow makeFirstResponder:]` changes where AppKit sends key
    /// events; calling `becomeFirstResponder` on the view itself does not.
    /// GNU uses the same call in both directions (nsxwidget.m:250, :321).
    /// A view that is not in a window cannot take focus, and a request the
    /// window refuses is not a focus change.
    pub(crate) fn focus(&mut self, intent: FocusIntent) {
        let accepted = match (self.web.window(), intent) {
            (None, _) => false,
            (Some(window), FocusIntent::Focus) => {
                let responder: &NSResponder = &self.web;
                window.makeFirstResponder(Some(responder))
            }
            (Some(window), FocusIntent::Blur) => {
                let emacs_view = self.web.ivars().emacs_view.borrow();
                let responder = emacs_view.as_deref().map(|view| {
                    let responder: &NSResponder = view;
                    responder
                });
                window.makeFirstResponder(responder)
            }
        };
        if let Some(focused) = focus_transition(self.focused, intent, accepted) {
            self.focused = focused;
            self.events.borrow_mut().push(WebViewEvent::FocusChanged {
                id: self.id,
                generation: self.generation,
                focused,
            });
        }
    }

    /// Drain the events the delegate, the KVO observer and the script
    /// completions queued, plus one fallback sample of the observed page
    /// properties in case a KVO notification was coalesced away.
    pub(crate) fn service_events(&mut self) -> Vec<WebViewEvent> {
        let mut events = std::mem::take(&mut *self.events.borrow_mut());
        let (title, uri, progress) = sample_web_view(&self.web);
        events.extend(self.load_state.borrow_mut().observe(
            self.id,
            self.generation,
            title,
            uri,
            progress,
        ));
        events
    }

    pub(crate) fn present(
        &mut self,
        host_id: HostWindowId,
        host: &NSView,
        placement: &ResolvedWebViewPlacement,
    ) {
        if self.attached != Some(host_id) || !self.web.hosted_by(host) {
            self.clip.removeFromSuperview();
            host.addSubview(&self.clip);
            self.attached = Some(host_id);
            self.web.set_emacs_view(Some(host.retain()));
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

    /// Take the view out of its host.
    ///
    /// The system sends `Hidden` both when the view leaves the scene and,
    /// from `unregister_host`, when its host window is going away; GNU's
    /// `nsxwidget_delete_view` (nsxwidget.m:586-596) removes the view in
    /// the second case.  This contract has one hidden state, so hiding
    /// always detaches: the clip leaves the host's hierarchy, the host is
    /// released, and the host epoch advances so a key probe in flight
    /// completes into nothing.  A later `present` attaches again.
    pub(crate) fn hide(&mut self) {
        if !self.hidden {
            self.clip.setHidden(true);
            self.hidden = true;
        }
        if self.attached.take().is_some() {
            self.clip.removeFromSuperview();
            self.web.set_emacs_view(None);
        }
    }
}

impl Drop for MacWebView {
    fn drop(&mut self) {
        // KVO registrations must not outlive the observer.
        self.observer.unregister();
        // The content controller retains the message handler, which retains
        // the view: break the cycle the way `nsxwidget_kill` does.
        unsafe {
            let scriptor = self.web.configuration().userContentController();
            scriptor.removeAllUserScripts();
            scriptor.removeScriptMessageHandlerForName(ns_string!(KEY_DOWN_MESSAGE_HANDLER));
        }
        self.web.set_emacs_view(None);
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
