#!/usr/bin/env bash
# Reproducible Option-B baseline builder and manifest generator for
# `org-editing-guard` (plain `--release` profile matching CI candidates).
#
# Builds a pinned git commit (default: v0.0.21 = 7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a)
# with `cargo xtask fresh-build --release` (non-PGO, default features, inside
# an explicitly constructed and logged allowlisted build environment), packages
# a self-contained install tree (`bin/neomacs`, `libexec/.../neomacs.pdump`,
# `share/neomacs/{lisp,etc,leim,info}`), installs it into `--out-dir`, emits a
# schema-v1 manifest JSON recording `build_profile`, `rustc_version`, and exact
# SHA-256/byte-size digests, and atomically installs `--manifest` (mode 0644)
# only after `scripts/ci/verify-perf-baseline.sh` passes.
#
# Usage (on the controlled `neomacs-perf` Linux runner):
#   scripts/ci/build-perf-baseline.sh \
#     --ref 7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a \
#     --version v0.0.21-release \
#     --out-dir /opt/neomacs-perf/v0.0.21-release \
#     --manifest .github/perf-baselines/v0.0.21-release.json
#
# Testing / manifest-only mode (emits + verifies a manifest for an already
# extracted tree without invoking cargo):
#   scripts/ci/build-perf-baseline.sh --manifest-only \
#     --ref <40-hex-commit> --version <label> --target <triple> \
#     --tarball <path> --archive-root <dir-name> \
#     --binary <path> --pdump <path> --relative-pdump <rel-path> \
#     --rustc-version "<rustc --version>" --manifest <out-json>

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"

ref="7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a"
version="v0.0.21-release"
target="x86_64-unknown-linux-gnu"
out_dir=""
manifest=""
jobs=""
manifest_only=0
tarball_path=""
archive_root=""
binary_path=""
pdump_path=""
relative_pdump=""
rustc_version_override=""
rustc_version_set=0
asset_url=""
active_tmp_manifest=""
work_tmp=""

cleanup() {
  if [ -n "$active_tmp_manifest" ]; then
    rm -f "$active_tmp_manifest"
  fi
  if [ -n "$work_tmp" ]; then
    rm -rf "$work_tmp"
  fi
}
trap cleanup EXIT INT TERM

require_opt_arg() {
  if [ "$2" -lt 2 ]; then
    echo "ERROR: option $1 requires a value" >&2
    exit 2
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --ref) require_opt_arg "$1" "$#"; ref="$2"; shift 2 ;;
    --version) require_opt_arg "$1" "$#"; version="$2"; shift 2 ;;
    --target) require_opt_arg "$1" "$#"; target="$2"; shift 2 ;;
    --out-dir) require_opt_arg "$1" "$#"; out_dir="$2"; shift 2 ;;
    --manifest) require_opt_arg "$1" "$#"; manifest="$2"; shift 2 ;;
    --jobs) require_opt_arg "$1" "$#"; jobs="$2"; shift 2 ;;
    --manifest-only) manifest_only=1; shift ;;
    --tarball) require_opt_arg "$1" "$#"; tarball_path="$2"; shift 2 ;;
    --archive-root) require_opt_arg "$1" "$#"; archive_root="$2"; shift 2 ;;
    --binary) require_opt_arg "$1" "$#"; binary_path="$2"; shift 2 ;;
    --pdump) require_opt_arg "$1" "$#"; pdump_path="$2"; shift 2 ;;
    --relative-pdump) require_opt_arg "$1" "$#"; relative_pdump="$2"; shift 2 ;;
    --rustc-version)
      require_opt_arg "$1" "$#"
      rustc_version_override="$2"
      rustc_version_set=1
      shift 2
      ;;
    --asset-url) require_opt_arg "$1" "$#"; asset_url="$2"; shift 2 ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ -z "$manifest" ]; then
  echo "ERROR: --manifest PATH is required" >&2
  exit 2
fi

resolve_rustc_version() {
  if [ "$rustc_version_set" -eq 1 ]; then
    printf '%s' "$rustc_version_override"
  else
    rustc --version
  fi
}

