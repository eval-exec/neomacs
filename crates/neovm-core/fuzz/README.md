# NeoVM core fuzzing

This independent Cargo workspace keeps nightly-only libFuzzer dependencies out
of NeoVM's production workspace. Both targets call the same typed differential
checker as the deterministic `neovm-core` smoke and regression tests.

Run either target from the repository root:

```sh
cargo +nightly fuzz run regex_pike_vm --fuzz-dir crates/neovm-core/fuzz
cargo +nightly fuzz run regex_search_optimizations --fuzz-dir crates/neovm-core/fuzz
```

Bound a local or CI run with libFuzzer options after `--`:

```sh
cargo +nightly fuzz run regex_pike_vm --fuzz-dir crates/neovm-core/fuzz -- \
  -max_total_time=300 -max_len=512 -timeout=10
```

`regex_pike_vm` compares the pure backtracker with the eligible Pike VM for
anchored matching and forward/backward search. `regex_search_optimizations`
compares exhaustive candidate scanning with production fastmap and prefilter
skips.

The weekly and manually dispatched `regex-fuzz` CI job runs both targets for
five minutes, restores the evolving corpora from the preceding run, and uploads
any crash artifacts. The fuzz workspace has its own committed `Cargo.lock`, so
those campaigns are reproducible without adding nightly-only dependencies to
the production workspace.

Generated corpora, crash artifacts, coverage output, and fuzz build products are
intentionally ignored. When a target finds a divergence:

1. Minimize the artifact with `cargo +nightly fuzz tmin`.
2. Add the minimized semantic case as a normal `neovm-core` regression test.
3. Fix the implementation and run the normal suite.
4. Re-run the relevant fuzz target against the artifact before deleting it.

This keeps every known bug permanently guarded without making ordinary test
completion depend on an arbitrary fuzzing time budget.
