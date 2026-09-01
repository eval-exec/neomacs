//! `sleep-for` — the only timer-adjacent C builtin neomacs keeps.
//!
//! GNU implements the whole timer system in Lisp (`timer.el`): `run-at-time`,
//! `run-with-timer`, `run-with-idle-timer`, `timer-activate`, `cancel-timer`,
//! and the retrigger/`timer-max-repeats` logic all live there, storing timer
//! vectors in the `timer-list` / `timer-idle-list` variables. The C side only
//! *reads* those lists: `timer_check` (keyboard.c) fires due timers via
//! `timer-event-handler`, and `wait_reading_process_output` folds the next
//! timer deadline into its pselect timeout. neomacs mirrors that split — the
//! Lisp-visible timer surface comes from loading GNU's timer.el, the wait loop
//! reads stable copies of `timer-list` (`capture_gnu_timer_batch` /
//! `next_ordinary_gnu_timer_timeout`), and no native timer
//! store exists. A previous native `TimerManager` "second brain" (with its own
//! unregistered `run-at-time`/`timer-activate` builtins and a divergent
//! `now + interval` rescheduling rule) had no live writers and was removed.
//!
//! `sleep-for` stays native because GNU's is C (`Fsleep_for`, dispnew.c): it
//! parses SECONDS/MILLISECONDS and enters `wait_reading_process_output`.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_min_args;
use std::time::Duration;

use super::error::{EvalResult, Flow, signal};
use super::eval::GnuTimerTimestamp;
use super::intern::intern;
use super::value::{Value, ValueKind, VecLikeType};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::rounding_modes::RoundingMode;

#[derive(Clone, Copy, Debug)]
struct PendingGnuTimer {
    when: GnuTimerTimestamp,
}

fn pending_gnu_timer(timer: Value) -> Option<PendingGnuTimer> {
    let slots = timer.as_vector_data()?.clone();
    if slots.len() != 10 || !slots[0].is_nil() || !slots[7].is_nil() {
        return None;
    }

    Some(PendingGnuTimer {
        when: GnuTimerTimestamp {
            high_seconds: slots[1].as_int()?,
            low_seconds: slots[2].as_int()?,
            usecs: slots[3].as_int()?,
            psecs: slots.get(8).and_then(|value| value.as_int()).unwrap_or(0),
        },
    })
}

fn pending_gnu_idle_timer(timer: Value) -> Option<PendingGnuTimer> {
    let slots = timer.as_vector_data()?.clone();
    if slots.len() != 10 || !slots[0].is_nil() || slots[7].is_nil() {
        return None;
    }

    Some(PendingGnuTimer {
        when: GnuTimerTimestamp {
            high_seconds: slots[1].as_int()?,
            low_seconds: slots[2].as_int()?,
            usecs: slots[3].as_int()?,
            psecs: slots.get(8).and_then(|value| value.as_int()).unwrap_or(0),
        },
    })
}

/// Stable copy of the GNU timer lists for one `timer_check`-shaped pass.
///
/// GNU copies `timer-list` and `timer-idle-list` before invoking callbacks so
/// a callback may schedule an already-ripe timer without adding it to the pass
/// currently being serviced.
struct GnuTimerBatch {
    ordinary: Vec<Value>,
    idle: Vec<Value>,
}

impl GnuTimerBatch {
    fn timers(&self) -> impl Iterator<Item = Value> + '_ {
        self.ordinary.iter().chain(&self.idle).copied()
    }

    fn next_due(
        &mut self,
        ordinary_now: GnuTimerTimestamp,
        idle_now: Option<GnuTimerTimestamp>,
    ) -> Option<Value> {
        let ordinary = self
            .ordinary
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, timer)| pending_gnu_timer(timer).map(|timer| (index, timer)))
            .next()
            .filter(|(_, timer)| timer.when <= ordinary_now);
        let idle = idle_now.and_then(|idle_now| {
            self.idle
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, timer)| {
                    pending_gnu_idle_timer(timer).map(|timer| (index, timer, idle_now))
                })
                .next()
                .filter(|(_, timer, _)| timer.when <= idle_now)
        });

        match (ordinary, idle) {
            (Some((ordinary_index, ordinary)), Some((idle_index, idle, idle_now))) => {
                let ordinary_overdue = ordinary.when.overdue_duration(ordinary_now);
                let idle_overdue = idle.when.overdue_duration(idle_now);
                if ordinary_overdue > idle_overdue {
                    Some(self.ordinary.remove(ordinary_index))
                } else {
                    Some(self.idle.remove(idle_index))
                }
            }
            (Some((index, _)), None) => Some(self.ordinary.remove(index)),
            (None, Some((index, _, _))) => Some(self.idle.remove(index)),
            (None, None) => None,
        }
    }
}

