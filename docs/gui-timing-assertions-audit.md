# GUI Timing-Assertion Audit and Ratchet (`crates/neomacs-gui-tests`)

This document records the merged inventory of every wall-clock, latency,
cadence, clock-uncertainty, promptness-budget, and indirect load-sensitive
assertion in `crates/neomacs-gui-tests` (audited across all 138 `.rs` files /
359 `#[test]` functions on `v10x/lead-77i`, plus all fixtures under
`crates/neomacs-gui-tests/fixtures/`).

It is enforced by
`crates/neomacs-gui-tests/tests/timing_assertion_ratchet.rs`, which pairs with
`crates/neomacs-gui-tests/src/wall_clock_guard_test.rs` (W1–W5):

- **`wall_clock_guard_test.rs` (W1–W5)** floors wait deadlines (`HangCap >= 5 s`,
  `NegativeSoak >= 1 s`), denies unallowlisted Rust clock reads (`CLOCK_ALLOWLIST`),
  and pins fixed sleeps (`SLEEP_PINS`).
- **`timing_assertion_ratchet.rs`** covers the complementary half: **what tests
  assert** across both direct classifications (latency/cadence/uncertainty and
  promptness/exit-within-budget) plus indirect Tier B load-sensitive rows whose
  wall-clock coupling lives in capture spacing or Elisp fixture timers (which
  W1–W5 do not scan).

## Why Two Classifications Matter (Audit Reconciliation)

Two independent audits of `crates/neomacs-gui-tests` were merged into this
inventory (folding in peer's `cd772a7284` audit and measurement notes):

1. **Latency / cadence / clock-uncertainty audit** found the calibrated and
   hardware-gated rows (`native_scrolling::continuous_120hz_*` plus its offline
   budget/latency unit tests, and `x11_presentation` latency + `EXT_present_timing`
   clock-uncertainty bounds).
2. **Full wall-clock + promptness audit** additionally caught
   `issue_417_startup::invalid_initial_frame_exits_promptly_without_a_gui_frame`,
   the only direct timing assertion that ran in default CI on every host (it
   previously asserted `published.elapsed() < 5s` and
   `finished.duration_since(published) <= 5s` against `SystemTime::now() - mtime`).
   Because a latency-only pattern scan missed that promptness-budget shape,
   `timing_assertion_ratchet.rs` enforces **both** classifications separately
   and pins promptness-budget assertions at **`0`**.

### The line this audit drew

Every borderline row was classified by one question: **does the elapsed value
appear in the pass/fail predicate, or only in the loop's exit condition and the
moment of sampling?** An `assert!(started.elapsed() < timeout)` inside a poll
loop is a deadline — failing it means the thing never arrived, not that it was
slow. `elapsed >= 350ms` requiring a geometry to hold is a stability wait.
`Instant::now() < deadline` in a wait is a hang cap. A p95 compared against a
calibrated tier, a rate compared against a second, or a promptness exit budget
against a wall clock — those are timing assertions, and they are what the pins
cover. Tier B exists because the boundary is real and arguable; this sentence
is the boundary.

### How the first scan missed a live Tier-A row, and why that is recorded here

The `issue_417_startup.rs` promptness assertion was found by the second
independent audit, not by the first. The first scan looked for
`elapsed\(\)\s*(<|>|<=|>=)` — the comparison operator immediately after the
call — and the real row was `published.elapsed().unwrap() < EXIT_BUDGET`, where
`.unwrap()` sat between them. It also skipped `duration_since` because prior
occurrences were `UNIX_EPOCH` unique-IDs. Two misses came from one assumption:
scanning against a remembered spelling rather than against the AST construct.
Hence `timing_assertion_ratchet.rs` parses with `syn` and pins
`TIER_A_PROMPTNESS_SITES = []` (`0`): the converted row must never come back.

### A conditionality that the count cannot see

The only live response-budget assertion in this crate is **conditional**. In
`tests/native_scrolling/continuous.rs` the check is guarded by
`if let Some(selected_budget) = &selected_budget`, and a budget exists only
when `budget.rs::budget()` returned `Ok` from a calibration probe (and only
under `Mode::CalibratedPacing`). Without a probe the assertion is skipped
entirely — no failure, no skip marker, no count. A ratchet over assertion
*sites* therefore pins a row that may not execute on every run, and "3 pinned"
does not mean "3 executed". That is not an argument against the pin; it is why
the pin is paired with explicit CI exclusion and hardware-gate documentation so
nobody reads a green CI run as evidence that the latency budget was measured.

### Two independent scans, two widths

A parallel audit on post-77i `main` counted 12 clock-consuming `assert` macros
across 3 files (`tests/native_scrolling/continuous.rs`,
`tests/native_scrolling/latency.rs`, `tests/x11_presentation.rs`) and **zero**
deadline-shaped ones — every `assert!(Instant::now() < deadline)` in this crate
has become `HangCap::assert_not_expired`, which is why deadline noise
disappeared between the two sweeps. The wider AST scan in
`timing_assertion_ratchet.rs` additionally reaches `src/` (0 live timing
assertions today) plus Tier B's `child_frame_animation` (9 pixel-ramp
assertions) and `real_gui_smoke/line_navigation` (1 `composite_only_frames`
ratio assertion). That difference is token width, not disagreement about what
is there, and recording it shows which way to widen next.

