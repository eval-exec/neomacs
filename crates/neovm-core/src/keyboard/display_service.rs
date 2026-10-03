//! Bounded GUI service opportunities between completed commands.
use super::*;

impl crate::emacs_core::Context {
    pub(crate) fn service_gui_command_boundary(
        &mut self,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        self.service_gui_command_boundary_at(Instant::now())
    }

    fn service_gui_command_boundary_at(
        &mut self,
        now: Instant,
    ) -> Result<(), crate::emacs_core::error::Flow> {
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
        self.service_input_pending_with_timers()?;
        if let Some(mut maintenance) = self.display_idle_maintenance_fn.take() {
            let (_, publish) = maintenance(self);
            self.display_idle_maintenance_fn = Some(maintenance);
            if publish {
                self.invalidate_redisplay();
            }
        }
        self.redisplay_for_input_wait()?;
        // Expensive layout must not make the next deadline overdue before
        // this service call even returns. Reserve at least an equal amount
        // of evaluator time for queued commands; cheap frames retain the
        // normal refresh cadence. Native input remains ordered and intact.
        self.command_loop.gui_display_deadline =
            Some(now + period.max(service_started.elapsed().saturating_mul(2)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn gui_service_keeps_deadline_and_pending_input_order() {
        let mut eval = crate::emacs_core::Context::new();
        eval.eval_str("(setq inhibit-redisplay nil)").unwrap();
        let captures = Rc::new(Cell::new(0));
        let observed = captures.clone();
        eval.display_idle_maintenance_fn = Some(Box::new(move |_| {
            observed.set(observed.get() + 1);
            (None, true)
        }));
        let paints = Rc::new(Cell::new(0));
        let observed = paints.clone();
        eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));
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
        assert_eq!(paints.get(), 1);
        eval.service_gui_command_boundary_at(now + Duration::from_millis(21))
            .unwrap();
        assert_eq!(captures.get(), 1);
        assert_eq!(
            eval.command_loop.read_key_event(),
            Some(Value::fixnum('a' as i64))
        );
        assert_eq!(
            eval.command_loop.read_key_event(),
            Some(Value::fixnum('b' as i64))
        );
        eval.service_gui_command_boundary_at(now + Duration::from_millis(50))
            .unwrap();
        assert_eq!(captures.get(), 1, "idle wait owns idle maintenance");
        assert!(eval.command_loop.gui_display_deadline.is_none());
    }
}
