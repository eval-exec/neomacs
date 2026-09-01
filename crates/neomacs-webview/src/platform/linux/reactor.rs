//! Event-driven owner for Linux WPEPlatform state.
//!
//! The winit thread only holds [`WpeReactorHandle`]. Every WPE/GObject pointer
//! is created, called, and destroyed on this reactor thread, whose blocking
//! `GMainContext` is woken by typed commands. Native pointers therefore cannot
//! cross the thread boundary by construction.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use super::engine::WpeBackend;
use super::sys::platform as plat;
use super::view::{
    CapturedFrame, DmaBufData, NativeWpeBufferLease, WpeFrameTransport, WpeViewCreation,
    WpeViewState, WpeWebView,
};
use super::{LinuxProfileKey, NetworkSession, file_navigation_uri};
use crate::backend::{PlatformCreateRequest, PlatformUpdate};
use crate::model::{DmaBufFrameLease, DmaBufLease};
use crate::{
    DmaBufFrame, DmaBufPlane, FocusIntent, HistoryAction, NavigationTarget, PixelFrame,
    ScriptError, ScriptRequest, WebContentSize, WebViewEvent, WebViewFrame, WebViewGeneration,
    WebViewId, WebViewInput, WebViewSystemConfig, WebViewWake,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FrameLeaseId(u64);

enum WpeCommand {
    Create(PlatformCreateRequest),
    Resize {
        id: WebViewId,
        generation: WebViewGeneration,
        size: WebContentSize,
    },
    Navigate {
        id: WebViewId,
        generation: WebViewGeneration,
        target: NavigationTarget,
    },
    History {
        id: WebViewId,
        generation: WebViewGeneration,
        action: HistoryAction,
    },
    EvaluateScript {
        generation: WebViewGeneration,
        request: ScriptRequest,
    },
    Focus {
        id: WebViewId,
        generation: WebViewGeneration,
        intent: FocusIntent,
    },
    Input {
        id: WebViewId,
        generation: WebViewGeneration,
        input: WebViewInput,
    },
    Close {
        id: WebViewId,
        generation: WebViewGeneration,
    },
    ReleaseFrame(FrameLeaseId),
    Shutdown,
}

/// Thread-safe wake access to the reactor's GLib context.
///
/// The mutex closes the load/free race during shutdown: `clear_context` cannot
/// return while another thread is inside `g_main_context_wakeup`.
#[derive(Default)]
struct ReactorSignal {
    context: Mutex<Option<WakeableMainContext>>,
}

/// Typed cross-thread capability for GLib's explicitly thread-safe wake call.
/// It permits no access to the context other than `g_main_context_wakeup`.
#[derive(Clone, Copy)]
struct WakeableMainContext(NonNull<plat::GMainContext>);

// SAFETY: GLib documents g_main_context_wakeup as callable from any thread;
// the mutex in ReactorSignal serializes it with reactor teardown.
unsafe impl Send for WakeableMainContext {}
unsafe impl Sync for WakeableMainContext {}

impl ReactorSignal {
    fn install_context(&self, context: *mut plat::GMainContext) {
        *self.context.lock().expect("WPE reactor context") = Some(WakeableMainContext(
            NonNull::new(context).expect("WPE reactor context is null"),
        ));
    }

    fn clear_context(&self) {
        *self.context.lock().expect("WPE reactor context") = None;
    }

    fn wake(&self) {
        let context = self.context.lock().expect("WPE reactor context");
        if let Some(context) = *context {
            // SAFETY: `install_context` publishes one referenced context and
            // `clear_context` is serialized with this call before teardown.
            unsafe { plat::g_main_context_wakeup(context.0.as_ptr()) };
        }
    }
}

#[derive(Clone)]
struct ReactorSender {
    commands: mpsc::Sender<WpeCommand>,
    signal: Arc<ReactorSignal>,
}

impl ReactorSender {
    fn send(&self, command: WpeCommand) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "WPE reactor has stopped".to_owned())?;
        self.signal.wake();
        Ok(())
    }
}

struct RemoteWpeBufferLease {
    id: FrameLeaseId,
    reactor: ReactorSender,
}

impl DmaBufLease for RemoteWpeBufferLease {}

impl Drop for RemoteWpeBufferLease {
    fn drop(&mut self) {
        let _ = self.reactor.send(WpeCommand::ReleaseFrame(self.id));
    }
}

pub(super) enum ReactorEvent {
    CreateFinished {
        id: WebViewId,
        generation: WebViewGeneration,
        result: Result<(), String>,
    },
    View(WebViewEvent),
}

