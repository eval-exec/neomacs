# Process / Network / Event-Pool Migration — Status & Remaining Work

Last updated: 2026-07-02. Owner: oracle-parity effort (see `git log` commits
referenced below). Method and evidence for every claim: live-GNU oracle probes
(`emacs --batch -Q` vs `./target/release/neomacs --batch -Q`), GNU source study
(`process.c`, `keyboard.c`, `timefns.c` in the emacs-mirror checkout), and the
neovm-oracle-tests suite in live mode.

This document is the durable, verbose record of the audit's divergence catalog
(D1–D14), the migration slices (S1–S10), what has landed, what is in flight,
and exactly how to execute the remaining slices. A scratch copy of the original
audit lives in `drafts/process-event-pool-audit.md` (gitignored by repo
convention); this file supersedes it.

---

## 1. The GNU model (the contract being matched)

GNU Emacs runs **one synchronous loop** — `wait_reading_process_output`
(process.c:5336) — as the only place where process I/O, filters, sentinels,
timers, and async-connect completion ever execute. Per iteration, in order:

1. `maybe_quit` / pending-signal processing.
2. **Deadline check — break BEFORE servicing timers** (process.c:5469-5478).
3. Async DNS (`getaddrinfo_a`) / TLS / `:nowait` connect completion →
   `connect_network_socket`.
4. `timer_check` (keyboard.c:4911) unless `just_wait_proc < 0`: copies
   `timer-list`/`timer-idle-list` (so timers can reschedule themselves), fires
   ALL ripe timers one at a time via the Lisp `timer-event-handler`
   (re-looping `timer_check_2` while it returns `{0,0}`), returns the delta to
   the next timer.
5. `status_notify` if `update_tick != process_tick`.
6. `pselect` with timeout = min(remaining-deadline, next-timer-delta). No
   polling cap; wakes exactly on readiness.
7. Server accepts — `server_accept_connection` creates the connection process,
   calls the `:log` function, and `exec_sentinel(proc, "open from HOST\n")`
   **immediately, same iteration** (the only push-style sentinel in GNU).
8. `read_process_output` per ready fd, round-robin: chunk =
   `read-process-output-max`, decoding carryover in `p->decoding_buf`,
   adaptive read buffering (delay ladder: increment ≈10ms, max ≈50ms), filter
   runs synchronously (current buffer = process buffer, `running_asynch_code`,
   match-data save/restore). EOF ⇒ status transition + `tick++`.
9. Keyboard check; loop.

