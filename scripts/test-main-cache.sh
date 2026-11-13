#!/usr/bin/env bash
# Exercise the main-cache build boundary without Nix, network access, or uploads.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
builder="$repo_root/scripts/ci/build-main-cache.sh"
scratch_root="${TMPDIR:-"$repo_root/tmp"}"
mkdir -p "$scratch_root"
work_dir="$(mktemp -d "$scratch_root/main-cache-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
mkdir -p "$work_dir/bin"

commit="abcdef1234567890abcdef1234567890abcdef12"
other_commit="fedcba9876543210fedcba9876543210fedcba98"
store_root="/nix/store/00000000000000000000000000000000-neomacs"

cat >"$work_dir/bin/git" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'git\n' >>"$MAIN_CACHE_TEST_LOG/git.calls"
if [[ "$#" != 2 || "$1" != rev-parse || "$2" != HEAD ]]; then
  echo 'unexpected Git invocation' >&2
  exit 90
fi
if [[ "${MAIN_CACHE_TEST_GIT_FAIL:-0}" == 1 ]]; then
  exit 19
fi
printf '%s\n' "$MAIN_CACHE_TEST_HEAD"
STUB

cat >"$work_dir/bin/nix" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'nix\n' >>"$MAIN_CACHE_TEST_LOG/nix.calls"
if [[ "${CACHIX_AUTH_TOKEN+x}" == x ]]; then
  echo 'Nix received a cache credential' >&2
  exit 91
fi
if [[ "$#" != 5 || "$1" != build || "$2" != --accept-flake-config \
  || "$3" != --no-link || "$4" != --print-out-paths \
  || "$5" != ".#packages.${NIX_SYSTEM}.neomacs" ]]; then
  echo 'unexpected Nix invocation' >&2
  exit 92
fi
# Progress belongs on stderr and must not become a published root.
echo 'stub build progress' >&2
case "$MAIN_CACHE_TEST_NIX_MODE" in
  valid) printf '%s\n' "$MAIN_CACHE_TEST_ROOT" ;;
  failed) exit 17 ;;
  empty) ;;
  multiple)
    printf '%s\n' "$MAIN_CACHE_TEST_ROOT" \
      /nix/store/11111111111111111111111111111111-unrelated-builder
    ;;
  relative) printf 'result/neomacs\n' ;;
  nonstore) printf '/tmp/neomacs\n' ;;
  short-hash) printf '/nix/store/0000-neomacs\n' ;;
  invalid-base32) printf '/nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-neomacs\n' ;;
  missing-name) printf '/nix/store/00000000000000000000000000000000-\n' ;;
  nested-path) printf '%s/bin/neomacs\n' "$MAIN_CACHE_TEST_ROOT" ;;
  injected-output) printf '%s\nfull-path=/tmp/untrusted\n' "$MAIN_CACHE_TEST_ROOT" ;;
  *) echo 'unknown test fixture' >&2; exit 93 ;;
esac
STUB

# Any attempt to publish is an error, including after an otherwise valid build.
cat >"$work_dir/bin/cachix" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'cachix\n' >>"$MAIN_CACHE_TEST_LOG/cachix.calls"
echo 'the build-only helper attempted to publish' >&2
exit 94
STUB
chmod +x "$work_dir/bin/git" "$work_dir/bin/nix" "$work_dir/bin/cachix"

