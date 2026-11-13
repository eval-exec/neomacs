#!/usr/bin/env bash
# Deferred until the post-split pass: no mutation receipt is claimed here.
# Positive: the exact DATA caller is accepted. Red: add a second caller.
# Restore: remove only the extra call. Extra adapter or second seam is refused.
set -euo pipefail
cargo nextest run -p neovm-core --lib --no-fail-fast --retries 0 \
  -E 'test(architecture_pc2_data_install_mutation_red_then_restored)'