**Status pipeline, not an event queue.** Every transition writes `p->status`
and bumps `p->tick`. `status_notify` (process.c:7873), for each process with
`tick != update_tick`: drains remaining output FIRST (the drained bytes feed
the wait's `got_some_output`), derives the sentinel message from the
**current** status via `status_message(p)` — so transitions that happen within
one wait **collapse into a single sentinel with the final message** — then
deactivates/removes terminated processes BEFORE `exec_sentinel`.

Key contract points verified empirically this cycle:

- **`got_some_output` counts read bytes only** (process.c:5588/6018 and the
  status_notify drain). Connect completions, accepts, sentinel runs, and EOFs
  are *serviced* inside the wait but never complete it; `accept-process-output`
  returns `t` only when actual output was read.
- **A WAIT_PROC wait breaks when the target's INTERNAL status is neither
  `Qrun` nor a pending connect** (the `!EQ (wait_proc->status, Qrun) &&
  !connecting_status` drain-then-break). GNU-internal statuses differ from the
  `process-status` projection: a listen server is stored `Qlisten` (so
  `accept-process-output` on a server returns immediately — verified), while a
  connected netconn is stored `Qrun` (projected to `open` by
  `Fprocess_status`), and an io-paused connection (`stop-process` on a
  netconn: `p->command = Qt`) stays internally `Qrun` and does NOT break the
  wait even though `process-status` projects `stop`.
- **Blocking (non-`:nowait`) client connects fire NO sentinel** —
  `connect_network_socket` contains zero `exec_sentinel` calls. Only the
  deferred `:nowait` completion path delivers `"open\n"` / `"failed with code
  N\n"`.
- **Peer EOF on a running network connection sets `(exit . 256)`**
  (process.c:6090); `status_message` maps network exit 0 → `"deleted\n"` and
  non-zero → `"connection broken by remote peer\n"`.
- **Observation points decode pending child status**: both `Fprocess_status`
  and `Fprocess_exit_status` run `update_status` when `raw_status_new` is set
  (raw status arrives asynchronously via SIGCHLD). So `process-status`
  immediately after a wait reports `exit` even though the sentinel
  notification is still pending for the next `status_notify`.
- **Timers live in timer.el.** C keeps only the `timer-list`/`timer-idle-list`
  variables, `timer_check`, and `current_timespec` (nanosecond resolution:
  `PSEC = (ns % 1000) * 1000`). `run-at-time`, `timer-activate`, sorted
  insertion, retriggering at **old time + repeat**, and `timer-max-repeats`
  catch-up are all Lisp.
- **`send_process` can re-enter the wait**: on EAGAIN it queues the remainder
  in `p->write_queue` and calls `wait_reading_process_output(0, 20ms, …)`, so
  filters can run during `process-send-string` (documented cascading).
- **`stop-process` semantics split**: network/serial/`make-pipe-process`
  connections → pause reading (`p->command = Qt`), status untouched; real
  subprocesses (any `:connection-type`) → `process_send_signal(SIGTSTP)` with
  **status untouched** — status only changes if `waitpid(WUNTRACED)` later
  *observes* a stop. (Verified: an sh child ignores SIGTSTP — kernel state
  stays S — and GNU keeps reporting `run`.)

## 2. Divergence catalog (D1–D14) and disposition

| # | Divergence | Status |
|---|---|---|
| D1 | `current-time` PSEC always 0 (µs truncation in `TimeMicros::now`) | **FIXED** `497350bff` |
| D2 | Native timer "second brain" (`TimerManager`, unregistered `run-at-time`/`timer-activate` builtins, `now+interval` rescheduling) | **DELETED** `5829b6f11` (was dead code: no live writers; the real brain was already Lisp timer-list + `timer-event-handler`); the real psec bug was `decode_lisp_time`'s nil branch — **FIXED** `0dd82abd1` |
| D3 | Wait loop serviced ripe timers after a wake even when the deadline had elapsed | **FIXED** `bf71ed726` (deadline-first, GNU loop order). Note: the exact-boundary fire-count (cx64/cx179/cx338) is a jitter race **inside GNU itself** (10 idle GNU runs: `21 20 20 20 20 20 20 20 20 21`); classified EXPECTED-TIMING |
| D4 | Sentinel messages stored at event time instead of derived from current status at notify | **FIXED — S4c** (terminal notifications now derive via `gnu_process_status_message_for_process`; accept / `:nowait` literals remain GNU push-style exceptions) |
| D5 | Spurious `"open\n"` sentinel for blocking client connects | **FIXED** `d02f31edb` |
| D6 | Network EOF set `(exit . 0)`; "connection broken" delivered rounds late; accepts/EOF treated as wait-completing activity | **FIXED**: exit-256 + derived message + reap in `d02f31edb`; completion semantics in S4b (in tree, see §4) |
| D7 | `stop-process` eagerly sets status `stop` for real subprocesses | **FIXED — S5** (`stop-process` sends only SIGTSTP; stop/run status comes from `waitpid(WUNTRACED|WCONTINUED)`) |
| D8 | seqpacket not advertised in `featurep 'make-network-process` though fully implemented | **FIXED** `69b874f9b` |
| D9 | Child exit observed only inside wait iterations; `process-status` stale outside waits (the load-flaky family's root cause) | **IN TREE — S6** (see §4) |
| D10 | Fixed 4096-byte reads; no `read-process-output-max`; no adaptive read buffering | **FIXED — S7** |
| D11 | `process-send-string` = `write_all`+`flush`; never re-enters the wait; `write_queue` field exists unused | **FIXED — S8** |
| D12 | DNS always blocking, even for `:nowait` | **FIXED — S9** |
| D13 | 50ms polling cap per wait iteration instead of exact timeouts | **FIXED — S9** |
| D14 | Lisp threads: mutex ownership error where GNU blocks; dynamic `let` leaks across threads; `all-threads` misses blocked workers; `thread-signal` handler detail | **RESOLVED — S10** (all four verified 2026-07-20 at live parity vs GNU 31.0.90; see §5) |

## 3. Landed slices (all pushed to main)

| Commit | Slice | Content |
|---|---|---|
| `497350bff` | S1 | `TimeMicros::now` uses `subsec_nanos`; `PSEC = (ns%1000)*1000` like GNU `Ftime_convert`. Fixed `relative/repeating_timer_microseconds`. |
| `69b874f9b` | S2 | `(:type seqpacket)` added to `make_network_process_subfeatures` (verified end-to-end vs GNU first: local seqpacket server+client+data byte-identical). |
| `0dd82abd1` | S3 | `decode_lisp_time` nil branch keeps the ns remainder (`ticks += psecs/1000`) — timer vectors built by timer.el's `timer--time-setter` now carry PSEC. Also normalized `div_u5_timer_create_cancel_reorder` to be clock-independent (raw `memq` tails embedded wall-clock timer vectors that can never match across processes); expectation regenerated and verified **via GNU** (refresh mode). |
| `bf71ed726` | S3 | `run_timers` flag threaded through the post-block service pass, false when the deadline elapsed at wake — GNU's loop-top deadline-break-before-`timer_check` order. Ready fds are still drained at the deadline (GNU reads the final pselect's fds). |
| `5829b6f11` | S3b | Dead native `TimerManager` deleted (timer.rs 695→~100 lines, keeping only `sleep-for`, which is C in GNU too). `eval.rs` field, the wait loop's empty-Vec fire pass, and keyboard.rs's always-None `next_fire_time` merge removed. Ten bare-Context unit tests rewritten to push real GNU timer vectors onto `timer-list` with a timer.el-shaped `timer-event-handler` stub — they now exercise the production dispatch path. |
| `d02f31edb` | S4a | D5 + D6a: no sentinel for sync connects (two creation sites removed); network EOF → `(exit . 256)` + sentinel text derived from status + reap per `delete-exited-processes`. Both network probe scripts diff byte-identical vs GNU (normalize ephemeral ports). The unit test that pinned creation-time `"open\n"` was retargeted to the `delete-process` `"deleted\n"` sentinel (same state-preservation assertions) with the sync-connect silence locked in by an explicit nil assertion; its expectation verified against live GNU. |

Oracle trajectory: pre-audit clean baseline 131 fails → **121** after S1–S4a
(all targets fixed: seqpacket, u5, both microseconds tests, cx179,
`network_client_open_delete_sentinels`), zero regressions across five clean
idle full-suite gates.

## 4. LANDED — S4b + S6 (commit `e53ccc2d5` lineage, pushed 2026-07-02)

Final form includes two refinements discovered during gating: (a) the
observation-point decode is PASSIVE — `process_effective_status` reads a
status already reaped by a wait iteration's poll; actively `try_wait`-ing at
`process-status` was tried and reverted because it let
`(while (process-live-p p) (accept-process-output p))` exit between waits
before any wait delivered the pending sentinel (cx423 process-send-eof caught
this); (b) the wait delivers the terminal notification in the SAME pass that
saw output + EOF + a reaped exit for pipes as well as ptys (was pty-gated).
kill-buffer's hangup no longer synthesizes a pending (signal . SIGHUP) at
send time — GNU only sends. Final gate: clean idle full regression, 129 fails
where every non-baseline name passed standalone x3 except the two documented
repeat-timer coin-flips. Historical in-flight notes below.

### (historical) S4b + S6 in-tree state

These two interlock: S4b (only output completes waits) removes the hazard that
previously made S6's observation-time polling unsafe (a poll that parked a
pending status used to complete waits spuriously via the status-notification
activity path).

