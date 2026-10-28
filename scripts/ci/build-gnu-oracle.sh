#!/usr/bin/env bash
# Build the GNU Emacs parity oracle from the pinned mirror commit.
#
#     scripts/ci/build-gnu-oracle.sh <commit> <prefix> [<work-dir>]
#
# This is the ONE recipe, with one configure line per host family.
# .github/actions/setup-gnu-emacs/action.yml calls it on Linux CI,
# .github/actions/setup-gnu-emacs-macos/action.yml on macOS CI, and the NixOS
# shell `nix develop .#gnu-oracle` (nix/gnu-oracle-shell.nix) calls the same
# file locally, so a local oracle is the CI oracle by construction.  Everything
# that identifies the oracle lives here: the source commit (not a tag), the
# configure line, and the build/install commands.  Callers supply only the
# install prefix -- which `system-configuration-options' reports, and which
# scripts/ci/check-gnu-oracle.sh strips before comparing -- and optionally the
# transport repository the commit is fetched from.
#
# The configure line is load-bearing, not a preference.  On Linux a TTY-only
# build (no GTK3/X11) lacks the 107 window-system mouse bindings, and their
# long keys shift every describe-bindings block's alignment.  On macOS there is
# no X server in CI, so the characterized oracle is --without-x --without-ns;
# the questions the macOS suite asks it (long-double formatting, load suffixes,
# kqueue notification, time zones, raw-byte file names) are batch behaviors.
# On both hosts a native-comp build ships inline docstrings where a canonical
# byte-compiled build has lazy (#$ . N) references, so doc-slot-4 oracle
# records differ, and --without-compress-install keeps the installed Lisp tree
# as plain .el: GNU's default `make install' gzips it and jka-compr then prints
# "uncompressing FILE.el.gz...done" into the message stream the MELPA/TUI
# harnesses compare.
#
# On Linux the oracle also bundles the pinned Tree-sitter runtime.  GNU's
# `treesit' refuses a grammar whose ABI is newer than the runtime it was
# COMPILED against (src/treesit.c:728-755 compares the language's abi_version
# with TREE_SITTER_LANGUAGE_VERSION); Ubuntu 24.04 ships tree-sitter 0.20
# (ABI 13) while the pinned grammar revisions include ABI 15.  The runtime is
# built into $prefix, configure finds it through $prefix/lib/pkgconfig, and
# the installed executables get a RUNPATH that puts $prefix/lib first, so a CI
# cache HIT resolves it without a compiler, LD_LIBRARY_PATH, or the system's
# tree-sitter.  The macOS oracle is a batch reference and builds none.
#
# A change here -- including the Tree-sitter pin below -- is a recipe change:
# bump `cache-version` in the setup actions so CI rebuilds its cache.
#
# Environment:
#   EMACS_ORACLE_REPO     git repository to fetch <commit> from
#                         (default https://github.com/emacs-mirror/emacs.git;
#                         the local shell points it at the emacs-mirror clone)
#   EMACS_ORACLE_JOBS     make -j (default: nproc)
#   EMACS_ORACLE_WORKDIR  parent directory for the throwaway source tree
#                         (default: ${TMPDIR:-/tmp})
#   EMACS_ORACLE_KEEP_SRC 1 keeps the source tree after install
#   EMACS_ORACLE_RPATH    extra directories appended to the installed
#                         executables' RUNPATH.  On Linux $prefix/lib is
#                         ALWAYS first (the bundled Tree-sitter runtime); this
#                         value adds the rest, which NixOS needs because it
#                         has no system library path and `make install' does
#                         no patchelf pass.  This changes ELF metadata only --
#                         GNU's dump fingerprint is a compiled-in byte array,
#                         not a hash of the executable file
#                         (src/pdumper.c:5694-5701).
#
# The build writes $prefix/share/gnu-emacs-oracle/build-manifest, which
# check-gnu-oracle.sh uses to prove the installed binary was built by exactly
# this recipe.
set -euo pipefail

readonly usage="usage: $0 <commit> <prefix> [<work-dir>]"

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "$usage" >&2
    exit 2
fi

