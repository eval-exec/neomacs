//! Wall-clock regrowth tripwire for `crates/neomacs-tui-tests`.
//!
//! # Enforcement and tripwire
//!
//! **The runtime floors are the enforcement, for the waits listed here.**
//! These APIs derive their deadline through `timing::HangCap::new`
//! (`>= MIN_HANG_CAP`, 5 s) or `timing::NegativeSoak::observe`
//! (`>= MIN_NEGATIVE_SOAK`, 1 s). It does not matter which expression
//! produced the `Duration`, so no spelling of a short duration gets past
//! them:
//!
//! - `HangCap::new`, `HangCap::from_secs`, `NegativeSoak::observe`.
//! - `HangCap::poll_slice` / `expect_poll_slice` (`<= MAX_POLL_SLICE`, below
//!   the floor, so a slice can never seed another cap).
//! - `TuiSession::read_until`, `read_until_screen`, `run_to_completion`,
//!   `wait_for_raw_output`.
//! - `pair::boot_pair`, `pair::wait_for_both`, `pair::CommandTrace::wait_for`,
//!   `pair::eval_expression_one_within` (through `read_until`).
//! - `package_scenario::PairTimeout::{same, per_editor}`.
//!
//! **These waits are NOT floored**, and accept any `Duration`:
//!
//! - `TuiSession::read(Duration)`. It is the idle-cutoff read; its clock code
//!   is allowlisted in `src/lib.rs` as `TuiSession::read`.
//! - `pair::read_both`, `pair::settle_session`,
//!   `package_scenario::PackageTuiPair::settle_both`, which wrap it.
//! - `neomacs_melpa_test_support::{output_with_timeout,
//!   group_output_with_timeout, output_with_timeout_pinned}` and
//!   `EmacsRuntime::with_timeout`, in another crate.
//! - `mpsc::Receiver::recv_timeout` and `Condvar::wait_timeout`.
//! - `poll(2)`, `select(2)` and `epoll_wait(2)` timeouts.
//!
//! A test can therefore still write `session.read(Duration::from_millis(200));
//! assert!(..)` with no panic and no finding. That is the fixed-window settle
//! shape, and review has to catch it.
//!
//! **This scan is the tripwire.** It keeps code from routing around the
//! floors, and it never tries to evaluate a duration. It denies by default:
//!
//! - **W1 discovery.** Every `*.rs` file under `src/`, `tests/`, `examples/`
//!   and `benches/` is scanned, found by walking the filesystem rather than
//!   by following `mod` declarations, and so is the build script (`build.rs`,
//!   or `package.build`). Every Cargo target `path` must lie under a scanned
//!   directory, and `package.build` must be a plain relative `.rs` path.
//! - **W2 parse.** Every file must lex and parse with `syn`. A file the scan
//!   cannot read is a violation, never a skip.
//! - **W3 escape.** These are absolute violations with no allowlist:
//!   - an attribute that contains a `path` identifier in any position
//!     (`#[cfg_attr(.., path = ..)]`, nested `cfg_attr`, `#![path]`,
//!     `#[path = concat!(..)]`, a literal containing escapes, `..`, an
//!     absolute path or an empty component). The one admitted spelling is
//!     `#[path = "<relative/descendant.rs>"]`: its target is always below
//!     the declaring file's directory, which the walk scans anyway;
//!   - an attribute that contains a `$` metavariable;
//!   - a `#` that does not open an attribute;
//!   - the identifiers `include`, `include_str`, `include_bytes` and
//!     `concat_idents`, in any position;
//!   - `macro` (declarative macros 2.0);
//!   - any `macro_rules!` definition outside [`MACRO_ALLOWLIST`].
//!
//!   Together these mean every compiled line of this crate is a file the
//!   walk scanned, and a macro cannot assemble a hidden attribute or
//!   identifier.
//! - **W4 clock.** The identifiers in [`CLOCK_IDENTS`], plus `now` called as
//!   `::now` or `.now`, are counted per `(file, fn)` scope. `impl Trait for
//!   Type` scopes are named `Type<Trait>`. The counts must equal
//!   [`CLOCK_ALLOWLIST`] exactly. A new scope, a changed count and a stale
//!   entry are all violations.
//! - **W5 sleep.** The identifiers in [`SLEEP_IDENTS`] (`std::thread` and
//!   async sleeps, `park_timeout`, and the libc `usleep`, `nanosleep` and
//!   `clock_nanosleep`), in any position, are counted per `(file, fn)`
//!   scope. There are two exceptions. A plain `use` import is not counted (a
//!   renamed import is a violation). And a sleep is not counted when its
//!   innermost enclosing `loop` or `while` also makes a `HangCap` poll in
//!   method-call syntax (`.is_expired(`, `.assert_not_expired(`,
//!   `.poll_slice(`, `.expect_poll_slice(`). `for` loops are never admitted:
//!   they are the fixed-count retry shape. Every other sleep needs a
//!   [`SLEEP_ALLOWLIST`] entry that gives each sleep a typed `SleepKind` and
//!   a reason. `SleepKind::BlindSettle` marks a fixed wait before an
//!   observation (the settle-flake shape). `SLEEP_PINS` pins, per crate,
//!   both the total of allowlisted sleeps and the `BlindSettle` count, so
//!   relabelling a new blind settle as another kind still needs a pin edit.
//!
//!   The cap-polling-loop admission is a **heuristic**, not a proof. It
//!   establishes that the sleep sits inside a `loop` or `while` that calls a
//!   `HangCap` poll method. It does not establish that the sleep is that
//!   loop's poll interval rather than a settle between a poll and an
//!   assertion, nor that the loop does not break right after the sleep.
//!   "Admitted" therefore does not mean "proven pacing". The compensating
//!   controls are human review of every new cap-polling loop and the exact
//!   `SLEEP_PINS` for everything the heuristic does not admit.
//!
//! # Not covered
//!
//! 1. **Other crates.** That includes their macros, their wait helpers (see
//!    the unfloored list above), and any clock or sleep they re-export under
//!    another name. Calls spelled `::now`/`.now`, `.elapsed` and `.sleep` are
//!    still counted whatever type they belong to.
//! 2. **Non-Rust inputs.** `.el` fixtures, inline Elisp strings, shell and
//!    Python helpers, and any file read at runtime.
//! 3. **Code generated outside the crate.** Proc macros from dependencies.
//!    Generated files cannot be compiled in, because `include!` is banned.
//! 4. **Time passing without a clock read, outside W5.** Iteration-count
//!    loops (including a caller counting its own `NegativeSoak` samples or
//!    poll slices), the unfloored waits listed above, and blocking syscalls
//!    with timeouts. W5 admits a sleep inside a `HangCap`-polling `loop` or
//!    `while` on structure alone, and does not check what that loop asserts
//!    afterwards, nor whether it breaks right after the sleep.
//! 5. **Clocks read through a subprocess or the filesystem.** `date`,
//!    `/proc/uptime`, and file mtimes compared without the identifiers above.
//! 6. **The scopes in [`CLOCK_ALLOWLIST`].** These are reviewed PTY transport
//!    and recording drivers. Each has its own reason, and its count is pinned
//!    exactly. Swapping one counted token for another inside one scope keeps
//!    the count and is not detected.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, LineColumn, TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

// ── Crate configuration ──────────────────────────────────────────────

/// Crate-specific identifiers counted like [`CLOCK_IDENTS`].
const CRATE_COUNTED_IDENTS: &[&str] = &[
    // The crate-private `Instant` accessor on `HangCap`.
    "deadline_instant",
];

/// Files under `tests/` that [`CLOCK_ALLOWLIST`] may name. Integration tests
/// in this crate never read the clock directly.
const INTEGRATION_TEST_CLOCK_FILES: &[&str] = &[];

/// String-literal fragments counted like clock identifiers. None in this
/// crate.
const COUNTED_LITERAL_FRAGMENTS: &[&str] = &[];

/// `macro_rules!` definitions allowed in this crate, as `(file, macro)`.
const MACRO_ALLOWLIST: &[(&str, &str)] = &[("src/lib.rs", "compare_field")];

/// The sibling guard whose shared engine must stay byte-identical.
const SIBLING_GUARD: &str = "../neomacs-gui-tests/src/wall_clock_guard_test.rs";

