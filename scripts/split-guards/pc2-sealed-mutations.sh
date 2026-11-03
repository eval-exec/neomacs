#!/usr/bin/env bash
# Deferred until the one post-split pass; no execution receipt is claimed.
# Includes unexpected SQLite caller, unlisted sealed-value static, generic
# local-cell positive control and static-forwarded helper mutation/restoration.
set -euo pipefail
cargo nextest run -p neovm-core --lib --no-fail-fast --retries 0 \
  -E 'test(/architecture_pc2_(guard_self_test|sqlite_exact_caller_mutation_red_then_restored|generic_local_helper_is_not_a_static_write)/)'