**S4b — completion semantics** (`process.rs`, `wait.rs`):

- `ProcessOutputServiceOutcome` gained a `serviced` flag alongside the
  completing `activity`; `record_serviced()` marks non-output servicing.
- Reclassified sites (all in the wait's service pass): `:nowait` connect
  completion ×3 (Retrying/Connected/Failed), server accepts, status
  notifications ×2, stderr-pipe terminal EOF, network EOF → serviced.
  Data reads (×3 sites) remain the only completing activity.
- `run_process_status_notification` now returns `(drained_output, notified)` —
  GNU `status_notify`'s return counts DRAINED bytes into `got_some_output`, so
  the drain completes waits while the sentinel run only services.
- New `WaitCompletion::TargetProcessTerminated`: a targeted wait
  (`accept-process-output PROC`) returns nil immediately when the target is
  missing (reaped) or `process_status_ends_target_wait` — the GNU-internal
  rule mapped onto neomacs's storage model:
  - stored `run` for a **network server** ends the wait (GNU stores `Qlisten`
    internally; neomacs stores `run` + projects via `process_contact_server_p`);
  - stored `run`/`open`/`connect` for anything else keeps waiting (io-paused
    connections included: pause is a separate flag, GNU-internal status stays
    `Qrun`);
  - `listen`/`stop`/`exit`/`signal`/`failed`/`closed` end the wait.
- Verified probes (byte-parity with GNU): listen-wait returns immediately
  `(nil t)`; open connection waits full timeout; io-paused connection waits
  full timeout with public status `stop`; sentinel-only activity returns nil;
  `:nowait` connect completes without terminating its wait `(nil t ("open")
  open)`; both network sentinel scripts byte-identical.

**S6 — observation points** (`process.rs`):

- New `process_effective_status(process)`: the GNU `update_status` view —
  decode `pending_terminal_status` when `status_notify_pending`, else stored
  status. Used by `process_public_status_symbol`, `process_live_status_value`
  (whose deliberate pending→`run` masking branch is removed), and
  `process-exit-status`.
- `builtin_process_status_impl` and `builtin_process_exit_status_impl` now
  poll `check_child_exit(id)` (non-blocking `try_wait`) before reporting —
  neomacs's equivalent of GNU's asynchronous SIGCHLD `raw_status_new`, decoded
  at exactly GNU's two observation points. The sentinel notification stays
  with the wait loop (tick/update_tick split preserved).
- Rationale for reversing the old "do not probe the OS here" comment: that
  comment guarded against the pre-S4b world where a status change completed
  waits. S4b removed that coupling.

**State at handoff (2026-07-02, S4b+S6 in tree, uncommitted —
`neovm-core/src/emacs_core/system/process/mod.rs` +
`neovm-core/src/emacs_core/system/wait/mod.rs` dirty):**

Verified green so far (release binary built from the in-tree state):
- All S4b probes byte-identical with GNU: listen-wait `(nil t)` immediate;
  open connection waits full timeout; io-paused connection waits with public
  `stop`; sentinel-only activity → nil; `:nowait` connect `(nil t ("open")
  open)`; both network sentinel scripts identical.
- S6 probes: `status-after-exit` → `(exit 0)` both; the exited-child wait
  contract verified byte-identical:
  `(accept-process-output P 0.5)` on a 200ms-dead `echo` child returns
  `(nil exit "out\nProcess x finished\n")` in BOTH GNU and neomacs (GNU
  breaks the targeted wait on the terminated child, drains the output, runs
  the default sentinel, reports `exit`).
- `div_v8_process_attributes_status_type_tty` (the S6 target) passes on an
  idle machine; the rest of the flaky family passes idle, still flips under
  load (their windows involve output-arrival order, not just status
  observation).

Remaining to finish S4b+S6 (exact list):
1. Retarget unit test
   `accept_process_output_keeps_status_pending_after_ready_output`
   (process_test.rs): it pins pre-S6 neomacs-only behavior `(t run)`. The
   GNU-verified contract is `accepted = nil`, `process-status = exit`, buffer
   = `"out\nProcess x finished\n"` (probe: `tmp/t_pending.el` pattern above).
   Rename it accordingly (e.g. `accept_process_output_target_terminated_...`).
2. Fix/retarget unit test
   `accept_process_output_defers_pty_status_after_explicit_coding`: the
   explicit-coding defer branch in `run_process_status_notification` now
   returns `(saw_output, false)`; the caller no longer records completing
   activity for the defer round. Read the test's intent (pty status deferral
   after explicit coding change) and check whether the defer branch should
   count drained output (`saw_output` may need to be recorded by the caller —
   it is, via `drained_output`) or whether the test just pinned the old
   bool-conflation. Verify the underlying scenario against GNU before
   changing either side.
