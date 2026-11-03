#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/test-linux-release-artifacts.sh [options]

Extract, smoke-test, and audit packaged NEO Emacs Linux artifacts.

Options:
  --dist DIR       Artifact directory. Defaults to ./dist.
  --target TRIPLE  Rust target triple. Defaults to x86_64-unknown-linux-gnu.
  --tar-version V  Version component of the tarball name. Defaults to any version.
  --formats LIST   Comma-separated formats. Defaults to
                   tar,appimage,deb,rpm.
  --glibc VERSION  Maximum permitted GLIBC symbol version. Defaults to 2.35.
  -h, --help       Show this help.

All scratch data is created below ./tmp, never /tmp.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"
dist_dir="$repo_root/dist"
target_triple="x86_64-unknown-linux-gnu"
tar_version="*"
formats="tar,appimage,deb,rpm"
glibc_baseline="2.35"

while (($#)); do
  case "$1" in
    --dist)
      dist_dir="${2:?--dist requires a value}"
      shift 2
      ;;
    --target)
      target_triple="${2:?--target requires a value}"
      shift 2
      ;;
    --tar-version)
      tar_version="${2:?--tar-version requires a value}"
      shift 2
      ;;
    --formats)
      formats="${2:?--formats requires a value}"
      shift 2
      ;;
    --glibc)
      glibc_baseline="${2:?--glibc requires a value}"
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

if [[ ! -d "$dist_dir" ]]; then
  echo "artifact directory not found: $dist_dir" >&2
  exit 1
fi
dist_dir="$(cd "$dist_dir" && pwd)"

case "$target_triple" in
  x86_64-*)
    deb_arch="amd64"
    rpm_arch="x86_64"
    ;;
  aarch64-*)
    deb_arch="arm64"
    rpm_arch="aarch64"
    ;;
  armv7-*)
    deb_arch="armhf"
    rpm_arch="armv7hl"
    ;;
  *)
    echo "unsupported Linux target: $target_triple" >&2
    exit 1
    ;;
esac

mkdir -p "$repo_root/tmp"
work_dir="$(mktemp -d "$repo_root/tmp/linux-artifact-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

