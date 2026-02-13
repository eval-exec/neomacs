# Neo-Elisp VM Design (Compatibility-First)

A modern Elisp virtual machine for Neomacs, built in Rust, with strict GNU Emacs behavioral compatibility and a staged path to high performance.

Related detailed architecture RFC:

- `docs/neovm-rfc-modular-multithread.md`

## Compatibility Contract

Neo-Elisp VM must preserve observable GNU Emacs behavior:

- Same Elisp language semantics (dynamic binding, lexical binding, special forms, macros)
- Same error, signal, and unwind behavior (`condition-case`, `unwind-protect`, debugger entry)
- Same object identity and mutation behavior (`eq`, `eql`, `equal`, `setcar`, `setcdr`)
- Same symbol/function/value cell behavior, including redefinition during runtime
- Same module-facing behavior for existing Emacs package ecosystems

Performance features are allowed only when semantics remain identical.

Compatibility scope is source-first and explicit:

- Compatibility target is GNU Emacs Elisp source behavior (`.el`), not GNU `.elc` binary format compatibility.
- Loading GNU Emacs `.elc` artifacts is explicitly unsupported in NeoVM.

### Compatibility Matrix

- `.el` (GNU Emacs Lisp source): required compatibility target.
- `.elc` (GNU Emacs byte-compiled artifact): unsupported; loader must signal `file-error`.
- `.neoc` (NeoVM parse cache sidecar): optional internal optimization artifact; never part of compatibility contract.

## Compatibility Verification Harness

NeoVM compatibility should be verified continuously against GNU Emacs oracle results.

Initial scaffold:

- Form corpus: `test/neovm/vm-compat/cases/core.forms`
- Oracle evaluator: `test/neovm/vm-compat/oracle_eval.el`
- Oracle runner: `test/neovm/vm-compat/run-oracle.sh`
- NeoVM runner: `test/neovm/vm-compat/run-neovm.sh`
- Comparator: `test/neovm/vm-compat/compare-results.sh`

Current command:

```bash
test/neovm/vm-compat/run-oracle.sh test/neovm/vm-compat/cases/core.forms
test/neovm/vm-compat/run-neovm.sh test/neovm/vm-compat/cases/core.forms
cd test/neovm/vm-compat && make check-neovm
```

Next phase:

- Expand corpus coverage (binding rules, non-local exits, mutation, numeric edges)
- Wire compatibility target(s) into CI
- Run differential checks against larger GNU Emacs test subsets

## Design Principles

1. Compatibility first, then speed.
2. Keep the interpreter as source of truth; JIT is an optimized cache of semantics.
3. Use feedback-driven specialization (ICs and quickening), not global assumptions.
4. Prefer targeted invalidation (per symbol/cell), not whole-VM cache flushes.
5. Add concurrency through isolates and message passing before shared mutable Lisp heaps.
6. Ship in small phases, each with compatibility gates and measurable wins.
7. Default threading model is isolate-first; shared-heap Common Lisp-style threading is not a default compatibility goal.

## Non-Goals (Early Phases)

- No immediate replacement of all editor internals with lock-free shared state.
- No mandatory optimizing JIT in phase 1.
- No compatibility-breaking language extensions.
- No Common Lisp-style shared-heap multi-threaded Elisp execution in default mode.
- No GNU Emacs `.elc` binary compatibility goal.

## Architecture Overview

```
Source (.el)
  -> Reader/Parser
  -> Macro expansion + byte compiler
  -> Register bytecode + feedback vector
  -> Tier 0 Interpreter (always available)
     -> Tier 1 Baseline JIT (hot code)
        -> Tier 2 Optimizing JIT (very hot and stable code)

Runtime Core
  - LispValue representation
  - Heap + GC (young + old generations)
  - Symbol/value/function cells + version counters
  - Specpdl/handler/backtrace stacks
  - Module ABI layer (Emacs-compatible C ABI + Rust-native wrappers)

Concurrency
  - Main editor isolate (semantic authority)
  - Worker isolates for async tasks (LSP/indexing/parsing)
  - Message passing between isolates
```

