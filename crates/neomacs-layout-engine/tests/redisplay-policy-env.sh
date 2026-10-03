#!/usr/bin/env bash
# Startup fixture policy only. Nextest gives matching tests independent child
# processes. Each process publishes one immutable numeric knob through its
# existing OnceLock; independent mutators share no mutable selector or Lisp state.
# This script never changes the caller environment or selects cost policies.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf '%s\n' 'expected exactly one redisplay fixture policy' >&2
  exit 2
fi
case "$1" in
  gnu-hooks) policy_key=NEOMACS_REDISPLAY_GNU_HOOKS ;;
  object-extent) policy_key=NEOMACS_POSN_OBJECT_EXTENT ;;
  *)
    printf 'unknown redisplay fixture policy: %s\n' "$1" >&2
    exit 2
    ;;
esac
if [[ -z "${NEXTEST_ENV:-}" ]]; then
  printf '%s\n' 'cargo-nextest must supply NEXTEST_ENV' >&2
  exit 2
fi
printf '%s=on\n' "$policy_key" >>"$NEXTEST_ENV"
