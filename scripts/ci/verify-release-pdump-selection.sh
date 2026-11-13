#!/usr/bin/env bash
# Fail the linux CI release-pdump step when its nextest filter selects a subset
# of the suites it is meant to run.
#
# The filter is defined ONCE as the job-level RELEASE_PDUMP_FILTER env var in
# linux.yml and is read by both this verifier and the run step, so there is no
# second copy that can drift from what actually runs. Every test in those
# modules is behind cfg(feature = "release-pdump-tests"). If the feature is not
# enabled for the test build, a module moves, or a test list is enumerated
# incompletely, the filter silently matches fewer tests and the step stays
# green while running a subset. That failure mode was observed once locally in
# a worktree whose target directory was being rebuilt by a concurrent cargo
# job: the same command selected 2 tests instead of 20 and nothing reported a
# problem.
#
# This guard recounts the selection and requires one selected test for every
# `#[test]` declared in the module source. The expected counts are derived from
# the source, not pinned here: adding a test needs no script edit, and losing
# one (or failing to select it) fails the step loudly instead of passing a
# smaller run.
#
# If a module's tests are macro-generated or cfg-gated such that the source
# `#[test]` count disagrees with what nextest selects on this platform, this
# guard fails loudly. That is the safe direction: update the guard deliberately
# together with the module change; never weaken it with a blanket allow/skip.
set -euo pipefail

readonly filter=${RELEASE_PDUMP_FILTER:?RELEASE_PDUMP_FILTER must be set (defined once in linux.yml)}
readonly modules=(batch_startup jit_observability main_stack_overflow)

if ! list=$(cargo nextest list -p neomacs --features release-pdump-tests -E "$filter"); then
    echo "ERROR: cargo nextest list failed for the release-pdump selection" >&2
    exit 1
fi

# nextest colors the test names; strip the escapes before parsing (and keep the
# stripped selection in the CI log for attribution).
list=$(printf '%s\n' "$list" | sed -e 's/\x1b\[[0-9;]*m//g')
printf '%s\n' "$list" >&2

failed=0
for module in "${modules[@]}"; do
    source_file="crates/neomacs/tests/${module}.rs"
    expected=$(grep -cE '^[[:space:]]*#\[test\]' "$source_file")
    # Test list lines look like `<package>::<binary> <test name>`; the test name
    # of a module test starts with the module path.
    actual=$(printf '%s\n' "$list" | grep -cE "[[:space:]]${module}::" || true)
    printf '%s: source tests=%s selected=%s\n' "$module" "$expected" "$actual" >&2
    if [ "$actual" -ne "$expected" ]; then
        failed=1
    fi
done

if [ "$failed" -ne 0 ]; then
    echo "ERROR: the release-pdump filter selected a strict subset of its suites;" >&2
    echo "refusing to run a green step that drops tests (see counts above)" >&2
    exit 1
fi
