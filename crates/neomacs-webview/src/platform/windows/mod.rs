//! Windows backend using WebView2's composition controller.
//!
//! A composition controller does not create a child HWND. Its WebView visual
//! is inserted into a DirectComposition tree above the wgpu surface, while
//! pointer input is forwarded explicitly through `SendMouseInput`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    CoTaskMemPWSTR, CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
    FocusChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    ProcessFailedEventHandler,
};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDevice, IDCompositionRectangleClip,
    IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::core::{HSTRING, IUnknown, Interface, PCWSTR, PWSTR};

use crate::backend::{
    BackendEvent, CreateOutcome, HostRegistration, MissingPrerequisites, NavigationMilestone,
    Platform, PlatformCreateRequest, PlatformPresentation, PlatformUpdate,
};
use crate::{
    BrowsingRelationship, ButtonState, FocusIntent, HistoryAction, HostWindowId, NavigationTarget,
    PointerButton, ScriptError, ScriptWorld, StoragePartition, WebContentPoint, WebProcessFailure,
    WebValue, WebViewEvent, WebViewGeneration, WebViewHost, WebViewId, WebViewInput,
    WebViewModifiers, WebViewScrollDelta, WebViewSystemConfig, WebViewWake,
};

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        // SAFETY: WebViewSystem is thread-affine and this guard balances the
        // successful apartment initialization on that same thread.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("failed to initialize the WebView2 STA: {error}"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: paired with the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
}

struct HostComposition {
    hwnd: HWND,
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    root: IDCompositionVisual,
}

impl HostComposition {
    fn new(hwnd: HWND, device: IDCompositionDevice) -> Result<Self, String> {
        let target = unsafe { device.CreateTargetForHwnd(hwnd, true) }
            .map_err(|error| format!("failed to create DirectComposition HWND target: {error}"))?;
        let root = unsafe { device.CreateVisual() }
            .map_err(|error| format!("failed to create DirectComposition root visual: {error}"))?;
        unsafe {
            target
                .SetRoot(&root)
                .and_then(|()| device.Commit())
                .map_err(|error| format!("failed to install DirectComposition root: {error}"))?;
        }
        Ok(Self {
            hwnd,
            device,
            _target: target,
            root,
        })
    }

    fn add_webview_visual(
        &self,
    ) -> Result<(IDCompositionVisual, IDCompositionRectangleClip), String> {
        let visual = unsafe { self.device.CreateVisual() }
            .map_err(|error| format!("failed to create WebView2 visual: {error}"))?;
        let clip = unsafe { self.device.CreateRectangleClip() }
            .map_err(|error| format!("failed to create WebView2 clip: {error}"))?;
        unsafe {
            visual
                .SetClip(&clip)
                .and_then(|()| {
                    self.root
                        .AddVisual(&visual, true, None::<&IDCompositionVisual>)
                })
                .and_then(|()| self.device.Commit())
                .map_err(|error| format!("failed to attach WebView2 visual: {error}"))?;
        }
        Ok((visual, clip))
    }
}

pub(crate) enum PendingWindowsView {
    AwaitingPresentation(PlatformCreateRequest),
    CreatingController,
}

enum WindowsEnvironmentState {
    Creating,
    Ready(ICoreWebView2Environment),
    Failed(String),
}

pub(crate) struct WindowsWebView {
    host: HostWindowId,
    host_hwnd: HWND,
    composition: ICoreWebView2CompositionController,
    controller: ICoreWebView2Controller,
    controller3: ICoreWebView2Controller3,
    core: ICoreWebView2,
    visual: IDCompositionVisual,
    clip: IDCompositionRectangleClip,
    id: WebViewId,
    generation: WebViewGeneration,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
    title: String,
    uri: String,
    navigation_starting_token: i64,
    navigation_completed_token: i64,
    process_failed_token: i64,
    got_focus_token: i64,
    lost_focus_token: i64,
}

