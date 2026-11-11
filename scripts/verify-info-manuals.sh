#!/usr/bin/env bash
set -euo pipefail

fixture_runtime_root=0
if [[ ${1:-} == --fixture-runtime-root ]]; then
  fixture_runtime_root=1
  shift
fi
if (($# != 2)); then
  echo "Usage: scripts/verify-info-manuals.sh [--fixture-runtime-root] BINARY EXPECTED_RUNTIME_ROOT" >&2
  echo "Artifacts discover their runtime naturally; --fixture-runtime-root is only for arbitrary staged fixtures." >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$repo_root/tmp"
python3 - "$repo_root" "$1" "$2" "$fixture_runtime_root" <<'PY'
import os
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path(sys.argv[1]).resolve()
# Preserve the invoked wrapper/symlink location when testing an installation.
binary = Path(sys.argv[2]).absolute()
runtime = Path(sys.argv[3]).resolve()
fixture_runtime_root = sys.argv[4] == "1"
info = runtime / "info"
if info.is_symlink():
    sys.exit(f"Artifact Info directory must be inside its runtime tree: {info}")
with tempfile.TemporaryDirectory(prefix="info-manuals.", dir=repo / "tmp") as scratch:
    env = os.environ.copy()
    for name in ("INFOPATH", "EMACSLOADPATH", "NEOMACS_RUNTIME_ROOT"):
        env.pop(name, None)
    env.update(
        HOME=scratch,
        XDG_CONFIG_HOME=os.path.join(scratch, "config"),
        NEOMACS_EXPECTED_INFO_DIRECTORY=str(info),
    )
    # Artifact checks must exercise the executable or its production wrapper.
    # Only fixtures placed outside an executable's installation layout need
    # the explicit override; the expected path above never controls discovery.
    if fixture_runtime_root:
        env["NEOMACS_RUNTIME_ROOT"] = str(runtime)
    command = [str(binary), "--batch", "-Q", "-l", str(repo / "scripts/lib/verify-info-manuals.el")]
    try:
        result = subprocess.run(
            command, env=env, cwd=scratch, stdin=subprocess.DEVNULL,
            capture_output=True, text=True, timeout=60,
        )
    except subprocess.TimeoutExpired:
        sys.exit(f"Info manual probe timed out after 60 seconds: {binary}")
    if result.returncode or "NEOMACS_INFO_MANUALS_OK" not in result.stdout.splitlines():
        print(result.stdout, end="", file=sys.stderr)
        print(result.stderr, end="", file=sys.stderr)
        sys.exit(f"Info manual probe failed (exit {result.returncode}): {binary}")
    print(result.stdout, end="")
PY
