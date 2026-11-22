#![forbid(unsafe_code)]

use super::*;
use crate::tagged::gc::HeapIdentity;

fn keys() -> [MutatorKey; 2] {
    let heap = HeapIdentity::issue();
    std::array::from_fn(|index| MutatorKey {
        heap,
        world: heap,
        owner: index as u64 + 1,
        registration: NonZeroU64::new(index as u64 + 1).unwrap(),
        generation: NonZeroU64::new(index as u64 + 1).unwrap(),
    })
}

fn members(keys: [MutatorKey; 2], first: Mode, second: Mode) -> [(MutatorKey, Mode); 2] {
    [(keys[0], first), (keys[1], second)]
}

fn requested(state: &mut State, membership: Membership<'_>) -> NonZeroU64 {
    match state.request(membership).unwrap() {
        Request::Active(epoch) => epoch,
        Request::Queued => panic!("fixture expected an active request"),
    }
}

#[test]
fn coalescing_and_last_restore_keep_membership_frozen_until_idle() {
    let keys = keys();
    let running = members(keys, Mode::Running, Mode::Running);
    let stopped = members(keys, Mode::Stopped, Mode::Stopped);
    let mut state = State::new();
    let epoch = requested(&mut state, &running);
    assert_eq!(requested(&mut state, &running), epoch);
    assert!(!state.ready(&running).unwrap());
    state.begin_trace(epoch, &stopped).unwrap();
    state.complete(epoch, &stopped).unwrap();
    let mut owners = [
        OwnerState::new(Mode::Stopped),
        OwnerState::new(Mode::Stopped),
    ];
    for owner in &mut owners {
        owner.image_epoch = epoch.get();
    }
    state.begin_restore(keys[0], &mut owners[0], false).unwrap();
    assert!(matches!(state.request(&stopped), Ok(Request::Queued)));
    state.finish_restore(keys[0], &mut owners[0]).unwrap();
    assert!(
        !state
            .finish_debt(&members(keys, Mode::Running, Mode::Stopped))
            .unwrap()
    );
    assert_eq!(state.epoch, epoch.get());
    state.begin_restore(keys[1], &mut owners[1], false).unwrap();
    state.finish_restore(keys[1], &mut owners[1]).unwrap();
    assert!(state.finish_debt(&running).unwrap());
    assert!(state.queued);
    assert_eq!(state.phase, Phase::Idle);
    assert_eq!(requested(&mut state, &running).get(), epoch.get() + 1);
    assert!(!state.queued);
    assert_eq!(state.frozen, keys);
}

#[test]
fn native_idle_return_reserves_debt_before_request_can_activate() {
    let keys = keys();
    let mut state = State::new();
    let mut native = OwnerState::new(Mode::NativeStopped);
    assert_eq!(state.begin_restore(keys[0], &mut native, true).unwrap(), 0);
    assert_eq!(native.mode, Mode::Resuming);
    assert_eq!(state.debt, vec![keys[0]]);
    let restoring = members(keys, Mode::Resuming, Mode::Running);
    assert!(matches!(state.request(&restoring), Ok(Request::Queued)));
    assert_eq!(state.epoch, 0);
    state.finish_restore(keys[0], &mut native).unwrap();
    assert!(
        state
            .finish_debt(&members(keys, Mode::Running, Mode::Running))
            .unwrap()
    );
    assert_eq!(native.image_takes, 1);
}

#[test]
fn unchanged_native_publication_spans_two_epochs_and_takes_latest_image_once() {
    let keys = keys();
    let mut state = State::new();
    let mut native = OwnerState::new(Mode::NativeStopped);
    let mut ordinary = OwnerState::new(Mode::Running);
    for expected_epoch in 1..=2 {
        let epoch = requested(&mut state, &members(keys, native.mode, ordinary.mode));
        assert_eq!(epoch.get(), expected_epoch);
        native.coverage_epoch = epoch.get();
        ordinary.mode = Mode::Stopped;
        ordinary.coverage_epoch = epoch.get();
        let stopped = members(keys, native.mode, ordinary.mode);
        state.begin_trace(epoch, &stopped).unwrap();
        // The real wrapper owns and validates the latest images before this
        // transition; this pure model carries their completion epoch only.
        native.image_epoch = epoch.get();
        ordinary.image_epoch = epoch.get();
        state.complete(epoch, &stopped).unwrap();
        assert_eq!(state.debt, vec![keys[1]]);
        state.begin_restore(keys[1], &mut ordinary, false).unwrap();
        state.finish_restore(keys[1], &mut ordinary).unwrap();
        assert!(
            state
                .finish_debt(&members(keys, native.mode, ordinary.mode))
                .unwrap()
        );
        assert_eq!(native.image_takes, 0);
    }
    assert_eq!(state.begin_restore(keys[0], &mut native, true).unwrap(), 2);
    state.finish_restore(keys[0], &mut native).unwrap();
    assert!(
        state
            .finish_debt(&members(keys, native.mode, ordinary.mode))
            .unwrap()
    );
    assert_eq!(native.image_takes, 1);
    assert_eq!(ordinary.image_takes, 2);
    assert_eq!(state.traces, 2);
}