## Implementation Snapshot (February 13, 2026)

Implemented now:

- `rust/neovm-host-abi`: typed VM/host boundary (`HostAbi`, `Affinity`, `EffectClass`, snapshots, patches, task/select types)
- `rust/neovm-core`: `Vm<H, S>` shell with explicit `TaskScheduler` trait to keep core decoupled from host/editor internals
- `rust/neovm-core/src/elisp.rs`: first interpreter slice with parser + evaluator for `let`/`let*`/`setq`, quote/list mutation (`setcar`/`setcdr`), `catch`/`throw`, `condition-case`, `unwind-protect`, `if`/`progn`, lambda/funcall/`defun`, arithmetic, and `eq`/`equal`
- `rust/neovm-worker`: first worker runtime scaffold with:
  - bounded priority task queues (`Interactive`, `Default`, `Background`)
  - cancellation-aware task lifecycle (`Queued/Running/Completed/Cancelled`)
  - worker-pool execution loop
  - pluggable task executor closure and result/error propagation through `task_await`
  - `with_elisp_executor` path to execute Elisp source payloads on worker tasks
  - bounded channels and timeout-aware `select`
  - condition-variable wakeups for `task_await` and channel readiness (no default busy-spin waits)
  - finished-task reaping API to prevent task registry growth
  - runtime metrics for queue pressure and completion/cancellation counters
- `rust/neovm-worker/examples/scheduler_bench.rs`: quick throughput benchmark for task scheduling and channel round-trips
- `rust/neovm-worker/examples/elisp_bench.rs`: throughput benchmark for threaded Elisp task execution path
- `rust/neovm-core/examples/compat_runner.rs`: NeoVM compatibility runner producing oracle-style TSV
- `rust/neovm-worker/examples/elisp_compat_runner.rs`: compatibility runner through threaded worker runtime path
- configure-time core backend selection: `--with-neovm-core-backend=emacs-c|rust`
  - `emacs-c` keeps the current legacy core path
  - `rust` enables the future Rust-core build mode surface for migration
- Source-only loading policy in `neovm-core` loader:
  - load-path suffix resolution uses source-first policy (`.el`, not `.elc`)
  - explicit `.elc` rejection with `file-error` and actionable message
  - no implicit `.elc` parse/eval fallback behavior
- Source parse cache (`.neoc` sidecar):
  - cache file: `<module>.neoc` beside `<module>.el`
  - key includes schema version, VM version, and lexical-binding mode
  - source-content hash invalidates stale cache entries
  - cache stores parsed forms and bypasses parse work on valid hits
  - cache is non-authoritative: corruption, mismatch, or read failure must always fall back to source parse
  - cache format is internal to NeoVM and may change by schema/version without compatibility guarantees
  - cache writes should be atomic (temp file + rename) so interrupted writes degrade to cache miss, not semantic failure

Package/runtime policy:

- Packages are expected to ship/load `.el` sources under NeoVM.
- `.elc`-only package delivery is unsupported and should be treated as incompatible packaging for NeoVM.
- When both `.el` and `.elc` are present, NeoVM resolves and loads `.el` source.
- When only `.elc` is present, NeoVM must hard-fail with an explicit unsupported-artifact error.

Not implemented yet:

- Full Elisp reader/compiler/bytecode pipeline
- Native execution of GNU Emacs `.elc` compiled-function objects (explicitly out of scope for current compatibility contract)
- Per-isolate Lisp heaps and snapshot/patch transfer semantics
- Tiered JIT pipeline and deoptimization metadata
- Incremental/concurrent GC engine
- Full GNU Emacs behavior compatibility layer (current interpreter slice covers only a starter subset)

This confirms the module split and isolate-first scheduler direction in code, while performance-critical VM internals remain planned work.