run_in_allowlisted_build_env() {
  # Construct an explicit allowlisted build environment from scratch (never
  # inherit-and-subtract), log the exact constructed environment, and run both
  # `cargo xtask fresh-build --release` and `scripts/package-release.sh` inside
  # it. Preserves host `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` /
  # `DYLD_FALLBACK_LIBRARY_PATH` and toolchain variables needed for build-time
  # `neomacs-temacs` bootstrap/linking (matching `.github/workflows/neomacs-perf.yml`
  # where loader search paths are only scrubbed for benchmark execution), while
  # excluding `NEOMACS_*`, `NEOVM_*`, `CARGO_TARGET_DIR`, `CARGO_PROFILE_*`,
  # `CARGO_TARGET_*`, `CARGO_BUILD_*`, `CARGO_INCREMENTAL`, `RUSTFLAGS`,
  # `CARGO_ENCODED_RUSTFLAGS`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`,
  # `RUST_LOG`, `LD_PRELOAD`, `LD_AUDIT`, `DYLD_INSERT_LIBRARIES`, and any
  # other unallowlisted ambient variable by construction.
  local cwd="$1"
  local b_target="$2"
  local b_jobs="$3"
  env -u LD_PRELOAD -u LD_AUDIT -u DYLD_INSERT_LIBRARIES \
    python3 - "$cwd" "$b_target" "$b_jobs" <<'PY'
import os, subprocess, sys

cwd, target, jobs = sys.argv[1:4]

EXACT_ALLOWLIST = {
    # Core POSIX / locale / user
    "PATH", "HOME", "USER", "LOGNAME", "TMPDIR", "TMP", "TEMP",
    "TERM", "LANG", "LC_ALL", "LC_CTYPE", "TZ",
    # Rustup / Cargo home and compiler executable (never target/profile/flag overrides)
    "CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN", "RUSTC",
    # C/C++/bindgen/pkg-config/macOS SDK toolchain
    "CC", "CC_FOR_BUILD", "CC_FOR_TARGET",
    "CXX", "CXX_FOR_BUILD", "CXX_FOR_TARGET",
    "AR", "AR_FOR_BUILD", "AR_FOR_TARGET",
    "RANLIB", "LD", "LD_FOR_BUILD", "LD_FOR_TARGET",
    "NM", "STRIP", "OBJCOPY", "CMAKE", "MAKE",
    "LIBCLANG_PATH",
    "PKG_CONFIG", "PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR",
    "PKG_CONFIG_ALLOW_SYSTEM_CFLAGS", "PKG_CONFIG_ALLOW_SYSTEM_LIBS",
    "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "DEVELOPER_DIR",
    # Build-time linker / library / cert / git / gstreamer paths (never preload/audit/insert)
    "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH",
    "LIBRARY_PATH", "CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH",
    "SSL_CERT_FILE", "SSL_CERT_DIR",
    "XDG_CACHE_HOME", "XDG_DATA_DIRS",
    "GIT_EXEC_PATH", "GIT_TEMPLATE_DIR",
    "GST_PLUGIN_SCANNER_1_0", "GST_PLUGIN_SYSTEM_PATH_1_0",
    # Nix toolchain singletons
    "NIX_ENFORCE_NO_NATIVE", "NIX_HARDENING_ENABLE",
    "NIX_STORE", "NIX_SSL_CERT_FILE", "NIX_BUILD_CORES",
}

PREFIX_ALLOWLIST = (
    "NIX_CC",
    "NIX_BINTOOLS",
    "NIX_CFLAGS_COMPILE",
    "NIX_LDFLAGS",
    "NIX_LD",
    "NIX_PKG_CONFIG_",
)

def allowed(key: str) -> bool:
    return key in EXACT_ALLOWLIST or key.startswith(PREFIX_ALLOWLIST)

build_env = {k: v for k, v in os.environ.items() if allowed(k)}
sorted_keys = sorted(build_env)

print("=== BUILD ENVIRONMENT (ALLOWLISTED) ===", flush=True)
print(f"build-env-keys: {' '.join(sorted_keys)}", flush=True)
for k in sorted_keys:
    print(f"  {k}={build_env[k]}", flush=True)
print("=== END BUILD ENVIRONMENT ===", flush=True)

fresh_cmd = ["cargo", "xtask", "fresh-build", "--release"]
if jobs:
    fresh_cmd.extend(["--jobs", jobs])

subprocess.run(fresh_cmd, cwd=cwd, env=build_env, check=True)
subprocess.run(
    ["./scripts/package-release.sh", "--target", target, "--skip-build", "--no-smoke"],
    cwd=cwd,
    env=build_env,
    check=True,
)
PY
}

