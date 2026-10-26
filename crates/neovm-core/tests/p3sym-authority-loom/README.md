# Proof 3 symbol authority models — C1

## C6 preparation extension

`tests/combined.rs` contains 16 specification cases (nine positive, seven
negative), extending the original 15-case preparation from stacked C5 tip
`7d1f1a21eb`. Six stopped epochs transfer one immutable
function/name graph between owner homes, real-role global/projection cells,
a function cell, a selected old activation and the exact-name registry index.
Later full census permits old-definition reclamation. A separate unscripted
ACK case explores root-image publication and release-before-ACK; stale epochs
and a held lease cannot authorize tracing. Existing authority/support code is
reused, with no engine dependency or helper fork. A refused acquisition is
handled explicitly; the scripted phase actions have no competing lease.

The full crate now defines **78 cases** and **26 intended witnesses**. Run all
cases at preemption bounds 2 and 3 using the existing entered/completed receipt
accounting. These are expected counts before C6 validation, not a qualification
result. Retained C1–C5 receipt revisions remain unchanged.

New exact witness headings:

- `C6 second stop omitted live global reseeding`
- `C6 stop omitted canonical projection backing`
- `C6 stop omitted the live function-cell root`
- `C6 replacement lost the active old definition`
- `C6 stop omitted the exact registry name root`
- `C6 ACK preceded publication of its owner root image`
- `C6 keeper traced while a symbol lease was held`

Generation bits and sweep masks are finite specifications. They do **not**
implement a collector, all-owner young-log capture, remsets or real minor GC.
Omission controls inspect copied root censuses, then complete every handoff
and join before emitting the named witness; they do not deliberately sweep a
native heap while omitting required roots. Native combined executor, actual
GEN0/GEN1 collections and eventual reclamation, GNU twins, keeper sealing,
Miri/TSan, assertion-enabled families, scaling diagnostics, census and board
remain separate C6 obligations. The running JIT/spine-lifetime gap remains a
later N-mutator JIT obligation because production JIT does not run in this era.


### C6 model type correction

The retained preparation originally had 77 cases and 25 witnesses. The added
function-cell omission control raises those static expectations to 78 and 26;
existing qualification receipts remain pinned to their original sources and
are not evidence for this addition. The new control removes only the audit
census's function-cell root, completes all real censuses and joins the actor,
then reports `C6 stop omitted the live function-cell root`.

C6's model now distinguishes `Backing`, `Attempt::{Committed,Busy}`,
`CollectionKind::{Major,Minor}`, `Owner`, `StopSequence`, transfer/replacement
phases, ACK order and word-gate policy. Root object sets and atomic root words
cannot be substituted for stop sequences or owner indices. Array indexing and
atomic encoding convert these private domains only at their storage boundaries.
Phase and policy matches are exhaustive. No new model synchronization or engine
TLS is added; the shared authority/helper implementation remains unchanged.

These types prevent domain confusion. Matching ACK epochs, the globally zero
authority word, safe scripted handoffs and root reachability remain modeled
runtime invariants; native lifetime evidence is a separate obligation. Scripted uncontended attempts
must commit; concurrent refusal branches remain legal without an acquisition
unwrap or fairness assumption. The generation model still does not implement
a real minor collector. Native combined sessions and keeper sealing need their
own later implementation and receipts.

## C4 preparation extension

`tests/functions.rs` adds ten cases (seven positive, three negative) on the
stacked C3 tip `baa49909d5`. The complete crate now has **47 cases** and
**13 expected negative witnesses**. These are scalar protocol/lifetime models,
not callable Lisp, production VM/JIT overlap, real GC or GNU oracle evidence.

Coverage: Empty/ExplicitlyUnbound transitions and same-definition no-ops;
refusal of the reserved maximum epoch before mutation; two symbols sharing one
change clock; unrelated-stamp resync without target refill; affected-target
refill; activation rooting before reader release; an active old definition
surviving unbind and later reclamation; deduplicated quarantine until stopped
legacy-cache eviction. No snapshot is retagged with a later clock. Positive
contention cases retain Busy as a legal result and complete through an explicit
serial suffix after joins; no refused try-acquire is unwrapped. No payload mutex
or fairness assumption is added. Run all 47 cases at bounds 2 and 3 with the
same existing receipt accounting.

The added exact negative witnesses are:

* `restricted call accepted a retagged old definition`
* `explicit unbound transition omitted its stamp`
* `legacy cache target lost before stopped eviction`

C4 engine qualification separately requires real rooted cells, bounded calls,
owner-private dispatch-header retirement, inline-dependent eviction, old-target
GC reclamation, GNU 31.1 twins, Miri/TSan, zero dormant census and board. Distinct
actual Worlds retain independent bindings; these models cover one shared World.

## C3 preparation extension

