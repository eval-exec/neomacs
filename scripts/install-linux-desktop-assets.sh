#!/usr/bin/env bash
set -euo pipefail

if (($# != 1)); then
  echo "usage: $0 DESTINATION_ROOT" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
destination_root="$1"
asset_root="$repo_root/crates/neomacs-display-runtime/assets"
desktop_source="$asset_root/neomacs.desktop"
desktop_file_id="$(basename "$desktop_source")"
icon_name="$(sed -n 's/^Icon=//p' "$desktop_source")"

if [[ -z "$icon_name" || "$icon_name" == *$'\n'* ]]; then
  echo "desktop entry must define Icon exactly once" >&2
  exit 1
fi

install -D -m 0644 \
  "$desktop_source" \
  "$destination_root/share/applications/$desktop_file_id"
install -D -m 0644 \
  "$asset_root/window-icon.svg" \
  "$destination_root/share/icons/hicolor/scalable/apps/$icon_name.svg"
