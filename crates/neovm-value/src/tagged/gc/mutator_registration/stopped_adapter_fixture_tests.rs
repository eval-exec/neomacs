//! E3 slice 1 fixtures for the stopped-major post-mark adapters.
//!
//! The core shell owns the Context, the ordinary (no-permit) proof admission,
//! and the real `AllStopped` job. This module owns the fixtures that need
//! value-private allocators (surface/video handles, buffer/process/frame
//! registries) and the post-job observations that need the private reclaimed
//! drains. Everything here is called only from `core_tests/**` through the
//! `test_support::authority` fence.
//!
//! The properties under test are the stopped major's, not the serial stop's:
//! the serial path is already covered by `tests/ephemeron_cross_kind_test.rs`
//! (reached/unreached chains) and by the B8 weak-table bodies.

use crate::heap_types::ProcessId;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;
use crate::value::buffer_vocab::BufferId;

/// Resource-drain fixture: two handles the stop must record, two that must
/// survive because the caller roots them, and one preexisting pending id per
/// drain that admission must preserve.
pub struct StoppedAdapterResourceFixture {
    /// Live (caller-rooted) handles. The stop must not queue their ids.
    pub live_surface: TaggedValue,
    pub live_video: TaggedValue,
    dead_surface: u32,
    dead_video: u32,
    pending_surface: u32,
    pending_video: neovm_display_identity::VideoId,
}

/// Allocate the resource fixture on the serial heap before admission.
///
/// # Safety
///
/// The caller holds the sole serial keeper of `heap`; no collection, callback,
/// or owner runs between this call and the admission that freezes the heap.
pub unsafe fn prepare_stopped_adapter_resources(
    heap: &mut TaggedHeap,
) -> StoppedAdapterResourceFixture {
    let dead_surface = 0xE3_1001_u32;
    let dead_video = 0xE3_1002_u32;
    let pending_surface = 0xE3_2001_u32;
    let pending_video = neovm_display_identity::VideoId::new(0xE3_2002);

    let live_surface = heap.alloc_surface_handle(0xE3_3001);
    let live_video = heap.alloc_video_handle(neovm_display_identity::VideoId::new(0xE3_3002));
    // Unrooted: the stopped sweep frees both shells and records their ids.
    let _dead_surface = heap.alloc_surface_handle(dead_surface);
    let _dead_video = heap.alloc_video_handle(neovm_display_identity::VideoId::new(dead_video));

    // A preexisting drain entry is not a Value: admission keeps it and the
    // stop appends to the same queue instead of clearing it.
    heap.pending_surface_destroys.push(pending_surface);
    heap.pending_video_destroys.push(pending_video);

    StoppedAdapterResourceFixture {
        live_surface,
        live_video,
        dead_surface,
        dead_video,
        pending_surface,
        pending_video,
    }
}

/// Assert the stopped major's resource drains. Runs after the job has returned
/// the serial Context, so the drains are still waiting for their host drain.
pub fn assert_stopped_adapter_resources(
    heap: &mut TaggedHeap,
    fixture: &StoppedAdapterResourceFixture,
) {
    let surfaces = heap.take_pending_surface_destroys();
    assert_eq!(
        surfaces,
        vec![fixture.pending_surface, fixture.dead_surface],
        "the stopped major keeps the pending surface id and appends only the dead handle"
    );
    let videos = heap.take_pending_video_destroys();
    assert_eq!(
        videos,
        vec![
            fixture.pending_video,
            neovm_display_identity::VideoId::new(fixture.dead_video)
        ],
        "the stopped major keeps the pending video id and appends only the dead handle"
    );
    // The live handles are still owned by the caller-rooted variables; the
    // exact vectors above already prove the sweep queued no destroy for them.
}

/// Explicit-ephemeron fixture: one reached and one unreached id of each kind.
/// The reachable objects are returned so the caller roots them; their armed
/// records must be resolved without a reclaim.
pub struct StoppedAdapterEphemeronFixture {
    pub live_buffer: TaggedValue,
    pub live_process: TaggedValue,
    pub live_frame: TaggedValue,
    /// Field only reachable through the reached buffer's armed record.
    pub live_field: TaggedValue,
    dead_buffer: BufferId,
    dead_process: ProcessId,
    dead_frame: u64,
    live_buffer_id: BufferId,
    live_process_id: ProcessId,
    live_frame_id: u64,
}

