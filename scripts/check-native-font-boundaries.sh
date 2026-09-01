#!/usr/bin/env bash
# Enforce the platform font-catalog split against Cargo's resolved target graph.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0

target_depends_on() {
  local target="$1"
  local package="$2"
  [[ -n "$(cargo tree --locked -p neomacs --target "$target" -e normal -i "$package" --prefix none 2>/dev/null)" ]]
}

require_dependency() {
  local target="$1"
  local package="$2"
  if target_depends_on "$target" "$package"; then
    printf '  ok   %s uses %s\n' "$target" "$package"
  else
    printf '  FAIL %s must use %s as its native font catalog\n' "$target" "$package" >&2
    failures=$((failures + 1))
  fi
}

forbid_dependency() {
  local target="$1"
  local package="$2"
  if target_depends_on "$target" "$package"; then
    printf '  FAIL %s unexpectedly depends on %s\n' "$target" "$package" >&2
    cargo tree --locked -p neomacs --target "$target" -e normal -i "$package" >&2
    failures=$((failures + 1))
  else
    printf '  ok   %s excludes %s\n' "$target" "$package"
  fi
}

linux_target="x86_64-unknown-linux-gnu"
macos_target="aarch64-apple-darwin"
windows_target="x86_64-pc-windows-msvc"

require_dependency "$linux_target" fontconfig
require_dependency "$macos_target" objc2-core-text
require_dependency "$windows_target" dwrote

for target in "$macos_target" "$windows_target"; do
  forbid_dependency "$target" fontconfig
  forbid_dependency "$target" yeslogic-fontconfig-sys
  forbid_dependency "$target" x11-dl
done

forbid_dependency "$macos_target" core-text

if ((failures != 0)); then
  printf 'native font dependency boundary failed: %d violation(s)\n' "$failures" >&2
  exit 1
fi

echo "native font dependency boundary passed"
