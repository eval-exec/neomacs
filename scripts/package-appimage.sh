#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-appimage.sh [--target TRIPLE] [--skip-build] [--no-smoke]

Build and package the NEO Emacs Linux AppImage.

The AppImage carries the same product as the tarball, so it links GStreamer
and linuxdeploy bundles that closure into the image. Codec plugin families
still come from the host.

Options:
  --target TRIPLE Rust target triple. Defaults to x86_64-unknown-linux-gnu.
  --skip-build    Reuse existing target/release artifacts.
  --no-smoke      Do not smoke-test the AppImage.

Environment:
  LINUXDEPLOY_APPIMAGE   Path to a target-native linuxdeploy AppImage or binary.
  APPIMAGETOOL_APPIMAGE  Path to a target-native appimagetool AppImage or binary.

Output:
  dist/neomacs-{version}-{target}.AppImage
USAGE
}

get_version() {
  local v
  v="$(git describe --tags --abbrev=0 2>/dev/null)" && echo "${v#v}" && return
  v="$(git rev-parse --short=12 HEAD 2>/dev/null)" && echo "$v" && return
  echo "0.0.0-dev"
}

target_triple="x86_64-unknown-linux-gnu"
skip_build=0
smoke=1

while (($#)); do
  case "$1" in
    --target)
      target_triple="${2:?--target requires a value}"
      shift 2
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --no-smoke)
      smoke=0
      shift
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

case "$target_triple" in
  x86_64-*)  appimage_arch="x86_64" ;;
  aarch64-*) appimage_arch="aarch64" ;;
  *)
    echo "unsupported Linux AppImage target: $target_triple" >&2
    exit 1
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"

dist_dir="$repo_root/dist"
version="$(get_version)"
package_name="neomacs-${version}-${target_triple}"
package_dir="$dist_dir/$package_name"
appdir="$dist_dir/$package_name.AppDir"
appimage="$dist_dir/$package_name.AppImage"

linuxdeploy="${LINUXDEPLOY_APPIMAGE:-$(command -v linuxdeploy || true)}"
appimagetool="${APPIMAGETOOL_APPIMAGE:-$(command -v appimagetool || true)}"

if [[ -z "$linuxdeploy" || ! -x "$linuxdeploy" ]]; then
  echo "linuxdeploy not found; set LINUXDEPLOY_APPIMAGE to an executable path" >&2
  exit 1
fi
if [[ -z "$appimagetool" || ! -x "$appimagetool" ]]; then
  echo "appimagetool not found; set APPIMAGETOOL_APPIMAGE to an executable path" >&2
  exit 1
fi

if [[ ! -x "$package_dir/bin/neomacs" || ! -d "$package_dir/share/neomacs/lisp" ]]; then
  package_args=(--target "$target_triple")
  if ((skip_build)); then
    package_args+=(--skip-build)
  fi
  package_args+=(--no-smoke)
  scripts/package-release.sh "${package_args[@]}"
elif ((skip_build == 0)); then
  scripts/package-release.sh --target "$target_triple" --no-smoke
fi

# The AppImage's /usr is the install prefix, so the release tree's three
# top-level directories map straight across -- including libexec, which is
# GNU's archlibdir and holds the dump image.  Copying bin/ without it ships a
# binary that cannot start.
archlib_rel="$(neomacs_archlib_relpath "$repo_root/Cargo.toml" "$target_triple")"

rm -rf "$appdir" "$appimage"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/neomacs" "$appdir/usr/$archlib_rel"

cp -a "$package_dir/bin/." "$appdir/usr/bin/"
cp -a "$package_dir/$archlib_rel/." "$appdir/usr/$archlib_rel/"
cp -a "$package_dir/share/neomacs/." "$appdir/usr/share/neomacs/"
install -m 0644 "$package_dir/README.md" "$appdir/usr/share/neomacs/README.md"
install -m 0644 "$package_dir/COPYING" "$appdir/usr/share/neomacs/COPYING"

neomacs_verify_archlib \
  "$appdir/usr/bin/neomacs" \
  "$appdir/usr/$archlib_rel/neomacs.pdump" \
  "$appdir/usr/$archlib_rel" \
  "$appdir/usr/share/neomacs"

scripts/install-linux-desktop-assets.sh "$appdir/usr"
desktop_file="$appdir/usr/share/applications/neomacs.desktop"
icon_file="$appdir/usr/share/icons/hicolor/scalable/apps/neomacs.svg"

cat >"$appdir/AppRun" <<'APPRUN'
#!/usr/bin/env sh
HERE="$(dirname "$(readlink -f "$0")")"
export NEOMACS_RUNTIME_ROOT="${NEOMACS_RUNTIME_ROOT:-$HERE/usr/share/neomacs}"
exec "$HERE/usr/bin/neomacs" "$@"
APPRUN
chmod 0755 "$appdir/AppRun"

"$linuxdeploy" \
  --appdir "$appdir" \
  --executable "$appdir/usr/bin/neomacs" \
  --desktop-file "$desktop_file" \
  --icon-file "$icon_file"

env -u SOURCE_DATE_EPOCH ARCH="$appimage_arch" "$appimagetool" "$appdir" "$appimage"
chmod 0755 "$appimage"

if ((smoke)); then
  APPIMAGE_EXTRACT_AND_RUN=1 \
    NEOMACS_RUNTIME_ROOT='' \
    timeout 30s "$appimage" --batch --eval "(kill-emacs 0)"
fi

echo "wrote $appimage"
