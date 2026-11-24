# Worker isolates: zero-copy transfer of large immutable data (follow-up note)

Investigation record for the isolate series: what sharing immutable string payloads between isolates would need, and why it is a follow-up. Measurements were taken on a 4-CPU machine with the copy path only.


## String storage and mutation

`LispString` is `#[repr(C)]` with pinned `data` and `storage_capacity` words
([`heap_types.rs:96-131`](crates/neovm-value/src/heap_types.rs#L96)). An owned
payload enters through `from_owned_payload`: it appends the trailing NUL,
stores the `Vec` pointer and capacity, then forgets the `Vec`
([`heap_types.rs:487-507`](crates/neovm-value/src/heap_types.rs#L487)). The
payload is released by reconstructing that exact `Vec` from `data`, logical
length plus NUL, and `storage_capacity`
([`heap_types.rs:531-541`](crates/neovm-value/src/heap_types.rs#L531)). The
string object's `Drop` calls this release, and the string-page sweep
`drop_in_place`s dead slots ([`heap_types.rs:1528-1558`](crates/neovm-value/src/heap_types.rs#L1528),
[`gc/allocation.rs:80-86`](crates/neovm-value/src/tagged/gc/allocation.rs#L80)).

Mapped pdump storage is different: `from_mapped_bytes` stores a borrowed
pointer with capacity zero and documents that it must remain mapped and
immutable; mutation first copies it into owned storage
([`heap_types.rs:509-529`](crates/neovm-value/src/heap_types.rs#L509),
[`heap_types.rs:697-712`](crates/neovm-value/src/heap_types.rs#L697)). This
proves that the pointer can alias bytes owned elsewhere while the object lives,
but it does **not** provide a release hook for an owner such as `Arc<[u8]>`.
Capacity zero means “borrowed and never free”; nonzero means “rebuild a `Vec`
and free it.” Neither state can decrement an Arc when the GC drops the string.
The pdump mapping has an external lifetime that outlives the heap; a temporary
shared payload does not.

GNU strings are mutable. The mutation paths call `ensure_owned` for borrowed
storage, and writable access goes through `owned_data_guard`
([`heap_types.rs:560-578`](crates/neovm-value/src/heap_types.rs#L560)). For a
shared payload, that path must detect sharing and copy before handing out
mutable bytes. It must never use `Arc::get_mut` as its only safety check unless
every Lisp and JIT write path is proven to pass that check. The inline JIT
`aset` path also gates writes on `storage_capacity != 0`
([`heap_types.rs:389-396`](crates/neovm-value/src/heap_types.rs#L389)); marking
a shared payload as owned would permit in-place mutation visible from both
heaps. `fillarray` ultimately needs the same private writable storage rule.
Without that complete barrier, refusing mutation with an explicit condition is
safer than sharing but would diverge from GNU behavior.

**Answer:** A mapped/static alias fits the current object without a new field,
but a refcounted external payload does not. To make it work, add ownership
metadata or an external-payload side table keyed to each string object's
lifetime, release that reference during GC sweep, make `ensure_owned` detect
shared storage, and ensure `aset` and `fillarray` both copy before
writing. The side table must be collector-safe and per heap; a process-global
registry would violate the isolate constraints. A new field changes the
layout-pinned `LispString` and its JIT offsets. An external side table and
mutation barriers reach the value crate and its promotion ledgers. I did not
change those files.

## Transport and shared roots

Worker transport currently materializes strings from byte vectors:
`from_value` walks into `Transfer`, `encode_call` serializes it, `run_call`
decodes bytes, and `to_value` clones the byte vector into a new `LispString`
([`transfer.rs:405-435`](crates/neovm-worker/src/transfer.rs#L405),
[`isolate.rs:339-355`](crates/neovm-worker/src/isolate.rs#L339)). A local
`Arc` variant in `Transfer` would still be serialized to bytes by this boundary;
the queued `LispValue` contains `Vec<u8>`. Replacing that with a handle requires
changing the task envelope and ownership transfer so the in-process job keeps
the `Arc` alive while both heaps consume it. Those interfaces are shared with
the base-series pods, so this slice does not alter them.

The value transport types are not a precedent for sharing a Lisp object across
heaps. `ImmediateValue` carries only fixnums, `nil`, and `t`, which need no GC
root ([`transport/immediate.rs:7-21`](crates/neovm-value/src/tagged/transport/immediate.rs#L7)).
`SharedRoot` retains a heap identity and a root lease for a value in one heap;
its safety contract requires the source object to remain in that heap's
allocation domain ([`transport/shared_root.rs:25-42`](crates/neovm-value/src/tagged/transport/shared_root.rs#L25),
[`transport/shared_root.rs:93-112`](crates/neovm-value/src/tagged/transport/shared_root.rs#L93)).
That does not make a mutable heap object usable by another isolate.

The worker crate contains an `Arc<[u8]>` ownership model in
[`shared.rs`](crates/neovm-worker/src/shared.rs). It enforces immutable reads,
explicit private mutation copies, a 4 KiB candidate threshold, and observable
reference counts. Its focused test validates those properties for the payload
model. It is deliberately not connected to `LispString` or the wire path: the
required GC release hook and mutation barrier are absent. This is groundwork,
not an integrated zero-copy prototype.

## Result adoption and GC obligations

The worker finishes by serializing the result as bytes, and the receiving side
rebuilds new values. Adopting a completed result graph into the main heap is
not a bounded worker-only change. `TaggedHeap` owns its object allocation
regions and collector state ([`gc.rs:461-495`](crates/neovm-value/src/tagged/gc.rs#L461));
string pages are reclaimed by their owning heap's sweep
([`gc/allocation.rs:80-104`](crates/neovm-value/src/tagged/gc/allocation.rs#L80)).
The runtime tracks heap identity explicitly; values in side tables are not
materialized for another identity ([`gc/heap_identity.rs:5-18`](crates/neovm-value/src/tagged/gc/heap_identity.rs#L5)).
`SharedRoot` explicitly does not own the heap and relies on that heap's future
collections tracing its root table ([`transport/shared_root.rs:93-109`](crates/neovm-value/src/tagged/transport/shared_root.rs#L93)).

Adoption would need a transfer protocol that reassigns every object's
allocation ownership, validates or rewrites heap identities, reconciles mark
bits and generation state with the destination collection cycle, and moves
allocation pages so only one collector reclaims them. Alternatively, the
collector would need a cross-heap tracing and reclamation model. Merely keeping
the worker graph alive or copying tagged words would let the main collector
misclassify or reclaim foreign objects. No result adoption prototype is
recommended in this slice.

## Measurements and recommendation

the `worker-isolates-json` scenario in `crates/neomacs-perf` (`cargo xtask perf run worker-isolates-json`)
is the copy-path baseline harness: it builds a 200 KB JSON string, runs one
serial task and one isolate task per configured worker owner, with 20 parses per
task, checks the results for equality, and only then prints absolute times and
the speed ratio.
It currently cannot toggle real sharing because no shared-handle transport is
connected. Do not interpret the harness as an after-change comparison. The
verification run measured the copy baseline once on 4 logical CPUs:

| Mode | Serial (s) | Isolates (s) | Serial / isolates |
| --- | ---: | ---: | ---: |
| Existing copy path, 4 owners, 4 tasks, 20 parses each | 0.031173 | 0.101229 | 0.308 |

No integrated after-change measurement exists because the shared payload model
is not connected to `LispString` or task transport.

**Recommendation: follow up rather than ship the prototype.** In the measured
copy-path run, isolates took 0.101229 s versus 0.031173 s serially (0.308
serial/isolate ratio). No after-change number can be reported because the
worker helper establishes only Arc payload ownership, not a Lisp string that
two heaps can safely retain. A follow-up must include a ledger-approved value
storage ownership hook, a complete copy-on-write barrier covering native and
JIT mutations, and an in-process handle transport. Its benchmark must compare
the existing copy path and that integrated path in the same process, with
equal results checked before reporting times. Result graph adoption remains a
separate collector architecture project with the ownership, heap identity,
mark state, and page reclamation obligations above.
