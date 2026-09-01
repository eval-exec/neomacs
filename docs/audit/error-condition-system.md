# Error / Condition System Audit

Date: 2026-03-29

Scope:
- `signal`, `condition-case`, `handler-bind`, debugger entry policy, throw/catch dispatch, and signal payload fidelity.
- GNU Emacs reference tree: `/home/exec/Projects/github.com/emacs-mirror/emacs/`

## GNU design and implementation

GNU Emacs keeps the Lisp-facing syntax for this subsystem partly in Lisp, but
the runtime semantics live in C:

- `lisp/subr.el`
  - `handler-bind`
  - `condition-case-unless-debug`
- `src/eval.c`
  - `Fsignal`
  - `signal_or_quit`
  - `find_handler_clause`
  - `maybe_call_debugger`
  - `internal_lisp_condition_case`
  - `Fhandler_bind_1`
- `src/lisp.h`
  - `struct handler`
  - `enum handlertype`

The important architectural point is that GNU uses one active handler runtime
for:

- `catch` / `throw`
- `condition-case`
- `handler-bind`
- debugger suppression vs `(debug ...)`
- interpreter / bytecode resumption choice

GNU does not fold everything into one generic stack. Handler search is unified,
while bindings, backtraces, and unwind cleanup remain separate via `specpdl`.

### Signal search in GNU

`signal_or_quit` in `src/eval.c` does this, in order:

1. constructs the error object
2. runs `signal-hook-function` if applicable
3. canonicalizes invalid error symbols
4. walks the unified handler chain
5. decides debugger entry after handler search
6. unwinds to the selected target

`handler-bind` participates directly in that search via `HANDLER_BIND` and
temporary `SKIP_CONDITIONS` masking. That is what gives GNU all of these
properties at once:

- handlers run in the signal's dynamic extent
- lower condition handlers are muted while a handler runs
- handlers do not recursively apply inside themselves
- non-error exits like `throw` remain visible

### Peculiar `signal` cases in GNU

GNU's public `signal` entry point also has a few intentionally odd edge cases:

- `(signal nil 1)` behaves like `(signal 'error 1)`
- `(signal nil nil)` behaves like `(signal 'error nil)`
- `(signal nil '(error 1))` treats the cons as a full error object
- that cons/object path does not run `signal-hook-function`

Those details come from `Fsignal` and `signal_or_quit` sharing the same internal
representation, including GNU's special `ERROR_SYMBOL == nil` carrier used for
memory-full style errors.

## Neomacs current design

After the unification refactor, neomacs now matches GNU's structure much more
closely:

- [eval module](../../crates/neovm-core/src/emacs_core/runtime/eval/mod.rs)
  owns the shared condition runtime and signal dispatch
- [bytecode VM](../../crates/neovm-core/src/emacs_core/runtime/bytecode/vm.rs)
  consumes already-selected resume targets instead of deciding winners again
- [debug module](../../crates/neovm-core/src/emacs_core/runtime/debug/mod.rs)
  no longer owns signal/debugger semantics
- VM unwind cleanup is separate from condition selection, matching GNU's
  "unified search, separate unwind" split

The shared `Context.condition_stack` now carries:

- catches
- `condition-case` resumptions
- `handler-bind` frames
- `SKIP_CONDITIONS`-style masking

Interpreter and VM both feed that same runtime.

## Fixed by this audit pass

### `signal-hook-function` timing now matches GNU

This audit found one remaining real runtime mismatch after the larger handler
unification: neomacs had seeded `signal-hook-function` as a Lisp variable, but
shared dispatch did not actually consult it.

GNU runs the hook before invalid-error-symbol canonicalization, and passes the
split `(SYMBOL DATA)` form while preserving raw payload shape. Neomacs now does
the same in shared dispatch:

- hook runs before canonicalization
- hook sees raw payloads such as `(signal 'error 1)` as `(error . 1)`
- invalid symbols such as `(signal 'bogus 1)` reach the hook as `(bogus 1)`

This behavior is now covered in:

- [evaluator tests](../../crates/neovm-core/src/emacs_core/runtime/eval/tests/mod.rs)
- [bytecode VM tests](../../crates/neovm-core/src/emacs_core/runtime/bytecode/tests/vm.rs)

### Public `signal nil ...` behavior now matches GNU

This audit also found a separate entry-point mismatch in neomacs's `builtin_signal`.
Because `Value::Nil` is represented as a symbol-like value, neomacs had been
treating `(signal nil ...)` as `signal` on the literal symbol `nil`, which is
not what GNU does.

Neomacs now matches GNU's public behavior for the audited cases:

- `(signal nil 1)` binds as `(error . 1)`
- `(signal nil nil)` binds as `(error)`
- `(signal nil '(error 1))` binds as `(error 1)`
- the cons/object path suppresses `signal-hook-function`
- `(signal nil '(bogus 1))` reports `(error "Invalid error symbol")`

The runtime models this by distinguishing normal signals from the hook-suppressed
error-object path at construction time, while still keeping shared dispatch as
the semantic owner.

## Current audit conclusion

Within the audited runtime path, neomacs now matches GNU Emacs's design much
more closely than the original split implementation:

- one shared condition search path
- `handler-bind` participates during signal search
- debugger policy lives in signal dispatch
- interpreter and VM share the same winner selection
- raw signal payloads are preserved end-to-end
- `signal-hook-function` runs at the GNU point in the pipeline
- public `signal nil ...` peculiar-error behavior matches GNU in the tested cases

## Remaining confirmed mismatches

No confirmed user-visible runtime mismatch remains in the audited scope after
this pass.

Residual risk remains in two narrower areas:

- GNU's actual internal memory-full carrier (`ERROR_SYMBOL == nil` inside
  `signal_or_quit`) is only emulated at neomacs's public `signal` boundary,
  not represented as a separate internal signal form.
- broader compatibility still depends on un-audited callers and larger oracle
  coverage outside this focused error/condition slice.

Neither of those produced a confirmed semantic mismatch in the GNU checks or
focused regressions used here.

## Verification notes

GNU reference checks used during this audit included:

```sh
/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs --batch -Q \
  --eval "(prin1 (let (seen) (let ((signal-hook-function (lambda (sym data) (setq seen (cons sym data))))) (condition-case nil (signal 'error 1) (error seen)))))"

/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs --batch -Q \
  --eval "(prin1 (catch 'tag (let ((signal-hook-function (lambda (sym data) (throw 'tag (list sym data))))) (signal 'bogus 1))))"

/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs --batch -Q \
  --eval "(prin1 (let (seen) (let ((signal-hook-function (lambda (&rest xs) (setq seen xs)))) (condition-case err (signal nil '(error 1)) (error (list err seen))))))"
```

Observed GNU results:

```elisp
(error . 1)
(bogus 1)
((error 1) nil)
```

Neomacs verification for this patch used:

- `cargo fmt --all`
- `cargo check`
- focused `cargo nextest` with redirected output, covering interpreter and VM
  regressions for:
  - `signal-hook-function` timing
  - raw payload fidelity
  - `signal nil ...` peculiar-error behavior
  - existing `handler-bind` dispatch invariants
