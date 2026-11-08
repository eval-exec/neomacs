# Proof 2 authority-word models (B1)

This standalone test workspace models the sealed hash-table authority of
`tmp/peer-p2ht/design.md` (proof 2, stage B1): the one-word authority
(`WRITER` bit + counted readers, §2.1), the `switch_epoch` revalidation
protocol across a modeled user callback (§2.1/§3.1), the Rev R2 user-test
nesting count that replaces the `mutable` bool (§3.1), and growth under
WRITER with the reader drain (§3.3/§4 Rev R3). It pins Loom 0.7.2 and
static_assertions 1.1.0. It has no production dependency, feature, engine
code or unsafe code. Its own `[workspace]`, manifest and lockfile isolate it
from the production workspace. Adding these tests changes no Sim runtime
inputs, hot paths, generated code or layouts.

## Model map

| Files | Modeled contract |
| --- | --- |
| `tests/model/mod.rs` | The modeled table: authority word (bit 63 WRITER, low bits reader count), epoch, guard depth, one two-word entry with two consistent states, backing generations with a freed-generation ledger, the enter/exit protocols, thread-confined lease stand-ins (`!Send`/`!Sync`) and the RAII guard hold |
| `tests/authority.rs` | Reader overlap (parallel readers do not serialise), the writer drain, reader retry under WRITER, keeper quiescence after all actors stop, and the torn-entry negative control for a writer that skips the drain |
| `tests/revalidate.rs` | Epoch revalidation retry across a modeled callback, convergence under repeated mutation, and the stale-answer negative control for skipping the recheck |
| `tests/guard.rs` | Rev R2 nesting count: two owners in user tests plus a third whose mutation must signal, RAII unwind to depth zero, own-mutation-inside-own-test, and the lost-restore negative control for GNU's unconditional bool restore |
| `tests/growth.rs` | Growth frees the old backing only after the drain, and the freed-under-a-reader negative control |
| `tests/support/mod.rs` | Exploration receipts and exact expected-panic witnesses |

## What the model deliberately is not

The two-word entry stands for the whole slot array; consistent states stand
for the index/slots invariants; the generation ledger stands for the
allocator. Actors are scripted in one of two ways, both carrying real
happens-before edges: bounded spin waits (`wait_for`) on flags the other
actor is guaranteed to store, and blocking latches (`Latch`, loom
Mutex+Condvar) for scripts that chain more than one wait — a waiter leaves
the runnable set instead of yielding, which keeps deep scripts inside
`max_branches`. Where a case needs a THIRD sequenced actor, the model's main
thread plays it: with all actors spawned, Loom's unique-thread count rises
and the interleaving space exceeds the 512-branch budget for minutes on end.
The lease stand-ins model the thread-confinement shape of
`TableRef`/`TableMut`, not the Rust type-system seal itself (that is B2's
compile-fail fixture family). The keeper actor models the `AllStopped`
license; the model does not prove the rendezvous (proof 1's loom crate owns
that).

## Negative controls

Each negative control runs a deliberately broken variant and requires Loom
to find the specific witness (exact panic text, caught by
`support::negative`):

- `writer_skipping_drain_observes_torn_entry` — `torn entry observed`
- `reader_skipping_revalidation_accepts_stale_entry` — `stale answer accepted after callback`
- `bool_guard_lost_restore_misses_signal` — `mutation inside active user test was not signaled`
- `growth_without_drain_frees_live_backing` — `reader walked a freed backing`

## Running

From this directory, inside `nix develop`:

```
cargo nextest run --tests --no-capture
```

`LOOM_ITERATIONS` receipts print per test on stderr; `LOOM_WITNESS` lines
print for each negative control that found its witness. Runs with no
`LOOM_MAX_PREEMPTIONS` set (full exhaustive exploration, no preemption
bound); the receipts in RESULTS.md are from such runs.
