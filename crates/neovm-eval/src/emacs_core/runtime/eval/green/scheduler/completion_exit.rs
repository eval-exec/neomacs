//! Exit publication after the native completion registration was retired.

use super::{
    Control, EraState, GroupTurn, NextOwner, OwnerLocation, PickNext, ThreadKey, TurnRight, Wait,
};

impl Control {
    /// ME ended: hand the Baton to the next thread, wake its joiners, and
    /// return without waiting. With no thread able to run, the runtime is
    /// closing: every waiting thread is woken (it signals) and gets the
    /// Baton in turn.
    #[doc(hidden)]
    pub fn finish(
        &self,
        completion: crate::emacs_core::threads::RetiredCompletion,
        baton: TurnRight,
    ) {
        let thread = completion.into_thread_id();
        if !baton.held_here() || thread.heap_identity() != baton.parallel_heap_identity() {
            std::process::abort();
        }
        let me = ThreadKey::from_id(thread.native_key());
        self.publish_owner_exit(me);
        // A finished thread must stop charging the scheduler (P11 soak
        // finding #2): the joiners and the native main already observed the
        // exit publication above, and the record goes before the Baton can
        // move, so the next owner never selects or scans a dead entry.
        {
            let mut state = self.state();
            Self::remove_key(&mut state, me);
        }
        if baton.is_era() {
            self.finish_era(baton);
            return;
        }
        let next = match self.pick_next() {
            PickNext::Owner(next) => Some(next),
            PickNext::NoRunnable(_) => {
                let mut state = self.state();
                state.closing = true;
                let blocked: Vec<ThreadKey> = state
                    .waits
                    .iter()
                    .filter(|(_, wait)| !matches!(wait, Wait::Exited))
                    .map(|(key, _)| *key)
                    .collect();
                for key in blocked {
                    Self::wake_locked(&mut state, key);
                }
                Self::pick(&mut state).map(|(next, _)| NextOwner::Resume(next))
            }
        };
        match next {
            Some(next) => {
                let (target, wake) = Self::wake_for(next, baton);
                self.send_wake(target, wake);
            }
            // Nothing else lives: the Baton and facade epoch end together.
            None => drop(baton),
        }
    }

