//! Display-free daemon startup with native GUI ownership on the OS main thread.
//! The evaluator (including dynamic-module TLS) is created once on its worker.

use super::*;
use crossbeam_channel::{Receiver, Sender};
use neomacs_display_protocol::GraphicalDisplayIdentity;
use neomacs_display_runtime::render_thread::{InitialWindowLifetime, InitialWindowReply};
use std::cell::RefCell;
use std::sync::atomic::AtomicU64;
#[path = "deferred_gui_control.rs"]
mod control;

pub(super) struct FrameDefaults {
    pub terminal_id: u64,
    pub identity: GraphicalDisplayIdentity,
    pub metrics: BootstrapFrameMetrics,
    pub font: String,
    pub frame_id: Arc<AtomicU64>,
    pub initial: Rc<RefCell<Option<InitialWindow>>>,
    pub ready: HashMap<FrameId, Receiver<Result<(), String>>>,
    pub leases: HashMap<FrameId, Arc<AtomicBool>>,
    pub visual: RefCell<Option<VisualConfig>>,
}

pub(super) enum InitialWindow {
    Pending(InitialWindowReply),
    Ready { _lifetime: InitialWindowLifetime },
}

#[derive(Clone)]
pub(super) struct DisplaySender {
    open: Sender<Request>,
    stop: Sender<()>,
    control: Arc<control::Control>,
}
#[derive(Debug, PartialEq, Eq)]
struct DisplaySelection {
    name: Option<String>,
    runtime: Option<String>,
}

impl DisplaySelection {
    #[cfg(target_os = "linux")]
    fn socket(&self) -> Result<std::path::PathBuf, String> {
        let name = self.name.as_deref().unwrap_or("wayland-0");
        if name.is_empty() || name.starts_with(':') {
            return Err(
                "Deferred explicit X11 displays are not supported; request a Wayland socket".into(),
            );
        }
        let path = std::path::PathBuf::from(name);
        if path.is_absolute() {
            return Ok(path);
        }
        if path.components().count() != 1 {
            return Err("A relative Wayland display must be a socket name".into());
        }
        let runtime = self
            .runtime
            .as_deref()
            .ok_or("Lisp XDG_RUNTIME_DIR is required for a relative Wayland display")?;
        let runtime = std::path::Path::new(runtime);
        if !runtime.is_absolute() {
            return Err("Lisp XDG_RUNTIME_DIR must be absolute".into());
        }
        Ok(runtime.join(path))
    }
}

fn lisp_environment(
    eval: &mut Context,
    name: &str,
) -> Result<Option<String>, neovm_core::emacs_core::error::EvalError> {
    // Fixed, internal names only. Read the evaluator's dynamically bound Lisp
    // environment on its owning worker, never the OS-main native environment.
    let value = eval.eval_str(&format!("(getenv-internal \"{name}\")"))?;
    Ok(value.as_str_owned())
}

struct Request {
    display: DisplaySelection,
    reply: Sender<Result<Opened, String>>,
    attempt: Arc<control::Attempt>,
}

pub(super) struct Opened {
    display: BootstrapDisplayConfig,
    observer: neomacs_display_runtime::font_defaults::FontDefaultsObserver,
    comms: EmacsComms,
    images: SharedImageRenderState,
    reply: InitialWindowReply,
    waker: GuiEventLoopWaker,
    alive: Arc<AtomicBool>,
    #[cfg(feature = "neo-term")]
    terminals: SharedTerminals,
}

struct Attached {
    native: Rc<RefCell<Option<InitialWindow>>>,
    _fonts: neomacs_display_runtime::font_defaults::FontDefaultsObserver,
}

pub(super) struct Lifetime(Rc<RefCell<Option<Attached>>>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.borrow_mut().take();
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.native.borrow_mut().take();
    }
}

struct StopOnExit(DisplaySender);
impl Drop for StopOnExit {
    fn drop(&mut self) {
        self.0.control.stopped.store(true, Ordering::Release);
        self.0.control.wake();
        let _ = self.0.stop.try_send(());
    }
}

