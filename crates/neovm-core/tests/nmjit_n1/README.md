# N1: proposed parallel JIT boundary

Planning/audit base: `ef66aa9ca2f2a17683807fd8c892ac63e5d2d840`.
These tests and sketches do **not** enable parallel JIT or implement GEN1
production admission. Missing-feature fixtures deliberately fail in N1; their
validation runner must check the exact assertion plus clean teardown. No test
is ignored. `sketch.rs` is included only by test clients, never product code.

## N2 landing and continuation

The N2 landing range on `4d489bddf1` implements production GEN1 admission.
Its manifest keeps the real admission case and the no-op and early-return
controls, all passing. The seven later missing-feature cases, their enum
variants and their match arms are absent from that landing range. A single
continuation commit preserves those cases unchanged as plain failing tests;
that commit is not eligible to land. Each case returns in an N3-N5 landing
range only alongside the implementation that makes it pass. The N1 audit and
ABI sketches below describe the original stage and future work.

## Current reaches

Paths relative to `crates/neovm-core/src/`:

| Reach | Source at this base | Required parallel treatment |
| --- | --- | --- |
| Named identity | `emacs_core/runtime/jit/compile/named_frames.rs:24,42,49,58` | Acquire descriptor; sealed function snapshot and target root |
| HOF callback identity | `emacs_core/runtime/jit/compile/inline_hof.rs:80,91,98` | Same lookup; admit callback/source guard separately; no legacy Context shim |
| VarRef/I6 | `emacs_core/runtime/jit/compile/intrinsics.rs:764,782,793` | Same lookup; redirect/value snapshot under authority, result in root home |
| fboundp | `emacs_core/runtime/jit/compile/dispatch.rs:2737` | Currently Context-based dedicated shim, not an inline spine walk; refuse it in parallel code until an admitted variant exists |
| Ownership | `emacs_core/runtime/symbol/mod.rs:990-1018,1190` | Unique owning chunk/side boxes; independent non-owning immutable pointer table |
| Capacity/refusal | `emacs_core/runtime/symbol/parallel/intern.rs:326-384` | Vec reserve may move before refusal; old descriptor must remain independently owned |
| Sidecar | `tagged/gc/mutator_registration/symbol_authority.rs:32` | Era-only atomic publication and typed retirement ledger |
| Construction/permits | `tagged/gc/mutator_registration.rs:298,347,475` | One checked GEN1 production construction; no unchecked permit route |
| Production Context | `emacs_core/runtime/eval/world/parallel_admission.rs:128,171` | Exclusive collector/keeper capability boundary |
| Mode | `tagged/gc/generational.rs:55,629` | Actual heap mode, not env; setter test-only and excluded by borrow |
| Stop | `tagged/gc/mutator_registration/rendezvous/keeper.rs:91-114` | Add JIT quiescence proof before grace; ACK cannot preserve a borrowed pointer |
| Native handoff | `tagged/gc/mutator_registration/native_extent.rs:104-175` | Root-only extent, no descriptor address across ACK |

Re-run the raw-walk inventory at every landed-base rebase. Current direct
OBARRAY_JIT_LEN/SPINE uses in emitters are the first three rows; jit_layout
reexports the offsets and aot.rs fingerprints them. A future fboundp inline
must join the same lookup recipe, not inherit safety from the present shim.

## Proposed ABI and exact initial operation envelope

The following are signatures/roles, not a new implemented entry point:

```text
Gen1Admission::check(exclusive collector, keeper)
    -> Result<Gen1Admission<'heap>, OwnershipReturningAdmissionFailure>
ProductionEra::from_admission(Gen1Admission<'heap>) -> ProductionEra<'heap>
ProductionEra::prepare_parallel_code(AdmittedKernelPlan)
    -> Result<ParallelCompiledCode, AdmissionRefusal>
ProductionEra::enter(&mut owner, &ParallelCompiledCode, &mut RootHomes)
    -> Result<JitActivation<'run>, EntryRefusal>
JitActivation::run(BoundedKernelInput) -> Result<RootHomeResult, KernelRefusal>
AllStopped::jit_grace() -> Result<JitGracePeriod<'stop>, OutstandingActivation>
RetiredTables::reclaim(JitGracePeriod<'stop>) -> ReclaimedTableBytes
```