    /// Era-mode exit handoff: honor Case (ii) (`ring_waiter()` only when an
    /// eligible Baton owner is ready, avoiding spurious empty round-trips —
    /// MEDIUM-2) and Case (i) (end interval before blocking, respecting
    /// `main_excluded` and running QD2 deadlock detection so surviving blocked
    /// Baton threads wake cleanly and an already-parked carrier slot never
    /// wedges — HIGH-2).
    pub(super) fn finish_era(&self, mut baton: TurnRight) {
        const DEADLOCK_POLL: std::time::Duration = std::time::Duration::from_millis(5);
        loop {
            enum EraFinishStep {
                CaseIi {
                    baton: TurnRight,
                    requester: super::GroupRequestSender,
                    port: Option<super::GroupPort>,
                },
                Dispatch {
                    baton: TurnRight,
                    next: NextOwner,
                },
                EndToIdle,
            }
            let step = {
                let mut state = self.state();
                let eligible_now = self.has_eligible_owner_now(&mut state);
                if self.era_ring_waiter(&state) {
                    if eligible_now {
                        let EraState::EraOpen(era) = &mut state.era else {
                            std::process::abort();
                        };
                        let Some(requester) = era.requester.clone() else {
                            std::process::abort();
                        };
                        let port = era.port.take();
                        state.set_era_turn(GroupTurn::Requested);
                        EraFinishStep::CaseIi {
                            baton,
                            requester,
                            port,
                        }
                    } else {
                        Self::release_group_turn_to_idle(&mut state, baton);
                        EraFinishStep::EndToIdle
                    }
                } else if let Some(turn) = Self::pick_service(&mut state) {
                    EraFinishStep::Dispatch {
                        baton,
                        next: NextOwner::Native(turn),
                    }
                } else {
                    match Self::pick(&mut state) {
                        Some((key, None)) => EraFinishStep::Dispatch {
                            baton,
                            next: NextOwner::Resume(key),
                        },
                        Some((_, Some(_))) | None => {
                            Self::release_group_turn_to_idle(&mut state, baton);
                            EraFinishStep::EndToIdle
                        }
                    }
                }
            };
            #[cfg(test)]
            self.observe_finish_era_step_for_test();
            match step {
                EraFinishStep::CaseIi {
                    baton: returning,
                    requester,
                    port,
                } => {
                    Self::give_back_era_turn(returning);
                    if requester.request_tail().is_err() {
                        return;
                    }
                    let Some(port) = port else {
                        return;
                    };
                    baton = match self.await_era_world_with_port(port) {
                        Ok(right) => right,
                        Err(_) => return,
                    };
                    continue;
                }
                EraFinishStep::Dispatch { baton, next } => {
                    let (target, wake) = Self::wake_for(next, baton);
                    self.send_wake(target, wake);
                    return;
                }
                EraFinishStep::EndToIdle => {
                    // Case (i): `release_group_turn_to_idle` already sent
                    // `WorldReturn` before storing `GroupTurn::Idle` under
                    // `Control.state`.
                }
            }

            loop {
                enum IdleAction {
                    Done,
                    AwaitWorld(super::GroupPort),
                    WaitDeadline(std::time::Instant),
                    CheckDeadlock,
                }
                let action = {
                    let mut state = self.state();
                    if state.baton_participants_empty() {
                        IdleAction::Done
                    } else {
                        let turn = match &state.era {
                            EraState::EraOpen(era) => era.turn,
                            _ => return,
                        };
                        match turn {
                            GroupTurn::Holding | GroupTurn::Collecting => IdleAction::Done,
                            GroupTurn::Requested => {
                                match Self::take_era_port_if_requested(&mut state) {
                                    Some(port) => IdleAction::AwaitWorld(port),
                                    None => IdleAction::Done,
                                }
                            }
                            GroupTurn::Idle => {
                                let eligible_now = self.has_eligible_owner_now(&mut state);
                                let now_requested = matches!(
                                    &state.era,
                                    EraState::EraOpen(era) if era.turn == GroupTurn::Requested
                                );
                                if now_requested {
                                    match Self::take_era_port_if_requested(&mut state) {
                                        Some(port) => IdleAction::AwaitWorld(port),
                                        None => IdleAction::Done,
                                    }
                                } else if eligible_now {
                                    Self::coalesce_request_locked(&mut state);
                                    match Self::take_era_port_if_requested(&mut state) {
                                        Some(port) => IdleAction::AwaitWorld(port),
                                        None => IdleAction::Done,
                                    }
                                } else if state.physical.get(&ThreadKey::Main)
                                    == Some(&OwnerLocation::NativeReleased)
                                {
                                    // Main's native wait owns reacquisition.
                                    IdleAction::Done
                                } else {
                                    let main_excluded = Self::main_excluded(&state);
                                    let next_deadline = state
                                        .waits
                                        .select_index
                                        .keys()
                                        .find(|d| !(d.key == ThreadKey::Main && main_excluded))
                                        .map(|d| d.wait_end);
                                    match next_deadline {
                                        Some(deadline) => IdleAction::WaitDeadline(deadline),
                                        None => IdleAction::CheckDeadlock,
                                    }
                                }
                            }
                        }
                    }
                };
                match action {
                    IdleAction::Done => return,
                    IdleAction::AwaitWorld(port) => {
                        baton = match self.await_era_world_with_port(port) {
                            Ok(right) => right,
                            Err(_) => return,
                        };
                        break;
                    }
                    IdleAction::WaitDeadline(deadline) => {
                        let state = self.state();
                        if state.baton_participants_empty()
                            || !matches!(
                                &state.era,
                                EraState::EraOpen(era) if era.turn == GroupTurn::Idle
                            )
                        {
                            continue;
                        }
                        drop(
                            self.metadata_changed
                                .wait_timeout(
                                    state,
                                    deadline.saturating_duration_since(std::time::Instant::now()),
                                )
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                    }
                    IdleAction::CheckDeadlock => {
                        // D8 (Q2a): BEFORE check under lock -> IdleQuery
                        // outside lock -> AFTER check under lock verifying
                        // unchanged epoch, all Baton and Parallel participants
                        // in deadlock waits, and all enrolled owners accounted
                        // for. Wake surviving blocked threads and hand off a
                        // fresh WorldTurn so the carrier never wedges.
                        if let Some((epoch_before, enrolled_owners)) =
                            self.query_era_idle_for_deadlock()
                        {
                            let port = {
                                let mut state = self.state();
                                if Self::confirm_era_deadlock_after(
                                    &state,
                                    epoch_before,
                                    enrolled_owners,
                                ) && !state.baton_participants_empty()
                                {
                                    state.closing = true;
                                    let blocked: Vec<ThreadKey> = state
                                        .waits
                                        .iter()
                                        .filter(|(_, wait)| !matches!(wait, Wait::Exited))
                                        .map(|(key, _)| *key)
                                        .collect();
                                    for key in blocked {
                                        Self::wake_locked(&mut state, key);
                                    }
                                    Self::coalesce_request_locked(&mut state);
                                    Self::take_era_port_if_requested(&mut state)
                                } else {
                                    None
                                }
                            };
                            if let Some(port) = port {
                                baton = match self.await_era_world_with_port(port) {
                                    Ok(right) => right,
                                    Err(_) => return,
                                };
                                break;
                            }
                            continue;
                        }
                        let state = self.state();
                        if state.baton_participants_empty()
                            || !matches!(
                                &state.era,
                                EraState::EraOpen(era) if era.turn == GroupTurn::Idle
                            )
                        {
                            continue;
                        }
                        drop(
                            self.metadata_changed
                                .wait_timeout(state, DEADLOCK_POLL)
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                    }
                }
            }
        }
    }
}