## 0. VM/Editor Decoupling Contract

Neo-Elisp VM and the editor host must be explicitly decoupled. This is a hard architecture rule, not a style preference.

Required module split:

- `neovm-core`: evaluator, bytecode, JIT, GC, `specpdl`, value model
- `neovm-host-abi`: typed boundary (primitive calls, handles, snapshots, patches)
- `neomacs-host`: buffers/windows/redisplay/process/input/filesystem/timers

Required dependency direction:

- `neovm-core` may depend only on `neovm-host-abi`, never on editor internals
- `neomacs-host` may depend on `neovm-host-abi`, but not mutate VM internals directly

Forbidden coupling:

- VM reading or writing editor structs directly
- Raw host pointers exposed as Elisp-visible objects
- Cross-layer calls that bypass ABI metadata checks (affinity/effects)

Why this is mandatory:

- Enables independent evolution of VM and editor subsystems
- Makes isolate-based concurrency tractable
- Preserves compatibility while still allowing deep runtime optimization

## 1. Runtime Representation

### 1.1 LispValue

Use a 64-bit tagged value with a platform-safe layout:

- Inline immediates: fixnum, char, constants (`nil`, `t`)
- Inline float when feasible (NaN-boxing on validated targets)
- Heap pointers for cons, vectors, strings, symbols, etc.

Implementation rule:

- Keep an internal `ValueRepr` abstraction so NaN-boxing is a backend choice, not a hard ABI promise.

### 1.2 Heap Object Header

Every heap object begins with a compact header:

- Type tag
- GC mark/age bits
- Size/class metadata

This supports fast type checks, tracing, and stable object walking for GC/debugging.

### 1.3 Symbols

Do not force all symbols into one non-GC global table. Preserve full Elisp semantics:

- Interned symbols: indexed by obarray/intern tables
- Uninterned symbols: regular heap objects with identity semantics
- Value/function/plist cells carry independent version counters for cache invalidation

## 2. Execution Model

### 2.1 Tier 0: Interpreter

Primary correctness engine:

- Register-based bytecode
- Fast dispatch loop
- Feedback collection per instruction site
- Full safepoints for GC, signal handling, and debugger integration

### 2.2 Bytecode

Use register bytecode with explicit operands and side-table metadata:

- Fewer dispatches than stack bytecode
- Better JIT lowering
- Easier value liveness tracking for deopt/GC maps

Add quickening:

- Generic ops can rewrite to specialized variants after profiling (`add -> add_fixnum_checked`).
- Quickened opcodes must dequickening-fallback safely when assumptions fail.

### 2.3 Feedback Vector

Each function has a feedback vector, with per-site entries for:

- Call target shape
- Operand type pairs
- Global/buffer-local variable access patterns
- Branch probability

This feeds quickening and JIT tiering.

## 3. Inline Caching and Invalidation

### 3.1 IC Types

- Monomorphic IC: one stable target/type
- Polymorphic IC: a small set of targets/types (for example <= 4)
- Megamorphic fallback: generic slow path

### 3.2 Invalidation Strategy

Avoid one global epoch. Use targeted version checks:

- Function cell version for call ICs
- Value cell version for global lookup ICs
- Context-local version (buffer/frame) for local binding ICs
- Type/shape guard versions for optimized arithmetic/access

This keeps unrelated caches hot after local redefinitions.

## 4. JIT Design

### 4.1 JIT Tiers

Tier 1: Baseline JIT

- Compile bytecode almost 1:1 to native code
- Preserve runtime checks
- Inline IC fast paths
- Deopt to interpreter on guard fail

Tier 2: Optimizing JIT

- Type specialization from stable feedback
- Small function inlining
- Unboxing in local regions
- Redundant check elimination
- Still fully deoptimizable

### 4.2 Deoptimization Requirements

At each deopt safepoint, store enough metadata to reconstruct:

