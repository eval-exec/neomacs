#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
  echo "the GStreamer macOS SDK can only be installed on macOS" >&2
  exit 1
fi

version="${GSTREAMER_VERSION:-1.26.9}"
case "$version" in
  1.26.9)
    runtime_sha256=1776d6dea6edeb74def606b88ceac20c4fab1397d26987d9c09f9133ab02900e
    devel_sha256=226a98881ab890708b44c86993a2f562db1ec87d32371d7545d00e8a27d8f5b4
    ;;
  *)
    echo "unsupported GStreamer SDK version: $version" >&2
    echo "add its release checksums to scripts/setup-macos-gstreamer.sh first" >&2
    exit 1
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache_dir="$repo_root/tmp/gstreamer-macos/$version"
base_url="https://gstreamer.freedesktop.org/data/pkg/osx/$version"
runtime_name="gstreamer-1.0-$version-universal.pkg"
devel_name="gstreamer-1.0-devel-$version-universal.pkg"
runtime_pkg="$cache_dir/$runtime_name"
devel_pkg="$cache_dir/$devel_name"

mkdir -p "$cache_dir"

download_and_verify() {
  local name="$1"
  local expected="$2"
  local destination="$cache_dir/$name"
  local actual

  if [[ ! -f "$destination" ]] \
    || [[ "$(shasum -a 256 "$destination" | awk '{print $1}')" != "$expected" ]]; then
    rm -f "$destination"
    curl --fail --location --retry 3 --output "$destination" "$base_url/$name"
  fi

  actual="$(shasum -a 256 "$destination" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $name" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

download_and_verify "$runtime_name" "$runtime_sha256"
download_and_verify "$devel_name" "$devel_sha256"

sudo installer -pkg "$runtime_pkg" -target /
sudo installer -pkg "$devel_pkg" -target /

framework_root=/Library/Frameworks/GStreamer.framework/Versions/1.0
framework_bin="$framework_root/bin"
framework_pkgconfig="$framework_root/lib/pkgconfig"
if [[ ! -x "$framework_bin/pkg-config" ]]; then
  echo "GStreamer SDK pkg-config is missing after installation" >&2
  exit 1
fi

"$framework_bin/pkg-config" --modversion gstreamer-1.0

if [[ -n "${GITHUB_PATH:-}" ]]; then
  printf '%s\n' "$framework_bin" >>"$GITHUB_PATH"
fi
if [[ -n "${GITHUB_ENV:-}" ]]; then
  printf 'PKG_CONFIG_PATH=%s\n' "$framework_pkgconfig" >>"$GITHUB_ENV"
fi

echo "installed verified GStreamer macOS SDK $version"
