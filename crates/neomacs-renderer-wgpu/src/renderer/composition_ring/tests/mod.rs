use super::*;
use crate::renderer::gpu_budget::GpuBudget;
use crate::renderer::snapshot_pool::{SnapshotId, SnapshotPool};
use std::num::NonZeroU64;

/// The ring cares only about which slot it holds, never about what is in it,
/// and every machine this is developed on would skip a GPU test.
#[derive(Debug)]
struct FakeTexture;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

fn size() -> SnapshotSize {
    SnapshotSize::new(8, 8).expect("non-zero test size")
}

fn pool(slots_that_fit: u64) -> SnapshotPool<FakeTexture> {
    let bytes = crate::renderer::snapshot_pool::texture_bytes(size(), FORMAT) * slots_that_fit;
    SnapshotPool::new(GpuBudget::with_limit_bytes(
        NonZeroU64::new(bytes).expect("non-zero test limit"),
    ))
}

fn advance(
    ring: &mut CompositionRing<FakeTexture>,
    pool: &mut SnapshotPool<FakeTexture>,
) -> Result<(), BudgetExceeded> {
    ring.advance(|| pool.acquire(size(), FORMAT, || FakeTexture))
}

fn new_ring(pool: &mut SnapshotPool<FakeTexture>) -> CompositionRing<FakeTexture> {
    CompositionRing::new(
        pool.acquire(size(), FORMAT, || FakeTexture)
            .expect("the first composition fits"),
    )
}

#[test]
fn advancing_the_ring_makes_the_picture_just_composed_the_previous_one() {
    // If this fails there is no "previous composition" for a crossfade or a
    // departing pane to sample, and every transition starts from whatever the
    // destination frame drew.
    let mut pool = pool(4);
    let mut ring = new_ring(&mut pool);
    let composed = ring.current().id();
    assert!(ring.previous().is_none(), "nothing has been composed yet");

    advance(&mut ring, &mut pool).expect("the ring fits");
    assert_eq!(ring.previous().map(SnapshotLease::id), Some(composed));
    assert_ne!(
        ring.current().id(),
        composed,
        "the frame about to be drawn must not overwrite the picture it animates from"
    );
}

#[test]
fn the_composition_a_transition_is_animating_from_is_not_reused_while_it_runs() {
    // This is the invariant the `current_is_a` flip-flop used to get from an
    // index and the pool would break by handing back an arbitrary free slot.
    // If it fails, a crossfade's second frame samples the frame it is fading
    // *to*, because the slot holding its source was re-leased underneath it.
    let mut pool = pool(8);
    let mut ring = new_ring(&mut pool);
    advance(&mut ring, &mut pool).expect("the ring fits");

    let animating_from = ring
        .previous()
        .expect("one frame has been composed")
        .clone();
    let pinned = animating_from.id();

    let mut handed_out: Vec<SnapshotId> = Vec::new();
    for _ in 0..6 {
        advance(&mut ring, &mut pool).expect("the ring fits");
        handed_out.push(ring.current().id());
    }
    assert!(
        !handed_out.contains(&pinned),
        "the pool handed back {pinned:?}, which a transition was still animating from"
    );
}

#[test]
fn a_ring_at_rest_cycles_two_slots_instead_of_allocating_a_new_one_each_frame() {
    // If this fails, every offscreen frame allocates a full-frame texture
    // (14.75 MB at 2560x1440) and the pool is a leak with extra steps.
    let mut pool = pool(8);
    let mut ring = new_ring(&mut pool);
    for _ in 0..10 {
        advance(&mut ring, &mut pool).expect("the ring fits");
    }
    assert_eq!(pool.slot_count(), 2);
}

#[test]
fn a_ring_whose_advance_is_refused_keeps_the_picture_it_had_composed() {
    // If this fails, a frame that cannot afford a new slot loses the one
    // already on screen and the window flashes empty under GPU pressure
    // instead of simply drawing without an offscreen.
    let mut pool = pool(1);
    let mut ring = new_ring(&mut pool);
    let composed = ring.current().id();
    advance(&mut ring, &mut pool).expect_err("only one slot fits under this ceiling");
    assert_eq!(
        ring.current().id(),
        composed,
        "the refused advance must not discard the current composition"
    );
}
