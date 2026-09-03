//! GNU-style wait policy for VM event servicing.
//!
//! GNU Emacs routes process waits, input waits, timer waits, and redisplay
//! through `wait_reading_process_output` with explicit policy flags.  This
//! module gives Neomacs the same shape: callers describe what may be serviced
//! and what should complete the wait; lower-level process/input code only
//! performs the service pass.

use std::time::{Duration, Instant};

use crate::keyboard::SpecialInputServiceOutcome;

use super::error::Flow;
use super::eval::GnuTimerTimestamp;
use super::process::{
    ProcessId, ProcessOutputServiceOutcome, ProcessOutputServiceRequest, ProcessOutputWaitRequest,
    ProcessOutputWaitTiming, ProcessWaitBackendInterest, ProcessWaitEvents,
};

/// GNU's `status_notify` call *inside* `wait_reading_process_output`
/// (src/process.c:5554 and :5854), as a capability that only this module can
/// hand out.
///
/// # The fact this type exists to enforce
///
/// GNU has exactly five `status_notify` call sites
/// (`grep -n 'status_notify' src/process.c`):
///
/// | GNU line | function | what it had just done |
/// |---|---|---|
/// | :1129 | `Fdelete_process`, connection arm | set the status itself |
/// | :1149 | `Fdelete_process`, child arm | set the status itself |
/// | :7181 | `process_send_signal`, SIGCONT arm | set the status itself |
/// | :5554 | `wait_reading_process_output`, top of loop | nothing -- the record is the SIGCHLD handler's |
/// | :5854 | `wait_reading_process_output`, after the select | nothing -- as above |
///
/// The three subr sites notify a status the subr wrote on the line above.
/// **Every status GNU discovered ASYNCHRONOUSLY is notified from the wait, and
/// from nowhere else.**  `process_pending_signals` -- what `maybe_quit` reaches
/// through `probably_quit` (src/lisp.h:3896-3900, src/eval.c:1868-1876) -- is
/// not on the list at all; its entire body is
///
/// ```c
///   pending_signals = false;
///   handle_async_input ();
///   do_pending_atimers ();                          src/keyboard.c:8367-8372
/// ```
///
/// and `grep -c status_notify` over it is **0**.  Nor does GNU's SIGCHLD reach
/// `pending_signals` in the first place: `grep -n 'pending_signals = '
/// src/*.c` returns eleven lines and **not one of them is in `process.c`**.
/// `handle_child_signal`'s wake is `child_signal_notify` (:7766-7767), one
/// `emacs_write` to a self-pipe that the `select` in the wait is watching.
/// GNU's own header for the handler names the destination in words:
/// *"That is saved for the next time keyboard input is done"* (:7669-7671) --
/// and the function that does keyboard input is `wait_reading_process_output`.
///
/// # Why a type rather than a comment
///
/// Ledger 193 drained the child-status record at `Context::maybe_quit`, and
/// the suites did not catch it: a green engine run and a green oracle run
/// prove nothing about *when* a sentinel runs.  What caught it is Lisp that
/// binds a variable around its wait -- `magit-run-post-commit-hook` is keyed on
/// `last-command` -- and finds the binding gone by the time the sentinel runs.
///
/// So the constructors below are private to this module.  `maybe_quit` cannot
/// build one; neither can a subr, a filter, or a future safe point.  Since
/// `ProcessManager::record_child_status_changes` and
/// `Context::record_and_notify_status_changes` both require one, "the child-status
/// record was drained at the wrong safe point" is not rejected by a check --
/// it is a sentence with no grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WaitStatusNotifySite {
    gnu: &'static str,
}

impl WaitStatusNotifySite {
    /// GNU src/process.c:5540-5556 -- the top-of-loop notify, run BEFORE the
    /// block.
    ///
    /// GNU guards it with `if (update_tick != process_tick)`, *"If status of
    /// something has changed, and no input is available, notify the user of
    /// the change right away"*, and does a zero-timeout `thread_select` that
    /// deliberately clears the child-signal fd from the read mask first --
    /// *"If a process status has changed, the child signal pipe will likely be
    /// readable.  We want to ignore it for now, because otherwise we wouldn't
    /// run into a timeout below."*  The point of the placement is that a
    /// status recorded while Lisp was busy cannot sit out the wait's whole
    /// timeout unnotified.
    fn before_the_block() -> Self {
        Self {
            gnu: "src/process.c:5554",
        }
    }

    /// GNU src/process.c:5840-5856 -- the notify after the select returned.
    ///
    /// GNU's guard there is `nfds == 0 && !read_kbd && update_tick !=
    /// process_tick`, and its comment says it is the case the :5554 check
    /// bypassed.  This port reaches it on every wake rather than only on the
    /// empty one, because this port's block returns the READY SET rather than
    /// a count, and a process the drain records is not necessarily in it --
    /// GNU's `status_notify` walks the whole alist and is under no such
    /// restriction.
    fn after_the_block() -> Self {
        Self {
            gnu: "src/process.c:5854",
        }
    }

    /// The GNU line this drain stands for, so a log or a panic can name it.
    pub(crate) fn gnu(self) -> &'static str {
        self.gnu
    }

    /// A stand-in for the unit tests that exercise `handle_child_signal`'s
    /// BODY rather than its placement.
    ///
    /// `#[cfg(test)]`, so it does not exist in the shipped crate.  The
    /// guarantee this type carries is about production code, and what enforces
    /// it is `cargo check -p neovm-core` on the library alone: with the two
    /// private constructors above and this one compiled out, `wait.rs` is the
    /// only module that can build the argument
    /// `ProcessManager::record_child_status_changes` and
    /// `Context::record_and_notify_status_changes` demand.  A unit test that drives
    /// a bare `ProcessManager` has no wait to be inside, and saying so here is
    /// better than letting it reach for `pub(crate)`.
    #[cfg(test)]
    pub(crate) fn for_a_unit_test_of_the_walk_itself() -> Self {
        Self {
            gnu: "src/process.c:7734 (the walk, without a wait around it)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitDeadline {
    Poll,
    Until {
        instant: Instant,
        coarse_end: Option<Duration>,
        timer_deadline: GnuTimerTimestamp,
    },
    Forever,
}

fn monotonic_coarse_now() -> Option<Duration> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        let mut ts = std::mem::MaybeUninit::<libc::timespec>::uninit();
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_COARSE, ts.as_mut_ptr()) };
        if rc == 0 {
            let ts = unsafe { ts.assume_init() };
            if ts.tv_sec >= 0 && ts.tv_nsec >= 0 {
                return Some(Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32));
            }
        }
    }

    None
}

impl WaitDeadline {
    fn until(instant: Instant) -> Self {
        let now = Instant::now();
        let remaining = instant.saturating_duration_since(now);
        Self::until_with_timer_deadline(
            instant,
            None,
            GnuTimerTimestamp::now().add_duration(remaining),
        )
    }

    fn for_duration(duration: Duration) -> Self {
        Self::for_duration_with_timer_deadline(
            duration,
            GnuTimerTimestamp::now().add_duration(duration),
        )
    }

    fn for_duration_with_timer_deadline(
        duration: Duration,
        timer_deadline: GnuTimerTimestamp,
    ) -> Self {
        Self::until_with_timer_deadline(
            Instant::now() + duration,
            monotonic_coarse_now().map(|now| now + duration),
            timer_deadline,
        )
    }

    fn until_with_timer_deadline(
        instant: Instant,
        coarse_end: Option<Duration>,
        timer_deadline: GnuTimerTimestamp,
    ) -> Self {
        Self::Until {
            instant,
            coarse_end,
            timer_deadline,
        }
    }

    fn expired(self, now: Instant) -> bool {
        match self {
            Self::Until {
                instant,
                coarse_end: Some(coarse_end),
                ..
            } => {
                monotonic_coarse_now().map_or(now >= instant, |coarse_now| coarse_now >= coarse_end)
            }
            Self::Until { instant, .. } => now >= instant,
            Self::Poll | Self::Forever => false,
        }
    }