impl WindowsWebView {
    fn from_composition_controller(
        request: PlatformCreateRequest,
        generation: WebViewGeneration,
        host: HostWindowId,
        host_hwnd: HWND,
        visual: IDCompositionVisual,
        clip: IDCompositionRectangleClip,
        composition: ICoreWebView2CompositionController,
        wake: WebViewWake,
    ) -> Result<Self, String> {
        let controller: ICoreWebView2Controller = composition.cast().map_err(|error| {
            format!("WebView2 composition controller lacks base controller: {error}")
        })?;
        let controller3: ICoreWebView2Controller3 = controller
            .cast()
            .map_err(|error| format!("WebView2 runtime lacks raw-pixel bounds support: {error}"))?;
        let core = unsafe { controller.CoreWebView2() }
            .map_err(|error| format!("failed to acquire WebView2 core: {error}"))?;
        unsafe {
            composition
                .SetRootVisualTarget(&visual)
                .and_then(|()| controller.SetIsVisible(false))
                .map_err(|error| {
                    format!("failed to connect WebView2 composition visual: {error}")
                })?;
            let settings = core
                .Settings()
                .map_err(|error| format!("failed to acquire WebView2 settings: {error}"))?;
            settings
                .SetIsScriptEnabled(request.policy().javascript())
                .and_then(|()| settings.SetAreDevToolsEnabled(request.policy().developer_tools()))
                .map_err(|error| format!("failed to apply WebView2 policy: {error}"))?;
        }

        let mut view = Self {
            host,
            host_hwnd,
            composition,
            controller,
            controller3,
            core,
            visual,
            clip,
            id: request.id(),
            generation,
            events: Rc::new(RefCell::new(Vec::new())),
            wake,
            title: String::new(),
            uri: String::new(),
            navigation_starting_token: 0,
            navigation_completed_token: 0,
            process_failed_token: 0,
            got_focus_token: 0,
            lost_focus_token: 0,
        };
        view.install_event_handlers()?;
        if let Some(navigation) = request.navigation() {
            view.update_navigation(navigation)?;
        }
        Ok(view)
    }

    fn update_navigation(&self, target: &NavigationTarget) -> Result<(), String> {
        match target {
            NavigationTarget::Uri(uri) => unsafe {
                self.core.Navigate(&HSTRING::from(uri))
            },
            NavigationTarget::Html { contents, base_uri } => {
                if base_uri.is_some() {
                    return Err(
                        "WebView2 NavigateToString cannot assign an HTML base URI; use a URI or file target"
                            .to_owned(),
                    );
                }
                unsafe { self.core.NavigateToString(&HSTRING::from(contents)) }
            }
            NavigationTarget::File(path) => {
                let uri = url::Url::from_file_path(path)
                    .map_err(|()| format!("cannot convert file path {path:?} to a URI"))?;
                unsafe { self.core.Navigate(&HSTRING::from(uri.as_str())) }
            }
        }
        .map_err(|error| format!("WebView2 navigation failed: {error}"))
    }

    fn history(&self, action: HistoryAction) -> Result<(), String> {
        unsafe {
            match action {
                HistoryAction::Back => self.core.GoBack(),
                HistoryAction::Forward => self.core.GoForward(),
                HistoryAction::Reload => self.core.Reload(),
            }
        }
        .map_err(|error| format!("WebView2 history operation failed: {error}"))
    }

