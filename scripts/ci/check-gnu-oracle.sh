#!/usr/bin/env bash
# Prove that an installed GNU Emacs is the parity oracle built by
# scripts/ci/build-gnu-oracle.sh.
#
#     scripts/ci/check-gnu-oracle.sh <emacs> <version> <manifest> [<commit>]
#
# .github/actions/setup-gnu-emacs/action.yml,
# .github/actions/setup-gnu-emacs-macos/action.yml and the local NixOS build
# all run THIS file; there is no second copy of the assertions.  It fails at
# the environment seam -- before any oracle test runs -- when the installed
# build differs from the pinned reference, because a divergence measured
# against a different GNU is not a divergence.  The manifest's `profile` line
# selects the host family's expectations: `linux-gtk3` (the default for
# manifests written before the line existed), `macos-ns` (the macOS CI
# reference), `macos-batch` (the pre-NS macOS reference, opt-in only) or
# `macos-x11` (opt-in window-system comparison).
#
# The checks, and what each one catches:
#   * emacs-version is the pinned release.
#   * system-configuration-features has the features the profile promises.
#     linux-gtk3: the window system (X11/GTK3), the native primitives
#     (TREE_SITTER/DBUS/GSETTINGS/GNUTLS/LIBXML2/GMP) and the image libraries
#     the face fixtures use.  A TTY-only build succeeds and reports the same
#     version, but is a different oracle: it lacks 107 window-system mouse
#     bindings, and their long keys shift every describe-bindings block one
#     tab-width wider.  macos-ns: NS/PDUMPER/KQUEUE/GNUTLS/LIBXML2/MODULES/
#     THREADS plus the explicitly enabled RSVG/WEBP, with (featurep 'ns)
#     true and the AppKit native image API covering png/jpeg/gif/tiff/heic
#     (src/image.c:13147-13186, 13226-13246; src/nsterm.m:11749 provides
#     `ns`).  Macos-batch: PDUMPER/KQUEUE/GNUTLS/LIBXML2/MODULES/THREADS and
#     no window system, because that is the reference the pre-NS macOS
#     numbers were measured against.
#   * ...and does NOT have NATIVE_COMP or the features configure was told to
#     disable (linux-gtk3: GCONF/SOUND/LIBSYSTEMD; the macOS profiles also
#     GSETTINGS, macos-ns additionally X11/GTK3/PGTK/IMAGEMAGICK).
#     A native-comp build ships inline docstrings where canonical byte-compiled
#     Lisp has lazy (#$ . N) references, which changes preloaded doc slot 4.
#   * GSETTINGS is present on linux-gtk3: the desktop-font scenarios read the
#     font configuration through GSettings, and a build without it silently
#     falls back to a different font list.
#   * on linux-gtk3 the bundled Tree-sitter runtime (the version in the
#     manifest) is installed in the prefix, its .pc file points back at the
#     prefix, the executable resolves libtree-sitter from there, and
#     `treesit-available-p' is true.  An older system runtime links fine but
#     rejects the ABI-15 grammars (src/treesit.c:728-755).
#   * system-configuration-options equals the configure line recorded in the
#     manifest, apart from its --prefix.  This is the identity the version
#     string cannot provide.
#   * the installed bindings.elc carries lazy (#$ . N) docstring references.
#     This is the artifact check the feature check cannot replace: a binary
#     built without native compilation can still be *installed over* a Lisp
#     tree that an earlier native-comp build byte-compiled, and that stale
#     tree is what changes doc-slot-4 oracle records.
#   * the D-Bus native primitives exist on linux-gtk3, because notification
#     fixtures use the oracle as a private D-Bus service.
set -euo pipefail

readonly usage="usage: $0 <emacs> <version> <manifest> [<commit>]"