    fn remaining(self, now: Instant) -> Option<Duration> {
        match self {
            Self::Poll => Some(Duration::ZERO),
            Self::Until {
                instant,
                coarse_end: Some(coarse_end),
                ..
            } => Some(
                monotonic_coarse_now()
                    .map(|coarse_now| coarse_end.saturating_sub(coarse_now))
                    .unwrap_or_else(|| instant.saturating_duration_since(now)),
            ),
            Self::Until { instant, .. } => Some(instant.saturating_duration_since(now)),
            Self::Forever => None,
        }
    }

    fn timer_deadline(self) -> Option<GnuTimerTimestamp> {
        match self {
            Self::Until { timer_deadline, .. } => Some(timer_deadline),
            Self::Poll | Self::Forever => None,
        }
    }
}

fn process_output_wait_deadline(timing: ProcessOutputWaitTiming) -> WaitDeadline {
    match timing {
        ProcessOutputWaitTiming::Poll => WaitDeadline::Poll,
        ProcessOutputWaitTiming::For(duration) => WaitDeadline::for_duration(duration),
        ProcessOutputWaitTiming::Forever => WaitDeadline::Forever,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyboardWaitPolicy {
    ServiceSpecialOnly,
    WaitForSpecialInput,
    YieldOnCommandInput,
    ReadCommandInput,
}

impl KeyboardWaitPolicy {
    fn completes_on_command_input(self) -> bool {
        matches!(self, Self::YieldOnCommandInput | Self::ReadCommandInput)
    }

    fn waits_for_host_input(self) -> bool {
        matches!(
            self,
            Self::WaitForSpecialInput | Self::YieldOnCommandInput | Self::ReadCommandInput
        )
    }

    fn sets_waiting_for_user_input(self) -> bool {
        matches!(self, Self::ReadCommandInput)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessWaitPolicy {
    None,
    ServiceAny,
    Any,
    Target(ProcessId),
    TargetOnly(ProcessId),
}

impl ProcessWaitPolicy {
    fn target(process: ProcessId, just_this_one: bool) -> Self {
        if just_this_one {
            Self::TargetOnly(process)
        } else {
            Self::Target(process)
        }
    }

    fn target_process(self) -> Option<ProcessId> {
        match self {
            Self::Target(id) | Self::TargetOnly(id) => Some(id),
            Self::None | Self::ServiceAny | Self::Any => None,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn just_this_one(self) -> bool {
        matches!(self, Self::TargetOnly(_))
    }

    fn services_processes(self) -> bool {
        !matches!(self, Self::None)
    }

    fn satisfied_by(self, outcome: WaitServiceOutcome) -> bool {
        match self {
            Self::Any => outcome.has_any_process_activity(),
            Self::Target(_) | Self::TargetOnly(_) => outcome.has_target_process_activity(),
            Self::None | Self::ServiceAny => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerWaitPolicy {
    Run,
    Suppress,
}

impl TimerWaitPolicy {
    fn allow(self) -> bool {
        matches!(self, Self::Run)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecialInputWaitPolicy {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Suppress,
    ServiceOnly,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    CompleteOnAny,
    CompleteOnResize,
}

impl SpecialInputWaitPolicy {
    fn services_input(self) -> bool {
        !matches!(self, Self::Suppress)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WaitRequest {
    deadline: WaitDeadline,
    keyboard: KeyboardWaitPolicy,
    processes: ProcessWaitPolicy,
    timers: TimerWaitPolicy,
    redisplay: bool,
    special_input: SpecialInputWaitPolicy,
}

impl WaitRequest {
    fn accept_process_output_request(request: ProcessOutputWaitRequest) -> Self {
        match (request.target_process(), request.allow_timers()) {
            (Some(id), true) => Self::accept_target_process_output_with_timers(
                request.timing(),
                id,
                request.just_this_one(),
            ),
            (Some(id), false) => Self::accept_target_process_output_without_timers(
                request.timing(),
                id,
                request.just_this_one(),
            ),
            (None, true) => Self::accept_any_process_output_with_timers(request.timing()),
            (None, false) => Self::accept_any_process_output_without_timers(request.timing()),
        }
    }

    /// GNU `Faccept_process_output` calls `wait_reading_process_output` with
    /// READ_KBD = 0 (process.c:4957-4959), and that value is exactly what
    /// suppresses the return-on-input path: when input is pending the loop only
    /// runs `swallow_events` and keeps waiting, with the `break` deliberately
    /// `#if 0`-ed out under the comment "Exiting when read_kbd doesn't request
    /// that seems wrong, though" (process.c:5930-5937).  The docstring states
    /// the same contract: "if PROCESS is nil, the function should not be
    /// expected to return before the timeout expires".
    ///
    /// So this wait services special input but never completes on pending
    /// command input.  Yielding there made `accept-process-output` a no-op for
    /// any caller pumping the event loop while ordinary input was queued --
    /// most visibly inside a keyboard macro, where the not-yet-executed macro
    /// events are pending input, so a package that waits for an asynchronous
    /// HTTP reply from `post-command-hook` returned instantly and never saw the
    /// reply.  `sit-for` keeps yielding: GNU passes a non-zero READ_KBD there
    /// (`sit_for`, dispnew.c).
    fn accept_process_output(
        deadline: WaitDeadline,
        processes: ProcessWaitPolicy,
        timers: TimerWaitPolicy,
    ) -> Self {
        Self {
            deadline,
            keyboard: KeyboardWaitPolicy::ServiceSpecialOnly,
            processes,
            timers,
            redisplay: false,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn accept_process_output_with_timers(
        deadline: WaitDeadline,
        processes: ProcessWaitPolicy,
    ) -> Self {
        Self::accept_process_output(deadline, processes, TimerWaitPolicy::Run)
    }

    fn accept_process_output_without_timers(
        deadline: WaitDeadline,
        processes: ProcessWaitPolicy,
    ) -> Self {
        Self::accept_process_output(deadline, processes, TimerWaitPolicy::Suppress)
    }

    fn accept_any_process_output_with_timers(timing: ProcessOutputWaitTiming) -> Self {
        Self::accept_process_output_with_timers(
            process_output_wait_deadline(timing),
            ProcessWaitPolicy::Any,
        )
    }

    fn accept_any_process_output_without_timers(timing: ProcessOutputWaitTiming) -> Self {
        Self::accept_process_output_without_timers(
            process_output_wait_deadline(timing),
            ProcessWaitPolicy::Any,
        )
    }

    fn accept_target_process_output_with_timers(
        timing: ProcessOutputWaitTiming,
        process: ProcessId,
        just_this_one: bool,
    ) -> Self {
        Self::accept_process_output_with_timers(
            process_output_wait_deadline(timing),
            ProcessWaitPolicy::target(process, just_this_one),
        )
    }

    fn accept_target_process_output_without_timers(
        timing: ProcessOutputWaitTiming,
        process: ProcessId,
        just_this_one: bool,
    ) -> Self {
        Self::accept_process_output_without_timers(
            process_output_wait_deadline(timing),
            ProcessWaitPolicy::target(process, just_this_one),
        )
    }

    fn read_command_input(deadline: WaitDeadline) -> Self {
        Self {
            deadline,
            keyboard: KeyboardWaitPolicy::ReadCommandInput,
            processes: ProcessWaitPolicy::ServiceAny,
            timers: TimerWaitPolicy::Run,
            redisplay: true,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn read_command_input_until(deadline: Instant) -> Self {
        Self::read_command_input(WaitDeadline::until(deadline))
    }

    fn read_command_input_forever() -> Self {
        Self::read_command_input(WaitDeadline::Forever)
    }

    fn service_once(redisplay: bool) -> Self {
        Self {
            deadline: WaitDeadline::Poll,
            keyboard: KeyboardWaitPolicy::ServiceSpecialOnly,
            processes: ProcessWaitPolicy::ServiceAny,
            timers: TimerWaitPolicy::Run,
            redisplay,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn input_pending_poll(timers: TimerWaitPolicy) -> Self {
        Self {
            deadline: WaitDeadline::Poll,
            keyboard: KeyboardWaitPolicy::YieldOnCommandInput,
            processes: ProcessWaitPolicy::None,
            timers,
            redisplay: false,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn input_pending_without_timers() -> Self {
        Self::input_pending_poll(TimerWaitPolicy::Suppress)
    }

    fn input_pending_with_timers() -> Self {
        Self::input_pending_poll(TimerWaitPolicy::Run)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn timer_service(redisplay: bool) -> Self {
        Self {
            deadline: WaitDeadline::Poll,
            keyboard: KeyboardWaitPolicy::ServiceSpecialOnly,
            processes: ProcessWaitPolicy::None,
            timers: TimerWaitPolicy::Run,
            redisplay,
            special_input: SpecialInputWaitPolicy::Suppress,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sleep_until(deadline: Instant) -> Self {
        Self {
            deadline: WaitDeadline::until(deadline),
            keyboard: KeyboardWaitPolicy::ServiceSpecialOnly,
            processes: ProcessWaitPolicy::ServiceAny,
            timers: TimerWaitPolicy::Run,
            redisplay: false,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn sleep_for_duration_until_timer_deadline(
        duration: Duration,
        timer_deadline: GnuTimerTimestamp,
    ) -> Self {
        Self {
            deadline: WaitDeadline::for_duration_with_timer_deadline(duration, timer_deadline),
            keyboard: KeyboardWaitPolicy::ServiceSpecialOnly,
            processes: ProcessWaitPolicy::ServiceAny,
            timers: TimerWaitPolicy::Run,
            redisplay: false,
            special_input: SpecialInputWaitPolicy::ServiceOnly,
        }
    }

    fn resize_ack(deadline: Instant) -> Self {
        Self {
            deadline: WaitDeadline::until(deadline),
            keyboard: KeyboardWaitPolicy::WaitForSpecialInput,
            processes: ProcessWaitPolicy::None,
            timers: TimerWaitPolicy::Suppress,
            redisplay: false,
            special_input: SpecialInputWaitPolicy::CompleteOnResize,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn deadline(self) -> WaitDeadline {
        self.deadline
    }

    fn deadline_is_finite(self) -> bool {
        matches!(self.deadline, WaitDeadline::Until { .. })
    }

    fn timer_deadline(self) -> Option<GnuTimerTimestamp> {
        self.deadline.timer_deadline()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn deadline_is_forever(self) -> bool {
        matches!(self.deadline, WaitDeadline::Forever)
    }

    fn target_process(self) -> Option<ProcessId> {
        self.processes.target_process()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn completes_on_any_process_activity(self) -> bool {
        matches!(self.processes, ProcessWaitPolicy::Any)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn completes_on_target_process_activity(self, process: ProcessId) -> bool {
        matches!(
            self.processes,
            ProcessWaitPolicy::Target(id) | ProcessWaitPolicy::TargetOnly(id) if id == process
        )
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn restricts_process_service_to_target(self) -> bool {
        self.processes.just_this_one()
    }

    fn services_process_output(self) -> bool {
        self.processes.services_processes()
    }

    fn process_output_service_request(self) -> ProcessOutputServiceRequest {
        match self.processes {
            ProcessWaitPolicy::None => ProcessOutputServiceRequest::none(),
            ProcessWaitPolicy::ServiceAny | ProcessWaitPolicy::Any => {
                ProcessOutputServiceRequest::any(None)
            }
            ProcessWaitPolicy::Target(target) => ProcessOutputServiceRequest::any(Some(target)),
            ProcessWaitPolicy::TargetOnly(target) => {
                ProcessOutputServiceRequest::target_only(target)
            }
        }
    }

    fn services_special_input(self) -> bool {
        self.special_input.services_input()
    }

    fn waits_for_host_input(self) -> bool {
        self.keyboard.waits_for_host_input()
    }

    fn completes_on_command_input(self) -> bool {
        self.keyboard.completes_on_command_input()
    }

    fn sets_waiting_for_user_input(self) -> bool {
        self.keyboard.sets_waiting_for_user_input()
    }

    fn runs_timers(self) -> bool {
        self.timers.allow()
    }

    /// The specific process this wait targets (`accept-process-output PROC`),
    /// if any.
    fn target_pid(self) -> Option<ProcessId> {
        self.processes.target_process()
    }

    fn poll_or_deadline_elapsed(self, now: Instant) -> bool {
        matches!(self.deadline, WaitDeadline::Poll) || self.deadline.expired(now)
    }

    fn deadline_elapsed(self, now: Instant) -> bool {
        self.deadline.expired(now)
    }

    fn base_timeout(self, now: Instant) -> Duration {
        self.deadline
            .remaining(now)
            .unwrap_or_else(|| Duration::from_secs(100_000))
    }

    fn needs_redisplay_after_service(
        self,
        special_input: SpecialInputServiceOutcome,
        outcome: WaitServiceOutcome,
    ) -> bool {
        // GNU `wait_reading_process_output` runs `redisplay_preserve_echo_area`
        // at the top of every loop iteration when `do_display` is set
        // (process.c), so a process filter/sentinel that modifies a displayed
        // buffer during the wait is reflected on screen without waiting for the
        // next keystroke. Neomacs gates redisplay on activity (a battery-minded
        // divergence from GNU's unconditional per-iteration redisplay), but that
        // gate MUST include process activity: a filter running is exactly when a
        // buffer may have changed. Omitting it left async output — eww "Loading…"
        // never resolving, comint/async-shell output, LSP — stale on screen until
        // the user pressed a key. `redisplay()` still no-ops via the unchanged
        // `RedisplaySignature` when the filter changed nothing displayed, so this
        // is strictly less aggressive than GNU.
        self.redisplay
            && (special_input.redisplay_needed()
                || outcome.has_timer_activity()
                || outcome.ran_process_callbacks())
    }

    fn needs_redisplay_after_command_input(
        self,
        special_input: SpecialInputServiceOutcome,
    ) -> bool {
        self.redisplay && special_input.redisplay_needed()
    }

    fn completion_for(self, outcome: WaitServiceOutcome) -> Option<WaitCompletion> {
        if self.completes_on_command_input() && outcome.has_command_input_pending() {
            return Some(WaitCompletion::CommandInputPending);
        }

        if self.processes.satisfied_by(outcome) {
            return Some(WaitCompletion::ProcessActivity);
        }

        match self.special_input {
            SpecialInputWaitPolicy::CompleteOnAny if outcome.has_special_input_activity() => {
                return Some(WaitCompletion::SpecialInputActivity);
            }
            SpecialInputWaitPolicy::CompleteOnResize if outcome.has_resize_activity() => {
                return Some(WaitCompletion::SpecialInputActivity);
            }
            SpecialInputWaitPolicy::Suppress
            | SpecialInputWaitPolicy::ServiceOnly
            | SpecialInputWaitPolicy::CompleteOnAny
            | SpecialInputWaitPolicy::CompleteOnResize => {}
        }

        None
    }

    fn needs_minimum_process_drain_before_completion(
        self,
        completion: WaitCompletion,
        outcome: WaitServiceOutcome,
    ) -> bool {
        completion == WaitCompletion::ProcessActivity
            && self.target_process().is_some()
            && outcome.has_target_process_activity()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitCompletion {
    ProcessActivity,
    CommandInputPending,
    SpecialInputActivity,
    DeadlineElapsed,
    /// The wait's target process reached a status that ends the wait in GNU
    /// (`wait_reading_process_output` breaks when WAIT_PROC's status is
    /// neither `run` nor a pending connect) or no longer exists (reaped).
    /// `accept-process-output` returns nil for this, like a timeout.
    TargetProcessTerminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandInputWaitOutcome {
    InputPending,
    Interrupted,
    DeadlineElapsed,
}

impl CommandInputWaitOutcome {
    fn from_completion(completion: WaitCompletion) -> Self {
        match completion {
            WaitCompletion::CommandInputPending => Self::InputPending,
            WaitCompletion::DeadlineElapsed | WaitCompletion::TargetProcessTerminated => {
                Self::DeadlineElapsed
            }
            WaitCompletion::ProcessActivity | WaitCompletion::SpecialInputActivity => {
                Self::Interrupted
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutputWaitOutcome {
    ProcessActivity,
    NoProcessActivity,
}

impl ProcessOutputWaitOutcome {
    fn from_completion(completion: WaitCompletion) -> Self {
        match completion {
            WaitCompletion::ProcessActivity => Self::ProcessActivity,
            WaitCompletion::CommandInputPending
            | WaitCompletion::SpecialInputActivity
            | WaitCompletion::DeadlineElapsed
            | WaitCompletion::TargetProcessTerminated => Self::NoProcessActivity,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WaitProcessActivity {
    #[default]
    None,
    Any,
    Target,
}

impl WaitProcessActivity {
    fn record(self, target: bool) -> Self {
        if target || matches!(self, Self::Target) {
            Self::Target
        } else {
            Self::Any
        }
    }

    fn any(self) -> bool {
        matches!(self, Self::Any | Self::Target)
    }

    fn target(self) -> bool {
        matches!(self, Self::Target)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WaitSpecialInputActivity {
    #[default]
    None,
    Any,
    Resize,
}

impl WaitSpecialInputActivity {
    fn record(self, activity: Self) -> Self {
        match (self, activity) {
            (Self::Resize, _) | (_, Self::Resize) => Self::Resize,
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::None, Self::None) => Self::None,
        }
    }

    fn any(self) -> bool {
        matches!(self, Self::Any | Self::Resize)
    }

    fn resize(self) -> bool {
        matches!(self, Self::Resize)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WaitServiceOutcome {
    process_activity: WaitProcessActivity,
    /// Non-output servicing ran during the wait: a connect completed, a
    /// sentinel/status-notify fired, or an EOF was handled. Kept SEPARATE from
    /// `process_activity` (which is output-bytes-only and gates wait
    /// completion, matching GNU's `got_some_output`) because a sentinel must
    /// not complete an `accept-process-output` — but it CAN change a displayed
    /// buffer (eww's completion callback runs in url-http's EOF sentinel), so
    /// the redisplay decision must see it.
    process_serviced: bool,
    special_input_activity: WaitSpecialInputActivity,
    timers_fired: bool,
    command_input_pending: bool,
}

impl WaitServiceOutcome {
    fn has_any_process_activity(self) -> bool {
        self.process_activity.any()
    }

    fn has_target_process_activity(self) -> bool {
        self.process_activity.target()
    }

    fn absorb_process_activity(&mut self, process_outcome: ProcessOutputServiceOutcome) {
        if process_outcome.has_target_process_activity() {
            self.process_activity = self.process_activity.record(true);
        } else if process_outcome.has_any_process_activity() {
            self.process_activity = self.process_activity.record(false);
        }
        self.process_serviced |= process_outcome.has_serviced_activity();
    }

    /// Any process work ran that may have changed a displayed buffer — output
    /// bytes read (filters) or non-output servicing (sentinels/connects/EOF).
    /// Distinct from [`Self::has_any_process_activity`], which gates wait
    /// completion and stays output-only to match GNU's `got_some_output`.
    fn ran_process_callbacks(self) -> bool {
        self.process_activity.any() || self.process_serviced
    }

    fn absorb_special_input_activity(&mut self, special_input: SpecialInputServiceOutcome) {
        if special_input.has_resize_activity() {
            self.special_input_activity = self
                .special_input_activity
                .record(WaitSpecialInputActivity::Resize);
        } else if special_input.has_any_activity() {
            self.special_input_activity = self
                .special_input_activity
                .record(WaitSpecialInputActivity::Any);
        }
    }

    fn has_special_input_activity(self) -> bool {
        self.special_input_activity.any()
    }

    fn has_resize_activity(self) -> bool {
        self.special_input_activity.resize()
    }

    fn record_command_input_pending(&mut self) {
        self.command_input_pending = true;
    }

    fn has_command_input_pending(self) -> bool {
        self.command_input_pending
    }

    fn record_timer_activity(&mut self, fired: bool) {
        self.timers_fired |= fired;
    }

    fn has_timer_activity(self) -> bool {
        self.timers_fired
    }
}

#[derive(Debug, PartialEq, Eq)]
enum WaitProcessService {
    Poll,
    Ready(ProcessWaitEvents),
}

#[derive(Debug, PartialEq, Eq)]
struct WaitBlockActivity {
    notification_wakeup: bool,
    process_service: WaitProcessService,
}

impl WaitBlockActivity {
    fn poll() -> Self {
        Self {
            notification_wakeup: false,
            process_service: WaitProcessService::Poll,
        }
    }

    fn ready_processes(processes: Vec<ProcessId>) -> Self {
        Self {
            notification_wakeup: false,
            process_service: WaitProcessService::Ready(ProcessWaitEvents::ready_processes(
                processes,
            )),
        }
    }

    fn from_source_events(events: ProcessWaitEvents) -> Self {
        let notification_wakeup = events.has_notification_wakeup();
        let process_service = if !events.has_ready_processes() && !events.has_writable_processes() {
            WaitProcessService::Poll
        } else {
            WaitProcessService::Ready(events)
        };
        Self {
            notification_wakeup,
            process_service,
        }
    }

    fn has_notification_wakeup(&self) -> bool {
        self.notification_wakeup
    }

    fn has_external_activity(&self) -> bool {
        self.notification_wakeup || matches!(self.process_service, WaitProcessService::Ready(_))
    }

    fn into_process_service(self) -> WaitProcessService {
        self.process_service
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WaitTimeoutChoice {
    duration: Duration,
    finite_deadline_timeout: bool,
    shortened_by_timer: bool,
}

impl WaitTimeoutChoice {
    fn run_timers_after_block(self, activity: &WaitBlockActivity, deadline_elapsed: bool) -> bool {
        if deadline_elapsed {
            return false;
        }

        self.shortened_by_timer || !self.finite_deadline_timeout || activity.has_external_activity()
    }
}

/// How the wait loop blocks for one iteration.
///
/// When a wait poller exists (the normal case on every OS) we block on it —
/// GNU's single `pselect` equivalent — watching input notifications and/or
/// process fds as the request requires (`Poller`). Without a poller (creation
/// failed / headless) we fall back to an explicit degraded primitive.
#[derive(Debug, PartialEq, Eq)]
enum WaitBlock {
    /// Zero timeout: service immediately, don't block.
    Poll,
    /// Block on the unified poller with the given interest.
    Poller(ProcessWaitBackendInterest),
    /// Block directly on the host-input channel (`recv_timeout`), ignoring
    /// process fds. Used when no poller exists, or when host input is wanted
    /// but process output is not *and* live process fds exist — polling those
    /// with no interest would spin (they stay readable), so we wait on the
    /// channel instead.
    HostInputChannel,
    /// No poller: poll process output via a short sleep + harvest all live.
    ProcessOutputSleep,
    /// No poller, pure timeout: blind sleep.
    Sleep,
}

impl super::eval::Context {
    fn service_wait_request_once(&mut self, request: &WaitRequest) -> Result<(), Flow> {
        let _ = self.service_wait_request_once_outcome(request)?;
        Ok(())
    }

    pub(crate) fn service_input_pending_without_timers(&mut self) -> Result<(), Flow> {
        self.service_wait_request_once(&WaitRequest::input_pending_without_timers())
    }

    pub(crate) fn service_input_pending_with_timers(&mut self) -> Result<(), Flow> {
        self.service_wait_request_once(&WaitRequest::input_pending_with_timers())
    }

    pub(crate) fn service_input_wait_with_redisplay(&mut self) -> Result<(), Flow> {
        self.service_wait_request_once(&WaitRequest::service_once(true))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn service_timers_with_redisplay(&mut self) -> Result<(), Flow> {
        self.service_wait_request_once(&WaitRequest::timer_service(true))
    }

    pub(crate) fn service_timers_without_redisplay(&mut self) -> Result<(), Flow> {
        self.service_wait_request_once(&WaitRequest::timer_service(false))
    }

    fn service_wait_request_once_outcome(
        &mut self,
        request: &WaitRequest,
    ) -> Result<WaitServiceOutcome, Flow> {
        self.service_wait_request_processes(request, WaitProcessService::Poll, true)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn service_wait_request_source_events_outcome(
        &mut self,
        request: &WaitRequest,
        events: ProcessWaitEvents,
    ) -> Result<WaitServiceOutcome, Flow> {
        self.service_wait_request_block_activity(
            request,
            WaitBlockActivity::from_source_events(events),
            true,
        )
    }

    fn service_wait_request_block_activity(
        &mut self,
        request: &WaitRequest,
        activity: WaitBlockActivity,
        run_timers: bool,
    ) -> Result<WaitServiceOutcome, Flow> {
        if activity.has_notification_wakeup() {
            // Frontend input is one possible notification source.  Probing the
            // channel is harmless for diagnostics/DNS notifications and keeps
            // the generic notifier independent of its producers.
            let _ = self.stage_next_host_input_event_if_available()?;
        }
        self.service_wait_request_processes(request, activity.into_process_service(), run_timers)
    }

    fn service_wait_request_processes(
        &mut self,
        request: &WaitRequest,
        process_service: WaitProcessService,
        run_timers: bool,
    ) -> Result<WaitServiceOutcome, Flow> {
        let mut outcome = WaitServiceOutcome::default();
        let special_input = if request.services_special_input() {
            self.service_wait_request_special_input_events()?
        } else {
            SpecialInputServiceOutcome::default()
        };
        outcome.absorb_special_input_activity(special_input);

        // Run timers before draining process output, matching GNU's
        // `wait_reading_process_output` (src/process.c): it runs `timer_check`
        // near the top of the loop (before `pselect`) and only reads readable
        // process fds afterwards, so timer callbacks fire before process
        // filters in the same service pass. `run_timers` is false when the
        // wait's deadline has already elapsed at wake time: GNU's loop-top
        // deadline check (process.c:5469-5478) precedes `timer_check`, so a
        // timer becoming ripe exactly at the deadline does not fire inside
        // this wait — while ready process fds from the final pselect are
        // still read.
        if run_timers && request.runs_timers() {
            // A non-local `throw` from a timer callback propagates out of the
            // wait to the matching outer `catch` (GNU `timer-event-handler`
            // catches `error` signals only).  `?` returns the throw up through
            // `accept-process-output` to the VM `catch`, e.g. the
            // `jsonrpc-request` catch tag completed by a zero-delay timer.
            outcome.record_timer_activity(self.service_pending_timers_with_wait_policy(false)?);
        }

        // Drain ready process output (a non-blocking poll of already-readable
        // fds plus filter dispatch) BEFORE yielding to pending command input.
        // GNU's `wait_reading_process_output` reads readable process fds from
        // the `pselect` result and runs their filters before it notices
        // keyboard input (the keyboard `break` at the "Check for keyboard
        // input" comment happens, but the process fds available from the prior
        // select are read first), so the bytes are always drained.  The old
        // order early-returned on pending command input before this poll, which
        // starved a process that already had a full response waiting in its
        // pipe: a re-entrant `accept-process-output` from a jsonrpc/Copilot
        // timer would hang forever even though the child had written its reply.
        let process_request = request.process_output_service_request();
        // A non-local `throw` from a process filter/sentinel callback likewise
        // propagates out of the wait to the matching outer `catch` (GNU's
        // `read_process_output`/`exec_sentinel` never catch throws).
        let process_outcome = match process_service {
            WaitProcessService::Poll => {
                self.poll_process_output_for_service_request(&process_request)?
            }
            WaitProcessService::Ready(events) => {
                self.poll_ready_process_output_for_service_request(events, &process_request)?
            }
        };
        outcome.absorb_process_activity(process_outcome);

        if request.completes_on_command_input()
            && self.stage_pending_command_input_for_wait_request()?
        {
            outcome.record_command_input_pending();
            if request.needs_redisplay_after_command_input(special_input) {
                self.redisplay();
            }
            return Ok(outcome);
        }
        if request.needs_redisplay_after_service(special_input, outcome) {
            self.redisplay();
        }
        Ok(outcome)
    }

    /// GNU ends a WAIT_PROC wait once the target process is no longer
    /// running/connecting (process.c drains remaining output, then breaks);
    /// output read during the final drain still wins via `completion_for`.
    fn wait_request_target_terminated(&self, request: &WaitRequest) -> bool {
        request.target_pid().is_some_and(|pid| {
            self.processes
                .get(pid)
                .is_none_or(super::process::process_status_ends_target_wait)
        })
    }

    fn service_minimum_process_drain(
        &mut self,
        request: &WaitRequest,
    ) -> Result<WaitServiceOutcome, Flow> {
        let activity = if self.processes.has_wait_notification_backend() {
            let events = self
                .processes
                .wait_for_backend_events(Duration::ZERO, ProcessWaitBackendInterest::ProcessesOnly)
                .unwrap_or_default();
            WaitBlockActivity::from_source_events(events)
        } else {
            WaitBlockActivity::poll()
        };
        self.service_wait_request_block_activity(request, activity, true)
    }

    fn complete_wait_after_required_minimum_drain(
        &mut self,
        request: &WaitRequest,
        outcome: WaitServiceOutcome,
    ) -> Result<Option<WaitCompletion>, Flow> {
        let Some(completion) = request.completion_for(outcome) else {
            return Ok(None);
        };

        // GNU `wait_reading_process_output` sets `wait = MINIMUM` after reading
        // bytes from WAIT_PROC.  That makes one zero-time process wait/service
        // pass before the function returns, so a concurrent PTY EOF/SIGCHLD can
        // run `status_notify` and the sentinel in the same
        // `accept-process-output` call.
        let explicit_coding_status_deferred = request
            .target_pid()
            .is_some_and(|pid| self.processes.defers_minimum_status_drain_after_output(pid));
        if request.needs_minimum_process_drain_before_completion(completion, outcome)
            && !explicit_coding_status_deferred
        {
            let _ = self.service_minimum_process_drain(request)?;
        }

        Ok(Some(completion))
    }

    /// GNU's `got_some_output = status_notify (NULL, wait_proc)`
    /// (src/process.c:5554, :5854), with this port's `handle_child_signal`
    /// immediately in front of it.
    ///
    /// GNU splits the two: the record is made in the SIGCHLD handler and the
    /// notification here.  This port cannot record in a handler (the walk
    /// allocates and the table is the Lisp thread's), so it does both here --
    /// **in one call, and that is the invariant.**  A record made without the
    /// notification following it in the same call is what makes
    /// `(process-live-p p)` answer `nil` with the sentinel unrun, which is the
    /// state ledger 198 is about.
    ///
    /// GNU's return value is folded into `got_some_output`, which decides
    /// whether the wait made progress; this port's equivalent is the
    /// [`ProcessOutputServiceOutcome`] the caller absorbs into the wait's
    /// completion decision, so the notification's own output reads count for
    /// the wait exactly as GNU's do.
    fn record_and_notify_status_changes(
        &mut self,
        request: &WaitRequest,
        site: WaitStatusNotifySite,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let target = request.target_process();
        // GNU `handle_child_signal`'s `FOR_EACH_PROCESS` walk (src/process.c:
        // 7734-7763).  GNU runs it in the SIGCHLD handler; this port cannot
        // (the process table is a `HashMap` owned by the Lisp thread and
        // iterating it allocates, which GNU's own two warnings above the
        // function forbid), so it runs here, at GNU's own `status_notify`
        // sites.  It is UNCONDITIONAL because GNU's own arming is not a
        // signal: `update_tick != process_tick` (:5524, :5845) is a counter,
        // and it is a performance short-circuit rather than the correctness
        // invariant -- deleting it from both of GNU's wait sites leaves GNU
        // correct, because `status_notify`'s body is guarded per process at
        // :7892.
        let _stamped = self.processes.record_child_status_changes(site);
        // GNU `status_notify (NULL, wait_proc)` (:5554, :5854), over the
        // processes whose own tick moved -- from ANY of GNU's eight sites, not
        // just the walk above.
        self.notify_processes_with_unnotified_status_change(target)
    }

    fn wait_reading_process_output(
        &mut self,
        request: WaitRequest,
    ) -> Result<WaitCompletion, Flow> {
        // GNU src/process.c:5540-5556, on the first pass through the loop:
        // before anything blocks, notify any status that changed while Lisp
        // was busy.  Here that means running `handle_child_signal`'s walk
        // first, because this port could not run it in the handler.
        let notified = self
            .record_and_notify_status_changes(&request, WaitStatusNotifySite::before_the_block())?;
        let mut outcome = self.service_wait_request_once_outcome(&request)?;
        outcome.absorb_process_activity(notified);
        if let Some(completion) =
            self.complete_wait_after_required_minimum_drain(&request, outcome)?
        {
            return Ok(completion);
        }
        if self.wait_request_target_terminated(&request) {
            return Ok(WaitCompletion::TargetProcessTerminated);
        }
        if request.poll_or_deadline_elapsed(Instant::now()) {
            return Ok(WaitCompletion::DeadlineElapsed);
        }

        loop {
            // GNU `wait_reading_process_output` runs `maybe_quit` at the top of
            // every `while(1)` iteration when `read_kbd >= 0` (process.c:5399-5400),
            // and `Fsleep_for`/`Faccept_process_output` pass `read_kbd >= 0`.  We
            // have no internal-no-quit (`read_kbd < 0`) wait policy, so this applies
            // to all of our waits.  Placing it first means a quit pending before the
            // first block is caught on iteration 1 (before blocking) and every
            // iteration thereafter, so C-g interrupts `sleep-for` / poll loops
            // within one iteration instead of running for the full deadline.  It is
            // inhibit-quit-safe: `maybe_quit` returns `Ok` when `inhibit-quit` is
            // non-nil, so `accept-process-output` keeps blocking with quit inhibited.
            self.maybe_quit()?;

            let now = Instant::now();
            if request.deadline_elapsed(now) {
                return Ok(WaitCompletion::DeadlineElapsed);
            }

            let wait_timeout = self.next_wait_request_timeout(&request, now);
            let activity = self.block_for_wait_request(&request, wait_timeout.duration)?;
            // Service cross-thread eval-tasks (e.g. a diagnostics profile
            // capture) here: a `Poller::notify()` wakes this inner block without
            // host input, and the block would otherwise re-loop without ever
            // returning to the outer read_char drain point. Tasks are only the
            // diagnostics profiler ops today (safe at any wait point); a general
            // task must stay non-reentrant here.
            self.drain_eval_tasks();
            // GNU services a wake in [read fds] → [loop-top deadline check] →
            // [timer_check] order, so when the deadline elapses during the
            // block, ready process output is still drained but ripe timers are
            // NOT run — the deadline break precedes the next `timer_check`
            // (process.c:5469-5478). Without this, a repeating timer whose
            // next fire lands exactly on the wait deadline fires once more
            // than GNU (e.g. 21 vs 20 fires for a 1ms timer in `(sit-for
            // 0.02)`).
            let deadline_elapsed = request.deadline_elapsed(Instant::now());
            let run_timers = wait_timeout.run_timers_after_block(&activity, deadline_elapsed);
            // GNU src/process.c:5840-5856, immediately after the select
            // returns.  It runs BEFORE the service pass because the service
            // pass may be restricted to the block's ready set, and a status a
            // SIGCHLD recorded is not necessarily in it -- GNU's
            // `status_notify` walks the whole alist and has no such
            // restriction.
            let notified = self.record_and_notify_status_changes(
                &request,
                WaitStatusNotifySite::after_the_block(),
            )?;
            outcome = self.service_wait_request_block_activity(&request, activity, run_timers)?;
            outcome.absorb_process_activity(notified);

            if let Some(completion) =
                self.complete_wait_after_required_minimum_drain(&request, outcome)?
            {
                return Ok(completion);
            }
            if self.wait_request_target_terminated(&request) {
                return Ok(WaitCompletion::TargetProcessTerminated);
            }
        }
    }

    pub(crate) fn wait_for_process_output(
        &mut self,
        request: ProcessOutputWaitRequest,
    ) -> Result<ProcessOutputWaitOutcome, Flow> {
        self.wait_reading_process_output(WaitRequest::accept_process_output_request(request))
            .map(ProcessOutputWaitOutcome::from_completion)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn wait_until(&mut self, deadline: Instant) -> Result<(), Flow> {
        let _ = self.wait_reading_process_output(WaitRequest::sleep_until(deadline))?;
        Ok(())
    }

    pub(crate) fn wait_for_duration_until_timer_deadline(
        &mut self,
        duration: Duration,
        timer_deadline: GnuTimerTimestamp,
    ) -> Result<(), Flow> {
        let _ = self.wait_reading_process_output(
            WaitRequest::sleep_for_duration_until_timer_deadline(duration, timer_deadline),
        )?;
        Ok(())
    }

    pub(crate) fn wait_for_resize_ack_until(&mut self, deadline: Instant) -> Result<bool, Flow> {
        let completion = self.wait_reading_process_output(WaitRequest::resize_ack(deadline))?;
        Ok(completion == WaitCompletion::SpecialInputActivity)
    }

    pub(crate) fn wait_for_command_input(
        &mut self,
        deadline: Option<Instant>,
    ) -> Result<CommandInputWaitOutcome, Flow> {
        let request = if let Some(deadline) = deadline {
            if deadline <= Instant::now() {
                return Ok(CommandInputWaitOutcome::DeadlineElapsed);
            }
            WaitRequest::read_command_input_until(deadline)
        } else {
            WaitRequest::read_command_input_forever()
        };
        self.wait_reading_process_output(request)
            .map(CommandInputWaitOutcome::from_completion)
    }

    fn block_for_wait_request(
        &mut self,
        request: &WaitRequest,
        wait_time: Duration,
    ) -> Result<WaitBlockActivity, Flow> {
        match self.wait_block(request, wait_time) {
            WaitBlock::Poll => Ok(WaitBlockActivity::poll()),
            WaitBlock::Poller(interest) => {
                // Mirror GNU: while blocked reading a command key,
                // `waiting-for-user-input-p` must report t. The channel path
                // sets this itself (see `wait_for_next_host_input_event`); the
                // poller path must set it explicitly around the block.
                let restore = self.begin_waiting_for_user_input_if_requested(request);
                let events = self
                    .processes
                    .wait_for_backend_events(wait_time, interest)
                    .unwrap_or_default();
                self.end_waiting_for_user_input(restore);
                Ok(WaitBlockActivity::from_source_events(events))
            }
            WaitBlock::HostInputChannel => {
                let _ = self.wait_for_next_host_input_event(
                    wait_time,
                    request.sets_waiting_for_user_input(),
                )?;
                Ok(WaitBlockActivity::poll())
            }
            WaitBlock::ProcessOutputSleep => {
                let events = self.processes.wait_for_process_events(wait_time);
                Ok(WaitBlockActivity::from_source_events(events))
            }
            WaitBlock::Sleep => {
                std::thread::sleep(wait_time);
                Ok(WaitBlockActivity::ready_processes(Vec::new()))
            }
        }
    }

    /// Set `waiting_for_user_input` for the duration of a poller block when the
    /// request reads a command key, returning the previous value to restore.
    fn begin_waiting_for_user_input_if_requested(&mut self, request: &WaitRequest) -> Option<bool> {
        if request.sets_waiting_for_user_input() {
            let previous = self.waiting_for_user_input();
            self.set_waiting_for_user_input(true);
            Some(previous)
        } else {
            None
        }
    }

    fn end_waiting_for_user_input(&mut self, restore: Option<bool>) {
        if let Some(previous) = restore {
            self.set_waiting_for_user_input(previous);
        }
    }

    fn next_gnu_timer_timeout(
        &self,
        timer_deadline: Option<GnuTimerTimestamp>,
    ) -> Option<Duration> {
        let ordinary = self.next_ordinary_gnu_timer_timeout_before(timer_deadline);
        let idle = self.next_idle_gnu_timer_timeout();

        match (ordinary, idle) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(timeout), None) | (None, Some(timeout)) => Some(timeout),
            (None, None) => None,
        }
    }

    fn next_wait_request_timeout(&self, request: &WaitRequest, now: Instant) -> WaitTimeoutChoice {
        let base_timeout = request.base_timeout(now);
        let mut timeout = base_timeout;
        let mut shortened_by_timer = false;

        if request.runs_timers() {
            if let Some(next) = self.next_gnu_timer_timeout(request.timer_deadline())
                && next < timeout
            {
                timeout = next;
                shortened_by_timer = true;
            }

            if !self.processes.live_process_ids().is_empty() {
                timeout = timeout.min(Duration::from_millis(100));
            }
        }
        if request.services_process_output()
            && request.target_process().is_none()
            && let Some(next) = self.processes.adaptive_read_timeout()
        {
            timeout = timeout.min(next);
        }

        WaitTimeoutChoice {
            duration: timeout,
            finite_deadline_timeout: request.deadline_is_finite() && timeout == base_timeout,
            shortened_by_timer,
        }
    }

    /// Choose how to block for one wait-loop iteration. See [`WaitBlock`].
    ///
    /// With a poller present (the normal case) this is GNU's single `pselect`:
    /// we accept input notifications whenever the request reads host input and
    /// the process fds whenever it services process output. The one wrinkle is
    /// `(host input, no process output)` while process fds are live: the poller
    /// is level-triggered, so polling readable-but-unserviced process fds with
    /// no interest would spin — there we block on the input channel instead.
    fn wait_block(&self, request: &WaitRequest, wait_time: Duration) -> WaitBlock {
        if wait_time.is_zero() {
            return WaitBlock::Poll;
        }

        let wants_input = request.waits_for_host_input();
        let wants_processes = request.services_process_output();

        // An explicitly installed host backend owns input suspension. This is
        // how a browser Worker replaces an OS poller with JSPI/Atomics while
        // preserving the same typed input channel. Native sessions do not
        // install one, so their existing unified poller selection is unchanged.
        if wants_input && self.has_host_input_wait_backend() {
            return WaitBlock::HostInputChannel;
        }

        if self.processes.has_wait_notification_backend() {
            use ProcessWaitBackendInterest::{
                NotificationsAndProcesses, NotificationsOnly, ProcessesOnly,
            };
            return match (wants_input, wants_processes) {
                (true, true) => WaitBlock::Poller(NotificationsAndProcesses),
                (true, false) => {
                    if self.processes.live_process_ids().is_empty() {
                        WaitBlock::Poller(NotificationsOnly)
                    } else if self.input_rx.is_some() {
                        // Live process fds we won't service → channel, no spin.
                        WaitBlock::HostInputChannel
                    } else {
                        // No channel to drain; watch the live process fds.
                        WaitBlock::Poller(ProcessesOnly)
                    }
                }
                // Process output, or a pure timeout (sleep-for / sit-for timer
                // service): register notification interest too, so a cross-thread
                // `Poller::notify()` returns the block. For example, the input
                // bridge raises it on C-g and also sets `quit_requested`; the wait loop
                // then re-iterates and `maybe_quit()` raises Quit within one
                // iteration. This is neomacs' analogue of GNU's SIGINT
                // interrupting `pselect` inside `sleep-for` (read_kbd == 0): the
                // wait does not COMPLETE on the staged key (sleep-for's
                // `completes_on_command_input()` is false), it only wakes to
                // re-check the quit flag, and the key is reconciled later by the
                // command loop.
                (false, _) => WaitBlock::Poller(NotificationsAndProcesses),
            };
        }

        // No poller (creation failed / headless): explicit degraded fallbacks.
        if wants_input && self.input_rx.is_some() {
            WaitBlock::HostInputChannel
        } else if wants_processes {
            WaitBlock::ProcessOutputSleep
        } else {
            WaitBlock::Sleep
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_process_activity_implies_any_process_activity() {
        let mut process = ProcessOutputServiceOutcome::default();
        let mut outcome = WaitServiceOutcome::default();

        process.record_activity(true);
        outcome.absorb_process_activity(process);

        assert!(outcome.has_target_process_activity());
        assert!(outcome.has_any_process_activity());
    }

    #[test]
    fn resize_special_input_activity_implies_any_special_input_activity() {
        let mut outcome = WaitServiceOutcome::default();

        outcome.absorb_special_input_activity(SpecialInputServiceOutcome::resize_with_redisplay());

        assert!(outcome.has_resize_activity());
        assert!(outcome.has_special_input_activity());
    }

    #[test]
    fn special_input_outcome_constructs_resize_activity_explicitly() {
        let outcome = SpecialInputServiceOutcome::resize_with_redisplay();

        assert!(outcome.has_resize_activity());
        assert!(outcome.has_any_activity());
        assert!(outcome.redisplay_needed());
    }

    #[test]
    fn special_input_outcome_merges_activity_and_redisplay() {
        let outcome = SpecialInputServiceOutcome::any_activity()
            .merge(SpecialInputServiceOutcome::resize_with_redisplay());

        assert!(outcome.has_resize_activity());
        assert!(outcome.has_any_activity());
        assert!(outcome.redisplay_needed());
    }

    #[test]
    fn command_input_pending_is_recorded_explicitly() {
        let mut outcome = WaitServiceOutcome::default();

        outcome.record_command_input_pending();

        assert!(outcome.has_command_input_pending());
    }

    #[test]
    fn timer_activity_is_recorded_explicitly() {
        let mut outcome = WaitServiceOutcome::default();

        outcome.record_timer_activity(true);

        assert!(outcome.has_timer_activity());
    }

    fn gnu_timer_vector_at(deadline: GnuTimerTimestamp) -> crate::emacs_core::value::Value {
        use crate::emacs_core::value::Value;

        Value::vector(vec![
            Value::NIL,
            Value::fixnum(deadline.high_seconds),
            Value::fixnum(deadline.low_seconds),
            Value::fixnum(deadline.usecs),
            Value::NIL,
            Value::symbol("ignore"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(deadline.psecs),
            Value::NIL,
        ])
    }

    #[test]
    fn ordinary_timer_deadline_filter_excludes_boundary_timer() {
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::value::Value;

        let now = GnuTimerTimestamp::now();
        let before_deadline = now.add_duration(Duration::from_millis(10));
        let deadline = now.add_duration(Duration::from_millis(20));
        let mut context = Context::new();

        context.set_variable(
            "timer-list",
            Value::list(vec![gnu_timer_vector_at(deadline)]),
        );
        assert_eq!(
            context.next_ordinary_gnu_timer_timeout_before(Some(deadline)),
            None
        );

        context.set_variable(
            "timer-list",
            Value::list(vec![gnu_timer_vector_at(before_deadline)]),
        );
        assert!(
            context
                .next_ordinary_gnu_timer_timeout_before(Some(deadline))
                .is_some()
        );
    }

    #[test]
    fn source_events_construct_notification_wakeup_explicitly() {
        let events = ProcessWaitEvents::notification_wakeup();

        assert!(events.has_notification_wakeup());
        assert!(!events.has_ready_processes());
    }

    #[test]
    fn source_events_construct_ready_processes_explicitly() {
        let events = ProcessWaitEvents::ready_processes(vec![7]);

        assert!(!events.has_notification_wakeup());
        assert!(events.has_ready_process(7));
    }

    #[test]
    fn source_events_query_individual_ready_processes() {
        let events = ProcessWaitEvents::ready_processes(vec![7]);

        assert!(events.has_ready_process(7));
        assert!(!events.has_ready_process(8));
    }

    #[test]
    fn source_events_empty_query_reflects_recorded_activity() {
        let empty = ProcessWaitEvents::default();
        let ready = ProcessWaitEvents::ready_processes(vec![7]);

        assert!(empty.is_empty());
        assert!(!ready.is_empty());
    }

    #[test]
    fn source_events_convert_to_process_service() {
        let events = ProcessWaitEvents::ready_processes(vec![7]);
        let activity = WaitBlockActivity::from_source_events(events);

        assert_eq!(
            activity.into_process_service(),
            WaitProcessService::Ready(ProcessWaitEvents::ready_processes(vec![7]))
        );
    }

    #[test]
    fn empty_source_events_poll_all_processes() {
        let activity = WaitBlockActivity::from_source_events(ProcessWaitEvents::default());

        assert!(!activity.has_notification_wakeup());
        assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
    }

    #[test]
    fn notification_only_source_events_poll_all_processes() {
        let activity =
            WaitBlockActivity::from_source_events(ProcessWaitEvents::notification_wakeup());

        assert!(activity.has_notification_wakeup());
        assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
    }

    #[test]
    fn block_activity_from_source_events_preserves_wakeup_and_processes() {
        let events = ProcessWaitEvents::from_sources(true, vec![3]);

        let activity = WaitBlockActivity::from_source_events(events);

        assert!(activity.has_notification_wakeup());
        assert_eq!(
            activity.into_process_service(),
            WaitProcessService::Ready(ProcessWaitEvents::from_sources(true, vec![3]))
        );
    }

    #[test]
    fn block_activity_from_ready_processes_has_no_notification_wakeup() {
        let activity = WaitBlockActivity::ready_processes(vec![4, 9]);

        assert!(!activity.has_notification_wakeup());
        assert!(activity.has_external_activity());
        assert_eq!(
            activity.into_process_service(),
            WaitProcessService::Ready(ProcessWaitEvents::ready_processes(vec![4, 9]))
        );
    }

    #[test]
    fn context_services_source_events_directly() {
        let mut context = crate::emacs_core::eval::Context::new();
        let request = WaitRequest::service_once(false);

        let outcome = context
            .service_wait_request_source_events_outcome(&request, ProcessWaitEvents::default())
            .expect("service source events");

        assert!(!outcome.has_command_input_pending());
        assert!(!outcome.has_any_process_activity());
    }

    #[test]
    fn block_for_wait_request_zero_timeout_returns_poll_activity() {
        let mut context = crate::emacs_core::eval::Context::new();
        let request = WaitRequest::service_once(false);

        let activity = context
            .block_for_wait_request(&request, Duration::ZERO)
            .expect("block for wait request");

        assert!(!activity.has_notification_wakeup());
        assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
    }

    #[test]
    fn deadline_timeout_without_activity_suppresses_timers_after_block() {
        let timeout = WaitTimeoutChoice {
            duration: Duration::from_millis(20),
            finite_deadline_timeout: true,
            shortened_by_timer: false,
        };
        let activity = WaitBlockActivity::poll();

        assert!(!timeout.run_timers_after_block(&activity, false));
    }

    #[test]
    fn timer_shortened_timeout_runs_timers_after_block_before_deadline() {
        let timeout = WaitTimeoutChoice {
            duration: Duration::from_millis(1),
            finite_deadline_timeout: false,
            shortened_by_timer: true,
        };
        let activity = WaitBlockActivity::poll();

        assert!(timeout.run_timers_after_block(&activity, false));
        assert!(!timeout.run_timers_after_block(&activity, true));
    }

    #[test]
    fn external_activity_before_deadline_allows_timers_after_block() {
        let timeout = WaitTimeoutChoice {
            duration: Duration::from_millis(20),
            finite_deadline_timeout: true,
            shortened_by_timer: false,
        };
        let activity = WaitBlockActivity::ready_processes(vec![1]);

        assert!(timeout.run_timers_after_block(&activity, false));
    }

    #[test]
    fn wait_request_exposes_deadline_and_process_completion_queries() {
        let request = WaitRequest::accept_target_process_output_with_timers(
            ProcessOutputWaitTiming::Poll,
            12,
            false,
        );

        assert_eq!(request.deadline(), WaitDeadline::Poll);
        assert_eq!(request.target_process(), Some(12));
        assert!(request.completes_on_target_process_activity(12));
        assert!(!request.completes_on_any_process_activity());
        assert!(!request.restricts_process_service_to_target());
    }

    #[test]
    fn wait_request_accept_process_output_constructors_capture_timer_policy() {
        let run = WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::Poll);
        let suppress =
            WaitRequest::accept_any_process_output_without_timers(ProcessOutputWaitTiming::Poll);

        assert!(run.runs_timers());
        assert!(!suppress.runs_timers());
    }

    #[test]
    fn wait_request_accept_process_output_named_constructors_capture_process_scope() {
        let any = WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::Poll);
        let target = WaitRequest::accept_target_process_output_with_timers(
            ProcessOutputWaitTiming::Poll,
            7,
            false,
        );
        let target_only = WaitRequest::accept_target_process_output_without_timers(
            ProcessOutputWaitTiming::Forever,
            9,
            true,
        );

        assert!(any.completes_on_any_process_activity());
        assert_eq!(any.target_process(), None);
        assert!(target.completes_on_target_process_activity(7));
        assert!(!target.restricts_process_service_to_target());
        assert!(target_only.completes_on_target_process_activity(9));
        assert!(target_only.restricts_process_service_to_target());
        assert!(!target_only.runs_timers());
        assert!(target_only.deadline_is_forever());
    }

    #[test]
    fn wait_request_process_output_timing_converts_duration_to_finite_deadline() {
        let request = WaitRequest::accept_any_process_output_with_timers(
            ProcessOutputWaitTiming::For(Duration::from_millis(5)),
        );

        assert!(request.deadline_is_finite());
    }

    #[test]
    fn wait_request_timer_service_suppresses_special_input_and_processes() {
        let request = WaitRequest::timer_service(true);

        assert_eq!(request.deadline(), WaitDeadline::Poll);
        assert_eq!(request.target_process(), None);
        assert!(!request.completes_on_any_process_activity());
        assert!(!request.services_special_input());
    }

    #[test]
    fn wait_request_input_pending_constructors_capture_timer_policy() {
        let suppress = WaitRequest::input_pending_without_timers();
        let run = WaitRequest::input_pending_with_timers();

        assert!(!suppress.runs_timers());
        assert!(run.runs_timers());
    }

    #[test]
    fn wait_request_exposes_scheduler_queries() {
        let now = Instant::now();
        let read = WaitRequest::read_command_input_until(now + Duration::from_secs(1));
        let poll = WaitRequest::service_once(true);
        let resize = WaitRequest::resize_ack(now);

        assert!(read.waits_for_host_input());
        assert!(read.completes_on_command_input());
        assert!(read.sets_waiting_for_user_input());
        assert!(read.runs_timers());
        assert!(!read.poll_or_deadline_elapsed(now));
        assert_eq!(read.base_timeout(now), Duration::from_secs(1));
        assert_eq!(
            read.base_timeout(now + Duration::from_secs(2)),
            Duration::ZERO
        );

        assert!(!poll.waits_for_host_input());
        assert!(!poll.completes_on_command_input());
        assert!(poll.poll_or_deadline_elapsed(now));

        assert!(resize.waits_for_host_input());
        assert!(!resize.runs_timers());
    }

    #[test]
    fn wait_request_redisplay_query_tracks_request_and_activity() {
        let redisplay = WaitRequest::service_once(true);
        let quiet = WaitRequest::service_once(false);
        let mut special = SpecialInputServiceOutcome::default();
        let mut service = WaitServiceOutcome::default();

        assert!(!redisplay.needs_redisplay_after_service(special, service));

        special = SpecialInputServiceOutcome::resize_with_redisplay();
        assert!(redisplay.needs_redisplay_after_service(special, service));
        assert!(!quiet.needs_redisplay_after_service(special, service));

        special = SpecialInputServiceOutcome::from_internal_effects(
            crate::frontend_events::InternalEventEffects {
                redisplay_needed: true,
            },
        );
        assert!(
            redisplay.needs_redisplay_after_service(special, service),
            "a late frontend report must repaint even when it is the only idle-wait activity"
        );

        special = SpecialInputServiceOutcome::default();
        service.record_timer_activity(true);
        assert!(redisplay.needs_redisplay_after_service(special, service));

        // A process filter that read output bytes may have changed a displayed
        // buffer (comint/async-shell output): redisplay after service.
        let mut output = WaitServiceOutcome::default();
        let mut output_process = ProcessOutputServiceOutcome::default();
        output_process.record_activity(false);
        output.absorb_process_activity(output_process);
        assert!(
            redisplay.needs_redisplay_after_service(SpecialInputServiceOutcome::default(), output)
        );
        // But output activity must NOT complete a command-input wait's redisplay
        // path via a quiet request.
        assert!(
            !quiet.needs_redisplay_after_service(SpecialInputServiceOutcome::default(), output)
        );

        // A sentinel/EOF that ran no output read still repaints (eww's
        // completion callback runs in url-http's EOF sentinel): the `serviced`
        // bit alone drives redisplay, even though it must never complete an
        // `accept-process-output` wait.
        let mut serviced = WaitServiceOutcome::default();
        let mut serviced_process = ProcessOutputServiceOutcome::default();
        serviced_process.record_serviced();
        serviced.absorb_process_activity(serviced_process);
        assert!(!serviced.has_any_process_activity());
        assert!(serviced.ran_process_callbacks());
        assert!(
            redisplay
                .needs_redisplay_after_service(SpecialInputServiceOutcome::default(), serviced)
        );
    }
}
