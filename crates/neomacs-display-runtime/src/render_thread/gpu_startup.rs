//! Driver discovery and device acquisition without worker-owned native surfaces.

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use neomacs_display_protocol::frame_time::EventTime;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use winit::{
    event::WindowEvent,
    event_loop::EventLoopProxy,
    window::{Window, WindowId},
};

type Reply<T> = Receiver<Result<T, String>>;

struct WorkerReply<T> {
    sender: Option<Sender<Result<T, String>>>,
    proxy: EventLoopProxy,
}

impl<T> Drop for WorkerReply<T> {
    fn drop(&mut self) {
        // Panic/disconnection must become visible before the native-loop wake.
        drop(self.sender.take());
        self.proxy.wake_up();
    }
}

fn spawn<T: Send + 'static>(
    name: &str,
    proxy: EventLoopProxy,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Reply<T>, String> {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let reply = WorkerReply {
        sender: Some(sender),
        proxy,
    };
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let result = work();
            let _ = reply.sender.as_ref().unwrap().send(result);
            drop(reply);
        })
        .map_err(|error| error.to_string())?;
    Ok(receiver)
}

pub(super) struct PreparedGpu {
    pub(super) window: Arc<dyn Window>,
    pub(super) instance: wgpu::Instance,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
}

/// Only replayable input may outlive the native dispatch callback. In
/// particular, lifecycle and surface-size-writer events stay synchronous.
struct PendingNativeInput(WindowEvent);

impl TryFrom<WindowEvent> for PendingNativeInput {
    type Error = WindowEvent;

