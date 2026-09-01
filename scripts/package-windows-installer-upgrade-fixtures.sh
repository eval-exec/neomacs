#!/usr/bin/env bash
set -euo pipefail

if (($# < 1 || $# > 2)); then
  echo "usage: $0 OUTPUT_DIR [PRODUCT_ARCH]" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="$1"
product_arch="${2:-}"
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd -P)"

work_dir="$(mktemp -d "$output_dir/payloads.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
payload_a="$work_dir/a"
payload_b="$work_dir/b"

mkdir -p "$payload_a/bin" "$payload_a/share/neomacs"
mkdir -p "$payload_b/bin" "$payload_b/share/neomacs"

for payload in "$payload_a" "$payload_b"; do
  printf 'fixture executable\n' > "$payload/bin/neomacs.exe"
  printf 'fixture client executable\n' > "$payload/bin/neomacsclient.exe"
done

printf 'version a\n' > "$payload_a/share/neomacs/common.txt"
printf 'owned only by version a\n' > "$payload_a/share/neomacs/removed-in-b.txt"
printf 'version b\n' > "$payload_b/share/neomacs/common.txt"
printf 'owned only by version b\n' > "$payload_b/share/neomacs/added-in-b.txt"

"$repo_root/scripts/compile-windows-installer.sh" \
  "$payload_a" \
  "0.0.0-contract-a" \
  "$output_dir/neomacs-installer-contract-a.exe" \
  "$product_arch"
"$repo_root/scripts/compile-windows-installer.sh" \
  "$payload_b" \
  "0.0.0-contract-b" \
  "$output_dir/neomacs-installer-contract-b.exe" \
  "$product_arch"