/// Allocate and arm the explicit-ephemeron fixture on the serial heap.
///
/// # Safety
///
/// Same contract as [`prepare_stopped_adapter_resources`].
pub unsafe fn prepare_stopped_adapter_ephemerons(
    heap: &mut TaggedHeap,
) -> StoppedAdapterEphemeronFixture {
    let dead_buffer = BufferId(0xE3B1);
    let dead_process: ProcessId = 0xE3C1;
    let dead_frame = 0xE3F1;

    let dead_buffer_value = heap.alloc_buffer(dead_buffer);
    let dead_process_value = heap.alloc_process(dead_process);
    let dead_frame_value = heap.alloc_frame(dead_frame);
    heap.register_buffer_value(dead_buffer, dead_buffer_value);
    heap.register_process_value(dead_process, dead_process_value);
    heap.register_frame_value(dead_frame, dead_frame_value);

    let live_buffer_id = BufferId(0xE3B2);
    let live_process_id: ProcessId = 0xE3C2;
    let live_frame_id = 0xE3F2;
    let live_buffer = heap.alloc_buffer(live_buffer_id);
    let live_process = heap.alloc_process(live_process_id);
    let live_frame = heap.alloc_frame(live_frame_id);
    heap.register_buffer_value(live_buffer_id, live_buffer);
    heap.register_process_value(live_process_id, live_process);
    heap.register_frame_value(live_frame_id, live_frame);

    let live_field = heap.alloc_cons(TaggedValue::fixnum(0xE3), TaggedValue::NIL);

    // The dead chain is unreached end to end; the live chain is reached by
    // the caller's root, so its records resolve and their fields are marked.
    heap.arm_killed_buffer_ephemerons(vec![
        (dead_buffer, vec![dead_process_value]),
        (live_buffer_id, vec![live_field]),
    ]);
    heap.arm_retired_process_ephemerons(vec![
        (dead_process, vec![dead_frame_value]),
        (live_process_id, vec![live_field]),
    ]);
    heap.arm_deleted_frame_ephemerons(vec![
        (dead_frame, vec![dead_buffer_value]),
        (live_frame_id, vec![live_field]),
    ]);

    StoppedAdapterEphemeronFixture {
        live_buffer,
        live_process,
        live_frame,
        live_field,
        dead_buffer,
        dead_process,
        dead_frame,
        live_buffer_id,
        live_process_id,
        live_frame_id,
    }
}

/// Assert the stopped major ran all three explicit reclaims: the unreached
/// ids are recorded and their registry roots dropped; the reached ids and the
/// field their armed records publish are kept.
pub fn assert_stopped_adapter_ephemerons(
    heap: &mut TaggedHeap,
    fixture: &StoppedAdapterEphemeronFixture,
) {
    assert_eq!(
        heap.take_reclaimed_buffer_ids(),
        vec![fixture.dead_buffer],
        "the stopped major reclaims the unreached killed buffer"
    );
    assert_eq!(
        heap.take_reclaimed_process_ids(),
        vec![fixture.dead_process],
        "the stopped major reclaims the unreached retired process"
    );
    assert_eq!(
        heap.take_reclaimed_frame_ids(),
        vec![fixture.dead_frame],
        "the stopped major reclaims the unreached deleted frame"
    );

    assert!(
        heap.buffer_value(fixture.dead_buffer).is_none(),
        "a reclaimed killed buffer keeps no registry root"
    );
    assert!(
        heap.process_value(fixture.dead_process).is_none(),
        "a reclaimed retired process keeps no registry root"
    );
    assert!(
        heap.frame_value(fixture.dead_frame).is_none(),
        "a reclaimed deleted frame keeps no registry root"
    );

    assert!(
        heap.buffer_value(fixture.live_buffer_id).is_some(),
        "a reached killed buffer keeps its registry root"
    );
    assert!(
        heap.process_value(fixture.live_process_id).is_some(),
        "a reached retired process keeps its registry root"
    );
    assert!(
        heap.frame_value(fixture.live_frame_id).is_some(),
        "a reached deleted frame keeps its registry root"
    );

    assert!(
        heap.is_value_marked(fixture.live_field),
        "a reached ephemeron's field is published by the fixpoint"
    );
    // The reached objects themselves are still owned; a reclaimed id would
    // have dropped the registry root checked above. Their identity values are
    // returned to the caller, which roots them, so no separate bit check is
    // meaningful here.
}