if [[ $# -lt 3 || $# -gt 4 ]]; then
    echo "$usage" >&2
    exit 2
fi

readonly emacs=$1
readonly version=$2
readonly manifest=$3
readonly want_commit=${4:-}

fail() {
    echo "check-gnu-oracle: $*" >&2
    exit 1
}

[[ -x $emacs ]] || fail "$emacs is not executable"
[[ -f $manifest ]] || fail "missing build manifest $manifest"

# The manifest is the recipe's own record: build-gnu-oracle.sh writes the
# configure arguments in the order it passed them, and configure records that
# same order in system-configuration-options (configure.ac:58-93).
prefix=
manifest_version=
manifest_commit=
tree_sitter_version=
tree_sitter_sha256=
manifest_system_configuration=
manifest_x_bitmap_file_path=
manifest_ns_feature=
manifest_native_image_api=
manifest_image_types=
# Manifests written before the profile line are the Linux GTK3 oracle.
profile=linux-gtk3
configure_args=()
configure_env=()
while IFS= read -r line; do
    case $line in
        '#'*) ;;
        prefix=*) prefix=${line#prefix=} ;;
        emacs_version=*) manifest_version=${line#emacs_version=} ;;
        commit=*) manifest_commit=${line#commit=} ;;
        profile=*) profile=${line#profile=} ;;
        tree_sitter_version=*) tree_sitter_version=${line#tree_sitter_version=} ;;
        system_configuration=*) manifest_system_configuration=${line#system_configuration=} ;;
        x_bitmap_file_path=*) manifest_x_bitmap_file_path=${line#x_bitmap_file_path=} ;;
        tree_sitter_sha256=*) tree_sitter_sha256=${line#tree_sitter_sha256=} ;;
        ns_feature=*) manifest_ns_feature=${line#ns_feature=} ;;
        native_image_api=*) manifest_native_image_api=${line#native_image_api=} ;;
        image_types=*) manifest_image_types=${line#image_types=} ;;
        configure_arg=*) configure_args+=("${line#configure_arg=}") ;;
        configure_env=*) configure_env+=("${line#configure_env=}") ;;
    esac
done <"$manifest"

[[ -n $prefix ]] || fail "$manifest records no prefix"
[[ -n $manifest_version ]] || fail "$manifest records no emacs_version"
[[ -n $manifest_commit ]] || fail "$manifest records no commit"
[[ ${#configure_args[@]} -gt 0 ]] || fail "$manifest records no configure args"
if [[ $profile == linux-gtk3 ]]; then
    [[ -n $tree_sitter_version ]] || fail "$manifest records no tree_sitter_version"
    [[ -n $tree_sitter_sha256 ]] || fail "$manifest records no tree_sitter_sha256"
fi

if [[ -n $want_commit && $want_commit != "$manifest_commit" ]]; then
    fail "manifest is for commit $manifest_commit, parity-reference pins $want_commit"
fi

got_version=$("$emacs" --batch --quick --eval '(princ emacs-version)')
if [[ $got_version != "$version" ]]; then
    fail "oracle reports emacs-version $got_version, expected $version"
fi
if [[ $manifest_version != "$version" ]]; then
    fail "manifest records emacs_version $manifest_version, expected $version"
fi

# system-configuration is a configure-time host input: on Darwin config.guess
# appends the build host's kernel release.  A manifest written by the current
# recipe records it, and the oracle must still answer that value; otherwise
# the "reference" the tests compare against has drifted from its own recipe
# (an OS update between build and test is the usual cause).
if [[ -n $manifest_system_configuration ]]; then
    got_system_configuration=$("$emacs" --batch --quick --eval '(princ system-configuration)')
    if [[ $got_system_configuration != "$manifest_system_configuration" ]]; then
        fail "oracle reports system-configuration $got_system_configuration, manifest records $manifest_system_configuration"
    fi
fi
# x-bitmap-file-path is the configure bitmapdir substitution, another
# build-host input (configure.ac:2754-2769, src/image.c:13270).
if [[ -n $manifest_x_bitmap_file_path ]]; then
    got_x_bitmap_file_path=$("$emacs" --batch --quick --eval '(princ (if (boundp (quote x-bitmap-file-path)) (prin1-to-string x-bitmap-file-path) "unbound"))')
    if [[ $got_x_bitmap_file_path != "$manifest_x_bitmap_file_path" ]]; then
        fail "oracle reports x-bitmap-file-path $got_x_bitmap_file_path, manifest records $manifest_x_bitmap_file_path"
    fi
fi

features=$("$emacs" --batch --quick --eval '(princ system-configuration-features)')
echo "oracle profile: $profile"
echo "oracle features: $features"

missing=
case $profile in
    linux-gtk3)
        for want in \
            X11 GTK3 TREE_SITTER PDUMPER DBUS GSETTINGS \
            GNUTLS LIBXML2 GMP \
            PNG JPEG GIF TIFF WEBP RSVG LCMS2 ZLIB XPM CAIRO HARFBUZZ FREETYPE; do
            case " $features " in
                *" $want "*) ;;
                *) missing="$missing $want" ;;
            esac
        done
        for forbidden in NATIVE_COMP GCONF SOUND LIBSYSTEMD LIBOTF; do
            case " $features " in
                *" $forbidden "*) missing="$missing -$forbidden" ;;
            esac
        done
        if [[ -n $missing ]]; then
            fail "GNU oracle configuration does not match the pinned reference; wrong:$missing