pub(super) fn run_daemon(
    mode: RuntimeMode,
    startup: StartupOptions,
    bootstrap: BootstrapDisplayConfig,
    notifier: Option<neovm_core::emacs_core::eval::DaemonNotifier>,
    started: Instant,
    args: Vec<OsString>,
) {
    let (open, rx) = crossbeam_channel::bounded(1);
    let (stop, stopped) = crossbeam_channel::bounded(1);
    let control = Arc::new(control::Control::default());
    let controller = control
        .start()
        .expect("Failed to spawn native cancellation owner");
    let tx = DisplaySender {
        open,
        stop,
        control: control.clone(),
    };
    let restart_args = args.clone();
    let worker = std::thread::Builder::new()
        .name("neomacs-evaluator".into())
        .stack_size(GUI_EVALUATOR_THREAD_STACK_SIZE)
        .spawn(move || {
            let _stop = StopOnExit(tx.clone());
            run_tty_evaluator(mode, startup, bootstrap, notifier, started, args, Some(tx))
        })
        .expect("Failed to spawn daemon evaluator");
    let mut disposition = run_native(rx, stopped, &control);
    tracing::info!("daemon native root retired before evaluator join/re-exec");
    control.stopped.store(true, Ordering::Release);
    controller
        .join()
        .expect("Native cancellation owner panicked");
    // Native owner has released the event loop before any exit or re-exec.
    let exit = match worker.join() {
        Ok(exit) => exit,
        Err(payload) => {
            if disposition.bypass_finalizers {
                exit_cancelled_gui_startup(101);
            }
            std::panic::resume_unwind(payload);
        }
    };
    disposition.bypass_finalizers |= control.foreign_pending.load(Ordering::Acquire);
    if exit.restart {
        daemon::restart(&restart_args, disposition.bypass_finalizers);
    }
    if disposition.bypass_finalizers {
        exit_cancelled_gui_startup(exit.exit_code);
    }
    if exit.exit_code != 0 {
        std::process::exit(exit.exit_code);
    }
}

#[derive(Default)]
struct NativeDisposition {
    bypass_finalizers: bool,
}
impl NativeDisposition {
    #[cfg(test)]
    fn observe(
        &mut self,
        result: &Result<
            neomacs_display_runtime::render_thread::RenderLoopExit,
            neomacs_display_runtime::render_thread::RenderLoopError,
        >,
    ) {
        use neomacs_display_runtime::render_thread::{RenderLoopError, RenderLoopExit};
        self.bypass_finalizers |= matches!(
            result,
            Ok(RenderLoopExit::GpuStartupCancelled) | Err(RenderLoopError::StartupInterrupted(_))
        );
    }
}