impl super::eval::Context {
    fn capture_gnu_timer_batch(&self) -> GnuTimerBatch {
        let ordinary = self
            .obarray
            .symbol_value("timer-list")
            .and_then(super::value::list_to_vec)
            .unwrap_or_default();

        let idle = if self.current_idle_duration().is_some() {
            self.obarray
                .symbol_value("timer-idle-list")
                .and_then(super::value::list_to_vec)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        GnuTimerBatch { ordinary, idle }
    }

    fn current_idle_timer_timestamp(&self) -> Option<GnuTimerTimestamp> {
        self.current_idle_duration()
            .map(GnuTimerTimestamp::from_duration)
    }

    #[allow(dead_code)] // test/debug entry point around the shared timeout logic
    pub(crate) fn next_ordinary_gnu_timer_timeout(&self) -> Option<Duration> {
        self.next_ordinary_gnu_timer_timeout_before(None)
    }

    pub(crate) fn next_ordinary_gnu_timer_timeout_before(
        &self,
        deadline: Option<GnuTimerTimestamp>,
    ) -> Option<Duration> {
        let timers = self
            .obarray
            .symbol_value("timer-list")
            .and_then(super::value::list_to_vec)
            .unwrap_or_default();
        let now = GnuTimerTimestamp::now();

        timers
            .into_iter()
            .filter_map(pending_gnu_timer)
            .next()
            .filter(|timer| deadline.is_none_or(|deadline| timer.when < deadline))
            .map(|timer| timer.when.duration_until(now))
    }

    pub(crate) fn next_idle_gnu_timer_timeout(&self) -> Option<Duration> {
        let idle_now = self.current_idle_timer_timestamp()?;
        self.obarray
            .symbol_value("timer-idle-list")
            .and_then(super::value::list_to_vec)
            .unwrap_or_default()
            .into_iter()
            .filter_map(pending_gnu_idle_timer)
            .next()
            .map(|timer| timer.when.duration_until(idle_now))
    }

    #[allow(dead_code)] // test/debug entry point around the shared timeout logic
    pub(crate) fn next_input_wait_timeout(&self) -> Option<Duration> {
        let mut timeout: Option<Duration> = None;

        if let Some(ordinary) = self.next_ordinary_gnu_timer_timeout() {
            timeout = Some(timeout.map_or(ordinary, |current| current.min(ordinary)));
        }
        if let Some(idle) = self.next_idle_gnu_timer_timeout() {
            timeout = Some(timeout.map_or(idle, |current| current.min(idle)));
        }
        if !self.processes.live_process_ids().is_empty() {
            let process_poll = Duration::from_millis(100);
            timeout = Some(timeout.map_or(process_poll, |current| current.min(process_poll)));
        }

        timeout
    }

    #[allow(dead_code)] // test entry point
    pub(crate) fn fire_pending_timers(&mut self) {
        let _ = self.service_timers_with_redisplay();
    }

    pub(crate) fn service_pending_timers_with_wait_policy(
        &mut self,
        redisplay: bool,
    ) -> Result<bool, Flow> {
        self.flush_pending_safe_funcalls();
        let mut fired_any = false;
        let mut batch = self.capture_gnu_timer_batch();
        let batch_roots = self.save_specpdl_roots();
        for timer in batch.timers() {
            self.push_specpdl_root(timer);
        }

        // GNU `timer_check` services a copy of the timer lists. Non-local
        // throws still propagate out of the callback to their owning catch,
        // but callback-created timers remain outside this stable batch.
        let service_result = (|| {
            while let Some(timer) = batch.next_due(
                GnuTimerTimestamp::now(),
                self.current_idle_timer_timestamp(),
            ) {
                fired_any = true;
                if timer.is_vector() {
                    let _ = timer.set_vector_slot(0, Value::T);
                }
                self.run_timer_callback_preserving_state(
                    Value::symbol("timer-event-handler"),
                    vec![timer],
                )?;
            }
            Ok(())
        })();
        self.restore_specpdl_roots(batch_roots);
        service_result?;

        if fired_any && redisplay {
            self.redisplay();
        }

        Ok(fired_any)
    }

    fn run_timer_callback_preserving_state(
        &mut self,
        callback: Value,
        args: Vec<Value>,
    ) -> Result<(), Flow> {
        let saved_current_buffer = self.buffers.current_buffer_id();
        let saved_deactivate_mark = self.eval_symbol("deactivate-mark").unwrap_or(Value::NIL);
        let specpdl_count = self.specpdl.len();

        let gc_roots = self.save_specpdl_roots();
        self.push_specpdl_root(callback);
        for arg in &args {
            self.push_specpdl_root(*arg);
        }
        // saved_deactivate_mark is a heap Value held only in a Rust local
        // across the callback; a plain setq in the timer function unlinks
        // its previous root and a GC frees it before the restore below.
        // The GcRoot is popped by unbind_to together with the specbind.
        self.push_specpdl_root(saved_deactivate_mark);

        let result = (|| {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
            self.apply(callback, args)
        })();
        if let Some(buffer_id) = saved_current_buffer {
            self.restore_current_buffer_if_live(buffer_id);
        }
        // Restore before unbinding — the saved value loses its root when
        // unbind_to pops the GcRoot above (GNU restores it under the
        // still-bound inhibit-quit via its specpdl ordering too).
        self.assign("deactivate-mark", saved_deactivate_mark);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_specpdl_roots(gc_roots);

        self.finish_callback_flow(result, crate::emacs_core::process::AsyncCallbackKind::Timer)
    }
}

fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(value.xfloat()),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(f64::rounding_from(value.as_bignum().unwrap(), RoundingMode::Nearest).0)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        )),
    }
}

