#!/bin/sh
# NEO Emacs (neomacs) installer.
#
#   curl -fsSL https://neomacs.org/install.sh | bash
#
# Downloads a release tarball from GitHub Releases and installs it without
# root privileges:
#
#   Linux   ~/.local/share/neomacs/versions/<ver>/{bin, libexec, share/neomacs}
#           ~/.local/share/neomacs/current -> versions/<ver>
#           ~/.local/bin/neomacs -> ../share/neomacs/current/bin/neomacs
#   macOS   ~/Applications/neomacs.app (full bundle, vendored GStreamer)
#           ~/.local/bin/neomacs -> .../neomacs.app/Contents/MacOS/neomacs
#
# The version directory mirrors the release tarball's own bin/ + share/
# layout, which every shipped resolver generation locates relative to the
# executable -- including the v0.0.15 binaries, so installing old --tag
# releases works without NEOMACS_RUNTIME_ROOT.
#
# Upgrades flip the `current' symlink, so they never disturb a running
# instance, and the previous version is kept for rollback (re-run with
# --tag). The layout is what the binary's runtime-root resolver
# auto-detects, mirroring GNU Emacs' walk-up from the executable
# (src/emacs.c init_cmdargs + src/lread.c load_path_default): no
# environment variables are needed.
#
# This script is intentionally POSIX sh (dash-compatible) and asks no
# questions, so it is safe to pipe into a shell.

set -u

repo="eval-exec/neomacs"
tag=""
prefix=""
app_dir=""
bin_dir=""
keep_download="no"
skip_smoke="no"

usage() {
  cat <<'USAGE'
Usage: install.sh [options]

Install NEO Emacs from GitHub Releases into your user account (no root).

Options:
  --prefix DIR    Installation prefix (Linux). The versioned tree goes to
                  DIR/share/neomacs and the `neomacs` command to DIR/bin.
                  Default: ~/.local
  --app-dir DIR   Where to place neomacs.app (macOS). Default: ~/Applications
  --bin-dir DIR   Where to place the `neomacs` command. Default: ~/.local/bin
  --tag TAG       Release to install, e.g. v0.0.15 (also rolls back to a
                  kept version). Default: latest release
  --repo SPEC     GitHub OWNER/NAME to install from.
                  Default: eval-exec/neomacs (env NEOMACS_GH_REPO)
  --keep-download Keep the downloaded archive (useful for debugging)
  --skip-smoke    Do not run the installed binary as a post-install check
  -h, --help      Show this help

Examples:
  curl -fsSL https://neomacs.org/install.sh | bash
  install.sh --tag v0.0.15        # install (or roll back to) a specific release
  install.sh --prefix /opt/neomacs

Linux layout (previous version kept for rollback):
  ~/.local/bin/neomacs -> ../share/neomacs/current/bin/neomacs
  ~/.local/share/neomacs/current -> versions/0.0.16
  ~/.local/share/neomacs/versions/0.0.16/bin/{neomacs,neomacsclient}
  ~/.local/share/neomacs/versions/0.0.16/libexec/neomacs/<ver>/<triple>/
  ~/.local/share/neomacs/versions/0.0.16/share/neomacs/{lisp,etc,...}

Linux also ships .deb/.rpm packages and an AppImage; macOS ships a .dmg.
See all release assets at:
https://github.com/eval-exec/neomacs/releases/latest
USAGE
}

die() {
  echo "install.sh: error: $*" >&2
  exit 1
}

say() {
  printf '%s\n' "$*"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix)
      [ "$#" -ge 2 ] || die "$1 requires a value"
      prefix=$2
      shift 2
      ;;
    --app-dir)
      [ "$#" -ge 2 ] || die "$1 requires a value"
      app_dir=$2
      shift 2
      ;;
    --bin-dir)
      [ "$#" -ge 2 ] || die "$1 requires a value"
      bin_dir=$2
      shift 2
      ;;
    --tag)
      [ "$#" -ge 2 ] || die "$1 requires a value"
      tag=$2
      shift 2
      ;;
    --repo)
      [ "$#" -ge 2 ] || die "$1 requires a value"
      repo=$2
      shift 2
      ;;
    --keep-download)
      keep_download=yes
      shift
      ;;
    --skip-smoke)
      skip_smoke=yes
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (see --help)"
      ;;
  esac