    fn evaluate_script(&self, request: &crate::ScriptRequest) -> Result<(), String> {
        if request.world == ScriptWorld::Isolated {
            return Err(
                "WebView2 does not expose an isolated world through ExecuteScript".to_owned(),
            );
        }
        let events = self.events.clone();
        let id = self.id;
        let generation = self.generation;
        let request_id = request.request;
        let wake = self.wake.clone();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |error, json| {
            let result = match error {
                Ok(()) => serde_json::from_str(&json)
                    .map(WebValue::from_json)
                    .map_err(|error| ScriptError::Rejected(error.to_string())),
                Err(error) => Err(ScriptError::Rejected(error.to_string())),
            };
            events.borrow_mut().push(WebViewEvent::ScriptFinished {
                view: id,
                generation,
                request: request_id,
                result,
            });
            wake.notify();
            Ok(())
        }));
        unsafe {
            self.core
                .ExecuteScript(&HSTRING::from(&request.source), &handler)
        }
        .map_err(|error| format!("WebView2 script evaluation failed: {error}"))
    }

    fn focus(&self) -> Result<(), String> {
        unsafe {
            self.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
        }
        .map_err(|error| format!("failed to focus WebView2: {error}"))
    }

    fn install_event_handlers(&mut self) -> Result<(), String> {
        let starting_events = self.events.clone();
        let starting_wake = self.wake.clone();
        let id = self.id;
        let generation = self.generation;
        let navigation_starting =
            NavigationStartingEventHandler::create(Box::new(move |_sender, _args| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    starting_events
                        .borrow_mut()
                        .extend(NavigationMilestone::Started.normalized_events(id, generation));
                    starting_wake.notify();
                }));
                Ok(())
            }));
        unsafe {
            self.core
                .add_NavigationStarting(&navigation_starting, &mut self.navigation_starting_token)
        }
        .map_err(|error| format!("failed to observe WebView2 navigation start: {error}"))?;

        let navigation_events = self.events.clone();
        let navigation_wake = self.wake.clone();
        let navigation =
            NavigationCompletedEventHandler::create(Box::new(move |_sender, _args| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    navigation_events
                        .borrow_mut()
                        .extend(NavigationMilestone::Finished.normalized_events(id, generation));
                    navigation_wake.notify();
                }));
                Ok(())
            }));
        unsafe {
            self.core
                .add_NavigationCompleted(&navigation, &mut self.navigation_completed_token)
        }
        .map_err(|error| format!("failed to observe WebView2 navigation: {error}"))?;

        let process_events = self.events.clone();
        let process_wake = self.wake.clone();
        let process_failed = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let failure = args
                    .as_ref()
                    .map(webview2_process_failure)
                    .unwrap_or(WebProcessFailure::Other(-1));
                process_events
                    .borrow_mut()
                    .push(WebViewEvent::ProcessFailed {
                        id,
                        generation,
                        failure,
                    });
                process_wake.notify();
            }));
            Ok(())
        }));
        unsafe {
            self.core
                .add_ProcessFailed(&process_failed, &mut self.process_failed_token)
        }
        .map_err(|error| format!("failed to observe WebView2 process failure: {error}"))?;

        let install_focus = |focused: bool, token: &mut i64| -> Result<(), String> {
            let events = self.events.clone();
            let wake = self.wake.clone();
            let handler = FocusChangedEventHandler::create(Box::new(move |_sender, _args| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    events.borrow_mut().push(WebViewEvent::FocusChanged {
                        id,
                        generation,
                        focused,
                    });
                    wake.notify();
                }));
                Ok(())
            }));
            let result = unsafe {
                if focused {
                    self.controller.add_GotFocus(&handler, token)
                } else {
                    self.controller.add_LostFocus(&handler, token)
                }
            };
            result.map_err(|error| format!("failed to observe WebView2 focus: {error}"))
        };
        let mut got_focus_token = 0;
        install_focus(true, &mut got_focus_token)?;
        self.got_focus_token = got_focus_token;
        let mut lost_focus_token = 0;
        install_focus(false, &mut lost_focus_token)?;
        self.lost_focus_token = lost_focus_token;
        Ok(())
    }

    fn service_events(&mut self) -> Vec<WebViewEvent> {
        let mut events = std::mem::take(&mut *self.events.borrow_mut());
        if let Ok(title) = webview2_string(|value| unsafe { self.core.DocumentTitle(value) })
            && title != self.title
        {
            self.title = title.clone();
            events.push(WebViewEvent::TitleChanged {
                id: self.id,
                generation: self.generation,
                title,
            });
        }
        if let Ok(uri) = webview2_string(|value| unsafe { self.core.Source(value) })
            && uri != self.uri
        {
            self.uri = uri.clone();
            events.push(WebViewEvent::UriChanged {
                id: self.id,
                generation: self.generation,
                uri,
            });
        }
        events
    }

    fn set_model_size(&self, width: u32, height: u32) -> Result<(), String> {
        unsafe {
            self.controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: saturating_i32(width),
                bottom: saturating_i32(height),
            })
        }
        .map_err(|error| format!("failed to resize WebView2: {error}"))
    }

    fn present(
        &self,
        host: &HostComposition,
        placement: &crate::ResolvedWebViewPlacement,
    ) -> Result<(), String> {
        let content = placement.content_rect();
        let visible = placement.visible_rect();
        let offset = placement.content_offset();
        unsafe {
            self.controller3
                .SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)
                .and_then(|()| {
                    self.controller3
                        .SetRasterizationScale(f64::from(placement.device_scale().get()))
                })
                .and_then(|()| {
                    self.controller.SetBounds(RECT {
                        left: 0,
                        top: 0,
                        right: round_i32(content.width()),
                        bottom: round_i32(content.height()),
                    })
                })
                .and_then(|()| self.visual.SetOffsetX2(content.x()))
                .and_then(|()| self.visual.SetOffsetY2(content.y()))
                .and_then(|()| self.clip.SetLeft2(offset.x()))
                .and_then(|()| self.clip.SetTop2(offset.y()))
                .and_then(|()| self.clip.SetRight2(offset.x() + visible.width()))
                .and_then(|()| self.clip.SetBottom2(offset.y() + visible.height()))
                .and_then(|()| self.controller.SetIsVisible(true))
                .and_then(|()| host.device.Commit())
                .map_err(|error| {
                    format!("failed to present WebView2 composition visual: {error}")
                })?;
        }
        Ok(())
    }

    fn hide(&self, host: &HostComposition) -> Result<(), String> {
        unsafe {
            self.controller
                .SetIsVisible(false)
                .and_then(|()| host.device.Commit())
        }
        .map_err(|error| format!("failed to hide WebView2: {error}"))
    }

    fn send_input(&self, input: WebViewInput) -> Result<(), String> {
        let (kind, modifiers, data, point) = match input {
            // Once focused, WebView2 receives keyboard messages from its
            // parent HWND. Injecting the winit copy would type every key twice.
            WebViewInput::Keyboard { .. } => return Ok(()),
            WebViewInput::PointerMove {
                position,
                modifiers,
            } => (
                COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
                mouse_modifiers(modifiers),
                0,
                webview_point(position),
            ),
            WebViewInput::PointerButton {
                position,
                button,
                state,
                modifiers,
            } => {
                let (kind, data, held) = mouse_button(button, state)?;
                (
                    kind,
                    mouse_modifiers(modifiers) | held,
                    data,
                    webview_point(position),
                )
            }
            WebViewInput::Scroll {
                position,
                delta,
                modifiers,
            } => {
                let (kind, delta) = match delta {
                    WebViewScrollDelta::Lines { x, y } if y.abs() >= x.abs() => {
                        (COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, y * 120.0)
                    }
                    WebViewScrollDelta::Lines { x, .. } => {
                        (COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL, x * 120.0)
                    }
                    WebViewScrollDelta::Pixels { x, y } if y.abs() >= x.abs() => {
                        (COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, y)
                    }
                    WebViewScrollDelta::Pixels { x, .. } => {
                        (COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL, x)
                    }
                };
                (
                    kind,
                    mouse_modifiers(modifiers),
                    wheel_data(delta),
                    webview_point(position),
                )
            }
        };
        unsafe {
            self.composition
                .SendMouseInput(kind, modifiers, data, point)
        }
        .map_err(|error| format!("failed to forward WebView2 pointer input: {error}"))
    }
}

