#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-deb.sh [--target TRIPLE] [--skip-build] [--no-smoke]

Build and package a .deb for NEO Emacs.

Options:
  --target TRIPLE Rust target triple. Defaults to host.
  --skip-build    Reuse existing target/release artifacts.
  --no-smoke      Do not smoke-test the binary.

Output:
  dist/neomacs_{version}_{arch}.deb
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

arch_from_triple() {
  case "$1" in
    x86_64-*)  echo "amd64" ;;
    aarch64-*) echo "arm64" ;;
    armv7-*)   echo "armhf" ;;
    *)         echo "unknown" ;;
  esac
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"

dist_dir="$repo_root/dist"
version="$(get_version)"
deb_arch="$(arch_from_triple "$target_triple")"
deb_name="neomacs_${version}_${deb_arch}.deb"
deb_path="$dist_dir/$deb_name"
pkg_dir="$dist_dir/deb-staging"

pkg_args=(--target "$target_triple")
if ((skip_build)); then
  pkg_args+=(--skip-build)
fi
pkg_args+=(--no-smoke)

scripts/package-release.sh "${pkg_args[@]}"

release_tree="$dist_dir/neomacs-${version}-${target_triple}"
if [[ ! -d "$release_tree" ]]; then
  echo "release tree not found: $release_tree" >&2
  exit 1
fi

echo "building .deb package..."

rm -rf "$pkg_dir"
mkdir -p "$pkg_dir/DEBIAN"
mkdir -p "$pkg_dir/usr/bin"
mkdir -p "$pkg_dir/usr/share/neomacs"
mkdir -p "$pkg_dir/usr/share/doc/neomacs"

install -m 0755 "$release_tree/bin/neomacs" "$pkg_dir/usr/bin/neomacs"
if [[ -x "$release_tree/bin/neomacsclient" ]]; then
  install -m 0755 "$release_tree/bin/neomacsclient" "$pkg_dir/usr/bin/neomacsclient"
fi

# GNU's archlibdir under the /usr prefix -- the dump and the private helpers,
# which is what keeps a 20 MB memory image and three build-internal binaries
# out of /usr/bin.  `exec-directory' names this directory at runtime; the
# check below proves the binary agrees.
archlib_rel="$(neomacs_archlib_relpath "$repo_root/Cargo.toml" "$target_triple")"
mkdir -p "$pkg_dir/usr/$archlib_rel"
cp -a "$release_tree/$archlib_rel/." "$pkg_dir/usr/$archlib_rel/"

cp -a "$release_tree/share/neomacs/." "$pkg_dir/usr/share/neomacs/"

install -m 0644 README.md "$pkg_dir/usr/share/doc/neomacs/README.md"
install -m 0644 COPYING "$pkg_dir/usr/share/doc/neomacs/copyright"

scripts/install-linux-desktop-assets.sh "$pkg_dir/usr"

installed_size="$(du -sk "$pkg_dir" | cut -f1)"

# Let Debian's shlibs/symbols database derive the complete ELF dependency
# closure. This includes GStreamer for the full product and automatically
# follows SONAME/package transitions instead of duplicating them here.
mkdir -p "$pkg_dir/debian"
cat >"$pkg_dir/debian/control" <<CONTROL
Source: neomacs
Section: editors
Priority: optional
Maintainer: eval-exec <noreply@github.com>

Package: neomacs
Architecture: ${deb_arch}
Description: NEO Emacs
CONTROL
mapfile -d '' -t packaged_executables < <(
  find "$pkg_dir/usr" -type f -perm /111 -print0
)
shlib_args=()
for executable in "${packaged_executables[@]}"; do
  shlib_args+=("-e${executable#"$pkg_dir/"}")
done
shlibs="$({
  cd "$pkg_dir"
  dpkg-shlibdeps -O "${shlib_args[@]}"
} | sed -n 's/^shlibs:Depends=//p')"
rm -rf "$pkg_dir/debian"
if [[ -z "$shlibs" ]]; then
  echo "dpkg-shlibdeps did not resolve the Neomacs runtime closure" >&2
  exit 1
fi

cat >"$pkg_dir/DEBIAN/control" <<CONTROL
Package: neomacs
Version: ${version}
Section: editors
Priority: optional
Architecture: ${deb_arch}
Installed-Size: ${installed_size}
Maintainer: eval-exec <noreply@github.com>
Homepage: https://github.com/eval-exec/neomacs
Description: NEO Emacs
 Extensible, programmable text editor based on Emacs Lisp
 and the Neovim virtual machine, built with Rust.
Depends: ${shlibs}
Recommends: gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-plugins-ugly, gstreamer1.0-libav
CONTROL

cat >"$pkg_dir/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
fi
POSTINST
chmod 0755 "$pkg_dir/DEBIAN/postinst"

cat >"$pkg_dir/DEBIAN/postrm" <<'POSTRM'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
fi
POSTRM
chmod 0755 "$pkg_dir/DEBIAN/postrm"

# The staged tree is the /usr the package will unpack to, so the archlib
# check runs against it directly -- before dpkg-deb turns a wrong layout into
# a published artifact.
neomacs_verify_archlib \
  "$pkg_dir/usr/bin/neomacs" \
  "$pkg_dir/usr/$archlib_rel/neomacs.pdump" \
  "$pkg_dir/usr/$archlib_rel" \
  "$pkg_dir/usr/share/neomacs"

dpkg-deb --build "$pkg_dir" "$deb_path"
rm -rf "$pkg_dir"

if ((smoke)); then
  echo "smoke-testing binary..."
  NEOMACS_RUNTIME_ROOT="$release_tree/share/neomacs" \
    timeout 30s "$release_tree/bin/neomacs" --batch --eval "(kill-emacs 0)"
fi

echo "wrote $deb_path"
