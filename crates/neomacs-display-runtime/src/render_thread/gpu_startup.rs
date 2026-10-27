//! Driver discovery and device acquisition without worker-owned native surfaces.

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::{collections::VecDeque, sync::Arc};
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
/// through GPU acquisition and scene ingestion preserves native event order.
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

    pub(super) fn pop_ready(&mut self, has_scene: bool) -> Option<WindowEvent> {
        let next = self.events.front()?;
        if !has_scene
            && matches!(
                next.0,
                WindowEvent::PointerMoved { .. }
                    | WindowEvent::PointerLeft { .. }
                    | WindowEvent::PointerButton { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            return None;
        }
        self.events.pop_front().map(|input| input.0)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

pub(super) struct PendingGpuAttempt {
    gpu: PendingGpu,
    input: StartupInputReplay,
}

pub(super) enum GpuAttemptPoll {
    Pending(PendingGpuAttempt),
    Ready {
        gpu: PreparedGpu,
        input: StartupInputReplay,
    },
}

impl PendingGpuAttempt {
    pub(super) fn start(
        window: Arc<dyn Window>,
        descriptor: wgpu::InstanceDescriptor,
        proxy: EventLoopProxy,
    ) -> Result<Self, String> {
        let input = StartupInputReplay::new(window.id());
        Ok(Self {
            gpu: PendingGpu::start(window, descriptor, proxy)?,
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

    pub(super) fn poll(self, proxy: &EventLoopProxy) -> Result<GpuAttemptPoll, String> {
        let Self { gpu, input } = self;
        match gpu.poll(proxy)? {
            GpuPoll::Pending(gpu) => Ok(GpuAttemptPoll::Pending(Self { gpu, input })),
            GpuPoll::Ready(gpu) => Ok(GpuAttemptPoll::Ready { gpu, input }),
        }
    }
}

pub(super) enum PendingGpu {
    SurfaceSize(PreparedGpu),
    Adapters {
        window: Arc<dyn Window>,
        reply: Reply<(wgpu::Instance, Vec<wgpu::Adapter>)>,
    },
    Device {
        window: Arc<dyn Window>,
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        adapter: wgpu::Adapter,
        reply: Reply<(wgpu::Device, wgpu::Queue)>,
    },
}

pub(super) enum GpuPoll {
    Pending(PendingGpu),
    Ready(PreparedGpu),
}

impl PendingGpu {
    pub(super) fn start(
        window: Arc<dyn Window>,
        descriptor: wgpu::InstanceDescriptor,
        proxy: EventLoopProxy,
    ) -> Result<Self, String> {
        let reply = spawn("gpu-adapters", proxy, move || {
            let instance = wgpu::Instance::new(descriptor);
            let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
            Ok((instance, adapters))
        })?;
        Ok(Self::Adapters { window, reply })
    }

    pub(super) fn window(&self) -> &dyn Window {
        match self {
            Self::SurfaceSize(prepared) => prepared.window.as_ref(),
            Self::Adapters { window, .. } | Self::Device { window, .. } => window.as_ref(),
        }
    }

    pub(super) fn poll(self, proxy: &EventLoopProxy) -> Result<GpuPoll, String> {
        match self {
            Self::SurfaceSize(prepared) => Ok(Self::when_sized(prepared)),
            Self::Adapters { ref reply, .. } => {
                let result = match reply.try_recv() {
                    Ok(result) => result?,
                    Err(TryRecvError::Empty) => return Ok(GpuPoll::Pending(self)),
                    Err(TryRecvError::Disconnected) => {
                        return Err("GPU adapter worker exited without a result".into());
                    }
                };
                let Self::Adapters { window, .. } = self else {
                    unreachable!()
                };
                let (instance, mut adapters) = result;
                // The pinned desktop HALs do not use enumeration's surface
                // hint. Preserve wgpu-core's stable preference ranking, then
                // apply its same surface-capability filter on the main thread.
                let preference = crate::gpu_power_preference();
                adapters.sort_by_key(|adapter| {
                    adapter_rank(adapter.get_info().device_type, preference)
                });
                let surface = instance
                    .create_surface(window.clone())
                    .map_err(|error| error.to_string())?;
                let adapter = adapters
                    .into_iter()
                    .find(|adapter| adapter.is_surface_supported(&surface))
                    .ok_or_else(|| "no GPU adapter supports the native surface".to_owned())?;
                let worker_adapter = adapter.clone();
                let reply = spawn("gpu-device", proxy.clone(), move || {
                    pollster::block_on(neomacs_renderer_wgpu::request_renderer_device(
                        &worker_adapter,
                        "Neomacs Render Thread Device",
                    ))
                    .map_err(|error| error.to_string())
                })?;
                Ok(GpuPoll::Pending(Self::Device {
                    window,
                    instance,
                    surface,
                    adapter,
                    reply,
                }))
            }
            Self::Device { ref reply, .. } => {
                let (device, queue) = match reply.try_recv() {
                    Ok(result) => result?,
                    Err(TryRecvError::Empty) => return Ok(GpuPoll::Pending(self)),
                    Err(TryRecvError::Disconnected) => {
                        return Err("GPU device worker exited without a result".into());
                    }
                };
                let Self::Device {
                    window,
                    instance,
                    surface,
                    adapter,
                    ..
                } = self
                else {
                    unreachable!()
                };
                Ok(Self::when_sized(PreparedGpu {
                    window,
                    instance,
                    surface,
                    adapter,
                    device,
                    queue,
                }))
            }
        }
    }

    fn when_sized(prepared: PreparedGpu) -> GpuPoll {
        let size = prepared.window.surface_size();
        if size.width == 0 || size.height == 0 {
            GpuPoll::Pending(Self::SurfaceSize(prepared))
        } else {
            GpuPoll::Ready(prepared)
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
mod tests {
    use super::*;
    use winit::{
        event::{Ime, Modifiers, MouseScrollDelta, TouchPhase},
        keyboard::ModifiersState,
    };

    #[test]
    fn startup_input_preserves_focus_modifier_and_ime_order() {
        let mut input = StartupInputReplay::new(WindowId::from_raw(1));
        let events = vec![
            WindowEvent::Focused(true),
            WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
            WindowEvent::Ime(Ime::Enabled),
            WindowEvent::Ime(Ime::Commit("A".into())),
            WindowEvent::ModifiersChanged(Modifiers::default()),
            WindowEvent::Focused(false),
        ];
        for event in &events {
            input.defer_input(event.clone()).unwrap();
        }
        let mut replayed = Vec::new();
        while let Some(event) = input.pop_ready(false) {
            replayed.push(event);
        }
        assert_eq!(replayed, events);
        assert!(input.is_empty());
    }

    #[test]
    fn startup_input_waits_for_pointer_scene_without_overtaking() {
        let mut input = StartupInputReplay::new(WindowId::from_raw(1));
        input.defer_input(WindowEvent::Focused(true)).unwrap();
        let wheel = WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
            phase: TouchPhase::Moved,
        };
        input.defer_input(wheel.clone()).unwrap();
        let commit = WindowEvent::Ime(Ime::Commit("after wheel".into()));
        input.defer_input(commit.clone()).unwrap();

        assert_eq!(input.pop_ready(false), Some(WindowEvent::Focused(true)));
        assert_eq!(input.pop_ready(false), None);
        // Input arriving after GPU completion cannot overtake the wheel.
        let release = WindowEvent::ModifiersChanged(Modifiers::default());
        input.defer_input(release.clone()).unwrap();
        assert_eq!(input.pop_ready(true), Some(wheel));
        assert_eq!(input.pop_ready(true), Some(commit));
        assert_eq!(input.pop_ready(true), Some(release));
        assert!(input.is_empty());
    }

    #[test]
    fn startup_input_keeps_lifecycle_events_synchronous() {
        let mut input = StartupInputReplay::new(WindowId::from_raw(1));
        let commit = WindowEvent::Ime(Ime::Commit("owned input".into()));
        input.defer_input(commit.clone()).unwrap();
        for event in [
            WindowEvent::CloseRequested,
            WindowEvent::Destroyed,
            WindowEvent::SurfaceResized(winit::dpi::PhysicalSize::new(0, 0)),
            WindowEvent::RedrawRequested,
        ] {
            assert_eq!(input.defer_input(event.clone()), Err(event));
        }
        assert_eq!(input.pop_ready(false), Some(commit));
        assert!(input.is_empty());
    }
}
