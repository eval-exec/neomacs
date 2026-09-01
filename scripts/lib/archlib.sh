#!/usr/bin/env bash
# The architecture-dependent private directory a packaged neomacs installs --
# GNU's PATH_EXEC / archlibdir.
#
# GNU configures it once:
#
#   archlibdir='${libexecdir}/emacs/${version}/${configuration}'   configure.ac:290
#
# and bakes the result into src/epaths.h, so its build system and its C code
# cannot disagree.  Neomacs has no configure step: the runtime PROBES for this
# directory (crates/neovm-core/src/emacs_core/system/path_exec/mod.rs), which means the staging
# scripts and the binary derive the same path twice, from different inputs.
#
# So every caller MUST finish with neomacs_verify_archlib, which asks the
# staged binary what it actually resolved and fails the build if it is not the
# directory that was staged.  A packaging script that stages an archlib and
# does not verify it is a script that can ship a binary unable to find its own
# dump.

# The workspace version -- the same string cargo passes to the crate as
# CARGO_PKG_VERSION and path_exec.rs re-exports as ARCHLIB_VERSION.  Read from
# [workspace.package] rather than `git describe`: the artifact NAME may carry a
# tag or a commit hash, but the archlib path has to match what the compiled
# binary believes.
neomacs_archlib_version() {
  local cargo_toml="$1"
  awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/                     { in_section = 0 }
    in_section && /^version[[:space:]]*=/ {
      gsub(/^version[[:space:]]*=[[:space:]]*"/, "")
      gsub(/".*$/, "")
      print
      exit
    }
  ' "$cargo_toml"
}

# GNU's archlibdir tail, relative to an install prefix:
#   libexec/neomacs/<version>/<target triple>
# Mirrors emacs_core::path_exec::archlib_relative_path.
neomacs_archlib_relpath() {
  local cargo_toml="$1" triple="$2" version
  version="$(neomacs_archlib_version "$cargo_toml")"
  if [[ -z "$version" ]]; then
    echo "could not read [workspace.package] version from $cargo_toml" >&2
    return 1
  fi
  if [[ -z "$triple" ]]; then
    echo "neomacs_archlib_relpath needs a target triple" >&2
    return 1
  fi
  printf 'libexec/neomacs/%s/%s\n' "$version" "$triple"
}

# Prove a staged tree is coherent, reporting EVERY failure rather than the
# first: a macOS release round trip costs a quarter of an hour, so a script
# that stops at the first mistake wastes several of them.
#
#   $1 the staged executable
#   $2 the dump image that was staged into the archlib
#   $3 the absolute archlib directory that was staged
#   $4 the runtime root to hand the binary (NEOMACS_RUNTIME_ROOT), or ""
#
# Two independent claims are checked:
#
#   1. With the dump named explicitly, the binary resolves exec-directory to
#      the staged archlib.  This is the PATH_EXEC probe
#      (path_exec.rs::path_exec_candidates) agreeing with what was staged, and
#      it is checked with --dump-file so that a probe failure cannot be
#      mistaken for a dump-lookup failure.
#   2. With no --dump-file, the binary starts anyway -- so the dump-lookup
#      rungs (load.rs::runtime_image_candidate_paths_for_executable) reach the
#      archlib copy.  This is the claim that decides whether the shipped
#      artifact runs at all.
neomacs_verify_archlib() {
  local binary="$1" dump="$2" archlib="$3" runtime_root="${4:-}"
  local failures=0 reported exec_directory

  local -a env_prefix=(env -u EMACSPATH -u NEOMACS_RUNTIME_ROOT)
  if [[ -n "$runtime_root" ]]; then
    env_prefix=(env -u EMACSPATH "NEOMACS_RUNTIME_ROOT=$runtime_root")
  fi

  if [[ ! -x "$binary" ]]; then
    echo "archlib check: staged executable is missing or not executable: $binary" >&2
    return 1
  fi
  if [[ ! -f "$dump" ]]; then
    echo "archlib check: staged dump image is missing: $dump" >&2
    failures=$((failures + 1))
  fi
  if [[ ! -d "$archlib" ]]; then
    echo "archlib check: staged archlib directory is missing: $archlib" >&2
    failures=$((failures + 1))
  fi

  if exec_directory="$(
    "${env_prefix[@]}" "$binary" --dump-file "$dump" \
      --batch --eval '(princ exec-directory)' 2>/dev/null
  )"; then
    # GNU stores exec-directory through file-name-as-directory
    # (src/callproc.c:1961), so it always carries a trailing slash.
    reported="${exec_directory%/}"
    if [[ "$reported" != "${archlib%/}" ]]; then
      echo "archlib check: staged $archlib but the binary resolved exec-directory to $exec_directory" >&2
      echo "  the PATH_EXEC probe in crates/neovm-core/src/emacs_core/system/path_exec/mod.rs and this script disagree" >&2
      failures=$((failures + 1))
    fi
  else
    echo "archlib check: $binary --dump-file $dump could not start" >&2
    failures=$((failures + 1))
  fi

  if ! "${env_prefix[@]}" "$binary" --batch --eval '(kill-emacs 0)' >/dev/null 2>&1; then
    echo "archlib check: $binary could not find its dump image without --dump-file" >&2
    echo "  the dump-lookup rungs in crates/neovm-core/src/emacs_core/lisp/load/mod.rs do not reach $dump" >&2
    failures=$((failures + 1))
  fi

  if ((failures > 0)); then
    echo "archlib check: $failures problem(s) in the staged layout" >&2
    return 1
  fi
  echo "archlib check: $binary resolves exec-directory to $archlib and loads its dump from there"
}
