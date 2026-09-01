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
mod tests {
    use super::*;

    #[test]
    fn vertex_upload_byte_range_starts_at_zero_for_first_upload() {
        assert_eq!(upload_byte_range(0, 48), 0..48);
    }

    #[test]
    fn vertex_upload_byte_range_tracks_arena_offset() {
        assert_eq!(upload_byte_range(48, 48), 48..96);
    }

    #[test]
    fn align_up_basic() {
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 4), 8);
        assert_eq!(align_up(48, 4), 48);
        assert_eq!(align_up(49, 4), 52);
    }

    #[test]
    fn grown_capacity_first_allocation_is_at_least_4096() {
        assert_eq!(grown_capacity(0, 1), Some(4096));
        assert_eq!(grown_capacity(0, 4096), Some(4096));
        assert_eq!(grown_capacity(0, 5000), Some(5000));
    }

    #[test]
    fn grown_capacity_doubles_until_fit() {
        assert_eq!(grown_capacity(4096, 4097), Some(8192));
        assert_eq!(grown_capacity(4096, 20000), Some(32768));
    }

    #[test]
    fn grown_capacity_steady_state_allocates_nothing() {
        assert_eq!(grown_capacity(4096, 4096), None);
        assert_eq!(grown_capacity(8192, 100), None);
    }
}
