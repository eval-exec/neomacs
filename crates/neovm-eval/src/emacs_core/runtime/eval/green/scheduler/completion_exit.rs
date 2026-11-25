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

    /// Era-mode exit handoff: honor Case (ii) (`ring_waiter()`) and Case (i)
    /// (end interval before blocking) so an exiting worker never blocks holding
    /// `WorldTurn` or prematurely closes the runtime while a parallel owner is
    /// live.
    fn finish_era(&self, mut baton: TurnRight) {
        loop {
            let case_ii = {
                let mut state = self.state();
                if self.era_ring_waiter(&state) {
                    let any_live = !state.senders.is_empty();
                    let EraState::EraOpen(era) = &mut state.era else {
                        std::process::abort();
                    };
                    let Some(requester) = era.requester.clone() else {
                        std::process::abort();
                    };
                    if any_live {
                        era.turn = GroupTurn::Requested;
                        Some((Some(requester), era.port.take()))
                    } else {
                        era.turn = GroupTurn::Idle;
                        Some((None, None))
                    }
                } else {
                    None
                }
            };
            if let Some((requester, port)) = case_ii {
                Self::give_back_era_turn(baton);
                let Some(requester) = requester else {
                    return;
                };
                if requester.request_tail().is_err() {
                    std::process::abort();
                }
                let Some(port) = port else {
                    return;
                };
                baton = match self.await_era_world_with_port(port) {
                    Ok(right) => right,
                    Err(_) => std::process::abort(),
                };
            }

            let pick = {
                let mut state = self.state();
                if let Some(turn) = Self::pick_service(&mut state) {
                    Some(NextOwner::Native(turn))
                } else {
                    match Self::pick(&mut state) {
                        Some((key, None)) => Some(NextOwner::Resume(key)),
                        Some((_, Some(_))) | None => {
                            let EraState::EraOpen(era) = &mut state.era else {
                                std::process::abort();
                            };
                            era.turn = GroupTurn::Idle;
                            None
                        }
                    }
                }
            };
            if let Some(next) = pick {
                let (target, wake) = Self::wake_for(next, baton);
                self.send_wake(target, wake);
                return;
            }
            // Case (i): no Baton thread runnable right now; end the interval
            // and wait until a surviving Baton thread becomes runnable (or
            // main reacquires on its own native-wait path).
            Self::give_back_era_turn(baton);
            loop {
                let action = {
                    let mut state = self.state();
                    if state.senders.is_empty() {
                        return;
                    }
                    let turn = match &state.era {
                        EraState::EraOpen(era) => era.turn,
                        _ => return,
                    };
                    match turn {
                        GroupTurn::Holding | GroupTurn::Collecting => return,
                        GroupTurn::Requested => {
                            if let Some(port) = Self::take_era_port_if_requested(&mut state) {
                                Some(port)
                            } else {
                                return;
                            }
                        }
                        GroupTurn::Idle => {
                            Self::expire_deadlines(&mut state, std::time::Instant::now());
                            if state.waits.runnable_count() > 0
                                || Self::queued_native_work(&state) == super::NativeWork::Ready
                            {
                                Self::coalesce_request_locked(&mut state);
                                if let Some(port) = Self::take_era_port_if_requested(&mut state) {
                                    Some(port)
                                } else {
                                    return;
                                }
                            } else if state.physical.get(&ThreadKey::Main)
                                == Some(&OwnerLocation::NativeReleased)
                            {
                                // Main's native wait owns reacquisition.
                                return;
                            } else {
                                None
                            }
                        }
                    }
                };
                if let Some(port) = action {
                    baton = match self.await_era_world_with_port(port) {
                        Ok(right) => right,
                        Err(_) => std::process::abort(),
                    };
                    break;
                }
                let state = self.state();
                if state.senders.is_empty()
                    || !matches!(&state.era, EraState::EraOpen(era) if era.turn == GroupTurn::Idle)
                {
                    continue;
                }
                drop(
                    self.metadata_changed
                        .wait_timeout(state, std::time::Duration::from_millis(5))
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                );
            }
        }
    }
}