    fn try_from(event: WindowEvent) -> Result<Self, Self::Error> {
        match event {
            event @ (WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::Focused(_)
            | WindowEvent::Ime(_)
            | WindowEvent::PointerEntered { .. }
            | WindowEvent::PointerMoved { .. }
            | WindowEvent::PointerLeft { .. }
            | WindowEvent::PointerButton { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Occluded(_)) => Ok(Self(event)),
            other => Err(other),
        }
    }
}

/// Input belongs to one native window creation attempt. Moving this owner
/// through GPU acquisition and native owner installation preserves event order.
pub(super) struct StartupInputReplay {
    window_id: WindowId,
    events: VecDeque<PendingNativeInput>,
}

impl StartupInputReplay {
    fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            events: VecDeque::new(),
        }
    }

    pub(super) fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub(super) fn defer_input(&mut self, event: WindowEvent) -> Result<(), WindowEvent> {
        self.events.push_back(PendingNativeInput::try_from(event)?);
        Ok(())
    }

    pub(super) fn pop_front(&mut self) -> Option<WindowEvent> {
        self.events.pop_front().map(|input| input.0)
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

pub(super) struct PendingGpuAttempt {
    gpu: PendingGpu,
    input: StartupInputReplay,
}

pub(super) struct GpuStartupError {
    pub(super) message: String,
    pub(super) choice: StartupAdapterChoice,
}

fn startup_err(choice: StartupAdapterChoice, message: impl Into<String>) -> GpuStartupError {
    GpuStartupError {
        message: message.into(),
        choice,
    }
}

pub(super) enum GpuAttemptPoll {
    Pending(PendingGpuAttempt),
    Ready {
        gpu: PreparedGpu,
        input: StartupInputReplay,
        choice: StartupAdapterChoice,
    },
}

impl PendingGpuAttempt {
    pub(super) fn start(
        window: Arc<dyn Window>,
        descriptor: wgpu::InstanceDescriptor,
        proxy: EventLoopProxy,
        started: EventTime,
    ) -> Result<Self, String> {
        let input = StartupInputReplay::new(window.id());
        Ok(Self {
            gpu: PendingGpu::start(window, descriptor, proxy, started)?,
            input,
        })
    }

    pub(super) fn window_id(&self) -> WindowId {
        self.input.window_id()
    }

    pub(super) fn window(&self) -> &dyn Window {
        self.gpu.window()
    }

    pub(super) fn retain_input(&mut self, event: WindowEvent) {
        // Other events have synchronous pending-startup policies at the caller.
        let _ = self.input.defer_input(event);
    }

    pub(super) fn poll(
        self,
        proxy: &EventLoopProxy,
        now: EventTime,
    ) -> Result<GpuAttemptPoll, GpuStartupError> {
        let Self { gpu, input } = self;
        match gpu.poll(proxy, now)? {
            GpuPoll::Pending(gpu) => Ok(GpuAttemptPoll::Pending(Self { gpu, input })),
            GpuPoll::Ready { gpu, choice } => Ok(GpuAttemptPoll::Ready { gpu, input, choice }),
        }
    }

    pub(super) fn watch_deadline(&self) -> Option<EventTime> {
        self.gpu.watch_deadline()
    }

    pub(super) fn adapter_choice(&self) -> &StartupAdapterChoice {
        self.gpu.adapter_choice()
    }
}

pub(super) enum PendingGpu {
    SurfaceSize {
        prepared: PreparedGpu,
        choice: StartupAdapterChoice,
    },
    Adapters {
        window: Arc<dyn Window>,
        reply: Reply<(wgpu::Instance, Vec<wgpu::Adapter>)>,
        choice: StartupAdapterChoice,
        /// One 10s budget from [`PendingGpu::start`], shared with the device phase.
        watch_started: EventTime,
        watch_warned: bool,
    },
    Device {
        window: Arc<dyn Window>,
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        adapters: Vec<wgpu::Adapter>,
        adapter: wgpu::Adapter,
        choice: StartupAdapterChoice,
        reply: Reply<(wgpu::Device, wgpu::Queue)>,
        watch_started: EventTime,
        watch_warned: bool,
        /// The device-phase line already named this adapter. Enumeration may
        /// have spent the 10s warning before this becomes true.
        device_named: bool,
    },
}

pub(super) enum GpuPoll {
    Pending(PendingGpu),
    Ready {
        gpu: PreparedGpu,
        choice: StartupAdapterChoice,
    },
}

impl PendingGpu {
    pub(super) fn start(
        window: Arc<dyn Window>,
        descriptor: wgpu::InstanceDescriptor,
        proxy: EventLoopProxy,
        started: EventTime,
    ) -> Result<Self, String> {
        let reply = spawn("gpu-adapters", proxy, move || {
            Ok(open_startup_instance(descriptor))
        })?;
        Ok(Self::Adapters {
            window,
            reply,
            choice: StartupAdapterChoice::Ranked,
            watch_started: started,
            watch_warned: false,
        })
    }

    pub(super) fn adapter_choice(&self) -> &StartupAdapterChoice {
        match self {
            Self::SurfaceSize { choice, .. }
            | Self::Adapters { choice, .. }
            | Self::Device { choice, .. } => choice,
        }
    }

    /// `Some(start + 10s)` until the one startup warning has been sent.
    pub(super) fn watch_deadline(&self) -> Option<EventTime> {
        let (started, warned) = match self {
            Self::SurfaceSize { .. } => return None,
            Self::Adapters {
                watch_started,
                watch_warned,
                ..
            }
            | Self::Device {
                watch_started,
                watch_warned,
                ..
            } => (*watch_started, *watch_warned),
        };
        watch_deadline_for(started, warned)
    }

    pub(super) fn window(&self) -> &dyn Window {
        match self {
            Self::SurfaceSize { prepared, .. } => prepared.window.as_ref(),
            Self::Adapters { window, .. } | Self::Device { window, .. } => window.as_ref(),
        }
    }

    pub(super) fn poll(
        self,
        proxy: &EventLoopProxy,
        now: EventTime,
    ) -> Result<GpuPoll, GpuStartupError> {
        match self {
            Self::SurfaceSize { prepared, choice } => Ok(Self::when_sized(prepared, choice)),
            Self::Adapters {
                reply,
                window,
                choice,
                watch_started,
                watch_warned,
            } => {
                let result = match reply.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty) => {
                        let step = drive_startup_watch(
                            watch_started,
                            watch_warned,
                            now,
                            StartupWorkerPoll::Empty,
                            &StartupWatchPhase::Enumeration,
                        );
                        warn_if_startup_still_running(&step);
                        return Ok(GpuPoll::Pending(Self::Adapters {
                            window,
                            reply,
                            choice,
                            watch_started,
                            watch_warned: step.warned,
                        }));
                    }
                    Err(TryRecvError::Disconnected) => {
                        return Err(startup_err(
                            choice,
                            "GPU adapter worker exited without a result",
                        ));
                    }
                };
                let (instance, adapters) = match result {
                    Ok(ready) => ready,
                    Err(message) => return Err(startup_err(choice, message)),
                };
                let surface = match instance.create_surface(window.clone()) {
                    Ok(surface) => surface,
                    Err(error) => return Err(startup_err(choice, error.to_string())),
                };
                let Some(adapter) =
                    select_adapter(adapters.clone(), &surface, window.as_ref(), &choice)
                else {
                    return Err(startup_err(
                        choice,
                        "no GPU adapter supports the native surface",
                    ));
                };
                let worker_adapter = adapter.clone();
                let reply = match spawn("gpu-device", proxy.clone(), move || {
                    pollster::block_on(neomacs_renderer_wgpu::request_renderer_device(
                        &worker_adapter,
                        "Neomacs Render Thread Device",
                    ))
                    .map_err(|error| error.to_string())
                }) {
                    Ok(reply) => reply,
                    Err(message) => return Err(startup_err(choice, message)),
                };
                Ok(GpuPoll::Pending(Self::Device {
                    window,
                    instance,
                    surface,
                    adapters,
                    adapter,
                    choice,
                    reply,
                    watch_started,
                    watch_warned,
                    device_named: false,
                }))
            }
            Self::Device {
                reply,
                window,
                instance,
                surface,
                adapters,
                adapter,
                choice,
                watch_started,
                watch_warned,
                mut device_named,
            } => {
                let ready = match reply.try_recv() {
                    Ok(Ok(ready)) => ready,
                    Ok(Err(error)) => {
                        return Self::retry_device(
                            proxy,
                            window,
                            instance,
                            surface,
                            adapters,
                            adapter,
                            choice,
                            watch_started,
                            watch_warned,
                            device_named,
                            error,
                        );
                    }
                    Err(TryRecvError::Empty) => {
                        let info = adapter.get_info();
                        if watch_warned {
                            if let Some(line) = startup_device_watch_line(
                                now.saturating_since(watch_started),
                                device_named,
                                &info.name,
                                &info.driver,
                            ) {
                                tracing::warn!("{line}");
                                device_named = true;
                            }
                        }
                        let step = drive_startup_watch(
                            watch_started,
                            watch_warned,
                            now,
                            StartupWorkerPoll::Empty,
                            &StartupWatchPhase::Device {
                                name: info.name,
                                driver: info.driver,
                            },
                        );
                        warn_if_startup_still_running(&step);
                        if step.warning.is_some() {
                            device_named = true;
                        }
                        return Ok(GpuPoll::Pending(Self::Device {
                            window,
                            instance,
                            surface,
                            adapters,
                            adapter,
                            choice,
                            reply,
                            watch_started,
                            watch_warned: step.warned,
                            device_named,
                        }));
                    }
                    Err(TryRecvError::Disconnected) => {
                        return Err(startup_err(
                            choice,
                            "GPU device worker exited without a result",
                        ));
                    }
                };
                let (device, queue) = ready;
                Ok(Self::when_sized(
                    PreparedGpu {
                        window,
                        instance,
                        surface,
                        adapter,
                        device,
                        queue,
                    },
                    choice,
                ))
            }
        }
    }

    /// Record this adapter and request the next one. Stop once every enumerated
    /// adapter has failed, or the choice would select the adapter that just failed.
    fn retry_device(
        proxy: &EventLoopProxy,
        window: Arc<dyn Window>,
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        adapters: Vec<wgpu::Adapter>,
        adapter: wgpu::Adapter,
        mut choice: StartupAdapterChoice,
        watch_started: EventTime,
        watch_warned: bool,
        device_named: bool,
        error: String,
    ) -> Result<GpuPoll, GpuStartupError> {
        let failed = AdapterIdentity::from_info(&adapter.get_info());
        choice.record_failure(failed.clone());
        if choice.exclusion_len() >= adapters.len() {
            tracing::error!(
                %error,
                adapter = %failed.name,
                "device request failed and no alternate adapter remains"
            );
            return Err(startup_err(choice, error));
        }
        let Some(next) = select_adapter(adapters.clone(), &surface, window.as_ref(), &choice)
        else {
            tracing::error!(
                %error,
                adapter = %failed.name,
                "device request failed and no alternate adapter remains"
            );
            return Err(startup_err(choice, error));
        };
        if AdapterIdentity::from_info(&next.get_info()) == failed {
            tracing::error!(
                %error,
                adapter = %failed.name,
                "device request failed and the same adapter was selected again"
            );
            return Err(startup_err(choice, error));
        }
        let worker_adapter = next.clone();
        let reply = match spawn("gpu-device", proxy.clone(), move || {
            pollster::block_on(neomacs_renderer_wgpu::request_renderer_device(
                &worker_adapter,
                "Neomacs Render Thread Device",
            ))
            .map_err(|error| error.to_string())
        }) {
            Ok(reply) => reply,
            Err(message) => return Err(startup_err(choice, message)),
        };
        Ok(GpuPoll::Pending(Self::Device {
            window,
            instance,
            surface,
            adapters,
            adapter: next,
            choice,
            reply,
            watch_started,
            watch_warned,
            device_named,
        }))
    }

    fn when_sized(prepared: PreparedGpu, choice: StartupAdapterChoice) -> GpuPoll {
        let size = prepared.window.surface_size();
        if size.width == 0 || size.height == 0 {
            GpuPoll::Pending(Self::SurfaceSize { prepared, choice })
        } else {
            GpuPoll::Ready {
                gpu: prepared,
                choice,
            }
        }
    }
}

