# Issue 382: unintern rejects nil during centaur-tabs redisplay

The reported Doom/Vertico stack reaches `centaur-tabs-delete-tabset`, which
calls `unintern` with nil and a custom obarray. The primitive failure reproduces
without Doom:

```elisp
(unintern nil (make-vector 31 0))
```

GNU Emacs returns nil. The pre-fix Neomacs release signals
`(wrong-type-argument stringp nil)` and exits with status 255 in batch mode.
This isolates the reported exception; it does not reproduce the reporter's
complete Doom configuration or recent-file interaction.

## GNU behavior and root cause

GNU `src/lread.c`, `Funintern` (near line 4858 in the local Emacs checkout),
first validates the obarray, then accepts any `SYMBOLP` argument. That includes
nil and t. A symbol argument removes only the identical object in the target
obarray; a string argument searches by name. A different symbol with the same
name must survive an exact-symbol removal request.

Neomacs stores nil and t as symbol-tagged values with valid `SymId`s, but
`Value::kind()` exposes them as distinct `ValueKind::Nil` and `ValueKind::T`
variants. `builtin_unintern` matched only `ValueKind::Symbol`, incorrectly
routing the two constants to the string type error.

## Fix and abstraction

Normalize all three Lisp symbol variants to `UninternTarget::Symbol(SymId)`.
Keep name-based deletion separately typed as `UninternTarget::Name(&LispString)`.
The enum now stores only identity for symbol targets, deriving the symbol name
when hashing. This eliminates the redundant identity/name pair and its possible
inconsistency. The borrowed name retains the input lifetime without an unused
owned `Cow` variant. Bucket matching uses symbol identity, including constant
symbols, and exhaustive matching over the target enum distinguishes identity
from name semantics. Obarray validation and argument-error ordering are retained.

No general tagged-value redesign is needed: `xsymbol_id` and `as_symbol_id`
already provide the semantic identity of nil and t. This is a boundary conversion
bug in one primitive, not a reason to erase the useful nil/t variants elsewhere.

## Regression coverage

The core regression evaluates the public Lisp primitive for nil and t, checks
absent constants, preserves custom-obarray namesakes, and removes those namesakes
only when their exact symbol is supplied. It failed before the implementation
change with the reported stringp/nil condition.

The oracle regression covers nil, t, ordinary symbols, foreign-obarray symbols,
exact deletion, repeated deletion, and deletion by string name. GNU's results
were checked directly and through oracle verify mode. Before the fix the oracle
reported GNU's successful result versus Neomacs's stringp/nil error.

Commands:

```sh
cargo nextest run -p neovm-core \
  -E 'test(unintern)|test(emacs_core::hashtab::tests)' --no-fail-fast
cargo xtask fresh-build --release
NEOVM_FORCE_ORACLE_PATH=/home/exec/.local/bin/emacs \
NEOVM_BINARY_PATH="$PWD/target/release/neomacs" \
NEOVM_ORACLE_MODE=verify \
  cargo nextest run -p neovm-oracle-tests \
  -E 'test(oracle_prop_unintern_constant_symbols_and_namesakes)'
```

The focused core suite passed all 71 tests after the fix.

`cargo xtask fresh-build --release` completed successfully. The rebuilt executable
returns nil (exit 0) for the original batch reproducer. All 17 tests in
`mapatoms_obarray_comprehensive`, including the new regression, passed with
`NEOVM_ORACLE_MODE=verify` against live GNU Emacs. Formatting and diff checks pass.
