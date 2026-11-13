#![forbid(unsafe_code)]

use super::*;

fn keeper_case(body: impl FnOnce() + Send + 'static) {
    let (done, result) = mpsc::channel();
    let actor = thread::spawn(move || {
        body();
        done.send(()).unwrap();
    });
    result
        .recv_timeout(Duration::from_secs(30))
        .expect("keeper operation must refuse instead of waiting for itself");
    actor.join().unwrap();
}

#[test]
fn keeper_running_owner_stop_refuses_and_can_ack_afterward() {
    keeper_case(|| {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let heap = era.heap_identity();
            let mut owner = activate(era.register(801, 1).unwrap(), heap, 801);
            assert!(matches!(era.stop(), Err(RendezvousError::Busy)));
            let epoch = owner.requested_epoch().unwrap().unwrap();
            owner.acknowledge(epoch, &roots[..1]).unwrap();
            let mut stopped = era.try_stop().unwrap().unwrap();
            trace_owners(&mut stopped, &[owner.key()]);
            complete(stopped);
            apply(owner.begin_restore(false).unwrap());
            retire(owner, &roots[..1]);
            finish_era(era);
        });
    });
}

#[test]
fn keeper_native_restore_refuses_active_stop_and_keeps_image() {
    keeper_case(|| {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let heap = era.heap_identity();
            let mut owner = activate(era.register(802, 1).unwrap(), heap, 802);
            owner.enter_native(&roots[..1]).unwrap();
            let epoch = active(era.rendezvous().request().unwrap());
            assert!(matches!(
                owner.begin_restore(true),
                Err(RendezvousError::Busy)
            ));
            let mut stopped = era.stop().unwrap();
            assert_eq!(stopped.epoch(), epoch);
            assert!(matches!(
                owner.begin_restore(true),
                Err(RendezvousError::Busy)
            ));
            trace_owners(&mut stopped, &[owner.key()]);
            complete(stopped);
            assert_eq!(owner.leave_native().unwrap(), epoch.get());
            retire(owner, &roots[..1]);
            finish_era(era);
        });
    });
}

#[test]
fn keeper_starting_owner_waits_refuse_until_bootstrap_opens() {
    keeper_case(|| {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let heap = era.heap_identity();
            active(era.rendezvous().request().unwrap());
            let mut owner = activate(era.register(803, 1).unwrap(), heap, 803);
            assert_eq!(owner.wait_started(), Err(RendezvousError::Busy));
            assert_eq!(owner.wait_request(), Err(RendezvousError::Busy));
            let mut stopped = era.stop().unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            complete(stopped);
            owner.wait_started().unwrap();
            retire(owner, &roots[..1]);
            finish_era(era);
        });
    });
}

#[test]
fn already_published_owner_wait_request_refuses() {
    keeper_case(|| {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let heap = era.heap_identity();
            let mut owner = activate(era.register(804, 1).unwrap(), heap, 804);
            owner.enter_native(&roots[..1]).unwrap();
            assert_eq!(owner.wait_request(), Err(RendezvousError::Busy));
            owner.leave_native().unwrap();
            let epoch = active(era.rendezvous().request().unwrap());
            owner.acknowledge(epoch, &roots[..1]).unwrap();
            assert_eq!(owner.wait_request(), Err(RendezvousError::Busy));
            let mut stopped = era.stop().unwrap();
            trace_owners(&mut stopped, &[owner.key()]);
            complete(stopped);
            apply(owner.begin_restore(false).unwrap());
            retire(owner, &roots[..1]);
            finish_era(era);
        });
    });
}

#[test]
fn keeper_resume_debt_refuses_next_stop_until_owner_applies() {
    keeper_case(|| {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let heap = era.heap_identity();
            let mut owner = activate(era.register(805, 1).unwrap(), heap, 805);
            let first = active(era.rendezvous().request().unwrap());
            owner.acknowledge(first, &roots[..1]).unwrap();
            let mut stopped = era.stop().unwrap();
            trace_owners(&mut stopped, &[owner.key()]);
            complete(stopped);
            assert!(matches!(era.stop(), Err(RendezvousError::Busy)));
            let debt = era.rendezvous().snapshot().unwrap();
            assert_eq!(debt.phase, Phase::DrainingResumes);
            assert_eq!(debt.debt, vec![owner.key()]);
            assert!(debt.queued);
            apply(owner.begin_restore(false).unwrap());
            let second = active(era.rendezvous().request().unwrap());
            assert_eq!(second.get(), first.get() + 1);
            owner.acknowledge(second, &roots[..1]).unwrap();
            let mut stopped = era.stop().unwrap();
            trace_owners(&mut stopped, &[owner.key()]);
            complete(stopped);
            apply(owner.begin_restore(false).unwrap());
            retire(owner, &roots[..1]);
            finish_era(era);
        });
    });
}
