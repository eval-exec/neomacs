# Worker isolates for Elisp

Worker isolates run Elisp on owner threads without sharing Lisp heaps. Each
owner has its own heap, obarray values, `specpdl`, handlers, buffers and
collector. The owners share only the process symbol-name registry and the
runtime image file; Lisp values cross as copies. A worker can mutate its own
copy without changing the value in the main isolate.

## Lisp interface

The base series registers six `neomacs-worker-*` primitives in the host.
`neomacs-worker-run` copies a function and arguments into a task. The caller
can continue, then collect the result with `neomacs-worker-wait`.

| Primitive | Signature and result | Example |
|---|---|---|
| `neomacs-worker-run` | `(neomacs-worker-run FUNCTION &rest ARGS)` returns a `#s(neomacs-worker-task ID)` handle. | `(neomacs-worker-run #'+ 20 22)` |
| `neomacs-worker-wait` | `(neomacs-worker-wait TASK &optional TIMEOUT)` returns the function result or re-signals its error. `TIMEOUT` is seconds; expiry signals `neomacs-worker-timeout`. | `(neomacs-worker-wait task 2)` |
| `neomacs-worker-cancel` | `(neomacs-worker-cancel TASK)` returns non-nil if the task had not finished. | `(neomacs-worker-cancel task)` |
| `neomacs-worker-status` | `(neomacs-worker-status TASK)` returns `queued`, `running`, `finished`, `cancelled`, or nil for an unknown task. | `(neomacs-worker-status task)` |
| `neomacs-worker-pool-size` | `(neomacs-worker-pool-size)` returns the configured owner count, or zero when no pool is installed. | `(neomacs-worker-pool-size)` |
| `neomacs-worker-stats` | `(neomacs-worker-stats)` returns a plist with thread, startup, boot-time, enqueue, completion and cancellation statistics. | `(plist-get (neomacs-worker-stats) :ready)` |

The pool starts lazily when the first task is submitted. Errors raised by a
worker are copied back and signaled by `wait` with the same condition name;
error data is printed in the worker and read in the main isolate.

## Values at the boundary

Every transferred value is rebuilt in the receiving heap. The boundary uses
`neomacs-worker-transfer-error` when it cannot copy a value.

| Value | Transfer rule |
|---|---|
| nil, t, fixnum, bignum, float | Copy by value; bignums use decimal digits and floats preserve IEEE bits. |
| String | Copy bytes and multibyte state; text properties are dropped. |
| Symbol or keyword | Copy by name and re-intern; an uninterned symbol becomes interned. |
| Cons/list | Copy proper and dotted lists; shared substructure is duplicated. Nesting deeper than 4096 is refused. |
| Vector, record, bool-vector | Copy elements or bits recursively. |
| Hash table | Copy entries in slot order, preserving `eq`/`eql`/`equal` test and weakness. User-defined tests are refused. |
| Interpreted closure | Copy all six slots, including captured lexical environment. |
| Byte-code function | Copy its printed literal and read it on the receiving side. |
| Subr | Copy by the name of its function symbol. |

The following values are refused as `neomacs-worker-transfer-error` with
data `(unsupported-object TYPE)`: buffers, markers, overlays, windows,
frames, processes, terminals, threads, mutexes, condition variables,
module functions, user pointers, finalizers, obarrays, fonts, char tables,
timers, tree-sitter nodes, SQLite values, xwidgets, and Neomacs surface or
video handles. Circular values signal data `(circular-object)`; excessive
nested depth signals `(too-deep)`; user-defined hash tests signal
`(user-defined-hash-table-test)`. A byte-code literal that cannot be read
signals `(byte-code-literal MESSAGE)`. These are the condition data lists,
following the condition name in the error object.

## Worker capabilities and limits

Workers boot an evaluator from the same image as the editor, with
`noninteractive` set. They can evaluate image-defined Lisp and use private
buffers, including `with-temp-buffer`. Their variables, definitions and
buffers are private to the owner and can persist between tasks assigned to
that owner. A worker cannot see main-isolate variables, definitions or
buffers. The worker primitives are registered by the host and are not in the
worker image, so tasks do not nest.

There is no display frontend or command loop in a worker. Display, process
and keyboard primitives may be present, but are unsupported there and are
not gated by the base series. **UNVERIFIED:** the base-series tests do not
exercise those calls.

Queued cancellation prevents execution. Cancelling a running task raises the
owner's quit request; Lisp observes `quit` at its next safepoint and the
owner can then take another task. `neomacs-worker-wait` waits in short
intervals and calls `maybe_quit`, so C-g quits the wait. The base-series
behavior is for C-g to interrupt the wait; it does not cancel the worker
task.

## Opt-in and package use

The intended build opt-in is the Cargo feature `worker-isolates`, off by
default. The intended Lisp capability check is
`(featurep 'neomacs-worker-isolates)`. The explicit worker API is the runtime
opt-in: work runs on an owner only after a call to `neomacs-worker-run`.
`neomacs-worker-pool-size` is intended to be the pool setting, with zero
disabling the pool. These feature and variable details are defined by the
slice interfaces and this POC prompt; they are not in the checked-out base
series. **UNVERIFIED.**

<!-- verify against slice S4 -->
The package-author interface is defined by the base series and the S4
interface: `neomacs-worker-pmap` maps a function over values in parallel,
and `neomacs-worker-call` starts a call for later collection. When worker
execution is unavailable, the library runs the function serially. The
contract is the same values, with only elapsed time differing. The S4 Lisp
library is not present in the base series. **UNVERIFIED.**

```elisp
(neomacs-worker-pmap #'json-parse-string json-strings)
(let ((task (neomacs-worker-call #'expensive-fold items)))
  (do-other-things)
  (neomacs-worker-wait task))
```

## Startup and memory

Each owner loads the image into its own heap. The image file is privately
mapped once per owner; the current loader is designed around one image per
process, so each owner applies relocations and performs the symbol fixup
walk. Each private mapping remains for the process lifetime. Pool startup
cost therefore grows with owner count; startup runs lazily and in parallel.

## Relation to the RFC

This POC implements the isolate-first direction in
[`neovm-rfc-modular-multithread.md`](neovm-rfc-modular-multithread.md): §4
places the worker runtime beside the main isolate; §8.1 gives isolates
separate heaps, stacks, handlers and `specpdl`; §8.2 defines `Copy` and
forbids cross-isolate `Borrow`; §8.3 keeps shared mutable editor state
host-owned and main-thread committed; §8.4.4 describes cancellation and C-g.
The current pool supplies worker owners and a value-copy boundary. It does
not implement the RFC's channels/select, work-stealing scheduler, deadlines,
buffer snapshot/patch protocol, or effect-based affinity enforcement.

Shared-heap execution remains out of scope. The GC design describes
thread-confined heap ownership, and the RFC assigns dynamic execution state
and editor mutations to explicit owners. Sharing a heap would require a
collector protocol across mutators, per-thread dynamic-binding semantics,
and coordinated buffer mutation. Isolates keep those states separate.