---

## Pinned Inventory Summary

| Classification | Pin on `d6f1de3ca8` | Policy |
|---|---:|---|
| Tier A1 — Direct latency / cadence / clock-uncertainty assertions | **3** | Calibrated / hardware-only measurements; equality-pinned |
| Tier A2 — Direct promptness / exit-within-budget assertions | **0** | Shrink-only (`0`); never reintroduce |
| Tier B — Indirect load-sensitive / capture-latency / rate-coupled rows | **4** | Documented debt; shrink as rows convert, explicit reviewed diff to grow |

---

## Tier A1 — Direct Latency, Cadence, and Clock-Uncertainty Assertions (`3` active rows)

| Row | File & Lines | What It Asserts | Disposition | Acceptable Host Bracket |
|---|---|---|---|---|
| `native_scrolling::continuous_120hz_scroll_in_large_plain_buffer` | `tests/native_scrolling.rs:179`, `tests/native_scrolling/continuous.rs:222`, `tests/native_scrolling/latency.rs:175-205` | `report.assess(selected_budget) == WithinBudget` (`p95 <= 50 ms`, `max <= 250 ms`, `median_gap <= 20 ms`, `max_gap <= 3.25 * median`) | `calibrated-by-design` (measurement; CI-excluded via `not test(~continuous_120hz_)`; CI runs state twin `sustained_scroll_stream_drains_and_reverses_in_large_plain_buffer`) | Any hardware Vulkan GPU (`Original`), or quiet calibrated software-Vulkan host (`glmark2` llvmpipe median within recorded tiers) |
| `native_scrolling::continuous_120hz_scroll_in_large_rich_buffer` | `tests/native_scrolling.rs:197`, `tests/native_scrolling/continuous.rs:222`, `tests/native_scrolling/budget.rs:210-234`, `tests/native_scrolling/latency.rs:175-205` | `glmark2` capacity probe (`<= 3.0 ms` Fast / `<= 3.6 ms` ReducedCapacity) + `report.assess(selected_budget) == WithinBudget` (`p95 <= 500/1500 ms`, `max <= 750/2000 ms`, `median_gap <= 20/25 ms`, `max_gap <= 200 ms`) | `calibrated-by-design` (measurement; CI-excluded via `not test(~continuous_120hz_)`; CI runs state twin `sustained_scroll_stream_drains_and_reverses_in_large_rich_buffer`) | Dedicated hardware Vulkan GPU (`Original`) OR quiet calibrated host with `glmark2` llvmpipe median `<= 3.0 ms` (`SoftwareVulkanFast`) or `<= 3.6 ms` (`SoftwareVulkanReducedCapacity`, e.g., i9-14900K / dedicated EPYC 9V74 / 7763) |
| `x11_presentation::x11_rich_scroll_inputs_have_native_output_receipts_across_resize` | `tests/x11_presentation.rs:98, 153, 210, 286, 294-295` | `:uncertainty-ns <= 1_000_000` (`:153`), `timestamp_uncertainty_ns <= 1_000_000` (`:210`), `cold_page_ns <= 250 ms` (`:286`), warm `page` and `wheel` `p95_ns <= 50 ms && max_ns <= 250 ms` (`:294-295`) | `hardware-only` (pre-release hardware gate; opt-in via `NEOMACS_REQUIRE_X11_TIMING=1`; see `docs/releasing-linux.md#pre-release-hardware-x11-presentation-timing-gate`) | Unloaded Linux X11 desktop with physical Vulkan GPU advertising `VK_EXT_present_timing` (`<= 1 ms` clock uncertainty) and `xdotool` |

*Offline unit tests excluded from Tier A1 live-row count:* `native_scrolling/budget_tests.rs` (3 unit rows), `native_scrolling/budget_regression_tests.rs` (2 unit rows), and `native_scrolling/latency.rs::tests` (5 unit rows) test `ResponseBudget` and `Report::assess` over fixed synthetic/recorded numbers with zero wall-clock measurement.

---

## Tier A2 — Direct Promptness / Exit-Within-Budget Assertions (`0` active rows)

| Row | File & Lines | Previous Assertion | Disposition | Verification |
|---|---|---|---|---|
| `issue_417_startup::invalid_initial_frame_exits_promptly_without_a_gui_frame` | `tests/issue_417_startup.rs:55, 361, 397-401` | Previously asserted `published.elapsed() < 5s` and `finished.duration_since(published) <= 5s` (`SystemTime::now() - mtime`, non-monotonic, ran in default CI) | `converted-by-guard` (`a92a33ed1a`): replaced with `HangCap::new(EXIT_HANG_CAP)` (`15 s` monotonic hang cap) + `exit_cap.assert_not_expired("invalid initial frame did not terminate")` | Verified RED (`hang-avoidance cap (15s) expired: invalid initial frame did not terminate`) against a deliberately wedged `before-init-hook` startup, and GREEN on unmodified startup |