impl Drop for WindowsWebView {
    fn drop(&mut self) {
        if self.navigation_starting_token != 0 {
            let _ = unsafe {
                self.core
                    .remove_NavigationStarting(self.navigation_starting_token)
            };
        }
        if self.navigation_completed_token != 0 {
            let _ = unsafe {
                self.core
                    .remove_NavigationCompleted(self.navigation_completed_token)
            };
        }
        if self.process_failed_token != 0 {
            let _ = unsafe { self.core.remove_ProcessFailed(self.process_failed_token) };
        }
        if self.got_focus_token != 0 {
            let _ = unsafe { self.controller.remove_GotFocus(self.got_focus_token) };
        }
        if self.lost_focus_token != 0 {
            let _ = unsafe { self.controller.remove_LostFocus(self.lost_focus_token) };
        }
        let _ = unsafe { self.controller.Close() };
    }
}

fn webview2_process_failure(args: &ICoreWebView2ProcessFailedEventArgs) -> WebProcessFailure {
    if let Ok(args2) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
        let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
        if unsafe { args2.Reason(&mut reason) }.is_ok() {
            return match reason {
                COREWEBVIEW2_PROCESS_FAILED_REASON_CRASHED => WebProcessFailure::Crashed,
                COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY => {
                    WebProcessFailure::ExceededMemoryLimit
                }
                COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED
                | COREWEBVIEW2_PROCESS_FAILED_REASON_PROFILE_DELETED => {
                    WebProcessFailure::Terminated
                }
                COREWEBVIEW2_PROCESS_FAILED_REASON_UNRESPONSIVE => WebProcessFailure::Unresponsive,
                COREWEBVIEW2_PROCESS_FAILED_REASON_LAUNCH_FAILED => WebProcessFailure::LaunchFailed,
                other => WebProcessFailure::Other(other.0),
            };
        }
    }

    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
    if unsafe { args.ProcessFailedKind(&mut kind) }.is_ok() {
        if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
            WebProcessFailure::Unresponsive
        } else {
            WebProcessFailure::Other(kind.0)
        }
    } else {
        WebProcessFailure::Other(-1)
    }
}

