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
#[path = "tests/display_service_test.rs"]
mod tests;