---

## Tier B — Indirect Load-Sensitive / Capture-Latency / Rate-Coupled Rows (`4` open rows)

These rows do not assert an elapsed duration directly, but can fail purely from host CPU load or compositor capture latency. They are kept visible and pinned by `timing_assertion_ratchet.rs` rather than silently forgotten:

| Row | File & Lines | Disposition | Why Load-Sensitive & Future State Conversion |
|---|---|---|---|
| `child_frame_animation::child_frame_fades_progress_across_presented_frames` | `tests/child_frame_animation.rs:185, 269-275, 393-403, 433-442, 603-683` + `fixtures/child-frame-animation.el:102-168` | `open-tier-b` (runs in default CI) | Races fixed `sleep(500/800/1200ms)` `grim` captures against fixed `3.5s..35.5s` Elisp timers and asserts strict mid-ramp pixel inequalities (`opening_strip`, `fading`, `fading_strip`, `resized_strip`); convert by driving phase transitions via explicit file-ack barriers and keeping ramp monotonicity on the capture-latency-independent render-thread alpha/scale/x/mix logs (`:494-597, :690-795`) plus state-polled endpoint captures. |
| `agent_shell_source::melpa_agent_shell_resume_paints_without_false_source_mutation_like_gnu` | `tests/agent_shell_source.rs:57, 219-231` + `fixtures/agent-shell-source-observation.el:172-175` | `open-tier-b` (oracle-gated) | Requires the fixed `t = 1.5s` wall-clock timer sample (`label == "1.5"`) to already observe `initialized == true` and `body_seen == true`, implicitly asserting Python ACP startup + resume completes within `1.5s`; convert by triggering the first settled sample on the state transition (`:init-finished` + body present) and taking a second post-redisplay stability sample. |
| `real_gui_smoke::line_navigation::timer_line_navigation_reuses_scene_during_cursor_motion` | `tests/real_gui_smoke/line_navigation.rs:5, 120-131` + `fixtures/line-navigation.el:8-17` | `open-tier-b` (hardware GPU `quality_mode=Full`, opt-in via `NEOMACS_GUI_TEST_BACKEND`) | While `root_glyph_passes <= commits * 3` is a pure work-per-commit ratio, `motion > 100` and `composite_only_frames > commits` over `0.1s` timer ticks require `> 10 fps` on a hardware GPU; convert or isolate as an explicit `gpu_only_` row driven by presentation receipts rather than a fixed `0.1s` wall-clock timer. |
| `presentation_pacing_receipts::wayland_gpu_only_cursor_motion_presents_paced_receipts_without_unpaced_storm` | `tests/presentation_pacing_receipts.rs:476, 518-534` + `fixtures/presentation-pacing-receipts.el:93` | `open-tier-b` (hardware GPU `quality_mode=Full`, excluded on pixman CI via `not test(~gpu_only_)`; landed in `2b08b5dbca`) | While `p2_platform_redraw == 0` and `(1..=2).contains(&p2_cursor_anim)` are pure scheduling invariants, `phase2_receipts.len() >= 2`, `p2_composite_only >= 3`, and `p2_cursor_motion >= 3` over a `0.35s` animation require `>= ~9 fps` on a hardware GPU; convert by scaling animation duration via deterministic render-thread step control if a software-clocked test harness is ever added. |

---

## Notable Borderline Rows Audited and Excluded

To keep the distinction between *waiting* and *asserting* crisp, the following
borderline rows were inspected and confirmed **not** to assert wall-clock speed:

- **`native_scrolling.rs` stalled-scroll rows (`5` rows):** `assert_projected_precedes_present` (`input_to_projected_present_ns < input_to_present_ns`) compares two timestamps from the same input separated by `fs::remove_file(&stall_path)` — a load-invariant causal ordering check.
- **`issue_582_fullscreen.rs` (`StabilityHold`) & `native-resize-rejection-oracle.el` (`>= 1.0s` / `>= 0.5s`):** minimum dwell/stability waits that wait *longer* under load and never fail for being slow.
- **`present_contract.rs` (`500ms` / `1000ms` phase holds):** minimum hold delays before polling pixels inside a `30 s` cap.
- **`idle_timer_output.rs` (`idle-timer-output.el`):** `(not (time-less-p after before))` verifies monotonic idle-epoch preservation across a timed `read-char`, independent of host speed.
- **`desktop_font_startup.rs`:** matches `:seconds`/`:nanoseconds`/`:clock-id` verbatim against `WAYLAND_DEBUG=client` `wp_presentation_feedback#*.presented(...)` wire events (exact identity, not elapsed time).
- **`braille_blank.rs`:** records `edit_seconds` as diagnostic JSON only and never asserts on it.
