//! Timer system for the Elisp VM.
//!
//! Provides Emacs-compatible timer functionality:
//! - `run-at-time` / `run-with-timer` — schedule a callback after a delay
//! - `run-with-idle-timer` — schedule a callback during idle time
//! - `cancel-timer` — deactivate a timer
//! - `timerp` — type predicate
//! - `timer-activate` — reactivate a timer
//! - `sit-for` — sleep/yield (stub)

use std::time::{Duration, Instant};

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

// ---------------------------------------------------------------------------
// Timer types
// ---------------------------------------------------------------------------

/// Unique timer identifier.
pub type TimerId = u64;

/// A single timer entry.
#[derive(Clone, Debug)]
pub struct Timer {
    /// Unique identifier.
    pub id: TimerId,
    /// Absolute time when this timer should next fire.
    pub fire_time: Instant,
    /// If Some, the timer repeats at this interval after firing.
    pub repeat_interval: Option<Duration>,
    /// The callback to invoke (a lambda, symbol name, or other callable).
    pub callback: Value,
    /// Arguments to pass to the callback.
    pub args: Vec<Value>,
    /// Whether this timer is currently active.
    pub active: bool,
    /// Whether this is an idle timer.
    pub idle: bool,
}

// ---------------------------------------------------------------------------
// TimerManager
// ---------------------------------------------------------------------------

/// Central registry for all timers.
pub struct TimerManager {
    timers: Vec<Timer>,
    next_id: TimerId,
}

impl TimerManager {
    /// Create a new empty timer manager.
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a new timer that fires after `delay_secs` seconds.
    ///
    /// If `repeat_secs` is > 0, the timer repeats at that interval.
    /// Returns the timer id.
    pub fn add_timer(
        &mut self,
        delay_secs: f64,
        repeat_secs: f64,
        callback: Value,
        args: Vec<Value>,
        idle: bool,
    ) -> TimerId {
        let id = self.next_id;
        self.next_id += 1;

        let delay = Duration::from_secs_f64(delay_secs.max(0.0));
        let fire_time = Instant::now() + delay;
        let repeat_interval = if repeat_secs > 0.0 {
            Some(Duration::from_secs_f64(repeat_secs))
        } else {
            None
        };

        self.timers.push(Timer {
            id,
            fire_time,
            repeat_interval,
            callback,
            args,
            active: true,
            idle,
        });

        id
    }

    /// Cancel a timer by id. Returns true if the timer was found and cancelled.
    pub fn cancel_timer(&mut self, id: TimerId) -> bool {
        for timer in &mut self.timers {
            if timer.id == id {
                timer.active = false;
                return true;
            }
        }
        false
    }

    /// Check if a timer is active.
    pub fn timer_active_p(&self, id: TimerId) -> bool {
        self.timers.iter().any(|t| t.id == id && t.active)
    }

    /// Update a timer's delay (reschedules from now).
    pub fn timer_set_time(&mut self, id: TimerId, new_delay: f64) {
        let delay = Duration::from_secs_f64(new_delay.max(0.0));
        for timer in &mut self.timers {
            if timer.id == id {
                timer.fire_time = Instant::now() + delay;
                timer.active = true;
                return;
            }
        }
    }

    /// Reactivate a cancelled timer (reschedules from now using its repeat interval or zero).
    pub fn timer_activate(&mut self, id: TimerId) -> bool {
        for timer in &mut self.timers {
            if timer.id == id {
                if !timer.active {
                    timer.active = true;
                    // Reschedule from now using repeat interval or immediately.
                    let delay = timer.repeat_interval.unwrap_or(Duration::ZERO);
                    timer.fire_time = Instant::now() + delay;
                }
                return true;
            }
        }
        false
    }

    /// Collect all pending callbacks whose fire_time has passed.
    ///
    /// Returns a vec of (callback, args) pairs to be executed by the evaluator.
    /// Repeating timers are rescheduled; one-shot timers are deactivated.
    pub fn fire_pending_timers(&mut self, current_time: Instant) -> Vec<(Value, Vec<Value>)> {
        let mut fired = Vec::new();

        for timer in &mut self.timers {
            if !timer.active {
                continue;
            }
            if current_time >= timer.fire_time {
                fired.push((timer.callback.clone(), timer.args.clone()));

                if let Some(interval) = timer.repeat_interval {
                    // Reschedule: advance fire_time by interval (catch up if needed)
                    timer.fire_time = current_time + interval;
                } else {
                    timer.active = false;
                }
            }
        }

        fired
    }

