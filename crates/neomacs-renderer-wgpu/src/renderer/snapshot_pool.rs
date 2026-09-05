//! The single owner of every pooled full-frame GPU texture.
//!
//! # How a lease gets back to the pool
//!
//! A lease is an `Rc<Slot>`; the pool keeps one `Rc` per slot and hands out
//! clones. A slot is free exactly when `Rc::strong_count` is 1, which is to
//! say when the pool is the only holder. `Rc`'s own `Drop` is what returns a
//! lease, and [`SnapshotPool::acquire`] observes the result before it decides
//! anything — no `Drop` impl of our own, no generation counters, no return
//! queue.
//!
//! The two alternatives were both worse here:
//!
//! * An `Arc<Mutex<Pool>>` handle inside each lease lets `Drop` re-enter the
//!   pool directly, but a lease released while a render pass holds
//!   `&mut pool` deadlocks — and every release site in this codebase is
//!   inside such a pass. `Rc<RefCell<Pool>>` converts that deadlock into a
//!   panic, which is not an improvement.
//! * A generational slot index cannot notify the pool from `Drop` at all, so
//!   it needs a return queue anyway, and it replaces an ownership guarantee
//!   with a runtime "is this handle still valid" check. That is backwards:
//!   the illegal state should be unrepresentable, not detected.
//!
//! `Rc` is sound here because every one of these textures is created, read
//! and dropped on the render thread alone. `RenderApp` is built inside the
//! spawned render closure and only ever passed by `&mut` to
//! `event_loop.run_app`, which imposes no `Send` bound; the crate has no
//! `unsafe impl Send`, and the only worker threads (image decoders) carry CPU
//! pixel data and never a wgpu handle.
//!
//! # Why the use-after-free cannot happen
//!
//! A slot's wgpu objects live in the `Slot` behind the `Rc`, and the pool
//! only ever drops a slot whose strong count is 1. A slot any lease still
//! names therefore cannot be released, so a frame that holds a lease cannot
//! be sampling freed GPU memory. Handing out an arbitrary free slot is
//! likewise impossible for a slot someone holds — which is the property the
//! old `current_is_a` flip-flop got from an index invariant and this gets
//! from ownership.
//!
//! # Why the slot payload is a type parameter
//!
//! Only [`SnapshotResources`] is ever used in production. The parameter
//! exists so the pool's reuse, eviction and budget behaviour can be tested
//! with no GPU at all: this code is developed against a software rasterizer
//! where the whole offscreen path is forced off, and a test that skips when
//! no adapter is present proves nothing about the invariant it claims to
//! defend.

use std::num::{NonZeroU32, NonZeroU64};
use std::rc::Rc;

use super::gpu_budget::{BudgetExceeded, GpuBudget};

/// A full-frame texture size. Zero in either axis is unrepresentable, so no
/// call site has to check for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotSize {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl SnapshotSize {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }

    pub fn width(&self) -> u32 {
        self.width.get()
    }

    pub fn height(&self) -> u32 {
        self.height.get()
    }
}

/// Bytes one full-frame texture of this size and format occupies.
pub fn texture_bytes(size: SnapshotSize, format: wgpu::TextureFormat) -> u64 {
    // Every format the pool is asked for is a single-plane colour format, so
    // one block copy is one texel; `None` would mean a depth/stencil aspect
    // was requested, which this pool never allocates.
    let bytes_per_texel = u64::from(format.block_copy_size(None).unwrap_or(4));
    u64::from(size.width.get()) * u64::from(size.height.get()) * bytes_per_texel
}

/// Identity of one pool slot, stable for as long as the slot exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotId(NonZeroU64);

/// The GPU objects one production pool slot owns.
pub struct SnapshotResources {
    /// Held, never read: this is the handle the budget charged for, and the
    /// allocation's lifetime should be a fact of this struct rather than a
    /// consequence of wgpu's internal refcounting behind the view.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl SnapshotResources {
    pub(super) fn create(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: SnapshotSize,
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Snapshot Pool Slot"),
            size: wgpu::Extent3d {
                width: size.width(),
                height: size.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Snapshot Pool Slot Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Self {
            _texture: texture,
            view,
            bind_group,
        }
    }
}

struct Slot<R> {
    id: SnapshotId,
    size: SnapshotSize,
    format: wgpu::TextureFormat,
    bytes: u64,
    resources: R,
}

/// A held claim on one pool slot.
///
/// Deliberately exposes no texture: a pass that could name the texture could
/// render into a slot it does not hold, and in wgpu a `Texture` is a cloneable
/// handle, so "the texture cannot outlive the lease" is not something a lease
/// could enforce anyway. What it does enforce is that the pool's accounting is
/// exact and that the slot is not re-leased while anyone holds it.
pub struct SnapshotLease<R = SnapshotResources>(Rc<Slot<R>>);

impl<R> Clone for SnapshotLease<R> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<R> std::fmt::Debug for SnapshotLease<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapshotLease")
            .field("id", &self.0.id)
            .field("size", &self.0.size)
            .finish()
    }
}

