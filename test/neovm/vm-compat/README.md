# NeoVM Compatibility Oracle Scaffold

This directory provides a minimal GNU Emacs oracle harness for compatibility testing.

## Goal

Capture canonical GNU Emacs results for a corpus of Elisp forms, then compare NeoVM
results against that baseline once evaluator execution is wired in.

## Files

- `oracle_eval.el`: batch-mode evaluator used as the GNU Emacs oracle
- `run-oracle.sh`: runs all forms from a corpus file and prints TSV output
- `cases/core.forms`: starter corpus for expression and error behavior

## Usage

```bash
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

## Next step

Add a NeoVM runner that evaluates the same corpus and diffs against this oracle output.
