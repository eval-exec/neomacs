//! Driver discovery and device acquisition without worker-owned native surfaces.

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use winit::{
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
        #[cfg(feature = "gui-test-hooks")]
        if crate::gui_test_controls::take("gpu-start-error") {
            return Err("Injected ordinary GPU worker start error".into());
        }
        #[cfg(feature = "gui-test-hooks")]
        if crate::gui_test_controls::take("gpu-error") {
            let (sender, reply) = crossbeam_channel::bounded(1);
            sender
                .send(Err("Injected ordinary adapter error".into()))
                .unwrap();
            return Ok(Self::Adapters { window, reply });
        }
        let reply = spawn("gpu-adapters", proxy, move || {
            let instance = wgpu::Instance::new(descriptor);
            let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
            Ok((instance, adapters))
        })?;
        Ok(Self::Adapters { window, reply })
    }

    pub(super) fn window_id(&self) -> WindowId {
        self.window().id()
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
