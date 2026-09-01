#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-rpm.sh [--target TRIPLE] [--skip-build] [--no-smoke]

Build and package an .rpm for NEO Emacs.

Requires rpmbuild to be installed.

Options:
  --target TRIPLE Rust target triple. Defaults to host.
  --skip-build    Reuse existing target/release artifacts.
  --no-smoke      Do not smoke-test the binary.

Output:
  dist/neomacs-{version}-1.{arch}.rpm
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
    x86_64-*)  echo "x86_64" ;;
    aarch64-*) echo "aarch64" ;;
    armv7-*)   echo "armv7hl" ;;
    *)         echo "unknown" ;;
  esac
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"

dist_dir="$repo_root/dist"
version="$(get_version)"
rpm_arch="$(arch_from_triple "$target_triple")"

if ! command -v rpmbuild &>/dev/null; then
  echo "rpmbuild not found; install rpm-build first" >&2
  exit 1
fi

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

echo "building .rpm package..."

rpm_topdir="$dist_dir/rpm-topdir"
rm -rf "$rpm_topdir"
mkdir -p "$rpm_topdir"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

payload="$rpm_topdir/SOURCES/neomacs-payload"
install -d "$payload/usr/bin"
install -d "$payload/usr/share/neomacs"
install -d "$payload/usr/share/doc/neomacs"

install -m 0755 "$release_tree/bin/neomacs" "$payload/usr/bin/neomacs"
extra_bin_files=""
if [[ -x "$release_tree/bin/neomacsclient" ]]; then
  install -m 0755 "$release_tree/bin/neomacsclient" "$payload/usr/bin/neomacsclient"
  extra_bin_files="/usr/bin/neomacsclient
"
fi

# GNU's archlibdir under /usr: the dump and the private helpers.  Owned as a
# directory in %files so every file below it is packaged without listing them.
archlib_rel="$(neomacs_archlib_relpath "$repo_root/Cargo.toml" "$target_triple")"
install -d "$payload/usr/$archlib_rel"
cp -a "$release_tree/$archlib_rel/." "$payload/usr/$archlib_rel/"

cp -a "$release_tree/share/neomacs/." "$payload/usr/share/neomacs/"

install -m 0644 README.md "$payload/usr/share/doc/neomacs/README.md"
install -m 0644 COPYING "$payload/usr/share/doc/neomacs/COPYING"

scripts/install-linux-desktop-assets.sh "$payload/usr"

neomacs_verify_archlib \
  "$payload/usr/bin/neomacs" \
  "$payload/usr/$archlib_rel/neomacs.pdump" \
  "$payload/usr/$archlib_rel" \
  "$payload/usr/share/neomacs"

cat >"$rpm_topdir/SPECS/neomacs.spec" <<SPEC
Name:           neomacs
Version:        ${version}
Release:        1%{?dist}
Summary:        NEO Emacs - Extensible text editor

License:        LGPL-3.0
URL:            https://github.com/eval-exec/neomacs

Requires:       fontconfig
Requires:       glib2
Requires:       cairo
Requires:       pango
Recommends:     gstreamer1-plugins-base
Recommends:     gstreamer1-plugins-good
Recommends:     gstreamer1-plugins-bad-free
Recommends:     gstreamer1-plugins-ugly-free
Recommends:     gstreamer1-libav

%description
NEO Emacs is an extensible, programmable text editor based on
Emacs Lisp and the Neovim virtual machine, built with Rust.

%install
mkdir -p %{buildroot}
cp -a %{_sourcedir}/neomacs-payload/. %{buildroot}/

%files
%doc /usr/share/doc/neomacs/README.md
%license /usr/share/doc/neomacs/COPYING
/usr/bin/neomacs
${extra_bin_files}/usr/libexec/neomacs/
/usr/share/neomacs/
/usr/share/applications/neomacs.desktop
/usr/share/icons/hicolor/scalable/apps/neomacs.svg

%post
update-desktop-database /usr/share/applications 2>/dev/null || :
gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || :

%postun
update-desktop-database /usr/share/applications 2>/dev/null || :
gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || :

%changelog
* $(date '+%a %b %d %Y') eval-exec <noreply@github.com> - ${version}-1
- Release ${version}
SPEC

rpmbuild -bb \
  --define "_topdir $rpm_topdir" \
  --define "_dbpath $rpm_topdir/rpmdb" \
  --target "$rpm_arch" \
  "$rpm_topdir/SPECS/neomacs.spec"

rpm_file="$(find "$rpm_topdir/RPMS" -name '*.rpm' -type f | head -1)"
if [[ -z "$rpm_file" ]]; then
  echo "rpmbuild did not produce an .rpm" >&2
  exit 1
fi

cp "$rpm_file" "$dist_dir/"
rm -rf "$rpm_topdir"

if ((smoke)); then
  echo "smoke-testing binary..."
  NEOMACS_RUNTIME_ROOT="$release_tree/share/neomacs" \
    timeout 30s "$release_tree/bin/neomacs" --batch --eval "(kill-emacs 0)"
fi

echo "wrote $dist_dir/$(basename "$rpm_file")"