#[derive(Default)]
struct PendingFrames {
    latest: Option<WebViewFrame>,
}

impl PendingFrames {
    fn publish(&mut self, frame: WebViewFrame) {
        self.latest = Some(frame);
    }

    fn take(&mut self) -> Option<WebViewFrame> {
        self.latest.take()
    }

    fn is_empty(&self) -> bool {
        self.latest.is_none()
    }
}

#[derive(Default)]
struct ReactorMailbox {
    events: Mutex<Vec<ReactorEvent>>,
    frames: Mutex<HashMap<(WebViewId, WebViewGeneration), PendingFrames>>,
}

impl ReactorMailbox {
    fn publish_event(&self, event: ReactorEvent) {
        self.events.lock().expect("WPE reactor events").push(event);
    }

    fn drain_events(&self) -> Vec<ReactorEvent> {
        std::mem::take(&mut *self.events.lock().expect("WPE reactor events"))
    }

    fn publish_frame(&self, id: WebViewId, generation: WebViewGeneration, frame: WebViewFrame) {
        self.frames
            .lock()
            .expect("WPE reactor frames")
            .entry((id, generation))
            .or_default()
            .publish(frame);
    }

    fn take_frame(&self, id: WebViewId, generation: WebViewGeneration) -> Option<WebViewFrame> {
        let mut frames = self.frames.lock().expect("WPE reactor frames");
        let pending = frames.get_mut(&(id, generation))?;
        let frame = pending.take();
        if pending.is_empty() {
            frames.remove(&(id, generation));
        }
        frame
    }

    fn has_frame(&self, id: WebViewId, generation: WebViewGeneration) -> bool {
        self.frames
            .lock()
            .expect("WPE reactor frames")
            .get(&(id, generation))
            .is_some_and(|pending| !pending.is_empty())
    }

    fn discard_view(&self, id: WebViewId, generation: WebViewGeneration) {
        self.frames
            .lock()
            .expect("WPE reactor frames")
            .remove(&(id, generation));
    }
}

pub(super) struct WpeReactorHandle {
    sender: ReactorSender,
    mailbox: Arc<ReactorMailbox>,
    thread: Option<JoinHandle<()>>,
}

impl WpeReactorHandle {
    pub(super) fn spawn(config: WebViewSystemConfig, wake: WebViewWake) -> Self {
        let (commands, receiver) = mpsc::channel();
        let signal = Arc::new(ReactorSignal::default());
        let mailbox = Arc::new(ReactorMailbox::default());
        let sender = ReactorSender {
            commands,
            signal: Arc::clone(&signal),
        };
        let thread_mailbox = Arc::clone(&mailbox);
        let thread_sender = sender.clone();
        let thread = std::thread::Builder::new()
            .name("neomacs-wpe-reactor".to_owned())
            .spawn(move || {
                run_reactor(
                    config,
                    receiver,
                    thread_sender,
                    signal,
                    thread_mailbox,
                    wake,
                );
            })
            .expect("failed to spawn WPE reactor thread");
        Self {
            sender,
            mailbox,
            thread: Some(thread),
        }
    }

    pub(super) fn create(&self, request: PlatformCreateRequest) -> Result<(), String> {
        self.sender.send(WpeCommand::Create(request))
    }

    pub(super) fn update(
        &self,
        id: WebViewId,
        generation: WebViewGeneration,
        update: PlatformUpdate<'_>,
    ) -> Result<(), String> {
        let command = match update {
            PlatformUpdate::ModelSize(size) => WpeCommand::Resize {
                id,
                generation,
                size,
            },
            PlatformUpdate::Navigation(target) => WpeCommand::Navigate {
                id,
                generation,
                target: target.clone(),
            },
            PlatformUpdate::History(action) => WpeCommand::History {
                id,
                generation,
                action,
            },
            PlatformUpdate::EvaluateScript(request) => WpeCommand::EvaluateScript {
                generation,
                request: request.clone(),
            },
            PlatformUpdate::Focus(intent) => WpeCommand::Focus {
                id,
                generation,
                intent,
            },
        };
        self.sender.send(command)
    }

    pub(super) fn input(
        &self,
        id: WebViewId,
        generation: WebViewGeneration,
        input: WebViewInput,
    ) -> Result<(), String> {
        self.sender.send(WpeCommand::Input {
            id,
            generation,
            input,
        })
    }