#[test]
fn late_starts_stay_outside_frozen_set_and_removed_owner_is_refused() {
    let keys = keys();
    let mut state = State::new();
    let epoch = requested(
        &mut state,
        &members(keys, Mode::Running, Mode::StartingStopped),
    );
    assert_eq!(state.frozen, vec![keys[0]]);
    assert!(matches!(
        state.ready(&members(keys, Mode::Retired, Mode::StartingStopped)),
        Err(RendezvousError::Identity)
    ));
    let mut stale = keys[0];
    stale.generation = NonZeroU64::new(99).unwrap();
    assert!(matches!(
        state.ready(&[(stale, Mode::Stopped)]),
        Err(RendezvousError::Identity)
    ));
    assert_eq!(state.phase, Phase::Stopping(epoch));
    let stopped = members(keys, Mode::Stopped, Mode::StartingStopped);
    state.begin_trace(epoch, &stopped).unwrap();
    state.active_readers = 1;
    assert!(matches!(
        state.complete(epoch, &stopped),
        Err(RendezvousError::Readers)
    ));
    assert_eq!(state.completed, 0);
    assert!(state.debt.is_empty());
    state.active_readers = 0;
    state.complete(epoch, &stopped).unwrap();
    assert_eq!(state.debt, vec![keys[0]]);
}

#[test]
fn exhaustion_duplicate_membership_and_terminal_poison_do_not_discharge_obligations() {
    let keys = keys();
    let mut state = State::new();
    assert!(matches!(
        state.activate(&[(keys[0], Mode::Running), (keys[0], Mode::Running)]),
        Err(RendezvousError::Identity)
    ));
    assert_eq!(state.epoch, 0);
    state.epoch = u64::MAX;
    assert!(matches!(
        state.request(&members(keys, Mode::Running, Mode::Running)),
        Err(RendezvousError::Generation)
    ));
    assert!(state.frozen.is_empty());
    assert_eq!(state.phase, Phase::Idle);
    state.epoch = 0;
    let mut native = OwnerState::new(Mode::NativeStopped);
    state.begin_restore(keys[0], &mut native, true).unwrap();
    let debt = state.debt.clone();
    state.poison();
    assert!(matches!(
        state.finish_restore(keys[0], &mut native),
        Err(RendezvousError::Poisoned)
    ));
    assert_eq!(state.debt, debt);
    assert_eq!(native.mode, Mode::Resuming);
    assert_eq!(native.image_takes, 0);
}

fn ring_keys() -> [MutatorKey; 3] {
    let heap = HeapIdentity::issue();
    std::array::from_fn(|index| MutatorKey {
        heap,
        world: heap,
        owner: index as u64 + 1,
        registration: NonZeroU64::new(index as u64 + 1).unwrap(),
        generation: NonZeroU64::new(index as u64 + 1).unwrap(),
    })
}

fn ring_generation() -> NonZeroU64 {
    NonZeroU64::new(11).unwrap()
}

fn running(keys: &[MutatorKey]) -> Vec<(MutatorKey, Mode)> {
    keys.iter().map(|key| (*key, Mode::Running)).collect()
}

#[test]
fn ring_grants_in_commit_order_and_only_once() {
    let mut state = State::new();
    let keys = ring_keys();
    let generation = ring_generation();
    let a = state.enqueue_serial(keys[0], generation).unwrap();
    let b = state.enqueue_serial(keys[1], generation).unwrap();
    let c = state.enqueue_serial(keys[2], generation).unwrap();
    let members = running(&keys);

    assert_eq!(state.ring_head(), Some(a));
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(a));
    assert_eq!(
        state.try_grant_head(&members, |_| true).unwrap(),
        None,
        "a granted head is not regranted"
    );
    state.retire_serial(a).unwrap();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(b));
    state.retire_serial(b).unwrap();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(c));
    assert_eq!(state.ring_len(), 1);
}

