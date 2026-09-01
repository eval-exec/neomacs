# Concurrent GC for neovm-core — as-built architecture

A background GC thread marks concurrently with the mutator, which stops only
for two short safe-point handshakes per cycle (root snapshot at the start,
mark termination at the end). This is the Go-style design: concurrent
tri-color mark, SATB (snapshot-at-the-beginning / Yuasa) deletion barrier,
cooperative safe points, precise (non-conservative) rooting.

STATUS: the concurrent collector is the ONLY sliced/marking collector. The
old incremental slicer and the `NEOVM_GC_CONCURRENT` / `NEOVM_GC_SATB` env
gates were REMOVED in the GC-modernization path-collapse (branch
`neovm-gc-modernization`, 2026-06-19) and this doc describes the code
as-built through the 2026-07 arc (parity mark bits, size-class object arenas,
the task-01 concurrent-claim dispatcher, the adaptive pacer, dump-less
concurrent, per-group handshake stats). A stop-the-world (STW) full trace
survives ONLY as an internal phase — the first-cycle bootstrap (before the
heap is eligible for concurrent marking) and explicit `garbage-collect` — not
as a rival path. Everything else runs concurrently and terminates at a brief
STW handshake. Verify every claim below against `tagged/gc.rs` +
`tagged/header.rs` + `emacs_core/runtime/symbol/mod.rs` (the code is truth; this doc is a
map).

## The Rust-specific hard problem

A GC thread reading the object graph while the mutator writes it is a data
race = UB in Rust, not merely a logic bug. Every GC-visible word the two
threads touch is therefore accessed atomically. To bound the shared surface,
ONLY MARKING goes concurrent; allocation and sweep stay mutator-side
(**no-free-during-mark**: the free list and every deallocation are
mutator-only — the GC thread never frees, and sweep runs world-stopped at the
termination handshake). The GC thread holds NO `&mut TaggedHeap` (two `&mut`
to one heap is UB even with atomic fields); it works entirely through:

- Cons block mark bitmaps (atomic `fetch_or`), read via atomic `load_car/load_cdr`.
- `GcHeader.marked` (`AtomicBool`), claimed with an atomic `swap`.
- Read-only `Arc` snapshots of owned page bases + the dump address span.
- A shared `Mutex`-backed SATB buffer (mutator appends; GC drains into gray).

x86-64 relaxed atomic loads/stores compile to plain `mov`, so the per-slot
cost is ~zero; the cost was the pervasive representation change.

## Object model & the heap

### Parity mark bits (`tagged/header.rs`)

`GcHeader { marked: AtomicBool, kind: HeapObjectKind, tenured: bool, next }`.

- `marked` is the RAW mark-parity bit (relaxed atomic). For a YOUNG
  (non-tenured, heap-owned) non-cons object, "marked this cycle" ≡
  `marked == TaggedHeap::mark_parity`. The heap FLIPS `mark_parity` at
  `begin_collection` instead of walking `all_objects` to clear bits, so the
  raw value alone is meaningless — interpret it via `is_marked_at` /
  `mark_claim_at` against the owning heap's parity. `mark_parity` starts
  `false`.
- `is_marked_at(parity)` = `marked.load(Relaxed) == parity`.
- `mark_claim_at(parity)` = `marked.swap(parity, Relaxed) != parity` — the
  atomic CLAIM: sets the bit and returns `true` iff THIS call flipped it from
  unmarked → marked. Used by the concurrent GC thread to mark a heap object
  exactly once with no `&mut TaggedHeap`; the non-cons analogue of
  `atomic_mark_owned_cons_ptr`. A lost race is benign (the object ends up
  `== parity` either way).
- **born-at-parity**: heap allocation paths overwrite `marked` at the link
  seams with the current parity (allocate-black), so a mid-cycle birth is
  already marked and never enters the GC's gray.
- **tenured-before-parity read order**: `tenured` is a frozen bit (set at a
  world-stopped promotion) meaning "permanently black, never re-traced /
  re-swept". EVERY reader short-circuits on `tenured` FIRST
  (`tenured || is_marked_at(parity)`), because the frozen flag is stable to
  read on any thread while the parity bit is live.
- Mapped (pdump) objects mark via the heap's mutator-only side tables and
  never interpret `marked` at all.

`marked` is atomic precisely so the GC thread can claim it while the mutator
allocate-blacks / reads it without a data race (`AtomicBool` has the same
size/layout as `bool`, so the header does not grow).