fn webview2_string(
    read: impl FnOnce(*mut PWSTR) -> windows_core::Result<()>,
) -> Result<String, String> {
    let mut raw = PWSTR::null();
    read(&mut raw).map_err(|error| error.to_string())?;
    Ok(CoTaskMemPWSTR::from(raw).to_string())
}

pub(crate) struct WindowsPlatform {
    _apartment: Option<ComApartment>,
    environment: Rc<RefCell<WindowsEnvironmentState>>,
    composition_device: Result<IDCompositionDevice, String>,
    hosts: HashMap<HostWindowId, HostComposition>,
    completions: Rc<RefCell<Vec<BackendEvent<WindowsWebView>>>>,
    wake: WebViewWake,
}

impl WindowsPlatform {
    pub(crate) fn new(config: WebViewSystemConfig, wake: WebViewWake) -> Self {
        match ComApartment::initialize() {
            Ok(apartment) => {
                let environment = Rc::new(RefCell::new(WindowsEnvironmentState::Creating));
                if let Err(error) = start_environment_creation(
                    config.profile_root.as_deref(),
                    environment.clone(),
                    wake.clone(),
                ) {
                    *environment.borrow_mut() = WindowsEnvironmentState::Failed(error);
                }
                // A software-independent DComp device is sufficient because
                // WebView2 owns the content supplied to each child visual.
                let composition_device = unsafe { DCompositionCreateDevice2(None::<&IUnknown>) }
                    .map_err(|error| format!("failed to create DirectComposition device: {error}"));
                Self {
                    _apartment: Some(apartment),
                    environment,
                    composition_device,
                    hosts: HashMap::new(),
                    completions: Rc::new(RefCell::new(Vec::new())),
                    wake,
                }
            }
            Err(error) => {
                let environment =
                    Rc::new(RefCell::new(WindowsEnvironmentState::Failed(error.clone())));
                Self {
                    _apartment: None,
                    environment,
                    composition_device: Err(error),
                    hosts: HashMap::new(),
                    completions: Rc::new(RefCell::new(Vec::new())),
                    wake,
                }
            }
        }
    }

