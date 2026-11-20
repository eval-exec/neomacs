# Stopped serial World-loan contract models (E2a)

This standalone crate has its own workspace and pinned dependency lock. It does
not depend on, compile, or change a product crate. There is no product keeper,
admission constructor, heap reference transfer, or new Lisp path here.

## Run it

From the repo root (any cwd works; on this box, inside `nix develop`):

```sh
./tests/pentry-keeper-loom/run.sh
```

That command is what the batch gate runs. With no arguments it runs `keeper`
at `PENTRY_PREEMPTIONS=2`, then `keeper` at `PENTRY_PREEMPTIONS=3`, then
`lifetime_contracts`. Each invocation is one `cargo nextest run` on the pinned
CPUs, with `RUST_LOG` unset, `--test-threads 1`, and `--success-output final`.
Extra arguments replace that plan and pass through to one nextest invocation
(`./tests/pentry-keeper-loom/run.sh --test lifetime_contracts`).

`PENTRY_PREEMPTIONS` is a model-only exploration bound, defaults to 2, and accepts
2 or 3. The Builder has no permutation or duration cap. Each positive case
prints `PENTRY_MODEL case=... bound=... executions=...`. The eight cases cover:

- stop/publication from two participants;
- a complete no-GC serial ticket roundtrip;
- two serial collection jobs while a ticket is out, with a frozen peer;
- a queue receiving two concurrent serial requests;
- failure after ticket receipt and failure during restore;
- main publishing before blocking on a distinct keeper;
- nested serial reborrow without another ticket.

The model uses one Loom mutex with keeper and per-owner condvars for the abstract canonical protocol
state. Thus it explores queue, publication, transfer, return and failure races
and the happens-before edges supplied by that control gate. It does not prove
the production admission atomics, root table implementation, GC tracing, raw
pointer provenance, actual World retention, native wakes, fairness, unbounded
execution or a future implementation. Those remain E2b/E3 gates. The existing
value-layer weak-memory proofs must be preserved when implementing this model.

The six original negative controls run the *same* protocol after deleting exactly
one edge, and catch the model's panic only if it contains the named property.
A generic panic, deadlock, branch limit or unexpected success fails the test.
The paired positive cases retain every property assertion. These are deliberate
negative controls, not missing-feature tests or hidden runtime failures.

| Witness | Single omitted edge |
| --- | --- |
| `TICKET_WHILE_RUNNING` | wait for all frozen participants' publication |
| `KEEPER_TOUCHES_HEAP_WITH_TICKET_OUT` | end keeper collector loan before sending |
| `STALE_PUBLICATION_USED_FOR_SERIAL_GC` | use current attached roots in serial GC |
| `DOUBLE_TICKET` | wait for return before serving the next queued request |
| `RESUME_AFTER_FAILED_RESTORE` | fail-closed poison fence before resume |
| `VIEW_OUTLIVES_GUARD` | end views before returning the ticket |

R2/A5 adds `ACK_BEFORE_PUBLICATION`: root publication and acknowledgment are
separate locked steps in every positive case. Its negative reverses only their
order. It must complete execution 1 successfully and find the named assertion
only in a later explored interleaving (`executions_to_witness > 1` at both
bounds). The keeper validates between the two steps in that counterexample.
Neither a run counter nor a forced yield selects or manufactures the schedule.
The other six controls remain unchanged in purpose.

`lifetime_contracts` additionally runs a real trybuild lifetime-escape negative,
two scoped/nested positives, and a one-line mutation removing the HRTB on the
same skeleton. The identical escaping fixture must then compile. Its actual
borrowed View and static !Send/!Sync/!Clone/!Copy assertions exercise Rust,
which Loom's opaque abstract views cannot substitute for. The skeleton is
source-only test code and does not claim runtime ticket provenance.