### Size-class object arenas (`tagged/gc.rs`)

Non-cons heap objects live in per-TYPE `ObjectArena<T>` allocators, one arena
per kind, each paging a homogeneous size class. `OBJECT_PAGE_BYTES = 64 KiB`;
a page holds `OBJECT_PAGE_BYTES / T::SLOT_BYTES` slots. Size classes
(`T::SLOT_BYTES`, the arena stride):

| Arena | Object | `SLOT_BYTES` | `size_of::<T>()` bound |
|-------|--------|-------------:|-----------------------:|
| `float_arena` | `FloatObj` | 32 | == 24 |
| `string_arena` | `StringObj` | 64 | ≤ 56 |
| `vector_arena` | `VectorObj` | 64 | ≤ 48 |
| `bytecode_arena` | `ByteCodeObj` | 384 | ≤ 376 |
| `lambda_arena` | `LambdaObj` | 128 | ≤ 120 |
| `macro_arena` | `MacroObj` | 128 | ≤ 120 |
| `record_arena` | `RecordObj` | 64 | ≤ 56 |
| `symbol_with_pos_arena` | `SymbolWithPosObj` | 64 | ≤ 56 |

The struct must fit in `SLOT_BYTES` minus an 8-byte trailing free-link word
(`FREE_LINK_OFFSET = SLOT_BYTES - size_of::<usize>()`), const-checked. Conses
use a separate block allocator (`CONS_BLOCK`) with its own mark bitmap.

**Page-ownership oracle** — `ObjectArena::owns` (registry + stride + alloc
bit) is the EXACT page-span test that classifies a discovered address at claim
time. The owned-page set enumerated for sweep and for the claim snapshots
INCLUDES RETIRED (bump-exhausted, full) pages — a retired page's slots are all
live tenured/owned objects, so it is swept and snapshotted like any other.
`ObjectArena::sweep_range` is the only reclaimer.

### When the concurrent collector runs — dump-less concurrent

```rust
should_run_concurrent = if partition_dump { dump_blackened }
                        else { bootstrap_collected }
```

- WITH a dump partition: concurrent runs once the mapped pdump image has been
  promoted + blackened (`promote_and_blacken` at the first partition cycle's
  sweep end sets `dump_blackened`). The first partition cycle itself is the STW
  full trace that does the promotion.
- WITHOUT a dump (a **dump-less heap**): concurrent runs after the bootstrap
  cycle (`bootstrap_collected`) — the first collection is an STW bootstrap,
  and every later cycle marks concurrently. (This is new vs. the historical
  "concurrent only post-blackened-dump" rule; test
  `dumpless_heap_enables_concurrent_after_bootstrap_and_collects`.)
- A heap that registers a dump AFTER dump-less cycles reverts to the dump rule
  (the first partition cycle must be the STW full trace regardless of earlier
  bootstraps).

## The two handshakes (the only STW)

1. **Start / root snapshot** (`concurrent_begin` + `launch_concurrent_mark`):
   stop the mutator briefly; flip `mark_parity` (O(1) "clear"); seed the
   collector-internal + remembered + context roots (push to gray); capture the
   read-only snapshots the GC thread will use (cons-block bases, Tier-B vector
   backings, and the float / vector / bytecode PAGE-BASE sets); arm the SATB
   barrier + allocate-black; send the job and RETURN (do not block).
2. **Termination** (`terminate_concurrent_mark`): stop the mutator; join the
   GC thread and fold residual SATB + deferred values into gray; re-seed the
   runtime / remembered / context roots + mid-cycle new symbol cells
   (`trace_new_symbol_cells`); dirty-owner re-gray (see invariant 4); final
   mutator-side `mark_all`; weak-table / finalizer / dead-marker post-passes;
   then the deferred sweep.

Both are bounded root scans, sub-millisecond. The expensive cons-spine
traversal runs entirely between them, off the pause.

**HandshakeStats** (`handshake_stats` / `handshake_stats_mut`) is a
diagnostics-only (no behavior) sibling of `SweepStats` that records both
handshakes decomposed PER PHASE and PER ROOT GROUP: the mark-clear three-way
split (cons-bitmap memset / young non-cons `all_objects` walk / mapped resets),
`seed_all_context_roots`'s per-group `RootSeedBreakdown` at both start AND
termination, the task-01 float / vector / bytecode page-base snapshot timings,
join/fold, new-symbol tracing, and once-per-handshake O() size probes. The
evaluator populates the context-root breakdown via `handshake_stats_mut`.

