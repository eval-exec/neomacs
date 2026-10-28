#!/usr/bin/env bash
# The macOS neovm-core parity job, minus the runner plumbing (checkout, Rust
# and Homebrew caches).  `.github/workflows/test-suite.yml`'s
# `neovm-core-tests-macos` job runs exactly this file, so a lane can run the
# same file on a Mac to prove the job before main's CI does:
#
#     scripts/ci/run-neovm-core-macos-job.sh \
#         /opt/gnu-emacs-oracle/bin/emacs \
#         /opt/gnu-emacs-oracle/share/gnu-emacs-oracle/build-manifest \
#         31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9
#
# The job exists so the Darwin parity work cannot rot: the suite runs the
# neovm-core package (unit and integration tests, including the oracle
# families that spawn GNU) against the pinned macOS GNU built by
# scripts/ci/build-gnu-oracle.sh (Darwin profile, window-system-less with
# kqueue).  A missing or different oracle fails in the checker below; nothing
# is skipped.
set -euo pipefail

readonly usage="usage: $0 <gnu-emacs> <build-manifest> <version> <commit>"

if [[ $# -ne 4 ]]; then
    echo "$usage" >&2
    exit 2
fi

readonly gnu=$1
readonly manifest=$2
readonly version=$3
readonly commit=$4

root=$(cd "$(dirname "$0")/../.." && pwd)
readonly root
cd "$root"

# The oracle is the environment seam: prove it is the pinned build before
# running anything.  A cache miss that produced a TTY-less, native-comp or
# otherwise different GNU fails here, not inside a hundred parity tests.
scripts/ci/check-gnu-oracle.sh "$gnu" "$version" "$manifest" "$commit"

# The suite's runtime image and the generated Lisp tree come from fresh-build.
# These are harness artifacts, never shipped: a hosted macOS runner has ~7 GB
# of RAM, and this workspace's shipping profile is fat LTO with a link that
# needs far more.  The Linux archive job makes the same trade for the same
# reason (test-suite.yml, neomacs-workspace-test-archive), and no neovm-core
# assertion is about the runtime binary's optimization level.
export CARGO_PROFILE_RELEASE_LTO=${CARGO_PROFILE_RELEASE_LTO:-false}
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}
cargo xtask fresh-build --release

# Prove the nil-`text-quoting-style' locale resolution, the one input this job
# can check end to end (batch, no pty): C locale is grave, UTF-8 is curve, and
# the display-table backtick entry is grave, agreeing with the pinned GNU
# byte for byte (scripts/ci/check-text-quoting-locale.sh cites the GNU lines).
# `CARGO_TARGET_DIR` is honored so a lane's private target directory works.
target_dir=${CARGO_TARGET_DIR:-$root/target}
scripts/ci/check-text-quoting-locale.sh "$target_dir/release/neomacs" "$gnu"

# The suite.  Oracle families resolve GNU through the same attested resolver
# (neomacs-test-oracle) from this variable, and a missing oracle panics inside
# them rather than skipping.
NEOVM_FORCE_ORACLE_PATH="$gnu" \
    cargo nextest run -p neovm-core --test-threads 16 --no-fail-fast