impl<R> SnapshotLease<R> {
    pub fn id(&self) -> SnapshotId {
        self.0.id
    }

    pub fn size(&self) -> SnapshotSize {
        self.0.size
    }
}

impl SnapshotLease<SnapshotResources> {
    pub fn view(&self) -> &wgpu::TextureView {
        &self.0.resources.view
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.0.resources.bind_group
    }
}

struct PoolSlot<R> {
    shared: Rc<Slot<R>>,
    /// The acquire serial that last handed this slot out, so a slot nobody
    /// has wanted for a while can be released instead of sitting resident
    /// until the next device loss.
    last_leased_at: u64,
}

impl<R> PoolSlot<R> {
    fn is_free(&self) -> bool {
        Rc::strong_count(&self.shared) == 1
    }
}

/// How many acquires a free slot survives before the pool releases it.
///
/// It must comfortably exceed the number of acquires one frame makes across
/// every frame window, or two windows of different sizes would evict each
/// other's slots every frame and reallocate them the next. A frame acquires
/// about one slot per window, so this leaves room for far more windows than
/// a session has while still bounding how long a stale size stays resident
/// after a resize.
const IDLE_SLOT_GRACE_ACQUIRES: u64 = 64;

/// The single owner of every pooled full-frame texture, and of the budget
/// they are charged to.
pub struct SnapshotPool<R = SnapshotResources> {
    slots: Vec<PoolSlot<R>>,
    budget: GpuBudget,
    acquires: u64,
    next_id: u64,
}

impl<R> SnapshotPool<R> {
    pub fn new(budget: GpuBudget) -> Self {
        Self {
            slots: Vec::new(),
            budget,
            acquires: 0,
            next_id: 0,
        }
    }

    pub fn budget(&self) -> &GpuBudget {
        &self.budget
    }

    pub(super) fn budget_mut(&mut self) -> &mut GpuBudget {
        &mut self.budget
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Lease a slot of `size` and `format`, reusing a free one if there is
    /// one and allocating through the budget otherwise.
    ///
    /// `create` runs only when a new allocation is actually made, and only
    /// after the budget has admitted it.
    pub fn acquire(
        &mut self,
        size: SnapshotSize,
        format: wgpu::TextureFormat,
        create: impl FnOnce() -> R,
    ) -> Result<SnapshotLease<R>, BudgetExceeded> {
        self.acquires += 1;
        let serial = self.acquires;
        // Reclaim before deciding: a lease released earlier in this same
        // frame must be reusable now, or the ring would allocate a fresh
        // full-frame texture every frame instead of cycling two.
        self.release_idle_slots();

        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.is_free() && slot.shared.size == size && slot.shared.format == format)
        {
            slot.last_leased_at = serial;
            return Ok(SnapshotLease(Rc::clone(&slot.shared)));
        }

        let bytes = texture_bytes(size, format);
        while let Err(exceeded) = self.budget.try_charge_pooled(bytes) {
            if !self.evict_least_recently_leased_free_slot() {
                return Err(exceeded);
            }
        }

        self.next_id += 1;
        let id = SnapshotId(NonZeroU64::new(self.next_id).expect("serial starts at one"));
        let shared = Rc::new(Slot {
            id,
            size,
            format,
            bytes,
            resources: create(),
        });
        self.slots.push(PoolSlot {
            shared: Rc::clone(&shared),
            last_leased_at: serial,
        });
        tracing::trace!(
            ?id,
            width = size.width(),
            height = size.height(),
            bytes,
            slots = self.slots.len(),
            pooled_bytes = self.budget.pooled_bytes(),
            "allocated a snapshot pool slot"
        );
        Ok(SnapshotLease(shared))
    }

    fn release_idle_slots(&mut self) {
        let serial = self.acquires;
        let budget = &mut self.budget;
        self.slots.retain(|slot| {
            let idle = slot.is_free()
                && serial.saturating_sub(slot.last_leased_at) > IDLE_SLOT_GRACE_ACQUIRES;
            if idle {
                budget.refund_pooled(slot.shared.bytes);
            }
            !idle
        });
    }

    fn evict_least_recently_leased_free_slot(&mut self) -> bool {
        let Some(index) = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_free())
            .min_by_key(|(_, slot)| slot.last_leased_at)
            .map(|(index, _)| index)
        else {
            return false;
        };
        let slot = self.slots.remove(index);
        self.budget.refund_pooled(slot.shared.bytes);
        true
    }
}

#[cfg(test)]
#[path = "snapshot_pool_test.rs"]
mod tests;