## The concurrent mark loop & claim dispatcher

`run_concurrent_mark` loops draining its local gray queue and the shared SATB
buffer until both are empty and the mutator asks it to stop. It marks:

- **Conses** via `atomic_mark_owned_cons_ptr`: the mark bitmap is at
  `block_base + CONS_MARKS_OFFSET`, derivable from the cons pointer alone; the
  bit is set with an atomic `fetch_or` (Relaxed), returning true iff it
  flipped. Children read with atomic `load_car` / `load_cdr`. An immutable
  `Arc<HashSet>` of owned cons-block bases (snapshotted at the STW start) says
  which conses it may mark vs. defer.
- **A small owned non-cons set** via `concurrent_try_mark_owned(val, job,
  gray)`, DEFERRING every other non-cons (and non-owned cons) to the STW
  termination. Each arm first tests a PAGE-BASE snapshot hit BEFORE any
  dereference (a hit is simultaneously the ownership proof AND the type
  classification — an arena page owns its 64 KiB span exclusively, which also
  rules out mapped objects); a MISS returns `false` = defer (fail-safe):

  - **strings** → `concurrent_try_mark_string`: owned page strings with NO
    interval tree (`intervals_ptr()` null) claim the side-table bool (claiming
    the `GcHeader` bit would collide with string interval roots); zero Lisp
    children ⇒ the claim IS the trace. Interval-bearing strings defer.
    Counter: `str_claimed`.
  - **floats** → page-snapshot claim: a float has ZERO Lisp children
    (`mark_value`'s float arm is mark-only), so `mark_claim_at` IS the complete
    trace. Counter: `float_claimed`.
  - **vectors** → page-snapshot HEADER claim only. The children are NOT
    inline-traced here; they are covered by the union of {the Tier-B
    start-snapshot scan of the possibly-retired-on-write start backing} +
    {the SATB deletion barrier on every slot / bulk overwrite} +
    {allocate-black for mid-cycle values} + {the termination root reseed} +
    {the termination dirty-owner re-gray of `satb_snapshotted_owners`
    (invariant 4)}. The last leg is NOT optional (it restores the
    current-backing re-trace that header-claiming removed for mid-cycle
    register→heap insertions). Counter: `vec_claimed`.
  - **bytecode** → page-snapshot HEADER claim + GC-thread GRAY-PUSH of the
    children (arglist / constants / env / doc_form / interactive / extra_slots;
    `params` is SymIds, untraced by design). Sound ONLY because published
    bytecode is COMPILE-TIME immutable (the sole mutation seam is
    `#[cfg(test)] with_bytecode_data_mut_for_test`), so a fresh claim proves
    the object pre-dates the cycle and its fields are stable to read on the GC
    thread. Counter: `bc_claimed`.
  - **subrs** → recognize-and-drop (NOT a claim): `SubrObj`s are `Box::leak`ed
    statics (`allocate_static_subr_object`), never page-allocated, never linked
    into `all_objects`, never swept — permanently live by construction. The
    header bit is DEAD STATE nobody reads. The arm just bumps `subr_dropped`
    and returns `true` ("handled, nothing owed"); it writes no header and a
    subr has no Lisp children. Gated on `type_tag == VecLikeType::Subr`.
  - **mapped (pdump) veclikes** (`dump_lo..dump_hi`) → always DEFER: they mark
    via the mutator-only side table; recognizing one as leaked or claiming its
    header would leave the side-table mark unset and panic the tricolor /
    partition verifiers (the mis-claim UAF shape).

**Claim-ordering rules (H4/H5, MANDATED):**
1. Any inspection that can still send the value to `deferred` runs BEFORE the
   claim — a claimed-THEN-deferred object whose termination trace early-returns
   on the (now-set) mark bit would DROP its children.
2. The TENURED check runs BEFORE the parity claim — tenured ≡ permanently
   black; the flag froze at the world-stopped promotion, so the read is stable
   on this thread and "handled, nothing owed" needs no touch of the frozen bit.

## The SATB (deletion / Yuasa) barrier

Before the mutator overwrites a heap slot it logs the OLD value; the GC drains
those into gray, keeping the start-of-cycle snapshot live regardless of
concurrent mutation, with a wait-free append fast path. New objects
allocate-black.

- Every `tagged/mutate.rs` heap-slot store fires `note_heap_write` /
  `note_heap_slot_write` (owner-driven `record_heap_write`, which logs the
  pre-overwrite children via `collect_veclike_children`) BEFORE the store.
- Every symbol value / function / plist overwrite fires `note_root_overwrite`.
- The barrier's fast-path gate keys on the `TAGGED_HEAP_CONCURRENT_ACTIVE`
  thread-local, so the SATB log fires even when owner write-tracking is
  Disabled (this was a real correctness bug: `note_heap_write_record`
  short-circuited before `record_heap_write` and the log never fired).
- The GC drains a shared `Mutex<Vec<TaggedValue>>` SATB buffer into gray.

## The adaptive pacer

After each sweep the heap recomputes `live_bytes` EXACTLY (cons + object +
arena-page + mapped live bytes) and resets `bytes_since_gc`. `should_collect()`
is simply `bytes_since_gc >= gc_threshold`. The evaluator's
`effective_gc_threshold_bytes` sets the threshold to the MAX of three terms,
clamped to `[1, GC_HI_THRESHOLD_BYTES]`:

1. `gc-cons-threshold` (elisp), floored at `GC_THRESHOLD_FLOOR_BYTES` — the
   GNU floor.
2. the `gc-cons-percentage` term: `(live_bytes + bytes_since_gc/2) *
   percentage`, clamped to `GC_HI` — the GNU proportional model.
3. an internal live-proportional growth term: `live_bytes *
   GC_LIVE_GROWTH_NUM/GC_LIVE_GROWTH_DEN`, clamped to `GC_HI`, so the O(live)
   full-mark cost amortizes as the heap grows.

The elisp-derived value (1–2) is a FLOOR term 3 never lowers (user settings
and defaults keep their meaning as minimum budgets); an explicit
`set_gc_threshold` override bypasses all of this (only the pacer value flows
through `set_gc_threshold_from_runtime`).

## Insertion coverage & the precise-rooting precondition (task 01 / #17)

Task 01 made the GC thread CLAIM (mark, without inline-tracing) the headers of
owned floats / vectors / bytecode. A claimed veclike's header bit is set, so
the STW termination's `mark_value` early-returns on it and never re-runs
`trace_veclike`. Before claiming, that termination re-trace of every *deferred*
veclike's CURRENT backing silently covered mid-cycle INSERTIONS (a value stored
into an owner AFTER it was scanned). Claiming removed that backstop, so the
collector now leans on these invariants (verified sound by a two-lens
adversarial review — a soundness proof + a failed reproduction):

