#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${GSTREAMER_ROOT:-}" ]]; then
  echo "GSTREAMER_ROOT is not set" >&2
  return 1 2>/dev/null || exit 1
fi

if [[ -z "${PKG_CONFIG:-}" ]]; then
  echo "PKG_CONFIG is not set" >&2
  return 1 2>/dev/null || exit 1
fi

if ! command -v cygpath &>/dev/null; then
  echo "cygpath is required to prepare Windows GStreamer paths" >&2
  return 1 2>/dev/null || exit 1
fi

gst_root_posix="$(cygpath -u "$GSTREAMER_ROOT")"
pkg_config_posix="$(cygpath -u "$PKG_CONFIG")"

export PATH="$(dirname "$pkg_config_posix"):$gst_root_posix/bin:$PATH"
export PKG_CONFIG="$(cygpath -w "$PKG_CONFIG")"
export PKG_CONFIG_PATH="$(cygpath -w "$gst_root_posix/lib/pkgconfig")"
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"

if [[ "${1:-}" == "--verify" ]]; then
  # librsvg links Pango and PangoCairo.  GStreamer's MSVC build implements
  # PangoCairo with the native Pangowin32 backend; release packaging validates
  # those runtime DLLs separately.
  find "$gst_root_posix" \( \
    -name 'glib-2.0.pc' -o \
    -name 'gstreamer-1.0.pc' -o \
    -name 'cairo.pc' -o \
    -name 'pango.pc' -o \
    -name 'pangocairo.pc' \
  \)
  "$pkg_config_posix" --version
  "$pkg_config_posix" --modversion glib-2.0
  "$pkg_config_posix" --modversion gstreamer-1.0
  "$pkg_config_posix" --modversion cairo
  "$pkg_config_posix" --modversion pango
  "$pkg_config_posix" --modversion pangocairo
fi