/// The adapter `request_adapter` would pick for `surface`, except that an
/// adapter known to present nothing to this window gives way to the next one.
/// The pinned desktop HALs do not use enumeration's surface hint, so this
/// preserves wgpu-core's stable preference ranking, then applies its same
/// surface-capability filter.
pub(super) fn select_adapter(
    adapters: Vec<wgpu::Adapter>,
    surface: &wgpu::Surface<'_>,
    window: &dyn Window,
    choice: &StartupAdapterChoice,
) -> Option<wgpu::Adapter> {
    let preference = crate::gpu_power_preference();
    let mut candidates: Vec<StartupAdapterCandidate> = adapters
        .iter()
        .map(|adapter| {
            let info = adapter.get_info();
            StartupAdapterCandidate {
                identity: AdapterIdentity::from_info(&info),
                device_type: info.device_type,
                surface_compatible: adapter.is_surface_supported(surface),
            }
        })
        .collect();
    let decision = integrated_startup_scope(&candidates, startup_gpu_scope());
    if decision.restored_discrete {
        tracing::warn!(
            skipped = ?decision.skipped_names,
            "NEOMACS_GPU=integrated left no surface-capable adapter; restoring discrete adapters"
        );
    } else {
        for name in &decision.skipped_names {
            tracing::info!(
                adapter = %name,
                "NEOMACS_GPU=integrated skipping discrete adapter"
            );
        }
    }
    let mut allowed = vec![false; candidates.len()];
    for index in decision.indexes {
        allowed[index] = true;
    }
    for (index, candidate) in candidates.iter_mut().enumerate() {
        if !allowed[index] {
            candidate.surface_compatible = false;
        }
    }
    let os = host_startup_os();
    let backends = startup_backend_policy();
    let Some(pick) = choose_startup_adapter_on(&candidates, preference, choice, os, backends)
    else {
        if let StartupAdapterChoice::SkipFailed { excluded } = choice {
            tracing::error!(
                adapters = %excluded.iter().map(|id| id.name.as_str()).collect::<Vec<_>>().join(", "),
                "excluded adapters are the only surface-compatible choices; not selecting them again"
            );
        }
        return None;
    };
    let mut chosen = pick.index;
    if is_x11(window) && lavapipe_drops_x11_frames(&adapters[chosen].get_info()) {
        if let Some(other) = (0..adapters.len()).find(|&index| {
            index != chosen
                && candidates[index].surface_compatible
                && backend_allowed(backends, candidates[index].identity.backend)
                && !choice.excludes(&candidates[index].identity)
                && !lavapipe_drops_x11_frames(&adapters[index].get_info())
        }) {
            let from = adapters[chosen].get_info();
            let to = adapters[other].get_info();
            tracing::warn!(
                "skipping {} ({}): it presents nothing to X11 windows before Mesa 24.0.2; using {} ({:?})",
                from.name,
                from.driver_info,
                to.name,
                to.backend
            );
            chosen = other;
        } else {
            let info = adapters[chosen].get_info();
            tracing::warn!(
                "{} ({}) presents nothing to X11 windows before Mesa 24.0.2, and no other adapter supports the window",
                info.name,
                info.driver_info
            );
        }
    }
    if chosen == pick.index
        && let Some(warning) = pick.warning
    {
        tracing::warn!("{warning}");
    }
    if let StartupAdapterChoice::SkipFailed { excluded } = choice {
        let using = adapters[chosen].get_info();
        tracing::warn!(
            skipped = %excluded.iter().map(|id| id.name.as_str()).collect::<Vec<_>>().join(", "),
            using = %using.name,
            "skipping adapters that failed before the first present"
        );
    }
    let chosen_info = adapters[chosen].get_info();
    let why = match (os, backends) {
        (_, StartupBackendPolicy::Explicit(backend)) => {
            format!("WGPU_BACKEND={}", backend.to_str())
        }
        (StartupOs::Windows, StartupBackendPolicy::Default) => {
            "Windows default instance is DX12; GL then Vulkan are later instances".to_string()
        }
        (StartupOs::Windows, StartupBackendPolicy::Limited(_)) => {
            "Windows rank inside WGPU_BACKEND is DX12, then Vulkan, then GL".to_string()
        }
        _ => "device-type rank".to_string(),
    };
    tracing::info!(
        backend = chosen_info.backend.to_str(),
        adapter = %chosen_info.name,
        "{why}"
    );
    Some(adapters[chosen].clone())
}

