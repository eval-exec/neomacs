#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/vendor-windows-gstreamer-runtime.sh --package-root DIR --bin-dir DIR

Copy the GStreamer MSVC runtime into a Windows release package.

The executable loader must find startup DLLs such as glib-2.0-0.dll and
gstreamer-1.0-0.dll before neomacs.exe runs, so the DLLs are copied beside
neomacs.exe.  GStreamer plugins and helper programs are copied under the
package root using the upstream runtime layout.

Requires GSTREAMER_ROOT to point at a GStreamer MSVC runtime root.
USAGE
}

package_root=
bin_dir=

while (($#)); do
  case "$1" in
    --package-root)
      package_root="${2:?--package-root requires a value}"
      shift 2
      ;;
    --bin-dir)
      bin_dir="${2:?--bin-dir requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$package_root" || -z "$bin_dir" ]]; then
  usage >&2
  exit 2
fi

if [[ -z "${GSTREAMER_ROOT:-}" ]]; then
  echo "GSTREAMER_ROOT is not set; cannot vendor Windows GStreamer runtime" >&2
  exit 1
fi

to_posix_path() {
  local path="$1"
  if command -v cygpath &>/dev/null; then
    cygpath -u "$path"
  else
    printf '%s\n' "$path"
  fi
}

copy_dir_if_present() {
  local source="$1"
  local dest="$2"
  if [[ -d "$source" ]]; then
    rm -rf "$dest"
    mkdir -p "$(dirname "$dest")"
    cp -a "$source" "$dest"
  fi
}

copy_runtime_dlls_to() {
  local dest="$1"
  mkdir -p "$dest"
  find "$gst_bin" -maxdepth 1 -type f -name '*.dll' -exec cp -p '{}' "$dest/" ';'
}

package_root="$(to_posix_path "$package_root")"
bin_dir="$(to_posix_path "$bin_dir")"
gst_root="$(to_posix_path "$GSTREAMER_ROOT")"
gst_bin="$gst_root/bin"

if [[ ! -d "$package_root" ]]; then
  echo "package root does not exist: $package_root" >&2
  exit 1
fi
if [[ ! -d "$bin_dir" ]]; then
  echo "binary directory does not exist: $bin_dir" >&2
  exit 1
fi
if [[ ! -d "$gst_bin" ]]; then
  echo "GStreamer bin directory does not exist: $gst_bin" >&2
  exit 1
fi

required_dlls=(
  glib-2.0-0.dll
  gobject-2.0-0.dll
  gstreamer-1.0-0.dll
  gstvideo-1.0-0.dll
)

for dll in "${required_dlls[@]}"; do
  if [[ ! -f "$gst_bin/$dll" ]]; then
    echo "required GStreamer runtime DLL is missing: $gst_bin/$dll" >&2
    exit 1
  fi
done

required_dll_families=(cairo 'pango-' pangocairo pangowin32)
for dll_family in "${required_dll_families[@]}"; do
  if ! find "$gst_bin" -maxdepth 1 -type f -iname "*${dll_family}*.dll" -print -quit \
    | grep -q .; then
    echo "required ${dll_family} runtime DLL is missing from: $gst_bin" >&2
    exit 1
  fi
done

echo "vendoring GStreamer runtime from $gst_root"
copy_runtime_dlls_to "$bin_dir"

copy_dir_if_present "$gst_root/lib/gstreamer-1.0" "$package_root/lib/gstreamer-1.0"
copy_dir_if_present "$gst_root/lib/gio" "$package_root/lib/gio"
copy_dir_if_present "$gst_root/lib/girepository-1.0" "$package_root/lib/girepository-1.0"
copy_dir_if_present "$gst_root/libexec/gstreamer-1.0" "$package_root/libexec/gstreamer-1.0"
copy_dir_if_present "$gst_root/share/gstreamer-1.0" "$package_root/share/gstreamer-1.0"
copy_dir_if_present "$gst_root/share/glib-2.0" "$package_root/share/glib-2.0"
copy_dir_if_present "$gst_root/etc" "$package_root/etc/gstreamer"

scanner_dir="$package_root/libexec/gstreamer-1.0"
if [[ -d "$scanner_dir" ]]; then
  copy_runtime_dlls_to "$scanner_dir"
fi

mkdir -p "$package_root/vendor/gstreamer"
cat >"$package_root/vendor/gstreamer/README.txt" <<README
This package includes the GStreamer MSVC runtime files required by neomacs.exe.
They are copied from:

$gst_root
README

echo "vendored GStreamer runtime into $package_root"
