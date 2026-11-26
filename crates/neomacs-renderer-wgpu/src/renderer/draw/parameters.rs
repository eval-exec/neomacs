use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

/// The device was lost or destroyed while a frame was encoding a buffer.
///
/// `create_buffer_init` turns that into a panic (`get_mapped_range_mut`'s
/// `expect`). These snapshots omit `COPY_DST` so a later painter cannot
/// overwrite them, which also rules out `queue.write_buffer`. Mapping the
/// `MapRangeError` keeps that usage and lets the frame stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceLost;

pub(in crate::renderer) fn create_initialized_buffer(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> Result<wgpu::Buffer, DeviceLost> {
    if contents.is_empty() {
        return Ok(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 0,
            usage,
            mapped_at_creation: false,
        }));
    }
    let unpadded = contents.len() as wgpu::BufferAddress;
    let align = wgpu::COPY_BUFFER_ALIGNMENT;
    let padded = ((unpadded + align - 1) & !(align - 1)).max(align);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: padded,
        usage,
        mapped_at_creation: true,
    });
    let mut mapped = match buffer.get_mapped_range_mut(..) {
        Ok(mapped) => mapped,
        Err(error) => {
            tracing::error!(%error, label, "device lost while mapping an initialized buffer");
            return Err(DeviceLost);
        }
    };
    mapped.slice(..contents.len()).copy_from_slice(contents);
    drop(mapped);
    buffer.unmap();
    Ok(buffer)
}

use crate::vertex::Uniforms;

/// A snapshot of one draw's projection and animation sample.
/// The bind group retains its immutable buffer; queued commands retain the
/// binding even after this snapshot or its cache entry is dropped.
#[derive(Clone)]
pub(in crate::renderer) struct DrawParameters {
    binding: wgpu::BindGroup,
}

impl DrawParameters {
    pub(in crate::renderer) fn binding(&self) -> &wgpu::BindGroup {
        &self.binding
    }
}

/// Reuse identical immutable snapshots without an implicit "current" target.
/// Eviction never rewrites GPU storage referenced by another draw.
pub(in crate::renderer) struct DrawParameterCache {
    layout: wgpu::BindGroupLayout,
    entries: RefCell<VecDeque<([u32; 7], DrawParameters)>>,
    lost: Cell<bool>,
}

impl DrawParameterCache {
    pub(in crate::renderer) fn new(layout: wgpu::BindGroupLayout) -> Self {
        Self {
            layout,
            entries: RefCell::new(VecDeque::new()),
            lost: Cell::new(false),
        }
    }

    pub(in crate::renderer) fn mark_lost(&self) {
        self.lost.set(true);
    }

    pub(in crate::renderer) fn take_lost(&self) -> bool {
        self.lost.replace(false)
    }

    pub(in crate::renderer) fn get(
        &self,
        device: &wgpu::Device,
        size: [f32; 2],
        time: f32,
        content_alpha: f32,
        content_scale: f32,
        content_pivot: [f32; 2],
    ) -> Result<DrawParameters, DeviceLost> {
        let key = [
            size[0].to_bits(),
            size[1].to_bits(),
            time.to_bits(),
            content_alpha.to_bits(),
            content_scale.to_bits(),
            content_pivot[0].to_bits(),
            content_pivot[1].to_bits(),
        ];
        let mut entries = self.entries.borrow_mut();
        if let Some(index) = entries.iter().position(|(k, _)| *k == key) {
            let entry = entries.remove(index).unwrap();
            let result = entry.1.clone();
            entries.push_back(entry);
            return Ok(result);
        }
        let buffer = match create_initialized_buffer(
            device,
            "Immutable draw parameters",
            bytemuck::bytes_of(&Uniforms {
                screen_size: size,
                time,
                content_alpha,
                content_scale,
                _pivot_padding: 0.0,
                content_pivot,
            }),
            // Deliberately no COPY_DST: painters cannot overwrite this snapshot.
            wgpu::BufferUsages::UNIFORM,
        ) {
            Ok(buffer) => buffer,
            Err(lost) => {
                drop(entries);
                self.lost.set(true);
                return Err(lost);
            }
        };
        let result = DrawParameters {
            binding: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Draw parameter binding"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            }),
        };
        if entries.len() == 64 {
            entries.pop_front();
        }
        entries.push_back((key, result.clone()));
        Ok(result)
    }

    /// Encode one frame's immutable draw parameters.
    ///
    /// A lost or destroyed device abandons the frame and requests recovery.
    /// `create_buffer_init` panics on that device (`expect` on the mapped
    /// range), so this is the seam the frame path must use instead.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::renderer) fn encode_frame(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        size: [f32; 2],
        time: f32,
        content_alpha: f32,
        content_scale: f32,
        content_pivot: [f32; 2],
        recovery_attempted: &mut bool,
    ) -> FrameParameterEncode {
        match self.get(
            device,
            size,
            time,
            content_alpha,
            content_scale,
            content_pivot,
        ) {
            Ok(_) => {
                *recovery_attempted = false;
                FrameParameterEncode::Ready
            }
            Err(DeviceLost) => {
                self.lost.set(true);
                *recovery_attempted = true;
                FrameParameterEncode::Abandoned
            }
        }
    }
}

/// Outcome of encoding one frame's draw parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::renderer) enum FrameParameterEncode {
    Ready,
    Abandoned,
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{DrawParameterCache, FrameParameterEncode};

    fn required_device() -> (wgpu::Device, wgpu::Queue) {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("a real wgpu adapter is required for the device-loss row");
        let info = adapter.get_info();
        eprintln!(
            "device-loss adapter: {} backend={:?} type={:?} driver={} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
        pollster::block_on(crate::request_renderer_device(
            &adapter,
            "device-loss draw parameters",
        ))
        .expect("a real renderer device is required for the device-loss row")
    }

    fn uniform_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("device-loss uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    /// Issue #826 (a): destroying the real device mid-frame must not panic.
    /// The frame is abandoned and recovery is requested.
    #[test]
    fn destroyed_device_abandons_the_draw_parameter_frame() {
        let (device, queue) = required_device();
        let cache = DrawParameterCache::new(uniform_layout(&device));
        device.destroy();
        let mut recovery_attempted = false;
        let encoded = catch_unwind(AssertUnwindSafe(|| {
            cache.encode_frame(
                &device,
                &queue,
                [2.0, 2.0],
                0.0,
                1.0,
                1.0,
                [0.0, 0.0],
                &mut recovery_attempted,
            )
        }));
        let fate = encoded.expect("draw-parameter encode panicked on a destroyed device");
        assert_eq!(fate, FrameParameterEncode::Abandoned);
        assert!(
            recovery_attempted,
            "the abandoned frame did not request recovery"
        );
    }
}