fn is_x11(window: &dyn Window) -> bool {
    window.display_handle().is_ok_and(|handle| {
        matches!(
            handle.as_raw(),
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
        )
    })
}

/// Mesa's lavapipe before 24.0.2 sends frames to an X11 server without DRI3
/// through a core `PutImage` hardcoded to depth 24 (`x11_present_to_x11_sw` in
/// `src/vulkan/wsi/wsi_common_x11.c`). Neomacs windows use a depth-32 visual
/// with alpha, as GNU's `select_visual` picks, so the server rejects every
/// frame with `BadMatch` and the window shows nothing.
// ponytail: also skips lavapipe on X servers with DRI3, where Mesa's MIT-SHM
// path works; query the DRI3 extension if software Vulkan there matters.
fn lavapipe_drops_x11_frames(info: &wgpu::AdapterInfo) -> bool {
    info.backend == wgpu::Backend::Vulkan
        && info.driver == "llvmpipe"
        && mesa_version(&info.driver_info).is_some_and(|version| version < (24, 0, 2))
}

/// `"Mesa 23.2.1-1ubuntu3.1~22.04.4 (LLVM 15.0.7)"` is `(23, 2, 1)`.
fn mesa_version(driver_info: &str) -> Option<(u32, u32, u32)> {
    let mut numbers = driver_info
        .strip_prefix("Mesa ")?
        .split(|c: char| !c.is_ascii_digit());
    let mut next = || numbers.next()?.parse().ok();
    Some((next()?, next()?, next()?))
}

