# n5-grace-loom — the N5 grace model over the real N3 ledger

Isolated Cargo workspace (no engine dependency, not a member of the main
workspace, no CI runner wired — adding one is a separate, named floor
commit). It exists to check the N5 grace-period design two ways:

* **`ledger`** — a Loom model that includes the **real**
  `src/tagged/gc/symbol_table/publication.rs` byte-for-byte and composes
  it with activation release points (writer / frame / world stop). The
  three named negatives each flip exactly one edge of the stop proof and
  must trip exactly their witness; the positive explores every execution
  clean. The composed positive also caught a real constraint: entry
  validation must be **atomic with entry** (a validate-before-gate draft
  let a frame enter after grace end on a pre-grace binding).
* **`measurement`** — the same real ledger, natively against `std`,
  driven by the symbol authority's exact writer rule, counting
  retirements and `NeedGrace` refusals per stop interval for the P12
  rows (the N5 design's section 2.3 addendum).

## Run it

From the repo root (any cwd works; on this box, inside `nix develop`):

```sh
./crates/neovm-core/tests/n5-grace-loom/run.sh
# or directly:
cargo test --manifest-path crates/neovm-core/tests/n5-grace-loom/Cargo.toml
```

Extra arguments pass through (`run.sh --test ledger`, `run.sh -- --nocapture`
to see the receipts and witness lines).

**Bounds and cost.** Loom preemption bound **2** (`N5_PREEMPTIONS` to
override; must be ≥ 2 — two preemptions suffice for every witness:
frame → writer → stop, frame resumed to read), `max_branches = 4096`,
unbounded permutations. Nine tests total: the positive explores **3262
executions**, the negatives find their witness in **40 / 19 / 21**
executions, test time **~0.3 s**. Measured wall time via `run.sh`
(16 pinned cores, sccache warm for the pinned deps): **~8 s from a
clean `target/`** (compile of loom + generator + static_assertions +
both test crates, then the suite) and **~0.3 s warm**.

## Negative-trigger table

| Edge flipped (one per run) | Witness (exact panic) | Executions |
| --- | --- | --- |
| `entry_validation = false` | `RESUME_USES_PRE_GRACE_POINTER` | 40 |
| `quiescence_wait = false` | `ACK_WITH_LIVE_JIT_VIEW` | 19 |
| `receipt_gates_reclaim = false` | `OLD_TABLE_RECLAIMED_BEFORE_GRACE` | 21 |
| none (positive control) | — | 3262, all clean |

Retained N3 witnesses are asserted outcomes (shape refusal, the
`Unchanged` publication path, both `NeedGrace` refusals with their
charge semantics); see `tests/ledger.rs` for the detector-arming
argument (why each weakened run trips only its own witness).

## The seam, in five lines

1. `build.rs` copies `../../src/tagged/gc/symbol_table/publication.rs`
   to `OUT_DIR/publication.inc`, stripping **only** the leading `//!`
   block (E0753 in `include!` expansions) and the trailing
   `#[cfg(test)] mod tests;` (E0583 — test crates set `cfg(test)`).
2. `tests/ledger.rs` pastes it with `include!` next to a **same-module**
   `mod std` shim, so the ledger's own `std::` imports resolve to Loom's
   modeled cells (a shim in the *including* crate was compile-tested and
   does not work — E0425). `tests/measurement.rs` pastes it with no
   shim: real `std`.
3. The descriptor vocabulary (`SymbolTableShape` and friends) comes from
   `tests/support/vocab.rs`, the named stand-in mirroring
   `symbol_table.rs`'s descriptor half; the ledger never dereferences
   chunk entries, so modeled reads observe only publication ordering,
   retention and refusal.
4. `build.rs` **asserts** the seam contract (the two `std` sync imports
   present; no `super::sync` yet). Any edit to `publication.rs`'s header,
   imports or trailing test module fails the model's build with a
   re-anchor message — never edit `OUT_DIR` by hand.
5. When the in-file `super::sync` seam lands (P9.1b), that detector
   fires: drop the shim and bind to the file's own imports.

## Layout

```
Cargo.toml            own workspace; loom =0.7.2, static_assertions =1.1.0
build.rs              the seam (see above)
run.sh                one-line runner
tests/ledger.rs       the Loom model (config edges, actors, negatives)
tests/measurement.rs  the count-based measurement (P12 rows)
tests/support/        mod.rs = check/negative harness; vocab.rs = stand-in
```
