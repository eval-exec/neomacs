# b6 plan: the GNU thread suite with an era open and one idle opt-in owner

Owner: peer (claude-ds). Date: 2026-11-25. Base: main `5f5a239737`. Status: **r3 plan; harness landed as r3 rows**.

Moved here from the scratch planning tree on 2026-11-26 (r3) so the plan travels with the code it
describes: a plan that lives only under `tmp/` is a dead reference for every reader but its author.
r3 amends §4.1: the covering claim is asserted on all 30 parity rows through the witness, not by a
separate named row, and names the row it excludes.
r2 (peer, 07:4xZ) folds the lead's rulings #14342 into §4.2, §5, §8 and §9: Q1 yes, Q2 yes, Q3 yes,
Q4 = option (b) as its own slice with a self-containment fallback. r1 (peer, 07:38Z) added §4.2,
the §4.1 row audit, the lexical-capture correction in §2.1, the §5 status, and Q4.
Every GNU-behavior claim here is a source reading **UNATTESTED** until b6 itself runs on the
attested oracle. b6 is the qualification that must be green before production admission of
`neomacs-make-parallel-thread` turns ON (L6.7b-BATON-GROUP.md §b6, E-SPINE-PLAN Track P).

## 1. What b6 must establish

b6 has to show one thing, with evidence:

> With a parallel keeper era open and one idle opt-in owner admitted, the GNU thread suite's
> Lisp-observable answers are identical to GNU 31.1 at the pinned commit.

It is a *parity* gate, not a parallelism gate. It does not prove two owners running together
(E6-P.3), and it does not claim any speedup. Its purpose is to show that the mere existence of
the era and an idle owner changes nothing a Lisp program can observe — the precondition for ever
letting a real program opt in.

Two failure modes it exists to catch:
- an open era changes serial answers (GC pacing, thread registry, teardown at exit, keeper wake
  storms), and
- the harness passes vacuously because the owner was never admitted (no era, silent Baton
  fallback).

## 2. Chosen harness shape (recommended)

**Replay the suite rows behind a GNU-legal, `fboundp`-guarded prelude that admits one idle owner
and writes a witness file.** No new binary knob, no feature-gated build, no hidden production
door.

### 2.1 The prelude

Each b6 arm runs the row as `(progn <prelude> <row-form>)`. The prelude:

```elisp
(when (and (getenv "NEOVM_B6_ERA")
           (fboundp 'neomacs-make-parallel-thread))
  (let* ((m (make-mutex)) (c (make-condition-variable m "b6-idle"))
         (parked nil)
         (owner (neomacs-make-parallel-thread
                 (lambda ()
                   (mutex-lock m) (setq parked t)
                   (condition-wait c))   ; never notified: parks to exit
                 "b6-idle-owner"))
         (deadline (+ (float-time) 5.0)))
    (while (and (not parked) (< (float-time) deadline)) (thread-yield))
    (when-let ((path (getenv "NEOVM_B6_WITNESS")))
      (write-region
       ;; `admitted' is the honest bit: the creator returns a thread even when
       ;; it falls back to Baton, so only `neomacs-thread-parallel-p' proves an
       ;; admission. A refused creator writes admitted=nil.
       (format "admitted=%s parked=%s\n"
               (and (fboundp 'neomacs-thread-parallel-p)
                    (funcall 'neomacs-thread-parallel-p owner))
               parked)
       nil path))))
```

The `(getenv "NEOVM_B6_ERA")` guard is essential and was a caught bug in the first draft:
`fboundp` alone is true in **every** Neomacs arm, so without it the Sim and Green control arms
would also admit an owner and the controls would stop being controls. The env var is set only on
the b6 arm; GNU never sees it and skips the body either way.

