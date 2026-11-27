#!/usr/bin/env bash
# Fail-closed verifier for an immutable Neomacs performance baseline binary and
# its sibling portable dump against a committed manifest JSON.
#
# Usage:
#   scripts/ci/verify-perf-baseline.sh \
#     --manifest .github/perf-baselines/v0.0.21.json \
#     --env-var NEOMACS_PERF_V0021_BASELINE_EDITOR \
#     "$PERF_GUARD_BASELINE"

set -euo pipefail

manifest=""
env_var="NEOMACS_PERF_V0021_BASELINE_EDITOR"

require_opt_arg() {
  if [ "$2" -lt 2 ]; then
    echo "ERROR: option $1 requires a value" >&2
    exit 2
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --manifest)
      require_opt_arg "$1" "$#"
      manifest="$2"
      shift 2
      ;;
    --env-var)
      require_opt_arg "$1" "$#"
      env_var="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "ERROR: unknown option: $1" >&2
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

if [ "$#" -gt 1 ]; then
  echo "ERROR: unexpected extra arguments: ${*:2}" >&2
  exit 2
fi

baseline_path="${1:-}"

if [ -z "$manifest" ]; then
  echo "ERROR: --manifest PATH is required" >&2
  exit 2
fi

if [ ! -f "$manifest" ]; then
  echo "ERROR: baseline manifest not found: $manifest" >&2
  exit 1
fi

if [ -z "$baseline_path" ]; then
  echo "ERROR: frozen baseline is required; set $env_var (or workflow input guard_baseline_editor) to the extracted baseline neomacs executable pinned in $manifest" >&2
  exit 1
fi

if [ ! -f "$baseline_path" ] || [ ! -x "$baseline_path" ]; then
  echo "ERROR: $env_var does not point to an executable file: $baseline_path" >&2
  exit 1
fi

python3 - "$manifest" "$baseline_path" "$env_var" <<'PY'
import hashlib
import json
import os
import sys

manifest_path, baseline_path, env_var = sys.argv[1:4]

with open(manifest_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

if data.get("schema_version") != 1:
    print(
        f"ERROR: unsupported baseline manifest schema_version={data.get('schema_version')!r} in {manifest_path}",
        file=sys.stderr,
    )
    sys.exit(1)

def check_hex64(label, value):
    if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
        print(f"ERROR: invalid {label} sha256 in {manifest_path}: {value!r}", file=sys.stderr)
        sys.exit(1)

def sha256_and_size(path):
    h = hashlib.sha256()
    size = 0
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
            size += len(chunk)
    return h.hexdigest(), size

release_asset_spec = data.get("release_asset") or {}
binary_spec = data.get("binary") or {}
pdump_spec = data.get("pdump") or {}
check_hex64("release_asset", release_asset_spec.get("sha256"))
check_hex64("binary", binary_spec.get("sha256"))
check_hex64("pdump", pdump_spec.get("sha256"))

for opt_field in ("build_profile", "rustc_version", "pgo_profile", "codegen_settings"):
    if opt_field in data and data[opt_field] is not None:
        val = data[opt_field]
        if not isinstance(val, str) or not val.strip():
            print(
                f"ERROR: {opt_field} must be a non-empty string when present in {manifest_path}: {val!r}",
                file=sys.stderr,
            )
            sys.exit(1)

rel_pdump = pdump_spec.get("relative_to_binary")
if not isinstance(rel_pdump, str) or not rel_pdump or os.path.isabs(rel_pdump):
    print(
        f"ERROR: pdump.relative_to_binary must be a non-empty relative path in {manifest_path}: {rel_pdump!r}",
        file=sys.stderr,
    )
    sys.exit(1)

bin_real = os.path.realpath(baseline_path)
bin_sha, bin_size = sha256_and_size(bin_real)
exp_bin_sha = binary_spec["sha256"]
exp_bin_size = binary_spec.get("size_bytes")
if bin_size != exp_bin_size or bin_sha != exp_bin_sha:
    print(
        f"ERROR: {env_var} binary digest mismatch for {baseline_path} against {manifest_path}: "
        f"expected sha256={exp_bin_sha} size={exp_bin_size}, "
        f"observed sha256={bin_sha} size={bin_size}",
        file=sys.stderr,
    )
    sys.exit(1)

pdump_path = os.path.normpath(os.path.join(os.path.dirname(bin_real), rel_pdump))
if not os.path.isfile(pdump_path):
    print(
        f"ERROR: {env_var} sibling portable dump missing at {pdump_path} (expected by {manifest_path})",
        file=sys.stderr,
    )
    sys.exit(1)

pdump_sha, pdump_size = sha256_and_size(pdump_path)
exp_pdump_sha = pdump_spec["sha256"]
exp_pdump_size = pdump_spec.get("size_bytes")
if pdump_size != exp_pdump_size or pdump_sha != exp_pdump_sha:
    print(
        f"ERROR: {env_var} portable dump digest mismatch for {pdump_path} against {manifest_path}: "
        f"expected sha256={exp_pdump_sha} size={exp_pdump_size}, "
        f"observed sha256={pdump_sha} size={pdump_size}",
        file=sys.stderr,
    )
    sys.exit(1)

extra = []
if data.get("build_profile"):
    extra.append(f"profile={data['build_profile']}")
if data.get("pgo_profile"):
    extra.append(f"pgo={data['pgo_profile']}")
if data.get("rustc_version"):
    extra.append(f"rustc={data['rustc_version']!r}")
extra_str = (" " + " ".join(extra)) if extra else ""

print(
    f"baseline verified: version={data.get('version')} commit={data.get('git_commit')}{extra_str} "
    f"binary={bin_sha} ({bin_size} B) pdump={pdump_sha} ({pdump_size} B)"
)
PY
