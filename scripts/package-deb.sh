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

# preinst: refuse to unpack over a tree that another account can modify.
# Packages up to v0.0.20 were built without --root-owner-group, so an
# installed one leaves its directories to the build uid (1001), and dpkg never
# changes a directory that already exists: an upgrade would write root's files
# into directories that account can still rename, replace or add to.
#
# Refusing rather than repairing, because a repair would make whatever that
# account planted there (a site-start.el, say) root-owned and
# indistinguishable from the package, destroying the one sign of it -- and
# would have to chown through a tree its adversary controls.
#
# "Can modify" is: not owned by root, or (symlinks aside) writable by group or
# others.  The write bits matter as much as the owner: before handing a tree
# back, its owner can `chmod -R a+w' it or grant itself a named ACL entry, and
# on Linux a named entry's write right only takes effect through the ACL mask,
# which is what the group bits show (acl(5)).  With both rules the check only
# reads, and a race cannot hide anything from it: the topmost entry another
# account can modify sits in a directory it cannot, so it can be neither
# moved nor deleted, and only its owner -- root -- could clear its bits.  A
# path whose existence cannot be established (EACCES, a FUSE mount root
# cannot see into) and a file system mounted inside the trees are refused as
# well.  So is a symlink where the package has a directory: dpkg follows it
# and unpacks wherever it points (Debian Policy 6.6), and once the link is
# root's, as `chown -hR' makes it, none of the rules above would see it.
# The package ships no symlinks in its trees; any other symlink there is left
# alone (install.sh --prefix /usr makes share/neomacs/current).  On failure
# dpkg unpacks nothing and leaves the installed version as it was.
#
# The package's own trees are checked entry by entry, and each directory the
# package has in them must not be a symlink; above them, every directory it
# unpacks into, which it shares with the system, is checked itself (owner,
# and group or other write).  The directory lists come from the staged tree
# so they follow the layout; all are written into the script, so they are
# limited to characters that need no quoting.
own_trees=(/usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs)
if [[ "$archlib_rel" != libexec/neomacs/* ]]; then
  echo "archlib $archlib_rel is outside /usr/libexec/neomacs; update own_trees" >&2
  exit 1
fi
prune=(-path ./DEBIAN -prune)
for tree in "${own_trees[@]}"; do
  if [[ ! -d "$pkg_dir$tree" ]]; then
    echo "package tree $tree is not staged; update own_trees" >&2
    exit 1
  fi
  # The repair instructions and the preinst message say it ships none.
  if [[ -n "$(find "$pkg_dir$tree" -type l -print -quit)" ]]; then
    echo "package tree $tree contains a symlink; the preinst message says it ships none" >&2
    exit 1
  fi
  prune+=(-o -path ".$tree" -prune)
done
mapfile -t unpack_dirs < <(
  cd "$pkg_dir"
  find . \( "${prune[@]}" \) -o -type d -print | sed 's|^\.||; s|^$|/|' | sort
)
if ((${#unpack_dirs[@]} == 0)); then
  echo "no directories found under $pkg_dir" >&2
  exit 1
fi
# Sorted bytewise, so each directory comes after the one that holds it.
mapfile -t own_dirs < <(
  cd "$pkg_dir"
  find "${own_trees[@]/#/.}" -type d -print | sed 's|^\.||' | LC_ALL=C sort
)
for path in "${own_trees[@]}" "${own_dirs[@]}" "${unpack_dirs[@]}"; do
  if [[ ! "$path" =~ ^[A-Za-z0-9._/+-]+$ ]]; then
    echo "path not safe to write into the preinst: $path" >&2
    exit 1
  fi
done
{
  cat <<'PREINST'
#!/bin/sh
# Refuse to unpack over a tree that an account other than root can modify
# (see scripts/package-deb.sh in the neomacs repository).
set -e
# The path lists below are split on spaces; never glob them.
set -f
PREINST
  printf "own_trees='%s'\n" "${own_trees[*]}"
  printf "own_dirs='%s'\n" "${own_dirs[*]}"
  printf "unpack_dirs='%s'\n" "${unpack_dirs[*]}"
  cat <<'PREINST'

case "$1" in
  install|upgrade) ;;
  *) exit 0 ;;
esac

# DPKG_ROOT is the root dpkg unpacks into (empty for the running system).
# dpkg strips trailing slashes except from "/" itself, which would make
# //usr/... paths that never match the mount table.
root=${DPKG_ROOT:-}
root=${root%/}

# present PATH: 0 if PATH exists; 1 only if its name is missing from a parent
# that can be listed; 2 if neither can be established -- a failed stat is not
# an absence (EACCES, a FUSE mount root cannot see into).
present() (
  if [ -e "$1" ] || [ -h "$1" ]; then exit 0; fi
  [ "$1" != / ] || exit 2
  parent=${1%/*}
  parent=${parent:-/}
  present "$parent" || exit $?
  names=$(ls -A -- "$parent" 2>/dev/null) || exit 2
  if printf '%s\n' "$names" | grep -Fxq -- "${1##*/}"; then exit 2; fi
  exit 1
)