find_one() {
  local label="$1"
  local pattern="$2"
  local matches=()
  mapfile -t matches < <(find "$dist_dir" -maxdepth 1 -type f -name "$pattern" -print | sort)
  if ((${#matches[@]} != 1)); then
    echo "expected exactly one $label matching $dist_dir/$pattern; found ${#matches[@]}" >&2
    printf '  %s\n' "${matches[@]}" >&2
    return 1
  fi
  printf '%s\n' "${matches[0]}"
}

version_is_newer() {
  local candidate="$1"
  [[ "$candidate" != "$glibc_baseline" \
    && "$(printf '%s\n%s\n' "$candidate" "$glibc_baseline" | sort -V | tail -n 1)" == "$candidate" ]]
}

audit_elf() {
  local file="$1"
  local required
  while IFS= read -r required; do
    if version_is_newer "$required"; then
      echo "$file requires GLIBC_$required, newer than GLIBC_$glibc_baseline" >&2
      return 1
    fi
  done < <(
    readelf --version-info "$file" 2>/dev/null \
      | sed -n 's/.*Name: GLIBC_\([0-9][0-9.]*\).*/\1/p' \
      | sort -Vu
  )
}

audit_tree() {
  local root="$1"
  local file
  while IFS= read -r -d '' file; do
    if readelf --file-header "$file" &>/dev/null; then
      audit_elf "$file"
    fi
  done < <(find "$root" -type f -print0)
}

audit_desktop_identity() {
  local prefix="$1"
  local desktop="$prefix/share/applications/neomacs.desktop"
  local icon="$prefix/share/icons/hicolor/scalable/apps/neomacs.svg"

  cmp "$desktop" "$repo_root/crates/neomacs-display-runtime/assets/neomacs.desktop"
  cmp "$icon" "$repo_root/crates/neomacs-display-runtime/assets/window-icon.svg"
  grep -Fxq 'Exec=neomacs %F' "$desktop"
  grep -Fxq 'Icon=neomacs' "$desktop"
}

smoke_binary() {
  local binary="$1"
  local runtime_root="$2"
  echo "smoke-testing $binary"
  NEOMACS_RUNTIME_ROOT="$runtime_root" \
    timeout 30s "$binary" --batch --eval "(kill-emacs 0)"
}

audit_linked_video_backend() {
  local prefix="$1" binary="$2"
  if find "$prefix" -type f -name 'libneomacs_video_gstreamer.so' -print -quit \
    | grep -q .; then
    echo "release contains obsolete private GStreamer adapter" >&2
    return 1
  fi
  if ! readelf --dynamic "$binary" 2>/dev/null | grep -Eq 'Shared library: \[libgstreamer-1[.]0[.]so'; then
    echo "full executable does not link GStreamer: $binary" >&2
    return 1
  fi
}

# The smoke test above already fails if the dump cannot be found, but it fails
# as "the editor did not start".  This says WHICH directory the artifact
# staged and which one the binary looked in, which is the difference between a
# five-minute fix and an afternoon.
audit_archlib() {
  local prefix="$1" binary="$2" runtime_root="$3" archlib_rel
  archlib_rel="$(neomacs_archlib_relpath "$repo_root/Cargo.toml" "$target_triple")"
  if [[ ! -d "$prefix/$archlib_rel" ]]; then
    echo "artifact has no archlib at $prefix/$archlib_rel" >&2
    return 1
  fi
  neomacs_verify_archlib \
    "$binary" \
    "$prefix/$archlib_rel/neomacs.pdump" \
    "$prefix/$archlib_rel" \
    "$runtime_root"
  audit_linked_video_backend "$prefix" "$binary"
}

# The tarball and the .deb are unpacked by root (dpkg, install.sh --prefix, a
# manual `tar -x` into /usr/local), and root restores each member's recorded
# owner and mode.  They are built by an unprivileged CI account, so a member
# that does not say 0/0 hands an installed binary, dump or Lisp file to
# whichever local account holds that uid, and one writable by group or others
# hands it to them (a symlink's own mode means nothing).  No member may be
# g+w either, as GNU's make-dist strips it (--mode=go-w), and the .deb's
# preinst refuses a g+w tree.  $1 names the artifact in the message; $2 is the
# tar stream ("-" for stdin).
audit_root_ownership() {
  local artifact="$1" archive="$2" listing offending
  # Tested here, not left to set -e: tar's own message may not name the
  # artifact (the .deb's stream comes on stdin).
  if ! listing="$(tar --list --verbose --numeric-owner --file "$archive")"; then
    echo "$artifact: tar could not list the archive" >&2
    return 1
  fi
  if [[ -z "$listing" ]]; then
    echo "$artifact: tar listed no members" >&2
    return 1
  fi
  # GNU tar lists each member as "MODE UID/GID SIZE DATE TIME NAME"; the mode
  # string's 6th and 9th characters are the group and other write bits.  A
  # line whose fields say anything else -- including one this does not parse
  # -- fails the audit.
  offending="$(awk '$2 != "0/0" || (substr($1, 1, 1) != "l" &&
    (substr($1, 6, 1) == "w" || substr($1, 9, 1) == "w"))' <<<"$listing")"
  if [[ -n "$offending" ]]; then
    echo "$artifact: $(grep -c '' <<<"$offending") of $(grep -c '' <<<"$listing") members are not owned by 0/0 (root:root) or are writable by group or others:" >&2
    head -n 20 <<<"$offending" >&2
    return 1
  fi
}

test_tar() {
  local artifact root package_root binary runtime_root
  artifact="$(find_one tarball "neomacs-${tar_version}-${target_triple}.tar.gz")"
  audit_root_ownership "$artifact" "$artifact"
  root="$work_dir/tar"
  mkdir -p "$root"
  tar -C "$root" -xzf "$artifact"
  package_root="$(find "$root" -mindepth 1 -maxdepth 1 -type d -print -quit)"
  test -n "$package_root"

  if [[ -x "$package_root/bin/neomacs" ]]; then
    binary="$package_root/bin/neomacs"
    runtime_root="$package_root/share/neomacs"
  else
    binary="$package_root/neomacs"
    runtime_root="$package_root"
  fi
  smoke_binary "$binary" "$runtime_root"
  if [[ -x "$package_root/bin/neomacs" ]]; then
    audit_archlib "$package_root" "$binary" "$runtime_root"
  fi
  audit_desktop_identity "$package_root"
  audit_tree "$package_root"
}

test_appimage() {
  local artifact root extracted
  artifact="$(find_one AppImage "neomacs-*-${target_triple}.AppImage")"
  audit_elf "$artifact"
  root="$work_dir/appimage"
  mkdir -p "$root"
  (
    cd "$root"
    "$artifact" --appimage-extract >/dev/null
  )
  extracted="$root/squashfs-root"
  test -x "$extracted/AppRun"
  audit_archlib "$extracted/usr" "$extracted/usr/bin/neomacs" \
    "$extracted/usr/share/neomacs"
  audit_desktop_identity "$extracted/usr"
  audit_tree "$extracted"
  echo "smoke-testing $artifact"
  env -u NEOMACS_RUNTIME_ROOT \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    TMPDIR="$work_dir" \
    timeout 30s "$artifact" --batch --eval "(kill-emacs 0)"
}

# expect_preinst ARTIFACT PREINST ACTION ROOT STATUS COUNT WHAT [LINE...]: run
# PREINST with ACTION for an installation into ROOT.  It must exit STATUS; if
# COUNT is not empty, it must refuse exactly COUNT paths; and each LINE must be
# among the paths it lists.  WHAT describes ROOT in messages.
expect_preinst() {
  local artifact="$1" preinst="$2" action="$3" root="$4" want="$5" count="$6"
  local what="$7" status=0 output line
  shift 7
  output="$(DPKG_ROOT="$root" sh "$preinst" "$action" 0.0.20 2>&1)" || status=$?
  if ((status != want)); then
    echo "$artifact: preinst $action exited $status, not $want, over $what:" >&2
    printf '%s\n' "$output" >&2
    return 1
  fi
  if [[ -n "$count" ]] &&
    ! grep -Fq -- "refusing to unpack: $count path(s)" <<<"$output"; then
    echo "$artifact: preinst did not refuse exactly $count path(s) over $what:" >&2
    printf '%s\n' "$output" >&2
    return 1
  fi
  for line in "$@"; do
    if ! grep -Fxq -- "  $line" <<<"$output"; then
      echo "$artifact: preinst refused $what, but did not list $line:" >&2
      printf '%s\n' "$output" >&2
      return 1
    fi
  done
}

# The maintainer scripts parse, and the preinst refuses what it must and
# nothing else.  dpkg does not run them here, since that would install the
# package on the runner, and the preinst is not run against the runner's own
# /usr either: that would judge the runner's image, not the package (GitHub's
# Ubuntu images make /usr/share world-writable, which the preinst refuses).
# It runs over fixture trees instead.
#
# Over a world-writable tree the preinst refuses whoever owns it, so that
# holds on any runner.  On a runner that is not root, though, a fixture's
# owner alone gets it refused, which says nothing about the other rules.  So
# each rule is also run on its own, through a copy of the preinst in which the
# runner's uid stands for root (on a root runner that changes nothing) and the
# mount table is a file written here; each refusal must list that one path.
# A second copy, in which root is another uid, must refuse the tree for its
# owner.
audit_maintainer_scripts() {
  local artifact="$1" control="$work_dir/deb-control" hostile="$work_dir/deb-hostile"
  dpkg-deb --control "$artifact" "$control"
  test -x "$control/preinst"
  local script
  for script in preinst postinst postrm; do
    sh -n "$control/$script"
  done
  mkdir -p "$hostile/usr/share/neomacs"
  chmod 0757 "$hostile/usr/share/neomacs"
  if DPKG_ROOT="$hostile" sh "$control/preinst" upgrade 0.0.20 2>/dev/null; then
    echo "$artifact: preinst accepted a world-writable /usr/share/neomacs" >&2
    return 1
  fi

  local uid other as_root="$work_dir/preinst-as-runner"
  local as_other="$work_dir/preinst-as-other" mountinfo="$work_dir/mountinfo"
  local fixture="$work_dir/deb-fixture" elsewhere="$work_dir/deb-elsewhere"
  local tree decoy
  # The fixture paths go into sed replacements and a mount table, which
  # escapes blanks and backslashes.
  if [[ ! "$work_dir" =~ ^[A-Za-z0-9._/+-]+$ ]]; then
    echo "$artifact: the preinst fixtures need a work directory named with [A-Za-z0-9._/+-]" >&2
    return 1
  fi
  uid="$(id -u)"
  other=$((uid + 1))
  sed -e "s/-uid 0\([^0-9]\)/-uid $uid\1/g" -e "s/-uid 0\$/-uid $uid/" \
    -e "s|/proc/self/mountinfo|$mountinfo|g" "$control/preinst" >"$as_root"
  sed -e "s/-uid 0\([^0-9]\)/-uid $other\1/g" -e "s/-uid 0\$/-uid $other/" \
    -e "s|/proc/self/mountinfo|$mountinfo|g" "$control/preinst" >"$as_other"
  if { ((uid != 0)) && grep -Eq -- '-uid 0([^0-9]|$)' "$as_root"; } ||
    ! grep -q -- "-uid $uid" "$as_root" || ! grep -q -- "-uid $other" "$as_other" ||
    grep -q /proc/self/mountinfo "$as_root" || ! grep -qF -- "$mountinfo" "$as_root"; then
    echo "$artifact: could not point a copy of the preinst at uid $uid and $mountinfo" >&2
    return 1
  fi
  # A directory the package has below lisp/: once lisp/ is a symlink, a decoy
  # of that name in the link's target must not be reached.
  decoy="$(awk -F"'" '/^own_dirs=/ { n = split($2, dirs, " ")
    for (i = 1; i <= n; i++) if (dirs[i] ~ /^\/usr\/share\/neomacs\/lisp\/[^\/]+$/) {
      print dirs[i]; exit } }' "$control/preinst")"
  if [[ -z "$decoy" ]]; then
    echo "$artifact: the preinst names no directory below /usr/share/neomacs/lisp" >&2
    return 1
  fi

  tree="$fixture/usr/share/neomacs"
  mkdir -p "$tree/lisp" "$elsewhere"
  : >"$tree/lisp/subr.el"
  : >"$mountinfo"
  ln -s lisp "$tree/current"
  ln -s . "$elsewhere/${decoy##*/}"
  chmod 0755 "$fixture" "$fixture/usr" "$fixture/usr/share" "$tree" "$tree/lisp" \
    "$elsewhere"
  chmod 0644 "$tree/lisp/subr.el"

  local action
  for action in install upgrade; do
    expect_preinst "$artifact" "$as_root" "$action" "$fixture" 0 "" \
      "a healthy tree with an unrelated symlink"
  done
  # dpkg runs `preinst install' too, e.g. after `dpkg -r' of a release that
  # left its directories behind: both actions must refuse.
  chmod 0757 "$tree"
  for action in install upgrade; do
    expect_preinst "$artifact" "$as_root" "$action" "$fixture" 1 1 \
      "a world-writable tree" "$tree"
  done
  chmod 0755 "$tree"
  chmod 0664 "$tree/lisp/subr.el"
  expect_preinst "$artifact" "$as_root" upgrade "$fixture" 1 1 \
    "a group-writable file" "$tree/lisp/subr.el"
  chmod 0644 "$tree/lisp/subr.el"
  chmod 0775 "$fixture/usr/share"
  expect_preinst "$artifact" "$as_root" upgrade "$fixture" 1 1 \
    "a group-writable shared directory" "$fixture/usr/share"
  chmod 0757 "$fixture/usr/share"
  expect_preinst "$artifact" "$as_root" upgrade "$fixture" 1 1 \
    "a world-writable shared directory" "$fixture/usr/share"
  chmod 0755 "$fixture/usr/share"
  expect_preinst "$artifact" "$as_other" upgrade "$fixture" 1 "" \
    "a tree root does not own" "$tree" "$fixture/usr/share"
  # A trailing slash, as with DPKG_ROOT=/, must not leave a // in what the
  # preinst compares with the mount table or lists.
  printf '36 35 0:50 / %s rw - fuse fixture rw\n' "$tree/lisp" >"$mountinfo"
  expect_preinst "$artifact" "$as_root" upgrade "$fixture/" 1 1 \
    "a file system mounted in the tree" "$tree/lisp (a mount point)"
  : >"$mountinfo"
  mv "$tree/lisp" "$tree/lisp.moved"
  ln -s "$elsewhere" "$tree/lisp"
  expect_preinst "$artifact" "$as_root" upgrade "$fixture/" 1 1 \
    "a symlink where the package has a directory" \
    "$tree/lisp (a symlink where the package has a directory)"
}

test_deb() {
  local artifact root
  artifact="$(find_one Debian-package "neomacs_*_${deb_arch}.deb")"
  dpkg-deb --fsys-tarfile "$artifact" | audit_root_ownership "$artifact" -
  audit_maintainer_scripts "$artifact"
  root="$work_dir/deb"
  mkdir -p "$root"
  dpkg-deb --extract "$artifact" "$root"
  smoke_binary "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_archlib "$root/usr" "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_desktop_identity "$root/usr"
  audit_tree "$root"
}

test_rpm() {
  local artifact root
  # The glob tolerates the build host's dist tag: the RPM is built inside the
  # target distro (el9, see `build-linux-rpm` in .github/workflows/release.yml),
  # so `%{?dist}` makes it neomacs-<version>-1.el9.<arch>.rpm rather than
  # neomacs-<version>-1.<arch>.rpm.
  artifact="$(find_one RPM-package "neomacs-*-1.*${rpm_arch}.rpm")"

  # Refuse a version-stamped ncurses/tinfo requirement (issue #388).  That is
  # what a package built on Debian-family hosts records --
  # libtinfo.so.6(NCURSES6_TINFO_5.0.19991023)(64bit) -- and no Fedora provides
  # that version, so the package installs nowhere in the RPM family.  A build on
  # the target distro records the empty version instead:
  # libtinfo.so.6()(64bit).  This check lives here rather than in the workflow
  # so a local run exercises it too; an earlier version of it matched the
  # library NAME and rejected the correct requirement, which only CI could have
  # caught.
  if rpm -qp --requires "$artifact" | grep -qE 'NCURSES'; then
    echo "$artifact requires a versioned ncurses/tinfo symbol:" >&2
    rpm -qp --requires "$artifact" | grep -E 'NCURSES' >&2
    echo "no Fedora provides that symbol version; build the RPM on the target distro" >&2
    return 1
  fi
  root="$work_dir/rpm"
  mkdir -p "$root"
  rpm2cpio "$artifact" | (
    cd "$root"
    cpio --extract --make-directories --quiet
  )
  smoke_binary "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_archlib "$root/usr" "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_desktop_identity "$root/usr"
  audit_tree "$root"
}

IFS=',' read -r -a requested_formats <<<"$formats"
for format in "${requested_formats[@]}"; do
  case "$format" in
    tar) test_tar ;;
    appimage) test_appimage ;;
    deb) test_deb ;;
    rpm) test_rpm ;;
    *)
      echo "unsupported artifact format: $format" >&2
      exit 1
      ;;
  esac
done

echo "verified $formats artifacts for $target_triple at GLIBC_$glibc_baseline or older"