3. Re-run: unit family (`-E 'test(process) or test(wait) or …'`), oracle
   family (257), then a **clean idle full regression**; diff leaf names vs
   the 121-fail baseline (`tmp/s4a_leaf.txt`); re-run any new failure
   standalone ×2.
4. Commit S4b and S6 as separate commits (message drafts: S4b = completion
   semantics / got_some_output parity + TargetProcessTerminated; S6 =
   observation-point status decode, reversing the old "do not probe" comment
   with the S4b rationale). Push (note: the pre-push hook runs `cargo fmt
   --check` + `cargo check`, which QUEUES on the target-dir lock if a build
   or nextest compile is running — be patient, it is not hung).

## 4.1. LANDED — S4c + S5 (current in-tree state)

S4c completed the remaining derive-at-notify sweep for terminal process
notifications. `notify_process_status_sentinel` now derives the sentinel text
from the process's current status at delivery time. GNU's literal push-style
exceptions remain literal: server accepts keep `"open from HOST\n"`, and
`:nowait` connect completion keeps `"open\n"` / `"failed with code N\n"`.

S5 implemented GNU's real-subprocess signal contract:

- Unix child status observation now uses `waitpid(WNOHANG | WUNTRACED |
  WCONTINUED)` before falling back to the portable child/pty paths, so
  `(stop SIG)` and continued `run` states are observed by the wait loop.
