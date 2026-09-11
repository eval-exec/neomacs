# One evaluator execution model for native and browser hosts

Status: incremental implementation; not a claim that every evaluator path is
already independent of the host stack.

## Problem and evidence

Native main recursively evaluates Lisp and uses `stacker::maybe_grow` at guarded
entry points. The browser port cannot use that native stack-switching mechanism.
Its target-specific helper calls the callback without growing the stack. Thus
sharing Rust source did not preserve the guarantee that evaluation reaches
`max-lisp-eval-depth` before exhausting the host stack.

The packaged Chrome regression has reproduced both engine `RangeError` and WASM
out-of-bounds traps at the GNU default depth of 1600. Native main catches
`excessive-lisp-nesting` for the same interpreted forms. This is a portability
gap exposed by the browser port, not evidence of an equivalent native crash.

## Decision

Use one explicit continuation driver in `neovm-core` for native desktop,
Android, and WASM. Platform frontends and host adapters do not select a different
Lisp interpreter. No evaluator-continuation feature flag or WASM-only semantic
implementation is introduced.

Pending Lisp execution belongs in VM-managed storage, not a chain of retained
Rust calls. This is not Lisp tail-call optimization: pending backtraces,
bindings, debugger exits, and cleanup still exist and execute in GNU order.

## Invariants

1. `Context` remains on its VM thread. A continuation is internal execution
   state, not a message sent to a renderer, JavaScript, or another thread.
2. Values retained across driver steps are reachable from traced `bc_buf` slots
   or the specpdl. A Rust `Vec<Continuation>` is not a GC root by itself.
3. Continuation variants carry operand indices and owned cleanup tokens. Tokens
   are consumed by the driver on normal and nonlocal return. Rust `Drop` must
   not execute arbitrary Lisp cleanup.
4. Logical evaluation depth is independent of continuation-vector length.
   Sequence and cleanup bookkeeping must not add artificial Lisp depth. Preserve
   GNU's default 1600 and shared interpreted/bytecode counter.
5. Preserve GNU's distinct overflow signals: interpreted calls signal
   `excessive-lisp-nesting`; bytecode Bcall can signal plain `error` with the
   max-depth message. Do not normalize observable behavior to simplify tests.
6. Resolve native callable identity at the same point as today. Argument
   evaluation may redefine function cells. A saved subr entry must not silently
   change meaning after its arguments have run.
7. Preserve function aliases, autoload retry, error payload identity, lexical and
   dynamic binding order, UNEVALLED/EVALD backtraces, debugger entry/exit,
   watcher callbacks, and thread-blocked flow.
8. Keep native stack-growth protection during migration. Remove dependencies on
   it only when all relevant Lisp-reentrant paths have explicit states.

## Module structure

The public evaluator interface remains in `runtime/eval`. Its private
`continuation` module owns scheduling and frame storage. Existing GNU-mirror
modules own declaration metadata, form validation, binding installation, and
unwind semantics; they prepare typed work instead of recursively executing it.

The current `continuation/mod.rs` contains the driver and closed state enums.
As migration makes individual responsibilities substantial, separate them inside
that directory into call dispatch, form states, and unwind/resume handling. Do
not create empty modules or a parallel `wasm_eval` tree merely to anticipate
future work.

`special_forms.rs` shares preparation between the driver and remaining direct
native entry points. The prepared conditional captures condition, then-form,
and else-sequence before condition evaluation, matching the prior evaluation
order. Sequence execution is reusable by conditionals and binding bodies.

## Migration boundaries

Already implemented: ordinary interpreted calls/arguments, lambda bodies,
`let`/`let*` bodies, conditional condition/branch selection, protected bodies,
and interpreted `funcall`/`apply` applications. Pending values
are rooted in the existing VM arenas, and bindings unwind through owned tokens.

Application handlers are typed evaluator declarations. Their subr objects are
still materialized at their original early registration positions, separately
from the late special-form declarations. Dispatch uses the captured callable
object/entry, not the current spelling of a function cell. A saved `funcall`
subr remains callable after Lisp redefines the symbol.

An owned application scope preserves Ffuncall's extra logical depth, backtrace,
GC/debugger entry order, and return unwinding. Native entry points and the driver
share that scope's entry/exit implementation. Symbol indirection and interpreted
lambda application are driver steps, including calls through first-class
application subrs. GNU's subr-object wrong-arity payload is checked before
argument preparation; the Lisp form's own arity error remains distinct.

The next work is deliberately not a collection of string-name fast paths:

- Protected bodies now keep cleanup on the existing specpdl while the driver
  runs the body. This removes body recursion but does not itself make arbitrary Lisp
  recursion *inside cleanup callbacks* stack-independent.
- Application reentrancy: autoload execution and native callbacks still enter
  existing synchronous paths. Preserve their retry/error semantics while moving
  their Lisp execution onto explicit states; do not mistake ordinary application
  recursion passing for complete callback or autoload stack independence.
- General cleanup: eventually make Lisp cleanup execution resumable in the
  shared unwinder. Retain the pending return value or nonlocal flow as a traced
  object while cleanup runs. Cleanup can replace the pending exit, and remaining
  outer cleanups must still execute.
- Remaining special forms and native callbacks: migrate tests and execution
  states in vertical slices. Passing direct recursion does not establish safety
  for conditionals, initializers, mapping callbacks, autoload, or debugging.
- Bytecode/native transitions: retain the current bytecode machine and add an
  explicit suspension/resumption seam where Bcall invokes interpreted execution.
  Do not rewrite the bytecode engine merely to share its existing operand arena.

## Verification and rollout

Use the public Lisp evaluator and packaged browser editor as test interfaces.
Study GNU `src/eval.c` for each migrated form and `src/bytecode.c` for Bcall.
Reproduce a failing shape before changing its execution path. Test dynamic and
lexical closures explicitly; implicit function-construction context previously
hid failures in browser probes.

The browser test must require a catchable nesting error, restored bindings or
executed cleanup where applicable, and a subsequent successful editor command.
Keep known failing shapes in the reproduction tool; enable required CI cases
only after they pass. Never claim the entire evaluator is stack-independent
because selected cases pass at one depth or on one host.

Run native evaluator/GC/debugger tests and the full core suite. Build Neomacs
with `cargo xtask fresh-build`, then assemble and test the current worker with
the portable runtime assets. **Do not run core suites during fresh-build's Lisp
regeneration**, or rebuild a test executable while another run is using it.
Those are shared mutable test inputs; both overlaps have produced invalid suite
verdicts. Finish with an isolated full run on stable inputs.

Before declaring the migration complete, benchmark native startup, ordinary
calls, recursive calls, byte-compilation, and representative interactive work.
Reuse bounded frame-vector capacity without retaining live Lisp values between
evaluations. Correctness improvements must not conceal uncontrolled allocation
or root-retention growth.