    fn start_controller_creation(
        &mut self,
        request: PlatformCreateRequest,
        generation: WebViewGeneration,
        host_id: HostWindowId,
    ) -> Result<(), String> {
        let environment = match &*self.environment.borrow() {
            WindowsEnvironmentState::Ready(environment) => environment.clone(),
            WindowsEnvironmentState::Creating => {
                return Err("WebView2 environment is still being created".to_owned());
            }
            WindowsEnvironmentState::Failed(error) => return Err(error.clone()),
        };
        let host = self
            .hosts
            .get(&host_id)
            .ok_or_else(|| format!("host {host_id:?} has no registered HWND"))?;
        let (visual, clip) = host.add_webview_visual()?;
        let environment10: ICoreWebView2Environment10 = environment.cast().map_err(|error| {
            format!("installed WebView2 runtime lacks controller options: {error}")
        })?;
        let options = unsafe { environment10.CreateCoreWebView2ControllerOptions() }
            .map_err(|error| format!("failed to create WebView2 controller options: {error}"))?;
        let (profile, private) = match request.storage() {
            StoragePartition::Persistent(profile) => (*profile, false),
            StoragePartition::Ephemeral(profile) => (*profile, true),
        };
        unsafe {
            options
                .SetProfileName(&HSTRING::from(format!("neomacs-{}", profile.get())))
                .and_then(|()| options.SetIsInPrivateModeEnabled(private))
                .map_err(|error| format!("failed to configure WebView2 profile: {error}"))?;
        }
        let hwnd = host.hwnd;
        let completions = self.completions.clone();
        let view_wake = self.wake.clone();
        let completion_wake = self.wake.clone();
        let id = request.id();
        let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
            move |status, composition| {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    status
                        .map_err(|error| {
                            format!("failed to create WebView2 composition controller: {error}")
                        })
                        .and_then(|()| {
                            composition.ok_or_else(|| {
                                "WebView2 returned no composition controller".to_owned()
                            })
                        })
                        .and_then(|composition| {
                            WindowsWebView::from_composition_controller(
                                request,
                                generation,
                                host_id,
                                hwnd,
                                visual,
                                clip,
                                composition,
                                view_wake,
                            )
                        })
                }))
                .unwrap_or_else(|_| {
                    Err("WebView2 controller completion callback panicked".to_owned())
                });
                completions.borrow_mut().push(BackendEvent::CreateFinished {
                    id,
                    generation,
                    result,
                });
                completion_wake.notify();
                Ok(())
            },
        ));
        unsafe {
            environment10
                .CreateCoreWebView2CompositionControllerWithOptions(hwnd, &options, &handler)
                .map_err(|error| {
                    format!("failed to start WebView2 composition controller creation: {error}")
                })?;
        }
        Ok(())
    }

    fn hwnd(host: &WebViewHost) -> Option<HWND> {
        let handle = host.window().window_handle().ok()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        Some(HWND(handle.hwnd.get() as *mut c_void))
    }
}

impl Platform for WindowsPlatform {
    type Host = WebViewHost;
    type PendingCreate = PendingWindowsView;
    type View = WindowsWebView;

    fn register_host(&mut self, id: HostWindowId, host: Self::Host) -> HostRegistration {
        let Some(hwnd) = Self::hwnd(&host) else {
            tracing::warn!(?id, "winit did not expose an HWND for WebView2 hosting");
            return HostRegistration::Unavailable;
        };
        if self.hosts.get(&id).is_some_and(|host| host.hwnd == hwnd) {
            return HostRegistration::Unchanged;
        }
        let device = match self.composition_device.as_ref() {
            Ok(device) => device.clone(),
            Err(error) => {
                tracing::error!(?id, %error, "DirectComposition is unavailable");
                return HostRegistration::Unavailable;
            }
        };
        match HostComposition::new(hwnd, device) {
            Ok(host) => {
                if self.hosts.insert(id, host).is_some() {
                    HostRegistration::Replaced
                } else {
                    HostRegistration::Added
                }
            }
            Err(error) => {
                tracing::error!(?id, %error, "failed to register WebView2 host");
                HostRegistration::Unavailable
            }
        }
    }

