#!/usr/bin/env bash
# Deferred guard mutation/restoration code. Do not run during split FAST MODE.
# Invoke only in the coordinator's post-split test pass, under its normal runner.
set -euo pipefail
cargo nextest run -p neovm-core --lib --no-fail-fast --retries 0 \
  -E 'test(/split_shared_guards::architecture_.*(self_test|mutation|red_then|explicit_test|missing_member|explicit_target|generic_local_helper)/) | test(/split_test_support_manifest_test::architecture_a2_/)'