    /// Return the duration until the next timer fires, or None if no active timers.
    pub fn next_fire_time(&self) -> Option<Duration> {
        let now = Instant::now();
        self.timers
            .iter()
            .filter(|t| t.active)
            .map(|t| {
                if t.fire_time > now {
                    t.fire_time - now
                } else {
                    Duration::ZERO
                }
            })
            .min()
    }

    /// Return a list of all timer ids (both active and inactive).
    pub fn list_timers(&self) -> Vec<TimerId> {
        self.timers.iter().map(|t| t.id).collect()
    }

    /// Return a list of active timer ids.
    pub fn list_active_timers(&self) -> Vec<TimerId> {
        self.timers
            .iter()
            .filter(|t| t.active)
            .map(|t| t.id)
            .collect()
    }

    /// Check if the given id refers to a known timer.
    pub fn is_timer(&self, id: TimerId) -> bool {
        self.timers.iter().any(|t| t.id == id)
    }
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Builtin helpers
// ===========================================================================

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::Char(c) => Ok(*c as u32 as f64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), other.clone()],
        )),
    }
}

fn parse_run_at_time_delay(value: &Value) -> Result<f64, Flow> {
    match value {
        Value::Nil => Ok(0.0),
        Value::Int(_) | Value::Float(_) | Value::Char(_) => expect_number(value),
        Value::Str(s) => {
            let spec = s.trim();
            if spec.is_empty() {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid time specification")],
                ));
            }

            if let Ok(delay) = spec.parse::<f64>() {
                return Ok(delay);
            }

            if let Some(token) = spec.split_whitespace().next() {
                if let Ok(delay) = token.parse::<f64>() {
                    return Ok(delay);
                }
            }

            Err(signal(
                "error",
                vec![Value::string("Invalid time specification")],
            ))
        }
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid time specification")],
        )),
    }
}

fn parse_idle_timer_delay(value: &Value) -> Result<f64, Flow> {
    match value {
        Value::Nil => Ok(0.0),
        Value::Int(_) | Value::Float(_) | Value::Char(_) => expect_number(value),
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid time specification")],
        )),
    }
}

fn expect_timer_id(value: &Value) -> Result<TimerId, Flow> {
    match value {
        Value::Timer(id) => Ok(*id),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("timerp"), other.clone()],
        )),
    }
}

// ===========================================================================
// Builtins (evaluator-dependent)
// ===========================================================================

/// (run-at-time TIME REPEAT FUNCTION &rest ARGS) -> timer
///
/// TIME is seconds from now (float or int). REPEAT is nil or seconds.
pub(crate) fn builtin_run_at_time(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-at-time", &args, 3)?;
    let delay = parse_run_at_time_delay(&args[0])?;
    let repeat = if args[1].is_nil() {
        0.0
    } else {
        expect_number(&args[1])?
    };
    let callback = args[2].clone();
    let timer_args: Vec<Value> = args[3..].to_vec();

    let id = eval
        .timers
        .add_timer(delay, repeat, callback, timer_args, false);
    Ok(Value::Timer(id))
}

/// (run-with-timer SECS REPEAT FUNCTION &rest ARGS) -> timer
///
/// Alias for run-at-time.
pub(crate) fn builtin_run_with_timer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    builtin_run_at_time(eval, args)
}

/// (run-with-idle-timer SECS REPEAT FUNCTION &rest ARGS) -> timer
///
/// Like run-at-time, but marks the timer as idle.
pub(crate) fn builtin_run_with_idle_timer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-with-idle-timer", &args, 3)?;
    let delay = parse_idle_timer_delay(&args[0])?;
    let repeat = if args[1].is_nil() {
        0.0
    } else {
        expect_number(&args[1])?
    };
    let callback = args[2].clone();
    let timer_args: Vec<Value> = args[3..].to_vec();

    let id = eval
        .timers
        .add_timer(delay, repeat, callback, timer_args, true);
    Ok(Value::Timer(id))
}

/// (cancel-timer TIMER) -> nil
pub(crate) fn builtin_cancel_timer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("cancel-timer", &args, 1)?;
    let id = expect_timer_id(&args[0])?;
    eval.timers.cancel_timer(id);
    Ok(Value::Nil)
}

