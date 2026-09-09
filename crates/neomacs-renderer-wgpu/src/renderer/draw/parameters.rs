use std::{cell::RefCell, collections::VecDeque};
use wgpu::util::DeviceExt;

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
    entries: RefCell<VecDeque<([u32; 3], DrawParameters)>>,
}

impl DrawParameterCache {
    pub(in crate::renderer) fn new(layout: wgpu::BindGroupLayout) -> Self {
        Self {
            layout,
            entries: RefCell::new(VecDeque::new()),
        }
    }

    pub(in crate::renderer) fn get(
        &self,
        device: &wgpu::Device,
        size: [f32; 2],
        time: f32,
    ) -> DrawParameters {
        let key = [size[0].to_bits(), size[1].to_bits(), time.to_bits()];
        let mut entries = self.entries.borrow_mut();
        if let Some(index) = entries.iter().position(|(k, _)| *k == key) {
            let entry = entries.remove(index).unwrap();
            let result = entry.1.clone();
            entries.push_back(entry);
            return result;
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Immutable draw parameters"),
            contents: bytemuck::bytes_of(&Uniforms {
                screen_size: size,
                time,
                _padding: 0.0,
            }),
            // Deliberately no COPY_DST: painters cannot overwrite this snapshot.
            usage: wgpu::BufferUsages::UNIFORM,
        });
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
        result
    }
}