The `let*`-bound mutex, condition variable and `(setq parked t)` flag read from the owner thread
follow the exact idiom `e8s_thread_signal_to_worker_pending_at_join_entry`
(`oracle_default_threads.rs:203-226`) already proves GNU-legal, so the prelude introduces no new
binding contract. The mechanism is **lexical capture, not dynamic binding**: the harness evaluates
each form with `(eval (read source-buf) t)` (`common.rs:900` and `:959`), so the thread's lambda
closes over `m`, `c` and `parked` in the shared lexical environment. Do not move the prelude out of
that lexical `eval` argument, and do not rely on specpdl/dynamic bindings surviving into the owner
thread (GNU's new thread has its own). The prelude sets no global symbol that a row reads; the plan
verifies that by grepping the row bodies for `b6-` before landing.

Why this is the right shape:
- **GNU-legal.** GNU has no `neomacs-make-parallel-thread`, so `(fboundp ...)` is nil, the whole
  body is skipped, and GNU's printed outcome is exactly the row's. The stored expectations stay
  valid with no refresh.
- **It exercises the production route.** The era opens at the *first opt-in admission* through
  the real creator, keeper start, registration and activation — not through a test double. That
  is the policy E6-P owes (E6P-INTEGRATION-MAP step 4).
- **No production default moves.** `make-thread` never admits; only a program that calls the
  doc-hidden creator does, and nothing in the suite does except this prelude.
- **Non-vacuous.** The witness file is written from inside the same process that answered, so a
  build that silently refused admission writes `admitted=nil` and the arm fails.
- **It also covers exit with a parked owner**, in every row: the owner is still in
  `condition-wait` when `kill-emacs` runs. That is a real teardown exercise (G5 territory).

The idle owner deliberately sets `parked` and then waits forever: "idle" means it never touches
Lisp state after parking, and the 5 s bounded wait makes a never-scheduled owner a red, not a hang.

### 2.2 Witness plumbing in the oracle harness

There is an in-tree precedent for exactly this: `NEOVM_JIT_STATS_FILE` plus
`assert_oracle_parity_under_envs_expect_with_witness` (crates/neovm-oracle-tests/src/common.rs
:1949-2078). It reads a report the *same run* produced and turns a silent no-op arm red.

b6 needs the same for the era:
1. New helper `assert_oracle_parity_under_envs_expect_with_b6_witness(form, envs, expected)` (or
   generalize the witness type), which per arm:
   - creates its own `tempfile::tempdir()`,
   - sets `NEOVM_B6_WITNESS=<dir>/era-witness.txt` in `extra_env` (Neomacs arms only; GNU runs
     with no extra env),
   - after the parity compare, requires the file to exist and to contain exactly
     `admitted=t parked=t`.
2. A missing file, `admitted=nil`, `parked=nil`, or `parallel=nil` all fail the row with the
   reason named — never "skip".
3. `check_form_source` must accept the prelude (it is a complete form; the prelude is composed as
   `(progn ...)` in the test source).

The witness is a file, not printed output: printing would enter the parity payload.

### 2.3 Negative control (red first)

The witness must be able to fail. Before the green run, take one row with the prelude and:
- mutate 1: build the arm with the creator still refusing (`ParallelAdmission` uninhabited, i.e.
  today's tree) → the row's *answers* still match GNU but the witness says `admitted=nil` → the
  b6 helper must fail. This is the vacuity red: it proves the gate cannot pass without an era.
- mutate 2: drop the `(while (not parked)...)` wait and make the owner function return
  immediately → `parked=t` still (it sets it), so instead mutate the owner to exit without
  parking → `parked=nil` red. (Cheap sanity that the witness reads the run.)

Record the first as the named red; the second is optional.

## 3. Alternatives rejected

- **Env knob in the binary** (`NEOMACS_TEST_PARALLEL_ERA=idle-owner`). It needs a door in a
  production binary that admits a parallel owner with no Lisp request. That is precisely the
  capability "production admission stays OFF" forbids, and it would be reachable by accident.
- **Feature-gated (test-support) oracle binary.** The oracle recipe requires the release pdump
  binary (`scripts/oracle-full-run.sh`); a test-support release build changes layout-pin cfg
  classes and feature unification, so its provenance is not the shipped artifact. It also still
  needs a knob to decide what to do. Keep this only as a fallback if the creator cannot be called
  from Lisp before loadup for some reason; in that case the plan must state the provenance delta.
- **In-process integration test comparing to checked-in expectations.** That is snapshot-grade
  evidence, not an attested oracle run. It may be added *in addition* (it would catch a witness
  bug cheaply), but it cannot be the b6 gate.

## 4. Scope: rows and modes

File: `crates/neovm-oracle-tests/tests/oracle_default_threads.rs` — 31 tests today
(24 `e8s_`, 3 `e4p2_`, 2 `e6p1_`, 2 `p15_`). The historical E8-S suite is the 19 cases in the S3
baseline (E-SPINE-PLAN Track S); the file has grown since, so b6 names the delta from 19.

| Arm | Env | Purpose |
|---|---|---|
| GNU | none | the one expectation |
| Green (control) | `NEOVM_THREADS=green` | today's Green answers |
| Green + era (b6 arm) | `NEOVM_THREADS=green`, `NEOVM_B6_ERA=1`, `NEOVM_B6_WITNESS=…` | the claim, with the witness |
| Sim (control) | none | unchanged default dispatch |

The b6 arm adds `NEOVM_THREADS=green` because a parallel era is a Green-runtime concept; Sim has a
simulated scheduler and no keeper. The Sim control is kept so the default dispatch is proven
untouched by the same row definition. `NEOVM_B6_ERA` gates the prelude (§2.1): the two control
arms must stay bit-identical to today's rows.

### 4.1 One row cannot carry a live owner: the `all-threads` count

`e8s_make_thread_arity_and_main_thread_identity` (oracle_default_threads.rs:63-73) prints
`(length (all-threads))`, which GNU answers `1`. An admitted owner is a real thread object and a
real member of `all-threads`, so in the b6 arm the same form legitimately answers `2`. That is not
a divergence; it is the owner existing. No single form can produce GNU's `1` and also prove the
owner is present.

**Recommended: exclude that one row from the b6 arm and cover it with a stronger Neomacs-only b6
row** (`b6_an_admitted_owner_is_the_only_thread_the_era_arm_adds`), which asserts on the b6 arm:
`(length (all-threads))` = 2, the second element is the owner, `neomacs-thread-parallel-p` = t for
it and nil for main, and the owner is still parked at the end. The existing parity row keeps
running unchanged in the Sim and Green controls, so nothing is weakened or deleted.

Resulting b6 numbers (r3, lead-ruled 2026-11-26): **30/30 parity rows + 1 named exclusion, with the
covering claim asserted on every row.** Because the excluded row is one of the 19 S3-baseline cases,
b6 is **18/19 of the historical suite by parity** plus the per-row thread-count assertion.

There is no separate named covering row: the oracle crate has no public way to run Neomacs alone with
`extra_env` (every such helper routes through `run_parity_under_envs`, which compares against GNU in
verify and live modes), and adding a private runner path to the most sensitive test crate to buy back
one named row is more risk for less coverage. Instead the prelude writes `threads=<n>` into the
witness and every one of the 30 parity rows asserts `threads=2`, so an era arm that admitted zero or
two owners is red on every row rather than in one place. The rejected alternative — running the row as
`(length (delq b6-idle-owner (all-threads)))` — would edit a passing row's form, which the
repository's "never adjust a test to fit" rule discourages.

**Row audit (peer, 2026-11-25 07:38Z): the exclusion set is exactly one row.** Grepping all 31
forms for thread enumeration: only line 70 prints a count. The other `all-threads` uses
(`:101`, `:122`, `:123`, `:125`) are `(memq th (all-threads))` / `(memq main-thread ...)`
identity tests, which tolerate an extra idle owner unchanged. The `condition-notify-one wakes
exactly one waiter` row (`:444`) parks its waiters on its own condition variable, so the prelude's
idle waiter does not perturb it. No other row needs an exclusion.

### 4.2 Neomacs-only rows for what L6.7c (a) changes inside an era turn

L6.7c (a) (gcfix `5f76462da3`, peer-reviewed by this session)
inhibits safe-point GC by holding `gc_inhibit_depth` for the whole `SerialTurn`. That counter is a
shared multi-purpose flag, so a whole-turn hold also changes four other consumers *for the duration
of a turn*:

| consumer | effect inside an era turn |
|---|---|
| `green/switch.rs:367` `switch_refusal` | a thread switch is refused with GNU's "Cannot switch threads while garbage collection is inhibited" |
| `misc_pure.rs:405` | `set-message-function` is **not** called by `message` |
| `context_api.rs:1818` | `clear-message-function` is **not** called |
| `sort_predicate.rs:282` | the buffered-sort fast path defers (invisible to Lisp) |

Default Green runs are unaffected (a `SerialTurn` exists only in the opt-in parallel path), which is
why (a) is signable. But these are user-visible Lisp answers inside an owner's turn, and b6 is the
gate that must pin them. They cannot be parity rows (GNU has no parallel era — it would just run the
row), so they are Neomacs-only rows in the §4.1 covering-row style, each asserting the GNU answer:

- `b6_message_runs_set_message_function_inside_an_era_turn` — bind a counting
  `set-message-function`, call `(message "x")` from *inside* the admitted owner's turn, assert it ran
  once. **Red on today's tree** by the table above.
- `b6_thread_yield_inside_an_era_turn_is_not_refused` — `(thread-yield)` inside the owner's turn
  returns nil (GNU's answer), not a "GC is inhibited" signal. **Red on today's tree.**

Same rule as §7: a red here is a finding to report, not to route around. **RULED (Q4, #14342):
option (b) as its own slice** — a dedicated per-turn inhibit field read only by
`gc_safe_point_exact_should_collect`/`should_collect_stopped`, leaving `gc_inhibit_depth` for the
short GC-critical scopes it was built for. The whole-turn hold changing `switch_refusal`,
`set-message-function`, `clear-message-function` and the sort fast path is a *silent user-visible
Lisp divergence* and should not be the default if a small typed field removes it.

- The slice lands after L6.7c (c) and before ADMISSION-ON, and does **not** gate b6 (b6 is written
  against current behavior with the §4.2 rows red).
- Self-containment condition: if the field cannot land as its own slice — i.e. it would touch the
  keeper driver or widen the turn API — stop and fall back to (a) (accept the three era-turn
  divergences, document them here, keep the two §4.2 rows), and mail the lead *before* switching.
- The two §4.2 Neomacs-only rows are required in both options.

b6 is one gate, not a rerun of every oracle suite. Suites outside the thread file are covered by
the normal gate; the risk b6 addresses is concentrated in thread semantics.

## 5. Preconditions and sequencing

b6 cannot run before all of these are on the branch it runs against:

1. **b4.3** (threadsig) signed — Green switch points, interval-end seams, QD1/QD3 guards.
2. **b4.4** (gcfix) signed — the WorldTurn GC funnel; and **L6.7c (a)+(b)+(c)** — SerialTurn GC
   inhibit, alloc-region close at detach, and the explicit `garbage-collect` owner shape. Without
   (c), a row that calls `garbage-collect` in the main thread while an owner is parked is the
   open risk. Status 2026-11-25: (a)+(b) are READY at `5f76462da3` (peer review SIGN with nits);
   (c) driver slice is READY `094e0d35a2` awaiting the lead's sign, and its Context side follows
   b4.4. (a)+(b) alone do **not** make `(garbage-collect)` inside a turn safe
   (`builtin_garbage_collect` bypasses the inhibit; `misc_eval.rs:806`).
3. **An inhabited admission** (E6-P.1's `ParallelAdmission` given a production route) and the
   **E6-P.4 admission path** — the legacy wait bridge (G4) so the admitted owner can be started
   while main is in batch evaluation, and so the era construction passes `Gen1Admission`.
4. E6-P.2/P.3 may land after b6 where they do not change the idle-owner path, but the parking and
   exit teardown rows b6 exercises must exist.

b6 is the gate that precedes turning production admission ON. **RULED (Q3, #14342):** the sequence
is three separate signed steps, each its own READY/sign — (1) E6-P.4's admission path first,
dormant (production admission still OFF); (2) b6; (3) the ADMISSION-ON commit as a separate
signed change after b6 is green. b6 is not merged into admission, and admission does not flip in
b6.

## 6. Commands and attestation

Attestation (AGENTS.md §1), local oracle under `./tmp/gnu-emacs-oracle`:

```bash
prefix="$(git rev-parse --show-toplevel)/tmp/gnu-emacs-oracle"
scripts/ci/check-gnu-oracle.sh \
  "$prefix/bin/emacs" "$(awk -F'"' '/^emacs_version = /{print $2}' parity-reference.toml)" \
  "$prefix/share/gnu-emacs-oracle/build-manifest" \
  "$(awk -F'"' '/^mirror_commit = /{print $2}' parity-reference.toml)"
# paste the profile line and the feature line into the b6 record
```

b6 run (verify mode, attested oracle forced; the Neomacs binary is the release pdump build of the
b6 branch, built with `cargo xtask fresh-build --release`):

```bash
NEOVM_ORACLE_MODE=verify \
NEOVM_FORCE_ORACLE_PATH="$prefix/bin/emacs" \
NEOVM_BINARY_PATH="<worktree>/target/release/neomacs" \
  cargo nextest run -p neovm-oracle-tests -E 'test(oracle_default_threads)' --no-fail-fast
```

**Launch the run from the tree the binary was built in.** `ensure_fresh_build_attested`
(`common.rs:472`) hashes `<NEXTEST_WORKSPACE_ROOT>/lisp` — nextest sets that to the workspace it is
invoked in — and compares it with the stamp recorded for `$NEOVM_BINARY_PATH`. `NEOMACS_RUNTIME_ROOT`
is not consulted by that check. A run started from a different worktree fails all rows at harness
setup (`the Lisp tree … does not match the manifest recorded by the fresh-build stamp`), which is
neither a b6 red nor a b6 green. For the scratch flip run (§8 Q3) this means the b6 test files are
copied into the flip tree and nextest is invoked there; `NEOVM_SKIP_FRESH_BUILD_STAMP=1` is never
used, because it would brand the run unattested.

Requirements on the record:
- the profile and feature lines from `check-gnu-oracle.sh`;
- the exact binary provenance (source SHA of the b6 branch tip, `fresh-build --release` stamp);
- per-arm counts: 30/30 parity rows for the b6 arm, each with `threads=2` asserted from its witness
  (the named exclusion and why it is excluded, §4.1), 31/31 for each control, and the witness line for
  every b6-arm row;
- a `snapshot` run is NOT evidence here (it compares to checked-in strings).

`scripts/oracle-full-run.sh` is the wrapper the gate uses for a full pass; b6 may use it with the
test filter above, as long as the record names the filter.

## 7. Risks and limitations

- **Owner scheduling.** The 5 s bounded `parked` wait can flake on a loaded host (a green that
  becomes red under load is worse than a slow row). Bound it generously (5 s) and treat a timeout
  as a *failure with the elapsed time in the message*, not a skip. Repeat-run the b6 arm 3× and
  require 3/3.
- **The era arm doubles green-run time** for this file (one extra Neomacs process per row). That
  is ~31 extra processes; acceptable. Do not batch rows into one process — each row must start
  with a fresh era.
- **Exit with a parked owner** is exercised implicitly by every row. If `kill-emacs` hangs with a
  parked owner, b6 reds with a timeout, which is the correct signal (it is the teardown b6 should
  catch). Name the failing row in the message.
- **Witness write path.** The sandbox's default case filesystem is private (no writable TMPDIR
  env); the witness path is an absolute path the helper owns, and `write-region` to it is
  unaffected by the private policy. Confirm on the first run that the file appears; if the pinned
  `HOME`/umask policy interferes, write under the helper's tempdir as designed.
- **What b6 does not prove:** two owners running together (E6-P.3), any throughput, any GNU
  interleaving (the atomic contract with histories is pentry DESIGN §2), host-affine primitives
  from an owner (H2/H3), or interpreted thread functions (G2, whole-call escape in v1).
- **A GNU thread-suite row that diverges only with the era open** is a real finding. Do not route
  the row around it and do not weaken it; report it, as with the SerialTurn GC finding (#14268).

## 8. Questions for the lead — ALL RULED (#14342)

- **Q1 (scope).** Ratify the §4.1 exclusion: b6 parity is 30/30 rows plus a covering Neomacs-only
  row for the `all-threads` count, rather than touching the passing row's form to reach 31/31.
  The L6.7b wording "19/19 must match" then reads 18/19 by parity plus one stronger row.
  **RULED: YES.** Keep the exclusion named and the covering row's assertion explicit.
  **AMENDED 2026-11-26 (r3):** the covering assertion is `threads=2` on all 30 parity rows read from
  the witness, replacing the separate named row — the crate has no public Neomacs-only runner with
  `extra_env`, and 30 per-row counts catch a zero- or two-owner arm everywhere instead of once.
  Re-ruled YES on that basis.
- **Q2 (owner of the harness change).** The witness helper (§2.2) lives in
  `crates/neovm-oracle-tests/src/common.rs`, next to the JIT witness. The peer proposes to own it
  and the prelude plumbing as a test-only READY, since the oracle crate is not a lane's file.
  **RULED: YES**, peer owns it; the READY must prove test-only with a file list.
- **Q3 (admission sequencing).** Confirm the split: E6-P.4's admission path lands first (dormant,
  no default change), then b6, then the ADMISSION-ON commit as a separate signed change. If the
  lead prefers admission and b6 in one batch, b6's red receipt must be taken on the same tip
  before the default flips.
  **RULED, and the sequence is binding:** b6 runs first on a scratch tree whose only delta is
  `PRODUCTION_ADMISSION_ENABLED` flipped to true (disclosed in the READY with the diff and the
  binary's source SHA). That run justifies landing the flip. Once the flip is on main, **b6 runs
  again on the landed tip and that second run is the standing evidence** — the scratch run is not,
  because a run on a tree that is not main cannot certify main. A disagreement between the two runs
  is a finding about the constant's reach and is surfaced before any release.
  **b6 must therefore be green on the landed tip before production admission is considered on**,
  not merely green on the scratch tree.
- **Q4 (the whole-turn `gc_inhibit_depth` hold, §4.2).** L6.7c (a) reuses a flag four other
  consumers read. Options: (a) accept the era-turn divergences and pin them as known limitations;
  (b) narrow it now — add a dedicated per-turn inhibit field consulted only by
  `gc_safe_point_exact_should_collect`/`should_collect_stopped`, leaving `gc_inhibit_depth` for the
  short GC-critical scopes it was built for. The peer recommends (b) *if* it can land as its own
  small, test-only-plus-one-field slice before admission ON; otherwise (a) with the §4.2 rows
  marked as accepted divergences. Either way the two §4.2 rows must exist and be honest.
  **RULED: (b), with the self-containment fallback to (a)** — see §4.2 for the conditions. b6 does
  not wait for (b).

## 9. Deliverables

1. The harness helper + witness (§2.2), with the vacuity negative control red (§2.3).
2. The prelude composed into every row (a shared `const B6_PRELUDE: &str` and a
   `assert_b6_threads_oracle(form, expected)` helper that runs `(progn prelude form)` under the
   four arms).
3. The §4.2 Neomacs-only rows (message/set-message-function and thread-yield inside an era turn),
   required in both Q4 options.
4. The b6 record: attested-oracle lines, commands, per-arm counts, witness samples, binary
   provenance, and the UNATTESTED-marked remainder (what b6 leaves).
5. The ADMISSION-ON commit as a separate follow-up, after b6 is green and the lead signs.
6. **The Q4-(b) slice** (dedicated per-turn inhibit field) as its own small slice after L6.7c (c)
   and before ADMISSION-ON, with the two §4.2 rows turning green; fallback to (a) per §8 Q4.

## 10. Amendment — 2026-11-26, from the first execution attempt (peer)

The first attempt to run b6 was abandoned at the probe stage; these are the facts it produced. They
are cheaper to read than to rediscover.

### 10.1 b6 is BLOCKED on an E6-P.4 defect, not on the plan

`neomacs-make-parallel-thread` **panics** in a Green build with admission enabled
(`KeeperStartFailed` → fallback runs with Main's heap TLS parked → `prepare_make_thread` panics,
rc=101, uncatchable). Filed with pentry; details in that report. **Do not start b6 until the fix
lands** — every row would be measuring the panic path.

### 10.2 The b6 arm MUST run with `NEOVM_THREADS=green` (amends §2.1 and §6)

`admit_parallel` refuses with `Unavailable` unless `green_world(ctx)` —
`ThreadingMode::Green && green_admission_readiness` (`green/api.rs:392`). A `--batch -Q` run is
**Sim** by default, so §2.1's prelude and §6's command would silently test the Baton fallback.
`NEOVM_THREADS=green` is the existing World-mode selector (already used by
`oracle_green_threads.rs`), i.e. the configuration S3 will make the default — so b6 under it is
**evidence for S3**, not a private config. Ordering: b6 → S3 (user OK) → Step 3.

Also: generational GC defaults **on** in production (`TaggedHeap::new()`, `gc.rs:1163-1165`), so gate
1 passes; `NEOVM_GC_GENERATIONAL=0` is the probe control that names gate 1 and must never appear in
a parity arm.

### 10.3 The qualification tree is `main + the E6-P.4 stack + ONE line`

`PRODUCTION_ADMISSION_ENABLED = true` as a **commit** on the qualification branch (not a dirty
tree), so the binary's `--version` stamp is a real SHA. The receipt then reads: qualification tree =
`<sha>`, and the one-line delta quoted against the E6-P.4 tip. 77h's other commits are absent by
design; the E6-P.4 commits are patch-id identical to their copies there, so the admission code is
the same, but b6 does not prove interaction with the rest of that batch — Step 3's gate does.

### 10.4 Artifacts: the target dir is scratch, never storage

Reproducing cells means keeping **pairs**, not binaries. The final image's pdump resolves
**relative to the executable** (verified: a copied pair runs from its new path; the path-hashing at
`load/mod.rs:3700-3720` is *bootstrap* identity only). So: build, then immediately copy
`neomacs` + `neomacs.pdump` to a stable directory, and **prove the copy starts** before recording
its hashes. A pair that only works in place is the same failure one layer down.

### 10.5 Witness plumbing (extends §2.1)

`common::run_parity_under_envs` already gives every Neomacs arm a private tempdir and an env var
pointing at an artifact inside it, and fails the arm when the artifact is missing
("the tier witness would be vacuous"). Reuse that mechanism for `NEOVM_B6_WITNESS`: the prelude
writes `admitted=<t|nil> parked=<t|nil>`, and the b6 arm asserts `admitted=t` per row. Do **not**
fold the witness into the row's returned value — GNU would return a different shape and the parity
comparison would compare the harness instead of the answer.

### 10.6 Two negative-control rows are part of the deliverable

`b6-admission-disabled-witness-is-red` (the same harness on the **unflipped** tree must report
`admitted=nil` and fail the arm) and `b6-admission-enabled-witness-is-green`. A witness never
observed failing proves nothing — that rule has cost this project four rounds tonight.

### 10.7 What b6 does NOT cover (goes in the receipt, one sentence)

b6 qualifies the feature under a one-line flip on a tree that is not the shipping tree; it does not
qualify the shipped default, and the union/contract/GUI rows were not run on the qualification
build — Step 3's own gate covers the shipping tree.
