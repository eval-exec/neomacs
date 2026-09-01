#!/usr/bin/env bash
set -euo pipefail

if (($# < 3 || $# > 4)); then
  echo "usage: $0 PACKAGE_DIR PRODUCT_VERSION OUTPUT_FILE [PRODUCT_ARCH]" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$(cd "$1" && pwd -P)"
version="$2"
output_dir="$(cd "$(dirname "$3")" && pwd -P)"
output_file="$output_dir/$(basename "$3")"
product_arch="${4:-}"

if [[ -z "$product_arch" ]]; then
  case "$(uname -m)" in
    x86_64|amd64) product_arch="x86_64" ;;
    aarch64|arm64) product_arch="aarch64" ;;
    *)
      echo "cannot detect Windows installer architecture; pass PRODUCT_ARCH" >&2
      exit 1
      ;;
  esac
fi
case "$product_arch" in
  x86_64|aarch64) ;;
  *)
    echo "unsupported Windows installer architecture: $product_arch" >&2
    exit 1
    ;;
esac

if ! command -v makensis &>/dev/null; then
  echo "makensis not found; install NSIS first" >&2
  exit 1
fi

uninstall_include="$(mktemp "$output_dir/neomacs-uninstall-files.XXXXXX.nsh")"
trap 'rm -f "$uninstall_include"' EXIT
"$repo_root/scripts/generate-nsis-uninstall-include.sh" \
  "$package_dir" \
  "$uninstall_include"

makensis -V2 \
  -DPRODUCT_VERSION="$version" \
  -DPRODUCT_ARCH="$product_arch" \
  -DSOURCE_DIR="$(cygpath -w "$package_dir" 2>/dev/null || echo "$package_dir")" \
  -DOUTPUT_FILE="$(cygpath -w "$output_file" 2>/dev/null || echo "$output_file")" \
  -DUNINSTALL_INCLUDE="$(cygpath -w "$uninstall_include" 2>/dev/null || echo "$uninstall_include")" \
  "$repo_root/assets/windows-installer.nsi"

echo "wrote $output_file"