fn run_native(
    rx: Receiver<Request>,
    stopped: Receiver<()>,
    control: &Arc<control::Control>,
) -> NativeDisposition {
    use neomacs_display_runtime::render_thread::DaemonRenderRoot;
    let mut root: Option<(DisplaySelection, DaemonRenderRoot)> = None;
    let mut alive: Option<Arc<AtomicBool>> = None;
    let mut connection_closed = false;
    loop {
        if control.stopped.load(Ordering::Acquire) || stopped.try_recv().is_ok() {
            break;
        }
        let request = if let Some((_, native)) = root.as_mut() {
            if !connection_closed && (control.window_waits.terminal() || !native.pump()) {
                connection_closed = true;
                native.retire();
                if let Some(alive) = &alive {
                    alive.store(false, Ordering::Release);
                }
            }
            match rx.try_recv() {
                Ok(request) => request,
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    if connection_closed {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    continue;
                }
                Err(_) => break,
            }
        } else {
            crossbeam_channel::select_biased! {
                recv(stopped) -> _ => break,
                recv(rx) -> request => match request { Ok(request) => request, Err(_) => break },
            }
        };
        let Request {
            display: selection,
            reply,
            attempt,
        } = request;
        if control.cancelled(&attempt) {
            let _ = reply.send(Err("Native display preparation cancelled".into()));
            continue;
        }
        if root
            .as_ref()
            .is_some_and(|(display, _)| display != &selection)
            || connection_closed
        {
            let _ = reply.send(Err(
                "The retained native connection supports only its original live display".into(),
            ));
            continue;
        }
        if root.is_some() {
            control.constructed(&attempt);
        }
        control.begin(attempt.clone());
        #[cfg(target_os = "linux")]
        let observer = neomacs_display_runtime::font_defaults::observe_font_defaults_controlled(
            &|| control.cancelled(&attempt),
            control.foreign_pending.clone(),
        );

        #[cfg(target_os = "linux")]
        let observer = match observer {
            Ok(observer) => observer,
            Err(error) => {
                let _ = reply.send(Err(error.to_string()));
                continue;
            }
        };
        if root.is_none() {
            #[cfg(target_os = "linux")]
            let event_loop = selection.socket().and_then(|socket| {
                let stream = control.connect(&socket, &attempt)?;
                neomacs_display_runtime::render_thread::build_render_event_loop_wayland_stream(
                    stream, &socket,
                )
            });
            #[cfg(not(target_os = "linux"))]
            let event_loop = if selection.name.is_some() {
                Err("Explicit deferred displays are supported only on Linux Wayland".into())
            } else {
                build_render_event_loop()
            };
            match event_loop {
                Ok(event_loop) => {
                    // Root ownership precedes every remaining fallible phase.
                    root = Some((selection, DaemonRenderRoot::new(event_loop)));
                    control.constructed(&attempt);
                    control.install_proxy(root.as_ref().unwrap().1.proxy());
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                    continue;
                }
            }
        }
        let (selection, native) = root.as_mut().unwrap();
        if control.cancelled(&attempt) {
            let _ = reply.send(Err("Native display preparation cancelled".into()));
            continue;
        }
        let resolver =
            neomacs_display_runtime::display_identity::DisplayIdentityResolver::explicit_wayland(
                selection.name.clone().unwrap_or_else(|| "wayland-0".into()),
            );
        let observation = observe_event_loop_display(native.event_loop());
        let system_name = hostname::get()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".into());
        let identity = match resolver.resolve(native.event_loop(), &system_name) {
            Ok(identity) => identity,
            Err(error) => {
                let _ = reply.send(Err(format!("Cannot resolve display: {error:?}")));
                continue;
            }
        };
        #[cfg(not(target_os = "linux"))]
        let observer =
            match neomacs_display_runtime::font_defaults::observe_font_defaults(identity.backend())
            {
                Ok(observer) => observer,
                Err(error) => {
                    let _ = reply.send(Err(error.to_string()));
                    continue;
                }
            };
        let mut display = bootstrap_gui_display_config(
            Interactivity::from_noninteractive(false),
            gui_frame_font_scale_from_observation(observation),
            identity,
        );
        display.font_defaults = observer.initial().clone();
        let (initial_reply, initial_rx) = InitialWindowReply::channel(native.proxy());
        let (emacs, mut render) = ThreadComms::new().split();
        render.keep_alive_without_frames = true;
        render.native_window_waits = Some(control.window_waits.clone());
        let images = Arc::new(neomacs_display_runtime::render_thread::ImageRenderState::default());
        let monitors = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let current_alive = Arc::new(AtomicBool::new(true));
        #[cfg(feature = "neo-term")]
        let terminals = new_shared_terminals();
        native.install(
            render,
            initial_rx,
            images.clone(),
            monitors,
            #[cfg(feature = "neo-term")]
            terminals.clone(),
        );
        alive = Some(current_alive.clone());
        let opened = Opened {
            display,
            observer,
            comms: emacs,
            images,
            reply: initial_reply,
            waker: GuiEventLoopWaker::new(native.proxy()),
            alive: current_alive,
            #[cfg(feature = "neo-term")]
            terminals,
        };
        // A queued success later abandoned drops only this attempt. Pumping
        // observes its geometry/lifetime disconnect, never consumes the root.
        let _ = reply.send(Ok(opened));
    }
    let mut disposition = NativeDisposition::default();
    if let Some((_, native)) = root.as_mut() {
        native.retire();
        disposition.bypass_finalizers |= native.bypass_finalizers();
    }
    if let Some(alive) = alive {
        alive.store(false, Ordering::Release);
    }
    disposition.bypass_finalizers |= control.foreign_pending.load(Ordering::Acquire);
    disposition
}