Compilation happens serially before the era, against an immutable admitted
plan. `prepare_parallel_code` describes validation/binding of that precompiled
artifact, not background compilation from an aliased World. Lifetimes/identity
checks must connect code, canonical backing, era, owner and heap. `sketch.rs`
probes nominal separation and borrowing only; it does not yet enforce the full
runtime identity/stop state machine. N2/N4 replace sketches with product types.

A proposed separate machine entry ABI is an opaque `ParallelRuntime*`,
owner-confined `JitActivationFrame*`, immutable bounded-input block and an
output/status block. The Rust wrapper owns all pointers and validates their
identity/lifetime; generated code receives no Context pointer. Named enum
status discriminants and private-field root/register/budget newtypes are
converted once at this ABI boundary. No general function-pointer cast can
convert SerialCompiledCode into ParallelCompiledCode. Existing serial entry
ABI, emitters, object layout and generated bytes remain unchanged.

Initially permitted operations are:

- Immutable descriptor acquire, typed symbol bounds/address derivation.
- Admitted Plain/projected-Plain global value, boundp, unbound outcome; VarRef/I6
  share the same rooted snapshot rule. No raw managed result escapes.
- Named-function and supported HOF identity checks; rooted old/new definition
  selection with existing function stamps/epoch at the call boundary.
- A bounded call to an admitted immutable plan/kernel, with explicit activation
  roots and return-before-ACK. No arbitrary callback or legacy call shim.
- An admitted fboundp cell snapshot or an explicit unsupported refusal until its
  implementation exists; no implicit serial fallback.
- Scalar control/check operations with a successful-step budget, then a total
  return/refusal epilogue. No safepoint/deopt can preserve a table pointer.

Other owners exercise existing sealed fset/global/intern writers. Allocating
strings, unconstrained Lisp/VM/JIT entry, BLVs, aliases, watchers, generic HOFs,
legacy TLS helpers, compiling during an era and ACK in the middle of a JIT
stack remain excluded. Retirement makes pointer tables live; it does not make
mutable cell reads safe. Reader authority protects each cell/root transfer.

## Type contracts and their limits

The sketch has nominal production/proof and serial/parallel code types, one
checked collector-mode conversion, an exclusive borrow held for the era,
borrow-confined table views, and a non-Copy grace token. Negative clients
exercise private construction, absent non-test proof type, serial entry type,
admission/table lifetime escape, table confinement and reclamation without a
grace token. A positive client must compile first, so missing dependencies or
bad include paths cannot satisfy the negatives. The contract feature is the
existing `contract-fixtures`, not a product proof-bypass feature.

GEN0 is a runtime constructor input: the sketch runtime test proves its typed
refusal. Rust does not infer a runtime heap mode from an integer or environment
string. N2 must check the real exclusive collector and close all product admit
routes; these N1 contracts do not claim that guarantee is already implemented.
Likewise, sketch grace construction is not the real AllStopped/JIT proof.

Every new quantity/state in the implemented tests uses a domain type or enum.
No new product TLS, mode flag, environment switch, unsafe product leaf or object
field exists. Busy is legal; real fixture retries use the landed Deadline
helper. No count-capped contention loop, new ignore or unconditional read of a
debug-only item is permitted. Tests run with debug assertions on, and release
all-target checks remain required.

## Isolated compiler driver

The `nmjit_n1_sketch_compile_contracts` entry (in the `thread_safety_contracts`
test target, with the other contract runners) remains opt-in behind
`contract-fixtures` and invokes the standalone `nmjit-type-contracts` workspace.
The same positive client and all seven negative clients above remain unchanged.
The independent sketches must have no neovm-core/Cranelift dependency: otherwise
trybuild inherits the entire editor graph and recompiles it for its two sets of
diagnostic flags inside the test watchdog. The old generated project had 83
dependencies including neovm-core. The standalone driver has only pinned
trybuild 1.0.121, and its generated client project must contain no engine crate.
The original 600-second watchdog and every assertion remain in effect. This is
a compiler-harness boundary repair, not a skipped, quarantined or deleted case.

The shared sketch now lives at `nmjit-type-contracts/tests/support/sketch.rs`,
and every compiler client lives under that workspace's `tests/ui/`. Keeping
clients inside their manifest root lets trybuild normalize diagnostic paths;
no committed snapshot may contain a lane's absolute worktree path. All client
contents and expected type/lifetime bounds are preserved.