/// (timerp OBJECT) -> t or nil
pub(crate) fn builtin_timerp(args: Vec<Value>) -> EvalResult {
    expect_args("timerp", &args, 1)?;
    Ok(Value::bool(matches!(args[0], Value::Timer(_))))
}

/// (timer-activate TIMER) -> nil
pub(crate) fn builtin_timer_activate(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("timer-activate", &args, 1)?;
    let id = match &args[0] {
        Value::Timer(id) => *id,
        _ => return Err(signal("error", vec![Value::string("Invalid timer")])),
    };
    if !eval.timers.is_timer(id) {
        return Err(signal("error", vec![Value::string("Invalid timer")]));
    }
    if eval.timers.timer_active_p(id) {
        return Err(signal("error", vec![Value::string("Timer is already active")]));
    }
    eval.timers.timer_activate(id);
    Ok(Value::Nil)
}

/// (sit-for SECONDS &optional NODISP) -> t
///
/// Stub implementation: just returns t.
pub(crate) fn builtin_sit_for(args: Vec<Value>) -> EvalResult {
    expect_min_args("sit-for", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("sit-for"), Value::Int(args.len() as i64)],
        ));
    }
    // Validate that the first arg is a number
    let _secs = expect_number(&args[0])?;
    // In a full implementation this would yield to the event loop.
    // For now, just return t.
    Ok(Value::True)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn timer_creation_and_list() {
        let mut mgr = TimerManager::new();
        let id1 = mgr.add_timer(1.0, 0.0, Value::symbol("my-callback"), vec![], false);
        let id2 = mgr.add_timer(
            2.0,
            0.0,
            Value::symbol("other-callback"),
            vec![Value::Int(42)],
            false,
        );

        assert_ne!(id1, id2);
        assert!(mgr.is_timer(id1));
        assert!(mgr.is_timer(id2));
        assert!(!mgr.is_timer(999));

        let all = mgr.list_timers();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&id1));
        assert!(all.contains(&id2));
    }

    #[test]
    fn timer_cancellation() {
        let mut mgr = TimerManager::new();
        let id = mgr.add_timer(1.0, 0.0, Value::symbol("cb"), vec![], false);

        assert!(mgr.timer_active_p(id));
        assert!(mgr.cancel_timer(id));
        assert!(!mgr.timer_active_p(id));

        // Cancelling again still returns true (timer exists, just already inactive)
        assert!(mgr.cancel_timer(id));

        // Cancelling non-existent timer returns false
        assert!(!mgr.cancel_timer(999));
    }

    #[test]
    fn fire_pending_timers_one_shot() {
        let mut mgr = TimerManager::new();
        // Create a timer with 0 delay (fires immediately)
        let id = mgr.add_timer(
            0.0,
            0.0,
            Value::symbol("immediate"),
            vec![Value::Int(1)],
            false,
        );

        // Fire it
        let now = Instant::now();
        let fired = mgr.fire_pending_timers(now);

        assert_eq!(fired.len(), 1);
        // Check callback is the symbol we set
        match &fired[0].0 {
            Value::Symbol(s) => assert_eq!(s, "immediate"),
            other => panic!("Expected Symbol, got {:?}", other),
        }
        assert_eq!(fired[0].1.len(), 1);

        // Timer should be inactive after one-shot fire
        assert!(!mgr.timer_active_p(id));

        // Fire again: nothing should fire
        let fired2 = mgr.fire_pending_timers(Instant::now());
        assert!(fired2.is_empty());
    }

    #[test]
    fn fire_pending_timers_repeat() {
        let mut mgr = TimerManager::new();
        // Create a repeating timer with 0 delay and 1-second repeat
        let id = mgr.add_timer(0.0, 1.0, Value::symbol("repeater"), vec![], false);

        // Fire it once
        let now = Instant::now();
        let fired = mgr.fire_pending_timers(now);
        assert_eq!(fired.len(), 1);

        // Timer should still be active (it repeats)
        assert!(mgr.timer_active_p(id));

        // Immediately firing again should NOT fire (needs 1 second)
        let fired2 = mgr.fire_pending_timers(Instant::now());
        assert!(fired2.is_empty());

        // Advance time by simulating future instant
        let future = Instant::now() + Duration::from_secs(2);
        let fired3 = mgr.fire_pending_timers(future);
        assert_eq!(fired3.len(), 1);
        assert!(mgr.timer_active_p(id));
    }

    #[test]
    fn timer_not_yet_due() {
        let mut mgr = TimerManager::new();
        // Timer fires in 10 seconds
        let id = mgr.add_timer(10.0, 0.0, Value::symbol("future"), vec![], false);

        let fired = mgr.fire_pending_timers(Instant::now());
        assert!(fired.is_empty());
        assert!(mgr.timer_active_p(id));
    }

    #[test]
    fn next_fire_time_works() {
        let mut mgr = TimerManager::new();

        // No timers => None
        assert!(mgr.next_fire_time().is_none());

        // Add a timer in the future
        let _id = mgr.add_timer(5.0, 0.0, Value::symbol("cb"), vec![], false);
        let next = mgr.next_fire_time();
        assert!(next.is_some());
        // Should be roughly 5 seconds (with some tolerance for test execution time)
        let dur = next.unwrap();
        assert!(dur.as_secs_f64() > 4.0);
        assert!(dur.as_secs_f64() < 6.0);
    }

    #[test]
    fn next_fire_time_overdue() {
        let mut mgr = TimerManager::new();
        // Timer with 0 delay => immediately overdue
        let _id = mgr.add_timer(0.0, 0.0, Value::symbol("cb"), vec![], false);
        let next = mgr.next_fire_time();
        assert!(next.is_some());
        assert!(next.unwrap() <= Duration::from_millis(10));
    }

    #[test]
    fn idle_timer_flag() {
        let mut mgr = TimerManager::new();
        let id = mgr.add_timer(1.0, 0.0, Value::symbol("idle-cb"), vec![], true);

        // The timer is stored with idle=true
        let timer = mgr.timers.iter().find(|t| t.id == id).unwrap();
        assert!(timer.idle);
    }

    #[test]
    fn timer_set_time_reschedules() {
        let mut mgr = TimerManager::new();
        let id = mgr.add_timer(100.0, 0.0, Value::symbol("cb"), vec![], false);

        // Originally 100 seconds away — won't fire now
        let fired = mgr.fire_pending_timers(Instant::now());
        assert!(fired.is_empty());

        // Reschedule to 0 seconds
        mgr.timer_set_time(id, 0.0);
        let fired = mgr.fire_pending_timers(Instant::now());
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn timer_activate_reactivates() {
        let mut mgr = TimerManager::new();
        let id = mgr.add_timer(0.0, 0.0, Value::symbol("cb"), vec![], false);

        // Fire and deactivate
        mgr.fire_pending_timers(Instant::now());
        assert!(!mgr.timer_active_p(id));

        // Reactivate
        assert!(mgr.timer_activate(id));
        assert!(mgr.timer_active_p(id));

        // Fire again
        let fired = mgr.fire_pending_timers(Instant::now());
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn timer_activate_nonexistent() {
        let mut mgr = TimerManager::new();
        assert!(!mgr.timer_activate(999));
    }

    #[test]
    fn list_active_timers() {
        let mut mgr = TimerManager::new();
        let id1 = mgr.add_timer(1.0, 0.0, Value::symbol("a"), vec![], false);
        let id2 = mgr.add_timer(2.0, 0.0, Value::symbol("b"), vec![], false);

        let active = mgr.list_active_timers();
        assert_eq!(active.len(), 2);

        mgr.cancel_timer(id1);
        let active = mgr.list_active_timers();
        assert_eq!(active.len(), 1);
        assert!(active.contains(&id2));
    }

    // -----------------------------------------------------------------------
    // Builtin-level tests (via Evaluator)
    // -----------------------------------------------------------------------

    #[test]
    fn test_builtin_timerp() {
        // Timer value
        let result = builtin_timerp(vec![Value::Timer(1)]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        // Non-timer value
        let result = builtin_timerp(vec![Value::Int(42)]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Nil
        let result = builtin_timerp(vec![Value::Nil]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_sit_for() {
        let result = builtin_sit_for(vec![Value::Float(0.1)]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        // Wrong type
        let result = builtin_sit_for(vec![Value::string("bad")]);
        assert!(result.is_err());

        // Wrong arity
        let result = builtin_sit_for(vec![Value::Int(0), Value::Nil, Value::Nil]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn test_eval_run_at_time_and_cancel() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // run-at-time with 0 delay
        let result = builtin_run_at_time(
            &mut eval,
            vec![
                Value::Float(0.0),
                Value::Nil,
                Value::symbol("my-func"),
                Value::Int(1),
                Value::Int(2),
            ],
        );
        assert!(result.is_ok());
        let timer_val = result.unwrap();
        assert!(matches!(timer_val, Value::Timer(_)));

        // cancel-timer
        let result = builtin_cancel_timer(&mut eval, vec![timer_val.clone()]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Verify it's cancelled
        if let Value::Timer(id) = timer_val {
            assert!(!eval.timers.timer_active_p(id));
        }
    }

    #[test]
    fn test_eval_run_with_idle_timer() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let result = builtin_run_with_idle_timer(
            &mut eval,
            vec![Value::Int(5), Value::Nil, Value::symbol("idle-func")],
        );
        assert!(result.is_ok());
        let timer_val = result.unwrap();

        // Should be a timer
        assert!(matches!(timer_val, Value::Timer(_)));

        // The timer should be idle
        if let Value::Timer(id) = timer_val {
            let timer = eval.timers.timers.iter().find(|t| t.id == id).unwrap();
            assert!(timer.idle);
        }
    }

    #[test]
    fn test_eval_run_at_time_accepts_nil_and_string_specs() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let from_nil = builtin_run_at_time(
            &mut eval,
            vec![Value::Nil, Value::Nil, Value::symbol("cb-from-nil")],
        )
        .expect("nil time spec should be accepted");
        assert!(matches!(from_nil, Value::Timer(_)));

        let from_string = builtin_run_at_time(
            &mut eval,
            vec![Value::string("0 sec"), Value::Nil, Value::symbol("cb-from-string")],
        )
        .expect("string time spec should be accepted");
        assert!(matches!(from_string, Value::Timer(_)));
    }

    #[test]
    fn test_eval_run_at_time_invalid_spec_signals_error() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let invalid_string = builtin_run_at_time(
            &mut eval,
            vec![Value::string("abc"), Value::Nil, Value::symbol("cb")],
        );
        assert!(matches!(
            invalid_string,
            Err(Flow::Signal(sig)) if sig.symbol == "error"
        ));

        let invalid_type =
            builtin_run_at_time(&mut eval, vec![Value::True, Value::Nil, Value::symbol("cb")]);
        assert!(matches!(
            invalid_type,
            Err(Flow::Signal(sig)) if sig.symbol == "error"
        ));
    }

    #[test]
    fn test_eval_run_with_idle_timer_nil_ok_string_error() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let from_nil =
            builtin_run_with_idle_timer(&mut eval, vec![Value::Nil, Value::Nil, Value::symbol("cb")])
                .expect("nil idle delay should be accepted");
        assert!(matches!(from_nil, Value::Timer(_)));

        let from_string = builtin_run_with_idle_timer(
            &mut eval,
            vec![Value::string("0 sec"), Value::Nil, Value::symbol("cb")],
        );
        assert!(matches!(
            from_string,
            Err(Flow::Signal(sig)) if sig.symbol == "error"
        ));
    }

    #[test]
    fn test_eval_timer_activate() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Create and cancel a timer
        let result = builtin_run_at_time(
            &mut eval,
            vec![Value::Float(1.0), Value::Nil, Value::symbol("cb")],
        );
        let timer_val = result.unwrap();
        builtin_cancel_timer(&mut eval, vec![timer_val.clone()]).unwrap();

        if let Value::Timer(id) = &timer_val {
            assert!(!eval.timers.timer_active_p(*id));
        }

        // Reactivate
        let result = builtin_timer_activate(&mut eval, vec![timer_val.clone()]);
        assert!(result.is_ok());

        if let Value::Timer(id) = &timer_val {
            assert!(eval.timers.timer_active_p(*id));
        }

        // Active timers cannot be activated again.
        let second = builtin_timer_activate(&mut eval, vec![timer_val.clone()]);
        assert!(matches!(second, Err(Flow::Signal(sig)) if sig.symbol == "error"));
    }

    #[test]
    fn test_eval_timer_activate_rejects_non_timer_with_error() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_timer_activate(&mut eval, vec![Value::Nil]);
        assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol == "error"));
    }
}