const CLOCK_ALLOWLIST: &[ScopeAllowance] = &[
    ScopeAllowance {
        file: "src/lib.rs",
        scope: "<items>",
        count: 2,
        reason: "`Instant` import and the `PtyReadProgress::Activity(Instant)` observation stamp",
    },
    ScopeAllowance {
        file: "src/lib.rs",
        scope: "TuiSession::read",
        count: 7,
        reason: "idle-cutoff read: compares PTY observation stamps; returns `()` and exposes no time",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "<items>",
        count: 3,
        reason: "imports and `Instant` fields of PTY write requests and output events",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "PtyOutputEvent::observed_at",
        count: 1,
        reason: "crate-private observation stamp consumed by `TuiSession::read`",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "Request::ensure_active",
        count: 2,
        reason: "expiry check of a crate-private PTY write request",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "PtyOutputPump::request",
        count: 2,
        reason: "fixed 10 s transport deadline for one PTY write request",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "write_all_nonblocking_pty_within",
        count: 1,
        reason: "converts a caller `HangCap` into the poll(2) deadline",
    },
    ScopeAllowance {
        file: "src/pty_output.rs",
        scope: "write_all_nonblocking_pty",
        count: 7,
        reason: "poll(2) millisecond timeout slicing for nonblocking PTY writes",
    },
    ScopeAllowance {
        file: "src/pty_output/owner.rs",
        scope: "PtyTransport::poll",
        count: 1,
        reason: "poll(2) takes its timeout in milliseconds",
    },
    ScopeAllowance {
        file: "src/pty_output/owner.rs",
        scope: "Owner::emit",
        count: 2,
        reason: "stamps PTY output events for the idle cutoff",
    },
    ScopeAllowance {
        file: "src/pty_output/owner.rs",
        scope: "Owner::drain_available",
        count: 3,
        reason: "bounded drain against a crate-private deadline",
    },
    ScopeAllowance {
        file: "src/pty_output/owner.rs",
        scope: "Owner::input",
        count: 5,
        reason: "poll(2) slicing for the owner thread write path",
    },
    ScopeAllowance {
        file: "src/pty_output/owner.rs",
        scope: "run_transport",
        count: 4,
        reason: "owner thread poll(2) loop deadlines",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "request",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "already_available_output_precedes_the_committed_input_fence",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "partial_write_then_drain_deadline_preserves_committed_prefix",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "write_all_nonblocking_pty_handles_short_writes_eagain_and_timeout",
        count: 8,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "an_expired_request_never_writes_or_completes_successfully",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "an_expired_drain_refuses_without_sampling_output_or_writing",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "observer_failure_cancels_without_erasing_an_already_committed_prefix",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "a_foreign_completion_is_rejected_before_the_observer_can_apply_it",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/pty_output/tests.rs",
        scope: "a_failed_completion_invalidates_later_requests_before_they_are_queued",
        count: 2,
        reason: "unit row driving PtyOutput deadline and expiry error paths with explicit Instants",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "<items>",
        count: 3,
        reason: "`Instant` import and recording start/flush fields",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "AsciicastV3Writer::write_event",
        count: 1,
        reason: "asciicast v3 relative event timestamps are seconds as f64",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::start",
        count: 2,
        reason: "asciicast recording epoch",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::write_at",
        count: 2,
        reason: "monotonic clamping of event timestamps",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::write",
        count: 2,
        reason: "stamps a recording event",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::flush_if_due",
        count: 1,
        reason: "periodic flush throttle",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::flush",
        count: 2,
        reason: "records the last flush time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::output_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "ActiveRecording::input_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::output_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::input_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::resize_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::marker_at",
        count: 1,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::output",
        count: 2,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::input",
        count: 2,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::resize",
        count: 2,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording.rs",
        scope: "SessionRecording::marker",
        count: 2,
        reason: "stamps a recording event at the call time",
    },
    ScopeAllowance {
        file: "src/recording/tests/recording_test.rs",
        scope: "recording_rejects_time_reversal_without_saturating_its_interval",
        count: 2,
        reason: "unit row passing explicit Instants to prove monotonic clamping",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "<items>",
        count: 6,
        reason: "the timing primitives' imports, fields and the MAX_POLL_SLICE const assertion",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "HangCap::new",
        count: 2,
        reason: "timing primitive owner",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "HangCap::is_expired",
        count: 2,
        reason: "timing primitive owner",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "HangCap::poll_slice",
        count: 3,
        reason: "timing primitive owner",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "HangCap::deadline_instant",
        count: 2,
        reason: "timing primitive owner",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "NegativeSoak::observe",
        count: 4,
        reason: "timing primitive owner",
    },
    ScopeAllowance {
        file: "src/timing.rs",
        scope: "fixed_file_mtime",
        count: 4,
        reason: "deterministic mtime fixture; never reads the clock",
    },
];

/// Scopes allowed a pinned number of sleeps that are not inside a
/// `HangCap`-polling loop. Each reason says why the sleep paces rather than
/// waits for an asserted condition.
const SLEEP_ALLOWLIST: &[SleepAllowance] = &[
    SleepAllowance {
        file: "src/lib.rs",
        scope: "TuiSession::send_keys",
        kinds: &[SleepKind::InputCadence],
        reason: "50 ms between the keys of one sequence; callers then wait on screen state",
    },
    SleepAllowance {
        file: "src/lib.rs",
        scope: "TuiSession::pace_recording",
        kinds: &[SleepKind::InputCadence],
        reason: "opt-in asciicast recording delay between committed inputs",
    },
    SleepAllowance {
        file: "src/pair.rs",
        scope: "send_both",
        kinds: &[SleepKind::InputCadence],
        reason: "50 ms between keys sent to both editors; callers then wait on screen state",
    },
    SleepAllowance {
        file: "src/pty_output/tests.rs",
        scope: "FakeTransport<Transport>::read",
        kinds: &[SleepKind::ModelledWork],
        reason: "the fake transport models a slow read for the owner-thread unit rows",
    },
    SleepAllowance {
        file: "src/tests/lib_test.rs",
        scope: "nonblocking_pty_output_fixture",
        kinds: &[SleepKind::ModelledWork],
        reason: "the fixture child paces its EAGAIN writes like GNU tty output",
    },
    SleepAllowance {
        file: "src/tests/lib_test.rs",
        scope: "tui_session_drains_nonblocking_output_between_client_observations",
        kinds: &[SleepKind::ModelledWork],
        reason: "models client work between observations; completion then waits on a 5 s cap",
    },
    SleepAllowance {
        file: "src/timing.rs",
        scope: "NegativeSoak::observe",
        kinds: &[SleepKind::PrimitivePacing],
        reason: "the soak's own sample interval",
    },
    SleepAllowance {
        file: "src/timing.rs",
        scope: "tests::shared_cap_keeps_serving_slices_below_the_floor",
        kinds: &[SleepKind::ModelledWork],
        reason: "consumes part of a cap so its remainder drops below the floor",
    },
    SleepAllowance {
        file: "tests/issue_621_mouse_drag_selection.rs",
        scope: "a_tty_mouse_drag_leaves_the_region_active_like_gnu",
        kinds: &[SleepKind::BlindSettle],
        reason: "300 ms for the fixture to return to the command loop before the drag bytes",
    },
    SleepAllowance {
        file: "tests/package_tui/helm_pydoc_test/harness.rs",
        scope: "assert_overlay_introspection",
        kinds: &[SleepKind::BlindSettle],
        reason: "3 s for the prelude's idle timer to rewrite the overlay dumps before they are read",
    },
    SleepAllowance {
        file: "tests/package_tui/helm_pydoc_test/mod.rs",
        scope: "overlay_face_renders_with_the_minibuffer_active",
        kinds: &[SleepKind::BlindSettle],
        reason: "500 ms after the minibuffer prompt appears, before the pairwise screen comparison",
    },
];

// ── Shared engine: byte-identical in neomacs-tui-tests and neomacs-gui-tests ──

/// The line that starts the shared engine in both guards.
const SHARED_ENGINE_MARKER: &str =
    "// ── Shared engine: byte-identical in neomacs-tui-tests and neomacs-gui-tests ──";

/// Identifiers that read, carry, or convert a clock value, in every crate.
const CLOCK_IDENTS: &[&str] = &[
    "Instant",
    "SystemTime",
    "UNIX_EPOCH",
    "clock_gettime",
    "clock_gettime64",
    "gettimeofday",
    "elapsed",
    "duration_since",
    "checked_duration_since",
    "saturating_duration_since",
    "as_millis",
    "as_micros",
    "as_nanos",
    "as_secs",
    "as_secs_f32",
    "as_secs_f64",
    "subsec_millis",
    "subsec_micros",
    "subsec_nanos",
];