# Each check prints what it refuses.  A find that cannot finish names its
# start point instead of passing; its own diagnostics are dropped because they
# would echo names another account chose.
check_tree() {
  find -H "$1" \( ! -uid 0 -o \( ! -type l \( -perm -020 -o -perm -002 \) \) \) \
    -print 2>/dev/null || printf '%s (could not be fully inspected)\n' "$1"
}
check_dir() {
  find -H "$1" -maxdepth 0 \( ! -uid 0 -o -perm -020 -o -perm -002 \) \
    -print 2>/dev/null || printf '%s (could not be inspected)\n' "$1"
}
# A symlink where the package has a directory, which dpkg would unpack
# through.  Only the link itself is examined: a directory below one already
# listed is skipped, since reaching it would mean following the link.
check_links() {
  linked=
  for own in $own_dirs; do
    for link in $linked; do
      case $own in "$link"/*) continue 2 ;; esac
    done
    if [ -h "$root$own" ]; then
      printf '%s (a symlink where the package has a directory)\n' "$root$own"
      linked="$linked $own"
    fi
  done
}
inspect() {
  status=0
  present "$2" || status=$?
  case $status in
    0) "$1" "$2" ;;
    1) ;;
    *) printf '%s (could not tell whether it exists)\n' "$2" ;;
  esac
}

foreign=$(
  for tree in $own_trees; do inspect check_tree "$root$tree"; done
  check_links
  for dir in $unpack_dirs; do inspect check_dir "$root$dir"; done
  # A file system mounted in the package's trees is not the package's,
  # whatever owner and mode it reports.
  if [ -r /proc/self/mountinfo ]; then
    for tree in $own_trees; do
      awk -v t="$root$tree" '$5 == t || index($5, t "/") == 1 {
        print $5 " (a mount point)" }' /proc/self/mountinfo \
        || echo "/proc/self/mountinfo (could not be read)"
    done
  fi
)
[ -z "$foreign" ] && exit 0

count=$(printf '%s\n' "$foreign" | grep -c '')
{
  echo "neomacs: refusing to unpack: $count path(s) the package writes through"
  echo "can be modified by an account other than root:"
  # The names come from a tree another account controls: print them inert.
  printf '%s\n' "$foreign" | head -n 20 | LC_ALL=C tr -c '[:print:]\n' '?' \
    | sed 's/^/  /'
  if [ "$count" -gt 20 ]; then echo "  ..."; fi
  cat <<'MESSAGE'

neomacs packages up to v0.0.20 installed their files as the build account's
uid/gid (1001 for v0.0.20), and dpkg never changes a directory that already
exists, so unpacking over them would leave those directories to that account.
Nothing has been unpacked.

First list, in bash, what the package did not install and every symlink in
its trees (it ships none), and examine each entry: that account created them,
and one site-start.el is enough to run code.  A symlink where the package has
a directory is refused, because dpkg would unpack through it: remove it.

  comm -23 <(find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs | sort) \
    <(dpkg -L neomacs | sort)
  find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs -type l -ls

Unmount any file system listed as a mount point.  Then, as root, work from the
top down.  First each listed directory DIR outside those trees, whether an old
package created it or the system left it writable, parents first (-type d: a
symlink is neither followed nor changed):

  find DIR -maxdepth 0 -type d -exec chown -h root:root {} + -exec chmod go-w {} +

If a listed DIR is itself a symlink, this changes nothing: examine where it
points, and fix or remove it.

Then check that no tree is itself a symlink, since chmod -R follows one named
on its command line.  This must print nothing:

  find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs -maxdepth 0 -type l

Only then the trees:

  chown -hR root:root /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs
  chmod -R go-w /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs

and install the package again.  Details:
https://github.com/eval-exec/neomacs/blob/main/docs/releasing-linux.md#repairing-an-installation-from-v0020-or-earlier
MESSAGE
} >&2
exit 1
PREINST
} >"$pkg_dir/DEBIAN/preinst"
chmod 0755 "$pkg_dir/DEBIAN/preinst"

# The staged tree is the /usr the package will unpack to, so the archlib
# check runs against it directly -- before dpkg-deb turns a wrong layout into
# a published artifact.
neomacs_verify_archlib \
  "$pkg_dir/usr/bin/neomacs" \
  "$pkg_dir/usr/$archlib_rel/neomacs.pdump" \
  "$pkg_dir/usr/$archlib_rel" \
  "$pkg_dir/usr/share/neomacs"

# No member may be writable by group or others: the preinst refuses such a
# tree, and GNU's make-dist strips the same bits (--mode=go-w).  Normalized
# here so a runner with umask 002 cannot ship them.
chmod -R go-w "$pkg_dir"

# --root-owner-group: without it dpkg-deb records the staged tree's owner, the
# release runner's unprivileged uid/gid (1001), and `dpkg -i` then installs
# /usr/bin/neomacs, the dump and the Lisp tree as that uid -- rewritable by any
# local account holding the number.  Needs dpkg 1.19.0 or later (ubuntu-22.04
# ships 1.21.1).
dpkg-deb --build --root-owner-group "$pkg_dir" "$deb_path"
rm -rf "$pkg_dir"

if ((smoke)); then
  echo "smoke-testing binary..."
  NEOMACS_RUNTIME_ROOT="$release_tree/share/neomacs" \
    timeout 30s "$release_tree/bin/neomacs" --batch --eval "(kill-emacs 0)"
fi

echo "wrote $deb_path"