1. **Precise-rooting precondition.** The collector is precise; there is NO
   conservative stack scan (`set_stack_bottom` is a literal no-op). Every heap
   value the mutator will use after a safepoint MUST be reachable from a seeded
   root (`trace_roots` — operand stacks / `bc_buf` / `vm_root_frames` /
   specpdl / scratch `GcRoot`s — or a thread-local root table) at every
   world-stopped mark-start. Allocation never triggers a GC, so a value need
   only be rooted before the next *safepoint*, not the next allocation. A live
   value held ONLY in an un-seeded Rust local across a mark-start is a
   root-discipline violation the STW collector mishandles at the same safe
   point too — NOT a concurrent-GC regression.

2. **SATB snapshot completeness.** Every value reachable from roots at the
   start snapshot is marked by end-of-mark. This rests on: the atomic
   world-stopped seed; a deletion barrier on EVERY `mutate.rs` heap-slot
   overwrite (all fire `note_heap_write` BEFORE the store) and EVERY symbol
   value/function/plist overwrite (`note_root_overwrite`); allocate-black for
   mid-cycle births; and the BLV-pool full re-scan at both handshakes. Symbol
   cells are the ONE root skipped at the concurrent-mark TERMINATION re-seed
   (via `ObarraySymbolCellSkipGuard`, which sets `SEED_SKIP_OBARRAY_SYMBOL_CELLS`
   so `Obarray::trace_roots` omits the per-symbol val/function/plist walk),
   because the GC thread scanned the whole obarray concurrently at the start
   (`ObarrayScanSnapshot::scan`) and `note_root_overwrite` covered every
   mid-window overwrite. The guard MUST NOT wrap the start seed or the STW
   full-collection seeds.