validate_and_normalize_out_dir() {
  local raw_out="$1"
  local repo="$2"
  python3 - "$raw_out" "$repo" <<'PY'
import os, sys

raw_out, repo = sys.argv[1:3]
if not raw_out or not os.path.isabs(raw_out):
    print(f"ERROR: --out-dir must be an absolute path, got: {raw_out!r}", file=sys.stderr)
    sys.exit(2)

# Check BOTH lexical `normpath` (catches `/tmp/..` even on macOS where `/tmp`
# symlinks to `/private/tmp`) AND kernel-order `realpath` (catches symlinks
# and symlink+`..` escapes).
norm_out = os.path.normpath(raw_out)
real_out = os.path.realpath(raw_out)
repo_real = os.path.realpath(repo)

if norm_out == "/" or real_out == "/":
    print(f"ERROR: --out-dir must not resolve to /: {raw_out!r}", file=sys.stderr)
    sys.exit(2)

for candidate in (norm_out, real_out):
    if (
        candidate == repo_real
        or candidate.startswith(repo_real + os.sep)
        or repo_real.startswith(candidate + os.sep)
    ):
        print(
            f"ERROR: --out-dir must not be equal to, inside, or an ancestor of the repository ({repo_real}): {raw_out!r}",
            file=sys.stderr,
        )
        sys.exit(2)

print(real_out)
PY
}

write_and_verify_manifest() {
  local m_out="$1"
  local m_ver="$2"
  local m_commit="$3"
  local m_target="$4"
  local m_rustc="$5"
  local m_tarball="$6"
  local m_url="$7"
  local m_root="$8"
  local m_bin="$9"
  local m_pdump="${10}"
  local m_rel_pdump="${11}"

  local m_dir
  m_dir="$(dirname -- "$m_out")"
  mkdir -p "$m_dir"

  local tmp_manifest
  tmp_manifest="$(mktemp "$m_dir/.perf-baseline-manifest.XXXXXX")"
  active_tmp_manifest="$tmp_manifest"

  local rc=0
  python3 - \
    "$tmp_manifest" "$m_ver" "$m_commit" "$m_target" "$m_rustc" \
    "$m_tarball" "$m_url" "$m_root" "$m_bin" "$m_pdump" "$m_rel_pdump" <<'PY' || rc=$?
import hashlib, json, os, sys

(
    out_path,
    version,
    git_commit,
    target,
    rustc_version,
    tarball_path,
    asset_url,
    archive_root,
    binary_path,
    pdump_path,
    relative_pdump,
) = sys.argv[1:12]

if len(git_commit) != 40 or any(c not in "0123456789abcdef" for c in git_commit):
    print(f"ERROR: git_commit must be a 40-char lowercase hex SHA, got: {git_commit!r}", file=sys.stderr)
    sys.exit(2)

def sha256_and_size(path):
    h = hashlib.sha256()
    size = 0
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
            size += len(chunk)
    return h.hexdigest(), size

tar_sha, tar_size = sha256_and_size(tarball_path)
bin_sha, bin_size = sha256_and_size(binary_path)
pdump_sha, pdump_size = sha256_and_size(pdump_path)

tar_name = os.path.basename(tarball_path)
if not asset_url:
    asset_url = f"local-controlled-runner://{tar_name}"

bin_archive = f"{archive_root}/bin/neomacs"
pdump_archive = os.path.normpath(os.path.join(f"{archive_root}/bin", relative_pdump))

if target == "x86_64-unknown-linux-gnu":
    codegen_settings = (
        "cargo xtask fresh-build --release "
        "(opt-level=3, lto=true, codegen-units=1, debug=false, strip=none, features=video, "
        "rustflags=[-Cllvm-args=-align-all-functions=6, -Clink-arg=-Wl,--build-id, "
        "-Clink-arg=-Wl,-z,max-page-size=0x200000, -Clink-arg=-Wl,-z,common-page-size=0x200000])"
    )
else:
    codegen_settings = (
        "cargo xtask fresh-build --release "
        "(opt-level=3, lto=true, codegen-units=1, debug=false, strip=none)"
    )

doc = {
    "schema_version": 1,
    "version": version,
    "git_commit": git_commit,
    "target": target,
    "build_profile": "release",
    "rustc_version": rustc_version,
    "pgo_profile": "none",
    "codegen_settings": codegen_settings,
    "release_asset": {
        "name": tar_name,
        "url": asset_url,
        "size_bytes": tar_size,
        "sha256": tar_sha,
    },
    "binary": {
        "archive_path": bin_archive,
        "size_bytes": bin_size,
        "sha256": bin_sha,
    },
    "pdump": {
        "archive_path": pdump_archive,
        "relative_to_binary": relative_pdump,
        "size_bytes": pdump_size,
        "sha256": pdump_sha,
    },
}

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2)
    fh.write("\n")