pub(super) fn install(
    eval: &mut Context,
    sender: DisplaySender,
    input: Sender<neovm_core::keyboard::InputEvent>,
    ttys: secondary_tty::SecondaryTtyRegistry,
    startup: &StartupOptions,
) -> Lifetime {
    let lifetime = Rc::new(RefCell::new(None));
    let keep = lifetime.clone();
    let gui = startup.gui.clone();
    let mut connected: Option<(GraphicalDisplayIdentity, Arc<AtomicBool>)> = None;
    load_neomacs_gui_term_layer(eval);
    eval.set_gui_display_initializer(Box::new(move |eval, display| {
        if let Some((identity, alive)) = &connected {
            if !alive.load(Ordering::Acquire) { return Err(display_error("Graphical display connection has closed")); }
            if display.is_some_and(|name| name != identity.terminal_name()) {
                return Err(display_error("Multiple graphical display connections are not supported by winit"));
            }
            return Ok(());
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let selection = DisplaySelection { name: display.map(str::to_owned).or(lisp_environment(eval, "WAYLAND_DISPLAY")?), runtime: lisp_environment(eval, "XDG_RUNTIME_DIR")? };
        let attempt = control::Attempt::new();
        let _cancel = control::CancelOnDrop(attempt.clone());
        sender.open.try_send(Request { display: selection, reply: tx, attempt: attempt.clone() }).map_err(|error| display_error(format!("Native display owner unavailable: {error}")))?;
        sender.control.wake();
        let deadline = attempt.deadline;
        let mut opened = loop {
            eval.poll_host_wait()?;
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(result) => break result.map_err(display_error)?,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) if Instant::now() < deadline => {},
                Err(error) => return Err(display_error(format!("Native display initialization failed: {error}"))),
            }
        };
        let BootstrapDisplayKind::Gui { identity, .. } = &opened.display.kind else { unreachable!() };
        let identity = identity.clone();
        let (resources, preferred) = gui.prepare(&invocation_name());
        let font = startup_font::StartupFont::select(&opened.display, preferred.as_deref()).ok_or_else(|| display_error("No usable GUI font"))?;
        let metrics = font.metrics();
        let selected = font.into_selected();
        let font_name = format!("-*-{}-{}-{}-*-*-{}-*-*-*-*-*-*-*",
            selected.resolved.family,
            startup_font_weight_symbol(FontWeight::from_css_weight(selected.resolved.weight)),
            selected.slant.symbol_name(),
            selected.metrics.pixel_size);
        let (width, height) = startup_dimensions(FrontendKind::Gui, metrics, false);
        let native = Rc::new(RefCell::new(Some(InitialWindow::Pending(opened.reply))));
        let primary_size = Arc::new(Mutex::new(PrimaryWindowSize { width, height }));
        let frame_id = Arc::new(AtomicU64::new(0));

        let notifier = eval.wait_notifier();
        let quit_requested = eval.quit_requested.clone();
        let display_input = opened.comms.input_rx;
        let bridge_input = input.clone();
        let bridge_frame_id = frame_id.clone();
        let bridge_size = primary_size.clone();
        let mut fonts = opened.observer.take_changes();
        let font_identity = identity.clone();
        let terminal_id = admit_input_bridge(identity.clone(), || std::thread::Builder::new().name("daemon-gui-input".into()).spawn(move || {
            loop {
                let event = crossbeam_channel::select! {
                    recv(display_input) -> event => match event { Ok(event) => event, Err(_) => break },
                    recv(fonts) -> change => {
                        match change {
                            Ok(fonts) => { let _ = bridge_input.send(neovm_core::keyboard::InputEvent::SystemFontsChanged { fonts, display: font_identity.clone() }); if let Some(notifier) = &notifier { let _ = notifier.notify(); } },
                            Err(_) => fonts = crossbeam_channel::never(),
                        }
                        continue;
                    }
                };
                // Connection exit is not the initial-terminal root shutdown.
                if matches!(event, DisplayInputEvent::WindowClose { emacs_frame_id: 0 }) { continue; }
                record_primary_window_resize(&bridge_size, &event);
                for mut event in input_bridge::convert_display_event(&event) {
                    if let neovm_core::keyboard::InputEvent::Resize { emacs_frame_id, .. } = &mut event {
                        if *emacs_frame_id == 0 { *emacs_frame_id = bridge_frame_id.load(Ordering::Acquire); }
                        if *emacs_frame_id == 0 { continue; }
                    }
                    if event.requests_default_quit() { quit_requested.request(); }
                    if bridge_input.send(event).is_err() { return; }
                }
                if let Some(notifier) = &notifier { let _ = notifier.notify(); }
            }
        }))?;
        let redisplay_waker = RedisplayWaker::new(input.clone(), eval.wait_notifier());
        eval.set_display_host(Box::new(PrimaryWindowDisplayHost {
            deferred_frame: Some(FrameDefaults { terminal_id, identity: identity.clone(), metrics, font: font_name, frame_id: frame_id.clone(), initial: native.clone(), ready: HashMap::new(), leases: HashMap::new(), visual: RefCell::new(None) }),
            resources, system_fonts: opened.display.font_defaults.system_fonts(),
            tooltip_client: neomacs_display_protocol::tooltip::TooltipClient::new(opened.comms.tooltip_context.clone()),
            cmd_tx: opened.comms.cmd_tx.clone(), render_waker: Some(opened.waker.clone()),
            font_sizing: opened.display.font_sizing(), primary_window_adopted: false, primary_frame_id: None,
            last_window_titles: Mutex::new(HashMap::new()), font_metrics: None, primary_window_size: primary_size.clone(),
            image_catalog: Rc::new(AsyncImageCatalog::new(opened.comms.cmd_tx.clone(), Some(opened.waker.clone()), opened.images.clone(), Some(redisplay_waker))),
            #[cfg(feature = "video")]
            resolved_videos: Mutex::new(ResolvedVideoRegistry::default()),
            resolved_webkits: Mutex::new(HashMap::new()), resolved_surfaces: Mutex::new(ResolvedSurfaceMemo::default()),
            render_capabilities: opened.comms.capabilities.clone(), requested_frame_shader: Mutex::new(None),
            #[cfg(feature = "neo-term")]
            terminal_state: TerminalHostState::new(opened.terminals),
        }));
        frame_layout::REDISPLAY_RUNTIME.with(|runtime| {
            runtime.enable_cosmic_metrics();
            runtime.set_font_sizing(opened.display.font_sizing());
        });
        let frame_tx = opened.comms.frame_tx;
        let waker = opened.waker.clone();
        let ttys = ttys.clone();
        eval.redisplay_fn = Some(Box::new(move |eval| {
            if !ttys.render_selected(eval) && eval.frame_manager().selected_frame().is_some_and(|frame| frame.effective_window_system().is_some()) {
                publish_gui_frame(eval, &frame_tx, Some(&waker));
            }
        }));
        frame_layout::install_frame_snapshot_fn(eval);
        frame_layout::install_window_layout_query_fn(eval);
        frame_layout::install_font_shape_driver(eval);
        let preview_tx = opened.comms.cmd_tx.clone();
        let preview_waker = opened.waker.clone();
        eval.scroll_preview_fn = Some(Box::new(move |eval, frame, window, inputs| {
            let intent = frame_layout::REDISPLAY_RUNTIME.with(|runtime| runtime.resolved_scroll_preview(eval, frame, window, inputs));
            if let Some(intent) = intent
                && preview_tx.try_send(RenderCommand::Window(WindowCommand::ScrollPreview(intent))).is_ok() { preview_waker.wake(); }
        }));
        connected = Some((identity, opened.alive));
        *keep.borrow_mut() = Some(Attached { native, _fonts: opened.observer });
        Ok(())
    }));
    Lifetime(lifetime)
}