3. **Cons interior invariant.** A value stored into a cons (fresh OR
   pre-existing) is protected by its OWN provenance — snapshot-marked,
   born-black, deletion-logged on unlink, or obarray-covered — NEVER by tracing
   the cons. Conses are DELIBERATELY excluded from the dirty-owner re-gray:
   `push_value_children_to_satb_shared` (the callee `record_heap_write`
   invokes) gates the owner dedup on `!owner.is_cons()`, so conses never enter
   `satb_snapshotted_owners` and instead enumerate their two children directly
   each write; and `mark_cons` returns `false` on an already-set bit, so a
   fresh or already-marked cons is never re-traced. Correctness therefore
   requires the inserted value to be provenance-covered, i.e. precise rooting
   (invariant 1). Regression guard:
   `concurrent_fresh_cons_interior_of_snapshot_value_survives`.

4. **Fix-(2) scope (`join_concurrent_mark` dirty-owner re-gray).** At join, the
   CURRENT children of every multi-child owner mutated this cycle
   (`satb_snapshotted_owners`, drained via `push_value_children_to_gray`) are
   re-grayed. This restores, for concurrently-CLAIMED veclike/string owners,
   the current-backing re-trace that header-claiming removed. It is
   defense-in-depth for the relaxed-snapshot exposure of claiming — NOT a
   correctness requirement for correctly-rooted programs — and it deliberately
   does NOT extend to conses (invariant 3; conses never enter the set). It also
   MASKS precise-rooting bugs for veclike/string owners (an un-seeded inserted
   value survives) where conses would surface them as a rare UAF; extending the
   re-gray to conses is a possible future symmetric hardening.

## Load-bearing invariants / risks (future critics grep these)

- **`collect_veclike_children` MUST stay a SUPERSET of `trace_veclike`** for
  every `VecLikeType` — the remembered/SATB strong-trace path AND
  `verify_dump_partition` both rely on it. A field traced by `trace_veclike`
  but omitted from `collect_veclike_children` is invisible to the verifier
  (it uses `collect`) yet unmarked via the remembered path → a silent UAF the
  gate cannot catch. When adding a veclike field, update BOTH.
- **no-free-during-mark**: allocation and the free list stay mutator-only; the
  GC thread never frees, and sweep runs world-stopped at termination.
- **tenured-before-parity read order**: every `marked` reader short-circuits on
  `tenured` first.
- **born-at-parity**: allocation link seams write `marked` at the current
  parity (allocate-black).
- **page ownership including retired pages**: the `ObjectArena::owns` oracle
  and the claim/sweep page-sets include retired (full) pages.
- Any NEW root source must be seeded at BOTH the start and termination
  handshakes (the lesson from the historical incremental-termination UAF).
- Weak hash tables must be resolved on EVERY termination path
  (`mark_and_sweep_weak_tables`); the remembered/SATB path
  (`push_value_children_to_gray`) must STRONG-trace veclike children
  (`collect_veclike_children`, not the weak-deferring `trace_veclike`) so a
  dumped/tenured weak table conservatively retains its entries. (Reproduces
  only under `gc_stress` + `NEOVM_GC_VERIFY_PARTITION` TOGETHER.)
- UB from a missed atomic / wrong `Ordering` is worse than a UAF — gate every
  change with `gc_stress` + `NEOVM_GC_VERIFY_PARTITION` + the full suite, and
  the concurrent-mark tests under ThreadSanitizer (`run-gc-tsan.sh`).

## Closed gaps & known races (grep-able)

**CLOSED GAP (2026-07):** `module_make_interactive` (`dynamic_module.rs`) used
to write a fresh cons into a live, GC-traced `ModuleFunctionObj.interactive_form`
slot with no deletion barrier. FIXED: it now fires
`note_heap_write(func_val, HeapWriteKind::ModuleFunction)` before the store, so
`record_heap_write` logs the pre-overwrite `interactive_form` via
`collect_veclike_children`. Regression test
`concurrent_module_function_interactive_form_overwrite_keeps_child_alive`.

