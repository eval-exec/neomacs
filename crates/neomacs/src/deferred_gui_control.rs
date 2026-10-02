//! Exact socket interruption; never dispatches or frees native Wayland objects.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) struct Attempt {
    pub deadline: Instant,
    cancelled: AtomicBool,
    constructing: AtomicBool,
}
impl Attempt {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + Duration::from_secs(15),
            cancelled: AtomicBool::new(false),
            constructing: AtomicBool::new(true),
        })
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
pub(super) struct CancelOnDrop(pub Arc<Attempt>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Default)]
struct State {
    attempt: Option<Arc<Attempt>>,
    #[cfg(target_os = "linux")]
    socket: Option<std::os::unix::net::UnixStream>,
}
#[derive(Default)]
pub(super) struct Control {
    pub stopped: AtomicBool,
    pub foreign_pending: Arc<AtomicBool>,
    pub window_waits: Arc<neomacs_display_runtime::native_window_wait::NativeWindowWaits>,
    #[cfg(feature = "gui-test-hooks")]
    pub quit: Mutex<Option<neovm_core::emacs_core::eval::QuitRequest>>,
    state: Mutex<State>,
    proxy: Mutex<Option<neomacs_display_runtime::render_thread::RenderEventLoopProxy>>,
}
impl Control {
    pub fn install_proxy(
        &self,
        proxy: neomacs_display_runtime::render_thread::RenderEventLoopProxy,
    ) {
        *self.proxy.lock().unwrap() = Some(proxy);
    }
    pub fn wake(&self) {
        if let Some(proxy) = &*self.proxy.lock().unwrap() {
            proxy.wake_up();
        }
    }

    pub fn start(self: &Arc<Self>) -> std::io::Result<std::thread::JoinHandle<()>> {
        let control = self.clone();
        std::thread::Builder::new()
            .name("native-attach-control".into())
            .spawn(move || {
                loop {
                    let stop = control.stopped.load(Ordering::Acquire);
                    #[cfg(feature = "gui-test-hooks")]
                    if neomacs_display_runtime::gui_test_controls::take("frame-quit") {
                        if let Some(quit) = &*control.quit.lock().unwrap() {
                            quit.request();
                        }
                    }
                    #[cfg(feature = "gui-test-hooks")]
                    if std::env::var_os("NEOMACS_GUI_TEST_DIR")
                        .map(std::path::PathBuf::from)
                        .is_some_and(|root| root.join("fault-consumer").exists())
                    {
                        control.wake();
                    }
                    let state = control.state.lock().unwrap();
                    let cancelled = stop
                        || control.window_waits.interrupt_required()
                        || state.attempt.as_ref().is_some_and(|attempt| {
                            attempt.constructing.load(Ordering::Acquire) && attempt.cancelled()
                        });
                    #[cfg(target_os = "linux")]
                    if cancelled && let Some(socket) = &state.socket {
                        let _ = socket.shutdown(std::net::Shutdown::Both);
                    }
                    drop(state);
                    if stop {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
    }
    pub fn cancelled(&self, attempt: &Attempt) -> bool {
        self.stopped.load(Ordering::Acquire) || attempt.cancelled()
    }
    pub fn begin(&self, attempt: Arc<Attempt>) {
        let mut state = self.state.lock().unwrap();
        state.attempt = Some(attempt);
    }
    /// Serialized with cancellation: the attempt deadline loses authority over
    /// the root socket once native construction has successfully completed.
    pub fn constructed(&self, attempt: &Attempt) {
        let _state = self.state.lock().unwrap();
        attempt.constructing.store(false, Ordering::Release);
    }
    #[cfg(target_os = "linux")]
    pub fn connect(
        &self,
        path: &std::path::Path,
        attempt: &Attempt,
    ) -> Result<std::os::unix::net::UnixStream, String> {
        use socket2::{Domain, SockAddr, Socket, Type};
        let socket =
            Socket::new(Domain::UNIX, Type::STREAM, None).map_err(|error| error.to_string())?;
        let addr = SockAddr::unix(path).map_err(|error| error.to_string())?;
        loop {
            if self.cancelled(attempt) {
                return Err("Native display preparation cancelled".into());
            }
            match socket.connect_timeout(&addr, Duration::from_millis(20)) {
                Ok(()) => break,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => {
                    return Err(format!(
                        "Cannot connect to Wayland socket {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        let stream: std::os::unix::net::UnixStream = socket.into();
        let mut state = self.state.lock().unwrap();
        if self.cancelled(attempt) {
            return Err("Native display preparation cancelled".into());
        }
        state.socket = Some(stream.try_clone().map_err(|error| error.to_string())?);
        Ok(stream)
    }
}