- `WIFSIGNALED` status preserves the core-dump bit, which keeps
  `quit-process` sentinel text at GNU's `"quit (core dumped)\n"` when the
  platform reports a core-dumping SIGQUIT.
- Real-process `stop-process` sends SIGTSTP only; it does not publish a stop
  status synchronously. `continue-process` sends SIGCONT and preserves GNU's
  immediate `run` status/notification behavior.
- Signal delivery targets the child process group on Unix, matching GNU's
  subprocess setup for pipe children.
- Input-only wait wakeups now still poll child status, so a quiet SIGSTOP or
  SIGCONT cannot be hidden behind command-input readiness.

Validated gates for this slice:

- GNU source study: `process.c` `status_convert`, `status_message`,
  `status_notify`, `Fstop_process`, `Fcontinue_process`, and `callproc.c`
  process-session setup.
- Live probes in `./tmp`: stop/continue, signal-combo, SIGSTOP continuation,
  and process-attributes/kernel-state checks matched GNU byte-for-byte.
- `cargo nextest run -p neovm-core -E 'test(process) or test(wait)'
  --no-fail-fast`: 284/284 passed.
- Focused oracle targets `div_u1_process_signal_combo`,
  `div_core_divergence_surface_quit_process_sentinel_message`, and
  `div_core_divergence_surface_stop_continue_delete_process_sentinels` passed.
- Process/network/timer oracle family after refreshing stale inline GNU
  expectations: 257 run, 255 passed, with only documented baseline failures
  remaining (`div_core_divergence_surface_process_attributes_running_child_combo`
  and `div_v3_window_margins_fringes_body_width_combo`).

## 4.2. LANDED — S7 (current in-tree state)

S7 implemented GNU's per-process output read sizing and adaptive
read-buffering bookkeeping:

- `read-process-output-max` is now snapshotted into each process record at
  allocation time (`readmax`), clamped to GNU's `[1, INT_MAX]` range, and used
  by all output sources: child stdout, split stderr pipe-processes, ptys, TLS
  streams, stream sockets, and datagram sockets.
- Lisp-facing creators refresh the snapshot source before allocation:
  `start-process`, `make-process`, `make-network-process`,
  `make-pipe-process`, and `make-serial-process`. Existing Rust helper
  constructors keep the GNU default (`65536`, adaptive disabled).
- Server accepts refresh the read config at accept time before allocating the
  client process, matching GNU's `server_accept_connection` -> `make_process`
  path for `readmax`. Accepted clients keep adaptive buffering disabled,
  matching the GNU source path, which does not set
  `p->adaptive_read_buffering` there.
- `process-adaptive-read-buffering` now drives GNU's delay ladder for positive
  reads: short reads (`<256` bytes) add 20ms up to 70ms; full reads subtract
  10ms; non-targeted service passes skip a delayed process once and wait-loop
  timeouts consider the adaptive delay capped at 50ms. Targeted
  `accept-process-output PROC` remains eager, matching GNU's "not reading
  output for a specific process" guard.
- Process output decoding now keeps GNU-style carryover between reads before
  calling filters. Verified cases include split UTF-8 sequences, incomplete
  UTF-8 flushed at EOF, and split DOS `\r\n` under `utf-8-dos`; the wait loop
  treats bytes that only fill decoder carryover as GNU `got_some_output` even
  when no filter string is emitted yet.

Validated gates for this slice:

- GNU source study: `process.c` `make_process`, `Fmake_process`,
  `Fmake_pipe_process`, `server_accept_connection`,
  `wait_reading_process_output`, and `read_process_output`.
- Live probes in `./tmp`: GNU and release Neomacs both return `(5 5 5 1)` for
  the `read-process-output-max` snapshot case; split UTF-8 returns
  `((1 (233)) (1 (88)))`; incomplete UTF-8 at EOF returns
  `((1 (4194243)))`; split `utf-8-dos` CRLF returns `((1 (10)) (1 (88)))`.