**FIXED RACE (task #23, 2026-07):** the concurrent obarray scan
(`ObarrayScanSnapshot::scan`) used to read a symbol slot's presence
(`if let Some(sym)` over `[Option<LispSymbol>; 4096]`) non-atomically while the
mutator wrote `LispSymbol.function_unbound` — Rust had niched the `Option` tag
INTO that `bool` byte, so the presence read raced the write. FIXED by removing
the niche: `LispSymbol.name` is now an `AtomicU32`; the chunk store is
`[LispSymbol; 4096]` (`OBARRAY_CHUNK = 4096`) with
`name == SYMBOL_NAME_SENTINEL (NameId(u32::MAX))` marking an empty slot; a fill
writes the arm fields then publishes with a terminal `name.store(id, Release)`;
the scan gates on `name.load(Acquire) != SENTINEL` before the seqlock arm reads.
`name` is write-once, so presence is monotonic (None→Some only) and the slot
stays 32 B (`assert!(size_of::<LispSymbol>() == 32)`), no pause-floor-scan
regression; single-mutator reads stay `Relaxed`. Regression test
`gc_concurrent_obarray_scan_vs_defalias_churn` (a TSan gate — the race was
benign on x86, so a functional pass does not prove the fix). TSan: 9 races → 0.

**HEADER-PUBLICATION ORDERING — FIXED (task #24 option A, 2026-07-10):** the
concurrent-claim dispatcher (`concurrent_try_mark_owned`) reads an arena
object's `GcHeader.tenured` byte (a plain `bool`) after reaching it through a
heap pointer; the mutator's arena `ptr::write` (a non-atomic whole-header
memcpy in `alloc_bytecode` / `alloc_float` / `alloc_vector`) writes that
header just before publishing the pointer. This chain used to be all
`Ordering::Relaxed` — benign on x86-64 (TSO retires the memcpy before the
publication store; a single byte cannot tear; a reused slot's prior occupant
was non-tenured, so a stale read was still `false`), but REAL UB on weak
memory (ARM / Apple Silicon), where a StoreStore reorder could expose a
torn/premature header. Fixed with option A, RELEASE/ACQUIRE ON THE
PUBLICATION CHAIN (`header.rs`): every mutator store that can make a heap
pointer GC-visible mid-mark — `ConsCell::set_car`/`set_cdr`,
`LispValueVec::store_atomic`, `store_value_atomic` (symbol value/function/
plist cells and object fields route through these) — is now `Release`, and
every GC-thread first-acquisition load — `ConsCell::load_car`/`load_cdr`,
`LispValueVec::load_atomic`/`iter_atomic`, `load_value_atomic` (the obarray +
Tier-B vector scans and the gray drain read through these) — is `Acquire`.
The pairing makes the constructor's header write happen-before any dispatcher
dereference, closing the whole class (tenured/kind/type_tag reads AND the
born-black parity-bit visibility for mid-cycle objects in reused snapshot
slots). FREE on x86 (both orderings are plain `mov` under TSO; `stlr`/`ldar`
on AArch64). Objects reached through pre-cycle channels (start-handshake
snapshots, the gray-queue channel handoff, the SATB/deferred mutexes) already
carried happens-before from those seams; the mark bits (`GcHeader.marked`,
cons block bitmaps) intentionally stay `Relaxed` — they never publish field
data, and a claim's field reads are ordered by the pointer chain, not by the
bit. The canonical contract comment lives on `load_value_atomic`
(`header.rs`). This removes the one known-benign residual TSan race
(`gc_concurrent_leaked_subr_drop_under_pdump_verifiers`); `run-gc-tsan.sh`
gates the surface (verified CLEAN, 100/100 tests, 0 reports, 2026-07-10).

**MARK-START PACING — INSTRUMENTATION ONLY (2026-07-10):** the reactive
`must_finish` cap (`bytes_since_gc > gc_threshold*4`, evaluator-side) can in
principle degrade a concurrent mark to a synchronous STW residual drain when
allocation outruns marking. Every terminated mark now measures its window
(`launch_concurrent_mark` stamps wall + `bytes_since_gc`;
`incremental_finish` closes it): alpha-1/2 EWMAs of allocation rate and mark
wall duration, `pace_lead_bytes` = rate x duration = the projected
next-window allocation, escalated (x2, EWMA-bypassing) on cap-forced
terminations; `NEOVM_GC_TRACE=1` emits `pace[bytes/thr/lead]` on every
concurrent start, a per-cycle `mark_window` line, and `must_finish#` events
(plus a lifetime `must_finish_count()` counter). A paced start trigger
(`clamp(cap - lead, threshold/4, threshold)` on the adaptive path) was built
on this and REVERTED after two-regime measurement: debug replay storms
peaked at lead ≈ 2% of threshold (0 must_finish across isolated + contended
runs), and the release-profile probe stayed dormant too (313 concurrent
starts, 0 activations, max lead/threshold 10.2% vs the 300% activation bar
— release marking outruns release allocation 40-50x, structural ceiling
~4-10%). GO-CRITERION for reintroducing the trigger (a two-line swap of
`should_collect` in `gc_safe_point_exact_should_collect` plus the clamp
helper; see ladder task-3/5 reports): any real workload whose traced
`mark_window` lead approaches `3x threshold`, or any nonzero
`must_finish_count` in the field.