/// Sleep identifiers: `std::thread` and async runtimes (`sleep`, `sleep_ms`,
/// `sleep_until`), thread parking with a timeout (`park_timeout`) and the
/// libc sleeps (`usleep`, `nanosleep`, `clock_nanosleep`). A sleep is "time
/// passing without a clock read": a fixed sleep followed by an assertion is
/// the settle-flake shape.
const SLEEP_IDENTS: &[&str] = &[
    "sleep",
    "sleep_ms",
    "sleep_until",
    "park_timeout",
    "usleep",
    "nanosleep",
    "clock_nanosleep",
];

/// `HangCap` polls, recognised only in method-call syntax
/// (`.is_expired(`, `.poll_slice::<..>(`). A sleep is admitted when its
/// innermost enclosing `loop` or `while` also makes one of these calls.
/// `for` loops are never admitted: in these crates `for` is the fixed-count
/// retry shape, not a cap-bounded poll.
const CAP_POLL_IDENTS: &[&str] = &[
    "is_expired",
    "assert_not_expired",
    "poll_slice",
    "expect_poll_slice",
];

/// Why an allowlisted sleep exists. Every kind except `BlindSettle` either
/// paces input, or paces something whose result is re-checked or never
/// asserted. `BlindSettle` is a fixed wait before an observation or
/// assertion: the settle-flake shape, kept visible as debt that may only
/// shrink (see [`SLEEP_PINS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SleepKind {
    /// Spaces injected input events; assertions wait on observed state.
    InputCadence,
    /// Poll interval of a loop or callback that re-checks its condition
    /// under a deadline, cap or fixed retry count.
    PolledRetry,
    /// Delays the first sample of a wait for the expected state CHANGE
    /// (a completion or receipt the following poll observes). A delay
    /// before an equality-stability poll is `BlindSettle`, because two equal
    /// samples taken before the change has started also look stable.
    PreSampleDelay,
    /// Spaces captures along a known animation ramp; exact values are
    /// asserted from the render thread's own log.
    SampleSpacing,
    /// Simulates work inside a fixture, a fake transport, or a unit row of a
    /// timing primitive.
    ModelledWork,
    /// A timing primitive's own poll interval.
    PrimitivePacing,
    /// Debt: a fixed wait before an observation or assertion.
    BlindSettle,
}

/// Pinned sleep inventory per crate: the total number of allowlisted sleeps
/// and the number of them that are `SleepKind::BlindSettle`. The table lives
/// in the shared engine, so changing a number edits both guards (the
/// identity row enforces that). Both numbers are compared by equality:
/// - a new allowlisted sleep of ANY kind without a raised total fails, so
///   relabelling a new blind settle as another kind still needs the
///   two-file pin edit;
/// - a new `BlindSettle` sleep without a raised blind pin fails;
/// - a raised pin without a new sleep fails;
/// - a converted or removed sleep without a lowered pin fails.
///
/// The blind pin is the debt and may only shrink. A coordinated raise (a new
/// entry plus bumped pins) cannot be forbidden by any in-repo row. It is a
/// visible two-file diff for review to refuse.
const SLEEP_PINS: &[(&str, SleepPin)] = &[
    (
        "neomacs-gui-tests",
        SleepPin {
            total: 41,
            blind_settle: 16,
        },
    ),
    (
        "neomacs-tui-tests",
        SleepPin {
            total: 11,
            blind_settle: 3,
        },
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SleepPin {
    total: usize,
    blind_settle: usize,
}

fn crate_sleep_pin() -> SleepPin {
    SLEEP_PINS
        .iter()
        .find(|(name, _)| *name == env!("CARGO_PKG_NAME"))
        .map(|(_, pin)| *pin)
        .expect("SLEEP_PINS names this crate")
}

/// The allowlist's sleep totals must equal the pin exactly, in both
/// directions.
fn check_sleep_pin(allowlist: &[SleepAllowance], pin: SleepPin) -> Result<(), String> {
    let tally = SleepKind::ALL.map(|kind| {
        let count = allowlist
            .iter()
            .flat_map(|entry| entry.kinds)
            .filter(|entry_kind| **entry_kind == kind)
            .count();
        (kind, count)
    });
    let observed = SleepPin {
        total: tally.iter().map(|(_, count)| count).sum(),
        blind_settle: tally
            .iter()
            .find(|(kind, _)| *kind == SleepKind::BlindSettle)
            .map_or(0, |(_, count)| *count),
    };
    if observed == pin {
        return Ok(());
    }
    Err(format!(
        "allowlisted sleeps {observed:?} but SLEEP_PINS pins {pin:?}. The BlindSettle debt may \
         only shrink: lower the pins when a sleep becomes a state wait; a raise needs a reviewed \
         reason. Tally: {tally:?}"
    ))
}

impl SleepKind {
    const ALL: [Self; 7] = [
        Self::InputCadence,
        Self::PolledRetry,
        Self::PreSampleDelay,
        Self::SampleSpacing,
        Self::ModelledWork,
        Self::PrimitivePacing,
        Self::BlindSettle,
    ];
}

/// One `(file, fn)` scope allowed sleeps outside a `HangCap`-polling loop,
/// one [`SleepKind`] per counted sleep in source order.
struct SleepAllowance {
    file: &'static str,
    scope: &'static str,
    kinds: &'static [SleepKind],
    reason: &'static str,
}

/// One `(file, fn)` scope allowed a pinned number of counted tokens.
struct ScopeAllowance {
    file: &'static str,
    /// Qualified scope: inline `mod`s, `impl` self type or trait, then the
    /// `fn` chain. `<items>` collects tokens outside every `fn` (imports,
    /// fields, consts).
    scope: &'static str,
    /// Exact number of counted tokens in the scope.
    count: usize,
    reason: &'static str,
}

// ── Findings ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Finding {
    file: String,
    line: usize,
    rule: &'static str,
    detail: String,
}

fn finding(file: &str, line: usize, rule: &'static str, detail: impl Into<String>) -> Finding {
    Finding {
        file: file.to_owned(),
        line,
        rule,
        detail: detail.into(),
    }
}

fn render(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("  {}:{} [{}] {}", f.file, f.line, f.rule, f.detail))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The configuration a scan checks against; tests substitute their own.
struct Policy<'a> {
    clock_allowlist: &'a [ScopeAllowance],
    sleep_allowlist: &'a [SleepAllowance],
    macro_allowlist: &'a [(&'a str, &'a str)],
}

const CRATE_POLICY: Policy<'static> = Policy {
    clock_allowlist: CLOCK_ALLOWLIST,
    sleep_allowlist: SLEEP_ALLOWLIST,
    macro_allowlist: MACRO_ALLOWLIST,
};

// ── W1: discovery ────────────────────────────────────────────────────

/// Every directory Cargo can compile Rust from by convention. Each is
/// walked whether or not it exists today, so a new `examples/` or
/// `benches/` file is scanned the moment it is added.
const GOVERNED_DIRS: &[&str] = &["src", "tests", "examples", "benches"];