    fn unregister_host(&mut self, host: HostWindowId) {
        self.hosts.remove(&host);
    }

    fn missing_prerequisites(&self, _request: &PlatformCreateRequest) -> MissingPrerequisites {
        match &*self.environment.borrow() {
            WindowsEnvironmentState::Creating => MissingPrerequisites::RUNTIME,
            WindowsEnvironmentState::Ready(_) | WindowsEnvironmentState::Failed(_) => {
                MissingPrerequisites::empty()
            }
        }
    }

    fn begin_create(
        &mut self,
        request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String> {
        match &*self.environment.borrow() {
            WindowsEnvironmentState::Creating => {
                return Err("WebView2 environment is still being created".to_owned());
            }
            WindowsEnvironmentState::Ready(_) => {}
            WindowsEnvironmentState::Failed(error) => return Err(error.clone()),
        }
        match request.relationship() {
            BrowsingRelationship::Independent | BrowsingRelationship::Related(_) => {}
        }
        Ok(CreateOutcome::Pending(
            PendingWindowsView::AwaitingPresentation(request),
        ))
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>> {
        std::mem::take(&mut *self.completions.borrow_mut())
    }

    fn activate_pending(
        &mut self,
        generation: WebViewGeneration,
        pending: &mut Self::PendingCreate,
        presentation: PlatformPresentation<'_>,
    ) -> Result<Option<Self::View>, String> {
        match presentation {
            PlatformPresentation::Hidden => Ok(None),
            PlatformPresentation::Visible { host, .. } => {
                let PendingWindowsView::AwaitingPresentation(request) =
                    std::mem::replace(pending, PendingWindowsView::CreatingController)
                else {
                    return Ok(None);
                };
                self.start_controller_creation(request, generation, host)?;
                Ok(None)
            }
        }
    }

    fn service_view(
        &mut self,
        _id: WebViewId,
        _generation: WebViewGeneration,
        view: &mut Self::View,
    ) -> Vec<WebViewEvent> {
        view.service_events()
    }

    fn update(&mut self, view: &mut Self::View, update: PlatformUpdate<'_>) -> Result<(), String> {
        match update {
            PlatformUpdate::ModelSize(size) => view.set_model_size(size.width(), size.height()),
            PlatformUpdate::Navigation(target) => view.update_navigation(target),
            PlatformUpdate::History(action) => view.history(action),
            PlatformUpdate::EvaluateScript(request) => view.evaluate_script(request),
            PlatformUpdate::Focus(FocusIntent::Focus) => view.focus(),
            PlatformUpdate::Focus(FocusIntent::Blur) => {
                if let Some(host) = self.hosts.get(&view.host) {
                    let _ = unsafe { SetFocus(Some(host.hwnd)) };
                }
                Ok(())
            }
        }
    }

    fn input(
        &mut self,
        _generation: WebViewGeneration,
        view: &mut Self::View,
        input: WebViewInput,
    ) -> Result<(), String> {
        view.send_input(input)
    }

    fn present(
        &mut self,
        _generation: WebViewGeneration,
        view: &mut Self::View,
        presentation: PlatformPresentation<'_>,
    ) -> Result<(), String> {
        match presentation {
            PlatformPresentation::Hidden => {
                let host = self
                    .hosts
                    .get(&view.host)
                    .ok_or_else(|| format!("host {:?} is no longer registered", view.host))?;
                view.hide(host)
            }
            PlatformPresentation::Visible {
                host: requested,
                placement,
            } => {
                let next = self
                    .hosts
                    .get(&requested)
                    .ok_or_else(|| format!("host {requested:?} has no registered HWND"))?;
                let logical_host_changed = requested != view.host;
                let native_host_changed = next.hwnd != view.host_hwnd;
                if logical_host_changed || native_host_changed {
                    // When only the native HWND changed, registration already
                    // released the old HostComposition.  A logical host move
                    // can still detach explicitly from the registered old
                    // root before attaching to the next one.
                    if logical_host_changed && let Some(old) = self.hosts.get(&view.host) {
                        unsafe { old.root.RemoveVisual(&view.visual) }.map_err(|error| {
                            format!("failed to detach WebView2 from its old host: {error}")
                        })?;
                    }
                    unsafe {
                        next.root
                            .AddVisual(&view.visual, true, None::<&IDCompositionVisual>)
                            .and_then(|()| view.controller.SetParentWindow(next.hwnd))
                            .and_then(|()| next.device.Commit())
                            .map_err(|error| {
                                format!("failed to migrate WebView2 to its new host: {error}")
                            })?;
                    }
                    view.host = requested;
                    view.host_hwnd = next.hwnd;
                }
                view.present(next, placement)
            }
        }
    }

    fn close(&mut self, view: Self::View) {
        if let Some(host) = self.hosts.get(&view.host) {
            let _ = unsafe {
                host.root
                    .RemoveVisual(&view.visual)
                    .and_then(|()| host.device.Commit())
            };
        }
    }
}

fn start_environment_creation(
    user_data_folder: Option<&std::path::Path>,
    state: Rc<RefCell<WindowsEnvironmentState>>,
    wake: WebViewWake,
) -> Result<(), String> {
    let user_data_folder = user_data_folder.map(std::path::Path::to_path_buf);
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        move |status, environment| {
            let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                status
                    .map_err(|error| format!("failed to create WebView2 environment: {error}"))
                    .and_then(|()| {
                        environment.ok_or_else(|| "WebView2 returned no environment".to_owned())
                    })
            }))
            .unwrap_or_else(|_| {
                Err("WebView2 environment completion callback panicked".to_owned())
            });
            *state.borrow_mut() = match next {
                Ok(environment) => WindowsEnvironmentState::Ready(environment),
                Err(error) => WindowsEnvironmentState::Failed(error),
            };
            wake.notify();
            Ok(())
        },
    ));
    unsafe {
        match user_data_folder {
            Some(path) => CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &HSTRING::from(path.as_path()),
                None::<&ICoreWebView2EnvironmentOptions>,
                &handler,
            ),
            None => CreateCoreWebView2Environment(&handler),
        }
        .map_err(|error| format!("failed to start WebView2 environment creation: {error}"))?;
    }
    Ok(())
}