done

[ -n "${NEOMACS_GH_REPO:-}" ] && repo=$NEOMACS_GH_REPO
case "$repo" in
  */*) ;;
  *) die "repo must be OWNER/NAME, got: $repo" ;;
esac

# ---------------------------------------------------------------- fetchers --

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1"; }
  download() { curl -fsSL -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO- "$1"; }
  download() { wget -q -O "$2" "$1"; }
else
  die "neither curl nor wget is available"
fi

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    return 1
  fi
}

# ------------------------------------------------------------- platform ----

os=$(uname -s)
case "$os" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  MINGW*|MSYS*|CYGWIN*)
    die "this installer does not support Windows; download the -user-setup.exe from https://github.com/$repo/releases/latest"
    ;;
  *)
    die "unsupported operating system: $os"
    ;;
esac

arch=$(uname -m)
case "$os:$arch" in
  linux:x86_64) triple=x86_64-unknown-linux-gnu ;;
  linux:aarch64|linux:arm64) triple=aarch64-unknown-linux-gnu ;;
  darwin:arm64|darwin:aarch64) triple=aarch64-apple-darwin ;;
  darwin:x86_64)
    die "no Intel Mac (x86_64) builds are published yet; see https://github.com/$repo/releases"
    ;;
  *)
    die "unsupported $os architecture: $arch"
    ;;
esac

# --------------------------------------------------------------- release ---

if [ -z "$tag" ]; then
  say "-> resolving the latest release of $repo"
  tag=$(fetch "https://api.github.com/repos/$repo/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$tag" ] || die "could not resolve the latest release (API rate limit?); pass --tag vX.Y.Z"
fi
case "$tag" in
  v*) version=${tag#v} ;;
  *) version=$tag; tag="v$tag" ;;
esac

asset="neomacs-${version}-${triple}.tar.gz"
# The download base is overridable so CI can exercise this script against a
# locally staged release tree (file:// URLs work with both curl and wget)
# before anything is published.
release_base=${NEOMACS_DOWNLOAD_BASE:-"https://github.com/$repo/releases/download"}
asset_url="$release_base/$tag/$asset"
say "-> installing $repo $tag ($triple)"

# ------------------------------------------------------------- download ----

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/neomacs-install.XXXXXX") \
  || die "could not create a temporary directory"
cleanup() {
  if [ "$keep_download" = yes ]; then
    say "download kept in $tmp_dir"
  else
    rm -rf "$tmp_dir"
  fi
}
trap cleanup EXIT INT TERM

archive="$tmp_dir/$asset"
say "-> downloading $asset"
download "$asset_url" "$archive" || die "download failed: $asset_url"
[ -s "$archive" ] || die "download produced an empty file: $asset_url"

# Verify against the release checksum manifest. A published manifest that
# does not list this asset is an error, not a skip; only releases that
# predate SHA256SUMS entirely downgrade to a notice. The name comparison
# tolerates the "./<asset>" form `sha256sum ./*` produces.
sums=$(fetch "$release_base/$tag/SHA256SUMS" || true)
if [ -n "$sums" ]; then
  expected=$(printf '%s\n' "$sums" \
    | awk -v f="$asset" '{ name = $2; sub(/^\.\//, "", name); if (name == f) print $1 }')
  if [ -z "$expected" ]; then
    die "release $tag publishes SHA256SUMS but it does not list $asset"
  fi
  actual=$(sha256_of "$archive") \
    || die "neither sha256sum nor shasum is available to verify the download"
  [ "$actual" = "$expected" ] \
    || die "checksum mismatch for $asset (expected $expected, got $actual)"
  say "-> checksum verified"
else
  say "note: release $tag publishes no SHA256SUMS; skipping checksum verification"
fi

# -------------------------------------------------------------- install ----

extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"
tar -C "$extract_dir" -xzf "$archive" || die "could not extract $asset"
pkg_dir="$extract_dir/neomacs-${version}-${triple}"
[ -d "$pkg_dir" ] || die "archive did not contain neomacs-${version}-${triple}/"

if [ "$os" = linux ]; then
  # Release tarballs ship either the canonical layout (bin/ + share/neomacs/,
  # produced by scripts/package-release.sh, runnable in place) or the older
  # flat layout (executables beside lisp/ and etc/). Handle both so --tag can
  # reach every published release.
  if [ -x "$pkg_dir/bin/neomacs" ] && [ -d "$pkg_dir/share/neomacs" ]; then
    src_bin=$pkg_dir/bin
    src_data=$pkg_dir/share/neomacs
    # From 0.0.16 the dump lives in the archive's archlib -- GNU's
    # ${libexecdir}/emacs/${version}/${configuration} (configure.ac:290),
    # here libexec/neomacs/<ver>/<triple> -- which the binary probes for
    # (path_exec.rs) and loads from (load.rs).  Older tarballs put it beside
    # the executable, which is still the binary's first lookup rung, so both
    # install unchanged and --tag can reach every published release.
    src_archlib=""
    if [ -d "$pkg_dir/libexec" ]; then
      src_archlib=$pkg_dir/libexec
    elif [ ! -f "$src_bin/neomacs.pdump" ]; then
      die "package has neither libexec/ nor bin/neomacs.pdump"
    fi
    [ -d "$src_data/lisp" ] || die "package is missing the lisp/ runtime tree"
    [ -d "$src_data/etc" ] || die "package is missing the etc/ runtime tree"
  else
    src_bin=$pkg_dir
    src_archlib=""
    [ -x "$pkg_dir/neomacs" ] || die "package is missing the neomacs binary"
    [ -f "$pkg_dir/neomacs.pdump" ] || die "package is missing neomacs.pdump"
    [ -d "$pkg_dir/lisp" ] || die "package is missing the lisp/ runtime tree"
    [ -d "$pkg_dir/etc" ] || die "package is missing the etc/ runtime tree"
  fi

  [ -z "$prefix" ] && prefix=${HOME}/.local
  [ -z "$bin_dir" ] && bin_dir=$prefix/bin
  root=$prefix/share/neomacs
  ver_dir=$root/versions/$version
  staged=$root/versions/.staged.$version

  # A crashed previous run may have left staging behind; start clean so no
  # stale file can ride into the installed tree.
  rm -rf "$staged"
  mkdir -p "$staged/bin" "$staged/share/neomacs" "$bin_dir" \
    || die "cannot create directories under $prefix (writable?)"

  if [ -n "${src_data:-}" ]; then
    # Canonical layout: the runtime tree is already gathered; take it as is
    # and add the top-level docs.
    cp -a "$src_data/." "$staged/share/neomacs/" \
      || die "could not stage the runtime tree"
    for doc in COPYING VERSION README.md; do
      if [ -f "$pkg_dir/$doc" ]; then
        cp -a "$pkg_dir/$doc" "$staged/$doc" || die "could not stage $doc"
      fi
    done
  else
    # Flat layout: everything except the executables and the dump is runtime
    # data (lisp, etc, COPYING, and anything else the release carries).
    find "$pkg_dir" -mindepth 1 -maxdepth 1 \
      ! -name neomacs ! -name neomacsclient ! -name 'neomacs.pdump' \
      ! -name libexec \
      ! -name 'neomacs-*.pdump' -exec cp -a {} "$staged/share/neomacs/" \; \
      || die "could not stage the runtime tree"
  fi

  for exe in neomacs neomacsclient; do
    if [ -f "$src_bin/$exe" ]; then
      install -m 0755 "$src_bin/$exe" "$staged/bin/$exe" \
        || die "could not stage $exe"
    fi
  done
  # The version directory is the install prefix, so the archive's libexec
  # tree lands beside bin/ and share/ unchanged: that relative shape is what
  # the PATH_EXEC probe walks up to from bin/neomacs.
  if [ -n "$src_archlib" ]; then
    cp -a "$src_archlib" "$staged/libexec" \
      || die "could not stage the libexec archlib tree"
  else
    install -m 0644 "$src_bin/neomacs.pdump" "$staged/bin/neomacs.pdump" \
      || die "could not stage neomacs.pdump"
  fi

  # Validate the staged tree before anything installed changes.
  [ -x "$staged/bin/neomacs" ] || die "staged tree lost the neomacs binary"
  if [ -n "$src_archlib" ]; then
    staged_dump=$(find "$staged/libexec" -name 'neomacs*.pdump' -type f 2>/dev/null | head -n 1)
    [ -n "$staged_dump" ] || die "staged tree has no dump image under libexec/"
  else
    [ -f "$staged/bin/neomacs.pdump" ] || die "staged tree lost neomacs.pdump"
  fi
  [ -d "$staged/share/neomacs/lisp" ] \
    || die "staged tree lost the lisp/ runtime directory"
  [ -d "$staged/share/neomacs/etc" ] \
    || die "staged tree lost the etc/ runtime directory"

  # Pre-flight the dynamic-loader requirements of the release binary (built
  # against distro fontconfig/glib/ncurses, plus GStreamer in the full Linux
  # product). Missing libraries do
  # NOT block the install -- the tree is complete and starts working the
  # moment the packages are installed, with no re-run needed -- but say so
  # up front, with the exact fix, instead of letting a raw loader error be
  # the user's first launch experience. Skipped where ldd cannot run.
  missing_libs=$(ldd "$staged/bin/neomacs" 2>/dev/null | sed -n 's/^[[:space:]]*\([^[:space:]]*\).*not found.*/\1/p' | sort -u)
  if [ -n "$missing_libs" ]; then
    say ""
    say "NOTE: this system is missing runtime libraries neomacs needs:"
    for lib in $missing_libs; do
      say "  $lib"
    done
    say ""
    say "neomacs is being installed anyway, but it will not start until"
    say "they are available. Install them with, for example:"
    if command -v apt-get >/dev/null 2>&1; then
      say "  apt install libglib2.0-0 libfontconfig1 libtinfo6 libgstreamer1.0-0 libgstreamer-plugins-base1.0-0"
    elif command -v dnf >/dev/null 2>&1; then
      say "  dnf install glib2 fontconfig ncurses-libs gstreamer1 gstreamer1-plugins-base"
    elif command -v pacman >/dev/null 2>&1; then
      say "  pacman -S glib2 fontconfig ncurses gstreamer gst-plugins-base-libs"
    elif command -v zypper >/dev/null 2>&1; then
      say "  zypper install glib2 fontconfig libncurses6 gstreamer gstreamer-plugins-base"
    else
      say "  your distribution's GLib, fontconfig, ncurses, GStreamer, and GStreamer base-plugin runtime packages"
    fi
    say "No need to re-run the installer afterwards."
    say ""
  fi

  # Swap the version directory through a same-volume rename, then flip
  # `current'. Installing over an tag that is already active first moves the
  # existing directory aside, so a failure mid-swap leaves the old tree
  # recoverable on disk rather than deleted, and a running instance is never
  # pointing at a half-replaced tree (the smoke test below would also catch
  # one).
  old_current=$(cat "$root/current-version" 2>/dev/null || true)
  aside="$ver_dir.old.$$"
  if [ -d "$ver_dir" ]; then
    mv "$ver_dir" "$aside" || die "could not move the existing version aside"
  fi
  if ! mv "$staged" "$ver_dir"; then
    if [ -d "$aside" ]; then
      mv "$aside" "$ver_dir" || true
    fi
    die "could not move the new version into place"
  fi
  rm -rf "$aside"
  ln -sfn "versions/$version" "$root/current" \
    || die "could not update the current symlink"
  printf '%s\n' "$version" > "$root/current-version" \
    || die "could not record the installed version"
  if [ -n "$old_current" ] && [ "$old_current" != "$version" ]; then
    printf '%s\n' "$old_current" > "$root/previous-version"
  fi

  # The command symlink is relative, so the prefix stays relocatable. The
  # runtime-root resolver follows it (canonicalize) into the version tree.
  ln -sfn "../share/neomacs/current/bin/neomacs" "$bin_dir/neomacs" \
    || die "could not link neomacs into $bin_dir"
  [ -f "$ver_dir/bin/neomacsclient" ] \
    && ln -sfn "../share/neomacs/current/bin/neomacsclient" "$bin_dir/neomacsclient"

  # Keep the current version and the one it replaced (the rollback target);
  # prune older ones -- each version directory is a few hundred MB.
  keep_prev=$(cat "$root/previous-version" 2>/dev/null || true)
  for d in "$root"/versions/*; do
    [ -d "$d" ] || continue
    name=${d##*/}
    case "$name" in
      [0-9]*) ;;
      *) continue ;;
    esac
    [ "$name" = "$version" ] && continue
    [ -n "$keep_prev" ] && [ "$name" = "$keep_prev" ] && continue
    say "-> pruning old version $name"
    rm -rf "$d"
  done

  installed_bin=$bin_dir/neomacs
