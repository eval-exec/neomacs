#!/bin/sh
# NEO Emacs (neomacs) installer.
#
#   curl -fsSL https://neomacs.org/install.sh | bash
#
# Downloads a release tarball from GitHub Releases and installs it without
# root privileges:
#
#   Linux   ~/.local/share/neomacs/versions/<ver>/{bin,lisp,etc,...}
#           ~/.local/share/neomacs/current -> versions/<ver>
#           ~/.local/bin/neomacs -> ../share/neomacs/current/bin/neomacs
#   macOS   ~/Applications/neomacs.app (full bundle, vendored GStreamer)
#           ~/.local/bin/neomacs -> .../neomacs.app/Contents/MacOS/neomacs
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
  ~/.local/share/neomacs/current -> versions/0.0.15
  ~/.local/share/neomacs/versions/0.0.15/{bin/neomacs,bin/neomacs.pdump,lisp,etc,...}

Linux also ships AppImage/.deb/.rpm release assets, and macOS a .dmg, for
those who prefer system packages:
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

# Verify against the release checksum manifest when the release provides one
# (releases before SHA256SUMS existed only warn).
sums=$(fetch "$release_base/$tag/SHA256SUMS" || true)
if [ -n "$sums" ]; then
  expected=$(printf '%s\n' "$sums" | awk -v f="$asset" '$2 == f {print $1}')
  if [ -n "$expected" ]; then
    actual=$(sha256_of "$archive") \
      || die "neither sha256sum nor shasum is available to verify the download"
    [ "$actual" = "$expected" ] \
      || die "checksum mismatch for $asset (expected $expected, got $actual)"
    say "-> checksum verified"
  fi
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
    [ -f "$src_bin/neomacs.pdump" ] || die "package is missing neomacs.pdump"
    [ -d "$src_data/lisp" ] && [ -d "$src_data/etc" ] \
      || die "package is missing the lisp/ or etc/ runtime tree"
  else
    src_bin=$pkg_dir
    [ -x "$pkg_dir/neomacs" ] || die "package is missing the neomacs binary"
    [ -f "$pkg_dir/neomacs.pdump" ] || die "package is missing neomacs.pdump"
    [ -d "$pkg_dir/lisp" ] || die "package is missing the lisp/ runtime tree"
  fi

  [ -z "$prefix" ] && prefix=${HOME}/.local
  [ -z "$bin_dir" ] && bin_dir=$prefix/bin
  root=$prefix/share/neomacs
  ver_dir=$root/versions/$version
  staged=$root/versions/.staged.$version

  mkdir -p "$staged/bin" "$bin_dir" \
    || die "cannot create directories under $prefix (writable?)"

  if [ -n "${src_data:-}" ]; then
    # Canonical layout: runtime data is already gathered under share/neomacs
    # (lisp, etc, leim, and anything added later); add the top-level docs.
    cp -a "$src_data/." "$staged/" || die "could not stage the runtime tree"
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
      ! -name 'neomacs-*.pdump' -exec cp -a {} "$staged/" \; \
      || die "could not stage the runtime tree"
  fi

  # The loader resolves neomacs.pdump next to the canonical executable, so
  # the dump must live in bin/ with the binary (as in the shipped .deb).
  for exe in neomacs neomacsclient; do
    if [ -f "$src_bin/$exe" ]; then
      install -m 0755 "$src_bin/$exe" "$staged/bin/$exe" \
        || die "could not stage $exe"
    fi
  done
  install -m 0644 "$src_bin/neomacs.pdump" "$staged/bin/neomacs.pdump" \
    || die "could not stage neomacs.pdump"

  # Validate the staged tree before anything installed changes.
  [ -x "$staged/bin/neomacs" ] || die "staged tree lost the neomacs binary"
  [ -f "$staged/bin/neomacs.pdump" ] || die "staged tree lost neomacs.pdump"
  [ -d "$staged/lisp" ] && [ -d "$staged/etc" ] \
    || die "staged tree lost the lisp/ or etc/ runtime directories"

  # Swap the version directory in whole (a rename), then flip `current' --
  # a running instance keeps its old tree, and a crash between the two
  # leaves the previous release active.
  old_current=$(cat "$root/current-version" 2>/dev/null || true)
  rm -rf "$ver_dir"
  mv "$staged" "$ver_dir" || die "could not move the new version into place"
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
if [ "$skip_smoke" = no ]; then
  say "-> verifying the installed binary starts"
  env -u NEOMACS_RUNTIME_ROOT "$installed_bin" --batch --eval '(kill-emacs 0)' \
    || die "installed neomacs failed to start; report this at https://github.com/$repo/issues (files are in place; see --skip-smoke to bypass)"
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
if [ "$os" = linux ]; then
  say "Rollback to a kept version: install.sh --tag vX.Y.Z"
fi
