//! Own every activation and published image through all cleanup paths.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) words: Vec<usize>,
    pub(super) cache: Option<u64>,
    pub(super) selected: Option<u64>,
    pub(super) facts: [(u64, u64); 2],
    pub(super) pc: usize,
    pub(super) points: usize,
}
#[derive(Debug)]
pub(super) struct Terminal {
    pub(super) snapshot: Snapshot,
    pub(super) execution: Result<(), ExecError>,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Command {
    Advance,
    Resume,
    Finish,
}
#[derive(Debug)]
pub(super) enum Event {
    Seeded,
    Paused(Snapshot),
    Resumed(Snapshot),
    Finished {
        result: Result<Terminal, String>,
        cleanup: Result<(), String>,
    },
}
pub(super) struct Actor {
    pub(super) commands: Sender<Command>,
    pub(super) events: Receiver<Event>,
}
pub(super) fn receive<T>(rx: &Receiver<T>) -> T {
    #[cfg(not(miri))]
    {
        rx.recv_timeout(CHANNEL_WAIT).expect("C4 handoff progress")
    }
    #[cfg(miri)]
    {
        rx.recv().expect("C4 handoff progress")
    }
}
fn snapshot<'era>(
    owner: &mut Mutator<'era>,
    session: &Session,
    frame: &Frame<'era>,
) -> Result<Snapshot, ExecError> {
    retry_exec(|| {
        with_run(owner, |run| {
            let mut facts = [(0, 0); 2];
            for (dst, key) in facts.iter_mut().zip(&session.handles) {
                let (epoch, stamp, _) = run.function_facts_for_test(key)?;
                *dst = (epoch, stamp);
            }
            Ok(Snapshot {
                words: frame
                    .slots
                    .iter()
                    .map(|slot| slot.word_for_test(run))
                    .collect(),
                cache: frame.cache.observed_epoch(),
                selected: frame.activation.selected_epoch(),
                facts,
                pc: frame.machine.pc,
                points: frame.machine.safepoints,
            })
        })
    })
}
pub(super) fn pause<'era>(
    session: &Session,
    owner: &mut Mutator<'era>,
    frame: &mut Frame<'era>,
) -> Result<(), ExecError> {
    loop {
        match session.wait_step(owner, frame)? {
            Step::Safepoint => return Ok(()),
            Step::Retry | Step::Yielded => thread::yield_now(),
            Step::Progress => {}
            Step::Done => return Err(ExecError::Type),
        }
    }
}
fn retire(owner: Mutator<'_>) -> Result<(), String> {
    retry_owned(
        owner,
        |owner| owner.retire_rooted(&[]),
        |failure| match contention::rendezvous(&failure.reason) {
            Retry::Busy => ControlFlow::Continue(failure.into_owner()),
            Retry::Refuse => ControlFlow::Break(failure),
        },
    )
    .map_err(|failure| format!("retire: {:?}", failure.reason))
}
pub(super) fn run<'era>(
    session: &Session,
    nil: &SharedRoot,
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    index: usize,
    commands: Receiver<Command>,
    events: Sender<Event>,
) {
    let _probes = a8_probes::suppress();
    let mut owner = activate_owner(permit, heap, OWNER_BASE + index as u64);
    let mut frame = None;
    let mut native = NativeState::Running;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Terminal, ExecError> {
            frame = Some(session.seed(&mut owner, index, nil)?);
            let frame = frame.as_mut().ok_or(ExecError::Slot)?;
            events.send(Event::Seeded).expect("keeper at seed");
            for phase in 0..PHASES {
                assert_eq!(receive(&commands), Command::Advance);
                pause(session, &mut owner, frame)?;
                if index == 0 && matches!(phase, 1..=3) {
                    // Trusted frame housekeeping, through the production API.
                    // The opaque activation home remains independently rooted.
                    with_run(&mut owner, |run| {
                        frame.cache.clear(run).map_err(ExecError::from)
                    })?;
                }
                let before = snapshot(&mut owner, session, frame)?;
                owner.with_native_extent(&[], |_| {
                    native = NativeState::Published;
                    events.send(Event::Paused(before)).expect("keeper at pause");
                    assert_eq!(receive(&commands), Command::Resume);
                })?;
                native = NativeState::Running;
                events
                    .send(Event::Resumed(snapshot(&mut owner, session, frame)?))
                    .expect("keeper at resume");
            }
            assert_eq!(receive(&commands), Command::Finish);
            let execution = loop {
                match session.wait_step(&mut owner, frame) {
                    Ok(Step::Done) => break Ok(()),
                    Ok(Step::Retry | Step::Yielded) => thread::yield_now(),
                    Ok(Step::Progress | Step::Safepoint) => {}
                    Err(error) => break Err(error),
                }
            };
            Ok(Terminal {
                snapshot: snapshot(&mut owner, session, frame)?,
                execution,
            })
        },
    ));
    let result = match outcome {
        Ok(result) => result.map_err(|error| format!("owner {index} infrastructure: {error:?}")),
        Err(payload) => Err(payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string assertion".into())),
    };
    drop(frame);
    let cleanup = (|| -> Result<(), String> {
        if matches!(native, NativeState::Published) {
            contention::retry(|| owner.leave_native(), contention::rendezvous)
                .map_err(|e| format!("restore: {e:?}"))?;
        }
        with_run(&mut owner, |run| {
            run.clear_root_slots()?;
            Ok(())
        })
        .map_err(|e| format!("clear: {e:?}"))?;
        retire(owner)
    })();
    let _ = events.send(Event::Finished { result, cleanup });
}