    pub(super) fn close(&self, id: WebViewId, generation: WebViewGeneration) -> Result<(), String> {
        self.mailbox.discard_view(id, generation);
        self.sender.send(WpeCommand::Close { id, generation })
    }

    pub(super) fn drain_events(&self) -> Vec<ReactorEvent> {
        self.mailbox.drain_events()
    }

    pub(super) fn take_frame(
        &self,
        id: WebViewId,
        generation: WebViewGeneration,
    ) -> Option<WebViewFrame> {
        self.mailbox.take_frame(id, generation)
    }

    pub(super) fn has_frame(&self, id: WebViewId, generation: WebViewGeneration) -> bool {
        self.mailbox.has_frame(id, generation)
    }
}

impl Drop for WpeReactorHandle {
    fn drop(&mut self) {
        let _ = self.sender.send(WpeCommand::Shutdown);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("WPE reactor thread panicked during shutdown");
        }
    }
}

struct WpeRuntime {
    profile_root: Option<std::path::PathBuf>,
    frame_transport: WpeFrameTransport,
    profiles: HashMap<LinuxProfileKey, NetworkSession>,
    views: HashMap<WebViewId, WpeWebView>,
    frame_leases: HashMap<FrameLeaseId, NativeWpeBufferLease>,
    next_frame_lease: AtomicU64,
    sender: ReactorSender,
    mailbox: Arc<ReactorMailbox>,
    wake: WebViewWake,
    // Must drop after views, profiles, and native leases so the thread-default
    // GLib context remains alive for all WPE/GObject destruction.
    backend: WpeBackend,
}

impl WpeRuntime {
    unsafe fn new(
        config: WebViewSystemConfig,
        sender: ReactorSender,
        mailbox: Arc<ReactorMailbox>,
        wake: WebViewWake,
    ) -> Result<Self, String> {
        let backend = WpeBackend::new(std::ptr::null_mut()).map_err(|error| error.to_string())?;
        let frame_transport = WpeFrameTransport::resolve(config.frame_transport);
        Ok(Self {
            profile_root: config.profile_root,
            frame_transport,
            profiles: HashMap::new(),
            views: HashMap::new(),
            frame_leases: HashMap::new(),
            next_frame_lease: AtomicU64::new(1),
            sender,
            mailbox,
            wake,
            backend,
        })
    }

    fn context(&self) -> *mut plat::GMainContext {
        self.backend.main_context()
    }

    fn session(
        &mut self,
        storage: &crate::StoragePartition,
    ) -> Result<std::ptr::NonNull<super::sys::webkit::WebKitNetworkSession>, String> {
        let key = LinuxProfileKey::from(storage);
        if !self.profiles.contains_key(&key) {
            let session = NetworkSession::create(storage, self.profile_root.as_deref())?;
            self.profiles.insert(key, session);
        }
        Ok(self.profiles.get(&key).expect("profile was inserted").raw())
    }

    fn navigate(view: &mut WpeWebView, target: &NavigationTarget) -> Result<(), String> {
        match target {
            NavigationTarget::Uri(uri) => view.load_uri(uri),
            NavigationTarget::Html { contents, base_uri } => {
                view.load_html(contents, base_uri.as_deref())
            }
            NavigationTarget::File(path) => {
                let uri = file_navigation_uri(path)?;
                view.load_uri(&uri)
            }
        }
        .map_err(|error| error.to_string())
    }

    fn create(&mut self, request: PlatformCreateRequest) -> Result<(), String> {
        let session = self.session(request.storage())?;
        let display = self.backend.platform_display();
        let related_view = match request.relationship() {
            crate::BrowsingRelationship::Independent => None,
            crate::BrowsingRelationship::Related(id) => Some(
                self.views
                    .get(id)
                    .ok_or_else(|| format!("related WebView {id} is not ready"))?
                    .native(),
            ),
        };
        let mut view = WpeWebView::new(WpeViewCreation {
            id: request.id(),
            generation: request.generation(),
            platform_display: display,
            network_session: session,
            related_view,
            size: request.size(),
            policy: request.policy(),
            frame_transport: self.frame_transport,
            wake: WebViewWake::noop(),
        })
        .map_err(|error| error.to_string())?;
        if let Some(navigation) = request.navigation() {
            Self::navigate(&mut view, navigation)?;
        }
        self.views.insert(request.id(), view);
        Ok(())
    }

    fn with_current_view(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        operation: impl FnOnce(&mut WpeWebView) -> Result<(), String>,
    ) -> Result<(), String> {
        let Some(view) = self.views.get_mut(&id) else {
            return Ok(());
        };
        if view.generation() != generation {
            return Ok(());
        }
        operation(view)
    }

