# NeoVM Compatibility Oracle Scaffold

This directory provides a minimal GNU Emacs oracle harness for compatibility testing.

## Goal

Capture canonical GNU Emacs results for a corpus of Elisp forms, then compare NeoVM
results against that baseline once evaluator execution is wired in.

## Files

- `oracle_eval.el`: batch-mode evaluator used as the GNU Emacs oracle
- `run-oracle.sh`: runs all forms from a corpus file and prints TSV output
- `run-neovm.sh`: runs NeoVM worker-runtime compatibility runner and prints TSV output
- `compare-results.sh`: diffs oracle TSV vs NeoVM TSV
- `bench-load-cache.sh`: runs cold/warm `.neoc` load benchmark via `load_cache_bench`
- `cases/core.forms`: starter corpus for expression and error behavior
- `cases/input-batch-readers.forms`: batch-mode input reader compatibility corpus

## Usage

```bash
test/neovm/vm-compat/run-oracle.sh test/neovm/vm-compat/cases/core.forms
test/neovm/vm-compat/run-neovm.sh test/neovm/vm-compat/cases/core.forms
```

Use official Emacs explicitly by setting `NEOVM_ORACLE_EMACS` (or `ORACLE_EMACS`), for example:

```bash
NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs \
  test/neovm/vm-compat/run-oracle.sh test/neovm/vm-compat/cases/core.forms
```

Output columns:

1. source line number
2. input form
3. oracle result (`OK <value>` or `ERR <signal+data>`)

## Baseline Workflow

```bash
cd test/neovm/vm-compat
make record   # writes cases/core.expected.tsv
make check    # diffs fresh oracle output against expected
```

When NeoVM produces TSV output for the same corpus:

```bash
cd test/neovm/vm-compat
make compare NEOVM_OUT=cases/core.neovm.tsv
make check-neovm
```

`compare-results.sh` checks index/form/result equality and reports drift.
Set `STRICT_FORM=1` to fail on form-printing differences too.

Run all checked-in corpora in one shot:

```bash
cd test/neovm/vm-compat
make check-all
make check-all-neovm
```

Run the focused callable-introspection suite (faster loop for `fboundp`/`symbol-function`/`indirect-function`/`functionp`/`macrop`/`func-arity`):

```bash
cd test/neovm/vm-compat
make check-introspection-neovm
```

Run the focused thread primitive suite (faster loop for `make-thread`/`thread-join`/`thread-last-error`/`thread-signal`/mutex/condition-variable semantics):

```bash
cd test/neovm/vm-compat
make check-thread-neovm
```

Run the ERT allowlist oracle scaffold (for upstream differential bootstrapping):

```bash
cd test/neovm/vm-compat
make check-ert-allowlist
```

If `emacs` is not on `PATH`, set `NEOVM_ORACLE_EMACS` explicitly:

```bash
cd test/neovm/vm-compat
NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs make check-ert-allowlist
```

The default fixture uses:

- allowlist file: `cases/ert-allowlist-smoke.txt`
- loaded test file: `cases/ert-allowlist-fixtures/smoke-tests.el`
- baseline output: `cases/ert-allowlist-smoke.expected.tsv`

You can override all three via `ERT_ALLOWLIST`, `ERT_LOAD_FILES`, and `ERT_EXPECTED`.

`run-neovm.sh` sets `NEOVM_DISABLE_LOAD_CACHE_WRITE=1` so compatibility runs do
not mutate fixture directories with `.neoc` sidecars.
It also executes the built `elisp_compat_runner` binary directly and rebuilds it
only when relevant Rust sources are newer than the binary.

NeoVM-only policy cases (expected to diverge from GNU Emacs oracle baselines)
can be run separately:

```bash
cd test/neovm/vm-compat
make check-all-neovm-only
```

Current NeoVM-only policy cases include source-only loading behavior (`.elc`
rejection and `.neoc` fallback safety) plus NeoVM extension behavior
(`neovm-precompile-file` cache warming).

You can also precompile source files into NeoVM cache sidecars ahead of load:

```bash
cargo run --manifest-path rust/neovm-core/Cargo.toml --example precompile_neoc -- \
  path/to/file.el [path/to/another.el ...]
```

Run cache-load benchmark reporting cold miss vs warm hit timing:

```bash
cd test/neovm/vm-compat
make bench-load-cache
# or override:
make bench-load-cache BENCH_SOURCE=cases/load-policy-fixtures/vm-policy-cache-probe.el BENCH_ITERS=200
```

## Batch Freeze Notes (2026-02-13)

- Queue slice completed and frozen: commits `7a688f4a` through `8de23c4f`.
- Introspection behavior now oracle-guarded for predicate boundaries, function-cell lookup, alias traversal, and arity/error signaling edges.
- Added fast introspection gate target: `make check-introspection-neovm`.
- Required periodic full gate for this batch was run: `make check-all-neovm`.

Post-freeze updates:

- Added `cases/input-batch-readers` corpus and wired it into default `check-all-neovm` coverage.
- Added CI gate job for `make check-ert-allowlist` in `.github/workflows/vm-compat.yml`.
- Added reader stream compatibility cases: `cases/read-from-string-edges` and `cases/read-stream-semantics`.
- Added obarray argument compatibility cases: `cases/intern-obarray-semantics` and `cases/obarray-arg-semantics`.
- Added string primitive compatibility cases:
  - `cases/split-string-semantics`
  - `cases/make-string-semantics`
  - `cases/make-string-raw-byte-semantics`
  - `cases/make-string-nonunicode-semantics`
  - `cases/string-print-unicode-semantics`
  - `cases/string-nonunicode-char-semantics`
  - `cases/string-nonunicode-indexing-semantics`
  - `cases/string-nonunicode-concat-semantics`
  - `cases/string-nonunicode-sequence-semantics`
  - `cases/string-concat-error-paths`
  - `cases/string-trim-semantics`
  - `cases/string-prefix-suffix-semantics`
  - `cases/string-join-semantics`
  - `cases/string-to-number-semantics`
  - `cases/substring-edge-semantics`
- Added append/vconcat sequence compatibility cases:
  - `cases/append-vconcat-error-paths`
  - `cases/append-tail-object-semantics`
  - `cases/vconcat-mixed-sequence-semantics`
- Added destructive sequence mutation compatibility case:
  - `cases/nreverse-destructive-semantics`
- Added destructive list concatenation compatibility case:
  - `cases/nconc-destructive-semantics`
- Added destructive element-removal compatibility case:
  - `cases/delete-delq-semantics`
- Added destructive ordering compatibility case:
  - `cases/sort-semantics`
- Added list tail/suffix compatibility case:
  - `cases/last-butlast-semantics`
  - includes arg normalization edges (`N=nil`, `number-or-marker-p`, float payload signaling)
- Added list indexing/tail-walk compatibility case:
  - `cases/nth-nthcdr-semantics`
- Added sequence element accessor compatibility case:
  - `cases/elt-semantics`
- Added array index typing compatibility case:
  - `cases/aref-aset-index-semantics`
- Added sequence copy compatibility case:
  - `cases/copy-sequence-semantics`
- Added list membership/alist lookup compatibility case:
  - `cases/member-assoc-semantics`
- Added higher-order map primitive compatibility case:
  - `cases/map-family-semantics`
- Added `ignore` callable compatibility case:
  - `cases/ignore-semantics`
- Full NeoVM gate is green with these additions:
  - `make check-all-neovm`
