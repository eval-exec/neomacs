#!/usr/bin/env bash
# Each fresh nextest child initializes one immutable process policy. This
# publishes no Lisp state/cache and imposes no single-mutator assumption.
set -euo pipefail
if [[ $# -ne 1 || -z "${NEXTEST_ENV:-}" ]]; then
  printf '%s\n' 'expected one snapshot policy and nextest NEXTEST_ENV' >&2
  exit 2
fi
case "$1" in
  gnu) policy=on ;;
  legacy) policy=off ;;
  *) printf 'unknown snapshot fixture policy: %s\n' "$1" >&2; exit 2 ;;
esac
printf 'NEOMACS_REDISPLAY_GNU_HOOKS=%s\n' "$policy" >>"$NEXTEST_ENV"