    fn handle_command(&mut self, command: WpeCommand) -> bool {
        match command {
            WpeCommand::Create(request) => {
                let id = request.id();
                let generation = request.generation();
                let result = self.create(request);
                self.mailbox.publish_event(ReactorEvent::CreateFinished {
                    id,
                    generation,
                    result,
                });
                self.wake.notify();
            }
            WpeCommand::Resize {
                id,
                generation,
                size,
            } => {
                let _ = self.with_current_view(id, generation, |view| {
                    view.resize(size.width(), size.height());
                    Ok(())
                });
            }
            WpeCommand::Navigate {
                id,
                generation,
                target,
            } => {
                let result =
                    self.with_current_view(id, generation, |view| Self::navigate(view, &target));
                self.publish_command_failure(id, generation, result);
            }
            WpeCommand::History {
                id,
                generation,
                action,
            } => {
                let result = self.with_current_view(id, generation, |view| {
                    match action {
                        HistoryAction::Back => view.go_back(),
                        HistoryAction::Forward => view.go_forward(),
                        HistoryAction::Reload => view.reload(),
                    }
                    .map_err(|error| error.to_string())
                });
                self.publish_command_failure(id, generation, result);
            }
            WpeCommand::EvaluateScript {
                generation,
                request,
            } => {
                let id = request.view;
                let request_id = request.request;
                let result = self.with_current_view(id, generation, |view| {
                    view.execute_javascript(&request)
                        .map_err(|error| error.to_string())
                });
                if let Err(error) = result {
                    self.mailbox
                        .publish_event(ReactorEvent::View(WebViewEvent::ScriptFinished {
                            view: id,
                            generation,
                            request: request_id,
                            result: Err(ScriptError::Rejected(error)),
                        }));
                    self.wake.notify();
                }
            }
            WpeCommand::Focus {
                id,
                generation,
                intent,
            } => {
                let mut changed = false;
                let _ = self.with_current_view(id, generation, |view| {
                    changed = view.set_focus(intent);
                    Ok(())
                });
                if changed {
                    self.mailbox
                        .publish_event(ReactorEvent::View(WebViewEvent::FocusChanged {
                            id,
                            generation,
                            focused: intent == FocusIntent::Focus,
                        }));
                    self.wake.notify();
                }
            }
            WpeCommand::Input {
                id,
                generation,
                input,
            } => {
                let _ = self.with_current_view(id, generation, |view| {
                    view.send_input(input);
                    Ok(())
                });
            }
            WpeCommand::Close { id, generation } => {
                if self
                    .views
                    .get(&id)
                    .is_some_and(|view| view.generation() == generation)
                {
                    self.views.remove(&id);
                    self.mailbox.discard_view(id, generation);
                }
            }
            WpeCommand::ReleaseFrame(id) => {
                self.frame_leases.remove(&id);
            }
            WpeCommand::Shutdown => return false,
        }
        true
    }

    fn publish_command_failure(
        &self,
        id: WebViewId,
        generation: WebViewGeneration,
        result: Result<(), String>,
    ) {
        if let Err(error) = result {
            self.mailbox
                .publish_event(ReactorEvent::View(WebViewEvent::Failed {
                    id,
                    generation,
                    error,
                }));
            self.wake.notify();
        }
    }

    fn service_views(&mut self) {
        let ids: Vec<_> = self.views.keys().copied().collect();
        let mut published = false;
        for id in ids {
            let Some(view) = self.views.get_mut(&id) else {
                continue;
            };
            let generation = view.generation();
            let old_title = view.title.clone();
            let old_uri = view.url.clone();
            let old_progress = view.progress;
            let old_state = view.state;
            view.update();

            if view.title != old_title
                && let Some(title) = view.title.clone()
            {
                self.mailbox
                    .publish_event(ReactorEvent::View(WebViewEvent::TitleChanged {
                        id,
                        generation,
                        title,
                    }));
                published = true;
            }
            if view.url != old_uri {
                self.mailbox
                    .publish_event(ReactorEvent::View(WebViewEvent::UriChanged {
                        id,
                        generation,
                        uri: view.url.clone(),
                    }));
                published = true;
            }
            if (view.progress - old_progress).abs() > f64::EPSILON {
                self.mailbox
                    .publish_event(ReactorEvent::View(WebViewEvent::LoadProgressChanged {
                        id,
                        generation,
                        progress: view.progress,
                    }));
                published = true;
            }
            if old_state == WpeViewState::Loading && view.state == WpeViewState::Ready {
                self.mailbox
                    .publish_event(ReactorEvent::View(WebViewEvent::LoadFinished {
                        id,
                        generation,
                        navigation: None,
                    }));
                published = true;
            }
            for event in view.take_events() {
                self.mailbox.publish_event(ReactorEvent::View(event));
                published = true;
            }

            let frame = view.take_latest_frame();

            if let Some(frame) = frame {
                let frame = match frame {
                    CapturedFrame::DmaBuf(frame) => self.export_dmabuf(frame),
                    CapturedFrame::Pixels(frame) => WebViewFrame::Pixels(PixelFrame::new(
                        frame.pixels,
                        frame.width,
                        frame.height,
                    )),
                };
                self.mailbox.publish_frame(id, generation, frame);
                published = true;
            }
        }
        if published {
            self.wake.notify();
        }
    }

