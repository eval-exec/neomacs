#!/usr/bin/env bash
# Build one trusted main runtime root. Authentication and uploading belong to
# the workflow's separate final step, never to Nix evaluation or builders.
set -euo pipefail

fail() {
  printf 'main cache: %s\n' "$1" >&2
  exit 1
}

[[ "${GITHUB_REPOSITORY:-}" == eval-exec/neomacs ]] || fail 'unexpected repository'
[[ "${GITHUB_REF:-}" == refs/heads/main ]] || fail 'only main may be published'
case "${GITHUB_EVENT_NAME:-}" in
  schedule | workflow_dispatch) ;;
  *) fail 'only scheduled or manual main publication is allowed' ;;
esac

[[ "${GITHUB_SHA:-}" =~ ^[0-9a-f]{40}$ ]] || fail 'event commit must be a full lowercase SHA'
[[ "${CACHIX_AUTH_TOKEN+x}" != x ]] || fail 'the build must not receive a cache credential'
[[ -n "${GITHUB_OUTPUT:-}" ]] || fail 'GITHUB_OUTPUT is required'

case "${NIX_SYSTEM:-}" in
  x86_64-linux | aarch64-linux | aarch64-darwin) ;;
  *) fail 'unsupported native system' ;;
esac

actual_head="$(git rev-parse HEAD)"
[[ "$actual_head" == "$GITHUB_SHA" ]] || fail 'checkout does not match the event commit'

# Do not substitute a moving branch, development shell, check, or incidental
# store path. An unsuccessful build must not produce a root for the next step.
path="$(
  nix build --accept-flake-config --no-link --print-out-paths \
    ".#packages.${NIX_SYSTEM}.neomacs"
)"
[[ "$path" =~ ^/nix/store/[0123456789abcdfghijklmnpqrsvwxyz]{32}-[a-zA-Z0-9+._?=-]+$ ]] \
  || fail 'expected exactly one runtime store root'

printf 'full-path=%s\n' "$path" >>"$GITHUB_OUTPUT"