else
  # macOS: the tarball ships the complete neomacs.app bundle (binary, dump,
  # lisp/etc trees, vendored GStreamer frameworks). Install the app, then
  # expose the CLI through symlinks; the runtime resolver follows the
  # canonical path into Contents/Resources/neomacs automatically.
  [ -z "$app_dir" ] && app_dir=${HOME}/Applications
  [ -z "$bin_dir" ] && bin_dir=${HOME}/.local/bin
  app="neomacs.app"
  [ -d "$pkg_dir/$app" ] || die "package did not contain $app"

  mkdir -p "$app_dir" "$bin_dir" \
    || die "cannot create $app_dir or $bin_dir (writable?)"

  # Extract landed on the same volume as $app_dir only if TMPDIR is; move
  # through a staging dir inside $app_dir so the final move is a rename.
  staging="$app_dir/.neomacs-install.staged"
  rm -rf "$staging"
  mkdir -p "$staging"
  mv "$pkg_dir/$app" "$staging/$app" || die "could not stage $app"
  if [ -d "$app_dir/$app" ]; then
    mv "$app_dir/$app" "$app_dir/neomacs.app.old" \
      || die "could not move the old $app aside"
  fi
  mv "$staging/$app" "$app_dir/$app" || die "could not move $app into place"
  rm -rf "$app_dir/neomacs.app.old" "$staging"

  for exe in neomacs neomacsclient; do
    target="$app_dir/$app/Contents/MacOS/$exe"
    if [ -f "$target" ]; then
      ln -sfn "$target" "$bin_dir/$exe" || die "could not link $exe into $bin_dir"
    fi
  done

  installed_bin=$bin_dir/neomacs
