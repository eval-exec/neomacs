#![forbid(unsafe_code)]

//! L6 ring control under Loom. The product `rendezvous/control.rs` is copied
//! into OUT_DIR by build.rs; these tests drive that real state machine under a
//! Loom Mutex/Condvar. The model supplies the two product vocabulary types.
//! A grant happens only inside the keeper's traced stop (L6.7 K3;
//! `grant_head_in_stop`), so each keeper step here takes a stop, grants the
//! head, retires it, and completes the stop.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MutatorKey {
    pub heap: u64,
    pub world: u64,
    pub owner: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousError {
    Poisoned,
    Identity,
    Loan,
    Publication,
    Generation,
    Busy,
    Readers,
    Capacity,
    Root,
}

mod control {
    include!(concat!(env!("OUT_DIR"), "/control.rs"));
}

use control::{Mode, Request, RequestId, State};
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Condvar, Mutex};
use std::num::NonZeroU64;

fn key(owner: u64) -> MutatorKey {
    MutatorKey {
        heap: 0,
        world: 0,
        owner,
    }
}

fn generation() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap()
}

fn running(keys: &[MutatorKey]) -> Vec<(MutatorKey, Mode)> {
    keys.iter().map(|k| (*k, Mode::Running)).collect()
}

/// The same members once each has stopped natively (no resume debt).
fn stopped(members: &[(MutatorKey, Mode)]) -> Vec<(MutatorKey, Mode)> {
    members
        .iter()
        .map(|(k, _)| (*k, Mode::NativeStopped))
        .collect()
}

/// Take a stop over MEMBERS and trace it: the only place a grant happens.
fn traced_stop(s: &mut State, members: &[(MutatorKey, Mode)]) -> NonZeroU64 {
    let epoch = match s.request(members).unwrap() {
        Request::Active(epoch) => epoch,
        Request::Queued => panic!("an Idle ring activates the stop"),
    };
    s.begin_trace(epoch, &stopped(members)).unwrap();
    epoch
}

/// Complete the stop and drain back to Idle.
fn finish_stop(s: &mut State, epoch: NonZeroU64, members: &[(MutatorKey, Mode)]) {
    s.complete(epoch, &stopped(members)).unwrap();
    assert!(s.finish_debt(&stopped(members)).unwrap(), "back to Idle");
}