- Focused unit tests
  `read_process_output_max_limits_filter_chunks_and_snapshots_at_creation`
  `read_process_output_carries_split_decode_sequences_between_chunks`, and
  `adaptive_read_buffering_updates_delay_with_gnu_thresholds` passed.
- Focused oracle tests
  `process_wait_semantics::read_process_output_max_limits_filter_chunks_and_snapshots_at_creation`
  and
  `process_wait_semantics::read_process_output_carries_split_decode_sequences_between_chunks`
  passed with inline GNU expectations.
- `cargo nextest run -p neovm-core -E 'test(process) or test(wait)'
  --no-fail-fast`: 287/287 passed.
- Process/network/timer oracle family: 257 run, 256 passed, with only the
  documented display-backend baseline failure
  `div_v3_window_margins_fringes_body_width_combo`.

## 4.3. LANDED — S8 (current in-tree state)

S8 implemented GNU's `send_process` write-queue and re-entrant wait behavior:

- `write_queue` entries now use GNU's shape `(STRING . (OFFSET . LENGTH))`.
  New sends append to an existing queue; blocked remainders are pushed back to
  the front before waiting.
- `process-send-string` and `process-send-region` encode through the process's
  input coding system, queue the bytes, then flush through a single
  re-entrant path. On `EAGAIN` / `WouldBlock`, the flush registers writable
  interest and waits for up to 20ms with `wait_for_process_output`, so filters,
  sentinels, timers, and async connect completion can run during the send just
  as GNU permits.
- Pipe child stdin is switched to non-blocking mode before writes. PTY writers,
  TLS streams, stream sockets, seqpacket streams, UDP sockets, and Unix-domain
  sockets all use single-attempt writes/sends through the same queue flush.
- The event-pool writable path now flushes non-empty process write queues as
  well as pending `:nowait` connects. Once a queue drains, writable interest is
  removed and readable interest is kept.
- `BrokenPipe` during a write publishes GNU's network-style broken output
  status `(exit . 256)` and signals `"Process NAME no longer connected to
  pipe; closed it"`. Real no-output-source records still signal a closed fd;
  harness-only records keep their queued data so bytecode shared-runtime unit
  tests can exercise the Lisp-visible queue shape without an OS child.
- Sends to listener/server processes are rejected before entering the write
  queue, using GNU's `status_message`-style reason text (`listen`, with no
  trailing newline).
- Successful writes reset adaptive read delay when
  `process-adaptive-read-buffering` is `t`, matching GNU's
  `p->read_output_delay = 0` reset in `send_process`.

Validated gates for this slice:

- GNU source study: `process.c` `send_process`, `write_queue_push`,
  `write_queue_pop`, `status_message`, and the `wait_reading_process_output`
  re-entry call used after `EAGAIN`.
- Live probes in `./tmp`: GNU and release Neomacs both return `(t 4 262144)`
  for `send-reentrant-probe.el`, proving that `process-send-string` drains
  child output via the filter while the send itself is still in progress.
  `send-listener-probe.el` matches GNU exactly:
  `(error "Process send-listener-probe not running: listen")`.
- Focused unit tests for write-queue shape, process designators, region
  sends, and re-entrant blocked writes passed:
  `process_manager_send_input`,
  `builtin_process_send_string_preserves_raw_unibyte_write_queue_entries`,
  `process_send_string_accepts_get_process_designators_like_gnu`,
  `process_send_region_accepts_get_process_designators_like_gnu`, and
  `process_send_string_reenters_wait_and_runs_filter_when_write_blocks`.
  The listener/server rejection regression
  `process_send_string_rejects_network_server_like_gnu` also passed.
- Focused oracle test
  `process_wait_semantics::process_send_string_reenters_wait_and_runs_filter_when_write_blocks`
  passed with inline GNU expectation `"OK (t 4 262144)"`; listener/server
  oracle
  `process_wait_semantics::process_send_string_rejects_network_server_like_gnu`
  passed with the exact GNU not-running message.
- `cargo nextest run -p neovm-core -E 'test(process) or test(wait)'
  --no-fail-fast`: 289/289 passed.
- Process/network/timer oracle family: 258 run, 257 passed. The only failure
  was the documented display-backend baseline
  `div_v3_window_margins_fringes_body_width_combo`.

## 4.4. LANDED — S9 (current in-tree state)