PY

  if [ "$rc" -ne 0 ]; then
    rm -f "$tmp_manifest"
    active_tmp_manifest=""
    return "$rc"
  fi

  rc=0
  "$script_dir/verify-perf-baseline.sh" \
    --manifest "$tmp_manifest" \
    --env-var NEOMACS_PERF_V0021_BASELINE_EDITOR \
    -- "$m_bin" || rc=$?
  if [ "$rc" -ne 0 ]; then
    rm -f "$tmp_manifest"
    active_tmp_manifest=""
    return "$rc"
  fi

  chmod 0644 "$tmp_manifest"
  mv -f "$tmp_manifest" "$m_out"
  active_tmp_manifest=""
}

if [ "$manifest_only" -eq 1 ]; then
  for req in "$tarball_path" "$archive_root" "$binary_path" "$pdump_path" "$relative_pdump"; do
    if [ -z "$req" ]; then
      echo "ERROR: --manifest-only requires --tarball, --archive-root, --binary, --pdump, and --relative-pdump" >&2
      exit 2
    fi
  done
  rustc_ver="$(resolve_rustc_version)"
  write_and_verify_manifest \
    "$manifest" "$version" "$ref" "$target" "$rustc_ver" \
    "$tarball_path" "$asset_url" "$archive_root" "$binary_path" "$pdump_path" "$relative_pdump"
  exit 0
fi

if [ -z "$out_dir" ]; then
  echo "ERROR: --out-dir PATH is required" >&2
  exit 2
fi
out_dir="$(validate_and_normalize_out_dir "$out_dir" "$repo_root")"

commit="$(git -C "$repo_root" rev-parse --verify "${ref}^{commit}")"
rustc_ver="$(resolve_rustc_version)"

work_tmp="$(mktemp -d)"

clone_dir="$work_tmp/src"
# Use `--shared` (git alternates) rather than `--local` hardlinks so cloning
# into a `tmpfs` `/tmp` on a different filesystem from `$repo_root` succeeds
# without `EXDEV` and without copying the entire git object store.
git clone --no-checkout --shared "$repo_root" "$clone_dir" >/dev/null
git -C "$clone_dir" checkout --detach "$commit" >/dev/null

run_in_allowlisted_build_env "$clone_dir" "$target" "$jobs"

shopt -s nullglob
tarballs=("$clone_dir"/dist/neomacs-*-"${target}".tar.gz)
shopt -u nullglob
if [ "${#tarballs[@]}" -eq 0 ] || [ ! -f "${tarballs[0]}" ]; then
  echo "ERROR: package-release.sh did not produce a tarball under $clone_dir/dist" >&2
  exit 1
fi
tarball="${tarballs[0]}"

extract_tmp="$work_tmp/extract"
mkdir -p "$extract_tmp"
tar -xzf "$tarball" -C "$extract_tmp"
top_dir="$(find "$extract_tmp" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
if [ -z "$top_dir" ]; then
  echo "ERROR: extracted baseline archive has no top-level directory" >&2
  exit 1
fi
root_name="$(basename -- "$top_dir")"

rm -rf "$out_dir"
mkdir -p "$(dirname -- "$out_dir")"
cp -a "$top_dir" "$out_dir"
cp -a "$tarball" "$out_dir/$(basename -- "$tarball")"

installed_bin="$out_dir/bin/neomacs"
installed_pdump="$(find "$out_dir/libexec" -type f -name 'neomacs.pdump' | head -n 1)"
if [ ! -x "$installed_bin" ] || [ ! -f "$installed_pdump" ]; then
  echo "ERROR: packaged baseline missing executable or sibling pdump under $out_dir" >&2
  exit 1
fi
rel_pdump="$(python3 -c 'import os, sys; print(os.path.relpath(sys.argv[1], os.path.dirname(sys.argv[2])))' "$installed_pdump" "$installed_bin")"

write_and_verify_manifest \
  "$manifest" "$version" "$commit" "$target" "$rustc_ver" \
  "$out_dir/$(basename -- "$tarball")" "$asset_url" "$root_name" \
  "$installed_bin" "$installed_pdump" "$rel_pdump"

echo "installed baseline binary: $installed_bin"
echo "wrote verified manifest:   $manifest"