The window system is required: without X11/GTK3 the oracle lacks the
window-system mouse bindings that keep describe-bindings aligned."
        fi
        ;;
    macos-ns)
        # The macOS CI reference: an NS build.  GNU registers syms_of_image
        # (image.c; x-bitmap-file-path, image-types) and syms_of_fontset
        # (set-fontset-font) only under HAVE_WINDOW_SYSTEM/HAVE_NS
        # (src/emacs.c), and Neomacs presents its own GUI window system, so
        # the window-system surface is the point of this profile.  NS answers
        # --batch without a display.  kqueue is the Darwin notification
        # backend; RSVG/WEBP are enabled explicitly from Homebrew librsvg and
        # libwebp; the AppKit native image API covers png/jpeg/gif/tiff/heic
        # (image.c:13147-13186, 13226-13246) and HAVE_NS covers xpm, so the C
        # image libraries are deliberately absent.
        for want in NS PDUMPER KQUEUE GNUTLS LIBXML2 MODULES THREADS RSVG WEBP TREE_SITTER LCMS2; do
            case " $features " in
                *" $want "*) ;;
                *) missing="$missing $want" ;;
            esac
        done
        for forbidden in NATIVE_COMP X11 GTK3 PGTK GCONF GSETTINGS SOUND LIBSYSTEMD IMAGEMAGICK SQLITE3; do
            case " $features " in
                *" $forbidden "*) missing="$missing -$forbidden" ;;
            esac
        done
        if [[ -n $missing ]]; then
            fail "GNU oracle configuration does not match the pinned macOS NS reference; wrong:$missing
The NS window system is required: a --without-ns build has no syms_of_image/
syms_of_fontset and compares a GUI Neomacs against a batch GNU."
        fi
        ;;
    macos-batch)
        # The macOS parity reference is deliberately window-system-less
        # (CI has no X server, and an NS build answers different face
        # questions); kqueue is the Darwin notification backend
        # (src/kqueue.c), and the TLS/XML libraries are the ones the recipe
        # installs.  A silent --without-gnutls downgrade is a different oracle.
        for want in PDUMPER KQUEUE GNUTLS LIBXML2 MODULES THREADS; do
            case " $features " in
                *" $want "*) ;;
                *) missing="$missing $want" ;;
            esac
        done
        for forbidden in NATIVE_COMP X11 GTK3 NS PGTK GCONF GSETTINGS SOUND LIBSYSTEMD; do
            case " $features " in
                *" $forbidden "*) missing="$missing -$forbidden" ;;
            esac
        done
        if [[ -n $missing ]]; then
            fail "GNU oracle configuration does not match the pinned macOS reference; wrong:$missing