/// Every `*.rs` file under the governed directories, plus the build script,
/// keyed by its path relative to `root`. Symlinks are followed; a directory
/// reached twice is walked once.
fn discover_sources(root: &Path, manifest: &str) -> (BTreeMap<String, String>, Vec<Finding>) {
    let mut sources = BTreeMap::new();
    let mut findings = Vec::new();
    let mut visited = BTreeSet::new();
    for dir in GOVERNED_DIRS {
        let dir = root.join(dir);
        if dir.exists() {
            walk(root, &dir, &mut visited, &mut sources, &mut findings);
        }
    }
    for script in build_scripts(manifest) {
        let path = root.join(&script);
        match fs::read_to_string(&path) {
            Ok(text) => {
                sources.insert(script, text);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => findings.push(finding(&script, 0, "W1-unreadable", error.to_string())),
        }
    }
    (sources, findings)
}

/// The build script Cargo would run: `package.build` when it names a path,
/// none when it is `false`, `build.rs` otherwise.
fn build_scripts(manifest: &str) -> Vec<String> {
    let build = toml::from_str::<toml::Table>(manifest)
        .ok()
        .and_then(|table| table.get("package")?.get("build").cloned());
    match build {
        Some(toml::Value::Boolean(false)) => Vec::new(),
        Some(toml::Value::String(path)) => vec![path],
        _ => vec!["build.rs".to_owned()],
    }
}

fn walk(
    root: &Path,
    dir: &Path,
    visited: &mut BTreeSet<PathBuf>,
    sources: &mut BTreeMap<String, String>,
    findings: &mut Vec<Finding>,
) {
    let rel_dir = relative(root, dir);
    let Ok(canonical) = fs::canonicalize(dir) else {
        findings.push(finding(
            &rel_dir,
            0,
            "W1-unreadable",
            "cannot resolve directory",
        ));
        return;
    };
    if !visited.insert(canonical) {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            findings.push(finding(&rel_dir, 0, "W1-unreadable", error.to_string()));
            return;
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(error) => findings.push(finding(&rel_dir, 0, "W1-unreadable", error.to_string())),
        }
    }
    paths.sort();
    for path in paths {
        let rel = relative(root, &path);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => walk(root, &path, visited, sources, findings),
            Ok(_) if path.extension().is_some_and(|ext| ext == "rs") => {
                match fs::read_to_string(&path) {
                    Ok(text) => {
                        sources.insert(rel, text);
                    }
                    Err(error) => {
                        findings.push(finding(&rel, 0, "W1-unreadable", error.to_string()))
                    }
                }
            }
            Ok(_) => {}
            Err(error) => findings.push(finding(&rel, 0, "W1-unreadable", error.to_string())),
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every Cargo target path must lie under a governed directory, and the
/// build script must be a plain relative path (it is scanned by
/// `discover_sources`). A target with no `path` uses Cargo's inferred
/// location, which is always under a governed directory.
fn check_manifest(manifest: &str) -> Vec<Finding> {
    const FILE: &str = "Cargo.toml";
    let table = match toml::from_str::<toml::Table>(manifest) {
        Ok(table) => table,
        Err(error) => {
            return vec![finding(
                FILE,
                0,
                "W1-manifest-unparseable",
                error.to_string(),
            )];
        }
    };
    let mut findings = Vec::new();
    let check_target = |kind: &str, target: &toml::Value, findings: &mut Vec<Finding>| {
        let Some(target) = target.as_table() else {
            findings.push(finding(
                FILE,
                0,
                "W1-manifest-unanalysable",
                format!("`{kind}` target is not a table"),
            ));
            return;
        };
        match target.get("path") {
            None => {}
            Some(toml::Value::String(path)) if governed_target_path(path) => {}
            Some(path) => findings.push(finding(
                FILE,
                0,
                "W1-target-outside-governed-tree",
                format!("`{kind}` target path {path} is not under a scanned directory"),
            )),
        }
    };
    match table
        .get("package")
        .and_then(|package| package.get("build"))
    {
        None => {}
        Some(toml::Value::Boolean(_)) => {}
        Some(toml::Value::String(path)) if plain_relative_rs_path(path) => {}
        Some(build) => findings.push(finding(
            FILE,
            0,
            "W1-build-script-unanalysable",
            format!("`package.build = {build}` is not a plain relative .rs path"),
        )),
    }
    if let Some(lib) = table.get("lib") {
        check_target("lib", lib, &mut findings);
    }
    for kind in ["test", "bin", "bench", "example"] {
        match table.get(kind) {
            None => {}
            Some(toml::Value::Array(targets)) => {
                for target in targets {
                    check_target(kind, target, &mut findings);
                }
            }
            Some(_) => findings.push(finding(
                FILE,
                0,
                "W1-manifest-unanalysable",
                format!("`{kind}` is not an array of tables"),
            )),
        }
    }
    findings
}

fn governed_target_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    GOVERNED_DIRS
        .iter()
        .any(|dir| normalized.starts_with(&format!("{dir}/")))
        && plain_relative_rs_path(&normalized)
}

fn plain_relative_rs_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && path.ends_with(".rs")
        && !path.split('/').any(|part| part == ".." || part.is_empty())
}

// ── W2: parse and scope attribution ──────────────────────────────────

/// `fn` scopes of one file: `(start, end, qualified name)`, plus the
/// spans of every loop expression and `use` item.
struct Scopes {
    ranges: Vec<(LineColumn, LineColumn, String)>,
    loops: Vec<(LineColumn, LineColumn)>,
    uses: Vec<(LineColumn, LineColumn)>,
}

impl Scopes {
    fn collect(file: &syn::File) -> Self {
        let mut collector = ScopeCollector {
            stack: Vec::new(),
            ranges: Vec::new(),
            loops: Vec::new(),
            uses: Vec::new(),
        };
        collector.visit_file(file);
        Self {
            ranges: collector.ranges,
            loops: collector.loops,
            uses: collector.uses,
        }
    }

    fn in_use_item(&self, at: LineColumn) -> bool {
        self.uses.iter().any(|span| contains(*span, at))
    }

    /// The innermost loop expression containing `at`.
    fn innermost_loop(&self, at: LineColumn) -> Option<(LineColumn, LineColumn)> {
        self.loops
            .iter()
            .copied()
            .filter(|span| contains(*span, at))
            .max_by_key(|(start, _)| position(*start))
    }

    /// The innermost `fn` containing `at`, or `<items>`.
    fn name_at(&self, at: LineColumn) -> String {
        self.ranges
            .iter()
            .filter(|(start, end, _)| {
                position(*start) <= position(at) && position(at) < position(*end)
            })
            .max_by_key(|(start, _, _)| position(*start))
            .map_or_else(|| "<items>".to_owned(), |(_, _, name)| name.clone())
    }
}

fn position(at: LineColumn) -> (usize, usize) {
    (at.line, at.column)
}

fn contains((start, end): (LineColumn, LineColumn), at: LineColumn) -> bool {
    position(start) <= position(at) && position(at) < position(end)
}

struct ScopeCollector {
    stack: Vec<String>,
    ranges: Vec<(LineColumn, LineColumn, String)>,
    loops: Vec<(LineColumn, LineColumn)>,
    uses: Vec<(LineColumn, LineColumn)>,
}

impl ScopeCollector {
    fn nested(&mut self, segment: String, inner: impl FnOnce(&mut Self)) {
        self.stack.push(segment);
        inner(self);
        self.stack.pop();
    }

    fn function(
        &mut self,
        ident: &syn::Ident,
        span: proc_macro2::Span,
        inner: impl FnOnce(&mut Self),
    ) {
        self.stack.push(ident.to_string());
        self.ranges
            .push((span.start(), span.end(), self.stack.join("::")));
        inner(self);
        self.stack.pop();
    }
}

fn type_label(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map_or_else(|| "<impl>".to_owned(), |segment| segment.ident.to_string()),
        _ => "<impl>".to_owned(),
    }
}

impl<'ast> Visit<'ast> for ScopeCollector {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.nested(item.ident.to_string(), |this| {
            visit::visit_item_mod(this, item)
        });
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        // `impl Trait for Type` scopes are `Type<Trait>`, so two trait impls
        // on one type never share an allowlist bucket.
        let label = match &item.trait_ {
            Some((path, _)) => format!(
                "{}<{}>",
                type_label(&item.self_ty),
                path.segments
                    .last()
                    .map_or_else(|| "?".to_owned(), |segment| segment.ident.to_string())
            ),
            None => type_label(&item.self_ty),
        };
        self.nested(label, |this| visit::visit_item_impl(this, item));
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.nested(item.ident.to_string(), |this| {
            visit::visit_item_trait(this, item)
        });
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(&item.sig.ident, item.span(), |this| {
            visit::visit_item_fn(this, item)
        });
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.function(&item.sig.ident, item.span(), |this| {
            visit::visit_impl_item_fn(this, item)
        });
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.function(&item.sig.ident, item.span(), |this| {
            visit::visit_trait_item_fn(this, item)
        });
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let span = item.span();
        self.uses.push((span.start(), span.end()));
        visit::visit_item_use(self, item);
    }

    fn visit_expr_loop(&mut self, expr: &'ast syn::ExprLoop) {
        let span = expr.span();
        self.loops.push((span.start(), span.end()));
        visit::visit_expr_loop(self, expr);
    }

    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        let span = expr.span();
        self.loops.push((span.start(), span.end()));
        visit::visit_expr_while(self, expr);
    }
}

// ── W3/W4: token rules ───────────────────────────────────────────────

/// A token stream flattened so neighbouring tokens across group boundaries
/// can be inspected.
#[derive(Debug)]
enum Flat {
    Ident(String, LineColumn),
    Punct(char, LineColumn),
    Literal(String, LineColumn),
    Open(Delimiter),
    Close,
}