/// (a) N requesters plus a keeper: grants follow commit order and no committed
/// requester is lost. Commit order is the id assignment order, recorded under
/// the same state lock.
#[test]
fn grant_order_is_commit_order() {
    loom::model(|| {
        let state = Arc::new(Mutex::new(State::new()));
        let wake = Arc::new(Condvar::new());
        let committed = Arc::new(Mutex::new(Vec::new()));
        let committed_len = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for owner in 0..2u64 {
            let state = Arc::clone(&state);
            let wake = Arc::clone(&wake);
            let committed = Arc::clone(&committed);
            let committed_len = Arc::clone(&committed_len);
            handles.push(loom::thread::spawn(move || {
                let mut s = state.lock().unwrap();
                let id = s.enqueue_serial(key(owner), generation()).unwrap();
                committed.lock().unwrap().push((owner, id));
                committed_len.fetch_add(1, Ordering::SeqCst);
                wake.notify_one();
            }));
        }

        let members = running(&[key(0), key(1)]);
        let mut granted: Vec<RequestId> = Vec::new();
        while granted.len() < 2 {
            let mut s = state.lock().unwrap();
            while committed_len.load(Ordering::SeqCst) <= granted.len() {
                s = wake.wait(s).unwrap();
            }
            let epoch = traced_stop(&mut s, &members);
            let id = s
                .grant_head_in_stop(epoch, |_| true)
                .unwrap()
                .expect("a committed head is grantable");
            s.retire_serial(id).unwrap();
            finish_stop(&mut s, epoch, &members);
            granted.push(id);
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let committed = committed.lock().unwrap().clone();
        assert_eq!(committed.len(), 2, "no committed requester is lost");
        assert_eq!(
            granted,
            committed.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            "grants follow commit order"
        );
    });
}

/// (b) a queued requester is cancelled (owner exit): the entry is retired
/// exactly once, the other requester is granted, and nothing is duplicated.
#[test]
fn cancel_while_queued_retires_once_and_the_other_grants() {
    loom::model(|| {
        let state = Arc::new(Mutex::new(State::new()));
        let wake = Arc::new(Condvar::new());
        let committed = Arc::new(Mutex::new(Vec::new()));
        let committed_len = Arc::new(AtomicUsize::new(0));
        let a_id = Arc::new(Mutex::new(None::<RequestId>));
        let mut handles = Vec::new();

        // A enqueues, then cancels its own entry. If the keeper already
        // granted and retired it, the second retirement is the typed Identity
        // refusal; both outcomes mean the entry is gone exactly once.
        {
            let state = Arc::clone(&state);
            let wake = Arc::clone(&wake);
            let committed = Arc::clone(&committed);
            let committed_len = Arc::clone(&committed_len);
            let a_id = Arc::clone(&a_id);
            handles.push(loom::thread::spawn(move || {
                let id = {
                    let mut s = state.lock().unwrap();
                    let id = s.enqueue_serial(key(0), generation()).unwrap();
                    committed.lock().unwrap().push((0, id));
                    committed_len.fetch_add(1, Ordering::SeqCst);
                    wake.notify_one();
                    id
                };
                *a_id.lock().unwrap() = Some(id);
                let mut s = state.lock().unwrap();
                match s.retire_serial(id) {
                    Ok(()) | Err(RendezvousError::Identity) => {}
                    Err(error) => panic!("unexpected cancel refusal: {error:?}"),
                }
            }));
        }

        // B enqueues.
        {
            let state = Arc::clone(&state);
            let wake = Arc::clone(&wake);
            let committed = Arc::clone(&committed);
            let committed_len = Arc::clone(&committed_len);
            handles.push(loom::thread::spawn(move || {
                let mut s = state.lock().unwrap();
                let id = s.enqueue_serial(key(1), generation()).unwrap();
                committed.lock().unwrap().push((1, id));
                committed_len.fetch_add(1, Ordering::SeqCst);
                wake.notify_one();
            }));
        }

        let members = running(&[key(0), key(1)]);
        let mut granted: Vec<RequestId> = Vec::new();
        loop {
            let mut s = state.lock().unwrap();
            // The keeper drains only after both requesters have committed; the
            // cancel-vs-grant race is still explored across schedules.
            while committed_len.load(Ordering::SeqCst) < 2 {
                s = wake.wait(s).unwrap();
            }
            if s.ring_len() == 0 {
                break;
            }
            let epoch = traced_stop(&mut s, &members);
            if let Some(id) = s.grant_head_in_stop(epoch, |_| true).unwrap() {
                s.retire_serial(id).unwrap();
                granted.push(id);
            }
            finish_stop(&mut s, epoch, &members);
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let b_id = committed
            .lock()
            .unwrap()
            .iter()
            .find(|(owner, _)| *owner == 1)
            .map(|(_, id)| *id)
            .expect("B committed");
        assert_eq!(
            granted.iter().filter(|id| **id == b_id).count(),
            1,
            "the surviving requester is granted exactly once"
        );
        for (index, id) in granted.iter().enumerate() {
            assert!(!granted[..index].contains(id), "no id is granted twice");
        }
    });
}

/// (c) two grant calls race for the head inside one traced stop: exactly one
/// grant (one turn out at a time). The Idle/DrainingResumes grant this row
/// used to race was deleted at L6.7b b2.
#[test]
fn grant_is_single_inside_one_stop() {
    loom::model(|| {
        let members = running(&[key(0), key(1)]);
        let state = Arc::new(Mutex::new(State::new()));
        let epoch = {
            let mut s = state.lock().unwrap();
            s.enqueue_serial(key(0), generation()).unwrap();
            traced_stop(&mut s, &members)
        };
        let results = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let state = Arc::clone(&state);
            let results = Arc::clone(&results);
            handles.push(loom::thread::spawn(move || {
                let mut s = state.lock().unwrap();
                let granted = s.grant_head_in_stop(epoch, |_| true).unwrap().is_some();
                results.lock().unwrap().push(granted);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let results = results.lock().unwrap();
        assert_eq!(
            results.iter().filter(|granted| **granted).count(),
            1,
            "the head is granted exactly once"
        );
    });
}

#[test]
fn generated_control_keeps_the_ring_api() {
    let mut state = State::new();
    let id = state.enqueue_serial(key(0), generation()).unwrap();
    assert_eq!(state.ring_head(), Some(id));
    assert_eq!(state.ring_len(), 1);
}
