//! Bounded GUI service opportunities between completed commands.
use super::*;
use crate::emacs_core::eval::GuiBoundaryServiceActivity;
#[cfg(feature = "test-support")]
use crate::emacs_core::eval::{DisplayMaintenanceOutcome, DisplayMaintenancePublication};
#[cfg(feature = "test-support")]
use std::{cell::Cell, rc::Rc};

impl crate::emacs_core::Context {
    #[doc(hidden)]
    pub fn service_gui_command_boundary(&mut self) -> Result<(), neovm_value::error::Flow> {
        self.service_gui_command_boundary_at(Instant::now())
    }

    #[doc(hidden)]
    pub fn service_gui_command_boundary_at(
        &mut self,
        now: Instant,
    ) -> Result<(), neovm_value::error::Flow> {
        // This callback is installed by the GUI frontend. Batch/terminal
        // command loops keep their existing input-priority behavior.
        let pending = self.command_loop.keyboard.has_pending_low_level_input()
            || self.has_pending_command_input_for_query()
            || self.input_rx.as_ref().is_some_and(|rx| !rx.is_empty());
        if self.display_idle_maintenance_fn.is_none() || !pending {
            self.command_loop.gui_display_deadline = None;
            return Ok(());
        }
        let period = Duration::from_nanos(1_000_000_000 / 60);
        let deadline = self
            .command_loop
            .gui_display_deadline
            .get_or_insert(now + period);
        if now < *deadline {
            return Ok(());
        }
        // Advance before Lisp callbacks: recursive command loops cannot
        // recursively service the same deadline. No input is merged/dropped.
        *deadline = now + period;
        let service_started = Instant::now();
        self.timer_stop_idle();
        let activity =
            crate::emacs_core::eval::hooks_proof::wait::service_input_pending_with_timers(self)?;
        if let Some(mut maintenance) = self.display_idle_maintenance_fn.take() {
            let _outcome = maintenance(self);
            self.display_idle_maintenance_fn = Some(maintenance);
        }
        // Background coverage can only publish natively. Real callbacks keep
        // GNU's display opportunity even if they changed no visible facts;
        // otherwise enter ordinary redisplay only for genuine existing debt.
        if activity == GuiBoundaryServiceActivity::CallbacksRan
            || self
                .auxiliary_redisplay_readiness(self.redisplay_generation())
                .is_err()
        {
            self.redisplay_for_input_wait()?;
        }
        // Expensive layout must not make the next deadline overdue before
        // this service call even returns. Reserve at least an equal amount
        // of evaluator time for queued commands; cheap frames retain the
        // normal refresh cadence. Native input remains ordered and intact.
        self.command_loop.gui_display_deadline =
            Some(now + period.max(service_started.elapsed().saturating_mul(2)));
        Ok(())
    }
    // Core supplies its canonical Context factory; private assertions stay with the owner.
    #[cfg(feature = "test-support")]
    pub(crate) fn run_gui_service_keeps_deadline_and_pending_input_order_fixture(
        make_context: fn() -> Self,
    ) {
        // This Context-owned fixture uses the legacy producer/signature
        // acceptance contract; the layout-engine fixture covers actual GNU
        // presentation acceptance. Neither may invent debt for native coverage.
        let _policy = super::super::RedisplayHookPolicyGuard::legacy();
        let mut eval = make_context();
        eval.eval_str(
            "(setq inhibit-redisplay nil
                   timer-list nil timer-idle-list nil
                   gui-service-pre-count 0 gui-service-timer-count 0
                   pre-redisplay-function
                   (lambda (_windows)
                     (setq gui-service-pre-count (1+ gui-service-pre-count))))
             (fset 'timer-event-handler
                   (lambda (_timer)
                     (setq timer-list nil
                           gui-service-timer-count (1+ gui-service-timer-count))))",
        )
        .unwrap();
        let captures = Rc::new(Cell::new(0));
        let observed = captures.clone();
        eval.display_idle_maintenance_fn = Some(Box::new(move |_| {
            observed.set(observed.get() + 1);
            DisplayMaintenanceOutcome {
                next_wake: None,
                publication: DisplayMaintenancePublication::PublishedNatively,
            }
        }));
        let paints = Rc::new(Cell::new(0));
        let observed = paints.clone();
        eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));
        eval.redisplay().expect("accept an ordinary baseline first");
        assert_eq!(paints.get(), 1);
        paints.set(0);
        eval.eval_str("(setq gui-service-pre-count 0)").unwrap();
        assert_eq!(
            eval.auxiliary_redisplay_readiness(eval.redisplay_generation()),
            Ok(())
        );
        for ch in ['a', 'b'] {
            eval.command_loop.unread_key(KeyEvent::char(ch));
        }
        let now = Instant::now();
        eval.service_gui_command_boundary_at(now).unwrap();
        eval.service_gui_command_boundary_at(now + Duration::from_millis(8))
            .unwrap();
        assert_eq!(captures.get(), 0);
        eval.service_gui_command_boundary_at(now + Duration::from_millis(20))
            .unwrap();
        assert_eq!(captures.get(), 1);
        // The previous paints==1 expectation encoded a logical redraw forced
        // by the publication boolean. Native publication owes no Lisp/producer.
        assert_eq!(paints.get(), 0);
        assert_eq!(
            eval.eval_symbol("gui-service-pre-count").unwrap(),
            Value::fixnum(0)
        );
        eval.service_gui_command_boundary_at(now + Duration::from_millis(21))
            .unwrap();
        assert_eq!(captures.get(), 1);
        // A callback with no display mutation still owns GNU's post-timer
        // pre-hook opportunity. The signature proof by itself cannot see it.
        eval.set_variable(
            "timer-list",
            Value::list(vec![Value::from(super::super::GnuTimerTimestamp::now())]),
        );
        assert_eq!(
            eval.auxiliary_redisplay_readiness(eval.redisplay_generation()),
            Ok(())
        );
        eval.service_gui_command_boundary_at(now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(captures.get(), 2);
        assert_eq!(
            paints.get(),
            0,
            "an unchanged timer does not force a producer"
        );
        assert_eq!(
            eval.eval_symbol("gui-service-pre-count").unwrap(),
            Value::fixnum(1)
        );
        assert_eq!(
            eval.eval_symbol("gui-service-timer-count").unwrap(),
            Value::fixnum(1)
        );
        // Genuine debt keeps its ordinary producer even when native coverage
        // was also published during this same command-boundary opportunity.
        eval.invalidate_redisplay();
        eval.service_gui_command_boundary_at(now + Duration::from_secs(2))
            .unwrap();
        assert_eq!(captures.get(), 3);
        assert_eq!(paints.get(), 1);
        assert_eq!(
            eval.eval_symbol("gui-service-pre-count").unwrap(),
            Value::fixnum(2)
        );
        assert_eq!(
            eval.command_loop.read_key_event(),
            Some(Value::fixnum('a' as i64))
        );
        assert_eq!(
            eval.command_loop.read_key_event(),
            Some(Value::fixnum('b' as i64))
        );
        eval.service_gui_command_boundary_at(now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(captures.get(), 3, "idle wait owns idle maintenance");
        assert!(eval.command_loop.gui_display_deadline.is_none());
    }
}

#[cfg(test)]
#[path = "tests/display_service_test.rs"]
mod tests;
