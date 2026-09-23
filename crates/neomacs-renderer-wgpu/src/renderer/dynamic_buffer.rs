use bytemuck::Pod;
use std::marker::PhantomData;

pub struct FrameVertexArena<T: Pod> {
    buffer: Option<wgpu::Buffer>,
    capacity_bytes: wgpu::BufferAddress,
    cursor_bytes: wgpu::BufferAddress,
    retired: Vec<wgpu::Buffer>,
    /// Monotonic count of GPU buffer allocations (growth events) over the
    /// arena's lifetime. Snapshotted per frame for the `buffers_created` stat.
    buffers_created: u64,
    label: &'static str,
    _marker: PhantomData<T>,
}

pub struct VertexUpload {
    buffer: wgpu::Buffer,
    offset_bytes: wgpu::BufferAddress,
    len_bytes: wgpu::BufferAddress,
}

impl VertexUpload {
    pub fn byte_range(&self) -> std::ops::Range<wgpu::BufferAddress> {
        upload_byte_range(self.offset_bytes, self.len_bytes)
    }

    /// Slice of the arena buffer holding this upload. The upload owns a
    /// handle to the buffer, so the slice needs no borrow of the arena.
    pub fn buffer_slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(self.byte_range())
    }
}

const ALIGN: wgpu::BufferAddress = 4;

fn align_up(offset: wgpu::BufferAddress, align: wgpu::BufferAddress) -> wgpu::BufferAddress {
    (offset + align - 1) & !(align - 1)
}

fn upload_byte_range(
    offset: wgpu::BufferAddress,
    len: wgpu::BufferAddress,
) -> std::ops::Range<wgpu::BufferAddress> {
    offset..offset + len
}

/// Pure growth policy: the capacity after ensuring `needed_bytes` fit.
/// Returns `None` when the current capacity already suffices (no new buffer).
fn grown_capacity(
    capacity_bytes: wgpu::BufferAddress,
    needed_bytes: wgpu::BufferAddress,
) -> Option<wgpu::BufferAddress> {
    if needed_bytes <= capacity_bytes {
        return None;
    }
    Some(if capacity_bytes == 0 {
        needed_bytes.max(4096)
    } else {
        let mut c = capacity_bytes;
        while c < needed_bytes {
            c *= 2;
        }
        c
    })
}

impl<T: Pod> FrameVertexArena<T> {
    pub fn new(label: &'static str) -> Self {
        Self {
            buffer: None,
            capacity_bytes: 0,
            cursor_bytes: 0,
            retired: Vec::new(),
            buffers_created: 0,
            label,
            _marker: PhantomData,
        }
    }

    pub fn begin_frame(&mut self) {
        self.cursor_bytes = 0;
        self.retired.clear();
    }

    /// Total GPU buffer allocations over the arena's lifetime (monotonic;
    /// steady-state frames add zero).
    pub fn buffers_created(&self) -> u64 {
        self.buffers_created
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[T],
    ) -> Option<VertexUpload> {
        if vertices.is_empty() {
            return None;
        }

        let bytes = bytemuck::cast_slice(vertices);
        let len = bytes.len() as wgpu::BufferAddress;
        let offset = align_up(self.cursor_bytes, ALIGN);
        let end = offset + len;

        self.ensure_capacity(device, end);

        let buffer = self.buffer.as_ref().unwrap().clone();
        queue.write_buffer(&buffer, offset, bytes);
        self.cursor_bytes = end;

        Some(VertexUpload {
            buffer,
            offset_bytes: offset,
            len_bytes: len,
        })
    }

    pub fn slice<'a>(&self, upload: &'a VertexUpload) -> wgpu::BufferSlice<'a> {
        upload.buffer.slice(upload.byte_range())
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, needed_bytes: wgpu::BufferAddress) {
        let Some(new_capacity) = grown_capacity(self.capacity_bytes, needed_bytes) else {
            return;
        };

        if let Some(old) = self.buffer.take() {
            self.retired.push(old);
        }

        self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: new_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.capacity_bytes = new_capacity;
        self.buffers_created += 1;
    }
}

#[cfg(test)]
#[path = "dynamic_buffer/tests/dynamic_buffer_test.rs"]
mod tests;