/// Identity of one enumerated adapter. A pre-present failure records this
/// and the next startup skips that adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AdapterIdentity {
    pub backend: wgpu::Backend,
    pub vendor: u32,
    pub device: u32,
    pub name: String,
}

impl AdapterIdentity {
    pub(super) fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            backend: info.backend,
            vendor: info.vendor,
            device: info.device,
            name: info.name.clone(),
        }
    }
}

/// How startup picks among the enumerated adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StartupAdapterChoice {
    /// Highest `adapter_rank`, then the surface filter.
    Ranked,
    /// Adapters that failed before the first present. The set should grow.
    /// Replacing it retries an adapter that already failed.
    SkipFailed { excluded: Vec<AdapterIdentity> },
}

impl StartupAdapterChoice {
    /// Append one failure. Replacing the list would retry an adapter that already failed.
    pub(super) fn record_failure(&mut self, failed: AdapterIdentity) {
        match self {
            Self::Ranked => {
                *self = Self::SkipFailed {
                    excluded: vec![failed],
                };
            }
            Self::SkipFailed { excluded } => {
                if !excluded.contains(&failed) {
                    excluded.push(failed);
                }
            }
        }
    }

    pub(super) fn exclusion_len(&self) -> usize {
        match self {
            Self::Ranked => 0,
            Self::SkipFailed { excluded } => excluded.len(),
        }
    }

    pub(super) fn excludes(&self, identity: &AdapterIdentity) -> bool {
        match self {
            Self::Ranked => false,
            Self::SkipFailed { excluded } => excluded.iter().any(|id| id == identity),
        }
    }
}

/// One enumerated adapter reduced to the fields the pure choice uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupAdapterCandidate {
    pub identity: AdapterIdentity,
    pub device_type: wgpu::DeviceType,
    pub surface_compatible: bool,
}

/// The adapter startup will request a device from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupAdapterPick {
    pub index: usize,
    /// `DeviceType::Cpu` chosen only after every hardware adapter is gone.
    pub software: bool,
    /// Set when the pick is the software last resort.
    pub warning: Option<String>,
}

/// Host used to rank backends. Tests pass `Windows` on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupOs {
    Windows,
    Other,
}

/// `WGPU_BACKEND` when set, otherwise the platform default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupBackendPolicy {
    Default,
    /// One backend, for example `WGPU_BACKEND=vulkan`.
    Explicit(wgpu::Backend),
    /// Several backends named together. Windows still ranks inside this set.
    Limited(wgpu::Backends),
}

/// Rank surface-compatible adapters and apply a pre-present failure skip.
///
/// A `DeviceType::Cpu` adapter is chosen only when every hardware adapter is
/// gone, and that pick carries a warning. Hardware that is still eligible
/// wins even when power preference gives every device the same rank.
pub(super) fn choose_startup_adapter(
    candidates: &[StartupAdapterCandidate],
    preference: wgpu::PowerPreference,
    choice: &StartupAdapterChoice,
) -> Option<StartupAdapterPick> {
    choose_startup_adapter_on(
        candidates,
        preference,
        choice,
        StartupOs::Other,
        StartupBackendPolicy::Default,
    )
}

/// Same choice as [`choose_startup_adapter`], plus a platform backend policy.
///
/// On Windows, with no single `WGPU_BACKEND`, DX12 ranks ahead of Vulkan and
/// GL after the device-type rank. `DeviceType::Cpu` stays last. An explicit
/// backend drops every other backend before that rank.
pub(super) fn choose_startup_adapter_on(
    candidates: &[StartupAdapterCandidate],
    preference: wgpu::PowerPreference,
    choice: &StartupAdapterChoice,
    os: StartupOs,
    backends: StartupBackendPolicy,
) -> Option<StartupAdapterPick> {
    let eligible: Vec<usize> = (0..candidates.len())
        .filter(|&index| candidates[index].surface_compatible)
        .filter(|&index| backend_allowed(backends, candidates[index].identity.backend))
        .filter(|&index| !choice.excludes(&candidates[index].identity))
        .collect();
    let hardware: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|&index| candidates[index].device_type != wgpu::DeviceType::Cpu)
        .collect();
    let pool = if hardware.is_empty() {
        &eligible
    } else {
        &hardware
    };
    let mut ranked = pool.clone();
    ranked.sort_by_key(|&index| {
        (
            adapter_rank(candidates[index].device_type, preference),
            windows_backend_rank(os, backends, candidates[index].identity.backend),
        )
    });
    let index = *ranked.first()?;
    let software = candidates[index].device_type == wgpu::DeviceType::Cpu;
    let warning = software.then(|| {
        format!(
            "no hardware adapter left; using software adapter {}",
            candidates[index].identity.name
        )
    });
    Some(StartupAdapterPick {
        index,
        software,
        warning,
    })
}

