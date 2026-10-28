#!/usr/bin/env bash
# Build the Weston compositor the real-GUI suite talks to.
#
# Pinned: weston 16.0.0.  Why not the distribution's 13.0.0, and why not 14:
#
#  - Weston 13's pixman renderer keeps a raw pointer into a wl_shm buffer's
#    mapping: when a client destroys the buffer between commit and repaint,
#    repaint_region() composites through the unmapped memory and the
#    compositor dies with SIGSEGV (upstream #922, fixed in 14.0.0 by
#    08c7015a "pixman-renderer: Check if the shm_buffer is gone during
#    attach").  Neomacs's Vulkan and EGL swapchains recycle wl_buffers
#    exactly that way, so on the runner every Wayland scenario lost its
#    compositor mid-test.
#  - Weston 13 and 14 do not implement xdg_positioner constraint adjustment
#    at all: xdg-shell.c returns the requested geometry with a "TODO: add
#    compositor policy configuration and the code here".  Native menus place
#    their popup with SLIDE_X/SLIDE_Y/FLIP_* adjustments and, like every
#    Wayland client, rely on the compositor to keep the popup on the usable
#    area; the native-menu contracts assert exactly that constrained
#    placement.  Weston 16 is the first release whose xdg-shell adjusts the
#    popup (weston 15 implements it but aborts in
#    weston_coord_global_to_surface when a parent relayouts during the popup
#    sequence; 16 fixes the dirty parent transform).
#
# Only the headless and X11 backends and the pixman renderer are built: every
# scenario pins pixman, the suite needs no DRM device, and the runner has no
# EGL screen.  The X11 backend presents through MIT-SHM, so the harness starts
# that Xvfb on its local Unix socket (start_local_xvfb_with_size).
#
# Ubuntu 24.04 (noble) satisfies weston 16's wayland >= 1.22 and meson
# requirements, but not three of its build inputs; each is resolved here:
#
#  - wayland-protocols >= 1.46 (noble has 1.34).  Built from source into
#    /usr/local; it is a protocol-data package with no runtime library.
#  - libdisplay-info >= 0.2.0 (noble has 0.1.1).  Weston vendors the 0.2.0
#    subproject, but that subproject generates its PNP-id table from
#    /usr/share/hwdata/pnp.ids, which only the hwdata package provides.
#  - libwayland-dev ships no wayland-scanner.pc (upstream added it in
#    wayland 1.23); the scanner program itself comes from libwayland-bin, so
#    a one-variable .pc in /usr/local points weston's meson at it.
set -euo pipefail

readonly version=16.0.0
readonly sha256=6a81af51045ccb2813f3a1d63ff8a66c121743c1a5ff3ce78388660bf650c55c
readonly url="https://gitlab.freedesktop.org/wayland/weston/-/archive/${version}/weston-${version}.tar.gz"
readonly protocols_version=1.46
readonly protocols_sha256=83afedde1e63751578ba51b5f6c36052802f85634ed68be41ecfcc3a515ab03d
readonly protocols_url="https://gitlab.freedesktop.org/wayland/wayland-protocols/-/archive/${protocols_version}/wayland-protocols-${protocols_version}.tar.gz"

work="${TMPDIR:-/tmp}/weston-${version}"
rm -rf "$work"
mkdir -p "$work"

# The self-contained dependency list.  wayland, xkbcommon, X11 and xcb
# development files also come from setup-linux.sh's build profile; listing
# them here keeps this script runnable on its own and pins the exact inputs
# the versions above are built against.  `git` is what meson's wrap fetches
# the display-info subproject with; `hwdata` supplies pnp.ids.
sudo apt-get install -y --no-install-recommends \
    meson ninja-build git \
    libpixman-1-dev libinput-dev libevdev-dev libdrm-dev \
    libcairo2-dev libxcb-shm0-dev libx11-xcb-dev \
    libwayland-dev libwayland-bin wayland-protocols libxkbcommon-dev \
    libx11-dev libxcb1-dev hwdata

# wayland-protocols 1.46, installed ahead of the distribution's 1.34 in
# /usr/local (pkg-config searches it first).
curl -fsSL -o "$work/wayland-protocols.tar.gz" "$protocols_url"
echo "$protocols_sha256  $work/wayland-protocols.tar.gz" | sha256sum -c -
tar xzf "$work/wayland-protocols.tar.gz" -C "$work"
meson setup "$work/protocols-build" "$work/wayland-protocols-$protocols_version" \
    --prefix=/usr/local \
    --buildtype=release \
    -Dtests=false
sudo ninja -C "$work/protocols-build" install

# The scanner program is installed by libwayland-bin; only its .pc is
# missing, and weston reads the scanner's path from that file.
sudo mkdir -p /usr/local/lib/pkgconfig
sudo tee /usr/local/lib/pkgconfig/wayland-scanner.pc >/dev/null <<'PC'
prefix=/usr
Name: wayland-scanner
Description: Wayland protocol scanner
Version: 1.22.0
wayland_scanner=/usr/bin/wayland-scanner
PC

curl -fsSL -o "$work/weston.tar.gz" "$url"
echo "$sha256  $work/weston.tar.gz" | sha256sum -c -
tar xzf "$work/weston.tar.gz" -C "$work"
source="$work/weston-$version"

export PKG_CONFIG_PATH="/usr/local/share/pkgconfig:/usr/local/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
meson setup "$work/build" "$source" \
    --prefix=/usr/local \
    --buildtype=release \
    -Dbackend-drm=false \
    -Dbackend-pipewire=false \
    -Dbackend-rdp=false \
    -Dbackend-vnc=false \
    -Dbackend-wayland=false \
    -Dbackend-x11=true \
    -Dbackend-headless=true \
    -Dbackend-default=headless \
    -Drenderer-gl=false \
    -Drenderer-vulkan=false \
    -Dxwayland=false \
    -Dsystemd=false \
    -Ddeprecated-remoting=false \
    -Ddeprecated-pipewire=false \
    -Dshell-desktop=true \
    -Dshell-ivi=false \
    -Dshell-kiosk=true \
    -Dshell-lua=false \
    -Dcolor-management-lcms=false \
    -Dimage-jpeg=false \
    -Dimage-webp=false \
    -Dtools= \
    -Ddemo-clients=false \
    -Dsimple-clients= \
    -Dresize-pool=false \
    -Dtests=false \
    -Ddoc=false \
    -Dperfetto=false \
    -Dallow-deprecated-modeset=false
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