S9 implemented async DNS for `:nowait` TCP clients and removed the global
50ms wait-iteration cap:

- `make-network-process :nowait` with a real hostname now creates the process
  in `connect` state immediately and stores a `PendingNetworkConnect::Dns`
  request. A Rust resolver thread performs `getaddrinfo`; it never touches
  Lisp state and only sends address data back through a channel plus
  `Poller::notify()`.
- The wait service pass checks completed DNS before socket I/O, matching GNU's
  `check_for_dns` position near the top of `wait_reading_process_output`.
  Successful DNS hands the address list to the existing non-blocking TCP
  connect path; DNS failure records GNU's string-valued failed status
  `(failed "Name lookup of HOST failed")` and does not run a sentinel.
- Invalid hostnames that libc would reject (`-bad.example`, interior empty
  labels, whitespace, overlong labels) are represented as an already-ready
  async DNS failure. Root/trailing-dot names are left to the resolver; the
  host-candidate probe keeps `"."` matching GNU's environment-dependent
  resolver result. This preserves GNU's Lisp-visible sequence (`connect` now,
  `failed` at the next wait) without letting the resolver retry for tens of
  seconds.
- Already-known local/literal destinations (`:host nil`, `:host 'local`,
  `"localhost"`, and numeric IP literals) continue to use the immediate
  non-blocking connect path so `process-contact` exposes remote/local vectors
  at construction time, matching GNU probes.
- `WaitRequest::base_timeout` now uses the exact remaining deadline, with GNU's
  long 100000-second block for forever waits. Timer, adaptive-read, and fd
  readiness still reduce or wake the block through their existing paths.

Validated gates for this slice:

- GNU source study: `process.c` `Fmake_network_process` around
  `getaddrinfo_a`, `check_for_dns`, `connect_network_socket`, and the
  `wait_reading_process_output` timeout calculation / async retry logic.
- Live probes in `./tmp`: GNU and release Neomacs both return
  `(ok (connect (connect stop)) failed nil)` for async DNS failure,
  `(connect nil failed nil)` for targeted accept return, and
  `(ok (connect (connect stop)) failed nil nil)` for the sentinel probe
  (DNS failure does not run the sentinel). The host-candidate probe matches GNU
  for `""`, `"."`, `"-bad.example"`, `"bad..example"`, `"bad host"`,
  `"::bad::"`, and an overlong-label hostname. `:host 'local :nowait` still
  returns `(connect (connect stop) t t)` for immediate remote/local contact
  vectors.
- Focused unit tests
  `make_network_process_nowait_hostname_dns_failure_is_async_like_gnu`,
  `wait_request_exposes_scheduler_queries`,
  `make_network_process_nowait_tcp_loopback_opens_like_gnu`, and
  `process_send_string_waits_for_nowait_tcp_connect_like_gnu` passed.
- Focused oracle test
  `process_wait_semantics::make_network_process_nowait_hostname_dns_failure_is_async_like_gnu`
  passed with inline GNU expectation `"OK (connect nil failed nil)"`.
- `cargo nextest run -p neovm-core -E 'test(process) or test(wait)'
  --no-fail-fast`: 290/290 passed.
- Process/network/timer oracle family: 259 run, 257 passed. The only failures
  were documented baseline cases:
  `div_v3_window_margins_fringes_body_width_combo` and flaky
  `div_core_divergence_surface_process_attributes_running_child_combo`, which
  passed standalone three times.

## 5. REMAINING SLICES — detailed execution guidance

### S10 — Lisp threads (D14) — RESOLVED

**Status (verified 2026-07-20):** all four divergences below now match GNU.
Live-mode parity against GNU 31.0.90 passes for **153** thread/mutex/
condition oracle tests (`NEOVM_ORACLE_MODE=live`), plus 65 neovm-core
`threads` unit tests. neomacs's cooperative simulation (`threads.rs` —
single OS thread, `make-thread` runs the thunk and parks it at cooperative
blocking points) was extended to cover the GNU semantics: a second
`mutex-lock` parks the worker (it stays `thread-live-p` t and appears in
`all-threads`) instead of erroring, and dynamic bindings are swapped per
thread (a `let` in one thread is invisible in another). The four
`div_core_divergence_surface_*` tests in
`core_subsystems_strict.rs` now assert the GNU value and pass; their inline
"Neomacs: …" comments document the historical (pre-fix) divergence only.
The original spec is retained below for reference.