fn expect_fixnum_like(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *value],
        )),
    }
}

fn gnu_sleep_duration_from_secs(seconds: f64) -> Duration {
    let whole = seconds.trunc();
    let frac = seconds - whole;
    let mut secs = whole as u64;
    let mut nanos = (frac * 1_000_000_000.0).ceil() as u32;

    if nanos >= 1_000_000_000 {
        secs += u64::from(nanos / 1_000_000_000);
        nanos %= 1_000_000_000;
    }

    Duration::new(secs, nanos)
}

/// `(sleep-for SECONDS &optional MILLISECONDS)` — GNU `Fsleep_for`
/// (dispnew.c): pause, reading process output, without redisplay or servicing
/// command input.
pub(crate) fn builtin_sleep_for(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("sleep-for", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("sleep-for"), Value::fixnum(args.len() as i64)],
        ));
    }

    let secs = expect_number(&args[0])?;
    let millis = if args.len() > 1 {
        if args[1].is_nil() {
            0.0
        } else {
            // GNU Emacs requires a fixnum for the MILLISECONDS argument.
            expect_fixnum_like(&args[1])? as f64
        }
    } else {
        0.0
    };

    let total_secs = secs + millis / 1000.0;
    if total_secs > 0.0 {
        if eval.threads.current_thread_id() != 0 {
            return Err(Flow::thread_blocked(
                crate::emacs_core::threads::make_sleep_blocker(total_secs),
                Value::NIL,
            ));
        }
        let total = gnu_sleep_duration_from_secs(total_secs);
        let end_time = GnuTimerTimestamp::now().add_duration(total);

        loop {
            let now = GnuTimerTimestamp::now();
            if now >= end_time {
                break;
            }
            let remaining = end_time.duration_until(now);
            eval.wait_for_duration_until_timer_deadline(remaining, end_time)?;
        }
    }

    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