kqueue is Darwin's notification backend and the build must be
window-system-less (--without-x --without-ns), as characterized."
        fi
        ;;
    macos-x11)
        # Decision (a): the macOS oracle with the Linux reference's window
        # system.  X11 is the compile-time input that restores syms_of_image,
        # the x-* functions and the window-system faces; XPM/PNG/JPEG/GIF/
        # TIFF/WEBP/RSVG/LCMS2 are the image libraries the face fixtures use.
        # The build uses no X toolkit: GTK3 is unavailable against Homebrew's
        # Quartz-only GTK, and Lucid/Motif would add toolkit-specific Lisp
        # names (lucid--menu-grab-keyboard, motif-version-string) that the
        # toolkit-neutral Linux X11 surface does not have.
        for want in \
            X11 TREE_SITTER PDUMPER KQUEUE GNUTLS LIBXML2 GMP GSETTINGS \
            PNG JPEG GIF TIFF WEBP RSVG LCMS2 ZLIB XPM CAIRO HARFBUZZ FREETYPE; do
            case " $features " in
                *" $want "*) ;;
                *) missing="$missing $want" ;;
            esac
        done
        for forbidden in NATIVE_COMP NS PGTK GCONF SOUND LIBSYSTEMD DBUS SQLITE3 LUCID MOTIF; do
            case " $features " in
                *" $forbidden "*) missing="$missing -$forbidden" ;;
            esac
        done
        if [[ -n $missing ]]; then
            fail "GNU oracle configuration does not match the pinned macOS X11 reference; wrong:$missing
The X11 X toolkit and image libraries are required: without them the
window-system Lisp surface (image support, x-* functions, face :font)
differs from the Linux reference."
        fi
        ;;
    *)
        fail "$manifest records unknown profile '$profile'"
        ;;
esac

if [[ $profile == macos-ns ]]; then
    # The window-system identity.  src/nsterm.m:11749 provides `ns` from
    # syms_of_nsterm, so it is true even in --batch; an NS build's window
    # system surface (syms_of_image, syms_of_fontset) is what the macOS
    # reference is.
    ns_feature=$("$emacs" --batch --quick --eval '(princ (if (featurep (quote ns)) "yes" "no"))')
    [[ $ns_feature == yes ]] || fail "(featurep 'ns) is $ns_feature; the oracle is not an NS build"
    if [[ -n $manifest_ns_feature && $manifest_ns_feature != "$ns_feature" ]]; then
        fail "oracle reports (featurep 'ns) $ns_feature, manifest records $manifest_ns_feature"
    fi
    native_image_api=$("$emacs" --batch --quick --eval '(princ (if (image-type-available-p (quote native-image)) "yes" "no"))')
    [[ $native_image_api == yes ]] || fail "the AppKit native image API is unavailable; the NS oracle lost its image surface"
    if [[ -n $manifest_native_image_api && $manifest_native_image_api != "$native_image_api" ]]; then
        fail "oracle reports native-image $native_image_api, manifest records $manifest_native_image_api"
    fi
    # AppKit provides png/jpeg/gif/tiff/heic; HAVE_NS provides xpm; the
    # explicitly enabled librsvg and libwebp provide svg and webp
    # (src/image.c:13140-13246).
    for image_type in png jpeg gif tiff xpm svg webp heic; do
        got_image=$("$emacs" --batch --quick --eval "(princ (if (image-type-available-p (quote $image_type)) \"yes\" \"no\"))")
        [[ $got_image == yes ]] || fail "image type '$image_type' is unavailable in the NS oracle"
    done
    if [[ -n $manifest_image_types ]]; then
        got_image_types=$("$emacs" --batch --quick --eval '(prin1 image-types)')
        [[ $got_image_types == "$manifest_image_types" ]] || fail "oracle image-types $got_image_types, manifest records $manifest_image_types"
    fi
fi

# The prefix is the one legitimate difference between CI's oracle and a local
# one, so strip it before comparing the rest of the configure line verbatim.
expected=
for arg in "${configure_args[@]}"; do
    case $arg in
        --prefix=*) [[ $arg == "--prefix=$prefix" ]] || fail "manifest prefix $prefix disagrees with $arg" ;;
        *) expected+="${expected:+ }$arg" ;;
    esac
