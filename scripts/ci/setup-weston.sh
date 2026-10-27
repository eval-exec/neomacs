#!/usr/bin/env bash
# Build the Weston compositor the real-GUI suite talks to.
#
# Ubuntu 24.04 ships weston 13.0.0, whose pixman renderer keeps a raw pointer
# into a wl_shm buffer's mapping: when a client destroys the buffer between
# commit and repaint, repaint_region() composites through the unmapped memory
# and the compositor dies with SIGSEGV (upstream #922, fixed in 14.0.0 by
# 08c7015a "pixman-renderer: Check if the shm_buffer is gone during attach").
# Neomacs's Vulkan and EGL swapchains recycle wl_buffers exactly that way, so
# on the runner every Wayland scenario lost its compositor mid-test and the
# editor reported a broken display connection instead of the status Lisp asked
# for.  The pinned release is the oldest one with that fix and the newest whose
# build inputs Ubuntu 24.04 provides: weston 14 needs wayland-protocols >= 1.33
# (noble has 1.34), while weston 16 wants >= 1.46.
#
# Only the headless and X11 backends and the pixman renderer are built: every
# scenario pins pixman, the suite needs no DRM device, and the runner has no
# EGL screen.  The X11 backend presents through MIT-SHM, so the harness starts
# that Xvfb on its local Unix socket (start_local_xvfb_with_size).
set -euo pipefail

readonly version=14.0.1
readonly sha256=0cd789cf0a2a4db43733078bc69d99c296cf79c7cbed439af8593359e3a97678
readonly url="https://gitlab.freedesktop.org/wayland/weston/-/archive/${version}/weston-${version}.tar.gz"

work="${TMPDIR:-/tmp}/weston-${version}"
rm -rf "$work"
mkdir -p "$work"

# The self-contained dependency list.  wayland, wayland-protocols, xkbcommon,
# X11 and xcb development files also come from setup-linux.sh's build profile;
# listing them here keeps this script runnable on its own and pins the exact
# inputs the version above is built against.
sudo apt-get install -y --no-install-recommends \
    meson ninja-build \
    libpixman-1-dev libinput-dev libevdev-dev libdrm-dev \
    libcairo2-dev libxcb-shm0-dev libx11-xcb-dev \
    libwayland-dev libwayland-bin wayland-protocols libxkbcommon-dev \
    libx11-dev libxcb1-dev

curl -fsSL -o "$work/weston.tar.gz" "$url"
echo "$sha256  $work/weston.tar.gz" | sha256sum -c -
tar xzf "$work/weston.tar.gz" -C "$work"
source="$work/weston-$version"

# The runner's wayland is 1.22, whose libwayland-dev does not ship the
# wayland-scanner.pc that weston 14's meson asks for (upstream added it in
# wayland 1.23), and whose generated "private-code" output type is newer than
# the 1.14.91 cut-off weston checks.  Both are packaging facts about the
# pinned runner, so resolve them here instead of rebuilding wayland itself:
# locate the scanner installed by libwayland-bin, and keep the modern output
# type unconditionally.
python3 - "$source/protocol/meson.build" <<'PY'
import sys

path = sys.argv[1]
source = open(path, encoding="utf-8").read()
scanner = (
    "dep_scanner = dependency('wayland-scanner', native: true)\n"
    "prog_scanner = find_program(dep_scanner.get_variable(pkgconfig: 'wayland_scanner'))\n"
)
if scanner not in source:
    sys.exit(f"unexpected wayland-scanner discovery in {path}")
source = source.replace(
    scanner,
    "# Ubuntu 24.04's wayland 1.22 has no wayland-scanner.pc; libwayland-bin\n"
    "# installs the program itself.\n"
    "prog_scanner = find_program('wayland-scanner')\n",
)
legacy_code_type = (
    "\t\t\tif dep_scanner.version().version_compare('< 1.14.91')\n"
    "\t\t\t\toutput_type = 'code'\n"
    "\t\t\tendif\n"
)
if legacy_code_type not in source:
    sys.exit(f"unexpected wayland-scanner version gate in {path}")
source = source.replace(
    legacy_code_type,
    "\t\t\t# wayland-scanner >= 1.14.91 emits 'private-code'; the runner's\n"
    "\t\t\t# scanner is 1.22, so the legacy 'code' spelling is never needed.\n",
)
open(path, "w", encoding="utf-8").write(source)
PY

meson setup "$work/build" "$source" \
    --prefix=/usr/local \
    --buildtype=release \
    -Dbackend-drm=false \
    -Dbackend-drm-screencast-vaapi=false \
    -Dbackend-pipewire=false \
    -Dbackend-rdp=false \
    -Dbackend-vnc=false \
    -Dbackend-wayland=false \
    -Dbackend-x11=true \
    -Dbackend-headless=true \
    -Dbackend-default=headless \
    -Drenderer-gl=false \
    -Dxwayland=false \
    -Dsystemd=false \
    -Dremoting=false \
    -Dpipewire=false \
    -Dscreenshare=false \
    -Dimage-jpeg=false \
    -Dimage-webp=false \
    -Ddemo-clients=false \
    -Dsimple-clients= \
    -Dtests=false \
    -Ddoc=false \
    -Dcolor-management-lcms=false \
    -Dwcap-decode=false
ninja -C "$work/build"
sudo ninja -C "$work/build" install
sudo ldconfig

# Fail at the environment seam.  The tests spawn `weston` from PATH and
# /usr/local/bin precedes /usr/bin on the runner, so this is the binary they
# will use; if that ever stops being true the suite must fail here rather than
# silently run against the broken 13.0.0 that apt installs.
installed="$(command -v weston)"
test "$installed" = /usr/local/bin/weston
test "$(weston --version | awk '{print $2}')" = "$version"
