#!/usr/bin/env bash
# Provide labwc 0.9.7 for the live xdg-toplevel-icon test.
#
# Weston 16 deliberately implements no xdg-toplevel-icon-v1 (its registry has
# no xdg_toplevel_icon_manager_v1), so the protocol test cannot run under the
# menu compositor.  labwc 0.9.0+ implements it; 0.9.7 is the version this
# workspace's pinned nixpkgs carries, so the job runs the compositor a NixOS
# developer gets from `nix shell` instead of building wlroots on a 4-core
# runner: cache.nixos.org substitutes the exact store path for the flake.lock
# revision.
#
# Only the launcher is linked into /usr/local/bin (ahead of the distribution's
# bin, which has no labwc at all).  The launcher exec's the store binary so
# labwc resolves its theme and icon data from its own prefix, and the Nix
# RUNPATH keeps its library closure independent of the runner.
set -euo pipefail

# The icon protocol appeared in labwc 0.9.0; the current flake.lock nixpkgs
# carries 0.9.7.  The pin follows flake.lock, and this gate fails the job at
# the environment seam if that ever resolves to an older compositor.
readonly min_version=0.9.0
# Only a dry run that must not touch /usr/local overrides this; CI uses the
# default.
readonly bin_dir=${LABWC_BIN_DIR:-/usr/local/bin}

fail() {
    echo "setup-labwc: $*" >&2
    exit 1
}

version_ge() {
    printf '%s\n%s\n' "$min_version" "$1" | sort -V -C
}

if command -v labwc >/dev/null 2>&1; then
    have=$(labwc --version | awk '{print $2}')
    if version_ge "$have"; then
        echo "setup-labwc: labwc $have already on PATH"
        exit 0
    fi
    fail "labwc $have is on PATH, expected at least $min_version"
fi

command -v nix >/dev/null 2>&1 ||
    fail "nix is required; run .github/actions/setup-nix first"
command -v python3 >/dev/null 2>&1 ||
    fail "python3 is required to read flake.lock"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
readonly lock="$repo_root/flake.lock"
rev=$(python3 - "$lock" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    lock = json.load(stream)
node = lock["nodes"]["nixpkgs"]["locked"]
if node.get("repo") != "nixpkgs":
    raise SystemExit(f"setup-labwc: flake.lock's nixpkgs node is {node!r}")
print(node["rev"])
PY
)

out=
while IFS= read -r candidate; do
    if [[ -x $candidate/bin/labwc ]]; then
        out=$candidate
    fi
done < <(nix build --no-link --print-out-paths "github:NixOS/nixpkgs/$rev#labwc")
[[ -n $out ]] || fail "the pinned labwc package produced no labwc launcher"

sudo mkdir -p "$bin_dir"
sudo tee "$bin_dir/labwc" >/dev/null <<EOF
#!/bin/sh
exec "$out/bin/labwc" "\$@"
EOF
sudo chmod 755 "$bin_dir/labwc"

installed=$(command -v labwc)
[[ $installed == "$bin_dir/labwc" ]] ||
    fail "labwc resolves to $installed, not $bin_dir/labwc"
have=$(labwc --version | awk '{print $2}')
version_ge "$have" || fail "installed labwc reports $have, expected at least $min_version"
labwc --version