done
# Configure appends exported CFLAGS/CPPFLAGS/LDFLAGS to the options string
# (configure.ac:64-93), in that order; a manifest written by the X11 recipe
# records the ones it exported.
if [[ ${#configure_env[@]} -gt 0 ]]; then
    for assignment in "${configure_env[@]}"; do
        expected+="${expected:+ }$assignment"
    done
fi

options=$("$emacs" --batch --quick --eval '(princ system-configuration-options)')
prefix_token="--prefix=$prefix"
if [[ $options == "$prefix_token" ]]; then
    options=
elif [[ $options == "$prefix_token "* ]]; then
    options=${options#"$prefix_token" }
else
    fail "system-configuration-options does not start with $prefix_token:
$options"
fi
if [[ $options != "$expected" ]]; then
    fail "system-configuration-options differs from the recipe's configure line
  expected: $expected
  actual:   $options"
fi

if [[ $profile == linux-gtk3 ]]; then
    # The bundled Tree-sitter runtime.  Parity-reference.toml's identity is
    # the commit plus the recipe; the runtime version is part of the recipe
    # because GNU refuses a grammar whose ABI is newer than the runtime it was
    # compiled against (src/treesit.c:728-755), and Ubuntu 24.04's system 0.20
    # runtime is ABI 13 while the pinned grammars include ABI 15.
    readonly pc="$prefix/lib/pkgconfig/tree-sitter.pc"
    [[ -f $pc ]] || fail "missing bundled Tree-sitter runtime: $pc"
    pc_version=$(sed -n 's/^[Vv]ersion: *//p' "$pc" | head -n 1)
    [[ $pc_version == "$tree_sitter_version" ]] ||
        fail "bundled Tree-sitter .pc reports version '$pc_version', manifest pins $tree_sitter_version"
    pc_prefix=$(sed -n 's/^prefix=//p' "$pc" | head -n 1)
    [[ $pc_prefix == "$prefix" ]] ||
        fail "bundled Tree-sitter .pc prefix '$pc_prefix' disagrees with $prefix"
    "$emacs" --batch --quick --eval '(unless (treesit-available-p)
  (error "GNU oracle cannot load its bundled Tree-sitter runtime"))'
fi

# Lazy docstring references are what a canonical byte-compiled installation
# has; see the header.  A file with none is native-comp output or a stale
# install.
installed_bindings="$prefix/share/emacs/$version/lisp/bindings.elc"
[[ -f $installed_bindings ]] || fail "missing $installed_bindings"
lazy_refs=$(grep -a -o -F '#$' "$installed_bindings" | wc -l | tr -d ' ' || true)
if [[ $lazy_refs -eq 0 ]]; then
    fail "$installed_bindings has no lazy (#\$ . N) docstring references:
the installed Lisp tree is not canonical byte-compiler output"
fi
echo "installed bindings.elc: $(wc -c <"$installed_bindings" | tr -d ' ') bytes, $lazy_refs lazy docstring references"

if [[ $profile == linux-gtk3 ]]; then
    "$emacs" --batch --quick --eval "(progn
  (require 'dbus)
  (dolist (function '(dbus-call-method dbus-register-method))
    (unless (fboundp function)
      (error \"GNU oracle lacks D-Bus function %s\" function)))
  (dolist (function '(dbus--init-bus dbus-get-unique-name dbus-message-internal))
    (unless (and (fboundp function) (subrp (symbol-function function)))
      (error \"GNU oracle lacks native D-Bus primitive %s\" function))))"
fi

# A manually installed oracle needs its dynamic libraries at run time; NixOS
# has no system loader path.  The shell's build sets a RUNPATH, and this
# catches a relocation that did not take before a test run fails obscurely.
# LD_LIBRARY_PATH is unset for this check on purpose: inside the build shell
# it would resolve the libraries and hide an incomplete RUNPATH, which is
# exactly the failure this guards.  On CI the system loader provides them.
if command -v ldd >/dev/null; then
    if ldd_out=$(env -u LD_LIBRARY_PATH ldd "$emacs" 2>&1) && [[ $ldd_out == *"not found"* ]]; then
        fail "unresolved shared libraries:
$ldd_out"
    fi
    if [[ $profile == linux-gtk3 ]]; then
        tree_line=$(printf '%s\n' "$ldd_out" | grep 'libtree-sitter' || true)
        [[ -n $tree_line ]] || fail "the oracle does not link the Tree-sitter runtime"
        case $tree_line in
            *"$prefix/lib/"*) ;;
            *) fail "the oracle resolves libtree-sitter outside the bundled prefix: $tree_line" ;;
        esac
    fi
fi

"$emacs" --version | head -n 1
