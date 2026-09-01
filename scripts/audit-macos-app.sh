#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/audit-macos-app.sh PATH/TO/neomacs.app

Verify that a macOS application bundle is relocatable.  Every Mach-O image
under Contents/MacOS, Contents/Frameworks, Contents/Helpers, and
Contents/PlugIns and Contents/Resources are inspected recursively
(loadable modules live under Resources: a SUBDIRECTORY of a nested-code root
such as PlugIns or Frameworks cannot be signed unless it is a real bundle).  Non-system dependencies must use a
bundle-relative install name and must resolve to a file inside the app.

The audit runs with Apple's `otool` on macOS and LLVM's compatible
`llvm-otool` on other development hosts.
USAGE
}

if (($# != 1)); then
  usage >&2
  exit 2
fi

app="$1"
contents="$app/Contents"
macos_dir="$contents/MacOS"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=./scripts/lib/macos-macho.sh
source "$script_dir/lib/macos-macho.sh"

if [[ ! -d "$app" || ! -d "$macos_dir" ]]; then
  echo "invalid macOS application bundle: $app" >&2
  exit 1
fi

if command -v otool >/dev/null 2>&1; then
  otool_cmd=otool
elif command -v llvm-otool >/dev/null 2>&1; then
  otool_cmd=llvm-otool
else
  echo "otool or llvm-otool is required to audit a macOS application" >&2
  exit 1
fi

for command in file python3; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required to audit a macOS application" >&2
    exit 1
  fi
done

canonical_path() {
  python3 - "$1" <<'PY'
import os
import sys

print(os.path.realpath(sys.argv[1]))
PY
}

app_canonical="$(canonical_path "$app")"
contents_canonical="$(canonical_path "$contents")"
if [[ "$contents_canonical" != "$app_canonical/Contents" ]]; then
  echo "application Contents directory escapes the bundle: $contents_canonical" >&2
  exit 1
fi

canonical_path_within_contents() {
  local label="$1"
  local path="$2"
  local canonical

  if [[ ! -e "$path" ]]; then
    echo "$label does not exist: $path" >&2
    return 1
  fi
  canonical="$(canonical_path "$path")"
  case "$canonical" in
    "$contents_canonical"/*)
      ;;
    *)
      echo "$label escapes Contents: $path" >&2
      echo "  resolved: $canonical" >&2
      return 1
      ;;
  esac
}

resolve_dependency() {
  local image="$1"
  local dependency="$2"
  local target

  case "$dependency" in
    /usr/lib/*|/System/Library/*)
      return 0
      ;;
    @executable_path/*)
      target="$macos_dir/${dependency#@executable_path/}"
      ;;
    @loader_path/*)
      target="$(dirname "$image")/${dependency#@loader_path/}"
      ;;
    @rpath/*)
      # A bare existence check cannot prove which LC_RPATH dyld will select,
      # and the build runner's SDK could make an external run path look valid.
      # The vendoring stage therefore canonicalizes private dependencies to an
      # explicit @executable_path or @loader_path before this audit.
      echo "ambiguous @rpath dependency in $image: $dependency" >&2
      return 1
      ;;
    /*)
      echo "external dependency in $image: $dependency" >&2
      return 1
      ;;
    *)
      echo "non-relocatable dependency in $image: $dependency" >&2
      return 1
      ;;
  esac

  if [[ ! -e "$target" ]]; then
    echo "missing bundled dependency for $image: $dependency" >&2
    echo "  expected: $target" >&2
    return 1
  fi

  local target_canonical
  target_canonical="$(canonical_path "$target")"
  if ! canonical_path_within_contents "bundled dependency in $image" "$target"; then
    echo "  load command: $dependency" >&2
    return 1
  fi
  if [[ ! -f "$target_canonical" ]] || ! is_macho "$target_canonical"; then
    echo "bundled dependency is not a regular Mach-O file in $image: $dependency" >&2
    echo "  resolved: $target_canonical" >&2
    return 1
  fi
}

images=0
failures=0
gstreamer_linked=0

for root in $(macos_bundle_scan_roots); do
  [[ -d "$contents/$root" ]] || continue
  while IFS= read -r -d '' image; do
    is_macho "$image" || continue
    images=$((images + 1))
    if ! dependencies="$(macho_dependency_paths "$otool_cmd" "$image")"; then
      echo "failed to inspect Mach-O load commands: $image" >&2
      failures=$((failures + 1))
      continue
    fi
    while IFS= read -r dependency; do
      [[ -n "$dependency" ]] || continue
      [[ "$dependency" == *libgstreamer-1.0* ]] && gstreamer_linked=1
      if [[ "$dependency" == *libfontconfig* \
        || "$dependency" == *libX11* \
        || "$dependency" == *libXft* ]]; then
        echo "forbidden non-native macOS font dependency in $image: $dependency" >&2
        failures=$((failures + 1))
      fi
      if ! resolve_dependency "$image" "$dependency"; then
        failures=$((failures + 1))
      fi
    done <<<"$dependencies"
  done < <(find "$contents/$root" -type f -print0)
done

if ((gstreamer_linked)); then
  scanner="$contents/Helpers/gst-plugin-scanner"
  if ! canonical_path_within_contents "GStreamer plugin scanner" "$scanner" \
    || [[ -L "$scanner" ]] \
    || [[ ! -f "$scanner" ]] \
    || [[ ! -x "$scanner" ]] \
    || ! is_macho "$scanner"; then
    echo "bundled GStreamer scanner is not a contained executable Mach-O file" >&2
    failures=$((failures + 1))
  fi

  plugin_root="$contents/Resources/gstreamer-1.0"
  plugin_count=0
  if ! canonical_path_within_contents "GStreamer plugin directory" "$plugin_root" \
    || [[ ! -d "$plugin_root" ]]; then
    failures=$((failures + 1))
  elif find "$plugin_root" -type l -print -quit | grep -q .; then
    echo "bundled GStreamer plugin directory contains a symlink" >&2
    failures=$((failures + 1))
  else
    while IFS= read -r -d '' plugin; do
      plugin_count=$((plugin_count + 1))
      if ! canonical_path_within_contents "GStreamer plugin" "$plugin" \
        || [[ ! -f "$plugin" ]] \
        || ! is_macho "$plugin"; then
        echo "bundled GStreamer plugin is not a contained regular Mach-O file: $plugin" >&2
        failures=$((failures + 1))
      fi
    done < <(find "$plugin_root" -type f -print0)
  fi
  if ((plugin_count == 0)); then
    echo "bundled GStreamer has no plugins" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -e "$contents/Resources/fontconfig" ]]; then
  echo "macOS bundle contains forbidden Fontconfig resources" >&2
  failures=$((failures + 1))
fi

if ((images == 0)); then
  echo "no Mach-O images found in $app" >&2
  exit 1
fi

if ((failures != 0)); then
  echo "macOS bundle audit failed: $failures unresolved or external dependencies" >&2
  exit 1
fi

echo "macOS bundle audit passed: $images Mach-O images are relocatable"