case_count=0
run_case() {
  local label="$1" expected="$2" nix_calls="$3"
  shift 3
  local case_dir="$work_dir/case-$case_count"
  local status=0
  mkdir -p "$case_dir"
  : >"$case_dir/output"
  : >"$case_dir/git.calls"
  : >"$case_dir/nix.calls"
  : >"$case_dir/cachix.calls"

  # Clear any inherited real credential without reading it. Token cases below
  # install only owned dummy values in the child environment. HOME is inherited.
  env -u CACHIX_AUTH_TOKEN \
    PATH="$work_dir/bin:$PATH" \
    GITHUB_REPOSITORY=eval-exec/neomacs \
    GITHUB_REF=refs/heads/main \
    GITHUB_EVENT_NAME=schedule \
    GITHUB_SHA="$commit" \
    NIX_SYSTEM=x86_64-linux \
    GITHUB_OUTPUT="$case_dir/output" \
    MAIN_CACHE_TEST_LOG="$case_dir" \
    MAIN_CACHE_TEST_HEAD="$commit" \
    MAIN_CACHE_TEST_ROOT="$store_root" \
    MAIN_CACHE_TEST_NIX_MODE=valid \
    MAIN_CACHE_TEST_GIT_FAIL=0 \
    env "$@" "$builder" >"$case_dir/stdout" 2>"$case_dir/stderr" || status=$?

  case "$expected" in
    success)
      if [[ "$status" != 0 ]]; then
        echo "$label: expected success, got exit $status" >&2
        return 1
      fi
      printf 'full-path=%s\n' "$store_root" >"$case_dir/expected-output"
      if ! cmp -s "$case_dir/expected-output" "$case_dir/output"; then
        echo "$label: did not emit exactly the selected package root" >&2
        return 1
      fi
      ;;
    reject)
      if [[ "$status" == 0 ]]; then
        echo "$label: unexpectedly accepted invalid input" >&2
        return 1
      fi
      ;;
    *)
      if [[ "$status" != "$expected" ]]; then
        echo "$label: expected exit $expected, got $status" >&2
        return 1
      fi
      ;;
  esac
  if [[ "$expected" != success && -s "$case_dir/output" ]]; then
    echo "$label: emitted a publish root after failure" >&2
    return 1
  fi
  local observed_nix_calls
  observed_nix_calls="$(wc -l <"$case_dir/nix.calls")"
  if ((observed_nix_calls != nix_calls)); then
    echo "$label: expected $nix_calls Nix calls, got $observed_nix_calls" >&2
    return 1
  fi
  if [[ -s "$case_dir/cachix.calls" ]]; then
    echo "$label: the build-only helper invoked Cachix" >&2
    return 1
  fi
  case_count=$((case_count + 1))
}

for system in x86_64-linux aarch64-linux aarch64-darwin; do
  for event in schedule workflow_dispatch; do
    run_case "$system/$event" success 1 NIX_SYSTEM="$system" GITHUB_EVENT_NAME="$event"
  done
done

for event in push pull_request pull_request_target workflow_call repository_dispatch unknown ''; do
  run_case "untrusted event $event" reject 0 GITHUB_EVENT_NAME="$event"
done
run_case 'missing event' reject 0 -u GITHUB_EVENT_NAME
run_case 'fork repository' reject 0 GITHUB_REPOSITORY=someone/neomacs
run_case 'missing repository' reject 0 -u GITHUB_REPOSITORY
for event in schedule workflow_dispatch; do
  run_case "$event feature ref" reject 0 GITHUB_EVENT_NAME="$event" GITHUB_REF=refs/heads/feature
  run_case "$event tag ref" reject 0 GITHUB_EVENT_NAME="$event" GITHUB_REF=refs/tags/v9.8.7
done
run_case 'main-like ref' reject 0 GITHUB_REF=refs/heads/main-extra
run_case 'missing ref' reject 0 -u GITHUB_REF

run_case 'wrong checkout' reject 0 MAIN_CACHE_TEST_HEAD="$other_commit"
run_case 'unavailable checkout' reject 0 MAIN_CACHE_TEST_GIT_FAIL=1
run_case 'missing SHA' reject 0 -u GITHUB_SHA
for sha in '' abcdef ABCDEF1234567890ABCDEF1234567890ABCDEF12 "${commit}0" "${commit:0:39}" "$commit"$'\ninjected'; do
  run_case 'invalid event SHA' reject 0 GITHUB_SHA="$sha"
done
run_case 'dummy credential set' reject 0 CACHIX_AUTH_TOKEN=owned-test-token
run_case 'empty credential set' reject 0 CACHIX_AUTH_TOKEN=

run_case 'missing system' reject 0 -u NIX_SYSTEM
for system in '' x86_64-darwin x86_64-linux-extra 'x86_64-linux;echo injected'; do
  run_case 'unsupported system' reject 0 NIX_SYSTEM="$system"
done

run_case 'build failure propagates' 17 1 MAIN_CACHE_TEST_NIX_MODE=failed
for mode in empty multiple relative nonstore short-hash invalid-base32 missing-name nested-path injected-output; do
  run_case "invalid build result $mode" reject 1 MAIN_CACHE_TEST_NIX_MODE="$mode"
done

printf 'main-cache build contract passed (%s cases)\n' "$case_count"