fn mouse_modifiers(modifiers: WebViewModifiers) -> COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS {
    let mut result = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
    if modifiers.contains(WebViewModifiers::CONTROL) {
        result |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_CONTROL;
    }
    if modifiers.contains(WebViewModifiers::SHIFT) {
        result |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_SHIFT;
    }
    result
}

fn mouse_button(
    button: PointerButton,
    state: ButtonState,
) -> Result<
    (
        COREWEBVIEW2_MOUSE_EVENT_KIND,
        u32,
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
    ),
    String,
> {
    let (down, up, data, held) = match button {
        PointerButton::Primary => (
            COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
            0,
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON,
        ),
        PointerButton::Middle => (
            COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP,
            0,
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_MIDDLE_BUTTON,
        ),
        PointerButton::Secondary => (
            COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
            0,
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON,
        ),
        PointerButton::Back => (
            COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP,
            1 << 16,
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_X_BUTTON1,
        ),
        PointerButton::Forward => (
            COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP,
            2 << 16,
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_X_BUTTON2,
        ),
        PointerButton::Other(button) => {
            return Err(format!("WebView2 cannot represent pointer button {button}"));
        }
    };
    Ok((
        match state {
            ButtonState::Pressed => down,
            ButtonState::Released => up,
        },
        data,
        match state {
            ButtonState::Pressed => held,
            ButtonState::Released => COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE,
        },
    ))
}

fn webview_point(point: WebContentPoint) -> POINT {
    POINT {
        x: round_i32(point.x()),
        y: round_i32(point.y()),
    }
}

fn wheel_data(delta: f32) -> u32 {
    let signed = round_i32(delta).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    u32::from(signed as u16) << 16
}

fn round_i32(value: f32) -> i32 {
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn saturating_i32(value: u32) -> i32 {
    value.min(i32::MAX as u32) as i32
}