fn backend_allowed(policy: StartupBackendPolicy, backend: wgpu::Backend) -> bool {
    match policy {
        StartupBackendPolicy::Default => true,
        StartupBackendPolicy::Explicit(required) => backend == required,
        StartupBackendPolicy::Limited(allowed) => allowed.contains(wgpu::Backends::from(backend)),
    }
}

/// DX12, then Vulkan, then GL. Other policies do not reorder backends.
fn windows_backend_rank(os: StartupOs, policy: StartupBackendPolicy, backend: wgpu::Backend) -> u8 {
    let rank_backends = matches!(
        (os, policy),
        (
            StartupOs::Windows,
            StartupBackendPolicy::Default | StartupBackendPolicy::Limited(_)
        )
    );
    if !rank_backends {
        return 0;
    }
    match backend {
        wgpu::Backend::Dx12 => 0,
        wgpu::Backend::Vulkan => 1,
        wgpu::Backend::Gl => 2,
        _ => 3,
    }
}

/// `WGPU_BACKEND` when set. A single name is [`StartupBackendPolicy::Explicit`].
pub(super) fn startup_backend_policy() -> StartupBackendPolicy {
    let Ok(raw) = std::env::var("WGPU_BACKEND") else {
        return StartupBackendPolicy::Default;
    };
    if raw.trim().is_empty() {
        return StartupBackendPolicy::Default;
    }
    let set = wgpu::Backends::from_comma_list(&raw);
    if set.is_empty() {
        tracing::warn!("WGPU_BACKEND={raw}: no known backend, using the platform default");
        return StartupBackendPolicy::Default;
    }
    let singles = [
        (wgpu::Backends::DX12, wgpu::Backend::Dx12),
        (wgpu::Backends::VULKAN, wgpu::Backend::Vulkan),
        (wgpu::Backends::GL, wgpu::Backend::Gl),
        (wgpu::Backends::METAL, wgpu::Backend::Metal),
        (wgpu::Backends::BROWSER_WEBGPU, wgpu::Backend::BrowserWebGpu),
        (wgpu::Backends::NOOP, wgpu::Backend::Noop),
    ];
    for (bits, backend) in singles {
        if set == bits {
            return StartupBackendPolicy::Explicit(backend);
        }
    }
    StartupBackendPolicy::Limited(set)
}

fn host_startup_os() -> StartupOs {
    if cfg!(target_os = "windows") {
        StartupOs::Windows
    } else {
        StartupOs::Other
    }
}

/// One `Instance` to open, in order. A fallback runs only after the previous
/// instance enumerated no hardware adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InstanceBackendAttempt {
    pub backends: wgpu::Backends,
    pub fallback: bool,
}

/// GLES present needs the display handle. DX12 and Vulkan ignore it, so the
/// owned handle stays available for a later GL instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupDisplaySlot {
    Attach,
    Omit,
}

/// Windows default opens DX12 only. GL, then Vulkan, are later instances.
/// `WGPU_BACKEND` is that set alone. Other hosts keep `descriptor_backends`,
/// which already includes `Backends::with_env`.
pub(super) fn instance_backend_plan(
    os: StartupOs,
    policy: StartupBackendPolicy,
    descriptor_backends: wgpu::Backends,
) -> Vec<InstanceBackendAttempt> {
    match (os, policy) {
        (StartupOs::Windows, StartupBackendPolicy::Default) => vec![
            InstanceBackendAttempt {
                backends: wgpu::Backends::DX12,
                fallback: false,
            },
            InstanceBackendAttempt {
                backends: wgpu::Backends::GL,
                fallback: true,
            },
            InstanceBackendAttempt {
                backends: wgpu::Backends::VULKAN,
                fallback: true,
            },
        ],
        (StartupOs::Windows, StartupBackendPolicy::Explicit(backend)) => {
            vec![InstanceBackendAttempt {
                backends: wgpu::Backends::from(backend),
                fallback: false,
            }]
        }
        (StartupOs::Windows, StartupBackendPolicy::Limited(backends)) => {
            vec![InstanceBackendAttempt {
                backends,
                fallback: false,
            }]
        }
        (StartupOs::Other, _) => vec![InstanceBackendAttempt {
            backends: descriptor_backends,
            fallback: false,
        }],
    }
}