// Publish terminal ownership only after the last fallible preparation phase.
// A failed thread spawn must not leave a live connection without a host.
fn admit_input_bridge(
    identity: GraphicalDisplayIdentity,
    start: impl FnOnce() -> std::io::Result<std::thread::JoinHandle<()>>,
) -> Result<u64, neovm_core::emacs_core::error::EvalError> {
    start().map_err(display_error)?;
    Ok(neovm_core::emacs_core::terminal::pure::register_graphical_terminal(identity))
}

fn display_error(error: impl std::fmt::Display) -> neovm_core::emacs_core::error::EvalError {
    neovm_core::emacs_core::error::EvalError::signal(
        neovm_core::emacs_core::intern::intern("error"),
        vec![Value::string(error.to_string())],
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomacs_display_protocol::GraphicalBackend;

    #[test]
    fn deferred_gui_selection_uses_lisp_runtime_and_absolute_socket() {
        let mut eval = Context::new();
        eval.set_variable(
            "process-environment",
            Value::list(vec![Value::string("XDG_RUNTIME_DIR=/chosen/lisp/runtime")]),
        );
        let runtime = lisp_environment(&mut eval, "XDG_RUNTIME_DIR").unwrap();
        assert_eq!(runtime.as_deref(), Some("/chosen/lisp/runtime"));
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                DisplaySelection {
                    name: Some("selected".into()),
                    runtime
                }
                .socket()
                .unwrap(),
                std::path::PathBuf::from("/chosen/lisp/runtime/selected")
            );
            assert_eq!(
                DisplaySelection {
                    name: Some("/absolute/socket".into()),
                    runtime: None
                }
                .socket()
                .unwrap(),
                std::path::PathBuf::from("/absolute/socket")
            );
            assert!(
                DisplaySelection {
                    name: Some("selected".into()),
                    runtime: None
                }
                .socket()
                .is_err()
            );
        }
    }

    #[test]
    fn deferred_gui_cancelled_disposition_survives_later_success() {
        use neomacs_display_runtime::render_thread::{RenderLoopError, RenderLoopExit};
        let mut disposition = NativeDisposition::default();
        disposition.observe(&Ok(RenderLoopExit::Finished));
        assert!(!disposition.bypass_finalizers);
        disposition.observe(&Ok(RenderLoopExit::GpuStartupCancelled));
        disposition.observe(&Ok(RenderLoopExit::Finished));
        assert!(disposition.bypass_finalizers);
        let mut error = NativeDisposition::default();
        error.observe(&Err(RenderLoopError::StartupInterrupted(
            "controlled startup failure".into(),
        )));
        assert!(error.bypass_finalizers);
    }

    #[test]
    fn deferred_gui_failed_input_spawn_does_not_publish_terminal() {
        let mut eval = Context::new();
        let before = eval.eval_str("(terminal-list)").unwrap();
        let identity =
            GraphicalDisplayIdentity::named(GraphicalBackend::Wayland, "failed-bridge").unwrap();
        assert!(
            admit_input_bridge(identity, || Err(std::io::Error::other(
                "injected spawn failure"
            )))
            .is_err()
        );
        let after = eval.eval_str("(terminal-list)").unwrap();
        eval.set_variable("terminals-before", before);
        eval.set_variable("terminals-after", after);
        assert!(
            eval.eval_str("(equal terminals-before terminals-after)")
                .unwrap()
                .is_truthy()
        );
    }

    #[test]
    fn deferred_gui_successful_input_spawn_publishes_terminal() {
        let mut eval = Context::new();
        let identity =
            GraphicalDisplayIdentity::named(GraphicalBackend::Wayland, "ready-bridge").unwrap();
        let terminal =
            admit_input_bridge(identity, || std::thread::Builder::new().spawn(|| {})).unwrap();
        assert!(terminal > 0);
        assert!(
            eval.eval_str("(let ((terminals (terminal-list)) found) (while terminals (if (equal (terminal-name (car terminals)) \"ready-bridge\") (setq found (terminal-live-p (car terminals)))) (setq terminals (cdr terminals))) found)")
                .unwrap()
                .is_truthy()
        );
    }
}