- Program counter
- Virtual registers/temporaries
- Specpdl depth
- Handler stack state
- Backtrace/debug frames

Correct deopt is mandatory before enabling aggressive optimizations.

### 4.3 Compilation Triggers

- Warm threshold: compile baseline in background
- Hot threshold: consider optimizing JIT only if profile is stable
- Backoff: repeated deopts lower optimization aggressiveness

## 5. GC Architecture

### 5.1 Generational Heap

Young generation:

- Bump allocation fast path
- Copying minor collections
- Very short pauses target

Old generation:

- Incremental Immix-style marking
- Concurrent sweeping where safe
- Optional compaction on fragmentation pressure

### 5.2 Barriers and Roots

- Card marking for old->young references
- Precise roots from interpreter and JIT stack maps
- Specpdl/handler stacks always scanned as roots

### 5.3 Latency Targets

Initial practical targets:

- Minor pause: generally sub-millisecond
- Major incremental slice: low milliseconds
- Avoid long stop-the-world pauses in interactive paths

### 5.4 Near-Nonblocking GC Clarification

Neo-Elisp VM targets near-nonblocking behavior in practice, not mathematically zero-stop GC.

- Keep stop-the-world phases minimal (for example root snapshot / phase flip only).
- Do most work incrementally or concurrently within strict time slices.
- Keep pauses isolate-local so worker GC does not stall main interactive execution.
- Enforce pause budgets with p95/p99 telemetry gates in CI.

## 6. Concurrency Model

### 6.1 Isolate-First

- Main isolate executes editor-critical Elisp semantics.
- Worker isolates run parallel compute tasks with explicit message passing.
- Cross-isolate sharing uses serialization or controlled transfer objects.

### 6.1.1 Explicit Decision

Neo-Elisp VM adopts isolate-first threading as the default architecture.

- We intentionally give up Common Lisp-style shared-heap multi-threaded Elisp in compatibility mode.
- If a shared-heap model is explored later, it must be behind explicit experimental feature flags.

### 6.2 Why This First

- Preserves Emacs semantics while still using multiple cores
- Avoids early complexity of a fully shared mutable Lisp heap
- Provides immediate wins for background operations

Shared mutable Lisp-state parallelism is a later phase and only after correctness and tooling maturity.

## 7. Module and FFI Strategy

Support two layers:

1. Emacs-compatible C module ABI for ecosystem compatibility
2. Rust-native module API for Neomacs internals and new modules

Rules:

- Common ABI boundary keeps GC/lifetime safety explicit
- Native wrappers must honor GC tracing and safepoint constraints
- No module can bypass unwind/error semantics

## 8. Observability and Tooling

Ship VM telemetry from day one:

- IC hit/miss rates per opcode/site
- JIT compile time and code size
- Deopt counts by reason
- GC pause distributions (p50/p95/p99)
- Allocation rate by object type

Provide developer toggles:

- Disable JIT per function
- Dump bytecode/quickening/JIT IR for debugging
- Force interpreter-only mode for bisection

## 9. Performance Expectations

Realistic, compatibility-first expectations:

Phase A (interpreter + register bytecode + IC + generational GC):

- Lisp-heavy microbenchmarks: about 2x to 4x
- Interactive editor latency: about 20% to 50% improvement
- Major GC pauses: substantially reduced versus non-generational baseline
- Startup/load performance should be reported separately for cold parse-cache (`.neoc` miss) and warm parse-cache (`.neoc` hit) runs.
- Startup/load reporting should also include post-edit cache rebuild cost (source changed -> cache invalidated -> rebuilt).

Phase B (+ baseline JIT):

- Hot function speedups: about 3x to 8x
- Whole-editor workloads: about 1.3x to 2x additional gain depending on workload

Phase C (+ optimizing JIT):

- Tight compute loops: about 5x to 20x
- Whole-editor gains remain workload-dependent and usually lower than microbenchmarks

### 9.1 About "10x"