pub(super) fn startup_display_slot(backends: wgpu::Backends) -> StartupDisplaySlot {
    if backends.contains(wgpu::Backends::GL) {
        StartupDisplaySlot::Attach
    } else {
        StartupDisplaySlot::Omit
    }
}

/// First attempt that enumerated something other than `DeviceType::Cpu`.
/// CPU-only (Microsoft Basic Render) is not usable, so a later attempt can
/// still win. When every attempt is empty or CPU-only, the first CPU-only
/// attempt remains so the software last resort still runs.
pub(super) fn choose_startup_instance(attempts: &[Vec<wgpu::DeviceType>]) -> Option<usize> {
    if let Some(index) = attempts
        .iter()
        .position(|types| types.iter().any(|kind| *kind != wgpu::DeviceType::Cpu))
    {
        return Some(index);
    }
    attempts
        .iter()
        .position(|types| types.iter().any(|kind| *kind == wgpu::DeviceType::Cpu))
}

/// Open the startup instance plan. Enumerate only each attempt's backends.
/// A single attempt keeps the display handle. A multi-attempt plan attaches
/// it to the GL instance, because DX12 and Vulkan do not use it.
pub(super) fn open_startup_instance(
    descriptor: wgpu::InstanceDescriptor,
) -> (wgpu::Instance, Vec<wgpu::Adapter>) {
    let plan = instance_backend_plan(
        host_startup_os(),
        startup_backend_policy(),
        descriptor.backends,
    );
    tracing::info!(
        attempts = ?plan.iter().map(|attempt| attempt.backends).collect::<Vec<_>>(),
        "startup instance backend plan"
    );
    let plan_len = plan.len();
    let mut display = descriptor.display;
    let mut opened = Vec::with_capacity(plan_len);
    let mut device_types = Vec::with_capacity(plan_len);
    for attempt in plan {
        if attempt.fallback {
            tracing::warn!(
                backends = ?attempt.backends,
                "no hardware adapter on the previous startup instance; opening a fallback instance"
            );
        }
        let attach_display = plan_len == 1
            || matches!(
                startup_display_slot(attempt.backends),
                StartupDisplaySlot::Attach
            );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: attempt.backends,
            flags: descriptor.flags,
            memory_budget_thresholds: descriptor.memory_budget_thresholds,
            backend_options: descriptor.backend_options.clone(),
            display: if attach_display { display.take() } else { None },
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(attempt.backends));
        let types = adapters
            .iter()
            .map(|adapter| adapter.get_info().device_type)
            .collect::<Vec<_>>();
        let hardware = types.iter().any(|kind| *kind != wgpu::DeviceType::Cpu);
        if hardware {
            tracing::info!(
                backends = ?attempt.backends,
                adapters = adapters.len(),
                "startup instance has a hardware adapter"
            );
        } else if types.iter().any(|kind| *kind == wgpu::DeviceType::Cpu) {
            tracing::info!(
                backends = ?attempt.backends,
                "startup instance enumerated only a software adapter"
            );
        }
        device_types.push(types);
        opened.push((instance, adapters));
        if hardware {
            break;
        }
    }
    let Some(index) = choose_startup_instance(&device_types) else {
        tracing::error!("no adapter enumerated on any startup instance");
        return opened
            .pop()
            .expect("instance backend plan opens one instance");
    };
    if device_types[index]
        .iter()
        .all(|kind| *kind == wgpu::DeviceType::Cpu)
    {
        tracing::warn!("no hardware adapter on any startup instance; keeping the software adapter");
    }
    opened.swap_remove(index)
}

/// Where the 10s startup budget is waiting. Enumeration has no adapter yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StartupWatchPhase {
    Enumeration,
    Device { name: String, driver: String },
}

/// One warning at 10s. Enumeration has no adapter name yet. The device phase
/// names the adapter and driver, including NVK when that is the driver.
pub(super) fn startup_watch_warning(
    elapsed: Duration,
    already_warned: bool,
    phase: &StartupWatchPhase,
) -> Option<String> {
    if already_warned || elapsed < Duration::from_secs(10) {
        return None;
    }
    Some(match phase {
        StartupWatchPhase::Enumeration => format!(
            "GPU enumeration still running after {}s; try WGPU_BACKEND=gl or VK_DRIVER_FILES. NEOMACS_GPU=integrated applies after enumeration",
            elapsed.as_secs()
        ),
        StartupWatchPhase::Device { name, driver } => format!(
            "GPU startup still running after {}s on adapter {name} ({driver}); try WGPU_BACKEND=gl or NEOMACS_GPU=integrated",
            elapsed.as_secs()
        ),
    })
}