readonly commit=$1
readonly prefix=$2
readonly workdir=${3:-${EMACS_ORACLE_WORKDIR:-${TMPDIR:-/tmp}}}
readonly repo=${EMACS_ORACLE_REPO:-https://github.com/emacs-mirror/emacs.git}
# GNU/Linux has nproc; Darwin does not (and its bash 3.2 does not either).
if [[ $(uname -s) == Darwin ]]; then
    readonly jobs=${EMACS_ORACLE_JOBS:-$(sysctl -n hw.ncpu)}
else
    readonly jobs=${EMACS_ORACLE_JOBS:-$(nproc)}
fi
# Linux-profile bundled runtime pin (see the header).  The macOS profile does
# not build it.
readonly tree_sitter_version=0.26.11
readonly tree_sitter_sha256=1bab01ed21464f3272665b9c60e39ee79f68da1333e80b23f2c9356569d06971
readonly tree_sitter_url="https://github.com/tree-sitter/tree-sitter/archive/refs/tags/v$tree_sitter_version.tar.gz"

if [[ ! $commit =~ ^[0-9a-f]{40}$ ]]; then
    echo "build-gnu-oracle: commit must be a full 40-hex mirror commit, got '$commit'" >&2
    exit 2
fi
if [[ $prefix != /* ]]; then
    echo "build-gnu-oracle: prefix must be absolute, got '$prefix'" >&2
    exit 2
fi

log() { printf 'build-gnu-oracle: %s\n' "$*" >&2; }

# The configure line.  parity-reference.toml pins the commit; this array pins
# what that commit is configured with.  There is one recipe per host family,
# because a window system is not portable: the Linux oracle is a GTK3 build
# whose window-system bindings and face realization the TUI/describe-bindings
# parity depends on, while macOS has neither X11 nor NS in CI and its
# neovm-core parity questions (long double formatting, load suffixes, file
# notification, time zones, file names) are batch behaviors.  Both are recorded
# in the manifest and asserted by check-gnu-oracle.sh, and the cache key in
# each setup action hashes this file, so changing a line rebuilds the oracle.
case $(uname -s) in
    Darwin)
        profile=macos-batch
        configure_args=(
            # No window system: CI has no X server, and an NS build would
            # answer different face questions than the reference the macOS
            # suite was characterized against.
            --without-x
            --without-ns
            --with-native-compilation=no
            --without-compress-install
            # Darwin's file-notify backend (src/kqueue.c); asking for it
            # explicitly keeps configure from silently dropping notification.
            --with-file-notification=kqueue
            --with-gnutls=ifavailable
            --with-xml2=ifavailable
            --with-libgmp=ifavailable
            --with-sound=no
            --without-dbus
        )
        ;;
    *)
        profile=linux-gtk3
        configure_args=(
            # X11 + GTK3, never PGTK: configure would otherwise prefer PGTK
            # when the Wayland headers are present, which is a different
            # window system.
            --with-x-toolkit=gtk3
            --without-pgtk
            --with-tree-sitter
            # Real package-notification tests use the oracle as a private
            # D-Bus service; require the native primitives rather than letting
            # configure omit them silently.
            --with-dbus
            --with-native-compilation=no
            --without-compress-install
            --with-gnutls=ifavailable
            --with-xml2=ifavailable
            --with-libgmp=ifavailable
            --with-sound=no
            --without-gconf
            # GSettings stays ENABLED (no --without-gsettings).
            # font-get-system-font answers from the GNOME desktop settings
            # only in a GSettings build; with --without-gsettings it returns
            # nil, and the desktop-font scenarios (the GNU half and the
            # Neomacs half that compares against it) fail with "Initial
            # settings are not isolated" / "Startup ignored desktop font".
            # The CI image and the NixOS shell both provide gio >= 2.26.
            # The macOS profile needs no flag: its action installs no glib,
            # so configure omits GSettings there (and the macos-batch check
            # forbids it).
            --without-libsystemd
            --without-selinux
            --without-gpm
            --without-sqlite3
        )
        ;;
esac

mkdir -p "$workdir" "$prefix"
src=$(mktemp -d "$workdir/gnu-emacs-oracle.XXXXXX")
ts_work=
cleanup() {
    if [[ ${EMACS_ORACLE_KEEP_SRC:-0} == 1 ]]; then
        log "keeping source tree at $src"
    else
        rm -rf "$src"
    fi
    if [[ -n $ts_work ]]; then
        rm -rf "$ts_work"
    fi
}
trap cleanup EXIT

log "fetching $commit from $repo"
git -C "$src" init -q
# The commit itself, not the tag: the tag is documentation, the commit is the
# identity the parity numbers were measured against.
git -C "$src" fetch -q --depth 1 "$repo" "$commit"
git -C "$src" checkout -q FETCH_HEAD
readonly fetched=$(git -C "$src" rev-parse HEAD)
if [[ $fetched != "$commit" ]]; then
    echo "build-gnu-oracle: fetched $fetched, wanted $commit" >&2
    exit 1
fi

cd "$src"
readonly version=$(
    sed -n 's/^AC_INIT(\[GNU Emacs\], \[\([^]]*\)\].*/\1/p' configure.ac
)
if [[ -z $version ]]; then
    echo "build-gnu-oracle: cannot read the Emacs version from configure.ac" >&2
    exit 1
fi

log "autogen.sh"
./autogen.sh

# configure records CFLAGS/CPPFLAGS/LDFLAGS in `system-configuration-options'
# when they are exported (configure.ac:58-93), so an inherited -march=native or
# -flto would become part of the oracle's identity *and* change code
# generation.  CI exports none of them; be explicit rather than trusting the
# caller's environment.
unset CFLAGS CPPFLAGS LDFLAGS

if [[ $profile == linux-gtk3 ]]; then
    log "tree-sitter $tree_sitter_version into $prefix"
    ts_work=$(mktemp -d "$workdir/tree-sitter.XXXXXX")
    curl -fsSL -o "$ts_work/tree-sitter.tar.gz" "$tree_sitter_url"
    echo "$tree_sitter_sha256  $ts_work/tree-sitter.tar.gz" | sha256sum -c -
    tar xzf "$ts_work/tree-sitter.tar.gz" -C "$ts_work"
    # PREFIX belongs on the BUILD too: tree-sitter generates its .pc file
    # with the prefix in effect at build time, and a .pc pointing at
    # /usr/local would send configure to headers that are not there.
    make -C "$ts_work/tree-sitter-$tree_sitter_version" -j"$jobs" PREFIX="$prefix"
    make -C "$ts_work/tree-sitter-$tree_sitter_version" install PREFIX="$prefix"
    test "$(PKG_CONFIG_PATH="$prefix/lib/pkgconfig" \
        pkg-config --modversion tree-sitter)" = "$tree_sitter_version"
    rm -rf "$ts_work"
    ts_work=
    export PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
fi

log "configure (prefix $prefix)"
./configure --prefix="$prefix" "${configure_args[@]}"

log "make -j$jobs"
make -j"$jobs"

log "make install"
make install

# The installed Linux oracle must find the bundled Tree-sitter runtime by
# itself: CI's cached tree is restored onto a runner that builds nothing, and
# NixOS has no system loader path.  $prefix/lib goes first so the bundled
# runtime wins over any system tree-sitter; a caller-provided
# EMACS_ORACLE_RPATH (the NixOS shell's library set) is appended for the other
# dynamic libraries.  Other host families keep the caller-driven behavior.
if [[ $profile == linux-gtk3 ]]; then
    if ! command -v patchelf >/dev/null; then
        echo "build-gnu-oracle: patchelf is required to set the oracle RUNPATH" >&2
        exit 1
    fi
    readonly rpath="$prefix/lib${EMACS_ORACLE_RPATH:+:$EMACS_ORACLE_RPATH}"
elif [[ -n ${EMACS_ORACLE_RPATH:-} ]]; then
    if ! command -v patchelf >/dev/null; then
        echo "build-gnu-oracle: EMACS_ORACLE_RPATH is set but patchelf is not on PATH" >&2
        exit 1
    fi
    readonly rpath=$EMACS_ORACLE_RPATH
else
    readonly rpath=
fi

if [[ -n $rpath ]]; then
    log "setting RUNPATH on the installed executables"
    # emacs is the only installed program that links the window-system, image
    # and Tree-sitter libraries.  bin/emacs may be a symlink or a hardlink to
    # bin/emacs-VERSION; patch every non-symlink path so both names work.
    shopt -s nullglob
    for exe in "$prefix"/bin/emacs "$prefix"/bin/emacs-[0-9]*; do
        [[ -f $exe && ! -L $exe ]] || continue
        patchelf --set-rpath "$rpath" "$exe"
    done
    shopt -u nullglob
fi

readonly manifest_dir="$prefix/share/gnu-emacs-oracle"
readonly manifest="$manifest_dir/build-manifest"
mkdir -p "$manifest_dir"
{
    printf '# GNU Emacs parity oracle build manifest.\n'
    printf '# Written by scripts/ci/build-gnu-oracle.sh; read by check-gnu-oracle.sh.\n'
    printf 'commit=%s\n' "$commit"
    printf 'prefix=%s\n' "$prefix"
    printf 'profile=%s\n' "$profile"
    printf 'emacs_version=%s\n' "$version"
    printf 'build_time=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'cc=%s\n' "$(cc --version | head -n 1)"
    if [[ $profile == linux-gtk3 ]]; then
        printf 'tree_sitter_version=%s\n' "$tree_sitter_version"
        printf 'tree_sitter_sha256=%s\n' "$tree_sitter_sha256"
    fi
    # The full configure invocation, prefix first, exactly as it was run.
    printf 'configure_arg=--prefix=%s\n' "$prefix"
    for arg in "${configure_args[@]}"; do
        printf 'configure_arg=%s\n' "$arg"
    done
} >"$manifest.tmp"
mv -f "$manifest.tmp" "$manifest"

log "installed GNU Emacs $version at $prefix"
log "manifest: $manifest"