    fn export_dmabuf(&mut self, frame: DmaBufData) -> WebViewFrame {
        let lease = FrameLeaseId(self.next_frame_lease.fetch_add(1, Ordering::Relaxed));
        self.frame_leases.insert(lease, frame.lease);
        let planes = frame
            .planes
            .into_iter()
            .map(|plane| {
                DmaBufPlane::new(std::fs::File::from(plane.fd), plane.stride, plane.offset)
            })
            .collect();
        let rendering_fence = frame.rendering_fence.map(std::fs::File::from);
        WebViewFrame::DmaBuf(DmaBufFrame::new(
            planes,
            rendering_fence,
            frame.fourcc,
            frame.modifier,
            frame.width,
            frame.height,
            DmaBufFrameLease::new(RemoteWpeBufferLease {
                id: lease,
                reactor: self.sender.clone(),
            }),
        ))
    }
}

fn run_reactor(
    config: WebViewSystemConfig,
    receiver: mpsc::Receiver<WpeCommand>,
    sender: ReactorSender,
    signal: Arc<ReactorSignal>,
    mailbox: Arc<ReactorMailbox>,
    wake: WebViewWake,
) {
    // SAFETY: this thread is the sole owner of every WPE object and of the
    // thread-default GLib context for their complete lifetime.
    let mut runtime =
        match unsafe { WpeRuntime::new(config, sender, mailbox.clone(), wake.clone()) } {
            Ok(runtime) => runtime,
            Err(error) => {
                run_failed_reactor(error, receiver, mailbox, wake);
                return;
            }
        };
    let context = runtime.context();
    signal.install_context(context);

    'reactor: loop {
        while let Ok(command) = receiver.try_recv() {
            if !runtime.handle_command(command) {
                break 'reactor;
            }
        }
        runtime.service_views();
        // SAFETY: `context` is owned by this thread. A command calls
        // `g_main_context_wakeup`, so this blocks only until real WebKit/GLib
        // work or a typed reactor command exists. Per-frame retirement never
        // pauses unrelated native sources.
        unsafe { plat::g_main_context_iteration(context, 1) };
    }

    runtime.views.clear();
    runtime.frame_leases.clear();
    signal.clear_context();
}

fn run_failed_reactor(
    error: String,
    receiver: mpsc::Receiver<WpeCommand>,
    mailbox: Arc<ReactorMailbox>,
    wake: WebViewWake,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            WpeCommand::Create(request) => {
                mailbox.publish_event(ReactorEvent::CreateFinished {
                    id: request.id(),
                    generation: request.generation(),
                    result: Err(error.clone()),
                });
                wake.notify();
            }
            WpeCommand::Shutdown => return,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeWpeBufferLease, PendingFrames};
    use crate::{PixelFrame, WebViewFrame};

    #[test]
    fn frame_mailbox_keeps_only_the_latest_negotiated_frame() {
        let mut frames = PendingFrames::default();
        frames.publish(WebViewFrame::Pixels(PixelFrame::new(vec![1; 4], 1, 1)));
        frames.publish(WebViewFrame::Pixels(PixelFrame::new(vec![2; 4], 1, 1)));

        let Some(WebViewFrame::Pixels(frame)) = frames.take() else {
            panic!("latest pixel frame");
        };
        assert_eq!(frame.pixels(), &[2; 4]);
        assert!(frames.take().is_none());
    }

    #[test]
    fn native_wpe_acknowledgement_cannot_cross_threads() {
        static_assertions::assert_not_impl_any!(NativeWpeBufferLease: Send, Sync);
    }
}
