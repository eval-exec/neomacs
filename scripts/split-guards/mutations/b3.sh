#!/usr/bin/env bash
# Deferred PC11 B3 mutations. Execute only in the post-split test pass.
# Each mutation test asserts its red diagnostic and restores its green fixture.
set -euo pipefail
cargo nextest run -p neovm-core --lib -E 'test(/architecture_pc11_mutation_/)' "$@"