fi

# ---------------------------------------------------------------- smoke ----

# Same post-install check the release pipeline applies to every artifact:
# batch-start through the installed path, with no environment variables, so
# the runtime-root resolution is exercised exactly as a user launch would.
# Skipped only when the pre-flight already established WHY the binary cannot
# start (missing system libraries); any other failure is a real defect.
if [ "$skip_smoke" = no ] && [ -z "${missing_libs:-}" ]; then
  say "-> verifying the installed binary starts"
  # Subshell + unset instead of `env -u`: strictly POSIX.
  (
    unset NEOMACS_RUNTIME_ROOT
    exec "$installed_bin" --batch --eval '(kill-emacs 0)'
  ) || die "installed neomacs failed to start. If the error names a missing libtinfo shared library, install your distribution's ncurses runtime package and re-run; otherwise report it at https://github.com/$repo/issues (files are in place; --skip-smoke bypasses this check)"
fi

# ----------------------------------------------------------------- PATH ----

case ":${PATH}:" in
  *":$(dirname "$installed_bin"):"*)
    ;;
  *)
    bin_parent=$(dirname "$installed_bin")
    say ""
    say "NOTE: $bin_parent is not in your PATH. Add"
    say "  export PATH=\"$bin_parent:\$PATH\""
    say "to your shell profile (~/.profile, ~/.bashrc, or ~/.zshrc)."
    ;;
esac

say ""
say "NEO Emacs $version installed:"
say "  $installed_bin"
say "Run 'neomacs' to start (or 'neomacs -nw' for terminal mode)."
if [ -n "${missing_libs:-}" ]; then
  say "Reminder: install the system libraries listed above first -- neomacs"
  say "will not start until they are present (no re-install needed)."
fi
if [ "$os" = linux ]; then
  say "Rollback to a kept version: install.sh --tag vX.Y.Z"
fi