`tests/globals.rs` adds ten cases on b142 (`fd1bd8edbb`), while production
storage implementation waits for b143. Run it with the same pinned toolchain,
target, bounds and receipt rules below. The full crate now has 37 cases and
ten required negative witnesses; historical C1 receipts remain unchanged.

The new ordinary checked payload and writer-only history ledger model nil
versus void, two writer orders, a reader plus two writers (three owners),
two readers plus two writers (four owners), set versus makunbound, independent
set answers, rooted read answers across overwrite/stop, and exact accounting
for completed writer commits across bounded Busy retries. An acquisition refusal is retried only after joins, as an explicit
serial suffix. No fairness assumption or extra payload mutex is introduced.
Readers copy under the counted-reader lease; every observed value/version
pair must belong to the committed writer history. Existing C1 tests provide
the explicit overlapping-reader count==2 and writer-drain witnesses.

Two new controls require these exact headings:

* `set returned another owner's value`
* `symbol answer escaped before rooting`

The root control is a finite root ledger, not real GC. GNU twins, actual
Plain/projected-Plain engine operations, real stopped collection, Miri, TSan,
reader scaling/drain latency, census and board are separate C3 obligations.

## Historical C1 qualification

Standalone, test-only workspace on `fa5aa55e2b`. Loom is pinned to **0.7.2**,
static_assertions to **1.1.0**, with the dependency lock copied from proof 2's
reviewed model workspace (only the root package name changes). There is no
engine dependency, workspace membership/feature change, generated Lisp,
layout change, new TLS or Sim code. Run receipts are in [RESULTS.md](RESULTS.md).

The coordinator approved the Stage A design in `tmp/peer-p3sym/design.md`:
plain globals and ordinary initial intern/soft are the core; direct function
cells and restricted calls are the cache/lifetime stage. Get/put, custom
obarrays, unintern, BLVs, aliases, watchers, keyword creation and defalias
callbacks remain follow-up proofs. This crate models protocols, not Lisp
builtins. It does not authorize the C2 engine stage.

The base correction is explicit: Miri atomic-cell/FwdRef work is the lead's
queued b135 (`v10x/peer-miriub`, tip `0916d674fa`), not on this main. Proof 2
is still in B9, also absent from this base. C1 models may run here; C2+ waits
for the coordinator's integrated-main signal containing b135, preferably
proof 2. The process fence forbidding other Worlds' canonical deletion or
registry restore is confirmed for this closed single-World proof.

## Model map

| File | Contract |
| --- | --- |
| `tests/model/mod.rs` | One era-only counted-reader/WRITER word. Successful reader CAS and writer CAS are AcqRel; reader exit RMW and writer release are Release, matching proof 2's reviewed protocol. Reader count saturates without wrapping. Leases are noncloneable and !Send/!Sync. `Checked<T>` uses Loom tracked UnsafeCell accesses for ordinary payloads. |
| `tests/authority.rs` | Two plain symbol fields copied coherently across read/write attempts; two simultaneous readers; writer drain; reader refusal/retry; saturation; missing retry and missing drain controls. |
| `tests/publication.rs` | Distinct World word and registry Mutex, always word→registry. Two same-name interns converge to one ID, one World member and one winning name object; soft misses have no effects; soft never observes partial arms. Different names keep distinct IDs/count. Separate name-release publication and spine-growth/lifetime models. |
| `tests/epoch.rs` | Checked function cells, per-symbol stamps and existing global function clock. Call copies a complete target/epoch into an activation while holding read authority; stale cache revalidates. Different-symbol writers cannot lose an increment; identical fset is a no-op. Root/mark ledger proves an active old callee survives replacement and becomes reclaimable after return. |
| `tests/keeper.rs` | Stop request plus every owner's Release ACK plus zero authority word, then checked live-cell trace. Zero word alone is insufficient. A contending writer can leave/retry and ACK without another owner's authority. RAII unfinished-commit guard publishes sticky poison before field-drop releases WRITER; keeper and resume refuse poison. |
| `tests/support/mod.rs` | Per-case entered/completed execution receipts using host counters outside the model. Negative controls catch only an exact invariant heading, not arbitrary panics. Source locations may follow Loom's exact checked-cell heading. |

`Authority::read/write` models **one acquisition attempt**. Busy/CAS failure
returns without a loan; callers can retry or service a stop outside authority.
Scripts use finite attempts, not a spin loop with a magic cap or an unproved
fairness assumption. A same-name intern returning Busy retries after actor
join; if soft holds a reader, both initial intern attempts may be Busy. This
is an acquisition/commit model, not a proof of eventual scheduling progress.

The two-reader row explicitly observes word==2 and a refused writer before
drain. Latches script only stated ordering/held-lease/stop witnesses. The
unscripted checked-cell rows rely on authority, not latches or a payload mutex.
No parked owner can be used to justify arbitrary new admission: proof 1 owns
the rendezvous state machine, identities, root publication and resume proof.

## Negative controls

All controls must emit their exact `LOOM_WITNESS` heading and be counted as
passing test cases only after a failing model execution is observed:

| Broken variant | Required witness |
| --- | --- |
| Reader increments the word unconditionally through WRITER instead of refusing/retrying | `reader admitted across WRITER without retry` |
| Writer bypasses drain for an ordinary checked symbol-cell write | ``Causality violation: Concurrent read and write accesses to `UnsafeCell`.`` |
| Presence name publishes before arms | `name published before symbol arms` |
| Growth retires backing while a reader retains its generation | `spine retired while reader retained generation` |
| Fset changes a target but omits the function epoch bump | `call accepted stale target at unchanged epoch` |
| Active caller fails to publish its old-target root | `active old function reclaimed after fset` |
| Keeper grants trace entry from ACK alone while word is nonzero | `keeper traced under nonzero symbol authority` |
| Failed transaction releases WRITER without sticky poison | `unpoisoned partial commit reached keeper` |

The growth negative uses a freed-generation ledger, never a real freed-pointer
dereference. The presence submodel has atomic arm storage so the deliberately
broken order can expose incomplete data without introducing Rust UB. This
submodel represents the independent arms-before-name publication guarantee,
not a claim that correctly leased intern-soft can see inside a locked
transaction. Its good order is exhaustively checked. Checked ordinary storage
in the intern and authority models separately tests exclusion and visibility.

## Running and bounds

From the worktree root (TMPDIR is in the lane, never `/tmp`):

```sh
mkdir -p tmp/peer-p3sym/c1/tmpdir
export TMPDIR="$PWD/tmp/peer-p3sym/c1/tmpdir"
export CARGO_TARGET_DIR=/home/exec/nvme-rust-target/v10x-peer-p3sym/loom
taskset -c 0-3,14-31 nix develop -c bash -c '
  unset RUST_LOG
  export RUST_BACKTRACE=0 P3SYM_PREEMPTIONS=2
  cargo test --locked --manifest-path crates/neovm-core/tests/p3sym-authority-loom/Cargo.toml --tests -- --test-threads=1 --nocapture
' > tmp/peer-p3sym/c1/loom-bound2.log 2>&1
```

Repeat with `P3SYM_PREEMPTIONS=3`. The 13 `support::exhaustive` cases ignore
that bound; all remaining cases explicitly use the selected bound (default
2, qualification rejects <2). The helper overrides ambient
LOOM_MAX_PREEMPTIONS, records the effective bound, sets max_branches=4096,
and sets **no duration or permutation cap**. A max-branches panic or timeout
is a failure, never a truncated success. Each good case must complete every
entered execution. A negative stops at its first required failing schedule,
so entered=completed+1, not exhaustive completion of its broken variant.

Use the same pinned-CPU/Nix/inside-shell `unset RUST_LOG` wrapper for fmt and
workspace check. The Nix shell hook sets a logging default, so unsetting only
in the parent shell is insufficient. The model target subdirectory avoids
the workspace check's Cargo target lock. Independent-workspace formatting
requires its own `cargo fmt --manifest-path ... -- --check` as well as the
root workspace fmt check.

## Limits and integration obligations

The small models have two symbol/name slots, scalar name/root identities,
two-field cells, one growth generation and a finite root/mark ledger. They
do not allocate Lisp, prove GNU behavior, model all projection arms, perform
real GC, validate Rust pointer provenance, test compiler-generated code or
establish fairness/throughput. The only unsafe operations are isolated
model-only checked-cell pointer reads/writes plus its documented Sync bridge;
Loom tracks accesses before these pointer operations. Engine Miri, GNU 31.1
oracles, trybuild contracts, TSan x86/arm64 and performance/census gates
remain C2+ work. No production type named in a compile-contract fixture changes.

The future engine must reuse proof 2's reviewed word helper once it lands.
If C2 is explicitly started earlier, copy only behind a TODO and deduplicate
at integration; C1 factors nothing into shared engine code. Every global read
enters/exits via RMWs on one shared line. C3+ must report reader scaling and
writer-drain latency; C7 must name the follow-up lever: BRAVO-style per-owner
reader indicators, sharded authority, or per-symbol authority plus the common
catalog/epoch domain. No promise of reader scaling follows from mutual
exclusion alone. Sim must still have zero dormant hot diffs and the board
must remain within ±1%.

## C3 scaffold correction

The initial lost-increment script incorrectly unwrapped a single reader CAS
attempt: two readers can contend and one can refuse. Its replacement,
`completed_writer_history_never_loses_a_commit`, uses at most three writer
attempts with `loom::thread::yield_now()` after each Busy refusal. Exhausted
attempts are legal no-write outcomes. Every successful writer must appear
exactly once in the ledger and increment the committed version exactly once;
failures print the complete bounded history and refusal counts. A protocol
lost increment fails qualification immediately. This adds no atomic Lisp
read/compute/set opcode; GNU's separate lost-update oracle remains unchanged.
The crate still has 37 cases and ten required negative witnesses.