- 10x or higher is a realistic target for selected hot compute-heavy Elisp paths.
- 10x is not a realistic default expectation for whole-editor wall-clock latency.
- End-to-end editor responsiveness is bounded by redisplay, I/O, subprocesses, and host-side work.
- Practical whole-editor target range remains around 1.5x to 3x for mixed workloads.

### 9.2 How Neo-Elisp VM Reaches 10x-Class Hot-Path Speedups

1. Fast interpreter baseline (register bytecode, efficient dispatch).
2. Feedback-driven quickening of common generic operations.
3. High-hit inline caches for calls, globals, and arithmetic.
4. Targeted invalidation (versioned cells), avoiding global cache flushes.
5. Baseline JIT for warm code, then optimizing JIT for very hot/stable code.
6. Low-overhead generational GC to reduce allocation and pause costs.
7. Parallel worker isolates for off-main-thread compute workloads.

## 10. Validation Plan

### 10.1 Correctness Gates

- Differential testing against GNU Emacs for language semantics
- Bytecode behavior parity tests for NeoVM-compiled/internal bytecode and interpreted paths (not GNU `.elc` format parity)
- Fuzzing reader/evaluator/error unwinding
- Deterministic tests for deopt state reconstruction
- Negative artifact tests:
  - loading `.elc` signals expected `file-error`
  - `get-load-suffixes` excludes `.elc`/`.elc.gz`
  - corrupted or mismatched `.neoc` safely falls back to source parse
  - lexical-binding mode changes produce distinct cache keys and no stale-cache semantic drift

### 10.2 Performance Gates

- Startup benchmarks (init files, package load)
- Interactive command latency suite
- GC stress suite (allocation-heavy editing and LSP scenarios)
- Long-session memory fragmentation and pause stability tests
- Startup/load benchmarks must publish cold-cache, warm-cache, and post-edit rebuild numbers.

### 10.3 CI Guardrails

- Fail CI on statistically significant regressions beyond configured thresholds.
- Track and report p50/p95/p99 command latency and GC pauses by build.
- Require no compatibility regressions when any higher tier (quickened/JIT) is enabled.

No phase is accepted without both correctness and performance gates.

## 11. Delivery Plan

### Phase 1: Compatibility Core

- Reader/evaluator/specpdl/condition system parity
- Basic heap and non-moving conservative old-gen fallback
- Differential harness online

### Phase 2: Register Bytecode + Feedback

- New bytecode pipeline
- Interpreter quickening
- Monomorphic and polymorphic ICs

Expected outcome: first substantial speedup while preserving behavior.

### Phase 2.5: Source Precompile Pipeline (NeoVM Internal)

- Optional precompile step from `.el` to NeoVM-internal cache artifacts.
- Background precompile hooks for package installation/update workflows.
- Explicitly no GNU `.elc` ingestion; pipeline remains source-based.

Expected outcome: improved startup/package-load latency without expanding compatibility surface.

### Phase 3: Generational GC

- Young copying collector
- Incremental old-gen collector
- Precise barriers and root maps

Expected outcome: lower pause times and better throughput.

### Phase 4: Baseline JIT

- Cranelift lowering from bytecode
- Deopt framework and metadata
- Background compilation workers

Expected outcome: strong hot-path wins with low semantic risk.

### Phase 5: Optimizing JIT (Selective)

- Guarded specialization and inlining
- Deopt/backoff heuristics
- Profile-guided thresholds

Expected outcome: large wins on stable compute hotspots.

### Phase 6: Expanded Parallelism

- Worker isolate ecosystem
- Optional controlled shared-state experiments
- Strict opt-in and feature gating

## 12. Key Decision

The highest-confidence route is:

- Build a compatibility-solid interpreter and runtime first
- Add ICs and generational GC for broad wins
- Introduce JIT as a guarded optimization layer
- Expand parallelism via isolates before shared mutable runtime state

This path gives modern architecture and strong performance without risking Elisp compatibility.