#[test]
fn ring_stop_priority_keeps_order_and_defers_the_grant() {
    let mut state = State::new();
    let keys = ring_keys();
    let generation = ring_generation();
    let a = state.enqueue_serial(keys[0], generation).unwrap();
    let b = state.enqueue_serial(keys[1], generation).unwrap();
    let members = running(&keys);

    // A GC stop activates while both requests are queued.
    let _epoch = requested(&mut state, &members);
    assert!(matches!(state.phase, Phase::Stopping(_)));
    assert_eq!(state.ring_head(), Some(a), "the stop does not reorder");
    assert_eq!(state.ring_len(), 2, "the stop loses no requester");
    assert_eq!(
        state.try_grant_head(&members, |_| true).unwrap(),
        None,
        "no grant during Stopping"
    );

    // Unit shortcut back to Idle: the ring itself was untouched, so the first
    // commit still grants first.
    state.phase = Phase::Idle;
    state.frozen.clear();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(a));
    state.retire_serial(a).unwrap();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(b));
}

#[test]
fn ring_request_id_wrap_is_a_typed_refusal() {
    let mut state = State::new();
    let keys = ring_keys();
    state.next_request = u64::MAX;
    assert_eq!(
        state.enqueue_serial(keys[0], ring_generation()),
        Err(RendezvousError::Generation)
    );
    assert_eq!(state.ring_len(), 0, "the refused enqueue changed nothing");

    state.next_request = u64::MAX - 1;
    let last = state.enqueue_serial(keys[0], ring_generation()).unwrap();
    assert_eq!(last.get(), u64::MAX);
    assert_eq!(
        state.enqueue_serial(keys[1], ring_generation()),
        Err(RendezvousError::Generation)
    );
}

#[test]
fn ring_poison_while_granted_retires_exactly_once() {
    let mut state = State::new();
    let keys = ring_keys();
    let a = state.enqueue_serial(keys[0], ring_generation()).unwrap();
    let members = running(&keys);
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(a));
    state.poison();
    assert_eq!(
        state.try_grant_head(&members, |_| true),
        Err(RendezvousError::Poisoned)
    );
    assert_eq!(
        state.enqueue_serial(keys[1], ring_generation()),
        Err(RendezvousError::Poisoned)
    );
    assert_eq!(state.ring_head(), Some(a), "the granted ticket is retained");
    assert_eq!(state.ring_len(), 1);
    state.retire_serial(a).unwrap();
    assert_eq!(state.ring_len(), 0);
    assert_eq!(
        state.retire_serial(a),
        Err(RendezvousError::Identity),
        "a second retire is a typed refusal"
    );
}

#[test]
fn ring_grants_once_during_draining_resumes() {
    let mut state = State::new();
    let keys = ring_keys();
    let a = state.enqueue_serial(keys[0], ring_generation()).unwrap();
    let b = state.enqueue_serial(keys[1], ring_generation()).unwrap();
    state.phase = Phase::DrainingResumes;
    assert!(state.debt.is_empty());
    let members = running(&keys);
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(a));
    assert_eq!(
        state.try_grant_head(&members, |_| true).unwrap(),
        None,
        "no double grant while the head is Granted"
    );
    state.retire_serial(a).unwrap();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(b));
}

#[test]
fn ring_holds_the_head_when_the_owner_is_not_ready() {
    let mut state = State::new();
    let keys = ring_keys();
    let a = state.enqueue_serial(keys[0], ring_generation()).unwrap();
    let b = state.enqueue_serial(keys[1], ring_generation()).unwrap();
    let members = running(&keys);
    assert_eq!(
        state
            .try_grant_head(&members, |entry| entry.owner() != keys[0])
            .unwrap(),
        None,
        "an unready head is not skipped"
    );
    assert_eq!(state.ring_head(), Some(a));
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(a));
    state.retire_serial(a).unwrap();
    assert_eq!(state.try_grant_head(&members, |_| true).unwrap(), Some(b));
}