fn flatten(stream: TokenStream, out: &mut Vec<Flat>) {
    for tree in stream {
        match tree {
            TokenTree::Group(group) => {
                out.push(Flat::Open(group.delimiter()));
                flatten(group.stream(), out);
                out.push(Flat::Close);
            }
            TokenTree::Ident(ident) => {
                let text = ident.to_string();
                let text = text.strip_prefix("r#").map_or(text.clone(), str::to_owned);
                out.push(Flat::Ident(text, ident.span().start()));
            }
            TokenTree::Punct(punct) => out.push(Flat::Punct(punct.as_char(), punct.span().start())),
            TokenTree::Literal(literal) => {
                out.push(Flat::Literal(literal.to_string(), literal.span().start()))
            }
        }
    }
}

fn matching_close(tokens: &[Flat], open: usize) -> usize {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token {
            Flat::Open(..) => depth += 1,
            Flat::Close => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

const FILE_INCLUSION_IDENTS: &[&str] =
    &["include", "include_str", "include_bytes", "concat_idents"];

/// Counted clock tokens of one file: `(scope, token, line)`.
type ClockHits = Vec<(String, String, usize)>;

struct FileScan {
    findings: Vec<Finding>,
    clock_hits: ClockHits,
    sleep_hits: ClockHits,
    macros: Vec<(String, usize)>,
}

fn scan_file(rel: &str, source: &str) -> FileScan {
    let mut scan = FileScan {
        findings: Vec::new(),
        clock_hits: Vec::new(),
        sleep_hits: Vec::new(),
        macros: Vec::new(),
    };
    let stream: TokenStream = match source.parse() {
        Ok(stream) => stream,
        Err(error) => {
            let line = error.span().start().line;
            scan.findings
                .push(finding(rel, line, "W2-unlexable", error.to_string()));
            return scan;
        }
    };
    let scopes = match syn::parse_file(source) {
        Ok(file) => Scopes::collect(&file),
        Err(error) => {
            let line = error.span().start().line;
            scan.findings
                .push(finding(rel, line, "W2-unparseable", error.to_string()));
            return scan;
        }
    };
    let mut tokens = Vec::new();
    flatten(stream, &mut tokens);
    let cap_polls: Vec<LineColumn> = (0..tokens.len())
        .filter_map(|index| match &tokens[index] {
            Flat::Ident(name, at)
                if CAP_POLL_IDENTS.contains(&name.as_str()) && is_method_call(&tokens, index) =>
            {
                Some(*at)
            }
            _ => None,
        })
        .collect();

    for index in 0..tokens.len() {
        match &tokens[index] {
            Flat::Punct('#', at) => check_attribute(rel, &tokens, index, *at, &mut scan.findings),
            Flat::Ident(name, at) => {
                if FILE_INCLUSION_IDENTS.contains(&name.as_str()) {
                    scan.findings.push(finding(
                        rel,
                        at.line,
                        "W3-file-inclusion",
                        format!("`{name}` compiles or embeds a file this scan cannot follow"),
                    ));
                }
                if name == "macro" {
                    scan.findings.push(finding(
                        rel,
                        at.line,
                        "W3-macro-definition",
                        "`macro` definitions can assemble hidden attributes or identifiers",
                    ));
                }
                if name == "macro_rules" {
                    match (tokens.get(index + 1), tokens.get(index + 2)) {
                        (Some(Flat::Punct('!', _)), Some(Flat::Ident(macro_name, _))) => {
                            scan.macros.push((macro_name.clone(), at.line))
                        }
                        _ => scan.findings.push(finding(
                            rel,
                            at.line,
                            "W3-macro-definition",
                            "unanalysable `macro_rules` token",
                        )),
                    }
                }
                if SLEEP_IDENTS.contains(&name.as_str()) {
                    let renamed =
                        matches!(tokens.get(index + 1), Some(Flat::Ident(next, _)) if next == "as");
                    if renamed {
                        scan.findings.push(finding(
                            rel,
                            at.line,
                            "W5-renamed-sleep",
                            format!("`{name} as ..` hides later sleeps from this scan"),
                        ));
                    } else if scopes.in_use_item(*at) {
                        // A plain import; the calls it enables are counted.
                    } else if !scopes
                        .innermost_loop(*at)
                        .is_some_and(|span| cap_polls.iter().any(|poll| contains(span, *poll)))
                    {
                        scan.sleep_hits
                            .push((scopes.name_at(*at), name.clone(), at.line));
                    }
                }
                let called_now =
                    name == "now" && index > 0 && is_path_or_method_call(&tokens, index);
                if called_now
                    || CLOCK_IDENTS.contains(&name.as_str())
                    || CRATE_COUNTED_IDENTS.contains(&name.as_str())
                {
                    scan.clock_hits
                        .push((scopes.name_at(*at), name.clone(), at.line));
                }
            }
            Flat::Literal(text, at) => {
                for fragment in COUNTED_LITERAL_FRAGMENTS {
                    if text.contains(fragment) {
                        scan.clock_hits.push((
                            scopes.name_at(*at),
                            format!("\"{fragment}\""),
                            at.line,
                        ));
                    }
                }
            }
            Flat::Punct(..) | Flat::Open(_) | Flat::Close => {}
        }
    }
    scan
}

/// `.name(` or `.name::<..>(`: a method call, not a binding or path.
fn is_method_call(tokens: &[Flat], index: usize) -> bool {
    let after_dot = index > 0 && matches!(tokens[index - 1], Flat::Punct('.', _));
    let called = match tokens.get(index + 1) {
        Some(Flat::Open(Delimiter::Parenthesis)) => true,
        Some(Flat::Punct(':', _)) => matches!(tokens.get(index + 2), Some(Flat::Punct(':', _))),
        _ => false,
    };
    after_dot && called
}

/// `now` preceded by `.` or `::`.
fn is_path_or_method_call(tokens: &[Flat], index: usize) -> bool {
    match &tokens[index - 1] {
        Flat::Punct('.', _) => true,
        Flat::Punct(':', _) => index >= 2 && matches!(tokens[index - 2], Flat::Punct(':', _)),
        _ => false,
    }
}

/// `#` must open an attribute. The attribute must not carry a `$`
/// metavariable, and must not name `path` except in the one admitted
/// spelling (see [`is_descendant_path_attribute`]).
fn check_attribute(
    rel: &str,
    tokens: &[Flat],
    index: usize,
    at: LineColumn,
    findings: &mut Vec<Finding>,
) {
    let mut open = index + 1;
    let inner = matches!(tokens.get(open), Some(Flat::Punct('!', _)));
    if inner {
        open += 1;
    }
    if !matches!(tokens.get(open), Some(Flat::Open(Delimiter::Bracket))) {
        findings.push(finding(
            rel,
            at.line,
            "W3-unanalysable-attribute",
            "`#` that does not open an attribute",
        ));
        return;
    }
    let close = matching_close(tokens, open);
    let body = &tokens[open + 1..close];
    let admitted_path = !inner && is_descendant_path_attribute(body);
    for token in body {
        match token {
            Flat::Ident(name, _) if name == "path" && !admitted_path => findings.push(finding(
                rel,
                at.line,
                "W3-path-attribute",
                "only `#[path = \"<relative path without ..>\"]` is admitted; \
                 any other attribute naming `path` can load a module from outside the scanned tree",
            )),
            Flat::Punct('$', _) => findings.push(finding(
                rel,
                at.line,
                "W3-macro-attribute",
                "an attribute built from a macro metavariable is unanalysable",
            )),
            _ => {}
        }
    }
}

/// The one admitted `path` attribute: exactly `path = "<literal>"` (outer,
/// no other tokens). The literal must be a plain string with no escapes,
/// relative, with no `..` or empty components, naming a `.rs` file. Rust
/// resolves such a path below the declaring file's directory, which the
/// filesystem walk already scans. `tests/tui.rs` roots the single `tui` test
/// binary this way.
fn is_descendant_path_attribute(body: &[Flat]) -> bool {
    let [
        Flat::Ident(key, _),
        Flat::Punct('=', _),
        Flat::Literal(text, _),
    ] = body
    else {
        return false;
    };
    let Some(path) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return false;
    };
    key == "path"
        && !path.contains(['\\', '"', ':'])
        && !path.starts_with('/')
        && path.ends_with(".rs")
        && path.split('/').all(|part| !part.is_empty() && part != "..")
}

// ── Whole-crate scan ─────────────────────────────────────────────────

fn scan_tree(
    manifest: &str,
    sources: &BTreeMap<String, String>,
    policy: &Policy<'_>,
) -> Vec<Finding> {
    let mut findings = check_manifest(manifest);
    let mut counts: ScopeCounts = BTreeMap::new();
    let mut sleep_counts: ScopeCounts = BTreeMap::new();
    let mut macros_seen = BTreeSet::new();
    for (rel, source) in sources {
        let scan = scan_file(rel, source);
        findings.extend(scan.findings);
        for (scope, token, line) in scan.clock_hits {
            counts
                .entry((rel.clone(), scope))
                .or_default()
                .push((token, line));
        }
        for (scope, token, line) in scan.sleep_hits {
            sleep_counts
                .entry((rel.clone(), scope))
                .or_default()
                .push((token, line));
        }
        for (name, line) in scan.macros {
            if policy
                .macro_allowlist
                .iter()
                .any(|(file, allowed)| file == rel && *allowed == name)
            {
                macros_seen.insert((rel.clone(), name));
            } else {
                findings.push(finding(
                    rel,
                    line,
                    "W3-macro-definition",
                    format!("`macro_rules! {name}` is not in MACRO_ALLOWLIST"),
                ));
            }
        }
    }
    for (file, name) in policy.macro_allowlist {
        if !macros_seen.contains(&((*file).to_owned(), (*name).to_owned())) {
            findings.push(finding(
                file,
                0,
                "W3-stale-macro-allowlist",
                format!("allowlisted `macro_rules! {name}` no longer exists"),
            ));
        }
    }
    check_allowlist(&counts, policy.clock_allowlist, &CLOCK_RULES, &mut findings);
    let sleep_allowlist = policy
        .sleep_allowlist
        .iter()
        .map(|entry| ScopeAllowance {
            file: entry.file,
            scope: entry.scope,
            count: entry.kinds.len(),
            reason: entry.reason,
        })
        .collect::<Vec<_>>();
    check_allowlist(&sleep_counts, &sleep_allowlist, &SLEEP_RULES, &mut findings);
    findings.sort();
    findings
}

/// Counted tokens per `(file, scope)`: `(token, line)` each.
type ScopeCounts = BTreeMap<(String, String), Vec<(String, usize)>>;

/// Rule names and advice for one counted-token allowlist.
struct AllowRules {
    unlisted: &'static str,
    drift: &'static str,
    reason: &'static str,
    stale: &'static str,
    advice: &'static str,
}

const CLOCK_RULES: AllowRules = AllowRules {
    unlisted: "W4-clock",
    drift: "W4-allowlist-drift",
    reason: "W4-allowlist-reason",
    stale: "W4-stale-allowlist",
    advice: "bound waits with HangCap / NegativeSoak",
};

const SLEEP_RULES: AllowRules = AllowRules {
    unlisted: "W5-sleep",
    drift: "W5-allowlist-drift",
    reason: "W5-allowlist-reason",
    stale: "W5-stale-allowlist",
    advice: "wait for a re-checked condition in a HangCap-polling loop, \
             or allowlist the scope with the reason it is pacing",
};

/// Counts must equal the allowlist exactly: an unlisted scope, a changed
/// count, a missing reason and a stale entry are all findings.
fn check_allowlist(
    counts: &ScopeCounts,
    allowlist: &[ScopeAllowance],
    rules: &AllowRules,
    findings: &mut Vec<Finding>,
) {
    for ((file, scope), hits) in counts {
        let entry = allowlist
            .iter()
            .find(|entry| entry.file == file && entry.scope == scope);
        match entry {
            None => {
                for (token, line) in hits {
                    findings.push(finding(
                        file,
                        *line,
                        rules.unlisted,
                        format!("`{token}` in `{scope}`; {}", rules.advice),
                    ));
                }
            }
            Some(entry) if entry.count != hits.len() => {
                let lines = hits
                    .iter()
                    .map(|(_, line)| line.to_string())
                    .collect::<Vec<_>>();
                findings.push(finding(
                    file,
                    hits[0].1,
                    rules.drift,
                    format!(
                        "`{scope}` has {} counted tokens (lines {}), allowlist pins {}",
                        hits.len(),
                        lines.join(","),
                        entry.count
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    for entry in allowlist {
        if entry.reason.trim().is_empty() {
            findings.push(finding(entry.file, 0, rules.reason, "entry needs a reason"));
        }
        if !counts.contains_key(&(entry.file.to_owned(), entry.scope.to_owned())) {
            findings.push(finding(
                entry.file,
                0,
                rules.stale,
                format!("`{}` has no counted tokens any more", entry.scope),
            ));
        }
    }
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_crate(root: &Path, policy: &Policy<'_>) -> Vec<Finding> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let (sources, mut findings) = discover_sources(root, &manifest);
    findings.extend(scan_tree(&manifest, &sources, policy));
    findings
}

// ── Rows ─────────────────────────────────────────────────────────────

#[test]
fn crate_has_no_unallowlisted_wall_clock_reads_or_scan_escapes() {
    let root = crate_root();
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let (sources, _) = discover_sources(&root, &manifest);
    assert!(
        sources.contains_key("src/lib.rs") && sources.contains_key("src/wall_clock_guard_test.rs"),
        "discovery must reach the crate root files"
    );
    let findings = scan_crate(&root, &CRATE_POLICY);
    assert!(
        findings.is_empty(),
        "wall-clock guard findings:\n{}",
        render(&findings)
    );
}

mod rows {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"probe\"\n";

    fn scan_with(files: &[(&str, &str)], policy: &Policy<'_>) -> Vec<Finding> {
        let sources = files
            .iter()
            .map(|(rel, text)| ((*rel).to_owned(), (*text).to_owned()))
            .collect();
        scan_tree(MANIFEST, &sources, policy)
    }

    const EMPTY: Policy<'static> = Policy {
        clock_allowlist: &[],
        sleep_allowlist: &[],
        macro_allowlist: &[],
    };

    fn rules(findings: &[Finding]) -> BTreeSet<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[track_caller]
    fn assert_red(source: &str, rule: &'static str) {
        let findings = scan_with(&[("tests/probe.rs", source)], &EMPTY);
        assert!(
            rules(&findings).contains(rule),
            "expected {rule} for:\n{source}\ngot:\n{}",
            render(&findings)
        );
    }

    #[test]
    fn every_path_attribute_except_a_plain_descendant_is_a_violation() {
        for source in [
            "#[path = \"../../outside.rs\"] mod escape;",
            "#[path = \"tests/../../outside.rs\"] mod escape;",
            "#[path = \"/abs/outside.rs\"] mod escape;",
            "#[path = \"\\x2e\\x2e/outside.rs\"] mod escape;",
            "#[path = r\"../outside.rs\"] mod escape;",
            "#[path = \"a//b.rs\"] mod escape;",
            "#[path = \"c:/outside.rs\"] mod escape;",
            "#[path = \"outside\"] mod escape;",
            "#[path = concat!(\"..\", \"/x.rs\")] mod escape;",
            "#[cfg_attr(all(), path = \"../../outside.rs\")] mod escape;",
            "#[cfg_attr(all(), path = \"inside.rs\")] mod escape;",
            "#[cfg_attr(unix, cfg_attr(all(), path = \"x.rs\"))] mod escape;",
            "#![cfg_attr(all(), path = \"x\")]",
            "#![path = \"x.rs\"]",
            "#[r#path = \"../x.rs\"] mod escape;",
            "mod outer { #[path = \"../x.rs\"] mod escape; }",
        ] {
            assert_red(source, "W3-path-attribute");
        }
        // The admitted spelling: its target is below the declaring file.
        let findings = scan_with(
            &[(
                "tests/tui.rs",
                "#[path = \"package_tui/mod.rs\"] mod package_tui; #[cfg(unix)] #[path = \"a.rs\"] mod a;",
            )],
            &EMPTY,
        );
        assert!(findings.is_empty(), "{}", render(&findings));
    }

    #[test]
    fn macro_assembled_attributes_are_violations() {
        assert_red(
            "macro_rules! m { ($a:meta) => { #[$a] mod escape; } } m!(path = \"x.rs\");",
            "W3-macro-attribute",
        );
        // `#` and the bracket group reach the expansion separately.
        assert_red(
            "macro_rules! m { ($s:tt, $b:tt) => { $s $b mod escape; } } m!(#, [path = \"x.rs\"]);",
            "W3-unanalysable-attribute",
        );
        assert_red(
            "macro_rules! m { ($a:tt) => { # $a mod e; } }",
            "W3-unanalysable-attribute",
        );
        assert_red("macro_rules! m { () => {} }", "W3-macro-definition");
        assert_red("macro m() {}", "W3-macro-definition");
    }

    #[test]
    fn every_spelling_of_file_inclusion_is_a_violation() {
        for source in [
            "include!(\"../outside.rs\");",
            "include![\"../outside.rs\"];",
            "include! { \"../outside.rs\" }",
            "std::include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/x.rs\"));",
            "const S: &str = include_str!(\"x.el\");",
            "const B: &[u8] = core::include_bytes!(\"x.bin\");",
            "fn f() { call!(include); }",
            "fn f() { r#include!(\"x.rs\"); }",
            "fn f() { concat_idents!(Inst, ant); }",
        ] {
            assert_red(source, "W3-file-inclusion");
        }
    }

    #[test]
    fn every_clock_spelling_is_counted_outside_the_allowlist() {
        for source in [
            "fn t() { let c = std::time::Instant::now(); }",
            "use std::time::Instant as Clock;",
            "use std::time::{self as t}; fn f() { t::Instant::now(); }",
            "fn t() { let _ = std::time::SystemTime::now(); }",
            "fn t() { let _ = std::time::UNIX_EPOCH; }",
            "fn t(x: X) { assert!(x.elapsed() < LIMIT); }",
            "fn t(a: A, b: B) { a.duration_since(b); }",
            "fn t(d: D) { d.as_millis(); }",
            "fn t() { unsafe { libc::clock_gettime(0, p) }; }",
            "fn t() { chrono::Utc::now(); }",
            "fn t(clock: C) { clock.now(); }",
            "fn t() { r#Instant::now(); }",
            "fn t() { mac!(Instant); }",
        ] {
            assert_red(source, "W4-clock");
        }
    }

    #[test]
    fn crate_counted_idents_are_counted_outside_the_allowlist() {
        for ident in CRATE_COUNTED_IDENTS {
            assert_red(&format!("fn t(x: X) {{ x.{ident}(); }}"), "W4-clock");
        }
        for fragment in COUNTED_LITERAL_FRAGMENTS {
            assert_red(&format!("fn t(v: V) {{ v[\"{fragment}\"]; }}"), "W4-clock");
        }
    }

    #[test]
    fn short_durations_are_left_to_the_runtime_floors() {
        // The tripwire does not evaluate durations: every one of these
        // reaches `HangCap::new` / `NegativeSoak::observe`, whose floors
        // panic at runtime whatever the spelling.
        let findings = scan_with(
            &[(
                "tests/probe.rs",
                "use neomacs_tui_tests::HangCap as Cap;
                 fn short() -> Duration { Duration::from_secs(5) / 25 }
                 fn t(s: &mut TuiSession) {
                     let d = Duration::from_secs(10);
                     let d = short();
                     Cap::new(d);
                     s.read_until::<_>(Duration::from_millis(200), |_| true);
                     NegativeSoak::observe::<()>(Duration::from_secs_f64(0.2), d, || ());
                 }",
            )],
            &EMPTY,
        );
        assert!(findings.is_empty(), "{}", render(&findings));
    }

    #[test]
    fn allowlist_is_exact_per_fn_scope() {
        const ALLOW: &[ScopeAllowance] = &[ScopeAllowance {
            file: "src/driver.rs",
            scope: "Driver::poll",
            // `Instant`, `now`, `elapsed`.
            count: 3,
            reason: "probe driver",
        }];
        let policy = Policy {
            clock_allowlist: ALLOW,
            sleep_allowlist: &[],
            macro_allowlist: &[],
        };
        let allowed = "struct Driver; impl Driver { fn poll(&self) { let a = Instant::now(); a.elapsed(); } }";
        assert!(scan_with(&[("src/driver.rs", allowed)], &policy).is_empty());

        // A sibling fn in the same file is not covered by the entry.
        let sibling = "struct Driver; impl Driver {
            fn poll(&self) { let a = Instant::now(); a.elapsed(); }
            fn other(&self) { Instant::now(); }
        }";
        let findings = scan_with(&[("src/driver.rs", sibling)], &policy);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "W4-clock" && f.detail.contains("Driver::other")),
            "{}",
            render(&findings)
        );

        // A closure or nested fn inside the scope counts toward it.
        let grown = "struct Driver; impl Driver { fn poll(&self) { let a = Instant::now(); let f = || a.elapsed(); f(); a.elapsed(); } }";
        assert!(
            rules(&scan_with(&[("src/driver.rs", grown)], &policy)).contains("W4-allowlist-drift")
        );

        let shrunk = "struct Driver; impl Driver { fn poll(&self) { Instant::now(); } }";
        assert!(
            rules(&scan_with(&[("src/driver.rs", shrunk)], &policy)).contains("W4-allowlist-drift")
        );

        let gone = "struct Driver; impl Driver { fn poll(&self) {} }";
        assert!(
            rules(&scan_with(&[("src/driver.rs", gone)], &policy)).contains("W4-stale-allowlist")
        );

        // The same fn name in another file is a different scope.
        let elsewhere = scan_with(
            &[("src/driver.rs", allowed), ("tests/probe.rs", allowed)],
            &policy,
        );
        assert!(
            elsewhere
                .iter()
                .any(|f| f.rule == "W4-clock" && f.file == "tests/probe.rs"),
            "{}",
            render(&elsewhere)
        );
    }

    #[test]
    fn a_sleep_outside_a_cap_polling_loop_is_a_violation() {
        for source in [
            "fn t() { std::thread::sleep(Duration::from_millis(500)); assert!(ready()); }",
            "use std::thread::sleep; fn t() { sleep(SETTLE); }",
            "fn t() { for _ in 0..20 { thread::sleep(TICK); if ready() { break; } } assert!(ready()); }",
            "fn t() { loop { thread::sleep(TICK); if ready() { break; } } }",
            "async fn t() { tokio::time::sleep(TICK).await; }",
            "fn t() { let f = std::thread::sleep; f(TICK); }",
            "fn t() { std::thread::sleep_ms(500); }",
            "fn t(cap: &HangCap) { cap.assert_not_expired(\"x\"); thread::sleep(TICK); }",
            // A binding or path named like a cap poll is not a poll.
            "fn t() { loop { let is_expired = (); let _ = is_expired; thread::sleep(TICK); } }",
            "fn t() { while ready() { HangCap::is_expired; thread::sleep(TICK); } }",
            // `for` is the fixed-count retry shape, even with a real cap call.
            "fn t() { for _ in 0..1 { let is_expired = (); let _ = is_expired; thread::sleep(TICK); } }",
            "fn t(cap: &HangCap) { for _ in 0..1 { cap.is_expired(); thread::sleep(TICK); } }",
            "fn t() { std::thread::park_timeout(TICK); }",
            "fn t() { unsafe { libc::usleep(200_000) }; }",
            "fn t() { unsafe { libc::nanosleep(&req, null_mut()) }; }",
            "fn t() { unsafe { libc::clock_nanosleep(0, 0, &req, null_mut()) }; }",
        ] {
            assert_red(source, "W5-sleep");
        }
        assert_red("use std::thread::sleep as nap;", "W5-renamed-sleep");
        assert_red("use std::thread::{sleep as nap};", "W5-renamed-sleep");
    }

    #[test]
    fn a_sleep_pacing_a_cap_polling_loop_is_admitted() {
        let findings = scan_with(
            &[(
                "tests/probe.rs",
                "use std::thread::{self, sleep};
                 fn a(cap: &HangCap) {
                     loop {
                         if ready() { break; }
                         cap.assert_not_expired(\"ready\");
                         thread::sleep(TICK);
                     }
                 }
                 fn b(cap: &HangCap) { while let Some(slice) = cap.poll_slice(TICK) { sleep(slice); } }
                 fn c(cap: &HangCap) { while !cap.is_expired() { if ready() { return; } sleep(TICK); } }",
            )],
            &EMPTY,
        );
        assert!(findings.is_empty(), "{}", render(&findings));
    }

    #[test]
    fn sleep_allowlist_is_exact_per_fn_scope() {
        const SLEEPS: &[SleepAllowance] = &[SleepAllowance {
            file: "tests/probe.rs",
            scope: "pace",
            kinds: &[SleepKind::InputCadence],
            reason: "probe: paces input injection; nothing is asserted after it",
        }];
        let policy = Policy {
            clock_allowlist: &[],
            sleep_allowlist: SLEEPS,
            macro_allowlist: &[],
        };
        let paced = "fn pace() { thread::sleep(TICK); }";
        assert!(scan_with(&[("tests/probe.rs", paced)], &policy).is_empty());
        let grown = "fn pace() { thread::sleep(TICK); thread::sleep(TICK); }";
        assert!(
            rules(&scan_with(&[("tests/probe.rs", grown)], &policy)).contains("W5-allowlist-drift")
        );
        let fresh = "fn pace() { thread::sleep(TICK); } fn other() { thread::sleep(TICK); }";
        let findings = scan_with(&[("tests/probe.rs", fresh)], &policy);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "W5-sleep" && f.detail.contains("`other`")),
            "{}",
            render(&findings)
        );
        let gone = "fn pace() {}";
        assert!(
            rules(&scan_with(&[("tests/probe.rs", gone)], &policy)).contains("W5-stale-allowlist")
        );
    }

    #[test]
    fn sleep_pins_fail_in_both_directions_and_for_every_kind() {
        const THREE: &[SleepAllowance] = &[SleepAllowance {
            file: "tests/probe.rs",
            scope: "settle",
            kinds: &[
                SleepKind::BlindSettle,
                SleepKind::InputCadence,
                SleepKind::BlindSettle,
            ],
            reason: "probe",
        }];
        let pin = |total, blind_settle| SleepPin {
            total,
            blind_settle,
        };
        assert!(check_sleep_pin(THREE, pin(3, 2)).is_ok());
        // Raised blind pin with no new BlindSettle sleep.
        assert!(check_sleep_pin(THREE, pin(3, 3)).is_err());
        // A new BlindSettle sleep, or a stale pin after a conversion.
        assert!(check_sleep_pin(THREE, pin(3, 1)).is_err());
        // A new sleep labelled with any other kind still moves the total.
        const RELABELLED: &[SleepAllowance] = &[SleepAllowance {
            file: "tests/probe.rs",
            scope: "settle",
            kinds: &[
                SleepKind::BlindSettle,
                SleepKind::InputCadence,
                SleepKind::BlindSettle,
                SleepKind::PreSampleDelay,
            ],
            reason: "probe",
        }];
        assert!(check_sleep_pin(RELABELLED, pin(3, 2)).is_err());
        // Every crate in the shared table has a pin; this crate is one of them.
        assert!(
            SLEEP_PINS
                .iter()
                .any(|(name, _)| *name == env!("CARGO_PKG_NAME"))
        );
    }

    #[test]
    fn macro_allowlist_is_exact() {
        const MACROS: &[(&str, &str)] = &[("src/lib.rs", "compare_field")];
        let policy = Policy {
            clock_allowlist: &[],
            sleep_allowlist: &[],
            macro_allowlist: MACROS,
        };
        let ok = "fn f() { macro_rules! compare_field { () => {} } }";
        assert!(scan_with(&[("src/lib.rs", ok)], &policy).is_empty());
        let moved = scan_with(&[("src/other.rs", ok)], &policy);
        assert!(rules(&moved).contains("W3-macro-definition"));
        assert!(rules(&moved).contains("W3-stale-macro-allowlist"));
    }

    #[test]
    fn unreadable_sources_are_violations_not_skips() {
        assert_red("fn broken( {", "W2-unlexable");
        assert_red("fn broken() -> { }", "W2-unparseable");
        assert_red("let s = \"unterminated;", "W2-unlexable");
    }

    #[test]
    fn cargo_targets_must_stay_in_the_scanned_tree() {
        for manifest in [
            "[lib]\npath = \"../outside/lib.rs\"\n",
            "[[test]]\nname = \"t\"\npath = \"../outside.rs\"\n",
            "[[test]]\nname = \"t\"\npath = \"tests/../../outside.rs\"\n",
            "[[bin]]\nname = \"b\"\npath = \"bin/main.rs\"\n",
            "lib.path = \"elsewhere/lib.rs\"\n",
            "test = [{ name = \"t\", path = \"/abs/outside.rs\" }]\n",
            "test = \"tests/tui.rs\"\n",
            "[lib\n",
            "[package]\nname = \"p\"\nbuild = \"../outside/build.rs\"\n",
            "[package]\nname = \"p\"\nbuild = \"/abs/build.rs\"\n",
            "[package]\nname = \"p\"\nbuild = 7\n",
            "[[example]]\nname = \"e\"\npath = \"elsewhere/e.rs\"\n",
        ] {
            let findings = check_manifest(manifest);
            assert!(!findings.is_empty(), "manifest accepted:\n{manifest}");
        }
        for manifest in [
            "[[test]]\nname = \"tui\"\npath = \"tests/tui.rs\"\n[dependencies]\nx = { path = \"../x\" }\n",
            // Inferred paths land in scanned directories.
            "[[example]]\nname = \"e\"\n[[bench]]\nname = \"b\"\n",
            "[[example]]\nname = \"e\"\npath = \"examples/e.rs\"\n",
            "[package]\nname = \"p\"\nbuild = \"build/main.rs\"\n",
            "[package]\nname = \"p\"\nbuild = false\n",
        ] {
            let findings = check_manifest(manifest);
            assert!(findings.is_empty(), "{manifest}\n{}", render(&findings));
        }
    }

    #[test]
    fn discovery_walks_the_filesystem_not_the_module_tree() {
        let root = std::env::temp_dir().join(format!(
            "wall-clock-guard-discovery-{}",
            crate::timing::unique_nonce()
        ));
        fs::create_dir_all(root.join("tests/orphan/deeper")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), MANIFEST).unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        // Not reachable through any `mod` declaration.
        fs::write(
            root.join("tests/orphan/deeper/unlinked.rs"),
            "fn t() { std::time::Instant::now(); }",
        )
        .unwrap();
        // Cargo compiles these without any manifest entry.
        fs::create_dir_all(root.join("examples")).unwrap();
        fs::create_dir_all(root.join("benches")).unwrap();
        fs::write(
            root.join("examples/clock.rs"),
            "fn main() { std::thread::sleep(T); }",
        )
        .unwrap();
        fs::write(
            root.join("benches/clock.rs"),
            "fn main() { std::time::Instant::now(); }",
        )
        .unwrap();
        fs::write(
            root.join("build.rs"),
            "fn main() { std::time::Instant::now(); }",
        )
        .unwrap();
        let findings = scan_crate(&root, &EMPTY);
        fs::remove_dir_all(&root).unwrap();
        for (file, rule) in [
            ("tests/orphan/deeper/unlinked.rs", "W4-clock"),
            ("examples/clock.rs", "W5-sleep"),
            ("benches/clock.rs", "W4-clock"),
            ("build.rs", "W4-clock"),
        ] {
            assert!(
                findings.iter().any(|f| f.rule == rule && f.file == file),
                "{file} {rule}\n{}",
                render(&findings)
            );
        }
    }

    #[test]
    fn shared_engine_is_identical_in_both_guards() {
        let engine = |text: &str| -> String {
            let start = text
                .find(&format!("{SHARED_ENGINE_MARKER}\n"))
                .expect("shared engine marker");
            text[start..].to_owned()
        };
        let own = fs::read_to_string(crate_root().join("src/wall_clock_guard_test.rs")).unwrap();
        let sibling = fs::read_to_string(crate_root().join(SIBLING_GUARD)).unwrap();
        assert!(
            engine(&own) == engine(&sibling),
            "the wall-clock guard engines diverged; copy the shared section from one to the other"
        );
    }

    #[test]
    fn crate_allowlists_are_well_formed() {
        let mut seen = BTreeSet::new();
        for entry in SLEEP_ALLOWLIST {
            assert!(
                seen.insert((entry.file, entry.scope)),
                "duplicate sleep allowlist entry {}::{}",
                entry.file,
                entry.scope
            );
            assert!(!entry.kinds.is_empty() && !entry.reason.trim().is_empty());
        }
        if let Err(message) = check_sleep_pin(SLEEP_ALLOWLIST, crate_sleep_pin()) {
            panic!("{message}");
        }
        let mut seen = BTreeSet::new();
        for entry in CLOCK_ALLOWLIST {
            assert!(
                seen.insert((entry.file, entry.scope)),
                "duplicate allowlist entry {}::{}",
                entry.file,
                entry.scope
            );
            assert!(entry.count > 0 && !entry.reason.trim().is_empty());
            assert!(
                !entry.file.starts_with("tests/")
                    || INTEGRATION_TEST_CLOCK_FILES.contains(&entry.file),
                "{} is not in INTEGRATION_TEST_CLOCK_FILES",
                entry.file
            );
        }
    }
}
