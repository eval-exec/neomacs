use super::*;
use crate::renderer::gpu_budget::UnpooledTexture;

/// The pool's payload in these tests. Nothing about reuse, eviction or
/// accounting depends on the payload being a real texture, and a GPU test
/// would skip on the software-rasterizer machines this code is written on.
#[derive(Debug, PartialEq, Eq)]
struct FakeTexture(u32);

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

fn size(width: u32, height: u32) -> SnapshotSize {
    SnapshotSize::new(width, height).expect("non-zero test size")
}

fn pool(limit_bytes: u64) -> SnapshotPool<FakeTexture> {
    SnapshotPool::new(GpuBudget::with_limit_bytes(
        NonZeroU64::new(limit_bytes).expect("non-zero test limit"),
    ))
}

fn acquire(
    pool: &mut SnapshotPool<FakeTexture>,
    size: SnapshotSize,
) -> Result<SnapshotLease<FakeTexture>, BudgetExceeded> {
    pool.acquire(size, FORMAT, || FakeTexture(size.width()))
}

#[test]
fn a_lease_returns_its_slot_to_the_pool_once_the_last_holder_drops_it() {
    // If this fails the pool leaks a full-frame texture per transition and
    // the budget drifts upward until it refuses every lease.
    let mut pool = pool(1 << 30);
    let first = acquire(&mut pool, size(8, 4)).expect("the first lease fits");
    let id = first.id();
    let held = first.clone();
    drop(first);

    let other = acquire(&mut pool, size(8, 4)).expect("the second lease fits");
    assert_ne!(
        other.id(),
        id,
        "a slot with a holder left is not free to hand out again"
    );

    drop(held);
    drop(other);
    let reused = acquire(&mut pool, size(8, 4)).expect("a released slot is available");
    assert_eq!(
        reused.id(),
        id,
        "the first slot came back once nobody held it"
    );
}

#[test]
fn a_second_lease_of_the_same_size_reuses_the_slot_the_first_one_freed() {
    // If this fails, the composition ring allocates a fresh full-frame
    // texture (14.75 MB at 2560x1440) every single frame instead of cycling
    // two.
    let mut pool = pool(1 << 30);
    let first = acquire(&mut pool, size(16, 16)).expect("the first lease fits");
    let id = first.id();
    drop(first);
    let second = acquire(&mut pool, size(16, 16)).expect("the second lease fits");
    assert_eq!(second.id(), id);
    assert_eq!(pool.slot_count(), 1, "reuse must not add a slot");
}

#[test]
fn the_pool_reclaims_before_it_decides_whether_a_lease_fits() {
    // If this fails, a lease released earlier in the same frame is still
    // charged and `acquire` reports BudgetExceeded for memory that is free —
    // the ring would degrade to un-animated frames on a budget it fits in.
    let bytes = texture_bytes(size(8, 8), FORMAT);
    let mut pool = pool(bytes);
    let only = acquire(&mut pool, size(8, 8)).expect("one slot fits exactly");
    drop(only);
    acquire(&mut pool, size(8, 8)).expect("the released slot is reusable, not a second allocation");
}

#[test]
fn the_pool_refuses_a_lease_that_would_cross_the_ceiling_rather_than_allocating_it() {
    // If this fails, the ceiling is decoration and the render thread can
    // exhaust GPU memory on a large window.
    let bytes = texture_bytes(size(8, 8), FORMAT);
    let mut pool = pool(bytes);
    let _held = acquire(&mut pool, size(8, 8)).expect("one slot fits exactly");
    let refused = acquire(&mut pool, size(8, 8)).expect_err("a second slot does not fit");
    assert_eq!(refused.requested_bytes, bytes);
    assert_eq!(pool.slot_count(), 1, "a refusal must allocate nothing");
}

#[test]
fn a_texture_the_render_thread_owns_outside_the_pool_shrinks_what_the_pool_may_allocate() {
    // If this fails, the atlas and the retained static scene are free as far
    // as leasing is concerned, and the "budget" only ever bounds the smaller
    // half of the render thread's GPU memory.
    let bytes = texture_bytes(size(8, 8), FORMAT);
    let mut pool = pool(bytes * 2);
    pool.budget_mut().record_unpooled(
        crate::renderer::gpu_budget::GpuBudgetOwner::FrameWindow(1),
        UnpooledTexture::GlyphAtlas,
        bytes + 1,
    );
    acquire(&mut pool, size(8, 8)).expect_err("the atlas already took more than half the ceiling");
}

#[test]
fn a_free_slot_of_the_wrong_size_is_evicted_to_make_room_rather_than_refusing_the_lease() {
    // If this fails, resizing a window permanently strands its old-size
    // textures under the ceiling and every later transition is refused.
    let old = texture_bytes(size(8, 8), FORMAT);
    let mut pool = pool(old + texture_bytes(size(4, 4), FORMAT));
    let stale = acquire(&mut pool, size(8, 8)).expect("the old size fits");
    drop(stale);
    let _small = acquire(&mut pool, size(4, 4)).expect("the new size fits beside the old one");
    let _also = acquire(&mut pool, size(4, 4))
        .expect("the stale free slot is evicted to make room for the new size");
    assert_eq!(
        pool.budget().pooled_bytes(),
        texture_bytes(size(4, 4), FORMAT) * 2
    );
}

#[test]
fn a_slot_someone_still_holds_is_never_evicted_to_satisfy_a_new_lease() {
    // If this fails the pool would free GPU memory a frame in flight is
    // sampling, which shows up as corruption rather than as a panic.
    let bytes = texture_bytes(size(8, 8), FORMAT);
    let mut pool = pool(bytes);
    let held = acquire(&mut pool, size(8, 8)).expect("one slot fits exactly");
    acquire(&mut pool, size(4, 4)).expect_err("nothing evictable, so the request is refused");
    assert_eq!(pool.slot_count(), 1);
    assert_eq!(held.size(), size(8, 8));
}

#[test]
fn a_free_slot_nobody_has_wanted_for_a_long_time_stops_being_charged() {
    // If this fails, textures are freed only by device loss or resize —
    // exactly the state this pool exists to end — and a window that stops
    // using the offscreen path keeps paying for it forever.
    let mut pool = pool(1 << 30);
    let stale = acquire(&mut pool, size(8, 8)).expect("the first lease fits");
    drop(stale);
    for _ in 0..(IDLE_SLOT_GRACE_ACQUIRES + 2) {
        let _churn = acquire(&mut pool, size(4, 4)).expect("small leases fit");
    }
    assert_eq!(
        pool.budget().pooled_bytes(),
        texture_bytes(size(4, 4), FORMAT),
        "only the slot still in rotation is charged"
    );
}