- GNU model (`thread.c`): every Lisp thread is an OS thread holding THE
  global lock while running Lisp; switches happen only at blocking points
  (`thread_select` inside `wait_reading_process_output`, `mutex-lock`,
  `condition-wait`, `thread-yield`, `sleep-for`). Dynamic bindings are
  swapped on context switch: `unbind_for_thread_switch` /
  `rebind_for_thread_switch` walk the outgoing/incoming thread's specpdl.
- Four deterministic oracle divergences:
  1. `mutex_lock_blocks_other_thread` — NEO errors "Cannot unlock mutex owned
     by another thread" where GNU's second thread blocks parked on the mutex.
  2. `thread_dynamic_binding_isolation` — a `let` in one NEO thread is
     visible in another (GNU: `nil`, NEO: `(main "*scratch*" local)`); needs
     the specpdl swap.
  3. `all_threads_includes_live_worker` — a worker blocked in `mutex-lock`
     must appear in `all-threads` with `thread-live-p` t.
  4. `thread_signal_condition_handler` — `(void-variable log)` propagation
     detail in the handler path.
- This is an architecture slice: audit how neomacs implements `make-thread`
  today (OS threads? green?), then either implement the GIL + binding-swap
  model or document divergence. Estimated as the largest remaining item.

## 6. Methodology (hard-won, follow these)

- **Gates**: every slice gets (a) targeted GNU-vs-NEO probes (write forms to
  `./tmp/*.el`, run both binaries, `diff` normalized output), (b) neovm-core
  unit tests for touched paths, (c) the oracle process/network/timer family
  (`-E 'test(process) or test(network) or test(timer) or …'`, ~257 tests,
  seconds), (d) a **clean idle full regression** before push (~30 min;
  38.5k tests). Compare leaf-name sets against the previous clean baseline
  with `comm`; re-run any "new" failure standalone ×2 before believing it.
- **Full regressions are only valid on an idle machine.** A run concurrent
  with builds/tests produced 127 timeouts (baseline: 6) and a garbage diff.
- **`cargo build | tail -1 && …` masks failures** — the `&&` sees `tail`'s
  exit code. Use `set -o pipefail`, check `grep -c "^error"`, or verify the
  binary mtime.
- **Rebuild ⇒ pdump regen**: `cargo xtask fresh-build --release --skip-build
  --no-byte-compile` after every `cargo build --release -p neomacs`, or the
  binary panics on the stale dump.
- **Oracle expect strings come from GNU**: `NEOVM_ORACLE_MODE=refresh
  UPDATE_EXPECT=1 cargo nextest run -p neovm-oracle-tests <test>` regenerates
  from live GNU. Never hand-write them. Live mode ignores expects (parity
  only); refresh mode asserts expect == GNU.
- **Study GNU's C before classifying any divergence** as expected/artifact —
  two earlier misclassifications (window top_line, "queued pipeline") were
  both corrected only after reading the source. Empirical probes beat
  reasoning about microdynamics: when GNU behavior looks jitter-dependent,
  run GNU 10× (see the cx64 boundary study).
- **GNU does it in Lisp ⇒ neomacs must not do it in Rust.** The timer brain
  is the canonical example: the fix was deleting Rust code and trusting
  timer.el.
- Timing tests that print wall-clock-bearing values (timer vectors, ephemeral
  ports) can never match across two processes — normalize the form, not the
  harness.

## 7. Known-remaining divergences outside this subsystem

- **EXPECTED-TIMING**: the exact-boundary repeating-timer trio
  (`div_cx64_timer_repeated_invocation_in_order`,
  `div_cx179/cx338_repeat_timer_fires_multiple`) — fire-at-deadline is a race
  GNU itself flips on (2/10 idle runs); visible mainly on idle machines.
- **Display-backend geometry** (~15 tests): window-pixel/frame metrics,
  wrapped `vertical-motion`, `count-screen-lines` under the `neo` backend.
- **EXPECTED**: `neo` ≠ `x` window-system probes; absent features
  (native-comp, treesit, dbus, lcms2, sqlite differences); `features` list
  contents; GC-count internals; weak-hash GC-conservatism.
- **Noted real bug (unfixed, small)**: `split-window-below` leaves the
  window's horizontal `normal-size` at 0.5 after split+delete (should be
  1.0; a vertical split must not touch horizontal fractions). See
  `refactor/window-topline-decouple` merge (`25a0ae2b3`) for the adjacent
  geometry model.