/// Device-phase line after enumeration already spent the 10s warning.
/// `device_named` keeps the line from printing on every later poll.
pub(super) fn startup_device_watch_line(
    elapsed: Duration,
    device_named: bool,
    name: &str,
    driver: &str,
) -> Option<String> {
    if device_named || elapsed < Duration::from_secs(10) {
        return None;
    }
    Some(format!(
        "GPU startup still running after {}s on adapter {name} ({driver}); try WGPU_BACKEND=gl or NEOMACS_GPU=integrated",
        elapsed.as_secs()
    ))
}

fn warn_if_startup_still_running(step: &StartupWatchStep) {
    if let Some(warning) = &step.warning {
        tracing::warn!("{warning}");
    }
}

/// Whether the worker has produced a result on this poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupWorkerPoll {
    /// `try_recv` returned empty. The request is still running.
    Empty,
    /// The worker returned. Do not describe that work as still running.
    Finished,
}

/// One step of the startup watch: the warning, the latch, and the next wake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupWatchStep {
    pub warned: bool,
    pub warning: Option<String>,
    /// `WaitUntil` this moment while the warning has not been sent.
    pub deadline: Option<EventTime>,
}

/// Warn only while the worker is still empty. A finished worker is not
/// "still running". Arm `start + 10s` until that warning has been sent.
pub(super) fn drive_startup_watch(
    started: EventTime,
    already_warned: bool,
    now: EventTime,
    worker: StartupWorkerPoll,
    phase: &StartupWatchPhase,
) -> StartupWatchStep {
    if worker == StartupWorkerPoll::Finished {
        return StartupWatchStep {
            warned: already_warned,
            warning: None,
            deadline: None,
        };
    }
    let elapsed = now.saturating_since(started);
    let warning = startup_watch_warning(elapsed, already_warned, phase);
    let warned = already_warned || warning.is_some();
    StartupWatchStep {
        warned,
        warning,
        deadline: watch_deadline_for(started, warned),
    }
}

/// Moment `watch_deadline` waits until. One formula for the event loop and the row.
pub(super) fn watch_deadline_for(started: EventTime, warned: bool) -> Option<EventTime> {
    if warned {
        None
    } else {
        Some(started.plus(Duration::from_secs(10)))
    }
}

/// `NEOMACS_GPU=integrated` drops discrete adapters before ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupGpuScope {
    Any,
    Integrated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IntegratedScopeDecision {
    pub indexes: Vec<usize>,
    pub skipped_names: Vec<String>,
    pub restored_discrete: bool,
}

/// `NEOMACS_GPU=integrated` or `low`. Other values keep every adapter.
pub(super) fn startup_gpu_scope() -> StartupGpuScope {
    match std::env::var("NEOMACS_GPU").as_deref() {
        Ok("low") | Ok("integrated") => StartupGpuScope::Integrated,
        _ => StartupGpuScope::Any,
    }
}

/// Drop `DeviceType::DiscreteGpu` before ranking. If nothing left supports
/// the surface, restore the full list and say so.
pub(super) fn integrated_startup_scope(
    candidates: &[StartupAdapterCandidate],
    scope: StartupGpuScope,
) -> IntegratedScopeDecision {
    let all: Vec<usize> = (0..candidates.len()).collect();
    if scope != StartupGpuScope::Integrated {
        return IntegratedScopeDecision {
            indexes: all,
            skipped_names: Vec::new(),
            restored_discrete: false,
        };
    }
    let skipped_names: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.device_type == wgpu::DeviceType::DiscreteGpu)
        .map(|candidate| candidate.identity.name.clone())
        .collect();
    let kept: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.device_type != wgpu::DeviceType::DiscreteGpu)
        .map(|(index, _)| index)
        .collect();
    let kept_supports_surface = kept
        .iter()
        .any(|&index| candidates[index].surface_compatible);
    if kept_supports_surface || skipped_names.is_empty() {
        IntegratedScopeDecision {
            indexes: kept,
            skipped_names,
            restored_discrete: false,
        }
    } else {
        IntegratedScopeDecision {
            indexes: all,
            skipped_names,
            restored_discrete: true,
        }
    }
}

fn adapter_rank(kind: wgpu::DeviceType, preference: wgpu::PowerPreference) -> u8 {
    use wgpu::{DeviceType, PowerPreference};
    match (preference, kind) {
        (PowerPreference::None, _) => 0,
        (PowerPreference::LowPower, DeviceType::IntegratedGpu)
        | (PowerPreference::HighPerformance, DeviceType::DiscreteGpu) => 0,
        (_, DeviceType::IntegratedGpu | DeviceType::DiscreteGpu) => 1,
        (_, DeviceType::Other) => 2,
        (_, DeviceType::VirtualGpu) => 3,
        (_, DeviceType::Cpu) => 4,
    }
}

#[cfg(test)]
#[path = "tests/gpu_startup_test.rs"]
mod gpu_startup_test;
